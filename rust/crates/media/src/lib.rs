//! ffmpeg and yt-dlp processes, Opus, ICY and the address rules for media. See docs/TASARIM.md.

pub mod address;
pub mod cache;
pub mod encoder;
pub mod icy;
pub mod links;
pub mod pcm;
pub mod radio_browser;
pub mod youtube;

pub use encoder::OpusEncoder;
pub use pcm::{PcmEvent, PcmStream, FRAME_MS};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn err<T>(message: impl Into<String>) -> Result<T> {
    Err(Error(message.into()))
}
