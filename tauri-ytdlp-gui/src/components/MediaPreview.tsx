// Preview the suppoused content
import type { MediaInfo } from "../types";
import "./MediaPreview.css";

interface MediaPreviewProps {
  media: MediaInfo | null;
  loading?: boolean;
  /** Error message from the backend, if analysis failed. */
  error?: string | null;
}

function formatDuration(seconds: number): string {
  const total = Math.round(seconds);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = total % 60;
  const pad = (value: number) => value.toString().padStart(2, "0");

  return hours > 0
    ? `${hours}:${pad(minutes)}:${pad(secs)}`
    : `${minutes}:${pad(secs)}`;
}

export default function MediaPreview({
  media,
  loading = false,
  error = null,
}: MediaPreviewProps) {
  if (loading) {
    return (
      <section
        className="media-preview media-preview--empty"
        aria-live="polite"
      >
        <p>Analyzing URL…</p>
      </section>
    );
  }

  if (error) {
    return (
      <section
        className="media-preview media-preview--empty media-preview--error"
        role="alert"
      >
        <p>{error}</p>
      </section>
    );
  }

  if (!media) {
    return (
      <section className="media-preview media-preview--empty">
        <p>Paste a supported URL to preview the media.</p>
      </section>
    );
  }

  const hasMeta = media.uploader !== undefined || media.duration !== undefined;

  return (
    <section className="media-preview" aria-live="polite">
      {media.thumbnail && (
        <img
          className="media-preview__thumbnail"
          src={media.thumbnail}
          alt=""
          referrerPolicy="no-referrer"
          onError={(event) => {
            event.currentTarget.style.display = "none";
          }}
        />
      )}
      <div className="media-preview__details">
        <h2 className="media-preview__title">{media.title}</h2>
        {hasMeta && (
          <p className="media-preview__meta">
            {media.uploader}
            {media.uploader !== undefined &&
              media.duration !== undefined &&
              " · "}
            {media.duration !== undefined && formatDuration(media.duration)}
          </p>
        )}
      </div>
    </section>
  );
}
