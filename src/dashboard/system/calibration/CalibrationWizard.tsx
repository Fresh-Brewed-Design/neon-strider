import { useEffect, useRef, useState } from "react";
import { Calculator } from "lucide-react";
import { useAppState } from "../../../state/useAppState";
import { useTreadmillStore } from "../../../store/treadmillStore";
import type { CalibrationPoint } from "../../../store/types";
import { STEPS_PER_RUN, isSavablePoint, wizardResult, type WizardMode } from "./math";

type Phase = "SETUP" | "COUNTDOWN" | "WALKING" | "RESULT";

const COUNTDOWN_S = 3;

/**
 * Measures a new point: a countdown, a timed run of counted steps (or belt
 * logo passes), then the result to adjust and save.
 */
export function CalibrationWizard({ onClose, onSave }: { onClose: () => void; onSave: (p: CalibrationPoint) => void }) {
  const [phase, setPhase] = useState<Phase>("SETUP");
  const [countdown, setCountdown] = useState(COUNTDOWN_S);
  const [startTime, setStartTime] = useState(0);
  const [endTime, setEndTime] = useState(0);
  const [manualStride, setManualStride] = useState(74);
  const [calibrationMode, setCalibrationMode] = useState<WizardMode>("MANUAL");
  const [logoPasses, setLogoPasses] = useState(1);
  const [stepsOverride, setStepsOverride] = useState(STEPS_PER_RUN);
  const [capturedTargetSpeed, setCapturedTargetSpeed] = useState(0);
  const beltLength = useTreadmillStore((s) => s.beltLength);
  const currentSpeed = useAppState((s) => s.deck.raw_speed_kmh) ?? 0;
  // The last nonzero raw speed seen while walking. The belt may be stopped or
  // coasting by the time DONE is clicked, so the click itself is no source.
  const walkingSpeed = useRef(0);

  useEffect(() => {
    if (phase === "COUNTDOWN") walkingSpeed.current = 0;
    if (phase === "WALKING" && currentSpeed > 0) walkingSpeed.current = currentSpeed;
  }, [phase, currentSpeed]);

  useEffect(() => {
    if (phase !== "COUNTDOWN") return;
    let left = COUNTDOWN_S;
    const interval = setInterval(() => {
      left -= 1;
      if (left > 0) {
        setCountdown(left);
      } else {
        setStartTime(Date.now());
        setPhase("WALKING");
      }
    }, 1000);
    return () => clearInterval(interval);
  }, [phase]);

  const startCountdown = () => {
    setCountdown(COUNTDOWN_S);
    setPhase("COUNTDOWN");
  };

  const finishWalking = () => {
    setEndTime(Date.now());
    // Calibrate against the raw speed the deck reported while walking. The
    // calibrated speed already has the old points applied and would feed them back in.
    setCapturedTargetSpeed(walkingSpeed.current);
    setPhase("RESULT");
  };

  const result = wizardResult({
    mode: calibrationMode,
    strideCm: manualStride,
    logoPasses,
    beltLengthCm: beltLength,
    steps: stepsOverride,
    elapsedSeconds: (endTime - startTime) / 1000,
    reportedKmh: capturedTargetSpeed,
  });

  const canSave = isSavablePoint(capturedTargetSpeed, result.scale);

  return (
    <div className="fixed inset-0 z-[60] bg-black/80 backdrop-blur-sm flex items-center justify-center p-4">
      <div className="bg-[#0a0a0a] border border-neon-cyan/30 rounded-2xl w-full max-w-md p-6 shadow-[0_0_50px_rgba(0,243,255,0.1)] relative overflow-hidden">
        <div className="absolute top-0 left-0 w-full h-1 bg-gradient-to-r from-transparent via-neon-cyan to-transparent" />

        <h4 className="text-lg font-black text-white uppercase tracking-widest mb-6 flex items-center gap-3">
          <Calculator size={18} className="text-neon-cyan" />
          GROUND_TRUTH // SYNC
        </h4>

        {phase === "SETUP" && (
          <div className="space-y-6 text-center">
            <div className="text-xs text-white/60 font-mono leading-relaxed">
              Set treadmill to your target calibration speed.
              <br />
              Once you are walking comfortably, we will begin.
            </div>
            <div className="bg-white/5 p-4 rounded-xl border border-white/5 inline-block">
              <div className="text-[11px] uppercase tracking-widest text-white/50 mb-1">TARGET VELOCITY</div>
              <div className={`text-2xl font-digital ${currentSpeed > 0 ? "text-neon-cyan" : "text-white/50"}`}>
                {currentSpeed.toFixed(1)}
              </div>
            </div>
            <div className="bg-neon-cyan/5 p-3 rounded text-[11px] text-neon-cyan/80 border border-neon-cyan/10">
              REQUIRED: Count exactly <span className="font-bold text-white">{STEPS_PER_RUN} STEPS</span> (or tracked logo
              cycles) once the countdown finishes.
              <div className="mt-1 text-[11px] opacity-60 italic">Precision timing will align your steps with target speed.</div>
            </div>
            <div className="flex gap-2">
              <button
                onClick={onClose}
                className="flex-1 py-3 bg-white/5 hover:bg-white/10 rounded font-black text-[11px] tracking-widest transition-all"
              >
                CANCEL
              </button>
              <button
                onClick={startCountdown}
                disabled={currentSpeed <= 0}
                className="flex-1 py-3 bg-neon-cyan text-black hover:bg-white rounded font-black text-[11px] tracking-widest transition-all disabled:opacity-30 disabled:cursor-not-allowed"
              >
                INITIATE
              </button>
            </div>
          </div>
        )}

        {phase === "COUNTDOWN" && (
          <div className="py-12 text-center">
            <div className="text-[11px] uppercase tracking-widest text-white/50 mb-4">PREPARE TO COUNT</div>
            <div className="text-6xl font-black text-white">{countdown}</div>
          </div>
        )}

        {phase === "WALKING" && (
          <div className="py-8 text-center space-y-6">
            <div className="text-neon-cyan animate-pulse font-black text-xl tracking-widest">TIMING GROUND TRUTH...</div>
            <div className="text-sm font-mono text-white/60">Perform your measured steps or belt cycles.</div>
            <button
              onClick={finishWalking}
              className="w-full py-6 bg-neon-magenta text-white rounded-xl font-black text-lg tracking-widest hover:bg-white hover:text-black transition-all shadow-[0_0_20px_rgba(255,0,255,0.4)]"
            >
              STOP (DONE)
            </button>
          </div>
        )}

        {phase === "RESULT" && (
          <div className="space-y-6">
            <div className="flex bg-white/5 p-1 rounded-lg">
              <button
                onClick={() => setCalibrationMode("MANUAL")}
                className={`flex-1 py-2 text-[11px] font-black uppercase rounded transition-all ${calibrationMode === "MANUAL" ? "bg-neon-cyan/20 text-neon-cyan border border-neon-cyan/30" : "text-white/50"}`}
              >
                Manual Correction
              </button>
              <button
                onClick={() => setCalibrationMode("LOGO")}
                className={`flex-1 py-2 text-[11px] font-black uppercase rounded transition-all ${calibrationMode === "LOGO" ? "bg-neon-magenta/20 text-neon-magenta border border-neon-magenta/30" : "text-white/50"}`}
              >
                Logo-Sync (Precision)
              </button>
            </div>

            {calibrationMode === "MANUAL" ? (
              <div className="p-4 bg-white/5 rounded-xl border border-white/10 space-y-4">
                <div className="flex justify-between items-end">
                  <div className="text-[11px] font-black uppercase text-neon-cyan tracking-widest">Identify Real Stride</div>
                  <div className="text-xl font-digital text-white">
                    {manualStride}
                    <span className="text-[11px] ml-1 opacity-40">CM</span>
                  </div>
                </div>
                <input
                  type="range"
                  min="40"
                  max="130"
                  step="1"
                  value={manualStride}
                  onChange={(e) => setManualStride(Number(e.target.value))}
                  className="w-full h-1.5 bg-white/10 rounded-lg appearance-none cursor-pointer accent-neon-cyan"
                />
                <div className="flex justify-between text-[11px] font-mono uppercase text-white/50">
                  <span>SHUFFLE (40cm)</span>
                  <span>POWER_WALK (130cm)</span>
                </div>
              </div>
            ) : (
              <div className="p-4 bg-white/5 rounded-xl border border-white/10 space-y-4">
                <div className="grid grid-cols-2 gap-4">
                  <div className="space-y-1">
                    <div className="text-[11px] font-black text-white/50 uppercase">Logo Revolutions</div>
                    <input
                      type="number"
                      value={logoPasses}
                      onChange={(e) => setLogoPasses(Number(e.target.value))}
                      className="w-full bg-white/5 border border-white/10 rounded p-2 text-neon-magenta font-mono text-center outline-none"
                    />
                  </div>
                  <div className="space-y-1">
                    <div className="text-[11px] font-black text-white/50 uppercase">Physical Steps</div>
                    <input
                      type="number"
                      value={stepsOverride}
                      onChange={(e) => setStepsOverride(Number(e.target.value))}
                      className="w-full bg-white/5 border border-white/10 rounded p-2 text-neon-cyan font-mono text-center outline-none"
                    />
                  </div>
                </div>
                <div className="text-[11px] text-white/50 font-mono text-center">
                  Calculated Distance: <span className="text-white">{((logoPasses * beltLength) / 100).toFixed(2)}m</span>
                </div>
              </div>
            )}

            <div className="bg-white/5 p-3 rounded-lg border border-white/5 flex justify-between items-center text-[11px] font-mono">
              <div className="space-y-1">
                <div className="text-[11px] text-white/50 uppercase">Parsed Stride</div>
                <div className="text-white font-bold">{result.strideCm.toFixed(1)}cm</div>
              </div>
              <div className="text-right space-y-1">
                <div className="text-[11px] text-white/50 uppercase">Velocity Align</div>
                <div className={`font-bold ${Math.abs(1 - result.scale) > 0.1 ? "text-neon-magenta" : "text-emerald-400"}`}>
                  {result.scale.toFixed(3)}x FACTOR
                </div>
              </div>
            </div>

            {!canSave && (
              <p role="alert" className="text-center font-mono text-xs text-neon-magenta">
                {capturedTargetSpeed > 0
                  ? "The calibration result is not valid. Run the calibration again."
                  : "No belt speed was seen. Run the calibration again."}
              </p>
            )}

            <div className="flex gap-2">
              <button
                onClick={() => setPhase("SETUP")}
                className="flex-1 py-3 bg-white/5 hover:bg-white/10 rounded font-black text-[11px] tracking-widest transition-all"
              >
                RETRY
              </button>
              <button
                disabled={!canSave}
                onClick={() => {
                  if (!canSave) return;
                  onSave({ speed: capturedTargetSpeed, stride_length_cm: result.strideCm, distance_scale: result.scale });
                  onClose();
                }}
                className="flex-1 py-3 bg-neon-cyan text-black hover:bg-white rounded font-black text-[11px] tracking-widest shadow-[0_0_20px_rgba(0,243,255,0.3)] transition-all disabled:opacity-30 disabled:cursor-not-allowed"
              >
                SAVE MULTI-AXIS DATA
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
