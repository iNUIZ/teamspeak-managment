//! What every request can reach.

use std::sync::Arc;
use std::time::Duration;

use teamhub_core::{Config, FieldCipher};
use teamhub_db::Db;
use teamhub_voice::activity::ActivityLog;
use teamhub_voice::manager::BotManager;

use crate::client_ip::TrustProxy;
use crate::pool::ConnectionPool;
use crate::rate::RateLimiter;

const FIFTEEN_MINUTES: Duration = Duration::from_secs(15 * 60);

pub struct Limiters {
    /// Sign-in, refresh and password change. Entries are small, so this store is large.
    pub auth: RateLimiter,
    pub me: RateLimiter,
}

impl Default for Limiters {
    fn default() -> Self {
        Self { auth: RateLimiter::new(15, FIFTEEN_MINUTES, 100_000), me: RateLimiter::new(100, FIFTEEN_MINUTES, 10_000) }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub config: Arc<Config>,
    pub cipher: Arc<FieldCipher>,
    pub trust_proxy: Arc<TrustProxy>,
    pub limiters: Arc<Limiters>,
    pub pool: Arc<ConnectionPool>,
    pub bots: Option<Arc<BotManager>>,
    pub activity: Arc<ActivityLog>,
}
