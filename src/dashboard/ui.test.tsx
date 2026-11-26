import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Codename, Joined, OptionPicker } from "./ui";

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

describe("Joined", () => {
  it("keeps the whole line's text, one part per box", () => {
    const { container } = render(<Joined text="3.0 km/h · 5% · heart zone" />);
    const line = container.querySelector("[data-joined]")!;
    expect(line.textContent).toBe("3.0 km/h · 5% · heart zone");
    const row = line.firstElementChild as HTMLElement;
    expect(row.children).toHaveLength(3);
    expect(row.children[1].textContent).toBe(" · 5%");
  });

  // The row is pulled left by one separator's width and the box clips it, so the separator that
  // opens a wrapped line is never seen; the first part is padded by the same width.
  it("hides the separator that would start a wrapped line", () => {
    const { container } = render(<Joined text="a · b" />);
    const line = container.querySelector("[data-joined]") as HTMLElement;
    expect(line).toHaveClass("overflow-hidden");
    const row = line.firstElementChild as HTMLElement;
    expect(row.style.marginLeft).toBe("-3ch");
    expect((row.children[0] as HTMLElement).style.paddingLeft).toBe("3ch");
  });

  it("splits at a separator with a no-break space beside it, and at //", () => {
    const { container } = render(
      <>
        <Joined text={"T-MINUS 17:08\u00a0· ENDS\u00a09:15"} />
        <Joined sep="//" text="QUIET // HOLD" />
      </>,
    );
    const [a, b] = [...container.querySelectorAll("[data-joined]")].map((l) => l.firstElementChild!);
    expect(a.children).toHaveLength(2);
    expect(b.children).toHaveLength(2);
    expect((b as HTMLElement).style.marginLeft).toBe("-4ch");
  });
});
