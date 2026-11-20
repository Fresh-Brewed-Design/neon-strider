import { SETTING_LIMITS } from "../../ipc";
import { saveSessionMergeThreshold } from "../../services/sessionMergeSetting";
import { useTreadmillStore } from "../../store/treadmillStore";
import { NumberField, Section } from "./fields";

/** How close together two records must be to count as one session. */
export function SessionsSection() {
  const minutes = useTreadmillStore((s) => s.sessionMergeThresholdMinutes);
  return (
    <Section
      id="sys-sessions"
      title="Sessions"
      description="Stopping and starting the treadmill creates separate records. Records close together are shown, counted and uploaded to Strava as one session."
    >
      <NumberField
        label="Join sessions closer than"
        unit="minutes"
        min={SETTING_LIMITS.session_merge_minutes.min}
        max={SETTING_LIMITS.session_merge_minutes.max}
        value={minutes}
        hint="Measured from the end of one record to the start of the next."
        onCommit={saveSessionMergeThreshold}
      />
    </Section>
  );
}
