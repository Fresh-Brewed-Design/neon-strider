import { useState } from "react";
import { ChevronDown, ChevronRight, Edit, Plus, Trash2 } from "lucide-react";
import { useAppState } from "../../../state/useAppState";
import { useTreadmillStore } from "../../../store/treadmillStore";
import type { CalibrationPoint } from "../../../store/types";
import { CalibrationWizard } from "./CalibrationWizard";
import { EditPointModal } from "./EditPointModal";
import { GuidedCalibration } from "./GuidedCalibration";
import { appliedScale } from "./math";

/**
 * GROUND_TRUTH, always. The old panel (count loops, edit a point by hand)
 * stays under ADVANCED // POINT_EDITOR until #92 retires it.
 */
export function CalibrationPanel() {
  const [editor, setEditor] = useState(false);
  return (
    <div className="space-y-4">
      <GuidedCalibration />
      <button
        type="button"
        aria-expanded={editor}
        onClick={() => setEditor((v) => !v)}
        className="flex items-center gap-1 font-mono text-[11px] font-bold uppercase tracking-[0.15em] text-white/60 transition-colors hover:text-neon-cyan"
      >
        {editor ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        ADVANCED // POINT_EDITOR
      </button>
      {editor && <ClassicCalibrationPanel />}
    </div>
  );
}

/** The saved calibration points, with the wizard that measures a new one. */
export function ClassicCalibrationPanel() {
  const calibrationPoints = useTreadmillStore((s) => s.calibrationPoints);
  const addCalibrationPoint = useTreadmillStore((s) => s.addCalibrationPoint);
  const removeCalibrationPoint = useTreadmillStore((s) => s.removeCalibrationPoint);
  const updateCalibrationPoint = useTreadmillStore((s) => s.updateCalibrationPoint);
  const limits = useAppState((s) => s.calibration.limits);
  const [isWizardOpen, setIsWizardOpen] = useState(false);
  const [editing, setEditing] = useState<{ index: number; point: CalibrationPoint } | null>(null);

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-3">
        <p className="font-mono text-xs leading-relaxed text-white/50">
          Each point records how far the belt really moves at one speed. Speeds in between are interpolated.
        </p>
        <button
          onClick={() => setIsWizardOpen(true)}
          className="flex shrink-0 items-center gap-2 rounded border border-neon-cyan/60 bg-neon-cyan/5 px-3 py-1.5 font-digital text-xs font-bold uppercase tracking-[0.18em] text-neon-cyan transition-colors hover:bg-neon-cyan/15"
        >
          <Plus size={14} /> New point
        </button>
      </div>

      <div className="space-y-2">
        {calibrationPoints.length === 0 ? (
          <div className="p-8 text-center bg-white/5 rounded-xl border border-dashed border-white/10">
            <div className="text-[11px] text-white/50 uppercase font-mono">No calibration points yet.</div>
            <div className="text-[11px] text-white/50 uppercase font-mono mt-1 italic">
              Distances use average human stride values until you add one.
            </div>
          </div>
        ) : (
          calibrationPoints.map((p, idx) => (
            <div
              key={idx}
              className="hacker-panel p-3 rounded-lg border-white/5 bg-white/[0.02] flex justify-between items-center group relative overflow-hidden"
            >
              <div className="absolute inset-0 bg-gradient-to-r from-neon-cyan/5 to-transparent -translate-x-full group-hover:translate-x-0 transition-transform duration-500" />
              <div className="flex gap-4 items-center relative z-10">
                <PointValue label="VELOCITY" className="text-neon-cyan" value={p.speed.toFixed(1)} />
                <div className="w-[1px] h-6 bg-white/10" />
                <PointValue label="STRIDE" className="text-white" value={`${p.stride_length_cm.toFixed(1)}cm`} />
                <div className="w-[1px] h-6 bg-white/10" />
                <PointValue
                  label="FACTOR"
                  className="text-neon-magenta"
                  value={`${p.distance_scale.toFixed(3)}x`}
                  hint={
                    limits && appliedScale(p.distance_scale, limits) !== p.distance_scale
                      ? `Applied as ${appliedScale(p.distance_scale, limits).toFixed(2)}.`
                      : undefined
                  }
                />
              </div>
              <div className="flex gap-1 relative z-10">
                <button
                  onClick={() => setEditing({ index: idx, point: p })}
                  aria-label="Edit point"
                  title="Edit"
                  className="p-2 text-white/50 hover:text-neon-cyan transition-colors"
                >
                  <Edit size={14} />
                </button>
                <button
                  onClick={() => removeCalibrationPoint(idx)}
                  aria-label="Delete point"
                  title="Delete"
                  className="p-2 text-white/50 hover:text-neon-magenta transition-colors"
                >
                  <Trash2 size={14} />
                </button>
              </div>
            </div>
          ))
        )}

        {calibrationPoints.length > 1 && (
          <div className="mt-4 p-2 bg-neon-cyan/5 rounded border border-neon-cyan/10 text-[11px] text-neon-cyan/60 font-mono text-center">
            Interpolating between your calibration points
          </div>
        )}
      </div>

      {isWizardOpen && <CalibrationWizard onClose={() => setIsWizardOpen(false)} onSave={addCalibrationPoint} />}

      {editing && (
        <EditPointModal
          point={editing.point}
          onClose={() => setEditing(null)}
          onSave={(p) => {
            updateCalibrationPoint(editing.index, p);
            setEditing(null);
          }}
        />
      )}
    </div>
  );
}

function PointValue({ label, value, className, hint }: { label: string; value: string; className: string; hint?: string }) {
  return (
    <div className="space-y-1">
      <div className="text-[11px] text-white/50 uppercase font-mono">{label}</div>
      <div className={`text-sm font-digital ${className}`}>{value}</div>
      {hint && <div className="text-[11px] font-mono text-neon-yellow">{hint}</div>}
    </div>
  );
}
