import { useCallback, useEffect, useRef, useState } from "react";
import { dispatch, type Command } from "../ipc";
import { errorText } from "../components/useAction";

const NOTE_MS = 6000;
const MAX_CHARS = 56;

/** A rejected command as a short themed line: `SYS // TREADMILL NOT CONNECTED`. */
export function hudErrorLine(e: unknown): string {
  const text = errorText(e).replace(/\s+/g, " ").trim().toUpperCase().replace(/\.$/, "");
  const cut = text.length > MAX_CHARS ? `${text.slice(0, MAX_CHARS - 1).trimEnd()}…` : text;
  return `SYS // ${cut}`;
}

/**
 * Sends HUD commands. A rejection (not connected, a program error) is never
 * lost: it becomes a short line, shown inside the card that holds the button
 * (`<HudNote>`), which clears itself after a few seconds. `report` shows any
 * other failure the same way.
 */
export function useHudSend() {
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => () => clearTimeout(timer.current), []);

  const report = useCallback((e: unknown) => {
    setError(hudErrorLine(e));
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setError(null), NOTE_MS);
  }, []);

  /** Resolves true once Rust accepted the command, false after a rejection (already shown). */
  const send = useCallback(
    (command: Command): Promise<boolean> =>
      dispatch(command).then(
        () => true,
        (e: unknown) => {
          console.error("[hud] command failed", command, e);
          report(e);
          return false;
        },
      ),
    [report],
  );

  return { send, report, error };
}
