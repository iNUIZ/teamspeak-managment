//! Creating the schema on a new database and checking it on an existing one.
//! `migrations/baseline.sql` is what the old program's Prisma schema generates.

use std::collections::{BTreeMap, BTreeSet};

use sqlx::sqlite::SqlitePool;
use sqlx::{Connection, Executor};

const BASELINE: &str = include_str!("../migrations/baseline.sql");

/// Every difference found, so one run shows the whole problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaError(pub Vec<String>);

impl std::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.join("; "))
    }
}

impl std::error::Error for SchemaError {}

/// Column name, type, required, primary key. Order is ignored: columns the old program
/// added later sit at the end of its tables.
type Columns = BTreeMap<String, (String, bool, bool)>;

#[derive(Debug, Default, PartialEq, Eq)]
struct Shape {
    tables: BTreeMap<String, Columns>,
    unique_indexes: BTreeMap<String, BTreeSet<Vec<String>>>,
}

pub(crate) async fn is_empty(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'")
        .fetch_one(pool)
        .await?;
    Ok(count == 0)
}

pub(crate) async fn create(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut connection = pool.acquire().await?;
    connection
        .transaction(|tx| {
            Box::pin(async move {
                tx.execute(BASELINE).await?;
                Ok(())
            })
        })
        .await
}

async fn shape_of(pool: &SqlitePool) -> Result<Shape, sqlx::Error> {
    let mut shape = Shape::default();
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")
            .fetch_all(pool)
            .await?;
    for table in tables {
        let columns: Vec<(String, String, bool, i64)> =
            sqlx::query_as(r#"SELECT name, type, "notnull", pk FROM pragma_table_info(?)"#)
                .bind(&table)
                .fetch_all(pool)
                .await?;
        let columns =
            columns.into_iter().map(|(name, kind, required, pk)| (name, (kind.to_uppercase(), required, pk > 0))).collect();
        shape.tables.insert(table.clone(), columns);

        let indexes: Vec<String> =
            sqlx::query_scalar(r#"SELECT name FROM pragma_index_list(?) WHERE "unique" = 1 AND origin = 'c'"#)
                .bind(&table)
                .fetch_all(pool)
                .await?;
        let mut unique = BTreeSet::new();
        for index in indexes {
            let names: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_index_info(?) ORDER BY seqno")
                .bind(&index)
                .fetch_all(pool)
                .await?;
            unique.insert(names);
        }
        shape.unique_indexes.insert(table, unique);
    }
    Ok(shape)
}

fn compare(expected: &Shape, actual: &Shape) -> Vec<String> {
    let mut problems = Vec::new();
    for (table, columns) in &expected.tables {
        let Some(found) = actual.tables.get(table) else {
            problems.push(format!("table {table} is missing"));
            continue;
        };
        for (name, want) in columns {
            match found.get(name) {
                None => problems.push(format!("column {table}.{name} is missing")),
                Some(have) if have != want => problems.push(format!("column {table}.{name} is {have:?}, expected {want:?}")),
                Some(_) => {}
            }
        }
        for name in found.keys().filter(|name| !columns.contains_key(*name)) {
            problems.push(format!("column {table}.{name} is unknown"));
        }
        if expected.unique_indexes.get(table) != actual.unique_indexes.get(table) {
            problems.push(format!("unique indexes of {table} differ"));
        }
    }
    for table in actual.tables.keys().filter(|table| !expected.tables.contains_key(*table)) {
        problems.push(format!("table {table} is unknown"));
    }
    problems
}

/// Compare a database with the baseline, built in a private memory database.
pub(crate) async fn check(pool: &SqlitePool) -> Result<(), SchemaError> {
    let failed = |error: sqlx::Error| SchemaError(vec![format!("could not read schema: {error}")]);
    let reference = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .map_err(failed)?;
    create(&reference).await.map_err(failed)?;
    let expected = shape_of(&reference).await.map_err(failed)?;
    reference.close().await;

    let problems = compare(&expected, &shape_of(pool).await.map_err(failed)?);
    if problems.is_empty() {
        Ok(())
    } else {
        Err(SchemaError(problems))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Db, Error};

    async fn reopen_after(sql: &'static str) -> Result<(), SchemaError> {
        let db = Db::open_in_memory().await.unwrap();
        sqlx::raw_sql(sql).execute(db.pool()).await.unwrap();
        check(db.pool()).await
    }

    #[tokio::test]
    async fn the_baseline_matches_itself() {
        let db = Db::open_in_memory().await.unwrap();
        check(db.pool()).await.unwrap();
    }

    #[tokio::test]
    async fn column_order_does_not_matter() {
        let db = Db::open_in_memory().await.unwrap();
        // Rebuild the way the old program's added column ended up: last in the table.
        sqlx::raw_sql(
            r#"ALTER TABLE "AppSetting" RENAME COLUMN "value" TO "tmp";
               ALTER TABLE "AppSetting" ADD COLUMN "value" TEXT NOT NULL DEFAULT '';
               ALTER TABLE "AppSetting" DROP COLUMN "tmp";"#,
        )
        .execute(db.pool())
        .await
        .unwrap();
        check(db.pool()).await.unwrap();
    }

    #[tokio::test]
    async fn every_kind_of_difference_is_reported() {
        let missing = reopen_after(r#"DROP TABLE "Widget""#).await.unwrap_err();
        assert_eq!(missing.0, ["table Widget is missing"]);

        let extra = reopen_after("CREATE TABLE _sqlx_migrations (id INTEGER)").await.unwrap_err();
        assert_eq!(extra.0, ["table _sqlx_migrations is unknown"]);

        let added = reopen_after(r#"ALTER TABLE "AppSetting" ADD COLUMN "other" TEXT"#).await.unwrap_err();
        assert_eq!(added.0, ["column AppSetting.other is unknown"]);

        let gone = reopen_after(r#"ALTER TABLE "RadioStation" DROP COLUMN "genre""#).await.unwrap_err();
        assert_eq!(gone.0, ["column RadioStation.genre is missing"]);

        let index = reopen_after(r#"DROP INDEX "User_username_key""#).await.unwrap_err();
        assert_eq!(index.0, ["unique indexes of User differ"]);
    }

    #[tokio::test]
    async fn opening_stops_on_a_different_schema_and_leaves_the_file_alone() {
        let dir = std::env::temp_dir().join(format!("teamhub-schema-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("other.db");
        let _ = std::fs::remove_file(&path);
        let url = format!("file:{}", path.display());
        let db = Db::open(&url).await.unwrap();
        sqlx::raw_sql(r#"DROP TABLE "Widget""#).execute(db.pool()).await.unwrap();
        db.close().await;

        assert!(matches!(Db::open(&url).await, Err(Error::Schema(_))));
        let again = Db::open(&url).await.err().unwrap();
        assert!(again.to_string().contains("Widget"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
