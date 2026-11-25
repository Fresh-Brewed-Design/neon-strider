import { dispatch } from "../../../ipc";
import { ActionStatus } from "../../../components/ActionStatus";
import { useAction } from "../../../components/useAction";
import { useTreadmillStore } from "../../../store/treadmillStore";
import { GhostButton, Hint, Label, NeonButton } from "../../ui";

const plural = (n: number, one: string, many: string) => (n === 1 ? `1 ${one}` : `${n} ${many}`);

/**
 * START FRESH (`calibration_start_fresh`): Rust sets the treadmill's points
 * aside, so the guided walk starts from none instead of averaging into them.
 * Shown with the top speed question while the treadmill has points and no set
 * is aside. Never moves the belt.
 */
export function StartFresh() {
  const action = useAction();
  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between gap-3">
        <Hint>START FRESH sets the saved points aside for a new walk.</Hint>
        <GhostButton
          className="shrink-0 border border-white/15"
          disabled={action.pending}
          onClick={() => void action.run(() => dispatch({ type: "calibration_start_fresh" }))}
        >
          START FRESH
        </GhostButton>
      </div>
      <ActionStatus pending={action.pending} error={action.error} />
    </div>
  );
}

/**
 * The set START FRESH put aside (`snapshot.calibration.aside_count`): RESTORE
 * brings it back and drops the new points (ending any walk), KEEP NEW deletes
 * it. Each asks first when it would drop points. Neither moves the belt.
 */
export function PointsAside({ count, newPoints }: { count: number; newPoints: number }) {
  const action = useAction();
  const confirm = useTreadmillStore((s) => s.confirm);
  const restore = () => void action.run(() => dispatch({ type: "calibration_restore" }));
  const keepNew = () => void action.run(() => dispatch({ type: "calibration_keep_new" }));
  const askRestore = () =>
    newPoints === 0
      ? restore()
      : confirm({
          title: "Restore the old points",
          message: `RESTORE drops the ${plural(newPoints, "new point", "new points")}.`,
          confirmLabel: "Restore",
          tone: "danger",
          onConfirm: restore,
        });
  const askKeepNew = () =>
    confirm({
      title: "Delete the old points",
      message: `KEEP NEW deletes the ${plural(count, "old point", "old points")} for good.`,
      confirmLabel: "Delete",
      tone: "danger",
      onConfirm: keepNew,
    });
  return (
    <div className="hacker-panel space-y-2 rounded-lg border-neon-yellow/30 bg-white/[0.02] p-3">
      <Label className="text-neon-yellow">POINTS_ASIDE</Label>
      <div className="font-digital text-sm text-white">
        {count === 1 ? "1 old point is set aside." : `${count} old points are set aside.`}
      </div>
      <Hint>RESTORE brings the old points back and KEEP NEW deletes them.</Hint>
      <div className="flex flex-wrap items-center gap-2">
        <NeonButton className="px-3 py-1.5" tone="magenta" disabled={action.pending} onClick={askRestore}>
          RESTORE
        </NeonButton>
        <GhostButton className="border border-white/15" disabled={action.pending} onClick={askKeepNew}>
          KEEP NEW
        </GhostButton>
      </div>
      <ActionStatus pending={action.pending} error={action.error} />
    </div>
  );
}
