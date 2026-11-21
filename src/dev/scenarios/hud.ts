// The HUD's place and layout. ?anchor=top_left (or any corner) picks its corner,
// ?onemonitor leaves a single display, ?custom puts it where it was dragged,
// ?minified shows the running strip (combine with ?program=resting|paused,
// ?noprogram or ?idle), and ?minifykey=off|pause|scroll_lock sets the minify hotkey.
import { emit } from "@tauri-apps/api/event";
import type { AppSnapshot } from "../../ipc";
import { expandHud, params, publish, snapshot } from "../mockState";
import type { Scenario } from "./types";

/**
 * A logo drag in the preview. Rust follows the real cursor across the desktop;
 * here the page can't move its window, so only the vertical part (the content's
 * top inside the strip) is simulated, with the same 24 px snapping to the top
 * and bottom corners.
 */
function simulateHudDrag(grabY: number) {
  const anchor = snapshot.hud.anchor;
  const right = anchor === "top_right" || anchor === "bottom_right";
  const content = document.querySelector(".hud-content");
  const height = content?.getBoundingClientRect().height ?? 200;
  const place = (clientY: number) => {
    const max = window.innerHeight - 8 - height;
    const top = Math.min(Math.max(clientY - grabY, 8), Math.max(max, 8));
    if (top - 8 <= 24) return { custom: null, anchor: right ? "top_right" : "top_left" } as const;
    if (max - top <= 24) return { custom: null, anchor: right ? "bottom_right" : "bottom_left" } as const;
    return { custom: { dx: 400, top }, anchor } as const;
  };
  let last: ReturnType<typeof place> | null = null;
  const move = (e: PointerEvent) => {
    last = place(e.clientY);
    // Mid-drag the content follows the pointer even over a snap zone, as the
    // real drag does (it snaps the frame, not just the drop).
    void emit("hud-drag", { custom: last.custom ?? { dx: 400, top: e.clientY - grabY }, anchor, done: false });
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    if (!last) return;
    snapshot.hud.custom = last.custom ? { ...last.custom } : null;
    snapshot.hud.anchor = last.anchor;
    void emit("hud-drag", { custom: snapshot.hud.custom, anchor: snapshot.hud.anchor, done: true });
    publish();
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

export const hud: Scenario = {
  apply() {
    const anchor = params.get("anchor");
    if (anchor) snapshot.hud.anchor = anchor as AppSnapshot["hud"]["anchor"];
    if (params.has("onemonitor")) snapshot.hud_monitors = snapshot.hud_monitors.slice(0, 1);
    if (params.has("custom")) snapshot.hud.custom = { dx: 400, top: 120 };
    if (params.has("minified")) snapshot.hud.minified = true;
    const minifyKey = params.get("minifykey");
    if (minifyKey === "off") snapshot.hud.minify_hotkey = { enabled: false, key: "insert" };
    else if (minifyKey === "pause" || minifyKey === "scroll_lock") snapshot.hud.minify_hotkey = { enabled: true, key: minifyKey };
  },
  commands: {
    hud_toggle_widget: ({ widget }) => {
      const w = snapshot.hud.widgets;
      snapshot.hud.widgets = w.includes(widget) ? w.filter((x) => x !== widget) : [...w, widget];
    },
    hud_set_widgets: (c) => (snapshot.hud.widgets = c.widgets),
    hud_set_anchor: (c) => Object.assign(snapshot.hud, { anchor: c.anchor, custom: null }),
    hud_reset_position: () => (snapshot.hud.custom = null),
    hud_drag_start: (c) => simulateHudDrag(c.grab_y),
    hud_set_monitor: (c) => {
      snapshot.hud.monitor = c.monitor;
      for (const m of snapshot.hud_monitors) m.active = c.monitor === null ? m.is_primary : m.name === c.monitor;
    },
    dashboard_set_pinned: (c) => (snapshot.hud.dashboard_pinned = c.pinned),
    hud_set_idle_mode: (c) => (snapshot.hud.idle_mode = c.mode),
    hud_expand: () => {
      expandHud();
      snapshot.hud.minified = false;
    },
    hud_set_minified: (c) => (snapshot.hud.minified = c.minified),
    hud_set_minify_hotkey: (c) => (snapshot.hud.minify_hotkey = { ...c.hotkey }),
  },
};
