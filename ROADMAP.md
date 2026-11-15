# Roadmap

## 1.0 (released 2026-11-13)

Everything in [CHANGELOG.md](CHANGELOG.md). Tested on the treadmill (Jogwell Ares 11 Elite).

## 1.1

1. **Fixes from real use of 1.0**: always first.
2. **Weekly summary**: on the first walk of the week, a HUD card with last week's distance,
   time and active days compared with the week before; also at the top of History.
3. **Strava upgrades**
   - Uploads marked as indoor treadmill walks, with the mileage added to the Strava shoes you
     pick.
   - Mute treadmill walks from followers' feeds (default: all of them; optional threshold).
   - Title and description templates, themed by default: `NEON STRIDER // 3.21 KM UPLINK`,
     description in the dashboard codenames.
   - Skip a walk Strava already has from another app or watch (same start time).
4. **Desk features**
   - Sit-too-long nudge: a HUD card after 60 minutes of PC use without walking (any time of
     day), with one-tap program buttons (the useful half of scheduled programs, which can't
     work because the treadmill powers itself off).
   - Slow down on calls: while the microphone is in use, ease the belt to a quiet speed
     (default 2.0 km/h), then restore it.
   - Lifetime odometer: total km and belt hours since tracking began, with the option to
     base the maintenance reminder on hours.

## Next (small, ready to build)

The scheduled improvement task picks from here first. Remove an item when its PR merges.

- Hide or shrink the HUD during stasis (belt paused or stopped): a setting to collapse it to a
  small strip (link state, today's total, a way to expand and Engage) or hide it until the belt
  moves again. Cards that need an answer (walk-away, update, maintenance, waiting for GO) still
  show.


## Later (not scheduled)

- Daily goal and streaks.
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
