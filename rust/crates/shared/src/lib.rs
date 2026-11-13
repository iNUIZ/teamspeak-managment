//! Types shared by the server and the web panel. Must also compile to WebAssembly.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VoiceBotStatus {
    Stopped,
    Starting,
    Connected,
    Playing,
    Paused,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_status_keeps_the_existing_json_names() {
        assert_eq!(serde_json::to_string(&VoiceBotStatus::Playing).unwrap(), "\"playing\"");
    }
}
