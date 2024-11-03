//! <https://github.com/kkawakam/rustyline/blob/master/src/sqlite_history.rs>
//! History impl. based on SQLite
use crate::db_path;
use anyhow::Result;
use rs_fsrs::Card;
use rs_fsrs::Parameters;
use rs_fsrs::FSRS;
use sqlx::migrate::MigrateDatabase;
use sqlx::sqlite::SqlitePool;
use sqlx::Row;
use sqlx::Sqlite;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;

/// 只在 非交互式的 情况下使用
pub async fn add_history(word: &str) -> Result<()> {
    let mut d = crate::fsrs::sqlite_history::SQLiteHistory::default().await;
    d.add(word).await?;
    Ok(())
}

/// History stored in an SQLite database.
#[derive(Clone)]
pub struct SQLiteHistory {
    max_len: usize,
    ignore_space: bool,
    ignore_dups: bool,
    path: PathBuf,
    pub conn: SqlitePool, /* we need to keep a connection opened at least for in memory
                           * database and also for cached statement(s) */
    session_id: i32, // 0 means no new entry added
    /// used in review
    /// search next word to review from `row_id`
    pub row_id: i32,
    pub fsrs: FSRS,
}

/*
https://sqlite.org/autoinc.html
If no ROWID is specified on the insert, or if the specified ROWID has a value of NULL, then an appropriate ROWID is created automatically.
The usual algorithm is to give the newly created row a ROWID that is one larger than the largest ROWID in the table prior to the insert.
If the table is initially empty, then a ROWID of 1 is used.
If the largest ROWID is equal to the largest possible integer (9223372036854775807) then the database engine starts picking positive candidate ROWIDs
at random until it finds one that is not previously used.
https://sqlite.org/lang_vacuum.html
The VACUUM command may change the ROWIDs of entries in any tables that do not have an explicit INTEGER PRIMARY KEY.
 */

impl SQLiteHistory {
    pub async fn default() -> Self {
        Self::new(db_path()).await.unwrap()
    }

    async fn new(path: PathBuf) -> Result<Self> {
        if !Sqlite::database_exists(path.to_str().unwrap()).await? {
            Sqlite::create_database(path.to_str().unwrap()).await?;
        }
        let conn = conn(&path).await?;
        let mut sh = Self {
            max_len: usize::MAX,
            ignore_space: true,
            // not strictly consecutive...
            ignore_dups: true,
            path,
            conn,
            session_id: 0,
            row_id: -1,
            fsrs: FSRS::new(Parameters::default()),
        };
        sh.check_schema().await?;
        Ok(sh)
    }

    async fn check_schema(&mut self) -> Result<()> {
        let user_version = &sqlx::query("pragma user_version;")
            .fetch_all(&self.conn)
            .await?[0];
        let user_version: i32 = user_version.get(0);

        if user_version <= 0 {
            sqlx::raw_sql(
                "
BEGIN EXCLUSIVE;
PRAGMA auto_vacuum = INCREMENTAL;
CREATE TABLE session (
    id INTEGER PRIMARY KEY NOT NULL,
    timestamp REAL NOT NULL DEFAULT (julianday('now'))
) STRICT; -- user, host, pid
CREATE TABLE fsrs (
    --entry TEXT NOT NULL,
    word TEXT PRIMARY KEY,
    due TEXT NOT NULL,
    stability REAL NOT NULL,
    difficulty REAL NOT NULL,
    elapsed_days INTEGER NOT NULL,
    scheduled_days INTEGER NOT NULL,
    reps INTEGER NOT NULL,
    lapses INTEGER NOT NULL,
    state TEXT NOT NULL,
    last_review TEXT NOT NULL,
    session_id INTEGER NOT NULL,
    -- card TEXT NOT NULL,
    -- timestamp REAL NOT NULL DEFAULT (julianday('now')),
    FOREIGN KEY (session_id) REFERENCES session(id) ON DELETE CASCADE
) STRICT;
CREATE VIRTUAL TABLE fts USING fts4(content=fsrs, word);
CREATE TRIGGER history_bu BEFORE UPDATE ON fsrs BEGIN
    DELETE FROM fts WHERE docid=old.rowid;
END;
CREATE TRIGGER history_bd BEFORE DELETE ON fsrs BEGIN
    DELETE FROM fts WHERE docid=old.rowid;
END;
CREATE TRIGGER history_au AFTER UPDATE ON fsrs BEGIN
    INSERT INTO fts (docid, word) VALUES (new.rowid, new.word);
END;
CREATE TRIGGER history_ai AFTER INSERT ON fsrs BEGIN
    INSERT INTO fts (docid, word) VALUES(new.rowid, new.word);
END;
PRAGMA user_version = 1;
COMMIT;
                 ",
            )
            .execute(&self.conn)
            .await?;
        }
        sqlx::query("pragma foreign_keys = 1;")
            .execute(&self.conn)
            .await?;
        if self.ignore_dups || user_version > 0 {
            self.set_ignore_dups().await?;
        }
        Ok(())
    }

    async fn set_ignore_dups(&mut self) -> Result<()> {
        if self.ignore_dups {
            // TODO Validate: ignore dups only in the same session_id ?
            sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS ignore_dups ON fsrs(word, session_id);")
                .execute(&self.conn)
                .await?;
            Ok(())
        } else {
            sqlx::query("DROP INDEX IF EXISTS ignore_dups;")
                .execute(&self.conn)
                .await?;
            Ok(())
        }
    }

    async fn create_session(&mut self) -> Result<()> {
        if self.session_id == 0 {
            self.check_schema().await?;
            self.session_id = sqlx::query("INSERT INTO session (id) VALUES (NULL) RETURNING id;")
                .fetch_one(&self.conn)
                .await?
                .get::<i32, _>(0);
        }
        Ok(())
    }

    fn ignore(&self, line: &str) -> bool {
        if self.max_len == 0 {
            return true;
        }
        if line.is_empty()
            || (self.ignore_space && line.chars().next().map_or(true, char::is_whitespace))
        {
            return true;
        }
        // ignore_dups => SQLITE_CONSTRAINT_UNIQUE
        false
    }

    async fn add_entry(&mut self, word: &str, card: Card) -> Result<bool> {
        // ignore SQLITE_CONSTRAINT_UNIQUE

        let done = sqlx::query("INSERT OR REPLACE INTO fsrs (session_id, word, due, stability, difficulty, elapsed_days, scheduled_days, reps, lapses, state, last_review) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING rowid;")
        .bind(self.session_id)
        .bind(word)
        .bind(serde_json::to_string(&card.due)?)
        .bind(card.stability)
        .bind(card.difficulty)
        .bind(card.elapsed_days)
        .bind(card.scheduled_days)
        .bind(card.reps)
        .bind(card.lapses)
        .bind(serde_json::to_string(&card.state)?)
        .bind(serde_json::to_string(&card.last_review)?)
        .execute(&self.conn).await?;

        Ok(true)
    }

    pub async fn delete(&self, term: &str) -> Result<usize> {
        let done = sqlx::query("DELETE FROM fsrs WHERE word=$1;")
            .bind(term)
            .execute(&self.conn)
            .await?;
        Ok(done.rows_affected().try_into().unwrap())
    }

    pub async fn add(&mut self, line: &str) -> Result<bool> {
        if self.ignore(line) {
            return Ok(false);
        }
        // Do not create a session until the first entry is added.
        self.create_session().await?;
        self.add_entry(line, Card::new()).await
    }
}

async fn conn(path: &Path) -> sqlx::Result<SqlitePool> {
    SqlitePool::connect(path.to_str().unwrap()).await
}
