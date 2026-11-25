import { useTreadmillStore } from "../../store/treadmillStore";
import { send } from "../util";

/** Asks before resetting the belt service counter; used by the Live banner and the System section. */
export function confirmServiced(): void {
  useTreadmillStore.getState().confirm({
    title: "Mark belt serviced",
    confirmLabel: "Mark serviced",
    message: "Reset the service counter to today? Distance and days start again from zero.",
    onConfirm: () => send({ type: "maintenance_mark_serviced" }),
  });
}

/** "YYYY-MM-DD" in local time, the format a date input uses. */
export function toDateInput(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Local midnight of a "YYYY-MM-DD" value, or null when it isn't a date. */
export function fromDateInput(value: string): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!m) return null;
  const ms = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3])).getTime();
  return Number.isFinite(ms) ? ms : null;
}
