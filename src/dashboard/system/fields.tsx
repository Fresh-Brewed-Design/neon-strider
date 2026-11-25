// Form controls for the System page sections.

import { useEffect, useRef, useState, type ReactNode } from "react";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { Hint, Label, OptionPicker, Panel } from "../ui";
import { cx, type Tone } from "../util";

const ACCENT: Record<Tone, string> = {
  cyan: "accent-neon-cyan",
  magenta: "accent-neon-magenta",
  yellow: "accent-neon-yellow",
  green: "accent-emerald-400",
};

const SWITCH_ON: Record<Tone, string> = {
  cyan: "bg-neon-cyan",
  magenta: "bg-neon-magenta",
  yellow: "bg-neon-yellow",
  green: "bg-emerald-400",
};

export const INPUT_CLASS =
  "rounded border border-white/15 bg-black/40 px-2.5 py-1.5 font-mono text-xs text-white outline-none focus:border-neon-cyan";

/** A titled block on the System page. `id` is where links from elsewhere scroll to (`systemTabs.ts`). */
export function Section({
  id,
  title,
  description,
  status,
  children,
}: {
  id: string;
  title: string;
  description?: string;
  /** A status pill shown at the end of the title row. */
  status?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section id={id} aria-labelledby={`${id}-title`} className="scroll-mt-3">
      <Panel className="p-4">
        <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
          <h2 id={`${id}-title`} className="font-digital text-sm font-bold uppercase tracking-[0.18em] text-white">
            {title}
          </h2>
          {status}
        </div>
        {description && <p className="mt-1 font-mono text-xs leading-relaxed text-white/60">{description}</p>}
        <div className="mt-4 flex flex-col gap-4">{children}</div>
      </Panel>
    </section>
  );
}

export function Toggle({
  label,
  description,
  hint,
  checked,
  onChange,
  tone = "cyan",
}: {
  label: string;
  description?: string;
  /** A second line for caveats and detail, under the description. */
  hint?: string;
  checked: boolean;
  onChange: (next: boolean) => void;
  tone?: Tone;
}) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="min-w-0">
        <div className="font-mono text-xs font-bold uppercase tracking-[0.12em] text-white/85">{label}</div>
        {description && <Hint>{description}</Hint>}
        {hint && <Hint className="mt-1">{hint}</Hint>}
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        onClick={() => onChange(!checked)}
        className={cx(
          "relative h-5 w-10 shrink-0 rounded-full transition-colors",
          checked ? SWITCH_ON[tone] : "bg-white/15",
        )}
      >
        <span
          className={cx(
            "absolute top-0.5 h-4 w-4 rounded-full transition-all",
            checked ? "left-[22px] bg-black" : "left-0.5 bg-white/70",
          )}
        />
      </button>
    </div>
  );
}

const CHOICE_ON: Record<"magenta" | "green", string> = {
  magenta: "border-neon-magenta bg-neon-magenta/15 text-neon-magenta",
  green: "border-emerald-400 bg-emerald-400/15 text-emerald-400",
};

const CHOICE_OFF: Record<"magenta" | "green", string> = {
  magenta: "border-white/15 bg-black/40 text-white/60 hover:border-neon-magenta/50",
  green: "border-white/15 bg-black/40 text-white/60 hover:border-emerald-400/50",
};

/** A row of radio buttons for picking one of a few named options. */
export function ChoiceGroup<T extends string>({
  label,
  options,
  value,
  onChange,
  tone,
}: {
  /** The group's accessible name. */
  label: string;
  options: readonly { value: T; label: string }[];
  value: T;
  onChange: (next: T) => void;
  tone: "magenta" | "green";
}) {
  return (
    <OptionPicker
      label={label}
      options={options}
      value={value}
      onChange={onChange}
      className="flex flex-wrap gap-2"
      optionClassName={(active) =>
        cx(
          "rounded border px-3 py-1.5 font-mono text-[11px] font-bold uppercase tracking-[0.12em] transition-colors",
          active ? CHOICE_ON[tone] : CHOICE_OFF[tone],
        )
      }
    />
  );
}

/**
 * Slider that applies shortly after the user stops moving it, so dragging does
 * not push a config update to Rust for every step.
 */
export function SliderField({
  label,
  hint,
  value,
  min,
  max,
  step,
  format,
  tone = "cyan",
  onCommit,
}: {
  label: string;
  hint?: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format: (v: number) => string;
  tone?: Tone;
  onCommit: (v: number) => void;
}) {
  const [draft, setDraft] = useState<number | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const pending = useRef<number | null>(null);
  const commit = useRef(onCommit);
  useEffect(() => {
    commit.current = onCommit;
  });

  const flush = () => {
    clearTimeout(timer.current);
    if (pending.current !== null) {
      const v = pending.current;
      pending.current = null;
      commit.current(v);
    }
    setDraft(null);
  };

  useEffect(
    () => () => {
      clearTimeout(timer.current);
      if (pending.current !== null) commit.current(pending.current);
    },
    [],
  );

  const shown = draft ?? value;
  return (
    <div>
      <div className="mb-1 flex items-baseline justify-between gap-3">
        <Label>{label}</Label>
        <span className="font-digital text-sm text-white">{format(shown)}</span>
      </div>
      <input
        type="range"
        aria-label={label}
        min={min}
        max={max}
        step={step}
        value={shown}
        onChange={(e) => {
          const v = Number(e.target.value);
          setDraft(v);
          pending.current = v;
          clearTimeout(timer.current);
          timer.current = setTimeout(flush, 250); // one snapshot tick: a drag sends a few values, not one per pixel
        }}
        onPointerUp={flush}
        onKeyUp={flush}
        className={cx("h-2 w-full cursor-pointer", ACCENT[tone])}
      />
      {hint && <Hint className="mt-1">{hint}</Hint>}
    </div>
  );
}

/** Number input that applies on blur or Enter, clamped to its range. Bad text reverts. */
export function NumberField({
  label,
  hint,
  value,
  min,
  max,
  step = 1,
  unit,
  snap,
  onCommit,
}: {
  label: string;
  hint?: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  unit?: string;
  /** Moves a committed value to an allowed one (a speed the deck runs); the field then shows it. */
  snap?: (v: number) => number;
  /** May return the command's promise: a rejection shows under the field. */
  onCommit: (v: number) => Promise<unknown> | void;
}) {
  const [text, setText] = useState<string | null>(null);
  const action = useAction();
  const unitSuffix = unit ? ` ${unit}` : "";

  const apply = () => {
    if (text === null) return;
    const n = Number(text);
    setText(null);
    if (text.trim() === "" || !Number.isFinite(n)) return;
    const clamped = Math.min(max, Math.max(min, n));
    const next = snap ? Math.min(max, Math.max(min, snap(clamped))) : clamped;
    if (next === value && clamped === n) return;
    const held = n > max ? `Kept to ${max}${unitSuffix}, the most allowed.` : n < min ? `Kept to ${min}${unitSuffix}, the least allowed.` : null;
    if (next === value) {
      // Held to a limit and already there: nothing to save, but the field shows the held value.
      action.reset();
      void action.run(async () => undefined, { done: held ?? "Saved." });
      return;
    }
    void action.run(async () => onCommit(next), { done: held ?? "Saved." });
  };

  return (
    <label className="block">
      <Label>{label}</Label>
      <div className="mt-1 flex items-center gap-2">
        <input
          type="number"
          min={min}
          max={max}
          step={step}
          value={text ?? String(value)}
          onChange={(e) => setText(e.target.value)}
          onBlur={apply}
          onKeyDown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
            if (e.key === "Escape") setText(null);
          }}
          className={cx(INPUT_CLASS, "w-24")}
        />
        {unit && <span className="font-mono text-xs text-white/60">{unit}</span>}
      </div>
      <ActionStatus pending={action.pending} error={action.error} done={action.done} className="mt-1" />
      {hint && <Hint className="mt-1">{hint}</Hint>}
    </label>
  );
}
