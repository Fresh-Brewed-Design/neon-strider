import { useTreadmillStore } from "../store/treadmillStore";

/**
 * Runs `go` (a change of page) once an editor with unsaved edits has asked and the user
 * discarded them (`leaveGuard`), or at once when nothing needs asking.
 */
export function leavePage(go: () => void) {
  const guard = useTreadmillStore.getState().leaveGuard;
  if (guard) guard(go);
  else go();
}
