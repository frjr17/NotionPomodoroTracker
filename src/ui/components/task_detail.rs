//! Selected-task panel: metadata, progress, conflict banner, session history.

use crate::application::task_service;
use crate::domain::task::Task;
use crate::ui::app::Shared;
use adw::prelude::*;

pub struct TaskDetail {
    pub root: gtk::Box,
    pub conflict_banner: adw::Banner,
    pub title_label: gtk::Label,
    pub meta_label: gtk::Label,
    pub progress_label: gtk::Label,
    pub sync_info_label: gtk::Label,
    pub done_btn: gtk::Button,
    pub add_time_btn: gtk::Button,
    pub sessions_list: gtk::ListBox,
    pub placeholder: adw::StatusPage,
    pub content: gtk::Box,
}

impl Default for TaskDetail {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskDetail {
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();

        let placeholder = adw::StatusPage::builder()
            .icon_name("object-select-symbolic")
            .title("No task selected")
            .description("Pick a task from the list to start tracking")
            .vexpand(true)
            .build();

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(18)
            .margin_end(18)
            .visible(false)
            .build();

        let conflict_banner = adw::Banner::builder()
            .title("This task changed both locally and in Notion")
            .button_label("Resolve…")
            .build();

        let title_label = gtk::Label::builder()
            .css_classes(["title-2"])
            .halign(gtk::Align::Start)
            .wrap(true)
            .build();
        let meta_label = gtk::Label::builder()
            .css_classes(["dim-label"])
            .halign(gtk::Align::Start)
            .build();
        let progress_label = gtk::Label::builder().halign(gtk::Align::Start).build();
        let sync_info_label = gtk::Label::builder()
            .css_classes(["dim-label", "caption"])
            .halign(gtk::Align::Start)
            .build();

        let actions = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        let done_btn = gtk::Button::with_label("Mark Done");
        let add_time_btn = gtk::Button::with_label("Add Time…");
        actions.append(&done_btn);
        actions.append(&add_time_btn);

        let sessions_header = gtk::Label::builder()
            .label("Session history")
            .css_classes(["heading"])
            .halign(gtk::Align::Start)
            .margin_top(12)
            .build();
        let sessions_list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        let sessions_scroll = gtk::ScrolledWindow::builder()
            .child(&sessions_list)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();

        content.append(&conflict_banner);
        content.append(&title_label);
        content.append(&meta_label);
        content.append(&progress_label);
        content.append(&sync_info_label);
        content.append(&actions);
        content.append(&sessions_header);
        content.append(&sessions_scroll);

        root.append(&placeholder);
        root.append(&content);

        Self {
            root,
            conflict_banner,
            title_label,
            meta_label,
            progress_label,
            sync_info_label,
            done_btn,
            add_time_btn,
            sessions_list,
            placeholder,
            content,
        }
    }

    pub fn refresh(&self, state: &Shared) {
        let task = state
            .selected_task_id
            .borrow()
            .as_ref()
            .and_then(|id| task_service::get(&state.conn, id).ok().flatten());
        let Some(task) = task else {
            self.placeholder.set_visible(true);
            self.content.set_visible(false);
            return;
        };
        self.placeholder.set_visible(false);
        self.content.set_visible(true);
        self.render(state, &task);
    }

    fn render(&self, state: &Shared, task: &Task) {
        self.conflict_banner.set_revealed(task.in_conflict());
        self.title_label.set_label(&task.title);

        let mut meta = vec![format!("Status: {}", task.status)];
        if let Some(due) = task.due_date {
            meta.push(format!("Due: {due}"));
        }
        if let Some(p) = &task.priority {
            meta.push(format!("Priority: {p}"));
        }
        self.meta_label.set_label(&meta.join("   ·   "));

        let hours = task.tracked_minutes / 60;
        let mins = task.tracked_minutes % 60;
        self.progress_label.set_label(&format!(
            "🍅 {} pomodoros   ·   ⏱ {}h {:02}m tracked",
            task.pomodoro_count, hours, mins
        ));

        let source = if task.notion_page_id.is_some() {
            "Notion task"
        } else {
            "Local-only task"
        };
        let synced = match &task.last_synced_at {
            Some(at) => format!("last synced {}", at.format("%Y-%m-%d %H:%M UTC")),
            None => "never synced".into(),
        };
        let pending = if task.dirty {
            " · changes pending"
        } else {
            ""
        };
        self.sync_info_label
            .set_label(&format!("{source} · {synced}{pending}"));

        self.done_btn.set_label(if task.done {
            "Reopen Task"
        } else {
            "Mark Done"
        });

        self.sessions_list.remove_all();
        let sessions = task_service::sessions(&state.conn, &task.id).unwrap_or_default();
        if sessions.is_empty() {
            let row = adw::ActionRow::builder().title("No sessions yet").build();
            row.add_css_class("dim-label");
            self.sessions_list.append(&row);
        }
        for s in sessions.iter().take(30) {
            let kind = match s.kind {
                crate::domain::time_session::SessionKind::Pomodoro => "Pomodoro",
                crate::domain::time_session::SessionKind::Manual => "Manual entry",
            };
            let row = adw::ActionRow::builder()
                .title(format!("{} — {} min", kind, s.minutes))
                .subtitle(s.started_at.format("%Y-%m-%d %H:%M UTC").to_string())
                .build();
            self.sessions_list.append(&row);
        }
    }
}
