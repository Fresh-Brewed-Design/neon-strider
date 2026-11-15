import { useEffect, useRef, type ReactNode } from "react";
import { AlertCircle, CheckCircle2, Loader2 } from "lucide-react";

/**
 * One status line for a control, shown right under it: working, failed or done.
 * Long text wraps inside the section; `children` carries follow-up actions
 * ("Retry now", "Open folder"). Renders nothing when there is nothing to say.
 * When a new error or result appears after the control was used, it scrolls
 * into view, so a failure is never left below the fold. A line that was
 * already there when the page opened, or an `ambient` one (background status
 * that changes without the user doing anything, such as a retry countdown),
 * never scrolls the page.
 */
export function ActionStatus({
  pending,
  error,
  done,
  children,
  className = "",
  ambient = false,
}: {
  /** Text while working; `true` shows the spinner alone. */
  pending?: string | boolean | null;
  error?: string | null;
  done?: string | null;
  children?: ReactNode;
  className?: string;
  /** Background status, not the result of something the user just did. */
  ambient?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const seen = error || done || "";
  const mounted = useRef(false);
  useEffect(() => {
    const first = !mounted.current;
    mounted.current = true;
    if (first || ambient || !seen) return;
    ref.current?.scrollIntoView?.({ block: "nearest", behavior: "smooth" });
  }, [seen, ambient]);

  if (!pending && !error && !done && !children) return null;

  return (
    <div
      ref={ref}
      role={error ? "alert" : "status"}
      className={`flex min-w-0 flex-wrap items-start gap-x-3 gap-y-1 font-mono text-[11px] leading-relaxed ${className}`}
    >
      {pending ? (
        <span className="flex min-w-0 items-start gap-1.5 text-neon-yellow">
          <Loader2 size={12} className="mt-0.5 shrink-0 animate-spin" />
          {typeof pending === "string" && <span className="min-w-0 break-words">{pending}</span>}
        </span>
      ) : error ? (
        <span className="flex min-w-0 items-start gap-1.5 text-red-400">
          <AlertCircle size={12} className="mt-0.5 shrink-0" />
          <span className="min-w-0 break-words">{error}</span>
        </span>
      ) : done ? (
        <span className="flex min-w-0 items-start gap-1.5 text-emerald-400">
          <CheckCircle2 size={12} className="mt-0.5 shrink-0" />
          <span className="min-w-0 break-words">{done}</span>
        </span>
      ) : null}
      {children}
    </div>
  );
}

/** A link-styled follow-up action inside an `<ActionStatus>` line ("Retry now", "Open folder"). */
export function InlineAction({
  onClick,
  disabled,
  children,
}: {
  onClick: () => void;
  disabled?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="font-bold uppercase tracking-[0.12em] text-neon-cyan underline-offset-2 hover:underline disabled:opacity-40"
    >
      {children}
    </button>
  );
}
