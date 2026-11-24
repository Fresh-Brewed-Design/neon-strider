import { useState } from "react";
import { ExternalLink, Loader2, ShieldCheck } from "lucide-react";
import { ActionStatus, InlineAction } from "../../../components/ActionStatus";
import { useAction, type BackendAction } from "../../../components/useAction";
import { dispatch, type StravaSnapshot } from "../../../ipc";
import { GhostButton, Label, NeonButton } from "../../ui";
import { post } from "../../util";
import { INPUT_CLASS } from "../fields";

/** The Strava API app's client ID and secret. The secret is write-only: Rust never sends it back. */
export function Credentials({ strava }: { strava: StravaSnapshot }) {
  const [editing, setEditing] = useState(false);
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const credentials = useAction();
  const canSave = clientId.trim() !== "" && clientSecret.trim() !== "";

  const save = async () => {
    const ok = await credentials.run(
      async () => {
        await dispatch({ type: "strava_set_credentials", client_id: clientId.trim(), client_secret: clientSecret.trim() });
        return true;
      },
      { done: "Credentials saved." },
    );
    if (ok) {
      setClientId("");
      setClientSecret("");
      setEditing(false);
    }
  };

  if (strava.has_credentials && !editing) {
    return (
      <div className="flex flex-col gap-1">
        <div className="flex items-center justify-between gap-3">
          <span className="font-mono text-xs text-white/60">Client ID and secret saved.</span>
          <GhostButton onClick={() => setEditing(true)}>Replace</GhostButton>
        </div>
        <ActionStatus done={credentials.done} />
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="grid grid-cols-2 gap-3">
        <label className="block">
          <Label>Client ID</Label>
          <input
            type="text"
            autoComplete="off"
            placeholder="From your Strava API app"
            value={clientId}
            onChange={(e) => setClientId(e.target.value)}
            className={`${INPUT_CLASS} mt-1 w-full`}
          />
        </label>
        <label className="block">
          <Label>Client secret</Label>
          <input
            type="password"
            autoComplete="off"
            placeholder="From your Strava API app"
            value={clientSecret}
            onChange={(e) => setClientSecret(e.target.value)}
            className={`${INPUT_CLASS} mt-1 w-full`}
          />
        </label>
      </div>
      <div className="flex items-center gap-2">
        <NeonButton
          tone="magenta"
          disabled={!canSave || credentials.pending}
          onClick={() => void save()}
          className="flex items-center gap-2 px-4 py-2"
        >
          {credentials.pending && <Loader2 size={14} className="animate-spin" />}
          {credentials.pending ? "Saving…" : "Save credentials"}
        </NeonButton>
        {strava.has_credentials && <GhostButton onClick={() => setEditing(false)}>Cancel</GhostButton>}
      </div>
      <ActionStatus error={credentials.error} />
    </div>
  );
}

/**
 * Linking the Strava account: Connect opens the browser and waits for the
 * approval; once linked, Disconnect. `connect` is shared with the sync block,
 * which shows the latest result without repeating this line. Under both sits
 * Rust's note while the login is kept outside Credential Manager.
 */
export function Account({
  strava,
  connect,
  activityText,
}: {
  strava: StravaSnapshot;
  connect: BackendAction;
  /** What Strava is doing right now, if anything. */
  activityText: string | null;
}) {
  const cancel = useAction();
  const disconnect = useAction();
  const authorizing = strava.activity.state === "authorizing";
  const result = strava.last_result;
  const startConnect = () => void connect.run(post({ type: "strava_connect" }));
  const working = activityText ?? "Opening the browser…";

  if (strava.connected) {
    const needsReconnect = result !== null && !result.ok && result.hint === "reconnect";
    return (
      <div className="flex flex-col gap-1">
        <div className="flex items-center justify-between gap-3 rounded border border-neon-magenta/30 bg-neon-magenta/5 px-3 py-2">
          <span className="flex min-w-0 items-center gap-2 font-mono text-xs font-bold uppercase tracking-[0.12em] text-neon-magenta">
            <ShieldCheck size={14} className="shrink-0" />
            <span className="truncate">Account linked{strava.athlete_name ? ` · ${strava.athlete_name}` : ""}</span>
          </span>
          <GhostButton
            disabled={disconnect.pending}
            onClick={() => void disconnect.run(post({ type: "strava_disconnect" }))}
          >
            {disconnect.pending ? "Disconnecting…" : "Disconnect"}
          </GhostButton>
        </div>
        <ActionStatus error={disconnect.error}>
          {needsReconnect && (
            <InlineAction onClick={startConnect} disabled={connect.pending}>
              Reconnect Strava
            </InlineAction>
          )}
        </ActionStatus>
        <ActionStatus pending={connect.pending && working} error={connect.error} done={connect.done} />
        <ActionStatus ambient error={strava.login_storage} />
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center gap-3">
        <NeonButton
          tone="magenta"
          disabled={!strava.has_credentials || connect.pending || authorizing}
          onClick={startConnect}
          className="flex items-center gap-2 px-4 py-2"
        >
          {connect.pending || authorizing ? <Loader2 size={14} className="animate-spin" /> : <ExternalLink size={14} />}
          {authorizing ? "Waiting for Strava…" : connect.pending ? "Opening browser…" : "Connect account"}
        </NeonButton>
        {authorizing && (
          <GhostButton disabled={cancel.pending} onClick={() => void cancel.run(post({ type: "strava_cancel_connect" }))}>
            Cancel
          </GhostButton>
        )}
        {!strava.has_credentials && (
          <span className="font-mono text-xs text-white/60">Save your credentials first.</span>
        )}
      </div>
      <ActionStatus
        pending={(connect.pending || authorizing) && working}
        error={cancel.error ?? connect.error}
        done={connect.done}
      />
      <ActionStatus ambient error={strava.login_storage} />
    </div>
  );
}
