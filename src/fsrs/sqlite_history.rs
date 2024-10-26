//! https://github.com/kkawakam/rustyline/blob/master/src/sqlite_history.rs
//! History impl. based on SQLite
use crate::fsrs::MemoryStateWrapper;
use anyhow::Context;
use anyhow::Result;
use dirs::data_dir;
use fsrs::{DEFAULT_PARAMETERS, FSRS};
use rusqlite::{Connection, DatabaseName, OptionalExtension};
use std::borrow::Cow;
use std::cell::Cell;
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
pub fn get_db() -> Result<Connection> {
    let path = get_db_path()?;
    let conn = Connection::open(path)?;
    Ok(conn)
}

/// 只在 非交互式的 情况下使用
pub fn add_history(word: &str) -> Result<()> {
    let mut d = crate::fsrs::sqlite_history::SQLiteHistory::default();
    d.add(word)?;
    Ok(())
}

/// History stored in an SQLite database.
#[derive(Clone)]
pub struct SQLiteHistory {
    max_len: usize,
    ignore_space: bool,
    ignore_dups: bool,
    path: Option<PathBuf>, // None => memory
    pub conn: Connection,  /* we need to keep a connection opened at least for in memory
                            * database and also for cached statement(s) */
    session_id: usize,         // 0 means no new entry added
    row_id: Arc<Mutex<usize>>, // max entry id
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

impl Default for SQLiteHistory {
    fn default() -> Self {
        Self::new(Some(get_db_path().unwrap())).unwrap()
    }
}

impl SQLiteHistory {
    fn new(path: Option<PathBuf>) -> Result<Self> {
        let conn = conn(path.as_ref())?;
        let mut sh = Self {
            max_len: usize::MAX,
            ignore_space: true,
            // not strictly consecutive...
            ignore_dups: true,
            path,
            conn,
            session_id: 0,
            row_id: Arc::new(Mutex::new(0)),
            fsrs: FSRS::new(Some(&DEFAULT_PARAMETERS)).unwrap(),
        };
        sh.check_schema()?;
        Ok(sh)
    }

    fn is_mem_or_temp(&self) -> bool {
        match self.path {
            None => true,
            Some(ref p) => is_mem_or_temp(p),
        }
    }

    fn reset(&mut self, path: &Path) -> Result<Connection> {
        self.path = normalize(path);
        self.session_id = 0;
        *self.row_id.lock().unwrap() = 0;
        Ok(std::mem::replace(&mut self.conn, conn(self.path.as_ref())?))
    }

    fn update_row_id(&mut self) -> Result<()> {
        let x = self
            .conn
            .query_row("SELECT ifnull(max(rowid), 0) FROM fsrs;", [], |r| r.get(0))?;
        *self.row_id.lock().unwrap() = x;
        Ok(())
    }

    fn check_schema(&mut self) -> Result<()> {
        let user_version: i32 = self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        if user_version <= 0 {
            self.conn.execute_batch(
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
    difficulty REAL NOT NULL,
    stability REAL NOT NULL,
    interval REAL NOT NULL,
    last_reviewed TEXT NOT NULL,
    session_id INTEGER NOT NULL,
    --timestamp REAL NOT NULL DEFAULT (julianday('now')),
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
            )?
        }
        self.conn.pragma_update(None, "foreign_keys", 1)?;
        if self.ignore_dups || user_version > 0 {
            self.set_ignore_dups()?;
        }
        if *self.row_id.lock().unwrap() == 0 && user_version > 0 {
            self.update_row_id()?;
        }
        Ok(())
    }

    fn set_ignore_dups(&mut self) -> Result<()> {
        if self.ignore_dups {
            // TODO Validate: ignore dups only in the same session_id ?
            self.conn.execute_batch(
                "CREATE UNIQUE INDEX IF NOT EXISTS ignore_dups ON fsrs(word, session_id);",
            )?;
        } else {
            self.conn
                .execute_batch("DROP INDEX IF EXISTS ignore_dups;")?;
        }
        Ok(())
    }

    fn create_session(&mut self) -> Result<()> {
        if self.session_id == 0 {
            self.check_schema()?;
            self.session_id = self.conn.query_row(
                "INSERT INTO session (id) VALUES (NULL) RETURNING id;",
                [],
                |r| r.get(0),
            )?;
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

    fn add_entry(&mut self, line: &str, sm: MemoryStateWrapper) -> Result<bool> {
        // ignore SQLITE_CONSTRAINT_UNIQUE
        let mut stmt = self.conn.prepare_cached(
"INSERT OR REPLACE INTO fsrs (session_id, word, stability, difficulty, interval, last_reviewed) VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING rowid;",
        )?;
        if let Some(row_id) = stmt
            .query_row(
                (
                    self.session_id,
                    line,
                    sm.stability,
                    sm.difficulty,
                    sm.interval,
                    sm.last_reviewed.to_string(),
                ),
                |r| r.get(0),
            )
            .optional()?
        {
            *self.row_id.lock().unwrap() = row_id;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn delete(&self, term: &str) -> std::result::Result<usize, rusqlite::Error> {
        self.conn.execute("DELETE FROM fsrs WHERE word=?1;", [term])
    }

    pub fn add(&mut self, line: &str) -> Result<bool> {
        if self.ignore(line) {
            return Ok(false);
        }
        // Do not create a session until the first entry is added.
        self.create_session()?;
        self.add_entry(line, Default::default())
    }
}

fn conn(path: Option<&PathBuf>) -> rusqlite::Result<Connection> {
    if let Some(ref path) = path {
        Connection::open(path)
    } else {
        Connection::open_in_memory()
    }
}

const MEMORY: &str = ":memory:";

fn normalize(path: &Path) -> Option<PathBuf> {
    if path.as_os_str() == MEMORY {
        None
    } else {
        Some(path.to_path_buf())
    }
}
fn is_mem_or_temp(path: &Path) -> bool {
    let os_str = path.as_os_str();
    os_str.is_empty() || os_str == MEMORY
}
fn is_same(old: Option<&PathBuf>, new: &Path) -> bool {
    if let Some(old) = old {
        old == new // TODO canonicalize ?
    } else {
        new.as_os_str() == MEMORY
    }
}
fn offset(s: String) -> usize {
    s.split(' ')
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}
