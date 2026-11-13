//! YouTube addresses and resolving a track to a direct audio URL with yt-dlp.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command;
use url::Url;

use crate::cache::{cache_key, identity_from_info, MediaCache, SourceIdentity, StagedDownload, MAX_CACHE_FILE_BYTES};
use crate::{err, Result};

/// Any best audio: ffmpeg converts whatever comes.
const AUDIO_FORMAT: &str = "ba/b/bestaudio/best";
/// Player clients to try, which helps against bot checks and empty format lists.
const PLAYER_CLIENTS: [&str; 3] = ["default", "web", "web_safari"];

/// Longest one yt-dlp process may run.
const YT_DLP_TIMEOUT: Duration = Duration::from_secs(180);
/// A single file over HTTP(S), never a playlist: ffmpeg plays this address.
const STREAM_FORMAT: &str = "ba[protocol~='^https?$']/b[protocol~='^https?$']";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedYouTubeUrl {
    pub video_id: Option<String>,
    pub list_id: Option<String>,
    pub canonical_url: String,
    pub playlist_url: Option<String>,
    pub watch_url: Option<String>,
}

pub fn is_youtube_hostname(hostname: &str) -> bool {
    let host = hostname.trim_end_matches('.').to_ascii_lowercase();
    host == "youtu.be" || host == "youtube.com" || host.ends_with(".youtube.com")
}

pub fn is_youtube_url(url: &str) -> bool {
    Url::parse(url).ok().as_ref().and_then(Url::host_str).is_some_and(is_youtube_hostname)
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// The word after the first occurrence of `marker`, when it is at least `length` long
/// (`None`: any non-empty length).
fn word_after(href: &str, marker: &str, length: Option<usize>) -> Option<String> {
    let mut from = 0;
    while let Some(found) = href[from..].find(marker) {
        let start = from + found + marker.len();
        let word: String = href[start..].chars().take_while(|c| is_word(*c)).collect();
        match length {
            Some(length) if word.len() >= length => return Some(word[..length].to_owned()),
            None if !word.is_empty() => return Some(word),
            _ => from = start,
        }
    }
    None
}

fn query_value(href: &str, name: &str, length: Option<usize>) -> Option<String> {
    let query = format!("?{name}=");
    let later = format!("&{name}=");
    word_after(href, &query, length).or_else(|| word_after(href, &later, length))
}

/// Canonical addresses. music.youtube.com becomes www.youtube.com; ids come from the
/// standard `v=`, `list=`, `youtu.be` and `/shorts/` style forms.
pub fn parse_youtube_url(raw: &str) -> ParsedYouTubeUrl {
    let trimmed = raw.trim();
    let other = || ParsedYouTubeUrl {
        video_id: None,
        list_id: None,
        canonical_url: trimmed.to_owned(),
        playlist_url: None,
        watch_url: None,
    };
    let Ok(mut parsed) = Url::parse(trimmed) else {
        return other();
    };
    let Some(host) = parsed.host_str().map(str::to_ascii_lowercase) else {
        return other();
    };
    if !is_youtube_hostname(&host) {
        return other();
    }
    if matches!(host.as_str(), "music.youtube.com" | "m.youtube.com" | "youtube.com") {
        let _ = parsed.set_host(Some("www.youtube.com"));
    }

    let href = parsed.to_string();
    let list_id = query_value(&href, "list", None);
    let video_id = query_value(&href, "v", Some(11))
        .or_else(|| word_after(&href, "youtu.be/", Some(11)))
        .or_else(|| ["/shorts/", "/embed/", "/live/", "/v/"].iter().find_map(|m| word_after(&href, m, Some(11))));

    let watch_url = video_id.as_ref().map(|id| format!("https://www.youtube.com/watch?v={id}"));
    let playlist_url = list_id.as_ref().map(|id| format!("https://www.youtube.com/playlist?list={id}"));
    let canonical_url = match (&video_id, &list_id) {
        (None, Some(_)) => playlist_url.clone().expect("list id"),
        (Some(video), Some(list)) => format!("https://www.youtube.com/watch?v={video}&list={list}"),
        (Some(_), None) => watch_url.clone().expect("video id"),
        (None, None) => href,
    };
    ParsedYouTubeUrl { video_id, list_id, canonical_url, playlist_url, watch_url }
}

/// Where yt-dlp finds cookies. The file lives outside the project; it is never logged.
#[derive(Clone, Default)]
pub struct YtDlpOptions {
    pub cookie_file: Option<PathBuf>,
}

fn cookie_args(cookie_file: Option<&Path>) -> Vec<String> {
    match cookie_file {
        Some(path) => vec!["--cookies".to_owned(), path.display().to_string()],
        None => vec!["--no-cookies".to_owned(), "--no-cookies-from-browser".to_owned()],
    }
}

/// `--` and the refusal of a leading `-` stop a URL from acting as an option.
fn with_media_url(mut args: Vec<String>, url: &str) -> Result<Vec<String>> {
    if url.trim().starts_with('-') {
        return err("Invalid URL: must not start with '-'");
    }
    args.push("--".to_owned());
    args.push(url.to_owned());
    Ok(args)
}

fn stream_args(url: &str, options: &YtDlpOptions) -> Result<Vec<String>> {
    let mut args = vec!["--extractor-args".to_owned(), "youtube:player_client=default".to_owned()];
    args.extend(cookie_args(options.cookie_file.as_deref()));
    args.extend(
        ["--no-warnings", "--no-playlist", "-f", STREAM_FORMAT, "--dump-single-json", "--no-download"].map(str::to_owned),
    );
    with_media_url(args, url)
}

/// The lines that explain a failure: warnings often drown the real cause.
pub fn summarize_stderr(stderr: &str) -> String {
    const MAX_LEN: usize = 280;
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.to_ascii_lowercase().starts_with("reading options from stdin"))
        .collect();
    let lower = |line: &&&str| line.to_ascii_lowercase();
    let auth: Vec<&str> = lines
        .iter()
        .filter(|line| {
            let text = lower(line);
            ["sign in to confirm", "not a bot", "http error 403", "http error 429"].iter().any(|m| text.contains(m))
        })
        .copied()
        .collect();
    let errors: Vec<&str> = lines.iter().filter(|line| lower(line).contains("error:")).copied().collect();
    let chosen = if !auth.is_empty() {
        auth
    } else if !errors.is_empty() {
        errors
    } else {
        lines
    };
    let text: String = chosen.join(" | ").chars().take(MAX_LEN).collect();
    if text.is_empty() {
        "unknown yt-dlp error".to_owned()
    } else {
        text
    }
}

async fn run(args: Vec<String>) -> Result<std::process::Output> {
    let child = Command::new("yt-dlp")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .or_else(|error| err(format!("yt-dlp not found: {error}")))?;
    match tokio::time::timeout(YT_DLP_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(error)) => err(format!("yt-dlp failed: {error}")),
        Err(_) => err("yt-dlp timed out"),
    }
}

/// A track resolved for streaming. No `Debug`: the address is signed.
pub struct ResolvedStream {
    pub stream_url: String,
    pub id: String,
    pub title: String,
    pub artist: String,
    /// Seconds; 0 when unknown.
    pub duration: f64,
    pub thumbnail: String,
    pub url: String,
}

fn parse_resolved(stdout: &str, media_url: &str, video_id: Option<&str>) -> Result<ResolvedStream> {
    let line = stdout.lines().find(|line| line.starts_with('{')).unwrap_or(stdout.trim());
    let Ok(data) = serde_json::from_str::<serde_json::Value>(line) else {
        return err("Failed to parse yt-dlp stream output");
    };
    let text = |key: &str| data[key].as_str().filter(|value| !value.is_empty()).map(str::to_owned);
    let Some(stream_url) = text("url") else {
        return err("No stream URL in yt-dlp output");
    };
    Ok(ResolvedStream {
        stream_url,
        id: text("id").or_else(|| video_id.map(str::to_owned)).unwrap_or_default(),
        title: text("title").unwrap_or_else(|| "Unknown".to_owned()),
        artist: text("uploader").or_else(|| text("channel")).unwrap_or_else(|| "Unknown".to_owned()),
        duration: data["duration"].as_f64().unwrap_or(0.0),
        thumbnail: text("thumbnail").unwrap_or_default(),
        url: media_url.to_owned(),
    })
}

/// The one metadata record in yt-dlp's output. More than one is a playlist.
fn single_metadata_record(stdout: &str) -> Result<Option<serde_json::Value>> {
    let mut records = Vec::new();
    for line in stdout.lines().filter(|line| line.trim_start().starts_with('{')) {
        let Ok(record) = serde_json::from_str(line) else {
            return err("The link's metadata could not be read.");
        };
        records.push(record);
    }
    if records.len() > 1 {
        return err("The link is not a single track.");
    }
    Ok(records.pop())
}

/// Whether the metadata offers something to download besides storyboard images.
fn has_playable_format(info: &serde_json::Value) -> bool {
    if info["url"].is_string() && info["ext"] != "mhtml" {
        return true;
    }
    info["formats"].as_array().is_some_and(|formats| {
        formats.iter().any(|format| format["ext"] != "mhtml" && format["protocol"] != "mhtml" && format["format_note"] != "storyboard")
    })
}

fn base_args(player_clients: &str, options: &YtDlpOptions) -> Vec<String> {
    let mut args = vec!["--extractor-args".to_owned(), format!("youtube:player_client={player_clients}")];
    args.extend(cookie_args(options.cookie_file.as_deref()));
    args
}

/// Metadata for one track, trying each player client until one offers a format.
async fn dump_info(url: &str, options: &YtDlpOptions) -> Result<serde_json::Value> {
    let (mut last_code, mut last_stderr, mut bot_check) = (None, String::new(), String::new());
    for clients in PLAYER_CLIENTS {
        let mut args = base_args(clients, options);
        args.extend(["--dump-json", "--no-playlist", "--ignore-no-formats-error"].map(str::to_owned));
        let output = run(with_media_url(args, url)?).await?;
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let lower = stderr.to_ascii_lowercase();
        if lower.contains("sign in to confirm") || lower.contains("not a bot") {
            bot_check.clone_from(&stderr);
        }
        last_code = output.status.code();
        last_stderr = stderr;
        let Some(info) = single_metadata_record(&String::from_utf8_lossy(&output.stdout))? else { continue };
        if info["id"].is_string() && has_playable_format(&info) {
            return Ok(info);
        }
    }
    // A later empty answer must not hide the earlier cause.
    let cause = if bot_check.is_empty() { last_stderr } else { bot_check };
    err(format!("yt-dlp info failed (code {last_code:?}): {}", summarize_stderr(&cause)))
}

/// Download the track described by a saved info document, trying the format choices in turn.
async fn download_from_info(info_file: &Path, staging: &Path, options: &YtDlpOptions) -> Result<Vec<String>> {
    let (mut last_code, mut last_stderr) = (None, String::new());
    for format_args in [
        vec!["-f", AUDIO_FORMAT, "-x", "--audio-quality", "0"],
        vec!["-f", AUDIO_FORMAT],
        vec!["-f", "b/best"],
    ] {
        // A failed attempt may leave partial files; each attempt starts clean.
        for entry in std::fs::read_dir(staging).into_iter().flatten().flatten() {
            if entry.file_name() != "info.json" {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        let mut args = base_args(PLAYER_CLIENTS[0], options);
        args.extend(["--no-warnings", "--no-progress", "--no-playlist"].map(str::to_owned));
        args.extend(format_args.into_iter().map(str::to_owned));
        args.push("--max-filesize".to_owned());
        args.push(MAX_CACHE_FILE_BYTES.to_string());
        args.push("--load-info-json".to_owned());
        args.push(info_file.display().to_string());
        args.push("-o".to_owned());
        args.push(staging.join("media.%(ext)s").display().to_string());
        args.extend(["--print", "after_move:filepath"].map(str::to_owned));

        let output = run(args).await?;
        last_code = output.status.code();
        last_stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('['))
                .map(str::to_owned)
                .collect());
        }
    }
    err(format!("yt-dlp download failed (code {last_code:?}): {}", summarize_stderr(&last_stderr)))
}

/// Saved privately: it can hold signed media links and, with cookies set up, cookie values.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(text.as_bytes())
}

/// A track on disk.
pub struct DownloadedTrack {
    pub file_path: PathBuf,
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration: f64,
}

/// The cached file for a single YouTube video, without asking yt-dlp anything.
pub async fn cached_video(url: &str, cache: &Arc<MediaCache>) -> Option<PathBuf> {
    let video_id = parse_youtube_url(url).video_id?;
    cache.lookup(&cache_key(&SourceIdentity::Youtube { video_id })).await.ok().flatten()
}

/// Download a track into the cache, or reuse the cached file. Dropping the returned future
/// stops yt-dlp once nobody else waits for the same track.
pub async fn download(url: &str, cache: &Arc<MediaCache>, options: &YtDlpOptions) -> Result<DownloadedTrack> {
    let parsed = parse_youtube_url(url);
    if parsed.list_id.is_some() && parsed.video_id.is_none() {
        return err("Refusing to download a playlist URL as a single track");
    }
    let media_url = parsed.watch_url.clone().unwrap_or_else(|| parsed.canonical_url.clone());
    let info = dump_info(&media_url, options).await?;
    let video_id = parsed.video_id.as_deref().filter(|_| is_youtube_url(&media_url));
    let identity = identity_from_info(video_id, &media_url, &info)?;
    let kind = match identity {
        SourceIdentity::Youtube { .. } => "youtube",
        SourceIdentity::Url { .. } => "url",
    };
    let duration = info["duration"].as_f64().filter(|duration| *duration > 0.0);
    let extractor = info["extractor_key"].as_str().unwrap_or_default().to_owned();
    let document = info.to_string();
    let job_options = options.clone();
    let file_path = cache
        .fetch(&cache_key(&identity), kind, move |staging| async move {
            let info_file = staging.join("info.json");
            write_private(&info_file, &document).or_else(|error| err(format!("Could not save the metadata: {error}")))?;
            let reported_paths = download_from_info(&info_file, &staging, &job_options).await?;
            Ok(StagedDownload { reported_paths, expected_duration: duration, extractor })
        })
        .await?;
    let text = |key: &str| info[key].as_str().filter(|value| !value.is_empty()).map(str::to_owned);
    Ok(DownloadedTrack {
        file_path,
        id: text("id").unwrap_or_default(),
        title: text("title").unwrap_or_else(|| "Unknown".to_owned()),
        artist: text("uploader").or_else(|| text("channel")).unwrap_or_else(|| "Unknown".to_owned()),
        duration: duration.unwrap_or(0.0),
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub artist: String,
    /// Seconds; 0 when unknown.
    pub duration: f64,
    pub thumbnail: String,
}

fn entry_text(entry: &serde_json::Value, key: &str) -> Option<String> {
    entry[key].as_str().filter(|value| !value.is_empty()).map(str::to_owned)
}

fn map_entry(entry: &serde_json::Value) -> Option<SearchResult> {
    Some(SearchResult {
        id: entry_text(entry, "id")?,
        title: entry_text(entry, "title").unwrap_or_else(|| "Unknown".to_owned()),
        artist: entry_text(entry, "uploader")
            .or_else(|| entry_text(entry, "channel"))
            .or_else(|| entry["artists"][0].as_str().map(str::to_owned))
            .unwrap_or_else(|| "Unknown".to_owned()),
        duration: entry["duration"].as_f64().unwrap_or(0.0),
        thumbnail: entry["thumbnails"][0]["url"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| entry_text(entry, "thumbnail"))
            .unwrap_or_default(),
    })
}

/// One JSON object per line, as `--flat-playlist --dump-json` prints them.
pub fn parse_search_output(stdout: &str) -> Vec<SearchResult> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('{'))
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|entry| map_entry(&entry))
        .collect()
}

/// Search YouTube. Dropping the future stops yt-dlp.
pub async fn search_youtube(query: &str, max_results: u32, options: &YtDlpOptions) -> Result<Vec<SearchResult>> {
    if query.trim().starts_with('-') {
        return err("Invalid search query");
    }
    let mut args = base_args(PLAYER_CLIENTS[0], options);
    args.push("--no-warnings".to_owned());
    args.push(format!("ytsearch{max_results}:{query}"));
    args.extend(["--dump-json", "--flat-playlist", "--no-download"].map(str::to_owned));
    let output = run(args).await?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() && stdout.trim().is_empty() {
        return err(format!(
            "yt-dlp search failed (code {:?}): {}",
            output.status.code(),
            summarize_stderr(&String::from_utf8_lossy(&output.stderr))
        ));
    }
    Ok(parse_search_output(&stdout))
}

pub fn is_video_id(id: &str) -> bool {
    id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn watch_url(id: &str) -> String {
    format!("https://www.youtube.com/watch?v={id}")
}

/// The most relevant song for free text: the top official-audio result, else the top result.
pub async fn find_song(query: &str, options: &YtDlpOptions) -> Result<Option<SearchResult>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(None);
    }
    match search_youtube(&format!("{query} official audio"), 1, options).await {
        Ok(results) => {
            if let Some(top) = results.into_iter().find(|item| is_video_id(&item.id)) {
                return Ok(Some(top));
            }
        }
        Err(error) => tracing::warn!(%error, "Audio search failed, trying a plain search"),
    }
    Ok(search_youtube(query, 1, options).await?.into_iter().find(|item| is_video_id(&item.id)))
}

/// A link's tracks as watch addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expanded {
    pub title: Option<String>,
    pub urls: Vec<String>,
}

/// Entries of a `--flat-playlist --dump-single-json` document. Literal `null` is what yt-dlp
/// prints when extraction fails with `--ignore-errors`.
fn parse_playlist_info(stdout: &str) -> Option<(Option<String>, Vec<SearchResult>)> {
    let line = stdout.lines().map(str::trim).find(|line| line.starts_with('{') || *line == "null").unwrap_or(stdout.trim());
    let data: serde_json::Value = serde_json::from_str(line).ok()?;
    let title = entry_text(&data, "title");
    let items: Vec<SearchResult> = data["entries"].as_array().map(Vec::as_slice).unwrap_or_default().iter().filter_map(map_entry).collect();
    if !items.is_empty() {
        return Some((title, items));
    }
    map_entry(&data).map(|item| (title, vec![item]))
}

/// Turn a YouTube link into watch addresses. Only bare playlist links expand: a video opened
/// from a playlist or Mix (`v=...&list=...`) plays that video only.
pub async fn expand_to_watch_urls(url: &str, cap: usize, options: &YtDlpOptions) -> Result<Expanded> {
    let parsed = parse_youtube_url(url);
    if parsed.list_id.is_none() || parsed.video_id.is_some() {
        if let Some(watch) = parsed.watch_url {
            return Ok(Expanded { title: None, urls: vec![watch] });
        }
    }
    let probe = parsed.playlist_url.clone().unwrap_or_else(|| parsed.canonical_url.clone());
    let mut args = base_args(PLAYER_CLIENTS[0], options);
    args.extend(["--no-warnings", "--yes-playlist", "--flat-playlist", "--dump-single-json", "--ignore-errors", "--no-download"].map(str::to_owned));
    let output = run(with_media_url(args, &probe)?).await?;
    match parse_playlist_info(&String::from_utf8_lossy(&output.stdout)) {
        Some((title, items)) => Ok(Expanded { title, urls: items.iter().take(cap).map(|item| watch_url(&item.id)).collect() }),
        None => err(format!("Could not resolve YouTube URL: {}", summarize_stderr(&String::from_utf8_lossy(&output.stderr)))),
    }
}

/// Resolve a direct audio URL for a track without downloading it.
pub async fn resolve_audio_stream(url: &str, options: &YtDlpOptions) -> Result<ResolvedStream> {
    let parsed = parse_youtube_url(url);
    let media_url = parsed.watch_url.as_deref().unwrap_or(&parsed.canonical_url);
    let output = run(stream_args(media_url, options)?).await?;
    if !output.status.success() {
        return err(format!(
            "yt-dlp stream resolve failed: {}",
            summarize_stderr(&String::from_utf8_lossy(&output.stderr))
        ));
    }
    parse_resolved(&String::from_utf8_lossy(&output.stdout), media_url, parsed.video_id.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "dQw4w9WgXcQ";

    #[test]
    fn all_the_video_forms_give_one_watch_address() {
        let watch = format!("https://www.youtube.com/watch?v={ID}");
        for url in [
            format!("https://www.youtube.com/watch?v={ID}"),
            format!("https://music.youtube.com/watch?v={ID}"),
            format!("https://m.youtube.com/watch?feature=x&v={ID}"),
            format!("https://youtube.com/watch?v={ID}&t=10"),
            format!("https://youtu.be/{ID}?si=abc"),
            format!("https://www.youtube.com/shorts/{ID}"),
            format!("https://www.youtube.com/embed/{ID}"),
            format!("https://www.youtube.com/live/{ID}"),
            format!("  https://www.youtube.com/v/{ID}  "),
        ] {
            let parsed = parse_youtube_url(&url);
            assert_eq!(parsed.video_id.as_deref(), Some(ID), "{url}");
            assert_eq!(parsed.watch_url.as_deref(), Some(watch.as_str()), "{url}");
            assert_eq!(parsed.canonical_url, watch, "{url}");
        }
    }

    #[test]
    fn playlists_keep_their_list() {
        let parsed = parse_youtube_url("https://www.youtube.com/playlist?list=PLabc-1_x");
        assert_eq!(parsed.video_id, None);
        assert_eq!(parsed.canonical_url, "https://www.youtube.com/playlist?list=PLabc-1_x");

        let both = parse_youtube_url(&format!("https://www.youtube.com/watch?v={ID}&list=PLabc"));
        assert_eq!(both.canonical_url, format!("https://www.youtube.com/watch?v={ID}&list=PLabc"));
        assert_eq!(both.playlist_url.as_deref(), Some("https://www.youtube.com/playlist?list=PLabc"));
    }

    #[test]
    fn other_hosts_are_left_alone() {
        for url in [format!("https://evil-youtube.com/watch?v={ID}"), "not a url".to_owned(), "https://a.example/x".to_owned()] {
            let parsed = parse_youtube_url(&url);
            assert_eq!(parsed.video_id, None);
            assert_eq!(parsed.canonical_url, url);
        }
        assert!(!is_youtube_url("https://youtube.com.evil.example/"));
        assert!(is_youtube_url("https://www.youtube.com./x"));
    }

    #[test]
    fn a_short_id_is_not_an_id() {
        assert_eq!(parse_youtube_url("https://www.youtube.com/watch?v=short").video_id, None);
    }

    #[test]
    fn a_url_cannot_become_an_option() {
        assert!(stream_args("--exec=evil", &YtDlpOptions::default()).is_err());
        let args = stream_args("https://youtu.be/x", &YtDlpOptions::default()).unwrap();
        assert_eq!(args[args.len() - 2..], ["--", "https://youtu.be/x"]);
    }

    #[test]
    fn cookies_are_used_only_when_configured() {
        let none = stream_args("https://a", &YtDlpOptions::default()).unwrap();
        assert!(none.contains(&"--no-cookies".to_owned()));
        let options = YtDlpOptions { cookie_file: Some(PathBuf::from("/secret/c.txt")) };
        let some = stream_args("https://a", &options).unwrap();
        assert!(some.contains(&"--cookies".to_owned()) && !some.contains(&"--no-cookies".to_owned()));
    }

    #[test]
    fn the_real_cause_is_chosen_from_stderr() {
        assert_eq!(summarize_stderr("WARNING: x\nERROR: Video unavailable\n"), "ERROR: Video unavailable");
        assert_eq!(
            summarize_stderr("ERROR: other\nERROR: Sign in to confirm you're not a bot"),
            "ERROR: Sign in to confirm you're not a bot"
        );
        assert_eq!(summarize_stderr(""), "unknown yt-dlp error");
        assert_eq!(summarize_stderr(&"x".repeat(1000)).len(), 280);
    }

    #[test]
    fn search_output_is_read_line_by_line() {
        let results = parse_search_output(
            "[x]\n{\"id\":\"aaaaaaaaaaa\",\"title\":\"T\",\"uploader\":\"U\",\"duration\":12.0,\"thumbnails\":[{\"url\":\"th\"}]}\n{broken\n{\"title\":\"no id\"}\n{\"id\":\"b\"}\n",
        );
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], SearchResult { id: "aaaaaaaaaaa".to_owned(), title: "T".to_owned(), artist: "U".to_owned(), duration: 12.0, thumbnail: "th".to_owned() });
        assert_eq!((results[1].title.as_str(), results[1].artist.as_str()), ("Unknown", "Unknown"));
    }

    #[test]
    fn playlist_documents_give_their_entries() {
        let (title, items) = parse_playlist_info(r#"{"title":"L","entries":[{"id":"a"},null,{"id":"b"}]}"#).unwrap();
        assert_eq!(title.as_deref(), Some("L"));
        assert_eq!(items.len(), 2);
        assert_eq!(parse_playlist_info(r#"{"id":"solo","title":"S"}"#).unwrap().1[0].id, "solo");
        assert!(parse_playlist_info("null").is_none());
        assert!(parse_playlist_info("nonsense").is_none());
    }

    #[tokio::test]
    async fn a_video_in_a_playlist_stays_one_track() {
        let expanded = expand_to_watch_urls(&format!("https://www.youtube.com/watch?v={ID}&list=PLabc"), 25, &YtDlpOptions::default()).await.unwrap();
        assert_eq!(expanded, Expanded { title: None, urls: vec![format!("https://www.youtube.com/watch?v={ID}")] });
    }

    #[test]
    fn metadata_must_be_one_record() {
        assert_eq!(single_metadata_record("[x] y\n").unwrap(), None);
        assert_eq!(single_metadata_record("{\"id\":\"a\"}\n").unwrap().unwrap()["id"], "a");
        assert!(single_metadata_record("{\"id\":\"a\"}\n{\"id\":\"b\"}\n").is_err());
        assert!(single_metadata_record("{broken\n").is_err());
    }

    #[test]
    fn storyboards_are_not_playable() {
        assert!(has_playable_format(&serde_json::json!({"url": "u", "ext": "webm"})));
        assert!(!has_playable_format(&serde_json::json!({"url": "u", "ext": "mhtml"})));
        assert!(has_playable_format(&serde_json::json!({"formats": [{"ext": "webm"}]})));
        assert!(!has_playable_format(&serde_json::json!({"formats": [{"ext": "mhtml"}, {"format_note": "storyboard"}]})));
        assert!(!has_playable_format(&serde_json::json!({})));
    }

    #[tokio::test]
    async fn a_playlist_is_not_downloaded_as_one_track() {
        let dir = std::env::temp_dir().join(format!("teamhub-yt-test-{}", std::process::id()));
        let cache = MediaCache::new(&dir);
        let result = download("https://www.youtube.com/playlist?list=PLabc", &cache, &YtDlpOptions::default()).await;
        assert!(result.is_err_and(|error| error.0.contains("playlist")));
    }

    #[test]
    fn resolver_output_is_read() {
        let line = r#"{"url":"https://rr.example/a","id":"abc","title":"T","channel":"C","duration":12.5}"#;
        let resolved = parse_resolved(&format!("[debug] x\n{line}\n"), "https://youtu.be/abc", None).unwrap();
        assert_eq!(resolved.stream_url, "https://rr.example/a");
        assert_eq!((resolved.title.as_str(), resolved.artist.as_str(), resolved.duration), ("T", "C", 12.5));
        assert!(parse_resolved(r#"{"id":"abc"}"#, "u", None).is_err());
        assert!(parse_resolved("nonsense", "u", None).is_err());
    }
}
