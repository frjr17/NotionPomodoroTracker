pub mod session_repo;
pub mod settings_repo;
pub mod sync_log_repo;
pub mod task_repo;

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Corrupt(String),
}

pub type StoreResult<T> = Result<T, StoreError>;

/// Default DB location: ~/.local/share/notion-pomodoro-tracker/app.db
pub fn default_db_path() -> PathBuf {
    let data_dir = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
        });
    data_dir.join("notion-pomodoro-tracker").join("app.db")
}

/// Open (creating directories as needed) and migrate the database.
pub fn open(path: &Path) -> StoreResult<Connection> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let conn = Connection::open(path)?;
    init(&conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> StoreResult<Connection> {
    let conn = Connection::open_in_memory()?;
    init(&conn)?;
    Ok(conn)
}

fn init(conn: &Connection) -> StoreResult<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(
            "BEGIN;
            CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                notion_page_id TEXT UNIQUE,
                title TEXT NOT NULL,
                status TEXT NOT NULL,
                due_date TEXT,
                priority TEXT,
                pomodoro_count INTEGER NOT NULL DEFAULT 0,
                tracked_minutes INTEGER NOT NULL DEFAULT 0,
                done INTEGER NOT NULL DEFAULT 0,
                dirty INTEGER NOT NULL DEFAULT 0,
                conflict_remote_json TEXT,
                notion_last_edited TEXT,
                last_synced_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS time_sessions (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                started_at TEXT NOT NULL,
                ended_at TEXT NOT NULL,
                minutes INTEGER NOT NULL,
                kind TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_sessions_task ON time_sessions(task_id);
            CREATE TABLE IF NOT EXISTS sync_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                at TEXT NOT NULL,
                level TEXT NOT NULL,
                message TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            PRAGMA user_version = 1;
            COMMIT;",
        )?;
    }
    if version < 2 {
        // Task description = markdown mirror of the Notion page body;
        // desc_dirty flags a local body edit awaiting push.
        conn.execute_batch(
            "BEGIN;
            ALTER TABLE tasks ADD COLUMN description TEXT;
            ALTER TABLE tasks ADD COLUMN desc_dirty INTEGER NOT NULL DEFAULT 0;
            PRAGMA user_version = 2;
            COMMIT;",
        )?;
    }
    Ok(())
}
