use crate::application::settings_service::{self, Settings};
use crate::application::timer_service::TimerService;
use crate::infrastructure::sqlite;
use adw::prelude::*;
use rusqlite::Connection;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

pub const APP_ID: &str = "com.frjr17.NotionPomodoroTracker";

/// Main-thread application state shared by all UI components. The background
/// sync thread never touches this — it opens its own SQLite connection.
pub struct AppState {
    pub conn: Connection,
    pub db_path: PathBuf,
    pub settings: RefCell<Settings>,
    pub timer: RefCell<TimerService>,
    pub selected_task_id: RefCell<Option<String>>,
    pub syncing: Cell<bool>,
    /// Message of the most recent failed sync, cleared on the next attempt.
    pub last_sync_error: RefCell<Option<String>>,
}

pub type Shared = Rc<AppState>;

impl AppState {
    pub fn init() -> Result<Shared, Box<dyn std::error::Error>> {
        let db_path = sqlite::default_db_path();
        let conn = sqlite::open(&db_path)?;
        let settings = settings_service::load(&conn)?;
        let timer = TimerService::restore(&conn, settings.timer)?;
        Ok(Rc::new(AppState {
            conn,
            db_path,
            settings: RefCell::new(settings),
            timer: RefCell::new(timer),
            selected_task_id: RefCell::new(None),
            syncing: Cell::new(false),
            last_sync_error: RefCell::new(None),
        }))
    }
}

pub fn run() -> gtk::glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(|app| match AppState::init() {
        Ok(state) => crate::ui::windows::main_window::build(app, state).present(),
        Err(e) => {
            eprintln!("failed to initialize app: {e}");
            app.quit();
        }
    });
    app.run()
}
