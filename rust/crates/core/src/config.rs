//! Settings from the environment, with the same names and defaults as the old program.

use crate::{Error, Result};

const DEV_JWT_SECRET: &str = "dev-secret-change-me-in-production";

#[derive(Clone)]
pub struct Config {
    pub port: u16,
    pub production: bool,
    pub database_url: String,
    pub jwt_secret: String,
    /// What the field-encryption key is derived from.
    pub encryption_source: String,
    pub jwt_access_expiry: String,
    pub jwt_refresh_expiry: String,
    pub frontend_url: String,
    pub trust_proxy: Option<String>,
    pub ts_allow_self_signed: bool,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("port", &self.port)
            .field("production", &self.production)
            .field("database_url", &self.database_url)
            .field("frontend_url", &self.frontend_url)
            .finish_non_exhaustive()
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let text = |name: &str, default: &str| get(name).unwrap_or_else(|| default.to_owned());
        let production = get("NODE_ENV").as_deref() == Some("production");
        let jwt_secret = text("JWT_SECRET", DEV_JWT_SECRET);
        let encryption_source = match get("ENCRYPTION_KEY").filter(|key| !key.is_empty()) {
            Some(key) => key,
            None if production => return Err(Error("ENCRYPTION_KEY is required in production".into())),
            None => jwt_secret.clone(),
        };
        Ok(Self {
            port: get("PORT").and_then(|port| port.parse().ok()).unwrap_or(3001),
            production,
            database_url: text("DATABASE_URL", "file:./data/ts6webui.db"),
            jwt_secret,
            encryption_source,
            jwt_access_expiry: text("JWT_ACCESS_EXPIRY", "15m"),
            jwt_refresh_expiry: text("JWT_REFRESH_EXPIRY", "7d"),
            frontend_url: text("FRONTEND_URL", "http://localhost:5173"),
            trust_proxy: get("TRUST_PROXY"),
            ts_allow_self_signed: matches!(get("TS_ALLOW_SELF_SIGNED").as_deref(), Some("true" | "1")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(values: &[(&str, &str)]) -> Result<Config> {
        Config::from_lookup(|name| values.iter().find(|(k, _)| *k == name).map(|(_, v)| (*v).to_owned()))
    }

    #[test]
    fn defaults_match_the_old_program() {
        let c = config(&[]).unwrap();
        assert_eq!(c.port, 3001);
        assert_eq!(c.database_url, "file:./data/ts6webui.db");
        assert_eq!(c.jwt_access_expiry, "15m");
        assert_eq!(c.jwt_refresh_expiry, "7d");
        assert_eq!(c.encryption_source, DEV_JWT_SECRET);
    }

    #[test]
    fn production_refuses_to_start_without_an_encryption_key() {
        assert!(config(&[("NODE_ENV", "production")]).is_err());
        let c = config(&[("NODE_ENV", "production"), ("ENCRYPTION_KEY", "k")]).unwrap();
        assert_eq!(c.encryption_source, "k");
    }

    #[test]
    fn debug_output_hides_secrets() {
        let c = config(&[("JWT_SECRET", "hidden-a"), ("ENCRYPTION_KEY", "hidden-b")]).unwrap();
        let printed = format!("{c:?}");
        assert!(!printed.contains("hidden-a") && !printed.contains("hidden-b"));
    }
}
