import type { AppSnapshot, Command, HudWidget, ZoneBand } from "../ipc";
import { describeActive, describePlan, waitHeadline, waitReason } from "../dashboard/programs/format";
import { statWidget } from "../widgets/registry";
import { Sparkline } from "../widgets/Sparkline";
import { ZONE_ACCENT, zoneRange } from "../widgets/zone";
import { HudNote } from "./HudNote";
import { isMoving, startFailure } from "../state/linkStatus";
import { SmallButton } from "./SmallButton";
import { engageLabel } from "./status";
import { useHudSend } from "./useHudSend";

/** One pinned stat, read through the shared widget registry. */
export function StatCell({ id, snapshot }: { id: HudWidget; snapshot: AppSnapshot }) {
  const def = statWidget(id);
  if (!def) return null;
  const { value, unit } = def.read(snapshot);
  const spark = def.spark?.(snapshot);
  const zone = def.zone?.(snapshot);
  return (
    <div className={`hud-stat accent-${def.accent}`}>
      <span className="hud-label">{def.label}</span>
      <span className="hud-value">
        {value}
        {unit && <span className="hud-unit">{unit}</span>}
      </span>
      {spark && <Sparkline data={spark} className="hud-spark" />}
      {zone && <ZoneLine zone={zone} />}
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

/** Engage / Stasis: one `toggle_start` to Rust, the same press as the shortcut. */
export function EngageButton({ snapshot, send }: { snapshot: AppSnapshot; send: (c: Command) => Promise<boolean> }) {
  const moving = isMoving(snapshot.deck);
  return (
    <button
      type="button"
      className={`hud-btn hud-btn-main ${moving ? "accent-magenta" : "accent-cyan"}`}
      disabled={!snapshot.connection.connected}
      onClick={() => void send({ type: "toggle_start" })}
    >
      {engageLabel(snapshot)}
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
        <EngageButton snapshot={snapshot} send={send} />
        <button
          type="button"
          className="hud-btn hud-btn-stop accent-magenta"
          disabled={!connection.connected || deck.status === "Stopped" || deck.status === "Stopping"}
          onClick={() => send({ type: "stop" })}
        >
          DUMP
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
  const a = snapshot.active_program;
  if (a) {
    // Waiting for GO: the next step would start a still belt, so nothing moves
    // until GO (Rust says why in `wait`).
    const wait = a.wait;
    return (
      <div className={`hud-module accent-yellow${wait ? " hud-ready" : ""}`} role={wait ? "alert" : undefined}>
        {wait && <span className="hud-ready-headline">{waitHeadline(wait)}</span>}
        <div className="hud-auto-readout">
          <span className="hud-label">{wait ? "READY" : a.paused ? "PAUSED" : "PROGRAM"}</span>
          <span className="hud-value hud-value-sm">{a.name}</span>
          <span className="hud-sub">{describeActive(a)}</span>
          {wait && <span className="hud-sub">{waitReason(wait)}</span>}
        </div>
        <div className="hud-auto-actions">
          <SmallButton
            className={wait ? "hud-btn-go" : undefined}
            onClick={() => send({ type: a.paused ? "program_resume" : "program_pause" })}
          >
            {a.paused ? "GO" : "HOLD"}
          </SmallButton>
          <SmallButton onClick={() => send({ type: "program_skip" })}>SKIP</SmallButton>
          <SmallButton onClick={() => send({ type: "program_end" })}>END</SmallButton>
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
      <HudNote error={error} />
    </div>
  );
}
