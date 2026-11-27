import { useEffect } from "react";
import { CloudUpload, Loader2 } from "lucide-react";
import { ActionStatus } from "../components/ActionStatus";
import type { BackendAction } from "../components/useAction";
import type { StravaSnapshot } from "../ipc";
import { PLAIN } from "../names";
import { dailyUplinkLine } from "./dailyUplinkText";
import { activityText } from "./stravaStatus";
import { Hint, NeonButton, PlainLine } from "./ui";
import { cx, post } from "./util";

/** The button's tooltip: what a press does, in plain words. */
export const DAILY_UPLINK_TITLE = "Uploads today's walks not on Strava yet as one activity.";

/**
 * DAILY_UPLINK: today's walks not on Strava yet, uploaded now as one activity
 * (the daily rollup on demand). Rust owns what goes up and why the button is
 * off; progress and the result show under the button. `onDone` runs after a
 * good result (History reloads its rows).
 */
export function DailyUplink({
  strava,
  action,
  compact = false,
  onDone,
}: {
  strava: StravaSnapshot;
  action: BackendAction;
  /** History's smaller button beside the day heading. */
  compact?: boolean;
  onDone?: () => void;
}) {
  const today = strava.today;
  const working = action.pending;
  const ready = today.blocked === null && !strava.syncing && !working;

  useEffect(() => {
    if (action.done) onDone?.();
  }, [action.done, onDone]);

  return (
    <div className={cx("flex flex-col gap-1.5", compact ? "items-end text-right" : "max-w-56 items-start")} data-testid="daily-uplink">
      <NeonButton
        disabled={!ready}
        title={DAILY_UPLINK_TITLE}
        aria-label={`DAILY_UPLINK: ${DAILY_UPLINK_TITLE}`}
        onClick={() => void action.run(post({ type: "strava_upload_today" }))}
        className={cx("flex shrink-0 items-center gap-2", compact ? "h-7 px-2.5" : "px-3 py-2")}
      >
        {working ? <Loader2 size={14} className="animate-spin" /> : <CloudUpload size={14} />}
        {working ? "Uploading…" : "DAILY_UPLINK"}
      </NeonButton>
      {!compact && <PlainLine text={PLAIN.DAILY_UPLINK} />}
      {!working && <Hint data-testid="daily-uplink-line">{dailyUplinkLine(today)}</Hint>}
      {!working && today.blocked === null && today.note && <Hint>{today.note}</Hint>}
      <ActionStatus
        pending={working && (activityText(strava.activity) ?? "Uploading today…")}
        error={action.error}
        done={action.done}
      />
    </div>
  );
}
