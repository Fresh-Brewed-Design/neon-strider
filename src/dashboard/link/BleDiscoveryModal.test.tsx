import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { BleDiscoveryModal } from "./BleDiscoveryModal";

function open(onClose = vi.fn()) {
  render(
    <BleDiscoveryModal
      show
      onClose={onClose}
      devices={[{ id: "AA:BB", name: "FS-TREADMILL" }]}
      isScanning={false}
      scanned
      scanError={null}
      adapterOff={false}
      connectingId={null}
      connectError={null}
      onScan={() => {}}
      onConnect={() => {}}
    />,
  );
  return onClose;
}

describe("BleDiscoveryModal", () => {
  it("is a labelled dialog that takes focus", () => {
    open();
    const dialog = screen.getByRole("dialog", { name: "Hardware_Link" });
    expect(dialog.contains(document.activeElement)).toBe(true);
  });

  it("closes from Cancel", () => {
    const onClose = open();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("closes on Escape", () => {
    const onClose = open();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
  });
});
