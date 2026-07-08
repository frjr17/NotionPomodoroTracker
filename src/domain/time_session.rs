use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    Pomodoro,
    Manual,
}

impl SessionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionKind::Pomodoro => "pomodoro",
            SessionKind::Manual => "manual",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "pomodoro" => SessionKind::Pomodoro,
            _ => SessionKind::Manual,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimeSession {
    pub id: String,
    pub task_id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub minutes: i64,
    pub kind: SessionKind,
}

impl TimeSession {
    pub fn new(
        task_id: &str,
        started_at: DateTime<Utc>,
        ended_at: DateTime<Utc>,
        minutes: i64,
        kind: SessionKind,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            task_id: task_id.to_string(),
            started_at,
            ended_at,
            minutes,
            kind,
        }
    }
}
