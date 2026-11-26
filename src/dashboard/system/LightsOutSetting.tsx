import { dispatch } from "../../ipc";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { useAppSnapshot } from "../../state/useAppState";
import { Toggle } from "./fields";

/** Lights out: stop the treadmill (ending any program) when the PC's display turns off. */
export function LightsOutSetting() {
  const s = useAppSnapshot();
  const action = useAction();
  if (!s) return null;
  return (
    <div>
      <Toggle
        label="LIGHTS_OUT: Stop when the display turns off"
        checked={s.lights_out.enabled}
        onChange={(enabled) => void action.run(() => dispatch({ type: "lights_out_set_config", enabled }))}
        description="The treadmill stops and any program ends."
        hint="The deck can then go to sleep."
      />
      <ActionStatus pending={action.pending} error={action.error} className="mt-1" />
    </div>
  );
}
