import { dispatch } from "../../../ipc";
import { ActionStatus } from "../../../components/ActionStatus";
import { useAction } from "../../../components/useAction";
import { useAppState } from "../../../state/useAppState";
import { GhostButton, Hint, NeonButton } from "../../ui";

/**
 * The first-connect offer: Rust sets `calibration.offer` while a connected
 * treadmill has no belt length and no points, until SKIP for that treadmill
 * (`calibration_offer_skip`, kept per treadmill). "Calibrate this treadmill"
 * only opens System, Calibration; nothing here moves the belt.
 */
export function CalibrateOffer({ onCalibrate }: { onCalibrate: () => void }) {
  const offer = useAppState((s) => s.calibration.offer) ?? false;
  const skip = useAction();
  if (!offer) return null;
  return (
    <div
      role="region"
      aria-label="Calibrate this treadmill"
      className="hacker-panel mx-5 mt-4 space-y-2 rounded-lg border-neon-yellow/40 bg-neon-yellow/5 p-3"
    >
      <div className="font-digital text-xs font-bold uppercase tracking-[0.18em] text-neon-yellow">
        NEON STRIDER // GROUND_TRUTH
      </div>
      <div className="font-digital text-sm text-white">The connected treadmill has no calibration yet.</div>
      <Hint>GROUND_TRUTH times the belt at a few speeds, so distance matches what you walk.</Hint>
      <div className="flex gap-2">
        <NeonButton className="flex-1 py-2" onClick={onCalibrate}>
          Calibrate this treadmill
        </NeonButton>
        <GhostButton
          disabled={skip.pending}
          onClick={() => void skip.run(() => dispatch({ type: "calibration_offer_skip" }))}
        >
          SKIP
        </GhostButton>
      </div>
      <ActionStatus pending={skip.pending} error={skip.error} />
      <Hint>Calibration stays in System, Calibration.</Hint>
    </div>
  );
}
