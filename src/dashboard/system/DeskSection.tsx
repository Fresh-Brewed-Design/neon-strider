import { dispatch } from "../../ipc";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { useAppSnapshot } from "../../state/useAppState";
import { GhostButton, Hint, Label } from "../ui";
import { cx, send } from "../util";
import { NumberField, Section, Toggle } from "./fields";

const MAX_PROGRAM_BUTTONS = 3;

/** Walk-away safety: pause the belt on lock, or after a long time without input. */
function WalkAwaySettings() {
  const s = useAppSnapshot();
  const lockAction = useAction();
  const idleAction = useAction();
  if (!s) return null;
  const w = s.walkaway;
  const save = (patch: { lock_enabled?: boolean; idle_enabled?: boolean; idle_minutes?: number }) =>
    dispatch({
      type: "walkaway_set_config",
      lock_enabled: w.lock_enabled,
      idle_enabled: w.idle_enabled,
      idle_minutes: w.idle_minutes,
      ...patch,
    });

  return (
    <>
      <div>
        <Toggle
          label="Pause when I lock the PC"
          tone="yellow"
          checked={w.lock_enabled}
          onChange={(lock_enabled) => void lockAction.run(() => save({ lock_enabled }))}
          description="Locking Windows (Win+L) pauses a moving belt at once, a running program too. The session is kept; it never restarts by itself, so the HUD asks to resume or end when you are back."
        />
        <ActionStatus pending={lockAction.pending} error={lockAction.error} className="mt-1" />
      </div>
      <div>
        <Toggle
          label={`Pause after no input for ${w.idle_minutes} minutes`}
          tone="yellow"
          checked={w.idle_enabled}
          onChange={(idle_enabled) => void idleAction.run(() => save({ idle_enabled }))}
          description="After a long time without keyboard or mouse input, the HUD warns for 30 seconds, then pauses the belt. Any input, or STAY on the card, cancels it. Not during a call, so you can talk while you walk."
        />
        <ActionStatus pending={idleAction.pending} error={idleAction.error} className="mt-1" />
      </div>
      {w.idle_enabled && (
        <NumberField
          label="Pause after"
          unit="minutes without input"
          min={2}
          max={30}
          step={1}
          value={w.idle_minutes}
          hint="2 to 30. Long enough to watch a video while you walk."
          onCommit={(idle_minutes) => save({ idle_minutes: Math.round(idle_minutes) })}
        />
      )}
    </>
  );
}

/** Settings for the sit-too-long nudge and for easing the belt during calls. */
export function DeskSection() {
  const s = useAppSnapshot();
  if (!s) return null;
  const { seated, calls, programs } = s;

  // The card's buttons as Rust resolves them. Sending `null` goes back to the
  // default (the first three saved programs).
  const saveSeated = (patch: { enabled?: boolean; minutes?: number; program_ids?: number[] | null }) =>
    send({
      type: "seated_set_config",
      enabled: seated.enabled,
      minutes: seated.minutes,
      program_ids: seated.custom_programs ? seated.program_ids : null,
      ...patch,
    });

  const toggleProgram = (id: number) => {
    const next = seated.program_ids.includes(id)
      ? seated.program_ids.filter((p) => p !== id)
      : [...seated.program_ids, id].slice(0, MAX_PROGRAM_BUTTONS);
    saveSeated({ program_ids: next.length > 0 ? next : null });
  };

  const saveCalls = (patch: { enabled?: boolean; quiet_kmh?: number }) =>
    send({ type: "calls_set_config", enabled: calls.enabled, quiet_kmh: calls.quiet_kmh, ...patch });

  const full = seated.program_ids.length >= MAX_PROGRAM_BUTTONS;

  return (
    <Section
      id="sys-desk"
      title="Desk"
      description="Small nudges for a day at the PC. The treadmill switches itself off when idle, so none of this can start it: a program you pick waits until the treadmill connects."
    >
      <Toggle
        label="Sit-too-long nudge"
        tone="yellow"
        checked={seated.enabled}
        onChange={(enabled) => saveSeated({ enabled })}
        description="After a while of keyboard and mouse use without walking, a card on the HUD offers a walk or a program. Only the time of the last input is kept, never which keys."
      />
      {seated.enabled && (
        <>
          <NumberField
            label="Remind after"
            unit="minutes of PC use"
            min={15}
            max={240}
            step={5}
            value={seated.minutes}
            hint="15 to 240. Walking for 2 minutes, or being away from the PC for 15, starts the count again."
            onCommit={(minutes) => saveSeated({ minutes })}
          />
          <div>
            <div className="flex flex-wrap items-center justify-between gap-2">
              <Label>Programs on the card</Label>
              {seated.custom_programs && (
                <GhostButton onClick={() => saveSeated({ program_ids: null })}>Use the first three</GhostButton>
              )}
            </div>
            {programs.length === 0 ? (
              <Hint className="mt-1">
                Save a program on the Programs page and it will appear here.
              </Hint>
            ) : (
              <ul className="mt-1.5 flex flex-wrap gap-1.5" aria-label="Programs on the card">
                {programs.map((p) => {
                  const picked = seated.program_ids.includes(p.id);
                  return (
                    <li key={p.id} className="min-w-0 max-w-full">
                      <button
                        type="button"
                        aria-pressed={picked}
                        disabled={!picked && full}
                        onClick={() => toggleProgram(p.id)}
                        className={cx(
                          "max-w-full break-words rounded border px-2.5 py-1.5 text-left font-mono text-[11px] transition-colors",
                          "disabled:cursor-default disabled:opacity-35",
                          picked
                            ? "border-neon-yellow bg-neon-yellow/15 text-neon-yellow"
                            : "border-white/15 text-white/70 hover:border-neon-yellow/50",
                        )}
                      >
                        {p.name}
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
            <Hint className="mt-1.5">
              Up to {MAX_PROGRAM_BUTTONS}.{" "}
              {seated.custom_programs
                ? "Programs you delete drop off the card."
                : "Until you pick some, the card shows your first three."}
            </Hint>
          </div>
        </>
      )}

      <div className="border-t border-white/10" />

      <WalkAwaySettings />

      <div className="border-t border-white/10" />

      <Toggle
        label="Slow down on calls"
        tone="yellow"
        checked={calls.enabled}
        onChange={(enabled) => saveCalls({ enabled })}
        description="While an app is using the microphone, a running belt eases to a quiet speed (never faster than it was), and goes back 10 seconds after the call ends. A speed you change during the call is left alone."
      />
      {calls.enabled && (
        <>
          <NumberField
            label="Quiet speed"
            unit="km/h"
            min={0.5}
            max={6}
            step={0.1}
            value={calls.quiet_kmh}
            hint="A running program is eased too, for the current step only; the saved program is not changed."
            onCommit={(quiet_kmh) => saveCalls({ quiet_kmh: Math.round(quiet_kmh * 10) / 10 })}
          />
          <Hint aria-live="polite">
            {calls.quiet_active
              ? "On a call: the belt is at the quiet speed."
              : calls.mic_in_use
                ? "Microphone in use."
                : "Microphone is free."}{" "}
            This uses the list Windows keeps of apps using the microphone; no audio is recorded or heard.
          </Hint>
        </>
      )}
    </Section>
  );
}
