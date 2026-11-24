import { Fragment } from "react";
import type { AppSnapshot, Command, HudWidget, PulseTarget, ZoneBand } from "../ipc";
import { describeActive, describePlan, describeTimeLeft, waitHeadline, waitReason } from "../dashboard/programs/format";
import { nextStepLine } from "../dashboard/programs/stepMap";
import { statWidget } from "../widgets/registry";
import { Sparkline } from "../widgets/Sparkline";
import { targetAccent, targetRange, ZONE_ACCENT, zoneRange } from "../widgets/zone";
import { HudNote } from "./HudNote";
import { isMoving, startFailure } from "../state/linkStatus";
import { SmallButton } from "./SmallButton";
import { engageLabel, engageSub, engageTitle } from "./status";
import { useHudSend } from "./useHudSend";
import { useRelink } from "./useRelink";
import { linkState } from "../state/linkStatus";

/** One pinned stat, read through the shared widget registry. */
export function StatCell({ id, snapshot }: { id: HudWidget; snapshot: AppSnapshot }) {
  const def = statWidget(id);
  if (!def) return null;
  const { value, unit } = def.read(snapshot);
  const spark = def.spark?.(snapshot);
  const zone = def.zone?.(snapshot);
  const target = def.target?.(snapshot);
  const detail = def.detail?.(snapshot);
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
      {detail && <span className="hud-sub hud-stat-detail">{detail}</span>}
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
  label,
  value,
  accent,
  disabled,
  onDown,
  onUp,
}: {
  label: string;
  value: string;
  accent: "cyan" | "magenta";
  disabled: boolean;
  onDown: () => void;
  onUp: () => void;
}) {
  return (
    <div className={`hud-stepper accent-${accent}`}>
      <span className="hud-label">{label}</span>
      <div className="hud-stepper-row">
        <button type="button" className="hud-btn hud-btn-step" disabled={disabled} onClick={onDown} aria-label={`${label} down`}>
          −
        </button>
        <span className="hud-stepper-value">{value}</span>
        <button type="button" className="hud-btn hud-btn-step" disabled={disabled} onClick={onUp} aria-label={`${label} up`}>
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
    <div className="hud-stride accent-yellow" data-testid="hud-stride" title={`The belt runs faster than STRIDE_FIT ${fit.toFixed(1)} km/h.`}>
      <span className="hud-label">ABOVE STRIDE_FIT</span>
      <span className="hud-stride-speed">
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
    <div className="hud-easystop accent-magenta" data-testid="hud-easystop" title={e.message ?? undefined}>
      <span className="hud-label">EASY STOP</span>
      <span className="hud-easystop-speed">
        to {e.to_kmh.toFixed(1)}
        <span className="hud-unit">km/h</span>
        {e.stop_in_s !== null && <span className="hud-unit">{e.stop_in_s} s</span>}
      </span>
    </div>
  );
}

/** Engage / Stasis: one `toggle_start` to Rust, the same press as the shortcut. */
export function EngageButton({
  snapshot,
  send,
  sub = false,
}: {
  snapshot: AppSnapshot;
  send: (c: Command) => Promise<boolean>;
  /** The panel adds a plain line under the codename; the strips are too short for it. */
  sub?: boolean;
}) {
  const moving = isMoving(snapshot.deck);
  return (
    <button
      type="button"
      className={`hud-btn hud-btn-main${sub ? " hud-btn-stack" : ""} ${moving ? "accent-magenta" : "accent-cyan"}`}
      disabled={!snapshot.connection.connected}
      title={engageTitle(snapshot)}
      onClick={() => void send({ type: "toggle_start" })}
    >
      {engageLabel(snapshot)}
      {sub && " "}
      {sub && <span className="hud-btn-sub">{engageSub(snapshot)}</span>}
    </button>
  );
}

/** Engage / Stasis, Dump and the speed and tilt steppers. */
export function Controls({ snapshot }: { snapshot: AppSnapshot }) {
  const { send, error } = useHudSend();
  const { deck, connection } = snapshot;
  // Every button sends one command to Rust. Nothing here derives the next value
  // from what is on screen, so rapid clicks can't collapse into one step.
  return (
    <div className="hud-module">
      <div className="hud-transport">
        <EngageButton snapshot={snapshot} send={send} sub />
        <button
          type="button"
          className="hud-btn hud-btn-stop hud-btn-stack accent-magenta"
          disabled={!connection.connected || deck.status === "Stopped" || deck.status === "Stopping"}
          title={snapshot.easy_stop.easing ? "Stop the belt at once." : "Stop the belt and end the session."}
          onClick={() => send({ type: "stop" })}
        >
          DUMP{" "}
          <span className="hud-btn-sub">{snapshot.easy_stop.easing ? "stop now" : "end walk"}</span>
        </button>
      </div>
      <div className="hud-steppers">
        <Stepper
          label="VEL"
          accent="cyan"
          disabled={!connection.connected}
          value={deck.target_speed_kmh.toFixed(1)}
          onDown={() => send({ type: "nudge_speed", up: false })}
          onUp={() => send({ type: "nudge_speed", up: true })}
        />
        <Stepper
          label="TILT"
          accent="magenta"
          disabled={!connection.connected}
          value={deck.target_incline_pct.toFixed(1)}
          onDown={() => send({ type: "nudge_incline", up: false })}
          onUp={() => send({ type: "nudge_incline", up: true })}
        />
      </div>
      <HudNote error={error ?? startFailure(snapshot) ?? deck.target_note} />
    </div>
  );
}

/** The running program's controls, or one button per saved program. */
export function ProgramsModule({ snapshot }: { snapshot: AppSnapshot }) {
  const { send, error } = useHudSend();
  const relink = useRelink(snapshot.connection);
  const a = snapshot.active_program;
  if (a) {
    // Waiting for GO: the next step would start a still belt, so nothing moves
    // until GO (Rust says why in `wait`).
    const wait = a.wait;
    // One line for the step after this one keeps the panel small: a chart would
    // need its own height and still be too small to read at 340 px.
    const next = nextStepLine(a);
    const timeLeft = describeTimeLeft(a);
    return (
      <div className={`hud-module accent-yellow${wait ? " hud-ready" : ""}`} role={wait ? "alert" : undefined}>
        {wait && <span className="hud-ready-headline">{waitHeadline(wait)}</span>}
        <div className="hud-auto-readout">
          <span className="hud-label">{wait ? "READY" : a.paused ? "PAUSED" : "PROGRAM"}</span>
          <span className="hud-value hud-value-sm">{a.name}</span>
          <span className="hud-sub hud-run-status">{describeActive(a)}</span>
          {timeLeft && (
            <span className="hud-sub hud-t-minus" data-testid="hud-t-minus">
              {timeLeft}
            </span>
          )}
          {next && (
            <span className="hud-sub hud-next">
              {/* Each part stays whole and keeps its trailing dot; a narrow panel wraps between them. */}
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
          )}
          {wait && <span className="hud-sub">{waitReason(wait)}</span>}
        </div>
        <div className="hud-auto-actions">
          <SmallButton
            className={wait ? "hud-btn-go" : undefined}
            sub={a.paused ? "resume" : "pause"}
            title={a.paused ? "Resume the program." : "Pause the program."}
            onClick={() => send({ type: a.paused ? "program_resume" : "program_pause" })}
          >
            {a.paused ? "GO" : "HOLD"}
          </SmallButton>
          <SmallButton sub="next step" title="Skip to the next step." onClick={() => send({ type: "program_skip" })}>
            SKIP
          </SmallButton>
          <SmallButton sub="end program" title="End the program." onClick={() => send({ type: "program_end" })}>
            END
          </SmallButton>
        </div>
        <HudNote error={error} />
      </div>
    );
  }
  if (snapshot.programs.length === 0) return null;
  // One tap starts a program; the panel switches to its status row.
  return (
    <div className="hud-module accent-yellow">
      <span className="hud-label">RUN PROGRAM</span>
      {!snapshot.connection.connected && <OfflineLine connection={snapshot.connection} relink={relink} />}
      <div className="hud-programs">
        {snapshot.programs.slice(0, 6).map((p) => (
          <button
            key={p.id}
            type="button"
            className="hud-btn hud-btn-program"
            disabled={!snapshot.connection.connected}
            title={describePlan(p)}
            onClick={() => send({ type: "program_run", id: p.id })}
          >
            {p.name}
          </button>
        ))}
      </div>
      <HudNote error={error ?? relink.error} />
    </div>
  );
}

/** Why the program buttons are off while no treadmill is linked, with the same
 *  relink the header's link label does. It only links; it never starts the belt. */
function OfflineLine({ connection, relink }: { connection: AppSnapshot["connection"]; relink: ReturnType<typeof useRelink> }) {
  const state = linkState(connection).state;
  const linking = state === "connecting" || state === "scanning";
  const pending = relink.view?.text != null;
  return (
    <div className="hud-offline" data-testid="hud-offline">
      <span className="hud-sub">{linking ? "Connecting to the treadmill." : "Treadmill not connected."}</span>
      {(relink.view?.clickable || pending) && (
        <button type="button" className="hud-btn hud-btn-sm" disabled={pending} onClick={relink.relink}>
          {pending ? "LINKING…" : "CONNECT"}
        </button>
      )}
    </div>
  );
}

