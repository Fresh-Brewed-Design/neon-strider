import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useTreadmillStore } from "../store/treadmillStore";
import type { ConfirmState } from "../store/types";
import { ConfirmDialog } from "./ConfirmDialog";

function openFrom(config: Partial<ConfirmState> = {}) {
  const onConfirm = vi.fn();
  const onCancel = vi.fn();
  render(
    <>
      <button type="button">Opener</button>
      <ConfirmDialog />
    </>,
  );
  const opener = screen.getByText("Opener");
  opener.focus();
  act(() =>
    useTreadmillStore.getState().confirm({
      title: "Delete program",
      message: "Workday and its steps are deleted for good.",
      onConfirm,
      onCancel,
      ...config,
    }),
  );
  return { opener, onConfirm, onCancel };
}

beforeEach(() => useTreadmillStore.getState().clearConfirmation());

describe("ConfirmDialog", () => {
  it("is a labelled alert dialog with focus on Cancel", () => {
    openFrom();
    const dialog = screen.getByRole("alertdialog", { name: "Delete program" });
    expect(dialog).toHaveAccessibleDescription("Workday and its steps are deleted for good.");
    // The X and the Cancel button both have a name; Cancel (the text one) has focus.
    expect(screen.getAllByRole("button", { name: "Cancel" })).toHaveLength(2);
    expect(document.activeElement?.textContent).toBe("Cancel");
  });

  it("cancels on Escape and gives focus back to the opener", () => {
    const { opener, onCancel, onConfirm } = openFrom();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledOnce();
    expect(onConfirm).not.toHaveBeenCalled();
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(document.activeElement).toBe(opener);
  });

  it("keeps Tab inside the dialog", () => {
    openFrom();
    const buttons = screen.getByRole("alertdialog").querySelectorAll("button");
    const first = buttons[0];
    const last = buttons[buttons.length - 1];
    last.focus();
    fireEvent.keyDown(window, { key: "Tab" });
    expect(document.activeElement).toBe(first);
    fireEvent.keyDown(window, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(last);
  });

  it("reads Confirm in cyan by default", () => {
    openFrom();
    const button = screen.getByRole("button", { name: "Confirm" });
    expect(button.className).toContain("text-neon-cyan");
  });

  it("puts a danger confirm's own verb on a magenta button", () => {
    const { onConfirm } = openFrom({ confirmLabel: "Delete", tone: "danger" });
    expect(screen.queryByRole("button", { name: "Confirm" })).toBeNull();
    const button = screen.getByRole("button", { name: "Delete" });
    expect(button.className).toContain("text-neon-magenta");
    fireEvent.click(button);
    expect(onConfirm).toHaveBeenCalledOnce();
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });
});
