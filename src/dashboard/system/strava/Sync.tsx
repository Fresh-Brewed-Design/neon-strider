import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FileDown, FolderOpen, Loader2, RefreshCw } from "lucide-react";
import { ActionStatus, InlineAction } from "../../../components/ActionStatus";
import { useAction, useBackendAction, type BackendAction } from "../../../components/useAction";
import type { SavedFile, StravaSnapshot } from "../../../ipc";
import { Hint, Label, NeonButton } from "../../ui";
import { post } from "../../util";
import { INPUT_CLASS, Toggle } from "../fields";

const HH_MM = /^([01]\d|2[0-3]):[0-5]\d$/;

/** Auto-sync and the daily rollup, with the time the rollup goes up. */
export function AutoSyncOptions({ strava }: { strava: StravaSnapshot }) {
  const [timeDraft, setTimeDraft] = useState<string | null>(null);
  const toggles = useAction();

  const changeShiftEnd = (value: string) => {
    setTimeDraft(value);
    // The time input reports "" while it is being cleared; only complete times are sent.
    if (HH_MM.test(value)) {
      void toggles.run(post({ type: "strava_set_daily_rollup", enabled: strava.daily_rollup, shift_end: value }));
    }
  };

  return (
    <div className="flex flex-col gap-4 border-t border-white/10 pt-4">
      <Toggle
        label="Auto-sync"
        description="Uploads finished walks in the background, even with the dashboard closed."
        hint="Walks that ended before auto-sync was on stay manual, in History."
        tone="magenta"
        checked={strava.auto_sync}
        onChange={(v) => void toggles.run(post({ type: "strava_set_auto_sync", enabled: v }))}
      />
      <Toggle
        label="Daily rollup"
        description="Uploads each day's walks as one Strava activity after the rollup time."
        hint="A day missed while Neon Strider was closed uploads on the next start."
        tone="magenta"
        checked={strava.daily_rollup}
        onChange={(v) =>
          void toggles.run(post({ type: "strava_set_daily_rollup", enabled: v, shift_end: strava.shift_end }))
        }
      />
      <label className="flex items-center justify-between gap-4">
        <Label>Rollup time</Label>
        <input
          type="time"
          value={timeDraft ?? strava.shift_end}
          onChange={(e) => changeShiftEnd(e.target.value)}
          onBlur={() => setTimeDraft(null)}
          className={INPUT_CLASS}
        />
      </label>
      <ActionStatus error={toggles.error} />
    </div>
  );
}

/**
 * Sync now and Save for Strava, then what Rust is doing right now and how the
 * last attempt went. Each result is shown once: under the button that caused
 * it, or as the last-known result.
 */
export function SyncActions({
  strava,
  connect,
  busy,
  activityText,
}: {
  strava: StravaSnapshot;
  connect: BackendAction;
  /** Rust is syncing or waiting for the browser login. */
  busy: boolean;
  activityText: string | null;
}) {
  const result = strava.last_result;
  const sync = useBackendAction(result, busy);
  const save = useAction();
  const folder = useAction();
  const apiSettings = useAction();
  const { connected, activity } = strava;
  const authorizing = activity.state === "authorizing";
  const stalled = activity.state === "backing_off" || activity.state === "paused";

  const saveForStrava = () =>
    void save.run(() => invoke<SavedFile>("strava_save_for_strava", { sessionIds: null }), {
      done: (f) => `Saved ${f.file_name}. Drop the file on the open Strava page.`,
    });

  const shownElsewhere = [connect.error, connect.done, sync.error, sync.done, save.error, save.done];
  const showResult = result !== null && !shownElsewhere.includes(result.message) && !busy && !stalled;
  const needsSettings =
    activity.state === "paused" || (result !== null && !result.ok && result.hint === "open_api_settings");
  const openApiSettings = (
    <InlineAction onClick={() => void apiSettings.run(post({ type: "strava_open_api_settings" }))}>
      Open Strava API settings
    </InlineAction>
  );

  return (
    <div className="flex flex-col gap-2 border-t border-white/10 pt-4">
      <div className="flex flex-wrap items-center gap-2">
        <NeonButton
          tone="magenta"
          disabled={!connected || !strava.auto_sync || busy || sync.pending}
          onClick={() => void sync.run(post({ type: "strava_sync_now" }))}
          className="flex shrink-0 items-center gap-2 px-3 py-2"
        >
          {busy || sync.pending ? <Loader2 size={14} className="animate-spin" /> : <RefreshCw size={14} />}
          {busy || sync.pending ? "Syncing…" : "Sync now"}
        </NeonButton>
        <NeonButton
          tone="cyan"
          disabled={save.pending}
          onClick={saveForStrava}
          title="Save the latest walk as a TCX file for Strava's upload page, which works without a linked account."
          className="flex shrink-0 items-center gap-2 px-3 py-2"
        >
          {save.pending ? <Loader2 size={14} className="animate-spin" /> : <FileDown size={14} />}
          {save.pending ? "Saving…" : "Save for Strava"}
        </NeonButton>
      </div>
      {!connected && (
        <Hint>
          Sync now needs a connected account while Save for Strava works with no account.
        </Hint>
      )}
      {connected && !strava.auto_sync && (
        <Hint>Turn auto-sync on to use Sync now.</Hint>
      )}

      <ActionStatus pending={save.pending && "Writing the file…"} error={save.error} done={save.done}>
        {save.done && (
          <InlineAction onClick={() => void folder.run(post({ type: "strava_open_save_folder" }))}>
            <span className="inline-flex items-center gap-1">
              <FolderOpen size={12} /> Open folder
            </span>
          </InlineAction>
        )}
      </ActionStatus>
      <ActionStatus error={folder.error} />

      {stalled ? (
        <ActionStatus ambient error={activityText}>
          <InlineAction onClick={() => void sync.run(post({ type: "strava_retry_now" }))} disabled={sync.pending}>
            Retry now
          </InlineAction>
          {needsSettings && openApiSettings}
        </ActionStatus>
      ) : (
        <ActionStatus
          pending={(busy || sync.pending) && !authorizing && (activityText ?? "Working…")}
          error={sync.error}
          done={sync.done}
        >
          {needsSettings && !busy && openApiSettings}
        </ActionStatus>
      )}
      <ActionStatus error={apiSettings.error} />
      {showResult && <LastResult result={result} />}
      {result === null && !busy && <p className="font-mono text-xs text-white/60">Nothing uploaded yet.</p>}
    </div>
  );
}

function LastResult({ result }: { result: NonNullable<StravaSnapshot["last_result"]> }) {
  return (
    <Hint className="min-w-0 break-words" role="status">
      Last result: <span className={result.ok ? "text-emerald-400/80" : "text-red-400/80"}>{result.message}</span>
      <span className="text-white/60"> · {new Date(result.at_ms).toLocaleString()}</span>
    </Hint>
  );
}
