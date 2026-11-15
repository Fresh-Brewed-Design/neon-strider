import { useBackendAction } from "../../../components/useAction";
import { useAppState } from "../../../state/useAppState";
import { StatusPill } from "../../StatusPill";
import { activityText, stravaPill } from "../../stravaStatus";
import { Section } from "../fields";
import { Account, Credentials } from "./Account";
import { AutoSyncOptions, SyncActions } from "./Sync";
import { UploadOptions } from "./UploadOptions";

/**
 * Strava settings. Rust owns all of it (credentials, login, auto-sync,
 * uploads); this shows `snapshot.strava` and dispatches commands. Every button
 * shows its own working state and puts its result directly under it.
 */
export function StravaSection() {
  const strava = useAppState((s) => s.strava);
  const result = strava?.last_result ?? null;
  const busy = (strava?.syncing ?? false) || strava?.activity.state === "authorizing";
  const connect = useBackendAction(result, busy);
  const pill = strava ? stravaPill(strava) : null;

  return (
    <Section
      id="sys-strava"
      title="Strava"
      description="Upload finished sessions to your Strava account."
      status={pill && <StatusPill tone={pill.tone} label={pill.label} title={pill.title} />}
    >
      {strava ? (
        <div className="flex flex-col gap-4">
          <Credentials strava={strava} />
          <Account strava={strava} connect={connect} activityText={activityText(strava.activity)} />
          <AutoSyncOptions strava={strava} />
          <UploadOptions strava={strava} />
          <SyncActions strava={strava} connect={connect} busy={busy} activityText={activityText(strava.activity)} />
        </div>
      ) : (
        <p className="font-mono text-xs text-white/40">Waiting for core link…</p>
      )}
    </Section>
  );
}
