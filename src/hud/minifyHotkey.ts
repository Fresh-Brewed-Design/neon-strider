import type { HudMinifyHotkey, HudMinifyKey, PlatformSnapshot } from "../ipc";

/** The keys the minify hotkey can use with Alt, as Rust lists them (`HudMinifyKey`). */
export const MINIFY_KEYS: { key: HudMinifyKey; name: string }[] = [
  { key: "insert", name: "Insert" },
  { key: "pause", name: "Pause" },
  { key: "scroll_lock", name: "Scroll Lock" },
];

/**
 * "Alt + Insert", as the dashboard writes a shortcut. Where the key can't be
 * chosen (`platform.minify_key_choice` off: macOS) it is the OS's own, "⌃⌥M".
 */
export function minifyKeyText(key: HudMinifyKey, platform: PlatformSnapshot): string {
  if (!platform.minify_key_choice) return platform.shortcuts.toggle_minified;
  return `Alt + ${MINIFY_KEYS.find((k) => k.key === key)?.name ?? "Insert"}`;
}

/** The hotkey for a HUD tooltip ("Alt+Insert", "⌃⌥M"), or null while it is off. */
export function minifyTooltipKey(hotkey: HudMinifyHotkey, platform: PlatformSnapshot): string | null {
  return hotkey.enabled ? minifyKeyText(hotkey.key, platform).replace(/ \+ /, "+") : null;
}

/** A HUD button's tooltip with the hotkey after it, while there is one. */
export function hotkeyTitle(text: string, hotkey: HudMinifyHotkey, platform: PlatformSnapshot): string {
  const key = minifyTooltipKey(hotkey, platform);
  return key ? `${text} (${key})` : text;
}
