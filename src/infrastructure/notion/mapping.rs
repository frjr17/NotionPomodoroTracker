//! Pure JSON ↔ domain mapping for Notion pages. Kept free of HTTP so it can
//! be tested against canned API responses.

use crate::application::settings_service::PropertyMappings;
use crate::domain::sync_state::RemoteTask;
use crate::domain::task::Task;
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{Value, json};

/// Extract a `RemoteTask` from a Notion page object. Returns None only if the
/// page has no usable id. Missing/unmapped optional properties become None.
pub fn remote_task_from_page(page: &Value, m: &PropertyMappings) -> Option<RemoteTask> {
    let page_id = page.get("id")?.as_str()?.to_string();
    let props = page.get("properties").unwrap_or(&Value::Null);
    let last_edited = page
        .get("last_edited_time")
        .and_then(Value::as_str)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_default();

    Some(RemoteTask {
        page_id,
        title: extract_title(props, &m.title).unwrap_or_else(|| "(untitled)".into()),
        status: extract_status(props, &m.status),
        due_date: extract_date(props, &m.due_date),
        priority: extract_select(props, &m.priority),
        pomodoro_count: extract_number(props, &m.pomodoro_count),
        tracked_minutes: extract_number(props, &m.tracked_minutes),
        last_edited,
    })
}

/// Build the `properties` object for a PATCH pushing local changes.
/// Only mapped properties are included, so databases missing optional
/// properties still sync.
pub fn build_push_properties(task: &Task, m: &PropertyMappings, now: DateTime<Utc>) -> Value {
    let mut props = serde_json::Map::new();
    if !m.title.trim().is_empty() {
        props.insert(
            m.title.clone(),
            json!({ "title": [{ "text": { "content": task.title } }] }),
        );
    }
    if !m.status.trim().is_empty() {
        let inner = json!({ "name": task.status });
        props.insert(m.status.clone(), json!({ m.status_type.as_str(): inner }));
    }
    // ponytail: priority assumed to be a Notion `select`, mirroring the pull
    // side (`extract_select`). null clears it when the user removed the value.
    if !m.priority.trim().is_empty() {
        let select = match &task.priority {
            Some(p) => json!({ "name": p }),
            None => Value::Null,
        };
        props.insert(m.priority.clone(), json!({ "select": select }));
    }
    if !m.due_date.trim().is_empty() {
        let date = match &task.due_date {
            Some(d) => json!({ "start": d.to_string() }),
            None => Value::Null,
        };
        props.insert(m.due_date.clone(), json!({ "date": date }));
    }
    if !m.pomodoro_count.trim().is_empty() {
        props.insert(
            m.pomodoro_count.clone(),
            json!({ "number": task.pomodoro_count }),
        );
    }
    if !m.tracked_minutes.trim().is_empty() {
        props.insert(
            m.tracked_minutes.clone(),
            json!({ "number": task.tracked_minutes }),
        );
    }
    if !m.last_synced.trim().is_empty() {
        props.insert(
            m.last_synced.clone(),
            json!({ "date": { "start": now.to_rfc3339() } }),
        );
    }
    Value::Object(props)
}

fn prop<'a>(props: &'a Value, name: &str) -> Option<&'a Value> {
    if name.trim().is_empty() {
        return None;
    }
    props.get(name)
}

fn extract_title(props: &Value, name: &str) -> Option<String> {
    let parts = prop(props, name)?.get("title")?.as_array()?;
    let text: String = parts
        .iter()
        .filter_map(|p| p.get("plain_text").and_then(Value::as_str))
        .collect();
    if text.is_empty() { None } else { Some(text) }
}

/// Handles both `status` and `select` typed status properties.
fn extract_status(props: &Value, name: &str) -> Option<String> {
    let p = prop(props, name)?;
    p.get("status")
        .or_else(|| p.get("select"))
        .and_then(|s| s.get("name"))
        .and_then(Value::as_str)
        .map(String::from)
}

fn extract_select(props: &Value, name: &str) -> Option<String> {
    prop(props, name)?
        .get("select")?
        .get("name")?
        .as_str()
        .map(String::from)
}

fn extract_date(props: &Value, name: &str) -> Option<NaiveDate> {
    let start = prop(props, name)?.get("date")?.get("start")?.as_str()?;
    start.get(..10)?.parse().ok()
}

fn extract_number(props: &Value, name: &str) -> Option<u32> {
    prop(props, name)?
        .get("number")?
        .as_f64()
        .map(|n| n.max(0.0) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::task::Task;

    fn mappings() -> PropertyMappings {
        PropertyMappings::default()
    }

    fn sample_page() -> Value {
        json!({
            "id": "page-123",
            "last_edited_time": "2026-07-01T10:00:00.000Z",
            "properties": {
                "Name": { "title": [
                    { "plain_text": "Ship " }, { "plain_text": "release" }
                ]},
                "Status": { "status": { "name": "In Progress" } },
                "Due": { "date": { "start": "2026-07-15" } },
                "Priority": { "select": { "name": "High" } },
                "Pomodoros": { "number": 3 },
                "Minutes": { "number": 75 }
            }
        })
    }

    #[test]
    fn extracts_all_mapped_fields() {
        let remote = remote_task_from_page(&sample_page(), &mappings()).unwrap();
        assert_eq!(remote.page_id, "page-123");
        assert_eq!(remote.title, "Ship release");
        assert_eq!(remote.status.as_deref(), Some("In Progress"));
        assert_eq!(remote.due_date, Some("2026-07-15".parse().unwrap()));
        assert_eq!(remote.priority.as_deref(), Some("High"));
        assert_eq!(remote.pomodoro_count, Some(3));
        assert_eq!(remote.tracked_minutes, Some(75));
        assert_eq!(remote.last_edited.to_rfc3339(), "2026-07-01T10:00:00+00:00");
    }

    #[test]
    fn tolerates_missing_optional_properties() {
        let page = json!({
            "id": "page-9",
            "last_edited_time": "2026-07-01T10:00:00.000Z",
            "properties": { "Name": { "title": [ { "plain_text": "Bare" } ] } }
        });
        let remote = remote_task_from_page(&page, &mappings()).unwrap();
        assert_eq!(remote.title, "Bare");
        assert_eq!(remote.status, None);
        assert_eq!(remote.due_date, None);
        assert_eq!(remote.pomodoro_count, None);
    }

    #[test]
    fn select_typed_status_also_works() {
        let page = json!({
            "id": "p",
            "last_edited_time": "2026-07-01T10:00:00.000Z",
            "properties": {
                "Name": { "title": [ { "plain_text": "X" } ] },
                "Status": { "select": { "name": "Doing" } }
            }
        });
        let remote = remote_task_from_page(&page, &mappings()).unwrap();
        assert_eq!(remote.status.as_deref(), Some("Doing"));
    }

    #[test]
    fn push_includes_only_mapped_properties() {
        let mut task = Task::new_local("T", Utc::now());
        task.status = "Done".into();
        task.pomodoro_count = 4;
        task.tracked_minutes = 100;

        let mut m = mappings();
        m.tracked_minutes = String::new();
        m.last_synced = String::new();
        let props = build_push_properties(&task, &m, Utc::now());

        assert_eq!(props["Status"]["status"]["name"], "Done");
        assert_eq!(props["Pomodoros"]["number"], 4);
        assert!(props.get("Minutes").is_none());
        assert!(props.get("Last Synced").is_none());
    }

    #[test]
    fn push_includes_title_priority_due() {
        let mut task = Task::new_local("Ship it", Utc::now());
        task.priority = Some("High".into());
        task.due_date = Some("2026-07-15".parse().unwrap());
        let props = build_push_properties(&task, &mappings(), Utc::now());
        assert_eq!(props["Name"]["title"][0]["text"]["content"], "Ship it");
        assert_eq!(props["Priority"]["select"]["name"], "High");
        assert_eq!(props["Due"]["date"]["start"], "2026-07-15");

        // Cleared values push explicit null so Notion unsets them.
        let cleared = Task::new_local("T", Utc::now());
        let props = build_push_properties(&cleared, &mappings(), Utc::now());
        assert!(props["Priority"]["select"].is_null());
        assert!(props["Due"]["date"].is_null());
    }

    #[test]
    fn push_uses_select_when_status_is_select_typed() {
        let task = Task::new_local("T", Utc::now());
        let mut m = mappings();
        m.status_type = "select".into();
        let props = build_push_properties(&task, &m, Utc::now());
        assert_eq!(props["Status"]["select"]["name"], "To Do");
    }
}
