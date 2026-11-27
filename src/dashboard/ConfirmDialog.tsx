import { useRef } from "react";
import { AlertCircle, AlertTriangle, Check } from "lucide-react";
import { useTreadmillStore } from "../store/treadmillStore";
import { DialogButtons, DialogCancelButton, DialogConfirmButton, DialogFrame } from "./DialogFrame";
import { useDialog } from "./useDialog";

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

  return (
    <DialogFrame
      boxRef={boxRef}
      role="alertdialog"
      titleId="confirm-title"
      describedBy="confirm-message"
      bodyId="confirm-message"
      icon={danger ? AlertTriangle : AlertCircle}
      title={pending.title}
      tone={danger ? "magenta" : "cyan"}
      closeLabel="Cancel"
      onClose={cancel}
      footer={
        <DialogButtons>
          <DialogCancelButton ref={cancelRef} onClick={cancel}>
            Cancel
          </DialogCancelButton>
          <DialogConfirmButton danger={danger} onClick={confirm}>
            {!danger && <Check size={14} />}
            {pending.confirmLabel ?? "Confirm"}
          </DialogConfirmButton>
        </DialogButtons>
      }
    >
      <p className="whitespace-pre-line font-mono text-xs leading-relaxed text-white/70">{pending.message}</p>
      {pending.hint && <p className="mt-3 whitespace-pre-line font-mono text-xs leading-relaxed text-white/60">{pending.hint}</p>}
    </DialogFrame>
  );
}
