use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// App-wide sync status shown in the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncStatus {
    Offline,
    Syncing,
    Synced,
    /// Local changes not yet pushed.
    Pending,
    Failed(String),
}

impl SyncStatus {
    pub fn label(&self) -> String {
        match self {
            SyncStatus::Offline => "Offline".into(),
            SyncStatus::Syncing => "Syncing…".into(),
            SyncStatus::Synced => "Synced".into(),
            SyncStatus::Pending => "Local changes pending".into(),
            SyncStatus::Failed(e) => format!("Sync failed: {e}"),
        }
    }
}

/// Snapshot of a task as it exists in Notion. Also stored as JSON on a local
/// task while it is in conflict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteTask {
    pub page_id: String,
    pub title: String,
    pub status: Option<String>,
    pub due_date: Option<NaiveDate>,
    pub priority: Option<String>,
    pub pomodoro_count: Option<u32>,
    pub tracked_minutes: Option<u32>,
    pub last_edited: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    KeepLocal,
    KeepNotion,
}

/// Outcome of one sync run, for the UI and the sync log.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SyncReport {
    pub pulled_new: u32,
    pub pulled_updated: u32,
    pub pushed: u32,
    pub conflicts: u32,
    pub errors: Vec<String>,
}
