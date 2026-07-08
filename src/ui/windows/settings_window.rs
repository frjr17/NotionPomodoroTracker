//! Preferences dialog: timer durations, auto-sync, Notion connection setup
//! with live validation and property mapping.

use crate::application::settings_service::{self, PropertyMappings};
use crate::infrastructure::notion::NotionClient;
use crate::infrastructure::secret_store;
use crate::ui::app::Shared;
use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

struct MappingRow {
    row: adw::ComboRow,
    names: Rc<RefCell<Vec<String>>>,
    optional: bool,
}

impl MappingRow {
    fn new(title: &str, subtitle: &str, optional: bool) -> Self {
        let row = adw::ComboRow::builder()
            .title(title)
            .subtitle(subtitle)
            .build();
        Self {
            row,
            names: Rc::new(RefCell::new(vec![])),
            optional,
        }
    }

    /// Fill with property names (already type-filtered), preselecting
    /// `current` when present.
    fn set_options(&self, names: Vec<String>, current: &str) {
        let mut items: Vec<&str> = Vec::new();
        if self.optional {
            items.push("(none)");
        }
        items.extend(names.iter().map(String::as_str));
        self.row.set_model(Some(&gtk::StringList::new(&items)));
        let offset = if self.optional { 1 } else { 0 };
        if let Some(pos) = names.iter().position(|n| n == current) {
            self.row.set_selected((pos + offset) as u32);
        }
        *self.names.borrow_mut() = names;
    }

    /// Currently selected property name, or "" for (none)/never populated.
    fn selected_name(&self) -> String {
        let names = self.names.borrow();
        if names.is_empty() {
            return String::new();
        }
        let idx = self.row.selected() as usize;
        let offset = if self.optional { 1 } else { 0 };
        if idx < offset {
            return String::new();
        }
        names.get(idx - offset).cloned().unwrap_or_default()
    }
}

pub fn show(parent: &impl IsA<gtk::Widget>, state: &Shared, on_closed: impl Fn() + 'static) {
    let settings = state.settings.borrow().clone();
    let dialog = adw::PreferencesDialog::builder().title("Settings").build();

    // ---- Timer page ----
    let timer_page = adw::PreferencesPage::builder()
        .title("Timer")
        .icon_name("alarm-symbolic")
        .build();
    let durations = adw::PreferencesGroup::builder()
        .title("Durations (minutes)")
        .build();
    let spin = |title: &str, value: u32, max: f64| {
        let row = adw::SpinRow::with_range(1.0, max, 1.0);
        row.set_title(title);
        row.set_value(value as f64);
        row
    };
    let pomodoro_row = spin("Pomodoro length", settings.timer.pomodoro_minutes, 240.0);
    let short_row = spin("Short break", settings.timer.short_break_minutes, 240.0);
    let long_row = spin("Long break", settings.timer.long_break_minutes, 240.0);
    let cycle_row = spin(
        "Pomodoros until long break",
        settings.timer.pomodoros_until_long_break,
        12.0,
    );
    for r in [&pomodoro_row, &short_row, &long_row, &cycle_row] {
        durations.add(r);
    }
    timer_page.add(&durations);

    let sync_group = adw::PreferencesGroup::builder().title("Sync").build();
    let auto_sync_row = adw::SwitchRow::builder()
        .title("Auto-sync")
        .subtitle("Sync with Notion automatically every 5 minutes")
        .active(settings.auto_sync_enabled)
        .build();
    sync_group.add(&auto_sync_row);
    timer_page.add(&sync_group);
    dialog.add(&timer_page);

    // ---- Notion page ----
    let notion_page = adw::PreferencesPage::builder()
        .title("Notion")
        .icon_name("network-server-symbolic")
        .build();
    let conn_group = adw::PreferencesGroup::builder()
        .title("Connection")
        .description("Create an internal integration and share your task database with it — see NOTION_SETUP.md")
        .build();

    let token_row = adw::PasswordEntryRow::builder()
        .title("Integration token")
        .build();
    let has_stored_token = matches!(secret_store::load_token(), Ok(Some(t)) if !t.is_empty());
    if has_stored_token {
        // Never echo the stored secret back into the UI; blank means "keep".
        token_row.set_title("Integration token (stored in keyring — blank keeps it)");
    }
    let db_id_row = adw::EntryRow::builder()
        .title("Database ID")
        .text(&settings.notion_database_id)
        .build();

    let validate_row = adw::ActionRow::builder()
        .title("Validate connection")
        .subtitle("Checks token + database and loads properties for mapping")
        .build();
    let validate_btn = gtk::Button::builder()
        .label("Validate")
        .valign(gtk::Align::Center)
        .css_classes(["suggested-action"])
        .build();
    let validate_spinner = gtk::Spinner::new();
    validate_row.add_suffix(&validate_spinner);
    validate_row.add_suffix(&validate_btn);

    conn_group.add(&token_row);
    conn_group.add(&db_id_row);
    conn_group.add(&validate_row);
    notion_page.add(&conn_group);

    let map_group = adw::PreferencesGroup::builder()
        .title("Property mappings")
        .description("Validate the connection to load your database's properties")
        .build();
    let rows = Rc::new(MappingRows {
        title: MappingRow::new("Task title", "Title property", false),
        status: MappingRow::new("Status", "Status or Select property", false),
        due: MappingRow::new("Due date", "Date property (optional)", true),
        priority: MappingRow::new("Priority", "Select property (optional)", true),
        pomodoros: MappingRow::new("Pomodoro count", "Number property (optional)", true),
        minutes: MappingRow::new("Tracked minutes", "Number property (optional)", true),
        last_synced: MappingRow::new("Last synced", "Date property (optional)", true),
        discovered: RefCell::new(vec![]),
    });
    for r in [
        &rows.title,
        &rows.status,
        &rows.due,
        &rows.priority,
        &rows.pomodoros,
        &rows.minutes,
        &rows.last_synced,
    ] {
        map_group.add(&r.row);
    }
    notion_page.add(&map_group);
    dialog.add(&notion_page);

    // ---- Validation flow ----
    let (tx, rx) = async_channel::bounded::<Result<Vec<(String, String)>, String>>(1);
    validate_btn.connect_clicked({
        let token_row = token_row.clone();
        let db_id_row = db_id_row.clone();
        let spinner = validate_spinner.clone();
        let tx = tx.clone();
        move |btn| {
            let token = if token_row.text().is_empty() {
                secret_store::load_token()
                    .ok()
                    .flatten()
                    .unwrap_or_default()
            } else {
                token_row.text().to_string()
            };
            let db_id = db_id_row.text().trim().to_string();
            if token.is_empty() || db_id.is_empty() {
                let _ = tx.send_blocking(Err("Enter a token and database ID first".into()));
                return;
            }
            btn.set_sensitive(false);
            spinner.start();
            let tx = tx.clone();
            std::thread::spawn(move || {
                let result = NotionClient::new(&token)
                    .database_properties(&db_id)
                    .map_err(|e| e.to_string());
                let _ = tx.send_blocking(result);
            });
        }
    });

    gtk::glib::spawn_future_local({
        let dialog = dialog.clone();
        let rows = rows.clone();
        let validate_btn = validate_btn.clone();
        let spinner = validate_spinner.clone();
        let current = settings.mappings.clone();
        async move {
            while let Ok(result) = rx.recv().await {
                validate_btn.set_sensitive(true);
                spinner.stop();
                match result {
                    Ok(props) => {
                        rows.populate(&props, &current);
                        dialog.add_toast(adw::Toast::new(&format!(
                            "Connected — {} properties loaded",
                            props.len()
                        )));
                    }
                    Err(e) => dialog.add_toast(adw::Toast::new(&format!("Validation failed: {e}"))),
                }
            }
        }
    });

    // ---- Save on close ----
    dialog.connect_closed({
        let state = state.clone();
        move |_| {
            let mut settings = state.settings.borrow().clone();
            settings.timer.pomodoro_minutes = pomodoro_row.value() as u32;
            settings.timer.short_break_minutes = short_row.value() as u32;
            settings.timer.long_break_minutes = long_row.value() as u32;
            settings.timer.pomodoros_until_long_break = cycle_row.value() as u32;
            settings.auto_sync_enabled = auto_sync_row.is_active();
            settings.notion_database_id = db_id_row.text().trim().to_string();
            rows.apply_to(&mut settings.mappings);

            let token = token_row.text();
            if !token.trim().is_empty()
                && let Err(e) = secret_store::store_token(token.trim())
            {
                eprintln!("failed to store token: {e}");
            }
            if let Err(e) = settings_service::save(&state.conn, &settings) {
                eprintln!("failed to save settings: {e}");
            }
            *state.settings.borrow_mut() = settings.clone();
            state.timer.borrow_mut().config = settings.timer;
            on_closed();
        }
    });

    dialog.present(Some(parent));
}

struct MappingRows {
    title: MappingRow,
    status: MappingRow,
    due: MappingRow,
    priority: MappingRow,
    pomodoros: MappingRow,
    minutes: MappingRow,
    last_synced: MappingRow,
    discovered: RefCell<Vec<(String, String)>>,
}

impl MappingRows {
    fn populate(&self, props: &[(String, String)], current: &PropertyMappings) {
        let of_type = |types: &[&str]| -> Vec<String> {
            props
                .iter()
                .filter(|(_, t)| types.contains(&t.as_str()))
                .map(|(n, _)| n.clone())
                .collect()
        };
        self.title.set_options(of_type(&["title"]), &current.title);
        self.status
            .set_options(of_type(&["status", "select"]), &current.status);
        self.due.set_options(of_type(&["date"]), &current.due_date);
        self.priority
            .set_options(of_type(&["select"]), &current.priority);
        self.pomodoros
            .set_options(of_type(&["number"]), &current.pomodoro_count);
        self.minutes
            .set_options(of_type(&["number"]), &current.tracked_minutes);
        self.last_synced
            .set_options(of_type(&["date"]), &current.last_synced);
        *self.discovered.borrow_mut() = props.to_vec();
    }

    /// Overwrite mappings from the combo selections — only if properties were
    /// loaded this session (otherwise keep what was configured before).
    fn apply_to(&self, mappings: &mut PropertyMappings) {
        let discovered = self.discovered.borrow();
        if discovered.is_empty() {
            return;
        }
        mappings.title = self.title.selected_name();
        mappings.status = self.status.selected_name();
        mappings.due_date = self.due.selected_name();
        mappings.priority = self.priority.selected_name();
        mappings.pomodoro_count = self.pomodoros.selected_name();
        mappings.tracked_minutes = self.minutes.selected_name();
        mappings.last_synced = self.last_synced.selected_name();
        mappings.status_type = discovered
            .iter()
            .find(|(n, _)| *n == mappings.status)
            .map(|(_, t)| t.clone())
            .unwrap_or_else(|| "status".into());
    }
}
