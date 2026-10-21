// Ask the user about media type
// The media type itself is detected by the backend; the user only chooses an
// output format that is valid for that detected type.
import {
  FORMAT_LABELS,
  FORMATS_BY_MEDIA_TYPE,
  GIF_QUALITY_OPTIONS,
  getAvailableFormats,
  isGifSupportedPlatform,
  MAX_GIF_DURATION_SECONDS,
  MEDIA_TYPE_OPTIONS,
  type GifQuality,
  type MediaType,
  type OutputFormat,
} from "../types";
import "./MediaTypeSelector.css";

interface MediaTypeSelectorProps {
  /** Media type returned by the backend, or null before analysis. */
  mediaType: MediaType | null;
  /** Duration in seconds, used to decide whether GIF is allowed. */
  duration?: number | null;
  /** Source platform, e.g. "YouTube"; some platforms never allow GIF. */
  platform?: string | null;
  /** Currently selected output format. */
  value: OutputFormat | null;
  onChange: (format: OutputFormat) => void;
  /** Only relevant while GIF is selected. */
  gifQuality?: GifQuality;
  onGifQualityChange?: (quality: GifQuality) => void;
  disabled?: boolean;
}

export default function MediaTypeSelector({
  mediaType,
  duration,
  platform,
  value,
  onChange,
  gifQuality = "better",
  onGifQualityChange,
  disabled = false,
}: MediaTypeSelectorProps) {
  const formats = mediaType
    ? getAvailableFormats(mediaType, duration, platform)
    : [];
  const detectedLabel = MEDIA_TYPE_OPTIONS.find(
    (option) => option.value === mediaType,
  )?.label;

  // Explain a hidden GIF option: platform restriction wins over the duration.
  const gifOfferedForType =
    mediaType !== null && FORMATS_BY_MEDIA_TYPE[mediaType].includes("gif");
  const gifUnavailable = gifOfferedForType && !formats.includes("gif");
  const gifBlockedByPlatform = !isGifSupportedPlatform(platform);

  return (
    <fieldset className="media-type-selector" disabled={disabled}>
      <legend className="media-type-selector__legend">
        Output format
        {detectedLabel && (
          <span className="media-type-selector__detected">
            detected {detectedLabel}
          </span>
        )}
      </legend>

      {formats.length > 0 ? (
        <div className="media-type-selector__options">
          {formats.map((format) => (
            <label key={format} className="media-type-option">
              <input
                className="media-type-option__input"
                type="radio"
                name="output-format"
                value={format}
                checked={value === format}
                onChange={() => onChange(format)}
              />
              <span className="media-type-option__label">
                {FORMAT_LABELS[format]}
              </span>
            </label>
          ))}
        </div>
      ) : (
        <p className="media-type-selector__hint">
          Media type will be detected after the URL is analyzed.
        </p>
      )}

      {value === "gif" && onGifQualityChange && (
        <div className="media-type-selector__quality">
          <span className="media-type-selector__quality-label">
            GIF quality
          </span>
          <div className="media-type-selector__options">
            {GIF_QUALITY_OPTIONS.map((option) => (
              <label key={option.value} className="media-type-option">
                <input
                  className="media-type-option__input"
                  type="radio"
                  name="gif-quality"
                  value={option.value}
                  checked={gifQuality === option.value}
                  onChange={() => onGifQualityChange(option.value)}
                />
                <span className="media-type-option__label">{option.label}</span>
              </label>
            ))}
          </div>
        </div>
      )}

      {gifUnavailable && (
        <p className="media-type-selector__hint">
          {gifBlockedByPlatform
            ? "GIF is not available for YouTube sources."
            : `GIF is only available for videos up to ${
                MAX_GIF_DURATION_SECONDS / 60
              } minute.`}
        </p>
      )}
    </fieldset>
  );
}
