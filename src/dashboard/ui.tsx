// The dashboard's shared building blocks: page frame, panel, label, buttons and the option picker.

import { Fragment, useRef, type ButtonHTMLAttributes, type HTMLAttributes, type KeyboardEvent, type ReactNode, type Ref } from "react";
import { cx, type Tone } from "./util";

const TONE_BUTTON: Record<Tone, string> = {
  cyan: "border-neon-cyan/60 text-neon-cyan bg-neon-cyan/5 hover:bg-neon-cyan/15",
  magenta: "border-neon-magenta/60 text-neon-magenta bg-neon-magenta/5 hover:bg-neon-magenta/15",
  yellow: "border-neon-yellow/60 text-neon-yellow bg-neon-yellow/5 hover:bg-neon-yellow/15",
  green: "border-emerald-400/60 text-emerald-400 bg-emerald-400/5 hover:bg-emerald-400/15",
};

/** Page frame: a small themed codename over a plain title, then content. */
export function Page({
  codename,
  title,
  actions,
  fitHeader = false,
  children,
}: {
  codename: string;
  title: string;
  actions?: ReactNode;
  /** In a short window (index.css, `.dash-page-fit`) the codename and title are hidden but still read out; the actions stay. */
  fitHeader?: boolean;
  children: ReactNode;
}) {
  return (
    <section className="dash-page flex h-full min-h-0 flex-col">
      {/* The actions wrap under the title when both don't fit (a large Windows text size). */}
      <header
        className={cx("flex shrink-0 flex-wrap items-end justify-between gap-3 px-5 pb-2 pt-3", fitHeader && "dash-page-fit")}
        data-tauri-drag-region
      >
        <div className="dash-page-heading" data-tauri-drag-region>
          <div className="font-mono text-xs tracking-[0.25em] text-white/60" data-tauri-drag-region>
            {codename}
          </div>
          <h1 className="font-digital text-lg font-bold tracking-[0.12em] text-white" data-tauri-drag-region>
            {title}
          </h1>
        </div>
        {actions && <div className="flex min-w-0 flex-wrap items-center gap-2">{actions}</div>}
      </header>
      <div className="dash-page-body min-h-0 flex-1 overflow-y-auto px-5 pb-5">{children}</div>
    </section>
  );
}

export function Panel({ className, children }: { className?: string; children: ReactNode }) {
  return <div className={cx("hacker-panel rounded-lg border-white/10", className)}>{children}</div>;
}

export function Label({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <span className={cx("font-mono text-xs font-bold uppercase tracking-[0.2em] text-white/60", className)}>
      {children}
    </span>
  );
}

/** A codename that may break after its underscores when its row is narrow (a large Windows text size). */
export function Codename({ text }: { text: string }) {
  const parts = text.split("_");
  return parts.map((part, i) => (
    <Fragment key={i}>
      {part}
      {i < parts.length - 1 && (
        <>
          _<wbr />
        </>
      )}
    </Fragment>
  ));
}

/** Splits a line at its separators: " · " or " // ", with a plain or no-break space either side. */
const SEPARATOR = { "·": /\s·\s/, "//": /\s\/\/\s/ } as const;

/**
 * A monospace line of parts joined by " · " (or " // "), such as a program's step line. Each part
 * stays whole while it fits a line, and where the line wraps the separator that would start the
 * new line is clipped, so no line starts or ends with a lone separator. The separator is three (or
 * four) characters wide, so this is for monospace text with no letter spacing.
 */
export function Joined({ text, sep = "·", className }: { text: string; sep?: keyof typeof SEPARATOR; className?: string }) {
  const parts = text.split(SEPARATOR[sep]);
  const width = `${sep.length + 2}ch`;
  return (
    <span data-joined className={cx("block overflow-hidden", className)}>
      {/* Every part opens with its separator (the first with as much space), and the row is pulled
          left by that width, so the one at the start of each line falls outside the box. */}
      <span className="flex flex-wrap" style={{ marginLeft: `-${width}` }}>
        {parts.map((part, i) => (
          <span key={i} className="min-w-0" style={i === 0 ? { paddingLeft: width } : undefined}>
            {i > 0 && <span className="whitespace-pre">{` ${sep} `}</span>}
            {part}
          </span>
        ))}
      </span>
    </span>
  );
}

/** A codename's plain meaning: the small grey line under it (`PLAIN` in src/names.ts). */
export function PlainLine({ text, className }: { text: string; className?: string }) {
  return <span className={cx("block font-mono text-[11px] font-normal normal-case leading-tight tracking-normal text-white/60", className)}>{text}</span>;
}

/** Small grey explanation under or beside a control. */
export function Hint({ className, ...rest }: HTMLAttributes<HTMLParagraphElement>) {
  return <p className={cx("font-mono text-xs leading-relaxed text-white/60", className)} {...rest} />;
}

export function NeonButton({
  tone = "cyan",
  className,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { tone?: Tone; ref?: Ref<HTMLButtonElement> }) {
  return (
    <button
      type="button"
      className={cx(
        "rounded border font-digital text-xs font-bold uppercase tracking-[0.18em] transition-colors",
        "disabled:cursor-default disabled:opacity-30 disabled:hover:bg-transparent",
        TONE_BUTTON[tone],
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
}

/** Small borderless control for secondary actions. */
export function GhostButton({ className, children, ...rest }: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      className={cx(
        "rounded px-2 py-1 font-mono text-xs font-bold uppercase tracking-[0.15em] text-white/60",
        "transition-colors hover:bg-white/5 hover:text-white disabled:opacity-30",
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
}

/** Shown on pages that need live data before the first snapshot arrives. */
export function Waiting() {
  return <div className="p-6 font-mono text-xs uppercase tracking-[0.2em] text-white/60">Waiting for core link…</div>;
}

export type PickerOption<T extends string> = { value: T; label: ReactNode };

/**
 * Pick one of a few options: a WAI-ARIA radio group. Tab lands on the chosen
 * option (or the first when none is), the arrow keys move to the next or
 * previous one and choose it, and Home and End jump to the ends. Each caller
 * keeps its own look through `className` and `optionClassName`.
 *
 * With `activation="manual"` the keys only move focus, and Space, Enter or a
 * click chooses: for options whose choice does something visible at once,
 * such as moving the HUD to another display.
 *
 * `disabled` marks the options aria-disabled and ignores presses instead of
 * disabling the buttons, so focus stays put while a choice is being saved.
 */
export function OptionPicker<T extends string>({
  label,
  options,
  value,
  onChange,
  disabled = false,
  activation = "auto",
  className,
  optionClassName,
}: {
  /** The group's accessible name. */
  label: string;
  options: readonly PickerOption<T>[];
  /** The chosen option, or null when none is. */
  value: T | null;
  onChange: (next: T) => void;
  disabled?: boolean;
  /** "auto": moving with the keys chooses. "manual": only Space, Enter or a click does. */
  activation?: "auto" | "manual";
  className?: string;
  optionClassName: (active: boolean) => string;
}) {
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  const chosen = options.findIndex((o) => o.value === value);
  const tabStop = chosen >= 0 ? chosen : 0;

  // A click on the chosen option still reports it (the HUD display picker
  // uses that to forget a display that is gone); the keys only report a move.
  const pick = (i: number) => {
    if (!disabled) onChange(options[i].value);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>, i: number) => {
    const last = options.length - 1;
    let next: number;
    switch (e.key) {
      case "ArrowDown":
      case "ArrowRight":
        next = i === last ? 0 : i + 1;
        break;
      case "ArrowUp":
      case "ArrowLeft":
        next = i === 0 ? last : i - 1;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = last;
        break;
      case " ":
      case "Enter":
        if (activation === "auto") return;
        e.preventDefault();
        pick(i);
        return;
      default:
        return;
    }
    e.preventDefault();
    buttons.current[next]?.focus();
    if (activation === "auto" && next !== i) pick(next);
  };

  return (
    <div role="radiogroup" aria-label={label} aria-disabled={disabled || undefined} className={className}>
      {options.map((o, i) => {
        const active = i === chosen;
        return (
          <button
            key={o.value}
            ref={(el) => {
              buttons.current[i] = el;
            }}
            type="button"
            role="radio"
            aria-checked={active}
            aria-disabled={disabled || undefined}
            tabIndex={i === tabStop ? 0 : -1}
            onClick={() => pick(i)}
            onKeyDown={(e) => onKeyDown(e, i)}
            className={optionClassName(active)}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}
