//! "Who did what" on each bot, from TeamSpeak chat and the panel. Kept in app settings so it
//! survives restarts. Only the visible name is stored, never an identity or address.

use serde_json::{json, Value};
use teamhub_db::Db;
use tokio::sync::Mutex;

const MAX_ENTRIES: usize = 50;
const MAX_ACTION_CHARS: usize = 160;
const MAX_NAME_CHARS: usize = 40;

fn key(bot_id: i64) -> String {
    format!("bot_activity:{bot_id}")
}

fn clean_name(who: &str) -> String {
    let name: String = who.chars().filter(|c| !c.is_control()).collect();
    let name = name.trim();
    if name.is_empty() {
        "Bilinmeyen".to_owned()
    } else {
        name.chars().take(MAX_NAME_CHARS).collect()
    }
}

/// The text for a chat command. Spoken text and remote links never reach the log.
pub fn describe_chat_command(command: &str, args: &str, search_choice: Option<usize>) -> String {
    if let Some(choice) = search_choice {
        return format!("!ara sonucu #{choice}");
    }
    if matches!(command, "say" | "remote") {
        return format!("!{command}");
    }
    let text = if args.is_empty() { format!("!{command}") } else { format!("!{command} {args}") };
    if text.chars().count() > MAX_ACTION_CHARS {
        return format!("{}…", text.chars().take(MAX_ACTION_CHARS - 1).collect::<String>());
    }
    text
}

fn parse_entries(stored: Option<&str>) -> Vec<Value> {
    let parsed = stored.and_then(|text| serde_json::from_str::<Value>(text).ok());
    let list = parsed.as_ref().and_then(Value::as_array).cloned().unwrap_or_default();
    list.into_iter().filter(|entry| entry["action"].is_string()).take(MAX_ENTRIES).collect()
}

/// One writer at a time, so entries stay in order and none is lost.
pub struct ActivityLog {
    db: Db,
    write: Mutex<()>,
}

impl ActivityLog {
    pub fn new(db: Db) -> Self {
        Self { db, write: Mutex::new(()) }
    }

    /// A log write never breaks what triggered it.
    pub async fn record(&self, bot_id: i64, via: &str, who: &str, action: &str) {
        let _guard = self.write.lock().await;
        let result = async {
            let stored = self.db.setting(&key(bot_id)).await?;
            let mut entries = vec![json!({
                "at": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                "via": via,
                "who": clean_name(who),
                "action": action.chars().take(MAX_ACTION_CHARS).collect::<String>(),
            })];
            entries.extend(parse_entries(stored.as_deref()));
            entries.truncate(MAX_ENTRIES);
            self.db.set_setting(&key(bot_id), &Value::Array(entries).to_string()).await
        }
        .await;
        if let Err(error) = result {
            tracing::warn!(bot_id, %error, "Could not record bot activity");
        }
    }

    pub async fn list(&self, bot_id: i64) -> Result<Vec<Value>, teamhub_db::Error> {
        Ok(parse_entries(self.db.setting(&key(bot_id)).await?.as_deref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_text_stays_out() {
        assert_eq!(describe_chat_command("say", "gizli", None), "!say");
        assert_eq!(describe_chat_command("remote", "x", None), "!remote");
        assert_eq!(describe_chat_command("play", "song", None), "!play song");
        assert_eq!(describe_chat_command("play", "", Some(3)), "!ara sonucu #3");
        assert_eq!(describe_chat_command("play", &"x".repeat(200), None).chars().count(), MAX_ACTION_CHARS);
    }

    #[tokio::test]
    async fn newest_first_and_bounded() {
        let log = ActivityLog::new(Db::open_in_memory().await.unwrap());
        for number in 0..60 {
            log.record(1, "panel", " Ali\u{7} ", &format!("!vol {number}")).await;
        }
        let entries = log.list(1).await.unwrap();
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(entries[0]["action"], "!vol 59");
        assert_eq!(entries[0]["who"], "Ali");
        assert_eq!(entries[0]["via"], "panel");
        assert!(log.list(2).await.unwrap().is_empty());
    }
}
