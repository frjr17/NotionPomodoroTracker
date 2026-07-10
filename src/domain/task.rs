use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

pub const DEFAULT_STATUS: &str = "To Do";
pub const DONE_STATUS: &str = "Done";

#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub id: String,
    pub notion_page_id: Option<String>,
    pub title: String,
    pub status: String,
    pub due_date: Option<NaiveDate>,
    pub priority: Option<String>,
    /// Markdown mirror of the Notion page body. `None` until first fetched.
    pub description: Option<String>,
    pub pomodoro_count: u32,
    pub tracked_minutes: u32,
    pub done: bool,
    pub dirty: bool,
    /// Set only when the description was edited locally, so sync knows to
    /// rewrite the page body (and never wipes an un-fetched body).
    pub desc_dirty: bool,
    pub conflict_remote_json: Option<String>,
    pub notion_last_edited: Option<DateTime<Utc>>,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Task {
    pub fn new_local(title: &str, now: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            notion_page_id: None,
            title: title.to_string(),
            status: DEFAULT_STATUS.to_string(),
            due_date: None,
            priority: None,
            description: None,
            pomodoro_count: 0,
            tracked_minutes: 0,
            done: false,
            dirty: false,
            desc_dirty: false,
            conflict_remote_json: None,
            notion_last_edited: None,
            last_synced_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn in_conflict(&self) -> bool {
        self.conflict_remote_json.is_some()
    }

    /// Marks a local edit: bumps `updated_at` and sets the dirty flag so the
    /// change is pushed on next sync (only meaningful for Notion-backed tasks).
    pub fn touch(&mut self, now: DateTime<Utc>) {
        self.updated_at = now;
        self.dirty = true;
    }
}
