import { dispatch, SETTING_LIMITS } from "../../ipc";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { roundSpeed, snapToDeck } from "../../state/deckSpeeds";
import { useAppSnapshot } from "../../state/useAppState";
import { GhostButton, Hint, Label } from "../ui";
import { cx } from "../util";
import { NumberField, Section, Toggle } from "./fields";
import { LightsOutSetting } from "./LightsOutSetting";
import { PresentingSetting } from "./PresentingSetting";

const L = SETTING_LIMITS;
const MAX_PROGRAM_BUTTONS = L.seated_program_buttons;

/** Walk-away safety: pause the belt on lock, or after a long time without input. */
function WalkAwaySettings() {
  const s = useAppSnapshot();
  const lockAction = useAction();
  const idleAction = useAction();
  if (!s) return null;
  const w = s.walkaway;
  const mac = s.platform.os === "macos";
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
          label={`Pause when I lock the ${s.platform.computer}`}
          tone="yellow"
          checked={w.lock_enabled}
          onChange={(lock_enabled) => void lockAction.run(() => save({ lock_enabled }))}
          description={
            mac
              ? "Locking the Mac (⌃⌘Q) pauses a moving belt and any running program."
              : "Locking Windows (Win+L) pauses a moving belt and any running program."
          }
          hint="The session is kept and the belt never restarts by itself. The HUD asks for RE-JACK or DUMP."
        />
        <ActionStatus pending={lockAction.pending} error={lockAction.error} className="mt-1" />
      </div>
      <div>
        <Toggle
          label={`Pause after no input for ${w.idle_minutes} minutes`}
          tone="yellow"
          checked={w.idle_enabled}
          onChange={(idle_enabled) => void idleAction.run(() => save({ idle_enabled }))}
          description={`After ${w.idle_minutes} minutes with no keyboard or mouse input, the HUD warns for 30 seconds, then pauses the belt.`}
          hint="Any input or STAY on the HUD card cancels the pause, while a call never starts one."
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
  const enableAction = useAction();
  const soundAction = useAction();
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
  const save = (patch: Parameters<typeof config>[0]) => dispatch(config(patch));
  return (
    <>
      <div>
        <Toggle
          label="OCULAR_IMPLANT"
          tone="cyan"
          checked={e.enabled}
          onChange={(enabled) => void enableAction.run(() => save({ enabled }))}
          description="The HUD reminds you to look away from the screen, with a minute's warning."
        />
        <ActionStatus pending={enableAction.pending} error={enableAction.error} className="mt-1" />
      </div>
      {e.enabled && (
        <>
          <NumberField
            label="Break every"
            unit="minutes of screen use"
            min={L.eye_break_minutes.min}
            max={L.eye_break_minutes.max}
            step={5}
            value={e.minutes}
            hint={`${L.eye_break_minutes.min} to ${L.eye_break_minutes.max}. Studies on myopia favour 5 minutes an hour.`}
            onCommit={(minutes) => save({ minutes })}
          />
          <NumberField
            label="Break lasts"
            unit="seconds"
            min={L.eye_break_seconds.min}
            max={L.eye_break_seconds.max}
            step={10}
            value={e.seconds}
            hint={`${L.eye_break_seconds.min} to ${L.eye_break_seconds.max}. Focusing needs about 90 seconds to relax.`}
            onCommit={(seconds) => save({ seconds })}
          />
          <div>
            <Toggle
              label="Tones"
              tone="cyan"
              checked={e.sound}
              onChange={(sound) => void soundAction.run(() => save({ sound }))}
              description="Beeps guide each full blink: falling to close, low to squeeze, high to open, rising at the end."
            />
            <ActionStatus pending={soundAction.pending} error={soundAction.error} className="mt-1" />
          </div>
          <div>
            <Toggle
              label="Pause the belt for breaks"
              tone="cyan"
              checked={e.pause_belt}
              onChange={(pause_belt) => void pauseAction.run(() => save({ pause_belt }))}
              description="A break pauses a walking belt. The HUD then offers RE-JACK or STAY STILL."
            />
            <ActionStatus pending={pauseAction.pending} error={pauseAction.error} className="mt-1" />
          </div>
          <div>
            <Toggle
              label="Take breaks at program rests"
              tone="cyan"
              checked={e.fit_rests}
              onChange={(fit_rests) => void restsAction.run(() => save({ fit_rests }))}
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
  const nudgeAction = useAction();
  // The program chips and Use the first three share one status line, under the chips.
  const chipsAction = useAction();
  const callsAction = useAction();
  if (!s) return null;
  const { seated, calls, programs } = s;
  const pc = s.platform.computer;

  // The card's buttons as Rust resolves them. Sending `null` goes back to the
  // default (the first three saved programs).
  const saveSeated = (patch: { enabled?: boolean; minutes?: number; program_ids?: number[] | null }) =>
    dispatch({
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
    void chipsAction.run(() => saveSeated({ program_ids: next.length > 0 ? next : null }));
  };

  const saveCalls = (patch: { enabled?: boolean; quiet_kmh?: number }) =>
    dispatch({ type: "calls_set_config", enabled: calls.enabled, quiet_kmh: calls.quiet_kmh, ...patch });

  const full = seated.program_ids.length >= MAX_PROGRAM_BUTTONS;

  return (
    <Section
      id="sys-desk"
      title="Desk"
      description="Small nudges that never start the belt."
    >
      <div>
        <Toggle
          label="Idle nudge"
          tone="yellow"
          checked={seated.enabled}
          onChange={(enabled) => void nudgeAction.run(() => saveSeated({ enabled }))}
          description={`After a stretch of ${pc} use without walking, a HUD card offers a walk or a program.`}
          hint="Only the time of the last input is kept, never which keys."
        />
        <ActionStatus pending={nudgeAction.pending} error={nudgeAction.error} className="mt-1" />
      </div>
      {seated.enabled && (
        <>
          <NumberField
            label="Remind after"
            unit={`minutes of ${pc} use`}
            min={L.seated_minutes.min}
            max={L.seated_minutes.max}
            step={5}
            value={seated.minutes}
            hint={`${L.seated_minutes.min} to ${L.seated_minutes.max}. Walking for 2 minutes or being away from the ${pc} for 15 starts the count again.`}
            onCommit={(minutes) => saveSeated({ minutes })}
          />
          <div>
            <div className="flex flex-wrap items-center justify-between gap-2">
              <Label>Programs on the card</Label>
              {seated.custom_programs && (
                <GhostButton
                  disabled={chipsAction.pending}
                  onClick={() => void chipsAction.run(() => saveSeated({ program_ids: null }))}
                >
                  Use the first three
                </GhostButton>
              )}
            </div>
            {programs.length === 0 ? (
              <Hint className="mt-1">
                Programs saved on the Programs page appear on the card.
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
                        disabled={(!picked && full) || chipsAction.pending}
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
            <ActionStatus pending={chipsAction.pending} error={chipsAction.error} className="mt-1.5" />
            <Hint className="mt-1.5">A program picked on the card waits until the treadmill connects.</Hint>
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

      {s.platform.os === "windows" && (
        <>
          <div className="border-t border-white/10" />
          <PresentingSetting />
        </>
      )}

      <div className="border-t border-white/10" />

      {/* Slow down on calls needs the OS's list of apps using the microphone (`platform.microphone`). */}
      {s.platform.microphone && (
        <div>
          <Toggle
            label="Slow down on calls"
            tone="yellow"
            checked={calls.enabled}
            onChange={(enabled) => void callsAction.run(() => saveCalls({ enabled }))}
            description="While an app uses the microphone, a running belt eases to the quiet speed until 10 seconds after the call."
            hint="The belt never speeds up for a call and keeps a speed changed during the call."
          />
          <ActionStatus pending={callsAction.pending} error={callsAction.error} className="mt-1" />
        </div>
      )}
      {/* The HUD's QUIET button (Quiet now) uses the same speed, with or without call detection. */}
      <NumberField
        label="Quiet speed"
        unit="km/h"
        min={L.calls_quiet_kmh.min}
        max={L.calls_quiet_kmh.max}
        step={0.1}
        value={calls.quiet_kmh}
        hint={[
          `${s.platform.microphone ? "Calls and QUIET on the HUD ease" : "QUIET on the HUD eases"} a running belt to the quiet speed.`,
          "A running program eases for the current step only.",
        ]}
        snap={(kmh) => roundSpeed(snapToDeck(kmh, s.deck))}
        onCommit={(quiet_kmh) => saveCalls({ quiet_kmh: Math.round(quiet_kmh * 100) / 100 })}
      />
      {s.platform.microphone && calls.enabled && (
        <Hint aria-live="polite">
          {calls.quiet_active
            ? "On a call: the belt is at the quiet speed."
            : calls.mic_in_use
              ? "Microphone in use."
              : "Microphone is free."}{" "}
          {s.platform.os === "macos"
            ? "Asks macOS which apps take microphone input and never records or hears audio."
            : "Reads the list Windows keeps of apps using the microphone and never records or hears audio."}
        </Hint>
      )}
    </Section>
  );
}
