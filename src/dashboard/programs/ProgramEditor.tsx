import { useState, type ReactNode } from "react";
import { HeartPulse, Plus, X } from "lucide-react";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { dispatch, SETTING_LIMITS } from "../../ipc";
import { roundSpeed, snapToDeck, snapToLevel } from "../../state/deckSpeeds";
import { useAppState } from "../../state/useAppState";
import { GhostButton, Hint, Label, NeonButton, Panel } from "../ui";
import { ChoiceGroup } from "../system/fields";
import { cx } from "../util";
import { fromDraft, withZoneUnit, zoneSummary, type Draft, type StepDraft, type ZoneUnit } from "./draft";

/** Until Rust's snapshot arrives: the estimate Rust starts from (220 minus age at 30). */
const FALLBACK_MAX_BPM = SETTING_LIMITS.default_max_heart_rate_bpm;

const ZONE_UNITS: readonly { value: ZoneUnit; label: string }[] = [
  { value: "pct", label: "% max" },
  { value: "bpm", label: "bpm" },
];

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
  const maxBpm = useAppState((snap) => snap.heart_rate.max_bpm) || FALLBACK_MAX_BPM;
  const anyZone = d.steps.some((s) => s.zone);
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
    const program = fromDraft(d, maxBpm);
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
          maxLength={SETTING_LIMITS.program_name_chars}
          placeholder="Post-meal"
          autoFocus
          onChange={(e) => {
            setError(null);
            setD({ ...d, name: e.target.value });
          }}
        />
      </Field>

      <div className="flex flex-col gap-2">
        <div className="grid grid-cols-[2rem_1fr_1fr_1fr_2rem_2rem] items-end gap-2">
          <span />
          <Label>Minutes</Label>
          <Label>Speed km/h</Label>
          <Label>Incline %</Label>
          <span />
          <span />
        </div>
        {d.steps.map((s, i) => (
          <div key={i} className="flex flex-col gap-1.5">
          <div className="grid grid-cols-[2rem_1fr_1fr_1fr_2rem_2rem] items-center gap-2">
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
              aria-label={`Heart zone on step ${i + 1}`}
              aria-pressed={s.zone}
              title="Hold a heart rate zone"
              className={cx(s.zone && "text-neon-magenta!")}
              onClick={() => {
                setError(null);
                patchStep(i, { zone: !s.zone });
              }}
            >
              <HeartPulse size={14} />
            </GhostButton>
            <GhostButton
              aria-label={`Remove step ${i + 1}`}
              disabled={d.steps.length === 1}
              onClick={() => setD({ ...d, steps: d.steps.filter((_, j) => j !== i) })}
            >
              <X size={14} />
            </GhostButton>
          </div>
          {s.zone && <ZoneRow step={s} n={i + 1} unit={d.zoneUnit} maxBpm={maxBpm} onChange={(patch) => patchStep(i, patch)} />}
          </div>
        ))}
        <GhostButton
          className="flex items-center gap-1.5 self-start"
          disabled={d.steps.length >= SETTING_LIMITS.program_steps}
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

      {anyZone && (
        <div className="flex flex-col gap-1.5">
          <div className="flex items-center gap-3">
            <Label>Heart zones in</Label>
            <ChoiceGroup
              label="Heart zone unit"
              tone="magenta"
              options={ZONE_UNITS}
              value={d.zoneUnit}
              onChange={(unit) => setD(withZoneUnit(d, unit, maxBpm))}
            />
          </div>
          <Hint>
            A heart zone trims a moving step&apos;s speed by up to 1 km/h and never starts the belt. Zones are a share of
            the {maxBpm} bpm max heart rate in System, Heart rate.
          </Hint>
        </div>
      )}

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

/** The low and high bounds of a step's heart zone, with the other unit beside them. */
function ZoneRow({
  step,
  n,
  unit,
  maxBpm,
  onChange,
}: {
  step: StepDraft;
  n: number;
  unit: ZoneUnit;
  maxBpm: number;
  onChange: (patch: Partial<StepDraft>) => void;
}) {
  const summary = zoneSummary(step, unit, maxBpm);
  const other =
    summary && (unit === "pct" ? `${summary.bpm[0]} to ${summary.bpm[1]} bpm` : `${summary.pct[0]} to ${summary.pct[1]} % of max`);
  const unitText = unit === "pct" ? "% of max" : "bpm";
  return (
    <div className="grid grid-cols-[2rem_1fr] items-center gap-2">
      <span />
      <div className="flex flex-wrap items-center gap-2 font-mono text-xs text-white/70">
        <span className="font-bold uppercase tracking-[0.12em] text-neon-magenta">Heart zone</span>
        <input
          className={cx(field, "w-16")}
          inputMode="decimal"
          aria-label={`Step ${n} heart zone low`}
          value={step.zoneLow}
          onChange={(e) => onChange({ zoneLow: e.target.value })}
        />
        <span>to</span>
        <input
          className={cx(field, "w-16")}
          inputMode="decimal"
          aria-label={`Step ${n} heart zone high`}
          value={step.zoneHigh}
          onChange={(e) => onChange({ zoneHigh: e.target.value })}
        />
        <span>{unitText}</span>
        {other && <span className="text-white/40">{other}</span>}
      </div>
    </div>
  );
}
