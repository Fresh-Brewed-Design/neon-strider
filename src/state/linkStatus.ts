// How the treadmill link is described: one view per link state, for the rail,
// the Live page and the HUD header.

import type { AppSnapshot, ConnectionSnapshot, DeckSnapshot, LinkState } from "../ipc";
import { PLAIN } from "../names";

/** One vocabulary for how things are going: cyan ok, yellow working or needs a look, red a problem. */
export type StatusTone = "ok" | "working" | "problem" | "idle";

export interface LinkView {
  tone: StatusTone;
  /** The rail pill: short, upper case. */
  label: string;
  /** The HUD header: the same word as the rail, one line at 340 px. */
  hudLabel: string;
  /** The plain line under the rail pill (NO_CARRIER: treadmill offline); null for none. */
  sub: string | null;
  /** A plain sentence for the Live page. */
  headline: string;
  /** Longer text for a tooltip, including the last error. */
  title: string;
  /** "Retry now" makes sense (the watchdog is waiting). */
  canRetry: boolean;
}

/** The state to show, falling back to the old flags if `link` is missing. */
export function linkState(c: ConnectionSnapshot): LinkState {
  if (c.link) return c.link;
  if (c.connected) return { state: "connected" };
  if (c.connecting) return { state: "connecting", attempt: 1, of: 3 };
  return { state: "idle" };
}

/** `settings`: where the OS switches Bluetooth on (`snapshot.platform.settings`). */
export function linkView(c: ConnectionSnapshot, settings = "Windows settings"): LinkView {
  const l = linkState(c);
  const withError = (text: string) => (c.last_error ? `${text} ${c.last_error.message}` : text);
  switch (l.state) {
    case "scanning":
      return {
        tone: "working",
        label: "SCANNING",
        hudLabel: "SCANNING",
        sub: null,
        headline: "Scanning for treadmills…",
        title: "Scanning for treadmills. Click to open the connect dialog.",
        canRetry: false,
      };
    case "connecting":
      return {
        tone: "working",
        label: `LINKING ${l.attempt}/${l.of}`,
        hudLabel: `LINKING ${l.attempt}/${l.of}`,
        sub: null,
        headline: `Linking to treadmill… try ${l.attempt} of ${l.of}`,
        title: `Linking to the treadmill, try ${l.attempt} of ${l.of}.`,
        canRetry: false,
      };
    case "connected":
      return {
        tone: "ok",
        label: "LINKED",
        hudLabel: "LINKED",
        sub: null,
        headline: "Linked",
        title: "Linked. Click to pick a different treadmill.",
        canRetry: false,
      };
    case "reconnecting":
      return {
        tone: "working",
        label: `RELINK ${l.next_retry_s}S`,
        hudLabel: `RELINK ${l.next_retry_s}S`,
        sub: null,
        headline: `Treadmill link is down. Trying again in ${l.next_retry_s} s (try ${l.attempt}).`,
        title: withError(`Trying again in ${l.next_retry_s} s.`),
        canRetry: true,
      };
    case "adapter_unavailable":
      return {
        tone: "problem",
        label: "BT OFF",
        hudLabel: "BT OFF",
        sub: null,
        headline: `Bluetooth is off. Turn it on in ${settings}.`,
        title: c.last_error?.message ?? "Bluetooth is off or there is no Bluetooth adapter.",
        canRetry: true,
      };
    case "failed":
      return {
        tone: "problem",
        label: "NO_CARRIER",
        hudLabel: "NO_CARRIER",
        sub: PLAIN.NO_CARRIER,
        headline: l.message,
        title: `${l.message} Click to try again.`,
        canRetry: false,
      };
    case "no_data":
      return {
        tone: "working",
        label: "NO SIGNAL",
        hudLabel: "NO SIGNAL",
        sub: null,
        headline: "Treadmill asleep? Press a button on it or the remote.",
        title: `Linked, but no data for ${l.since_s} s. The treadmill may be asleep or out of range.`,
        canRetry: false,
      };
    default:
      return {
        tone: "problem",
        label: "NO_CARRIER",
        hudLabel: "NO_CARRIER",
        sub: PLAIN.NO_CARRIER,
        headline: "Treadmill offline",
        title: c.last_error?.message ?? "Find and connect a treadmill",
        canRetry: false,
      };
  }
}

/** The belt is turning or spinning up. */
export const isMoving = (deck: DeckSnapshot) => deck.status === "Running" || deck.status === "Starting";

/** The last Engage went unanswered, while the belt is still not moving. */
export function startFailure(s: AppSnapshot): string | null {
  const r = s.connection.start_result;
  return r && !r.ok && !isMoving(s.deck) ? r.message : null;
}
