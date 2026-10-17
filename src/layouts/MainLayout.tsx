import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { SidebarStateContext } from "./sidebarState";
import "./MainLayout.css";

export interface MainLayoutProps {
  topBar?: React.ReactNode;
  sidebar?: React.ReactNode;
  notesList?: React.ReactNode;
  editor?: React.ReactNode;
  /** Keep the sidebar open. When false it collapses to an icon rail and opens on hover. */
  isSidebarPinned?: boolean;
  onToggleSidebarPin?: () => void;
}

/** Small delays so sweeping the pointer across the rail does not flicker the sidebar open. */
const OPEN_DELAY_MS = 90;
const CLOSE_DELAY_MS = 260;

export const MainLayout: React.FC<MainLayoutProps> = ({
  topBar,
  sidebar,
  notesList,
  editor,
  isSidebarPinned = true,
  onToggleSidebarPin,
}) => {
  const [isRevealed, setIsRevealed] = useState(false);
  const timerRef = useRef<number | null>(null);

  const clearTimer = useCallback(() => {
    if (timerRef.current !== null) {
      window.clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const reveal = useCallback(
    (open: boolean, immediate = false) => {
      clearTimer();
      if (immediate) {
        setIsRevealed(open);
        return;
      }
      timerRef.current = window.setTimeout(
        () => {
          timerRef.current = null;
          setIsRevealed(open);
        },
        open ? OPEN_DELAY_MS : CLOSE_DELAY_MS,
      );
    },
    [clearTimer],
  );

  useEffect(() => clearTimer, [clearTimer]);

  const isOpen = isSidebarPinned || isRevealed;

  const handleTogglePin = useCallback(() => {
    // Unpinning collapses right away instead of waiting for the pointer to leave.
    if (isSidebarPinned) reveal(false, true);
    onToggleSidebarPin?.();
  }, [isSidebarPinned, onToggleSidebarPin, reveal]);

  const sidebarState = useMemo(
    () => ({
      isCompact: !isOpen,
      isPinned: isSidebarPinned,
      togglePin: onToggleSidebarPin ? handleTogglePin : undefined,
    }),
    [isOpen, isSidebarPinned, onToggleSidebarPin, handleTogglePin],
  );

  const leaveIfOutside = (event: { currentTarget: Element; relatedTarget: EventTarget | null }) => {
    if (!event.currentTarget.contains(event.relatedTarget as Node | null)) reveal(false);
  };

  return (
    <div className="app-container">
      <header className="app-topbar">
        {topBar ?? (
          <div className="topbar-placeholder">Personal Notepad</div>
        )}
      </header>

      <div className="app-body">
        <SidebarStateContext.Provider value={sidebarState}>
          <aside
            className={`app-pane app-sidebar-pane ${isSidebarPinned ? "is-pinned" : "is-unpinned"} ${isOpen ? "is-open" : "is-collapsed"}`}
            aria-label="Sidebar navigation"
            onPointerEnter={(event) => {
              if (event.pointerType !== "touch") reveal(true);
            }}
            onPointerLeave={() => reveal(false)}
            onFocusCapture={() => reveal(true, true)}
            onBlurCapture={leaveIfOutside}
            onDragEnter={() => reveal(true, true)}
            onDragLeave={leaveIfOutside}
          >
            <div className="app-sidebar-surface">
              {sidebar ?? (
                <>
                  <div className="pane-header">Navigation</div>
                  <div className="pane-content">Sidebar Pane</div>
                </>
              )}
            </div>
          </aside>
        </SidebarStateContext.Provider>

        <section className="app-pane app-notes-list-pane" aria-label="Notes list">
          {notesList ?? (
            <>
              <div className="pane-header">Notes</div>
              <div className="pane-content">Notes List Pane</div>
            </>
          )}
        </section>

        <main className="app-pane app-editor-pane" aria-label="Note editor">
          {editor ?? (
            <>
              <div className="pane-header">Editor</div>
              <div className="pane-content">Editor Pane</div>
            </>
          )}
        </main>
      </div>
    </div>
  );
};

export default MainLayout;
