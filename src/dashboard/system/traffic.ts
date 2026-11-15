import type { TrafficSnapshot } from "../../ipc";

/** H:MM:SS, or M:SS under an hour (as Rust's `util::format_duration`). */
export function clock(totalSeconds: number): string {
  const t = Math.max(0, Math.floor(totalSeconds));
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const s = String(t % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}

/** File size: "820 B", "14 KB", "2.4 MB". */
export function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** "1,234 packets · 0:42 · 14 KB": the live line while recording, and the summary after. */
export function trafficSummary(t: Pick<TrafficSnapshot, "packets" | "elapsed_s" | "bytes">): string {
  const packets = `${t.packets.toLocaleString("en-US")} ${t.packets === 1 ? "packet" : "packets"}`;
  return `${packets} · ${clock(t.elapsed_s)} · ${fileSize(t.bytes)}`;
}
