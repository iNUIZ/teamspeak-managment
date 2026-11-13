//! TeamSpeak server connections and audit rows around remote actions.

use crate::users::{insert_audit, OwnedAudit};
use crate::{models, Db, Result, Timestamp};

/// Saved fields of a connection. Secrets are already encrypted here.
pub struct ServerFields {
    pub name: String,
    pub host: String,
    pub webquery_port: i64,
    pub api_key: String,
    pub use_https: bool,
    pub ssh_port: i64,
    pub ssh_username: Option<String>,
    pub ssh_password: Option<String>,
    pub enabled: bool,
    pub metrics_enabled: bool,
    pub metrics_port: i64,
    pub metrics_host: Option<String>,
}

impl ServerFields {
    pub fn from_row(row: &models::TsServerConfig) -> Self {
        Self {
            name: row.name.clone(),
            host: row.host.clone(),
            webquery_port: row.webquery_port,
            api_key: row.api_key.clone(),
            use_https: row.use_https,
            ssh_port: row.ssh_port,
            ssh_username: row.ssh_username.clone(),
            ssh_password: row.ssh_password.clone(),
            enabled: row.enabled,
            metrics_enabled: row.metrics_enabled,
            metrics_port: row.metrics_port,
            metrics_host: row.metrics_host.clone(),
        }
    }
}

impl Db {
    pub async fn server(&self, id: i64) -> Result<Option<models::TsServerConfig>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "TsServerConfig" WHERE "id" = ?"#).bind(id).fetch_optional(self.pool()).await?)
    }

    /// Create a connection and its audit row together.
    pub async fn create_server_audited(&self, fields: &ServerFields, audit: impl FnOnce(i64) -> OwnedAudit) -> Result<i64> {
        let now = Timestamp::now();
        let mut tx = self.pool().begin().await?;
        let id: i64 = sqlx::query_scalar(
            r#"INSERT INTO "TsServerConfig" ("name", "host", "webqueryPort", "apiKey", "useHttps", "sshPort", "sshUsername", "sshPassword", "enabled", "metricsEnabled", "metricsPort", "metricsHost", "createdAt", "updatedAt")
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING "id""#,
        )
        .bind(&fields.name)
        .bind(&fields.host)
        .bind(fields.webquery_port)
        .bind(&fields.api_key)
        .bind(fields.use_https)
        .bind(fields.ssh_port)
        .bind(&fields.ssh_username)
        .bind(&fields.ssh_password)
        .bind(fields.enabled)
        .bind(fields.metrics_enabled)
        .bind(fields.metrics_port)
        .bind(&fields.metrics_host)
        .bind(now)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        insert_audit(&mut *tx, &audit(id).as_row()).await?;
        tx.commit().await?;
        Ok(id)
    }

    /// Replace a connection's fields and write the audit row together. False when it is gone.
    pub async fn update_server_audited(&self, id: i64, fields: &ServerFields, audit: &OwnedAudit) -> Result<bool> {
        let mut tx = self.pool().begin().await?;
        let changed = sqlx::query(
            r#"UPDATE "TsServerConfig" SET "name" = ?, "host" = ?, "webqueryPort" = ?, "apiKey" = ?, "useHttps" = ?, "sshPort" = ?, "sshUsername" = ?, "sshPassword" = ?, "enabled" = ?, "metricsEnabled" = ?, "metricsPort" = ?, "metricsHost" = ?, "updatedAt" = ? WHERE "id" = ?"#,
        )
        .bind(&fields.name)
        .bind(&fields.host)
        .bind(fields.webquery_port)
        .bind(&fields.api_key)
        .bind(fields.use_https)
        .bind(fields.ssh_port)
        .bind(&fields.ssh_username)
        .bind(&fields.ssh_password)
        .bind(fields.enabled)
        .bind(fields.metrics_enabled)
        .bind(fields.metrics_port)
        .bind(&fields.metrics_host)
        .bind(Timestamp::now())
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if changed == 0 {
            return Ok(false);
        }
        insert_audit(&mut *tx, &audit.as_row()).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn delete_server_audited(&self, id: i64, audit: &OwnedAudit) -> Result<bool> {
        let mut tx = self.pool().begin().await?;
        let deleted = sqlx::query(r#"DELETE FROM "TsServerConfig" WHERE "id" = ?"#).bind(id).execute(&mut *tx).await?.rows_affected();
        if deleted == 0 {
            return Ok(false);
        }
        insert_audit(&mut *tx, &audit.as_row()).await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Save a pending row before a remote action. If this fails, the action must not run.
    pub async fn begin_audit(&self, audit: &OwnedAudit) -> Result<()> {
        insert_audit(self.pool(), &audit.as_row()).await
    }

    /// Finish a pending row. Only a row still pending changes.
    pub async fn complete_audit(&self, operation_id: &str, outcome: &str, result_code: &str, target_id: Option<&str>) -> Result<()> {
        sqlx::query(
            r#"UPDATE "AdminAuditEvent" SET "outcome" = ?, "resultCode" = ?, "completedAt" = ?, "targetId" = COALESCE(?, "targetId")
               WHERE "operationId" = ? AND "outcome" = 'pending'"#,
        )
        .bind(outcome)
        .bind(result_code)
        .bind(Timestamp::now())
        .bind(target_id)
        .bind(operation_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// A local change succeeded but a follow-up step did not.
    pub async fn mark_audit_partial(&self, operation_id: &str, result_code: &str) -> Result<()> {
        sqlx::query(
            r#"UPDATE "AdminAuditEvent" SET "outcome" = 'partial', "resultCode" = ?, "completedAt" = ?
               WHERE "operationId" = ? AND "outcome" IN ('success', 'pending')"#,
        )
        .bind(result_code)
        .bind(Timestamp::now())
        .bind(operation_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
