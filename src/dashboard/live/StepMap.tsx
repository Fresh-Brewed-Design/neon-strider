import { useEffect, useRef } from "react";
import type { ActiveProgram } from "../../ipc";
import { planRows, rowTime, stepChart, zoneText, type PlanRow, type StepChart } from "../programs/stepMap";
import { Label, Panel } from "../ui";
import { cx } from "../util";

// The chart's own units; the SVG stretches to the panel's width.
const W = 600;
const H = 52;

const num = (v: number) => (Number.isInteger(v) ? v.toString() : v.toFixed(1));

function Chart({ c }: { c: StepChart }) {
  return (
    <div className="shrink-0">
      <svg
        viewBox={`0 0 ${W} ${H}`}
        preserveAspectRatio="none"
        className="block h-[52px] w-full overflow-visible"
        role="img"
        aria-label="Program speed and incline, with a yellow line at the current point"
      >
        <defs>
          <pattern id="step-map-rest" width="8" height="8" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
            <line x1="0" y1="0" x2="0" y2="8" stroke="rgba(255,255,255,0.12)" strokeWidth="3" />
          </pattern>
        </defs>
        {c.blocks.map((b) =>
          b.rest ? (
            <rect key={b.key} x={b.x0} y={0} width={b.x1 - b.x0} height={H} fill="url(#step-map-rest)" />
          ) : (
            <rect
              key={b.key}
              x={b.x0}
              y={b.speedY ?? H}
              width={b.x1 - b.x0}
              height={H - (b.speedY ?? H)}
              fill={b.state === "current" ? "rgba(0,243,255,0.22)" : "rgba(0,243,255,0.1)"}
              strokeDasharray={b.open ? "4 3" : undefined}
            />
          ),
        )}
        <path d={c.speedPath} fill="none" stroke="#00f3ff" strokeWidth="2" vectorEffect="non-scaling-stroke" />
        <path d={c.inclinePath} fill="none" stroke="#ff00ff" strokeWidth="2" vectorEffect="non-scaling-stroke" />
        {/* What has run is dimmed up to the line. */}
        <rect x={0} y={0} width={c.hereX} height={H} fill="rgba(5,8,12,0.55)" />
        <line x1={c.hereX} y1={0} x2={c.hereX} y2={H} stroke="#fff300" strokeWidth="2" vectorEffect="non-scaling-stroke" />
      </svg>
    </div>
  );
}

function Row({ row }: { row: PlanRow }) {
  const current = row.state === "current";
  return (
    <li
      data-state={row.state}
      aria-current={current ? "step" : undefined}
      className={cx(
        "grid grid-cols-[1.5rem_minmax(0,1fr)_5.5rem_6.5rem_3.5rem_6rem] items-center gap-2 rounded px-1.5 font-mono text-[11px] leading-4",
        current ? "bg-neon-yellow/10 text-neon-yellow" : row.state === "done" ? "text-white/30" : "text-white/70",
      )}
    >
      <span>{current ? "▶" : row.number != null ? String(row.number).padStart(2, "0") : ""}</span>
      <span className="truncate font-bold tracking-[0.08em]">{row.code}</span>
      <span>{rowTime(row.minutes)}</span>
      <span>{row.speedKmh != null ? `${row.speedKmh.toFixed(1)} km/h` : "speed as before"}</span>
      <span>{row.inclinePct != null ? `${num(row.inclinePct)}%` : ""}</span>
      <span className="truncate">{row.zone ? zoneText(row.zone) : ""}</span>
    </li>
  );
}

/**
 * STEP_MAP on the Live page: the running program's steps and a stepped speed
 * and incline chart with a "you are here" line. It takes the stats' place and
 * the height left on the page (at least two list rows), so the page still fits
 * 760x600; the list scrolls inside it.
 */
export function StepMap({ a }: { a: ActiveProgram }) {
  const rows = planRows(a);
  const listRef = useRef<HTMLOListElement>(null);
  const currentKey = rows.find((r) => r.state === "current")?.key;

  // Keep the current step in view inside the list as the program moves on.
  useEffect(() => {
    const list = listRef.current;
    const row = list?.querySelector<HTMLElement>("[aria-current='step']");
    if (!list || !row) return;
    // The row above the current one stays in sight, so done steps show too.
    list.scrollTop = Math.max(0, row.offsetTop - row.offsetHeight);
  }, [currentKey]);

  const repeat = a.repeat_every_minutes;
  const chart = stepChart(a, W, H);
  return (
    <Panel className="flex min-h-[130px] flex-1 basis-0 flex-col gap-1.5 px-3 py-2">
      <div className="flex shrink-0 items-baseline gap-3 font-mono text-[11px]">
        <Label className="text-neon-yellow/80">STEP_MAP</Label>
        {repeat != null && (
          <span className="text-white/50">
            Cycle {a.cycle} · repeats every {num(repeat)} min
          </span>
        )}
        {/* Each axis top in its line's colour, so the labels double as the legend. */}
        <span className="ml-auto text-neon-cyan/80">{chart.maxSpeed.toFixed(1)} km/h</span>
        <span className="text-neon-magenta/80">{chart.maxIncline.toFixed(1)}% tilt</span>
      </div>
      <Chart c={chart} />
      <ol ref={listRef} aria-label="Program steps" className="relative min-h-0 flex-1 overflow-y-auto">
        {rows.map((r) => (
          <Row key={r.key} row={r} />
        ))}
      </ol>
    </Panel>
  );
}
