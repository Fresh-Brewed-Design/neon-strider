import { invoke } from "@tauri-apps/api/core";
import type { HistoryStats } from "../../../ipc";
import { SampleSchema, SessionSchema, type Sample, type Session } from "../schema";

const parseRows = (rows: unknown[]): Session[] => rows.map((r) => SessionSchema.parse(r));

/** Saved sessions and their samples, read from Rust's database and checked against the schema. */
export const SessionRepository = {
  /** Walks starting at or after `fromMs` (and before `beforeMs`), newest first. */
  async getSessionsSince(fromMs: number, beforeMs: number | null = null): Promise<Session[]> {
    return parseRows(await invoke<unknown[]>("history_sessions", { fromMs, beforeMs }));
  },

  /** The next few months of walks older than `beforeMs`; `fromMs` is null when none are left. */
  async getOlderSessions(beforeMs: number): Promise<{ sessions: Session[]; fromMs: number | null }> {
    const page = await invoke<{ sessions: unknown[]; from_ms: number | null }>("history_older", { beforeMs });
    return { sessions: parseRows(page.sessions), fromMs: page.from_ms };
  },

  /** Totals of every walk in `[fromMs, toMs)`, the calendar from `calendarFromMs`, and the oldest walk. */
  async getStats(fromMs: number | null, toMs: number | null, calendarFromMs: number): Promise<HistoryStats> {
    return invoke<HistoryStats>("history_stats", { fromMs, toMs, calendarFromMs });
  },

  async getSamples(sessionId: number): Promise<Sample[]> {
    const rows = await invoke<Record<string, unknown>[]>("get_samples", { sessionId });
    return rows.map((r) => SampleSchema.parse({ ...r, session_id: sessionId, timestamp: r.timestamp ?? 0 }));
  },

  async deleteSession(id: number): Promise<void> {
    await invoke("delete_session", { id });
  },
};
