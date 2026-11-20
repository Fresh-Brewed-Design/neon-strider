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
      description="Records close together are joined into one session for History and Strava."
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
