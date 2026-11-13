//! Fixed rate windows in a store with a hard size cap, so a flood of new addresses cannot grow
//! memory without limit. Capacity never refuses a key: a new key on a full store frees expired
//! windows, then replaces the oldest window still under its limit, and a window that has reached
//! its limit goes only when every window has.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::time::{Duration, Instant};

struct Window {
    count: u64,
    expires_at: Instant,
    sequence: u64,
}

#[derive(Default)]
struct Windows {
    by_key: HashMap<String, Window>,
    /// Creation order. All windows have one length, so this is also expiry order.
    all: BTreeMap<u64, String>,
    /// Those under the limit, in creation order.
    open: BTreeMap<u64, String>,
    next_sequence: u64,
}

impl Windows {
    fn remove(&mut self, key: &str) {
        if let Some(window) = self.by_key.remove(key) {
            self.all.remove(&window.sequence);
            self.open.remove(&window.sequence);
        }
    }

    fn prune(&mut self, now: Instant) {
        while let Some(key) = self.all.values().next().cloned() {
            if self.by_key.get(&key).is_some_and(|window| window.expires_at > now) {
                break;
            }
            self.remove(&key);
        }
    }

    fn make_room(&mut self, now: Instant, capacity: usize) {
        self.prune(now);
        if self.by_key.len() < capacity {
            return;
        }
        let victim = self.open.values().next().or_else(|| self.all.values().next()).cloned();
        if let Some(victim) = victim {
            self.remove(&victim);
        }
    }
}

/// What a hit found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub allowed: bool,
    pub limit: u64,
    pub remaining: u64,
    pub reset_after: Duration,
}

pub struct RateLimiter {
    limit: u64,
    window: Duration,
    capacity: usize,
    windows: Mutex<Windows>,
}

impl RateLimiter {
    pub fn new(limit: u64, window: Duration, capacity: usize) -> Self {
        Self { limit, window, capacity, windows: Mutex::default() }
    }

    pub fn hit(&self, key: &str) -> Hit {
        self.hit_at(key, Instant::now())
    }

    fn hit_at(&self, key: &str, now: Instant) -> Hit {
        let mut windows = self.windows.lock().expect("rate lock");
        if windows.by_key.get(key).is_some_and(|window| window.expires_at <= now) {
            windows.remove(key);
        }
        if !windows.by_key.contains_key(key) {
            if windows.by_key.len() >= self.capacity {
                windows.make_room(now, self.capacity);
            }
            let sequence = windows.next_sequence;
            windows.next_sequence += 1;
            windows.by_key.insert(key.to_owned(), Window { count: 0, expires_at: now + self.window, sequence });
            windows.all.insert(sequence, key.to_owned());
            windows.open.insert(sequence, key.to_owned());
        }
        let window = windows.by_key.get_mut(key).expect("just inserted");
        window.count += 1;
        let (count, sequence, expires_at) = (window.count, window.sequence, window.expires_at);
        if count >= self.limit {
            windows.open.remove(&sequence);
        }
        Hit {
            allowed: count <= self.limit,
            limit: self.limit,
            remaining: self.limit.saturating_sub(count),
            reset_after: expires_at.saturating_duration_since(now),
        }
    }

    pub fn size(&self) -> usize {
        self.windows.lock().expect("rate lock").by_key.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_secs(60);

    #[test]
    fn hits_over_the_limit_are_refused_until_the_window_ends() {
        let limiter = RateLimiter::new(2, MINUTE, 10);
        let now = Instant::now();
        assert!(limiter.hit_at("a", now).allowed);
        assert_eq!(limiter.hit_at("a", now).remaining, 0);
        let refused = limiter.hit_at("a", now + Duration::from_secs(30));
        assert!(!refused.allowed);
        assert_eq!(refused.reset_after, Duration::from_secs(30));
        assert!(limiter.hit_at("b", now).allowed, "another client has its own window");
        assert!(limiter.hit_at("a", now + MINUTE).allowed);
    }

    #[test]
    fn the_store_never_grows_past_its_capacity() {
        let limiter = RateLimiter::new(5, MINUTE, 100);
        let now = Instant::now();
        for i in 0..10_000 {
            limiter.hit_at(&format!("k{i}"), now);
        }
        assert_eq!(limiter.size(), 100);
    }

    #[test]
    fn an_attacker_filling_the_store_cannot_clear_a_blocked_client() {
        let limiter = RateLimiter::new(1, MINUTE, 3);
        let now = Instant::now();
        limiter.hit_at("blocked", now);
        assert!(!limiter.hit_at("blocked", now).allowed);
        for i in 0..50 {
            limiter.hit_at(&format!("new{i}"), now);
        }
        // Those at their limit go only when all are, oldest first; with a limit of 1 every new
        // window is at its limit at once, so here all are, and the oldest did go.
        let two = RateLimiter::new(2, MINUTE, 3);
        two.hit_at("blocked", now);
        two.hit_at("blocked", now);
        for i in 0..50 {
            two.hit_at(&format!("new{i}"), now);
        }
        assert!(!two.hit_at("blocked", now).allowed, "under-limit windows go first");
    }

    #[test]
    fn expired_windows_are_freed_first() {
        let limiter = RateLimiter::new(5, MINUTE, 2);
        let now = Instant::now();
        limiter.hit_at("a", now);
        limiter.hit_at("b", now + Duration::from_secs(10));
        limiter.hit_at("c", now + MINUTE);
        assert_eq!(limiter.size(), 2);
        assert_eq!(limiter.hit_at("b", now + MINUTE).remaining, 3, "b kept its count");
    }
}
