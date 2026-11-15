# Roadmap

## Next (small, ready to build)

The scheduled improvement task picks from here first. Remove an item when its PR merges.

## Unscheduled

The scheduled improvement task does not pick from here.

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
