// Plain-English text and pill state for what Strava is doing, from the snapshot.

import type { StravaActivity, StravaSnapshot, StravaStep } from "../ipc";
import { mmss } from "../format";
import type { StatusTone } from "../state/linkStatus";

const STEP_TEXT: Record<StravaStep, string> = {
  preparing: "Preparing the file…",
  sending: "Sending to Strava…",
  processing: "Strava is processing…",
  finishing: "Adding shoes and mute…",
};

/** One line saying what Strava is doing, or null when idle. */
export function activityText(a: StravaActivity): string | null {
  switch (a.state) {
    case "authorizing":
      return `Waiting for you to approve in the browser (${mmss(a.expires_in_s)} left)`;
    case "uploading":
      return a.label ? `${STEP_TEXT[a.step]} ${a.label}` : STEP_TEXT[a.step];
    case "checking_duplicates":
      return "Checking Strava for a duplicate upload…";
    case "backing_off":
      return `Trying again in ${mmss(a.retry_in_s)}. ${a.reason}`;
    case "paused":
      return `Auto-sync is paused. ${a.reason}`;
    case "queued":
      return `${a.count} uploads waiting…`;
    default:
      return null;
  }
}

export interface StravaPill {
  tone: StatusTone;
  label: string;
  title: string;
}

/** The pill in the Strava section header. */
export function stravaPill(s: StravaSnapshot): StravaPill {
  const a = s.activity;
  const text = activityText(a) ?? "";
  switch (a.state) {
    case "authorizing":
      return { tone: "working", label: `APPROVE ${mmss(a.expires_in_s)}`, title: text };
    case "uploading":
      return { tone: "working", label: "UPLOADING", title: text };
    case "checking_duplicates":
      return { tone: "working", label: "CHECKING", title: text };
    case "queued":
      return { tone: "working", label: `QUEUED ${a.count}`, title: text };
    case "paused":
      return { tone: "problem", label: "PAUSED", title: text };
    case "backing_off":
      return { tone: "problem", label: `RETRY ${mmss(a.retry_in_s)}`, title: text };
    default:
      break;
  }
  if (s.needs_reconnect) return { tone: "problem", label: "RECONNECT", title: "Strava needs a fresh login." };
  if (s.last_result && !s.last_result.ok) {
    return { tone: "problem", label: "FAILED", title: s.last_result.message };
  }
  if (s.connected) return { tone: "ok", label: "LINKED", title: s.athlete_name ?? "Linked to Strava" };
  return { tone: "idle", label: "NOT LINKED", title: "Not connected to Strava" };
}
