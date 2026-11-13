//! The download cache in the music folder. Same files as the old program, so tracks it
//! downloaded stay cached and rolling back keeps what was downloaded here:
//!
//! ```text
//! .ts6-media-cache/
//!   OWNER                    marker: this folder belongs to the program
//!   v2/<key>.<rand>.<ext>    media, published once and never overwritten
//!   v2/<key>.json            manifest, the commit marker
//!   v2/staging/<random>/     one folder per download
//! ```
//!
//! Every published name is built here from hex and an allowed extension: nothing the
//! downloader reports becomes part of a path. Manifests never hold a source URL.
//!
//! Only one program uses the folder at a time, so the old program's lock files are not made;
//! the ones it left behind are removed by [`MediaCache::sweep`].

use std::collections::HashMap;
use std::future::Future;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use chrono::{SecondsFormat, Utc};
use sha2::{Digest, Sha256};
use tokio::process::Command;
use tokio::sync::watch;
use tokio::task::AbortHandle;

use crate::{err, Error, Result};

pub const CACHE_DIR_NAME: &str = ".ts6-media-cache";
const OWNER_MARKER: &str = "ts6-manager media cache\nformat=2\n";
const FORMAT: u64 = 2;
const CACHE_EXTENSIONS: [&str; 9] = ["opus", "ogg", "webm", "m4a", "mp3", "aac", "flac", "wav", "mp4"];
/// Largest download accepted.
pub const MAX_CACHE_FILE_BYTES: u64 = 1024 * 1024 * 1024;
/// Staging folders and side files older than this are left over from a crash.
const STAGING_STALE: Duration = Duration::from_secs(60 * 60);
const LOCK_STALE: Duration = Duration::from_secs(3 * 60);
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

/// What a download is, as this program determined it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceIdentity {
    Youtube { video_id: String },
    Url { extractor: String, media_id: String, canonical_url: String },
}

impl SourceIdentity {
    fn kind(&self) -> &'static str {
        match self {
            Self::Youtube { .. } => "youtube",
            Self::Url { .. } => "url",
        }
    }
}

/// sha256 over a versioned, length-prefixed encoding of the identity, as hex.
pub fn cache_key(identity: &SourceIdentity) -> String {
    let format = FORMAT.to_string();
    let mut fields = vec!["ts6-media-cache", format.as_str(), identity.kind()];
    match identity {
        SourceIdentity::Youtube { video_id } => fields.push(video_id),
        SourceIdentity::Url { extractor, media_id, canonical_url } => fields.extend([extractor.as_str(), media_id, canonical_url]),
    }
    let mut hash = Sha256::new();
    for field in fields {
        hash.update((field.len() as u32).to_be_bytes());
        hash.update(field.as_bytes());
    }
    hex(&hash.finalize())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn is_hex(text: &str, length: usize) -> bool {
    text.len() == length && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn random_hex(bytes: usize) -> Result<String> {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).or_else(|error| err(format!("No random numbers: {error}")))?;
    Ok(hex(&buffer))
}

/// Check yt-dlp's metadata against what was asked for and return the identity to cache under.
pub fn identity_from_info(youtube_video_id: Option<&str>, url: &str, info: &serde_json::Value) -> Result<SourceIdentity> {
    let kind = &info["_type"];
    if (!kind.is_null() && kind != "video") || info.get("entries").is_some() {
        return err("The link is not a single track.");
    }
    if info["requested_downloads"].as_array().is_some_and(|downloads| downloads.len() > 1) {
        return err("The link is not a single track.");
    }
    let extractor = info["extractor_key"].as_str().unwrap_or_default();
    let media_id = info["id"].as_str().unwrap_or_default();
    if extractor.is_empty() || media_id.is_empty() {
        return err("The downloader did not identify the track.");
    }
    if let Some(video_id) = youtube_video_id {
        if extractor != "Youtube" || media_id != video_id {
            return err("The downloader returned a different video than the one requested.");
        }
        return Ok(SourceIdentity::Youtube { video_id: video_id.to_owned() });
    }
    Ok(SourceIdentity::Url {
        extractor: extractor.to_owned(),
        media_id: media_id.to_owned(),
        canonical_url: canonical_url(url)?,
    })
}

/// Lower-case scheme and host, no default port, credentials or fragment; path and query as they are.
pub fn canonical_url(raw: &str) -> Result<String> {
    let Ok(url) = url::Url::parse(raw) else {
        return err("Invalid URL format");
    };
    let host = url.host_str().unwrap_or_default();
    let port = url.port().map(|port| format!(":{port}")).unwrap_or_default();
    let query = url.query().map(|query| format!("?{query}")).unwrap_or_default();
    Ok(format!("{}://{host}{port}{}{query}", url.scheme(), url.path()))
}

#[derive(Debug, PartialEq)]
struct Probe {
    has_audio: bool,
    duration: Option<f64>,
}

fn parse_probe(json: &str) -> Result<Probe> {
    let Ok(data) = serde_json::from_str::<serde_json::Value>(json) else {
        return err("The downloaded file could not be read as audio.");
    };
    let positive = |value: &serde_json::Value| value.as_str()?.parse::<f64>().ok().filter(|n| n.is_finite() && *n > 0.0);
    let streams = data["streams"].as_array().map(Vec::as_slice).unwrap_or_default();
    let audio = streams.iter().find(|stream| stream["codec_type"] == "audio");
    Ok(Probe {
        has_audio: audio.is_some(),
        duration: positive(&data["format"]["duration"]).or_else(|| audio.and_then(|audio| positive(&audio["duration"]))),
    })
}

async fn probe_audio(file: &Path) -> Result<Probe> {
    let child = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "stream=codec_type,duration:format=duration", "-of", "json"])
        .arg(file)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .or_else(|error| err(format!("ffprobe could not run: {error}")))?;
    let output = match tokio::time::timeout(PROBE_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(output)) if output.status.success() => output,
        _ => return err("The downloaded file could not be read as audio."),
    };
    parse_probe(&String::from_utf8_lossy(&output.stdout))
}

/// A cached track must be audio with a positive length, and agree with the announced length
/// within 10 seconds or 10%, whichever is more.
fn check_probe(probe: &Probe, expected: Option<f64>) -> Result<f64> {
    if !probe.has_audio {
        return err("The downloaded file has no audio.");
    }
    let Some(duration) = probe.duration else {
        return err("The length of the downloaded file could not be read.");
    };
    if let Some(expected) = expected.filter(|expected| expected.is_finite() && *expected > 0.0) {
        if (duration - expected).abs() > (expected * 0.1).max(10.0) {
            return err("The downloaded file does not match the track length.");
        }
    }
    Ok(duration)
}

struct Staged {
    file: PathBuf,
    ext: String,
    size: u64,
}

/// The file the downloader reported must be exactly one regular file directly in the staging
/// folder, with an allowed extension and size.
fn check_staged_file(staging: &Path, reported: &[String]) -> Result<Staged> {
    let paths: Vec<&str> = reported.iter().map(|line| line.trim()).filter(|line| !line.is_empty()).collect();
    let [path] = paths.as_slice() else {
        return err("The downloader did not produce exactly one file.");
    };
    let root = std::fs::canonicalize(staging).or_else(|_| err("The staging folder is missing."))?;
    let Ok(file) = std::fs::canonicalize(path) else {
        return err("The downloaded file is missing.");
    };
    if file.parent() != Some(root.as_path()) {
        return err("The downloaded file is not where it was expected.");
    }
    let info = std::fs::symlink_metadata(&file).or_else(|_| err("The downloaded file is missing."))?;
    if !info.is_file() {
        return err("The downloaded file is not a regular file.");
    }
    let ext = file.extension().and_then(|ext| ext.to_str()).unwrap_or_default().to_ascii_lowercase();
    if !CACHE_EXTENSIONS.contains(&ext.as_str()) {
        return err("The downloaded file type is not allowed.");
    }
    if info.len() == 0 || info.len() > MAX_CACHE_FILE_BYTES {
        return err("The downloaded file size is not allowed.");
    }
    Ok(Staged { file, ext, size: info.len() })
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).or_else(|_| err("A cached file could not be read."))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        let read = file.read(&mut buffer).or_else(|_| err("A cached file could not be read."))?;
        if read == 0 {
            return Ok(hex(&hash.finalize()));
        }
        hash.update(&buffer[..read]);
    }
}

#[derive(Debug, PartialEq)]
struct Manifest {
    file: String,
    size: u64,
    sha256: String,
}

/// A well-formed manifest naming a file this cache could have published for `key`.
fn parse_manifest(text: &str, key: &str) -> Option<Manifest> {
    let data: serde_json::Value = serde_json::from_str(text).ok()?;
    let file = data["file"].as_str()?;
    let mut parts = file.split('.');
    let (file_key, random, ext) = (parts.next()?, parts.next()?, parts.next()?);
    let shape = parts.next().is_none() && is_hex(random, 16) && file_key == key && CACHE_EXTENSIONS.contains(&ext);
    if !shape || data["format"] != FORMAT || data["key"] != key {
        return None;
    }
    let size = data["size"].as_u64().filter(|size| *size > 0)?;
    let sha256 = data["sha256"].as_str().filter(|sum| is_hex(sum, 64))?;
    if !matches!(data["kind"].as_str(), Some("youtube" | "url")) {
        return None;
    }
    Some(Manifest { file: file.to_owned(), size, sha256: sha256.to_owned() })
}

/// What a download job reports about the file it wrote into the staging folder.
pub struct StagedDownload {
    /// Lines the downloader printed for its final file.
    pub reported_paths: Vec<String>,
    pub expected_duration: Option<f64>,
    pub extractor: String,
}

/// Removes a staging folder when the job ends, however it ends, including cancellation.
struct StagingGuard(PathBuf);

impl Drop for StagingGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Flight {
    result: watch::Receiver<Option<std::result::Result<PathBuf, String>>>,
    waiters: AtomicUsize,
    abort: AbortHandle,
}

/// A request that stops waiting when dropped; the download stops when nobody waits any more.
struct Waiter<'a> {
    cache: &'a MediaCache,
    key: String,
    flight: Arc<Flight>,
}

impl Drop for Waiter<'_> {
    fn drop(&mut self) {
        if self.flight.waiters.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.flight.abort.abort();
            let mut flights = self.cache.flights.lock().expect("flights lock");
            if flights.get(&self.key).is_some_and(|flight| Arc::ptr_eq(flight, &self.flight)) {
                flights.remove(&self.key);
            }
        }
    }
}

pub struct MediaCache {
    root: PathBuf,
    v2: PathBuf,
    flights: Mutex<HashMap<String, Arc<Flight>>>,
    /// Entries whose checksum was confirmed by this process: key to checksum, length and time.
    verified: Mutex<HashMap<String, (String, u64, Option<SystemTime>)>>,
}

impl MediaCache {
    pub fn new(music_dir: &Path) -> Arc<Self> {
        let root = music_dir.join(CACHE_DIR_NAME);
        let v2 = root.join("v2");
        Arc::new(Self { root, v2, flights: Mutex::default(), verified: Mutex::default() })
    }

    /// Create the folder with its marker, or check an existing one. A folder of that name
    /// without the marker may be the user's: it is left alone and downloads fail.
    fn init(&self) -> Result<()> {
        if std::fs::symlink_metadata(&self.root).is_err() {
            let building = self.root.with_extension(format!("{}.tmp", random_hex(8)?));
            std::fs::create_dir_all(&building).or_else(|error| err(format!("Could not create the cache: {error}")))?;
            let created = std::fs::write(building.join("OWNER"), OWNER_MARKER)
                .and_then(|()| std::fs::rename(&building, &self.root));
            if created.is_err() {
                // Only the folder this call built. Another process creating it first is fine.
                let _ = std::fs::remove_dir_all(&building);
            }
        }
        if std::fs::read_to_string(self.root.join("OWNER")).ok().as_deref() != Some(OWNER_MARKER) {
            return err(format!(
                "The folder {CACHE_DIR_NAME} in the music folder was not created by this program; rename or move it so downloads can use the cache."
            ));
        }
        self.assert_folder(&self.root)?;
        for dir in [self.v2.clone(), self.v2.join("staging")] {
            let _ = std::fs::create_dir(&dir);
            self.assert_folder(&dir)?;
        }
        Ok(())
    }

    /// Real folders only: nothing written or cleaned up may land outside through a link.
    fn assert_folder(&self, dir: &Path) -> Result<()> {
        match std::fs::symlink_metadata(dir) {
            Ok(info) if info.is_dir() && !info.file_type().is_symlink() => Ok(()),
            _ => err("The download cache contains a link where a folder belongs; downloads are paused until it is removed."),
        }
    }

    fn lookup_blocking(&self, key: &str) -> Result<Option<PathBuf>> {
        if !is_hex(key, 64) {
            return Ok(None);
        }
        self.init()?;
        let Ok(text) = std::fs::read_to_string(self.v2.join(format!("{key}.json"))) else {
            return Ok(None);
        };
        let Some(manifest) = parse_manifest(&text, key) else {
            return Ok(None);
        };
        let file = self.v2.join(&manifest.file);
        let Ok(info) = std::fs::symlink_metadata(&file) else {
            return Ok(None);
        };
        if !info.is_file() || info.len() != manifest.size {
            return Ok(None);
        }
        // The content is checked once per process, and again when the entry changes.
        let fingerprint = (manifest.sha256.clone(), info.len(), info.modified().ok());
        if self.verified.lock().expect("verified lock").get(key) != Some(&fingerprint) {
            if sha256_file(&file)? != manifest.sha256 {
                return Ok(None);
            }
            self.verified.lock().expect("verified lock").insert(key.to_owned(), fingerprint);
        }
        Ok(Some(file))
    }

    /// The cached file for `key`, if a complete and intact entry exists.
    pub async fn lookup(self: &Arc<Self>, key: &str) -> Result<Option<PathBuf>> {
        let cache = self.clone();
        let key = key.to_owned();
        tokio::task::spawn_blocking(move || cache.lookup_blocking(&key))
            .await
            .unwrap_or_else(|_| err("The cache check stopped."))
    }

    /// The cached file for `key`, downloading it once if needed. Requests for one key share a
    /// download; a request that is dropped stops waiting, and the download stops with the last.
    pub async fn fetch<F, Fut>(self: &Arc<Self>, key: &str, kind: &'static str, download: F) -> Result<PathBuf>
    where
        F: FnOnce(PathBuf) -> Fut + Send + 'static,
        Fut: Future<Output = Result<StagedDownload>> + Send + 'static,
    {
        if !is_hex(key, 64) {
            return err("Invalid cache key.");
        }
        if let Some(hit) = self.lookup(key).await? {
            return Ok(hit);
        }
        let flight = {
            let mut flights = self.flights.lock().expect("flights lock");
            let flight = match flights.get(key) {
                Some(flight) => flight.clone(),
                None => {
                    let (sender, result) = watch::channel(None);
                    let cache = self.clone();
                    let job_key = key.to_owned();
                    let task = tokio::spawn(async move {
                        let outcome = cache.produce(&job_key, kind, download).await.map_err(|error| error.0);
                        cache.flights.lock().expect("flights lock").remove(&job_key);
                        let _ = sender.send(Some(outcome));
                    });
                    let flight = Arc::new(Flight { result, waiters: AtomicUsize::new(0), abort: task.abort_handle() });
                    flights.insert(key.to_owned(), flight.clone());
                    flight
                }
            };
            flight.waiters.fetch_add(1, Ordering::SeqCst);
            flight
        };
        let _waiter = Waiter { cache: self, key: key.to_owned(), flight: flight.clone() };
        let mut result = flight.result.clone();
        let Ok(done) = result.wait_for(Option::is_some).await else {
            return err("The download stopped.");
        };
        match done.as_ref() {
            Some(Ok(path)) => Ok(path.clone()),
            Some(Err(message)) => Err(Error(message.clone())),
            None => err("The download stopped."),
        }
    }

    /// Download, verify, then publish media and manifest.
    async fn produce<F, Fut>(self: &Arc<Self>, key: &str, kind: &'static str, download: F) -> Result<PathBuf>
    where
        F: FnOnce(PathBuf) -> Fut,
        Fut: Future<Output = Result<StagedDownload>>,
    {
        let staging = self.v2.join("staging").join(random_hex(16)?);
        create_private_dir(&staging).or_else(|error| err(format!("Could not create a download folder: {error}")))?;
        let _guard = StagingGuard(staging.clone());
        let result = download(staging.clone()).await?;
        let staged = check_staged_file(&staging, &result.reported_paths)?;
        let duration = check_probe(&probe_audio(&staged.file).await?, result.expected_duration)?;

        let cache = self.clone();
        let key = key.to_owned();
        tokio::task::spawn_blocking(move || cache.publish(&key, kind, &staged, &result.extractor, duration))
            .await
            .unwrap_or_else(|_| err("Saving the download stopped."))
    }

    fn publish(&self, key: &str, kind: &str, staged: &Staged, extractor: &str, duration: f64) -> Result<PathBuf> {
        let failed = |error: std::io::Error| Error(format!("Could not save the download: {error}"));
        let name = format!("{key}.{}.{}", random_hex(8)?, staged.ext);
        let target = self.v2.join(&name);
        let temporary = self.v2.join(format!("{key}.json.{}.tmp", random_hex(8)?));
        let sha256 = sha256_file(&staged.file)?;
        std::fs::File::open(&staged.file).and_then(|file| file.sync_all()).ok();
        let extractor: String =
            extractor.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(64).collect();
        let manifest = serde_json::json!({
            "format": FORMAT,
            "key": key,
            "kind": kind,
            "extractor": extractor,
            "file": name,
            "size": staged.size,
            "sha256": sha256,
            "durationSec": duration,
            "createdAt": Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        });
        // A fresh name each time: nothing is renamed over a file a player may have open.
        let commit = || -> std::io::Result<()> {
            rename_with_retry(&staged.file, &target)?;
            std::fs::write(&temporary, manifest.to_string())?;
            rename_with_retry(&temporary, &self.v2.join(format!("{key}.json")))
        };
        if let Err(error) = commit() {
            let _ = std::fs::remove_file(&target);
            let _ = std::fs::remove_file(&temporary);
            return Err(failed(error));
        }
        Ok(target)
    }

    /// Remove what crashed jobs left behind. Published media and manifests are never touched.
    pub fn sweep(&self) -> Result<()> {
        self.init()?;
        let now = SystemTime::now();
        let is_old = |info: &std::fs::Metadata, limit: Duration| {
            info.modified().ok().and_then(|at| now.duration_since(at).ok()).is_some_and(|age| age > limit)
        };
        let entries = |dir: &Path| -> Vec<(PathBuf, String, std::fs::Metadata)> {
            std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|entry| {
                    let info = entry.path().symlink_metadata().ok()?;
                    Some((entry.path(), entry.file_name().to_str()?.to_owned(), info))
                })
                .filter(|(_, _, info)| !info.file_type().is_symlink())
                .collect()
        };
        for (path, name, info) in entries(&self.v2.join("staging")) {
            if is_hex(&name, 32) && info.is_dir() && is_old(&info, STAGING_STALE) {
                let _ = std::fs::remove_dir_all(path);
            }
        }
        for (path, name, info) in entries(&self.v2.join("locks")) {
            let is_lock = name.strip_suffix(".lock").is_some_and(|key| is_hex(key, 64));
            if is_lock && info.is_file() && is_old(&info, LOCK_STALE) {
                let _ = std::fs::remove_file(path);
            }
        }
        for (path, name, info) in entries(&self.v2) {
            let parts: Vec<&str> = name.split('.').collect();
            let is_side_file = matches!(parts.as_slice(), [key, "json", random, "tmp"] if is_hex(key, 64) && is_hex(random, 16));
            if is_side_file && info.is_file() && is_old(&info, STAGING_STALE) {
                let _ = std::fs::remove_file(path);
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
fn create_private_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new().mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_private_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir(path)
}

/// Windows refuses to move a file another process has open for a moment, such as a virus scan.
fn rename_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut attempt = 1;
    loop {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error) if attempt >= 10 || error.kind() != std::io::ErrorKind::PermissionDenied => return Err(error),
            Err(_) => {
                std::thread::sleep(Duration::from_millis(20 * attempt));
                attempt += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    const KEY: &str = "09dc9a9497109bae83cfddb275bdaa778a86b865901ece3a200b66fb26660283";

    fn temp_music_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("teamhub-cache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Background work finishes when it finishes; a loaded machine must not fail a test.
    async fn wait_until(condition: impl Fn() -> bool) {
        for _ in 0..1000 {
            if condition() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the condition never became true");
    }

    /// A short real audio file, made once for the tests that need ffprobe to accept it.
    fn wav(seconds: u32) -> Vec<u8> {
        let bytes = seconds * 16_000;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + bytes).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&8_000u32.to_le_bytes());
        out.extend_from_slice(&16_000u32.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&bytes.to_le_bytes());
        out.resize(44 + bytes as usize, 0);
        out
    }

    async fn job(staging: PathBuf) -> Result<StagedDownload> {
        let file = staging.join("media.wav");
        std::fs::write(&file, wav(2)).unwrap();
        Ok(StagedDownload {
            reported_paths: vec![file.display().to_string()],
            expected_duration: Some(2.0),
            extractor: "Youtube".to_owned(),
        })
    }

    #[test]
    fn keys_match_the_old_program() {
        // Printed by the old algorithm in Node.
        assert_eq!(cache_key(&SourceIdentity::Youtube { video_id: "dQw4w9WgXcQ".to_owned() }), KEY);
        assert_eq!(
            cache_key(&SourceIdentity::Url {
                extractor: "Generic".to_owned(),
                media_id: "abc-ş".to_owned(),
                canonical_url: "https://a.example/x?y=1".to_owned(),
            }),
            "f092fc27abea9c6812850c769eaa4fe8a89bca6ed23e2cd9025dd510fe75ec8d"
        );
    }

    #[test]
    fn urls_are_canonical() {
        assert_eq!(canonical_url("HTTPS://User:Pw@A.Example:443/P?q=1#frag").unwrap(), "https://a.example/P?q=1");
        assert_eq!(canonical_url("http://a.example:8080/").unwrap(), "http://a.example:8080/");
    }

    #[test]
    fn identity_needs_the_video_that_was_asked_for() {
        let info = serde_json::json!({"id": "abc", "extractor_key": "Youtube"});
        assert_eq!(
            identity_from_info(Some("abc"), "u", &info).unwrap(),
            SourceIdentity::Youtube { video_id: "abc".to_owned() }
        );
        assert!(identity_from_info(Some("other"), "u", &info).is_err());
        assert!(identity_from_info(None, "u", &serde_json::json!({"id": "abc"})).is_err());
        assert!(identity_from_info(None, "https://a", &serde_json::json!({"_type": "playlist", "id": "a", "extractor_key": "X"})).is_err());
        assert!(identity_from_info(None, "https://a", &serde_json::json!({"entries": [], "id": "a", "extractor_key": "X"})).is_err());
    }

    #[test]
    fn probes_are_read_and_checked() {
        let probe = parse_probe(r#"{"streams":[{"codec_type":"audio","duration":"12.5"}],"format":{}}"#).unwrap();
        assert_eq!(probe, Probe { has_audio: true, duration: Some(12.5) });
        assert_eq!(check_probe(&probe, None).unwrap(), 12.5);
        assert_eq!(check_probe(&probe, Some(20.0)).unwrap(), 12.5);
        assert!(check_probe(&probe, Some(40.0)).is_err());
        assert!(check_probe(&Probe { has_audio: false, duration: Some(1.0) }, None).is_err());
        assert!(check_probe(&Probe { has_audio: true, duration: None }, None).is_err());
    }

    #[test]
    fn only_manifests_this_cache_could_write_are_accepted() {
        let good = serde_json::json!({
            "format": 2, "key": KEY, "kind": "youtube", "extractor": "Youtube",
            "file": format!("{KEY}.0123456789abcdef.webm"), "size": 5, "sha256": "a".repeat(64),
            "durationSec": 1.0, "createdAt": "2026-01-01T00:00:00.000Z",
        });
        assert!(parse_manifest(&good.to_string(), KEY).is_some());
        for (field, value) in [
            ("format", serde_json::json!(1)),
            ("key", serde_json::json!("b")),
            ("kind", serde_json::json!("other")),
            ("file", serde_json::json!("../../etc/passwd")),
            ("file", serde_json::json!(format!("{KEY}.0123456789abcdef.exe"))),
            ("size", serde_json::json!(0)),
            ("sha256", serde_json::json!("xyz")),
        ] {
            let mut bad = good.clone();
            bad[field] = value;
            assert!(parse_manifest(&bad.to_string(), KEY).is_none(), "{field}");
        }
        assert!(parse_manifest("nonsense", KEY).is_none());
    }

    #[tokio::test]
    async fn a_download_is_published_once_and_found_again() {
        let dir = temp_music_dir("publish");
        let cache = MediaCache::new(&dir);
        assert_eq!(cache.lookup(KEY).await.unwrap(), None);
        let file = cache.fetch(KEY, "youtube", job).await.unwrap();
        assert!(file.starts_with(dir.join(CACHE_DIR_NAME).join("v2")));
        assert_eq!(std::fs::read_to_string(dir.join(CACHE_DIR_NAME).join("OWNER")).unwrap(), OWNER_MARKER);
        assert!(std::fs::read_dir(dir.join(CACHE_DIR_NAME).join("v2").join("staging")).unwrap().next().is_none());

        let again = cache
            .fetch(KEY, "youtube", |_| async { panic!("a cached track must not download again") })
            .await
            .unwrap();
        assert_eq!(again, file);
        // A new process finds the same entry.
        assert_eq!(MediaCache::new(&dir).lookup(KEY).await.unwrap(), Some(file));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn a_changed_file_is_not_a_hit() {
        let dir = temp_music_dir("changed");
        let cache = MediaCache::new(&dir);
        let file = cache.fetch(KEY, "youtube", job).await.unwrap();
        let mut bytes = std::fs::read(&file).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        std::fs::write(&file, bytes).unwrap();
        assert_eq!(MediaCache::new(&dir).lookup(KEY).await.unwrap(), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn a_folder_that_is_not_ours_is_left_alone() {
        let dir = temp_music_dir("foreign");
        std::fs::create_dir_all(dir.join(CACHE_DIR_NAME)).unwrap();
        std::fs::write(dir.join(CACHE_DIR_NAME).join("mine.txt"), "x").unwrap();
        let cache = MediaCache::new(&dir);
        assert!(cache.fetch(KEY, "youtube", job).await.is_err());
        assert_eq!(std::fs::read_dir(dir.join(CACHE_DIR_NAME)).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn requests_share_one_download() {
        let dir = temp_music_dir("shared");
        let cache = MediaCache::new(&dir);
        let runs = Arc::new(AtomicU32::new(0));
        let start = |runs: Arc<AtomicU32>| {
            let cache = cache.clone();
            tokio::spawn(async move {
                cache
                    .fetch(KEY, "youtube", move |staging| async move {
                        runs.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        job(staging).await
                    })
                    .await
            })
        };
        let first = start(runs.clone());
        tokio::time::sleep(Duration::from_millis(30)).await;
        let second = start(runs.clone());
        assert_eq!(first.await.unwrap().unwrap(), second.await.unwrap().unwrap());
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn the_last_request_leaving_stops_the_download_and_cleans_up() {
        let dir = temp_music_dir("cancel");
        let cache = MediaCache::new(&dir);
        let waiting = {
            let cache = cache.clone();
            tokio::spawn(async move {
                cache
                    .fetch(KEY, "youtube", |_staging| async {
                        tokio::time::sleep(Duration::from_secs(600)).await;
                        err("not reached")
                    })
                    .await
            })
        };
        let staging = dir.join(CACHE_DIR_NAME).join("v2").join("staging");
        let entries = || std::fs::read_dir(&staging).map_or(0, Iterator::count);
        wait_until(|| entries() == 1).await;
        waiting.abort();
        let _ = waiting.await;
        wait_until(|| entries() == 0 && cache.flights.lock().unwrap().is_empty()).await;
        // The next request starts over.
        assert!(cache.fetch(KEY, "youtube", job).await.is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn a_failed_download_leaves_nothing() {
        let dir = temp_music_dir("failed");
        let cache = MediaCache::new(&dir);
        let failed = cache.fetch(KEY, "youtube", |_| async { err("nope") }).await;
        assert_eq!(failed, Err(Error("nope".to_owned())));
        // Two files reported: refused.
        let two = cache
            .fetch(KEY, "youtube", |staging: PathBuf| async move {
                Ok(StagedDownload {
                    reported_paths: vec![staging.join("a").display().to_string(), staging.join("b").display().to_string()],
                    expected_duration: None,
                    extractor: String::new(),
                })
            })
            .await;
        assert!(two.is_err());
        // A path outside the staging folder: refused.
        let outside = dir.join("outside.wav");
        std::fs::write(&outside, wav(1)).unwrap();
        let reported = outside.display().to_string();
        let escape = cache
            .fetch(KEY, "youtube", move |_| async move {
                Ok(StagedDownload { reported_paths: vec![reported], expected_duration: None, extractor: String::new() })
            })
            .await;
        assert_eq!(escape, Err(Error("The downloaded file is not where it was expected.".to_owned())));
        assert!(outside.exists());
        assert_eq!(cache.lookup(KEY).await.unwrap(), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sweep_removes_only_leftovers() {
        let dir = temp_music_dir("sweep");
        let cache = MediaCache::new(&dir);
        cache.init().unwrap();
        let v2 = dir.join(CACHE_DIR_NAME).join("v2");
        let fresh = v2.join("staging").join("a".repeat(32));
        let unknown = v2.join("staging").join("not-ours");
        std::fs::create_dir_all(&fresh).unwrap();
        std::fs::create_dir_all(&unknown).unwrap();
        let media = v2.join(format!("{KEY}.0123456789abcdef.mp3"));
        std::fs::write(&media, "x").unwrap();
        cache.sweep().unwrap();
        assert!(fresh.exists() && unknown.exists() && media.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
