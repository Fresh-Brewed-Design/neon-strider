import { AnimatePresence, motion } from "framer-motion";
import { Bluetooth, BluetoothOff, Link2, Loader2, RefreshCw, SearchX, X } from "lucide-react";
import { ActionStatus } from "../../components/ActionStatus";
import { useAppSnapshot } from "../../state/useAppState";

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

/** The scan-and-connect dialog: found treadmills, each with its own linking state and error. */
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
  return (
    <AnimatePresence>
      {show && (
        <div className="fixed inset-0 z-[100] flex items-center justify-center p-4">
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            onClick={onClose}
            className="absolute inset-0 bg-black/80 backdrop-blur-md"
          />

          <motion.div
            initial={{ opacity: 0, scale: 0.9, y: 20 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.9, y: 20 }}
            className="relative w-full max-w-md bg-[#0a0a0a] border border-neon-cyan/30 rounded-2xl overflow-hidden shadow-[0_0_50px_rgba(0,243,255,0.15)]"
          >
            <div className="flex justify-between items-center p-4 border-b border-white/5 bg-white/5">
              <div className="flex items-center gap-2">
                <Bluetooth className="text-neon-cyan" size={18} />
                <h2 className="text-xs font-black uppercase tracking-[0.2em]">Hardware_Link</h2>
              </div>
              <button
                onClick={onClose}
                aria-label="Close"
                className="p-1 hover:bg-white/10 rounded transition-colors"
              >
                <X size={18} className="text-white/40" />
              </button>
            </div>

            <div className="p-4 max-h-[60vh] overflow-y-auto flex flex-col gap-2">
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
                      <SearchX className="text-white/50" size={24} />
                    )}
                  </div>
                  {adapterOff ? (
                    <div className="flex flex-col gap-1">
                      <p className="text-[11px] font-bold text-red-400 uppercase tracking-widest">Bluetooth is off</p>
                      <p className="text-[11px] text-white/60 break-words">
                        {scanError ?? `Turn Bluetooth on in ${settings}, then scan again.`}
                      </p>
                    </div>
                  ) : isScanning ? (
                    <div className="flex flex-col gap-1">
                      <p className="text-[11px] font-bold text-neon-yellow uppercase tracking-widest">Scanning for treadmills…</p>
                      <p className="text-[11px] text-white/50">This takes about 5 seconds.</p>
                    </div>
                  ) : scanError ? (
                    <div className="flex flex-col gap-1">
                      <p className="text-[11px] font-bold text-red-400 uppercase tracking-widest">Scan failed</p>
                      <p className="text-[11px] text-white/60 break-words">{scanError}</p>
                    </div>
                  ) : scanned ? (
                    <div className="flex flex-col gap-1">
                      <p className="text-[11px] font-bold text-white/60 uppercase tracking-widest">No treadmills found</p>
                      <p className="text-[11px] text-white/50">
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
                          <span className="text-xs font-black uppercase tracking-wider">{device.name || 'Unknown Device'}</span>
                          <span className="text-[11px] font-mono text-white/50 truncate max-w-[200px]">{device.id}</span>
                        </div>
                        {linking ? (
                          <span className="flex items-center gap-1.5 text-[11px] font-bold uppercase tracking-wider text-neon-yellow">
                            <Loader2 size={14} className="animate-spin" /> Linking…
                          </span>
                        ) : (
                          <Link2 className="text-white/50 group-hover:text-neon-cyan transition-colors" size={16} />
                        )}
                      </button>
                      <ActionStatus
                        pending={linking ? "Linking to the treadmill. This can take up to 20 seconds." : null}
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
            </div>

            <div className="p-4 bg-black/40 border-t border-white/5">
              <button
                onClick={onScan}
                disabled={busy}
                className={`w-full py-3 rounded-xl font-black text-[11px] uppercase tracking-[0.2em] transition-all flex items-center justify-center gap-2 border ${busy
                                        ? 'border-neon-magenta/50 text-neon-magenta/50 cursor-wait'
                                        : 'border-neon-cyan text-neon-cyan hover:bg-neon-cyan/20 active:scale-[0.98]'
                                    }`}
              >
                <RefreshCw size={14} className={isScanning ? 'animate-spin' : ''} />
                {isScanning ? 'Scanning_Signals...' : scanError || scanned && devices.length === 0 ? 'Scan_Again' : 'Refresh_Scan'}
              </button>
            </div>

            <div className="absolute top-0 right-0 w-16 h-16 pointer-events-none overflow-hidden">
              <div className="absolute top-[-32px] right-[-32px] w-16 h-16 bg-neon-cyan/10 rotate-45 border border-neon-cyan/20" />
            </div>
          </motion.div>
        </div>
      )}
    </AnimatePresence>
  );
}
