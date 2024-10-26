//! https://github.com/kkawakam/rustyline/blob/master/src/sqlite_history.rs
//! History impl. based on SQLite
use anyhow::Context;
use anyhow::Result;
use dirs::data_dir;
use fsrs::Card;
use fsrs::Parameters;
use fsrs::FSRS;
use sqlx::migrate::MigrateDatabase;
use sqlx::sqlite::SqlitePool;
use sqlx::Row;
use sqlx::Sqlite;
use std::fs::create_dir;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::usize;

pub fn get_db_path() -> Result<PathBuf> {
    let mut path = data_dir().with_context(|| "Couldn't find data directory")?;
    path.push("mdict-cli-rs");
    if !path.exists() {
        create_dir(&path).with_context(|| format!("Failed to create directory {:?}", path))?;
    }
    path.push("history.db");
    Ok(path)
}

/// Check and generate cache directory path.
pub async fn get_db() -> Result<SqlitePool> {
    let path = get_db_path()?;
    let conn = SqlitePool::connect(path.to_str().unwrap()).await?;
    Ok(conn)
}

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
    path: PathBuf, // None => memory
    pub conn: SqlitePool, /* we need to keep a connection opened at least for in memory
                    * database and also for cached statement(s) */
    session_id: i32,         // 0 means no new entry added
    row_id: Arc<Mutex<i32>>, // max entry id
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
        Self::new(get_db_path().unwrap()).await.unwrap()
    }
}

impl SQLiteHistory {
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
            row_id: Arc::new(Mutex::new(0)),
            fsrs: FSRS::new(Parameters::default()),
        };
        sh.check_schema().await?;
        Ok(sh)
    }

    fn is_mem_or_temp(&self) -> bool {
        is_mem_or_temp(&self.path)
    }

    async fn reset(&mut self, path: &Path) -> Result<SqlitePool> {
        self.path = path.to_path_buf();
        self.session_id = 0;
        *self.row_id.lock().unwrap() = 0;
        Ok(std::mem::replace(&mut self.conn, conn(&self.path).await?))
    }

    async fn update_row_id(&mut self) -> Result<()> {
        let x = sqlx::query("SELECT ifnull(max(rowid), 0) FROM fsrs;")
            .fetch_one(&self.conn)
            .await?
            .get::<i32, _>(0);
        // let x = self
        //     .conn
        //     .query_row(, [], |r| r.get(0))?;
        *self.row_id.lock().unwrap() = x;
        Ok(())
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
    card TEXT NOT NULL,
    session_id INTEGER NOT NULL,
    -- difficulty REAL NOT NULL,
    -- stability REAL NOT NULL,
    -- interval REAL NOT NULL,
    -- last_reviewed TEXT NOT NULL,
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
        if *self.row_id.lock().unwrap() == 0 && user_version > 0 {
            self.update_row_id().await?;
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

        let card_str: String = serde_json::to_string(&card)?;
        let done = sqlx::query("INSERT OR REPLACE INTO fsrs (session_id, word, card) VALUES ($1, $2, $3) RETURNING rowid;")
        .bind(self.session_id)
        .bind(word)
        .bind(card_str)
        .execute(&self.conn).await?;

        let row_id = done.rows_affected();
        *self.row_id.lock().unwrap() = row_id.try_into().unwrap();
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

const MEMORY: &str = ":memory:";

fn is_mem_or_temp(path: &Path) -> bool {
    let os_str = path.as_os_str();
    os_str.is_empty() || os_str == MEMORY
}

fn is_same(old: &PathBuf, new: &Path) -> bool {
    old == new
}

fn offset(s: String) -> usize {
    s.split(' ')
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}
