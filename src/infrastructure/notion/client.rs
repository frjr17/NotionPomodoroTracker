//! Blocking Notion API client (ureq). Runs on the background sync thread —
//! never on the GTK main thread. Handles pagination, rate limiting, and
//! retries in one place.

use super::{mapping, markdown};
use crate::application::settings_service::PropertyMappings;
use crate::application::sync_service::{NotionApi, NotionError};
use crate::domain::sync_state::RemoteTask;
use crate::domain::task::Task;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::time::{Duration, Instant};

const BASE: &str = "https://api.notion.com/v1";
const NOTION_VERSION: &str = "2022-06-28";
/// Notion allows ~3 requests/second.
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(350);
const MAX_RETRIES: u32 = 3;

pub struct NotionClient {
    token: String,
    agent: ureq::Agent,
    last_request: RefCell<Option<Instant>>,
}

impl NotionClient {
    pub fn new(token: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build();
        Self {
            token: token.trim().to_string(),
            agent: config.into(),
            last_request: RefCell::new(None),
        }
    }

    /// GET the database object — used by the setup screen to validate the
    /// token + database ID and discover properties. Returns (name, type)
    /// pairs.
    pub fn database_properties(
        &self,
        database_id: &str,
    ) -> Result<Vec<(String, String)>, NotionError> {
        let body = self.request("GET", &format!("{BASE}/databases/{database_id}"), None)?;
        let props = body
            .get("properties")
            .and_then(Value::as_object)
            .ok_or(NotionError::Api {
                status: 200,
                message: "no properties in response".into(),
            })?;
        Ok(props
            .iter()
            .map(|(name, p)| {
                let ty = p.get("type").and_then(Value::as_str).unwrap_or("unknown");
                (name.clone(), ty.to_string())
            })
            .collect())
    }

    /// All block children of a page/block (paginated). Nested children are not
    /// recursed into. ponytail: flat body only — fine for task notes.
    fn fetch_children(&self, page_id: &str) -> Result<Vec<Value>, NotionError> {
        let mut blocks = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut url = format!("{BASE}/blocks/{page_id}/children?page_size=100");
            if let Some(c) = &cursor {
                url.push_str(&format!("&start_cursor={c}"));
            }
            let page = self.request("GET", &url, None)?;
            if let Some(results) = page.get("results").and_then(Value::as_array) {
                blocks.extend(results.iter().cloned());
            }
            match page.get("next_cursor").and_then(Value::as_str) {
                Some(c) if page.get("has_more").and_then(Value::as_bool) == Some(true) => {
                    cursor = Some(c.to_string());
                }
                _ => break,
            }
        }
        Ok(blocks)
    }

    fn page_last_edited(&self, page_id: &str, fallback: DateTime<Utc>) -> DateTime<Utc> {
        self.request("GET", &format!("{BASE}/pages/{page_id}"), None)
            .ok()
            .and_then(|p| {
                p.get("last_edited_time")
                    .and_then(Value::as_str)
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            })
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or(fallback)
    }

    fn rate_limit(&self) {
        let mut last = self.last_request.borrow_mut();
        if let Some(at) = *last {
            let since = at.elapsed();
            if since < MIN_REQUEST_INTERVAL {
                std::thread::sleep(MIN_REQUEST_INTERVAL - since);
            }
        }
        *last = Some(Instant::now());
    }

    /// One request with rate limiting and retries on 429/5xx/transport errors.
    fn request(&self, method: &str, url: &str, body: Option<&Value>) -> Result<Value, NotionError> {
        let mut delay = Duration::from_secs(1);
        for attempt in 0..=MAX_RETRIES {
            self.rate_limit();
            let result = self.send_once(method, url, body);
            match &result {
                Ok(_) => return result,
                Err(NotionError::Network(_))
                | Err(NotionError::Api {
                    status: 429 | 500..=599,
                    ..
                }) if attempt < MAX_RETRIES => {
                    std::thread::sleep(delay);
                    delay *= 2;
                }
                Err(_) => return result,
            }
        }
        unreachable!("loop always returns")
    }

    fn send_once(
        &self,
        method: &str,
        url: &str,
        body: Option<&Value>,
    ) -> Result<Value, NotionError> {
        let auth = format!("Bearer {}", self.token);
        let response = match (method, body) {
            ("GET", _) => self
                .agent
                .get(url)
                .header("Authorization", &auth)
                .header("Notion-Version", NOTION_VERSION)
                .call(),
            ("POST", Some(json_body)) => self
                .agent
                .post(url)
                .header("Authorization", &auth)
                .header("Notion-Version", NOTION_VERSION)
                .send_json(json_body),
            ("PATCH", Some(json_body)) => self
                .agent
                .patch(url)
                .header("Authorization", &auth)
                .header("Notion-Version", NOTION_VERSION)
                .send_json(json_body),
            ("DELETE", _) => self
                .agent
                .delete(url)
                .header("Authorization", &auth)
                .header("Notion-Version", NOTION_VERSION)
                .call(),
            _ => unreachable!("unsupported method/body combination"),
        };
        let mut response = response.map_err(|e| NotionError::Network(e.to_string()))?;
        let status = response.status().as_u16();
        let value: Value = response.body_mut().read_json().unwrap_or(Value::Null);
        match status {
            200..=299 => Ok(value),
            401 | 403 => Err(NotionError::Unauthorized),
            404 => Err(NotionError::NotFound),
            _ => Err(NotionError::Api {
                status,
                message: value
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error")
                    .to_string(),
            }),
        }
    }
}

impl NotionApi for NotionClient {
    fn fetch_tasks(
        &self,
        database_id: &str,
        mappings: &PropertyMappings,
    ) -> Result<Vec<RemoteTask>, NotionError> {
        let url = format!("{BASE}/databases/{database_id}/query");
        let mut tasks = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut body = json!({ "page_size": 100 });
            if let Some(c) = &cursor {
                body["start_cursor"] = json!(c);
            }
            let page = self.request("POST", &url, Some(&body))?;
            if let Some(results) = page.get("results").and_then(Value::as_array) {
                tasks.extend(
                    results
                        .iter()
                        .filter_map(|p| mapping::remote_task_from_page(p, mappings)),
                );
            }
            match page.get("next_cursor").and_then(Value::as_str) {
                Some(c) if page.get("has_more").and_then(Value::as_bool) == Some(true) => {
                    cursor = Some(c.to_string());
                }
                _ => break,
            }
        }
        Ok(tasks)
    }

    fn push_task(
        &self,
        task: &Task,
        mappings: &PropertyMappings,
        now: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, NotionError> {
        let page_id = task.notion_page_id.as_deref().ok_or(NotionError::Api {
            status: 0,
            message: "task has no Notion page id".into(),
        })?;
        let body = json!({ "properties": mapping::build_push_properties(task, mappings, now) });
        let response = self.request("PATCH", &format!("{BASE}/pages/{page_id}"), Some(&body))?;
        Ok(response
            .get("last_edited_time")
            .and_then(Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or(now))
    }

    fn fetch_page_markdown(&self, page_id: &str) -> Result<String, NotionError> {
        let blocks = self.fetch_children(page_id)?;
        Ok(markdown::blocks_to_markdown(&blocks))
    }

    fn replace_page_body(
        &self,
        page_id: &str,
        markdown: &str,
    ) -> Result<DateTime<Utc>, NotionError> {
        // ponytail: delete every existing child, then append fresh blocks. Fine
        // for small task notes; a large page would want block-level diffing.
        for block in self.fetch_children(page_id)? {
            if let Some(id) = block.get("id").and_then(Value::as_str) {
                self.request("DELETE", &format!("{BASE}/blocks/{id}"), None)?;
            }
        }
        let blocks = markdown::markdown_to_blocks(markdown);
        // Notion accepts at most 100 children per append call.
        for chunk in blocks.chunks(100) {
            let body = json!({ "children": chunk });
            self.request(
                "PATCH",
                &format!("{BASE}/blocks/{page_id}/children"),
                Some(&body),
            )?;
        }
        Ok(self.page_last_edited(page_id, Utc::now()))
    }
}
