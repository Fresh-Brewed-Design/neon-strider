# Features

A plain inventory of what Neon Strider already does, so an idea can be checked against it before it is built. Each feature PR adds or edits its line here.

Format: name (`CODENAME` if it has one), what it does, where it lives. Switches show their default. Paths are repo-relative; `src-tauri/src/` is written `rs/` below to keep lines short.

## Treadmill link and control

- Bluetooth scan and connect: the dashboard's rail link opens a dialog that scans for FitShow decks (service `FFF0`) and connects the pick; `src/dashboard/link/`, `rs/bluetooth.rs`.
- Saved treadmill and auto reconnect: the last connected deck is remembered and reconnected after a restart or a dropped link; `rs/bluetooth.rs`, `rs/lib.rs`.
- Link watchdog: retries every 15 s while the PC is in use, backing off to 30 s, 1 min, 2 min once the PC is idle; `rs/lib.rs` (`run_link_watchdog`), `rs/link.rs`.
- Link status in plain words: SCANNING, CONNECTING, RECONNECTING with a retry countdown, NO DATA (10 s without a frame), BT OFF, LINK FAILED, each with a human sentence; `rs/link.rs`, `src/state/linkStatus.ts`.
- Retry now: a button on the Live page and a click on the HUD's link label reconnect straight away; with no treadmill saved the HUD label opens the connect dialog; `src/dashboard/live/Transport.tsx`, `src/hud/useRelink.ts`.
- Engage / Stasis: one button starts or pauses the belt. It reads ENGAGE, WAKING, STASIS, RE-JACK (belt paused) or GO (paused program); `src/hud/status.ts`, `rs/engine.rs`.
- Dump: stops the belt and ends the session; `rs/engine.rs` (`DeckAction::Stop`).
- Speed and tilt control: HUD steppers, Live page sliders with presets, and shortcuts; targets snap to what the deck runs (`DECK_PROFILE`); `src/dashboard/live/`, `src/hud/HudPanel.tsx`.
- Belt never starts by itself: no start or resume after a restart, reconnect, update, unlock or call end without a press (`hold_auto_start`); `rs/engine.rs`.
- Start at the treadmill counts as a press: its own buttons or remote act as GO on a paused or waiting program (`watch_for_deck_start`); `rs/engine.rs`.
- Pause at the treadmill pauses the running program with it (`notice_deck_pause`); `rs/engine.rs`.
- Deck standby handling: a deck that is off or asleep refuses starts, so the UI says "Treadmill is off or asleep" (DECK OFF); `src/dashboard/pages/LivePage.tsx`.
- Unknown status byte: the engine keeps the last known state, sends no automatic start, resume or speed change, and the Live page shows STATE ?; `rs/engine.rs`, `rs/protocol.rs`.
- `DECK_ADJUSTED` and `DECK_REFUSED` notes: shown under the control (HUD and Live page) when the deck lifts or refuses a speed or tilt that was set; `rs/engine.rs`.
- `DECK_PROFILE`: learns the speed bytes and incline levels the deck runs from its replies, never by probing; steppers, program editor and limits keep to them; `rs/deck_profile.rs`, `src/dashboard/system/DeckProfile.tsx`.
- Session recording: a session opens when the belt walks, saves a sample each second, checkpoints to SQLite, and resumes the same session when the app reconnects to a running belt; `rs/engine.rs`, `rs/persistence.rs`.
- Extended pause save: a pause of 5 minutes saves the session (and toasts) but keeps it open for the resume; `rs/engine.rs` (`PAUSE_SAVE_AFTER`).
- Calories: ACSM walking equation from body weight, speed and incline (`METABOLIC_BURN`); `rs/engine.rs` (`walking_kcal_per_min`).
- Dev simulated deck (`SIM_DECK`): see Dev and preview tooling.

## Programs

- Saved programs: named step lists (speed, incline, optional duration, optional heart zone) with an optional repeat interval; `rs/programs.rs`, `src/dashboard/pages/ProgramsPage.tsx`.
- Program editor: create, edit, delete; steps, repeat every N minutes, heart zone unit (% of max or bpm); `src/dashboard/programs/ProgramEditor.tsx`.
- Seeded programs: Post-meal, Workday (5 min every 30 by default), Focus, Brisk, VO2 max, Interval walk, each with a one-sentence note from `docs/research.md`; `rs/persistence.rs` migrations, `rs/programs.rs`.
- Run, pause (HOLD), GO, SKIP, END: on the Live and Programs pages (active program card), the HUD, and Alt shortcuts; `src/dashboard/programs/ActiveProgramCard.tsx`, `src/hud/HudPanel.tsx`.
- Rests and waits: a repeating program rests between cycles and then waits, paused, for GO (`REST COMPLETE // PRESS GO`, `HOLD COMPLETE // PRESS GO`, `BELT STILL // PRESS GO`); `rs/programs.rs` (`ProgramWait`).
- Program end restores the earlier state: the speed from before returns, or waits in `engage_speed` if the belt is not moving; `rs/programs.rs`.
- Heart zone steps (`HEART_LINK` programs): speed trimmed up to 1 km/h to hold the pulse, only on a moving belt, after a 2 minute settle; `rs/programs.rs` (`trim_speed_to_zone`).
- `ZONE_TILT` (Labs, off): zone steps may trim incline, speed or both, chosen per step; incline moves one deck level at most every 60 s; `rs/heart_rate.rs`, `src/dashboard/system/LabsSection.tsx`.
- Last program memory: Alt+End reruns the last program, and the idle nudge offers programs; `rs/programs.rs` (`persist_last_run`).
- Program tray and tooltip: the tray tooltip names the running program; `rs/tray.rs`.

## HUD panel and views (strip, minified, stasis, hidden)

- HUD window: transparent, always on top, click-through except over its content; `rs/hud.rs`, `src/hud/HudApp.tsx`.
- Widgets: any of VELOCITY, INCLINE, TIMER, DISTANCE, STEPS, CALORIES, DAILY, NEURAL, ODOMETER, HEART_RATE stats plus CONTROLS and PROGRAMS modules, chosen and ordered on the HUD page; `src/widgets/registry.ts`, `src/dashboard/pages/HudPage.tsx`.
- Stat cells: one definition per stat shared by HUD and dashboard (`PHYS_VELOCITY`, `ALTITUDE_BIAS`, `DECK_CYCLES`, `DISPLACEMENT`, `LOCOMOTION_INC`, `METABOLIC_BURN`, `DAILY_TOTAL`, `NEURAL_FLUIDITY`, `LIFETIME_ODOMETER`, `PULSE_RATE`); `src/widgets/registry.ts`.
- Pin to HUD: a pin on each Live page stat adds or removes it from the HUD; `src/dashboard/live/PinToggle.tsx`.
- Corner choice: four corners, bottom right by default; `src/dashboard/pages/HudPage.tsx`, `rs/snapshot/hud.rs`.
- Drag by logo: drag the NEON STRIDER logo to any spot on any display, snapping within 24 px of an edge or corner; double-click or Reset returns to the corner; `rs/hud_drag.rs`, `rs/placement.rs`, `src/hud/useHudDrag.ts`.
- Display picker: choose which display the HUD sits on (shown with two or more); found again by name or position, primary if missing; `rs/display.rs`, `rs/snapshot/hud.rs`.
- Stasis mode ("When the belt is still"): Show everything (default), Shrink to a strip, or Hide; a 10 s grace keeps the panel whole after a stop; `rs/snapshot/stasis.rs`.
- Stasis strip: link header, today's total, expand button and Engage; `src/hud/HudStrip.tsx` (`StripRow`).
- Expand: shows the whole HUD until the belt moves again; `src/hud/HudStrip.tsx`.
- Minified HUD: the header's minify button or the hotkey shrinks the panel to a running strip (speed, time, distance, program step, Engage, Dump) that persists across restarts; `src/hud/HudStrip.tsx` (`RunStripRow`), `rs/snapshot/stasis.rs`.
- Minify hotkey: Alt+Insert, on by default; the HUD page can switch it off or use Alt+Pause or Alt+Scroll Lock; `rs/hooks.rs`, `src/hud/minifyHotkey.ts`.
- Program waiting for GO keeps the whole panel: it ignores minify and the strip until GO; `rs/snapshot/stasis.rs`.
- HUD header: link LED, draggable logo, link status (a relink button while offline), minify button, SIM tag on the simulated deck; `src/hud/HudHeader.tsx`.
- Programs module: one button per saved program (first six) or the running program with HOLD or GO, SKIP, END; `src/hud/HudPanel.tsx`.
- Neural line: `NEURAL_FLUIDITY // N%` with target over base speed, shown quietly while easing is active; `src/hud/HudPanel.tsx`.
- `PULSE_RATE` on the HUD: bpm, a 10 minute spark chart, and a zone line (range, under, in or over, `ZONE // SETTLING`); also on the minified strip while a sensor is picked; `src/widgets/registry.ts`, `src/hud/HudPanel.tsx`, `src/hud/HudStrip.tsx`.
- Inline notes: errors and deck notes appear under the control that triggered them (`HudNote`, `useHudSend`); `src/hud/HudNote.tsx`.
- Eye break tones: the HUD plays the break tones; `src/hud/eyeBreakTones.ts`.
- Text scale: the HUD copes with the Windows text size setting (preview flag `?textscale`); `src/hud/hud.css`.

## HUD toasts

Toasts are transient (5 s, 8 s when they carry a button), one slot per type, and come from Rust through the `hud-update` event (`rs/hud.rs`, `src/hud/useToasts.ts`, `src/hud/ToastCard.tsx`).

- `VELOCITY` readout: `SPEED: n.n` with a bar when the speed changes by more than 0.15 km/h; suppressed while Neural flow eases the belt and while the panel or running strip already shows speed; `rs/bluetooth.rs`.
- `ELEVATION` readout: `INCLINE: n` with a bar when the incline level changes; hidden while the panel shows INCLINE; `rs/bluetooth.rs`.
- `STATUS` change: `STATUS: STOPPED`, `COUNTDOWN`, `RUNNING`, `PAUSED` or `SPINNING DOWN` when the deck's state changes; `rs/bluetooth.rs`, `rs/protocol.rs` (`toast_label`).
- `STATUS` connected: `DEVICE_CONNECTED` when a connection's worker starts; `rs/bluetooth.rs`.
- `STATUS` saved: `SESSION SAVED · duration · km` when a session is saved on a stop or after a 5 minute pause; `rs/engine.rs` (`save_and_notify`).
- `AUTOMATION` rest warning: `<PROGRAM NAME> IN 30S` 30 s before a rest ends, with a SKIP REST button; `rs/programs.rs`.
- `AUTOMATION` waiting for GO: `REST COMPLETE // PRESS GO`, `HOLD COMPLETE // PRESS GO` or `BELT STILL // PRESS GO` when a program starts waiting; `rs/programs.rs` (`ProgramWait::headline`).
- `AUTOMATION` deck start: `DECK START // <PROGRAM NAME> RESUMED` when a start at the treadmill resumes a program; `rs/programs.rs`.
- `CALL` quiet: `CALL DETECTED // QUIET MODE` when the belt eases for a call; `rs/desk.rs`.
- `CALL` restored: `CALL ENDED // RESTORED` when the speed returns; `rs/desk.rs`.
- `LIGHTS_OUT` stopped: `DISPLAY DARK // DECK SHUTDOWN` when the display goes off and the Stop is sent; `rs/lights_out.rs`.
- `LIGHTS_OUT` back: `DISPLAY BACK // BELT HELD STILL` when the display comes back on; `rs/lights_out.rs`.
- `STRAVA` uploaded: `STRAVA UPLOADED · n.nn KM` after an auto-sync upload; `rs/strava/worker.rs`.
- `STRAVA` skipped: `STRAVA SKIPPED · ALREADY LOGGED` when another app already put the walk on Strava; `rs/strava/worker.rs`.
- `STRAVA` failed: `STRAVA UPLOAD FAILED` (retry in 15 minutes) or `STRAVA PAUSED · API APP OFF` (paused until acted on), with a button flag; `rs/strava/worker.rs`.
- `ERGO` type: the HUD can render it (green accent) but no Rust code emits it today; `src/hud/ToastCard.tsx`.
- Toasts create the HUD window if none is open (desk toasts); `rs/desk.rs` (`announce`).

## HUD cards

Cards are persistent: each stays until it is acted on or its condition clears (`src/hud/HudCards.tsx`, mounted in `src/hud/HudApp.tsx`).

- Belt service due (`BELT SERVICE DUE`): shows usage and days since last service while a service is due; SERVICED needs a second press (`CONFIRM?`); `src/hud/HudCards.tsx`, `rs/maintenance.rs`.
- Last week (`LAST WEEK`): distance, change against the week before, active days and time, shown on the first walk of a new week until OK; Rust remembers it per ISO week; `rs/weekly.rs`.
- Idle nudge (`STASIS_ALERT`, `n MIN IDLE`): ENGAGE, up to three program buttons, SNOOZE 15 MIN, plus a line about the treadmill being off or a queued program; `rs/seated.rs`.
- Eye break heads-up (`OCULAR_IMPLANT`, `IMPLANT COOLDOWN IN n S`): a minute before a break, with SKIP; `rs/eyebreak.rs`.
- Eye break cooldown (`OCULAR_IMPLANT // COOLDOWN`): countdown, guided blink cue (`SHUTTER // CLOSE`, `SQUEEZE`, `OPEN`), SKIP; `rs/eyebreak.rs`.
- Eye break complete (`OCULAR_IMPLANT // RECALIBRATED`): RESUME or STAY STILL when the break paused the belt; `rs/eyebreak.rs`.
- Walk-away warning (`WALK-AWAY // PAUSING IN 30s`): STAY; any input also cancels; `rs/walkaway.rs`.
- Walk-away paused (`WALK-AWAY // BELT PAUSED`): RESUME or END, with the reason (locked PC or idle); `rs/walkaway.rs`.
- Update ready (`UPDATE READY // vX.Y.Z`): INSTALL opens System, Updates on the notes (never installs blind), LATER hides it; shown once per version; `rs/update.rs`.
- "Calibrate this treadmill" offer: a card in the dashboard (not the HUD) on any page but System, with SKIP remembered per treadmill; `src/dashboard/system/calibration/CalibrateOffer.tsx`, `rs/calibration.rs`.

## Shortcuts and tray

- Global Alt shortcuts: Alt+Volume Up or Alt+`=` speed up, Alt+Volume Down or Alt+`-` speed down, Alt+Page Up/Down incline, Alt+Home start or pause (GO on a paused program), Alt+End end the program or rerun the last, Alt+Insert minify; `rs/hooks.rs`, `rs/platform/windows.rs`.
- Alt+Space tap: only while a `GROUND_TRUTH` tap run listens; `rs/hooks.rs`.
- Dev build yields shortcuts: a dev build passes every Alt shortcut to the release app while it runs; `rs/hooks.rs` (`shortcuts_left_to_release`).
- Tray icon: left click opens the dashboard, the menu has Quit; closing windows never quits; `rs/lib.rs`.
- Live tray tooltip: speed, distance, today's total and the running program, throttled to one change per 2 s, with `UPDATE READY` while an update waits; `rs/tray.rs`.
- Tray icon states: lit rim while the belt runs, normal when idle, dimmed offline, pink dot while an update waits; `rs/tray_art.rs`.
- Dashboard window: tray popup that reopens where it was left and closes on blur unless pinned; `rs/dashboard.rs`.
- Pin open: keeps the dashboard open on blur; `src/dashboard/DashboardApp.tsx`.
- Guided tasks: a task (the `GROUND_TRUTH` run) holds the dashboard open and on top with a LOCKED ON TOP marker and UNPIN, ending after 30 minutes or a restart; `rs/guided_task.rs`, `src/dashboard/guided/`.

## Neural flow and desk signals (typing, calls, lock, idle)

- Neural flow (`NEURAL_FLUIDITY`, off by default): eases the belt down while sustained typing is counted and recovers once it stops; slowest speed 90 % (50 to 95) and sensitivity set in System; `rs/neural.rs`, `src/dashboard/system/NeuralSection.tsx`.
- Key counting: only a count of typing key presses, never which keys; switched by Rust only while needed (`neural_monitor_wanted`); `rs/hooks.rs`.
- Typing WPM per session: History shows an estimated typing rate for walks with Neural data; `src/dashboard/history/SessionRow.tsx`.
- Last input time: keyboard and mouse hooks keep the time of the last input for desk features and the link watchdog; `rs/hooks.rs`.
- Idle nudge (`STASIS_ALERT`, on by default, 30 min, 15 to 240): after PC use with the belt still, the card offers a walk; the program queues if the treadmill is off and starts on connect within 15 minutes; `rs/seated.rs`, `src/dashboard/system/DeskSection.tsx`.
- Slow down on calls (on by default, 2.0 km/h quiet speed): while another app uses the microphone, a running belt eases to the quiet speed (never faster than it was) and returns 10 s after the call, unless the speed was changed by hand; `rs/calls.rs`.
- Microphone detection: reads which apps use the microphone from Windows' own list; nothing is recorded; `rs/platform/windows.rs`.
- Walk-away on lock (on by default): locking Windows pauses a walking belt and any program; `rs/walkaway.rs`.
- Walk-away on idle (on by default, 10 min, 2 to 30): after no keyboard or mouse input the HUD warns for 30 s, then pauses; a call or any input holds it off; `rs/walkaway.rs`.
- Eye breaks (`OCULAR_IMPLANT`, off by default, every 20 min for 90 s): a heads-up, a break with guided blinks and tones, and optionally the belt paused; counting stops while the display is off; `rs/eyebreak.rs`.
- Eye break options: Tones (on), Pause the belt for breaks (on), Take breaks at program rests (on); `src/dashboard/system/DeskSection.tsx`.
- `LIGHTS_OUT` (off by default): when the PC display turns off the app sends Stop and ends any program, nothing restarts when it comes back; `rs/lights_out.rs`, `src/dashboard/system/LightsOutSetting.tsx`.
- Desk loop: one 1 s tick runs the nudge, eye break, call, walk-away and lights-out machines under a single lock; `rs/desk.rs`.
- `DESK_SYNC`: counts how many of the last 14 desk days (days with PC input) were also walked on; History card; `rs/desk_sync.rs`, `src/dashboard/history/DeskSync.tsx`.

## Heart rate

- `HEART_LINK`: reads a standard 0x180D heart rate sensor (such as a watch broadcasting), off until one is picked; apart from the treadmill link and never moves the belt; `rs/heart_rate.rs`, `rs/heart_rate/worker.rs`.
- Sensor picker: scan, pick and forget in System, Heart rate; `src/dashboard/system/HeartRateSection.tsx`.
- Max heart rate: set in System, Heart rate (120 to 230 bpm, 190 until set); zones are a percentage of it; `rs/heart_rate/worker.rs`.
- Heart rate per second: each walk saves the reading with its sample, plus average and maximum per session; `rs/persistence.rs` (migration 6).
- Heart rate in History: average and maximum on a session, and a yellow line with its own BPM axis on the opened chart, with gaps; `src/dashboard/history/SessionScope.tsx`.
- Heart rate in CSV export: average and maximum columns, empty without readings; `rs/export.rs`.
- Heart rate in Strava: `HeartRateBpm` on trackpoints plus lap average and maximum, in uploads and the manual TCX; `rs/strava_plan.rs`.
- Spark chart history: a rolling 10 minute window kept in Rust; `rs/heart_rate/history.rs`.
- Zone readout: `ZONE`, under, in or over, with `ZONE // SETTLING` while the step waits for the pulse; HUD and Live page; `src/dashboard/live/ZoneReadout.tsx`, `rs/heart_rate.rs`.

## Calibration

- `GROUND_TRUTH` (always on, System, Calibration): tap-timed belt runs; RUN sets a target speed and taps (button, Space, or Alt+Space) give the belt loop time; `rs/calibration.rs`, `rs/calibration/tap.rs`, `src/dashboard/system/calibration/`.
- Guided walk: asks the fastest speed you walk, times the deck's slowest and top speed, then checks midpoints and splits a span only when the error is over 2 %; `rs/calibration/guide.rs`, `src/dashboard/system/calibration/GuidedCalibration.tsx`.
- `GAIT_SYNC`: after a belt run, tapping footsteps gives cadence and stride, saved by itself with UNDO; `rs/calibration/steps.rs`, `src/dashboard/system/calibration/StepPanel.tsx`.
- Belt loop length: a per-treadmill setting (`BELT_LOOP` step); `rs/persistence.rs`.
- Target list and RUN ALL: add or remove target speeds and walk them in turn, under ADVANCED // TARGET_LIST; `src/dashboard/system/calibration/CalibrationPanel.tsx`.
- Point editor: edit or delete saved points, under ADVANCED // POINT_EDITOR; `src/dashboard/system/calibration/EditPointModal.tsx`.
- Per-treadmill points: calibration points are saved per treadmill, and a second treadmill starts uncalibrated; `rs/calibration/fresh.rs`.
- `START FRESH`: sets the points aside for a new walk; RESTORE brings them back, KEEP NEW deletes them (`POINTS_ASIDE`); `rs/calibration/fresh.rs`, `src/dashboard/system/calibration/FreshStart.tsx`.
- Calibrate offer: a connecting treadmill with no belt length and no points gets "Calibrate this treadmill", skippable per treadmill; `rs/calibration.rs`.
- Calibration never moves a running belt: new points keep the deck's byte (`CurveHold`) until the next speed change; `rs/engine.rs`.
- `DECK_SWEEP` (Labs, in Diagnostics, off by default): one press steps a walking belt through each unlearned speed byte, or incline levels, so `DECK_PROFILE` completes; never starts a belt; `rs/profile_sweep.rs`, `src/dashboard/system/DeckProfile.tsx`.
- Forget `DECK_PROFILE`: clears what the app learned about the deck; `src/dashboard/system/DeckProfile.tsx`.

## History and stats

- History page (`SESSION_ARCHIVE`): totals, weekly panel, `DESK_SYNC`, a year calendar and the session list; `src/dashboard/pages/HistoryPage.tsx`.
- Time frame: 7 days (default), 30 days, Year, All, or one day picked on the calendar; `src/dashboard/history/sessions.ts`.
- Totals: distance, time, sessions and energy for the shown range; `src/dashboard/history/Summary.tsx`.
- Weekly summary: last week against the week before, and this week so far, as a History panel; `rs/weekly.rs`, `src/dashboard/history/Summary.tsx`.
- Year calendar: a 53 week heat map of distance by day; click a day to filter; `src/dashboard/history/Calendar.tsx`.
- Session list: sessions grouped by day, close ones joined into one walk; `src/dashboard/history/SessionRow.tsx`.
- Session replay: an opened session draws speed and incline over time (and heart rate), plus steps, average incline and typing WPM; `src/dashboard/history/SessionScope.tsx`.
- Session join threshold: records closer than 10 minutes (1 to 60) are joined for History and Strava; `src/dashboard/system/SessionsSection.tsx`.
- Per-session actions: Upload to Strava, Save for Strava, Mark as on Strava, Preview TCX, Delete; `src/dashboard/history/SessionRow.tsx`.
- CSV export: every session to Downloads, regardless of the shown range; `rs/export.rs`.
- Daily total (`DAILY_TOTAL`): today's km including the live walk, a HUD and Live stat and the stasis strip's total; `rs/snapshot.rs`, `src/widgets/registry.ts`.
- Lifetime odometer (`LIFETIME_ODOMETER`): total distance, belt hours and since-when, in System, Maintenance, and pinnable to the HUD as ODO; `rs/odometer.rs`.
- Database: SQLite in WAL mode with versioned migrations, a copy before each upgrade, and 14 daily online backups; `rs/persistence.rs`, `rs/backup.rs`.
- No records or celebrations, on purpose: History has no personal bests, streak trophies or celebration cards.

## Strava

- OAuth connect with your own Strava API app: client ID and secret, browser login, cancel, disconnect; tokens stay in Rust and Windows Credential Manager; `rs/strava/auth.rs`, `rs/secrets.rs`, `src/dashboard/system/strava/Account.tsx`.
- Scope check: asks for upload, read activities (duplicate check) and profile (shoes); `rs/strava_plan.rs` (`REQUESTED_SCOPE`); `RECONNECT` state when a fresh grant is needed.
- Auto-sync (off by default): a pass every minute and one soon after a session ends uploads finished walks, only those ending after it was switched on; `rs/strava/worker.rs`.
- Sync now and Retry: run a pass at once, or clear a backoff or pause; `src/dashboard/system/strava/Sync.tsx`.
- Daily rollup (off by default): uploads a day's walks as one activity after the shift end time (18:00 default); `rs/strava_plan.rs`, `rs/strava/worker.rs`.
- Walk grouping: sessions closer than the join threshold become one walk; walks under 50 m are not uploaded; `rs/strava_plan.rs`.
- Duplicate check: skips a walk another app already put on Strava; `rs/strava/upload.rs`.
- Indoor walks: every upload is marked as an indoor walk; `rs/strava_plan.rs`.
- Shoes: pick a pair from the Strava profile, with refresh; `rs/strava/upload.rs`, `src/dashboard/system/strava/UploadOptions.tsx`.
- Mute from home feed: All walks (default), only walks under N minutes (20 default), or none; `rs/strava/settings.rs`.
- Title and description templates: placeholders such as `{distance_km}` and `{duration}`, a separate rollup title, preview from a sample walk, reset; `rs/strava_template.rs`, `src/dashboard/system/strava/TemplateEditor.tsx`.
- Manual upload: Upload to Strava on a session in History; `rs/strava/upload.rs`.
- Save for Strava (no API): writes the walk's TCX to `Downloads\Neon Strider`, opens Strava's upload page and shows the file; Mark as on Strava records a manual upload; `rs/strava_save.rs`, `rs/strava/fallback.rs`.
- TCX preview: History shows the TCX Strava would receive; `src/dashboard/history/TcxModal.tsx`.
- Strava status in plain words: AUTHORIZING, UPLOADING, PROCESSING, CHECKING, BACKOFF, ERROR, PAUSED, each with one sentence; `rs/strava_status.rs`, `src/dashboard/stravaStatus.ts`.
- Failure handling: transient failures back off 15 minutes; permanent ones (API app off) pause auto-sync until Retry; `rs/strava/worker.rs`.

## Updates

- Update check: a check 30 s after start, daily, hourly after a failure, or on demand, against `latest.json` on the newest GitHub release; `rs/update.rs`.
- Release notes: joins the notes of every release since the installed one (falls back to `latest.json`'s notes); `rs/update.rs`.
- Install: downloads, verifies the signature and runs the per-user installer in passive mode; the belt keeps going and the reconnect adopts the session; `rs/update.rs`.
- Install automatically (on by default): installs when the belt has been still with no program for 10 minutes and the PC has had no input for 5, rechecked right before the installer; `rs/update.rs` (`auto_install`).
- Not now: puts an offered update off until the next day's check; `src/dashboard/system/updates/UpdatesSection.tsx`.
- What's new: a dialog with the notes after an update, shown once; `src/dashboard/system/updates/WhatsNewDialog.tsx`.
- Update dots: a pink dot on the tray icon, the System rail item and the Updates tab while an update waits, and `UPDATE READY` in the tray tooltip; `src/dashboard/system/updates/UpdateDot.tsx`, `rs/tray.rs`.
- Update HUD card: see HUD cards.

## Maintenance

- Belt service reminder: due at 200 km or 3 months, whichever comes first, or by belt hours (50 default) when chosen; `rs/maintenance.rs`.
- Maintenance settings: interval by distance, days or hours, a count-by choice, last service date, Mark serviced; `src/dashboard/system/MaintenanceSection.tsx`.
- Maintenance banner: a one-line "Belt service due" with Mark serviced on the Live page; `src/dashboard/system/MaintenanceBanner.tsx`.
- Maintenance HUD card: see HUD cards.
- Odometer: see History and stats.

## Settings and Labs and Diagnostics

- Treadmill limits: slowest speed, top speed (with USE DECK LIMITS from `DECK_PROFILE`), highest incline level, incline per level, incline offset, body weight; `src/dashboard/system/TreadmillSection.tsx`, `rs/limits.rs`.
- Generated setting limits and defaults: `SETTING_LIMITS.ts` and `DEFAULT_APP_CONFIG.ts` are written by `cargo test` from Rust; `rs/limits.rs`, `rs/defaults.rs`.
- Sessions setting: how close sessions join; see History and stats.
- Desk section: idle nudge, calls, walk-away, eye breaks and `LIGHTS_OUT` switches; `src/dashboard/system/DeskSection.tsx`.
- Startup: Launch at login, off by default, a per-user entry; never in dev builds; `src/dashboard/system/StartupSection.tsx`.
- Single instance: a second launch does not start another app; `rs/lib.rs`.
- Labs `GHOST_POD` (off): the PC advertises the belt's speed and cadence to a watch as a Running Speed and Cadence foot pod, 0 km/h while paused; `rs/ghost_pod.rs`, `rs/ghost_pod/worker.rs`, `docs/watch-pod.md`.
- Labs `GHOST_POD` power (off): also serves a Cycling Power service with watts from the `METABOLIC_BURN` estimate; `rs/ghost_pod/worker.rs`.
- Labs `WRIST_LINK` (off): a second GATT service for the Connect IQ data field in `watch/`; phase 0 only sends the belt state; `rs/wrist_link.rs`, `docs/wrist-link.md`.
- Labs `ZONE_TILT`: see Programs.
- `TRAFFIC_TAP` (Diagnostics, off at every start): records every packet written and every notification received as JSON Lines in `logs/traffic/`; Open folder; `rs/traffic.rs`, `docs/protocol.md`.
- Open log folder: opens the rotating log folder (no secrets or keystrokes logged); `rs/logging.rs`.
- Developer tools (Diagnostics, off): show the command cycler that sends raw frames to the treadmill; `src/dashboard/system/CommandCycler.tsx`.
- Status pills and action status: every button that can fail shows progress and errors directly under itself; `src/components/ActionStatus.tsx`, `src/components/useAction.ts`.
- In-app confirmations: a store `confirm` dialog replaces native dialogs; `src/dashboard/ConfirmDialog.tsx`.
- System section index: sticky links to each System section; `src/dashboard/pages/SystemPage.tsx`.
- Logging: rotating files, panic capture, no secrets; `rs/logging.rs`.
- Secrets at rest: Strava client secret and tokens in Windows Credential Manager, with a settings-table fallback; `rs/secrets.rs`.
- Platform boundary: all OS code (hooks, lock, display power, microphone, idle time, BLE peripheral) sits in `rs/platform/`, with stand-ins for other OSes; `rs/platform/mod.rs`.

## Dev and preview tooling

- `SIM_DECK` simulated treadmill: dev builds list it in the scan; it runs the whole app with no hardware, shows a SIM tag, keeps walks in memory only; `rs/sim.rs`, `rs/sim_link.rs`.
- Simulator scenario tests: the real engine and programs run against `rs/sim.rs`; add a scenario for any change to how the app drives the belt.
- Separate dev identity: dev builds use their own data folder, logs, Credential Manager entries and single-instance lock; `rs/lib.rs` (`DEV_IDENTIFIER`).
- Mocked previews: `hud-preview.html` and `dashboard-preview.html` run on a mocked backend with URL flags; the flag list is in `CLAUDE.md`; `src/dev/mockBackend.ts`, `src/dev/scenarios/`.
- Generated IPC types: ts-rs writes `src/ipc/bindings/`; CI fails on a stale file.
- Auto-release: every merge to `main` releases from `changes/*.md` notes; `.github/workflows/auto-release.yml`, `scripts/release-version.mjs`.
