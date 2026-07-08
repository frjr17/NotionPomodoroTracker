use super::StoreResult;
use chrono::Utc;
use rusqlite::{Connection, params};

pub fn info(conn: &Connection, message: &str) -> StoreResult<()> {
    log(conn, "info", message)
}

pub fn error(conn: &Connection, message: &str) -> StoreResult<()> {
    log(conn, "error", message)
}

fn log(conn: &Connection, level: &str, message: &str) -> StoreResult<()> {
    conn.execute(
        "INSERT INTO sync_log (at, level, message) VALUES (?1, ?2, ?3)",
        params![Utc::now().to_rfc3339(), level, message],
    )?;
    // ponytail: unbounded log; add pruning if the table ever gets noticeably big
    Ok(())
}

pub fn recent(conn: &Connection, limit: u32) -> StoreResult<Vec<(String, String, String)>> {
    let mut stmt =
        conn.prepare("SELECT at, level, message FROM sync_log ORDER BY id DESC LIMIT ?1")?;
    let rows = stmt.query_map([limit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}
