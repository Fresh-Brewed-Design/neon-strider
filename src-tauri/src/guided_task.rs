//! Guided tasks: a window asks Rust to hold the dashboard open and on top
//! while the user walks through something that needs it in view (a
//! GROUND_TRUTH run first). Rust owns the state, so no window keeps its own
//! flag: `GuidedTaskStart` and `GuidedTaskEnd` set and clear it, `GuidedTaskCancel`
//! (UNPIN) clears every task, and each task ends by itself after
//! `TASK_TIMEOUT`. The state lives only in memory, so a restart starts unpinned.
//!
//! The rules are pure (`GuidedTasks`); `dashboard.rs` applies them to the window.

use serde::{Deserialize, Serialize};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// A pinned task ends by itself this long after it was last started. The
/// longest guided task, a GROUND_TRUTH sweep over the whole speed list at a
/// few minutes per speed, fits well inside it. Past it, the pin is almost
/// surely left over from a window that went away without ending its task, and
/// a popup stuck on top of everything is worse than one that hides on blur.
pub const TASK_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Which guided task holds the dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum GuidedTask {
    /// A GROUND_TRUTH calibration run or sweep is open.
    GroundTruth,
}

/// The guided-task pin as the windows see it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct GuidedTaskSnapshot {
    /// The dashboard is held open and on top (some task is running).
    pub pinned: bool,
    /// The running tasks, oldest first.
    pub tasks: Vec<GuidedTask>,
    /// Seconds until the pin ends by itself (the latest task's timeout).
    pub unpin_in_s: Option<u32>,
}

/// The running tasks and when each was last started. A task started again
/// while running restarts its timeout rather than nesting a second copy, so
/// one `end` always ends it; different tasks nest and the pin holds until the
/// last one ends.
#[derive(Debug, Default)]
pub struct GuidedTasks {
    running: Vec<(GuidedTask, Instant)>,
}

impl GuidedTasks {
    pub const fn new() -> Self {
        Self {
            running: Vec::new(),
        }
    }

    pub fn start(&mut self, task: GuidedTask, now: Instant) {
        match self.running.iter_mut().find(|(t, _)| *t == task) {
            Some(entry) => entry.1 = now,
            None => self.running.push((task, now)),
        }
    }

    /// Ending a task that isn't running is fine: the window may send it after
    /// UNPIN or the timeout already ended it.
    pub fn end(&mut self, task: GuidedTask) {
        self.running.retain(|(t, _)| *t != task);
    }

    /// UNPIN: end every task.
    pub fn cancel(&mut self) {
        self.running.clear();
    }

    /// Drop the tasks past `TASK_TIMEOUT`. Returns whether any ended.
    pub fn expire(&mut self, now: Instant) -> bool {
        let before = self.running.len();
        self.running
            .retain(|(_, at)| now.saturating_duration_since(*at) < TASK_TIMEOUT);
        self.running.len() != before
    }

    pub fn pinned(&self) -> bool {
        !self.running.is_empty()
    }

    pub fn view(&self, now: Instant) -> GuidedTaskSnapshot {
        let latest = self.running.iter().map(|(_, at)| *at).max();
        GuidedTaskSnapshot {
            pinned: self.pinned(),
            tasks: self.running.iter().map(|(t, _)| *t).collect(),
            unpin_in_s: latest.map(|at| {
                let left = TASK_TIMEOUT.saturating_sub(now.saturating_duration_since(at));
                u32::try_from(left.as_secs()).unwrap_or(u32::MAX)
            }),
        }
    }
}

static TASKS: Mutex<GuidedTasks> = Mutex::new(GuidedTasks::new());

/// Run `f` on the shared state. Returns its result and whether the pin
/// changed (pinned before != pinned after), so the caller knows when to touch
/// the window.
fn with_tasks<T>(f: impl FnOnce(&mut GuidedTasks) -> T) -> (T, bool) {
    let mut tasks = TASKS.lock().unwrap_or_else(PoisonError::into_inner);
    let before = tasks.pinned();
    let out = f(&mut tasks);
    (out, tasks.pinned() != before)
}

/// Returns whether the pin changed.
pub fn start(task: GuidedTask) -> bool {
    log::info!("Guided task {:?} started; dashboard pinned", task);
    with_tasks(|t| t.start(task, Instant::now())).1
}

/// Returns whether the pin changed.
pub fn end(task: GuidedTask) -> bool {
    let changed = with_tasks(|t| t.end(task)).1;
    if changed {
        log::info!("Guided task {:?} ended; dashboard unpinned", task);
    }
    changed
}

/// Returns whether the pin changed.
pub fn cancel() -> bool {
    let changed = with_tasks(GuidedTasks::cancel).1;
    if changed {
        log::info!("Guided task pin cancelled (UNPIN)");
    }
    changed
}

/// Returns whether the pin changed.
pub fn expire() -> bool {
    let (ended, changed) = with_tasks(|t| t.expire(Instant::now()));
    if ended {
        log::info!(
            "Guided task timed out after {} min",
            TASK_TIMEOUT.as_secs() / 60
        );
    }
    changed
}

pub fn pinned() -> bool {
    with_tasks(|t| t.pinned()).0
}

pub fn snapshot() -> GuidedTaskSnapshot {
    with_tasks(|t| t.view(Instant::now())).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_unpinned() {
        let tasks = GuidedTasks::new();
        assert!(!tasks.pinned());
        assert_eq!(tasks.view(Instant::now()), GuidedTaskSnapshot::default());
    }

    #[test]
    fn start_pins_and_end_unpins() {
        let now = Instant::now();
        let mut tasks = GuidedTasks::new();
        tasks.start(GuidedTask::GroundTruth, now);
        assert!(tasks.pinned());
        let view = tasks.view(now);
        assert_eq!(view.tasks, vec![GuidedTask::GroundTruth]);
        assert_eq!(view.unpin_in_s, Some(30 * 60));
        tasks.end(GuidedTask::GroundTruth);
        assert!(!tasks.pinned());
        assert_eq!(tasks.view(now).unpin_in_s, None);
    }

    #[test]
    fn ending_a_task_that_is_not_running_is_harmless() {
        let mut tasks = GuidedTasks::new();
        tasks.end(GuidedTask::GroundTruth);
        assert!(!tasks.pinned());
    }

    #[test]
    fn a_second_start_of_the_same_task_needs_only_one_end() {
        let now = Instant::now();
        let mut tasks = GuidedTasks::new();
        tasks.start(GuidedTask::GroundTruth, now);
        tasks.start(GuidedTask::GroundTruth, now + Duration::from_secs(5));
        assert_eq!(tasks.view(now).tasks.len(), 1);
        tasks.end(GuidedTask::GroundTruth);
        assert!(!tasks.pinned());
    }

    #[test]
    fn a_second_start_restarts_the_timeout() {
        let t0 = Instant::now();
        let mut tasks = GuidedTasks::new();
        tasks.start(GuidedTask::GroundTruth, t0);
        let later = t0 + TASK_TIMEOUT - Duration::from_secs(60);
        tasks.start(GuidedTask::GroundTruth, later);
        assert!(!tasks.expire(t0 + TASK_TIMEOUT));
        assert!(tasks.pinned());
        assert_eq!(
            tasks.view(t0 + TASK_TIMEOUT).unpin_in_s,
            Some(TASK_TIMEOUT.as_secs() as u32 - 60)
        );
    }

    #[test]
    fn a_task_ends_by_itself_at_the_timeout() {
        let t0 = Instant::now();
        let mut tasks = GuidedTasks::new();
        tasks.start(GuidedTask::GroundTruth, t0);
        assert!(!tasks.expire(t0 + TASK_TIMEOUT - Duration::from_secs(1)));
        assert!(tasks.pinned());
        assert!(tasks.expire(t0 + TASK_TIMEOUT));
        assert!(!tasks.pinned());
        // Nothing left to expire.
        assert!(!tasks.expire(t0 + TASK_TIMEOUT * 2));
    }

    #[test]
    fn cancel_ends_every_task() {
        let now = Instant::now();
        let mut tasks = GuidedTasks::new();
        tasks.start(GuidedTask::GroundTruth, now);
        tasks.cancel();
        assert!(!tasks.pinned());
    }

    #[test]
    fn the_snapshot_serialises_for_the_windows() {
        let now = Instant::now();
        let mut tasks = GuidedTasks::new();
        tasks.start(GuidedTask::GroundTruth, now);
        let json = serde_json::to_string(&tasks.view(now)).unwrap();
        assert_eq!(
            json,
            r#"{"pinned":true,"tasks":["ground_truth"],"unpin_in_s":1800}"#
        );
    }
}
