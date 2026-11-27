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
    expect(screen.getByRole("dialog", { name: /UPGRADE_COMPLETE/ })).toBeInTheDocument();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "OK" }));
  });

  it("keeps the version's case in the title", () => {
    render(<WhatsNewDialog />);
    const version = screen.getByText("v1.77.0", { selector: "h2 span" });
    expect(version).toHaveClass("normal-case");
    expect(screen.getByRole("heading", { level: 2 })).toHaveTextContent("UPGRADE_COMPLETE // v1.77.0");
  });

  it("Escape is OK", async () => {
    render(<WhatsNewDialog />);
    await act(async () => fireEvent.keyDown(window, { key: "Escape" }));
    expect(acks()).toHaveLength(1);
  });
});
