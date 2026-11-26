// The persistent HUD cards. Unlike toasts, each stays until it is acted on.

import { useEffect, useState } from "react";
import type { AppSnapshot, BlinkStep } from "../ipc";
import { formatDuration } from "../format";
import { PLAIN } from "../names";
import { formatChange, formatDeskDays } from "../dashboard/history/weekly";
import { describePlan } from "../dashboard/programs/format";
import { startFailure } from "../state/linkStatus";
import { resumeWord } from "./status";
import { HudNote } from "./HudNote";
import { DumpName } from "./DumpName";
import { SmallButton } from "./SmallButton";
import { useHudSend } from "./useHudSend";

/**
 * Belt service reminder. Not a toast: it stays until the service is marked
 * done, and that takes two clicks so a stray one can't clear it.
 */
export function MaintenanceCard({ snapshot }: { snapshot: AppSnapshot }) {
  const { send, error } = useHudSend();
  const [arming, setArming] = useState(false);
  useEffect(() => {
    if (!arming) return;
    const t = setTimeout(() => setArming(false), 4000); // time for the second press, short enough that a stray one disarms
    return () => clearTimeout(t);
  }, [arming]);

  const m = snapshot.maintenance;
  const usage = m.basis === "hours" ? `${m.hours_since.toFixed(1)} H` : `${Math.round(m.distance_since_km)} KM`;
  return (
    <div className="hud-toast hud-maint accent-yellow" role="alert">
      <div className="hud-toast-body">
        <span className="hud-label">BELT SERVICE DUE</span>
        <span className="hud-toast-message">
          {usage} · {m.days_since} DAYS SINCE LAST
        </span>
        <span className="hud-sub">Lubricate the deck with silicone oil.</span>
        <HudNote error={error} />
      </div>
      <SmallButton
        onClick={() => {
          if (arming) send({ type: "maintenance_mark_serviced" });
          setArming(!arming);
        }}
      >
        {arming ? "CONFIRM?" : "SERVICED"}
      </SmallButton>
    </div>
  );
}

/**
 * Last week's summary, shown on the first walk of a new week. Persistent like
 * the maintenance card: it stays until OK, which Rust remembers per ISO week.
 */
export function WeeklyCard({ snapshot }: { snapshot: AppSnapshot }) {
  const { send, error } = useHudSend();
  const w = snapshot.weekly.last_week;
  const change = formatChange(snapshot.weekly.distance_change);
  return (
    <div className="hud-toast hud-weekly accent-cyan" role="status">
      <div className="hud-toast-body">
        <span className="hud-label">LAST WEEK</span>
        <div className="hud-weekly-main">
          <span className="hud-value">
            {w.distance_km.toFixed(1)}
            <span className="hud-unit">KM</span>
          </span>
          {change && <span className={`hud-weekly-change tone-${change.tone}`}>{change.text}</span>}
        </div>
        <span className="hud-sub">
          {formatDeskDays(w).toUpperCase()} · {formatDuration(w.duration_s).toUpperCase()}
        </span>
        <HudNote error={error} />
      </div>
      <SmallButton onClick={() => send({ type: "weekly_summary_ack" })}>OK</SmallButton>
    </div>
  );
}

/** The card's one line under its buttons, if the treadmill's state needs one. */
function seatedLine(snapshot: AppSnapshot, programs: number): string | null {
  const queued = snapshot.programs.find((p) => p.id === snapshot.seated.pending_program_id);
  if (queued) return `${queued.name} starts when the treadmill is on.`;
  // Only an unlinked treadmill queues a program; one linked but asleep refuses every start.
  if (!snapshot.connection.connected && programs > 0) return "Treadmill off. Turn the treadmill on or queue a program.";
  if (!snapshot.connection.connected || snapshot.deck.standby) return "Treadmill off. Turn the treadmill on to engage.";
  return null;
}

/**
 * Belt still too long: a nudge to get up and walk. Persistent like the maintenance
 * card: it stays until acted on. ENGAGE is the HUD's Engage (a plain walk), and
 * needs the treadmill on; a program button starts it now, or, with the
 * treadmill off, remembers it and starts it as soon as the treadmill connects
 * (Rust drops it after 15 minutes). SNOOZE brings it back in 15 minutes.
 */
export function SeatedCard({ snapshot }: { snapshot: AppSnapshot }) {
  const { send, error } = useHudSend();
  const s = snapshot.seated;
  const programs = s.program_ids.flatMap((id) => snapshot.programs.find((p) => p.id === id) ?? []);
  const line = seatedLine(snapshot, programs.length);
  const note = error ?? startFailure(snapshot);
  return (
    <div className="hud-toast hud-seated accent-yellow" role="alert">
      <div className="hud-toast-body">
        <span className="hud-label">ATROPHY_WARNING</span>
        <span className="hud-plain">{PLAIN.ATROPHY_WARNING}</span>
        <span className="hud-toast-message">{s.seated_minutes} MIN IDLE</span>
      </div>
      <button
        type="button"
        className="hud-btn hud-seated-engage accent-cyan"
        disabled={!snapshot.connection.connected}
        title="Start walking at the usual speed, like Engage."
        onClick={() => void send({ type: "seated_engage" })}
      >
        ENGAGE
      </button>
      {programs.length > 0 && (
        <div className="hud-seated-programs">
          {programs.map((p) => (
            <button
              key={p.id}
              type="button"
              className={`hud-btn hud-btn-program${p.id === s.pending_program_id ? " is-queued" : ""}`}
              aria-pressed={p.id === s.pending_program_id}
              title={describePlan(p)}
              onClick={() => send({ type: "seated_start", id: p.id })}
            >
              {p.name}
            </button>
          ))}
        </div>
      )}
      {/* One line at a time: a failed press says more than the standing hint. */}
      {note ? <HudNote error={note} /> : line && <span className="hud-sub">{line}</span>}
      <div className="hud-seated-actions">
        <SmallButton onClick={() => send({ type: "seated_snooze" })}>SNOOZE 15 MIN</SmallButton>
      </div>
    </div>
  );
}

/**
 * Eye break (OCULAR_IMPLANT): a quiet heads-up a minute ahead, then the
 * cooldown with its countdown and SKIP. A break set for a running program's
 * rest (`at_rest`) waits for it and pauses nothing. Otherwise a walking belt is
 * paused for it (with "Pause the belt" on); after the break, while that pause holds, the card asks
 * GO or RE-JACK (the word for what the press resumes) or STAY STILL. Nothing here
 * starts the belt by itself. During the break the card shows the guided blink
 * step (`blink`: SHUTTER // CLOSE, SQUEEZE, OPEN). Rust counts the time and owns
 * the pause; the tones are played by the HUD (`useEyeBreakTones`).
 */
/** What each guided blink step asks, for when the tones are off. */
const BLINK_HINT: Record<BlinkStep, string> = {
  close: "Close your eyes and hold them closed.",
  squeeze: "Squeeze your eyelids gently.",
  open: "Open your eyes and look far away.",
};

export function EyeBreakCard({ snapshot }: { snapshot: AppSnapshot }) {
  const { send, error } = useHudSend();
  const e = snapshot.eye_break;
  if (e.phase === "complete") {
    return (
      <div className="hud-toast hud-eyebreak accent-cyan" role="alert">
        <div className="hud-toast-body">
          <span className="hud-toast-message">OCULAR_IMPLANT // RECALIBRATED</span>
          <span className="hud-plain">{PLAIN.OCULAR_IMPLANT}</span>
          <span className="hud-sub">The belt stays paused until you press {resumeWord(snapshot)}.</span>
        </div>
        <HudNote error={error} />
        <div className="hud-seated-actions">
          <SmallButton onClick={() => send({ type: "eye_break_resume" })}>{resumeWord(snapshot)}</SmallButton>
          <SmallButton onClick={() => send({ type: "eye_break_stay_still" })}>STAY STILL</SmallButton>
        </div>
      </div>
    );
  }
  const onBreak = e.phase === "break";
  const walking = snapshot.deck.status === "Running";
  return (
    <div className="hud-toast hud-eyebreak accent-cyan" role="alert">
      {onBreak ? (
        <div className="hud-toast-body">
          <span className="hud-toast-message">OCULAR_IMPLANT // COOLDOWN</span>
          <span className="hud-plain">{PLAIN.OCULAR_IMPLANT}</span>
          {e.blink && <span className="hud-eyebreak-cue">SHUTTER // {e.blink.toUpperCase()}</span>}
          <span className="hud-eyebreak-count">{e.remaining_s} S</span>
          <span className="hud-sub">
            {e.blink ? BLINK_HINT[e.blink] : "Look away from the screen."}
            {e.holds_belt ? " The belt is paused." : ""}
          </span>
          {/* END_TONE (eyeBreakTones.ts) plays as the break ends, only while tones are on. */}
          {e.sound && <span className="hud-sub">Two rising tones end the break.</span>}
        </div>
      ) : (
        <div className="hud-toast-body">
          <span className="hud-label">OCULAR_IMPLANT</span>
          <span className="hud-plain">{PLAIN.OCULAR_IMPLANT}</span>
          <span className="hud-toast-message">IMPLANT COOLDOWN IN {e.remaining_s} S</span>
          <span className="hud-sub">
            Finish what you are typing, then look away.
            {e.at_rest
              ? " The break starts at the program's rest."
              : e.pause_belt && walking
                ? " The belt will pause."
                : ""}
          </span>
        </div>
      )}
      <HudNote error={error} />
      <div className="hud-seated-actions">
        <SmallButton title="Skip the eye break." onClick={() => send({ type: "eye_break_skip" })}>
          SKIP BREAK
        </SmallButton>
      </div>
    </div>
  );
}

/**
 * A new version is ready, shown once per version. NOTES opens the dashboard
 * on the release notes, where Install is (the HUD never installs blind);
 * LATER just hides the card.
 */
export function UpdateCard({ snapshot }: { snapshot: AppSnapshot }) {
  const { send, error } = useHudSend();
  const version = snapshot.update.offer?.version;
  if (!version) return null;
  return (
    <div className="hud-toast hud-update accent-cyan" role="status">
      <div className="hud-toast-body">
        <span className="hud-toast-message">UPDATE READY // v{version}</span>
        <span className="hud-sub">Read the release notes in the dashboard, then press Install.</span>
        <HudNote error={error} />
      </div>
      <div className="hud-walkaway-actions">
        <SmallButton title="Open the release notes in the dashboard." onClick={() => send({ type: "update_hud_card", open: true })}>
          NOTES
        </SmallButton>
        <SmallButton onClick={() => send({ type: "update_hud_card", open: false })}>LATER</SmallButton>
      </div>
    </div>
  );
}

/**
 * Walk-away safety. The warning counts down to a pause after a long time with
 * no keyboard or mouse input (any input or STAY cancels it); after a pause,
 * the belt never restarts by itself, so the card waits for RE-JACK (GO on a paused program) or DUMP.
 */
export function WalkAwayCard({ snapshot }: { snapshot: AppSnapshot }) {
  const { send, error } = useHudSend();
  const w = snapshot.walkaway;
  if (w.card === "warning") {
    return (
      <div className="hud-toast hud-walkaway accent-yellow" role="alert">
        <div className="hud-toast-body">
          <span className="hud-toast-message">WALK-AWAY // PAUSING IN {w.pause_in_s}s</span>
          <span className="hud-sub">
            No input for {w.idle_minutes} minutes: move the mouse or press STAY to keep walking.
          </span>
          <HudNote error={error} />
        </div>
        <div className="hud-walkaway-actions">
          <SmallButton onClick={() => send({ type: "walkaway_stay" })}>STAY</SmallButton>
        </div>
      </div>
    );
  }
  const why =
    w.reason === "lock"
      ? `You locked the ${snapshot.platform.computer}.`
      : `No keyboard or mouse input for ${w.idle_minutes} minutes.`;
  return (
    <div className="hud-toast hud-walkaway accent-yellow" role="alert">
      <div className="hud-toast-body">
        <span className="hud-toast-message">WALK-AWAY // BELT PAUSED</span>
        <span className="hud-sub">{why} The session is kept. Press {resumeWord(snapshot)} when you are back.</span>
        <HudNote error={error} />
      </div>
      <div className="hud-walkaway-actions">
        <SmallButton className="hud-btn-primary accent-cyan" onClick={() => send({ type: "walkaway_resume" })}>
          {resumeWord(snapshot)}
        </SmallButton>
        <SmallButton
          className="hud-btn-dump"
          sub="end walk"
          title="Stop the belt and end the session."
          onClick={() => send({ type: "walkaway_end" })}
        >
          <DumpName />
        </SmallButton>
      </div>
    </div>
  );
}
