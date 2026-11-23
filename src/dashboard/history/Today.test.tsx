import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { TodayTimeline } from "../../ipc";
import { TodayPanel } from "./Today";
import { clockLabel, hourTicks, todayText } from "./todayText";

const day: TodayTimeline = {
  start_min: 480,
  now_min: 865,
  slot_min: 5,
  runs: [
    { start_min: 480, end_min: 555, mark: "desk" },
    { start_min: 555, end_min: 590, mark: "walked" },
    { start_min: 590, end_min: 605, mark: "away" },
    { start_min: 605, end_min: 865, mark: "desk" },
  ],
};

describe("Today timeline", () => {
  it("draws one mark per run across the span", () => {
    render(<TodayPanel timeline={day} />);
    const runs = screen.getAllByRole("listitem").filter((li) => li.getAttribute("aria-label")?.includes(" to "));
    expect(runs).toHaveLength(4);
    const walked = screen.getByLabelText("09:15 to 09:50: walking");
    expect(walked.style.left).toMatch(/^19\.48/);
    expect(walked.style.width).toMatch(/^9\.09/);
    expect(screen.getByLabelText("10:05 to 14:25: at the PC")).toBeTruthy();
    expect(screen.getByText("From 08:00 to 14:25, in 5-minute slots.")).toBeTruthy();
  });

  it("labels whole hours, never more than six", () => {
    expect(hourTicks(day)).toEqual([8, 10, 12, 14]);
    expect(hourTicks({ ...day, start_min: 400, now_min: 1290 })).toEqual([9, 12, 15, 18, 21]);
    expect(hourTicks({ ...day, start_min: 480, now_min: 502 })).toEqual([8]);
    expect(hourTicks({ ...day, start_min: 485, now_min: 502 })).toEqual([]);
    expect(hourTicks({ ...day, start_min: 0, now_min: 1440 }).length).toBeLessThanOrEqual(6);
  });

  it("writes clock times", () => {
    expect(clockLabel(0)).toBe("00:00");
    expect(clockLabel(865)).toBe("14:25");
    expect(clockLabel(1440)).toBe("24:00");
    expect(todayText({ ...day, start_min: 1435, now_min: 1440 })).toBe("From 23:55 to 24:00, in 5-minute slots.");
  });

  it("never celebrates", () => {
    const { container } = render(<TodayPanel timeline={day} />);
    expect(container.textContent).not.toMatch(/best|record|streak|goal|great/i);
  });
});
