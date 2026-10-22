use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use tokio::time::timeout;

use crate::error::AppError;
use crate::ytdlp::ytdlp_command;

/// X's extractor deliberately drops photos (`type != 'photo'`), so a photo post
/// reaches yt-dlp with no formats and no thumbnails. The public syndication
/// endpoint that powers embedded posts still exposes them as `mediaDetails`.
const SYNDICATION_ENDPOINT: &str = "https://cdn.syndication.twimg.com/tweet-result";

/// The endpoint does not validate the token, but it requires a non-empty one.
const SYNDICATION_TOKEN: &str = "ytdlp-gui";

/// Older or restricted posts come back from the syndication endpoint as a
/// `TweetTombstone`, even when the post itself is still viewable. FxTwitter
/// mirrors the public post metadata (including every photo), so it is used only
/// as a fallback when the syndication response has no photos to offer.
const FXTWITTER_ENDPOINT: &str = "https://api.fxtwitter.com";

/// Guard against a hung metadata request; the photo download itself keeps the
/// normal yt-dlp progress behaviour.
const METADATA_TIMEOUT: Duration = Duration::from_secs(30);

/// One image attached to a post, already pointing at the full-size file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TweetPhoto {
    pub url: String,
}

/// A single entry of the syndication response's `mediaDetails` list.
#[derive(Debug, Clone, Deserialize)]
struct MediaDetail {
    /// `photo`, `video`, or `animated_gif`.
    #[serde(rename = "type")]
    media_type: Option<String>,
    media_url_https: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TweetResult {
    /// `Tweet`, `TweetTombstone`, or absent when the endpoint returns `{}`.
    #[serde(default, rename = "__typename")]
    typename: Option<String>,
    #[serde(default, rename = "mediaDetails")]
    media_details: Vec<MediaDetail>,
}

impl TweetResult {
    /// A tombstoned (or empty) response does not describe a usable post, so the
    /// caller should try the fallback source instead of reporting "no photos".
    fn is_unavailable(&self) -> bool {
        self.typename.as_deref() == Some("TweetTombstone") || self.typename.is_none()
    }
}

/// FxTwitter wraps the post in `{ "code": 200, "tweet": { ... } }`.
#[derive(Debug, Clone, Default, Deserialize)]
struct FxResponse {
    #[serde(default)]
    tweet: Option<FxTweet>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FxTweet {
    #[serde(default)]
    media: Option<FxMedia>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FxMedia {
    #[serde(default)]
    photos: Vec<FxPhoto>,
}

#[derive(Debug, Clone, Deserialize)]
struct FxPhoto {
    #[serde(rename = "type")]
    media_type: Option<String>,
    url: Option<String>,
}

/// Pulls the numeric status id out of an X post URL. The extractor's `info.id`
/// is the media id for videos, so the URL is the reliable source of the post id.
pub fn tweet_id(url: &str) -> Option<&str> {
    let parts: Vec<&str> = url.split('/').collect();

    for (index, part) in parts.iter().enumerate() {
        if !matches!(*part, "status" | "statuses") {
            continue;
        }

        let Some(candidate) = parts.get(index + 1) else {
            continue;
        };

        // Stop at `/photo/1` or any other suffix.
        let id = candidate
            .split(|character: char| !character.is_ascii_digit())
            .next()
            .unwrap_or_default();
        if !id.is_empty() {
            return Some(id);
        }
    }

    None
}

/// X serves a resized copy by default; `name=orig` is the uploaded original.
/// FxTwitter already returns original-size links, so avoid adding the query twice.
fn original_size_url(url: &str) -> String {
    if url.contains("name=orig") {
        return url.to_string();
    }

    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}name=orig")
}

/// Keeps the photo attachments, skipping videos and animated GIFs.
fn photos_from(tweet: &TweetResult) -> Vec<TweetPhoto> {
    tweet
        .media_details
        .iter()
        .filter(|detail| detail.media_type.as_deref() == Some("photo"))
        .filter_map(|detail| detail.media_url_https.as_deref())
        .map(|url| TweetPhoto {
            url: original_size_url(url),
        })
        .collect()
}

/// Maps the FxTwitter photo list onto the same shape as syndication photos.
fn photos_from_fx(response: &FxResponse) -> Vec<TweetPhoto> {
    response
        .tweet
        .as_ref()
        .and_then(|tweet| tweet.media.as_ref())
        .map(|media| {
            media
                .photos
                .iter()
                .filter(|photo| photo.media_type.as_deref() == Some("photo"))
                .filter_map(|photo| photo.url.as_deref())
                .map(|url| TweetPhoto {
                    url: original_size_url(url),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A unique scratch folder so concurrent analyses cannot read each other's file.
fn scratch_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis().to_string())
        .unwrap_or_else(|_| "post".to_string());

    std::env::temp_dir()
        .join("yt-dlp-gui")
        .join(format!("x-{unique}"))
}

/// Reads the first file yt-dlp saved in `scratch` as UTF-8 text.
fn read_first_file(scratch: &Path) -> Result<String, AppError> {
    let path = std::fs::read_dir(scratch)
        .map_err(|error| {
            AppError::new(
                "analyze-failed",
                format!("Could not read the X response: {error}"),
            )
        })?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.is_file())
        .ok_or_else(|| AppError::new("analyze-failed", "X did not return data for this post."))?;

    std::fs::read_to_string(&path).map_err(|error| {
        AppError::new(
            "analyze-failed",
            format!("Could not read the X response: {error}"),
        )
    })
}

/// Fetches a JSON document through yt-dlp's generic extractor. X and FxTwitter
/// both expose post metadata as a direct link, and yt-dlp already bundles the
/// networking needed for it, so no second HTTP/TLS stack has to be added.
async fn download_json(url: &str) -> Result<String, AppError> {
    let scratch = scratch_dir();
    std::fs::create_dir_all(&scratch).map_err(|error| {
        AppError::new(
            "directory-error",
            format!("Could not create a temporary folder: {error}"),
        )
    })?;

    let mut command = ytdlp_command();
    command
        .args(["--no-warnings", "--no-playlist", "--no-progress", "-o"])
        .arg(scratch.join("data.%(ext)s").to_string_lossy().to_string())
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let output = match timeout(METADATA_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            let _ = std::fs::remove_dir_all(&scratch);
            return Err(if error.kind() == std::io::ErrorKind::NotFound {
                AppError::new(
                    "yt-dlp-missing",
                    "yt-dlp was not found. Install it or set YT_DLP_PATH.",
                )
            } else {
                AppError::new("yt-dlp-error", format!("Could not run yt-dlp: {error}"))
            });
        }
        Err(_) => {
            let _ = std::fs::remove_dir_all(&scratch);
            return Err(AppError::new(
                "timeout",
                "Reading the X post took too long. Check your connection and try again.",
            ));
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let line = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("X did not return data for this post.");
        let message = line
            .trim()
            .strip_prefix("ERROR: ")
            .unwrap_or(line.trim())
            .to_string();
        let _ = std::fs::remove_dir_all(&scratch);
        return Err(AppError::new("analyze-failed", message));
    }

    let json = read_first_file(&scratch);
    let _ = std::fs::remove_dir_all(&scratch);
    json
}

/// Asks the syndication endpoint for a post. `None` means the response did not
/// describe a usable post (a tombstone, an empty object, or malformed JSON), so
/// the caller should try the fallback source. A usable post without photos
/// yields `Some(empty)` and is not retried elsewhere.
async fn fetch_syndication_photos(id: &str) -> Result<Option<Vec<TweetPhoto>>, AppError> {
    let endpoint = format!("{SYNDICATION_ENDPOINT}?id={id}&token={SYNDICATION_TOKEN}");
    let raw = download_json(&endpoint).await?;

    let Ok(tweet) = serde_json::from_str::<TweetResult>(&raw) else {
        return Ok(None);
    };

    let photos = photos_from(&tweet);
    if photos.is_empty() && tweet.is_unavailable() {
        return Ok(None);
    }

    Ok(Some(photos))
}

/// Fallback for tombstoned posts: FxTwitter still mirrors their media.
async fn fetch_fxtwitter_photos(id: &str) -> Result<Vec<TweetPhoto>, AppError> {
    let endpoint = format!("{FXTWITTER_ENDPOINT}/status/{id}");
    let raw = download_json(&endpoint).await?;

    let response: FxResponse = serde_json::from_str(&raw).map_err(|error| {
        AppError::new(
            "analyze-failed",
            format!("X returned an unexpected response: {error}"),
        )
    })?;

    Ok(photos_from_fx(&response))
}

/// Fetches the photos of an X post through the public syndication endpoint,
/// falling back to FxTwitter when the post is tombstoned there. A post without
/// photos yields an empty list rather than an error.
pub async fn fetch_photos(url: &str) -> Result<Vec<TweetPhoto>, AppError> {
    let id = tweet_id(url)
        .ok_or_else(|| AppError::new("invalid-url", "That X link has no post id in it."))?;

    match fetch_syndication_photos(id).await {
        Ok(Some(photos)) => return Ok(photos),
        Ok(None) => {}
        Err(error) => {
            // A syndication failure should not hide an otherwise usable post.
            return match fetch_fxtwitter_photos(id).await {
                Ok(photos) if !photos.is_empty() => Ok(photos),
                _ => Err(error),
            };
        }
    }

    fetch_fxtwitter_photos(id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_the_status_id_from_common_x_urls() {
        assert_eq!(
            tweet_id("https://x.com/WhiteHouse/status/2096657477928448250"),
            Some("2096657477928448250")
        );
        assert_eq!(
            tweet_id("https://twitter.com/i/web/status/2096657477928448250/photo/1"),
            Some("2096657477928448250")
        );
        assert_eq!(
            tweet_id("https://x.com/i/status/2096657477928448250"),
            Some("2096657477928448250")
        );
        assert_eq!(tweet_id("https://twitter.com/statuses/20"), Some("20"));
        // Profiles and other X pages have no post id.
        assert_eq!(tweet_id("https://x.com/WhiteHouse"), None);
        assert_eq!(tweet_id("https://x.com/status/"), None);
    }

    #[test]
    fn keeps_only_photos_and_requests_the_original_size() {
        let payload = r#"{
            "mediaDetails": [
                {"type": "photo", "media_url_https": "https://pbs.twimg.com/media/AAA.jpg"},
                {"type": "video", "media_url_https": "https://pbs.twimg.com/media/BBB.jpg"},
                {"type": "photo", "media_url_https": "https://pbs.twimg.com/media/CCC.jpg"}
            ]
        }"#;
        let tweet: TweetResult = serde_json::from_str(payload).expect("payload should parse");
        let photos = photos_from(&tweet);

        assert_eq!(photos.len(), 2);
        assert_eq!(
            photos[0].url,
            "https://pbs.twimg.com/media/AAA.jpg?name=orig"
        );
        assert_eq!(
            photos[1].url,
            "https://pbs.twimg.com/media/CCC.jpg?name=orig"
        );
    }

    #[test]
    fn posts_without_media_yield_no_photos() {
        let tweet: TweetResult = serde_json::from_str("{}").expect("payload should parse");
        assert!(photos_from(&tweet).is_empty());
    }

    #[test]
    fn detects_tombstoned_and_empty_responses() {
        let tombstone: TweetResult =
            serde_json::from_str(r#"{"__typename":"TweetTombstone","tombstone":{}}"#)
                .expect("payload should parse");
        assert!(tombstone.is_unavailable());

        let empty: TweetResult = serde_json::from_str("{}").expect("payload should parse");
        assert!(empty.is_unavailable());

        let usable: TweetResult =
            serde_json::from_str(r#"{"__typename":"Tweet","mediaDetails":[]}"#)
                .expect("payload should parse");
        assert!(!usable.is_unavailable());
    }

    #[test]
    fn reads_photos_from_the_fallback_payload() {
        let payload = r#"{
            "code": 200,
            "message": "OK",
            "tweet": {
                "media": {
                    "all": [
                        {"type": "photo", "url": "https://pbs.twimg.com/media/AAA.jpg?name=orig"},
                        {"type": "video", "url": "https://video.twimg.com/BBB.mp4"}
                    ],
                    "photos": [
                        {"type": "photo", "url": "https://pbs.twimg.com/media/AAA.jpg?name=orig"},
                        {"type": "photo", "url": "https://pbs.twimg.com/media/CCC.jpg"}
                    ]
                }
            }
        }"#;
        let response: FxResponse = serde_json::from_str(payload).expect("payload should parse");
        let photos = photos_from_fx(&response);

        assert_eq!(photos.len(), 2);
        assert_eq!(
            photos[0].url,
            "https://pbs.twimg.com/media/AAA.jpg?name=orig"
        );
        assert_eq!(
            photos[1].url,
            "https://pbs.twimg.com/media/CCC.jpg?name=orig"
        );
    }

    #[test]
    fn fallback_payload_without_media_yields_no_photos() {
        let response: FxResponse = serde_json::from_str(r#"{"code":404,"message":"Not Found"}"#)
            .expect("payload should parse");
        assert!(photos_from_fx(&response).is_empty());
    }
}
