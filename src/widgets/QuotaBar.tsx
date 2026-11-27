import type { DailyQuotaSnapshot } from "../ipc";

/**
 * DAILY_QUOTA's thin bar: today's km over the daily target, in the current
 * text colour on a faint track, with Rust's sentence as the tooltip. The
 * caller's class places it out of the flow (absolute, at the cell's foot), so
 * the cell keeps its height with or without a target. Full once the target is
 * walked, and nothing more: no colour change, no mark.
 */
export function QuotaBar({ quota, className }: { quota: DailyQuotaSnapshot; className?: string }) {
  if (quota.target_km === null) return null;
  const pct = Math.round(Math.min(1, Math.max(0, quota.fraction)) * 1000) / 10;
  return (
    <span
      className={className}
      role="progressbar"
      aria-label="DAILY_QUOTA: daily km target"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={pct}
      aria-valuetext={quota.message}
      title={quota.message}
      data-testid="quota-bar"
    >
      <span style={{ position: "absolute", inset: 0, background: "currentColor", opacity: 0.2 }} />
      <span
        data-testid="quota-fill"
        style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: `${pct}%`, background: "currentColor" }}
      />
    </span>
  );
}
