//! One WebQuery client per configured TeamSpeak server.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use teamhub_core::FieldCipher;
use teamhub_db::{models::TsServerConfig, Db};
use teamhub_ts_query::{Endpoint, HostPolicy, WebQueryClient};

use crate::error::{ApiError, ApiResult};

const DEFAULT_WEBQUERY_PORT: u16 = 10080;

pub struct ConnectionPool {
    clients: RwLock<HashMap<i64, Arc<WebQueryClient>>>,
    allow_self_signed: bool,
}

impl ConnectionPool {
    pub fn new(allow_self_signed: bool) -> Self {
        Self { clients: RwLock::default(), allow_self_signed }
    }

    /// Connect to every enabled server. A broken row is logged and skipped: one bad server must
    /// not keep the panel from starting.
    pub async fn initialize(&self, db: &Db, cipher: &FieldCipher) -> ApiResult<()> {
        for server in db.servers().await? {
            if !server.enabled || server.is_demo {
                continue;
            }
            if let Err(error) = self.connect(&server, cipher) {
                tracing::warn!(server_id = server.id, error = %error.message, "Could not set up a server connection");
            }
        }
        Ok(())
    }

    pub fn add(&self, id: i64, host: &str, port: i64, api_key: &str, use_https: bool) -> ApiResult<()> {
        let endpoint = Endpoint::new(host, Some(port), use_https, DEFAULT_WEBQUERY_PORT, HostPolicy::from_env())?;
        let client = WebQueryClient::new(&endpoint, api_key, self.allow_self_signed)?;
        self.clients.write().expect("pool lock").insert(id, Arc::new(client));
        Ok(())
    }

    pub fn connect(&self, server: &TsServerConfig, cipher: &FieldCipher) -> ApiResult<()> {
        let api_key = cipher.decrypt(&server.api_key).map_err(|_| ApiError::new(500, "A stored API key could not be read"))?;
        self.add(server.id, &server.host, server.webquery_port, &api_key, server.use_https)
    }

    pub fn remove(&self, id: i64) {
        self.clients.write().expect("pool lock").remove(&id);
    }

    pub fn get(&self, id: i64) -> ApiResult<Arc<WebQueryClient>> {
        self.clients
            .read()
            .expect("pool lock")
            .get(&id)
            .cloned()
            .ok_or_else(|| ApiError::new(503, format!("No connection configured for server config ID {id}")))
    }

    /// Drop the old client and make a new one from the saved settings.
    pub async fn refresh(&self, id: i64, db: &Db, cipher: &FieldCipher) -> ApiResult<()> {
        self.remove(id);
        if let Some(server) = db.server(id).await?.filter(|server| server.enabled && !server.is_demo) {
            self.connect(&server, cipher)?;
        }
        Ok(())
    }
}
