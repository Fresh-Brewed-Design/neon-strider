import { useEffect, useRef, useState } from "react";
import { X } from "lucide-react";
import { dispatch, type KeyCombo, type Shortcut, type ShortcutView } from "../../ipc";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { useAppSnapshot } from "../../state/useAppState";
import { GhostButton, Hint } from "../ui";
import { cx } from "../util";
import { Section } from "./fields";

/** Each shortcut's name and what it does, in the order Rust lists them. */
const SHORTCUT_TEXT: Record<Shortcut, { name: string; does: string }> = {
  speed_up: { name: "Speed up", does: "Raises the belt speed one step." },
  speed_down: { name: "Speed down", does: "Lowers the belt speed one step." },
  incline_up: { name: "Incline up", does: "Raises the incline one level." },
  incline_down: { name: "Incline down", does: "Lowers the incline one level." },
  toggle_start: { name: "Start or pause", does: "Pauses a moving belt, otherwise starts it (GO on a paused program)." },
  toggle_program: { name: "Run or end a program", does: "Ends the running program, or runs the last one again." },
  toggle_minified: { name: "Minify the HUD", does: "Shrinks the HUD to a strip, or brings the whole panel back." },
  calibration_tap: {
    name: "GROUND_TRUTH tap",
    does: "Only while a tap run listens; the rest of the time the keys go to the app in front.",
  },
};

/** The most keys one shortcut can have (Rust's `shortcuts::MAX_KEYS`). */
const MAX_KEYS = 2;

/** Keys that only modify another: a recording waits past them for the real key. */
const MODIFIER_CODES = new Set([
  "ShiftLeft",
  "ShiftRight",
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
  "OSLeft",
  "OSRight",
  "CapsLock",
  "Fn",
  "FnLock",
]);

/** The key pressed, as Rust stores it, or null while only modifiers are down. */
function comboFrom(e: KeyboardEvent): KeyCombo | null {
  if (MODIFIER_CODES.has(e.code) || e.code === "") return null;
  return { code: e.code, ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, meta: e.metaKey };
}

/**
 * Listen for the next key combination, with the shortcuts paused meanwhile
 * (so one already in use reaches the dashboard). Esc cancels. Calls `done`
 * with the combination, or null when cancelled.
 */
function useRecorder(active: boolean, done: (combo: KeyCombo | null) => void) {
  // The latest callback, so re-renders (every snapshot tick) don't restart it.
  const doneRef = useRef(done);
  useEffect(() => {
    doneRef.current = done;
  });
  useEffect(() => {
    if (!active) return;
    const finish = (combo: KeyCombo | null) => doneRef.current(combo);
    void dispatch({ type: "shortcuts_recording", on: true }).catch(() => {});
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.code === "Escape" && !(e.ctrlKey || e.altKey || e.metaKey || e.shiftKey)) {
        finish(null);
        return;
      }
      const combo = comboFrom(e);
      if (combo) finish(combo);
    };
    const onBlur = () => finish(null);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", onBlur);
      void dispatch({ type: "shortcuts_recording", on: false }).catch(() => {});
    };
  }, [active]);
}

function ShortcutRow({
  view,
  recording,
  onRecord,
  onRecorded,
}: {
  view: ShortcutView;
  recording: boolean;
  onRecord: () => void;
  onRecorded: () => void;
}) {
  const change = useAction();
  const text = SHORTCUT_TEXT[view.shortcut];
  const combos = view.keys.map((k) => k.combo);
  const save = (keys: KeyCombo[]) =>
    void change.run(() => dispatch({ type: "shortcuts_set", shortcut: view.shortcut, keys }));

  // A recorded key joins the keys the row shows now.
  useRecorder(recording, (combo) => {
    onRecorded();
    if (combo) save([...combos, combo]);
  });

  return (
    <div className="flex flex-col gap-1">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0 flex-[1_1_15rem]">
          <span className="font-mono text-xs font-bold uppercase tracking-[0.15em] text-white">{text.name}</span>
          <Hint className="mt-0.5">{text.does}</Hint>
        </div>
        <div className="ml-auto flex flex-wrap items-center justify-end gap-1.5">
          {view.keys.length === 0 && !recording && (
            <span className="font-mono text-xs uppercase tracking-[0.15em] text-white/60">Off</span>
          )}
          {view.keys.map((k, i) => (
            <span
              key={k.label}
              title={k.taken ? "Another app already uses this shortcut, so it does nothing here." : undefined}
              className={cx(
                "flex items-center gap-1 rounded border px-2 py-1 font-mono text-xs",
                k.taken ? "border-red-400/50 text-red-300 line-through" : "border-neon-cyan/40 text-neon-cyan",
              )}
            >
              {k.label}
              <button
                type="button"
                aria-label={`Remove ${k.label} from ${text.name}`}
                disabled={change.pending || recording}
                onClick={() => save(combos.filter((_, j) => j !== i))}
                className="rounded text-white/60 hover:text-white disabled:opacity-30"
              >
                <X size={12} />
              </button>
            </span>
          ))}
          {recording ? (
            <span
              role="status"
              className="rounded border border-neon-yellow/60 bg-neon-yellow/10 px-2 py-1 font-mono text-xs text-neon-yellow"
            >
              Press the keys… Esc cancels
            </span>
          ) : (
            view.keys.length < MAX_KEYS && (
              <GhostButton disabled={change.pending} onClick={onRecord} aria-label={`Add a shortcut for ${text.name}`}>
                Add
              </GhostButton>
            )
          )}
        </div>
      </div>
      {view.keys.some((k) => k.taken) && (
        <Hint className="text-red-300/90">Another app already uses the crossed-out key; pick another.</Hint>
      )}
      <ActionStatus pending={change.pending} error={change.error} />
    </div>
  );
}

/** System -> Shortcuts: each global shortcut's keys, recorded from the keyboard. */
export function ShortcutsSection() {
  const s = useAppSnapshot();
  const [recording, setRecording] = useState<Shortcut | null>(null);
  const reset = useAction();
  if (!s) return null;
  const mac = s.platform.os === "macos";
  return (
    <Section
      id="sys-shortcuts"
      title="Shortcuts"
      description={
        mac
          ? "Work from any app. Each needs ⌃ or ⌘; ⌥ and ⇧ can join them."
          : "Work from any app. Each needs Ctrl, Alt or the Windows key; Shift can join them."
      }
    >
      {s.shortcuts.list.map((view) => (
        <ShortcutRow
          key={view.shortcut}
          view={view}
          recording={recording === view.shortcut}
          onRecord={() => setRecording(view.shortcut)}
          onRecorded={() => setRecording(null)}
        />
      ))}
      {s.shortcuts.changed && (
        <div className="flex flex-col items-start gap-1">
          <GhostButton
            disabled={reset.pending || recording !== null}
            onClick={() => void reset.run(() => dispatch({ type: "shortcuts_reset" }))}
          >
            Reset to the defaults
          </GhostButton>
          <ActionStatus pending={reset.pending} error={reset.error} />
        </div>
      )}
    </Section>
  );
}
