// Final button, after confirmation, downloads content, with expected type into expected directory
import "./DownloadButton.css";

interface DownloadButtonProps {
  onClick: () => void;
  disabled?: boolean;
  downloading?: boolean;
  /** Download progress as a percentage (0-100). */
  progress?: number;
  /** `downloading` while yt-dlp runs, `converting` during FFmpeg. */
  stage?: "downloading" | "converting";
  /** Download speed in bytes per second, when known. */
  speed?: number | null;
  /** Remaining seconds, when known. */
  eta?: number | null;
}

/** Formats a byte rate such as `1.2 MB/s`. */
function formatSpeed(speed?: number | null): string | null {
  if (speed === undefined || speed === null || speed <= 0) {
    return null;
  }

  const units = ["B/s", "KB/s", "MB/s", "GB/s"];
  let value = speed;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

/** Formats remaining seconds as `m:ss` (or `h:mm:ss` for long downloads). */
function formatEta(eta?: number | null): string | null {
  if (eta === undefined || eta === null || eta <= 0 || !Number.isFinite(eta)) {
    return null;
  }

  const total = Math.round(eta);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  const pad = (value: number) => value.toString().padStart(2, "0");

  return hours > 0
    ? `${hours}:${pad(minutes)}:${pad(seconds)}`
    : `${minutes}:${pad(seconds)}`;
}

export default function DownloadButton({
  onClick,
  disabled = false,
  downloading = false,
  progress = 0,
  stage = "downloading",
  speed = null,
  eta = null,
}: DownloadButtonProps) {
  const clampedProgress = Math.min(100, Math.max(0, Math.round(progress)));
  const converting = downloading && stage === "converting";
  const speedLabel = formatSpeed(speed);
  const etaLabel = formatEta(eta);

  return (
    <div className="download-button">
      <button
        type="button"
        className="download-button__button"
        onClick={onClick}
        disabled={disabled || downloading}
        aria-busy={downloading}
      >
        {converting
          ? "Converting…"
          : downloading
            ? `Downloading… ${clampedProgress}%`
            : "Download"}
      </button>

      {downloading && (
        <>
          <div
            className={`download-button__progress${
              converting ? " download-button__progress--converting" : ""
            }`}
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={clampedProgress}
          >
            <div
              className="download-button__progress-bar"
              style={{ width: `${clampedProgress}%` }}
            />
          </div>

          <div className="download-button__stats">
            {converting ? (
              <span>Processing with FFmpeg…</span>
            ) : (
              <>
                <span>{clampedProgress}%</span>
                {speedLabel && <span>{speedLabel}</span>}
                {etaLabel && <span>ETA {etaLabel}</span>}
              </>
            )}
          </div>
        </>
      )}
    </div>
  );
}
