import type { AppSnapshot } from "../ipc";
import { HudNote } from "./HudNote";
import { SmallButton } from "./SmallButton";
import { useHudSend } from "./useHudSend";
import type { Toast } from "./useToasts";

/** One transient notification: a speed or tilt readout with a bar, or a message. */
export function ToastCard({ toast, snapshot }: { toast: Toast; snapshot: AppSnapshot | null }) {
  const { send, error } = useHudSend();
  if (toast.type === "VELOCITY" || toast.type === "ELEVATION") {
    const isSpeed = toast.type === "VELOCITY";
    const deck = snapshot?.deck;
    // The event carries raw values; the snapshot has the calibrated ones.
    const value = isSpeed ? (toast.value ?? deck?.speed_kmh ?? 0) : (deck?.incline_pct ?? toast.value ?? 0);
    const max = isSpeed ? (deck?.max_speed_kmh ?? 12) : (deck?.max_incline_pct ?? 15);
    const fill = Math.max(0, Math.min(1, max > 0 ? value / max : 0));
    return (
      <div className={`hud-toast hud-toast-adjust accent-${isSpeed ? "cyan" : "magenta"}`}>
        <span className="hud-label">{isSpeed ? "VELOCITY" : "TILT"}</span>
        <span className="hud-value">
          {value.toFixed(1)}
          <span className="hud-unit">{isSpeed ? "KM/H" : "%"}</span>
        </span>
        <div className="hud-bar">
          <div className="hud-bar-fill" style={{ width: `${fill * 100}%` }} />
        </div>
      </div>
    );
  }

  const accent =
    toast.type === "AUTOMATION"
      ? "yellow"
      : toast.type === "LIGHTS_OUT"
        ? "magenta"
        : "cyan";
  return (
    <div className={`hud-toast accent-${accent}`}>
      <div className="hud-toast-body">
        <span className="hud-label">SYS // {toast.type}</span>
        <span className="hud-toast-message">{toast.message}</span>
      </div>
      {toast.type === "AUTOMATION" && toast.isActionable && (
        <SmallButton onClick={() => send({ type: "program_skip" })}>SKIP REST</SmallButton>
      )}
      <HudNote error={error} />
    </div>
  );
}
