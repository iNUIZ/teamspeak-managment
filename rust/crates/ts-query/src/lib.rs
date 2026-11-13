//! WebQuery client, SSH query and the connection pool. See docs/TASARIM.md.

pub mod host;
pub mod webquery;

pub use host::{Endpoint, HostPolicy};
pub use webquery::{Priority, QueryParams, WebQueryClient};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The input cannot be used; nothing was sent.
    Invalid(String),
    /// TeamSpeak answered with an error.
    Api { code: i64, message: String },
    /// TeamSpeak's flood protection is active.
    Flood { retry_after_seconds: u64 },
    /// The server cannot be reached right now, usually because it is still starting.
    Unavailable { retry_after_seconds: u64 },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::Api { code, message } => write!(f, "TeamSpeak API error {code}: {message}"),
            Self::Flood { retry_after_seconds } => write!(f, "TeamSpeak Query temporarily paused for {retry_after_seconds}s"),
            Self::Unavailable { .. } => f.write_str("TeamSpeak Query is still starting"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
