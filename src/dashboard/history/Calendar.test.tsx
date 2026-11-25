import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { dayKey } from "../../format";
import { Calendar } from "./Calendar";

describe("Calendar keyboard", () => {
  it("is one tab stop and arrow keys move by day and week", () => {
    const today = new Date();
    const day = (n: number) => dayKey(new Date(today.getFullYear(), today.getMonth(), today.getDate() - n).getTime());
    const onSelect = vi.fn();
    render(<Calendar metersByDay={new Map([[day(0), 3000], [day(1), 3000]])} selected={null} onSelect={onSelect} />);
    const cells = screen.getAllByRole("button");
    expect(cells.filter((c) => c.tabIndex === 0)).toHaveLength(1);
    const stop = cells.find((c) => c.tabIndex === 0)!;
    stop.focus();
    fireEvent.keyDown(stop, { key: "ArrowUp" });
    expect(document.activeElement).toBe(cells[cells.indexOf(stop) - 1]);
    fireEvent.keyDown(document.activeElement!, { key: "ArrowLeft" });
    expect(document.activeElement).toBe(cells[cells.indexOf(stop) - 8]);
    expect(cells.filter((c) => c.tabIndex === 0)).toHaveLength(1);
    fireEvent.click(stop);
    expect(onSelect).toHaveBeenCalledWith(day(0));
  });
});
