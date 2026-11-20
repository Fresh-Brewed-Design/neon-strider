# Neon Strider

Desktop app for controlling a FitShow Bluetooth treadmill, with a neon/cyberpunk UI,
an always-on-top HUD overlay, saved programs, session history, and Strava export.

## Stack

- **Tauri 2** (Rust backend) + **React 19** + **TypeScript** + **Vite 7**
- **Tailwind CSS 4**, **framer-motion**, **lucide-react**, **sonner**; Orbitron bundled via `@fontsource/orbitron`
- **ts-rs** generates TypeScript types for the IPC layer into `src/ipc/bindings/` (run `cargo test`)
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
types), `hud` (HUD config and display list), `commands` (what each command does) and `emitter`
(collecting and pushing the snapshot). The tray icon opens the dashboard; the windows are created
and placed by `dashboard.rs` and `hud.rs` (`display.rs` turns Tauri's monitors into `placement`
geometry); `hooks.rs` holds what the global input hooks record and the Alt shortcuts they fire.
Strava is Rust-owned too (`strava.rs` plus its `strava/` submodules: `settings`, `live`, `auth`,
`api`, `upload`, `worker` for the background auto-sync, and `fallback` for the manual-upload TCX;
`strava_plan.rs` pure logic, `strava_template.rs` title/description templates):
windows dispatch `strava_*` commands and read `snapshot.strava`; tokens never reach JS. The client secret and
tokens live in Windows Credential Manager (`secrets.rs`; plaintext DB values migrate at startup).
Auto-update is Rust-owned too (`update.rs`, `tauri-plugin-updater`; no window has updater
permissions and the CSP stays closed): it checks `latest.json` on the newest GitHub release 30 s
after startup and daily (hourly after a failure) or on `update_check`, joins the notes of every
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
Its walks stay in memory (no History, odometer or Strava) and it is never saved as the treadmill
to reconnect to, since dev and release builds share the database. Both modules are behind
`cfg(debug_assertions)`, so release builds never list it and `simulated` is always false.
Neural flow (typing-aware pace) is pure logic in `neural.rs`; key-press counting is switched by Rust (`neural_monitor_wanted`, 1 Hz tick), not by a window.
Status is visible, not silent. `link.rs` (treadmill link state, plain-English BLE errors) and
`strava_status.rs` (Strava activity/steps, `describe()` turns Strava's JSON errors into one
sentence) feed `snapshot.connection.link` and `snapshot.strava.activity`; countdowns are `*_s`
fields Rust recomputes each snapshot. Strava's no-API fallback is `strava_save.rs` (TCX file plus
the upload page). In the UI, every button that can fail or take time uses `useAction` /
`useBackendAction` (`src/components/useAction.ts`) and shows `<ActionStatus>` directly under
itself; `send()` toasts rejections as a backstop, the HUD uses `useHudSend` + `<HudNote>`.
GHOST_POD (`ghost_pod.rs`: pure RSC foot-pod encoding, Labs switch off by default; `ghost_pod/worker.rs`) advertises the belt's speed and cadence to a watch as a Running Speed and Cadence sensor. It only reports the belt; nothing from the watch moves it. See docs/watch-pod.md.

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
    header's minify button or Alt+Insert) it shows the running strip instead of the whole panel
    until expanded. Both are decided in `snapshot/stasis.rs` as `snapshot.hud_view`; cards and
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
  `snapshot/commands.rs`) over a new `#[tauri::command]`; settings and sessions still use named commands via `invoke` (plus `strava_tcx_preview`, a query).

## Platform

OS-specific code goes only in `src-tauri/src/platform/` (`mod.rs` is the interface, `windows.rs`
the Windows implementation, `other.rs` the stand-in for every other OS). It covers the input
hooks, session lock, display power (`display_off`, for `LIGHTS_OUT` in `snapshot/lights_out.rs`),
which apps use the microphone, the cursor and mouse button, the PC's idle time, and the BLE peripheral (the GHOST_POD running pod,
WinRT `GattServiceProvider`). The rest of
the code calls `platform::...` and never the OS directly. macOS and Linux are on the roadmap, so
new code must not import `windows::` (or other OS APIs) outside `platform/`.

## Commands

```
npm run dev       # Vite dev server (frontend only)
npm run tauri dev # Full desktop app (Rust + frontend)
npm run build     # tsc + vite build
npm run lint      # eslint (repo is clean: 0 problems)
npx tsc --noEmit  # typecheck
npx vitest run    # unit tests (vitest + jsdom)
cargo test        # in src-tauri/: Rust tests, and regenerates src/ipc/bindings
cargo fmt --check # in src-tauri/: formatting
cargo clippy --all-targets -- -D warnings   # in src-tauri/: lints, no warnings allowed
```

Design without a treadmill: with a Vite server running, open `/hud-preview.html` or
`/dashboard-preview.html`. Both run on a mocked backend (`src/dev/mockBackend.ts`); URL flags
like `?idle`, `?offline`, `?daily=12.34` (today's km), `?noprogram`, `?toasts` (`?toasts=deck` for the deck-start GO toast), `?anchor=top_left`, `?neural=active`, `?engage=timeout`, `?status=9|countdown|stopping|standby`, `?walkaway=warning|paused|idle`, `?custom`, `?update=available|downloading|failed|whatsnew` (with `?autoinstall=off`), `?stasis=strip|hidden` (with `?deck=paused` for RE-JACK), `?minified` (the running strip; with `?program=resting|paused`, `?noprogram` or `?idle`), `?traffic=recording|saved`, `?sim` (the SIM tag), `?decknote=refused|adjusted` (the HUD note after the deck refuses or changes a target), `?lightsout` (LIGHTS_OUT on; on the HUD preview its toast), `?desksync=open|new|few` (History's DESK_SYNC card), `?ghostpod=advertising|connected|error` (System -> Labs GHOST_POD), `?calwizard` (the Labs guided calibration; `=counting|counted` mid-run, `=sweep|sweeprun` RUN ALL between speeds or counting, `=oldpoint` a point stored below the factor limit), `?eyebreak=upcoming|rest|break|still|resume` (the OCULAR_IMPLANT cards: heads-up, heads-up set for a program rest, cooldown with the belt paused or left alone, resume card) pick scenarios.
`?program=ready|hold|still|resting|paused` shows a program waiting for GO after a rest, after a
0 km/h step, or after a reconnect found the belt still; mid-rest; or paused by the user.

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
`build-roadmap-issue.yml` builds a roadmap issue when Jeremy labels one of his own issues
`roadmap:next`: Claude opens a PR that closes it (secret `CLAUDE_CODE_OAUTH_TOKEN`). Only his
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
`TAURI_SIGNING_PRIVATE_KEY` secret (no password). The public key is in `tauri.conf.json`
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
  no app start in flight), and during a rest it becomes a manual walk. SKIP is a press and
  may start the belt; a step between two moving speeds stays automatic. A program that ends
  (END, SKIP on its last step, or into a rest) while the belt isn't moving leaves it still: the
  speed from before waits in `engage_speed` for the next Engage or RE-JACK. Tests in
  `engine.rs` and `programs.rs` pin it.
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
