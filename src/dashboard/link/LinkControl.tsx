import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { errorText } from "../../components/useAction";
import type { LinkError, LinkErrorKind } from "../../ipc";
import { linkView } from "../../state/linkStatus";
import { subscribeAppSnapshot, useAppSnapshot } from "../../state/useAppState";
import { readPref, writePref } from "../localPrefs";
import { PLAIN } from "../../names";
import { StatusPill } from "../StatusPill";
import { PlainLine } from "../ui";
import { OPEN_LINK_EVENT } from "../util";
import { BleDiscoveryModal, type BleDevice, type ConnectFailure } from "./BleDiscoveryModal";
import { CONNECT_SHOWN_KEY, isNewConnectRequest } from "./connectRequest";

/** What a rejected scan or connect call carries: a `LinkError` from Rust, or a plain message. */
function linkFailure(e: unknown): LinkError {
  if (e && typeof e === "object" && "message" in e && typeof e.message === "string") {
    const kind = "kind" in e && typeof e.kind === "string" ? (e.kind as LinkErrorKind) : "generic";
    return { kind, message: e.message };
  }
  return { kind: "generic", message: errorText(e) };
}

/** Device link status plus the connect flow, at the foot of the rail. */
export function LinkControl() {
  const snapshot = useAppSnapshot();
  const conn = snapshot?.connection;
  const [open, setOpen] = useState(false);
  const [scanning, setScanning] = useState(false);
  const [scanned, setScanned] = useState(false);
  const [scanError, setScanError] = useState<LinkError | null>(null);
  const [devices, setDevices] = useState<BleDevice[]>([]);
  const [connectingId, setConnectingId] = useState<string | null>(null);
  const [connectError, setConnectError] = useState<ConnectFailure | null>(null);

  const scan = useCallback(async () => {
    setOpen(true);
    setScanning(true);
    setScanError(null);
    setConnectError(null);
    try {
      setDevices(await invoke<BleDevice[]>("scan_bluetooth_devices"));
    } catch (e) {
      console.error("[dashboard] scan failed", e);
      setScanError(linkFailure(e));
    } finally {
      setScanning(false);
      setScanned(true);
    }
  }, []);

  // Other pages (e.g. Live's Connect button) open this same dialog.
  useEffect(() => {
    const onOpen = () => void scan();
    window.addEventListener(OPEN_LINK_EVENT, onOpen);
    return () => window.removeEventListener(OPEN_LINK_EVENT, onOpen);
  }, [scan]);

  // The HUD's link label with no treadmill saved: open the dialog, once per request.
  useEffect(
    () =>
      subscribeAppSnapshot((s) => {
        const at = s.connection.connect_dialog_at_ms;
        if (!isNewConnectRequest(at, readPref(CONNECT_SHOWN_KEY))) return;
        writePref(CONNECT_SHOWN_KEY, String(at));
        void scan();
      }),
    [scan],
  );

  const connect = async (id: string) => {
    setConnectingId(id);
    setConnectError(null);
    try {
      await invoke("connect_bluetooth_device", { id });
      setOpen(false);
    } catch (e) {
      console.error("[dashboard] connect failed", e);
      setConnectError({ id, message: linkFailure(e).message });
    } finally {
      setConnectingId(null);
    }
  };

  const view = conn ? linkView(conn, snapshot?.platform.settings) : null;
  const sub = view ? view.sub : PLAIN.NO_CARRIER;
  const adapterOff =
    scanError?.kind === "adapter_off" ||
    scanError?.kind === "adapter_missing" ||
    conn?.link?.state === "adapter_unavailable";

  return (
    <>
      <button
        type="button"
        onClick={() => void scan()}
        title={view?.title ?? "Find and connect a treadmill"}
        className="dash-rail-item flex w-full items-center gap-2 rounded px-2.5 py-2 text-left transition-colors hover:bg-white/5"
      >
        <span className="flex min-w-0 flex-col">
          <StatusPill
            bare
            icon="bluetooth"
            tone={view?.tone ?? "problem"}
            label={view?.label ?? "NO_CARRIER"}
            className="tracking-[0.12em]"
            labelClassName="dash-rail-text"
          />
          {sub && <PlainLine text={sub} className="dash-rail-text pl-5" />}
        </span>
        {conn?.simulated && (
          <span
            className="ml-auto shrink-0 rounded border border-neon-yellow/60 px-1 text-[11px] font-bold tracking-[0.12em] text-neon-yellow"
            title="Simulated treadmill (dev build)"
          >
            SIM
          </span>
        )}
      </button>
      <BleDiscoveryModal
        show={open}
        onClose={() => setOpen(false)}
        devices={devices}
        isScanning={scanning}
        scanned={scanned}
        scanError={scanError?.message ?? null}
        adapterOff={adapterOff}
        connectingId={connectingId}
        connectError={connectError}
        onScan={() => void scan()}
        onConnect={(id) => void connect(id)}
      />
    </>
  );
}
