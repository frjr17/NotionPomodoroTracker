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
    // libcanberra integrates best with GNOME; PipeWire and GStreamer cover
    // systems where canberra's command-line utility is not installed.
    let players: [(&str, &[&str]); 3] = [
        ("canberra-gtk-play", &["--file", sound_path]),
        ("pw-play", &[sound_path]),
        ("gst-play-1.0", &["--no-interactive", sound_path]),
    ];
    for (program, args) in players {
        match Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => return Some(child),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                eprintln!("failed to start {program} for notification sound: {error}");
                return None;
            }
        }
    }
    eprintln!(
        "cannot play notification sound: install canberra-gtk-play, pw-play, or gst-play-1.0"
    );
    None
}
