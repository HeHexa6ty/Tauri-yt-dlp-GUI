// Input field with URL 
import "./UrlInput.css";

interface UrlInputProps {
  value: string;
  onChange: (value: string) => void;
  /** Locked while a download is running. */
  disabled?: boolean;
}

export default function UrlInput({
  value,
  onChange,
  disabled = false,
}: UrlInputProps) {
  return (
    <input
      className="url-input"
      type="url"
      placeholder="Paste YouTube, Instagram, X url here..."
      value={value}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}