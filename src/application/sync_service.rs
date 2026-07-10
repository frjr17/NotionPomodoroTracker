//! Two-way sync between the local database and Notion, over the `NotionApi`
//! port so it is testable with a fake.

use crate::application::settings_service::{self, PropertyMappings, Settings};
use crate::application::task_service;
use crate::domain::sync_state::{RemoteTask, SyncReport};
use crate::domain::task::Task;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use thiserror::Error;

use crate::infrastructure::sqlite::{StoreError, sync_log_repo, task_repo};

#[derive(Debug, Error)]
pub enum NotionError {
    #[error("network error: {0}")]
    Network(String),
    #[error("Notion API error {status}: {message}")]
    Api { status: u16, message: String },
    #[error("unauthorized — check your integration token")]
    Unauthorized,
    #[error("database not found — check the ID and that it is shared with the integration")]
    NotFound,
}

/// Port to Notion. Production: `infrastructure::notion::NotionClient`;
/// tests: an in-memory fake.
pub trait NotionApi {
    fn fetch_tasks(
        &self,
        database_id: &str,
        mappings: &PropertyMappings,
    ) -> Result<Vec<RemoteTask>, NotionError>;

    /// Push local fields to a page; returns the page's new last_edited_time.
    fn push_task(
        &self,
        task: &Task,
        mappings: &PropertyMappings,
        now: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, NotionError>;

    /// Fetch the page body (block children) rendered as Markdown.
    fn fetch_page_markdown(&self, page_id: &str) -> Result<String, NotionError>;

    /// Replace the page body with blocks rendered from Markdown; returns the
    /// page's new last_edited_time.
    fn replace_page_body(
        &self,
        page_id: &str,
        markdown: &str,
    ) -> Result<DateTime<Utc>, NotionError>;
}

#[derive(Debug, Error)]
pub enum SyncError {
    #[error(transparent)]
    Notion(#[from] NotionError),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Run one full sync. Individual push failures are collected in the report;
/// a failed initial fetch aborts the whole run.
pub fn run_sync(
    conn: &Connection,
    api: &dyn NotionApi,
    settings: &Settings,
    now: DateTime<Utc>,
) -> Result<SyncReport, SyncError> {
    let mut report = SyncReport::default();
    let remotes = match api.fetch_tasks(&settings.notion_database_id, &settings.mappings) {
        Ok(r) => r,
        Err(e) => {
            let _ = sync_log_repo::error(conn, &format!("sync fetch failed: {e}"));
            return Err(e.into());
        }
    };

    for remote in &remotes {
        match task_repo::get_by_page_id(conn, &remote.page_id)? {
            None => {
                let mut task = Task::new_local(&remote.title, now);
                task.notion_page_id = Some(remote.page_id.clone());
                task_service::apply_remote(&mut task, remote);
                pull_body(conn, api, &mut task, &remote.page_id);
                task.last_synced_at = Some(now);
                task_repo::upsert(conn, &task)?;
                report.pulled_new += 1;
            }
            Some(mut local) => {
                let remote_changed = local
                    .notion_last_edited
                    .is_none_or(|seen| remote.last_edited > seen);
                if local.in_conflict() || (local.dirty && remote_changed) {
                    // Both sides changed: never overwrite silently. Store the
                    // latest remote snapshot and let the user decide.
                    local.conflict_remote_json = Some(serde_json::to_string(remote).unwrap());
                    local.updated_at = now;
                    task_repo::upsert(conn, &local)?;
                    report.conflicts += 1;
                } else if local.dirty {
                    push_one(conn, api, settings, &mut local, now, &mut report);
                } else if remote_changed {
                    task_service::apply_remote(&mut local, remote);
                    pull_body(conn, api, &mut local, &remote.page_id);
                    local.last_synced_at = Some(now);
                    local.updated_at = now;
                    task_repo::upsert(conn, &local)?;
                    report.pulled_updated += 1;
                }
            }
        }
    }

    // Pages deleted or archived in Notion: remove their clean local mirrors.
    // Tasks with unpushed local changes (or open conflicts) are kept and
    // logged — never discard the user's work silently.
    let remote_ids: std::collections::HashSet<&str> =
        remotes.iter().map(|r| r.page_id.as_str()).collect();
    let all = task_repo::list(
        conn,
        &task_repo::TaskFilter {
            include_done: true,
            ..Default::default()
        },
    )?;
    for task in all {
        let Some(page_id) = &task.notion_page_id else {
            continue;
        };
        if remote_ids.contains(page_id.as_str()) {
            continue;
        }
        if task.dirty || task.in_conflict() {
            let _ = sync_log_repo::info(
                conn,
                &format!(
                    "'{}' was deleted in Notion; kept locally because it has unsynced changes",
                    task.title
                ),
            );
        } else {
            task_repo::delete(conn, &task.id)?;
            report.deleted += 1;
        }
    }

    settings_service::set_last_sync_at(conn, &now.to_rfc3339())?;
    let _ = sync_log_repo::info(
        conn,
        &format!(
            "sync done: {} new, {} updated, {} pushed, {} removed, {} conflicts, {} errors",
            report.pulled_new,
            report.pulled_updated,
            report.pushed,
            report.deleted,
            report.conflicts,
            report.errors.len()
        ),
    );
    Ok(report)
}

/// Pull the page body into `task.description`. A body-fetch failure is logged,
/// not fatal — the properties still sync. Only called for new/changed pages.
fn pull_body(conn: &Connection, api: &dyn NotionApi, task: &mut Task, page_id: &str) {
    match api.fetch_page_markdown(page_id) {
        Ok(md) => {
            task.description = Some(md);
            task.desc_dirty = false;
        }
        Err(e) => {
            let _ = sync_log_repo::error(conn, &format!("fetch body '{}' failed: {e}", task.title));
        }
    }
}

fn push_one(
    conn: &Connection,
    api: &dyn NotionApi,
    settings: &Settings,
    local: &mut Task,
    now: DateTime<Utc>,
    report: &mut SyncReport,
) {
    match api.push_task(local, &settings.mappings, now) {
        Ok(mut new_last_edited) => {
            local.dirty = false;
            // The page body rides along on the same dirty push, but only when
            // the description itself changed (desc_dirty) — a pomodoro-only
            // push must never rewrite (or, for an un-fetched body, wipe) it.
            let mut body_ok = true;
            if local.desc_dirty {
                match (local.notion_page_id.clone(), local.description.clone()) {
                    (Some(page_id), Some(desc)) => match api.replace_page_body(&page_id, &desc) {
                        Ok(edited) => {
                            local.desc_dirty = false;
                            new_last_edited = new_last_edited.max(edited);
                        }
                        Err(e) => {
                            body_ok = false;
                            let msg = format!("push body '{}' failed: {e}", local.title);
                            let _ = sync_log_repo::error(conn, &msg);
                            report.errors.push(msg);
                        }
                    },
                    _ => local.desc_dirty = false,
                }
            }
            // If the body push failed, keep the task dirty so the next sync
            // retries (re-pushing properties is idempotent).
            local.dirty = !body_ok;
            local.notion_last_edited = Some(new_last_edited);
            local.last_synced_at = Some(now);
            local.updated_at = now;
            if let Err(e) = task_repo::upsert(conn, local) {
                report.errors.push(format!("saving '{}': {e}", local.title));
            } else if body_ok {
                report.pushed += 1;
            }
        }
        Err(e) => {
            let msg = format!("push '{}' failed: {e}", local.title);
            let _ = sync_log_repo::error(conn, &msg);
            report.errors.push(msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite::open_in_memory;
    use chrono::{Duration, TimeZone};
    use std::cell::RefCell;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 1, 8, 0, 0).unwrap()
    }

    struct FakeNotion {
        remotes: Vec<RemoteTask>,
        pushed: RefCell<Vec<String>>,
        body_pushed: RefCell<Vec<String>>,
        bodies: RefCell<std::collections::HashMap<String, String>>,
        fail_fetch: bool,
        fail_push: bool,
    }

    impl FakeNotion {
        fn with(remotes: Vec<RemoteTask>) -> Self {
            Self {
                remotes,
                pushed: RefCell::new(vec![]),
                body_pushed: RefCell::new(vec![]),
                bodies: RefCell::new(std::collections::HashMap::new()),
                fail_fetch: false,
                fail_push: false,
            }
        }
    }

    impl NotionApi for FakeNotion {
        fn fetch_tasks(
            &self,
            _db: &str,
            _m: &PropertyMappings,
        ) -> Result<Vec<RemoteTask>, NotionError> {
            if self.fail_fetch {
                return Err(NotionError::Network("offline".into()));
            }
            Ok(self.remotes.clone())
        }

        fn push_task(
            &self,
            task: &Task,
            _m: &PropertyMappings,
            now: DateTime<Utc>,
        ) -> Result<DateTime<Utc>, NotionError> {
            if self.fail_push {
                return Err(NotionError::Api {
                    status: 500,
                    message: "boom".into(),
                });
            }
            self.pushed
                .borrow_mut()
                .push(task.notion_page_id.clone().unwrap());
            Ok(now)
        }

        fn fetch_page_markdown(&self, page_id: &str) -> Result<String, NotionError> {
            Ok(self
                .bodies
                .borrow()
                .get(page_id)
                .cloned()
                .unwrap_or_default())
        }

        fn replace_page_body(
            &self,
            page_id: &str,
            markdown: &str,
        ) -> Result<DateTime<Utc>, NotionError> {
            self.body_pushed.borrow_mut().push(page_id.to_string());
            self.bodies
                .borrow_mut()
                .insert(page_id.to_string(), markdown.to_string());
            Ok(Utc::now())
        }
    }

    fn remote(page_id: &str, title: &str, edited: DateTime<Utc>) -> RemoteTask {
        RemoteTask {
            page_id: page_id.into(),
            title: title.into(),
            status: Some("To Do".into()),
            due_date: None,
            priority: None,
            pomodoro_count: Some(0),
            tracked_minutes: Some(0),
            last_edited: edited,
        }
    }

    #[test]
    fn pull_imports_new_tasks_without_duplicates() {
        let conn = open_in_memory().unwrap();
        let api = FakeNotion::with(vec![remote("p1", "A", t0()), remote("p2", "B", t0())]);
        let settings = Settings::default();

        let report = run_sync(&conn, &api, &settings, t0()).unwrap();
        assert_eq!(report.pulled_new, 2);
        // Second sync: nothing new, no duplicates.
        let report = run_sync(&conn, &api, &settings, t0() + Duration::minutes(1)).unwrap();
        assert_eq!(report.pulled_new, 0);
        let all = task_repo::list(&conn, &Default::default()).unwrap();
        assert_eq!(all.len(), 2);
        assert!(!all[0].dirty, "pulled tasks start clean");
    }

    #[test]
    fn dirty_local_task_is_pushed_and_cleared() {
        let conn = open_in_memory().unwrap();
        let api = FakeNotion::with(vec![remote("p1", "A", t0())]);
        let settings = Settings::default();
        run_sync(&conn, &api, &settings, t0()).unwrap();

        // Local edit → dirty.
        let task = &task_repo::list(&conn, &Default::default()).unwrap()[0];
        task_service::set_done(&conn, &task.id, true, t0() + Duration::hours(1)).unwrap();
        assert!(task_repo::any_dirty(&conn).unwrap());

        let report = run_sync(&conn, &api, &settings, t0() + Duration::hours(2)).unwrap();
        assert_eq!(report.pushed, 1);
        assert_eq!(*api.pushed.borrow(), vec!["p1".to_string()]);
        assert!(
            !task_repo::any_dirty(&conn).unwrap(),
            "dirty flag cleared after push"
        );
    }

    #[test]
    fn description_edit_pushes_body_and_clears_flags() {
        let conn = open_in_memory().unwrap();
        let api = FakeNotion::with(vec![remote("p1", "A", t0())]);
        let settings = Settings::default();
        run_sync(&conn, &api, &settings, t0()).unwrap();

        let task_id = task_repo::list(&conn, &Default::default()).unwrap()[0]
            .id
            .clone();
        task_service::edit_description(
            &conn,
            &task_id,
            "# Notes\n\n- item",
            t0() + Duration::hours(1),
        )
        .unwrap();

        let report = run_sync(&conn, &api, &settings, t0() + Duration::hours(2)).unwrap();
        assert_eq!(report.pushed, 1);
        assert_eq!(*api.body_pushed.borrow(), vec!["p1".to_string()]);
        let task = task_repo::get(&conn, &task_id).unwrap().unwrap();
        assert!(!task.dirty, "dirty cleared after successful push");
        assert!(!task.desc_dirty, "desc_dirty cleared after body push");
    }

    #[test]
    fn pomodoro_push_does_not_rewrite_body() {
        let conn = open_in_memory().unwrap();
        let api = FakeNotion::with(vec![remote("p1", "A", t0())]);
        let settings = Settings::default();
        run_sync(&conn, &api, &settings, t0()).unwrap();

        let task_id = task_repo::list(&conn, &Default::default()).unwrap()[0]
            .id
            .clone();
        // A pomodoro sets dirty but not desc_dirty → body must be left alone.
        task_repo::add_progress(&conn, &task_id, 1, 25, t0() + Duration::hours(1)).unwrap();

        run_sync(&conn, &api, &settings, t0() + Duration::hours(2)).unwrap();
        assert!(
            api.body_pushed.borrow().is_empty(),
            "counts-only push must not touch the page body"
        );
    }

    #[test]
    fn remote_only_change_updates_local() {
        let conn = open_in_memory().unwrap();
        let settings = Settings::default();
        run_sync(
            &conn,
            &FakeNotion::with(vec![remote("p1", "A", t0())]),
            &settings,
            t0(),
        )
        .unwrap();

        let newer = FakeNotion::with(vec![remote("p1", "A renamed", t0() + Duration::hours(1))]);
        let report = run_sync(&conn, &newer, &settings, t0() + Duration::hours(2)).unwrap();
        assert_eq!(report.pulled_updated, 1);
        let task = &task_repo::list(&conn, &Default::default()).unwrap()[0];
        assert_eq!(task.title, "A renamed");
    }

    #[test]
    fn both_sides_changed_is_a_conflict_not_an_overwrite() {
        let conn = open_in_memory().unwrap();
        let settings = Settings::default();
        run_sync(
            &conn,
            &FakeNotion::with(vec![remote("p1", "A", t0())]),
            &settings,
            t0(),
        )
        .unwrap();

        let task_id = task_repo::list(&conn, &Default::default()).unwrap()[0]
            .id
            .clone();
        task_service::rename(&conn, &task_id, "Local title", t0() + Duration::hours(1)).unwrap();

        let newer = FakeNotion::with(vec![remote(
            "p1",
            "Remote title",
            t0() + Duration::hours(2),
        )]);
        let report = run_sync(&conn, &newer, &settings, t0() + Duration::hours(3)).unwrap();

        assert_eq!(report.conflicts, 1);
        assert_eq!(report.pushed, 0);
        let task = task_repo::get(&conn, &task_id).unwrap().unwrap();
        assert!(task.in_conflict());
        assert_eq!(task.title, "Local title", "local kept until user resolves");
        assert!(
            newer.pushed.borrow().is_empty(),
            "no silent push over remote change"
        );
    }

    #[test]
    fn conflict_resolution_keep_notion_applies_remote() {
        use crate::domain::sync_state::ConflictResolution;
        let conn = open_in_memory().unwrap();
        let settings = Settings::default();
        run_sync(
            &conn,
            &FakeNotion::with(vec![remote("p1", "A", t0())]),
            &settings,
            t0(),
        )
        .unwrap();
        let task_id = task_repo::list(&conn, &Default::default()).unwrap()[0]
            .id
            .clone();
        task_service::rename(&conn, &task_id, "Local", t0() + Duration::hours(1)).unwrap();
        let newer = FakeNotion::with(vec![remote("p1", "Remote", t0() + Duration::hours(2))]);
        run_sync(&conn, &newer, &settings, t0() + Duration::hours(3)).unwrap();

        task_service::resolve_conflict(
            &conn,
            &task_id,
            ConflictResolution::KeepNotion,
            t0() + Duration::hours(4),
        )
        .unwrap();
        let task = task_repo::get(&conn, &task_id).unwrap().unwrap();
        assert!(!task.in_conflict());
        assert!(!task.dirty);
        assert_eq!(task.title, "Remote");
    }

    #[test]
    fn conflict_resolution_keep_local_stays_dirty_for_next_push() {
        use crate::domain::sync_state::ConflictResolution;
        let conn = open_in_memory().unwrap();
        let settings = Settings::default();
        run_sync(
            &conn,
            &FakeNotion::with(vec![remote("p1", "A", t0())]),
            &settings,
            t0(),
        )
        .unwrap();
        let task_id = task_repo::list(&conn, &Default::default()).unwrap()[0]
            .id
            .clone();
        task_service::rename(&conn, &task_id, "Local", t0() + Duration::hours(1)).unwrap();
        let newer = FakeNotion::with(vec![remote("p1", "Remote", t0() + Duration::hours(2))]);
        run_sync(&conn, &newer, &settings, t0() + Duration::hours(3)).unwrap();

        task_service::resolve_conflict(
            &conn,
            &task_id,
            ConflictResolution::KeepLocal,
            t0() + Duration::hours(4),
        )
        .unwrap();
        let task = task_repo::get(&conn, &task_id).unwrap().unwrap();
        assert!(!task.in_conflict());
        assert!(task.dirty);
        assert_eq!(task.title, "Local");

        // Next sync pushes the kept-local version instead of re-conflicting:
        // the conflicting remote edit was marked as seen during resolution.
        let report = run_sync(&conn, &newer, &settings, t0() + Duration::hours(5)).unwrap();
        assert_eq!(report.conflicts, 0);
        assert_eq!(report.pushed, 1);
    }

    #[test]
    fn remote_deletion_removes_clean_local_task() {
        let conn = open_in_memory().unwrap();
        let settings = Settings::default();
        run_sync(
            &conn,
            &FakeNotion::with(vec![remote("p1", "A", t0()), remote("p2", "B", t0())]),
            &settings,
            t0(),
        )
        .unwrap();

        // "B" gets deleted in Notion: next sync only returns "A".
        let report = run_sync(
            &conn,
            &FakeNotion::with(vec![remote("p1", "A", t0())]),
            &settings,
            t0() + Duration::hours(1),
        )
        .unwrap();
        assert_eq!(report.deleted, 1);
        let titles: Vec<String> = task_repo::list(&conn, &Default::default())
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();
        assert_eq!(titles, vec!["A".to_string()]);
    }

    #[test]
    fn remote_deletion_keeps_task_with_unsynced_changes() {
        let conn = open_in_memory().unwrap();
        let settings = Settings::default();
        run_sync(
            &conn,
            &FakeNotion::with(vec![remote("p1", "A", t0())]),
            &settings,
            t0(),
        )
        .unwrap();
        let task_id = task_repo::list(&conn, &Default::default()).unwrap()[0]
            .id
            .clone();
        task_service::rename(
            &conn,
            &task_id,
            "A (local work)",
            t0() + Duration::minutes(5),
        )
        .unwrap();

        // Page deleted remotely while the local task is dirty → keep it.
        let report = run_sync(
            &conn,
            &FakeNotion::with(vec![]),
            &settings,
            t0() + Duration::hours(1),
        )
        .unwrap();
        assert_eq!(report.deleted, 0);
        let task = task_repo::get(&conn, &task_id).unwrap().unwrap();
        assert_eq!(task.title, "A (local work)");
        assert!(task.dirty);
    }

    #[test]
    fn fetch_failure_aborts_and_reports_error() {
        let conn = open_in_memory().unwrap();
        let mut api = FakeNotion::with(vec![]);
        api.fail_fetch = true;
        let err = run_sync(&conn, &api, &Settings::default(), t0());
        assert!(err.is_err());
    }

    #[test]
    fn push_failure_is_collected_and_task_stays_dirty() {
        let conn = open_in_memory().unwrap();
        let settings = Settings::default();
        run_sync(
            &conn,
            &FakeNotion::with(vec![remote("p1", "A", t0())]),
            &settings,
            t0(),
        )
        .unwrap();
        let task_id = task_repo::list(&conn, &Default::default()).unwrap()[0]
            .id
            .clone();
        task_service::set_done(&conn, &task_id, true, t0() + Duration::hours(1)).unwrap();

        let mut api = FakeNotion::with(vec![remote("p1", "A", t0())]);
        api.fail_push = true;
        let report = run_sync(&conn, &api, &settings, t0() + Duration::hours(2)).unwrap();
        assert_eq!(report.errors.len(), 1);
        assert!(
            task_repo::any_dirty(&conn).unwrap(),
            "failed push keeps dirty flag"
        );
    }
}
