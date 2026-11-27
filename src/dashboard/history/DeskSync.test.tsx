import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DeskSync } from "../../ipc";
import { DeskSyncPanel } from "./DeskSync";
import { deskSyncText } from "./deskSyncText";

function sync(walked: boolean[], today_open = false, today_lit = false): DeskSync {
  return {
    walked: walked.filter(Boolean).length,
    desk_days: walked.length,
    window: 14,
    days: walked.map((w, i) => ({ date: `2026-11-${String(i + 1).padStart(2, "0")}`, walked: w })),
    today_open,
    today_lit,
  };
}

describe("DESK_SYNC", () => {
  it("says how many recent desk days were walked on", () => {
    const s = sync([true, true, false, true, true, true, true, true, true, true]);
    render(<DeskSyncPanel sync={s} />);
    expect(screen.getByText("Walked on 9 of your last 10 desk days.")).toBeTruthy();
    expect(screen.getByText("/ 10")).toBeTruthy();
    expect(screen.getAllByRole("listitem")).toHaveLength(10);
    expect(screen.getAllByLabelText(/no walk$/)).toHaveLength(1);
  });

  it("shows today as open until it is walked, without counting it", () => {
    render(<DeskSyncPanel sync={sync([true, false], true)} />);
    expect(screen.getByText("Walked on 1 of your last 2 desk days.")).toBeTruthy();
    expect(screen.getByLabelText("Today: not walked yet")).toBeTruthy();
  });

  it("has plain text for one day and for none", () => {
    expect(deskSyncText(sync([true]))).toBe("Walked on your last desk day.");
    expect(deskSyncText(sync([false]))).toBe("No walk on your last desk day.");
    expect(deskSyncText(sync([]))).toBe("No desk days recorded yet.");
    expect(deskSyncText(sync([], true))).toBe("Walk today to start the desk-day count.");
  });

  it("never celebrates", () => {
    const { container } = render(<DeskSyncPanel sync={sync(Array<boolean>(14).fill(true))} />);
    expect(container.textContent).not.toMatch(/best|record|streak/i);
  });

  describe("today's light", () => {
    const shown = () =>
      vi.mocked(invoke).mock.calls.filter(([cmd, args]) => cmd === "dispatch" && JSON.stringify(args).includes("desk_sync_light_shown"));
    let reduce = false;
    beforeEach(() => {
      reduce = false;
      vi.mocked(invoke).mockClear();
      vi.stubGlobal("matchMedia", (query: string) => ({ matches: reduce && query.includes("reduce"), media: query }));
    });
    afterEach(() => vi.unstubAllGlobals());

    const lighting = (c: HTMLElement) => c.querySelectorAll(".desk-sync-light");

    it("pulses once when today first counts while the card is open", () => {
      const { container, rerender } = render(<DeskSyncPanel sync={sync([true, false], true)} />);
      expect(lighting(container)).toHaveLength(0);
      rerender(<DeskSyncPanel sync={sync([true, false, true], false, true)} />);
      const lit = lighting(container);
      expect(lit).toHaveLength(1);
      const items = screen.getAllByRole("listitem");
      expect(lit[0]).toBe(items[items.length - 1]);
      expect(shown()).toHaveLength(1);
      // Rust clears the flag once told; the pulse plays out, then stays off.
      rerender(<DeskSyncPanel sync={sync([true, false, true])} />);
      expect(lighting(container)).toHaveLength(1);
      fireEvent.animationEnd(lit[0]);
      expect(lighting(container)).toHaveLength(0);
      rerender(<DeskSyncPanel sync={sync([true, false, true])} />);
      expect(lighting(container)).toHaveLength(0);
      expect(shown()).toHaveLength(1);
    });

    it("pulses when the card opens with the light still unshown", () => {
      const { container } = render(<DeskSyncPanel sync={sync([true, true], false, true)} />);
      expect(lighting(container)).toHaveLength(1);
      expect(shown()).toHaveLength(1);
    });

    it("stays still when today already counted at mount", () => {
      const { container } = render(<DeskSyncPanel sync={sync([true, false, true])} />);
      expect(lighting(container)).toHaveLength(0);
      expect(shown()).toHaveLength(0);
    });

    it("stays still under reduced motion, and still marks the light shown", () => {
      reduce = true;
      const { container, rerender } = render(<DeskSyncPanel sync={sync([true], true)} />);
      rerender(<DeskSyncPanel sync={sync([true, true], false, true)} />);
      expect(lighting(container)).toHaveLength(0);
      expect(shown()).toHaveLength(1);
    });
  });
});
