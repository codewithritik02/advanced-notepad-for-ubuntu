/**
 * "Recently opened" list: the notes (and files opened from the desktop) the user opened last.
 * Pure helpers, so the list logic is unit-testable and the storage layer stays thin.
 */

export interface RecentNote {
  id: string;
  title: string;
  /** Milliseconds since the epoch of the last time the note was opened. */
  openedAt: number;
  /** Path of the file this note was created from, when it was opened via "Open with". */
  file?: string;
}

export const MAX_RECENT_NOTES = 15;
export const RECENT_SETTING_KEY = "recent_notes";

export interface RecentInput {
  id: string;
  title?: string;
  file?: string;
}

/** Moves the note to the front (newest first), keeping what is already known about it. */
export function addRecent(
  list: RecentNote[],
  entry: RecentInput,
  now: number,
  max: number = MAX_RECENT_NOTES,
): RecentNote[] {
  const previous = list.find((item) => item.id === entry.id);
  const title = (entry.title ?? previous?.title ?? "").trim() || "Untitled Note";
  const file = entry.file ?? previous?.file;
  const next: RecentNote = { id: entry.id, title, openedAt: now, ...(file ? { file } : {}) };
  return [next, ...list.filter((item) => item.id !== entry.id)].slice(0, Math.max(1, max));
}

export function removeRecent(list: RecentNote[], id: string): RecentNote[] {
  return list.filter((item) => item.id !== id);
}

/** Refreshes stored titles from the notes currently loaded (a note may have been renamed). */
export function syncRecentTitles(
  list: RecentNote[],
  notes: ReadonlyArray<{ id: string; title: string }>,
): RecentNote[] {
  if (list.length === 0 || notes.length === 0) return list;
  const titles = new Map(notes.map((note) => [note.id, note.title]));
  let changed = false;
  const next = list.map((item) => {
    const latest = titles.get(item.id);
    if (latest === undefined) return item;
    const title = latest.trim() || "Untitled Note";
    if (title === item.title) return item;
    changed = true;
    return { ...item, title };
  });
  return changed ? next : list;
}

/** Reads the stored JSON defensively: anything malformed is dropped, never thrown. */
export function parseRecent(raw: string | null | undefined): RecentNote[] {
  if (!raw) return [];
  try {
    const data: unknown = JSON.parse(raw);
    if (!Array.isArray(data)) return [];
    const seen = new Set<string>();
    const items: RecentNote[] = [];
    for (const value of data) {
      if (!value || typeof value !== "object") continue;
      const { id, title, openedAt, file } = value as Record<string, unknown>;
      if (typeof id !== "string" || !id || seen.has(id)) continue;
      seen.add(id);
      items.push({
        id,
        title: typeof title === "string" && title.trim() ? title : "Untitled Note",
        openedAt: typeof openedAt === "number" && Number.isFinite(openedAt) ? openedAt : 0,
        ...(typeof file === "string" && file ? { file } : {}),
      });
    }
    return items.sort((a, b) => b.openedAt - a.openedAt).slice(0, MAX_RECENT_NOTES);
  } catch {
    return [];
  }
}

/** "just now", "5 min ago", "3 h ago", "yesterday", "4 d ago", or a date for older entries. */
export function formatRelativeTime(timestamp: number, now: number): string {
  const seconds = Math.max(0, Math.round((now - timestamp) / 1000));
  if (seconds < 45) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.round(hours / 24);
  if (days === 1) return "yesterday";
  if (days < 14) return `${days} d ago`;
  return new Date(timestamp).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}

/** File name and containing folder of a path, for display. */
export function describeFilePath(path: string): { name: string; folder: string } {
  const parts = path.split("/").filter((part) => part.length > 0);
  const name = parts[parts.length - 1] ?? path;
  const folder = parts.length > 1 ? parts[parts.length - 2] : "";
  return { name, folder };
}
