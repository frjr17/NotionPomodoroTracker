//! Pure Pomodoro timer state machine. No I/O, no wall clock: every method
//! that depends on time takes `now`, so it is fully testable and immune to
//! the app being minimized or suspended (remaining time is derived from
//! timestamps, never from tick counting).

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TimerConfig {
    pub pomodoro_minutes: u32,
    pub short_break_minutes: u32,
    pub long_break_minutes: u32,
    pub pomodoros_until_long_break: u32,
}

impl Default for TimerConfig {
    fn default() -> Self {
        Self {
            pomodoro_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            pomodoros_until_long_break: 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Work,
    ShortBreak,
    LongBreak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunState {
    Idle,
    Running,
    Paused,
}

/// Events emitted by `tick`/`complete_pomodoro` for the caller to act on
/// (persist session, notify, etc.).
#[derive(Debug, Clone, PartialEq)]
pub enum TimerEvent {
    PomodoroCompleted { task_id: String, minutes: i64 },
    BreakFinished { was_long: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Timer {
    pub phase: Phase,
    pub run_state: RunState,
    pub task_id: Option<String>,
    /// Pomodoros completed in the current cycle (resets after a long break).
    pub cycle_count: u32,
    started_at: Option<DateTime<Utc>>,
    /// Seconds elapsed in the current phase before the last pause.
    accumulated_secs: i64,
    /// When the current work phase actually began (for session records).
    work_started_at: Option<DateTime<Utc>>,
}

impl Default for Timer {
    fn default() -> Self {
        Self {
            phase: Phase::Work,
            run_state: RunState::Idle,
            task_id: None,
            cycle_count: 0,
            started_at: None,
            accumulated_secs: 0,
            work_started_at: None,
        }
    }
}

impl Timer {
    pub fn phase_duration(&self, config: &TimerConfig) -> Duration {
        let minutes = match self.phase {
            Phase::Work => config.pomodoro_minutes,
            Phase::ShortBreak => config.short_break_minutes,
            Phase::LongBreak => config.long_break_minutes,
        };
        Duration::minutes(minutes as i64)
    }

    pub fn elapsed(&self, now: DateTime<Utc>) -> Duration {
        let running = match (self.run_state, self.started_at) {
            (RunState::Running, Some(start)) => now - start,
            _ => Duration::zero(),
        };
        Duration::seconds(self.accumulated_secs) + running
    }

    pub fn remaining(&self, now: DateTime<Utc>, config: &TimerConfig) -> Duration {
        (self.phase_duration(config) - self.elapsed(now)).max(Duration::zero())
    }

    /// Start a work phase on `task_id` from Idle (or restart after stop).
    pub fn start(&mut self, task_id: &str, now: DateTime<Utc>) {
        self.phase = Phase::Work;
        self.run_state = RunState::Running;
        self.task_id = Some(task_id.to_string());
        self.started_at = Some(now);
        self.accumulated_secs = 0;
        self.work_started_at = Some(now);
    }

    pub fn pause(&mut self, now: DateTime<Utc>) {
        if self.run_state != RunState::Running {
            return;
        }
        self.accumulated_secs = self.elapsed(now).num_seconds();
        self.started_at = None;
        self.run_state = RunState::Paused;
    }

    pub fn resume(&mut self, now: DateTime<Utc>) {
        if self.run_state != RunState::Paused {
            return;
        }
        self.started_at = Some(now);
        self.run_state = RunState::Running;
    }

    /// Abandon the current phase. Returns minutes of focused work to record
    /// (0 for breaks or sub-minute work).
    pub fn stop(&mut self, now: DateTime<Utc>) -> i64 {
        let focused = if self.phase == Phase::Work && self.run_state != RunState::Idle {
            self.elapsed(now).num_minutes()
        } else {
            0
        };
        *self = Timer {
            cycle_count: self.cycle_count,
            task_id: self.task_id.clone(),
            ..Timer::default()
        };
        focused
    }

    /// Advance the state machine if the current phase has run out.
    /// Call this once a second (or after wake-up); it is idempotent while
    /// time remains.
    pub fn tick(&mut self, now: DateTime<Utc>, config: &TimerConfig) -> Option<TimerEvent> {
        if self.run_state != RunState::Running || self.remaining(now, config) > Duration::zero() {
            return None;
        }
        match self.phase {
            Phase::Work => Some(self.finish_work(now, config)),
            Phase::ShortBreak | Phase::LongBreak => {
                let was_long = self.phase == Phase::LongBreak;
                self.phase = Phase::Work;
                self.run_state = RunState::Idle;
                self.started_at = None;
                self.accumulated_secs = 0;
                Some(TimerEvent::BreakFinished { was_long })
            }
        }
    }

    /// Manually complete the running/paused pomodoro early ("Complete
    /// Pomodoro" button). Counts as a full pomodoro with actual minutes.
    pub fn complete_pomodoro(
        &mut self,
        now: DateTime<Utc>,
        config: &TimerConfig,
    ) -> Option<TimerEvent> {
        if self.phase != Phase::Work || self.run_state == RunState::Idle {
            return None;
        }
        Some(self.finish_work(now, config))
    }

    fn finish_work(&mut self, now: DateTime<Utc>, config: &TimerConfig) -> TimerEvent {
        let minutes = self.elapsed(now).num_minutes();
        let task_id = self.task_id.clone().unwrap_or_default();
        self.cycle_count += 1;
        self.phase = if self
            .cycle_count
            .is_multiple_of(config.pomodoros_until_long_break)
        {
            Phase::LongBreak
        } else {
            Phase::ShortBreak
        };
        self.run_state = RunState::Running;
        self.started_at = Some(now);
        self.accumulated_secs = 0;
        self.work_started_at = None;
        TimerEvent::PomodoroCompleted { task_id, minutes }
    }

    pub fn work_started_at(&self) -> Option<DateTime<Utc>> {
        self.work_started_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 9, 0, 0).unwrap()
    }

    fn cfg() -> TimerConfig {
        TimerConfig::default()
    }

    #[test]
    fn starts_running_work_phase() {
        let mut timer = Timer::default();
        timer.start("task-1", t0());
        assert_eq!(timer.run_state, RunState::Running);
        assert_eq!(timer.phase, Phase::Work);
        assert_eq!(timer.remaining(t0(), &cfg()), Duration::minutes(25));
    }

    #[test]
    fn remaining_derives_from_wall_clock() {
        let mut timer = Timer::default();
        timer.start("task-1", t0());
        // 10 minutes pass with zero ticks (app minimized) — still accurate.
        let later = t0() + Duration::minutes(10);
        assert_eq!(timer.remaining(later, &cfg()), Duration::minutes(15));
    }

    #[test]
    fn pause_freezes_and_resume_continues() {
        let mut timer = Timer::default();
        timer.start("task-1", t0());
        timer.pause(t0() + Duration::minutes(5));
        // Paused for an hour: remaining unchanged.
        let after_pause = t0() + Duration::minutes(65);
        assert_eq!(timer.remaining(after_pause, &cfg()), Duration::minutes(20));
        timer.resume(after_pause);
        assert_eq!(
            timer.remaining(after_pause + Duration::minutes(20), &cfg()),
            Duration::zero()
        );
    }

    #[test]
    fn pause_when_not_running_is_noop() {
        let mut timer = Timer::default();
        timer.pause(t0());
        assert_eq!(timer.run_state, RunState::Idle);
        timer.resume(t0());
        assert_eq!(timer.run_state, RunState::Idle);
    }

    #[test]
    fn tick_completes_pomodoro_and_starts_short_break() {
        let mut timer = Timer::default();
        timer.start("task-1", t0());
        let end = t0() + Duration::minutes(25);
        assert_eq!(timer.tick(t0() + Duration::minutes(24), &cfg()), None);
        let event = timer.tick(end, &cfg()).unwrap();
        assert_eq!(
            event,
            TimerEvent::PomodoroCompleted {
                task_id: "task-1".into(),
                minutes: 25
            }
        );
        assert_eq!(timer.phase, Phase::ShortBreak);
        assert_eq!(timer.run_state, RunState::Running);
        assert_eq!(timer.cycle_count, 1);
        assert_eq!(timer.remaining(end, &cfg()), Duration::minutes(5));
    }

    #[test]
    fn long_break_after_four_pomodoros() {
        let mut timer = Timer::default();
        let mut now = t0();
        for i in 1..=4 {
            timer.start("task-1", now);
            now += Duration::minutes(25);
            timer.tick(now, &cfg()).unwrap();
            if i < 4 {
                assert_eq!(timer.phase, Phase::ShortBreak);
                now += Duration::minutes(5);
                assert_eq!(
                    timer.tick(now, &cfg()),
                    Some(TimerEvent::BreakFinished { was_long: false })
                );
            }
        }
        assert_eq!(timer.phase, Phase::LongBreak);
    }

    #[test]
    fn break_finish_returns_to_idle_work() {
        let mut timer = Timer::default();
        timer.start("task-1", t0());
        let mut now = t0() + Duration::minutes(25);
        timer.tick(now, &cfg());
        now += Duration::minutes(5);
        let event = timer.tick(now, &cfg()).unwrap();
        assert_eq!(event, TimerEvent::BreakFinished { was_long: false });
        assert_eq!(timer.phase, Phase::Work);
        assert_eq!(timer.run_state, RunState::Idle);
    }

    #[test]
    fn manual_complete_counts_actual_minutes() {
        let mut timer = Timer::default();
        timer.start("task-1", t0());
        let event = timer
            .complete_pomodoro(t0() + Duration::minutes(17), &cfg())
            .unwrap();
        assert_eq!(
            event,
            TimerEvent::PomodoroCompleted {
                task_id: "task-1".into(),
                minutes: 17
            }
        );
        assert_eq!(timer.phase, Phase::ShortBreak);
    }

    #[test]
    fn stop_returns_partial_work_minutes_and_resets() {
        let mut timer = Timer::default();
        timer.start("task-1", t0());
        let focused = timer.stop(t0() + Duration::minutes(12));
        assert_eq!(focused, 12);
        assert_eq!(timer.run_state, RunState::Idle);
        assert_eq!(timer.remaining(t0(), &cfg()), Duration::minutes(25));
    }

    #[test]
    fn stop_during_break_records_nothing() {
        let mut timer = Timer::default();
        timer.start("task-1", t0());
        timer.tick(t0() + Duration::minutes(25), &cfg());
        assert_eq!(timer.stop(t0() + Duration::minutes(27)), 0);
    }

    #[test]
    fn survives_serialization_round_trip() {
        let mut timer = Timer::default();
        timer.start("task-1", t0());
        timer.pause(t0() + Duration::minutes(3));
        let json = serde_json::to_string(&timer).unwrap();
        let restored: Timer = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, timer);
        assert_eq!(
            restored.remaining(t0() + Duration::hours(2), &cfg()),
            Duration::minutes(22)
        );
    }
}
