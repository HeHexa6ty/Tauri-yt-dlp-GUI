// Shows the outcome of a download: success message, backend error, or the
// "file already exists" prompt where the user picks Cancel or Rename.
import "./DownloadStatus.css";

interface DownloadStatusProps {
  /** Success message shown after a download finishes. */
  message?: string | null;
  /** Backend error message. */
  error?: string | null;
  /** Set when the target file already exists; shows the conflict prompt. */
  conflict?: string | null;
  onCancelConflict?: () => void;
  onRenameConflict?: () => void;
  disabled?: boolean;
}

export default function DownloadStatus({
  message = null,
  error = null,
  conflict = null,
  onCancelConflict,
  onRenameConflict,
  disabled = false,
}: DownloadStatusProps) {
  if (!message && !error && !conflict) {
    return null;
  }

  return (
    <div className="download-status">
      {message && (
        <p className="download-status__message" role="status">
          {message}
        </p>
      )}

      {error && (
        <p className="download-status__error" role="alert">
          {error}
        </p>
      )}

      {conflict && (
        <div className="download-status__conflict" role="alert">
          <p className="download-status__conflict-text">
            {conflict} Choose another name or cancel.
          </p>
          <div className="download-status__actions">
            <button
              type="button"
              className="download-status__button download-status__button--ghost"
              onClick={onCancelConflict}
              disabled={disabled}
            >
              Cancel
            </button>
            <button
              type="button"
              className="download-status__button"
              onClick={onRenameConflict}
              disabled={disabled}
            >
              Rename
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
