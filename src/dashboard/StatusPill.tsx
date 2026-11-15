import type { ComponentType } from "react";
import { AlertTriangle, Bluetooth, Check, CircleSlash, Loader2 } from "lucide-react";
import type { StatusTone } from "../state/linkStatus";
import { cx } from "./util";

const TONE: Record<StatusTone, { text: string; border: string; Icon: ComponentType<{ size?: number; className?: string }> }> = {
  ok: { text: "text-neon-cyan", border: "border-neon-cyan/40", Icon: Check },
  working: { text: "text-neon-yellow", border: "border-neon-yellow/40", Icon: Loader2 },
  problem: { text: "text-red-400", border: "border-red-400/40", Icon: AlertTriangle },
  idle: { text: "text-white/50", border: "border-white/15", Icon: CircleSlash },
};

/**
 * The shared status chip: colour and icon by tone (cyan ok, yellow working,
 * red problem), a short upper-case label, and the long explanation in the
 * tooltip. Used by the rail link control, the Live page and the Strava header.
 */
export function StatusPill({
  tone,
  label,
  title,
  icon,
  bare = false,
  className,
}: {
  tone: StatusTone;
  label: string;
  title?: string;
  /** Replaces the tone's icon (the rail uses the Bluetooth glyph). */
  icon?: "bluetooth";
  /** No border: for use inside a button that already has its own frame. */
  bare?: boolean;
  className?: string;
}) {
  const t = TONE[tone];
  const Icon = icon === "bluetooth" ? Bluetooth : t.Icon;
  return (
    <span
      title={title}
      className={cx(
        "inline-flex max-w-full items-center gap-1.5 font-mono text-[11px] font-bold uppercase tracking-[0.14em]",
        t.text,
        !bare && cx("rounded border px-2 py-0.5", t.border),
        className,
      )}
    >
      <Icon size={12} className={cx("shrink-0", tone === "working" && "animate-spin")} />
      <span className="truncate">{label}</span>
    </span>
  );
}
