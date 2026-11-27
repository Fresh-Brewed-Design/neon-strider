import { useRef } from "react";
import { Sparkles } from "lucide-react";
import { ActionStatus } from "../../../components/ActionStatus";
import { useAction } from "../../../components/useAction";
import { dispatch } from "../../../ipc";
import { useAppState } from "../../../state/useAppState";
import { DialogConfirmButton, DialogFrame } from "../../DialogFrame";
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
    <DialogFrame
      boxRef={boxRef}
      titleId="whats-new-title"
      icon={Sparkles}
      title={`UPGRADE_COMPLETE // v${whatsNew.version}`}
      subline={`What's new in v${whatsNew.version}`}
      z="z-[90]"
      size="lg"
      onClose={close}
      footer={
        <>
          <DialogConfirmButton ref={okRef} disabled={ack.pending} onClick={close}>
            OK
          </DialogConfirmButton>
          <ActionStatus error={ack.error} />
        </>
      }
    >
      <Markdown text={whatsNew.notes} />
    </DialogFrame>
  );
}
