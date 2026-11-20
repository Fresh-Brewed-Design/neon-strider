import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { DeskSync } from "../../ipc";
import { DeskSyncPanel } from "./DeskSync";
import { deskSyncText } from "./deskSyncText";

function sync(walked: boolean[], today_open = false): DeskSync {
  return {
    walked: walked.filter(Boolean).length,
    desk_days: walked.length,
    window: 14,
    days: walked.map((w, i) => ({ date: `2026-11-${String(i + 1).padStart(2, "0")}`, walked: w })),
    today_open,
  };
}

describe("DESK_SYNC", () => {
  it("says how many recent desk days were walked on", () => {
    const s = sync([true, true, false, true, true, true, true, true, true, true]);
    render(<DeskSyncPanel sync={s} />);
    expect(screen.getByText("Walked on 9 of your last 10 desk days.")).toBeTruthy();
    expect(screen.getByText("/ 10")).toBeTruthy();
    expect(screen.getAllByRole("listitem")).toHaveLength(10);
    expect(screen.getAllByLabelText(/no walk$/)).toHaveLength(1);
  });

  it("shows today as open until it is walked, without counting it", () => {
    render(<DeskSyncPanel sync={sync([true, false], true)} />);
    expect(screen.getByText("Walked on 1 of your last 2 desk days.")).toBeTruthy();
    expect(screen.getByLabelText("Today: not walked yet")).toBeTruthy();
  });

  it("has plain text for one day and for none", () => {
    expect(deskSyncText(sync([true]))).toBe("Walked on your last desk day.");
    expect(deskSyncText(sync([false]))).toBe("No walk on your last desk day.");
    expect(deskSyncText(sync([]))).toBe("No desk days recorded yet.");
    expect(deskSyncText(sync([], true))).toBe("Walk today to start the desk-day count.");
  });

  it("never celebrates", () => {
    const { container } = render(<DeskSyncPanel sync={sync(Array<boolean>(14).fill(true))} />);
    expect(container.textContent).not.toMatch(/best|record|streak/i);
  });
});
