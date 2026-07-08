//! Sidebar: search, filters, task rows, and a "new task" entry.

use crate::application::task_service::{self, TaskFilter};
use crate::ui::app::Shared;
use adw::prelude::*;
use chrono::Utc;
use std::cell::RefCell;

pub struct TaskList {
    pub root: gtk::Box,
    pub list: gtk::ListBox,
    pub search: gtk::SearchEntry,
    pub status_filter: gtk::DropDown,
    pub priority_filter: gtk::DropDown,
    pub show_done: gtk::CheckButton,
    pub new_task_entry: gtk::Entry,
    statuses: RefCell<Vec<String>>,
    priorities: RefCell<Vec<String>>,
}

const ALL: &str = "All";

impl Default for TaskList {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskList {
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(6)
            .margin_end(6)
            .build();

        let search = gtk::SearchEntry::builder()
            .placeholder_text("Search tasks…")
            .build();
        root.append(&search);

        let filters = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        let status_filter = gtk::DropDown::from_strings(&[ALL]);
        status_filter.set_tooltip_text(Some("Filter by status"));
        let priority_filter = gtk::DropDown::from_strings(&[ALL]);
        priority_filter.set_tooltip_text(Some("Filter by priority"));
        let show_done = gtk::CheckButton::with_label("Done");
        show_done.set_tooltip_text(Some("Show completed tasks"));
        filters.append(&status_filter);
        filters.append(&priority_filter);
        filters.append(&show_done);
        root.append(&filters);

        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();
        let scroll = gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        root.append(&scroll);

        let new_task_entry = gtk::Entry::builder()
            .placeholder_text("New local task — press Enter")
            .secondary_icon_name("list-add-symbolic")
            .build();
        root.append(&new_task_entry);

        Self {
            root,
            list,
            search,
            status_filter,
            priority_filter,
            show_done,
            new_task_entry,
            statuses: RefCell::new(vec![]),
            priorities: RefCell::new(vec![]),
        }
    }

    fn dropdown_value(dd: &gtk::DropDown, values: &[String]) -> Option<String> {
        let idx = dd.selected() as usize;
        if idx == 0 {
            return None; // "All"
        }
        values.get(idx - 1).cloned()
    }

    pub fn current_filter(&self) -> TaskFilter {
        TaskFilter {
            status: Self::dropdown_value(&self.status_filter, &self.statuses.borrow()),
            priority: Self::dropdown_value(&self.priority_filter, &self.priorities.borrow()),
            due_on_or_before: None,
            search: Some(self.search.text().to_string()),
            include_done: self.show_done.is_active(),
        }
    }

    /// Rebuild filter dropdown options from current data, keeping selection.
    pub fn refresh_filter_options(&self, state: &Shared) {
        let tasks = task_service::list(
            &state.conn,
            &TaskFilter {
                include_done: true,
                ..Default::default()
            },
        )
        .unwrap_or_default();

        let mut statuses: Vec<String> = tasks.iter().map(|t| t.status.clone()).collect();
        statuses.sort();
        statuses.dedup();
        let mut priorities: Vec<String> = tasks.iter().filter_map(|t| t.priority.clone()).collect();
        priorities.sort();
        priorities.dedup();

        Self::update_dropdown(&self.status_filter, &self.statuses, statuses);
        Self::update_dropdown(&self.priority_filter, &self.priorities, priorities);
    }

    fn update_dropdown(dd: &gtk::DropDown, current: &RefCell<Vec<String>>, values: Vec<String>) {
        if *current.borrow() == values {
            return;
        }
        let selected = Self::dropdown_value(dd, &current.borrow());
        let mut items: Vec<&str> = vec![ALL];
        items.extend(values.iter().map(String::as_str));
        dd.set_model(Some(&gtk::StringList::new(&items)));
        if let Some(sel) = selected
            && let Some(pos) = values.iter().position(|v| *v == sel)
        {
            dd.set_selected((pos + 1) as u32);
        }
        *current.borrow_mut() = values;
    }

    /// Repopulate rows. Each row's widget name carries the task id.
    pub fn refresh(&self, state: &Shared) {
        let filter = self.current_filter();
        let tasks = task_service::list(&state.conn, &filter).unwrap_or_default();
        let selected_id = state.selected_task_id.borrow().clone();

        self.list.remove_all();
        for task in &tasks {
            let row = adw::ActionRow::builder()
                .title(&task.title)
                .activatable(true)
                .build();
            let mut subtitle: Vec<String> = vec![task.status.clone()];
            if let Some(due) = task.due_date {
                subtitle.push(format!("due {due}"));
            }
            if let Some(p) = &task.priority {
                subtitle.push(p.clone());
            }
            row.set_subtitle(&subtitle.join(" · "));

            if task.pomodoro_count > 0 {
                let badge = gtk::Label::new(Some(&format!("{} 🍅", task.pomodoro_count)));
                badge.add_css_class("dim-label");
                row.add_suffix(&badge);
            }
            if task.in_conflict() {
                let icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
                icon.set_tooltip_text(Some("Sync conflict — open task to resolve"));
                row.add_suffix(&icon);
            } else if task.dirty {
                let icon = gtk::Image::from_icon_name("document-edit-symbolic");
                icon.set_tooltip_text(Some("Local changes pending sync"));
                row.add_suffix(&icon);
            }

            let list_row = gtk::ListBoxRow::builder()
                .child(&row)
                .name(&task.id)
                .build();
            self.list.append(&list_row);
            if Some(&task.id) == selected_id.as_ref() {
                self.list.select_row(Some(&list_row));
            }
        }
    }

    /// Create a local task from the entry text.
    pub fn add_task_from_entry(&self, state: &Shared) -> Option<String> {
        let title = self.new_task_entry.text().to_string();
        if title.trim().is_empty() {
            return None;
        }
        let task = task_service::create(&state.conn, &title, Utc::now()).ok()?;
        self.new_task_entry.set_text("");
        Some(task.id)
    }
}
