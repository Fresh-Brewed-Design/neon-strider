import type { AppSnapshot, ConnectionSnapshot } from "../ipc";
import { stasisClock } from "../format";
import { isMoving, linkState, linkView } from "../state/linkStatus";

export type LinkTone = "live" | "idle" | "down" | "warn";

/** The HUD header's status word, LED colour and, while paused, the STASIS clock ("3:20"). */
export interface LinkLabel {
  text: string;
  tone: LinkTone;
  clock?: string;
}

/** The HUD header's status word and LED colour. */
export function linkLabel(s: AppSnapshot): LinkLabel {
  const state = linkState(s.connection).state;
  if (!s.connection.connected || state === "no_data") {
    const v = linkView(s.connection, s.platform.settings);
    return { text: v.hudLabel, tone: v.tone === "problem" ? "down" : "warn" };
  }
  if (s.deck.unknown_status_code !== null) {
    // A status byte we don't know (screen off?): don't claim STANDBY.
    return { text: `STATE ?${s.deck.unknown_status_code}`, tone: "warn" };
  }
  // Standby: off or asleep, and it refuses starts.
  if (s.deck.standby) return { text: "DECK OFF", tone: "warn" };
  switch (s.deck.status) {
    case "Running":
      return { text: "RUNNING", tone: "live" };
    case "Starting":
      return { text: "SPIN-UP", tone: "live" };
    case "Paused":
      // Counts up while paused (Rust's clock), so a long pause is plain to see.
      return s.deck.paused_for_s === null
        ? { text: "STASIS", tone: "idle" }
        : { text: "STASIS", tone: "idle", clock: stasisClock(s.deck.paused_for_s) };
    case "Stopping":
      return { text: "SPIN-DOWN", tone: "idle" };
    default:
      return { text: "STANDBY", tone: "idle" };
  }
}

/** What the header's link label does when clicked. */
export interface RelinkView {
  /** A click does something: wakes the watchdog (`connect_now`), or opens the dashboard's connect dialog. */
  clickable: boolean;
  /** No treadmill is saved: a click opens the dashboard's connect dialog (`open_connect_dialog`). */
  opensConnectDialog: boolean;
  /** The label while a click is being answered; otherwise `linkLabel`'s text. */
  text: string | null;
  /** Tooltip, when there is something to say. */
  title: string | null;
}

/** Link states where "try the saved treadmill now" is the useful thing to do. */
const RELINKABLE = new Set(["reconnecting", "adapter_unavailable", "failed", "idle"]);

const INERT: RelinkView = { clickable: false, opensConnectDialog: false, text: null, title: null };

/**
 * The HUD's link label as a button while the link is down and nothing is
 * already scanning or linking: "Retry now" for the saved treadmill, or, with
 * none saved yet (nothing to retry), "open the dashboard's connect dialog".
 * `pending` is a retry click still waiting for the link state to move.
 */
export function relinkView(c: ConnectionSnapshot, pending: boolean): RelinkView {
  const state = linkState(c).state;
  const down = !c.connected && RELINKABLE.has(state);
  if (!down) return INERT;
  if (!c.has_saved_device) {
    return { clickable: true, opensConnectDialog: true, text: null, title: "No treadmill saved yet. Click to pick one." };
  }
  if (pending) return { ...INERT, text: "LINKING…", title: "Trying the saved treadmill now…" };
  // The dashboard's tooltip may end in its own "Click to …" hint.
  const title = linkView(c).title.replace(/\s*Click to [^.]*\.$/, "");
  const why = /[.?!]$/.test(title) ? title : `${title}.`;
  return { clickable: true, opensConnectDialog: false, text: null, title: `${why} Click to reconnect now.` };
}

/** Which link state a pending click is waiting to leave (the countdown ticking doesn't count). */
export const relinkKey = (c: ConnectionSnapshot) => `${c.connected}:${linkState(c).state}`;

/** The main button's word, the same on the panel and the strip. */
export function engageLabel(snapshot: AppSnapshot): string {
  const { deck, connection } = snapshot;
  const moving = isMoving(deck);
  if (moving) return "STASIS";
  if (connection.start_pending) return "WAKING…";
  // On a paused program the main button is its GO (Rust resumes the program).
  const a = snapshot.active_program;
  if (a?.phase === "walking" && a.paused) return "GO";
  return deck.status === "Paused" ? "RE-JACK" : "ENGAGE";
}

/** What the main button does, in plain words: the small line under its codename. */
export function engageSub(snapshot: AppSnapshot): string {
  const { deck, connection } = snapshot;
  if (isMoving(deck)) return "pause";
  if (connection.start_pending) return "starting";
  const a = snapshot.active_program;
  return deck.status === "Paused" || (a?.phase === "walking" && a.paused) ? "resume" : "start";
}

/** The main button's tooltip, the same words as `engageSub` in a sentence. */
export function engageTitle(snapshot: AppSnapshot): string {
  const { deck, connection } = snapshot;
  if (isMoving(deck)) return "Pause the belt.";
  if (connection.start_pending) return "Waiting for the treadmill to start.";
  const a = snapshot.active_program;
  if (a?.phase === "walking" && a.paused) return "Resume the program.";
  return deck.status === "Paused" ? "Resume the belt." : "Start the belt.";
}
