// The dashboard's shared building blocks: page frame, panel, label and buttons.

import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode } from "react";
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
  children,
}: {
  codename: string;
  title: string;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="flex h-full min-h-0 flex-col">
      <header className="flex shrink-0 items-end justify-between gap-3 px-5 pb-2 pt-3" data-tauri-drag-region>
        <div data-tauri-drag-region>
          <div className="font-mono text-xs tracking-[0.25em] text-white/60" data-tauri-drag-region>
            {codename}
          </div>
          <h1 className="font-digital text-lg font-bold tracking-[0.12em] text-white" data-tauri-drag-region>
            {title}
          </h1>
        </div>
        {actions && <div className="flex items-center gap-2">{actions}</div>}
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-5">{children}</div>
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

/** Small grey explanation under or beside a control. */
export function Hint({ className, ...rest }: HTMLAttributes<HTMLParagraphElement>) {
  return <p className={cx("font-mono text-xs leading-relaxed text-white/60", className)} {...rest} />;
}

export function NeonButton({
  tone = "cyan",
  className,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { tone?: Tone }) {
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
