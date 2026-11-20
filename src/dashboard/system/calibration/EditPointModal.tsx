import { useEffect, useState } from "react";
import { Edit } from "lucide-react";
import type { CalibrationPoint } from "../../../store/types";
import { ActionStatus } from "../../../components/ActionStatus";
import { useAppState } from "../../../state/useAppState";
import { retimedPoint, scaleRefusal } from "./math";

/** Adjusts one point: re-time 50 steps with a stopwatch, or type the factor in. */
export function EditPointModal({
  point,
  onClose,
  onSave,
}: {
  point: CalibrationPoint;
  onClose: () => void;
  onSave: (p: CalibrationPoint) => void;
}) {
  const [stride, setStride] = useState(point.stride_length_cm);
  const [seconds, setSeconds] = useState(30);
  const [autoMode, setAutoMode] = useState(true);
  const [manualScale, setManualScale] = useState(point.distance_scale);
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const isTiming = startedAt !== null;

  useEffect(() => {
    if (startedAt === null) return;
    const interval = setInterval(() => setSeconds((Date.now() - startedAt) / 1000), 100); // the tenths it shows
    return () => clearInterval(interval);
  }, [startedAt]);

  const { realKmh, scale: calculatedScale } = retimedPoint(stride, seconds, point.speed);
  const finalScale = autoMode ? calculatedScale : manualScale;
  const limits = useAppState((s) => s.calibration.limits);
  const refusal = scaleRefusal(finalScale, limits, "Change the time or the factor.");

  return (
    <div className="fixed inset-0 z-[70] bg-black/95 backdrop-blur-xl flex items-center justify-center p-4">
      <div className="bg-[#0a0a0a] border border-neon-cyan/30 rounded-2xl w-full max-w-sm p-6 space-y-6 shadow-[0_0_100px_rgba(0,243,255,0.1)]">
        <h4 className="text-sm font-black text-white uppercase tracking-widest flex items-center gap-2">
          <Edit size={16} className="text-neon-cyan" /> CALIBRATION_FIX: {point.speed.toFixed(1)} KM/H
        </h4>

        <div className="space-y-4">
          <div className="p-4 bg-white/5 rounded-xl border border-white/10 space-y-3">
            <div className="flex justify-between items-end">
              <div className="text-[11px] font-black uppercase text-neon-cyan/60 tracking-widest">Physics Core: Stride</div>
              <div className="text-lg font-digital text-white">
                {stride.toFixed(1)}
                <span className="text-[11px] ml-1 opacity-40">CM</span>
              </div>
            </div>
            {/* Walking strides; Rust accepts 20 to 200 cm (`calibration.limits`). */}
            <input
              type="range"
              min="40"
              max="130"
              step="0.1"
              value={stride}
              onChange={(e) => setStride(Number(e.target.value))}
              className="w-full h-1 bg-white/10 rounded-lg appearance-none cursor-pointer accent-neon-cyan"
            />
            <div className="text-[11px] text-white/50 uppercase font-mono italic">Validated by belt loops? Keep this as is.</div>
          </div>

          <div className="flex bg-white/5 p-1 rounded-lg border border-white/5">
            <button
              onClick={() => setAutoMode(true)}
              className={`flex-1 py-1 text-[11px] font-black uppercase rounded ${autoMode ? "bg-neon-cyan text-black" : "text-white/50"}`}
            >
              Recalc from Time
            </button>
            <button
              onClick={() => setAutoMode(false)}
              className={`flex-1 py-1 text-[11px] font-black uppercase rounded ${!autoMode ? "bg-neon-magenta text-white" : "text-white/50"}`}
            >
              Manual Factor
            </button>
          </div>

          {autoMode ? (
            <div className="p-4 bg-neon-cyan/5 rounded-xl border border-neon-cyan/20 space-y-4">
              <div className="flex justify-between items-end">
                <div className="text-[11px] font-black uppercase text-neon-cyan tracking-widest">Time for 50 Steps</div>
                <div className="text-2xl font-digital text-neon-cyan">
                  {seconds.toFixed(1)}
                  <span className="text-[11px] ml-1 opacity-40">SEC</span>
                </div>
              </div>

              <div className="flex gap-2">
                {/* 50 steps at 0.2 to 2.4 s a step. */}
                <input
                  type="range"
                  min="10"
                  max="120"
                  step="0.1"
                  value={seconds}
                  onChange={(e) => setSeconds(Number(e.target.value))}
                  className="flex-1 h-1.5 mt-4 bg-white/10 rounded-lg appearance-none cursor-pointer accent-neon-cyan"
                />
                <button
                  onClick={() => {
                    if (!isTiming) setSeconds(0);
                    setStartedAt(isTiming ? null : Date.now());
                  }}
                  className={`px-4 py-2 rounded font-black text-[11px] uppercase tracking-widest transition-all ${isTiming ? "bg-neon-magenta text-white animate-pulse" : "bg-neon-cyan text-black hover:bg-white"}`}
                >
                  {isTiming ? "STOP_TIMER" : "START_WATCH"}
                </button>
              </div>

              <div className="bg-black/40 p-2 rounded border border-white/5 space-y-1">
                <div className="flex justify-between text-[11px] font-mono uppercase">
                  <span className="text-white/50">Target Speed</span>
                  <span className="text-white">{point.speed.toFixed(1)} km/h</span>
                </div>
                <div className="flex justify-between text-[11px] font-mono uppercase font-black">
                  <span className="text-neon-cyan/60">Actual Speed</span>
                  <span className="text-neon-cyan">{realKmh.toFixed(2)} km/h</span>
                </div>
              </div>
            </div>
          ) : (
            <div className="p-4 bg-neon-magenta/5 rounded-xl border border-neon-magenta/20 space-y-4">
              <div className="flex justify-between items-end">
                <div className="text-[11px] font-black uppercase text-neon-magenta tracking-widest">Manual Velocity Bias</div>
                <div className="text-2xl font-digital text-neon-magenta">
                  {manualScale.toFixed(3)}
                  <span className="text-[11px] ml-1 opacity-40">x</span>
                </div>
              </div>
              {/* A fine slider; Rust refuses a factor outside `calibration.limits`. */}
              <input
                type="range"
                min="0.5"
                max="2.0"
                step="0.001"
                value={manualScale}
                onChange={(e) => setManualScale(Number(e.target.value))}
                className="w-full h-1.5 bg-white/10 rounded-lg appearance-none cursor-pointer accent-neon-magenta"
              />
            </div>
          )}

          <div className="bg-white/5 p-3 rounded-xl border border-white/5 flex justify-between items-center group">
            <div className="text-[11px] text-white/50 uppercase font-black">Resulting Alignment</div>
            <div className={`text-sm font-digital ${Math.abs(1 - finalScale) > 0.1 ? "text-neon-magenta" : "text-emerald-400"}`}>
              {finalScale.toFixed(4)}x FACTOR
            </div>
          </div>
        </div>

        <div className="space-y-2 pt-2">
          <ActionStatus error={refusal} />
          <div className="flex gap-2">
            <button
              onClick={onClose}
              className="flex-1 py-3 bg-white/5 hover:bg-white/10 rounded font-black text-[11px] tracking-widest transition-all"
            >
              ABORT
            </button>
            <button
              disabled={refusal !== null}
              onClick={() => {
                // A stride typed in by hand counts as one measured stride, so GAIT_SYNC averages with it.
                if (refusal === null)
                  onSave({
                    ...point,
                    stride_length_cm: stride,
                    distance_scale: finalScale,
                    runs: 1,
                    stride_runs: stride === point.stride_length_cm ? (point.stride_runs ?? 0) : 1,
                  });
              }}
              className="flex-1 py-3 bg-neon-cyan text-black hover:bg-white rounded font-black text-[11px] tracking-widest shadow-[0_0_20px_rgba(0,243,255,0.4)] transition-all disabled:opacity-30 disabled:cursor-not-allowed"
            >
              COMMIT_FIX
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
