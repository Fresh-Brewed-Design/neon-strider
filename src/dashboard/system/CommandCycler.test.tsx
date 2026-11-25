import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CommandCycler } from "./CommandCycler";

describe("CommandCycler step delay", () => {
  it("offers a 200 ms minimum and clamps on blur", () => {
    render(<CommandCycler onSend={vi.fn().mockResolvedValue(true)} />);
    const input = screen.getByLabelText("Step every (ms)") as HTMLInputElement;
    expect(input.min).toBe("200");
    fireEvent.change(input, { target: { value: "1" } });
    fireEvent.blur(input);
    expect(input.value).toBe("200");
  });
});
