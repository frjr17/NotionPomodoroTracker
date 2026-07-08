//! Main window: task list sidebar, timer + task detail content, sync status
//! in the header. All signal wiring lives here; components stay dumb.

use crate::application::task_service;
use crate::domain::pomodoro::TimerEvent;
use crate::infrastructure::notifications;
use crate::ui::app::Shared;
use crate::ui::components::conflict_dialog;
use crate::ui::components::sync_controller::{self, SyncOutcome};
use crate::ui::components::task_detail::TaskDetail;
use crate::ui::components::task_list::TaskList;
use crate::ui::components::timer_view::TimerView;
use crate::ui::windows::settings_window;
use adw::prelude::*;
use chrono::Utc;
use std::rc::Rc;

struct Ui {
    state: Shared,
    window: adw::ApplicationWindow,
    title: adw::WindowTitle,
    toasts: adw::ToastOverlay,
    task_list: TaskList,
    detail: TaskDetail,
    timer_view: TimerView,
    sync_spinner: gtk::Spinner,
}

impl Ui {
    fn refresh_all(&self) {
        self.task_list.refresh_filter_options(&self.state);
        self.task_list.refresh(&self.state);
        self.detail.refresh(&self.state);
        self.timer_view.refresh(&self.state);
        self.refresh_sync_status();
    }

    fn refresh_sync_status(&self) {
        let status = sync_controller::current_status(&self.state);
        self.title.set_subtitle(&status.label());
        if self.state.syncing.get() {
            self.sync_spinner.start();
        } else {
            self.sync_spinner.stop();
        }
    }

    fn toast(&self, message: &str) {
        self.toasts.add_toast(adw::Toast::new(message));
    }
}

pub fn build(app: &adw::Application, state: Shared) -> adw::ApplicationWindow {
    load_css();

    let task_list = TaskList::new();
    let detail = TaskDetail::new();
    let timer_view = TimerView::new();

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    content.append(&timer_view.root);
    content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    content.append(&detail.root);
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&content));

    let split = adw::OverlaySplitView::builder()
        .sidebar(&task_list.root)
        .content(&toasts)
        .min_sidebar_width(260.0)
        .build();

    let title = adw::WindowTitle::new("Notion Pomodoro Tracker", "");
    let header = adw::HeaderBar::builder().title_widget(&title).build();
    let sync_btn = gtk::Button::builder()
        .icon_name("emblem-synchronizing-symbolic")
        .tooltip_text("Sync Now")
        .build();
    let sync_spinner = gtk::Spinner::new();
    header.pack_start(&sync_btn);
    header.pack_start(&sync_spinner);
    let settings_btn = gtk::Button::builder()
        .icon_name("emblem-system-symbolic")
        .tooltip_text("Settings")
        .build();
    header.pack_end(&settings_btn);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&split));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Notion Pomodoro Tracker")
        .default_width(1000)
        .default_height(680)
        .content(&toolbar)
        .build();

    let ui = Rc::new(Ui {
        state: state.clone(),
        window: window.clone(),
        title,
        toasts,
        task_list,
        detail,
        timer_view,
        sync_spinner,
    });

    wire_task_list(&ui);
    wire_timer(&ui, app);
    wire_detail(&ui);
    wire_sync(&ui, app, &sync_btn);
    settings_btn.connect_clicked({
        let ui = ui.clone();
        move |_| {
            let ui = ui.clone();
            settings_window::show(&ui.window.clone(), &ui.state.clone(), move || {
                ui.refresh_all()
            });
        }
    });

    ui.refresh_all();
    window
}

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(
        ".timer-display { font-size: 64px; font-weight: 300; font-feature-settings: 'tnum'; }",
    );
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn wire_task_list(ui: &Rc<Ui>) {
    let list = &ui.task_list;

    list.list.connect_row_selected({
        let ui = ui.clone();
        move |_, row| {
            let id = row.map(|r| r.widget_name().to_string());
            if *ui.state.selected_task_id.borrow() == id {
                return;
            }
            *ui.state.selected_task_id.borrow_mut() = id;
            ui.detail.refresh(&ui.state);
            ui.timer_view.refresh(&ui.state);
        }
    });

    list.search.connect_search_changed({
        let ui = ui.clone();
        move |_| ui.task_list.refresh(&ui.state)
    });
    for dd in [&list.status_filter, &list.priority_filter] {
        dd.connect_selected_notify({
            let ui = ui.clone();
            move |_| ui.task_list.refresh(&ui.state)
        });
    }
    list.show_done.connect_toggled({
        let ui = ui.clone();
        move |_| ui.task_list.refresh(&ui.state)
    });

    let add = {
        let ui = ui.clone();
        move || {
            if let Some(id) = ui.task_list.add_task_from_entry(&ui.state) {
                *ui.state.selected_task_id.borrow_mut() = Some(id);
                ui.refresh_all();
            }
        }
    };
    let add = Rc::new(add);
    list.new_task_entry.connect_activate({
        let add = add.clone();
        move |_| add()
    });
    list.new_task_entry.connect_icon_press(move |_, _| add());
}

fn wire_timer(ui: &Rc<Ui>, app: &adw::Application) {
    let tv = &ui.timer_view;

    tv.start_btn.connect_clicked({
        let ui = ui.clone();
        move |_| {
            let task_id = ui.state.selected_task_id.borrow().clone();
            if let Some(task_id) = task_id {
                let result =
                    ui.state
                        .timer
                        .borrow_mut()
                        .start(&ui.state.conn, &task_id, Utc::now());
                if let Err(e) = result {
                    ui.toast(&format!("Could not start timer: {e}"));
                }
            }
            ui.timer_view.refresh(&ui.state);
        }
    });
    tv.pause_btn.connect_clicked({
        let ui = ui.clone();
        move |_| {
            let _ = ui
                .state
                .timer
                .borrow_mut()
                .pause(&ui.state.conn, Utc::now());
            ui.timer_view.refresh(&ui.state);
        }
    });
    tv.resume_btn.connect_clicked({
        let ui = ui.clone();
        move |_| {
            let _ = ui
                .state
                .timer
                .borrow_mut()
                .resume(&ui.state.conn, Utc::now());
            ui.timer_view.refresh(&ui.state);
        }
    });
    tv.stop_btn.connect_clicked({
        let ui = ui.clone();
        move |_| {
            let result = ui.state.timer.borrow_mut().stop(&ui.state.conn, Utc::now());
            if let Err(e) = result {
                ui.toast(&format!("Could not stop timer: {e}"));
            }
            ui.refresh_all();
        }
    });
    tv.complete_btn.connect_clicked({
        let ui = ui.clone();
        let app = app.clone();
        move |_| {
            let event = ui
                .state
                .timer
                .borrow_mut()
                .complete_pomodoro(&ui.state.conn, Utc::now());
            handle_timer_event(&ui, &app, event);
            ui.refresh_all();
        }
    });

    // Once-a-second redraw + state machine advance. Remaining time is
    // timestamp-derived, so missed ticks (suspend, minimize) cost nothing.
    gtk::glib::timeout_add_seconds_local(1, {
        let ui = ui.clone();
        let app = app.clone();
        move || {
            let event = ui.state.timer.borrow_mut().tick(&ui.state.conn, Utc::now());
            if matches!(event, Ok(Some(_))) {
                handle_timer_event(&ui, &app, event);
                ui.refresh_all();
            } else {
                ui.timer_view.refresh(&ui.state);
            }
            gtk::glib::ControlFlow::Continue
        }
    });
}

fn handle_timer_event(
    ui: &Rc<Ui>,
    app: &adw::Application,
    event: Result<Option<TimerEvent>, impl std::fmt::Display>,
) {
    match event {
        Ok(Some(TimerEvent::PomodoroCompleted { task_id, minutes })) => {
            let title = task_service::get(&ui.state.conn, &task_id)
                .ok()
                .flatten()
                .map(|t| t.title)
                .unwrap_or_else(|| "task".into());
            notifications::notify(
                app,
                "pomodoro-done",
                "Pomodoro complete 🍅",
                &format!("{minutes} focused minutes on “{title}”. Break time!"),
            );
        }
        Ok(Some(TimerEvent::BreakFinished { was_long })) => {
            notifications::notify(
                app,
                "break-done",
                if was_long {
                    "Long break over"
                } else {
                    "Break over"
                },
                "Ready for the next pomodoro?",
            );
        }
        Ok(None) => {}
        Err(e) => ui.toast(&format!("Timer error: {e}")),
    }
}

fn wire_detail(ui: &Rc<Ui>) {
    ui.detail.done_btn.connect_clicked({
        let ui = ui.clone();
        move |_| {
            let id = ui.state.selected_task_id.borrow().clone();
            if let Some(id) = id {
                if let Ok(Some(task)) = task_service::get(&ui.state.conn, &id) {
                    let _ = task_service::set_done(&ui.state.conn, &id, !task.done, Utc::now());
                }
                ui.refresh_all();
            }
        }
    });

    ui.detail.add_time_btn.connect_clicked({
        let ui = ui.clone();
        move |_| show_add_time_dialog(&ui)
    });

    ui.detail.conflict_banner.connect_button_clicked({
        let ui = ui.clone();
        move |_| {
            let id = ui.state.selected_task_id.borrow().clone();
            let Some(task) =
                id.and_then(|id| task_service::get(&ui.state.conn, &id).ok().flatten())
            else {
                return;
            };
            let ui2 = ui.clone();
            conflict_dialog::show(&ui.window, &ui.state, &task, move || ui2.refresh_all());
        }
    });
}

fn show_add_time_dialog(ui: &Rc<Ui>) {
    let Some(task_id) = ui.state.selected_task_id.borrow().clone() else {
        return;
    };
    let spin = gtk::SpinButton::with_range(-600.0, 600.0, 5.0);
    spin.set_value(25.0);
    let dialog = adw::AlertDialog::builder()
        .heading("Add or correct tracked time")
        .body("Minutes to add (negative to subtract):")
        .extra_child(&spin)
        .build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("add", "Add Time");
    dialog.set_response_appearance("add", adw::ResponseAppearance::Suggested);
    dialog.set_close_response("cancel");
    let window = ui.window.clone();
    let ui = ui.clone();
    dialog.connect_response(Some("add"), move |_, _| {
        let minutes = spin.value() as i64;
        if minutes != 0 {
            if let Err(e) =
                task_service::add_manual_time(&ui.state.conn, &task_id, minutes, Utc::now())
            {
                ui.toast(&format!("Could not add time: {e}"));
            }
            ui.refresh_all();
        }
    });
    dialog.present(Some(&window));
}

fn wire_sync(ui: &Rc<Ui>, app: &adw::Application, sync_btn: &gtk::Button) {
    let do_sync: Rc<dyn Fn()> = Rc::new({
        let ui = ui.clone();
        let app = app.clone();
        move || {
            if ui.state.syncing.get() {
                return;
            }
            let ui2 = ui.clone();
            let app = app.clone();
            sync_controller::trigger(&ui.state, move |outcome| {
                match &outcome {
                    SyncOutcome::Done(report) => {
                        ui2.toast(&format!(
                            "Synced — {} new, {} updated, {} pushed",
                            report.pulled_new, report.pulled_updated, report.pushed
                        ));
                        if report.conflicts > 0 {
                            ui2.toast(&format!(
                                "{} task(s) in conflict — open them to resolve",
                                report.conflicts
                            ));
                        }
                        for err in &report.errors {
                            ui2.toast(err);
                        }
                    }
                    SyncOutcome::Failed(msg) => {
                        ui2.toast(&format!("Sync failed: {msg}"));
                        notifications::notify(&app, "sync-failed", "Sync failed", msg);
                    }
                }
                ui2.refresh_all();
            });
            ui.refresh_sync_status();
        }
    });

    sync_btn.connect_clicked({
        let do_sync = do_sync.clone();
        move |_| do_sync()
    });

    // Auto-sync: checked every 5 minutes; only fires when enabled in settings.
    gtk::glib::timeout_add_seconds_local(300, {
        let ui = ui.clone();
        move || {
            if ui.state.settings.borrow().auto_sync_enabled && !ui.state.syncing.get() {
                do_sync();
            }
            gtk::glib::ControlFlow::Continue
        }
    });
}
