import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type RefObject } from "react";
import { listen } from "@tauri-apps/api/event";
import { dispatch, type HudAnchor, type HudConfig, type HudCustom } from "../ipc";

/** Where the HUD content sits: a corner anchor, or a dragged custom spot. */
export type Placement = { custom: HudCustom | null; anchor: HudAnchor };

type DragEvent = Placement & { done: boolean };

/** Pointer travel (CSS px) before a press on the logo becomes a drag. */
const DRAG_THRESHOLD_PX = 4;

const same = (a: Placement, b: Placement) =>
  a.anchor === b.anchor && a.custom?.dx === b.custom?.dx && a.custom?.top === b.custom?.top;

/**
 * The placement to draw. Rust moves the strip window itself while the logo is
 * dragged and sends the content's top every frame (`hud-drag`); that wins
 * over the snapshot until the snapshot has caught up with the dropped position
 * (or a few seconds pass, in case the drop changed nothing).
 */
export function usePlacement(hud: HudConfig | undefined): Placement & { dragging: boolean } {
  const [live, setLive] = useState<DragEvent | null>(null);

  useEffect(() => {
    let off: (() => void) | undefined;
    let cancelled = false;
    listen<DragEvent>("hud-drag", (e) => setLive(e.payload))
      .then((un) => {
        if (cancelled) un();
        else off = un;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      off?.();
    };
  }, []);

  const saved: Placement = { custom: hud?.custom ?? null, anchor: hud?.anchor ?? "bottom_right" };
  const caughtUp = live !== null && live.done && same(live, saved);

  useEffect(() => {
    if (!live?.done) return;
    const t = setTimeout(() => setLive(null), caughtUp ? 0 : 3000); // a snapshot heartbeat and a half
    return () => clearTimeout(t);
  }, [live, caughtUp]);

  if (live && !caughtUp) return { custom: live.custom, anchor: live.anchor, dragging: !live.done };
  return { ...saved, dragging: false };
}

/**
 * Pointer handlers for the logo. A press that moves `DRAG_THRESHOLD_PX` starts
 * a drag (Rust follows the cursor from then on, until the button is released);
 * a press that doesn't is a plain click. Double-click puts the HUD back in its
 * corner. `panelRef` is the panel: in custom placement its top-left is the
 * content's, which is what Rust positions.
 */
export function useLogoDrag(panelRef: RefObject<HTMLElement | null>, onError: (error: unknown) => void) {
  const press = useRef<{ x: number; y: number; id: number; sent: boolean } | null>(null);

  const end = (e: ReactPointerEvent<HTMLElement>) => {
    if (press.current && e.currentTarget.hasPointerCapture(press.current.id)) {
      e.currentTarget.releasePointerCapture(press.current.id);
    }
    press.current = null;
  };

  return {
    onPointerDown: (e: ReactPointerEvent<HTMLElement>) => {
      if (e.button !== 0) return;
      press.current = { x: e.clientX, y: e.clientY, id: e.pointerId, sent: false };
      e.currentTarget.setPointerCapture(e.pointerId);
    },
    onPointerMove: (e: ReactPointerEvent<HTMLElement>) => {
      const p = press.current;
      if (!p || p.sent) return;
      if (Math.hypot(e.clientX - p.x, e.clientY - p.y) < DRAG_THRESHOLD_PX) return;
      p.sent = true;
      const panel = panelRef.current?.getBoundingClientRect();
      if (!panel) return;
      dispatch({ type: "hud_drag_start", grab_x: p.x - panel.left, grab_y: p.y - panel.top }).catch(onError);
    },
    onPointerUp: end,
    onPointerCancel: end,
    onDoubleClick: () => {
      dispatch({ type: "hud_reset_position" }).catch(onError);
    },
  };
}
