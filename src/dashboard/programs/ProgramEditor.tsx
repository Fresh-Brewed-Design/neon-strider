import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { AlertCircle, HeartPulse, Plus, X } from "lucide-react";
import { ActionStatus } from "../../components/ActionStatus";
import { errorText, useAction } from "../../components/useAction";
import { dispatch, SETTING_LIMITS, type StepKind, type ZoneTrim } from "../../ipc";
import { roundSpeed, snapToDeck, snapToLevel } from "../../state/deckSpeeds";
import { useAppState } from "../../state/useAppState";
import { useTreadmillStore } from "../../store/treadmillStore";
import { GhostButton, Hint, Label, NeonButton, Panel } from "../ui";
import { ChoiceGroup } from "../system/fields";
import { cx } from "../util";
import {
  draftChanged,
  draftErrors,
  errorPlaces,
  fromDraft,
  kindPatch,
  rustStepError,
  pulseSummary,
  strideFitFlag,
  withZoneUnit,
  zoneSummary,
  type Draft,
  type StepDraft,
  type StepErrors,
  type ZoneUnit,
} from "./draft";
import { STEP_KINDS } from "./format";

/** Until Rust's snapshot arrives: the estimate Rust starts from (220 minus age at 30). */
const FALLBACK_MAX_BPM = SETTING_LIMITS.default_max_heart_rate_bpm;

const ZONE_UNITS: readonly { value: ZoneUnit; label: string }[] = [
  { value: "pct", label: "% max" },
  { value: "bpm", label: "bpm" },
];

/** What a zone step trims, offered while System -> Labs ZONE_TILT is on. */
const ZONE_TRIMS: readonly { value: ZoneTrim; label: string }[] = [
  { value: "speed", label: "Speed" },
  { value: "incline", label: "Incline" },
  { value: "both", label: "Both" },
];

const field =
  "rounded border border-white/15 bg-black/50 px-2 py-1.5 font-mono text-xs text-white outline-none focus:border-neon-cyan/60 disabled:opacity-30 aria-invalid:border-red-400/70";
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

/** A field's error lines, right under the field; `id` is what the field's aria-describedby points at. */
function FieldErrors({ id, lines, indent = true }: { id: string; lines: (string | undefined)[]; indent?: boolean }) {
  const shown = lines.filter((l): l is string => !!l);
  if (shown.length === 0) return null;
  return (
    <div className={cx(indent && "grid grid-cols-[2rem_1fr] gap-2")}>
      {indent && <span />}
      <div id={id} className="flex flex-col gap-0.5 font-mono text-[11px] leading-relaxed text-red-400">
        {shown.map((l) => (
          <span key={l} className="flex items-start gap-1.5">
            <AlertCircle size={12} className="mt-0.5 shrink-0" />
            <span className="min-w-0 break-words">{l}</span>
          </span>
        ))}
      </div>
    </div>
  );
}

/** aria-invalid and aria-describedby for a field with an error. */
const invalid = (error: string | undefined, id: string) =>
  error ? { "aria-invalid": true as const, "aria-describedby": id } : {};

/** One of a step's number fields; its name shows beside it only when the column names can't (narrow). */
function StepCell({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="program-step-cell flex min-w-0 flex-col gap-1">
      <Label className="program-step-cell-label">{label}</Label>
      {children}
    </div>
  );
}

/**
 * Create or edit a program. Each field shows what is wrong with it as it is typed; Rust checks
 * everything again when it saves. Leaving with unsaved edits (X, Cancel, Escape or another
 * page) asks first.
 */
export function ProgramEditor({ initial, onDone }: { initial: Draft; onDone: () => void }) {
  const [opened] = useState(initial);
  const [d, setDraft] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  // Save was pressed: empty fields count as errors from now on.
  const [tried, setTried] = useState(false);
  // Rust refused the save for one step: its sentence shows under that step until the next edit.
  const [rustStep, setRustStep] = useState<{ step: number; text: string } | null>(null);
  const save = useAction();
  const confirm = useTreadmillStore((st) => st.confirm);
  const setLeaveGuard = useTreadmillStore((st) => st.setLeaveGuard);
  const ids = useId();
  const deck = useAppState((snap) => snap.deck);
  const maxBpm = useAppState((snap) => snap.heart_rate.max_bpm) || FALLBACK_MAX_BPM;
  const inclineTrim = useAppState((snap) => snap.heart_rate.incline_trim) ?? false;
  // STRIDE_FIT (System, Calibration): steps above it get a one-line flag, nothing more.
  const strideFit = useAppState((snap) => snap.deck.stride_fit_kmh) ?? null;
  const range = deck ? { min: deck.min_speed_kmh, max: deck.max_speed_kmh } : null;
  const errors = draftErrors(d, range, maxBpm, tried);
  const places = tried ? errorPlaces(errors) : null;
  const changed = draftChanged(opened, d, maxBpm);
  const anyZone = d.steps.some((s) => s.zone);
  const anyPulse = d.steps.some((s) => s.pulse && s.kind === "cool_down");
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

  // Any edit clears what the last Save said, Rust's step sentence included.
  const setD = (next: Draft | ((prev: Draft) => Draft)) => {
    setError(null);
    setRustStep(null);
    if (save.error) save.reset();
    setDraft(next);
  };

  const patchStep = (i: number, patch: Partial<StepDraft>) =>
    setD((prev) => ({ ...prev, steps: prev.steps.map((s, j) => (j === i ? { ...s, ...patch } : s)) }));

  const discardText = opened.id === 0 ? "Discard the new program?" : `Discard the unsaved edits to "${opened.name}"?`;
  const askDiscard = (then: () => void) => confirm({ title: "Discard edits", message: discardText, onConfirm: then });
  // X, Cancel and Escape: close at once with nothing to lose, else ask.
  const leave = () => (changed ? askDiscard(onDone) : onDone());

  // The latest of each for the listeners below, which are set up once.
  const latest = useRef({ leave, askDiscard, onDone });
  useEffect(() => {
    latest.current = { leave, askDiscard, onDone };
  });

  // Another page while the draft has unsaved edits: ask, then close the editor and go.
  useEffect(() => {
    if (!changed) return;
    setLeaveGuard((go) =>
      latest.current.askDiscard(() => {
        latest.current.onDone();
        go();
      }),
    );
    return () => setLeaveGuard(null);
  }, [changed, setLeaveGuard]);

  // Escape works like Cancel, unless a dialog (or a field of its own) takes it.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented || useTreadmillStore.getState().pendingConfirmation) return;
      e.preventDefault();
      latest.current.leave();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const submit = async () => {
    setTried(true);
    // Errors show at their fields; Save's line says where they are.
    if (errorPlaces(draftErrors(d, range, maxBpm, true)) !== null) return;
    const program = fromDraft(d, maxBpm);
    if (typeof program === "string") {
      setError(program);
      return;
    }
    setError(null);
    const saved = await save.run(async () => {
      try {
        await dispatch({ type: "program_save", program });
      } catch (e) {
        // Rust names the step it refused: its sentence goes under the step, Save's line says which.
        const step = rustStepError(errorText(e));
        if (!step) throw e;
        setRustStep(step);
        throw new Error(`Fix step ${step.step} to save.`);
      }
      return true;
    });
    if (saved) onDone();
  };

  const last = d.steps.length - 1;

  return (
    // Visible overflow lets the Save footer stick to the bottom of the scrolling page.
    <Panel className="flex flex-col gap-4 overflow-visible! p-4">
      <div className="flex items-center justify-between">
        <Label className="text-neon-cyan">{d.id === 0 ? "New program" : "Edit program"}</Label>
        <GhostButton aria-label="Cancel" onClick={leave}>
          <X size={14} />
        </GhostButton>
      </div>

      <div className="flex flex-col gap-1">
        <Field label="Name">
          <input
            className={input}
            value={d.name}
            maxLength={SETTING_LIMITS.program_name_chars}
            placeholder="Post-meal"
            autoFocus
            {...invalid(errors.name, `${ids}-name`)}
            onChange={(e) => setD({ ...d, name: e.target.value })}
          />
        </Field>
        <FieldErrors id={`${ids}-name`} lines={[errors.name]} indent={false} />
      </div>

      {/* Narrow (a large Windows text size), each step wraps its fields under its kind, each
          with its own name, in place of the column names (index.css, .program-steps). */}
      <div className="program-steps flex flex-col gap-2">
        <div className="program-step-head grid grid-cols-[2rem_7rem_1fr_1fr_1fr_2rem_2rem] items-end gap-2">
          <span />
          <Label>Kind</Label>
          <Label>Minutes</Label>
          <Label>Speed km/h</Label>
          <Label>Incline %</Label>
          <Label className="tracking-normal">Zone</Label>
          <span />
        </div>
        {d.steps.map((s, i) => {
          const e: StepErrors = errors.steps[i] ?? {};
          const rowId = `${ids}-step${i}`;
          const rust = rustStep?.step === i + 1 ? rustStep.text : undefined;
          return (
          <div key={i} className="flex flex-col gap-1.5">
          <div className="program-step-row grid grid-cols-[2rem_7rem_1fr_1fr_1fr_2rem_2rem] items-center gap-2">
            <span className="w-8 font-digital text-xs text-white/60">{i + 1}</span>
            <select
              className={cx(input, "program-step-kind px-1")}
              aria-label={`Step ${i + 1} kind`}
              value={s.kind ?? ""}
              onChange={(e) => patchStep(i, kindPatch(s, e.target.value === "" ? null : (e.target.value as StepKind), d.zoneUnit, maxBpm))}
            >
              <option value="">None</option>
              {STEP_KINDS.map((k) => (
                <option key={k.value} value={k.value}>
                  {k.code}
                </option>
              ))}
            </select>
            <StepCell label="Minutes">
              <input
                className={input}
                inputMode="decimal"
                aria-label={`Step ${i + 1} minutes`}
                value={s.open ? "" : s.minutes}
                placeholder={s.open ? "until ended" : ""}
                disabled={s.open}
                {...invalid(e.minutes, rowId)}
                onChange={(ev) => patchStep(i, { minutes: ev.target.value })}
              />
            </StepCell>
            <StepCell label="Speed km/h">
              <input
                className={input}
                inputMode="decimal"
                aria-label={`Step ${i + 1} speed in km/h`}
                value={s.speed}
                {...invalid(e.speed, rowId)}
                onChange={(ev) => patchStep(i, { speed: ev.target.value })}
                onBlur={() => patchStep(i, { speed: snapSpeedText(s.speed) })}
              />
            </StepCell>
            <StepCell label="Incline %">
              <input
                className={input}
                inputMode="decimal"
                aria-label={`Step ${i + 1} incline in %`}
                value={s.incline}
                {...invalid(e.incline, rowId)}
                onChange={(ev) => patchStep(i, { incline: ev.target.value })}
                onBlur={() => patchStep(i, { incline: snapInclineText(s.incline) })}
              />
            </StepCell>
            <GhostButton
              aria-label={`Heart zone on step ${i + 1}`}
              aria-pressed={s.zone}
              title={s.zone ? `Heart zone on for step ${i + 1}` : `Hold a heart zone on step ${i + 1}`}
              className={cx(s.zone && "text-neon-cyan!")}
              onClick={() => patchStep(i, { zone: !s.zone })}
            >
              <HeartPulse size={14} />
            </GhostButton>
            <GhostButton
              aria-label={`Remove step ${i + 1}`}
              title={d.steps.length === 1 ? "A program needs one step" : `Remove step ${i + 1}`}
              disabled={d.steps.length === 1}
              onClick={() => setD({ ...d, steps: d.steps.filter((_, j) => j !== i) })}
            >
              <X size={14} />
            </GhostButton>
          </div>
          <FieldErrors id={rowId} lines={[e.minutes, e.speed, e.incline, rust]} />
          {s.kind === "cool_down" && (
            <PulseRow
              step={s}
              n={i + 1}
              unit={d.zoneUnit}
              maxBpm={maxBpm}
              error={e.pulse}
              errorId={`${rowId}-pulse`}
              onChange={(patch) => patchStep(i, patch)}
            />
          )}
          <StrideFitFlag text={strideFitFlag(s.speed, strideFit)} />
          {s.zone && (
            <ZoneRow
              step={s}
              n={i + 1}
              unit={d.zoneUnit}
              maxBpm={maxBpm}
              inclineTrim={inclineTrim}
              error={e.zone}
              errorId={`${rowId}-zone`}
              onChange={(patch) => patchStep(i, patch)}
            />
          )}
          </div>
          );
        })}
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

      <Hint>A step&apos;s kind names the step on the HUD, the Live page and the tray menu.</Hint>

      {(anyZone || anyPulse) && (
        <div className="flex flex-col gap-1.5">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <Label>{anyZone ? "Heart zones in" : "Pulse target in"}</Label>
            <ChoiceGroup
              label="Heart zone unit"
              options={ZONE_UNITS}
              value={d.zoneUnit}
              onChange={(unit) => setD(withZoneUnit(d, unit, maxBpm))}
            />
          </div>
          {anyZone && (
            <Hint>
              A heart zone trims a moving step&apos;s speed by up to 1 km/h and never starts the belt. Each zone step
              waits 2 minutes for the pulse to settle before trimming. Zones are a share of the {maxBpm} bpm max heart
              rate in System, LINKS, Heart rate.
            </Hint>
          )}
          {anyPulse && (
            <>
              <Hint>A pulse target keeps a cool-down step walking past its minutes until heart rate holds at the target for 30 s.</Hint>
              <Hint>With no heart rate reading, the cool-down step ends on its minutes.</Hint>
            </>
          )}
          {inclineTrim && <Hint>Incline trims move at most one deck level a minute, up to 2 % from the step&apos;s incline.</Hint>}
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
        <div className="flex flex-wrap items-center gap-2 font-mono text-xs text-white/70">
          <label className="flex items-center gap-2 whitespace-nowrap">
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
            {...invalid(errors.repeat, `${ids}-repeat`)}
            onChange={(e) => setD({ ...d, repeatMinutes: e.target.value })}
          />
          <span className={cx(!d.repeat && "opacity-40")}>minutes</span>
        </div>
        <Hint className={cx(!d.repeat && "opacity-40")}>Between cycles the belt goes back to the speed from before the program.</Hint>
        <FieldErrors id={`${ids}-repeat`} lines={[errors.repeat]} indent={false} />
      </div>

      <Hint>
        When a program ends, the belt returns to the speed from before the program. A belt that was paused stays
        paused. A program never starts a still belt, so the walk after a rest or a 0 km/h step waits for GO.
      </Hint>

      {/* Stays at the bottom of the window while the steps scroll, so Save never needs a scroll. */}
      <div className="sticky bottom-0 z-10 -mx-4 -mb-4 flex flex-col gap-2 border-t border-white/10 bg-[#0a0a0a] px-4 py-3">
        <ActionStatus error={error ?? (places ? `Fix ${places} to save.` : save.error)} />
        <div className="flex justify-end gap-2">
          <GhostButton onClick={leave}>Cancel</GhostButton>
          <NeonButton className="h-9 px-5" disabled={save.pending} onClick={() => void submit()}>
            Save
          </NeonButton>
        </div>
      </div>
    </Panel>
  );
}

/** A COOL_DOWN step's pulse target: off, or the heart rate the step walks on until, with the other unit beside it. */
function PulseRow({
  step,
  n,
  unit,
  maxBpm,
  error,
  errorId,
  onChange,
}: {
  step: StepDraft;
  n: number;
  unit: ZoneUnit;
  maxBpm: number;
  error: string | undefined;
  errorId: string;
  onChange: (patch: Partial<StepDraft>) => void;
}) {
  const summary = pulseSummary(step, unit, maxBpm);
  const other = summary && (unit === "pct" ? `${summary.bpm} bpm` : `${summary.pct} % of max`);
  return (
    <>
    <div className="grid grid-cols-[2rem_1fr] items-center gap-2">
      <span />
      <div className="flex flex-wrap items-center gap-2 font-mono text-xs text-white/70">
        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={step.pulse}
            aria-label={`Step ${n} ends on heart rate`}
            onChange={(e) => onChange({ pulse: e.target.checked })}
          />
          <span className="font-bold uppercase tracking-[0.12em] text-neon-yellow">Pulse target</span>
        </label>
        <span className={cx(!step.pulse && "opacity-40")}>down to</span>
        <input
          className={cx(field, "w-16")}
          inputMode="decimal"
          aria-label={`Step ${n} pulse target`}
          disabled={!step.pulse}
          value={step.pulseTarget}
          {...invalid(error, errorId)}
          onChange={(e) => onChange({ pulseTarget: e.target.value })}
        />
        <span className={cx(!step.pulse && "opacity-40")}>{unit === "pct" ? "% of max" : "bpm"}</span>
        {step.pulse && other && <span className="text-white/60">{other}</span>}
      </div>
    </div>
    <FieldErrors id={errorId} lines={[error]} />
    </>
  );
}

/** STRIDE_FIT's one-line flag under a step, lined up with the step's fields. */
function StrideFitFlag({ text }: { text: string | null }) {
  if (text === null) return null;
  return (
    <div className="grid grid-cols-[2rem_1fr] items-center gap-2">
      <span />
      <span className="font-mono text-[11px] text-neon-yellow">{text}</span>
    </div>
  );
}

/** The low and high bounds of a step's heart zone, with the other unit beside them. */
function ZoneRow({
  step,
  n,
  unit,
  maxBpm,
  inclineTrim,
  error,
  errorId,
  onChange,
}: {
  step: StepDraft;
  n: number;
  unit: ZoneUnit;
  maxBpm: number;
  /** ZONE_TILT is on: offer what the zone trims. Off, it trims speed whatever was saved. */
  inclineTrim: boolean;
  error: string | undefined;
  errorId: string;
  onChange: (patch: Partial<StepDraft>) => void;
}) {
  const summary = zoneSummary(step, unit, maxBpm);
  const other =
    summary && (unit === "pct" ? `${summary.bpm[0]} to ${summary.bpm[1]} bpm` : `${summary.pct[0]} to ${summary.pct[1]} % of max`);
  const unitText = unit === "pct" ? "% of max" : "bpm";
  return (
    <>
    <div className="grid grid-cols-[2rem_1fr] items-center gap-2">
      <span />
      <div className="flex flex-wrap items-center gap-2 font-mono text-xs text-white/70">
        <span className="font-bold uppercase tracking-[0.12em] text-neon-yellow">Heart zone</span>
        <input
          className={cx(field, "w-16")}
          inputMode="decimal"
          aria-label={`Step ${n} heart zone low`}
          value={step.zoneLow}
          {...invalid(error, errorId)}
          onChange={(e) => onChange({ zoneLow: e.target.value })}
        />
        <span>to</span>
        <input
          className={cx(field, "w-16")}
          inputMode="decimal"
          aria-label={`Step ${n} heart zone high`}
          value={step.zoneHigh}
          {...invalid(error, errorId)}
          onChange={(e) => onChange({ zoneHigh: e.target.value })}
        />
        <span>{unitText}</span>
        {other && <span className="text-white/60">{other}</span>}
        {inclineTrim && (
          <span className="flex items-center gap-2">
            <span>trims</span>
            <ChoiceGroup
              label={`Step ${n} heart zone trims`}
              options={ZONE_TRIMS}
              value={step.zoneTrim}
              onChange={(zoneTrim) => onChange({ zoneTrim })}
            />
          </span>
        )}
      </div>
    </div>
    <FieldErrors id={errorId} lines={[error]} />
    </>
  );
}
