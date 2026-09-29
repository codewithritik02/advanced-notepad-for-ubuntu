import { invoke } from "@tauri-apps/api/core";

export const settingsStorage = {
  /**
   * Retrieves a setting value by key.
   */
  async get(key: string): Promise<string | null> {
    return await invoke<string | null>("get_setting", { key });
  },

  /**
   * Upserts a setting key/value pair.
   */
  async set(key: string, value: string): Promise<void> {
    await invoke<void>("set_setting", { key, value });
  },
};
