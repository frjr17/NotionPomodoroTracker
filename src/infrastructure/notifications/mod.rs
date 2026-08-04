//! Desktop notifications via GNotification (portal-friendly, GNOME-native).

use gtk::gio;
use gtk::prelude::*;
use rodio::Source;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

// Incrementing this cancels any notification sound already playing.
static SOUND_GENERATION: AtomicU64 = AtomicU64::new(0);

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

/// Play an audio file on the default system output.
///
/// Decoding and playback run off the GTK thread. Rodio talks directly to the
/// system audio device, so playback does not depend on optional command-line
/// programs or GTK's GStreamer backend.
pub fn play_sound(sound_path: &str) {
    if sound_path.trim().is_empty() {
        return;
    }
    if !std::path::Path::new(sound_path).is_file() {
        eprintln!("notification sound does not exist: {sound_path}");
        return;
    }

    let generation = SOUND_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    let sound_path = sound_path.to_owned();
    std::thread::spawn(move || {
        if let Err(error) = play_sound_file(&sound_path, generation) {
            eprintln!("failed to play notification sound {sound_path}: {error}");
        }
    });
}

fn play_sound_file(sound_path: &str, generation: u64) -> Result<(), Box<dyn std::error::Error>> {
    let mut output = rodio::OutputStreamBuilder::open_default_stream()?;
    output.log_on_drop(false);
    let file = std::fs::File::open(sound_path)?;
    let source = rodio::Decoder::try_from(file)?.take_duration(Duration::from_secs(10));
    let sink = rodio::Sink::connect_new(output.mixer());
    sink.append(source);

    while !sink.empty() {
        if SOUND_GENERATION.load(Ordering::Relaxed) != generation {
            sink.stop();
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}
