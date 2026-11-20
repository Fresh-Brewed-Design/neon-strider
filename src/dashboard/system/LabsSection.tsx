import { StatusPill } from "../StatusPill";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { dispatch } from "../../ipc";
import { useAppSnapshot } from "../../state/useAppState";
import { Hint } from "../ui";
import { Section, Toggle } from "./fields";
import { ghostPodPill, ghostPodReading, ghostPodTone } from "./ghostPod";

/**
 * System -> Labs: experiments that are off until switched on. GHOST_POD makes
 * the PC a Bluetooth foot pod for a watch (`snapshot.ghost_pod`, Rust's
 * `ghost_pod.rs`). It only reports the belt; nothing from the watch moves it.
 */
export function LabsSection() {
  const pod = useAppSnapshot()?.ghost_pod;
  const change = useAction();
  if (!pod) return null;

  const tone = ghostPodTone(pod);
  const reading = ghostPodReading(pod);

  return (
    <Section
      id="sys-labs"
      title="Labs"
      description="Experiments. Each one is off until you switch it on."
      status={<StatusPill tone={tone} label={ghostPodPill(pod)} title={pod.message} />}
    >
      <div>
        <Toggle
          label="GHOST_POD: Be a foot pod for my watch"
          description="The PC advertises as a Bluetooth running pod and sends the belt speed, cadence and distance once a second. Pausing the belt sends 0 km/h, so the watch's Auto Pause can stop its timer."
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
      </div>
    </Section>
  );
}
