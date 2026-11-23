import { dispatch } from "../../ipc";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { useAppSnapshot } from "../../state/useAppState";
import { Toggle } from "./fields";

/** Easy start: every belt start begins at the slowest speed and eases up to the target. */
export function EasyStartSetting() {
  const s = useAppSnapshot();
  const action = useAction();
  if (!s) return null;
  return (
    <div>
      <Toggle
        label="Easy start"
        checked={s.easy_start.enabled}
        onChange={(on) => void action.run(() => dispatch({ type: "easy_start_set", on }))}
        description="Every belt start begins at the slowest speed and eases up to the target speed over 30 s."
        hint="Pausing, stopping or setting a speed by hand ends the ease at once."
      />
      <ActionStatus pending={action.pending} error={action.error} className="mt-1" />
    </div>
  );
}
