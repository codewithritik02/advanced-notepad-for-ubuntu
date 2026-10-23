/** Bounded, field-local history. Store text deltas rather than copies of entire notes. */
type Edit = { start: number; removed: string; inserted: string };
export type HistoryAction = "undo" | "redo";

export function historyShortcut(event: {
  key: string; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean;
  altKey: boolean; isComposing?: boolean;
}): HistoryAction | null {
  if (event.isComposing || event.altKey || !(event.ctrlKey || event.metaKey)) return null;
  const key = event.key.toLowerCase();
  if (key === "z") return event.shiftKey ? "redo" : "undo";
  return key === "y" && !event.shiftKey ? "redo" : null;
}

export class EditHistory {
  private value = "";
  private past: Edit[] = [];
  private future: Edit[] = [];
  private size = 0;

  reset(value: string): void {
    this.value = value;
    this.past = [];
    this.future = [];
    this.size = 0;
  }

  get canUndo(): boolean { return this.past.length > 0; }
  get canRedo(): boolean { return this.future.length > 0; }

  record(next: string): void {
    if (next === this.value) return;
    let start = 0;
    while (start < this.value.length && start < next.length && this.value[start] === next[start]) start++;
    let oldEnd = this.value.length;
    let newEnd = next.length;
    while (oldEnd > start && newEnd > start && this.value[oldEnd - 1] === next[newEnd - 1]) {
      oldEnd--; newEnd--;
    }
    const edit = { start, removed: this.value.slice(start, oldEnd), inserted: next.slice(start, newEnd) };
    for (const item of this.future) this.size -= item.removed.length + item.inserted.length;
    this.future = [];
    this.past.push(edit);
    this.size += edit.removed.length + edit.inserted.length;
    this.value = next;
    // Keep the latest edit even if it alone exceeds the budget, so a large paste is undoable.
    while (this.past.length > 1 && (this.past.length > 200 || this.size > 4_000_000)) {
      const dropped = this.past.shift()!;
      this.size -= dropped.removed.length + dropped.inserted.length;
    }
  }

  apply(action: HistoryAction): { value: string; cursor: number } | null {
    const from = action === "undo" ? this.past : this.future;
    const to = action === "undo" ? this.future : this.past;
    const edit = from.pop();
    if (!edit) return null;
    const remove = action === "undo" ? edit.inserted : edit.removed;
    const insert = action === "undo" ? edit.removed : edit.inserted;
    this.value = this.value.slice(0, edit.start) + insert + this.value.slice(edit.start + remove.length);
    to.push(edit);
    return { value: this.value, cursor: edit.start + insert.length };
  }
}
