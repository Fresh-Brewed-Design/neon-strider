import { render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../../ipc";

let current: AppSnapshot | null = null;
vi.mock("../../state/useAppState", () => ({
  useAppState: <T,>(select: (s: AppSnapshot) => T) => (current ? select(current) : null),
}));

const { snapshot: mock } = await import("../../dev/mockBackend");
const { SystemTabs, SectionLinks } = await import("./SystemPage");

function withDot(dot: boolean): AppSnapshot {
  const s = structuredClone(mock);
  s.update = { ...s.update, dot };
  return s;
}

const tab = (name: string) => screen.getByRole("radio", { name: new RegExp(name) });

describe("System's group tabs", () => {
  afterEach(() => {
    current = null;
  });

  it("offers DECK, LINKS, ROUTINE and CORE, with the open one checked", () => {
    current = withDot(false);
    render(<SystemTabs tab="links" onChange={() => {}} />);
    const group = screen.getByRole("radiogroup", { name: "System groups" });
    expect(within(group).getAllByRole("radio").map((b) => b.textContent)).toEqual(["DECK", "LINKS", "ROUTINE", "CORE"]);
    expect(tab("LINKS").getAttribute("aria-checked")).toBe("true");
    expect(tab("DECK").getAttribute("aria-checked")).toBe("false");
  });

  it("reports the tab pressed", () => {
    current = withDot(false);
    const onChange = vi.fn();
    render(<SystemTabs tab="deck" onChange={onChange} />);
    tab("ROUTINE").click();
    expect(onChange).toHaveBeenCalledWith("routine");
  });

  it("shows the update dot on CORE while Rust says an update is waiting", () => {
    current = withDot(true);
    render(<SystemTabs tab="deck" onChange={() => {}} />);
    expect(within(tab("CORE")).getByRole("img", { name: "Update available" })).toBeTruthy();
    // Only the CORE tab carries it.
    expect(screen.getAllByRole("img", { name: "Update available" })).toHaveLength(1);
  });

  it("has no dot once it clears", () => {
    current = withDot(false);
    render(<SystemTabs tab="deck" onChange={() => {}} />);
    expect(within(tab("CORE")).queryByRole("img")).toBeNull();
  });
});

describe("System's section links", () => {
  it("lists the sections it is given and scrolls to one, focusing its heading", () => {
    document.body.insertAdjacentHTML(
      "beforeend",
      '<section id="sys-drift"><h2 id="sys-drift-title" tabindex="-1">Speed drift</h2></section>',
    );
    const section = document.getElementById("sys-drift")!;
    const scrollIntoView = vi.fn();
    section.scrollIntoView = scrollIntoView;
    render(<SectionLinks sections={["sys-treadmill", "sys-drift"]} />);
    const nav = screen.getByRole("navigation", { name: "Sections of the open tab" });
    expect(within(nav).getAllByRole("button").map((b) => b.textContent)).toEqual(["Treadmill", "Speed drift"]);
    within(nav).getByRole("button", { name: "Speed drift" }).click();
    expect(scrollIntoView).toHaveBeenCalled();
    expect(document.activeElement?.id).toBe("sys-drift-title");
    section.remove();
  });

  it("shows no links for a tab with one section", () => {
    render(<SectionLinks sections={["sys-sessions"]} />);
    expect(screen.queryByRole("navigation")).toBeNull();
  });
});
