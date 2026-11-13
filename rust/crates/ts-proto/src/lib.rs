//! TeamSpeak client protocol: UDP, encryption, identity, QuickLZ and commands.

pub mod client;
pub mod commands;
pub mod crypto;
pub mod identity;
pub mod license;
pub mod quicklz;
pub mod session;
pub mod view;

pub use client::TsClient;
pub use identity::Identity;
pub use session::{ClientOptions, Event};

/// A protocol or network failure, with a message fit for the log.
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
