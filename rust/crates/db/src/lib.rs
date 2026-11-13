//! The SQLite database, shared with the old program.
//!
//! The old program runs `prisma db push` at start and refuses to drop tables it does not
//! know, so a migration-history table here would break rolling back to the old image.
//! This crate therefore adds nothing to an existing database: it creates the schema on a
//! new file and, on an existing one, checks the schema and stops when it differs.

pub mod bots;
pub mod library;
pub mod models;
pub mod schema;
pub mod servers;
mod timestamp;
pub mod users;

use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

pub use schema::SchemaError;
pub use timestamp::Timestamp;
pub use servers::ServerFields;
pub use users::{OwnedAudit, UserChanges};

#[derive(Debug)]
pub enum Error {
    Database(sqlx::Error),
    Schema(SchemaError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(error) => write!(f, "Database error: {error}"),
            Self::Schema(error) => write!(f, "Database schema does not match: {error}"),
        }
    }
}

impl std::error::Error for Error {}

impl Error {
    /// A unique index refused the row, such as a username already taken.
    pub fn is_unique_violation(&self) -> bool {
        matches!(self, Self::Database(sqlx::Error::Database(error)) if error.is_unique_violation())
    }
}

impl From<sqlx::Error> for Error {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    /// Open a read-write database, creating the schema when the file is new.
    /// `url` is the `DATABASE_URL` value: `file:path` or `sqlite:path`.
    pub async fn open(url: &str) -> Result<Self> {
        let options = options(url)?.create_if_missing(true);
        Self::connect(options, true).await
    }

    /// Open without any chance of changing the file. Fails on a database with no schema.
    pub async fn open_read_only(url: &str) -> Result<Self> {
        Self::connect(options(url)?.read_only(true), false).await
    }

    /// A private database in memory, for tests.
    pub async fn open_in_memory() -> Result<Self> {
        Self::connect(SqliteConnectOptions::from_str("sqlite::memory:")?, true).await
    }

    async fn connect(options: SqliteConnectOptions, may_create: bool) -> Result<Self> {
        // SQLite allows one writer, and a memory database exists per connection.
        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await?;
        let db = Self { pool };
        if may_create && schema::is_empty(&db.pool).await? {
            schema::create(&db.pool).await?;
        }
        if let Err(error) = schema::check(&db.pool).await {
            // Release the file: on Windows an open connection keeps it locked.
            db.close().await;
            return Err(Error::Schema(error));
        }
        Ok(db)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }

    pub async fn setting(&self, key: &str) -> Result<Option<String>> {
        let value = sqlx::query_scalar(r#"SELECT "value" FROM "AppSetting" WHERE "key" = ?"#)
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(value)
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO "AppSetting" ("key", "value", "updatedAt") VALUES (?, ?, ?)
               ON CONFLICT ("key") DO UPDATE SET "value" = excluded."value", "updatedAt" = excluded."updatedAt""#,
        )
        .bind(key)
        .bind(value)
        .bind(Timestamp::now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn user_by_username(&self, username: &str) -> Result<Option<models::User>> {
        let user = sqlx::query_as(r#"SELECT * FROM "User" WHERE "username" = ?"#)
            .bind(username)
            .fetch_optional(&self.pool)
            .await?;
        Ok(user)
    }

    pub async fn servers(&self) -> Result<Vec<models::TsServerConfig>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "TsServerConfig" ORDER BY "id""#).fetch_all(&self.pool).await?)
    }

    pub async fn music_bots(&self) -> Result<Vec<models::MusicBot>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "MusicBot" ORDER BY "id""#).fetch_all(&self.pool).await?)
    }

    pub async fn music_bot(&self, id: i64) -> Result<Option<models::MusicBot>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "MusicBot" WHERE "id" = ?"#).bind(id).fetch_optional(&self.pool).await?)
    }

    pub async fn set_automation_state(&self, bot_id: i64, state: &str) -> Result<()> {
        sqlx::query(r#"UPDATE "MusicBot" SET "automationState" = ?, "updatedAt" = ? WHERE "id" = ?"#)
            .bind(state)
            .bind(Timestamp::now())
            .bind(bot_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn radio_stations(&self, server_config_id: i64) -> Result<Vec<models::RadioStation>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "RadioStation" WHERE "serverConfigId" = ? ORDER BY "name""#)
            .bind(server_config_id)
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn radio_station(&self, server_config_id: i64, id: i64) -> Result<Option<models::RadioStation>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "RadioStation" WHERE "id" = ? AND "serverConfigId" = ?"#)
            .bind(id)
            .bind(server_config_id)
            .fetch_optional(&self.pool)
            .await?)
    }

    /// This server's playlists and the shared ones.
    pub async fn playlists(&self, server_config_id: i64) -> Result<Vec<models::Playlist>> {
        Ok(sqlx::query_as(
            r#"SELECT * FROM "Playlist" WHERE "serverConfigId" = ? OR "serverConfigId" IS NULL ORDER BY "name""#,
        )
        .bind(server_config_id)
        .fetch_all(&self.pool)
        .await?)
    }

    /// The songs of a playlist in order, when the server may use it.
    pub async fn playlist_songs(&self, server_config_id: i64, playlist_id: i64) -> Result<Vec<models::Song>> {
        Ok(sqlx::query_as(
            r#"SELECT s.* FROM "PlaylistSong" ps
               JOIN "Song" s ON s."id" = ps."songId"
               JOIN "Playlist" p ON p."id" = ps."playlistId"
               WHERE ps."playlistId" = ? AND (p."serverConfigId" = ? OR p."serverConfigId" IS NULL)
               ORDER BY ps."position""#,
        )
        .bind(playlist_id)
        .bind(server_config_id)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Auto-DJ's playlist: also not one that belongs to a bot on another server.
    pub async fn automation_playlist_songs(&self, server_config_id: i64, playlist_id: i64) -> Result<Vec<models::Song>> {
        Ok(sqlx::query_as(
            r#"SELECT s.* FROM "PlaylistSong" ps
               JOIN "Song" s ON s."id" = ps."songId"
               JOIN "Playlist" p ON p."id" = ps."playlistId"
               LEFT JOIN "MusicBot" b ON b."id" = p."musicBotId"
               WHERE ps."playlistId" = ?1 AND (p."serverConfigId" = ?2 OR p."serverConfigId" IS NULL)
                 AND (p."musicBotId" IS NULL OR b."serverConfigId" = ?2)
               ORDER BY ps."position""#,
        )
        .bind(playlist_id)
        .bind(server_config_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn enabled_chat_commands(&self, server_config_id: i64) -> Result<Vec<models::ChatCommand>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "ChatCommand" WHERE "serverConfigId" = ? AND "enabled" = 1 ORDER BY "name""#)
            .bind(server_config_id)
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn chat_command(&self, server_config_id: i64, name: &str) -> Result<Option<models::ChatCommand>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "ChatCommand" WHERE "serverConfigId" = ? AND "name" = ?"#)
            .bind(server_config_id)
            .bind(name)
            .fetch_optional(&self.pool)
            .await?)
    }

    /// Save a request, refreshing it when the same source was requested before, then keep
    /// only the newest `limit` for that bot. One transaction per call.
    pub async fn record_music_requests(
        &self,
        server_config_id: i64,
        music_bot_id: i64,
        requests: &[NewMusicRequest],
        limit: i64,
    ) -> Result<()> {
        if requests.is_empty() {
            return Ok(());
        }
        let now = Timestamp::now();
        let mut tx = self.pool.begin().await?;
        for request in requests {
            let title = if request.title.is_empty() { "Unknown Title" } else { &request.title };
            sqlx::query(
                r#"INSERT INTO "MusicRequest" ("title", "artist", "duration", "url", "source", "status", "serverConfigId", "musicBotId", "requestedAt")
                   VALUES (?, ?, ?, ?, ?, 'available', ?, ?, ?)
                   ON CONFLICT ("serverConfigId", "musicBotId", "url") DO UPDATE SET
                     "title" = excluded."title", "artist" = excluded."artist", "duration" = excluded."duration",
                     "source" = excluded."source", "status" = 'available', "requestedAt" = excluded."requestedAt""#,
            )
            .bind(title)
            .bind(&request.artist)
            .bind(request.duration)
            .bind(&request.url)
            .bind(&request.source)
            .bind(server_config_id)
            .bind(music_bot_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        prune_requests(&mut tx, server_config_id, music_bot_id, limit).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Requests of a bot and the older rows that belong to no bot, newest first.
    pub async fn music_requests(&self, server_config_id: i64, music_bot_id: i64, limit: i64) -> Result<Vec<models::MusicRequest>> {
        Ok(sqlx::query_as(
            r#"SELECT * FROM "MusicRequest" WHERE "serverConfigId" = ? AND "musicBotId" IN (?, 0)
               ORDER BY "requestedAt" DESC, "id" DESC LIMIT ?"#,
        )
        .bind(server_config_id)
        .bind(music_bot_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn music_request(&self, server_config_id: i64, music_bot_id: i64, id: i64) -> Result<Option<models::MusicRequest>> {
        Ok(sqlx::query_as(r#"SELECT * FROM "MusicRequest" WHERE "serverConfigId" = ? AND "musicBotId" IN (?, 0) AND "id" = ?"#)
            .bind(server_config_id)
            .bind(music_bot_id)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn mark_music_request_unavailable(&self, id: i64) -> Result<()> {
        sqlx::query(r#"UPDATE "MusicRequest" SET "status" = 'unavailable' WHERE "id" = ?"#).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    /// Remember the file a download produced for a library song.
    pub async fn set_song_file(&self, song_id: i64, file_path: &str) -> Result<()> {
        sqlx::query(r#"UPDATE "Song" SET "filePath" = ? WHERE "id" = ?"#).bind(file_path).bind(song_id).execute(&self.pool).await?;
        Ok(())
    }
}

/// One entry for [`Db::record_music_requests`].
pub struct NewMusicRequest {
    pub title: String,
    pub artist: Option<String>,
    pub duration: Option<f64>,
    pub url: String,
    pub source: String,
}

async fn prune_requests(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, server_config_id: i64, music_bot_id: i64, limit: i64) -> Result<()> {
    sqlx::query(
        r#"DELETE FROM "MusicRequest" WHERE "id" IN (
             SELECT "id" FROM "MusicRequest" WHERE "serverConfigId" = ? AND "musicBotId" = ?
             ORDER BY "requestedAt" DESC, "id" DESC LIMIT -1 OFFSET ?)"#,
    )
    .bind(server_config_id)
    .bind(music_bot_id)
    .bind(limit)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn options(url: &str) -> Result<SqliteConnectOptions> {
    let path = url.strip_prefix("file:").or_else(|| url.strip_prefix("sqlite:")).unwrap_or(url);
    Ok(SqliteConnectOptions::new().filename(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_new_database_gets_the_schema_and_reopens() {
        let dir = std::env::temp_dir().join(format!("teamhub-db-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("new.db");
        let _ = std::fs::remove_file(&path);
        let url = format!("file:{}", path.display());

        let db = Db::open(&url).await.unwrap();
        db.set_setting("a", "1").await.unwrap();
        db.close().await;

        let db = Db::open_read_only(&url).await.unwrap();
        assert_eq!(db.setting("a").await.unwrap().as_deref(), Some("1"));
        assert!(db.set_setting("a", "2").await.is_err(), "read-only must not write");
        db.close().await;
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn settings_are_read_and_replaced() {
        let db = Db::open_in_memory().await.unwrap();
        assert_eq!(db.setting("k").await.unwrap(), None);
        db.set_setting("k", "1").await.unwrap();
        db.set_setting("k", "2").await.unwrap();
        assert_eq!(db.setting("k").await.unwrap().as_deref(), Some("2"));
    }

    #[tokio::test]
    async fn rows_written_the_old_way_decode() {
        let db = Db::open_in_memory().await.unwrap();
        sqlx::query(
            r#"INSERT INTO "User" ("username", "passwordHash", "displayName", "role", "enabled", "createdAt", "updatedAt")
               VALUES ('admin', 'hash', 'Admin', 'admin', 1, 1793767111000, 1793767111000)"#,
        )
        .execute(db.pool())
        .await
        .unwrap();
        let user = db.user_by_username("admin").await.unwrap().unwrap();
        assert!(user.enabled);
        assert_eq!(user.created_at.millis(), 1_793_767_111_000);
        assert!(db.user_by_username("nobody").await.unwrap().is_none());
    }

    async fn seed_server_and_bot(db: &Db) {
        sqlx::raw_sql(
            r#"INSERT INTO "TsServerConfig" ("id", "name", "host", "webqueryPort", "apiKey", "useHttps", "createdAt", "updatedAt")
               VALUES (1, 's', 'h', 1, 'k', 0, 0, 0), (2, 's2', 'h', 1, 'k', 0, 0, 0);
               INSERT INTO "MusicBot" ("id", "name", "serverConfigId", "createdAt", "updatedAt") VALUES (5, 'b', 1, 0, 0), (6, 'other', 2, 0, 0);"#,
        )
        .execute(db.pool())
        .await
        .unwrap();
    }

    fn request(url: &str, title: &str) -> NewMusicRequest {
        NewMusicRequest { title: title.to_owned(), artist: None, duration: None, url: url.to_owned(), source: "youtube".to_owned() }
    }

    #[tokio::test]
    async fn music_requests_refresh_and_are_pruned() {
        let db = Db::open_in_memory().await.unwrap();
        seed_server_and_bot(&db).await;
        db.record_music_requests(1, 5, &[request("a", "A"), request("b", "B")], 10).await.unwrap();
        db.record_music_requests(1, 5, &[request("a", "A2")], 10).await.unwrap();
        let rows = db.music_requests(1, 5, 10).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].title, "A2", "the refreshed one is newest");

        db.mark_music_request_unavailable(rows[1].id).await.unwrap();
        assert_eq!(db.music_request(1, 5, rows[1].id).await.unwrap().unwrap().status, "unavailable");
        assert!(db.music_request(1, 6, rows[1].id).await.unwrap().is_none(), "another bot's request");

        db.record_music_requests(1, 5, &[request("c", ""), request("d", "D"), request("e", "E")], 2).await.unwrap();
        let rows = db.music_requests(1, 5, 10).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.url != "a" && row.url != "b"));
    }

    #[tokio::test]
    async fn playlists_stay_within_their_server() {
        let db = Db::open_in_memory().await.unwrap();
        seed_server_and_bot(&db).await;
        sqlx::raw_sql(
            r#"INSERT INTO "Song" ("id", "title", "filePath", "source", "serverConfigId", "createdAt") VALUES (1, 'one', '', 'youtube', 1, 0), (2, 'two', '', 'youtube', 1, 0);
               INSERT INTO "Playlist" ("id", "name", "mode", "serverConfigId", "musicBotId", "createdAt") VALUES (1, 'mine', 'x', 1, NULL, 0), (2, 'shared', 'x', NULL, NULL, 0), (3, 'theirs', 'x', 2, NULL, 0), (4, 'bot of theirs', 'x', NULL, 6, 0);
               INSERT INTO "PlaylistSong" ("playlistId", "songId", "position") VALUES (1, 2, 0), (1, 1, 1), (3, 1, 0), (4, 1, 0);"#,
        )
        .execute(db.pool())
        .await
        .unwrap();
        let names: Vec<String> = db.playlists(1).await.unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["bot of theirs", "mine", "shared"]);
        let songs = db.playlist_songs(1, 1).await.unwrap();
        assert_eq!(songs.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["two", "one"]);
        assert!(db.playlist_songs(1, 3).await.unwrap().is_empty());
        assert_eq!(db.automation_playlist_songs(1, 1).await.unwrap().len(), 2);
        assert!(db.automation_playlist_songs(1, 4).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn stations_commands_and_state() {
        let db = Db::open_in_memory().await.unwrap();
        seed_server_and_bot(&db).await;
        sqlx::raw_sql(
            r#"INSERT INTO "RadioStation" ("id", "name", "url", "serverConfigId", "createdAt") VALUES (1, 'B', 'u', 1, 0), (2, 'A', 'u', 1, 0), (3, 'X', 'u', 2, 0);
               INSERT INTO "ChatCommand" ("serverConfigId", "name", "response", "enabled", "createdAt", "updatedAt") VALUES (1, 'on', 'r', 1, 0, 0), (1, 'off', 'r', 0, 0, 0);"#,
        )
        .execute(db.pool())
        .await
        .unwrap();
        assert_eq!(db.radio_stations(1).await.unwrap()[0].name, "A");
        assert!(db.radio_station(1, 3).await.unwrap().is_none());
        assert_eq!(db.enabled_chat_commands(1).await.unwrap().len(), 1);
        assert!(!db.chat_command(1, "off").await.unwrap().unwrap().enabled);
        db.set_automation_state(5, "{}").await.unwrap();
        assert_eq!(db.music_bot(5).await.unwrap().unwrap().automation_state.as_deref(), Some("{}"));
    }

    #[tokio::test]
    async fn every_table_decodes_on_the_baseline_schema() {
        let db = Db::open_in_memory().await.unwrap();
        let counts = models::decode_all(db.pool()).await.unwrap();
        assert_eq!(counts.len(), 25);
        assert!(counts.iter().all(|(_, rows)| *rows == 0));
    }
}
