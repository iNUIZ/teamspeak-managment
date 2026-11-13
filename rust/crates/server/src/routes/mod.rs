//! The API, under the same paths as before.

pub mod auth;
pub mod channels;
pub mod clients;
pub mod chat_commands;
pub mod music_bots;
pub mod music_requests;
pub mod radio;
pub mod servers;
pub mod setup;
pub mod simple;
pub mod ts;
pub mod users;
pub mod virtual_servers;

use serde_json::Value;

/// A non-empty text field of a JSON body. Other types count as missing.
pub fn text_field<'a>(body: &'a Value, name: &str) -> Option<&'a str> {
    body[name].as_str().filter(|text| !text.is_empty())
}
