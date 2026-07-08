# Database schema

SQLite at `~/.local/share/notion-pomodoro-tracker/app.db`, WAL mode,
`user_version` pragma for migrations. All timestamps are UTC RFC 3339 strings.

## tasks

| column               | type    | notes                                       |
|----------------------|---------|---------------------------------------------|
| id                   | TEXT PK | UUID v4                                     |
| notion_page_id       | TEXT    | UNIQUE, NULL for local-only tasks           |
| title                | TEXT    | NOT NULL                                    |
| status               | TEXT    | Notion status/select name; local default "To Do" |
| due_date             | TEXT    | ISO date, NULL                              |
| priority             | TEXT    | Notion select name, NULL                    |
| pomodoro_count       | INTEGER | NOT NULL DEFAULT 0                          |
| tracked_minutes      | INTEGER | NOT NULL DEFAULT 0                          |
| done                 | INTEGER | 0/1                                         |
| dirty                | INTEGER | 0/1 — local changes not yet pushed          |
| conflict_remote_json | TEXT    | remote snapshot when in conflict, else NULL |
| notion_last_edited   | TEXT    | last_edited_time seen from Notion           |
| last_synced_at       | TEXT    | NULL if never synced                        |
| created_at           | TEXT    | NOT NULL                                    |
| updated_at           | TEXT    | NOT NULL — bumped on every local edit       |

## time_sessions

| column        | type    | notes                                  |
|---------------|---------|----------------------------------------|
| id            | TEXT PK | UUID v4                                |
| task_id       | TEXT    | FK → tasks(id) ON DELETE CASCADE       |
| started_at    | TEXT    | NOT NULL                               |
| ended_at      | TEXT    | NOT NULL                               |
| minutes       | INTEGER | NOT NULL                               |
| kind          | TEXT    | 'pomodoro' or 'manual'                 |

## sync_log

| column     | type    | notes                       |
|------------|---------|------------------------------|
| id         | INTEGER PK AUTOINCREMENT |             |
| at         | TEXT    | timestamp                    |
| level      | TEXT    | 'info' or 'error'            |
| message    | TEXT    |                              |

## app_settings

Key/value store: `key TEXT PRIMARY KEY, value TEXT NOT NULL`.

Keys: `notion_database_id`, `property_mappings` (JSON), `pomodoro_minutes`,
`short_break_minutes`, `long_break_minutes`, `pomodoros_until_long_break`,
`auto_sync_enabled`, `timer_state` (JSON — persisted timer snapshot),
`last_sync_at`.

The Notion token is **not** here — it lives in the Secret Service keyring.
