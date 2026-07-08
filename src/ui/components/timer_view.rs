//! The Pomodoro timer panel: big countdown, phase, and controls.

use crate::domain::pomodoro::{Phase, RunState};
use crate::ui::app::Shared;
use adw::prelude::*;
use chrono::Utc;

pub struct TimerView {
    pub root: gtk::Box,
    pub phase_label: gtk::Label,
    pub time_label: gtk::Label,
    pub cycle_label: gtk::Label,
    pub start_btn: gtk::Button,
    pub pause_btn: gtk::Button,
    pub resume_btn: gtk::Button,
    pub stop_btn: gtk::Button,
    pub complete_btn: gtk::Button,
}

impl Default for TimerView {
    fn default() -> Self {
        Self::new()
    }
}

impl TimerView {
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .halign(gtk::Align::Center)
            .margin_top(24)
            .margin_bottom(12)
            .build();

        let phase_label = gtk::Label::builder()
            .css_classes(["title-4"])
            .label("Ready")
            .build();
        let time_label = gtk::Label::builder().label("25:00").build();
        time_label.add_css_class("timer-display");
        let cycle_label = gtk::Label::builder()
            .css_classes(["dim-label"])
            .label("")
            .build();

        let controls = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .halign(gtk::Align::Center)
            .build();
        let start_btn = gtk::Button::builder()
            .label("Start")
            .css_classes(["suggested-action", "pill"])
            .build();
        let pause_btn = gtk::Button::builder()
            .label("Pause")
            .css_classes(["pill"])
            .build();
        let resume_btn = gtk::Button::builder()
            .label("Resume")
            .css_classes(["suggested-action", "pill"])
            .build();
        let stop_btn = gtk::Button::builder()
            .label("Stop")
            .css_classes(["destructive-action", "pill"])
            .build();
        let complete_btn = gtk::Button::builder()
            .label("Complete Pomodoro")
            .css_classes(["pill"])
            .tooltip_text("Finish this pomodoro now and take the break")
            .build();
        for b in [
            &start_btn,
            &pause_btn,
            &resume_btn,
            &stop_btn,
            &complete_btn,
        ] {
            controls.append(b);
        }

        root.append(&phase_label);
        root.append(&time_label);
        root.append(&cycle_label);
        root.append(&controls);

        Self {
            root,
            phase_label,
            time_label,
            cycle_label,
            start_btn,
            pause_btn,
            resume_btn,
            stop_btn,
            complete_btn,
        }
    }

    /// Redraw from timer state. Called once a second and after every action.
    pub fn refresh(&self, state: &Shared) {
        let svc = state.timer.borrow();
        let timer = &svc.timer;
        let now = Utc::now();
        let remaining = timer.remaining(now, &svc.config).num_seconds();
        self.time_label
            .set_label(&format!("{:02}:{:02}", remaining / 60, remaining % 60));

        let has_task = state.selected_task_id.borrow().is_some();
        let (phase_text, in_work) = match (timer.phase, timer.run_state) {
            (_, RunState::Idle) => ("Ready".to_string(), false),
            (Phase::Work, RunState::Paused) => ("Focus — paused".into(), true),
            (Phase::Work, RunState::Running) => ("Focus".into(), true),
            (Phase::ShortBreak, _) => ("Short break".into(), false),
            (Phase::LongBreak, _) => ("Long break".into(), false),
        };
        self.phase_label.set_label(&phase_text);
        let until_long = svc.config.pomodoros_until_long_break;
        self.cycle_label.set_label(&format!(
            "{} of {} pomodoros until long break",
            timer.cycle_count % until_long,
            until_long
        ));

        let running = timer.run_state == RunState::Running;
        let paused = timer.run_state == RunState::Paused;
        self.start_btn
            .set_visible(timer.run_state == RunState::Idle);
        self.start_btn.set_sensitive(has_task);
        self.pause_btn.set_visible(running);
        self.resume_btn.set_visible(paused);
        self.stop_btn.set_visible(running || paused);
        self.complete_btn
            .set_visible((running || paused) && in_work);
    }
}
