//! The HUD during stasis: whether the panel shows in full, as a strip or not
//! at all. Rust decides (`decide`, pure) and the HUD only renders
//! `snapshot.hud_view`. What it remembers between snapshots (when the belt was
//! last seen moving, whether the user pressed expand) lives here, in memory.

use super::{AppSnapshot, HudIdleMode, HudView};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// After the belt stops, the panel stays whole this long, so pressing STASIS
/// doesn't pull the panel away from under the pointer.
pub const GRACE: Duration = Duration::from_secs(10);

/// Everything the decision depends on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewInput {
    pub mode: HudIdleMode,
    /// The belt is moving (Running or Starting) on a live link.
    pub moving: bool,
    /// How long since the belt was last seen moving; `None` when it hasn't
    /// been on this connection (app start, a new link with a still belt).
    pub still_for: Option<Duration>,
    /// Expand was pressed since the belt last moved.
    pub expanded: bool,
    /// A program is waiting for GO: the prompt lives on the panel. One the user
    /// paused collapses like a manual walk (the strip's main button is its GO).
    pub program_needs_go: bool,
}

/// The view the HUD panel takes now.
pub fn decide(i: ViewInput) -> HudView {
    let full = i.moving
        || i.mode == HudIdleMode::Full
        || i.program_needs_go
        || i.expanded
        || i.still_for.is_some_and(|d| d < GRACE);
    if full {
        return HudView::Full;
    }
    match i.mode {
        HudIdleMode::Full => HudView::Full,
        HudIdleMode::Strip => HudView::Strip,
        HudIdleMode::Hidden => HudView::Hidden,
    }
}

/// What is remembered between snapshots.
#[derive(Debug, Default)]
struct Stasis {
    last_moving: Option<Instant>,
    expanded: bool,
    connected: bool,
}

impl Stasis {
    const fn new() -> Self {
        Self {
            last_moving: None,
            expanded: false,
            connected: false,
        }
    }

    /// Record the link and belt now; returns (`still_for`, `expanded`).
    /// Offline counts as still. A new connection starts without a grace, and
    /// seeing the belt move clears expand, so the next stasis collapses again.
    fn observe(&mut self, connected: bool, moving: bool, now: Instant) -> (Option<Duration>, bool) {
        if connected && !self.connected {
            self.last_moving = None;
        }
        self.connected = connected;
        if connected && moving {
            self.last_moving = Some(now);
            self.expanded = false;
        }
        let still_for = self.last_moving.map(|t| now.saturating_duration_since(t));
        (still_for, self.expanded)
    }

    fn view(
        &mut self,
        mode: HudIdleMode,
        connected: bool,
        moving: bool,
        program_needs_go: bool,
        now: Instant,
    ) -> HudView {
        let (still_for, expanded) = self.observe(connected, moving, now);
        decide(ViewInput {
            mode,
            moving: connected && moving,
            still_for,
            expanded,
            program_needs_go,
        })
    }
}

static STASIS: Mutex<Stasis> = Mutex::new(Stasis::new());

fn lock() -> MutexGuard<'static, Stasis> {
    STASIS.lock().unwrap_or_else(|e| e.into_inner())
}

/// The panel's view for this snapshot. Called each time a snapshot is
/// assembled (every emitter tick), so the grace running out reaches the HUD
/// on the next tick's on-change emit.
pub(super) fn view(snapshot: &AppSnapshot, now: Instant) -> HudView {
    let program_needs_go = snapshot
        .active_program
        .as_ref()
        .is_some_and(|a| a.wait.is_some());
    lock().view(
        snapshot.hud.idle_mode,
        snapshot.connection.connected,
        snapshot.deck.status.is_moving(),
        program_needs_go,
        now,
    )
}

/// The strip's expand button: the whole panel until the belt next moves.
pub(super) fn expand() {
    lock().expanded = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(mode: HudIdleMode) -> ViewInput {
        ViewInput {
            mode,
            moving: false,
            still_for: None,
            expanded: false,
            program_needs_go: false,
        }
    }

    const MODES: [HudIdleMode; 2] = [HudIdleMode::Strip, HudIdleMode::Hidden];

    fn idle_view(mode: HudIdleMode) -> HudView {
        match mode {
            HudIdleMode::Full => HudView::Full,
            HudIdleMode::Strip => HudView::Strip,
            HudIdleMode::Hidden => HudView::Hidden,
        }
    }

    #[test]
    fn a_still_belt_takes_the_modes_view() {
        assert_eq!(decide(input(HudIdleMode::Strip)), HudView::Strip);
        assert_eq!(decide(input(HudIdleMode::Hidden)), HudView::Hidden);
        let long_still = ViewInput {
            still_for: Some(Duration::from_secs(600)),
            ..input(HudIdleMode::Strip)
        };
        assert_eq!(decide(long_still), HudView::Strip);
    }

    #[test]
    fn mode_full_is_always_full() {
        assert_eq!(decide(input(HudIdleMode::Full)), HudView::Full);
        let still = ViewInput {
            still_for: Some(Duration::from_secs(600)),
            ..input(HudIdleMode::Full)
        };
        assert_eq!(decide(still), HudView::Full);
    }

    #[test]
    fn a_moving_belt_is_full() {
        for mode in MODES {
            let i = ViewInput {
                moving: true,
                still_for: Some(Duration::ZERO),
                ..input(mode)
            };
            assert_eq!(decide(i), HudView::Full);
        }
    }

    #[test]
    fn a_program_waiting_for_go_is_full() {
        for mode in MODES {
            let i = ViewInput {
                program_needs_go: true,
                still_for: Some(Duration::from_secs(600)),
                ..input(mode)
            };
            assert_eq!(decide(i), HudView::Full);
        }
    }

    #[test]
    fn the_grace_after_stopping_is_full_then_collapses() {
        for mode in MODES {
            let at = |s: u64| ViewInput {
                still_for: Some(Duration::from_secs(s)),
                ..input(mode)
            };
            assert_eq!(decide(at(0)), HudView::Full);
            assert_eq!(decide(at(9)), HudView::Full);
            assert_eq!(decide(at(10)), idle_view(mode));
            assert_eq!(decide(at(11)), idle_view(mode));
        }
    }

    #[test]
    fn expand_is_full() {
        for mode in MODES {
            let i = ViewInput {
                expanded: true,
                still_for: Some(Duration::from_secs(600)),
                ..input(mode)
            };
            assert_eq!(decide(i), HudView::Full);
        }
    }

    #[test]
    fn app_start_with_a_still_belt_collapses_at_once() {
        let mut s = Stasis::new();
        let t0 = Instant::now();
        assert_eq!(
            s.view(HudIdleMode::Strip, true, false, false, t0),
            HudView::Strip
        );
        // Offline at start: still too.
        let mut s = Stasis::new();
        assert_eq!(
            s.view(HudIdleMode::Hidden, false, false, false, t0),
            HudView::Hidden
        );
    }

    #[test]
    fn stopping_keeps_the_panel_for_the_grace() {
        let mut s = Stasis::new();
        let t0 = Instant::now();
        let mode = HudIdleMode::Strip;
        assert_eq!(s.view(mode, true, true, false, t0), HudView::Full);
        let t = |secs: u64| t0 + Duration::from_secs(secs);
        assert_eq!(s.view(mode, true, false, false, t(1)), HudView::Full);
        assert_eq!(s.view(mode, true, false, false, t(9)), HudView::Full);
        assert_eq!(s.view(mode, true, false, false, t(10)), HudView::Strip);
        // Moving again: whole at once.
        assert_eq!(s.view(mode, true, true, false, t(12)), HudView::Full);
    }

    #[test]
    fn going_offline_counts_as_still() {
        let mut s = Stasis::new();
        let t0 = Instant::now();
        let mode = HudIdleMode::Hidden;
        assert_eq!(s.view(mode, true, true, false, t0), HudView::Full);
        // The link drops; a stale Running status doesn't count as moving.
        let t = |secs: u64| t0 + Duration::from_secs(secs);
        assert_eq!(s.view(mode, false, true, false, t(5)), HudView::Full);
        assert_eq!(s.view(mode, false, true, false, t(20)), HudView::Hidden);
    }

    #[test]
    fn a_new_connection_with_a_still_belt_needs_no_grace() {
        let mut s = Stasis::new();
        let t0 = Instant::now();
        let mode = HudIdleMode::Strip;
        let t = |secs: u64| t0 + Duration::from_secs(secs);
        assert_eq!(s.view(mode, true, true, false, t0), HudView::Full);
        assert_eq!(s.view(mode, false, false, false, t(2)), HudView::Full);
        // Back within the grace, belt still: collapsed at once.
        assert_eq!(s.view(mode, true, false, false, t(4)), HudView::Strip);
    }

    #[test]
    fn expand_lasts_until_the_belt_next_moves() {
        let mut s = Stasis::new();
        let t0 = Instant::now();
        let mode = HudIdleMode::Strip;
        let t = |secs: u64| t0 + Duration::from_secs(secs);
        assert_eq!(s.view(mode, true, false, false, t0), HudView::Strip);
        s.expanded = true;
        assert_eq!(s.view(mode, true, false, false, t(1)), HudView::Full);
        assert_eq!(s.view(mode, true, false, false, t(300)), HudView::Full);
        // The belt moves: expand is used up.
        assert_eq!(s.view(mode, true, true, false, t(301)), HudView::Full);
        assert_eq!(s.view(mode, true, false, false, t(400)), HudView::Strip);
    }

    #[test]
    fn a_program_waiting_for_go_keeps_the_panel_after_the_grace() {
        let mut s = Stasis::new();
        let t0 = Instant::now();
        let mode = HudIdleMode::Hidden;
        let t = |secs: u64| t0 + Duration::from_secs(secs);
        assert_eq!(s.view(mode, true, true, false, t0), HudView::Full);
        assert_eq!(s.view(mode, true, false, true, t(60)), HudView::Full);
        assert_eq!(s.view(mode, true, false, false, t(61)), HudView::Hidden);
    }
}
