import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { NotePop } from "./HudNote";

// The strip's note has a fixed box (#435): it shows the lines that fit, the last one ending in an ellipsis.
describe("NotePop's line count", () => {
  afterEach(() => vi.restoreAllMocks());

  it("sets --note-lines to the whole lines the box holds", () => {
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(48);
    vi.spyOn(window, "getComputedStyle").mockImplementation(
      () => ({ paddingTop: "4px", paddingBottom: "4px", lineHeight: "13.2px" }) as CSSStyleDeclaration,
    );
    render(<NotePop note={{ text: "SYS // WAKE THE DECK FIRST AT THE TREADMILL OR REMOTE" }} />);
    // 40px of room over 13.2px lines: three whole lines.
    expect(screen.getByTestId("hud-note").style.getPropertyValue("--note-lines")).toBe("3");
  });

  it("keeps the whole note for screen readers", () => {
    render(<NotePop note={{ text: "SYS // WAKE THE DECK FIRST AT THE TREADMILL OR REMOTE" }} />);
    expect(screen.getByRole("alert")).toHaveTextContent("SYS // WAKE THE DECK FIRST AT THE TREADMILL OR REMOTE");
  });
});
