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
        label="LIGHTS_OUT: Sleep with the display"
        tone="magenta"
        checked={s.lights_out.enabled}
        onChange={(enabled) => void action.run(() => dispatch({ type: "lights_out_set_config", enabled }))}
        description="When the display turns off, the treadmill stops and any program ends, so the deck can sleep."
      />
      <ActionStatus pending={action.pending} error={action.error} className="mt-1" />
    </div>
  );
}
