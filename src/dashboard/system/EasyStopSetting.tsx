import { dispatch } from "../../ipc";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { useAppSnapshot } from "../../state/useAppState";
import { Toggle } from "./fields";

/** Easy stop: DUMP on a manual walk eases the belt down to the slowest speed before stopping it. */
export function EasyStopSetting() {
  const s = useAppSnapshot();
  const action = useAction();
  if (!s) return null;
  return (
    <div>
      <Toggle
        label="Easy stop"
        checked={s.easy_stop.enabled}
        onChange={(on) => void action.run(() => dispatch({ type: "easy_stop_set", on }))}
        description={`Ending a manual walk slows the belt to the slowest speed over ${s.easy_stop.ramp_s} s, then stops it.`}
        hint="A second DUMP stops the belt at once."
      />
      <ActionStatus pending={action.pending} error={action.error} className="mt-1" />
    </div>
  );
}
