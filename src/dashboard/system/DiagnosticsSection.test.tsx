import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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
  afterEach(() => vi.useRealTimers());

  it("asks with the themed dialog first, then sends once and stays armed", async () => {
    renderCycler();
    sendOnce();
    expect(screen.getByText("Send raw commands")).toBeInTheDocument();
    expect(rawSends()).toHaveLength(0);

    // Further sends while the dialog is open are dropped, not queued.
    sendOnce();
    await act(async () => fireEvent.click(screen.getByText("Send")));
    expect(screen.queryByText("Send raw commands")).not.toBeInTheDocument();
    expect(rawSends()).toHaveLength(1);
    expect(rawSends()[0][1]).toEqual({ body: [0x53, 0x00, ...Array<number>(10).fill(0)] });

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

  it("auto-step waits for the confirm, then steps one full delay later", async () => {
    vi.useFakeTimers();
    renderCycler();
    act(() => fireEvent.click(screen.getByText("Auto-step")));
    act(() => vi.advanceTimersByTime(5000));
    expect(rawSends()).toHaveLength(0);

    await act(async () => fireEvent.click(screen.getByText("Send")));
    expect(rawSends()).toHaveLength(1);
    await act(async () => vi.advanceTimersByTime(1999));
    expect(rawSends()).toHaveLength(1);
    await act(async () => vi.advanceTimersByTime(1));
    expect(rawSends()).toHaveLength(2);
    expect(rawSends()[1][1]).toEqual({ body: [0x53, 0x01, ...Array<number>(10).fill(0)] });
    act(() => fireEvent.click(screen.getByText("Stop stepping")));
  });

  it("auto-step stops when the confirm is cancelled", async () => {
    renderCycler();
    act(() => fireEvent.click(screen.getByText("Auto-step")));
    await act(async () => fireEvent.click(screen.getByText("Cancel")));
    expect(screen.getByText("Auto-step")).toBeInTheDocument();
    expect(rawSends()).toHaveLength(0);
  });
});
