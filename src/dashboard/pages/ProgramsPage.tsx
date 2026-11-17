import { useState } from "react";
import { Pencil, Play, Plus, Trash2 } from "lucide-react";
import type { Program } from "../../ipc";
import { useAppSnapshot } from "../../state/useAppState";
import { useTreadmillStore } from "../../store/treadmillStore";
import { ActiveProgramCard } from "../programs/ActiveProgramCard";
import { NEW_PROGRAM, toDraft, type Draft } from "../programs/draft";
import { describePlan, describeProgram } from "../programs/format";
import { ProgramEditor } from "../programs/ProgramEditor";
import { GhostButton, NeonButton, Page, Panel, Waiting } from "../ui";
import { cx, send } from "../util";

export function ProgramsPage() {
  const s = useAppSnapshot();
  const confirm = useTreadmillStore((st) => st.confirm);
  const [editing, setEditing] = useState<Draft | null>(null);

  if (!s) return <Waiting />;
  const runningId = s.active_program?.program_id;

  const remove = (p: Program) =>
    confirm({
      title: "Delete program",
      message: `Delete "${p.name}"? This can't be undone.`,
      onConfirm: () => send({ type: "program_delete", id: p.id }),
    });

  return (
    <Page
      codename="ROUTINE_LIBRARY"
      title="Programs"
      actions={
        !editing && (
          <NeonButton className="flex h-8 items-center gap-1.5 px-3" onClick={() => setEditing(NEW_PROGRAM)}>
            <Plus size={14} /> New
          </NeonButton>
        )
      }
    >
      <div className="flex flex-col gap-3">
        <ActiveProgramCard />

        {editing ? (
          <ProgramEditor key={editing.id} initial={editing} onDone={() => setEditing(null)} />
        ) : s.programs.length === 0 ? (
          <Panel className="p-6 text-center">
            <p className="font-mono text-xs text-white/55">
              Save a routine once, then start it with one click here or from the HUD.
            </p>
          </Panel>
        ) : (
          <ul className="flex flex-col gap-2">
            {s.programs.map((p) => {
              const running = p.id === runningId;
              return (
                <li key={p.id}>
                  <Panel className={cx("flex items-center gap-3 px-4 py-3", running && "border-neon-yellow/40")}>
                    <div className="min-w-0 flex-1" title={describePlan(p)}>
                      <div className="break-words font-digital text-sm font-bold leading-tight tracking-wider text-white">{p.name}</div>
                      <div className="font-mono text-xs text-white/50">{describeProgram(p)}</div>
                      {p.note && <div className="text-xs leading-snug text-white/40">{p.note}</div>}
                    </div>
                    <GhostButton aria-label={`Edit ${p.name}`} onClick={() => setEditing(toDraft(p))}>
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
