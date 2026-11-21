import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { WallClock } from "../../ipc";
import { WallClockPanel } from "./WallClock";
import { wallClockText } from "./wallClockText";

function clock(lit: Record<number, number>, desk_days: number, first_hour = 7, last_hour = 19): WallClock {
  const hours = Array<number>(24).fill(0);
  for (const [h, n] of Object.entries(lit)) hours[Number(h)] = n;
  return { hours, desk_days, window: 14, first_hour, last_hour };
}

describe("WALL_CLOCK", () => {
  it("shows one cell per hour from the first to the last hour", () => {
    render(<WallClockPanel clock={clock({ 9: 10, 10: 3 }, 12)} />);
    expect(screen.getAllByRole("listitem")).toHaveLength(13);
    expect(screen.getByLabelText("09:00: walked on 10 of 12 desk days")).toBeTruthy();
    expect(screen.getByLabelText("10:00: walked on 3 of 12 desk days")).toBeTruthy();
    expect(screen.getAllByLabelText(/no walks$/)).toHaveLength(11);
    expect(screen.getByText("Brighter hours had walks on more of your last 12 desk days.")).toBeTruthy();
  });

  it("widens to a late hour Rust reports", () => {
    render(<WallClockPanel clock={clock({ 21: 1 }, 5, 7, 21)} />);
    expect(screen.getAllByRole("listitem")).toHaveLength(15);
    expect(screen.getByLabelText("21:00: walked on 1 of 5 desk days")).toBeTruthy();
  });

  it("has an empty state with no cells", () => {
    render(<WallClockPanel clock={clock({}, 0)} />);
    expect(screen.getByText("No desk days recorded yet.")).toBeTruthy();
    expect(screen.queryAllByRole("listitem")).toHaveLength(0);
  });

  it("has plain text for one day and for no walks", () => {
    expect(wallClockText(clock({ 9: 1 }, 1))).toBe("Lit hours had a walk on your last desk day.");
    expect(wallClockText(clock({}, 4))).toBe("No walks on your last 4 desk days.");
  });

  it("never celebrates", () => {
    const { container } = render(<WallClockPanel clock={clock({ 9: 14, 10: 14 }, 14)} />);
    expect(container.textContent).not.toMatch(/best|record|streak|total/i);
  });
});
