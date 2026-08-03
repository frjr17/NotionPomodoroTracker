//! Desktop notifications via GNotification (portal-friendly, GNOME-native).

use gtk::gio;
use gtk::prelude::*;
use std::cell::RefCell;

thread_local! {
    // Keep the media stream alive while it plays. Starting another sound
    // intentionally replaces the previous one, avoiding overlapping alerts.
    static ACTIVE_SOUND: RefCell<Option<(gtk::MediaFile, gtk::gdk::Surface)>> = const { RefCell::new(None) };
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
    app: &impl IsA<gtk::Application>,
    id: &str,
    title: &str,
    body: &str,
    sound_path: &str,
) {
    notify(app, id, title, body);
    let surface = app
        .as_ref()
        .active_window()
        .and_then(|window| window.surface());
    play_sound(sound_path, surface.as_ref());
}

/// Play an audio file through GTK's media backend.
///
/// A standalone `MediaFile` must be realized against a surface before GTK
/// allocates its playback resources. Without this step the stream appears to
/// be playing but produces no audio.
pub fn play_sound(sound_path: &str, surface: Option<&gtk::gdk::Surface>) {
    if sound_path.trim().is_empty() {
        return;
    }
    let Some(surface) = surface else {
        return;
    };
    let media = gtk::MediaFile::for_filename(sound_path);
    media.realize(surface);
    media.set_muted(false);
    media.set_volume(1.0);
    media.play();
    ACTIVE_SOUND.with(|active| {
        if let Some((previous, previous_surface)) =
            active.borrow_mut().replace((media, surface.clone()))
        {
            previous.pause();
            previous.unrealize(&previous_surface);
        }
    });
}
