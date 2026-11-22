import type { AppSnapshot, ConnectionSnapshot } from "../ipc";
import { isMoving, linkState, linkView } from "../state/linkStatus";

export type LinkTone = "live" | "idle" | "down" | "warn";

/** The HUD header's status word and LED colour. */
export function linkLabel(s: AppSnapshot): { text: string; tone: LinkTone } {
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
      return { text: "STASIS", tone: "idle" };
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
