//! Runs sync on a background thread (its own SQLite connection — the GTK
//! main thread's connection is never shared) and reports back on the main
//! loop. Also owns the idle sync-status derivation.

use crate::application::settings_service::Settings;
use crate::application::sync_service::{self, NotionApi};
use crate::domain::sync_state::{SyncReport, SyncStatus};
use crate::infrastructure::notion::NotionClient;
use crate::infrastructure::secret_store;
use crate::infrastructure::sqlite::{self, task_repo};
use crate::ui::app::Shared;
use chrono::Utc;
use std::path::Path;

pub enum SyncOutcome {
    Done(SyncReport),
    Failed(String),
}

/// Current status for the header label, from app state + database.
pub fn current_status(state: &Shared) -> SyncStatus {
    if state.syncing.get() {
        return SyncStatus::Syncing;
    }
    if let Some(err) = state.last_sync_error.borrow().as_ref() {
        return SyncStatus::Failed(err.clone());
    }
    if task_repo::any_dirty(&state.conn).unwrap_or(false) {
        return SyncStatus::Pending;
    }
    match crate::application::settings_service::last_sync_at(&state.conn) {
        Ok(Some(_)) => SyncStatus::Synced,
        _ => SyncStatus::Offline,
    }
}

/// Kick off a sync. `on_done` runs on the main thread with the outcome.
/// No-ops (with a reason) if a sync is already running or setup is missing.
pub fn trigger(state: &Shared, on_done: impl Fn(SyncOutcome) + 'static) {
    if state.syncing.get() {
        return;
    }
    let settings = state.settings.borrow().clone();
    if settings.notion_database_id.trim().is_empty() {
        on_done(SyncOutcome::Failed(
            "Notion is not configured — open Settings".into(),
        ));
        return;
    }
    let token = match secret_store::load_token() {
        Ok(Some(t)) if !t.is_empty() => t,
        Ok(_) => {
            on_done(SyncOutcome::Failed(
                "No Notion token stored — open Settings".into(),
            ));
            return;
        }
        Err(e) => {
            on_done(SyncOutcome::Failed(format!("keyring: {e}")));
            return;
        }
    };

    state.syncing.set(true);
    *state.last_sync_error.borrow_mut() = None;
    let db_path = state.db_path.clone();

    let (tx, rx) = async_channel::bounded::<SyncOutcome>(1);
    std::thread::spawn(move || {
        let outcome = run_in_thread(&db_path, &token, &settings);
        let _ = tx.send_blocking(outcome);
    });

    let state = state.clone();
    gtk::glib::spawn_future_local(async move {
        if let Ok(outcome) = rx.recv().await {
            state.syncing.set(false);
            if let SyncOutcome::Failed(msg) = &outcome {
                *state.last_sync_error.borrow_mut() = Some(msg.clone());
            }
            on_done(outcome);
        }
    });
}

fn run_in_thread(db_path: &Path, token: &str, settings: &Settings) -> SyncOutcome {
    let conn = match sqlite::open(db_path) {
        Ok(c) => c,
        Err(e) => return SyncOutcome::Failed(format!("open db: {e}")),
    };
    let client = NotionClient::new(token);
    match sync_service::run_sync(&conn, &client as &dyn NotionApi, settings, Utc::now()) {
        Ok(report) => SyncOutcome::Done(report),
        Err(e) => SyncOutcome::Failed(e.to_string()),
    }
}
