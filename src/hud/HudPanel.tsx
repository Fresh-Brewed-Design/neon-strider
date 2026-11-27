import { Fragment } from "react";
import type { ActiveProgram, AppSnapshot, Command, HudWidget, PulseTarget, ZoneBand } from "../ipc";
import { describeActive, describePlan, describeTimeLeft, waitHeadline } from "../dashboard/programs/format";
import { nextStepLine } from "../dashboard/programs/stepMap";
import { statWidget } from "../widgets/registry";
import { Sparkline } from "../widgets/Sparkline";
import { targetAccent, targetRange, ZONE_ACCENT, zoneRange } from "../widgets/zone";
import { NotePop, type PanelNote } from "./HudNote";
import { linkState, linkView } from "../state/linkStatus";
import { SmallButton } from "./SmallButton";
import { DumpName } from "./DumpName";
import { deckAsleep, dumpTitle, ENGAGE_SUBS, ENGAGE_WORDS, engageLabel, engageSub, engageTitle, firstStepLine, offlineText, quietHold, WAKE_FIRST, WAKE_FIRST_SUB } from "./status";
import { Swap } from "./Swap";
import type { useRelink } from "./useRelink";
import { useWheelSteps } from "./useWheelSteps";

type Send = (c: Command) => Promise<boolean>;

/** One pinned stat, read through the shared widget registry. */
export function StatCell({ id, snapshot }: { id: HudWidget; snapshot: AppSnapshot }) {
  const def = statWidget(id);
  if (!def) return null;
  const { value, unit } = def.read(snapshot);
  const spark = def.spark?.(snapshot);
  const zone = def.zone?.(snapshot);
  const target = def.target?.(snapshot);
  const hasDetail = def.detail !== undefined && def.panelDetail !== false;
  const detail = hasDetail ? def.detail?.(snapshot) : null;
  return (
    <div className={`hud-stat accent-${def.accent}`}>
      <span className="hud-label">{def.label}</span>
      <span className="hud-value">
        {value}
        {unit && <span className="hud-unit">{unit}</span>}
      </span>
      {spark && <Sparkline data={spark} className="hud-spark" />}
      {zone && <ZoneLine zone={zone} />}
      {target && <TargetLine target={target} />}
      {/* Room for a zone or pulse target line that comes and goes with program steps, so the panel keeps its height. */}
      {!zone && !target && def.reserveZone?.(snapshot) && (
        <div className="hud-zone hud-reserve" aria-hidden="true">
          <span className="hud-zone-label">{BLANK}</span>
          <span className="hud-sub">{BLANK}</span>
        </div>
      )}
      {hasDetail && (
        <span className={`hud-sub hud-stat-detail${detail ? "" : " hud-reserve"}`} aria-hidden={detail ? undefined : true}>
          {detail ?? BLANK}
        </span>
      )}
    </div>
  );
}

/** The heart rate zone under PULSE_RATE: Rust's word for under, in or over,
 *  then the range. Rust's sentence is the tooltip. */
function ZoneLine({ zone }: { zone: ZoneBand }) {
  const accent = zone.position ? ZONE_ACCENT[zone.position] : null;
  return (
    <div className="hud-zone" title={zone.message} data-testid="hud-zone">
      <span className={accent ? `hud-zone-label is-placed accent-${accent}` : "hud-zone-label"}>{zone.label}</span>
      <span className="hud-sub">
        {zoneRange(zone)}
        <span className="hud-unit"> BPM</span>
      </span>
    </div>
  );
}

/** A cool-down's pulse target under PULSE_RATE: Rust's word for where the
 *  pulse is, then the target. Rust's sentence is the tooltip. */
function TargetLine({ target }: { target: PulseTarget }) {
  const accent = targetAccent(target);
  return (
    <div className="hud-zone" title={target.message} data-testid="hud-pulse-target">
      <span className={accent ? `hud-zone-label is-placed accent-${accent}` : "hud-zone-label"}>{target.label}</span>
      <span className="hud-sub">
        {targetRange(target)}
        <span className="hud-unit"> BPM</span>
      </span>
    </div>
  );
}

function Stepper({
  name,
  label,
  value,
  accent,
  disabled,
  onDown,
  onUp,
  wheelHint,
}: {
  /** The codename the buttons are named by ("VEL up"). */
  name: string;
  /** The label over the stepper, which says it sets the target. */
  label: string;
  value: string;
  accent: "cyan" | "magenta";
  disabled: boolean;
  onDown: () => void;
  onUp: () => void;
  wheelHint: string;
}) {
  const wheel = useWheelSteps<HTMLSpanElement>((up) => (up ? onUp() : onDown()), disabled);
  return (
    <div className={`hud-stepper accent-${accent}`}>
      <span className="hud-label">{label}</span>
      <div className="hud-stepper-row">
        <button type="button" className="hud-btn hud-btn-step" disabled={disabled} onClick={onDown} aria-label={`${name} down`}>
          −
        </button>
        <span ref={wheel} className="hud-stepper-value" title={wheelHint}>{value}</span>
        <button type="button" className="hud-btn hud-btn-step" disabled={disabled} onClick={onUp} aria-label={`${name} up`}>
          +
        </button>
      </div>
    </div>
  );
}

/** Quiet note while Neural flow eases the belt off: no toast per change. */
export function NeuralLine({ snapshot }: { snapshot: AppSnapshot }) {
  const n = snapshot.neural;
  if (!n.enabled || !n.active) return null;
  return (
    <div className="hud-neural accent-green" title={`Typing: easing ${n.base_kmh.toFixed(1)} to ${n.target_kmh.toFixed(1)} km/h`}>
      <span className="hud-label">NEURAL_FLUIDITY // {Math.round(n.factor * 100)}%</span>
      <span className="hud-neural-speed">
        {n.target_kmh.toFixed(1)}
        <span className="hud-unit">/{n.base_kmh.toFixed(1)}</span>
      </span>
    </div>
  );
}

/**
 * Quiet line while the belt runs faster than STRIDE_FIT, by hand or from a
 * program. Rust decides when it shows (`deck.above_stride_fit`); it stays
 * while above, with no toast, and nothing here changes the speed.
 */
export function StrideFitLine({ snapshot }: { snapshot: AppSnapshot }) {
  const { deck } = snapshot;
  const fit = deck.stride_fit_kmh;
  if (!deck.above_stride_fit || fit === null) return null;
  return (
    <div
      className="hud-stride accent-yellow"
      data-testid="hud-stride"
      title={`The belt runs faster than your STRIDE_FIT of ${fit.toFixed(1)} km/h. Slow to ${fit.toFixed(1)} km/h or under.`}
    >
      <span className="hud-label">ABOVE STRIDE_FIT</span>
      <span className="hud-stride-speed">
        <span className="hud-unit">slow to</span>
        {fit.toFixed(1)}
        <span className="hud-unit">km/h</span>
      </span>
    </div>
  );
}

/**
 * Quiet line while speed drift sets the belt's speed, so a slow change never
 * looks like a fault. Rust decides when (`speed_drift.state`); its sentence
 * is the tooltip. A speed set by hand ends drift and the line goes.
 */
export function SpeedDriftLine({ snapshot }: { snapshot: AppSnapshot }) {
  const d = snapshot.speed_drift;
  const drifting = d.state === "drifting" || d.state === "held";
  if (!drifting || d.band_low_kmh === null || d.band_high_kmh === null) return null;
  return (
    <div className="hud-drift accent-cyan" data-testid="hud-drift" title={d.message}>
      <span className="hud-label">SPEED DRIFT</span>
      <span className="hud-drift-speed">
        {d.band_low_kmh.toFixed(1)} to {d.band_high_kmh.toFixed(1)}
        <span className="hud-unit">km/h</span>
      </span>
    </div>
  );
}

/**
 * Quiet line while easy stop eases the belt down before stopping it, so a
 * slowing belt never looks like a fault. Rust decides when
 * (`easy_stop.easing`) and gives the countdown; its sentence is the tooltip.
 */
export function EasyStopLine({ snapshot }: { snapshot: AppSnapshot }) {
  const e = snapshot.easy_stop;
  if (!e.easing || e.to_kmh === null) return null;
  return (
    <div className="hud-easystop accent-yellow" data-testid="hud-easystop" title={e.message ?? undefined}>
      <span className="hud-label">EASY STOP</span>
      <span className="hud-easystop-speed">
        to {e.to_kmh.toFixed(1)}
        <span className="hud-unit">km/h</span>
        {e.stop_in_s !== null && <span className="hud-unit">{e.stop_in_s} s</span>}
      </span>
    </div>
  );
}

/** The speed Quiet now eases to: the quiet setting, never under the deck's slowest. */
function quietKmh(snapshot: AppSnapshot): number {
  return Math.max(snapshot.calls.quiet_kmh, snapshot.deck.min_speed_kmh);
}

/**
 * Quiet line while the belt is held at the quiet speed, by Quiet now or by a
 * call (`quietHold`). A speed set by hand, a program step or a stop ends a
 * press's hold, the call's end puts the speed back, and the line goes.
 */
export function QuietNowLine({ snapshot }: { snapshot: AppSnapshot }) {
  const hold = quietHold(snapshot);
  if (!hold) return null;
  const back = hold.back;
  return (
    <div
      className="hud-quiet accent-yellow"
      data-testid="hud-quiet"
      title={hold.by === "call" ? `After a call, the belt goes back to ${back.toFixed(1)} km/h.` : `QUIET puts ${back.toFixed(1)} km/h back.`}
    >
      <span className="hud-label">QUIET // DAMPED</span>
      <span className="hud-quiet-speed">
        {quietKmh(snapshot).toFixed(1)}
        <span className="hud-unit">/{back.toFixed(1)} km/h</span>
      </span>
    </div>
  );
}

/**
 * A folded ATROPHY_WARNING on the whole panel (#422): the card showed once and
 * went unanswered, so the seated time stays in the status row, last in line.
 * The strip shows the same readout (`SeatedReadout`).
 */
export function SeatedLine({ snapshot }: { snapshot: AppSnapshot }) {
  if (!snapshot.seated.folded) return null;
  return (
    <div className="hud-seated-line accent-yellow" data-testid="hud-seated-line" title="Minutes at the PC since the last walk of 2 minutes or more.">
      <span className="hud-label">ATROPHY_WARNING</span>
      <span className="hud-seated-mins">
        {snapshot.seated.seated_minutes}
        <span className="hud-unit">MIN IDLE</span>
      </span>
    </div>
  );
}

/**
 * Quiet line while the HUD holds its non-urgent cards and toasts back,
 * because Windows says a presentation or full-screen app is in front. Rust
 * decides when (`presenting.holding`); the held cards show once it ends.
 */
export function PresentingLine({ snapshot }: { snapshot: AppSnapshot }) {
  if (!snapshot.presenting.holding) return null;
  return (
    <div
      className="hud-presenting accent-cyan"
      data-testid="hud-presenting"
      title="Cards wait until the presentation or full-screen app closes."
    >
      <span className="hud-label">PRESENTING // CARDS HELD</span>
    </div>
  );
}

/**
 * Engage / Stasis: one `toggle_start` to Rust, the same press as the shortcut.
 * On a strip, where no program card shows, its GO takes the programs' yellow.
 */
export function EngageButton({
  snapshot,
  send,
  sub = false,
  strip = false,
}: {
  snapshot: AppSnapshot;
  send: Send;
  /** A plain line under the codename (the panel and both strips). */
  sub?: boolean;
  /** On the still strip or the running strip. */
  strip?: boolean;
}) {
  const go = strip && engageLabel(snapshot) === "GO";
  return (
    <button
      type="button"
      className={`hud-btn hud-btn-main${sub ? " hud-btn-stack" : ""} accent-${go ? "yellow" : "cyan"}`}
      disabled={!snapshot.connection.connected}
      title={engageTitle(snapshot)}
      onClick={() => void send({ type: "toggle_start" })}
    >
      {/* Every word and line it can take share one cell each, so it keeps one size and DUMP never moves. */}
      <Swap shown={engageLabel(snapshot)} all={ENGAGE_WORDS} />
      {sub && " "}
      {sub && <Swap className="hud-btn-sub" shown={engageSub(snapshot)} all={ENGAGE_SUBS} tight={[WAKE_FIRST_SUB]} />}
    </button>
  );
}

/**
 * QUIET: Quiet now, one `quiet_now` to Rust. The first press eases a running
 * belt to the quiet speed and the second puts the speed back. Rust refuses a
 * still belt or one already as slow, and says why in the panel's note. While
 * the belt is held quiet, by a press or by a call, the button is filled and
 * its line says what a press does: put the speed back, or during a call keep
 * the belt slow after the call (Rust takes the call's hold over). Every line
 * shares one cell, so the button keeps its width and nothing beside it moves.
 */
/** QUIET's plain lines; a press's "back 3.5" takes the room of "back 00.0". */
const QUIET_SUBS = ["slow down", "keep slow", "back 00.0"] as const;

function QuietButton({ snapshot, send }: { snapshot: AppSnapshot; send: Send }) {
  const hold = quietHold(snapshot);
  const quiet = quietKmh(snapshot).toFixed(1);
  const back = hold?.back.toFixed(1) ?? "";
  const title =
    hold?.by === "press"
      ? `QUIET holds the belt at ${quiet} km/h. Press again for ${back} km/h.`
      : hold?.by === "call"
        ? `A call holds the belt at ${quiet} km/h. Press to keep ${quiet} km/h after the call.`
        : `Ease the belt to ${quiet} km/h.`;
  const sub = hold?.by === "press" ? `back ${back}` : hold?.by === "call" ? "keep slow" : "slow down";
  return (
    <button
      type="button"
      className={`hud-btn hud-btn-quiet hud-btn-stack accent-yellow${hold ? " is-on" : ""}`}
      aria-pressed={hold !== null}
      disabled={!snapshot.connection.connected}
      title={title}
      onClick={() => void send({ type: "quiet_now" })}
    >
      QUIET{" "}
      <Swap className="hud-btn-sub" shown={sub} all={QUIET_SUBS} />
    </button>
  );
}

/**
 * Engage / Stasis, Dump, QUIET and the speed and tilt steppers. A refused
 * press, a deck reply or the link being down shows in the status row over
 * them (`StatusRow`), never between them.
 */
export function Controls({ snapshot, send }: { snapshot: AppSnapshot; send: Send }) {
  const { deck, connection } = snapshot;
  // Every button sends one command to Rust. Nothing here derives the next value
  // from what is on screen, so rapid clicks can't collapse into one step.
  return (
    <div className="hud-module">
      <div className="hud-transport">
        {/* Engage keeps its slot in every state, GO included, so a press from habit never lands on DUMP. */}
        <EngageButton snapshot={snapshot} send={send} sub />
        <button
          type="button"
          className="hud-btn hud-btn-stop hud-btn-dump hud-btn-stack"
          disabled={!connection.connected || deck.status === "Stopped" || deck.status === "Stopping"}
          title={dumpTitle(snapshot)}
          onClick={() => send({ type: "stop" })}
        >
          <DumpName />{" "}
          <span className="hud-btn-sub">{snapshot.easy_stop.easing ? "stop now" : "end walk"}</span>
        </button>
        <QuietButton snapshot={snapshot} send={send} />
      </div>
      <div className="hud-steppers">
        <Stepper
          name="VEL"
          label="VEL TARGET"
          accent="cyan"
          disabled={!connection.connected}
          value={deck.target_speed_kmh.toFixed(1)}
          onDown={() => send({ type: "nudge_speed", up: false })}
          onUp={() => send({ type: "nudge_speed", up: true })}
          wheelHint="Scroll to change the target speed."
        />
        <Stepper
          name="TILT"
          label="TILT TARGET"
          accent="magenta"
          disabled={!connection.connected}
          value={deck.target_incline_pct.toFixed(1)}
          onDown={() => send({ type: "nudge_incline", up: false })}
          onUp={() => send({ type: "nudge_incline", up: true })}
          wheelHint="Scroll to change the target incline."
        />
      </div>
    </div>
  );
}

/** Every word and plain line HOLD / GO takes, so the button keeps one size in every state. */
const HOLD_WORDS = ["HOLD", "GO"] as const;
const HOLD_SUBS = ["pause program", "resume program", "start belt", WAKE_FIRST_SUB] as const;

/** STEP_MAP's next step: each part stays whole and keeps its trailing dot; a narrow panel wraps between them. */
function NextLine({ next }: { next: string }) {
  return (
    <span className="hud-sub hud-next">
      {next.split(" · ").map((part, i, all) => (
        <Fragment key={i}>
          {/* A flex gap draws the space; the text keeps it for copying and screen readers. */}
          {i > 0 && " "}
          <span className="hud-next-part">
            {part}
            {i < all.length - 1 ? " ·" : ""}
          </span>
        </Fragment>
      ))}
    </span>
  );
}

/**
 * HOLD / GO, SKIP and END. `a` null draws them for the reserve, unlabelled by state.
 * `asleep`: the deck refuses starts, so GO says to wake it first (the press is the same).
 */
function ProgramActions({ a, offline, asleep = false, send }: { a: ActiveProgram | null; offline: boolean; asleep?: boolean; send: Send }) {
  const wait = a?.wait ?? null;
  const paused = a?.paused ?? false;
  return (
    <div className="hud-auto-actions">
      <SmallButton
        className={wait ? "hud-btn-go" : undefined}
        disabled={offline}
        sub={paused && asleep ? WAKE_FIRST_SUB : wait ? "start belt" : paused ? "resume program" : "pause program"}
        subs={HOLD_SUBS}
        tight={[WAKE_FIRST_SUB]}
        title={
          offline
            ? "Treadmill not connected."
            : paused && asleep
              ? WAKE_FIRST
              : wait
              ? "Start the belt and carry on with the program."
              : paused
                ? "Resume the program."
                : "Hold the program."
        }
        onClick={() => send({ type: paused ? "program_resume" : "program_pause" })}
      >
        <Swap shown={paused ? "GO" : "HOLD"} all={HOLD_WORDS} />
      </SmallButton>
      <SmallButton
        disabled={offline}
        sub="next step"
        title={offline ? "Treadmill not connected." : "Skip to the next step."}
        onClick={() => send({ type: "program_skip" })}
      >
        SKIP
      </SmallButton>
      <SmallButton sub="end program" title="End the program." onClick={() => send({ type: "program_end" })}>
        END
      </SmallButton>
    </div>
  );
}

/** The running program: what it does now over its three buttons. */
function RunLayer({ a, offline, asleep, send }: { a: ActiveProgram; offline: boolean; asleep: boolean; send: Send }) {
  // Waiting for GO: the next step would start a still belt, so nothing moves
  // until GO (Rust says why in `wait`).
  const wait = a.wait;
  // One line for the step after this one keeps the panel small: a chart would
  // need its own height and still be too small to read at 340 px.
  const next = nextStepLine(a);
  const timeLeft = describeTimeLeft(a);
  // Every line whole, for the tooltip: a larger text size may cut one short.
  const lines = [wait ? waitHeadline(wait) : a.paused ? "PAUSED" : "PROGRAM", a.name, describeActive(a), timeLeft, next].filter(
    (l): l is string => !!l,
  );
  return (
    <div className="hud-program-run">
      {/* The readout shares its cell with a blank one, so the buttons under it keep their place. */}
      <div className="hud-program-readout">
        <ReadoutReserve />
        <div className="hud-auto-readout" title={lines.join(" · ")}>
          {/* Waiting for GO: the headline is the one prompt, in place of the label. */}
          {wait ? (
            <span className="hud-ready-headline">{waitHeadline(wait)}</span>
          ) : (
            <span className="hud-label">{a.paused ? "PAUSED" : "PROGRAM"}</span>
          )}
          <span className="hud-value hud-value-sm">{a.name}</span>
          <span className="hud-sub hud-run-status">{describeActive(a)}</span>
          {timeLeft && (
            <span className="hud-sub hud-t-minus" data-testid="hud-t-minus">
              {timeLeft}
            </span>
          )}
          {next && <NextLine next={next} />}
        </div>
      </div>
      <ProgramActions a={a} offline={offline} asleep={asleep} send={send} />
    </div>
  );
}

/** A no-break space: a blank line that still takes its height. */
const BLANK = "\u00a0";

/** A running program's readout with blank lines: a headline, the name and its three lines. */
function ReadoutReserve() {
  return (
    <div className="hud-auto-readout hud-reserve" aria-hidden="true">
      <span className="hud-ready-headline">{BLANK}</span>
      <span className="hud-value hud-value-sm">{BLANK}</span>
      <span className="hud-sub">{BLANK}</span>
      <span className="hud-sub">{BLANK}</span>
      <span className="hud-sub">{BLANK}</span>
    </div>
  );
}

/**
 * A blank running program over its three buttons. Laid unseen under whatever
 * the module shows, so the module is never shorter than a running program and
 * nothing moves when one starts, waits, rests or ends.
 */
function RunReserve() {
  return (
    <div className="hud-program-run hud-reserve" aria-hidden="true" inert>
      <ReadoutReserve />
      <ProgramActions a={null} offline send={() => Promise.resolve(false)} />
    </div>
  );
}

/** One button per saved program (up to six); one tap starts it. */
function ProgramGrid({
  programs,
  offline,
  send,
  reserve = false,
}: {
  programs: AppSnapshot["programs"];
  offline: boolean;
  send: Send;
  /** Laid unseen under a running program, so the module keeps the grid's height. */
  reserve?: boolean;
}) {
  return (
    <div className={`hud-program-grid${reserve ? " hud-reserve" : ""}`} aria-hidden={reserve || undefined} inert={reserve || undefined}>
      <span className="hud-label">RUN PROGRAM</span>
      <div className="hud-programs">
        {programs.slice(0, 6).map((p) => (
          <button
            key={p.id}
            type="button"
            className="hud-btn hud-btn-program hud-btn-stack"
            disabled={offline || reserve}
            title={`${p.name}: ${describePlan(p)}`}
            onClick={() => send({ type: "program_run", id: p.id })}
          >
            <span className="hud-btn-program-name">{p.name}</span>{" "}
            <span className="hud-btn-sub">{firstStepLine(p)}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

/**
 * The running program's controls, or one button per saved program. Both
 * shapes, and a blank running program, share one cell. While the panel grows
 * upward (`reserve`: a bottom corner, or a dragged spot pinned to the
 * screen's foot) the cell is as tall as the tallest of them in every state,
 * so nothing over it moves when a program starts, waits for GO, rests, pauses
 * or ends. In a top corner the module sits under every other control, so the
 * cell takes only what shows and the panel's foot may move.
 */
export function ProgramsModule({ snapshot, send, reserve }: { snapshot: AppSnapshot; send: Send; reserve: boolean }) {
  const a = snapshot.active_program;
  // HOLD, GO and SKIP act on the walk, which needs the treadmill; END always works.
  const offline = !snapshot.connection.connected;
  const programs = snapshot.programs;
  if (!a && programs.length === 0) return null;
  const wait = a?.wait ?? null;
  return (
    <div className={`hud-module hud-program accent-yellow${wait ? " hud-ready" : ""}`} role={wait ? "alert" : undefined}>
      <div className={`hud-program-stack${reserve ? " is-reserved" : ""}`}>
        <RunReserve />
        {programs.length > 0 && <ProgramGrid programs={programs} offline={offline} send={send} reserve={a !== null} />}
        {a && <RunLayer a={a} offline={offline} asleep={deckAsleep(snapshot)} send={send} />}
      </div>
    </div>
  );
}

/** Every word the status row's link button takes, so it keeps one size. */
const CONNECT_WORDS = ["CONNECT", "RETRY NOW", "LINKING…"] as const;

/** Why the belt and program buttons are off while no treadmill is linked, with
 *  the same relink the header's link label does. It only links; it never starts the belt. */
function OfflineLine({ connection, relink }: { connection: AppSnapshot["connection"]; relink: ReturnType<typeof useRelink> }) {
  const pending = relink.view?.text != null;
  // While RELINK counts down to its own retry, a press only brings it forward.
  const retrying = linkState(connection).state === "reconnecting";
  return (
    <div className="hud-offline accent-cyan" data-testid="hud-offline" title={linkView(connection).title}>
      <span className="hud-offline-text">{offlineText(connection)}</span>
      {(relink.view?.clickable || pending) && (
        <button type="button" className="hud-btn hud-btn-connect" disabled={pending} onClick={relink.relink}>
          <Swap shown={pending ? "LINKING…" : retrying ? "RETRY NOW" : "CONNECT"} all={CONNECT_WORDS} />
        </button>
      )}
    </div>
  );
}

/**
 * The whole panel's status row, between the readings and the controls. It
 * always takes one line, so nothing above or under it moves: the offline line
 * first, else the most pressing quiet line (EASY STOP, QUIET // DAMPED, ABOVE
 * STRIDE_FIT, NEURAL_FLUIDITY, SPEED DRIFT, PRESENTING; hud.css shows only the
 * first), else nothing. A note covers it, and the readings over it only when
 * it needs more than one line; never the controls (`NotePop`).
 */
export function StatusRow({
  snapshot,
  relink,
  note,
}: {
  snapshot: AppSnapshot;
  relink: ReturnType<typeof useRelink>;
  note: PanelNote | null;
}) {
  return (
    <div className="hud-status-row" data-testid="hud-status-row" title={note?.detail ?? undefined}>
      <div className="hud-status-line">
        {!snapshot.connection.connected && <OfflineLine connection={snapshot.connection} relink={relink} />}
        <EasyStopLine snapshot={snapshot} />
        <QuietNowLine snapshot={snapshot} />
        <StrideFitLine snapshot={snapshot} />
        <NeuralLine snapshot={snapshot} />
        <SpeedDriftLine snapshot={snapshot} />
        <PresentingLine snapshot={snapshot} />
        <SeatedLine snapshot={snapshot} />
      </div>
      <NotePop note={note} />
    </div>
  );
}
