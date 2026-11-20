import { StatusPill } from "../StatusPill";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { dispatch } from "../../ipc";
import { useAppSnapshot } from "../../state/useAppState";
import { Hint } from "../ui";
import { Section, Toggle } from "./fields";
import {
  ghostPodPill,
  ghostPodPowerReading,
  ghostPodReading,
  ghostPodTone,
  wristLinkTone,
} from "./ghostPod";

/**
 * System -> Labs: experiments that are off until switched on. GHOST_POD makes
 * the PC a Bluetooth foot pod for a watch (`snapshot.ghost_pod`, Rust's
 * `ghost_pod.rs`). Its power switch, shown while GHOST_POD is on, adds running
 * power for the watch. It only reports the belt; nothing from the watch moves it.
 */
export function LabsSection() {
  const pod = useAppSnapshot()?.ghost_pod;
  const change = useAction();
  const power = useAction();
  const wristChange = useAction();
  if (!pod) return null;

  const tone = ghostPodTone(pod);
  const reading = ghostPodReading(pod);
  const powerReading = ghostPodPowerReading(pod);
  const wrist = pod.wrist_link;
  const wristTone = wristLinkTone(pod);

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
              Watts come from the METABOLIC_BURN estimate and your weight. Switching power restarts GHOST_POD; the watch reconnects.
            </Hint>
          </div>
        )}
      </div>
      <div>
        <Toggle
          label="WRIST_LINK: Let the watch's data field read the belt"
          description="Serves a second Bluetooth service next to the foot pod for a Connect IQ data field on the watch. It only reads the belt for now; nothing the watch sends moves it."
          checked={wrist.enabled}
          onChange={(on) => void wristChange.run(() => dispatch({ type: "wrist_link_set", on }))}
        />
        {wristChange.pending || wristChange.error ? (
          <ActionStatus
            pending={wristChange.pending}
            error={wristChange.error && `Could not switch WRIST_LINK ${wrist.enabled ? "off" : "on"}: ${wristChange.error}`}
            className="mt-1"
          />
        ) : (
          wrist.enabled && (
            <ActionStatus
              ambient
              pending={wristTone === "working" ? wrist.message : null}
              done={wristTone === "ok" ? wrist.message : null}
              error={wristTone === "problem" ? wrist.message : null}
              className="mt-1"
            />
          )
        )}
        <Hint className="mt-0.5">Needs GHOST_POD on: the watch keeps its foot pod link and adds this one.</Hint>
      </div>
    </Section>
  );
}
