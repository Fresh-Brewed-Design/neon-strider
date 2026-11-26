import { dispatch } from "../../ipc";
import { ActionStatus, InlineAction } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { useAppSnapshot } from "../../state/useAppState";
import { Hint } from "../ui";
import { Toggle } from "./fields";
import { trafficSummary } from "./traffic";

/**
 * Diagnostics: record every packet to and from the treadmill to a file
 * (`snapshot.traffic`, Rust's `traffic.rs`). It only listens, so it sits
 * outside Developer tools.
 */
export function TrafficRecorder() {
  const t = useAppSnapshot()?.traffic;
  const record = useAction();
  const folder = useAction();
  if (!t) return null;

  const openFolder = () => void folder.run(() => dispatch({ type: "traffic_open_folder" }));
  const saved = !t.recording && t.file_path !== null;

  return (
    <div>
      <Toggle
        label="TRAFFIC_TAP: Record treadmill traffic"
        description="Saves every packet to and from the treadmill in the log folder."
        hint={["Each packet is saved with a timestamp.", "TRAFFIC_TAP only listens and starts off at every launch.", "A recording stops by itself after 4 hours or 20 MB."]}
        checked={t.recording}
        onChange={(on) => void record.run(() => dispatch({ type: "traffic_record", on }))}
      />
      {record.pending || record.error ? (
        <ActionStatus
          pending={record.pending}
          error={record.error && `Could not ${t.recording ? "stop" : "start"} the recording: ${record.error}`}
          className="mt-1"
        />
      ) : t.recording ? (
        <ActionStatus ambient pending={`Recording · ${trafficSummary(t)}`} className="mt-1">
          <span className="min-w-0 break-all text-white/60">{t.file_name}</span>
        </ActionStatus>
      ) : (
        saved && (
          <ActionStatus done={`Saved · ${trafficSummary(t)}`} className="mt-1">
            <InlineAction onClick={openFolder}>Open folder</InlineAction>
          </ActionStatus>
        )
      )}
      {saved && (
        <>
          <Hint className="mt-0.5 break-all">{t.file_path}</Hint>
          {t.stop_reason && <p className="mt-0.5 font-mono text-[11px] leading-relaxed text-neon-yellow">{t.stop_reason}</p>}
        </>
      )}
      <ActionStatus pending={folder.pending} error={folder.error && `Could not open the folder: ${folder.error}`} className="mt-1" />
    </div>
  );
}
