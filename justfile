dev:
    cargo run

test:
    cargo test

lint:
    cargo clippy --all-targets -- -D warnings

fmt:
    cargo fmt

build:
    cargo build --release

# Install for the current user: binary + desktop entry (needed for notifications)
install: build
    install -Dm755 target/release/notion-pomodoro-tracker ~/.local/bin/notion-pomodoro-tracker
    sed "s|@BINDIR@|$HOME/.local/bin|" data/com.frjr17.NotionPomodoroTracker.desktop \
        > ~/.local/share/applications/com.frjr17.NotionPomodoroTracker.desktop
    update-desktop-database ~/.local/share/applications || true

uninstall:
    rm -f ~/.local/bin/notion-pomodoro-tracker \
        ~/.local/share/applications/com.frjr17.NotionPomodoroTracker.desktop
