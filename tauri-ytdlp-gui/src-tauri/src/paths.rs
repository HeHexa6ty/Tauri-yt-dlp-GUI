use std::path::PathBuf;

use url::Url;

use crate::error::AppError;
use crate::models::{DirectoryMode, DirectoryRequest, OutputFormat};

/// Host → folder name. Mirrors the frontend allowlist, but the backend is the
/// authority because it cannot be bypassed by a modified client.
const SUPPORTED_HOSTS: &[(&str, &str)] = &[
    ("youtube.com", "YouTube"),
    ("youtu.be", "YouTube"),
    ("instagram.com", "Instagram"),
    ("x.com", "X"),
    ("twitter.com", "X"),
    ("tiktok.com", "TikTok"),
];

/// App folder created inside Downloads; every platform folder lives inside it.
const APP_FOLDER: &str = "yt-dlp-GUI";

/// Type folders created inside a platform folder. YouTube and TikTok only
/// deliver video/audio media; Instagram and X can also host images and GIFs.
fn platform_folders(platform: &str) -> &'static [&'static str] {
    match platform {
        "YouTube" | "TikTok" => &["videos", "audio"],
        _ => &["images", "gifs", "videos", "audio"],
    }
}

/// A validated URL together with the platform folder it belongs to.
pub struct SupportedTarget {
    pub url: String,
    pub platform: &'static str,
}

/// Folders prepared for a platform, returned to the frontend as a tree.
pub struct PlatformLayout {
    /// App root, e.g. `~/Downloads/yt-dlp-GUI`.
    pub app_root: PathBuf,
    /// Platform folder, e.g. `~/Downloads/yt-dlp-GUI/YouTube`.
    pub platform_dir: PathBuf,
    pub folders: Vec<String>,
}

/// Adds a scheme to bare input such as `youtube.com/watch?v=…`.
fn add_scheme(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

/// Validates the URL and resolves its platform. Rejects look-alike hosts such
/// as `youtube.com.evil.com`.
pub fn parse_supported_url(raw: &str) -> Result<SupportedTarget, AppError> {
    let candidate = add_scheme(raw);

    let parsed = Url::parse(&candidate).map_err(|_| {
        AppError::new(
            "invalid-url",
            "That does not look like a valid URL. Paste a full video or post link.",
        )
    })?;

    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::new(
            "unsupported-url",
            "Only http and https links are supported.",
        ));
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::new("invalid-url", "The URL has no host name."))?
        .to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);

    for (domain, platform) in SUPPORTED_HOSTS {
        if host == *domain || host.ends_with(&format!(".{domain}")) {
            return Ok(SupportedTarget {
                url: parsed.to_string(),
                platform,
            });
        }
    }

    Err(AppError::new(
        "unsupported-url",
        "Only YouTube, Instagram, X, and TikTok links are supported.",
    ))
}

/// Resolves the platform folder inside the app's Downloads folder and creates
/// its type folders the first time that platform is analyzed.
pub fn ensure_platform_dirs(platform: &str) -> Result<PlatformLayout, AppError> {
    let downloads = dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
        .ok_or_else(|| {
            AppError::new(
                "missing-downloads",
                "Could not locate the Downloads folder on this system.",
            )
        })?;

    let app_root = downloads.join(APP_FOLDER);
    let platform_dir = app_root.join(platform);
    let folders = platform_folders(platform);

    for folder in folders {
        std::fs::create_dir_all(platform_dir.join(folder)).map_err(|error| {
            AppError::new(
                "directory-error",
                format!("Could not create {}: {error}", platform_dir.display()),
            )
        })?;
    }

    Ok(PlatformLayout {
        app_root,
        platform_dir,
        folders: folders.iter().map(|folder| folder.to_string()).collect(),
    })
}

/// Type folder a given output format belongs to.
fn format_folder(format: OutputFormat) -> &'static str {
    match format {
        OutputFormat::Mp4 => "videos",
        OutputFormat::Mp3 => "audio",
        OutputFormat::Gif => "gifs",
        OutputFormat::Png | OutputFormat::Jpg => "images",
    }
}

/// Resolves where a download is written: the platform/type folder inside the
/// app folder by default, or a user-picked folder that must already exist.
pub fn resolve_download_dir(
    request: &DirectoryRequest,
    platform: &str,
    format: OutputFormat,
) -> Result<PathBuf, AppError> {
    match request.mode {
        DirectoryMode::Default => {
            let layout = ensure_platform_dirs(platform)?;
            let dir = layout.platform_dir.join(format_folder(format));
            std::fs::create_dir_all(&dir).map_err(|error| {
                AppError::new(
                    "directory-error",
                    format!("Could not create {}: {error}", dir.display()),
                )
            })?;
            Ok(dir)
        }
        DirectoryMode::Custom => {
            let raw = request.path.as_deref().unwrap_or("").trim();
            if raw.is_empty() {
                return Err(AppError::new(
                    "missing-directory",
                    "Choose an existing folder to save the file into.",
                ));
            }

            let dir = PathBuf::from(raw);
            let metadata = std::fs::metadata(&dir).map_err(|_| {
                AppError::new(
                    "missing-directory",
                    format!(
                        "{} does not exist. Choose an existing folder.",
                        dir.display()
                    ),
                )
            })?;

            if !metadata.is_dir() {
                return Err(AppError::new(
                    "missing-directory",
                    format!("{} is not a folder.", dir.display()),
                ));
            }

            Ok(dir)
        }
    }
}
