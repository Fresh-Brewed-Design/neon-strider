import { useState } from "react";
import { Check, Clipboard, X } from "lucide-react";
import type { MergedSession } from "../../services/sessionMerge";
import { GhostButton, Label } from "../ui";
import { useDialog } from "../useDialog";
import { cx } from "../util";

/** Read-only look at the TCX that would be uploaded, for checking distances before a Strava upload. */
export function TcxModal({ session, tcx, onClose }: { session: MergedSession; tcx: string; onClose: () => void }) {
  const [copied, setCopied] = useState(false);
  const boxRef = useDialog(true, onClose);

  const copy = () => {
    navigator.clipboard
      .writeText(tcx)
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 2000); // long enough to see COPIED
      })
      .catch((e) => console.error("[history] copy failed", e));
  };

  const start = new Date(session.start_time);
  const stats: [string, string][] = [
    ["Trackpoints", String(tcx.split("<Trackpoint>").length - 1)],
    ["Duration", `${Math.floor(session.duration_seconds / 60)} min`],
    ["Distance", `${session.distance_meters.toFixed(1)} m`],
    ["Payload", `${(tcx.length / 1024).toFixed(1)} KB`],
  ];

  return (
    <div
      ref={boxRef}
      role="dialog"
      aria-modal="true"
      aria-label="TCX preview"
      tabIndex={-1}
      className="fixed inset-0 z-[200] flex items-center justify-center bg-black/90 p-4 outline-none backdrop-blur-sm"
    >
      <div className="flex h-full max-h-[34rem] w-full max-w-2xl flex-col overflow-hidden rounded-lg border border-white/10 bg-[#050505]">
        <div className="flex items-center justify-between gap-3 border-b border-white/10 px-4 py-3">
          <div>
            <h2 className="font-digital text-sm font-bold tracking-[0.12em] text-white">TCX preview</h2>
            <p className="font-mono text-xs text-white/60">
              {start.toLocaleDateString()} {start.toLocaleTimeString()}
            </p>
          </div>
          <div className="flex items-center gap-1">
            <GhostButton onClick={copy} className={cx("flex items-center gap-1.5", copied && "text-emerald-400")}>
              {copied ? <Check size={14} /> : <Clipboard size={14} />}
              {copied ? "Copied" : "Copy"}
            </GhostButton>
            <GhostButton aria-label="Close" onClick={onClose}>
              <X size={16} />
            </GhostButton>
          </div>
        </div>
        <dl className="grid grid-cols-4 gap-3 border-b border-white/10 px-4 py-3">
          {stats.map(([k, v]) => (
            <div key={k}>
              <dt><Label>{k}</Label></dt>
              <dd className="font-digital text-sm text-neon-cyan">{v}</dd>
            </div>
          ))}
        </dl>
        <pre className="min-h-0 flex-1 select-text overflow-auto whitespace-pre-wrap break-all p-4 font-mono text-xs leading-relaxed text-white/60">
          {tcx}
        </pre>
      </div>
    </div>
  );
}
