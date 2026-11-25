import { useRef } from "react";
import { motion } from "framer-motion";
import { AlertCircle, AlertTriangle, Check, X } from "lucide-react";
import { useTreadmillStore } from "../store/treadmillStore";
import { useDialog } from "./useDialog";
import { cx } from "./util";

/**
 * The app's one confirm dialog, opened through the store's `confirm`. Cancel
 * has focus when it opens and Escape cancels; a `tone: "danger"` confirm (a
 * delete, a disconnect) is magenta with its own verb on the button.
 */
export function ConfirmDialog() {
  const pending = useTreadmillStore((s) => s.pendingConfirmation);
  const clearConfirmation = useTreadmillStore((s) => s.clearConfirmation);
  const cancelRef = useRef<HTMLButtonElement>(null);

  const cancel = () => {
    pending?.onCancel?.();
    clearConfirmation();
  };
  const boxRef = useDialog(pending !== null, cancel, cancelRef);
  if (!pending) return null;

  const confirm = () => {
    pending.onConfirm();
    clearConfirmation();
  };
  const danger = pending.tone === "danger";
  const Icon = danger ? AlertTriangle : AlertCircle;

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm">
      <motion.div
        ref={boxRef}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
        aria-describedby="confirm-message"
        tabIndex={-1}
        initial={{ opacity: 0, scale: 0.9, y: 20 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        className={cx(
          "w-full max-w-md bg-zinc-900/90 border-2 rounded-2xl overflow-hidden outline-none",
          danger
            ? "border-neon-magenta/40 shadow-[0_0_50px_rgba(255,0,255,0.12)]"
            : "border-neon-cyan/30 shadow-[0_0_50px_rgba(0,243,255,0.1)]",
        )}
      >
        <div
          className={cx(
            "p-4 flex items-center gap-3 border-b",
            danger ? "bg-neon-magenta/5 border-neon-magenta/25" : "bg-neon-cyan/5 border-neon-cyan/20",
          )}
        >
          <Icon className={danger ? "text-neon-magenta" : "text-neon-cyan"} size={20} />
          <h2
            id="confirm-title"
            className={cx(
              "text-sm font-black tracking-[0.2em] uppercase flex-1",
              danger ? "text-neon-magenta" : "text-neon-cyan",
            )}
          >
            {pending.title}
          </h2>
          <button
            type="button"
            onClick={cancel}
            aria-label="Cancel"
            title="Cancel"
            className="text-white/60 hover:text-white transition-colors"
          >
            <X size={18} />
          </button>
        </div>

        <div id="confirm-message" className="p-6">
          <p className="whitespace-pre-line text-xs text-white/70 font-mono leading-relaxed tracking-tight">{pending.message}</p>
          {pending.hint && <p className="mt-3 whitespace-pre-line text-xs text-white/60 font-mono leading-relaxed tracking-tight">{pending.hint}</p>}
        </div>

        <div className="p-4 bg-black/40 flex gap-3 justify-end">
          <button
            ref={cancelRef}
            type="button"
            onClick={cancel}
            className="px-6 py-2 rounded-lg text-xs font-black uppercase tracking-widest text-white/60 hover:text-white hover:bg-white/5 focus-visible:text-white focus-visible:bg-white/5 transition-all"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={confirm}
            className={cx(
              "border px-8 py-2 rounded-lg text-[11px] font-black uppercase tracking-widest flex items-center gap-2 transition-all",
              danger
                ? "bg-neon-magenta/20 border-neon-magenta/60 hover:bg-neon-magenta/30 text-neon-magenta shadow-[0_0_20px_rgba(255,0,255,0.12)]"
                : "bg-neon-cyan/20 border-neon-cyan/50 hover:bg-neon-cyan/30 text-neon-cyan shadow-[0_0_20px_rgba(0,243,255,0.1)]",
            )}
          >
            {!danger && <Check size={14} />}
            {pending.confirmLabel ?? "Confirm"}
          </button>
        </div>
      </motion.div>
    </div>
  );
}
