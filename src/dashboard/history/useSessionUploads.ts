import { useEffect, useRef, useState } from "react";
import { errorText } from "../../components/useAction";
import { dispatch } from "../../ipc";
import type { MergedSession } from "../../services/sessionMerge";
import { getAppSnapshot, useAppState } from "../../state/useAppState";
import { activityText } from "../stravaStatus";
import type { UploadState } from "./SessionRow";

export interface RowNote {
  ok: boolean;
  text: string;
}

export interface RowUpload {
  state: UploadState;
  error: string | null;
  /** What the running upload is doing, for the spinner's tooltip. */
  step: string | null;
}

/**
 * Upload buttons on the History rows, and a short note per row. Rust uploads
 * in the background and reports through the snapshot, so a row's state comes
 * from there too: an auto-sync upload shows on its row like a manual one.
 */
export function useSessionUploads(onUploaded: () => void) {
  const stravaConnected = useAppState((s) => s.strava.connected) ?? false;
  const stravaSyncing = useAppState((s) => s.strava.syncing) ?? false;
  const stravaResultAt = useAppState((s) => s.strava.last_result?.at_ms ?? null);
  const uploadingIds = useAppState((s) => s.strava.uploading_session_ids) ?? [];
  const uploadErrors = useAppState((s) => s.strava.upload_errors) ?? {};
  const activity = useAppState((s) => s.strava.activity) ?? { state: "idle" as const };

  const [uploads, setUploads] = useState<Record<number, UploadState>>({});
  const [notes, setNotes] = useState<Record<number, RowNote>>({});
  // The row whose upload is in flight, and the last Strava result seen when it was queued.
  const pending = useRef<{ row: number; baseline: number | null } | null>(null);

  const setNote = (row: number, note: RowNote) => setNotes((n) => ({ ...n, [row]: note }));
  const setUpload = (row: number, state: UploadState) => setUploads((u) => ({ ...u, [row]: state }));

  // The outcome is the next Strava result once nothing is syncing any more.
  useEffect(() => {
    const p = pending.current;
    if (!p || stravaSyncing || stravaResultAt === p.baseline) return;
    const result = getAppSnapshot()?.strava.last_result;
    if (!result) return;
    pending.current = null;
    setUploads((u) => ({ ...u, [p.row]: result.ok ? "success" : "error" }));
    setNotes((n) => ({ ...n, [p.row]: { ok: result.ok, text: result.message } }));
    if (result.ok) {
      onUploaded();
      setTimeout(() => setUploads((u) => ({ ...u, [p.row]: "idle" })), 5000);
      setTimeout(() => setNotes((n) => ({ ...n, [p.row]: { ok: true, text: "" } })), 8000);
    }
  }, [stravaResultAt, stravaSyncing, onUploaded]);

  const upload = (session: MergedSession, row: number) => {
    const fail = (text: string) => {
      setUpload(row, "error");
      setNote(row, { ok: false, text });
    };
    if (!stravaConnected) return fail("Strava is not linked. Connect it on the System page.");
    if (pending.current) return fail("Another upload is still running. Try again when it finishes.");
    pending.current = { row, baseline: getAppSnapshot()?.strava.last_result?.at_ms ?? null };
    setNote(row, { ok: true, text: "" });
    setUpload(row, "uploading");
    dispatch({ type: "strava_upload", session_ids: session.mergedIds }).catch((e) => {
      pending.current = null;
      fail(errorText(e));
    });
  };

  const rowUpload = (s: MergedSession, row: number): RowUpload => {
    const running = uploads[row] === "uploading" || s.mergedIds.some((id) => uploadingIds.includes(id));
    const fromRust = s.mergedIds.map((id) => uploadErrors[String(id)]).find(Boolean) ?? null;
    const fromNote = uploads[row] === "error" ? (notes[row]?.text ?? "Upload failed.") : null;
    if (running) return { state: "uploading", error: null, step: activityText(activity) };
    const error = fromRust ?? fromNote;
    if (error) return { state: "error", error, step: null };
    return { state: uploads[row] === "success" ? "success" : "idle", error: null, step: null };
  };

  /** The row's note, or null when there is nothing to say. */
  const noteFor = (row: number): RowNote | null => (notes[row]?.text ? notes[row] : null);

  return { stravaConnected, upload, rowUpload, noteFor, setNote };
}
