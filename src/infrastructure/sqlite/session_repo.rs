use super::StoreResult;
use crate::domain::time_session::{SessionKind, TimeSession};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};

pub fn insert(conn: &Connection, session: &TimeSession) -> StoreResult<()> {
    conn.execute(
        "INSERT INTO time_sessions (id, task_id, started_at, ended_at, minutes, kind)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            session.id,
            session.task_id,
            session.started_at.to_rfc3339(),
            session.ended_at.to_rfc3339(),
            session.minutes,
            session.kind.as_str(),
        ],
    )?;
    Ok(())
}

pub fn list_for_task(conn: &Connection, task_id: &str) -> StoreResult<Vec<TimeSession>> {
    let mut stmt = conn.prepare(
        "SELECT id, task_id, started_at, ended_at, minutes, kind
         FROM time_sessions WHERE task_id = ?1 ORDER BY started_at DESC",
    )?;
    let rows = stmt.query_map([task_id], |row| {
        Ok(TimeSession {
            id: row.get(0)?,
            task_id: row.get(1)?,
            started_at: parse(&row.get::<_, String>(2)?),
            ended_at: parse(&row.get::<_, String>(3)?),
            minutes: row.get(4)?,
            kind: SessionKind::parse(&row.get::<_, String>(5)?),
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn parse(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_default()
}
