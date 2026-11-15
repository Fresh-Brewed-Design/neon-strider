import { invoke } from "@tauri-apps/api/core";

/** Key-value settings in Rust's `settings` table, stored as JSON text. */
export const SettingsRepository = {
  async set(key: string, value: unknown): Promise<void> {
    await invoke("set_db_setting", { table: "settings", key, value: JSON.stringify(value) });
  },

  /** The parsed value, or null when it is missing or not valid JSON. Callers check its shape. */
  async get(key: string): Promise<unknown> {
    const text = await invoke<string | null>("get_db_setting", { table: "settings", key });
    if (!text) return null;
    try {
      return JSON.parse(text);
    } catch (e) {
      console.error(`[settings] ${key} is not valid JSON`, e);
      return null;
    }
  },
};
