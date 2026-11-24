import { fireEvent, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useWheelSteps } from "./useWheelSteps";

function Probe({ onStep, disabled = false }: { onStep: (up: boolean) => void; disabled?: boolean }) {
  const ref = useWheelSteps<HTMLDivElement>(onStep, disabled);
  return <div ref={ref} data-testid="target" />;
}

function setup(disabled = false) {
  const onStep = vi.fn();
  const r = render(<Probe onStep={onStep} disabled={disabled} />);
  return { onStep, el: r.getByTestId("target") };
}

describe("useWheelSteps", () => {
  it("sends one step per notch, up for wheel up", () => {
    const { onStep, el } = setup();
    fireEvent.wheel(el, { deltaY: -100 });
    fireEvent.wheel(el, { deltaY: 100 });
    expect(onStep.mock.calls).toEqual([[true], [false]]);
  });

  it("sends at most one step per event", () => {
    const { onStep, el } = setup();
    fireEvent.wheel(el, { deltaY: -500 });
    expect(onStep).toHaveBeenCalledTimes(1);
  });

  it("fires a trackpad swipe of small deltas once per 100 px", () => {
    const { onStep, el } = setup();
    for (let i = 0; i < 9; i++) fireEvent.wheel(el, { deltaY: 10 });
    expect(onStep).not.toHaveBeenCalled();
    fireEvent.wheel(el, { deltaY: 10 });
    expect(onStep).toHaveBeenCalledTimes(1);
    for (let i = 0; i < 10; i++) fireEvent.wheel(el, { deltaY: 10 });
    expect(onStep).toHaveBeenCalledTimes(2);
  });

  it("drops the build-up when the direction flips", () => {
    const { onStep, el } = setup();
    fireEvent.wheel(el, { deltaY: 90 });
    fireEvent.wheel(el, { deltaY: -90 });
    expect(onStep).not.toHaveBeenCalled();
    fireEvent.wheel(el, { deltaY: -10 });
    expect(onStep).toHaveBeenCalledWith(true);
  });

  it("reads line deltas as pixels", () => {
    const { onStep, el } = setup();
    fireEvent.wheel(el, { deltaY: 3, deltaMode: 1 });
    expect(onStep).not.toHaveBeenCalled();
    fireEvent.wheel(el, { deltaY: 4, deltaMode: 1 });
    expect(onStep).toHaveBeenCalledTimes(1);
  });

  it("does nothing while disabled and leaves the event alone", () => {
    const { onStep, el } = setup(true);
    const notCancelled = fireEvent.wheel(el, { deltaY: -100, cancelable: true });
    expect(onStep).not.toHaveBeenCalled();
    expect(notCancelled).toBe(true);
  });

  it("cancels the wheel event so the page doesn't scroll", () => {
    const { el } = setup();
    expect(fireEvent.wheel(el, { deltaY: -100, cancelable: true })).toBe(false);
  });
});
