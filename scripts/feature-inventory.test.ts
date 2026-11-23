// @ts-expect-error plain .mjs script, no types
import * as fi from "./feature-inventory.mjs";

interface Entry {
  path: string;
  line: number;
  name: string;
  detail: string;
}

// Every call scans the whole repo, and the "filters" test runs two full scans; that took about 4.5 s and hit the 5 s default
// when the PC was busy (installer builds running). 30 s is far above a loaded run and still ends a hung scan.
describe("feature-inventory", { timeout: 30_000 }, () => {
  it("finds the SESSION SAVED toast", () => {
    const toasts: Entry[] = fi.hudToasts();
    const saved = toasts.find((t) => t.detail.includes("SESSION SAVED"));
    expect(saved?.path).toBe("src-tauri/src/engine.rs");
    expect(saved?.name).toBe("STATUS");
  });

  it("finds HUD cards with their labels", () => {
    const cards: Entry[] = fi.hudCards();
    expect(cards.find((c) => c.name === "MaintenanceCard")?.detail).toContain("BELT SERVICE DUE");
  });

  it("finds snapshot fields and commands", () => {
    expect((fi.snapshotFields() as Entry[]).some((e) => e.name === "AppSnapshot.weekly")).toBe(true);
    expect((fi.commands() as Entry[]).some((e) => e.name === "Start")).toBe(true);
  });

  it("filters and never throws on an odd pattern", () => {
    const groups = fi.inventory("SESSION SAVED");
    expect(groups.find((g: { group: string }) => g.group === "HUD toasts").entries.length).toBeGreaterThan(0);
    expect(() => fi.format(fi.inventory("(unclosed"))).not.toThrow();
  });
});
