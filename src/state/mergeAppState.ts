import type { AppSnapshot, AppStateFrame } from "../ipc";

/** The frame's snapshot fields, without its `full` flag. */
function fieldsOf<T extends { full: boolean }>(frame: T): Omit<T, "full"> {
  const fields: Partial<T> = { ...frame };
  delete fields.full;
  return fields as Omit<T, "full">;
}

/**
 * The snapshot after an `app-state` frame. A full frame is the whole
 * snapshot; any other carries only the hot parts, and the cold ones
 * (`COLD_KEYS`) are kept from `current`, the same objects as before. With no
 * snapshot yet a hot frame is dropped (null): the first fetch, or the full
 * frame Rust sends a window that just showed, brings everything.
 */
export function mergeAppState(current: AppSnapshot | null, frame: AppStateFrame): AppSnapshot | null {
  if (frame.full) return fieldsOf(frame);
  if (!current) return null;
  return { ...current, ...fieldsOf(frame) };
}
