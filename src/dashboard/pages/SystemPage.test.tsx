import { render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../../ipc";

let current: AppSnapshot | null = null;
vi.mock("../../state/useAppState", () => ({
  useAppState: <T,>(select: (s: AppSnapshot) => T) => (current ? select(current) : null),
}));

const { snapshot: mock } = await import("../../dev/mockBackend");
const { SectionIndex } = await import("./SystemPage");

function withDot(dot: boolean): AppSnapshot {
  const s = structuredClone(mock);
  s.update = { ...s.update, dot };
  return s;
}

const updatesTab = () => screen.getByRole("button", { name: /Updates/ });

describe("System's Updates tab", () => {
  afterEach(() => {
    current = null;
  });

  it("shows the update dot while Rust says an update is waiting", () => {
    current = withDot(true);
    render(<SectionIndex />);
    expect(within(updatesTab()).getByRole("img", { name: "Update available" })).toBeTruthy();
    // Only the Updates tab carries it.
    expect(screen.getAllByRole("img", { name: "Update available" })).toHaveLength(1);
  });

  it("has no dot once it clears", () => {
    current = withDot(false);
    render(<SectionIndex />);
    expect(within(updatesTab()).queryByRole("img")).toBeNull();
  });
});
