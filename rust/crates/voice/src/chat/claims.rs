//! Short claims that stop one chat line from being answered twice, by peers or by repeats.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const CHAT_REPLY_COOLDOWN: Duration = Duration::from_millis(2500);
pub const HELP_COOLDOWN: Duration = Duration::from_millis(2500);
pub const HERE_ACTION_COOLDOWN: Duration = Duration::from_millis(1500);
pub const HERE_LIST_COOLDOWN: Duration = Duration::from_millis(2000);
pub const CHANNEL_COMMAND_COOLDOWN: Duration = Duration::from_millis(1500);

#[derive(Default)]
pub struct Claims {
    until: Mutex<HashMap<String, Instant>>,
}

impl Claims {
    /// True when the key is free, which also takes it for `hold`. Expired keys are dropped.
    pub fn claim(&self, key: String, hold: Duration, now: Instant) -> bool {
        let mut until = self.until.lock().expect("claims lock");
        until.retain(|_, expires| now < *expires);
        if until.contains_key(&key) {
            return false;
        }
        until.insert(key, now + hold);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_held_until_it_expires() {
        let claims = Claims::default();
        let now = Instant::now();
        let hold = Duration::from_secs(2);
        assert!(claims.claim("a".to_owned(), hold, now));
        assert!(!claims.claim("a".to_owned(), hold, now + Duration::from_secs(1)));
        assert!(claims.claim("b".to_owned(), hold, now + Duration::from_secs(1)));
        assert!(claims.claim("a".to_owned(), hold, now + hold));
    }
}
