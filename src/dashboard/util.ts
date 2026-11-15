// Small helpers shared by the dashboard pages.

import { toast } from "sonner";
import { errorText } from "../components/useAction";
import { dispatch, type Command } from "../ipc";

/**
 * Fire a command at Rust. A rejection (bad value, not connected, a program
 * error) is never swallowed: it shows as a toast. Controls that have a place
 * for a message use `useAction` instead, so the error appears next to them;
 * this toast is the backstop for the rest.
 */
export function send(command: Command) {
  dispatch(command).catch((e) => {
    console.error("[dashboard] command failed", command, e);
    toast.error(errorText(e), { id: `command-${command.type}` });
  });
}

/** A thunk that dispatches `command`, for `useAction().run`. */
export const post = (command: Command) => () => dispatch(command);

export function cx(...parts: (string | false | null | undefined)[]) {
  return parts.filter(Boolean).join(" ");
}

export type Tone = "cyan" | "magenta" | "yellow" | "green";

export const TONE_TEXT: Record<Tone, string> = {
  cyan: "text-neon-cyan",
  magenta: "text-neon-magenta",
  yellow: "text-neon-yellow",
  green: "text-emerald-400",
};

/** Opens the treadmill connect dialog owned by the dashboard shell. */
export const OPEN_LINK_EVENT = "neon:open-link";
export function openLinkDialog() {
  window.dispatchEvent(new Event(OPEN_LINK_EVENT));
}
