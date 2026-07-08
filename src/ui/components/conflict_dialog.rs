//! "Keep local / Keep Notion" resolution dialog for a conflicted task.

use crate::application::task_service;
use crate::domain::sync_state::{ConflictResolution, RemoteTask};
use crate::domain::task::Task;
use crate::ui::app::Shared;
use adw::prelude::*;
use chrono::Utc;

/// Show the resolution dialog; runs `on_resolved` after the user picks a side.
pub fn show(
    parent: &impl IsA<gtk::Widget>,
    state: &Shared,
    task: &Task,
    on_resolved: impl Fn() + 'static,
) {
    let Some(json) = &task.conflict_remote_json else {
        return;
    };
    let Ok(remote) = serde_json::from_str::<RemoteTask>(json) else {
        return;
    };

    let body = format!(
        "Local version\n  {} · {} · {} pomodoros · {} min\n\nNotion version\n  {} · {} · {} pomodoros · {} min\n\nWhich one should win? The other side will be overwritten.",
        task.title,
        task.status,
        task.pomodoro_count,
        task.tracked_minutes,
        remote.title,
        remote.status.as_deref().unwrap_or("—"),
        remote.pomodoro_count.map_or("—".into(), |n| n.to_string()),
        remote.tracked_minutes.map_or("—".into(), |n| n.to_string()),
    );

    let dialog = adw::AlertDialog::builder()
        .heading("Resolve sync conflict")
        .body(body)
        .build();
    dialog.add_response("cancel", "Decide Later");
    dialog.add_response("local", "Keep Local");
    dialog.add_response("notion", "Keep Notion");
    dialog.set_response_appearance("local", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");

    let state = state.clone();
    let task_id = task.id.clone();
    dialog.connect_response(None, move |_, response| {
        let resolution = match response {
            "local" => ConflictResolution::KeepLocal,
            "notion" => ConflictResolution::KeepNotion,
            _ => return,
        };
        if let Err(e) =
            task_service::resolve_conflict(&state.conn, &task_id, resolution, Utc::now())
        {
            eprintln!("conflict resolution failed: {e}");
        }
        on_resolved();
    });
    dialog.present(Some(parent));
}
