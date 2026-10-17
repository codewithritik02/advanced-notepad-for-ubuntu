import { createContext, useContext } from "react";

/**
 * Collapsed/expanded state of the left sidebar, provided by MainLayout and read by the Sidebar.
 * `isCompact` is true while the sidebar is shown as a narrow icon rail.
 */
export interface SidebarState {
  isCompact: boolean;
  isPinned: boolean;
  /** Present when the layout supports pinning; the Sidebar shows a pin button only then. */
  togglePin?: () => void;
}

export const SidebarStateContext = createContext<SidebarState>({
  isCompact: false,
  isPinned: true,
});

export function useSidebarState(): SidebarState {
  return useContext(SidebarStateContext);
}
