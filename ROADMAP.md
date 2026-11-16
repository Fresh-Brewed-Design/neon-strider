# Roadmap

## Next (small, ready to build)

The scheduled improvement task picks from here first. Remove an item when its PR merges.

- HUD strip (the mini view while the belt is still) cuts off today's distance and the
  relinking message: make both fit, and check every other text the strip can show.

- Treadmill simulator for testing, in two more PRs, in this order (step 1, recording real
  traffic, is done: Diagnostics' TRAFFIC_TAP writes the recordings the timings come from):
  1. A simulated deck in Rust at the protocol level (`sim.rs`, test-only): takes the bytes the
     engine sends and answers with status frames; models the start countdown, speed ramp, pause
     wind-down, standby (screen off) and power-off. Scenario tests run the real engine and
     programs against it (e.g. run a program, speed to zero, RE-JACK, belt still running 30 s
     later; pause and resume; reconnect while walking; rest ends and waits for GO).
  2. A "simulated treadmill" choice in dev builds, so the whole app (HUD and dashboard) runs
     with no hardware and behaviour changes can be checked end to end. Never in release builds.

- Desk-day consistency: show how many of the recent days the PC was used were also walked on
  (e.g. "walked on 9 of the last 10 desk days"). A day off the PC doesn't count and doesn't break
  anything. No records, bests or celebration: it's a walking pad, and the habit is what matters.

- Minify the HUD while the belt is running: a control on the HUD (and a shortcut) that
  collapses it to the strip at any time, not only while the belt is still. The running strip
  shows what matters at a glance (speed, time or distance, the program step if one runs) and
  keeps pause/stop within reach. It stays minified until expanded again; cards that need an
  answer still show.

- Research-based programs and alerts: an investigation first, then changes. The seeded
  programs (Post-meal, Workday, Focus) and the desk defaults (the sit-too-long alert's 60
  minutes, its snooze, the walk-away timings) are meant to be best practice from current
  research. Check each against the literature (post-meal walking and glucose, breaking up
  sitting time, walking pace and cognitive work, how long and how often), correct any that
  don't hold up, and add programs the research supports. Write the findings with sources in
  `docs/research.md`; keep the in-app text to one short sentence per program.

## Unscheduled

The scheduled improvement task does not pick from here.

- Bluetooth heart-rate strap: HUD, history and Strava.
- Program import/export.
- Standard FTMS treadmills (non-FitShow).
- macOS and Linux, sharing as much code as possible. Bluetooth (btleplug), the UI, the engine,
  Strava, storage, the keyring, tray, autostart and single-instance are already cross-platform.
  Windows-only today, behind one `platform` module with a Windows/macOS/Linux implementation each:
  global keyboard/mouse input (typing-aware pace, idle, shortcuts; macOS needs Accessibility
  permission, Wayland has no global hook), session lock events (walk-away), microphone-in-use
  (calls), the HUD's click-through and monitor placement, and the installer/CI matrix (dmg,
  AppImage/deb). Ship each feature where the OS supports it and hide it where it does not.
- Windows code signing, free only: SignPath Foundation (free certificates for open-source projects;
  apply once the repo is public with an OSI license). No paid certificates.
