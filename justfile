setup-fedora:
    sudo dnf install gcc gtk4-devel libadwaita-devel libsecret-devel alsa-lib-devel

setup-debian:
    sudo apt-get update
    sudo apt-get install build-essential libgtk-4-dev libadwaita-1-dev libsecret-1-dev libasound2-dev

setup-linux:
    #!/usr/bin/env sh
    set -eu
    . /etc/os-release
    case "${ID_LIKE:-$ID}" in
        *debian*) just setup-debian ;;
        *fedora*|*rhel*) just setup-fedora ;;
        *) echo "Unsupported distribution: $ID. Install GTK4, libadwaita, libsecret, and ALSA development packages." >&2; exit 1 ;;
    esac

dev:
    @if ! pkg-config --exists alsa; then \
        echo "Missing ALSA development files. Run: just setup-linux" >&2; \
        exit 1; \
    fi
    cargo run

test:
    cargo test

lint:
    cargo clippy --all-targets -- -D warnings

fmt:
    cargo fmt

build:
    cargo build --release

# Install for the current user: binary + desktop entry (needed for notifications) + icon
install: build
    install -Dm755 target/release/notion-pomodoro-tracker ~/.local/bin/notion-pomodoro-tracker
    sed "s|@BINDIR@|$HOME/.local/bin|" data/com.frjr17.NotionPomodoroTracker.desktop \
        > ~/.local/share/applications/com.frjr17.NotionPomodoroTracker.desktop
    install -Dm644 data/icons/com.frjr17.NotionPomodoroTracker.svg \
        ~/.local/share/icons/hicolor/scalable/apps/com.frjr17.NotionPomodoroTracker.svg
    gtk-update-icon-cache -t ~/.local/share/icons/hicolor || true
    update-desktop-database ~/.local/share/applications || true

uninstall:
    rm -f ~/.local/bin/notion-pomodoro-tracker \
        ~/.local/share/applications/com.frjr17.NotionPomodoroTracker.desktop \
        ~/.local/share/icons/hicolor/scalable/apps/com.frjr17.NotionPomodoroTracker.svg
    update-desktop-database ~/.local/share/applications || true
