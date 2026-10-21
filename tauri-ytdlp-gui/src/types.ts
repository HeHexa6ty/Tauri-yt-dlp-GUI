export type MediaType = "video" | "audio" | "gif" | "image";

/** Output formats the backend can produce for a detected media type. */
export type OutputFormat = "mp4" | "mp3" | "gif" | "png" | "jpg";

/** How much effort goes into the GIF conversion. */
export type GifQuality = "simple" | "better";

/** What the backend should do when the target file already exists. */
export type ConflictMode = "fail" | "rename";

/** Structure created by the backend under the global Downloads folder. */
export interface DirectoryTree {
  /** Absolute path of the app folder, e.g. `~/Downloads/yt-dlp-GUI`. */
  root: string;
  /** Folder created for the detected platform, e.g. "YouTube". */
  platform: string;
  /** Type folders created inside the platform folder. */
  folders: string[];
}

export interface MediaInfo {
  id: string;
  title: string;
  uploader?: string;
  thumbnail?: string;
  /** Duration in seconds. */
  duration?: number;
  webpageUrl: string;
  /** Detected by the backend; drives the available output formats. */
  mediaType: MediaType;
  /** Source platform folder name, e.g. "YouTube". */
  platform: string;
  /** Platform folder created under Downloads by the backend. */
  defaultDirectory: string;
  /** Actual folders created for this platform, used to render the tree. */
  directoryTree: DirectoryTree;
}

/** How the user wants to choose the download location. */
export interface DirectorySelection {
  /** `default` keeps the app folder layout; `custom` uses one folder. */
  mode: "default" | "custom";
  /** Absolute folder path; only meaningful when `mode` is `custom`. */
  path?: string;
}

/** Payload sent to the `download_media` command. */
export interface DownloadRequest {
  url: string;
  /** Title and id come from the earlier analysis and build the file name. */
  title: string;
  id: string;
  /** Detected source type; photo posts use a different yt-dlp pipeline. */
  mediaType: MediaType;
  format: OutputFormat;
  /** Only meaningful when `format` is `gif`. */
  gifQuality?: GifQuality;
  /** Defaults to `fail` so the UI can ask the user about a conflict. */
  conflict?: ConflictMode;
  directory: DirectorySelection;
}

/** Progress event emitted by the backend while downloading/converting. */
export interface DownloadProgress {
  id: string;
  /** `downloading` while yt-dlp runs, `converting` during FFmpeg. */
  stage: "downloading" | "converting";
  percent: number;
  downloadedBytes: number;
  totalBytes?: number;
  /** Bytes per second. */
  speed?: number;
  /** Seconds remaining. */
  eta?: number;
}

/** Result returned by a successful download. */
export interface DownloadResult {
  id: string;
  filePath: string;
  format: OutputFormat;
}

export const MEDIA_TYPE_OPTIONS: ReadonlyArray<{
  value: MediaType;
  label: string;
}> = [
  { value: "video", label: "Video" },
  { value: "audio", label: "Audio" },
  { value: "gif", label: "GIF" },
  { value: "image", label: "Image" },
];

/** Longest video (in seconds) that may still be exported as a GIF. */
export const MAX_GIF_DURATION_SECONDS = 60;

/** Platforms whose sources are never exported as GIF. */
const GIF_UNSUPPORTED_PLATFORMS = new Set(["YouTube"]);

/** Whether a platform offers GIF output at all (YouTube only has video/audio). */
export function isGifSupportedPlatform(platform?: string | null): boolean {
  return !platform || !GIF_UNSUPPORTED_PLATFORMS.has(platform);
}

/** Formats the user may pick once the backend reports a media type. */
export const FORMATS_BY_MEDIA_TYPE: Record<MediaType, readonly OutputFormat[]> = {
  // Video sources can also be extracted to MP3 (see README "Audio extraction").
  video: ["mp4", "mp3", "gif"],
  audio: ["mp3"],
  gif: ["gif"],
  image: ["png", "jpg", "gif"],
};

export const GIF_QUALITY_OPTIONS: ReadonlyArray<{
  value: GifQuality;
  label: string;
}> = [
  { value: "simple", label: "Simple" },
  { value: "better", label: "Better" },
];

/** Whether a source may be converted to GIF given its type, length and platform. */
export function isGifAllowed(
  mediaType: MediaType,
  duration?: number | null,
  platform?: string | null,
): boolean {
  if (!isGifSupportedPlatform(platform)) {
    return false;
  }
  if (mediaType === "video") {
    // Unknown durations cannot be proven short enough.
    return (
      duration !== undefined &&
      duration !== null &&
      duration <= MAX_GIF_DURATION_SECONDS
    );
  }

  return true;
}

/**
 * Applies per-type and per-platform limits to `FORMATS_BY_MEDIA_TYPE`. GIF
 * export is only offered for short videos and never for YouTube sources.
 */
export function getAvailableFormats(
  mediaType: MediaType,
  duration?: number | null,
  platform?: string | null,
): readonly OutputFormat[] {
  const formats = FORMATS_BY_MEDIA_TYPE[mediaType];

  if (!isGifAllowed(mediaType, duration, platform)) {
    return formats.filter((format) => format !== "gif");
  }

  return formats;
}

export const FORMAT_LABELS: Record<OutputFormat, string> = {
  mp4: "MP4",
  mp3: "MP3",
  gif: "GIF",
  png: "PNG",
  jpg: "JPG",
};
