use crate::domain::sync_state::{ConflictResolution, RemoteTask};
use crate::domain::task::{DONE_STATUS, Task};
use crate::domain::time_session::{SessionKind, TimeSession};
use crate::infrastructure::sqlite::{StoreError, StoreResult, session_repo, task_repo};
use chrono::{DateTime, Utc};
use rusqlite::Connection;

pub use crate::infrastructure::sqlite::task_repo::TaskFilter;

pub fn list(conn: &Connection, filter: &TaskFilter) -> StoreResult<Vec<Task>> {
    task_repo::list(conn, filter)
}

pub fn get(conn: &Connection, id: &str) -> StoreResult<Option<Task>> {
    task_repo::get(conn, id)
}

pub fn create(conn: &Connection, title: &str, now: DateTime<Utc>) -> StoreResult<Task> {
    let task = Task::new_local(title.trim(), now);
    task_repo::upsert(conn, &task)?;
    Ok(task)
}

pub fn rename(conn: &Connection, id: &str, title: &str, now: DateTime<Utc>) -> StoreResult<()> {
    let mut task = require(conn, id)?;
    task.title = title.trim().to_string();
    task.touch(now);
    task_repo::upsert(conn, &task)
}

pub fn set_done(conn: &Connection, id: &str, done: bool, now: DateTime<Utc>) -> StoreResult<()> {
    let mut task = require(conn, id)?;
    task.done = done;
    if done {
        task.status = DONE_STATUS.to_string();
    }
    task.touch(now);
    task_repo::upsert(conn, &task)
}

pub fn delete(conn: &Connection, id: &str) -> StoreResult<()> {
    task_repo::delete(conn, id)
}

pub fn sessions(conn: &Connection, task_id: &str) -> StoreResult<Vec<TimeSession>> {
    session_repo::list_for_task(conn, task_id)
}

/// Manually add (or correct) tracked time. Negative minutes subtract, but
/// never below zero total.
pub fn add_manual_time(
    conn: &Connection,
    task_id: &str,
    minutes: i64,
    now: DateTime<Utc>,
) -> StoreResult<()> {
    let mut task = require(conn, task_id)?;
    let session = TimeSession::new(task_id, now, now, minutes, SessionKind::Manual);
    session_repo::insert(conn, &session)?;
    task.tracked_minutes = (task.tracked_minutes as i64 + minutes).max(0) as u32;
    task.touch(now);
    task_repo::upsert(conn, &task)
}

/// Resolve a conflict the user decided on.
pub fn resolve_conflict(
    conn: &Connection,
    task_id: &str,
    resolution: ConflictResolution,
    now: DateTime<Utc>,
) -> StoreResult<()> {
    let mut task = require(conn, task_id)?;
    let Some(json) = task.conflict_remote_json.take() else {
        return Ok(());
    };
    match resolution {
        ConflictResolution::KeepLocal => {
            // Keep local fields and mark the remote edit as seen, so the next
            // sync pushes local instead of re-detecting the same conflict.
            if let Ok(remote) = serde_json::from_str::<RemoteTask>(&json) {
                task.notion_last_edited = Some(remote.last_edited);
            }
            task.dirty = true;
        }
        ConflictResolution::KeepNotion => {
            let remote: RemoteTask = serde_json::from_str(&json)
                .map_err(|e| StoreError::Corrupt(format!("bad conflict snapshot: {e}")))?;
            apply_remote(&mut task, &remote);
            task.dirty = false;
            task.last_synced_at = Some(now);
        }
    }
    task.updated_at = now;
    task_repo::upsert(conn, &task)
}

/// Copy remote fields onto a local task (shared by pull and conflict
/// resolution). Progress numbers are only taken when the remote actually has
/// that property mapped.
pub fn apply_remote(task: &mut Task, remote: &RemoteTask) {
    task.title = remote.title.clone();
    if let Some(status) = &remote.status {
        task.status = status.clone();
        task.done = status == DONE_STATUS;
    }
    task.due_date = remote.due_date;
    task.priority = remote.priority.clone();
    if let Some(p) = remote.pomodoro_count {
        task.pomodoro_count = p;
    }
    if let Some(m) = remote.tracked_minutes {
        task.tracked_minutes = m;
    }
    task.notion_last_edited = Some(remote.last_edited);
}

fn require(conn: &Connection, id: &str) -> StoreResult<Task> {
    task_repo::get(conn, id)?.ok_or_else(|| StoreError::Corrupt(format!("task {id} not found")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite::open_in_memory;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn create_list_edit_done_round_trip() {
        let conn = open_in_memory().unwrap();
        let task = create(&conn, "  Write report ", now()).unwrap();
        assert_eq!(task.title, "Write report");
        assert!(!task.dirty);

        rename(&conn, &task.id, "Write the report", now()).unwrap();
        set_done(&conn, &task.id, true, now()).unwrap();

        let all = list(
            &conn,
            &TaskFilter {
                include_done: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].title, "Write the report");
        assert!(all[0].done);
        assert!(all[0].dirty, "local edits must set the dirty flag");

        let open = list(&conn, &TaskFilter::default()).unwrap();
        assert!(open.is_empty(), "done tasks hidden by default");
    }

    #[test]
    fn search_filter_matches_title() {
        let conn = open_in_memory().unwrap();
        create(&conn, "Fix login bug", now()).unwrap();
        create(&conn, "Write docs", now()).unwrap();
        let found = list(
            &conn,
            &TaskFilter {
                search: Some("login".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Fix login bug");
    }

    #[test]
    fn manual_time_adjusts_but_never_negative() {
        let conn = open_in_memory().unwrap();
        let task = create(&conn, "T", now()).unwrap();
        add_manual_time(&conn, &task.id, 30, now()).unwrap();
        add_manual_time(&conn, &task.id, -100, now()).unwrap();
        let task = get(&conn, &task.id).unwrap().unwrap();
        assert_eq!(task.tracked_minutes, 0);
        assert_eq!(sessions(&conn, &task.id).unwrap().len(), 2);
    }
}
