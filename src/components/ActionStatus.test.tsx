import { beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "@testing-library/react";
import { ActionStatus } from "./ActionStatus";

describe("ActionStatus scrolling", () => {
  const scrolled = vi.fn();
  beforeEach(() => {
    scrolled.mockClear();
    Element.prototype.scrollIntoView = scrolled;
  });

  it("scrolls to a result that appears after the control was used", () => {
    const { rerender } = render(<ActionStatus />);
    rerender(<ActionStatus error="Could not save" />);
    expect(scrolled).toHaveBeenCalledTimes(1);
  });

  it("leaves the page alone for a line that was already there", () => {
    render(<ActionStatus error="Auto-sync is paused" />);
    expect(scrolled).not.toHaveBeenCalled();
  });

  it("never scrolls for background status, however often its text changes", () => {
    const { rerender } = render(<ActionStatus ambient />);
    rerender(<ActionStatus ambient error="Retrying in 12 s" />);
    rerender(<ActionStatus ambient error="Retrying in 11 s" />);
    expect(scrolled).not.toHaveBeenCalled();
  });
});
