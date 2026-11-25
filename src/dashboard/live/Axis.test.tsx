import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Axis } from "./Axis";
import { sliderValue, thumbCentre, trackPct } from "./sliderValue";

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

describe("slider access", () => {
  it("is named after its axis", () => {
    expect(speedAxis(() => {})).toHaveAccessibleName("Velocity target");
  });

  it("is really disabled when offline", () => {
    render(
      <Axis
        title="Tilt"
        tone="magenta"
        unit="%"
        actual={1}
        target={1}
        min={0}
        max={10}
        step={0.5}
        presets={[]}
        formatPreset={(v) => `${v}%`}
        disabled
        onSet={() => {}}
        onNudge={() => {}}
        pin={{ widget: "INCLINE", pinned: false }}
      />,
    );
    expect(screen.getByRole("slider", { name: "Tilt target" })).toBeDisabled();
  });

  it("shows the unit in lower case beside the upper-case TARGET label", () => {
    speedAxis(() => {});
    expect(screen.getByText("km/h", { selector: "span.normal-case" })).toBeInTheDocument();
  });
});

describe("STRIDE_FIT mark on the speed slider", () => {
  const axis = (mark: { value: number; label: string; title: string } | null) =>
    render(
      <Axis
        title="Velocity"
        tone="cyan"
        unit="km/h"
        actual={3}
        target={3}
        min={0.5}
        max={6.5}
        step={0.1}
        presets={[]}
        formatPreset={(v) => v.toFixed(1)}
        disabled={false}
        onSet={() => {}}
        onNudge={() => {}}
        pin={{ widget: "VELOCITY", pinned: false }}
        mark={mark}
      />,
    );

  it("sits at the fit speed, lined up with the thumb's centre there", () => {
    axis({ value: 3.5, label: "FIT 3.5", title: "STRIDE_FIT 3.5 km/h" });
    expect(trackPct(3.5, 0.5, 6.5)).toBe(50);
    expect(screen.getByTestId("slider-mark").style.left).toBe(thumbCentre(50));
    expect(screen.getByText("FIT 3.5")).toBeInTheDocument();
  });

  it("puts the thumb's centre 8 px in at each end", () => {
    expect(thumbCentre(0)).toBe("calc(0% + 8px)");
    expect(thumbCentre(100)).toBe("calc(100% + -8px)");
  });

  it("shows no mark while STRIDE_FIT is unset or outside the range", () => {
    const { unmount } = axis(null);
    expect(screen.queryByTestId("slider-mark")).toBeNull();
    unmount();
    axis({ value: 8, label: "FIT 8.0", title: "STRIDE_FIT 8.0 km/h" });
    expect(screen.queryByTestId("slider-mark")).toBeNull();
    expect(screen.queryByText("FIT 8.0")).toBeNull();
  });
});
