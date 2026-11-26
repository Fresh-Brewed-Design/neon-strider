import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { StatusPill } from "./StatusPill";

describe("StatusPill state icon", () => {
  it("adds the tone's icon after the glyph when asked, for the icon-only rail", () => {
    const { container } = render(
      <StatusPill bare icon="bluetooth" tone="problem" label="NO_CARRIER" labelClassName="dash-rail-text" toneIconClassName="dash-rail-state" />,
    );
    const icons = container.querySelectorAll("svg");
    expect(icons).toHaveLength(2);
    expect(icons[1]).toHaveClass("dash-rail-state");
    expect(icons[1]).toHaveAttribute("aria-hidden", "true");
  });

  it("keeps one icon otherwise", () => {
    const { container } = render(<StatusPill tone="ok" label="LINKED" />);
    expect(container.querySelectorAll("svg")).toHaveLength(1);
  });
});
