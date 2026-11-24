import { useState } from "react";
import { Loader2, RefreshCw } from "lucide-react";
import { ActionStatus } from "../../../components/ActionStatus";
import { useAction, useBackendAction } from "../../../components/useAction";
import { SETTING_LIMITS, type MuteMode, type StravaSnapshot } from "../../../ipc";
import { GhostButton, Hint, Label } from "../../ui";
import { cx, post } from "../../util";
import { ChoiceGroup, INPUT_CLASS } from "../fields";
import { TemplateEditor } from "./TemplateEditor";

const MUTE_CHOICES: { value: MuteMode; label: string; hint: string }[] = [
  { value: "all", label: "Mute all", hint: "Every treadmill walk is hidden from your home feed." },
  { value: "short", label: "Short walks", hint: "Only walks under the minute limit are hidden." },
  { value: "none", label: "Don't mute", hint: "Walks show in your home feed." },
];

/**
 * What every upload carries: the shoe, whether it is muted from the feed, and the
 * title and description templates. Rust applies them; this only edits the settings
 * and asks Rust to render a sample.
 */
export function UploadOptions({ strava }: { strava: StravaSnapshot }) {
  return (
    <div className="flex flex-col gap-4 border-t border-white/10 pt-4">
      <GearPicker strava={strava} />
      <MuteChoice strava={strava} />
      <TemplateEditor strava={strava} />
    </div>
  );
}

function GearPicker({ strava }: { strava: StravaSnapshot }) {
  const shoes = strava.gear.filter((g) => !g.retired);
  const current = strava.gear_id;
  // A saved shoe that is not in the list (retired, or the list is empty) stays visible.
  const missing = current !== null && !shoes.some((g) => g.id === current);
  const savedName = strava.gear.find((g) => g.id === current)?.name;
  const refresh = useBackendAction(strava.last_result, strava.gear_loading);
  const pick = useAction();
  const loading = strava.gear_loading || refresh.pending;

  return (
    <div className="flex flex-col gap-1">
      <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
        <Label>Shoes</Label>
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <select
            aria-label="Shoes for every upload"
            disabled={!strava.connected}
            value={current ?? ""}
            onChange={(e) => void pick.run(post({ type: "strava_set_gear", gear_id: e.target.value || null }))}
            className={cx(INPUT_CLASS, "max-w-64 truncate [color-scheme:dark] disabled:opacity-40")}
          >
            <option value="">None</option>
            {missing && <option value={current}>{savedName ?? "Saved shoe"} (not in list)</option>}
            {shoes.map((g) => (
              <option key={g.id} value={g.id}>
                {g.name} · {g.distance_km.toFixed(1)} km
              </option>
            ))}
          </select>
          <GhostButton
            aria-label="Refresh shoes"
            disabled={!strava.connected || loading}
            onClick={() => void refresh.run(post({ type: "strava_refresh_gear" }))}
            className="flex items-center gap-1.5"
          >
            {loading ? <Loader2 size={12} className="animate-spin" /> : <RefreshCw size={12} />}
            {loading ? "Loading…" : "Refresh"}
          </GhostButton>
        </div>
      </div>
      <ActionStatus
        pending={loading && "Asking Strava for your shoes…"}
        error={pick.error ?? refresh.error}
        done={refresh.done}
      />
      <Hint>
        {strava.needs_reconnect
          ? "Strava needs two more permissions (shoes, duplicate check). Disconnect, connect again and keep every box ticked."
          : "The chosen shoes go on every upload, and each pair shows Strava's logged distance."}
      </Hint>
    </div>
  );
}

function MuteChoice({ strava }: { strava: StravaSnapshot }) {
  const [minutesDraft, setMinutesDraft] = useState<string | null>(null);
  const minutes = minutesDraft ?? String(strava.mute_under_minutes);
  const hint = MUTE_CHOICES.find((c) => c.value === strava.mute_mode)?.hint;
  const mute = useAction();

  const commitMinutes = () => {
    const n = Math.round(Number(minutesDraft));
    setMinutesDraft(null);
    if (minutesDraft !== null && Number.isFinite(n) && n >= 1 && n <= 240 && n !== strava.mute_under_minutes) {
      void mute.run(post({ type: "strava_set_mute", mode: strava.mute_mode, under_minutes: n }));
    }
  };

  return (
    <div className="flex flex-col gap-2">
      <Label>Home feed</Label>
      <ChoiceGroup
        label="Mute from home feed"
        tone="magenta"
        options={MUTE_CHOICES}
        value={strava.mute_mode}
        onChange={(mode) =>
          void mute.run(post({ type: "strava_set_mute", mode, under_minutes: strava.mute_under_minutes }))
        }
      />
      <div className="flex items-center justify-between gap-3">
        <Hint>{hint}</Hint>
        {strava.mute_mode === "short" && (
          <label className="flex shrink-0 items-center gap-2">
            <span className="font-mono text-xs text-white/60">Under</span>
            <input
              type="number"
              min={SETTING_LIMITS.strava_mute_minutes.min}
              max={SETTING_LIMITS.strava_mute_minutes.max}
              value={minutes}
              onChange={(e) => setMinutesDraft(e.target.value)}
              onBlur={commitMinutes}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              className={cx(INPUT_CLASS, "w-16")}
            />
            <span className="font-mono text-xs text-white/60">min</span>
          </label>
        )}
      </div>
      <ActionStatus error={mute.error} />
    </div>
  );
}

