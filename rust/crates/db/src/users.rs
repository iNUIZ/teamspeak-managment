//! Users, their sessions and their server access.

use crate::{models, Db, Result, Timestamp};

/// Fields of a user to change; `None` leaves one as it is.
#[derive(Default)]
pub struct UserChanges {
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub enabled: Option<bool>,
    pub password_hash: Option<String>,
}

/// One row of an audit trail.
pub struct AuditRow<'a> {
    pub id: &'a str,
    pub operation_id: &'a str,
    pub actor_user_id: i64,
    pub actor_username: &'a str,
    pub action: &'a str,
    pub connection_id: Option<i64>,
    pub virtual_server_id: Option<i64>,
    pub target_type: Option<&'a str>,
    pub target_id: Option<String>,
    pub outcome: &'a str,
    pub result_code: Option<&'a str>,
    pub completed: bool,
}

pub(crate) async fn insert_audit(executor: impl sqlx::SqliteExecutor<'_>, row: &AuditRow<'_>) -> Result<()> {
    let now = Timestamp::now();
    sqlx::query(
        r#"INSERT INTO "AdminAuditEvent" ("id", "operationId", "actorUserId", "actorUsername", "action", "connectionId", "virtualServerId", "targetType", "targetId", "outcome", "resultCode", "createdAt", "completedAt")
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(row.id)
    .bind(row.operation_id)
    .bind(row.actor_user_id)
    .bind(row.actor_username)
    .bind(row.action)
    .bind(row.connection_id)
    .bind(row.virtual_server_id)
    .bind(row.target_type)
    .bind(&row.target_id)
    .bind(row.outcome)
    .bind(row.result_code)
    .bind(now)
    .bind(row.completed.then_some(now))
    .execute(executor)
    .await?;
    Ok(())
}

impl Db {
    pub async fn user_count(&self) -> Result<i64> {
        Ok(sqlx::query_scalar(r#"SELECT count(*) FROM "User""#).fetch_one(self.pool()).await?)
    }

    pub async fn user_by_id(&self, id: i64) -> Result<Option<models::User>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "User" WHERE "id" = ?"#).bind(id).fetch_optional(self.pool()).await?)
    }

    pub async fn users(&self) -> Result<Vec<models::User>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "User" ORDER BY "id""#).fetch_all(self.pool()).await?)
    }

    pub async fn create_user(&self, username: &str, password_hash: &str, display_name: &str, role: &str) -> Result<i64> {
        let now = Timestamp::now();
        let id = sqlx::query_scalar(
            r#"INSERT INTO "User" ("username", "passwordHash", "displayName", "role", "enabled", "createdAt", "updatedAt")
               VALUES (?, ?, ?, ?, 1, ?, ?) RETURNING "id""#,
        )
        .bind(username)
        .bind(password_hash)
        .bind(display_name)
        .bind(role)
        .bind(now)
        .bind(now)
        .fetch_one(self.pool())
        .await?;
        Ok(id)
    }

    /// Create a user and its audit row together, or neither.
    pub async fn create_user_audited(&self, username: &str, password_hash: &str, display_name: &str, role: &str, audit: impl FnOnce(i64) -> OwnedAudit) -> Result<i64> {
        let now = Timestamp::now();
        let mut tx = self.pool().begin().await?;
        let id: i64 = sqlx::query_scalar(
            r#"INSERT INTO "User" ("username", "passwordHash", "displayName", "role", "enabled", "createdAt", "updatedAt")
               VALUES (?, ?, ?, ?, 1, ?, ?) RETURNING "id""#,
        )
        .bind(username)
        .bind(password_hash)
        .bind(display_name)
        .bind(role)
        .bind(now)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        let row = audit(id);
        insert_audit(&mut *tx, &row.as_row()).await?;
        tx.commit().await?;
        Ok(id)
    }

    /// Change a user and write its audit row together. False when there is no such user.
    pub async fn update_user_audited(&self, id: i64, changes: &UserChanges, audit: &OwnedAudit) -> Result<bool> {
        let mut tx = self.pool().begin().await?;
        let updated = sqlx::query(
            r#"UPDATE "User" SET
                 "displayName" = COALESCE(?, "displayName"),
                 "role" = COALESCE(?, "role"),
                 "enabled" = COALESCE(?, "enabled"),
                 "passwordHash" = COALESCE(?, "passwordHash"),
                 "updatedAt" = ?
               WHERE "id" = ?"#,
        )
        .bind(&changes.display_name)
        .bind(&changes.role)
        .bind(changes.enabled)
        .bind(&changes.password_hash)
        .bind(Timestamp::now())
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated == 0 {
            return Ok(false);
        }
        insert_audit(&mut *tx, &audit.as_row()).await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Delete a user and write its audit row together. False when there is no such user.
    pub async fn delete_user_audited(&self, id: i64, audit: &OwnedAudit) -> Result<bool> {
        let mut tx = self.pool().begin().await?;
        let deleted = sqlx::query(r#"DELETE FROM "User" WHERE "id" = ?"#).bind(id).execute(&mut *tx).await?.rows_affected();
        if deleted == 0 {
            return Ok(false);
        }
        insert_audit(&mut *tx, &audit.as_row()).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn set_last_login(&self, id: i64) -> Result<()> {
        sqlx::query(r#"UPDATE "User" SET "lastLoginAt" = ?, "updatedAt" = ? WHERE "id" = ?"#)
            .bind(Timestamp::now())
            .bind(Timestamp::now())
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn set_password_hash(&self, id: i64, hash: &str) -> Result<()> {
        sqlx::query(r#"UPDATE "User" SET "passwordHash" = ?, "updatedAt" = ? WHERE "id" = ?"#)
            .bind(hash)
            .bind(Timestamp::now())
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn has_server_access(&self, user_id: i64, server_config_id: i64) -> Result<bool> {
        let found: i64 = sqlx::query_scalar(r#"SELECT count(*) FROM "UserServerAccess" WHERE "userId" = ? AND "serverConfigId" = ?"#)
            .bind(user_id)
            .bind(server_config_id)
            .fetch_one(self.pool())
            .await?;
        Ok(found > 0)
    }

    pub async fn create_refresh_token(&self, token: &str, user_id: i64, expires_at: Timestamp, family: &str) -> Result<()> {
        sqlx::query(r#"INSERT INTO "RefreshToken" ("token", "userId", "expiresAt", "createdAt", "family") VALUES (?, ?, ?, ?, ?)"#)
            .bind(token)
            .bind(user_id)
            .bind(expires_at)
            .bind(Timestamp::now())
            .bind(family)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn refresh_token(&self, token: &str) -> Result<Option<models::RefreshToken>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "RefreshToken" WHERE "token" = ?"#).bind(token).fetch_optional(self.pool()).await?)
    }

    /// The row whose token was replaced by this one: finding it means a used token came back.
    pub async fn refresh_token_replaced_by(&self, token: &str) -> Result<Option<models::RefreshToken>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "RefreshToken" WHERE "replacedBy" = ? LIMIT 1"#).bind(token).fetch_optional(self.pool()).await?)
    }

    /// Mark a token used. False when another request already removed it.
    pub async fn mark_refresh_token_replaced(&self, id: i64, replaced_by: &str) -> Result<bool> {
        let changed = sqlx::query(r#"UPDATE "RefreshToken" SET "replacedBy" = ? WHERE "id" = ?"#).bind(replaced_by).bind(id).execute(self.pool()).await?;
        Ok(changed.rows_affected() > 0)
    }

    pub async fn delete_refresh_token(&self, id: i64) -> Result<()> {
        sqlx::query(r#"DELETE FROM "RefreshToken" WHERE "id" = ?"#).bind(id).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn delete_refresh_token_value(&self, token: &str) -> Result<()> {
        sqlx::query(r#"DELETE FROM "RefreshToken" WHERE "token" = ?"#).bind(token).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn delete_user_refresh_tokens(&self, user_id: i64) -> Result<()> {
        sqlx::query(r#"DELETE FROM "RefreshToken" WHERE "userId" = ?"#).bind(user_id).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn audit_events(&self, limit: i64) -> Result<Vec<models::AdminAuditEvent>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "AdminAuditEvent" ORDER BY "createdAt" DESC, "id" DESC LIMIT ?"#).bind(limit).fetch_all(self.pool()).await?)
    }
}

/// An audit row that owns its text.
#[derive(Debug, Clone)]
pub struct OwnedAudit {
    pub id: String,
    pub operation_id: String,
    pub actor_user_id: i64,
    pub actor_username: String,
    pub action: String,
    pub connection_id: Option<i64>,
    pub virtual_server_id: Option<i64>,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub outcome: String,
    pub result_code: Option<String>,
    pub completed: bool,
}

impl OwnedAudit {
    pub(crate) fn as_row(&self) -> AuditRow<'_> {
        AuditRow {
            id: &self.id,
            operation_id: &self.operation_id,
            actor_user_id: self.actor_user_id,
            actor_username: &self.actor_username,
            action: &self.action,
            connection_id: self.connection_id,
            virtual_server_id: self.virtual_server_id,
            target_type: self.target_type.as_deref(),
            target_id: self.target_id.clone(),
            outcome: &self.outcome,
            result_code: self.result_code.as_deref(),
            completed: self.completed,
        }
    }
}
