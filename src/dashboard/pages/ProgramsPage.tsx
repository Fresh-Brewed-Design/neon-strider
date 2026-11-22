import { useState } from "react";
import { Pencil, Play, Plus, Trash2 } from "lucide-react";
import type { Program } from "../../ipc";
import { useAppSnapshot } from "../../state/useAppState";
import { useTreadmillStore } from "../../store/treadmillStore";
import { ActiveProgramCard } from "../programs/ActiveProgramCard";
import { FitToDeck } from "../programs/FitToDeck";
import { NEW_PROGRAM, toDraft, type Draft } from "../programs/draft";
import { describePlan, describeProgram } from "../programs/format";
import { ProgramEditor } from "../programs/ProgramEditor";
import { GhostButton, NeonButton, Page, Panel, Waiting } from "../ui";
import { linkState } from "../../state/linkStatus";
import { cx, openLinkDialog, send } from "../util";

/** Why Run is off while no treadmill is linked, with the dashboard's own Connect. It only links; it never starts the belt. */
function OfflineLine({ linking }: { linking: boolean }) {
  return (
    <div className="flex min-h-8 items-center gap-3 px-1" data-testid="programs-offline">
      <span className="min-w-0 flex-1 font-mono text-xs text-white/60">
        {linking ? "Connecting to the treadmill." : "Treadmill not connected."}
      </span>
      {!linking && (
        <NeonButton className="h-8 shrink-0 px-4" onClick={openLinkDialog}>
          Connect
        </NeonButton>
      )}
    </div>
  );
}

export function ProgramsPage() {
  const s = useAppSnapshot();
  const confirm = useTreadmillStore((st) => st.confirm);
  const programRequest = useTreadmillStore((st) => st.programRequest);
  const clearProgramRequest = useTreadmillStore((st) => st.clearProgramRequest);
  const [picked, setPicked] = useState<Draft | null>(null);

  if (!s) return <Waiting />;
  // A program asked for from History (ECHO) opens in the editor once the snapshot has it.
  const requested = picked === null && programRequest !== null ? s.programs.find((p) => p.id === programRequest) : undefined;
  const editing = picked ?? (requested ? toDraft(requested) : null);
  const closeEditor = () => {
    setPicked(null);
    clearProgramRequest();
  };
  const runningId = s.active_program?.program_id;
  const state = linkState(s.connection).state;
  const linking = state === "connecting" || state === "scanning";

  const remove = (p: Program) =>
    confirm({
      title: "Delete program",
      message: `Delete "${p.name}"? A deleted program can't be restored.`,
      onConfirm: () => send({ type: "program_delete", id: p.id }),
    });

  return (
    <Page
      codename="ROUTINE_LIBRARY"
      title="Programs"
      actions={
        !editing && (
          <NeonButton className="flex h-8 items-center gap-1.5 px-3" onClick={() => setPicked(NEW_PROGRAM)}>
            <Plus size={14} /> New
          </NeonButton>
        )
      }
    >
      <div className="flex flex-col gap-3">
        <ActiveProgramCard />

        {editing ? (
          <ProgramEditor key={editing.id} initial={editing} onDone={closeEditor} />
        ) : s.programs.length === 0 ? (
          <Panel className="flex flex-col items-center gap-3 p-6 text-center">
            <div className="flex flex-col gap-1 font-mono text-xs text-white/55" data-testid="programs-empty">
              <p>No programs yet.</p>
              <p>Use New to build one, or ECHO on History to turn a walk into one.</p>
            </div>
            <NeonButton className="flex h-8 items-center gap-1.5 px-3" onClick={() => setPicked(NEW_PROGRAM)}>
              <Plus size={14} /> New
            </NeonButton>
          </Panel>
        ) : (
          <ul className="flex flex-col gap-2">
            {!s.connection.connected && (
              <li>
                <OfflineLine linking={linking} />
              </li>
            )}
            {s.programs.map((p) => {
              const running = p.id === runningId;
              return (
                <li key={p.id}>
                  <Panel className={cx("flex items-center gap-3 px-4 py-3", running && "border-neon-yellow/40")}>
                    <div className="min-w-0 flex-1" title={describePlan(p)}>
                      <div className="break-words font-digital text-sm font-bold leading-tight tracking-wider text-white">{p.name}</div>
                      <div className="font-mono text-xs text-white/50">{describeProgram(p)}</div>
                      {p.note && <div className="text-xs leading-snug text-white/40">{p.note}</div>}
                      <FitToDeck offer={s.fit_to_deck.find((o) => o.program_id === p.id) ?? null} />
                    </div>
                    <GhostButton aria-label={`Edit ${p.name}`} onClick={() => setPicked(toDraft(p))}>
                      <Pencil size={14} />
                    </GhostButton>
                    <GhostButton aria-label={`Delete ${p.name}`} className="hover:text-neon-magenta" onClick={() => remove(p)}>
                      <Trash2 size={14} />
                    </GhostButton>
                    <NeonButton
                      tone={running ? "yellow" : "cyan"}
                      className="flex h-9 w-24 items-center justify-center gap-1.5"
                      disabled={!s.connection.connected || running}
                      title={s.connection.connected ? describePlan(p) : "Connect the treadmill first"}
                      onClick={() => send({ type: "program_run", id: p.id })}
                    >
                      {running ? "Running" : (
                        <>
                          <Play size={14} /> Run
                        </>
                      )}
                    </NeonButton>
                  </Panel>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </Page>
  );
}
