import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { HudHeader } from "./HudHeader";

const header = (simulated?: boolean, link: Parameters<typeof HudHeader>[0]["link"] = { text: "LINKED", tone: "live" }) =>
  render(
    <HudHeader
      link={link}
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

  it("shows the STASIS clock beside the status while paused", () => {
    header(false, { text: "STASIS", tone: "idle", clock: "1:05:09" });
    expect(screen.getByText("STASIS", { exact: false }).textContent).toBe("STASIS\u00a01:05:09");
  });
});
