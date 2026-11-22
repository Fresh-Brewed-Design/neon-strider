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
