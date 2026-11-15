import { useCallback, useEffect, useRef, useState } from "react";

/** A readable message from whatever a rejected `invoke` or `dispatch` threw. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  if (typeof e === "object" && e !== null && "message" in e && typeof e.message === "string") return e.message;
  return "Something went wrong";
}

interface ActionState {
  pending: boolean;
  error: string | null;
  done: string | null;
}

const IDLE: ActionState = { pending: false, error: null, done: null };

/**
 * Run one async action and keep its state next to the control that started it:
 * `pending` from the moment of the click, then either `error` or `done`.
 * Render it with `<ActionStatus>` directly under the button.
 */
export function useAction() {
  const [state, setState] = useState<ActionState>(IDLE);
  const seq = useRef(0);

  const run = useCallback(
    async <T,>(fn: () => Promise<T>, opts?: { done?: string | ((result: T) => string | null) }): Promise<T | undefined> => {
      const id = ++seq.current;
      setState({ pending: true, error: null, done: null });
      try {
        const result = await fn();
        if (id === seq.current) {
          const done = typeof opts?.done === "function" ? opts.done(result) : (opts?.done ?? null);
          setState({ pending: false, error: null, done });
        }
        return result;
      } catch (e) {
        if (id === seq.current) setState({ pending: false, error: errorText(e), done: null });
        return undefined;
      }
    },
    [],
  );

  const reset = useCallback(() => {
    seq.current += 1;
    setState(IDLE);
  }, []);

  return { run, reset, ...state };
}

/** What Rust recorded as the outcome of its last background task. */
export interface BackendResult {
  ok: boolean;
  message: string;
  at_ms: number;
}

/** A command that records nothing counts as finished after this long without Rust being busy. */
const SETTLE_MS = 1500;

type Armed = { since: number; done: string | null } & (
  | { phase: "sending" }
  | { phase: "rejected" }
  | { phase: "accepted"; at: number }
);

/**
 * For a command Rust queues and finishes later (uploads, shoe refresh): the
 * button is pending from the click until Rust records a newer result, or, for
 * a command that records nothing, until Rust has been quiet for a moment.
 * `result` is the latest outcome in the snapshot and `busy` says Rust is still working.
 */
export function useBackendAction(result: BackendResult | null, busy: boolean) {
  const base = useAction();
  const baseRun = base.run;
  const [armed, setArmed] = useState<Armed | null>(null);
  const [now, setNow] = useState(() => Date.now());

  // The result already there at the click is not this command's.
  const latestAt = useRef(result?.at_ms ?? 0);
  useEffect(() => {
    latestAt.current = result?.at_ms ?? 0;
  });

  const fresh = armed && result && result.at_ms > armed.since ? result : null;
  const acceptedAt = armed?.phase === "accepted" && !fresh ? armed.at : null;
  const settled = acceptedAt !== null && !busy && now - acceptedAt >= SETTLE_MS;

  // Wake up once the quiet period is over.
  useEffect(() => {
    if (acceptedAt === null) return;
    const t = setTimeout(() => setNow(Date.now()), Math.max(0, acceptedAt + SETTLE_MS - Date.now()) + 20);
    return () => clearTimeout(t);
  }, [acceptedAt]);

  const run = useCallback(
    async (fn: () => Promise<unknown>, opts?: { done?: string }) => {
      const since = latestAt.current;
      const done = opts?.done ?? null;
      setArmed({ since, done, phase: "sending" });
      const ok = await baseRun(async () => {
        await fn();
        return true as const;
      });
      const at = Date.now();
      setArmed((a) => (a ? (ok ? { ...a, phase: "accepted", at } : { ...a, phase: "rejected" }) : a));
      setNow(at);
    },
    [baseRun],
  );

  const reset = base.reset;
  return {
    run,
    pending: base.pending || (armed !== null && armed.phase !== "rejected" && !fresh && !settled),
    error: base.error ?? (fresh && !fresh.ok ? fresh.message : null),
    done: fresh?.ok ? fresh.message : settled ? (armed?.done ?? null) : null,
    reset: () => {
      reset();
      setArmed(null);
    },
  };
}

export type BackendAction = ReturnType<typeof useBackendAction>;
