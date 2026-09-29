import React from "react";
import "./MainLayout.css";

export interface MainLayoutProps {
  topBar?: React.ReactNode;
  sidebar?: React.ReactNode;
  notesList?: React.ReactNode;
  editor?: React.ReactNode;
}

export const MainLayout: React.FC<MainLayoutProps> = ({
  topBar,
  sidebar,
  notesList,
  editor,
}) => {
  return (
    <div className="app-container">
      <header className="app-topbar">
        {topBar ?? (
          <div className="topbar-placeholder">Personal Notepad</div>
        )}
      </header>

      <div className="app-body">
        <aside className="app-pane app-sidebar-pane" aria-label="Sidebar navigation">
          {sidebar ?? (
            <>
              <div className="pane-header">Navigation</div>
              <div className="pane-content">Sidebar Pane</div>
            </>
          )}
        </aside>

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
