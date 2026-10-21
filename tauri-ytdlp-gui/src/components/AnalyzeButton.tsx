// Button to confirm URL and load preview
import "./AnalyzeButton.css";

interface AnalyzeButtonProps {
  onClick: () => void;
  disabled?: boolean;
  loading?: boolean;
}

export default function AnalyzeButton({
  onClick,
  disabled = false,
  loading = false,
}: AnalyzeButtonProps) {
  return (
    <button
      type="button"
      className="analyze-button"
      onClick={onClick}
      disabled={disabled || loading}
      aria-busy={loading}
    >
      {loading ? "Analyzing…" : "Analyze"}
    </button>
  );
}
