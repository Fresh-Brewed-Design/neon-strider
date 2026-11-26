// The System page's four group tabs, which sections each one shows, and the jump
// other parts of the dashboard use to open a section on its tab.

import { readPref, writePref } from "../localPrefs";

export type SystemTab = "deck" | "links" | "routine" | "core";

/** Each tab with its sections, in the order they show. Permissions only shows where the OS asks for them (macOS). */
export const SYSTEM_TABS: readonly { id: SystemTab; label: string; sections: readonly string[] }[] = [
  { id: "deck", label: "DECK", sections: ["sys-treadmill", "sys-calibration", "sys-drift", "sys-maintenance"] },
  { id: "links", label: "LINKS", sections: ["sys-strava", "sys-heart"] },
  // AUTOMATION keeps the id "routine", so the tab remembered from before still opens.
  { id: "routine", label: "AUTOMATION", sections: ["sys-neural", "sys-sessions", "sys-desk"] },
  {
    id: "core",
    label: "CORE",
    sections: ["sys-startup", "sys-shortcuts", "sys-permissions", "sys-updates", "sys-labs", "sys-diagnostics"],
  },
];

export const SYSTEM_TAB_KEY = "system.tab";

/** The tab that holds a section, or null for an id no tab shows. */
export function tabOfSection(sectionId: string): SystemTab | null {
  return SYSTEM_TABS.find((t) => t.sections.includes(sectionId))?.id ?? null;
}

/** The tab open last time, DECK when none is remembered. */
export function readSystemTab(): SystemTab {
  const saved = readPref(SYSTEM_TAB_KEY);
  return SYSTEM_TABS.find((t) => t.id === saved)?.id ?? "deck";
}

type Listener = (tab: SystemTab) => void;
const listeners = new Set<Listener>();

/** The System page follows tab changes asked for from elsewhere while it is open. */
export function subscribeSystemTab(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Opens a tab and remembers it. */
export function openSystemTab(tab: SystemTab): void {
  writePref(SYSTEM_TAB_KEY, tab);
  for (const l of listeners) l(tab);
}

/**
 * Opens the tab that holds a section, then scrolls to the section. The caller
 * shows the System page first; 80 ms is a frame or so for it to render.
 */
export function showSystemSection(sectionId: string): void {
  const tab = tabOfSection(sectionId);
  if (tab) openSystemTab(tab);
  setTimeout(() => document.getElementById(sectionId)?.scrollIntoView({ block: "start" }), 80);
}
