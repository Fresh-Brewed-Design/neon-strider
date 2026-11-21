import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { createElement } from "react";
import { snapshot as mock } from "../../../dev/mockBackend";
import type { AppSnapshot, UpdateSnapshot } from "../../../ipc";
import { Markdown } from "./Markdown";
import { installConfirmText, installProgress, showUpdateBadge, updateBusy } from "./updates";

const base = (): AppSnapshot => structuredClone(mock);

const withStatus = (status: UpdateSnapshot["status"], dismissed = false): UpdateSnapshot => ({
  ...base().update,
  status,
  dismissed,
});

const available = { state: "available", version: "1.3.0", notes: "## v1.3.0", published: null } as const;

describe("update helpers", () => {
  it("shows the dot exactly when Rust says so", () => {
    // Rust decides (update::show_update_dot); the dashboard only reads it.
    expect(showUpdateBadge({ ...withStatus(available), dot: true })).toBe(true);
    expect(showUpdateBadge({ ...withStatus(available, true), dot: false })).toBe(false);
    expect(showUpdateBadge({ ...withStatus({ state: "up_to_date" }), dot: false })).toBe(false);
    expect(showUpdateBadge(undefined)).toBe(false);
  });

  it("reports progress while busy", () => {
    expect(updateBusy(withStatus({ state: "checking" }))).toBe(true);
    expect(updateBusy(withStatus(available))).toBe(false);
    expect(installProgress(withStatus({ state: "downloading", percent: 42 }))).toBe("Downloading 42%…");
    expect(installProgress(withStatus({ state: "downloading", percent: null }))).toBe("Downloading…");
    expect(installProgress(withStatus({ state: "installing" }))).toMatch(/opens again/);
    expect(installProgress(withStatus(available))).toBeNull();
  });

  it("says the walk carries on and the belt is never paused", () => {
    const idle = base();
    idle.deck.status = "Stopped";
    idle.active_program = null;
    expect(installConfirmText(idle, "1.3.0")).toEqual({
      message: "Neon Strider closes, installs v1.3.0 and opens again, usually in under a minute.",
    });

    const manual = base();
    manual.deck.status = "Running";
    manual.active_program = null;
    const manualText = installConfirmText(manual, "1.3.0");
    expect(manualText.message).toMatch(/installs v1\.3\.0/);
    expect(manualText.hint).toMatch(/belt keeps going and the walk carries on as the same session\./);
    expect(manualText.hint).toMatch(/HUD and shortcuts stop working for a few seconds.*treadmill's own controls/);
    expect(manualText.hint).not.toMatch(/pause|RE-JACK/);

    const program = base();
    program.deck.status = "Running";
    expect(program.active_program).not.toBeNull();
    program.active_program = { ...program.active_program!, phase: "walking", name: "Workday" };
    const programText = installConfirmText(program, "1.3.0");
    expect(programText.hint).toMatch(/Workday carries on at the same step/);
    expect(programText.hint).toMatch(/treadmill's own controls/);
    expect(programText.hint).not.toMatch(/pause/);

    program.active_program = { ...program.active_program, phase: "resting" };
    expect(installConfirmText(program, "1.3.0").hint).toMatch(/walk carries on as the same session, with Workday still resting/);

    program.deck.status = "Paused";
    expect(installConfirmText(program, "1.3.0").hint).toBe("Workday keeps resting and the next step waits for GO on the HUD.");

    // One sentence, no semicolons or dashes, in every state.
    for (const text of [manualText, programText, installConfirmText(idle, "1.3.0")]) {
      expect(text.message.match(/[.!?](s|$)/g)).toHaveLength(1);
      expect(`${text.message} ${text.hint ?? ""}`).not.toMatch(/[;–—]| - /);
    }
  });

  it("renders notes as elements, never as HTML", () => {
    render(createElement(Markdown, { text: "## v1.3.0\n\n- **Bold** <b>tag</b>" }));
    expect(screen.getByRole("heading", { name: "v1.3.0" })).toBeInTheDocument();
    expect(screen.getByText("Bold").tagName).toBe("STRONG");
    expect(screen.getByText(/<b>tag<\/b>/)).toBeInTheDocument();
    expect(document.querySelector("b")).toBeNull();
  });
});
