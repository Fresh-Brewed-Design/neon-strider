import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { HudHeader } from "./HudHeader";

const header = (simulated?: boolean) =>
  render(
    <HudHeader
      link={{ text: "LINKED", tone: "live" }}
      relink={null}
      onRelink={() => {}}
      logo={{} as Parameters<typeof HudHeader>[0]["logo"]}
      dragging={false}
      simulated={simulated}
    />,
  );

describe("HudHeader", () => {
  it("tags the dev builds' simulated treadmill", () => {
    header(true);
    expect(screen.getByText("SIM")).toBeInTheDocument();
  });

  it("shows no tag for a real treadmill", () => {
    header(false);
    expect(screen.queryByText("SIM")).not.toBeInTheDocument();
  });
});
