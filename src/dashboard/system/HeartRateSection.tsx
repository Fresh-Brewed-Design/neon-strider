import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { dispatch, SETTING_LIMITS } from "../../ipc";
import { systemLabel } from "../../names";
import { useAppSnapshot } from "../../state/useAppState";
import { useTreadmillStore } from "../../store/treadmillStore";
import { StatusPill } from "../StatusPill";
import { GhostButton, Hint, NeonButton } from "../ui";
import { NumberField, Section } from "./fields";
import { heartRatePill, heartRateTone } from "./heartRate";


/**
 * System -> Heart rate: pick a Bluetooth heart rate sensor, such as a watch
 * with Broadcast Heart Rate on (`snapshot.heart_rate`, Rust's `heart_rate.rs`).
 * Off until a sensor is picked, and separate from the treadmill link. It only
 * reads: a heart rate never moves the belt.
 */
export function HeartRateSection() {
  const heart = useAppSnapshot()?.heart_rate;
  const scan = useAction();
  const pick = useAction();
  if (!heart) return null;

  const tone = heartRateTone(heart);
  const others = heart.found.filter((f) => f.id !== heart.sensor?.id);
  const scanning = scan.pending || heart.scanning;

  return (
    <Section
      id="sys-heart"
      title="Heart rate"
      description="Read your heart rate from a Bluetooth sensor such as a watch."
      status={<StatusPill tone={tone} label={heartRatePill(heart)} title={`${systemLabel("HEART_LINK")}. ${heart.message}`} />}
    >
      <div className="flex flex-col gap-2">
        {heart.sensor && (
          <>
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0 truncate font-mono text-xs text-white/85">
                <span className="font-bold uppercase tracking-[0.12em]">{heart.sensor.name || "Heart rate sensor"}</span>
                {heart.bpm !== null && <span className="ml-2 text-neon-magenta">{heart.bpm} BPM</span>}
              </div>
              <GhostButton
                disabled={pick.pending}
                onClick={() =>
                  useTreadmillStore.getState().confirm({
                    title: "Forget heart rate sensor",
                    message: `Neon Strider stops reading ${heart.sensor?.name || "the heart rate sensor"} until you pick a sensor again.`,
                    confirmLabel: "Forget",
                    tone: "danger",
                    onConfirm: () => void pick.run(() => dispatch({ type: "heart_rate_forget" })),
                  })
                }
              >
                Forget
              </GhostButton>
            </div>
            <ActionStatus
              ambient
              pending={tone === "working" ? heart.message : null}
              done={tone === "ok" ? heart.message : null}
              error={tone === "problem" ? heart.message : null}
            />
          </>
        )}
        <div>
          <NeonButton
            className="px-3 py-1.5"
            disabled={scanning}
            onClick={() => void scan.run(() => dispatch({ type: "heart_rate_scan" }))}
          >
            {scanning ? "Scanning..." : "Scan for sensors"}
          </NeonButton>
          <ActionStatus
            pending={scan.pending ? "Listening for heart rate sensors for about 5 seconds." : null}
            error={scan.error && `Could not scan: ${scan.error}`}
            className="mt-1"
          />
        </div>
        {others.map((f) => (
          <div key={f.id} className="flex items-center justify-between gap-3">
            <span className="min-w-0 truncate font-mono text-xs text-white/85">{f.name}</span>
            <NeonButton
              className="px-3 py-1"
              disabled={pick.pending}
              onClick={() => void pick.run(() => dispatch({ type: "heart_rate_pick", id: f.id, name: f.name }))}
            >
              Use
            </NeonButton>
          </div>
        ))}
        {pick.error && <ActionStatus error={`Could not change the sensor: ${pick.error}`} />}
        {!heart.sensor && heart.found.length === 0 && !scanning && (
          <Hint>Heart rate stays off until you pick a sensor; on a Garmin watch, switch on Broadcast Heart Rate first.</Hint>
        )}
        <NumberField
          label="Max heart rate"
          unit="bpm"
          value={heart.max_bpm}
          min={SETTING_LIMITS.max_heart_rate_bpm.min}
          max={SETTING_LIMITS.max_heart_rate_bpm.max}
          hint={
            heart.max_bpm_set
              ? "Program heart zones are a percentage of the max heart rate."
              : `Program heart zones use ${heart.max_bpm} bpm, the estimate for age 30, until you set your own max.`
          }
          onCommit={(bpm) => dispatch({ type: "heart_rate_max_set", bpm })}
        />
      </div>
    </Section>
  );
}
