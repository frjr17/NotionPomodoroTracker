# Notion Pomodoro Tracker

Offline-first GTK4/libadwaita desktop app for Fedora/GNOME that imports tasks
from a Notion database, tracks focused work with a Pomodoro timer, and syncs
Pomodoro counts and tracked minutes back to Notion on demand.

## Features

- Offline-first: all tasks, sessions, and settings live in a local SQLite DB.
- Pomodoro timer (25/5/15 defaults, configurable) that stays accurate while
  minimized and survives app restarts.
- Per-task Pomodoro counts, tracked minutes, and session history.
- Manual two-way Notion sync with conflict detection (keep local / keep
  Notion), optional auto-sync.
- Notion token stored in the Secret Service keyring (libsecret), never on disk.
- Desktop notifications for Pomodoro/break completion and sync failures.

## Fedora setup

```sh
sudo dnf install gcc gtk4-devel libadwaita-devel libsecret-devel
# Rust via rustup or dnf: sudo dnf install rust cargo
```

SQLite is bundled (rusqlite `bundled` feature) — no sqlite-devel needed.

## Development commands

A `justfile` is provided (`sudo dnf install just`), or use cargo directly:

| Command      | Cargo equivalent            |
|--------------|-----------------------------|
| `just dev`   | `cargo run`                 |
| `just test`  | `cargo test`                |
| `just lint`  | `cargo clippy -- -D warnings` |
| `just fmt`   | `cargo fmt`                 |
| `just build` | `cargo build --release`     |
| `just install` | installs to `~/.local/bin` + desktop entry |
| `just uninstall` | removes both              |

## Running

```sh
cargo run
```

First run: open **Settings** (gear icon), paste your Notion internal
integration token and database ID, validate, and map properties. See
[NOTION_SETUP.md](NOTION_SETUP.md).

Data lives in `~/.local/share/notion-pomodoro-tracker/app.db`.

## Docs

- [ARCHITECTURE.md](ARCHITECTURE.md) — layers and module layout
- [DATABASE_SCHEMA.md](DATABASE_SCHEMA.md) — SQLite schema
- [NOTION_SETUP.md](NOTION_SETUP.md) — Notion integration setup
- [DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md) — milestones

## Packaging notes (Flatpak, later)

- Runtime: `org.gnome.Platform` / SDK `org.gnome.Sdk` (48+).
- Needed permissions: network (Notion sync only), `org.freedesktop.secrets`
  D-Bus access, notifications.
- Build rust with `--offline` using `flatpak-cargo-generator` for sources.
- App ID: `com.frjr17.NotionPomodoroTracker`; install the `.desktop` file and
  icon so notifications attribute correctly.
