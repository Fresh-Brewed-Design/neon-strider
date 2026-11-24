# Neon Strider

Desktop app for controlling a FitShow Bluetooth treadmill, with a neon/cyberpunk UI,
an always-on-top HUD overlay, saved programs, session history, and Strava export.

## Stack

- **Tauri 2** (Rust backend) + **React 19** + **TypeScript** + **Vite 7**
- **Tailwind CSS 4**, **framer-motion**, **lucide-react**, **sonner**; Orbitron bundled via `@fontsource/orbitron`
- **ts-rs** generates TypeScript types for the IPC layer into `src/ipc/bindings/` (run `cargo test`); the same test run writes `SETTING_LIMITS.ts` there from `limits.rs`, the one source for the ranges dashboard fields offer, and `DEFAULT_APP_CONFIG.ts` from `AppConfig::default()` (`defaults.rs`), the one source for the settings the dashboard shows before Rust's saved ones load. `TEMPLATE_FIELDS.ts` (from `strava_template.rs`) is the Strava template field list the template help text shows.
  They are marked generated in `.gitattributes`. Regenerate, don't hand-merge: on a conflict there,
  take either side and run `cargo test` in `src-tauri/`. CI exports them afresh and fails if the
  committed files differ (changed, missing or stale).
- **Zustand** for state, **Zod** for schema validation
- Rust crates under `src-tauri/` own all Bluetooth I/O and BLE packet parsing.

## Architecture

Rust is the source of truth. It connects over BLE, decodes the FitShow protocol, applies
calibration, runs programs (`programs.rs`) and the session lifecycle, and owns HUD config.
`lib.rs` is wiring only (plugins, tray, startup, link watchdog, window lifecycle). The engine
(`engine.rs`) speaks in typed values: `DeckAction` (a control verb from a window, the HUD or a
shortcut), `TargetSource` (which intent won), `PacketKind` (what a packet is, for the log), and
`protocol::Control` (byte 2 of a control packet).
`snapshot.rs` builds one `AppSnapshot` and emits it as `app-state` (on change, 2 s heartbeat);
`dispatch` takes one typed `Command`. It is split into `snapshot/`: `types` (snapshot and command
types), `hud` (HUD config and display list), `commands` (what each command does), `emitter`
(collecting and pushing the snapshot) and `frame` (what each window is sent: the hot parts on
every send, the cold ones in `COLD_KEYS` only when they change or the window just showed, and
nothing to a hidden window; `src/state/mergeAppState.ts` keeps the cold parts between full frames). The tray icon opens the dashboard; its right-click menu
(`snapshot/tray_menu.rs`: a pure model built from the snapshot, updated in place by the emitter)
sends the HUD's own `Command`s for the belt and programs, so it adds no path that moves the belt,
and Hide HUD hides the HUD window until shown again or a card appears; the windows are created
and placed by `dashboard.rs` and `hud.rs` (`display.rs` turns Tauri's monitors into `placement`
geometry); `hooks.rs` holds what the global input hooks record and the Alt shortcuts they fire.
Strava is Rust-owned too (`strava.rs` plus its `strava/` submodules: `settings`, `live`, `auth`,
`api`, `upload`, `worker` for the background auto-sync, and `fallback` for the manual-upload TCX;
`strava_plan.rs` pure logic, `strava_template.rs` title/description templates, whose fields cover every walk stat and collapse with their label and one separator when a walk has no value; `sessions.program_name`, migration 15, holds the programs a walk ran):
windows dispatch `strava_*` commands and read `snapshot.strava`; tokens never reach JS. The client secret and
tokens live in Windows Credential Manager (`secrets.rs`): a save the store refuses fails with
`NOT_SAVED_SECURELY` instead of going to the database; an older build's plaintext DB copy migrates
at startup, keeps working while the store fails (`snapshot.strava.login_storage` says so) and is
stripped from every backup (`backup.rs`).
Auto-update is Rust-owned too (`update.rs`, `tauri-plugin-updater`; no window has updater
permissions and the CSP stays closed): it checks `latest.json` on the newest GitHub release 30 s
after startup and hourly or on `update_check`, joins the notes of every
release since the installed one from GitHub's public releases list (falling back to
`latest.json`'s notes), and feeds `snapshot.update`. `update_install` downloads and verifies,
saves the notes for "What's new" and the running program, then runs the installer in passive
mode, which exits the app and reopens it. It sends nothing to the treadmill: the belt keeps
going while the app is closed (a few seconds, with no HUD or shortcuts), the reconnect adopts
it as the same session, and a running program carries on at the same step, its timer counting
the time the app was closed. Background check failures only show in System -> Updates.
With "Install automatically" on (default, `update_auto_install`), the scheduler (5 s tick) installs an
offered update by itself only when the belt has been still with no program (walking, resting,
paused or waiting for GO), start or countdown/stopping for 10 minutes and the PC has had no input
for 5 (`platform::input_idle_for`); `auto_install` is the pure rule, and it runs again right
before the installer starts.
The installer is per-user NSIS; "launch at login" (`get/set_launch_at_login`) and single-instance are Rust plugins.
`traffic.rs` is the Diagnostics traffic recorder: while switched on (`traffic_record`, off at every
start) `bluetooth.rs` hands it every packet written and every notification received, and it appends
them as JSON Lines to `logs/traffic/` (format in docs/protocol.md); it never sends anything.
`sim.rs` (tests and dev builds, never release) is a simulated deck that answers the engine's bytes
like the real one, and runs scenario tests of the real engine and programs against it; add a
scenario there for any change to how the app drives the belt. In dev builds (`npm run tauri dev`)
the Bluetooth scan lists `SIM_DECK // simulated treadmill`: `sim_link.rs` puts the same deck behind
`bluetooth::DeckLink` (the seam the worker and status listener use for the real BLE deck), so the
whole app runs with no hardware; `connection.simulated` shows a SIM tag on the HUD and dashboard.
Its walks stay in memory (no History, odometer or Strava); connecting it saves it as the treadmill
to reconnect to, in the dev database. Both modules are behind
`cfg(debug_assertions)`, so release builds never list it and `simulated` is always false.
Dev builds run under their own identifier (`lib.rs` `DEV_IDENTIFIER`,
`com.freshbreweddesign.neonstrider.dev`, swapped into the Tauri context by `app_context`), so they
get their own data folder (database, backups), logs, WebView data, Credential Manager entries and
single-instance lock, and never open the release app's. While the release app runs, a dev build
passes every Alt shortcut on to it (`hooks::shortcuts_left_to_release`, checked per press through
`platform::app_instance_running`, which looks for the release app's single-instance mutex); its
hooks stay installed for the idle time and the HUD's click-through. Release builds use `tauri.conf.json`
unchanged. Dev builds never launch at login (the autostart entry is named after the product).
Guided tasks (`guided_task.rs`): a window asks Rust to hold the dashboard open and on top while a task runs (`guided_task_start`/`_end`, UNPIN is `guided_task_cancel`); `dashboard.rs` keeps it through blurs, it ends by itself after 30 min and never survives a restart. The GROUND_TRUTH run pins it while a run or sweep is open (`useGuidedTask`).
GROUND_TRUTH (`calibration.rs`, always on in System, Calibration): RUN (a press) sets the belt to a target speed, then the user taps as the belt mark passes (`calibration_tap`, Space in the dashboard, or Alt+Space through `hooks.rs` while a run listens, `calibration::tap_listening`). Rust timestamps each tap; `calibration/tap.rs` is the pure maths (median gap, outliers dropped against it, done once enough gaps agree). After a run is measured, GAIT_SYNC (`calibration/steps.rs`) times footsteps with the same inputs: stride is the measured belt speed over the cadence, saved by itself with the factor kept, through the same save and UNDO as a belt run. A point counts its footstep runs apart from its belt runs (`stride_runs`, 0 for a stride never measured), so the first measured stride replaces a borrowed one. The guided walk (`calibration/guide.rs`) asks the top speed, times the deck's slowest speed and that one, then checks midpoints (the nearest speed the deck runs when DECK_PROFILE says it skips halfway) and splits a span only when the measured error there is over 2 %; every speed waits for RUN, and a point saved mid-run keeps the deck at the run's speed. The target list and RUN ALL are the advanced way in (ADVANCED // TARGET_LIST); GROUND_TRUTH is the only calibration UI, and a row shows the factor Rust applies when a stored one is outside the limits. A treadmill that connects with no belt length and no points gets "Calibrate this treadmill" (`calibration.offer`); SKIP is kept per treadmill (`calibrationOfferSkipped:<id>`). Tapping only reads the deck. The belt loop length is a setting per treadmill (`beltLoop:<id>`, switched with DECK_PROFILE; the old app-wide `beltLength` is no longer read, and `save_config` deletes it), and so are the points (`calibrationPoints:<id>`, `calibration/fresh.rs`; migration 9 gave the old app-wide points to the treadmill saved then, and points saved with none known go to the first one to connect). `AppConfig.calibration_treadmill` says whose points a config carries, and `sync_orchestrator_config` refuses another treadmill's. START FRESH (`calibration_start_fresh`) sets the treadmill's points aside (`calibrationPointsAside:<id>`) so the walk starts from none; while a set is aside (`calibration.aside_count`) RESTORE brings it back (ending any walk) and KEEP NEW deletes it. STRIDE_FIT (`calibration/stride_fit.rs`, `strideFit:<id>`, set by feel with `stride_fit_set`, held to the deck's speed range, `snapshot.deck.stride_fit_kmh`, none until set) is the fastest speed the user's stride still fits the deck; the program editor flags a step above it and changes nothing, the Live speed slider marks it, and the HUD shows an ABOVE STRIDE_FIT line while the belt runs faster (`snapshot.deck.above_stride_fit`, `stride_fit::above`: running, and the lower of reading and target above it in tenths). FIT_TO_DECK (`programs/fit_to_deck.rs`, #225) offers a seeded program still as seeded (its note kept, steps and repeat equal to `seed_programs`, which migrations 4, 7 and 8 match) with a step above STRIDE_FIT (`snapshot.fit_to_deck`, Rust's words): after the user confirms, `program_fit_to_deck` sets those steps to the fit speed with the grade that keeps their ACSM walking VO2, held to the deck's top incline (the confirm text says what effort that costs); it never starts the program. FIT_TO_TIME (`programs/fit_to_time.rs`, #258, `snapshot.fit_to_time`) offers each program a length picker beside Run on the Programs page (5 to 180 min); `program_run { minutes }` runs a scaled copy (WARM_UP and COOL_DOWN keep their minutes and `pulse_end`, the steps between scale together, a repeat runs one cycle, an open last step takes what is left) and never changes the saved program; `ProgramRun.fit_minutes` refits a run restored after a restart. The preview mirrors the rules in `src/dev/scenarios/fittotime.ts`. The HUD and tray run programs as saved. No sourced rule ties deck length to stride (#223), so nothing works it out from the belt loop or strides.
ECHO (`echo.rs`, the History row's ECHO button) turns a saved walk into a program: `echo_steps` is the pure split of the samples into steady stretches (limits beside each constant), and the named command `program_from_session` (it returns the new program, so it is not a `Command`) saves it through `save_program` and `change_programs` as `ECHO // <date>`, never started.
Neural flow (typing-aware pace) is pure logic in `neural.rs`; key-press counting is switched by Rust (`neural_monitor_wanted`, 1 Hz tick), not by a window.
Speed drift (`speed_drift.rs`, #251, off by default, System -> Speed drift; settings `speed_drift_*`, `Command::SpeedDriftSetConfig`, `snapshot.speed_drift`) moves a manual walk's speed within a band the user sets: `SpeedDrift` is the pure machine (limits beside each constant), `tick_speed_drift` runs it once a second before Neural flow, and `drift_speed` puts its speed in place of the manual intent in `resolve_eased` (the manual intent itself never changes). It waits one interval of the belt running steadily at the user's speed inside the band, so a ramp after a start is never taken for a steady speed, then picks a speed at most 0.3 km/h away every interval and steps one deck speed at a time, each step only once the belt reached the one before. Only speeds the deck runs (DECK_PROFILE) inside the band and at or below STRIDE_FIT. It never starts, resumes or stops the belt; a program, a pause or stop, a call's quiet speed, DECK_SWEEP and a GROUND_TRUTH run take the belt from it, Neural flow easing holds it, and a speed set by hand (app or remote) ends it until the belt stops. The HUD's SPEED DRIFT line shows while it sets the speed, and its steps are not toasted.
Typing by speed (`typing_speed.rs`, #250, Labs switch `labs_typing_speed` off by default, `typing_speed_set`, `snapshot.typing_speed`) reuses that count: while on, `neural_monitor_wanted` counts keys with the belt running whether or not Neural flow is on, and each whole minute of walking in a session saves its count (never which keys, capped per second like Neural flow) on the sample that closes it (`samples.key_presses`, migration 13). The table in System -> Labs puts each minute with a key press in the 0.5 km/h band of its average speed over the newest 20 walks with counts, and shows a band with 5 such minutes, its median per minute. Off, nothing is counted or saved for it; it only reads the belt.
PC time walked (`pc_walk_share.rs`, declared from `snapshot.rs`; the `PC_WALK_SHARE` stat, WALKED on the HUD) counts each desk-loop second with input in the last minute (DESK_SYNC's `ACTIVE_INPUT`) as active PC time, and as walked while the connected belt is Running; the counts are per local day in `desk_days` (migration 12), saved at most once a minute and loaded at startup, and `snapshot.pc_walk_share` gives the share once there are 10 minutes of PC time. It only reads the belt; there are no bests or targets, and the Live page shows it only when a stat tile is free. The same seconds fill today's 5-minute slots (`SLOT_MIN`, saved with the counts in `desk_days.slots`, migration 14), and `snapshot.today_timeline` lays them out as runs (away, at the PC, walking) from the first slot with input to now, None on a day with no input; History draws it as the Today strip.
Status is visible, not silent. `link.rs` (treadmill link state, plain-English BLE errors) and
`strava_status.rs` (Strava activity/steps, `describe()` turns Strava's JSON errors into one
sentence) feed `snapshot.connection.link` and `snapshot.strava.activity`; countdowns are `*_s`
fields Rust recomputes each snapshot. Strava's no-API fallback is `strava_save.rs` (TCX file plus
the upload page). In the UI, every button that can fail or take time uses `useAction` /
`useBackendAction` (`src/components/useAction.ts`) and shows `<ActionStatus>` directly under
itself; `send()` toasts rejections as a backstop, the HUD uses `useHudSend` + `<HudNote>`.
WRIST_LINK (`wrist_link.rs`: pure State encoding and status text, Labs switch off by default; `wrist_link/worker.rs`: the switch, the 1 s tick and the GATT service with its beacon) serves a GATT service for the Connect IQ data field in `watch/`; phase 0 only sends the belt State, nothing from the watch is acted on. See docs/wrist-link.md.
HEART_LINK (`heart_rate.rs`: pure Heart Rate Measurement decoding and snapshot types; `heart_rate/worker.rs`: the picked sensor, scan and link, apart from the treadmill's) reads a standard 0x180D sensor such as a watch with Broadcast Heart Rate, off until one is picked (`heart_rate_sensor_id`), and feeds `snapshot.heart_rate` and the `HEART_RATE` stat. It only reads; it never moves the belt. Its scans and the treadmill's share `bluetooth::SCAN_LOCK`, one at a time. Each second of a walk saves the fresh reading with its sample (`samples.heart_rate`, migration 6), and each session save writes `sessions.avg_heart_rate` and `max_heart_rate` from running totals (`SessionState.heart_rate`, `persistence::HeartRateTotals`, seeded from the samples once when a walk is resumed; #277, with the `(session_id, timestamp)` index of migration 16); History shows both. Strava uploads (auto-sync and the manual-upload TCX) carry it as `HeartRateBpm` on each trackpoint with a reading, plus the lap's average and maximum; a walk without heart rate builds the same TCX as before. A program step may carry a `zone` (`programs.rs`: `HeartZone`, `trim_speed_to_zone`): while the belt runs, the speed is trimmed within 1 km/h of the step's speed to hold the pulse in it, once the belt has run `ZONE_SETTLE` (2 minutes) into the step, counted again after a pause and GO (`ProgramRun::zone_settle_s`, the pulse lags a change in effort); it never starts, resumes or stops the belt, and the engine never auto-resumes a paused belt for a program target (max heart rate: `HeartRateMaxSet`). The max heart rate is set in System, Heart rate (`snapshot.heart_rate.max_bpm`), zones are edited per step in the program editor (% of max or bpm, saved as %), and the seeded VO2 max holds its climbs at 85 to 95 % (migration 7 adds the zones to an unedited one). While a zone step runs with a sensor picked, `snapshot.heart_rate.zone` carries the zone in bpm and whether the reading is under, in or over it (`heart_rate::zone_band`, the trim's own bounds), with Rust's label and sentence (`ZONE // SETTLING` and `settle_s` while the step waits for the pulse); PULSE_RATE shows it on the HUD and the Live page. ZONE_TILT (Labs switch off by default, `heart_rate_incline_trim_set`, `snapshot.heart_rate.incline_trim`, read into `OrchestratorState::zone_incline_trim`) lets a zone also trim incline: each step's `HeartZone::trim` (`ZoneTrim`: speed, the default stored as none so older programs need no migration; incline; or both) is picked in the editor while the switch is on. Incline moves one level the deck runs (DECK_PROFILE, up to the top level) at most every 60 s, within 2 % of the step's incline; both moves the incline first away from the step and the speed first back to it. Off, every zone trims speed only. With STRIDE_FIT set (`OrchestratorState::stride_fit_kmh`, set each second), `programs::fit_lever` turns a trim that would take the speed above it into one incline level up (up to `ZONE_FIT_INCLINE_PCT`, 2 %, past the zone's own incline range, held to the top level), and easing off above it lowers the speed to the fit first, then that extra incline; unset, the zones behave as before. A step may also carry a `kind` (`StepKind`: WARM_UP, WALK, INTERVAL, REST, COOL_DOWN; none for steps saved before #235, which show as before); `ActiveProgram.kind` (`StepKindSpot`) is the walking step's kind counted within that kind, labelled `INTERVAL 3/5` by Rust for the HUD, the Live page and the tray. Migration 10 gave the kinds to unedited seeded programs. `ActiveProgram.steps` and `repeat_every_minutes` are the copy that runs (a save mid-run reaches the next run), which STEP_MAP draws (`src/dashboard/programs/stepMap.ts`, pure: the step list, the chart geometry and the HUD's `NEXT // ...` line): on the Live page it takes the stats' place while a program runs (MAP on the program card switches back, kept in `localPrefs`). A COOL_DOWN step may also carry `pulse_end` (`PulseEnd`, % of max like a zone, 50 to 76; #233; `PULSE_END_DEFAULT_PCT`, 60 %, for every cool-down: the editor ticks it when a step becomes COOL_DOWN, the seeded VO2 max carries it, and migration 11 gives it to an unedited one with its new note): its minutes run first, then it walks on until the pulse has stayed at or under the target for `PULSE_END_HOLD` (30 s), with no upper limit (SKIP and END still work); with no fresh reading it ends on its minutes. Heart rate only decides when the step ends: it never starts, resumes or stops the belt, and the next step or the program end follows the timed-step rules. `snapshot.heart_rate.cool_down` (`PulseTarget`, `heart_rate::pulse_target`) carries the target and Rust's label for PULSE_RATE, and `ActiveProgram.pulse_to_bpm` is set while the step walks past its minutes (the program line reads `pulse to 114`).
Easy start (`easy_start.rs`, System -> Treadmill, on by default, `easy_start_set`, `snapshot.easy_start`): once the belt runs after a start seen from still on this connection (Engage, GO, a program, a speed set by hand, a start at the treadmill), `control_packet` sends the target's byte held to a ramp from the slowest speed (`floor_hw_speed`; a start at the treadmill from the deck's own speed) up to it over `RAMP` (30 s) in `STEP`s (2 s), stepping down past bytes DECK_PROFILE says the deck skips. It only lowers the speed sent: it never starts, resumes or pauses the belt; a pause, stop, speed set by hand or DECK_SWEEP ends it, and a belt found moving is never eased again. Calibration runs are never eased: `calibration::begin_run` calls `measure_at_set_speed`, which holds off the ramp until a speed is set another way, a program runs or the belt is stopped. Program step timers and zone settling count from the start as before (`RAMP` < `ZONE_SETTLE`).

- **Windows**:
  - `main` → `index.html` → `src/main.tsx` → `src/dashboard/DashboardApp.tsx` (760×600 tray
    popup: nav rail with Live, Programs, History, HUD, System pages in `src/dashboard/pages/`).
    Each page's pieces live in a folder beside it: `system/` (with `calibration/`, `strava/` and
    `updates/` inside), `history/`, `live/`, `programs/` and `link/` (Bluetooth scan and link control)
  - `hud` → `hud.html` → `src/hud/HudApp.tsx` (transparent, always-on-top, click-through except
    over its panel; separate small bundle). `HudApp` handles state and events; the panel is
    `HudPanel.tsx`, the cards `HudCards.tsx`, toasts `ToastCard.tsx`, notes `HudNote.tsx`, and
    status text in `status.ts`. While the belt is still, Rust may collapse the panel to a strip
    (`HudStrip.tsx`) or hide it (`hud.idle_mode`); minified by the user (`hud.minified`, the
    header's minify button or its Alt hotkey, `hud.minify_hotkey`: on or off and Insert, Pause or
    Scroll Lock, set on the HUD page, mirrored to `hooks::minify_key` for the hook thread) it shows the running strip instead of the whole panel
    until expanded (PULSE_RATE and its chart join the strip while a heart rate sensor is picked). Both are decided in `snapshot/stasis.rs` as `snapshot.hud_view`; cards and
    toasts always show
- **Shared**: `src/widgets/registry.ts` defines every stat once for the HUD and dashboard.
- **Live state**: Rust owns it. Every window reads one `AppSnapshot` via `src/state/useAppState.ts`
  (`useAppState`/`useAppSnapshot` in components, `getAppSnapshot`/`subscribeAppSnapshot` in services)
  and sends commands with `dispatch` from `src/ipc`.
- **Settings and small UI state**: `src/store/treadmillStore.ts` composed from slices in
  `src/store/slices/` (`settings`, `history`, `ui`). Types in `src/store/types.ts`.
- **Services** (`src/services/`): `startServices.ts` (main window only), `controlOrchestrator.ts`
  (`startSettingsSync`: hydrates settings from Rust config and pushes changes back),
  `sessionCoordinator.ts` (`startHistoryRefresh`: history refresh),
  `sessionMergeSetting.ts` (persisted setting; Rust reads the same key for Strava grouping), and `db/repositories/*`
  (thin wrappers over Rust `invoke` DB commands; rows validated with Zod schemas in `db/schema.ts`).
- **Persistence** (`persistence.rs`): one shared SQLite connection (WAL, busy timeout,
  foreign keys). Schema changes go in as a new function appended to `MIGRATIONS`
  (`PRAGMA user_version` counts applied ones); never edit an existing migration. An existing
  database is copied to `backups/treadmill-pre-v<N>-*.db` before migrating; `backup.rs` also
  keeps 14 daily online backups.
- **Logging**: use the `log` crate (`log::info!` etc.), not `println!` (release builds have no
  console). Files rotate in the app log dir. Never log secrets, tokens, OAuth codes or keys
  pressed.
- **Shared helpers** (`util.rs`): the clock (`now_ms`), duration text and `blocking` (run blocking
  work off the async runtime); `persistence::parse_setting` / `setting_in` read typed settings.
- **Rust commands**: prefer adding a `Command` variant in `snapshot/types.rs` (handled in
  `snapshot/commands.rs`) over a new `#[tauri::command]`. Variants, their match arms and the
  `AppSnapshot` fields are grouped by feature under `// ---- ... ----` markers: add to the end of
  your feature's group, or start a new group; settings and sessions still use named commands via `invoke` (plus `strava_tcx_preview`, a query).
  Each window may call only the app commands its capability allows: `build.rs` declares every
  `#[tauri::command]` in `APP_COMMANDS` (add a new one there too), the dashboard's
  `capabilities/default.json` allows the ones it calls, and the HUD's `capabilities/hud.json`
  only `dispatch`, `get_snapshot` and `set_hud_hit_region`. `src/ipc/capabilities.test.ts`
  fails if a window invokes a command its capability does not allow.

## Platform

OS-specific code goes only in `src-tauri/src/platform/` (`mod.rs` is the interface, `windows.rs`
the Windows implementation, `macos.rs` the macOS one with its pure key tables in `macos_keys.rs`,
`other.rs` the stand-in for every other OS). It covers the input
hooks, whether another instance of the app runs (its single-instance mutex), session lock, display power (`display_off`, for `LIGHTS_OUT` in `snapshot/lights_out.rs`),
which apps use the microphone, the cursor and mouse button, the PC's idle time, and the BLE peripheral (WRIST_LINK's service,
WinRT `GattServiceProvider`, one per service). The rest of
the code calls `platform::...` and never the OS directly; new code must not import `windows::`,
`objc2` (or other OS APIs) outside `platform/`. macOS (docs/macos.md, #33) is a menu bar app:
no Dock icon (`platform::app_started`, `LSUIElement` in `src-tauri/Info.plist`), a template tray
icon (`tray_art::template`), the dashboard dropping down from the icon (`platform::MENU_BAR_APP`,
`placement::below_menu_bar_icon`), ⌃⌥ Carbon hot keys, and the Keychain. Desktop units are
physical px on Windows and points on macOS (`platform::DESKTOP_IN_POINTS`): `display.rs`
converts at Tauri's edge, so placement and the HUD's hit rect never use Tauri's raw positions.
`snapshot.platform` (the OS, its words from `platform::WORDS`, its shortcut labels, and OS-only
features such as `microphone`) lets the UI word things the OS's way and hide what it lacks;
user-facing Rust text that names the OS uses `platform::WORDS`. macOS builds use
`src-tauri/tauri.macos.conf.json` on top of `tauri.conf.json`; CI runs the Rust tests and clippy
on a Mac too (`ci.yml` `macos`).

## Commands

```
npm run dev       # Vite dev server (frontend only)
npm run tauri dev # Full desktop app (Rust + frontend)
npm run build     # tsc + vite build
npm run lint      # eslint (repo is clean: 0 problems)
npx tsc --noEmit  # typecheck
npx vitest run    # unit tests (vitest + jsdom)
node scripts/feature-inventory.mjs [word]   # what the app already has (toasts, cards, snapshot fields, settings), from the code
cargo test        # in src-tauri/: Rust tests, and regenerates src/ipc/bindings
cargo fmt --check # in src-tauri/: formatting
cargo clippy --all-targets -- -D warnings   # in src-tauri/: lints, no warnings allowed
```

Design without a treadmill: with a Vite server running, open `/hud-preview.html` or
`/dashboard-preview.html`. Both run on a mocked backend (`src/dev/mockBackend.ts`, with the
shared snapshot in `src/dev/mockState.ts`). Each feature's URL flags, commands and queries are one
file in `src/dev/scenarios/`, listed in `scenarios/index.ts`; a new feature adds its own file
there. URL flags pick scenarios (one per line, sorted by flag name; add a new flag in its place):

- `?anchor=top_left` (or any corner): the HUD's corner
- `?autoinstall=off`: "Install automatically" switched off (with `?update=...`)
- `?blink=squeeze|open`: the guided blink step during an eye-break cooldown (CLOSE by default)
- `?call`: on a call, the belt held at the quiet speed (the HUD preview also shows the toast)
- `?caloffer`: a connected treadmill with no calibration, so the first-connect "Calibrate this treadmill" card shows (outside System)
- `?calwizard`: GROUND_TRUTH with a 280 cm belt loop, two saved points and the top speed question; `=belt` the BELT_LOOP step, `=tapping|dropped` a tap run five passes in or with a dropped tap and a restart note, `=measured` the plain-words result with the point already saved (UNDO, TAP AGAIN), `=sweep|sweeprun` RUN ALL between speeds or tapping, `=oldpoint` a point stored below the factor limit, `=targets` an added 3.5 km/h and a stored 14.0 km/h outside the range, `=steps` GAIT_SYNC tapping footsteps after a measured run, `=stepsaved` the stride measured and saved by itself, with UNDO, `=guiderun` the guided walk from 0.6 to 5.0 km/h tapping at 5.0, `=guidecheck` both ends saved and the midpoint check up next, `=guidesplit` the check 3.2 % off and split, `=guidedone` the walk done with its summary, `=guideskip` both ends saved on a deck that skips 2.8 km/h, so the check moved to 2.7, `=guideran` the 2.8 check the deck ran at 2.9 (chip, line and note show 2.9), `=aside` START FRESH with the two points set aside (POINTS_ASIDE, RESTORE, KEEP NEW), `=asidedone` the new walk done with the old set still aside
- `?connect=fail`: connecting from the connect dialog fails
- `?custom`: the HUD sits where it was dragged
- `?daily=12.34`: today's km (DAILY_TOTAL)
- `?deck=paused`: with `?stasis=...`, the belt is paused (Engage reads RE-JACK)
- `?decknote=refused|adjusted`: the HUD note after the deck refuses or changes a target
- `?deckprofile`: DECK_PROFILE learned (floor 0.6, ceiling 4.0, 1.5 skipped, top incline level 12, level 7 skipped; Diagnostics shows FORGET); `=sweep` with the LABS // DECK_SWEEP switch on and a speed sweep running, `=inclinesweep` an incline sweep running, `=sweepon` the switch on with no sweep running
- `?desksync=open|new|few`: History's DESK_SYNC card
- `?distance=12.34`: this walk's km (DIST)
- `?due`: the belt reminder well past its interval
- `?easystart=off`: Easy start switched off in System -> Treadmill (every belt start runs straight to its target)
- `?echo=short`: ECHO on a History walk is refused as too short, with Rust's message
- `?engage=timeout`: the deck's screen is off, so Engage waits and fails
- `?eyebreak=upcoming|rest|break|still|resume`: the OCULAR_IMPLANT cards: heads-up, heads-up set for a program rest, cooldown with the belt paused or left alone, resume card
- `?guided`: a guided task (GROUND_TRUTH) holding the dashboard open and on top, with UNPIN
- `?heart=on|waiting`: System -> Heart rate with a watch picked and reading, or not broadcasting yet; in History every other walk has an average and maximum heart rate
- `?hours`: the belt reminder counts belt hours
- `?hrcool=high|holding|clear|wait`: a COOL_DOWN step that ends at or under 114 bpm (60 % of 190), with a watch reading: past its minutes at 126 bpm (`pulse to 114`), past its minutes at 111 bpm with 18 s of the hold left, 1:35 of its minutes left with the pulse already down, or no reading (the minutes end the step)
- `?hrspark=steady|climb|gap`: PULSE_RATE's spark chart with a watch reading: a steady walk, a climb into a heart rate zone step (its band shows), or a minute with no reading
- `?hrzone=under|in|over|wait|settling`: a heart rate zone step (133 to 152 bpm) with a watch reading below, inside or above it, or not reading yet; `=settling` below it while the step waits for the pulse to settle
- `?idle`: the belt stopped
- `?lightsout`: LIGHTS_OUT on; on the HUD preview its toast
- `?link=scanning|connecting|reconnecting|nodata|bt-off|failed`: each treadmill link state
- `?loginstorage`: the Strava login kept in the database because Windows Credential Manager failed, with its note under the account
- `?mac`: running on macOS: its words ("this Mac", System Settings), ⌃⌥ shortcuts, no minify key choice, no Slow down on calls
- `?maxhr=178`: a max heart rate of 178 bpm set in System -> Heart rate (program heart zones use it)
- `?minified`: the running strip; with `?program=resting|paused`, `?noprogram` or `?idle`; with `?hrspark=...`, `?hrzone=...` or `?heart=on|waiting` it shows PULSE_RATE
- `?minifykey=off|pause|scroll_lock`: the minify hotkey switched off, or set to Alt+Pause or Alt+Scroll Lock
- `?neural=active`: Neural flow easing the belt off while typing
- `?nohistory`: History empty: no walks, an empty week, DESK_SYNC and WALL_CLOCK
- `?noprogram`: no program running
- `?noprograms`: no saved programs (Programs shows its empty state)
- `?nostrava`: no Strava credentials or account
- `?offline`: no treadmill saved yet
- `?onemonitor`: a single display
- `?pcwalk`: the WALKED stat pinned on the HUD at 38 % (2h 10m of 5h 40m); `=early` 6 minutes of PC time (`--`), `=long` 10h 05m of 11h 50m, `=none` no PC time today
- `?program=ready|hold|still|resting|paused|interval`: a program waiting for GO after a rest, after a 0 km/h step (its steps have no kind), or after a reconnect found the belt still; mid-rest; paused by the user; or Interval walk on its 3rd brisk round (`INTERVAL 3/5`); each shows STEP_MAP on the Live page and the next step on the HUD
- `?reconnect`: the Strava login needs a fresh grant
- `?reject`: every command fails
- `?saved`: a walk saved for a manual Strava upload
- `?savefail`: saving a walk for Strava fails
- `?scan=empty|error|bt-off`: the connect dialog's Bluetooth scan
- `?seated`: the idle-nudge card (STASIS_ALERT); comma-separated variants `off`, `queued`, `long`, `none`, `one`, `many`
- `?sim`: the SIM tag
- `?speeddrift=drifting|held|waiting|outside|ended`: System -> Speed drift on (3.2 to 3.8 km/h, every 5 minutes) on a manual walk: setting the belt at 3.6 km/h (the HUD's SPEED DRIFT line), holding while Neural flow eases off, counting to the first change, the speed set outside the band, or ended by a speed set by hand; `?speeddrift` alone is the switch on with the belt still
- `?stasis=strip|hidden`: "When the belt is still" set to that, with the belt stopped
- `?status=9|countdown|stopping|standby`: an unknown status byte, the 3-2-1, the belt winding down, or the deck asleep
- `?strava=authorizing|uploading|processing|checking|backoff|error|paused`: each Strava state
- `?stridefit=4.5`: STRIDE_FIT set to 4.5 km/h in System, Calibration, so the program editor flags Brisk's 5.0 km/h step and Interval walk's 6.0 km/h steps; the belt runs at 5.0 km/h, so the Live speed slider shows the FIT mark and the HUD its ABOVE STRIDE_FIT line; Programs offers FIT_TO_DECK on Brisk, VO2 max and Interval walk
- `?textscale=121`: the Windows text size setting, any whole percent from 100 to 225, as the page zoom WebView2 applies for it (dev previews only; size the HUD preview window to 340 px wide)
- `?toasts`: the HUD toasts; `=deck` for the deck-start GO toast
- `?today=early|long`: History's Today timeline for 08:02 to 14:25 with a walk after lunch; `=early` 20 minutes in, `=long` 06:40 to 21:30
- `?traffic=recording|saved`: the Diagnostics traffic recorder
- `?typingspeed`: System -> Labs typing by speed on, with a table of five speed bands from 12 walks; `=empty` on with no walks counted yet
- `?update=available|downloading|failed|whatsnew`: auto-update states
- `?walkaway=warning|paused|idle`: the walk-away cards
- `?weekly`: the last-week HUD card; `=long|new|down` variants
- `?wallclock=new|late|none`: History's WALL_CLOCK card with no desk days, a walk at 21:00 widening the row, or desk days with no walks
- `?wristlink=waiting|connected`: WRIST_LINK on
- `?zonetilt`: System -> Labs ZONE_TILT on, so the program editor offers each heart zone step speed, incline or both

Tests stub the Tauri APIs (`@tauri-apps/api/event|core|window`) in `src/test/setup.ts`,
since there's no Tauri runtime under jsdom.

## Releases

Every push to `main` (a merged PR) releases everything merged since the last `v*` tag as one
version (`.github/workflows/auto-release.yml`, logic in `scripts/release-version.mjs`). The bump
is the largest `release:major|minor|patch` label among those PRs (no label = patch;
`release:none` PRs don't count). Never bump versions, tag, or edit CHANGELOG.md by hand: each
PR adds a release note file `changes/<new|changed|fixed>-<slug>.md` (see `changes/README.md`),
which the release folds into CHANGELOG.md. All work goes to `main` through PRs. CI runs on
PRs and on `main` after each merge; the run on `main` saves the compiled Rust dependencies
(`shared-key: ci`) and PR runs restore them, so a PR compiles only this crate.
`audit.yml` runs `cargo audit` and `npm audit` weekly (ignores, with reasons, in `src-tauri/.cargo/audit.toml`).
`build-roadmap-issue.yml` builds a roadmap issue when Fresh Brewed Design labels one of its own issues
`roadmap:next`: Claude opens a PR that closes it (secret `CLAUDE_CODE_OAUTH_TOKEN`). Only its
account (user ID 3596382) can start it, as both the issue's author and whoever added the label.
`pr-followup.yml` is how he asks for changes afterwards: a comment or review of his on a PR that
says `@claude ...` starts a run that changes that PR's branch and replies there (again only his
account; PRs from forks are refused). Neither run appears in the desktop app. Both install
Claude Code from npm first, because the action's own installer does not run on Windows.

The GitHub release body is the version's CHANGELOG section (`release-version.mjs` writes it to
a file and auto-release passes it on; a hand-pushed `v*` tag reads it from CHANGELOG.md), then
the SmartScreen note below an `<!-- installer-note -->` marker, which the app cuts off. The
release build signs the installer with the updater key and tauri-action uploads `latest.json`
plus the `.sig`: `release.yml` turns on `createUpdaterArtifacts` with
`--config src-tauri/tauri.release.conf.json` and reads the key from the
`TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets of the `release`
environment (deployments limited to `main` and `v*` tags, so no other branch's workflow can read them). The public key is in `tauri.conf.json`
(`plugins.updater.pubkey`); the private key never goes in the repo. Local builds need no key.

## Voice

The neon/cyberpunk theme is part of the product, including text the app writes elsewhere
(Strava titles and descriptions, HUD messages, toasts). Use the existing codenames
(`DECK_CYCLES`, `DISPLACEMENT`, `ALTITUDE_BIAS`, `METABOLIC_BURN`, `LOCOMOTION_INC`,
`DAILY_TOTAL`, `NEURAL_FLUIDITY`) and the `NEON STRIDER // ...` style; keep explanations in
plain sentence-case English so they stay readable. Units: the user's stats are metric (km,
km/h, kg); a manufacturer's spec is quoted in its own unit. The belt reminder is km: the
sticker on the deck gives 200 km (125 miles), which is the default.

## Conventions

- **The belt never starts moving without a user action in that moment** (Engage, GO/Resume,
  starting a program, a speed set by hand, a HUD or shortcut press, or a start at the treadmill
  with its own buttons or remote). The app never starts or
  resumes the belt by itself after a restart, a reconnect, an update, an unlock or a call
  ending; carrying on with a belt that never stopped is fine. The engine enforces it with
  `hold_auto_start` (set on each new connection, released once the deck is seen moving on it or
  the user asks for a start): while it holds, the automatic start and resume and the programs'
  `kick_resume` stay off, and a program that wants the belt moving waits, paused, for GO. A
  program never starts a still belt by itself either: running it is the press for its first
  step, but a later step that would take a still belt to moving (the next cycle after a rest, a
  walking step after a 0 km/h step) pauses it with `ActiveProgram.wait` set (`ProgramWait`) and
  sets the hold again until GO (or Engage, which is GO on a paused program). A start at the
  treadmill is a press too: on a paused or waiting program it is GO (`watch_for_deck_start`:
  the deck reported the belt still since the pause, then moving for `DECK_GO_DEBOUNCE`, with
  no app start in flight), and during a rest it becomes a manual walk. A pause at the treadmill on a
  walking program pauses the program with it (`notice_deck_pause`), so GO resumes the step. SKIP is a press and
  may start the belt; a step between two moving speeds stays automatic. A program that ends
  (END, SKIP on its last step, or into a rest) while the belt isn't moving leaves it still: the
  speed from before waits in `engage_speed` for the next Engage or RE-JACK. New calibration
  points (a settings sync, a point saved or deleted) never move a running belt: the deck keeps
  its byte (`CurveHold`) until the next speed change or step. Tests in `engine.rs`,
  `programs.rs` and `sim.rs` pin it.
- Before building a feature, run `node scripts/feature-inventory.mjs <word>` and grep the code for the behaviour itself (toast and card text, snapshot fields, settings); improve what exists instead of adding a near-duplicate.
- After editing any Rust file, run `cargo check` in `src-tauri/`.
- The Rust version is pinned in `rust-toolchain.toml`; bump it there and in the workflows together, and fix any new clippy lints in the same PR.
- Packet decoding and speed/calibration scaling belong in **Rust**, not JS.
- UI text is never smaller than 11px.
- No native dialogs (`alert`, `confirm`, `prompt`, or OS message boxes): confirmations use the
  store's `confirm` (`ConfirmDialog`) and results show under the control via `<ActionStatus>`;
  eslint enforces it.
- Prefer precise types; the repo passes `eslint` with no `no-explicit-any` and no lint
  suppressions.
- For Zustand effects that only *call* actions, read them via `useTreadmillStore.getState()`
  or select the stable action — don't depend on the whole store object.
