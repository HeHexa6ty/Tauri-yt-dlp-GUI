use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;
use tokio::process::Command;
use tokio::time::timeout;

use crate::error::AppError;
use crate::models::{DirectoryTree, MediaInfo, MediaType};

/// Analysis should never hang the app; kill yt-dlp after this long.
const ANALYZE_TIMEOUT: Duration = Duration::from_secs(45);

/// Path to the yt-dlp binary, honouring the `YT_DLP_PATH` override.
pub fn yt_dlp_path() -> String {
    std::env::var("YT_DLP_PATH").unwrap_or_else(|_| "yt-dlp".to_string())
}

/// Path to the FFmpeg binary, honouring the `FFMPEG_PATH` override.
pub fn ffmpeg_path() -> String {
    std::env::var("FFMPEG_PATH").unwrap_or_else(|_| "ffmpeg".to_string())
}

/// Builds the yt-dlp command. A bundled sidecar or user-configured path can be
/// added later; for now an env override wins over the system PATH.
pub(crate) fn ytdlp_command() -> Command {
    let mut command = Command::new(yt_dlp_path());
    // Never leave a child behind when the future is dropped or timed out.
    command.kill_on_drop(true);
    command
}

/// Builds the FFmpeg command used for GIF/image conversions.
pub(crate) fn ffmpeg_command() -> Command {
    let mut command = Command::new(ffmpeg_path());
    command.kill_on_drop(true);
    command
}

/// Runs `yt-dlp --dump-single-json` and converts the result into `MediaInfo`.
pub async fn analyze(url: &str) -> Result<MediaInfo, AppError> {
    let mut command = ytdlp_command();
    command
        .args([
            "--dump-single-json",
            "--no-playlist",
            "--no-warnings",
            "--skip-download",
            // Photo posts have no downloadable format but still expose image
            // thumbnails, so treat "no formats" as success and detect them as
            // images instead of failing the whole analysis.
            "--ignore-no-formats-error",
            url,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = timeout(ANALYZE_TIMEOUT, command.output())
        .await
        .map_err(|_| {
            AppError::new(
                "timeout",
                "Analysis took too long. Check your connection and try again.",
            )
        })?
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                AppError::new(
                    "yt-dlp-missing",
                    "yt-dlp was not found. Install it or set YT_DLP_PATH.",
                )
            } else {
                AppError::new("yt-dlp-error", format!("Could not run yt-dlp: {error}"))
            }
        })?;

    if !output.status.success() {
        return Err(map_ytdlp_error(&String::from_utf8_lossy(&output.stderr)));
    }

    let json: Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        AppError::new(
            "invalid-output",
            format!("yt-dlp returned unreadable JSON: {error}"),
        )
    })?;

    parse_info(&json, url)
}

/// Shown when a site only serves a post to a logged-in session (for example an
/// age check on Instagram).
pub(crate) const ACCOUNT_REQUIRED_MESSAGE: &str =
    "This content may be age-restricted or private and requires a logged-in account.";

/// Detects the stderr messages sites use when an authenticated session is
/// required. Instagram reports an age-gated post as an "empty media response"
/// and points at `--cookies-from-browser`.
pub(crate) fn requires_account(message: &str) -> bool {
    let lower = message.to_lowercase();

    lower.contains("empty media response")
        || lower.contains("cookies-from-browser")
        || lower.contains("use --cookies")
        || lower.contains("login required")
        || lower.contains("login_required")
        || lower.contains("requires authentication")
        || lower.contains("authentication required")
}

/// Turns yt-dlp's stderr into a message the UI can display.
fn map_ytdlp_error(stderr: &str) -> AppError {
    let line = stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("yt-dlp could not analyze this URL.");
    let message = line
        .trim()
        .strip_prefix("ERROR: ")
        .unwrap_or(line.trim())
        .to_string();
    let lower = message.to_lowercase();

    if requires_account(&message) {
        return AppError::new("login-required", ACCOUNT_REQUIRED_MESSAGE);
    }

    let code = if lower.contains("unsupported url") {
        "unsupported-url"
    } else if lower.contains("private") {
        "private-media"
    } else if lower.contains("sign in") || lower.contains("login") {
        "login-required"
    } else {
        "analyze-failed"
    };

    AppError::new(code, message)
}

/// Reads a string field, ignoring JSON nulls.
fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// A codec is only meaningful when yt-dlp reports something other than "none".
fn codec(info: &Value, key: &str) -> Option<String> {
    string_field(info, key).filter(|value| value != "none")
}

/// Prefers the full source image for image posts, otherwise the best thumbnail.
fn pick_thumbnail(info: &Value, media_type: MediaType) -> Option<String> {
    let candidate = if media_type == MediaType::Image {
        string_field(info, "url")
    } else {
        None
    }
    .or_else(|| string_field(info, "thumbnail"))
    .or_else(|| {
        info.get("thumbnails")
            .and_then(Value::as_array)
            .and_then(|list| list.last())
            .and_then(|thumbnail| string_field(thumbnail, "url"))
    })?;

    // Some extractors return protocol-relative URLs.
    Some(if candidate.starts_with("//") {
        format!("https:{candidate}")
    } else {
        candidate
    })
}

/// Decides the source media type from codecs/extensions in the yt-dlp payload.
fn detect_media_type(info: &Value) -> MediaType {
    let ext = string_field(info, "ext").unwrap_or_default().to_lowercase();

    // Animated images are reported as GIF sources.
    if ext == "gif" {
        return MediaType::Gif;
    }

    if matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp" | "avif") {
        return MediaType::Image;
    }

    let formats = info.get("formats").and_then(Value::as_array);
    let has_video = codec(info, "vcodec").is_some()
        || formats.is_some_and(|list| list.iter().any(|f| codec(f, "vcodec").is_some()));
    if has_video {
        return MediaType::Video;
    }

    let has_audio = codec(info, "acodec").is_some()
        || formats.is_some_and(|list| list.iter().any(|f| codec(f, "acodec").is_some()));
    if has_audio {
        return MediaType::Audio;
    }

    // No codecs and no recognized extension: image posts end up here.
    MediaType::Image
}

/// Maps the yt-dlp JSON into our own shape. Playlists fall back to the first
/// entry so the preview always describes a single media item.
fn parse_info(root: &Value, original_url: &str) -> Result<MediaInfo, AppError> {
    let info = root
        .get("entries")
        .and_then(Value::as_array)
        .and_then(|entries| entries.first())
        .unwrap_or(root);

    let media_type = detect_media_type(info);

    Ok(MediaInfo {
        id: string_field(info, "id").unwrap_or_else(|| "unknown".to_string()),
        title: string_field(info, "title").unwrap_or_else(|| "Untitled media".to_string()),
        uploader: string_field(info, "uploader")
            .or_else(|| string_field(info, "channel"))
            .or_else(|| string_field(info, "uploader_id")),
        thumbnail: pick_thumbnail(info, media_type),
        duration: info.get("duration").and_then(Value::as_f64),
        webpage_url: string_field(info, "webpage_url").unwrap_or_else(|| original_url.to_string()),
        media_type,
        // Filled in by the command once the platform folder is known.
        platform: String::new(),
        default_directory: String::new(),
        directory_tree: DirectoryTree {
            root: String::new(),
            platform: String::new(),
            folders: Vec::new(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTAGRAM_AGE_GATE: &str = "ERROR: [Instagram] DecrKlojBOb: Instagram sent an empty media response. Check if this post is accessible in your browser without being logged-in. If it is not, then use --cookies-from-browser or --cookies for the authentication.";

    #[test]
    fn maps_an_instagram_age_gate_to_a_clear_message() {
        let error = map_ytdlp_error(INSTAGRAM_AGE_GATE);

        assert_eq!(error.code, "login-required");
        assert_eq!(error.message, ACCOUNT_REQUIRED_MESSAGE);
    }

    #[test]
    fn maps_a_login_required_response_to_a_clear_message() {
        let error = map_ytdlp_error(r#"ERROR: {"message":"login_required"}"#);

        assert_eq!(error.code, "login-required");
        assert_eq!(error.message, ACCOUNT_REQUIRED_MESSAGE);
    }

    #[test]
    fn keeps_unrelated_errors_unchanged() {
        let error = map_ytdlp_error("ERROR: Unsupported URL: https://example.com");

        assert_eq!(error.code, "unsupported-url");
        assert_eq!(error.message, "Unsupported URL: https://example.com");
    }
}
