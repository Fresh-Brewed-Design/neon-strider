# macOS

Neon Strider runs on macOS 13 (Ventura) or later as a menu bar app
([#33](https://github.com/Fresh-Brewed-Design/neon-strider/issues/33)): it builds, runs and
controls the treadmill, and feels like a Mac app. Every release has a universal disk image (Apple
Silicon and Intel), and installed copies update themselves; see Installing below, or build it
from source.

## Installing

1. Download `Neon Strider_<version>_universal.dmg` from the
   [GitHub Releases](https://github.com/Fresh-Brewed-Design/neon-strider/releases) page, open it
   and drag Neon Strider to Applications.
2. Open it. The app is ad-hoc signed, not notarized, so the first time macOS refuses: open System
   Settings, Privacy & Security, and choose Open Anyway (on macOS 14 and earlier, right-click the
   app and choose Open also works).
3. Updates install themselves like on Windows: the new app replaces the old one in Applications
   and reopens (System, Updates). Because an ad-hoc signature changes with each version, macOS
   may ask for Bluetooth and Input Monitoring again after an update.

## What it does on a Mac

- **Menu bar app.** No Dock icon and no ⌘-Tab entry. The icon is a template image, so macOS tints it
  for a light or dark menu bar: chevrons when connected, chevrons with the belt line under them
  while the belt runs, faint chevrons offline, and a dot when an update waits. Click it for the
  dashboard, which drops down under the icon and closes when you click elsewhere, like other menu bar
  apps. Right-click (or Control-click) for the menu.
- **HUD.** Floats over every app, on every Space and over full-screen apps, and stays out of
  ⌘-Tab and window cycling. It lets clicks through except over its panel, and the first click on it
  presses the button. Drag it by its logo as on Windows.
- **Shortcuts** (⌃⌥ plus a key by default; no permission needed). Change them in System,
  Shortcuts: ADD records a key (each needs ⌃ or ⌘; ⌥ and ⇧ can join them), × removes one, and a
  shortcut can have two keys or none:

  | Keys | Action |
  | --- | --- |
  | ⌃⌥ `=` | Speed up |
  | ⌃⌥ `-` | Speed down |
  | ⌃⌥ ↑ | Incline up |
  | ⌃⌥ ↓ | Incline down |
  | ⌃⌥ ↩ | Start / pause (GO on a paused program) |
  | ⌃⌥ P | End the running program, or rerun the last one |
  | ⌃⌥ M | Shrink the HUD to a strip, or show it again |
  | ⌃⌥ Space | GROUND_TRUTH tap, while a tap run listens |

  ⌥ alone types characters on a Mac (⌥= is ≠), and Macs have no Home, End or Page keys, so the
  Windows keys don't carry over. A key another app already owns shows crossed out in System,
  Shortcuts; pick another.
- **Strava** login and client secret live in the Keychain.
- **Walk-away** pauses on a screen lock (⌃⌘Q), **LIGHTS_OUT** stops the belt when the display
  sleeps, and the idle nudge, walk-away timer and auto-install read the Mac's idle time.
- **Neural flow** (and Labs' typing by speed) counts typing only after macOS grants **Input
  Monitoring**: the first time the count is wanted, macOS asks. Until you allow it, nothing is
  counted, and a note under the switch has OPEN SETTINGS. Keys are counted, never recorded.
- **Slow down on calls** asks Core Audio which apps take microphone input: by name on macOS 14.2
  and later, or just "an app" on macOS 13. Only that list is read; no audio, no permission.
- **Open at login** (System, Startup) is a Login Item, listed in System Settings, General, Login
  Items. If macOS wants it approved there, the switch opens that page and says so. An earlier
  build's LaunchAgent entry moves over by itself.
- **Updates** replace the app and reopen it. The belt keeps going meanwhile, as on Windows.

Not on macOS yet: **GHOST_POD** (it says it works on Windows only).

## Permissions

| macOS asks for | When | Why |
| --- | --- | --- |
| Bluetooth | First scan or connect | The treadmill and heart rate sensor |
| Input Monitoring | Neural flow first wants the typing count | Counting key presses |

System, Permissions shows where each stands (allowed, not allowed, not asked yet), with OPEN
SETTINGS for its pane in Privacy & Security. Nothing else needs a permission: the shortcuts, the
HUD's click-through, call detection, idle time and lock state don't.

## Building and running

Requirements: macOS 13+, Xcode Command Line Tools (`xcode-select --install`), Node 20+, and Rust
(`rust-toolchain.toml` pins the version; rustup installs it).

```bash
npm install
npm run tauri dev
```

Dev builds list `SIM_DECK // simulated treadmill` in the Bluetooth scan, so the whole app can be
tried without the treadmill. The first scan makes macOS ask for Bluetooth for the app that
started it (your terminal, in dev).

To build the app and a disk image:

```bash
npm run tauri build   # src-tauri/target/release/bundle/macos/ and bundle/dmg/
```

The build is ad-hoc signed, not notarized: the first time, open it with right-click, Open (or
System Settings, Privacy & Security, Open Anyway). Because the signature changes with each build,
macOS asks for Bluetooth again after each new build.

## How it's put together

- `src-tauri/src/platform/macos.rs`: the platform boundary on macOS (AppKit mouse monitors for
  the HUD's click-through, Carbon hot keys, the Input Monitoring key tap, CoreGraphics idle time,
  lock and display sleep, Core Audio's input processes, SMAppService for the Login Item, and the
  permission states). `macos_keys.rs` holds the typing rules and `keys.rs` the shortcut key table
  and rules for every OS, all pure and tested on every OS.
- Shortcuts are `src-tauri/src/shortcuts.rs` (saved keys, rules, recording); each OS applies them
  (`platform::set_shortcuts`: the Windows hook's table, or re-registered Carbon hot keys).
- Desktop units are points on macOS (`platform::DESKTOP_IN_POINTS`): `display.rs` converts at
  Tauri's edge, so `placement`'s geometry is the same code on both.
- `snapshot.platform` tells the windows the OS, how it names things ("this Mac", System
  Settings), which OS-only features it has and the permissions; `snapshot.shortcuts` the keys as
  the OS writes them. Preview them with `?mac` (`=denied` for a refused Input Monitoring) and
  `?shortcuts=changed|taken` on `/dashboard-preview.html` or `/hud-preview.html`.
- `src-tauri/Info.plist` (Bluetooth usage text, `LSUIElement`) and `tauri.macos.conf.json`
  (app and dmg targets, macOS 13 minimum, ad-hoc signing).
- CI runs the Rust tests and clippy on a Mac (`macos` job in `ci.yml`).

## Releases

`release.yml`'s `macos` job runs after the Windows job and adds to the same release: a universal
build (`--target universal-apple-darwin`), its disk image, and the signed updater bundle
(`.app.tar.gz` and `.sig`). tauri-action reads the `latest.json` the Windows job uploaded and adds
`darwin-aarch64` and `darwin-x86_64`, both pointing at the universal bundle; running after the
Windows job keeps the two from writing it at once. A failed Mac build leaves the Windows release
as it is.

Notarizing (no Gatekeeper prompt, and permissions that survive updates) needs an Apple Developer
ID; with one, `signingIdentity` in `tauri.macos.conf.json` and the notarization secrets are the
only changes.
