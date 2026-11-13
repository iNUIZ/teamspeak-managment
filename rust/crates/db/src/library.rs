//! Radio stations, custom chat commands and the panel's view of request history.

use crate::{models, prune_requests, Db, Result, Timestamp};

pub struct NewStation<'a> {
    pub name: &'a str,
    pub url: &'a str,
    pub genre: Option<&'a str>,
    pub image_url: Option<&'a str>,
}

pub struct CommandFields<'a> {
    pub name: &'a str,
    pub response: &'a str,
    pub description: Option<&'a str>,
    pub enabled: bool,
}

impl Db {
    pub async fn create_radio_station(&self, server_config_id: i64, new: &NewStation<'_>) -> Result<models::RadioStation> {
        Ok(sqlx::query_as(r#"INSERT INTO "RadioStation" ("name", "url", "genre", "imageUrl", "serverConfigId", "createdAt") VALUES (?, ?, ?, ?, ?, ?) RETURNING *"#)
            .bind(new.name)
            .bind(new.url)
            .bind(new.genre)
            .bind(new.image_url)
            .bind(server_config_id)
            .bind(Timestamp::now())
            .fetch_one(self.pool())
            .await?)
    }

    pub async fn update_radio_station(&self, server_config_id: i64, id: i64, name: &str, url: &str, genre: Option<&str>) -> Result<models::RadioStation> {
        Ok(sqlx::query_as(r#"UPDATE "RadioStation" SET "name" = ?, "url" = ?, "genre" = ? WHERE "id" = ? AND "serverConfigId" = ? RETURNING *"#)
            .bind(name)
            .bind(url)
            .bind(genre)
            .bind(id)
            .bind(server_config_id)
            .fetch_one(self.pool())
            .await?)
    }

    /// False when this server has no such station.
    pub async fn delete_radio_station(&self, server_config_id: i64, id: i64) -> Result<bool> {
        let done = sqlx::query(r#"DELETE FROM "RadioStation" WHERE "id" = ? AND "serverConfigId" = ?"#).bind(id).bind(server_config_id).execute(self.pool()).await?;
        Ok(done.rows_affected() > 0)
    }

    pub async fn first_radio_station(&self, server_config_id: i64) -> Result<Option<models::RadioStation>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "RadioStation" WHERE "serverConfigId" = ? ORDER BY "id" LIMIT 1"#).bind(server_config_id).fetch_optional(self.pool()).await?)
    }

    /// Give this server's stations the lowest free ids again, in the same order. The counter
    /// has to go before the new rows, or they only count on from the old highest id.
    pub async fn compact_radio_ids(&self, server_config_id: i64) -> Result<Vec<models::RadioStation>> {
        let mut tx = self.pool().begin().await?;
        let stations: Vec<models::RadioStation> = sqlx::query_as(r#"SELECT * FROM "RadioStation" WHERE "serverConfigId" = ? ORDER BY "id""#).bind(server_config_id).fetch_all(&mut *tx).await?;
        sqlx::query(r#"DELETE FROM "RadioStation" WHERE "serverConfigId" = ?"#).bind(server_config_id).execute(&mut *tx).await?;
        sqlx::query(r#"DELETE FROM sqlite_sequence WHERE name = 'RadioStation'"#).execute(&mut *tx).await?;
        let mut fresh = Vec::with_capacity(stations.len());
        for station in stations {
            fresh.push(
                sqlx::query_as(r#"INSERT INTO "RadioStation" ("name", "url", "genre", "imageUrl", "serverConfigId", "createdAt") VALUES (?, ?, ?, ?, ?, ?) RETURNING *"#)
                    .bind(&station.name)
                    .bind(&station.url)
                    .bind(&station.genre)
                    .bind(&station.image_url)
                    .bind(server_config_id)
                    .bind(Timestamp::now())
                    .fetch_one(&mut *tx)
                    .await?,
            );
        }
        tx.commit().await?;
        Ok(fresh)
    }

    /// Ready-made stations, once per connection: a station deleted on purpose stays deleted.
    pub async fn seed_radio_stations(&self, server_config_id: i64, stations: &[(&str, &str, &str)]) -> Result<()> {
        let key = format!("radio-defaults-v1:{server_config_id}");
        let mut tx = self.pool().begin().await?;
        let seeded: Option<String> = sqlx::query_scalar(r#"SELECT "value" FROM "AppSetting" WHERE "key" = ?"#).bind(&key).fetch_optional(&mut *tx).await?;
        if seeded.is_some() {
            return Ok(());
        }
        for (name, url, genre) in stations {
            let exists: Option<i64> = sqlx::query_scalar(r#"SELECT "id" FROM "RadioStation" WHERE "serverConfigId" = ? AND "url" = ? LIMIT 1"#).bind(server_config_id).bind(url).fetch_optional(&mut *tx).await?;
            if exists.is_none() {
                sqlx::query(r#"INSERT INTO "RadioStation" ("name", "url", "genre", "serverConfigId", "createdAt") VALUES (?, ?, ?, ?, ?)"#)
                    .bind(name)
                    .bind(url)
                    .bind(genre)
                    .bind(server_config_id)
                    .bind(Timestamp::now())
                    .execute(&mut *tx)
                    .await?;
            }
        }
        sqlx::query(r#"INSERT INTO "AppSetting" ("key", "value", "updatedAt") VALUES (?, 'seeded', ?)"#).bind(&key).bind(Timestamp::now()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn chat_command_by_id(&self, server_config_id: i64, id: i64) -> Result<Option<models::ChatCommand>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "ChatCommand" WHERE "id" = ? AND "serverConfigId" = ?"#).bind(id).bind(server_config_id).fetch_optional(self.pool()).await?)
    }

    pub async fn create_chat_command(&self, server_config_id: i64, fields: &CommandFields<'_>) -> Result<models::ChatCommand> {
        let now = Timestamp::now();
        Ok(sqlx::query_as(r#"INSERT INTO "ChatCommand" ("serverConfigId", "name", "response", "description", "enabled", "createdAt", "updatedAt") VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING *"#)
            .bind(server_config_id)
            .bind(fields.name)
            .bind(fields.response)
            .bind(fields.description)
            .bind(fields.enabled)
            .bind(now)
            .bind(now)
            .fetch_one(self.pool())
            .await?)
    }

    pub async fn update_chat_command(&self, server_config_id: i64, id: i64, fields: &CommandFields<'_>) -> Result<models::ChatCommand> {
        Ok(sqlx::query_as(r#"UPDATE "ChatCommand" SET "name" = ?, "response" = ?, "description" = ?, "enabled" = ?, "updatedAt" = ? WHERE "id" = ? AND "serverConfigId" = ? RETURNING *"#)
            .bind(fields.name)
            .bind(fields.response)
            .bind(fields.description)
            .bind(fields.enabled)
            .bind(Timestamp::now())
            .bind(id)
            .bind(server_config_id)
            .fetch_one(self.pool())
            .await?)
    }

    pub async fn delete_chat_command(&self, server_config_id: i64, id: i64) -> Result<bool> {
        let done = sqlx::query(r#"DELETE FROM "ChatCommand" WHERE "id" = ? AND "serverConfigId" = ?"#).bind(id).bind(server_config_id).execute(self.pool()).await?;
        Ok(done.rows_affected() > 0)
    }

    /// Every bot's requests on a connection, newest first.
    pub async fn server_music_requests(&self, server_config_id: i64, limit: i64) -> Result<Vec<models::MusicRequest>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "MusicRequest" WHERE "serverConfigId" = ? ORDER BY "requestedAt" DESC, "id" DESC LIMIT ?"#).bind(server_config_id).bind(limit).fetch_all(self.pool()).await?)
    }

    pub async fn server_music_request(&self, server_config_id: i64, id: i64) -> Result<Option<models::MusicRequest>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "MusicRequest" WHERE "id" = ? AND "serverConfigId" = ?"#).bind(id).bind(server_config_id).fetch_optional(self.pool()).await?)
    }

    /// Older rows belong to no bot and are trimmed too. Rows of bots that are gone are removed.
    pub async fn tidy_music_requests(&self, server_config_id: i64, limit: i64) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        prune_requests(&mut tx, server_config_id, 0, limit).await?;
        sqlx::query(
            r#"DELETE FROM "MusicRequest" WHERE "serverConfigId" = ? AND "musicBotId" > 0 AND NOT EXISTS (
                 SELECT 1 FROM "MusicBot" WHERE "MusicBot"."id" = "MusicRequest"."musicBotId" AND "MusicBot"."serverConfigId" = "MusicRequest"."serverConfigId")"#,
        )
        .bind(server_config_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
