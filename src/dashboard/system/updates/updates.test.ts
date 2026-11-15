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
  it("badges an offer unless it was put off", () => {
    expect(showUpdateBadge(withStatus(available))).toBe(true);
    expect(showUpdateBadge(withStatus(available, true))).toBe(false);
    expect(showUpdateBadge(withStatus({ state: "up_to_date" }))).toBe(false);
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
    expect(installConfirmText(idle, "1.3.0")).toBe(
      "Neon Strider closes, installs v1.3.0 and opens again by itself, usually in under a minute.",
    );

    const manual = base();
    manual.deck.status = "Running";
    manual.active_program = null;
    const manualText = installConfirmText(manual, "1.3.0");
    expect(manualText).toMatch(/belt keeps going and the walk carries on as the same session\./);
    expect(manualText).toMatch(/few seconds.*HUD and shortcuts don't work.*treadmill's own controls/);
    expect(manualText).not.toMatch(/pause|RE-JACK/);

    const program = base();
    program.deck.status = "Running";
    expect(program.active_program).not.toBeNull();
    program.active_program = { ...program.active_program!, phase: "walking", name: "Workday" };
    const programText = installConfirmText(program, "1.3.0");
    expect(programText).toMatch(/Workday carries on at the same step/);
    expect(programText).toMatch(/treadmill's own controls/);
    expect(programText).not.toMatch(/pause/);

    program.active_program = { ...program.active_program, phase: "resting" };
    expect(installConfirmText(program, "1.3.0")).toMatch(/walk carries on as the same session, with Workday still resting/);

    program.deck.status = "Paused";
    expect(installConfirmText(program, "1.3.0")).toMatch(/Workday keeps resting; its next step waits for GO/);
  });

  it("renders notes as elements, never as HTML", () => {
    render(createElement(Markdown, { text: "## v1.3.0\n\n- **Bold** <b>tag</b>" }));
    expect(screen.getByRole("heading", { name: "v1.3.0" })).toBeInTheDocument();
    expect(screen.getByText("Bold").tagName).toBe("STRONG");
    expect(screen.getByText(/<b>tag<\/b>/)).toBeInTheDocument();
    expect(document.querySelector("b")).toBeNull();
  });
});
