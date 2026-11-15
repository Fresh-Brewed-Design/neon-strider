# Changelog

## Unreleased (1.2)

### New

- **Walk-away safety**: locking the PC pauses the belt. After 10 minutes (2 to 30) with no
  keyboard or mouse input, the HUD shows `WALK-AWAY // PAUSING IN 30s` with STAY, then pauses.
  It never resumes by itself: RESUME or END on the HUD. Not while an app uses the microphone.
  System -> Desk.
- **Live tray readout**: the tray tooltip shows speed, distance, today's total and the running
  program; the icon is lit while the belt runs and grey when the treadmill is offline.
- **Drag the HUD** by its NEON STRIDER logo. It snaps to screen edges and corners, moves to
  another monitor, and remembers the spot. Double-click the logo, or Reset on the HUD page, to
  send it back to its corner.

### Changed

- **Typing-aware pace (Neural flow)** reworked: counts key presses only (not the mouse or
  shortcuts), eases off gradually after 10 s of steady typing, recovers after about 45 s
  without typing, and changes speed at most every 15 s. Works on manual walks as well as
  programs. Two settings: Slowest speed (default 90%) and Sensitivity. Floors under 90% move to
  90%. The HUD shows `NEURAL_FLUIDITY // 94%` while it is easing off.
- The HUD finds its monitor by position and size as well as name, so it stays put when a driver
  update renames a display.

### Fixed

- Engage did nothing, silently, when the treadmill's screen was off. It now says so under the
  button after 8 seconds and stops re-sending the start. Unknown treadmill status codes are
  shown and logged instead of being treated as idle.

### Security

- npm security fixes within the current major versions: zod 4.6.7 (memory exhaustion on
  oversized input), vite 7.3.8, postcss 8.5.33, vitest and @vitest/ui 4.1.12, plus patched
  transitive packages. `npm audit --omit=dev` is clean; 7 moderate lint-only advisories remain
  in typescript-eslint's minimatch, which has no fixed 8.x release yet.

## 1.1.0 (2026-11-13)

### New

- **Weekly summary**: last week against the week before (distance, time, active days,
  sessions, calories), plus this week so far, with a HUD card you can dismiss.
- **Strava: indoor walks.** Every upload is marked as an indoor (trainer) walk.
- **Strava: shoes.** Pick a pair from your Strava profile (System → Strava); it goes on every
  upload. The list is cached and refreshes on connect or with Refresh.
- **Strava: mute from feed.** Hide every treadmill walk from your home feed (the default),
  only walks under a set number of minutes (default 20), or none.
- **Strava: title and description templates** with placeholders (`{distance_km}`,
  `{duration}`, `{avg_speed}`, `{avg_incline}`, `{calories}`, `{steps}`, `{walks}`, `{date}`,
  `{time}`), a live preview and Reset to default. The History upload and the TCX preview use
  them too. New default titles read `NEON STRIDER // 3.42 KM UPLINK`.
- **Strava: skip duplicates.** If another app already put the same walk on Strava (a walk, run
  or hike, or any trainer activity, starting within 5 minutes), nothing is uploaded and the
  walk is marked as on Strava.

- **Sit-too-long nudge**: after 60 minutes (15 to 240) of PC use without walking, a HUD card
  (`STASIS_ALERT`) with one-tap buttons for up to three programs, LATER (snooze 15 minutes)
  and DISMISS. If the treadmill is off, the program you pick is remembered and starts when it
  connects (within 15 minutes). Walking 2 minutes, or being away for 15, resets the count.
  Counts only the time of the last keyboard or mouse input. System → Desk.
- **Slow down on calls**: when an app starts using the microphone while the belt runs, it
  eases to a quiet speed (default 2.0 km/h; never speeds up) with a
  `CALL DETECTED // QUIET MODE` toast, and goes back 10 seconds after the call ends
  (`CALL ENDED // RESTORED`) unless you changed the speed. Works with running programs. System → Desk.
- **Lifetime odometer**: total distance and belt hours across every session, and the date
  tracking began, in System → Maintenance, plus an `ODO` HUD widget (`LIFETIME_ODOMETER`).
- **Maintenance by belt hours**: count the service interval in belt hours (default 50) instead
  of distance. Distance (Jogwell's 100 miles) stays the default.

- **Link and Strava status you can see.** The treadmill link shows what it is doing: SCANNING,
  LINKING 2/3, RECONNECT 12s (with Retry now), NO DATA (linked but the deck sent nothing for
  10 s: "Treadmill asleep?"), BT OFF, LINK FAILED, with the last error in the tooltip. The
  connect dialog shows scanning, "No treadmills found", "Bluetooth is off" and a per-device
  error with Retry. The HUD header reads `RELINK 12S`, `NO SIGNAL`, `BT OFFLINE`,
  `LINK FAILED`. Strava shows its step (Preparing, Sending, Strava is processing, Adding
  shoes), "Waiting for you to approve in the browser (4:12 left)" with Cancel, the 15-minute
  back-off countdown with Retry now, and a status pill in the section header. History rows
  spin while uploading and show why a failed upload failed.
- **Errors appear next to the button that caused them.** Buttons show a working state at once
  and put the result (or the failure) on a line under themselves: Strava Connect, Sync now,
  Refresh shoes, template save, History upload, the connect dialog, Live Retry now, Engage and
  Dump, and the maintenance fields. Rejected HUD commands show a short `SYS // ...` line inside
  the card that holds the button. Anything else that is rejected raises a toast instead of
  failing silently. Background Strava failures also raise a toast.
- **Strava errors in plain words.** Strava's error replies are turned into one sentence
  (never raw JSON), with what to do: reconnect, check your internet, wait for the rate limit.
  An inactive API application (Strava now requires a subscription on the account that owns
  it) pauses auto-sync instead of retrying every 15 minutes, and offers Open Strava API
  settings.
- **Save for Strava**: no API needed. In History (per walk) and System → Strava (latest walk)
  it writes the walk's TCX file, with your title and description templates, to
  `Downloads\Neon Strider\<date> <title>.tcx`, opens strava.com/upload/select and shows the
  file in Explorer. The walk is marked as saved, with Mark as on Strava once you have uploaded it.
  Works with no account connected.

### Changed

- Strava now asks for two more permissions (`activity:read`, `profile:read_all`) for the
  duplicate check and the shoe list. Existing connections keep uploading and muting, but show a
  notice until you disconnect and connect again.

## 1.0.0 (2026-11-13)

The first release. Compared with the 0.1 prototype, almost everything was rebuilt.

### New

- **HUD overlay** rebuilt: one legible panel (no text under 11 px) in any screen corner and on
  any monitor, with live stats, start/pause/stop, speed and tilt steppers, and program buttons.
  Clicks pass through everywhere except the panel.
- **Dashboard** with five pages (Live, Programs, History, HUD, System) replacing the old
  three-column layout, settings modal and separate archive window. It reopens where you left
  it, and the Live page fits without scrolling.
- **Saved programs**: named routines (seeded with Post-meal, Workday and Focus) that start
  immediately, pause, skip and end, and put the treadmill back the way it was when they
  finish. Programs survive an app restart.
- **Background Strava sync** with token refresh, merged walks, an optional daily rollup and a
  manual upload button. Credentials live in Windows Credential Manager.
- **Belt maintenance reminder** on Jogwell's schedule (every 3 months or 100 miles,
  editable), shown on the HUD until you mark the belt serviced.
- **Calories** from the ACSM walking equation using your body weight, speed and incline.
- **Pin to HUD** from any stat on the Live page.
- **History**: year calendar, inline speed/incline replay, CSV export.
- **Start with Windows** (optional), single instance, per-user installer (no admin).
- **Logs** in a rotating file (System → Diagnostics → Open log folder).
- **Daily database backups** (newest 14 kept) and a copy before any database upgrade.
- New app and tray icon.

### Fixed

- Auto-reconnect never ran after a dropped Bluetooth link.
- Pressing DUMP re-sent the stop command forever and starved status polling.
- Programs and automation ticked twice, so timed steps ran short.
- Resuming at the same speed after a pause didn't restart the belt.
- Strava uploads were a single point (sample timestamps were zeroed), auto-sync never fired,
  and logins expired after 6 hours.
- Session averages were saved as 0 or a single reading; existing sessions are repaired once.
- Incline was recorded as a hardware level but read as a percentage.
- A walk started from the treadmill's own buttons could be attached to an older session.
- Global shortcuts were applied twice while the dashboard was open.
- The dashboard closed on blur even when pinned.
- The HUD panel could be pushed off-screen by a running program.

### Changed

- Rust now owns all app state; the windows are views of one snapshot and send typed commands.
- Neural flow counts activity only while a program runs with it enabled, whether or not the
  dashboard is open. It never records which keys are pressed.
- The webview runs under a strict content security policy, and each window gets only the
  permissions it uses.
