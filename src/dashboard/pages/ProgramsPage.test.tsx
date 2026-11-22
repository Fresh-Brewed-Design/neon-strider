import { fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../../ipc";

let current: AppSnapshot | null = null;
vi.mock("../../state/useAppState", () => ({
  useAppSnapshot: () => current,
  useAppState: <T,>(sel: (s: AppSnapshot) => T) => (current ? sel(current) : null),
}));

const { snapshot: mock } = await import("../../dev/mockBackend");
const { useTreadmillStore } = await import("../../store/treadmillStore");
const { OPEN_LINK_EVENT } = await import("../util");
const { ProgramsPage } = await import("./ProgramsPage");

function show(patch: (s: AppSnapshot) => void) {
  const s = structuredClone(mock);
  s.active_program = null;
  patch(s);
  current = s;
  useTreadmillStore.setState({ pendingConfirmation: null });
  return render(<ProgramsPage />);
}

beforeEach(() => vi.mocked(invoke).mockClear());

describe("Programs empty state", () => {
  it("says there are no programs and points to New and ECHO", () => {
    show((s) => (s.programs = []));
    const empty = screen.getByTestId("programs-empty");
    expect(empty).toHaveTextContent("No programs yet.");
    expect(empty).toHaveTextContent("Use New to build one, or ECHO on History to turn a walk into one.");
  });

  it("has a New button that opens the editor like the header's", () => {
    show((s) => (s.programs = []));
    const news = screen.getAllByRole("button", { name: "New" });
    expect(news).toHaveLength(2);
    fireEvent.click(news[1]);
    expect(screen.queryByTestId("programs-empty")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "New" })).not.toBeInTheDocument();
  });
});

describe("Programs offline", () => {
  it("shows why Run is off with a Connect that opens the link dialog", () => {
    const opened = vi.fn();
    window.addEventListener(OPEN_LINK_EVENT, opened);
    show((s) => Object.assign(s.connection, { connected: false, link: { state: "idle" } }));
    expect(screen.getByTestId("programs-offline")).toHaveTextContent("Treadmill not connected.");
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(opened).toHaveBeenCalledTimes(1);
    window.removeEventListener(OPEN_LINK_EVENT, opened);
    expect(vi.mocked(invoke).mock.calls.filter(([c]) => c === "dispatch")).toEqual([]);
  });

  it("offers no Connect while linking, and nothing when connected", () => {
    const { unmount } = show((s) => Object.assign(s.connection, { connected: false, link: { state: "connecting", attempt: 1, of: 3 } }));
    expect(screen.getByTestId("programs-offline")).toHaveTextContent("Connecting to the treadmill.");
    expect(screen.queryByRole("button", { name: "Connect" })).not.toBeInTheDocument();
    unmount();
    show((s) => Object.assign(s.connection, { connected: true }));
    expect(screen.queryByTestId("programs-offline")).not.toBeInTheDocument();
  });
});
