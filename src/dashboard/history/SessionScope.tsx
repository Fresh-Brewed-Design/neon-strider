import { useEffect, useMemo, useRef, useState } from "react";
import { SessionRepository } from "../../services/db/repositories/sessionRepository";
import type { Sample } from "../../services/db/schema";
import type { MergedSession } from "../../services/sessionMerge";
import { formatDuration } from "../../format";
import { heartAxis, heartRuns, scopePoints, type HeartPoint, type ScopePoint } from "./scope";

const HEIGHT = 150;
const SPEED = "#00f3ff";
const INCLINE = "#ff00ff";
const HEART = "#fff300";

function draw(canvas: HTMLCanvasElement, width: number, points: ScopePoint[], runs: HeartPoint[][]) {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const dpr = window.devicePixelRatio || 1;
  canvas.width = width * dpr;
  canvas.height = HEIGHT * dpr;
  canvas.style.width = `${width}px`;
  canvas.style.height = `${HEIGHT}px`;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, HEIGHT);

  const padTop = 18;
  const padBottom = 20;
  const plotH = HEIGHT - padTop - padBottom;

  ctx.strokeStyle = "rgba(255,255,255,0.06)";
  ctx.lineWidth = 1;
  ctx.beginPath();
  for (let i = 0; i <= 4; i++) {
    const y = padTop + (plotH * i) / 4;
    ctx.moveTo(0, y);
    ctx.lineTo(width, y);
  }
  ctx.stroke();

  const t0 = points[0].timestamp;
  const span = Math.max(1, points[points.length - 1].timestamp - t0);
  const maxSpeed = Math.max(4, ...points.map((p) => p.speedKmh)) * 1.1;
  const maxIncline = Math.max(5, ...points.map((p) => p.inclinePercent)) * 1.1;
  const x = (t: number) => ((t - t0) / span) * width;
  const y = (v: number, max: number) => padTop + plotH - (v / max) * plotH;

  const trace = (value: (p: ScopePoint) => number, max: number, color: string, fill: boolean) => {
    ctx.beginPath();
    points.forEach((p, i) => {
      if (i === 0) ctx.moveTo(x(p.timestamp), y(value(p), max));
      else ctx.lineTo(x(p.timestamp), y(value(p), max));
    });
    ctx.strokeStyle = color;
    ctx.lineWidth = 2;
    ctx.shadowColor = color;
    ctx.shadowBlur = 6;
    ctx.stroke();
    ctx.shadowBlur = 0;
    if (fill) {
      ctx.lineTo(x(points[points.length - 1].timestamp), padTop + plotH);
      ctx.lineTo(x(t0), padTop + plotH);
      ctx.closePath();
      ctx.fillStyle = "rgba(0,243,255,0.08)";
      ctx.fill();
    }
  };
  trace((p) => p.speedKmh, maxSpeed, SPEED, true);
  trace((p) => p.inclinePercent, maxIncline, INCLINE, false);

  // Heart rate on its own axis; each run of readings is drawn apart, so a gap stays a gap.
  const axis = heartAxis(runs);
  if (axis) {
    const yHeart = (bpm: number) => padTop + plotH - ((bpm - axis.min) / (axis.max - axis.min)) * plotH;
    ctx.strokeStyle = HEART;
    ctx.fillStyle = HEART;
    ctx.lineWidth = 2;
    ctx.shadowColor = HEART;
    ctx.shadowBlur = 6;
    for (const run of runs) {
      if (run.length === 1) {
        ctx.beginPath();
        ctx.arc(x(run[0].timestamp), yHeart(run[0].bpm), 1.5, 0, Math.PI * 2);
        ctx.fill();
        continue;
      }
      ctx.beginPath();
      run.forEach((p, i) => {
        if (i === 0) ctx.moveTo(x(p.timestamp), yHeart(p.bpm));
        else ctx.lineTo(x(p.timestamp), yHeart(p.bpm));
      });
      ctx.stroke();
    }
    ctx.shadowBlur = 0;
  }

  ctx.font = "11px 'Courier New', monospace";
  ctx.textBaseline = "alphabetic";
  ctx.fillStyle = "rgba(0,243,255,0.8)";
  ctx.textAlign = "left";
  ctx.fillText(`${maxSpeed.toFixed(1)} km/h`, 2, 12);
  ctx.fillStyle = "rgba(255,0,255,0.8)";
  ctx.textAlign = "right";
  ctx.fillText(`${maxIncline.toFixed(1)}% tilt`, width - 2, 12);
  ctx.fillStyle = "rgba(255,255,255,0.45)";
  ctx.textAlign = "left";
  ctx.fillText("0:00", 2, HEIGHT - 4);
  ctx.textAlign = "right";
  ctx.fillText(formatDuration(span / 1000), width - 2, HEIGHT - 4);
  if (axis) {
    ctx.fillStyle = "rgba(255,243,0,0.8)";
    ctx.textAlign = "center";
    ctx.fillText(`${axis.max} BPM`, width / 2, 12);
    ctx.fillText(`${axis.min} BPM`, width / 2, HEIGHT - 4);
  }
}

/** Speed, incline and (when saved) heart rate of one (possibly merged) session, drawn from its stored samples. */
export function SessionScope({ session }: { session: MergedSession }) {
  const boxRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [points, setPoints] = useState<ScopePoint[] | null>(null);
  const runs = useMemo(() => (points ? heartRuns(points) : []), [points]);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const all: Sample[] = [];
        for (const id of session.mergedIds) all.push(...(await SessionRepository.getSamples(id)));
        if (alive) setPoints(scopePoints(all));
      } catch (e) {
        console.error("[history] could not load samples", e);
        if (alive) setFailed(true);
      }
    })();
    return () => {
      alive = false;
    };
  }, [session]);

  useEffect(() => {
    const box = boxRef.current;
    const canvas = canvasRef.current;
    if (!box || !canvas || !points || points.length < 2) return;
    const paint = () => {
      const w = box.getBoundingClientRect().width;
      if (w > 0) draw(canvas, w, points, runs);
    };
    paint();
    const observer = new ResizeObserver(paint);
    observer.observe(box);
    return () => observer.disconnect();
  }, [points, runs]);

  if (failed) return <p className="py-4 text-center font-mono text-xs text-red-400">Could not load this session's samples.</p>;
  if (!points) return <p className="py-4 text-center font-mono text-xs text-white/40">Loading samples…</p>;
  if (points.length < 2) {
    return <p className="py-4 text-center font-mono text-xs text-white/40">This session has too few samples to plot.</p>;
  }

  return (
    <div>
      <div ref={boxRef} className="w-full" style={{ height: HEIGHT }}>
        <canvas
          ref={canvasRef}
          role="img"
          aria-label={runs.length > 0 ? "Speed, incline and heart rate over the session" : "Speed and incline over the session"}
        />
      </div>
      <div className="mt-1 flex gap-4 font-mono text-[11px] text-white/50">
        <span><span className="text-neon-cyan">Cyan</span> speed</span>
        <span><span className="text-neon-magenta">Magenta</span> incline</span>
        {runs.length > 0 && <span><span className="text-neon-yellow">Yellow</span> heart rate</span>}
      </div>
    </div>
  );
}
