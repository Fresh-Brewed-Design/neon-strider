import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useBackendAction } from "../../../components/useAction";
import { useTreadmillStore } from "../../../store/treadmillStore";
import { Account } from "./Account";

const { snapshot: mock } = await import("../../../dev/mockBackend");

const dispatched = () =>
  vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "dispatch")
    .map(([, a]) => (a as { command: unknown }).command);

function Linked() {
  const connect = useBackendAction(null, false);
  return <Account strava={{ ...mock.strava, connected: true, athlete_name: "Runner" }} connect={connect} activityText={null} />;
}

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  useTreadmillStore.getState().clearConfirmation();
});

describe("Strava account", () => {
  it("asks before Disconnect removes the link", async () => {
    render(<Linked />);
    fireEvent.click(screen.getByRole("button", { name: "Disconnect" }));
    expect(dispatched()).toEqual([]);
    const ask = useTreadmillStore.getState().pendingConfirmation;
    expect(ask).toMatchObject({
      title: "Disconnect Strava",
      message: "Walks stop uploading to Strava until you connect again.",
      confirmLabel: "Disconnect",
      tone: "danger",
    });
    await act(async () => ask?.onConfirm());
    expect(dispatched()).toEqual([{ type: "strava_disconnect" }]);
  });
});
