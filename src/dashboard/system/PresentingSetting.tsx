import { dispatch } from "../../ipc";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { useAppSnapshot } from "../../state/useAppState";
import { Toggle } from "./fields";

/** Hold cards while presenting (Windows only): hold the HUD's non-urgent cards and toasts while a presentation or full-screen app is in front. */
export function PresentingSetting() {
  const s = useAppSnapshot();
  const action = useAction();
  // Only Windows reports a presentation or full-screen app (`platform::presenting`).
  if (!s || s.platform.os !== "windows") return null;
  return (
    <div>
      <Toggle
        label="Hold cards while presenting"
        checked={s.presenting.enabled}
        onChange={(enabled) => void action.run(() => dispatch({ type: "presenting_set_config", enabled }))}
        description="Cards that can wait show once the slideshow or full-screen app closes."
      />
      <ActionStatus pending={action.pending} error={action.error} className="mt-1" />
    </div>
  );
}
