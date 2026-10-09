import { invoke } from "@tauri-apps/api/core";
import { BacklinkItem, ResolvedLinkTarget } from "./types";

export const linksStorage = {
  /**
   * Resolves an internal link target by its stable note ID (Phase 9, Task 9).
   * Distinguishes active, trashed, and missing targets.
   */
  async resolveTarget(targetId: string): Promise<ResolvedLinkTarget> {
    return await invoke<ResolvedLinkTarget>("resolve_link_target", { targetId });
  },

  /**
   * Discovers incoming links / backlinks pointing to the target note ID (Phase 9, Task 14).
   * Returns active source notes excluding trashed notes and self-references.
   */
  async getBacklinks(targetNoteId: string): Promise<BacklinkItem[]> {
    return await invoke<BacklinkItem[]>("get_backlinks", { targetNoteId });
  },
};

