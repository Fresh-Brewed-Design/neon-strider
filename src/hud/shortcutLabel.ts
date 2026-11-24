import type { Shortcut, ShortcutsSnapshot } from "../ipc";

/**
 * A shortcut's first key as the OS writes it ("Alt+Insert", "⌃⌥M"), or null
 * while it has none (switched off in System -> Shortcuts).
 */
export function shortcutLabel(shortcuts: ShortcutsSnapshot, shortcut: Shortcut): string | null {
  return shortcuts.list.find((s) => s.shortcut === shortcut)?.keys[0]?.label ?? null;
}

/** A HUD button's tooltip with the minify shortcut after it, while there is one. */
export function hotkeyTitle(text: string, shortcuts: ShortcutsSnapshot): string {
  const key = shortcutLabel(shortcuts, "toggle_minified");
  return key ? `${text} (${key})` : text;
}
