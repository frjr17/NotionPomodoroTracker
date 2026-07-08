use crate::domain::pomodoro::{Timer, TimerConfig};
use crate::infrastructure::sqlite::{StoreResult, settings_repo};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Names of Notion properties mapped to app fields. Empty string = unmapped
/// (allowed for everything except title and status).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyMappings {
    pub title: String,
    pub status: String,
    /// "status" or "select" — discovered during validation.
    #[serde(default = "default_status_type")]
    pub status_type: String,
    #[serde(default)]
    pub due_date: String,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub pomodoro_count: String,
    #[serde(default)]
    pub tracked_minutes: String,
    #[serde(default)]
    pub last_synced: String,
}

fn default_status_type() -> String {
    "status".into()
}

impl Default for PropertyMappings {
    fn default() -> Self {
        Self {
            title: "Name".into(),
            status: "Status".into(),
            status_type: default_status_type(),
            due_date: "Due".into(),
            priority: "Priority".into(),
            pomodoro_count: "Pomodoros".into(),
            tracked_minutes: "Minutes".into(),
            last_synced: "Last Synced".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Settings {
    pub timer: TimerConfig,
    pub notion_database_id: String,
    pub mappings: PropertyMappings,
    pub auto_sync_enabled: bool,
}

const K_TIMER: &str = "timer_config";
const K_DB_ID: &str = "notion_database_id";
const K_MAPPINGS: &str = "property_mappings";
const K_AUTO_SYNC: &str = "auto_sync_enabled";
const K_TIMER_STATE: &str = "timer_state";
const K_LAST_SYNC: &str = "last_sync_at";

pub fn load(conn: &Connection) -> StoreResult<Settings> {
    let mut settings = Settings::default();
    if let Some(json) = settings_repo::get(conn, K_TIMER)?
        && let Ok(cfg) = serde_json::from_str(&json)
    {
        settings.timer = cfg;
    }
    if let Some(id) = settings_repo::get(conn, K_DB_ID)? {
        settings.notion_database_id = id;
    }
    if let Some(json) = settings_repo::get(conn, K_MAPPINGS)?
        && let Ok(m) = serde_json::from_str(&json)
    {
        settings.mappings = m;
    }
    settings.auto_sync_enabled = settings_repo::get(conn, K_AUTO_SYNC)?.as_deref() == Some("true");
    Ok(settings)
}

pub fn save(conn: &Connection, settings: &Settings) -> StoreResult<()> {
    settings_repo::set(
        conn,
        K_TIMER,
        &serde_json::to_string(&settings.timer).unwrap(),
    )?;
    settings_repo::set(conn, K_DB_ID, &settings.notion_database_id)?;
    settings_repo::set(
        conn,
        K_MAPPINGS,
        &serde_json::to_string(&settings.mappings).unwrap(),
    )?;
    settings_repo::set(
        conn,
        K_AUTO_SYNC,
        if settings.auto_sync_enabled {
            "true"
        } else {
            "false"
        },
    )?;
    Ok(())
}

/// Validate user-entered settings before saving. Returns a list of problems.
pub fn validate(settings: &Settings) -> Vec<String> {
    let mut problems = Vec::new();
    let t = &settings.timer;
    for (name, v) in [
        ("Pomodoro length", t.pomodoro_minutes),
        ("Short break", t.short_break_minutes),
        ("Long break", t.long_break_minutes),
        ("Pomodoros until long break", t.pomodoros_until_long_break),
    ] {
        if v == 0 || v > 240 {
            problems.push(format!("{name} must be between 1 and 240"));
        }
    }
    if settings.mappings.title.trim().is_empty() {
        problems.push("Title property mapping is required".into());
    }
    if settings.mappings.status.trim().is_empty() {
        problems.push("Status property mapping is required".into());
    }
    problems
}

pub fn save_timer_state(conn: &Connection, timer: &Timer) -> StoreResult<()> {
    settings_repo::set(conn, K_TIMER_STATE, &serde_json::to_string(timer).unwrap())
}

pub fn load_timer_state(conn: &Connection) -> StoreResult<Timer> {
    Ok(settings_repo::get(conn, K_TIMER_STATE)?
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default())
}

pub fn set_last_sync_at(conn: &Connection, at: &str) -> StoreResult<()> {
    settings_repo::set(conn, K_LAST_SYNC, at)
}

pub fn last_sync_at(conn: &Connection) -> StoreResult<Option<String>> {
    settings_repo::get(conn, K_LAST_SYNC)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite::open_in_memory;

    #[test]
    fn settings_round_trip() {
        let conn = open_in_memory().unwrap();
        let mut s = Settings::default();
        s.timer.pomodoro_minutes = 50;
        s.notion_database_id = "abc123".into();
        s.mappings.priority = String::new();
        s.auto_sync_enabled = true;
        save(&conn, &s).unwrap();
        assert_eq!(load(&conn).unwrap(), s);
    }

    #[test]
    fn defaults_when_unset() {
        let conn = open_in_memory().unwrap();
        assert_eq!(load(&conn).unwrap(), Settings::default());
    }

    #[test]
    fn validation_rejects_zero_durations_and_missing_required_mappings() {
        let mut s = Settings::default();
        assert!(validate(&s).is_empty());
        s.timer.pomodoro_minutes = 0;
        s.mappings.title = String::new();
        let problems = validate(&s);
        assert_eq!(problems.len(), 2);
    }

    #[test]
    fn timer_state_round_trip() {
        let conn = open_in_memory().unwrap();
        let mut timer = Timer::default();
        timer.start("task-9", chrono::Utc::now());
        save_timer_state(&conn, &timer).unwrap();
        assert_eq!(load_timer_state(&conn).unwrap(), timer);
    }
}
