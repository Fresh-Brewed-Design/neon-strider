//! The HUD during stasis, and minified: whether the panel shows in full, as
//! the still-belt strip, as the running strip or not at all. Rust decides
//! (`decide`, pure) and the HUD only renders `snapshot.hud_view`. What it remembers between snapshots (when the belt was
//! last seen moving, whether the user pressed expand) lives here, in memory.

use super::{AppSnapshot, HudIdleMode, HudView};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// After the belt stops, the panel stays whole this long, so pressing STASIS
/// doesn't pull the panel away from under the pointer.
pub const GRACE: Duration = Duration::from_secs(10);

/// A deck that has sent no status frame for this long (the engine polls every
/// 0.6 s) is off or out of reach, even while the link still reads as up; its
/// last reported status no longer counts as the belt moving.
pub const SILENCE: Duration = Duration::from_secs(4);

/// A program waiting for GO keeps the whole panel up this long (minify waits
/// too), so the prompt can't be missed. Unanswered after that, the panel
/// takes the still belt's view again, minify works, and GO is the strip's
/// main button. Nothing here starts the belt: only GO does.
pub const GO_HOLD: Duration = Duration::from_secs(30);

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
    /// How long a program has waited for GO; `None` while none waits. For its
    /// first `GO_HOLD` the prompt holds the whole panel. One the user paused
    /// collapses like a manual walk (the strip's main button is its GO).
    pub go_wait_for: Option<Duration>,
    /// The user minified the HUD (`HudConfig::minified`).
    pub minified: bool,
}

/// The GO prompt holds the whole panel now (and minify waits).
pub fn go_held(go_wait_for: Option<Duration>) -> bool {
    go_wait_for.is_some_and(|d| d < GO_HOLD)
}

/// The view the HUD panel takes now. A program waiting for GO gets the whole
/// panel for `GO_HOLD`, then the views below, except that Hidden shows the
/// strip, so GO stays on screen. Minified, the running strip takes the place
/// of the whole panel, while still too when the idle mode is Full; once the
/// belt has been still past the grace, a Strip or Hidden idle mode applies as
/// before.
pub fn decide(i: ViewInput) -> HudView {
    if go_held(i.go_wait_for) {
        return HudView::Full;
    }
    let awake = i.moving || i.expanded || i.still_for.is_some_and(|d| d < GRACE);
    let full = awake || i.mode == HudIdleMode::Full;
    if full {
        return if i.minified {
            HudView::Minified
        } else {
            HudView::Full
        };
    }
    match i.mode {
        HudIdleMode::Full => HudView::Full,
        HudIdleMode::Strip => HudView::Strip,
        HudIdleMode::Hidden if i.go_wait_for.is_some() => HudView::Strip,
        HudIdleMode::Hidden => HudView::Hidden,
    }
}

/// What is remembered between snapshots.
#[derive(Debug, Default)]
struct Stasis {
    last_moving: Option<Instant>,
    expanded: bool,
    connected: bool,
    /// When the program's current wait for GO began.
    go_since: Option<Instant>,
}

impl Stasis {
    const fn new() -> Self {
        Self {
            last_moving: None,
            expanded: false,
            connected: false,
            go_since: None,
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
        minified: bool,
        now: Instant,
    ) -> (HudView, bool) {
        let (still_for, expanded) = self.observe(connected, moving, now);
        self.go_since = if program_needs_go {
            Some(self.go_since.unwrap_or(now))
        } else {
            None
        };
        let go_wait_for = self.go_since.map(|t| now.saturating_duration_since(t));
        let view = decide(ViewInput {
            mode,
            moving: connected && moving,
            still_for,
            expanded,
            go_wait_for,
            minified,
        });
        (view, go_held(go_wait_for))
    }
}

static STASIS: Mutex<Stasis> = Mutex::new(Stasis::new());

fn lock() -> MutexGuard<'static, Stasis> {
    STASIS.lock().unwrap_or_else(|e| e.into_inner())
}

/// A program waits for GO: the one test for it, read by the panel's view
/// here (the whole panel for `GO_HOLD`, then GO on the strip) and by the card
/// queue (no card below belt safety while GO waits, `card_queue.rs`).
pub(crate) fn go_waiting(snapshot: &AppSnapshot) -> bool {
    snapshot
        .active_program
        .as_ref()
        .is_some_and(|a| a.wait.is_some())
}

/// The panel's view for this snapshot, and whether a GO prompt holds it whole
/// (`hud_go_hold`). Called each time a snapshot is assembled (every emitter
/// tick), so the grace or the GO hold running out reaches the HUD on the next
/// tick's on-change emit.
pub(super) fn view(snapshot: &AppSnapshot, deck_silent: bool, now: Instant) -> (HudView, bool) {
    let program_needs_go = go_waiting(snapshot);
    lock().view(
        snapshot.hud.idle_mode,
        snapshot.connection.connected,
        snapshot.deck.status.is_moving() && !deck_silent,
        program_needs_go,
        snapshot.hud.minified,
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
            go_wait_for: None,
            minified: false,
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
    fn a_program_waiting_for_go_is_full_for_the_hold_then_the_strip() {
        let waited = |mode, s: u64| ViewInput {
            go_wait_for: Some(Duration::from_secs(s)),
            still_for: Some(Duration::from_secs(600)),
            ..input(mode)
        };
        for mode in MODES {
            assert_eq!(decide(waited(mode, 0)), HudView::Full);
            assert_eq!(decide(waited(mode, 29)), HudView::Full);
            assert!(go_held(Some(Duration::from_secs(29))));
            // Unanswered: the strip with GO as its main button, Hidden too,
            // so GO never leaves the screen.
            assert_eq!(decide(waited(mode, 30)), HudView::Strip);
            assert!(!go_held(Some(Duration::from_secs(30))));
        }
        assert_eq!(decide(waited(HudIdleMode::Full, 30)), HudView::Full);
        assert!(!go_held(None));
    }

    #[test]
    fn after_the_go_hold_minify_and_expand_work_again() {
        let after = |minified, expanded| ViewInput {
            go_wait_for: Some(Duration::from_secs(45)),
            still_for: Some(Duration::from_secs(600)),
            minified,
            expanded,
            ..input(HudIdleMode::Strip)
        };
        assert_eq!(decide(after(true, false)), HudView::Strip);
        assert_eq!(decide(after(false, true)), HudView::Full);
        let full_minified = ViewInput {
            minified: true,
            ..after(false, false)
        };
        assert_eq!(
            decide(ViewInput {
                mode: HudIdleMode::Full,
                ..full_minified
            }),
            HudView::Minified
        );
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
            s.view(HudIdleMode::Strip, true, false, false, false, t0).0,
            HudView::Strip
        );
        // Offline at start: still too.
        let mut s = Stasis::new();
        assert_eq!(
            s.view(HudIdleMode::Hidden, false, false, false, false, t0)
                .0,
            HudView::Hidden
        );
    }

    #[test]
    fn stopping_keeps_the_panel_for_the_grace() {
        let mut s = Stasis::new();
        let t0 = Instant::now();
        let mode = HudIdleMode::Strip;
        assert_eq!(s.view(mode, true, true, false, false, t0).0, HudView::Full);
        let t = |secs: u64| t0 + Duration::from_secs(secs);
        assert_eq!(
            s.view(mode, true, false, false, false, t(1)).0,
            HudView::Full
        );
        assert_eq!(
            s.view(mode, true, false, false, false, t(9)).0,
            HudView::Full
        );
        assert_eq!(
            s.view(mode, true, false, false, false, t(10)).0,
            HudView::Strip
        );
        // Moving again: whole at once.
        assert_eq!(
            s.view(mode, true, true, false, false, t(12)).0,
            HudView::Full
        );
    }

    #[test]
    fn going_offline_counts_as_still() {
        let mut s = Stasis::new();
        let t0 = Instant::now();
        let mode = HudIdleMode::Hidden;
        assert_eq!(s.view(mode, true, true, false, false, t0).0, HudView::Full);
        // The link drops; a stale Running status doesn't count as moving.
        let t = |secs: u64| t0 + Duration::from_secs(secs);
        assert_eq!(
            s.view(mode, false, true, false, false, t(5)).0,
            HudView::Full
        );
        assert_eq!(
            s.view(mode, false, true, false, false, t(20)).0,
            HudView::Hidden
        );
    }

    #[test]
    fn a_new_connection_with_a_still_belt_needs_no_grace() {
        let mut s = Stasis::new();
        let t0 = Instant::now();
        let mode = HudIdleMode::Strip;
        let t = |secs: u64| t0 + Duration::from_secs(secs);
        assert_eq!(s.view(mode, true, true, false, false, t0).0, HudView::Full);
        assert_eq!(
            s.view(mode, false, false, false, false, t(2)).0,
            HudView::Full
        );
        // Back within the grace, belt still: collapsed at once.
        assert_eq!(
            s.view(mode, true, false, false, false, t(4)).0,
            HudView::Strip
        );
    }

    #[test]
    fn expand_lasts_until_the_belt_next_moves() {
        let mut s = Stasis::new();
        let t0 = Instant::now();
        let mode = HudIdleMode::Strip;
        let t = |secs: u64| t0 + Duration::from_secs(secs);
        assert_eq!(
            s.view(mode, true, false, false, false, t0).0,
            HudView::Strip
        );
        s.expanded = true;
        assert_eq!(
            s.view(mode, true, false, false, false, t(1)).0,
            HudView::Full
        );
        assert_eq!(
            s.view(mode, true, false, false, false, t(300)).0,
            HudView::Full
        );
        // The belt moves: expand is used up.
        assert_eq!(
            s.view(mode, true, true, false, false, t(301)).0,
            HudView::Full
        );
        assert_eq!(
            s.view(mode, true, false, false, false, t(400)).0,
            HudView::Strip
        );
    }

    #[test]
    fn a_wait_for_go_holds_the_panel_from_its_start_for_the_hold() {
        let mut s = Stasis::new();
        let t0 = Instant::now();
        let mode = HudIdleMode::Hidden;
        let t = |secs: u64| t0 + Duration::from_secs(secs);
        assert_eq!(s.view(mode, true, true, false, false, t0).0, HudView::Full);
        // The wait begins long after the belt stopped: the hold counts from it.
        assert_eq!(
            s.view(mode, true, false, true, false, t(60)),
            (HudView::Full, true)
        );
        assert_eq!(
            s.view(mode, true, false, true, false, t(89)),
            (HudView::Full, true)
        );
        assert_eq!(
            s.view(mode, true, false, true, false, t(90)),
            (HudView::Strip, false)
        );
        // GO, SKIP or END ends the wait; a later wait gets its own hold.
        assert_eq!(
            s.view(mode, true, false, false, false, t(91)),
            (HudView::Hidden, false)
        );
        assert_eq!(
            s.view(mode, true, false, true, false, t(200)),
            (HudView::Full, true)
        );
    }

    #[test]
    fn minified_and_moving_is_the_running_strip() {
        for mode in [HudIdleMode::Full, HudIdleMode::Strip, HudIdleMode::Hidden] {
            let i = ViewInput {
                moving: true,
                minified: true,
                still_for: Some(Duration::ZERO),
                ..input(mode)
            };
            assert_eq!(decide(i), HudView::Minified);
        }
    }

    #[test]
    fn minified_and_still_keeps_the_idle_mode() {
        // Within the grace (just paused from the strip): still the running strip.
        for mode in [HudIdleMode::Full, HudIdleMode::Strip, HudIdleMode::Hidden] {
            let i = ViewInput {
                minified: true,
                still_for: Some(Duration::from_secs(3)),
                ..input(mode)
            };
            assert_eq!(decide(i), HudView::Minified);
        }
        // Past the grace: Strip and Hidden as before; Full stays minified.
        let long = |mode| ViewInput {
            minified: true,
            still_for: Some(Duration::from_secs(600)),
            ..input(mode)
        };
        assert_eq!(decide(long(HudIdleMode::Strip)), HudView::Strip);
        assert_eq!(decide(long(HudIdleMode::Hidden)), HudView::Hidden);
        assert_eq!(decide(long(HudIdleMode::Full)), HudView::Minified);
    }

    #[test]
    fn minified_gives_way_to_a_program_waiting_for_go() {
        let i = ViewInput {
            minified: true,
            go_wait_for: Some(Duration::ZERO),
            still_for: Some(Duration::from_secs(600)),
            ..input(HudIdleMode::Strip)
        };
        assert_eq!(decide(i), HudView::Full);
    }

    #[test]
    fn minified_lasts_through_a_pause_and_a_restart() {
        let mut s = Stasis::new();
        let t0 = Instant::now();
        let mode = HudIdleMode::Strip;
        let t = |secs: u64| t0 + Duration::from_secs(secs);
        assert_eq!(
            s.view(mode, true, true, false, true, t0).0,
            HudView::Minified
        );
        // Paused from the running strip: it stays put for the grace.
        assert_eq!(
            s.view(mode, true, false, false, true, t(5)).0,
            HudView::Minified
        );
        assert_eq!(
            s.view(mode, true, false, false, true, t(10)).0,
            HudView::Strip
        );
        // Walking again: minified until expanded.
        assert_eq!(
            s.view(mode, true, true, false, true, t(20)).0,
            HudView::Minified
        );
        assert_eq!(
            s.view(mode, true, true, false, false, t(21)).0,
            HudView::Full
        );
    }
}
