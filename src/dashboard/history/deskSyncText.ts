import type { DeskSync } from "../../ipc";

/** The plain sentence under DESK_SYNC. */
export function deskSyncText(sync: DeskSync): string {
  if (sync.desk_days === 0) {
    return sync.today_open ? "Walk today to start the count." : "No desk days recorded yet.";
  }
  if (sync.desk_days === 1) {
    return sync.walked > 0 ? "Walked on your last desk day." : "No walk on your last desk day.";
  }
  return `Walked on ${sync.walked} of your last ${sync.desk_days} desk days.`;
}
