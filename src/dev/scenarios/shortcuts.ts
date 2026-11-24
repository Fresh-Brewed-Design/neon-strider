// System -> Shortcuts: each OS's default keys (Windows, or ⌃⌥ with ?mac),
// recorded, removed and reset like Rust does (its rules are only checked in
// Rust; the mock takes any key with a modifier).
// ?minifykey=off|pause|scroll_lock: the minify shortcut switched off, or on
// Alt+Pause or Alt+Scroll Lock (Windows).
// ?shortcuts=changed: Start or pause moved to Ctrl+Alt+S (⌃⌥S), so RESET shows;
// =taken: on a Mac, ⌃⌥P held by another app (crossed out).
import type { KeyCombo, Shortcut, ShortcutView } from "../../ipc";
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

const ORDER: Shortcut[] = [
  "speed_up",
  "speed_down",
  "incline_up",
  "incline_down",
  "toggle_start",
  "toggle_program",
  "toggle_minified",
  "calibration_tap",
];

const NAMES: Record<string, [windows: string, mac: string]> = {
  Equal: ["=", "="],
  Minus: ["-", "-"],
  PageUp: ["Page Up", "⇞"],
  PageDown: ["Page Down", "⇟"],
  ArrowUp: ["Up", "↑"],
  ArrowDown: ["Down", "↓"],
  Home: ["Home", "↖"],
  End: ["End", "↘"],
  Enter: ["Enter", "↩"],
  Insert: ["Insert", "Insert"],
  Pause: ["Pause", "Pause"],
  ScrollLock: ["Scroll Lock", "Scroll Lock"],
  Space: ["Space", "Space"],
  AudioVolumeUp: ["Volume Up", "Volume Up"],
  AudioVolumeDown: ["Volume Down", "Volume Down"],
};

const mac = () => snapshot.platform.os === "macos";

function keyName(code: string): string {
  const named = NAMES[code];
  if (named) return named[mac() ? 1 : 0];
  return code.replace(/^Key|^Digit/, "");
}

/** As Rust's `keys::label` writes it. */
function label(c: KeyCombo): string {
  if (mac()) {
    return `${c.ctrl ? "⌃" : ""}${c.alt ? "⌥" : ""}${c.shift ? "⇧" : ""}${c.meta ? "⌘" : ""}${keyName(c.code)}`;
  }
  const mods = [c.ctrl && "Ctrl", c.alt && "Alt", c.shift && "Shift", c.meta && "Win"].filter(Boolean);
  return [...mods, keyName(c.code)].join("+");
}

const combo = (code: string, ctrl: boolean, alt: boolean): KeyCombo => ({ code, ctrl, alt, shift: false, meta: false });

function defaults(): Record<Shortcut, KeyCombo[]> {
  if (mac()) {
    const k = (code: string) => [combo(code, true, true)];
    return {
      speed_up: k("Equal"),
      speed_down: k("Minus"),
      incline_up: k("ArrowUp"),
      incline_down: k("ArrowDown"),
      toggle_start: k("Enter"),
      toggle_program: k("KeyP"),
      toggle_minified: k("KeyM"),
      calibration_tap: k("Space"),
    };
  }
  const alt = (code: string) => combo(code, false, true);
  return {
    speed_up: [alt("Equal"), alt("AudioVolumeUp")],
    speed_down: [alt("Minus"), alt("AudioVolumeDown")],
    incline_up: [alt("PageUp")],
    incline_down: [alt("PageDown")],
    toggle_start: [alt("Home")],
    toggle_program: [alt("End")],
    toggle_minified: [alt("Insert")],
    calibration_tap: [alt("Space")],
  };
}

let taken: KeyCombo[] = [];

function show(bindings: Record<Shortcut, KeyCombo[]>) {
  const base = defaults();
  const list: ShortcutView[] = ORDER.map((shortcut) => ({
    shortcut,
    keys: bindings[shortcut].map((c) => ({
      combo: c,
      label: label(c),
      taken: taken.some((t) => label(t) === label(c)),
    })),
  }));
  snapshot.shortcuts = {
    ...snapshot.shortcuts,
    list,
    changed: JSON.stringify(bindings) !== JSON.stringify(base),
  };
}

function current(): Record<Shortcut, KeyCombo[]> {
  const out = defaults();
  for (const v of snapshot.shortcuts.list) out[v.shortcut] = v.keys.map((k) => k.combo);
  return out;
}

export const shortcuts: Scenario = {
  apply() {
    const b = defaults();
    const minify = params.get("minifykey");
    if (!mac()) {
      if (minify === "off") b.toggle_minified = [];
      else if (minify === "pause") b.toggle_minified = [combo("Pause", false, true)];
      else if (minify === "scroll_lock") b.toggle_minified = [combo("ScrollLock", false, true)];
    }
    const flag = params.get("shortcuts");
    if (flag === "changed") b.toggle_start = [combo("KeyS", true, true)];
    if (flag === "taken" && mac()) taken = [combo("KeyP", true, true)];
    show(b);
  },
  commands: {
    shortcuts_set: ({ shortcut, keys }) => {
      const b = current();
      b[shortcut] = keys;
      show(b);
    },
    shortcuts_reset: () => show(defaults()),
    shortcuts_recording: ({ on }) => (snapshot.shortcuts = { ...snapshot.shortcuts, recording: on }),
  },
};
