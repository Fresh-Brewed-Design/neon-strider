# Contributing to Neon Strider

Thanks for helping out. Bug reports and feature ideas go in
[Issues](../../issues); please use the templates.

## Prerequisites

- Windows 10 or 11 (the app is Windows only)
- Node 20+ and npm
- Rust stable
- The [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) for Windows

```bash
npm install
npm run tauri dev   # full app
npm run dev         # frontend only
```

`npm run tauri build` needs no signing key: updater artifacts are only made by the release
workflow, which holds the key. You can work on the UI without a treadmill: with `npm run dev` running, open
`/dashboard-preview.html` or `/hud-preview.html` (mocked backend).

## Checks

All of these must pass before you open a PR:

```bash
npx tsc --noEmit
npx eslint .          # 0 problems
npx vitest run
npx vite build
cd src-tauri
cargo test            # also regenerates src/ipc/bindings; commit any changes
cargo check
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

OS-specific code belongs only in `src-tauri/src/platform/`; everywhere else calls
`platform::...` (see the Platform section of `CLAUDE.md`).

## UI rules

- Text is never smaller than 11 px.
- Nothing overflows or gets clipped at the 760×600 dashboard size or the 340 px wide HUD.
- Errors appear next to the control that caused them, visible without scrolling
  (`useAction` with `<ActionStatus>`; the HUD uses `useHudSend` with `<HudNote>`).

## Screenshots in UI pull requests

A PR that changes what the dashboard or HUD looks like shows it in its description: a
screenshot of each changed screen or state, with a "before" beside it when something existing
changed. Capture them from the mocked previews (`scripts/screenshots.py` shows how; dashboard at
760x600, HUD cropped to its panel), so they contain no personal data. Drag the images into the
PR description on GitHub, or push them to the `pr-screenshots` branch under `pr-<number>/` and
link them; don't commit them to the PR's own branch. The README images in `docs/img/` are
regenerated only when the screens they show changed.

## Release notes

Each PR adds one file `changes/<new|changed|fixed>-<slug>.md` containing `- ` bullet lines
written for users (see `changes/README.md`). Never edit `CHANGELOG.md`; the release does.

## Releases

Label the PR `release:major`, `release:minor` or `release:patch` (no label means patch;
`release:none` leaves it out of the version). Merging to `main` releases everything merged
since the last tag, so never bump versions or tag by hand.

## Architecture notes

`CLAUDE.md` holds the architecture notes and conventions. They apply to human contributors and
to AI assistants alike. The FitShow protocol notes are in [docs/protocol.md](docs/protocol.md).

By contributing you agree your work is released under the [MIT License](LICENSE).
