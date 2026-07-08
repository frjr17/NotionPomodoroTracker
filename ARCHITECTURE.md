# Architecture

Clean, four-layer layout. Dependencies point inward: `ui → application →
domain`; `infrastructure` implements interfaces the inner layers define.

```
src/
  domain/            # pure data + logic, no I/O
    task.rs          # Task entity, TaskStatus, Priority
    pomodoro.rs      # Timer state machine (pure, injected clock)
    time_session.rs  # TimeSession entity
    sync_state.rs    # SyncStatus, ConflictResolution, RemoteTask snapshot
  application/       # use cases, orchestration
    task_service.rs      # CRUD, filtering, mark done, manual time entries
    timer_service.rs     # drives domain timer, persists state, emits events
    sync_service.rs      # two-way sync over the NotionApi trait
    settings_service.rs  # settings + property mappings + validation
  infrastructure/
    sqlite/          # Db (connection + migrations), repositories
    notion/          # NotionClient (ureq) implementing NotionApi; property mapping
    secret_store/    # token in Secret Service keyring via `keyring` crate
    notifications/   # gio::Notification wrapper
  ui/                # GTK4/libadwaita; talks only to application services
    app.rs
    windows/         # main_window, settings (preferences) window
    components/      # task_list, task_detail, timer_view, sync_status, conflict_dialog
  main.rs
```

## Key decisions

- **Timer correctness**: `domain::pomodoro::Timer` is a pure state machine.
  Every transition takes `now: DateTime<Utc>`; remaining time is computed as
  `duration - (accumulated + (now - started_at))`. The UI merely ticks once a
  second to redraw. Minimization, suspend, or restart cannot drift it. Timer
  state is persisted to `app_settings` on every transition and restored at
  startup.
- **NotionApi trait** (`application::sync_service`): `SyncService` is generic
  over it. Production uses `infrastructure::notion::NotionClient` (blocking
  `ureq`, run in a background thread); tests use an in-memory fake. The UI
  never touches Notion directly.
- **Threading**: GTK main thread owns services (`Rc<RefCell<…>>`). Sync runs
  on a `std::thread` with its own SQLite connection (WAL mode); progress and
  results come back over an `async_channel` consumed with
  `glib::spawn_future_local`.
- **Offline-first**: the only network code is in `infrastructure/notion`,
  invoked solely by `SyncService` when the user clicks *Sync Now* (or
  auto-sync is enabled).
- **Errors**: `thiserror` enums per layer (`DomainError`, `SyncError`,
  `NotionError`, `StoreError`); no panics in library code.

## Sync algorithm

1. Push: for every task with `dirty = 1` and a `notion_page_id`, PATCH the
   mapped properties (status, pomodoro count, tracked minutes, last-synced
   timestamp) — unless the remote page also changed since `last_synced_at`,
   which marks a **conflict** instead.
2. Pull: query the database (paginated). New pages insert local tasks;
   remote-only changes update local; remote+local changes mark a conflict
   (remote snapshot stored in `tasks.conflict_remote_json`).
3. Conflicts are resolved explicitly in the UI: *keep local* (re-marks dirty
   for next push) or *keep Notion* (overwrites local fields, clears dirty).
4. Rate limiting: ≥350 ms between requests (Notion allows ~3 req/s); 429/5xx
   and transport errors retry 3× with exponential backoff. Errors are written
   to `sync_log`.
5. Identity: `tasks.notion_page_id` is UNIQUE — no duplicate imports.
