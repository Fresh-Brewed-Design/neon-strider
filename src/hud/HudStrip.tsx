import { Fragment, useLayoutEffect, useRef } from "react";
import type { AppSnapshot, Command, HudWidget } from "../ipc";
import { describeActive } from "../dashboard/programs/format";
import { formatClock } from "../format";
import { statTitle, statWidget } from "../widgets/registry";
import { Sparkline } from "../widgets/Sparkline";
import { NotePop, type PanelNote } from "./HudNote";
import { EngageButton, STASIS_CLOCK_TITLE, StasisClock } from "./HudPanel";
import { DumpName } from "./DumpName";
import { dumpTitle, stasisClockOf } from "./status";

type Send = (c: Command) => Promise<boolean>;

/**
 * A program's rest in the strip's stat slot: the program's name over the time
 * to its next walk, in the programs' yellow. A rest outranks today's total
 * there: it is what the still belt is doing. Same label-over-value shape as
 * the total, so Engage beside it keeps its place.
 */
function RestCell({ name, left }: { name: string; left: number }) {
  const clock = formatClock(left);
  return (
    <div className="hud-strip-total hud-strip-rest accent-yellow" title={`${name} · Rest · next walk in ${clock}`} data-testid="hud-strip-rest">
      <span className="hud-label">{name} // REST</span>
      <span className="hud-value hud-value-sm">{clock}</span>
    </div>
  );
}

/**
 * The panel collapsed during stasis (`hud_view` "strip"): under the header
 * (whose expand button shows the whole panel until the belt next moves),
 * today's total (DAILY_TOTAL, today's walking time beside its label) and
 * Engage. A note covers the slot and the
 * logo above it, beside Engage, so Engage never moves for it. The slot shows,
 * first that applies: a program's rest and its countdown (`RestCell`), a
 * folded ATROPHY_WARNING card (`SeatedReadout`: today's total can't change
 * while the belt is still; a walk lets the card go), the STASIS clock while
 * the belt is paused (`StasisCell`), or today's total.
 */
export function StripRow({ snapshot, send, note }: { snapshot: AppSnapshot; send: Send; note: PanelNote | null }) {
  const daily = statWidget("DAILY");
  const total = daily?.read(snapshot);
  const time = daily?.detail?.(snapshot);
  const a = snapshot.active_program;
  const rest = a?.phase === "resting" && a.seconds_remaining !== null ? { name: a.name, left: a.seconds_remaining } : null;
  const clock = stasisClockOf(snapshot);
  return (
    <div className="hud-strip">
      <div className="hud-strip-row">
        <div className="hud-strip-cell">
          {rest ? (
            <RestCell name={rest.name} left={rest.left} />
          ) : snapshot.seated.folded ? (
            <SeatedReadout snapshot={snapshot} />
          ) : clock !== null ? (
            <StasisCell clock={clock} />
          ) : (
            daily &&
            total && (
              <div className={`hud-strip-total accent-${daily.accent}`} title={statTitle(daily)}>
                <span className="hud-strip-head">
                  <span className="hud-label">{daily.label}</span>
                  {time && (
                    <span className="hud-sub hud-strip-time" data-testid="hud-strip-time">
                      {time}
                    </span>
                  )}
                </span>
                <span className="hud-value hud-value-sm">
                  {total.value}
                  {total.unit && <span className="hud-unit">{total.unit}</span>}
                </span>
              </div>
            )
          )}
          <NotePop note={note} />
        </div>
        <EngageButton snapshot={snapshot} send={send} sub strip />
      </div>
    </div>
  );
}

/**
 * The STASIS clock in today's total's place while the belt is paused: how
 * long the pause has lasted. Same label-over-value shape as the total, so
 * Engage beside it keeps its place and RE-JACK keeps its plain line.
 */
function StasisCell({ clock }: { clock: string }) {
  return (
    <div className="hud-strip-total hud-strip-stasis accent-cyan" data-testid="hud-strip-stasis" title={STASIS_CLOCK_TITLE}>
      <span className="hud-label">STASIS</span>{" "}
      <span className="hud-value hud-value-sm">
        <StasisClock clock={clock} />
      </span>
    </div>
  );
}

/**
 * ATROPHY_WARNING folded into the strip (#422): the card showed once and went
 * unanswered, so the seated time stays in today's total's place, in yellow,
 * until a walk. Rust decides when (`seated.folded`, `card_queue.rs`). Engage
 * keeps its place: the label shortens first.
 */
export function SeatedReadout({ snapshot }: { snapshot: AppSnapshot }) {
  const s = snapshot.seated;
  if (!s.folded) return null;
  return (
    <div className="hud-strip-total hud-seated-readout accent-yellow" data-testid="hud-seated-readout" title="Minutes at the PC since the last walk of 2 minutes or more.">
      <span className="hud-label">ATROPHY_WARNING</span>
      <span className="hud-value hud-value-sm">
        {s.seated_minutes}
        <span className="hud-unit">MIN IDLE</span>
      </span>
    </div>
  );
}

const RUN_STATS: HudWidget[] = ["VELOCITY", "TIMER", "DISTANCE"];

/** The program line as whole segments. */
function programSegments(name: string, detail: string): string[] {
  return [name, ...detail.split(" · ")];
}

/**
 * The program line: whole segments wrap, and a segment's trailing dot is
 * hidden when the next segment sits on the line below, so no line ends in one.
 */
function ProgramLine({ name, detail }: { name: string; detail: string }) {
  const ref = useRef<HTMLSpanElement>(null);
  const segs = programSegments(name, detail);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const mark = () => {
      const items = Array.from(el.querySelectorAll<HTMLElement>(".hud-run-seg"));
      items.forEach((item, i) => {
        const next = items[i + 1];
        item.toggleAttribute("data-wrapped", !!next && next.offsetTop > item.offsetTop);
      });
    };
    mark();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(mark);
    ro.observe(el);
    return () => ro.disconnect();
  });
  return (
    <span ref={ref} className="hud-sub hud-run-segments">
      {segs.map((seg, i) => (
        <Fragment key={i}>
          {i > 0 && " "}
          <span className={i === 0 ? "hud-run-seg hud-run-seg-name" : "hud-run-seg"} title={i === 0 ? name : undefined}>
            {seg}
            {i < segs.length - 1 && <span className="hud-run-dot"> ·</span>}
          </span>
        </Fragment>
      ))}
    </span>
  );
}

/**
 * PULSE_RATE on the running strip, only while a heart rate sensor is picked:
 * the bpm, then its spark chart in the rest of the row. Without a sensor the
 * strip stays as it was.
 */
function PulseRow({ snapshot }: { snapshot: AppSnapshot }) {
  const def = statWidget("HEART_RATE");
  if (!def || snapshot.heart_rate.sensor === null) return null;
  const { value, unit } = def.read(snapshot);
  const spark = def.spark?.(snapshot);
  return (
    <div className={`hud-run-pulse accent-${def.accent}`} title={statTitle(def)} data-testid="hud-run-pulse">
      <div className="hud-strip-total">
        <span className="hud-label">{def.label}</span>
        <span className="hud-value hud-value-sm">
          {value}
          {unit && <span className="hud-unit">{unit}</span>}
        </span>
      </div>
      {spark && <Sparkline data={spark} className="hud-spark hud-run-spark" />}
    </div>
  );
}

/**
 * The panel minified by the user (`hud_view` "minified"): speed, time and
 * distance, heart rate with a sensor picked, the program step (its room kept
 * while none runs),
 * then Engage/Stasis and Dump with their plain lines, as on the whole panel.
 * Expand is in the header. A note covers the readings over the buttons, so the
 * buttons never move for it. It stays until expanded again.
 */
export function RunStripRow({ snapshot, send, note }: { snapshot: AppSnapshot; send: Send; note: PanelNote | null }) {
  const { deck, connection } = snapshot;
  const a = snapshot.active_program;
  const stopped = deck.status === "Stopped" || deck.status === "Stopping";
  const clock = stasisClockOf(snapshot);
  return (
    <div className="hud-strip hud-run-strip">
      <div className="hud-run-stats">
        {RUN_STATS.map((id) => {
          const def = statWidget(id);
          if (!def) return null;
          const { value, unit } = def.read(snapshot);
          return (
            <div key={id} className={`hud-strip-total accent-${def.accent}`} title={statTitle(def)}>
              <span className="hud-label">{def.label}</span>
              <span className="hud-value hud-value-sm">
                {value}
                {unit && <span className="hud-unit">{unit}</span>}
              </span>
            </div>
          );
        })}
      </div>
      <PulseRow snapshot={snapshot} />
      {/* With no program the line keeps its room, so STASIS and DUMP stay put when a program starts or ends.
          While the belt is paused the line carries the STASIS clock: after the program's label, or alone. */}
      {a ? (
        <div className="hud-run-program accent-yellow" title={`${a.name} · ${describeActive(a)}`}>
          <span className="hud-label">
            {a.paused ? "PAUSED" : "PROGRAM"}
            {clock !== null && " "}
            {clock !== null && (
              <span className="hud-run-clock" data-testid="hud-run-clock" title={STASIS_CLOCK_TITLE}>
                {clock}
              </span>
            )}
          </span>
          <ProgramLine name={a.name} detail={describeActive(a)} />
        </div>
      ) : clock !== null ? (
        <div className="hud-run-program hud-run-stasis accent-cyan" data-testid="hud-run-stasis" title={STASIS_CLOCK_TITLE}>
          <span className="hud-label">STASIS</span>{" "}
          <StasisClock clock={clock} />
        </div>
      ) : (
        <div className="hud-run-program hud-reserve" aria-hidden="true" />
      )}
      <div className="hud-strip-row">
        <NotePop note={note} />
        <EngageButton snapshot={snapshot} send={send} sub strip />
        <button
          type="button"
          className="hud-btn hud-btn-stop hud-btn-stack hud-btn-dump"
          disabled={!connection.connected || stopped}
          title={dumpTitle(snapshot)}
          onClick={() => void send({ type: "stop" })}
        >
          <DumpName />{" "}
          <span className="hud-btn-sub">{snapshot.easy_stop.easing ? "stop now" : "end walk"}</span>
        </button>
      </div>
    </div>
  );
}
