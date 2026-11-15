import { invoke } from "@tauri-apps/api/core";
import { SampleSchema, SessionSchema, type Sample, type Session } from "../schema";

/** Saved sessions and their samples, read from Rust's database and checked against the schema. */
export const SessionRepository = {
  async getSessions(limit: number): Promise<Session[]> {
    const rows = await invoke<unknown[]>("get_sessions", { limit });
    return rows.map((r) => SessionSchema.parse(r));
  },

  async getSamples(sessionId: number): Promise<Sample[]> {
    const rows = await invoke<Record<string, unknown>[]>("get_samples", { sessionId });
    return rows.map((r) => SampleSchema.parse({ ...r, session_id: sessionId, timestamp: r.timestamp ?? 0 }));
  },

  async deleteSession(id: number): Promise<void> {
    await invoke("delete_session", { id });
  },
};
