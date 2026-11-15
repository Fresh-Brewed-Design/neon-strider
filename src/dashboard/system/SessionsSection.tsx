import {
  MERGE_THRESHOLD_MAX,
  MERGE_THRESHOLD_MIN,
  saveSessionMergeThreshold,
} from "../../services/sessionMergeSetting";
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
        min={MERGE_THRESHOLD_MIN}
        max={MERGE_THRESHOLD_MAX}
        value={minutes}
        hint="Measured from the end of one record to the start of the next."
        onCommit={saveSessionMergeThreshold}
      />
    </Section>
  );
}
