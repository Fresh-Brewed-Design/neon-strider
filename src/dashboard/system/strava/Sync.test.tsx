import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { AutoSyncOptions } from "./Sync";

const { snapshot: mock } = await import("../../../dev/mockBackend");

describe("Strava rollup time", () => {
  it("can't be changed while Daily rollup is off, and keeps the saved time", () => {
    render(<AutoSyncOptions strava={{ ...mock.strava, daily_rollup: false, shift_end: "17:30" }} />);
    const time = screen.getByLabelText("Rollup time");
    expect(time).toBeDisabled();
    expect(time).toHaveValue("17:30");
  });

  it("can be set while Daily rollup is on", () => {
    render(<AutoSyncOptions strava={{ ...mock.strava, daily_rollup: true, shift_end: "17:30" }} />);
    expect(screen.getByLabelText("Rollup time")).toBeEnabled();
  });
});
