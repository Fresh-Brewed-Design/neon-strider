import type { ButtonHTMLAttributes, ReactNode, Ref } from "react";
import { motion } from "framer-motion";
import { X, type LucideIcon } from "lucide-react";
import { NeonButton, PlainLine } from "./ui";
import { cx } from "./util";

const WIDTH = { md: "max-w-md", lg: "max-w-lg", xl: "max-w-2xl" } as const;

/**
 * The one frame every dashboard dialog wears: dimmed backdrop, rounded box with
 * a cyan (or magenta, for a destructive one) border, a header with the
 * codename title, its plain sub-line and a close button, a body and an
 * optional footer for the buttons. Behaviour (focus, Tab, Escape) stays with
 * `useDialog`: pass its ref as `boxRef`. Wrap it in `AnimatePresence` for an
 * exit animation.
 */
export function DialogFrame({
  boxRef,
  role = "dialog",
  titleId,
  describedBy,
  icon: Icon,
  title,
  subline,
  tone = "cyan",
  size = "md",
  tall = false,
  z = "z-[100]",
  closeLabel = "Close",
  onClose,
  onBackdrop,
  headerActions,
  footer,
  bodyClassName = "p-5",
  bodyId,
  children,
}: {
  boxRef: Ref<HTMLDivElement>;
  role?: "dialog" | "alertdialog";
  /** Id for the title, which names the dialog. */
  titleId: string;
  describedBy?: string;
  icon: LucideIcon;
  title: ReactNode;
  /** The title's plain meaning, the small grey line under it. */
  subline?: string;
  tone?: "cyan" | "magenta";
  size?: keyof typeof WIDTH;
  /** Fill the window's height (a long preview). */
  tall?: boolean;
  /** Stacking class: a dialog that opens over another needs a higher one. */
  z?: string;
  closeLabel?: string;
  /** The header's X. */
  onClose: () => void;
  /** A click on the backdrop; leave out for a dialog the backdrop must not close. */
  onBackdrop?: () => void;
  /** Controls beside the X. */
  headerActions?: ReactNode;
  footer?: ReactNode;
  bodyClassName?: string;
  bodyId?: string;
  children: ReactNode;
}) {
  const magenta = tone === "magenta";
  return (
    <div className={cx("fixed inset-0 flex items-center justify-center p-4", z)}>
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        onClick={onBackdrop}
        className="absolute inset-0 bg-black/70 backdrop-blur-sm"
      />
      <motion.div
        ref={boxRef}
        role={role}
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={describedBy}
        tabIndex={-1}
        initial={{ opacity: 0, scale: 0.95, y: 16 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.95, y: 16 }}
        className={cx(
          "relative flex max-h-full w-full flex-col overflow-hidden rounded-2xl border bg-zinc-900/95 outline-none",
          WIDTH[size],
          tall && "h-full max-h-[34rem]",
          magenta
            ? "border-neon-magenta/40 shadow-[0_0_50px_rgba(255,0,255,0.12)]"
            : "border-neon-cyan/30 shadow-[0_0_50px_rgba(0,243,255,0.1)]",
        )}
      >
        <header
          className={cx(
            "flex shrink-0 items-center gap-3 border-b p-4",
            magenta ? "border-neon-magenta/25 bg-neon-magenta/5" : "border-neon-cyan/20 bg-neon-cyan/5",
          )}
        >
          <Icon className={cx("shrink-0", magenta ? "text-neon-magenta" : "text-neon-cyan")} size={18} />
          <div className="min-w-0 flex-1">
            <h2
              id={titleId}
              className={cx(
                "break-words font-digital text-sm font-bold uppercase tracking-[0.12em]",
                magenta ? "text-neon-magenta" : "text-neon-cyan",
              )}
            >
              {title}
            </h2>
            {subline && <PlainLine text={subline} />}
          </div>
          {headerActions}
          <button
            type="button"
            onClick={onClose}
            aria-label={closeLabel}
            title={closeLabel}
            className="rounded p-1 text-white/60 transition-colors hover:bg-white/10 hover:text-white"
          >
            <X size={18} />
          </button>
        </header>
        <div id={bodyId} className={cx("min-h-0 flex-1 overflow-y-auto", bodyClassName)}>
          {children}
        </div>
        {footer && (
          <footer className="flex shrink-0 flex-col items-end gap-1 border-t border-white/10 bg-black/40 p-4">{footer}</footer>
        )}
      </motion.div>
    </div>
  );
}

/** A dialog's button row: Cancel left of the confirm, both right-aligned. */
export function DialogButtons({ children }: { children: ReactNode }) {
  return <div className="flex items-center justify-end gap-3">{children}</div>;
}

/** The neutral button: Cancel, the way out that changes nothing. */
export function DialogCancelButton({
  className,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { ref?: Ref<HTMLButtonElement> }) {
  return (
    <button
      type="button"
      className={cx(
        "rounded border border-white/20 px-5 py-2 font-digital text-xs font-bold uppercase tracking-[0.18em] text-white/60",
        "transition-colors hover:bg-white/5 hover:text-white focus-visible:bg-white/5 focus-visible:text-white disabled:cursor-default disabled:opacity-30",
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
}

/** The confirm button: cyan, or magenta for a destructive one. */
export function DialogConfirmButton({
  danger = false,
  className,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { danger?: boolean; ref?: Ref<HTMLButtonElement> }) {
  return <NeonButton tone={danger ? "magenta" : "cyan"} className={cx("flex items-center gap-2 px-5 py-2", className)} {...rest} />;
}
