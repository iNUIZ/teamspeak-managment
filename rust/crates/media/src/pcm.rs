//! ffmpeg decoding to raw PCM. A bounded channel carries the frames, so a paused or slow
//! reader stops ffmpeg through the pipe and memory stays flat on multi-hour tracks.

use std::process::Stdio;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::address::validate_url;
use crate::{err, Result};

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
pub const FRAME_SIZE: usize = 960;
pub const BYTES_PER_FRAME: usize = FRAME_SIZE * CHANNELS * 2;
pub const FRAME_MS: u64 = 20;

/// Protocols ffmpeg may open for a resolved track: nothing a playlist or redirect could abuse.
const REMOTE_PROTOCOLS: &str = "http,https,tls,tcp,httpproxy,crypto";
/// Single-file containers only: playlist formats would make ffmpeg open addresses found inside.
const REMOTE_FORMATS: &str = "matroska,webm,mov,mp4,m4a,mp3,ogg,aac,flac,wav";
/// One second of audio between ffmpeg and the voice clock.
const BUFFERED_FRAMES: usize = 50;
const STDERR_TAIL_BYTES: usize = 2048;

pub fn is_remote_input(input: &str) -> bool {
    let lower = input.get(..8).unwrap_or(input).to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Arguments for a finite track at media speed. `-re` keeps ffmpeg from racing the 20 ms clock.
pub fn file_args(input: &str, start_seconds: f64) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    if start_seconds > 0.0 {
        args.extend(["-ss".to_owned(), format!("{start_seconds:.3}")]);
    }
    if is_remote_input(input) {
        args.extend(
            [
                "-protocol_whitelist",
                REMOTE_PROTOCOLS,
                "-format_whitelist",
                REMOTE_FORMATS,
                "-reconnect",
                "1",
                "-reconnect_streamed",
                "1",
                "-reconnect_delay_max",
                "5",
            ]
            .map(str::to_owned),
        );
    }
    args.extend(["-re", "-i", input].map(str::to_owned));
    args.extend(output_args());
    args
}

/// Arguments for a live stream, which arrives at media speed by itself.
pub fn live_args(url: &str) -> Vec<String> {
    let mut args: Vec<String> =
        ["-reconnect", "1", "-reconnect_streamed", "1", "-reconnect_delay_max", "5", "-i", url].map(str::to_owned).to_vec();
    args.extend(output_args());
    args
}

fn output_args() -> Vec<String> {
    let rate = SAMPLE_RATE.to_string();
    let channels = CHANNELS.to_string();
    ["-f", "s16le", "-acodec", "pcm_s16le", "-ar", &rate, "-ac", &channels, "-loglevel", "error", "pipe:1"]
        .map(str::to_owned)
        .to_vec()
}

#[derive(Debug, PartialEq, Eq)]
pub enum PcmEvent {
    Frame(Vec<u8>),
    /// ffmpeg ended. `message` is its error output, with addresses removed.
    End { success: bool, message: String },
}

/// A running decoder. Dropping it kills ffmpeg.
pub struct PcmStream {
    events: mpsc::Receiver<PcmEvent>,
    task: JoinHandle<()>,
}

impl Drop for PcmStream {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl PcmStream {
    /// Decode a local file or a resolved track URL from `start_seconds`.
    pub async fn open_file(input: &str, start_seconds: f64) -> Result<Self> {
        if is_remote_input(input) {
            validate_url(input).await?;
        }
        Self::spawn(&file_args(input, start_seconds))
    }

    /// Decode a live radio stream.
    pub async fn open_live(url: &str) -> Result<Self> {
        validate_url(url).await?;
        Self::spawn(&live_args(url))
    }

    fn spawn(args: &[String]) -> Result<Self> {
        let mut child = Command::new("ffmpeg")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .or_else(|error| err(format!("Could not start ffmpeg: {error}")))?;
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let (sender, events) = mpsc::channel(BUFFERED_FRAMES);
        let task = tokio::spawn(async move {
            // Read errors first so ffmpeg cannot block on a full pipe; killed with this task.
            let tail = tokio::spawn(read_tail(stderr));
            let read = read_frames(stdout, &sender).await;
            let status = child.wait().await;
            let message = tail.await.unwrap_or_default();
            let success = read && status.is_ok_and(|status| status.success());
            let _ = sender.send(PcmEvent::End { success, message: remove_addresses(&message) }).await;
        });
        Ok(Self { events, task })
    }

    /// The next event, without waiting. `None` means the buffer is empty right now.
    pub fn try_next(&mut self) -> Option<PcmEvent> {
        self.events.try_recv().ok()
    }
}

/// Frames until the end of the output. A last partial frame is padded with silence.
async fn read_frames(mut stdout: impl AsyncRead + Unpin, sender: &mpsc::Sender<PcmEvent>) -> bool {
    loop {
        let mut frame = vec![0u8; BYTES_PER_FRAME];
        let mut filled = 0;
        while filled < BYTES_PER_FRAME {
            match stdout.read(&mut frame[filled..]).await {
                Ok(0) => {
                    if filled > 0 && sender.send(PcmEvent::Frame(frame)).await.is_err() {
                        return false;
                    }
                    return true;
                }
                Ok(read) => filled += read,
                Err(_) => return false,
            }
        }
        if sender.send(PcmEvent::Frame(frame)).await.is_err() {
            return false;
        }
    }
}

async fn read_tail(mut stderr: impl AsyncRead + Unpin) -> String {
    let mut tail = Vec::new();
    let mut chunk = [0u8; 1024];
    while let Ok(read) = stderr.read(&mut chunk).await {
        if read == 0 {
            break;
        }
        tail.extend_from_slice(&chunk[..read]);
        if tail.len() > STDERR_TAIL_BYTES {
            tail.drain(..tail.len() - STDERR_TAIL_BYTES);
        }
    }
    String::from_utf8_lossy(&tail).trim().to_owned()
}

/// Resolved track URLs are signed; they must not reach logs or the panel.
fn remove_addresses(text: &str) -> String {
    text.split_inclusive(char::is_whitespace)
        .map(|word| if is_remote_input(word.trim_start_matches(['\'', '"'])) { "[url] " } else { word })
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remote_track_is_limited_to_single_files_over_http() {
        let args = file_args("https://a.example/x", 12.5);
        assert_eq!(&args[..2], ["-ss", "12.500"]);
        let whitelist = args.iter().position(|arg| arg == "-protocol_whitelist").unwrap();
        assert_eq!(args[whitelist + 1], REMOTE_PROTOCOLS);
        assert!(args.contains(&"-re".to_owned()));
        assert!(args.ends_with(&["pipe:1".to_owned()]));
    }

    #[test]
    fn a_local_file_has_no_network_options() {
        let args = file_args("/data/music/a.mp3", 0.0);
        assert_eq!(args[..3], ["-re", "-i", "/data/music/a.mp3"]);
        assert!(!args.contains(&"-reconnect".to_owned()));
    }

    #[test]
    fn live_streams_are_not_paced_by_ffmpeg() {
        let args = live_args("http://a.example/live");
        assert!(!args.contains(&"-re".to_owned()));
        assert!(args.contains(&"-reconnect_streamed".to_owned()));
    }

    #[test]
    fn scheme_check_ignores_case() {
        assert!(is_remote_input("HTTPS://a"));
        assert!(!is_remote_input("/tmp/http://x"));
        assert!(!is_remote_input("C:\\music\\a.mp3"));
    }

    #[test]
    fn signed_addresses_are_removed_from_errors() {
        assert_eq!(
            remove_addresses("[https @ 0x1] HTTP error 403 https://rr1.example/videoplayback?sig=abc: Forbidden"),
            "[https @ 0x1] HTTP error 403 [url] Forbidden"
        );
    }

    #[tokio::test]
    async fn frames_are_cut_exactly_and_the_tail_is_padded() {
        let (sender, mut receiver) = mpsc::channel(10);
        let data = vec![7u8; BYTES_PER_FRAME + 10];
        assert!(read_frames(data.as_slice(), &sender).await);
        drop(sender);
        let Some(PcmEvent::Frame(first)) = receiver.recv().await else { panic!() };
        assert!(first.iter().all(|b| *b == 7));
        let Some(PcmEvent::Frame(last)) = receiver.recv().await else { panic!() };
        assert_eq!(last.len(), BYTES_PER_FRAME);
        assert!(last[..10].iter().all(|b| *b == 7) && last[10..].iter().all(|b| *b == 0));
        assert!(receiver.recv().await.is_none());
    }

    #[tokio::test]
    async fn exact_frames_add_no_silence() {
        let (sender, mut receiver) = mpsc::channel(10);
        assert!(read_frames(vec![1u8; BYTES_PER_FRAME].as_slice(), &sender).await);
        drop(sender);
        assert!(receiver.recv().await.is_some());
        assert!(receiver.recv().await.is_none());
    }

    #[tokio::test]
    async fn blocked_addresses_never_reach_ffmpeg() {
        assert!(PcmStream::open_live("http://127.0.0.1:8000/x").await.is_err());
        assert!(PcmStream::open_file("http://192.168.1.2/x.mp3", 0.0).await.is_err());
    }
}
