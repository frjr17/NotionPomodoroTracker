//! Selected-task panel: editable header (title/status/priority/due), a
//! live-styled markdown description editor, progress, conflict banner, and
//! session history.

use crate::application::task_service;
use crate::domain::task::Task;
use crate::ui::app::Shared;
use adw::prelude::*;
use chrono::{Datelike, NaiveDate};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Callback invoked when a header field is edited (set by the main window).
type EditCallback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

pub struct TaskDetail {
    pub root: gtk::Box,
    pub conflict_banner: adw::Banner,
    pub title_entry: gtk::Entry,
    pub status_dd: gtk::DropDown,
    pub priority_dd: gtk::DropDown,
    pub due_button: gtk::MenuButton,
    pub description_view: gtk::TextView,
    pub progress_label: gtk::Label,
    pub sync_info_label: gtk::Label,
    pub done_btn: gtk::Button,
    pub add_time_btn: gtk::Button,
    pub sessions_list: gtk::ListBox,
    pub placeholder: adw::StatusPage,
    pub content: gtk::Box,

    due_calendar: gtk::Calendar,
    // Shared editing state used by the change handlers.
    updating: Rc<Cell<bool>>,
    selected_due: Rc<RefCell<Option<NaiveDate>>>,
    on_edit: EditCallback,
    // Parallel maps so a dropdown index resolves back to its value.
    status_values: Rc<RefCell<Vec<String>>>,
    priority_values: Rc<RefCell<Vec<Option<String>>>>,
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

        // --- Compact editable header: title + one row of controls ---------
        let title_entry = gtk::Entry::builder()
            .placeholder_text("Task title")
            .hexpand(true)
            .css_classes(["title-2"])
            .build();
        let status_dd = flat_dropdown("Status");
        let priority_dd = flat_dropdown("Priority");

        let due_calendar = gtk::Calendar::new();
        let due_clear = gtk::Button::builder()
            .label("Clear date")
            .css_classes(["flat"])
            .build();
        let due_popover_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .build();
        due_popover_box.append(&due_calendar);
        due_popover_box.append(&due_clear);
        let due_popover = gtk::Popover::builder().child(&due_popover_box).build();
        let due_button = gtk::MenuButton::builder()
            .label("No date")
            .popover(&due_popover)
            .tooltip_text("Due date")
            .css_classes(["flat"])
            .build();

        let controls = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        controls.append(&status_dd);
        controls.append(&priority_dd);
        controls.append(&due_button);

        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .build();
        header.append(&title_entry);
        header.append(&controls);

        // --- Markdown description editor ----------------------------------
        let desc_header = gtk::Label::builder()
            .label("Description")
            .css_classes(["heading"])
            .halign(gtk::Align::Start)
            .margin_top(6)
            .build();
        let description_view = gtk::TextView::builder()
            .wrap_mode(gtk::WrapMode::WordChar)
            .top_margin(8)
            .bottom_margin(8)
            .left_margin(8)
            .right_margin(8)
            .build();
        create_markdown_tags(&description_view.buffer());
        description_view.buffer().connect_changed(restyle_markdown);
        let desc_scroll = gtk::ScrolledWindow::builder()
            .child(&description_view)
            .min_content_height(110)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let desc_frame = gtk::Frame::builder().child(&desc_scroll).build();

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
        content.append(&header);
        content.append(&desc_header);
        content.append(&desc_frame);
        content.append(&progress_label);
        content.append(&sync_info_label);
        content.append(&actions);
        content.append(&sessions_header);
        content.append(&sessions_scroll);

        root.append(&placeholder);
        root.append(&content);

        let updating = Rc::new(Cell::new(false));
        let selected_due = Rc::new(RefCell::new(None::<NaiveDate>));
        let on_edit: EditCallback = Rc::new(RefCell::new(None));

        // Fires the caller-supplied save callback, unless we're mid-render.
        let fire: Rc<dyn Fn()> = Rc::new({
            let updating = updating.clone();
            let on_edit = on_edit.clone();
            move || {
                if updating.get() {
                    return;
                }
                if let Some(cb) = on_edit.borrow().as_ref() {
                    cb();
                }
            }
        });

        title_entry.connect_activate({
            let fire = fire.clone();
            move |_| fire()
        });
        // Also save the title when the entry loses focus. edit_fields is a
        // no-op when nothing changed, so a plain focus traversal is harmless.
        let title_focus = gtk::EventControllerFocus::new();
        title_focus.connect_leave({
            let fire = fire.clone();
            move |_| fire()
        });
        title_entry.add_controller(title_focus);
        status_dd.connect_selected_notify({
            let fire = fire.clone();
            move |_| fire()
        });
        priority_dd.connect_selected_notify({
            let fire = fire.clone();
            move |_| fire()
        });
        due_calendar.connect_day_selected({
            let updating = updating.clone();
            let selected_due = selected_due.clone();
            let due_button = due_button.clone();
            let fire = fire.clone();
            move |cal| {
                if updating.get() {
                    return;
                }
                let d = cal.date();
                let nd =
                    NaiveDate::from_ymd_opt(d.year(), d.month() as u32, d.day_of_month() as u32);
                *selected_due.borrow_mut() = nd;
                due_button.set_label(&due_label(nd));
                fire();
            }
        });
        due_clear.connect_clicked({
            let selected_due = selected_due.clone();
            let due_button = due_button.clone();
            let due_popover = due_popover.clone();
            let fire = fire.clone();
            move |_| {
                *selected_due.borrow_mut() = None;
                due_button.set_label(&due_label(None));
                due_popover.popdown();
                fire();
            }
        });

        Self {
            root,
            conflict_banner,
            title_entry,
            status_dd,
            priority_dd,
            due_button,
            description_view,
            progress_label,
            sync_info_label,
            done_btn,
            add_time_btn,
            sessions_list,
            placeholder,
            content,
            due_calendar,
            updating,
            selected_due,
            on_edit,
            status_values: Rc::new(RefCell::new(vec![])),
            priority_values: Rc::new(RefCell::new(vec![])),
        }
    }

    /// Register the callback invoked whenever a header field is edited.
    pub fn connect_edited(&self, f: impl Fn() + 'static) {
        *self.on_edit.borrow_mut() = Some(Rc::new(f));
    }

    /// Current header field values from the widgets.
    pub fn current_edit(&self) -> (String, String, Option<String>, Option<NaiveDate>) {
        let title = self.title_entry.text().to_string();
        let s_idx = self.status_dd.selected() as usize;
        let status = self
            .status_values
            .borrow()
            .get(s_idx)
            .cloned()
            .unwrap_or_default();
        let p_idx = self.priority_dd.selected() as usize;
        let priority = self.priority_values.borrow().get(p_idx).cloned().flatten();
        let due = *self.selected_due.borrow();
        (title, status, priority, due)
    }

    pub fn description_text(&self) -> String {
        let buffer = self.description_view.buffer();
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string()
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
        // Guard the change handlers while we set widget values programmatically.
        self.updating.set(true);

        self.conflict_banner.set_revealed(task.in_conflict());
        self.title_entry.set_text(&task.title);

        // Status choices: distinct values seen locally, plus the current one.
        let mut statuses = task_service::distinct_statuses(&state.conn).unwrap_or_default();
        if !statuses.iter().any(|s| s == &task.status) {
            statuses.push(task.status.clone());
        }
        statuses.sort();
        statuses.dedup();
        let items: Vec<&str> = statuses.iter().map(String::as_str).collect();
        self.status_dd
            .set_model(Some(&gtk::StringList::new(&items)));
        let s_idx = statuses.iter().position(|s| s == &task.status).unwrap_or(0);
        self.status_dd.set_selected(s_idx as u32);
        *self.status_values.borrow_mut() = statuses;

        // Priority choices: "None" plus distinct values, plus the current one.
        let mut priorities = task_service::distinct_priorities(&state.conn).unwrap_or_default();
        if let Some(p) = &task.priority
            && !priorities.contains(p)
        {
            priorities.push(p.clone());
        }
        priorities.sort();
        priorities.dedup();
        let mut items: Vec<&str> = vec!["None"];
        items.extend(priorities.iter().map(String::as_str));
        self.priority_dd
            .set_model(Some(&gtk::StringList::new(&items)));
        let p_idx = match &task.priority {
            Some(p) => priorities.iter().position(|x| x == p).map_or(0, |i| i + 1),
            None => 0,
        };
        self.priority_dd.set_selected(p_idx as u32);
        let mut pvals: Vec<Option<String>> = vec![None];
        pvals.extend(priorities.into_iter().map(Some));
        *self.priority_values.borrow_mut() = pvals;

        // Due date.
        *self.selected_due.borrow_mut() = task.due_date;
        self.due_button.set_label(&due_label(task.due_date));
        if let Some(d) = task.due_date
            && let Some(gd) = gtk::glib::DateTime::from_local(
                d.year(),
                d.month() as i32,
                d.day() as i32,
                0,
                0,
                0.0,
            )
            .ok()
        {
            self.due_calendar.select_day(&gd);
        }

        // Description — only reset the buffer when it actually differs, so we
        // don't clobber the cursor while the user is editing.
        let desc = task.description.clone().unwrap_or_default();
        let buffer = self.description_view.buffer();
        let current = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string();
        if current != desc {
            buffer.set_text(&desc);
        }

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

        self.updating.set(false);
    }
}

/// Button label for a due date (the plain date) or "No date".
fn due_label(due: Option<NaiveDate>) -> String {
    match due {
        Some(d) => d.format("%b %-d, %Y").to_string(),
        None => "No date".into(),
    }
}

/// A compact, flat dropdown (like the sidebar filters) for the header row.
fn flat_dropdown(tooltip: &str) -> gtk::DropDown {
    let dd = gtk::DropDown::builder().build();
    dd.add_css_class("flat");
    // Themes style button.flat, not dropdown.flat; flatten the inner button.
    if let Some(button) = dd.first_child() {
        button.add_css_class("flat");
    }
    dd.set_tooltip_text(Some(tooltip));
    dd
}

// ---------------------------------------------------------------------------
// Live markdown styling for the description TextView.
// ---------------------------------------------------------------------------

/// ponytail: markers use a fixed neutral gray that reads on both themes; a
/// fully theme-adaptive dim would query the widget's style context.
const MARKER_GRAY: &str = "#9a9a9a";

fn create_markdown_tags(buffer: &gtk::TextBuffer) {
    let table = buffer.tag_table();
    for tag in [
        gtk::TextTag::builder()
            .name("h1")
            .weight(700)
            .scale(1.6)
            .build(),
        gtk::TextTag::builder()
            .name("h2")
            .weight(700)
            .scale(1.35)
            .build(),
        gtk::TextTag::builder()
            .name("h3")
            .weight(700)
            .scale(1.15)
            .build(),
        gtk::TextTag::builder().name("bold").weight(700).build(),
        gtk::TextTag::builder()
            .name("italic")
            .style(gtk::pango::Style::Italic)
            .build(),
        gtk::TextTag::builder()
            .name("strike")
            .strikethrough(true)
            .build(),
        gtk::TextTag::builder()
            .name("mono")
            .family("monospace")
            .build(),
        gtk::TextTag::builder()
            .name("marker")
            .foreground(MARKER_GRAY)
            .build(),
        gtk::TextTag::builder()
            .name("quote")
            .style(gtk::pango::Style::Italic)
            .foreground(MARKER_GRAY)
            .build(),
    ] {
        table.add(&tag);
    }
}

/// Re-tag the whole buffer from a fresh markdown parse. ponytail: full re-tag
/// per edit with O(n) byte→char mapping — fine at task-note size.
fn restyle_markdown(buffer: &gtk::TextBuffer) {
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.remove_all_tags(&start, &end);
    let text = buffer.text(&start, &end, false).to_string();
    if text.is_empty() {
        return;
    }

    let apply = |name: &str, s: usize, e: usize| {
        if s >= e || e > text.len() {
            return;
        }
        let so = text[..s].chars().count() as i32;
        let eo = text[..e].chars().count() as i32;
        let si = buffer.iter_at_offset(so);
        let ei = buffer.iter_at_offset(eo);
        buffer.apply_tag_by_name(name, &si, &ei);
    };
    // Dim a fixed-length delimiter on both sides of an inline span.
    let dim_delims = |apply: &dyn Fn(&str, usize, usize), r: &std::ops::Range<usize>, n: usize| {
        apply("marker", r.start, r.start + n);
        apply("marker", r.end - n, r.end);
    };

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    for (event, range) in Parser::new_ext(&text, opts).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let tag = match level {
                    HeadingLevel::H1 => "h1",
                    HeadingLevel::H2 => "h2",
                    _ => "h3",
                };
                let prefix_end = heading_prefix_end(&text, range.start);
                apply("marker", range.start, prefix_end);
                apply(tag, prefix_end, range.end);
            }
            Event::Start(Tag::Strong) => {
                apply("bold", range.start, range.end);
                dim_delims(&apply, &range, 2);
            }
            Event::Start(Tag::Emphasis) => {
                apply("italic", range.start, range.end);
                dim_delims(&apply, &range, 1);
            }
            Event::Start(Tag::Strikethrough) => {
                apply("strike", range.start, range.end);
                dim_delims(&apply, &range, 2);
            }
            Event::Code(_) => apply("mono", range.start, range.end),
            Event::Start(Tag::CodeBlock(_)) => apply("mono", range.start, range.end),
            Event::Start(Tag::BlockQuote(_)) => apply("quote", range.start, range.end),
            Event::Start(Tag::Item) => {
                let prefix_end = list_prefix_end(&text, range.start);
                apply("marker", range.start, prefix_end);
            }
            _ => {}
        }
    }
}

/// Byte offset just past a heading's `#`s and following spaces.
fn heading_prefix_end(text: &str, start: usize) -> usize {
    let b = text.as_bytes();
    let mut i = start;
    while i < b.len() && b[i] == b'#' {
        i += 1;
    }
    while i < b.len() && b[i] == b' ' {
        i += 1;
    }
    i
}

/// Byte offset just past a list item's marker (`- `, `1. `, `- [x] `).
fn list_prefix_end(text: &str, start: usize) -> usize {
    let b = text.as_bytes();
    let mut i = start;
    while i < b.len() && b[i] == b' ' {
        i += 1;
    }
    if i < b.len() && matches!(b[i], b'-' | b'*' | b'+') {
        i += 1;
    } else {
        let digits = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i > digits && i < b.len() && matches!(b[i], b'.' | b')') {
            i += 1;
        } else {
            return start; // not actually a list marker
        }
    }
    while i < b.len() && b[i] == b' ' {
        i += 1;
    }
    // Optional task-list checkbox.
    if i + 2 < b.len() && b[i] == b'[' && b[i + 2] == b']' {
        i += 3;
        while i < b.len() && b[i] == b' ' {
            i += 1;
        }
    }
    i
}
