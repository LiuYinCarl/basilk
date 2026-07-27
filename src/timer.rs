use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub enum TimerKind {
    /// Task-bound, open-ended; accumulates into the task's `time_spent_secs`.
    Stopwatch,
    /// Global pomodoro; not bound to any task, nothing is persisted.
    Countdown,
}

/// The task a stopwatch timer accumulates time into (list indexes shift on
/// sort, so the title is the stable identity, mirroring `Task::load_items`).
#[derive(Debug, Clone)]
pub struct TimerTaskBinding {
    pub project_index: usize,
    pub task_title: String,
}

/// Runtime state of a timer. Not persisted; a stopwatch settles its elapsed
/// seconds into the bound task, a countdown (pomodoro) never accumulates
/// anything — at zero it rings the bell once and stays in the finished
/// state until the user dismisses it.
#[derive(Debug, Clone)]
pub struct TimerState {
    pub kind: TimerKind,
    /// Countdown target in seconds (unused for stopwatch).
    pub target_secs: u64,
    /// Time accumulated across pause/resume cycles.
    pub accumulated: Duration,
    /// `Some` while the timer is running.
    pub started_at: Option<Instant>,
    /// Task binding (stopwatch only).
    pub bound: Option<TimerTaskBinding>,
    /// Countdown only: set once the terminal bell has been rung at zero,
    /// so it rings exactly once while the finished timer stays visible.
    pub rung: bool,
}

impl TimerState {
    pub fn new_stopwatch(project_index: usize, task_title: String) -> Self {
        Self {
            kind: TimerKind::Stopwatch,
            target_secs: 0,
            accumulated: Duration::ZERO,
            started_at: Some(Instant::now()),
            bound: Some(TimerTaskBinding {
                project_index,
                task_title,
            }),
            rung: false,
        }
    }

    pub fn new_countdown(target_secs: u64) -> Self {
        Self {
            kind: TimerKind::Countdown,
            target_secs,
            accumulated: Duration::ZERO,
            started_at: Some(Instant::now()),
            bound: None,
            rung: false,
        }
    }

    /// Whether this timer is bound to the given task.
    pub fn is_bound_to(&self, project_index: usize, task_title: &str) -> bool {
        self.bound
            .as_ref()
            .is_some_and(|b| b.project_index == project_index && b.task_title == task_title)
    }

    pub fn elapsed(&self) -> Duration {
        let running = self
            .started_at
            .map(|t| t.elapsed())
            .unwrap_or(Duration::ZERO);
        self.accumulated + running
    }

    /// `None` for stopwatch; `Some(remaining)` for countdown (zero once done).
    pub fn remaining(&self) -> Option<Duration> {
        if self.kind != TimerKind::Countdown {
            return None;
        }
        let target = Duration::from_secs(self.target_secs);
        Some(target.saturating_sub(self.elapsed()))
    }

    pub fn is_running(&self) -> bool {
        self.started_at.is_some()
    }

    /// `true` once a countdown has run down to zero (stopwatches never finish).
    pub fn is_finished(&self) -> bool {
        self.remaining() == Some(Duration::ZERO)
    }

    pub fn pause(&mut self) {
        if let Some(started) = self.started_at.take() {
            self.accumulated += started.elapsed();
        }
    }

    pub fn resume(&mut self) {
        if self.started_at.is_none() {
            self.started_at = Some(Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paused_timer(kind: TimerKind, accumulated_secs: u64, target_secs: u64) -> TimerState {
        let bound = if kind == TimerKind::Stopwatch {
            Some(TimerTaskBinding {
                project_index: 0,
                task_title: "t".to_string(),
            })
        } else {
            None
        };
        TimerState {
            kind,
            target_secs,
            accumulated: Duration::from_secs(accumulated_secs),
            started_at: None,
            bound,
            rung: false,
        }
    }

    #[test]
    fn new_timers_start_running() {
        assert!(TimerState::new_stopwatch(0, "t".to_string()).is_running());
        assert!(TimerState::new_countdown(1500).is_running());
    }

    #[test]
    fn stopwatch_is_bound_and_countdown_is_global() {
        let stopwatch = TimerState::new_stopwatch(1, "t".to_string());
        assert!(stopwatch.is_bound_to(1, "t"));
        assert!(!stopwatch.is_bound_to(0, "t"));
        assert!(!stopwatch.is_bound_to(1, "other"));

        let countdown = TimerState::new_countdown(60);
        assert!(countdown.bound.is_none());
        assert!(!countdown.is_bound_to(0, "t"));
    }

    #[test]
    fn countdown_stores_target_secs() {
        let timer = TimerState::new_countdown(1500);
        assert_eq!(timer.target_secs, 1500);
    }

    #[test]
    fn elapsed_is_accumulated_when_paused() {
        let timer = paused_timer(TimerKind::Stopwatch, 90, 0);
        assert_eq!(timer.elapsed(), Duration::from_secs(90));
    }

    #[test]
    fn pause_freezes_elapsed_and_resume_continues() {
        let mut timer = TimerState::new_stopwatch(0, "t".to_string());
        timer.pause();
        let frozen = timer.elapsed();
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(timer.elapsed(), frozen);
        assert!(!timer.is_running());

        timer.resume();
        assert!(timer.is_running());
        std::thread::sleep(Duration::from_millis(20));
        assert!(timer.elapsed() >= frozen);
    }

    #[test]
    fn remaining_is_none_for_stopwatch() {
        let timer = paused_timer(TimerKind::Stopwatch, 90, 0);
        assert_eq!(timer.remaining(), None);
    }

    #[test]
    fn remaining_counts_down_to_zero() {
        let timer = paused_timer(TimerKind::Countdown, 60, 90);
        assert_eq!(timer.remaining(), Some(Duration::from_secs(30)));

        let overtime = paused_timer(TimerKind::Countdown, 120, 90);
        assert_eq!(overtime.remaining(), Some(Duration::ZERO));
    }

    #[test]
    fn only_a_run_down_countdown_is_finished() {
        let done = paused_timer(TimerKind::Countdown, 120, 90);
        assert!(done.is_finished());

        let running = paused_timer(TimerKind::Countdown, 60, 90);
        assert!(!running.is_finished());

        let stopwatch = paused_timer(TimerKind::Stopwatch, 120, 0);
        assert!(!stopwatch.is_finished());
    }
}
