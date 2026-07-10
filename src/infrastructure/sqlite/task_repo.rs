//! Task persistence. Free functions over `&Connection` so both the GTK main
//! thread and the background sync thread (with its own connection) can use
//! them without shared-state plumbing.

use super::{StoreError, StoreResult};
use crate::domain::task::Task;
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{Connection, Row, params};

#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    pub status: Option<String>,
    pub priority: Option<String>,
    pub due_on_or_before: Option<NaiveDate>,
    pub search: Option<String>,
    pub include_done: bool,
}

const COLS: &str = "id, notion_page_id, title, status, due_date, priority, pomodoro_count, \
    tracked_minutes, done, dirty, conflict_remote_json, notion_last_edited, last_synced_at, \
    created_at, updated_at, description, desc_dirty";

fn from_row(row: &Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        notion_page_id: row.get(1)?,
        title: row.get(2)?,
        status: row.get(3)?,
        due_date: row
            .get::<_, Option<String>>(4)?
            .and_then(|s| s.parse().ok()),
        priority: row.get(5)?,
        pomodoro_count: row.get(6)?,
        tracked_minutes: row.get(7)?,
        done: row.get(8)?,
        dirty: row.get(9)?,
        conflict_remote_json: row.get(10)?,
        notion_last_edited: parse_ts(row.get::<_, Option<String>>(11)?),
        last_synced_at: parse_ts(row.get::<_, Option<String>>(12)?),
        created_at: parse_ts(row.get::<_, Option<String>>(13)?).unwrap_or_default(),
        updated_at: parse_ts(row.get::<_, Option<String>>(14)?).unwrap_or_default(),
        description: row.get(15)?,
        desc_dirty: row.get(16)?,
    })
}

fn parse_ts(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

fn ts(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

pub fn upsert(conn: &Connection, task: &Task) -> StoreResult<()> {
    conn.execute(
        "INSERT INTO tasks (id, notion_page_id, title, status, due_date, priority,
            pomodoro_count, tracked_minutes, done, dirty, conflict_remote_json,
            notion_last_edited, last_synced_at, created_at, updated_at, description,
            desc_dirty)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)
         ON CONFLICT(id) DO UPDATE SET
            notion_page_id=?2, title=?3, status=?4, due_date=?5, priority=?6,
            pomodoro_count=?7, tracked_minutes=?8, done=?9, dirty=?10,
            conflict_remote_json=?11, notion_last_edited=?12, last_synced_at=?13,
            created_at=?14, updated_at=?15, description=?16, desc_dirty=?17",
        params![
            task.id,
            task.notion_page_id,
            task.title,
            task.status,
            task.due_date.map(|d| d.to_string()),
            task.priority,
            task.pomodoro_count,
            task.tracked_minutes,
            task.done,
            task.dirty,
            task.conflict_remote_json,
            task.notion_last_edited.as_ref().map(ts),
            task.last_synced_at.as_ref().map(ts),
            ts(&task.created_at),
            ts(&task.updated_at),
            task.description,
            task.desc_dirty,
        ],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> StoreResult<Option<Task>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM tasks WHERE id = ?1"))?;
    let mut rows = stmt.query_map([id], from_row)?;
    Ok(rows.next().transpose()?)
}

pub fn get_by_page_id(conn: &Connection, page_id: &str) -> StoreResult<Option<Task>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM tasks WHERE notion_page_id = ?1"
    ))?;
    let mut rows = stmt.query_map([page_id], from_row)?;
    Ok(rows.next().transpose()?)
}

pub fn list(conn: &Connection, filter: &TaskFilter) -> StoreResult<Vec<Task>> {
    let mut sql = format!("SELECT {COLS} FROM tasks WHERE 1=1");
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if !filter.include_done {
        sql.push_str(" AND done = 0");
    }
    if let Some(status) = &filter.status {
        sql.push_str(" AND status = ?");
        args.push(Box::new(status.clone()));
    }
    if let Some(priority) = &filter.priority {
        sql.push_str(" AND priority = ?");
        args.push(Box::new(priority.clone()));
    }
    if let Some(due) = &filter.due_on_or_before {
        sql.push_str(" AND due_date IS NOT NULL AND due_date <= ?");
        args.push(Box::new(due.to_string()));
    }
    if let Some(search) = &filter.search
        && !search.trim().is_empty()
    {
        sql.push_str(" AND title LIKE ? ESCAPE '\\'");
        let escaped = search
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        args.push(Box::new(format!("%{escaped}%")));
    }
    sql.push_str(" ORDER BY done ASC, due_date IS NULL, due_date ASC, created_at ASC");
    let mut stmt = conn.prepare(&sql)?;
    let params = rusqlite::params_from_iter(args.iter().map(|a| a.as_ref()));
    let rows = stmt.query_map(params, from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn list_dirty(conn: &Connection) -> StoreResult<Vec<Task>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM tasks WHERE dirty = 1 AND notion_page_id IS NOT NULL"
    ))?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn any_dirty(conn: &Connection) -> StoreResult<bool> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM tasks WHERE dirty = 1", [], |r| {
        r.get(0)
    })?;
    Ok(n > 0)
}

pub fn list_conflicted(conn: &Connection) -> StoreResult<Vec<Task>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM tasks WHERE conflict_remote_json IS NOT NULL"
    ))?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn distinct_statuses(conn: &Connection) -> StoreResult<Vec<String>> {
    let mut stmt = conn.prepare("SELECT DISTINCT status FROM tasks ORDER BY status")?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn distinct_priorities(conn: &Connection) -> StoreResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT priority FROM tasks WHERE priority IS NOT NULL ORDER BY priority",
    )?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn delete(conn: &Connection, id: &str) -> StoreResult<()> {
    conn.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
    Ok(())
}

/// Atomically add a completed pomodoro / focused minutes to a task.
pub fn add_progress(
    conn: &Connection,
    id: &str,
    pomodoros: u32,
    minutes: i64,
    now: DateTime<Utc>,
) -> StoreResult<()> {
    let n = conn.execute(
        "UPDATE tasks SET pomodoro_count = pomodoro_count + ?2,
            tracked_minutes = tracked_minutes + ?3, dirty = 1, updated_at = ?4
         WHERE id = ?1",
        params![id, pomodoros, minutes, ts(&now)],
    )?;
    if n == 0 {
        return Err(StoreError::Corrupt(format!("task {id} not found")));
    }
    Ok(())
}
