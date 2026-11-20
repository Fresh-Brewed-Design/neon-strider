import { useCallback, useEffect, useState } from "react";
import type { ConnectionSnapshot } from "../ipc";
import { relinkKey, relinkView } from "./status";
import { useHudSend } from "./useHudSend";

/** A click with no visible effect stops showing LINKING… after this long: a scan (5 s) and a moment. */
const PENDING_MS = 6000;

/**
 * Clicking the HUD's link label: `connect_now` (the watchdog's "Retry now").
 * While the click waits for the link state to move, the label reads LINKING…;
 * a rejection shows as a `<HudNote>` under the header. With no treadmill saved
 * the click sends `open_connect_dialog` instead: the dashboard opens on its
 * connect dialog.
 */
export function useRelink(connection: ConnectionSnapshot | null) {
  const { send, error } = useHudSend();
  // The link state the click was made in; cleared once it changes.
  const [pendingFrom, setPendingFrom] = useState<string | null>(null);
  const key = connection ? relinkKey(connection) : null;
  // The state moved on (linking, linked, or a new failure): the click is answered.
  // Reset here rather than compare later, or a return to the same state would
  // look pending again.
  if (pendingFrom !== null && pendingFrom !== key) setPendingFrom(null);
  const pending = pendingFrom !== null && pendingFrom === key;

  useEffect(() => {
    if (!pending) return;
    const t = setTimeout(() => setPendingFrom(null), PENDING_MS);
    return () => clearTimeout(t);
  }, [pending]);

  const view = connection ? relinkView(connection, pending) : null;
  const clickable = view?.clickable ?? false;
  const opensConnectDialog = view?.opensConnectDialog ?? false;

  const relink = useCallback(() => {
    if (!clickable || key === null) return;
    if (opensConnectDialog) {
      void send({ type: "open_connect_dialog" });
      return;
    }
    setPendingFrom(key);
    void send({ type: "connect_now" }).then((ok) => {
      if (!ok) setPendingFrom(null);
    });
  }, [clickable, opensConnectDialog, key, send]);

  return { view, relink, error };
}
