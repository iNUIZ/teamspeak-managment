//! Music bots, library songs and the bot settings stored as app settings.

use crate::{models, Db, Result, Timestamp};

pub struct NewBotRow {
    pub name: String,
    pub server_config_id: i64,
    pub nickname: String,
    pub server_password: Option<String>,
    pub external_host: Option<String>,
    pub default_channel: Option<String>,
    pub channel_password: Option<String>,
    pub command_channel_ids: Option<String>,
    pub virtual_server_id: i64,
    pub voice_port: i64,
    pub volume: i64,
    pub auto_start: bool,
    pub identity_data: String,
}

/// Fields to change; the outer `None` leaves one alone, and for optional fields an inner
/// `None` clears it.
#[derive(Debug, Clone, Default)]
pub struct BotRowChanges {
    pub name: Option<String>,
    pub nickname: Option<String>,
    pub server_password: Option<Option<String>>,
    pub external_host: Option<Option<String>>,
    pub default_channel: Option<Option<String>>,
    pub channel_password: Option<Option<String>>,
    pub command_channel_ids: Option<Option<String>>,
    pub virtual_server_id: Option<i64>,
    pub voice_port: Option<i64>,
    pub volume: Option<i64>,
    pub auto_start: Option<bool>,
}

impl Db {
    pub async fn create_music_bot(&self, new: &NewBotRow) -> Result<i64> {
        let now = Timestamp::now();
        Ok(sqlx::query_scalar(
            r#"INSERT INTO "MusicBot" ("name", "serverConfigId", "nickname", "serverPassword", "externalHost", "defaultChannel", "channelPassword", "commandChannelIds", "virtualServerId", "voicePort", "volume", "autoStart", "avatarMode", "identityData", "createdAt", "updatedAt")
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'none', ?, ?, ?) RETURNING "id""#,
        )
        .bind(&new.name)
        .bind(new.server_config_id)
        .bind(&new.nickname)
        .bind(&new.server_password)
        .bind(&new.external_host)
        .bind(&new.default_channel)
        .bind(&new.channel_password)
        .bind(&new.command_channel_ids)
        .bind(new.virtual_server_id)
        .bind(new.voice_port)
        .bind(new.volume)
        .bind(new.auto_start)
        .bind(&new.identity_data)
        .bind(now)
        .bind(now)
        .fetch_one(self.pool())
        .await?)
    }

    /// False when there is no such bot.
    pub async fn update_music_bot(&self, id: i64, changes: &BotRowChanges) -> Result<bool> {
        let Some(row) = self.music_bot(id).await? else { return Ok(false) };
        let pick = |change: &Option<Option<String>>, current: &Option<String>| change.clone().unwrap_or_else(|| current.clone());
        sqlx::query(
            r#"UPDATE "MusicBot" SET "name" = ?, "nickname" = ?, "serverPassword" = ?, "externalHost" = ?, "defaultChannel" = ?, "channelPassword" = ?, "commandChannelIds" = ?, "virtualServerId" = ?, "voicePort" = ?, "volume" = ?, "autoStart" = ?, "updatedAt" = ? WHERE "id" = ?"#,
        )
        .bind(changes.name.as_deref().unwrap_or(&row.name))
        .bind(changes.nickname.as_deref().unwrap_or(&row.nickname))
        .bind(pick(&changes.server_password, &row.server_password))
        .bind(pick(&changes.external_host, &row.external_host))
        .bind(pick(&changes.default_channel, &row.default_channel))
        .bind(pick(&changes.channel_password, &row.channel_password))
        .bind(pick(&changes.command_channel_ids, &row.command_channel_ids))
        .bind(changes.virtual_server_id.unwrap_or(row.virtual_server_id))
        .bind(changes.voice_port.unwrap_or(row.voice_port))
        .bind(changes.volume.unwrap_or(row.volume))
        .bind(changes.auto_start.unwrap_or(row.auto_start))
        .bind(Timestamp::now())
        .bind(id)
        .execute(self.pool())
        .await?;
        Ok(true)
    }

    pub async fn delete_music_bot(&self, id: i64) -> Result<()> {
        sqlx::query(r#"DELETE FROM "MusicBot" WHERE "id" = ?"#).bind(id).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn set_bot_identity(&self, id: i64, stored: &str) -> Result<()> {
        sqlx::query(r#"UPDATE "MusicBot" SET "identityData" = ?, "updatedAt" = ? WHERE "id" = ?"#).bind(stored).bind(Timestamp::now()).bind(id).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn set_bot_volume(&self, id: i64, volume: i64) -> Result<()> {
        sqlx::query(r#"UPDATE "MusicBot" SET "volume" = ?, "updatedAt" = ? WHERE "id" = ?"#).bind(volume).bind(Timestamp::now()).bind(id).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn set_bot_automation_settings(&self, id: i64, stored: &str) -> Result<()> {
        sqlx::query(r#"UPDATE "MusicBot" SET "automationSettings" = ?, "updatedAt" = ? WHERE "id" = ?"#).bind(stored).bind(Timestamp::now()).bind(id).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn song(&self, id: i64) -> Result<Option<models::Song>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "Song" WHERE "id" = ?"#).bind(id).fetch_optional(self.pool()).await?)
    }

    pub async fn playlist(&self, id: i64) -> Result<Option<models::Playlist>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "Playlist" WHERE "id" = ?"#).bind(id).fetch_optional(self.pool()).await?)
    }

    /// The songs of a playlist in order, whichever server it belongs to.
    pub async fn playlist_songs_any(&self, playlist_id: i64) -> Result<Vec<models::Song>> {
        Ok(sqlx::query_as(
            r#"SELECT s.* FROM "PlaylistSong" ps JOIN "Song" s ON s."id" = ps."songId" WHERE ps."playlistId" = ? ORDER BY ps."position""#,
        )
        .bind(playlist_id)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn radio_station_any(&self, id: i64) -> Result<Option<models::RadioStation>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "RadioStation" WHERE "id" = ?"#).bind(id).fetch_optional(self.pool()).await?)
    }

    pub async fn chat_commands(&self, server_config_id: i64) -> Result<Vec<models::ChatCommand>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "ChatCommand" WHERE "serverConfigId" = ? ORDER BY "name""#).bind(server_config_id).fetch_all(self.pool()).await?)
    }
}
