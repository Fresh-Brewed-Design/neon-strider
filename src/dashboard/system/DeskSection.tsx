import { dispatch, SETTING_LIMITS } from "../../ipc";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { roundSpeed, snapToDeck } from "../../state/deckSpeeds";
import { useAppSnapshot } from "../../state/useAppState";
import { GhostButton, Hint, Label } from "../ui";
import { cx, send } from "../util";
import { NumberField, Section, Toggle } from "./fields";
import { LightsOutSetting } from "./LightsOutSetting";

const L = SETTING_LIMITS;
const MAX_PROGRAM_BUTTONS = L.seated_program_buttons;

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
          min={L.walkaway_idle_minutes.min}
          max={L.walkaway_idle_minutes.max}
          step={1}
          value={w.idle_minutes}
          hint={`${L.walkaway_idle_minutes.min} to ${L.walkaway_idle_minutes.max}. Long enough to watch a video while you walk.`}
          onCommit={(idle_minutes) => save({ idle_minutes: Math.round(idle_minutes) })}
        />
      )}
    </>
  );
}

/**
 * Eye breaks (OCULAR_IMPLANT): off by default; with them on, a break pauses a
 * walking belt unless turned off, and moves to a running program's rest when
 * one is near (`fit_rests`).
 */
function EyeBreakSettings() {
  const s = useAppSnapshot();
  const pauseAction = useAction();
  const restsAction = useAction();
  if (!s) return null;
  const e = s.eye_break;
  const config = (patch: {
    enabled?: boolean;
    minutes?: number;
    seconds?: number;
    sound?: boolean;
    pause_belt?: boolean;
    fit_rests?: boolean;
  }) => ({
    type: "eye_break_set_config" as const,
    enabled: e.enabled,
    minutes: e.minutes,
    seconds: e.seconds,
    sound: e.sound,
    pause_belt: e.pause_belt,
    fit_rests: e.fit_rests,
    ...patch,
  });
  const save = (patch: Parameters<typeof config>[0]) => send(config(patch));
  return (
    <>
      <Toggle
        label="OCULAR_IMPLANT"
        tone="cyan"
        checked={e.enabled}
        onChange={(enabled) => save({ enabled })}
        description="The HUD reminds you to look away from the screen, with a minute's warning."
      />
      {e.enabled && (
        <>
          <NumberField
            label="Break every"
            unit="minutes of screen use"
            min={L.eye_break_minutes.min}
            max={L.eye_break_minutes.max}
            step={5}
            value={e.minutes}
            hint={`${L.eye_break_minutes.min} to ${L.eye_break_minutes.max}. The 20-minute habit is popular, but studies on myopia favour 5 minutes an hour.`}
            onCommit={(minutes) => save({ minutes })}
          />
          <NumberField
            label="Break lasts"
            unit="seconds"
            min={L.eye_break_seconds.min}
            max={L.eye_break_seconds.max}
            step={10}
            value={e.seconds}
            hint={`${L.eye_break_seconds.min} to ${L.eye_break_seconds.max}. Focusing needs about 90 seconds to relax; 20 seconds is often too short.`}
            onCommit={(seconds) => save({ seconds })}
          />
          <Toggle
            label="Tones"
            tone="cyan"
            checked={e.sound}
            onChange={(sound) => save({ sound })}
            description="Beeps guide each full blink: falling to close, low to squeeze, high to open, rising at the end."
          />
          <div>
            <Toggle
              label="Pause the belt for breaks"
              tone="cyan"
              checked={e.pause_belt}
              onChange={(pause_belt) => void pauseAction.run(() => dispatch(config({ pause_belt })))}
              description="A break pauses a walking belt, and the HUD then offers RESUME or STAY STILL."
            />
            <ActionStatus pending={pauseAction.pending} error={pauseAction.error} className="mt-1" />
          </div>
          <div>
            <Toggle
              label="Take breaks at program rests"
              tone="cyan"
              checked={e.fit_rests}
              onChange={(fit_rests) => void restsAction.run(() => dispatch(config({ fit_rests })))}
              description="A break due near a program's rest moves to that rest, so the walk carries on."
            />
            <ActionStatus pending={restsAction.pending} error={restsAction.error} className="mt-1" />
          </div>
        </>
      )}
    </>
  );
}

/** Settings for the idle nudge and for easing the belt during calls. */
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
        label="Idle nudge"
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
            min={L.seated_minutes.min}
            max={L.seated_minutes.max}
            step={5}
            value={seated.minutes}
            hint={`${L.seated_minutes.min} to ${L.seated_minutes.max}. Walking for 2 minutes, or being away from the PC for 15, starts the count again.`}
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

      <EyeBreakSettings />

      <div className="border-t border-white/10" />

      <WalkAwaySettings />

      <div className="border-t border-white/10" />

      <LightsOutSetting />

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
            min={L.calls_quiet_kmh.min}
            max={L.calls_quiet_kmh.max}
            step={0.1}
            value={calls.quiet_kmh}
            hint="A running program is eased too, for the current step only; the saved program is not changed."
            snap={(kmh) => roundSpeed(snapToDeck(kmh, s.deck))}
            onCommit={(quiet_kmh) => saveCalls({ quiet_kmh: Math.round(quiet_kmh * 100) / 100 })}
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
