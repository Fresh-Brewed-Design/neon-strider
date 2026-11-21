import type { HudMinifyHotkey, HudMinifyKey } from "../ipc";

/** The keys the minify hotkey can use with Alt, as Rust lists them (`HudMinifyKey`). */
export const MINIFY_KEYS: { key: HudMinifyKey; name: string }[] = [
  { key: "insert", name: "Insert" },
  { key: "pause", name: "Pause" },
  { key: "scroll_lock", name: "Scroll Lock" },
];

/** "Alt + Insert", as the dashboard writes a shortcut. */
export function minifyKeyText(key: HudMinifyKey): string {
  return `Alt + ${MINIFY_KEYS.find((k) => k.key === key)?.name ?? "Insert"}`;
}

/** The hotkey for a HUD tooltip ("Alt+Insert"), or null while it is off. */
export function minifyTooltipKey(hotkey: HudMinifyHotkey): string | null {
  return hotkey.enabled ? minifyKeyText(hotkey.key).replace(/ \+ /, "+") : null;
}

/** A HUD button's tooltip with the hotkey after it, while there is one. */
export function hotkeyTitle(text: string, hotkey: HudMinifyHotkey): string {
  const key = minifyTooltipKey(hotkey);
  return key ? `${text} (${key})` : text;
}
