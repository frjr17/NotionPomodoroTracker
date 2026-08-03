//! Desktop notifications via GNotification (portal-friendly, GNOME-native).

use gtk::gio;
use gtk::prelude::*;
use std::cell::RefCell;

thread_local! {
    // Keep the media stream alive while it plays. Starting another sound
    // intentionally replaces the previous one, avoiding overlapping alerts.
    static ACTIVE_SOUND: RefCell<Option<gtk::MediaFile>> = const { RefCell::new(None) };
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

pub fn play_sound(sound_path: &str) {
    if sound_path.trim().is_empty() {
        return;
    }
    let media = gtk::MediaFile::for_filename(sound_path);
    media.play();
    ACTIVE_SOUND.with(|active| *active.borrow_mut() = Some(media));
}
