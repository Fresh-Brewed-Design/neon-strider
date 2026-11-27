import { render, screen } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";
import type { AppSnapshot } from "../ipc";

const { snapshot: mock } = await import("../dev/mockBackend");
const { Controls } = await import("./HudPanel");
const { RunStripRow } = await import("./HudStrip");
const { MaintenanceCard, WalkAwayCard } = await import("./HudCards");

beforeAll(() => {
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});

/** A connected, moving belt with no program, then `patch`. */
function walking(patch: (s: AppSnapshot) => void = () => {}): AppSnapshot {
  const s = structuredClone(mock);
  s.deck = { ...s.deck, status: "Running", speed_kmh: 3.2 };
  s.active_program = null;
  patch(s);
  return s;
}

// Colour roles (#391): cyan is on, selected and speed; magenta is tilt and destructive actions only;
// yellow is programs and warnings. DUMP ends the walk, so it no longer looks like STASIS.
describe("HUD colour roles", () => {
  it("keeps STASIS cyan while the belt moves and gives DUMP its own style", () => {
    render(<Controls snapshot={walking()} send={async () => true} />);
    const stasis = screen.getByRole("button", { name: "STASIS pause belt" });
    expect(stasis).toHaveClass("accent-cyan");
    expect(stasis).not.toHaveClass("accent-magenta");
    const dump = screen.getByRole("button", { name: "DUMP end walk" });
    expect(dump).toHaveClass("hud-btn-dump");
    expect(dump.querySelector(".hud-dump-glyph")).not.toBeNull();
  });

  it("styles the running strip's DUMP the same", () => {
    render(<RunStripRow snapshot={walking()} send={async () => true} note={null} />);
    expect(screen.getByRole("button", { name: "STASIS pause belt" })).toHaveClass("accent-cyan");
    const dump = screen.getByRole("button", { name: "DUMP end walk" });
    expect(dump).toHaveClass("hud-btn-dump");
    expect(dump.querySelector(".hud-dump-glyph")).not.toBeNull();
  });

  it("draws the walk-away card as a warning with a cyan RE-JACK over DUMP", () => {
    const { container } = render(
      <WalkAwayCard
        snapshot={walking((s) => {
          s.deck = { ...s.deck, status: "Paused", speed_kmh: 0 };
          s.walkaway = { ...s.walkaway, card: "paused", reason: "idle", idle_minutes: 10 };
        })}
      />,
    );
    expect(container.firstElementChild).toHaveClass("accent-yellow");
    expect(screen.getByRole("button", { name: "RE-JACK" })).toHaveClass("accent-cyan", "hud-btn-primary");
    expect(screen.getByRole("button", { name: "DUMP end walk" })).toHaveClass("hud-btn-dump");
  });

  it("draws the belt service card as a warning", () => {
    const { container } = render(<MaintenanceCard snapshot={walking()} />);
    expect(container.firstElementChild).toHaveClass("accent-yellow");
  });
});

const css = readFileSync(join(process.cwd(), "src/hud/hud.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
/** Every rule body for `selector`, joined: a selector may have more than one rule. */
const body = (selector: string) =>
  css
    .split("}")
    .map((chunk) => chunk.split("{"))
    .filter(([head, rest]) => rest !== undefined && head.trim() === selector)
    .map(([, rest]) => rest)
    .join(";");

describe("DUMP's look in the HUD CSS", () => {
  it("rests with a 2px magenta outline on the dark fill, without changing its box", () => {
    const rest = body(".hud-btn.hud-btn-dump");
    expect(rest).toMatch(/border-color:\s*var\(--hud-magenta\)/);
    expect(rest).toMatch(/box-shadow:\s*inset 0 0 0 1px var\(--hud-magenta\)/);
    expect(rest).toMatch(/background:\s*var\(--hud-bg\)/);
    expect(rest).not.toMatch(/border-width/);
  });

  it("turns solid magenta with a dark label while pressed and drops the inner ring when off", () => {
    expect(body(".hud-btn.hud-btn-dump:active:not(:disabled)")).toMatch(/background:\s*var\(--hud-magenta\)/);
    expect(body(".hud-btn.hud-btn-dump:disabled")).toMatch(/box-shadow:\s*none/);
  });

  it("leaves 12px on each side of DUMP", () => {
    expect(body(".hud-transport")).toMatch(/gap:\s*8px 12px/);
    expect(body(".hud-run-strip .hud-strip-row")).toMatch(/column-gap:\s*12px/);
    expect(body(".hud-walkaway .hud-walkaway-actions")).toMatch(/gap:\s*12px/);
  });
});
