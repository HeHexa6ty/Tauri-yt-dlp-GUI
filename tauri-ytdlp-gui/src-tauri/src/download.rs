use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::error::AppError;
use crate::models::{
    ConflictMode, DownloadProgress, DownloadRequest, DownloadResult, GifQuality, MediaType,
    OutputFormat,
};
use crate::paths::{parse_supported_url, resolve_download_dir};
use crate::x_api;
use crate::ytdlp::{ACCOUNT_REQUIRED_MESSAGE, ffmpeg_command, requires_account, ytdlp_command};

/// Only one download may run at a time; a second request is rejected.
#[derive(Default)]
pub struct DownloadState(pub AtomicBool);

/// RAII flag so the "busy" state is always released, even when a download
/// fails or the future is dropped.
pub struct DownloadGuard<'a>(&'a AtomicBool);

impl<'a> DownloadGuard<'a> {
    pub fn acquire(flag: &'a AtomicBool) -> Result<Self, AppError> {
        if flag.swap(true, Ordering::SeqCst) {
            return Err(AppError::new(
                "download-in-progress",
                "A download is already running. Wait for it to finish.",
            ));
        }
        Ok(Self(flag))
    }
}

impl Drop for DownloadGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Progress is reported by yt-dlp as a single stdout line we can parse. The
/// `download:` part selects the progress type; the marker after it makes the
/// line easy to tell apart from yt-dlp's other output.
const PROGRESS_TEMPLATE: &str = "download:ytdlp-gui-progress:%(progress.downloaded_bytes)s;%(progress.total_bytes)s;%(progress.total_bytes_estimate)s;%(progress.speed)s;%(progress.eta)s";
const PROGRESS_EVENT: &str = "download-progress";
const PROGRESS_PREFIX: &str = "ytdlp-gui-progress:";

/// Characters that are unsafe in file names on Windows and/or Linux.
const ILLEGAL_FILENAME_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
/// Long titles are truncated so the full path stays well under filesystem limits.
const MAX_FILENAME_LENGTH: usize = 120;

/// Makes a title safe to use as a file name while keeping it readable.
pub fn sanitize_filename(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|character| {
            if character.is_control() || ILLEGAL_FILENAME_CHARS.contains(&character) {
                '_'
            } else {
                character
            }
        })
        .collect();

    // Collapse runs of whitespace and drop trailing dots/spaces (Windows rules).
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_matches(|character| character == '.' || character == ' ');
    let limited: String = trimmed.chars().take(MAX_FILENAME_LENGTH).collect();

    if limited.is_empty() {
        "media".to_string()
    } else {
        limited
    }
}

/// yt-dlp uses `%` in output templates, so a literal one has to be escaped.
fn escape_template(value: &str) -> String {
    value.replace('%', "%%")
}

fn timestamp_id() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis().to_string())
        .unwrap_or_else(|_| "download".to_string())
}

/// A parses numeric field that yt-dlp may report as `NA` or leave empty.
fn parse_optional_u64(value: &str) -> Option<u64> {
    value.trim().parse::<u64>().ok()
}

fn parse_optional_f64(value: &str) -> Option<f64> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite() && *number > 0.0)
}

/// Converts one `download:` progress line into an event payload.
fn parse_progress_line(line: &str, id: &str) -> Option<DownloadProgress> {
    let rest = line.trim().strip_prefix(PROGRESS_PREFIX)?;
    let mut fields = rest.split(';');

    let downloaded = parse_optional_u64(fields.next()?)?;
    let total = parse_optional_u64(fields.next()?);
    let estimate = parse_optional_u64(fields.next()?);
    let speed = parse_optional_f64(fields.next().unwrap_or(""));
    let eta = parse_optional_f64(fields.next().unwrap_or(""));

    // Unknown totals are common for live or fragmented streams.
    let total = total.or(estimate);
    let percent = total
        .filter(|value| *value > 0)
        .map(|value| (downloaded as f64 / value as f64) * 100.0)
        .unwrap_or(0.0);

    Some(DownloadProgress {
        id: id.to_string(),
        stage: "downloading".to_string(),
        percent: percent.clamp(0.0, 100.0),
        downloaded_bytes: downloaded,
        total_bytes: total,
        speed,
        eta,
    })
}

/// Builds one file name per item. A single item keeps the plain base name;
/// multiple items are numbered so a carousel always forms a clear set.
fn output_names(base: &str, count: usize, suffix: Option<usize>) -> Vec<String> {
    (0..count.max(1))
        .map(|index| {
            let item = if count <= 1 {
                base.to_string()
            } else {
                format!("{base}_{}", index + 1)
            };

            match suffix {
                Some(suffix) => format!("{item}_{suffix}"),
                None => item,
            }
        })
        .collect()
}

/// Resolves free file names for one or more media items. If any name is taken,
/// `fail` reports the conflict and `rename` suffixes the whole set so the files
/// still sort as one group.
pub fn resolve_outputs(
    directory: &Path,
    base: &str,
    extension: &str,
    count: usize,
    conflict: ConflictMode,
) -> Result<Vec<PathBuf>, AppError> {
    let count = count.max(1);

    let to_paths = |names: &[String]| -> Vec<PathBuf> {
        names
            .iter()
            .map(|name| directory.join(format!("{name}.{extension}")))
            .collect()
    };
    let is_free = |names: &[String]| -> bool {
        names
            .iter()
            .all(|name| !directory.join(format!("{name}.{extension}")).exists())
    };

    let plain = output_names(base, count, None);
    if is_free(&plain) {
        return Ok(to_paths(&plain));
    }

    if conflict == ConflictMode::Fail {
        let existing = plain
            .iter()
            .find(|name| directory.join(format!("{name}.{extension}")).exists())
            .expect("a conflicting name exists");
        return Err(AppError::new(
            "file-exists",
            format!("{existing}.{extension} already exists."),
        ));
    }

    for suffix in 1..1000 {
        let candidate = output_names(base, count, Some(suffix));
        if is_free(&candidate) {
            return Ok(to_paths(&candidate));
        }
    }

    Err(AppError::new(
        "file-exists",
        "Could not find available file names.",
    ))
}

/// Finds a free file name, or reports the conflict so the UI can ask the user.
/// Returns the final path together with the base name (including any suffix).
pub fn resolve_output(
    directory: &Path,
    base: &str,
    extension: &str,
    conflict: ConflictMode,
) -> Result<(PathBuf, String), AppError> {
    let path = resolve_outputs(directory, base, extension, 1, conflict)?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::new("file-exists", "Could not find an available file name."))?;

    let resolved_base = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| base.to_string());

    Ok((path, resolved_base))
}

/// Shared yt-dlp arguments: quiet output, no playlists, progress on stdout.
fn base_args() -> Vec<String> {
    vec![
        "--no-playlist".to_string(),
        "--no-warnings".to_string(),
        "--windows-filenames".to_string(),
        "--no-overwrites".to_string(),
        "--newline".to_string(),
        "--progress-template".to_string(),
        PROGRESS_TEMPLATE.to_string(),
    ]
}

/// Format-specific download arguments. GIF and image formats download the best
/// source untouched and are converted by FFmpeg afterwards.
fn format_args(format: OutputFormat) -> Vec<String> {
    match format {
        OutputFormat::Mp4 => vec![
            "-f".to_string(),
            "bv*[ext=mp4]+ba[ext=m4a]/b[ext=mp4]/b".to_string(),
            "--merge-output-format".to_string(),
            "mp4".to_string(),
        ],
        OutputFormat::Mp3 => vec![
            "-x".to_string(),
            "--audio-format".to_string(),
            "mp3".to_string(),
            "--audio-quality".to_string(),
            "0".to_string(),
        ],
        OutputFormat::Gif | OutputFormat::Png | OutputFormat::Jpg => Vec::new(),
    }
}

/// Adds `--ffmpeg-location` only when the user pointed us at a custom binary.
fn apply_ffmpeg_location(command: &mut Command) {
    if let Ok(location) = std::env::var("FFMPEG_PATH") {
        command.arg("--ffmpeg-location").arg(location);
    }
}

/// Maps a spawn failure onto a user-facing error.
fn map_spawn_error(error: std::io::Error, binary: &str, code: &'static str) -> AppError {
    if error.kind() == std::io::ErrorKind::NotFound {
        AppError::new(
            code,
            format!("{binary} was not found. Install it or set the matching path variable."),
        )
    } else {
        AppError::new(code, format!("Could not run {binary}: {error}"))
    }
}

/// Turns yt-dlp's stderr into a readable error message.
fn map_ytdlp_stderr(stderr: &str) -> AppError {
    let line = stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("yt-dlp could not download this media.");
    let message = line
        .trim()
        .strip_prefix("ERROR: ")
        .unwrap_or(line.trim())
        .to_string();

    if requires_account(&message) {
        return AppError::new("login-required", ACCOUNT_REQUIRED_MESSAGE);
    }

    AppError::new("download-failed", message)
}

/// Runs yt-dlp while streaming progress events to the frontend.
async fn run_with_progress(
    app: &AppHandle,
    mut command: Command,
    id: &str,
) -> Result<(), AppError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| map_spawn_error(error, "yt-dlp", "yt-dlp-missing"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::new("download-failed", "Could not read yt-dlp output."))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::new("download-failed", "Could not read yt-dlp output."))?;

    // Drain stdout (progress) and stderr (errors) together so a full pipe on
    // either side can never deadlock the download.
    let stdout_reader = async {
        let mut lines = BufReader::new(stdout).lines();
        while let Some(line) = lines.next_line().await.map_err(|error| {
            AppError::new("download-failed", format!("Lost yt-dlp output: {error}"))
        })? {
            if let Some(progress) = parse_progress_line(&line, id) {
                let _ = app.emit(PROGRESS_EVENT, progress);
            }
        }
        Ok::<(), AppError>(())
    };

    let stderr_reader = async {
        let mut buffer = String::new();
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            buffer.push_str(&line);
            buffer.push('\n');
        }
        buffer
    };

    let (stdout_result, stderr) = tokio::join!(stdout_reader, stderr_reader);
    stdout_result?;

    let status = child
        .wait()
        .await
        .map_err(|error| AppError::new("download-failed", format!("yt-dlp failed: {error}")))?;

    if !status.success() {
        return Err(map_ytdlp_stderr(&stderr));
    }

    Ok(())
}

/// Downloads directly into `directory`, letting yt-dlp pick the extension.
async fn run_download(
    app: &AppHandle,
    request: &DownloadRequest,
    output_base: &str,
    directory: &Path,
    id: &str,
) -> Result<(), AppError> {
    let mut command = ytdlp_command();
    command.args(base_args());
    command.args(format_args(request.format));
    apply_ffmpeg_location(&mut command);

    let template = format!(
        "{}.%(ext)s",
        escape_template(&directory.join(output_base).to_string_lossy())
    );
    command.arg("-o").arg(template);
    command.arg(&request.url);

    run_with_progress(app, command, id).await
}

/// Downloads into a scratch folder; used as the input for FFmpeg conversions.
async fn run_download_temp(
    app: &AppHandle,
    request: &DownloadRequest,
    output_base: &str,
    temp: &Path,
    id: &str,
) -> Result<PathBuf, AppError> {
    let mut command = ytdlp_command();
    command.args(base_args());
    command.args(format_args(request.format));
    apply_ffmpeg_location(&mut command);

    let template = format!(
        "{}.%(ext)s",
        escape_template(&temp.join(output_base).to_string_lossy())
    );
    command.arg("-o").arg(template);
    command.arg(&request.url);

    run_with_progress(app, command, id).await?;

    first_file(temp)
}

/// Flags for image posts: yt-dlp has no downloadable format for a photo, but
/// `--write-thumbnail` saves the largest image it extracted. Carousels are
/// playlists, so every entry produces one thumbnail.
fn image_args() -> Vec<String> {
    vec![
        "--no-playlist".to_string(),
        "--no-warnings".to_string(),
        "--windows-filenames".to_string(),
        "--newline".to_string(),
        "--ignore-no-formats-error".to_string(),
        "--skip-download".to_string(),
        "--write-thumbnail".to_string(),
        "--progress-template".to_string(),
        PROGRESS_TEMPLATE.to_string(),
    ]
}

/// Builds the yt-dlp output template for scratch image downloads. Only the
/// folder is escaped: escaping `%(autonumber)s` would turn it into a literal
/// file name and every carousel item would overwrite the previous one.
fn image_output_template(temp: &Path) -> String {
    format!(
        "{}.%(autonumber)s.%(ext)s",
        escape_template(&temp.join("item").to_string_lossy())
    )
}

/// Downloads every image (or thumbnail) of a post into a scratch folder.
async fn run_download_temp_images(
    app: &AppHandle,
    request: &DownloadRequest,
    temp: &Path,
    id: &str,
) -> Result<Vec<PathBuf>, AppError> {
    let mut command = ytdlp_command();
    command.args(image_args());

    // `autonumber` keeps every carousel item in its own file and in order.
    command.arg("-o").arg(image_output_template(temp));
    command.arg(&request.url);

    run_with_progress(app, command, id).await?;

    sorted_files(temp)
}

/// Fetches and downloads every photo of an X post. yt-dlp exposes no format for
/// photos, so the image URLs come from the post's syndication data and are
/// fetched as plain direct links.
async fn run_download_temp_x_photos(
    app: &AppHandle,
    url: &str,
    temp: &Path,
    id: &str,
) -> Result<Vec<PathBuf>, AppError> {
    let photos = x_api::fetch_photos(url).await?;
    if photos.is_empty() {
        return Err(AppError::new(
            "download-failed",
            "This X post has no photo to download.",
        ));
    }

    let mut command = ytdlp_command();
    command.args([
        "--no-warnings",
        "--no-playlist",
        "--windows-filenames",
        "--no-overwrites",
        "--newline",
        "--progress-template",
        PROGRESS_TEMPLATE,
    ]);

    command.arg("-o").arg(image_output_template(temp));
    for photo in &photos {
        command.arg(&photo.url);
    }

    run_with_progress(app, command, id).await?;

    sorted_files(temp)
}

/// Returns the single downloaded file inside a scratch folder.
fn first_file(directory: &Path) -> Result<PathBuf, AppError> {
    let entries = std::fs::read_dir(directory).map_err(|error| {
        AppError::new(
            "download-failed",
            format!("Could not read {}: {error}", directory.display()),
        )
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            return Ok(path);
        }
    }

    Err(AppError::new(
        "download-failed",
        "yt-dlp did not produce a file to convert.",
    ))
}

/// Returns every downloaded file in a scratch folder, in `autonumber` order.
fn sorted_files(directory: &Path) -> Result<Vec<PathBuf>, AppError> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(directory)
        .map_err(|error| {
            AppError::new(
                "download-failed",
                format!("Could not read {}: {error}", directory.display()),
            )
        })?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();

    // `autonumber` is zero-padded, so plain path ordering matches the post order.
    files.sort();

    if files.is_empty() {
        return Err(AppError::new(
            "download-failed",
            "yt-dlp did not produce a file to convert.",
        ));
    }

    Ok(files)
}

/// Runs FFmpeg and surfaces its last stderr line when it fails.
async fn run_ffmpeg(args: Vec<String>) -> Result<(), AppError> {
    let mut command = ffmpeg_command();
    command
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let output = command
        .output()
        .await
        .map_err(|error| map_spawn_error(error, "ffmpeg", "ffmpeg-missing"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let line = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("FFmpeg could not convert this media.");
        return Err(AppError::new("convert-failed", line.trim().to_string()));
    }

    Ok(())
}

fn string_args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

/// Converts a downloaded clip to GIF: quick single pass, or a palette pass for
/// noticeably better colours.
async fn convert_gif(input: &Path, output: &Path, quality: GifQuality) -> Result<(), AppError> {
    let filter = "fps=15,scale=480:-1:flags=lanczos";
    let input = input.to_string_lossy().to_string();
    let output_path = output.to_string_lossy().to_string();

    match quality {
        GifQuality::Simple => {
            run_ffmpeg(string_args(&[
                "-y",
                "-i",
                &input,
                "-vf",
                filter,
                &output_path,
            ]))
            .await
        }
        GifQuality::Better => {
            let palette = output.with_extension("palette.png");
            let palette_path = palette.to_string_lossy().to_string();
            let palette_filter = format!("{filter},palettegen");
            let use_filter = format!("{filter}[x];[x][1:v]paletteuse");

            run_ffmpeg(string_args(&[
                "-y",
                "-i",
                &input,
                "-vf",
                &palette_filter,
                &palette_path,
            ]))
            .await?;

            let result = run_ffmpeg(string_args(&[
                "-y",
                "-i",
                &input,
                "-i",
                &palette_path,
                "-lavfi",
                &use_filter,
                &output_path,
            ]))
            .await;

            let _ = std::fs::remove_file(&palette);
            result
        }
    }
}

/// Converts a downloaded image to PNG or JPG.
async fn convert_image(input: &Path, output: &Path, format: OutputFormat) -> Result<(), AppError> {
    let input = input.to_string_lossy().to_string();
    let output_path = output.to_string_lossy().to_string();

    let mut args = vec!["-y".to_string(), "-i".to_string(), input];
    args.extend(string_args(&["-frames:v", "1"]));
    if format == OutputFormat::Jpg {
        args.extend(string_args(&["-q:v", "2"]));
    }
    args.push(output_path);

    run_ffmpeg(args).await
}

/// Creates an empty scratch folder for a single download.
fn make_temp_dir(id: &str) -> Result<PathBuf, AppError> {
    let dir = std::env::temp_dir().join("yt-dlp-gui").join(id);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|error| {
        AppError::new(
            "directory-error",
            format!("Could not create a temporary folder: {error}"),
        )
    })?;
    Ok(dir)
}

/// Progress payload shown while FFmpeg works on the downloaded images.
fn converting_progress(id: &str) -> DownloadProgress {
    DownloadProgress {
        id: id.to_string(),
        stage: "converting".to_string(),
        percent: 100.0,
        downloaded_bytes: 0,
        total_bytes: None,
        speed: None,
        eta: None,
    }
}

/// Downloads every image of a post, converts each one, then moves the results
/// into the destination. File conflicts are checked once the real item count is
/// known, so a carousel is always treated as a single set.
async fn download_image_set(
    app: &AppHandle,
    request: &DownloadRequest,
    platform: &str,
    directory: &Path,
    base: &str,
    id: &str,
    conflict: ConflictMode,
    gif_quality: GifQuality,
) -> Result<Vec<PathBuf>, AppError> {
    let temp = make_temp_dir(id)?;

    let outcome = async {
        // X photos are hidden from yt-dlp, so they come from the post's
        // syndication data; Instagram and other posts use thumbnails.
        let sources = if platform == "X" {
            run_download_temp_x_photos(app, &request.url, &temp, id).await?
        } else {
            run_download_temp_images(app, request, &temp, id).await?
        };

        let extension = request.format.extension();
        let paths = resolve_outputs(directory, base, extension, sources.len(), conflict)?;
        let _ = app.emit(PROGRESS_EVENT, converting_progress(id));

        for (index, source) in sources.iter().enumerate() {
            let converted = temp.join(format!("converted_{index}.{extension}"));
            match request.format {
                OutputFormat::Gif => convert_gif(source, &converted, gif_quality).await?,
                _ => convert_image(source, &converted, request.format).await?,
            }

            std::fs::copy(&converted, &paths[index]).map_err(|error| {
                AppError::new(
                    "download-failed",
                    format!("Could not save {}: {error}", paths[index].display()),
                )
            })?;
        }

        Ok(paths)
    }
    .await;

    let _ = std::fs::remove_dir_all(&temp);
    outcome
}

/// Full download pipeline: resolve the target, download, convert, report.
pub async fn download(
    app: &AppHandle,
    request: DownloadRequest,
) -> Result<DownloadResult, AppError> {
    let target = parse_supported_url(&request.url)?;
    let platform = target.platform;

    // YouTube only exposes videos and audio, so GIF output is not offered.
    if platform == "YouTube" && request.format == OutputFormat::Gif {
        return Err(AppError::new(
            "unsupported-format",
            "YouTube sources cannot be exported as GIF.",
        ));
    }

    let directory = resolve_download_dir(&request.directory, platform, request.format)?;
    let base = format!(
        "{} [{}]",
        sanitize_filename(&request.title),
        sanitize_filename(&request.id)
    );
    let conflict = request.conflict.unwrap_or(ConflictMode::Fail);

    let id = timestamp_id();

    // Photo posts have no downloadable format, so their pixels come from
    // thumbnails (other platforms) or the post's syndication data (X).
    let image_source = request.media_type == MediaType::Image;

    let paths = match request.format {
        OutputFormat::Mp4 | OutputFormat::Mp3 => {
            let (final_path, output_base) =
                resolve_output(&directory, &base, request.format.extension(), conflict)?;
            run_download(app, &request, &output_base, &directory, &id).await?;
            vec![final_path]
        }
        OutputFormat::Png | OutputFormat::Jpg => {
            download_image_set(
                app,
                &request,
                platform,
                &directory,
                &base,
                &id,
                conflict,
                GifQuality::Better,
            )
            .await?
        }
        OutputFormat::Gif if image_source => {
            download_image_set(
                app,
                &request,
                platform,
                &directory,
                &base,
                &id,
                conflict,
                request.gif_quality.unwrap_or(GifQuality::Better),
            )
            .await?
        }
        OutputFormat::Gif => {
            let (final_path, output_base) =
                resolve_output(&directory, &base, request.format.extension(), conflict)?;
            let temp = make_temp_dir(&id)?;
            let source = run_download_temp(app, &request, &output_base, &temp, &id).await?;
            let _ = app.emit(PROGRESS_EVENT, converting_progress(&id));
            let quality = request.gif_quality.unwrap_or(GifQuality::Better);
            convert_gif(&source, &final_path, quality).await?;
            let _ = std::fs::remove_dir_all(&temp);
            vec![final_path]
        }
    };

    Ok(DownloadResult {
        id,
        file_paths: paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
        format: request.format,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_illegal_characters_and_collapses_space() {
        assert_eq!(sanitize_filename("a/b:c*d?e"), "a_b_c_d_e");
        assert_eq!(sanitize_filename("  hello   world  "), "hello world");
        // Windows forbids trailing dots/spaces.
        assert_eq!(sanitize_filename("name..."), "name");
        // Nothing usable left falls back to a safe default.
        assert_eq!(sanitize_filename("   "), "media");
        assert_eq!(sanitize_filename(""), "media");
    }

    #[test]
    fn sanitize_truncates_to_the_maximum_length() {
        let long = "x".repeat(MAX_FILENAME_LENGTH + 50);
        assert_eq!(
            sanitize_filename(&long).chars().count(),
            MAX_FILENAME_LENGTH
        );
    }

    #[test]
    fn parses_a_download_progress_line() {
        let line = "ytdlp-gui-progress:1024;4096;NA;2048;3";
        let progress = parse_progress_line(line, "42").expect("line should parse");

        assert_eq!(progress.id, "42");
        assert_eq!(progress.stage, "downloading");
        assert_eq!(progress.downloaded_bytes, 1024);
        assert_eq!(progress.total_bytes, Some(4096));
        assert_eq!(progress.speed, Some(2048.0));
        assert_eq!(progress.eta, Some(3.0));
        assert_eq!(progress.percent, 25.0);
    }

    #[test]
    fn falls_back_to_the_estimated_total_and_ignores_na() {
        let line = "ytdlp-gui-progress:1000;NA;4000;NA;NA";
        let progress = parse_progress_line(line, "1").expect("line should parse");

        assert_eq!(progress.total_bytes, Some(4000));
        assert_eq!(progress.speed, None);
        assert_eq!(progress.eta, None);
        assert_eq!(progress.percent, 25.0);
    }

    #[test]
    fn ignores_lines_without_the_marker() {
        assert!(parse_progress_line("[download] Destination: file.mp4", "1").is_none());
    }

    #[test]
    fn reports_existing_files_and_finds_a_free_rename() {
        let dir = std::env::temp_dir().join(format!("yt-dlp-gui-test-{}", timestamp_id()));
        std::fs::create_dir_all(&dir).expect("temp dir should be created");

        // No file yet: the base name is used unchanged.
        let (path, base) =
            resolve_output(&dir, "clip [id]", "mp4", ConflictMode::Fail).expect("free name");
        assert_eq!(base, "clip [id]");
        assert_eq!(path, dir.join("clip [id].mp4"));

        std::fs::write(&path, b"x").expect("write target file");

        // fail mode surfaces the conflict for the UI.
        let error = resolve_output(&dir, "clip [id]", "mp4", ConflictMode::Fail)
            .expect_err("conflict should be reported");
        assert_eq!(error.code, "file-exists");

        // rename mode appends `_1`.
        let (renamed, renamed_base) =
            resolve_output(&dir, "clip [id]", "mp4", ConflictMode::Rename).expect("free name");
        assert_eq!(renamed_base, "clip [id]_1");
        assert_eq!(renamed, dir.join("clip [id]_1.mp4"));

        std::fs::write(&renamed, b"x").expect("write renamed file");
        let (_, second_base) =
            resolve_output(&dir, "clip [id]", "mp4", ConflictMode::Rename).expect("free name");
        assert_eq!(second_base, "clip [id]_2");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn image_template_keeps_the_ytdlp_placeholders() {
        let template = image_output_template(Path::new("/tmp/yt-dlp-gui/1"));
        assert_eq!(template, "/tmp/yt-dlp-gui/1/item.%(autonumber)s.%(ext)s");
    }

    #[test]
    fn image_template_escapes_percent_signs_in_the_folder() {
        let template = image_output_template(Path::new("/tmp/100%/1"));
        assert_eq!(template, "/tmp/100%%/1/item.%(autonumber)s.%(ext)s");
    }

    #[test]
    fn resolves_one_numbered_name_per_carousel_item() {
        let dir = std::env::temp_dir().join(format!("yt-dlp-gui-test-{}", timestamp_id()));
        std::fs::create_dir_all(&dir).expect("temp dir should be created");

        let paths =
            resolve_outputs(&dir, "post [id]", "jpg", 3, ConflictMode::Fail).expect("free names");
        assert_eq!(
            paths,
            vec![
                dir.join("post [id]_1.jpg"),
                dir.join("post [id]_2.jpg"),
                dir.join("post [id]_3.jpg"),
            ]
        );

        // Any existing item makes `fail` report the set as conflicting.
        std::fs::write(&paths[1], b"x").expect("write target file");
        let error = resolve_outputs(&dir, "post [id]", "jpg", 3, ConflictMode::Fail)
            .expect_err("conflict should be reported");
        assert_eq!(error.code, "file-exists");

        // `rename` suffixes the whole set so it still looks like one group.
        let renamed =
            resolve_outputs(&dir, "post [id]", "jpg", 3, ConflictMode::Rename).expect("free names");
        assert_eq!(
            renamed,
            vec![
                dir.join("post [id]_1_1.jpg"),
                dir.join("post [id]_2_1.jpg"),
                dir.join("post [id]_3_1.jpg"),
            ]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn maps_an_instagram_age_gate_to_a_clear_message() {
        let error = map_ytdlp_stderr(
            "ERROR: [Instagram] DecrKlojBOb: Instagram sent an empty media response. \
             Check if this post is accessible in your browser without being logged-in.",
        );

        assert_eq!(error.code, "login-required");
        assert_eq!(error.message, ACCOUNT_REQUIRED_MESSAGE);
    }
}
