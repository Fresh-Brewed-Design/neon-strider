import { useState, type ReactNode } from "react";
import { Plus, X } from "lucide-react";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { dispatch } from "../../ipc";
import { roundSpeed, snapToDeck, snapToLevel } from "../../state/deckSpeeds";
import { useAppState } from "../../state/useAppState";
import { GhostButton, Hint, Label, NeonButton, Panel } from "../ui";
import { cx } from "../util";
import { fromDraft, type Draft, type StepDraft } from "./draft";

const field =
  "rounded border border-white/15 bg-black/50 px-2 py-1.5 font-mono text-xs text-white outline-none focus:border-neon-cyan/60 disabled:opacity-30";
// Width is separate so a narrow field doesn't fight w-full in Tailwind's cascade.
const input = `${field} w-full`;

function Field({ label, children, className }: { label: string; children: ReactNode; className?: string }) {
  return (
    <label className={cx("flex flex-col gap-1", className)}>
      <Label>{label}</Label>
      {children}
    </label>
  );
}

/** Create or edit a program. Rust checks the ranges when it saves. */
export function ProgramEditor({ initial, onDone }: { initial: Draft; onDone: () => void }) {
  const [d, setD] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  const save = useAction();
  const deck = useAppState((snap) => snap.deck);
  // A typed speed moves to one the deck runs (DECK_PROFILE) when the field is left.
  const snapSpeedText = (text: string) => {
    const v = Number(text);
    if (text.trim() === "" || !Number.isFinite(v)) return text;
    const snapped = roundSpeed(snapToDeck(v, deck));
    return snapped === v ? text : String(snapped);
  };

  // A typed incline moves to the grade of a level the deck runs.
  const snapInclineText = (text: string) => {
    const pct = Number(text);
    if (!deck) return text;
    const per = deck.incline_pct_per_level;
    if (text.trim() === "" || !Number.isFinite(pct) || Math.abs(per) < 1e-6) return text;
    const level = Math.round((pct - deck.incline_offset_pct) / per);
    const snapped = snapToLevel(level, deck);
    if (snapped === level) return text;
    return String(Math.round((deck.incline_offset_pct + snapped * per) * 100) / 100);
  };

  const patchStep = (i: number, patch: Partial<StepDraft>) =>
    setD((prev) => ({ ...prev, steps: prev.steps.map((s, j) => (j === i ? { ...s, ...patch } : s)) }));

  const submit = async () => {
    const program = fromDraft(d);
    if (typeof program === "string") {
      setError(program);
      return;
    }
    setError(null);
    const saved = await save.run(async () => {
      await dispatch({ type: "program_save", program });
      return true;
    });
    if (saved) onDone();
  };

  const last = d.steps.length - 1;

  return (
    <Panel className="flex flex-col gap-4 p-4">
      <div className="flex items-center justify-between">
        <Label className="text-neon-cyan">{d.id === 0 ? "New program" : "Edit program"}</Label>
        <GhostButton aria-label="Cancel" onClick={onDone}>
          <X size={14} />
        </GhostButton>
      </div>

      <Field label="Name">
        <input
          className={input}
          value={d.name}
          maxLength={40}
          placeholder="Post-meal"
          autoFocus
          onChange={(e) => {
            setError(null);
            setD({ ...d, name: e.target.value });
          }}
        />
      </Field>

      <div className="flex flex-col gap-2">
        <div className="grid grid-cols-[2rem_1fr_1fr_1fr_2rem] items-end gap-2">
          <span />
          <Label>Minutes</Label>
          <Label>Speed km/h</Label>
          <Label>Incline %</Label>
          <span />
        </div>
        {d.steps.map((s, i) => (
          <div key={i} className="grid grid-cols-[2rem_1fr_1fr_1fr_2rem] items-center gap-2">
            <span className="font-digital text-xs text-white/40">{i + 1}</span>
            <input
              className={input}
              inputMode="decimal"
              value={s.open ? "" : s.minutes}
              placeholder={s.open ? "until ended" : ""}
              disabled={s.open}
              onChange={(e) => patchStep(i, { minutes: e.target.value })}
            />
            <input
              className={input}
              inputMode="decimal"
              value={s.speed}
              onChange={(e) => patchStep(i, { speed: e.target.value })}
              onBlur={() => patchStep(i, { speed: snapSpeedText(s.speed) })}
            />
            <input
              className={input}
              inputMode="decimal"
              value={s.incline}
              onChange={(e) => patchStep(i, { incline: e.target.value })}
              onBlur={() => patchStep(i, { incline: snapInclineText(s.incline) })}
            />
            <GhostButton
              aria-label={`Remove step ${i + 1}`}
              disabled={d.steps.length === 1}
              onClick={() => setD({ ...d, steps: d.steps.filter((_, j) => j !== i) })}
            >
              <X size={14} />
            </GhostButton>
          </div>
        ))}
        <GhostButton
          className="flex items-center gap-1.5 self-start"
          disabled={d.steps.length >= 20}
          onClick={() =>
            setD({
              ...d,
              // A new step follows a timed one, so the previous last step can't stay open-ended.
              steps: [...d.steps.map((s) => ({ ...s, open: false })), { ...d.steps[last], open: false }],
            })
          }
        >
          <Plus size={14} /> Add step
        </GhostButton>
      </div>

      <div className="flex flex-col gap-2">
        <label className="flex items-center gap-2 font-mono text-xs text-white/70">
          <input
            type="checkbox"
            checked={d.steps[last].open}
            disabled={d.repeat}
            onChange={(e) => patchStep(last, { open: e.target.checked })}
          />
          <span className={cx(d.repeat && "opacity-40")}>Keep the last step going until I end it</span>
        </label>
        <div className="flex items-center gap-2 whitespace-nowrap font-mono text-xs text-white/70">
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={d.repeat}
              onChange={(e) =>
                setD({ ...d, repeat: e.target.checked, steps: d.steps.map((st) => ({ ...st, open: e.target.checked ? false : st.open })) })
              }
            />
            Repeat every
          </label>
          <input
            className={cx(field, "w-16 shrink-0")}
            inputMode="decimal"
            aria-label="Repeat interval in minutes"
            disabled={!d.repeat}
            value={d.repeatMinutes}
            onChange={(e) => setD({ ...d, repeatMinutes: e.target.value })}
          />
          <span className={cx(!d.repeat && "opacity-40")}>minutes, back to normal in between</span>
        </div>
      </div>

      <Hint>
        When a program finishes or you end it, the treadmill goes back to what it was doing before: same speed if
        you were walking, otherwise it pauses. Ending it while the belt is paused leaves it paused; Engage then
        starts it at your earlier speed. A program never starts a still belt by itself: after a rest, or after a 0
        km/h step, the next walk waits for you to press GO.
      </Hint>

      <ActionStatus error={error ?? save.error} />

      <div className="flex justify-end gap-2">
        <GhostButton onClick={onDone}>Cancel</GhostButton>
        <NeonButton className="h-9 px-5" disabled={save.pending} onClick={() => void submit()}>
          Save
        </NeonButton>
      </div>
    </Panel>
  );
}
