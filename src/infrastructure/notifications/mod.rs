//! Desktop notifications via GNotification (portal-friendly, GNOME-native).

use gtk::gio;
use gtk::prelude::*;
use std::cell::RefCell;
use std::process::{Child, Command, Stdio};

thread_local! {
    // Keep the player process so a new alert can stop an overlapping sound.
    static ACTIVE_SOUND: RefCell<Option<Child>> = const { RefCell::new(None) };
}

pub fn notify(app: &impl IsA<gio::Application>, id: &str, title: &str, body: &str) {
    let notification = gio::Notification::new(title);
    notification.set_body(Some(body));
    app.as_ref().send_notification(Some(id), &notification);
}

/// Send a desktop notification and, when configured, play the chosen audio
/// file. Invalid or unavailable files fail quietly so an alert can never
/// disrupt timer progression.
pub fn notify_with_sound(
    app: &impl IsA<gio::Application>,
    id: &str,
    title: &str,
    body: &str,
    sound_path: &str,
) {
    notify(app, id, title, body);
    play_sound(sound_path);
}

/// Play an audio file using an installed desktop audio player.
///
/// Playback runs out of process because a malformed file or a GStreamer plugin
/// failure must never be able to abort the timer application. Fedora's GNOME
/// installation normally provides at least one of these players.
pub fn play_sound(sound_path: &str) {
    if sound_path.trim().is_empty() {
        return;
    }
    if !std::path::Path::new(sound_path).is_file() {
        eprintln!("notification sound does not exist: {sound_path}");
        return;
    }

    ACTIVE_SOUND.with(|active| {
        let mut active = active.borrow_mut();
        if let Some(mut previous) = active.take() {
            if matches!(previous.try_wait(), Ok(None)) {
                let _ = previous.kill();
            }
            let _ = previous.wait();
        }

        *active = spawn_player(sound_path);
    });
}

fn spawn_player(sound_path: &str) -> Option<Child> {
    // A program being present does not mean it can decode the selected file.
    // Run the players in a helper process and fall through when one exits with
    // an error (for example, libcanberra commonly rejects MP3 files). The path
    // is passed as a positional argument rather than interpolated into this
    // script, so spaces and shell metacharacters remain safe.
    const SCRIPT: &str = r#"
if command -v pw-play >/dev/null 2>&1 && pw-play "$1"; then exit 0; fi
if command -v paplay >/dev/null 2>&1 && paplay "$1"; then exit 0; fi
if command -v canberra-gtk-play >/dev/null 2>&1 && canberra-gtk-play --file="$1"; then exit 0; fi
if command -v ffplay >/dev/null 2>&1 && ffplay -nodisp -autoexit -loglevel error "$1"; then exit 0; fi
if command -v mpv >/dev/null 2>&1 && mpv --no-video --really-quiet "$1"; then exit 0; fi
if command -v gst-play-1.0 >/dev/null 2>&1 && gst-play-1.0 --no-interactive "$1"; then exit 0; fi
echo "cannot play notification sound: no installed player could decode $1" >&2
exit 1
"#;

    match Command::new("sh")
        .args(["-c", SCRIPT, "notification-sound-player", sound_path])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        // Keep stderr visible: decoder and audio-server errors are actionable
        // and should not be mistaken for a successful silent preview.
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(child) => Some(child),
        Err(error) => {
            eprintln!("failed to start notification sound player: {error}");
            None
        }
    }
}
