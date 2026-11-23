import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { dispatch } from "../../ipc";
import { useAppSnapshot } from "../../state/useAppState";
import { Section, Toggle } from "./fields";
import { TypingSpeedTable } from "./TypingSpeedTable";
import { wristLinkTone } from "./wristLink";

/**
 * System -> Labs: experiments that are off until switched on. WRIST_LINK
 * serves the watch's Connect IQ data field (`snapshot.wrist_link`, Rust's
 * `wrist_link.rs`). It only reports the belt; nothing from the watch moves it.
 * ZONE_TILT lets program heart zones trim incline (`heart_rate.incline_trim`,
 * Rust's `programs::ZoneTrim`); off, every zone trims speed only.
 * Typing by speed saves key presses per minute (a count only) with each walk
 * and shows the median per speed band (`typing_speed`, Rust's `typing_speed.rs`).
 */
export function LabsSection() {
  const snap = useAppSnapshot();
  const wrist = snap?.wrist_link;
  const heart = snap?.heart_rate;
  const wristChange = useAction();
  const tiltChange = useAction();
  const typingChange = useAction();
  const typing = snap?.typing_speed;
  if (!wrist || !heart || !typing) return null;

  const wristTone = wristLinkTone(wrist);

  return (
    <Section
      id="sys-labs"
      title="Labs"
      description="Labs features stay off until you switch them on."
    >
      <div>
        <Toggle
          label="WRIST_LINK: Let the watch's data field read the belt"
          description="Serves a Connect IQ data field on the watch through a Bluetooth service on the PC."
          hint="WRIST_LINK only reads the belt and can't change the speed."
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
