// Global TypeScript definitions for Personal Notepad

export type ThemeMode = "light" | "dark" | "system";

export interface BaseEntity {
  id: string;
  createdAt: string;
  updatedAt: string;
}
