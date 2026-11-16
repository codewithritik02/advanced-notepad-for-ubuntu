// TEMPORARY preview harness (not part of the app): renders the real NotebookTree with seeded data.
import { useState } from "react";
import ReactDOM from "react-dom/client";
import "./styles/index.css";
import { NotebookTree } from "./features/notebooks/components/NotebookTree";

const now = new Date().toISOString();
const nb = (id: string, name: string, parentId: string | null) =>
  ({ id, name, parentId, parent_id: parentId, createdAt: now, modifiedAt: now, created_at: now, modified_at: now }) as any;

const notebooks = [
  nb("w", "Work", null),
  nb("w2", "Projects", "w"),
  nb("w3", "Desktop App", "w2"),
  nb("w4", "Backend", "w3"),
  nb("w5", "Storage Layer", "w4"),
  nb("w6", "Migrations", "w5"),
  nb("w7", "2026 Schema Changes", "w6"),
  nb("w2b", "Meetings", "w"),
  nb("p", "Personal", null),
  nb("p2", "Health", "p"),
  nb("p3", "Fitness", "p2"),
  nb("l", "Learning", null),
  nb("l2", "Rust", "l"),
  nb("l3", "Ownership & Borrowing", "l2"),
];
const noteCounts: Record<string, number> = { w: 2, w2: 4, w3: 6, w4: 3, w5: 5, w6: 2, w7: 9, w2b: 12, p3: 7, l3: 3 };

const Demo = () => {
  const [selected, setSelected] = useState<string | null>("w7");
  const [expanded, setExpanded] = useState(new Set(notebooks.map((n) => n.id)));
  const toggle = (id: string) =>
    setExpanded((s) => { const n = new Set(s); n.has(id) ? n.delete(id) : n.add(id); return n; });
  return (
    <div style={{ display: "flex", gap: 24, padding: 24, background: "var(--color-bg-default, #fff)", minHeight: "100vh" }}>
      <div style={{ width: 260, border: "1px solid #8884", borderRadius: 8, padding: 8 }}>
        <NotebookTree
          notebooks={notebooks}
          noteCounts={noteCounts}
          selectedNotebookId={selected}
          expandedNotebookIds={expanded}
          onToggleExpand={toggle}
          onSelectNotebook={setSelected}
          onCreateNotebook={() => {}}
        />
      </div>
      <p style={{ maxWidth: 380, font: "14px system-ui", color: "var(--color-text-secondary, #666)" }}>
        Right-click “2026 Schema Changes” (level 7) — “New Sub-notebook” is disabled.
        Right-click “Migrations” (level 6) — it is enabled.
      </p>
    </div>
  );
};
ReactDOM.createRoot(document.getElementById("root")!).render(<Demo />);
