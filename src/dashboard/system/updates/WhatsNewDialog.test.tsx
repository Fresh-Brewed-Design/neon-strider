import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../../../ipc";

let current: AppSnapshot | null = null;
vi.mock("../../../state/useAppState", () => ({
  useAppSnapshot: () => current,
  useAppState: <T,>(select: (s: AppSnapshot) => T) => (current ? select(current) : null),
}));

const { snapshot: mock } = await import("../../../dev/mockBackend");
const { WhatsNewDialog } = await import("./WhatsNewDialog");

const acks = () =>
  vi
    .mocked(invoke)
    .mock.calls.filter(([cmd, a]) => cmd === "dispatch" && (a as { command: { type: string } }).command.type === "update_whats_new_ack");

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
  current.update.whats_new = { version: "1.77.0", notes: "- Dialogs close on Escape." };
});

describe("What's new", () => {
  it("is a labelled dialog with focus on OK", () => {
    render(<WhatsNewDialog />);
    expect(screen.getByRole("dialog", { name: "What's new in v1.77.0" })).toBeInTheDocument();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "OK" }));
  });

  it("Escape is OK", async () => {
    render(<WhatsNewDialog />);
    await act(async () => fireEvent.keyDown(window, { key: "Escape" }));
    expect(acks()).toHaveLength(1);
  });
});
