//! Desktop notifications via GNotification (portal-friendly, GNOME-native).

use gtk::gio;
use gtk::prelude::*;

pub fn notify(app: &impl IsA<gio::Application>, id: &str, title: &str, body: &str) {
    let notification = gio::Notification::new(title);
    notification.set_body(Some(body));
    app.as_ref().send_notification(Some(id), &notification);
}
