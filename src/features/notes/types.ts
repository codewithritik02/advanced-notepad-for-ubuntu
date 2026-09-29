// Types specific to the Notes feature

export interface NoteListItem {
  id: string;
  title: string;
  preview: string;
  updatedAt: string;
  isFavorite?: boolean;
  notebookId?: string;
  tags?: string[];
}

export type NotesListStatus = "idle" | "loading" | "error" | "empty";
