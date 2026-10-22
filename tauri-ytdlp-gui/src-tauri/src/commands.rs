use tauri::{AppHandle, State};

use crate::download::{self, DownloadGuard, DownloadState};
use crate::error::AppError;
use crate::models::{DirectoryTree, DownloadRequest, DownloadResult, MediaInfo, MediaType};
use crate::paths::{ensure_platform_dirs, parse_supported_url};
use crate::x_api;
use crate::ytdlp;

/// Validates the URL, asks yt-dlp for metadata, then lazily creates the
/// platform folder structure under Downloads.
#[tauri::command]
pub async fn analyze_url(url: String) -> Result<MediaInfo, AppError> {
    let target = parse_supported_url(&url)?;

    let mut info = ytdlp::analyze(&target.url).await?;

    // yt-dlp filters photos out of X posts, so a photo-only tweet is analyzed
    // with no image at all. Fill the preview from X's syndication data instead;
    // a failure here only costs the preview, not the whole analysis.
    if target.platform == "X" && info.media_type == MediaType::Image && info.thumbnail.is_none() {
        if let Ok(photos) = x_api::fetch_photos(&target.url).await {
            if let Some(photo) = photos.first() {
                info.thumbnail = Some(photo.url.clone());
            }
        }
    }

    let layout = ensure_platform_dirs(target.platform)?;
    info.platform = target.platform.to_string();
    info.default_directory = layout.platform_dir.to_string_lossy().to_string();
    info.directory_tree = DirectoryTree {
        root: layout.app_root.to_string_lossy().to_string(),
        platform: target.platform.to_string(),
        folders: layout.folders,
    };

    Ok(info)
}

/// Downloads one media item. A single download may run at a time; concurrent
/// requests are rejected with `download-in-progress`.
#[tauri::command]
pub async fn download_media(
    app: AppHandle,
    state: State<'_, DownloadState>,
    request: DownloadRequest,
) -> Result<DownloadResult, AppError> {
    let _guard = DownloadGuard::acquire(&state.0)?;
    download::download(&app, request).await
}
