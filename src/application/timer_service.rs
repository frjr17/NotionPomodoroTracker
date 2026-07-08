//! Drives the domain timer against the database: persists timer state on
//! every transition and records pomodoros/sessions when work completes.

use crate::application::settings_service;
use crate::domain::pomodoro::{Timer, TimerConfig, TimerEvent};
use crate::domain::time_session::{SessionKind, TimeSession};
use crate::infrastructure::sqlite::{StoreResult, session_repo, task_repo};
use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;

pub struct TimerService {
    pub timer: Timer,
    pub config: TimerConfig,
}

impl TimerService {
    /// Restore persisted timer state (survives app restarts).
    pub fn restore(conn: &Connection, config: TimerConfig) -> StoreResult<Self> {
        Ok(Self {
            timer: settings_service::load_timer_state(conn)?,
            config,
        })
    }

    pub fn start(
        &mut self,
        conn: &Connection,
        task_id: &str,
        now: DateTime<Utc>,
    ) -> StoreResult<()> {
        self.timer.start(task_id, now);
        self.persist(conn)
    }

    pub fn pause(&mut self, conn: &Connection, now: DateTime<Utc>) -> StoreResult<()> {
        self.timer.pause(now);
        self.persist(conn)
    }

    pub fn resume(&mut self, conn: &Connection, now: DateTime<Utc>) -> StoreResult<()> {
        self.timer.resume(now);
        self.persist(conn)
    }

    /// Stop and record any focused minutes as a partial session.
    pub fn stop(&mut self, conn: &Connection, now: DateTime<Utc>) -> StoreResult<()> {
        let started = self.timer.work_started_at();
        let task_id = self.timer.task_id.clone();
        let minutes = self.timer.stop(now);
        if minutes > 0
            && let Some(task_id) = task_id
            // The task may have been removed by a sync (deleted in Notion)
            // while the timer was running; nothing left to credit then.
            && task_repo::get(conn, &task_id)?.is_some()
        {
            let session = TimeSession::new(
                &task_id,
                started.unwrap_or(now - Duration::minutes(minutes)),
                now,
                minutes,
                SessionKind::Pomodoro,
            );
            session_repo::insert(conn, &session)?;
            task_repo::add_progress(conn, &task_id, 0, minutes, now)?;
        }
        self.persist(conn)
    }

    pub fn tick(
        &mut self,
        conn: &Connection,
        now: DateTime<Utc>,
    ) -> StoreResult<Option<TimerEvent>> {
        let started = self.timer.work_started_at();
        let event = self.timer.tick(now, &self.config);
        self.handle_event(conn, &event, started, now)?;
        Ok(event)
    }

    pub fn complete_pomodoro(
        &mut self,
        conn: &Connection,
        now: DateTime<Utc>,
    ) -> StoreResult<Option<TimerEvent>> {
        let started = self.timer.work_started_at();
        let event = self.timer.complete_pomodoro(now, &self.config);
        self.handle_event(conn, &event, started, now)?;
        Ok(event)
    }

    fn handle_event(
        &mut self,
        conn: &Connection,
        event: &Option<TimerEvent>,
        work_started: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> StoreResult<()> {
        if let Some(TimerEvent::PomodoroCompleted { task_id, minutes }) = event
            && !task_id.is_empty()
            && task_repo::get(conn, task_id)?.is_some()
        {
            let session = TimeSession::new(
                task_id,
                work_started.unwrap_or(now - Duration::minutes(*minutes)),
                now,
                *minutes,
                SessionKind::Pomodoro,
            );
            session_repo::insert(conn, &session)?;
            task_repo::add_progress(conn, task_id, 1, *minutes, now)?;
        }
        if event.is_some() {
            self.persist(conn)?;
        }
        Ok(())
    }

    fn persist(&self, conn: &Connection) -> StoreResult<()> {
        settings_service::save_timer_state(conn, &self.timer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::task_service;
    use crate::domain::pomodoro::{Phase, RunState};
    use crate::infrastructure::sqlite::open_in_memory;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 9, 0, 0).unwrap()
    }

    #[test]
    fn completed_pomodoro_updates_task_and_records_session() {
        let conn = open_in_memory().unwrap();
        let task = task_service::create(&conn, "T", t0()).unwrap();
        let mut svc = TimerService::restore(&conn, TimerConfig::default()).unwrap();

        svc.start(&conn, &task.id, t0()).unwrap();
        let event = svc.tick(&conn, t0() + Duration::minutes(25)).unwrap();
        assert!(matches!(event, Some(TimerEvent::PomodoroCompleted { .. })));

        let task = task_service::get(&conn, &task.id).unwrap().unwrap();
        assert_eq!(task.pomodoro_count, 1);
        assert_eq!(task.tracked_minutes, 25);
        assert!(task.dirty);
        let sessions = task_service::sessions(&conn, &task.id).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].minutes, 25);
        assert_eq!(sessions[0].started_at, t0());
    }

    #[test]
    fn timer_state_survives_restart() {
        let conn = open_in_memory().unwrap();
        let task = task_service::create(&conn, "T", t0()).unwrap();
        {
            let mut svc = TimerService::restore(&conn, TimerConfig::default()).unwrap();
            svc.start(&conn, &task.id, t0()).unwrap();
            svc.pause(&conn, t0() + Duration::minutes(10)).unwrap();
        }
        // "Reopen" the app.
        let svc = TimerService::restore(&conn, TimerConfig::default()).unwrap();
        assert_eq!(svc.timer.run_state, RunState::Paused);
        assert_eq!(
            svc.timer.remaining(t0() + Duration::hours(5), &svc.config),
            Duration::minutes(15)
        );
    }

    #[test]
    fn stop_records_partial_session_without_pomodoro_credit() {
        let conn = open_in_memory().unwrap();
        let task = task_service::create(&conn, "T", t0()).unwrap();
        let mut svc = TimerService::restore(&conn, TimerConfig::default()).unwrap();
        svc.start(&conn, &task.id, t0()).unwrap();
        svc.stop(&conn, t0() + Duration::minutes(9)).unwrap();

        let task = task_service::get(&conn, &task.id).unwrap().unwrap();
        assert_eq!(task.pomodoro_count, 0);
        assert_eq!(task.tracked_minutes, 9);
        assert_eq!(task_service::sessions(&conn, &task.id).unwrap().len(), 1);
        assert_eq!(svc.timer.run_state, RunState::Idle);
    }

    #[test]
    fn break_runs_after_completion() {
        let conn = open_in_memory().unwrap();
        let task = task_service::create(&conn, "T", t0()).unwrap();
        let mut svc = TimerService::restore(&conn, TimerConfig::default()).unwrap();
        svc.start(&conn, &task.id, t0()).unwrap();
        svc.tick(&conn, t0() + Duration::minutes(25)).unwrap();
        assert_eq!(svc.timer.phase, Phase::ShortBreak);
        let event = svc.tick(&conn, t0() + Duration::minutes(30)).unwrap();
        assert_eq!(event, Some(TimerEvent::BreakFinished { was_long: false }));
        // Break completion must not add pomodoros.
        let task = task_service::get(&conn, &task.id).unwrap().unwrap();
        assert_eq!(task.pomodoro_count, 1);
    }
}
