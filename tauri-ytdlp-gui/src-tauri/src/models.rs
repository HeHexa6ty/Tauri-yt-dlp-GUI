use serde::{Deserialize, Serialize};

/// Source media type reported by yt-dlp; drives the allowed output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Video,
    Audio,
    Gif,
    Image,
}

/// Output formats the backend can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Mp4,
    Mp3,
    Gif,
    Png,
    Jpg,
}

impl OutputFormat {
    /// File extension used for conflict checks and the final file name.
    pub fn extension(self) -> &'static str {
        match self {
            OutputFormat::Mp4 => "mp4",
            OutputFormat::Mp3 => "mp3",
            OutputFormat::Gif => "gif",
            OutputFormat::Png => "png",
            OutputFormat::Jpg => "jpg",
        }
    }
}

/// GIF encoding effort: a quick single pass or a higher-quality palette pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GifQuality {
    Simple,
    Better,
}

/// What to do when the target file already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictMode {
    /// Report the conflict back to the user (default).
    Fail,
    /// Append `_1`, `_2`, … until a free name is found.
    Rename,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DirectoryMode {
    Default,
    Custom,
}

/// Folders actually created for a platform, used to render the directory tree.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryTree {
    /// Absolute path of the app folder, e.g. `~/Downloads/yt-dlp-GUI`.
    pub root: String,
    pub platform: String,
    pub folders: Vec<String>,
}

/// Metadata returned to the frontend after a successful analysis.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub id: String,
    pub title: String,
    pub uploader: Option<String>,
    pub thumbnail: Option<String>,
    pub duration: Option<f64>,
    pub webpage_url: String,
    pub media_type: MediaType,
    /// Folder name of the source platform, e.g. "YouTube".
    pub platform: String,
    /// Platform folder created under the global Downloads folder.
    pub default_directory: String,
    pub directory_tree: DirectoryTree,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryRequest {
    pub mode: DirectoryMode,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub url: String,
    /// Title/id come from the earlier analysis and are used for the file name.
    pub title: String,
    pub id: String,
    /// Detected source type; image posts need the thumbnail pipeline because
    /// yt-dlp exposes no downloadable format for them.
    pub media_type: MediaType,
    pub format: OutputFormat,
    pub gif_quality: Option<GifQuality>,
    /// Defaults to `fail` so the frontend can ask the user what to do.
    pub conflict: Option<ConflictMode>,
    pub directory: DirectoryRequest,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub id: String,
    /// `downloading` while yt-dlp runs, `converting` during FFmpeg.
    pub stage: String,
    pub percent: f64,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    /// Bytes per second.
    pub speed: Option<f64>,
    /// Seconds remaining.
    pub eta: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    pub id: String,
    pub file_path: String,
    pub format: OutputFormat,
}
