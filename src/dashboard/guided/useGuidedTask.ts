import { useEffect, useRef } from "react";
import type { GuidedTask } from "../../ipc";
import { send } from "../util";

/**
 * Ask Rust to pin the dashboard while `active` is true: `guided_task_start`
 * when it turns true (or the component mounts with it true), `guided_task_end`
 * when it turns false. Rust owns the pin (`guided_task.rs`), so leaving the
 * page or closing the window ends nothing; the task's own end, UNPIN or the
 * timeout does.
 */
export function useGuidedTask(task: GuidedTask, active: boolean) {
  const wasActive = useRef(false);
  useEffect(() => {
    if (active === wasActive.current) return;
    wasActive.current = active;
    send({ type: active ? "guided_task_start" : "guided_task_end", task });
  }, [task, active]);
}
