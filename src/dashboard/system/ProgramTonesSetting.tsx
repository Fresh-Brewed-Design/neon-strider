import { dispatch } from "../../ipc";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { useAppSnapshot } from "../../state/useAppState";
import { Toggle } from "./fields";

/** Program tones: the HUD beeps at a step change, a wait for GO and just before a faster step. */
export function ProgramTonesSetting() {
  const s = useAppSnapshot();
  const action = useAction();
  if (!s) return null;
  return (
    <div>
      <Toggle
        label="Program tones"
        checked={s.program_tones.enabled}
        onChange={(on) => void action.run(() => dispatch({ type: "program_tones_set", on }))}
        description="The HUD beeps at a program step change, a wait for GO and 5 seconds before a faster step."
      />
      <ActionStatus pending={action.pending} error={action.error} className="mt-1" />
    </div>
  );
}
