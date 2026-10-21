import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { UnlistenFn } from "@tauri-apps/api/event";
import AnalyzeButton from "./components/AnalyzeButton";
import DirectoryDiv from "./components/DirectoryDiv";
import DownloadButton from "./components/DownloadButton";
import DownloadStatus from "./components/DownloadStatus";
import MediaPreview from "./components/MediaPreview";
import MediaTypeSelector from "./components/MediaTypeSelector";
import UrlInput from "./components/UrlInput";
import {
  analyzeUrl,
  downloadMedia,
  getBackendError,
  getErrorMessage,
  onDownloadProgress,
} from "./services/tauri";
import {
  getAvailableFormats,
  type ConflictMode,
  type DirectorySelection,
  type DownloadRequest,
  type GifQuality,
  type MediaInfo,
  type OutputFormat,
} from "./types";
import { isSupportedUrl } from "./utils/url";
import "./App.css";

/** Delay before a supported URL is analyzed automatically. */
const AUTO_ANALYZE_DELAY_MS = 5000;

/** Live download state shown under the download button. */
interface ProgressState {
  percent: number;
  stage: "downloading" | "converting";
  speed: number | null;
  eta: number | null;
}

const INITIAL_PROGRESS: ProgressState = {
  percent: 0,
  stage: "downloading",
  speed: null,
  eta: null,
};

function App() {
  const [url, setUrl] = useState("");
  const [media, setMedia] = useState<MediaInfo | null>(null);
  const [format, setFormat] = useState<OutputFormat | null>(null);
  const [gifQuality, setGifQuality] = useState<GifQuality>("better");
  const [directory, setDirectory] = useState<DirectorySelection>({
    mode: "default",
  });
  const [analyzing, setAnalyzing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Download state: one download at a time, with an optional conflict prompt.
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState<ProgressState>(INITIAL_PROGRESS);
  const [downloadMessage, setDownloadMessage] = useState<string | null>(null);
  const [downloadError, setDownloadError] = useState<string | null>(null);
  const [conflict, setConflict] = useState<string | null>(null);

  // Identifies the newest analysis so late responses cannot overwrite state.
  const requestId = useRef(0);

  const supported = isSupportedUrl(url);
  const mediaType = media?.mediaType ?? null;
  const platform = media?.platform ?? null;
  const duration = media?.duration ?? null;

  const handleUrlChange = (value: string) => {
    // Invalidate any analysis that is still running for the old URL.
    requestId.current += 1;
    setUrl(value);
    setMedia(null);
    setFormat(null);
    setError(null);
    setAnalyzing(false);
    // A new URL means the previous download result no longer applies.
    setDownloadMessage(null);
    setDownloadError(null);
    setConflict(null);
    setProgress(INITIAL_PROGRESS);
  };

  const runAnalyze = async () => {
    if (!supported || analyzing || downloading) {
      return;
    }

    const id = ++requestId.current;
    setAnalyzing(true);
    setError(null);

    try {
      const info = await analyzeUrl(url.trim());
      if (id !== requestId.current) {
        return;
      }
      setMedia(info);
    } catch (caught) {
      if (id !== requestId.current) {
        return;
      }
      setError(getErrorMessage(caught));
    } finally {
      if (id === requestId.current) {
        setAnalyzing(false);
      }
    }
  };

  // Streaming progress from yt-dlp/FFmpeg, subscribed once for the app.
  useEffect(() => {
    let unsubscribe: UnlistenFn | null = null;
    let active = true;

    onDownloadProgress((update) => {
      setProgress({
        percent: update.percent,
        stage: update.stage,
        speed: update.speed ?? null,
        eta: update.eta ?? null,
      });
    })
      .then((unlisten) => {
        if (active) {
          unsubscribe = unlisten;
        } else {
          // The component unmounted before the subscription resolved.
          unlisten();
        }
      })
      .catch((caught) => {
        console.error("Failed to subscribe to download progress:", caught);
      });

    return () => {
      active = false;
      unsubscribe?.();
    };
  }, []);

  /**
   * Starts a download. `conflict` is `fail` the first time so the backend can
   * report an existing file; the user can then retry with `rename`.
   */
  const startDownload = async (conflictMode: ConflictMode) => {
    if (!media || format === null || downloading) {
      return;
    }

    const request: DownloadRequest = {
      url: url.trim(),
      title: media.title,
      id: media.id,
      mediaType: media.mediaType,
      format,
      // GIF quality only matters for the GIF pipeline.
      gifQuality: format === "gif" ? gifQuality : undefined,
      conflict: conflictMode,
      directory,
    };

    setDownloading(true);
    setDownloadError(null);
    setDownloadMessage(null);
    setConflict(null);
    setProgress(INITIAL_PROGRESS);

    try {
      const result = await downloadMedia(request);
      setDownloadMessage(`Saved to ${result.filePath}`);
    } catch (caught) {
      const backend = getBackendError(caught);
      if (backend?.code === "file-exists") {
        // Let the user choose between cancelling and renaming.
        setConflict(backend.message);
      } else {
        setDownloadError(getErrorMessage(caught));
      }
    } finally {
      setDownloading(false);
    }
  };

  const handleDirectoryChange = (next: DirectorySelection) => {
    setDirectory((current) => ({
      ...next,
      // Keep the last picked folder when toggling Default ↔ Custom.
      path: next.mode === "custom" ? (next.path ?? current.path) : current.path,
    }));
  };

  const handleBrowse = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Choose download folder",
      });

      if (typeof selected === "string" && selected !== "") {
        setDirectory({ mode: "custom", path: selected });
      }
    } catch (caught) {
      // The dialog plugin only exists inside the Tauri window, so browser
      // dev mode cannot open a native picker. Keep this out of the preview
      // area because it says nothing about the analyzed media.
      console.error("Failed to open the folder picker:", caught);
    }
  };

  // Analyze a supported URL automatically after the user stops typing.
  useEffect(() => {
    if (!supported || media !== null || analyzing || error !== null) {
      return;
    }

    const timer = window.setTimeout(() => {
      void runAnalyze();
    }, AUTO_ANALYZE_DELAY_MS);

    return () => window.clearTimeout(timer);
  }, [url, supported, media, analyzing, error]);

  // Keep the format valid whenever the type, duration or platform changes
  // (GIF is limited to short videos and never offered for YouTube).
  useEffect(() => {
    if (mediaType === null) {
      setFormat(null);
      return;
    }

    const allowed = getAvailableFormats(mediaType, duration, platform);
    setFormat((current) =>
      current !== null && allowed.includes(current) ? current : allowed[0],
    );
  }, [mediaType, duration, platform]);

  const hasDirectory =
    directory.mode === "default" || Boolean(directory.path);

  return (
    <main className="container">
      <h1>yt-dlp GUI</h1>

      <div className="url-row">
        <UrlInput
          value={url}
          onChange={handleUrlChange}
          disabled={downloading}
        />
        <AnalyzeButton
          onClick={() => {
            void runAnalyze();
          }}
          disabled={!supported || downloading}
          loading={analyzing}
        />
      </div>

      <MediaPreview media={media} loading={analyzing} error={error} />

      <MediaTypeSelector
        mediaType={mediaType}
        duration={duration}
        platform={platform}
        value={format}
        onChange={setFormat}
        gifQuality={gifQuality}
        onGifQualityChange={setGifQuality}
        disabled={media === null || downloading}
      />

      <DirectoryDiv
        value={directory}
        onChange={handleDirectoryChange}
        onBrowse={() => {
          void handleBrowse();
        }}
        tree={media?.directoryTree ?? null}
        disabled={media === null || downloading}
      />

      <DownloadButton
        onClick={() => {
          void startDownload("fail");
        }}
        disabled={media === null || !hasDirectory || format === null}
        downloading={downloading}
        progress={progress.percent}
        stage={progress.stage}
        speed={progress.speed}
        eta={progress.eta}
      />

      <DownloadStatus
        message={downloadMessage}
        error={downloadError}
        conflict={conflict}
        disabled={downloading}
        onCancelConflict={() => setConflict(null)}
        onRenameConflict={() => {
          void startDownload("rename");
        }}
      />
    </main>
  );
}

export default App;
