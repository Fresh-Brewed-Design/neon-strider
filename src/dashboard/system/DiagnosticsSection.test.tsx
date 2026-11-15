import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useTreadmillStore } from "../../store/treadmillStore";
import { ConfirmDialog } from "../ConfirmDialog";
import { DiagnosticsSection } from "./DiagnosticsSection";

const rawSends = () => vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "send_bluetooth_command");

function renderCycler() {
  localStorage.setItem("system.devTools", "on");
  render(
    <>
      <DiagnosticsSection />
      <ConfirmDialog />
    </>,
  );
  fireEvent.click(screen.getByText("Show diagnostics"));
}

const sendOnce = () => act(() => fireEvent.click(screen.getByText("Send once")));

describe("Diagnostics raw send", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockClear();
    useTreadmillStore.getState().clearConfirmation();
  });

  it("asks with the themed dialog first, then sends once and stays armed", async () => {
    renderCycler();
    sendOnce();
    expect(screen.getByText("Send raw commands")).toBeInTheDocument();
    expect(rawSends()).toHaveLength(0);

    // Further sends while the dialog is open are dropped, not queued.
    sendOnce();
    await act(async () => fireEvent.click(screen.getByText("Confirm")));
    expect(screen.queryByText("Send raw commands")).not.toBeInTheDocument();
    expect(rawSends()).toHaveLength(1);

    await act(async () => fireEvent.click(screen.getByText("Send once")));
    expect(screen.queryByText("Send raw commands")).not.toBeInTheDocument();
    expect(rawSends()).toHaveLength(2);
  });

  it("sends nothing and stays unarmed when cancelled", () => {
    renderCycler();
    sendOnce();
    act(() => fireEvent.click(screen.getByText("Cancel")));
    expect(rawSends()).toHaveLength(0);
    sendOnce();
    expect(screen.getByText("Send raw commands")).toBeInTheDocument();
    expect(rawSends()).toHaveLength(0);
  });
});
