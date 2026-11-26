import { AnimatePresence } from "framer-motion";
import { Bluetooth, BluetoothOff, Link2, Loader2, RefreshCw, SearchX } from "lucide-react";
import { ActionStatus } from "../../components/ActionStatus";
import { PLAIN } from "../../names";
import { useAppSnapshot } from "../../state/useAppState";
import { DialogButtons, DialogCancelButton, DialogConfirmButton, DialogFrame } from "../DialogFrame";
import { useDialog } from "../useDialog";

export interface BleDevice {
  id: string;
  name: string;
}

export interface ConnectFailure {
  id: string;
  message: string;
}

interface BleDiscoveryModalProps {
  show: boolean;
  onClose: () => void;
  devices: BleDevice[];
  isScanning: boolean;
  /** At least one scan has finished since the dialog opened. */
  scanned: boolean;
  /** The scan failed; `message` is already plain English. */
  scanError: string | null;
  /** Bluetooth is off or missing: nothing can be found until it is on. */
  adapterOff: boolean;
  /** The device being linked right now. */
  connectingId: string | null;
  connectError: ConnectFailure | null;
  onScan: () => void;
  onConnect: (id: string) => void;
}

/**
 * The scan-and-connect dialog: found treadmills, each with its own linking
 * state and error. Escape closes it, like Close and a click outside.
 */
export function BleDiscoveryModal({
  show,
  onClose,
  devices,
  isScanning,
  scanned,
  scanError,
  adapterOff,
  connectingId,
  connectError,
  onScan,
  onConnect,
}: BleDiscoveryModalProps) {
  const busy = isScanning || connectingId !== null;
  const settings = useAppSnapshot()?.platform.settings ?? "Windows settings";
  const boxRef = useDialog(show, onClose);
  return (
    <AnimatePresence>
      {show && (
        <DialogFrame
          key="ble-discovery"
          boxRef={boxRef}
          titleId="ble-discovery-title"
          icon={Bluetooth}
          title="HARDWARE_LINK"
          subline={PLAIN.HARDWARE_LINK}
          onClose={onClose}
          onBackdrop={onClose}
          bodyClassName="max-h-[60vh] flex flex-col gap-2 p-4"
          footer={
            <DialogButtons>
              <DialogCancelButton onClick={onClose}>Cancel</DialogCancelButton>
              <DialogConfirmButton onClick={onScan} disabled={busy}>
                <RefreshCw size={14} className={isScanning ? "animate-spin" : ""} />
                Scan again
              </DialogConfirmButton>
            </DialogButtons>
          }
        >
              {devices.length === 0 ? (
                <div className="py-10 flex flex-col items-center justify-center gap-3 text-center" role="status">
                  <div className="w-12 h-12 rounded-full border border-dashed border-white/10 flex items-center justify-center">
                    {adapterOff ? (
                      <BluetoothOff className="text-red-400" size={24} />
                    ) : isScanning ? (
                      <RefreshCw className="text-neon-yellow animate-spin" size={24} />
                    ) : scanError ? (
                      <BluetoothOff className="text-red-400" size={24} />
                    ) : (
                      <SearchX className="text-white/60" size={24} />
                    )}
                  </div>
                  {adapterOff ? (
                    <div className="flex flex-col gap-1">
                      <p className="text-[11px] font-bold text-red-400 uppercase tracking-widest">Bluetooth is off</p>
                      <p className="text-xs text-white/60 break-words">
                        {scanError ?? `Turn Bluetooth on in ${settings}, then scan again.`}
                      </p>
                    </div>
                  ) : isScanning ? (
                    <div className="flex flex-col gap-1">
                      <p className="text-[11px] font-bold text-neon-yellow uppercase tracking-widest">Scanning for treadmills…</p>
                      <p className="text-xs text-white/60">Scanning takes about 5 seconds.</p>
                    </div>
                  ) : scanError ? (
                    <div className="flex flex-col gap-1">
                      <p className="text-[11px] font-bold text-red-400 uppercase tracking-widest">Scan failed</p>
                      <p className="text-xs text-white/60 break-words">{scanError}</p>
                    </div>
                  ) : scanned ? (
                    <div className="flex flex-col gap-1">
                      <p className="text-xs font-bold text-white/60 uppercase tracking-widest">No treadmills found</p>
                      <p className="text-xs text-white/60">
                        Make sure it&apos;s switched on and in range, then scan again.
                      </p>
                    </div>
                  ) : null}
                </div>
              ) : (
                devices.map((device) => {
                  const linking = connectingId === device.id;
                  const failure = connectError?.id === device.id ? connectError.message : null;
                  return (
                    <div key={device.id} className="flex flex-col gap-1.5">
                      <button
                        onClick={() => onConnect(device.id)}
                        disabled={connectingId !== null}
                        className="w-full p-4 bg-white/5 border border-white/5 rounded-xl hover:bg-neon-cyan/10 hover:border-neon-cyan/30 transition-all flex justify-between items-center group disabled:cursor-default disabled:hover:bg-white/5 disabled:hover:border-white/5"
                      >
                        <div className="flex flex-col items-start gap-1 min-w-0">
                          <span className="font-digital text-xs font-bold uppercase tracking-[0.12em]">{device.name || 'Unknown Device'}</span>
                          <span className="text-xs font-mono text-white/60 truncate max-w-[200px]">{device.id}</span>
                        </div>
                        {linking ? (
                          <span className="flex items-center gap-1.5 text-[11px] font-bold uppercase tracking-wider text-neon-yellow">
                            <Loader2 size={14} className="animate-spin" /> Linking…
                          </span>
                        ) : (
                          <Link2 className="text-white/60 group-hover:text-neon-cyan transition-colors" size={16} />
                        )}
                      </button>
                      <ActionStatus
                        pending={linking ? "Linking to the treadmill takes up to 20 seconds." : null}
                        error={failure}
                        className="px-1"
                      >
                        {failure && !busy && (
                          <button
                            type="button"
                            onClick={() => onConnect(device.id)}
                            className="font-bold uppercase tracking-wider text-neon-cyan hover:underline"
                          >
                            Retry
                          </button>
                        )}
                      </ActionStatus>
                    </div>
                  );
                })
              )}
              {devices.length > 0 && scanError && <ActionStatus error={scanError} className="px-1" />}
        </DialogFrame>
      )}
    </AnimatePresence>
  );
}
