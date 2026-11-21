import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Axis } from "./Axis";
import { sliderValue } from "./sliderValue";

/** The Live page's speed axis with a 0.93 km/h floor (the deck's 0.6 on a calibrated belt). */
function speedAxis(onSet: (v: number) => void, target = 3) {
  render(
    <Axis
      title="Velocity"
      tone="cyan"
      unit="km/h"
      actual={target}
      target={target}
      min={0.93}
      max={6.2}
      step={0.1}
      presets={[1.5, 3]}
      formatPreset={(v) => v.toFixed(1)}
      disabled={false}
      onSet={onSet}
      onNudge={() => {}}
      pin={{ widget: "VELOCITY", pinned: false }}
    />,
  );
  return screen.getByRole("slider");
}

describe("speed slider", () => {
  it("has the floor as its lowest setting", () => {
    const slider = speedAxis(() => {});
    expect(slider).toHaveAttribute("min", "0.93");
  });

  it("sends the floor, never 0, at its lowest position or mid-drag below it", () => {
    const onSet = vi.fn();
    const slider = speedAxis(onSet);
    // Dragged left: every value it sends on the way stays at or above the floor.
    for (const v of ["2.5", "1.4", "0.93", "0.5", "0"]) {
      fireEvent.change(slider, { target: { value: v } });
    }
    const sent = onSet.mock.calls.map(([v]) => v as number);
    expect(sent.length).toBeGreaterThan(0);
    expect(Math.min(...sent)).toBeGreaterThanOrEqual(0.93);
    expect(sent).not.toContain(0);
    expect(sent[sent.length - 1]).toBe(0.93);
  });

  it("keeps the 0.1 rounding inside the range", () => {
    expect(sliderValue("0.93", 0.93, 6.2)).toBe(0.93);
    expect(sliderValue("0", 0.93, 6.2)).toBe(0.93);
    expect(sliderValue("1.03", 0.93, 6.2)).toBe(1);
    expect(sliderValue("9", 0.93, 6.2)).toBe(6.2);
    expect(sliderValue("x", 0.93, 6.2)).toBe(0.93);
  });
});
