import { useEffect, type RefObject } from "react";
import { invoke } from "@tauri-apps/api/core";

const clear = () =>
  invoke("set_hud_hit_region", { left: 0, top: 0, width: 0, height: 0 }).catch(() => {});

/**
 * The HUD window is a fixed, click-through strip. Rust makes it interactive only
 * while the cursor is inside the rectangle we report here, so this tells Rust
 * where the visible content is, in physical pixels relative to this window's
 * client area. Rust adds the window's position on the desktop, so it stays right
 * on any monitor and when the window is moved.
 *
 * Measured on layout changes (ResizeObserver) rather than every frame; a slow
 * interval covers changes that don't resize anything (DPI change).
 */
export function useHitRegion(ref: RefObject<HTMLElement | null>, active: boolean) {
  useEffect(() => {
    const el = ref.current;
    if (!active || !el) {
      clear();
      return;
    }

    let last = "";
    const report = () => {
      const r = el.getBoundingClientRect();
      if (r.width <= 0 || r.height <= 0) {
        if (last !== "") {
          last = "";
          clear();
        }
        return;
      }
      const dpr = window.devicePixelRatio || 1;
      const rect = {
        // Relative to this window; Rust adds the window's desktop position.
        // (window.screenX is not in one unit across monitors with different scales.)
        left: Math.round(r.left * dpr),
        top: Math.round(r.top * dpr),
        width: Math.round(r.width * dpr),
        height: Math.round(r.height * dpr),
      };
      const key = `${rect.left},${rect.top},${rect.width},${rect.height}`;
      if (key === last) return;
      last = key;
      invoke("set_hud_hit_region", rect).catch(() => {});
    };

    const observer = new ResizeObserver(report);
    observer.observe(el);
    window.addEventListener("resize", report);
    const interval = setInterval(report, 1000); // backstop for moves the observers miss (a monitor change)
    report();

    return () => {
      observer.disconnect();
      window.removeEventListener("resize", report);
      clearInterval(interval);
      clear();
    };
  }, [ref, active]);
}
