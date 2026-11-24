import { motion } from "framer-motion";
import { AlertCircle, Check, X } from "lucide-react";
import { useTreadmillStore } from "../store/treadmillStore";

/** The app's one confirm dialog, opened through the store's `confirm`. */
export function ConfirmDialog() {
  const pending = useTreadmillStore((s) => s.pendingConfirmation);
  const clearConfirmation = useTreadmillStore((s) => s.clearConfirmation);
  if (!pending) return null;

  const confirm = () => {
    pending.onConfirm();
    clearConfirmation();
  };

  const cancel = () => {
    pending.onCancel?.();
    clearConfirmation();
  };

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm">
      <motion.div
        initial={{ opacity: 0, scale: 0.9, y: 20 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        className="w-full max-w-md bg-zinc-900/90 border-2 border-neon-cyan/30 rounded-2xl overflow-hidden shadow-[0_0_50px_rgba(0,243,255,0.1)]"
      >
        <div className="bg-neon-cyan/5 border-b border-neon-cyan/20 p-4 flex items-center gap-3">
          <AlertCircle className="text-neon-cyan" size={20} />
          <h2 className="text-sm font-black tracking-[0.2em] text-neon-cyan uppercase flex-1">{pending.title}</h2>
          <button onClick={cancel} className="text-white/60 hover:text-white transition-colors">
            <X size={18} />
          </button>
        </div>

        <div className="p-6">
          <p className="whitespace-pre-line text-xs text-white/70 font-mono leading-relaxed tracking-tight">{pending.message}</p>
          {pending.hint && <p className="mt-3 whitespace-pre-line text-xs text-white/60 font-mono leading-relaxed tracking-tight">{pending.hint}</p>}
        </div>

        <div className="p-4 bg-black/40 flex gap-3 justify-end">
          <button
            onClick={cancel}
            className="px-6 py-2 rounded-lg text-xs font-black uppercase tracking-widest text-white/60 hover:text-white hover:bg-white/5 transition-all"
          >
            Cancel
          </button>
          <button
            onClick={confirm}
            className="bg-neon-cyan/20 border border-neon-cyan/50 hover:bg-neon-cyan/30 text-neon-cyan px-8 py-2 rounded-lg text-[11px] font-black uppercase tracking-widest flex items-center gap-2 transition-all shadow-[0_0_20px_rgba(0,243,255,0.1)]"
          >
            <Check size={14} />
            Confirm
          </button>
        </div>
      </motion.div>
    </div>
  );
}
