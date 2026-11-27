import { useState } from "react";
import { Check, Clipboard, FileText } from "lucide-react";
import type { MergedSession } from "../../services/sessionMerge";
import { GhostButton, Label } from "../ui";
import { DialogFrame } from "../DialogFrame";
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
    <DialogFrame
      boxRef={boxRef}
      titleId="tcx-title"
      icon={FileText}
      title="TCX preview"
      subline={`${start.toLocaleDateString()} ${start.toLocaleTimeString()}`}
      size="xl"
      tall
      z="z-[200]"
      onClose={onClose}
      headerActions={
        <GhostButton onClick={copy} className={cx("flex items-center gap-1.5", copied && "text-emerald-400")}>
          {copied ? <Check size={14} /> : <Clipboard size={14} />}
          {copied ? "Copied" : "Copy"}
        </GhostButton>
      }
      bodyClassName="flex flex-col"
    >
      <dl className="grid shrink-0 grid-cols-4 gap-3 border-b border-white/10 px-4 py-3">
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
    </DialogFrame>
  );
}
