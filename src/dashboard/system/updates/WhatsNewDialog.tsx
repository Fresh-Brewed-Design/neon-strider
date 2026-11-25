import { useRef } from "react";
import { Sparkles } from "lucide-react";
import { ActionStatus } from "../../../components/ActionStatus";
import { useAction } from "../../../components/useAction";
import { dispatch } from "../../../ipc";
import { useAppState } from "../../../state/useAppState";
import { NeonButton } from "../../ui";
import { useDialog } from "../../useDialog";
import { Markdown } from "./Markdown";

/**
 * "What's new" after an update: the notes saved before installing, shown once
 * on the first launch of the new version until OK. OK has focus when it
 * opens, and Escape is OK too.
 */
export function WhatsNewDialog() {
  const whatsNew = useAppState((s) => s.update.whats_new);
  const ack = useAction();
  const okRef = useRef<HTMLButtonElement>(null);
  const close = () => {
    if (!ack.pending) void ack.run(() => dispatch({ type: "update_whats_new_ack" }));
  };
  const boxRef = useDialog(Boolean(whatsNew), close, okRef);
  if (!whatsNew) return null;

  return (
    <div className="fixed inset-0 z-[90] flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm">
      <div
        ref={boxRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="whats-new-title"
        tabIndex={-1}
        className="flex outline-none max-h-full w-full max-w-lg flex-col overflow-hidden rounded-2xl border-2 border-neon-cyan/30 bg-zinc-900/95 shadow-[0_0_50px_rgba(0,243,255,0.1)]"
      >
        <div className="flex shrink-0 items-center gap-3 border-b border-neon-cyan/20 bg-neon-cyan/5 p-4">
          <Sparkles className="shrink-0 text-neon-cyan" size={18} />
          <div className="min-w-0">
            <div className="font-mono text-xs tracking-[0.25em] text-white/60">UPGRADE_COMPLETE // v{whatsNew.version}</div>
            <h2 id="whats-new-title" className="font-digital text-sm font-bold tracking-[0.12em] text-white">
              What&apos;s new in v{whatsNew.version}
            </h2>
          </div>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-5">
          <Markdown text={whatsNew.notes} />
        </div>
        <div className="flex shrink-0 flex-col items-end gap-1 bg-black/40 p-4">
          <NeonButton ref={okRef} className="px-6 py-2" disabled={ack.pending} onClick={close}>
            OK
          </NeonButton>
          <ActionStatus error={ack.error} />
        </div>
      </div>
    </div>
  );
}
