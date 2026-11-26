import { dispatch } from "../../ipc";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { useAppSnapshot } from "../../state/useAppState";
import { Toggle } from "./fields";

/** Lights out: stop the treadmill (ending a walking program) when the PC's display turns off; a program at rest carries on. */
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
        description="The treadmill stops and a walking program ends."
        hint="A program at rest carries on."
      />
      <ActionStatus pending={action.pending} error={action.error} className="mt-1" />
    </div>
  );
}
