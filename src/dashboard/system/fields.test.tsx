import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ChoiceGroup, NumberField, Toggle } from "./fields";

function field(onCommit = vi.fn().mockResolvedValue(undefined), value = 80) {
  render(<NumberField label="Body weight" value={value} min={30} max={250} unit="kg" onCommit={onCommit} />);
  return { input: screen.getByRole("spinbutton") as HTMLInputElement, onCommit };
}

describe("NumberField done line", () => {
  it("says Saved after a commit", async () => {
    const { input, onCommit } = field();
    fireEvent.change(input, { target: { value: "90" } });
    fireEvent.blur(input);
    await waitFor(() => expect(screen.getByText("Saved.")).toBeTruthy());
    expect(onCommit).toHaveBeenCalledWith(90);
  });

  it("names the limit when the value was held to it", async () => {
    const { input, onCommit } = field();
    fireEvent.change(input, { target: { value: "400" } });
    fireEvent.blur(input);
    await waitFor(() => expect(screen.getByText("Kept to 250 kg, the most allowed.")).toBeTruthy());
    expect(onCommit).toHaveBeenCalledWith(250);
  });

  it("says nothing when the value did not change", () => {
    const { input, onCommit } = field();
    fireEvent.change(input, { target: { value: "80" } });
    fireEvent.blur(input);
    expect(onCommit).not.toHaveBeenCalled();
    expect(screen.queryByText("Saved.")).toBeNull();
  });
});

// Colour roles (#391): cyan is on and selected, whatever the section's own colour.
describe("switch and choice colours", () => {
  it("turns every switch on cyan", () => {
    render(<Toggle label="Auto-sync" checked onChange={() => {}} />);
    expect(screen.getByRole("switch", { name: "Auto-sync" })).toHaveClass("bg-neon-cyan");
  });

  it("marks the chosen option cyan", () => {
    render(
      <ChoiceGroup
        label="Mute from home feed"
        options={[
          { value: "all", label: "Mute all" },
          { value: "none", label: "Don't mute" },
        ]}
        value="all"
        onChange={() => {}}
      />,
    );
    const chosen = screen.getByRole("radio", { name: "Mute all" });
    expect(chosen.className).toContain("text-neon-cyan");
    expect(chosen.className).not.toContain("magenta");
  });
});
