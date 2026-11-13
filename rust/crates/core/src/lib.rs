//! Settings, field encryption and password hashes.

pub mod config;
pub mod field_cipher;
pub mod password;

pub use config::Config;
pub use field_cipher::FieldCipher;

/// A failure with a message fit for the log. Never contains a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
