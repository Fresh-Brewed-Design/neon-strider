import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { NumberField } from "./fields";

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
