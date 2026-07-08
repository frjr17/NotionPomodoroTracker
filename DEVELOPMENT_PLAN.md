# Development plan

## Milestone 1 — Skeleton
- Cargo project with gtk4/libadwaita; Adw window opens.
- SQLite DB created and migrated at startup (XDG data dir).
- Settings (preferences) window with timer durations and Notion fields.

## Milestone 2 — Local tasks + timer
- Domain: `Task`, `TimeSession`, pure `Timer` state machine.
- Task repository + `TaskService`: create, list, edit, mark done, filters.
- `TimerService`: start/pause/resume/stop, break scheduling, persistence,
  Pomodoro completion increments count + writes a session.
- Manual time-entry correction.
- Tests: timer transitions, completion, pause/resume, persistence.

## Milestone 3 — Notion pull
- `NotionClient` (ureq) behind `NotionApi` trait; token via keyring.
- Settings validation: token, DB ID, property discovery + mapping.
- Pull: import/refresh tasks into SQLite, no duplicates.
- Tests: mapping + adapter against canned JSON, fake-API pull.

## Milestone 4 — Push + manual sync
- Push dirty tasks (status, pomodoros, minutes, last-synced).
- Sync Now button, background thread, live status
  (Offline / Syncing / Synced / Pending / Failed), sync log, retries,
  rate limiting.
- Tests: dirty-flag behavior, retry logic with fake API.

## Milestone 5 — Conflicts + polish
- Conflict detection (both sides changed) + resolution dialog
  (keep local / keep Notion).
- Desktop notifications: Pomodoro done, break done, sync failed.
- Auto-sync toggle, dark/light follows system, UI polish.
- Tests: conflict detection and resolution paths.

After each milestone: `cargo fmt && cargo clippy && cargo test`, launch with
`cargo run`.
