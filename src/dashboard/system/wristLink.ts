import type { WristLinkSnapshot } from "../../ipc/bindings/WristLinkSnapshot";
import type { StatusTone } from "../../state/linkStatus";

/** The tone for WRIST_LINK: cyan with a field connected, yellow while it waits, red when it failed. */
export function wristLinkTone(w: WristLinkSnapshot): StatusTone {
  if (!w.enabled) return "idle";
  if (w.failed) return "problem";
  if (w.watches > 0) return "ok";
  return "working";
}
