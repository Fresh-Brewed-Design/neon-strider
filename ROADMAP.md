# Roadmap

## Next (small, ready to build)

The scheduled improvement task picks from here first. Remove an item when its PR merges.

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
