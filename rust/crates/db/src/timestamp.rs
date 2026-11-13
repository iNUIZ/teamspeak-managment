//! Date columns: stored as whole milliseconds since 1970, the way the old program writes them.
//! Rows created by a column default hold SQLite's text form, so that is read as well.

use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::sqlite::{Sqlite, SqliteArgumentsBuffer, SqliteTypeInfo, SqliteValueRef};
use sqlx::{Decode, Encode, Type, TypeInfo, ValueRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(pub DateTime<Utc>);

impl Timestamp {
    pub fn now() -> Self {
        Self(Utc::now())
    }

    pub fn millis(&self) -> i64 {
        self.0.timestamp_millis()
    }

    pub fn from_millis(millis: i64) -> Option<Self> {
        DateTime::from_timestamp_millis(millis).map(Self)
    }

    /// The form the API has always sent: `2026-11-04T07:38:31.000Z`.
    pub fn to_iso(&self) -> String {
        self.0.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }
}

impl Type<Sqlite> for Timestamp {
    fn type_info() -> SqliteTypeInfo {
        <i64 as Type<Sqlite>>::type_info()
    }

    fn compatible(_: &SqliteTypeInfo) -> bool {
        true
    }
}

impl Encode<'_, Sqlite> for Timestamp {
    fn encode_by_ref(&self, buffer: &mut SqliteArgumentsBuffer) -> Result<IsNull, BoxDynError> {
        <i64 as Encode<Sqlite>>::encode_by_ref(&self.millis(), buffer)
    }
}

impl<'r> Decode<'r, Sqlite> for Timestamp {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, BoxDynError> {
        if value.type_info().name() == "TEXT" {
            let text = <&str as Decode<Sqlite>>::decode(value)?;
            if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
                return Ok(Self(parsed.with_timezone(&Utc)));
            }
            let parsed = NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f")?;
            return Ok(Self(parsed.and_utc()));
        }
        let millis = <i64 as Decode<Sqlite>>::decode(value)?;
        Self::from_millis(millis).ok_or_else(|| format!("Date out of range: {millis}").into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn read(sql: &'static str) -> Timestamp {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query_scalar(sql).fetch_one(&pool).await.unwrap()
    }

    #[tokio::test]
    async fn it_reads_all_the_stored_forms() {
        assert_eq!(read("SELECT 1793767111000").await.millis(), 1_793_767_111_000);
        assert_eq!(read("SELECT '2026-11-04 07:38:31'").await.0.to_rfc3339(), "2026-11-04T07:38:31+00:00");
        assert_eq!(read("SELECT '2026-11-04T07:38:31.500Z'").await.0.timestamp_subsec_millis(), 500);
    }

    #[tokio::test]
    async fn it_writes_milliseconds() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let stored: (i64, String) = sqlx::query_as("SELECT ?1, typeof(?1)")
            .bind(Timestamp::from_millis(1_793_767_111_000).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stored, (1_793_767_111_000, "integer".to_owned()));
    }
}
