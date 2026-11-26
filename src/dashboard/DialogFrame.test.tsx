import { fireEvent, render, screen } from "@testing-library/react";
import { Bluetooth } from "lucide-react";
import { describe, expect, it, vi } from "vitest";
import { DialogButtons, DialogCancelButton, DialogConfirmButton, DialogFrame } from "./DialogFrame";

function frame(tone: "cyan" | "magenta" = "cyan", onClose = vi.fn()) {
  render(
    <DialogFrame
      boxRef={() => {}}
      titleId="t"
      icon={Bluetooth}
      title="HARDWARE_LINK"
      subline="connect treadmill"
      tone={tone}
      onClose={onClose}
      footer={
        <DialogButtons>
          <DialogCancelButton>Cancel</DialogCancelButton>
          <DialogConfirmButton danger={tone === "magenta"}>Go</DialogConfirmButton>
        </DialogButtons>
      }
    >
      body
    </DialogFrame>,
  );
  return onClose;
}

describe("DialogFrame", () => {
  it("keeps a version and a unit in the case written", () => {
    render(
      <DialogFrame boxRef={() => {}} titleId="t2" icon={Bluetooth} title="Delete the 2.8 km/h point, v1.2.0" onClose={() => {}}>
        body
      </DialogFrame>,
    );
    expect(screen.getByText("km/h", { selector: "h2 span" })).toHaveClass("normal-case");
    expect(screen.getByText("v1.2.0", { selector: "h2 span" })).toHaveClass("normal-case");
  });

  it("is named by its title and shows the plain sub-line", () => {
    frame();
    expect(screen.getByRole("dialog", { name: "HARDWARE_LINK" })).toBeInTheDocument();
    expect(screen.getByText("connect treadmill")).toBeInTheDocument();
  });

  it("closes from the X", () => {
    const onClose = frame();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("colours the confirm button by role", () => {
    frame("magenta");
    expect(screen.getByRole("button", { name: "Go" }).className).toContain("text-neon-magenta");
  });
});
