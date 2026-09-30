import type { Notebook, NotebookTreeNode } from "../types";
import { logger } from "../../../utils/logger.ts";

/**
 * Builds a nested hierarchical tree structure from a flat array of notebooks retrieved from SQLite.
 *
 * Requirements satisfied:
 * - Supports arbitrary nesting depth (root -> children -> grandchildren -> ...)
 * - Sorts deterministically at each hierarchy level (case-insensitive Unicode localeCompare)
 * - Safe handling of missing parent references (promotes orphan nodes to root to prevent data disappearance)
 * - Defensive cycle detection to guarantee termination and prevent infinite loops / crashes
 */
export function buildNotebookTree(
  notebooks: Notebook[],
  noteCounts?: Record<string, number>
): NotebookTreeNode[] {
  if (!notebooks || notebooks.length === 0) {
    return [];
  }

  // 1. Create a map of all nodes initialized with empty children and depth 0
  const nodeMap = new Map<string, NotebookTreeNode>();
  for (const nb of notebooks) {
    nodeMap.set(nb.id, {
      ...nb,
      children: [],
      depth: 0,
      noteCount: noteCounts ? noteCounts[nb.id] ?? 0 : undefined,
    });
  }

  // 2. Helper to detect cycles before linking parent_id
  const wouldCreateCycle = (nodeId: string, potentialParentId: string | null): boolean => {
    let currentId: string | null = potentialParentId;
    const visited = new Set<string>();

    while (currentId !== null) {
      if (currentId === nodeId) {
        return true; // Cycle detected!
      }
      if (visited.has(currentId)) {
        return true; // Indirect cycle detected
      }
      visited.add(currentId);
      const parentNode = nodeMap.get(currentId);
      currentId = parentNode ? parentNode.parent_id : null;
    }
    return false;
  };

  const rootNodes: NotebookTreeNode[] = [];

  // 3. Assemble parent-child tree relationships
  for (const node of nodeMap.values()) {
    if (!node.parent_id) {
      // Root level notebook
      rootNodes.push(node);
    } else {
      const parent = nodeMap.get(node.parent_id);

      if (!parent) {
        // Missing parent reference (Task 24) - treat safely as root
        logger.notebook.invalidHierarchy("Missing parent reference", {
          notebookName: node.name,
          notebookId: node.id,
          missingParentId: node.parent_id,
        });
        rootNodes.push(node);
      } else if (wouldCreateCycle(node.id, node.parent_id)) {
        // Circular reference detected (Task 25) - break cycle and treat as root
        logger.notebook.invalidHierarchy("Circular parent reference detected", {
          notebookName: node.name,
          notebookId: node.id,
          targetParentId: node.parent_id,
        });
        rootNodes.push(node);
      } else {
        // Valid parent
        parent.children.push(node);
      }
    }
  }

  // 4. Deterministic sorting and depth calculation
  const sortAndAssignDepth = (nodes: NotebookTreeNode[], depth: number): NotebookTreeNode[] => {
    nodes.sort((a, b) => {
      // Deterministic Unicode comparison preserving Hindi and other international characters
      return a.name.localeCompare(b.name, undefined, {
        sensitivity: "base",
        numeric: true,
      });
    });

    for (const node of nodes) {
      node.depth = depth;
      if (node.children.length > 0) {
        sortAndAssignDepth(node.children, depth + 1);
      }
    }

    return nodes;
  };

  return sortAndAssignDepth(rootNodes, 0);
}

/**
 * Checks whether a given notebook currently has any direct child notebooks.
 */
export function hasChildNotebooks(
  notebookId: string,
  notebooks: Notebook[]
): boolean {
  return notebooks.some((nb) => nb.parent_id === notebookId);
}

/**
 * Collects all descendant notebook IDs (children, grandchildren, etc.) for a given notebook ID.
 */
export function getDescendantNotebookIds(
  notebookId: string,
  notebooks: Notebook[]
): string[] {
  const result: string[] = [];
  const queue = [notebookId];

  while (queue.length > 0) {
    const currentId = queue.shift()!;
    const directChildren = notebooks.filter((nb) => nb.parent_id === currentId);
    for (const child of directChildren) {
      result.push(child.id);
      queue.push(child.id);
    }
  }

  return result;
}

/**
 * Recursively flattens a tree into a linear list in hierarchy traversal order.
 */
export function flattenNotebookTree(
  tree: NotebookTreeNode[]
): NotebookTreeNode[] {
  const flat: NotebookTreeNode[] = [];

  const traverse = (nodes: NotebookTreeNode[]) => {
    for (const node of nodes) {
      flat.push(node);
      if (node.children.length > 0) {
        traverse(node.children);
      }
    }
  };

  traverse(tree);
  return flat;
}

/**
 * Computes a human-readable hierarchical path for a notebook (e.g. "Work / Projects").
 * Returns "Unfiled" if notebookId is null, undefined, or empty.
 * Task 33: Notebook Metadata Integration
 */
export function computeNotebookPath(
  notebookId: string | null | undefined,
  notebooks: Notebook[]
): string {
  if (!notebookId) return "Unfiled";
  const path: string[] = [];
  let currentId: string | null = notebookId;
  const visited = new Set<string>();

  while (currentId && !visited.has(currentId)) {
    visited.add(currentId);
    const nb = notebooks.find((n) => n.id === currentId);
    if (nb) {
      path.unshift(nb.name);
      currentId = nb.parent_id;
    } else {
      break;
    }
  }

  return path.length > 0 ? path.join(" / ") : "Unfiled";
}

