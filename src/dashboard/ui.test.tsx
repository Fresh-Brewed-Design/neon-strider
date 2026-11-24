import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Codename, OptionPicker } from "./ui";

const OPTIONS = [
  { value: "a", label: "Alpha" },
  { value: "b", label: "Bravo" },
  { value: "c", label: "Charlie" },
] as const;

type Value = (typeof OPTIONS)[number]["value"];

function Picker({
  start,
  disabled = false,
  activation,
  onChange = () => {},
}: {
  start: Value | null;
  disabled?: boolean;
  activation?: "auto" | "manual";
  onChange?: (next: Value) => void;
}) {
  const [value, setValue] = useState<Value | null>(start);
  return (
    <OptionPicker
      label="Letters"
      options={OPTIONS}
      value={value}
      disabled={disabled}
      activation={activation}
      onChange={(next) => {
        onChange(next);
        setValue(next);
      }}
      optionClassName={(active) => (active ? "on" : "off")}
    />
  );
}

const radio = (name: string) => screen.getByRole("radio", { name });

describe("OptionPicker", () => {
  it("is a named radio group with the chosen option checked", () => {
    render(<Picker start="b" />);
    expect(screen.getByRole("radiogroup", { name: "Letters" })).toBeInTheDocument();
    expect(radio("Bravo")).toHaveAttribute("aria-checked", "true");
    expect(radio("Alpha")).toHaveAttribute("aria-checked", "false");
    expect(radio("Bravo")).toHaveClass("on");
  });

  it("puts the only tab stop on the chosen option", () => {
    render(<Picker start="b" />);
    expect(radio("Alpha")).toHaveAttribute("tabindex", "-1");
    expect(radio("Bravo")).toHaveAttribute("tabindex", "0");
    expect(radio("Charlie")).toHaveAttribute("tabindex", "-1");
  });

  it("puts the tab stop on the first option when none is chosen", () => {
    render(<Picker start={null} />);
    expect(radio("Alpha")).toHaveAttribute("tabindex", "0");
    expect(radio("Bravo")).toHaveAttribute("tabindex", "-1");
  });

  it("moves and chooses with the arrow keys, wrapping at the ends", () => {
    const onChange = vi.fn();
    render(<Picker start="a" onChange={onChange} />);
    radio("Alpha").focus();

    fireEvent.keyDown(radio("Alpha"), { key: "ArrowDown" });
    expect(radio("Bravo")).toHaveFocus();
    expect(radio("Bravo")).toHaveAttribute("aria-checked", "true");
    expect(radio("Bravo")).toHaveAttribute("tabindex", "0");

    fireEvent.keyDown(radio("Bravo"), { key: "ArrowRight" });
    expect(radio("Charlie")).toHaveFocus();

    fireEvent.keyDown(radio("Charlie"), { key: "ArrowDown" });
    expect(radio("Alpha")).toHaveFocus();
    expect(radio("Alpha")).toHaveAttribute("aria-checked", "true");

    fireEvent.keyDown(radio("Alpha"), { key: "ArrowUp" });
    expect(radio("Charlie")).toHaveFocus();

    fireEvent.keyDown(radio("Charlie"), { key: "ArrowLeft" });
    expect(radio("Bravo")).toHaveFocus();

    expect(onChange.mock.calls.map((c: unknown[]) => c[0])).toEqual(["b", "c", "a", "c", "b"]);
  });

  it("jumps to the ends with Home and End", () => {
    render(<Picker start="b" />);
    fireEvent.keyDown(radio("Bravo"), { key: "End" });
    expect(radio("Charlie")).toHaveFocus();
    expect(radio("Charlie")).toHaveAttribute("aria-checked", "true");
    fireEvent.keyDown(radio("Charlie"), { key: "Home" });
    expect(radio("Alpha")).toHaveFocus();
    expect(radio("Alpha")).toHaveAttribute("aria-checked", "true");
  });

  it("leaves other keys alone", () => {
    const onChange = vi.fn();
    render(<Picker start="a" onChange={onChange} />);
    fireEvent.keyDown(radio("Alpha"), { key: "Tab" });
    fireEvent.keyDown(radio("Alpha"), { key: "x" });
    expect(onChange).not.toHaveBeenCalled();
  });

  it("reports a click, even on the chosen option", () => {
    const onChange = vi.fn();
    render(<Picker start="a" onChange={onChange} />);
    fireEvent.click(radio("Charlie"));
    fireEvent.click(radio("Charlie"));
    expect(onChange.mock.calls.map((c: unknown[]) => c[0])).toEqual(["c", "c"]);
  });

  it("chooses on Space or Enter, not on the arrows, with manual activation", () => {
    const onChange = vi.fn();
    render(<Picker start="a" activation="manual" onChange={onChange} />);
    radio("Alpha").focus();
    fireEvent.keyDown(radio("Alpha"), { key: "ArrowDown" });
    fireEvent.keyDown(radio("Bravo"), { key: "End" });
    expect(radio("Charlie")).toHaveFocus();
    expect(radio("Alpha")).toHaveAttribute("aria-checked", "true");
    expect(onChange).not.toHaveBeenCalled();

    fireEvent.keyDown(radio("Charlie"), { key: " " });
    expect(radio("Charlie")).toHaveAttribute("aria-checked", "true");
    fireEvent.keyDown(radio("Charlie"), { key: "Home" });
    fireEvent.keyDown(radio("Alpha"), { key: "Enter" });
    expect(onChange.mock.calls.map((c: unknown[]) => c[0])).toEqual(["c", "a"]);
  });

  it("leaves Space and Enter to the button with automatic activation", () => {
    const onChange = vi.fn();
    render(<Picker start="a" onChange={onChange} />);
    fireEvent.keyDown(radio("Bravo"), { key: "Enter" });
    fireEvent.keyDown(radio("Bravo"), { key: " " });
    expect(onChange).not.toHaveBeenCalled();
  });

  it("keeps focus but ignores presses while disabled", () => {
    const onChange = vi.fn();
    render(<Picker start="a" disabled onChange={onChange} />);
    radio("Alpha").focus();
    fireEvent.keyDown(radio("Alpha"), { key: "ArrowDown" });
    fireEvent.click(radio("Charlie"));
    expect(onChange).not.toHaveBeenCalled();
    expect(radio("Alpha")).toHaveAttribute("aria-checked", "true");
    expect(radio("Alpha")).toHaveAttribute("aria-disabled", "true");
    expect(radio("Alpha")).not.toBeDisabled();
  });
});

describe("Codename", () => {
  it("offers a break after each underscore and keeps the text", () => {
    const { container } = render(<Codename text="LIFETIME_ODOMETER" />);
    expect(container.textContent).toBe("LIFETIME_ODOMETER");
    expect(container.querySelectorAll("wbr")).toHaveLength(1);
    expect(container.querySelector("wbr")?.previousSibling?.textContent).toBe("_");
  });
});
