import { StatusPill } from "../StatusPill";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { dispatch } from "../../ipc";
import { useAppSnapshot } from "../../state/useAppState";
import { Hint } from "../ui";
import { Section, Toggle } from "./fields";
import { ghostPodPill, ghostPodPowerReading, ghostPodReading, ghostPodTone } from "./ghostPod";
import { TypingSpeedTable } from "./TypingSpeedTable";

/**
 * System -> Labs: experiments that are off until switched on. GHOST_POD makes
 * the PC a Bluetooth foot pod for a watch (`snapshot.ghost_pod`, Rust's
 * `ghost_pod.rs`). Its power switch, shown while GHOST_POD is on, adds running
 * power for the watch. It only reports the belt; nothing from the watch moves it.
 * ZONE_TILT lets program heart zones trim incline (`heart_rate.incline_trim`,
 * Rust's `programs::ZoneTrim`); off, every zone trims speed only.
 * Typing by speed saves key presses per minute (a count only) with each walk
 * and shows the median per speed band (`typing_speed`, Rust's `typing_speed.rs`).
 */
export function LabsSection() {
  const snap = useAppSnapshot();
  const pod = snap?.ghost_pod;
  const heart = snap?.heart_rate;
  const change = useAction();
  const power = useAction();
  const tiltChange = useAction();
  const typingChange = useAction();
  const typing = snap?.typing_speed;
  if (!pod || !heart || !typing) return null;

  const tone = ghostPodTone(pod);
  const reading = ghostPodReading(pod);
  const powerReading = ghostPodPowerReading(pod);

  return (
    <Section
      id="sys-labs"
      title="Labs"
      description="Labs features stay off until you switch them on."
      status={<StatusPill tone={tone} label={ghostPodPill(pod)} title={pod.message} />}
    >
      <div>
        <Toggle
          label="GHOST_POD: Be a foot pod for my watch"
          description="Your watch reads belt speed, cadence and distance from the PC as a foot pod; a paused belt reads 0 km/h."
          checked={pod.enabled}
          onChange={(on) => void change.run(() => dispatch({ type: "ghost_pod_set", on }))}
        />
        {change.pending || change.error ? (
          <ActionStatus
            pending={change.pending}
            error={change.error && `Could not switch GHOST_POD ${pod.enabled ? "off" : "on"}: ${change.error}`}
            className="mt-1"
          />
        ) : (
          pod.enabled && (
            <ActionStatus
              ambient
              pending={tone === "working" ? pod.message : null}
              done={tone === "ok" ? [pod.message, reading].filter(Boolean).join(" · ") : null}
              error={tone === "problem" ? pod.message : null}
              className="mt-1"
            />
          )
        )}
        <Hint className="mt-0.5">Pausing on the watch can't pause the belt yet: a foot pod has no way to hear it.</Hint>
        {pod.enabled && (
          <div className="mt-3 border-l border-white/10 pl-3">
            <Toggle
              label="GHOST_POD power: Send running power to my watch"
              description="Your watch reads running power from the PC, rising with the incline; a paused belt sends 0 W."
              checked={pod.power_enabled}
              onChange={(on) => void power.run(() => dispatch({ type: "ghost_pod_power_set", on }))}
            />
            {power.pending || power.error ? (
              <ActionStatus
                pending={power.pending}
                error={
                  power.error &&
                  `Could not switch GHOST_POD power ${pod.power_enabled ? "off" : "on"}: ${power.error}`
                }
                className="mt-1"
              />
            ) : (
              powerReading && (
                <ActionStatus
                  ambient
                  pending={pod.power_watches > 0 ? null : powerReading}
                  done={pod.power_watches > 0 ? powerReading : null}
                  className="mt-1"
                />
              )
            )}
            <Hint className="mt-0.5">
              Watts come from the METABOLIC_BURN estimate and your weight. Switching power leaves the watch's foot pod link up.
            </Hint>
          </div>
        )}
      </div>
      <div>
        <Toggle
          label="ZONE_TILT: Let heart zones trim the incline"
          description="Zone steps in the program editor can trim speed, incline or both."
          hint="Switched off, every heart zone trims the speed only."
          checked={heart.incline_trim}
          onChange={(on) => void tiltChange.run(() => dispatch({ type: "heart_rate_incline_trim_set", on }))}
        />
        {(tiltChange.pending || tiltChange.error) && (
          <ActionStatus
            pending={tiltChange.pending}
            error={tiltChange.error && `Could not switch ZONE_TILT ${heart.incline_trim ? "off" : "on"}: ${tiltChange.error}`}
            className="mt-1"
          />
        )}
      </div>
      <div>
        <Toggle
          label="Typing by speed: Count key presses per minute while walking"
          description="Saves how many keys you press in each minute of a walk, never which keys."
          hint="Switched off, nothing is counted or saved."
          checked={typing.enabled}
          onChange={(on) => void typingChange.run(() => dispatch({ type: "typing_speed_set", on }))}
        />
        {(typingChange.pending || typingChange.error) && (
          <ActionStatus
            pending={typingChange.pending}
            error={
              typingChange.error && `Could not switch typing by speed ${typing.enabled ? "off" : "on"}: ${typingChange.error}`
            }
            className="mt-1"
          />
        )}
        {typing.enabled && <TypingSpeedTable data={typing} />}
      </div>
    </Section>
  );
}
