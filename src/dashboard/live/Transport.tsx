import { Loader2 } from "lucide-react";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction, useBackendAction } from "../../components/useAction";
import { dispatch, type AppSnapshot } from "../../ipc";
import { isMoving, linkState, linkView, startFailure } from "../../state/linkStatus";
import { StatusPill } from "../StatusPill";
import { NeonButton, Panel } from "../ui";
import { openLinkDialog } from "../util";

/** Engage / Stasis and Dump, or, while offline, the link state with Connect or Retry. */
export function Transport({ s }: { s: AppSnapshot }) {
  const { deck, connection } = s;
  const moving = isMoving(deck);
  const view = linkView(connection, s.platform.settings);
  const state = linkState(connection).state;
  const retry = useAction();
  const control = useAction();
  // Engage waits for the deck to answer (Rust gives up after 8 s and says why).
  const engage = useBackendAction(connection.start_result, connection.start_pending);
  const failed = startFailure(s);
  const waiting = engage.pending || (connection.start_pending && !moving);
  // On a paused program this button is its GO (Rust resumes the program).
  const programGo = !moving && s.active_program?.phase === "walking" && s.active_program.paused;
  const paused = deck.status === "Paused";

  if (!connection.connected) {
    const linking = state === "connecting" || state === "scanning";
    return (
      <Panel className="flex flex-col gap-1 px-4 py-1.5">
        {/* The headline keeps 8rem beside the pill; with less room (a large Windows text size) it
            and the button wrap onto lines of their own. */}
        <div className="flex min-h-7 flex-wrap items-center gap-x-3 gap-y-1">
          <StatusPill tone={view.tone} label={view.label} title={view.title} className="shrink-0" />
          <span className="min-w-0 flex-1 basis-32 break-words font-mono text-xs leading-snug text-white/60">
            {view.headline}
          </span>
          {view.canRetry && state === "reconnecting" ? (
            <NeonButton
              className="flex h-7 shrink-0 items-center gap-1.5 px-3"
              disabled={retry.pending}
              onClick={() => void retry.run(() => dispatch({ type: "connect_now" }), { done: "Trying the treadmill now…" })}
            >
              {retry.pending && <Loader2 size={12} className="animate-spin" />}
              {retry.pending ? "Retrying…" : "Retry now"}
            </NeonButton>
          ) : (
            !linking && (
              <NeonButton className="h-7 shrink-0 px-4" onClick={openLinkDialog}>
                Connect
              </NeonButton>
            )
          )}
        </div>
        <ActionStatus pending={retry.pending && "Asking the treadmill to link…"} error={retry.error} done={retry.done} />
      </Panel>
    );
  }
  return (
    <div className="flex flex-col gap-2">
      <div className="flex gap-3">
        <NeonButton
          tone={moving ? "magenta" : "cyan"}
          className="flex h-12 flex-1 flex-col items-center justify-center text-sm"
          disabled={control.pending || waiting}
          onClick={() => void engage.run(() => dispatch({ type: "toggle_start" }))}
        >
          <span className="flex items-center gap-2">
            {waiting && <Loader2 size={14} className="animate-spin" />}
            {moving ? "Stasis" : programGo ? "GO" : paused ? "Re-jack" : "Engage"}
          </span>{" "}
          <span className="font-mono text-[11px] font-normal normal-case leading-tight tracking-normal opacity-70">
            {moving ? "pause" : programGo || paused ? "resume" : "start"}
          </span>
        </NeonButton>
        <NeonButton
          tone="magenta"
          className="flex h-12 w-32 flex-col items-center justify-center"
          disabled={deck.status === "Stopped" || deck.status === "Stopping" || control.pending}
          onClick={() => void control.run(() => dispatch({ type: "stop" }))}
          title={s.easy_stop.easing ? "Stop the belt at once" : "Stop the belt and end the session"}
        >
          Dump{" "}
          <span className="font-mono text-[11px] font-normal normal-case leading-tight tracking-normal opacity-70">
            {s.easy_stop.easing ? "stop now" : "end walk"}
          </span>
        </NeonButton>
      </div>
      <ActionStatus
        pending={(waiting && !moving && "Waiting for the treadmill to start…") || s.easy_stop.message}
        error={engage.error ?? failed ?? control.error}
      />
    </div>
  );
}
