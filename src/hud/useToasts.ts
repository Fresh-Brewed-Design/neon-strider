import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

/** Payload of the `hud-update` event Rust emits. Hand-written: Rust builds it as plain JSON. */
export interface ToastPayload {
  type: "VELOCITY" | "ELEVATION" | "STATUS" | "AUTOMATION" | "STRAVA" | "ERGO" | "CALL";
  message?: string;
  value?: number;
  isActionable?: boolean;
}

export interface Toast extends ToastPayload {
  /** Changes whenever the content changes, so the entry animation replays. */
  key: string;
}

const LIFETIME_MS = 5000;
const ACTIONABLE_LIFETIME_MS = 8000;

/**
 * Transient HUD notifications. One slot per type: a newer toast of the same
 * type replaces the old one and restarts its timer, so a speed ramp shows one
 * updating readout instead of a pile of cards.
 */
export function useToasts(): Toast[] {
  const [toasts, setToasts] = useState<Toast[]>([]);

  useEffect(() => {
    const timers = new Map<string, ReturnType<typeof setTimeout>>();
    let seq = 0;
    let disposed = false;
    let unlisten: (() => void) | undefined;

    listen<ToastPayload>("hud-update", ({ payload }) => {
      setToasts((prev) => {
        const existing = prev.find((t) => t.type === payload.type);
        // Speed/tilt readouts update in place: a new key would remount the card
        // and replay its entry animation on every 0.1 km/h of a ramp, which
        // reads as a stream of separate toasts.
        const isReadout = payload.type === "VELOCITY" || payload.type === "ELEVATION";
        const keep = existing && (isReadout || existing.message === payload.message);
        const key = keep ? existing.key : `${payload.type}-${++seq}`;
        const next: Toast = { ...payload, key };
        return existing
          ? prev.map((t) => (t.type === payload.type ? next : t))
          : [...prev, next];
      });

      clearTimeout(timers.get(payload.type));
      timers.set(
        payload.type,
        setTimeout(
          () => {
            timers.delete(payload.type);
            setToasts((prev) => prev.filter((t) => t.type !== payload.type));
          },
          payload.isActionable ? ACTIONABLE_LIFETIME_MS : LIFETIME_MS,
        ),
      );
    }).then((fn) => {
      // The effect may have been cleaned up while listen() was pending.
      if (disposed) fn();
      else unlisten = fn;
    });

    return () => {
      disposed = true;
      unlisten?.();
      timers.forEach(clearTimeout);
    };
  }, []);

  return toasts;
}
