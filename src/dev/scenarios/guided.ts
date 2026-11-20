// Guided tasks: the dashboard held open and on top. ?guided: GROUND_TRUTH
// holds the pin, 28 min before it lets go by itself. The calibration preview
// (?calwizard=counting and the rest) pins it the same way, through the
// guided_task_start its run panel sends.
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

/** Same as `TASK_TIMEOUT` in src-tauri/src/guided_task.rs. */
const TIMEOUT_S = 30 * 60;

function setTasks(tasks: typeof snapshot.guided_task.tasks, unpin_in_s: number | null = TIMEOUT_S) {
  snapshot.guided_task = { pinned: tasks.length > 0, tasks, unpin_in_s: tasks.length > 0 ? unpin_in_s : null };
}

export const guided: Scenario = {
  apply: () => {
    if (params.has("guided")) setTasks(["ground_truth"], 28 * 60);
  },
  commands: {
    guided_task_start: (c) =>
      setTasks(snapshot.guided_task.tasks.includes(c.task) ? snapshot.guided_task.tasks : [...snapshot.guided_task.tasks, c.task]),
    guided_task_end: (c) => setTasks(snapshot.guided_task.tasks.filter((t) => t !== c.task)),
    guided_task_cancel: () => setTasks([]),
  },
};
