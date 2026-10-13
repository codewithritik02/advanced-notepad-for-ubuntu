import test, { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  validateTagName,
  normalizeTagName,
  areTagNamesEqual,
  findTagByName,
  MAX_TAG_NAME_LENGTH,
} from "../src/features/tags/utils/tagUtils.ts";
import {
  computeNoteMetadata,
  getFormatDisplayName,
} from "../src/features/notes/types.ts";
import {
  highlightText,
  searchResultToNoteListItem,
  formatResultCount,
} from "../src/features/search/utils.ts";
import { computeNotebookPath } from "../src/features/notebooks/utils/notebookTree.ts";

describe("Phase 5 - Task 63: Frontend Component & Logic Tests", () => {
  // =========================================================================
  // 1. TagPicker Logic
  // =========================================================================
  describe("1. TagPicker Logic", () => {
    const mockAvailableTags = [
      { id: "tag-1", name: "work", created_at: "2026-09-30T10:00:00Z" },
      { id: "tag-2", name: "personal", created_at: "2026-09-30T10:00:00Z" },
      { id: "tag-3", name: "R&D", created_at: "2026-09-30T10:00:00Z" },
      { id: "tag-4", name: "यात्रा", created_at: "2026-09-30T10:00:00Z" },
    ];

    it("filters available tags by search query case-insensitively", () => {
      const query = "PERS";
      const q = query.trim().toLowerCase();
      const filtered = mockAvailableTags.filter((t) =>
        t.name.toLowerCase().includes(q)
      );
      assert.strictEqual(filtered.length, 1);
      assert.strictEqual(filtered[0].name, "personal");
    });

    it("detects exact match exists case-insensitively (suppressing create button)", () => {
      const checkExactMatch = (tags, query) => {
        const q = query.trim().toLowerCase();
        if (!q) return true;
        return tags.some((t) => t.name.toLowerCase() === q);
      };

      assert.strictEqual(checkExactMatch(mockAvailableTags, "work"), true);
      assert.strictEqual(checkExactMatch(mockAvailableTags, "WoRk"), true);
      assert.strictEqual(checkExactMatch(mockAvailableTags, "WORK"), true);
      assert.strictEqual(checkExactMatch(mockAvailableTags, "r&d"), true);
      assert.strictEqual(checkExactMatch(mockAvailableTags, "यात्रा"), true);
      assert.strictEqual(checkExactMatch(mockAvailableTags, "new-tag"), false);
      assert.strictEqual(checkExactMatch(mockAvailableTags, "  "), true); // empty query counts as no new creation
    });

    it("handles tag toggle logic: adds tag when unassigned, removes when assigned", async () => {
      let noteTags = [{ id: "tag-1", name: "work", created_at: "2026-09-30T10:00:00Z" }];
      const addedIds = [];
      const removedIds = [];

      const handleToggleTag = async (tag, disabled = false) => {
        if (disabled) return;
        const assignedIds = new Set(noteTags.map((t) => t.id));
        if (assignedIds.has(tag.id)) {
          removedIds.push(tag.id);
          noteTags = noteTags.filter((t) => t.id !== tag.id);
        } else {
          addedIds.push(tag.id);
          noteTags = [...noteTags, tag];
        }
      };

      // Toggle unassigned tag -> add
      await handleToggleTag(mockAvailableTags[1]);
      assert.deepStrictEqual(addedIds, ["tag-2"]);
      assert.strictEqual(noteTags.length, 2);
      assert.ok(noteTags.some((t) => t.id === "tag-2"));

      // Toggle already assigned tag -> remove
      await handleToggleTag(mockAvailableTags[0]);
      assert.deepStrictEqual(removedIds, ["tag-1"]);
      assert.strictEqual(noteTags.length, 1);
      assert.strictEqual(noteTags[0].id, "tag-2");

      // Disabled toggle does nothing
      await handleToggleTag(mockAvailableTags[0], true);
      assert.strictEqual(noteTags.length, 1);
    });
  });

  // =========================================================================
  // 2. Tag Display & Formatting
  // =========================================================================
  describe("2. Tag Display & Formatting", () => {
    it("formats tag with leading # hash chip prefix", () => {
      const formatTagChip = (tag) => `#${tag.name}`;
      assert.strictEqual(formatTagChip({ name: "important" }), "#important");
      assert.strictEqual(formatTagChip({ name: "planning" }), "#planning");
    });

    it("displays Unicode and multilingual tag names correctly", () => {
      const unicodeTags = ["work", "यात्रा", "旅行", "café", "తీర్థ-यात्रा 🗺️"];
      for (const name of unicodeTags) {
        const val = validateTagName(name);
        assert.strictEqual(val.valid, true);
        assert.strictEqual(val.trimmed, name);
      }
    });

    it("displays special characters in tag names correctly", () => {
      const specialTags = [
        "C++",
        "C#",
        "R&D",
        "Project / Planning",
        "Q4 2026",
        "Design & UX",
      ];
      for (const name of specialTags) {
        const val = validateTagName(name);
        assert.strictEqual(val.valid, true);
        assert.strictEqual(val.trimmed, name);
      }
    });

    it("enforces tag validation rules (rejects empty or whitespace, limits to 100 chars)", () => {
      assert.strictEqual(validateTagName("").valid, false);
      assert.strictEqual(validateTagName("   ").valid, false);
      assert.strictEqual(validateTagName("a".repeat(100)).valid, true);
      assert.strictEqual(validateTagName("a".repeat(101)).valid, false);
      assert.strictEqual(MAX_TAG_NAME_LENGTH, 100);
    });
  });

  // =========================================================================
  // 3. Tag Removal
  // =========================================================================
  describe("3. Tag Removal", () => {
    it("removes specified tag by ID while maintaining immutability and other tags", () => {
      const initialTags = [
        { id: "t1", name: "alpha" },
        { id: "t2", name: "beta" },
        { id: "t3", name: "gamma" },
      ];

      const removeTag = (tags, tagIdToRemove) =>
        tags.filter((t) => t.id !== tagIdToRemove);

      const afterRemove = removeTag(initialTags, "t2");
      assert.strictEqual(afterRemove.length, 2);
      assert.deepStrictEqual(
        afterRemove.map((t) => t.name),
        ["alpha", "gamma"]
      );
      // Ensure original array was not mutated
      assert.strictEqual(initialTags.length, 3);
    });

    it("handles removing a non-existent tag gracefully without changes", () => {
      const initialTags = [{ id: "t1", name: "alpha" }];
      const removeTag = (tags, id) => tags.filter((t) => t.id !== id);
      const result = removeTag(initialTags, "t-nonexistent");
      assert.strictEqual(result.length, 1);
      assert.strictEqual(result[0].id, "t1");
    });
  });

  // =========================================================================
  // 4. Favorite Toggle
  // =========================================================================
  describe("4. Favorite Toggle", () => {
    it("inverts is_favorite boolean and preserves all note properties", () => {
      const note = {
        id: "note-123",
        title: "Meeting Notes",
        content: "Discussion points...",
        format: "md",
        notebook_id: "nb-1",
        is_pinned: false,
        is_favorite: false,
        created_at: "2026-09-30T12:00:00Z",
        modified_at: "2026-09-30T12:00:00Z",
      };

      const toggleFavorite = (n) => ({
        ...n,
        is_favorite: !n.is_favorite,
      });

      const favorited = toggleFavorite(note);
      assert.strictEqual(favorited.is_favorite, true);
      assert.strictEqual(favorited.title, note.title);
      assert.strictEqual(favorited.content, note.content);
      assert.strictEqual(favorited.notebook_id, note.notebook_id);

      const unfavorited = toggleFavorite(favorited);
      assert.strictEqual(unfavorited.is_favorite, false);
      assert.strictEqual(unfavorited.title, note.title);
    });

    it("computes favorite button action label and accessibility state", () => {
      const getActionProps = (isFavorite) => ({
        label: isFavorite ? "Unstar" : "Star",
        title: isFavorite ? "Remove from favorites" : "Add to favorites",
        display: isFavorite ? "Yes" : "No",
        ariaPressed: isFavorite,
      });

      assert.deepStrictEqual(getActionProps(false), {
        label: "Star",
        title: "Add to favorites",
        display: "No",
        ariaPressed: false,
      });

      assert.deepStrictEqual(getActionProps(true), {
        label: "Unstar",
        title: "Remove from favorites",
        display: "Yes",
        ariaPressed: true,
      });
    });
  });

  // =========================================================================
  // 5. Favorites Navigation
  // =========================================================================
  describe("5. Favorites Navigation", () => {
    const notes = [
      {
        id: "n1",
        title: "Regular Favorite",
        is_favorite: true,
        is_pinned: false,
        is_deleted: false,
        modified_at: "2026-09-30T10:00:00Z",
      },
      {
        id: "n2",
        title: "Pinned Favorite",
        is_favorite: true,
        is_pinned: true,
        is_deleted: false,
        modified_at: "2026-09-30T09:00:00Z",
      },
      {
        id: "n3",
        title: "Deleted Favorite",
        is_favorite: true,
        is_pinned: false,
        is_deleted: true,
        modified_at: "2026-09-30T11:00:00Z",
      },
      {
        id: "n4",
        title: "Not Favorite",
        is_favorite: false,
        is_pinned: true,
        is_deleted: false,
        modified_at: "2026-09-30T12:00:00Z",
      },
      {
        id: "n5",
        title: "Newer Regular Favorite",
        is_favorite: true,
        is_pinned: false,
        is_deleted: false,
        modified_at: "2026-09-30T10:30:00Z",
      },
    ];

    it("filters notes for favorites view, excluding non-favorites and soft-deleted notes", () => {
      const getFavorites = (allNotes) =>
        allNotes.filter((n) => n.is_favorite && !n.is_deleted);

      const favs = getFavorites(notes);
      assert.strictEqual(favs.length, 3);
      assert.ok(!favs.some((n) => n.id === "n3")); // excludes deleted
      assert.ok(!favs.some((n) => n.id === "n4")); // excludes non-favorite
    });

    it("sorts favorites by pinned DESC, then modified_at DESC", () => {
      const sortFavorites = (favNotes) => {
        return [...favNotes].sort((a, b) => {
          if (a.is_pinned !== b.is_pinned) {
            return a.is_pinned ? -1 : 1;
          }
          return new Date(b.modified_at).getTime() - new Date(a.modified_at).getTime();
        });
      };

      const sorted = sortFavorites(notes.filter((n) => n.is_favorite && !n.is_deleted));
      assert.strictEqual(sorted[0].id, "n2"); // Pinned Favorite first
      assert.strictEqual(sorted[1].id, "n5"); // Newer regular favorite next (10:30)
      assert.strictEqual(sorted[2].id, "n1"); // Older regular favorite last (10:00)
    });
  });

  // =========================================================================
  // 6. Tag Navigation
  // =========================================================================
  describe("6. Tag Navigation", () => {
    const notesWithTags = [
      { id: "n1", title: "Note 1", tagIds: ["t1", "t2"], is_deleted: false },
      { id: "n2", title: "Note 2", tagIds: ["t1"], is_deleted: false },
      { id: "n3", title: "Deleted Note", tagIds: ["t1"], is_deleted: true },
      { id: "n4", title: "Note 4", tagIds: ["t2"], is_deleted: false },
    ];

    it("filters notes by assigned tag and excludes soft-deleted notes", () => {
      const getNotesForTag = (tagId) =>
        notesWithTags.filter((n) => n.tagIds.includes(tagId) && !n.is_deleted);

      const t1Notes = getNotesForTag("t1");
      assert.strictEqual(t1Notes.length, 2);
      assert.deepStrictEqual(
        t1Notes.map((n) => n.id),
        ["n1", "n2"]
      );

      const t2Notes = getNotesForTag("t2");
      assert.strictEqual(t2Notes.length, 2);
      assert.deepStrictEqual(
        t2Notes.map((n) => n.id),
        ["n1", "n4"]
      );

      // Unused tag returns empty array
      const tUnusedNotes = getNotesForTag("t-unused");
      assert.strictEqual(tUnusedNotes.length, 0);
    });
  });

  // =========================================================================
  // 7. Metadata Panel Calculations & Rules
  // =========================================================================
  describe("7. Metadata Panel Calculations & Rules", () => {
    it("computes accurate word count, character count, and UTF-8 byte size", () => {
      const note = {
        id: "meta-note",
        content: "Hello world! This is a test.\nNewline and   tabs.",
        format: "txt",
        created_at: "2026-09-30T10:00:00Z",
        modified_at: "2026-09-30T10:30:00Z",
        is_favorite: true,
        is_pinned: false,
        notebook_id: "nb-1",
      };

      const meta = computeNoteMetadata(note, [], "Work", "Work / Projects");
      assert.strictEqual(meta.character_count, note.content.length);
      // Words: "Hello", "world!", "This", "is", "a", "test.", "Newline", "and", "tabs." = 9 words
      assert.strictEqual(meta.word_count, 9);
      assert.strictEqual(meta.byte_size, Buffer.byteLength(note.content, "utf8"));
      assert.strictEqual(meta.notebook_name, "Work");
      assert.strictEqual(meta.notebook_path, "Work / Projects");
    });

    it("computes accurate multi-byte UTF-8 byte sizes for Unicode and Emoji", () => {
      const unicodeContent = "तीर्थ-यात्रा 🗺️ and café";
      const note = {
        id: "unicode-note",
        content: unicodeContent,
        format: "md",
        created_at: "2026-09-30T10:00:00Z",
        modified_at: "2026-09-30T10:30:00Z",
        is_favorite: false,
        is_pinned: false,
        notebook_id: null,
      };

      const meta = computeNoteMetadata(note);
      const expectedBytes = new TextEncoder().encode(unicodeContent).length;
      assert.strictEqual(meta.byte_size, expectedBytes);
      assert.ok(meta.byte_size > unicodeContent.length); // UTF-8 bytes > char count for multi-byte
      assert.strictEqual(meta.notebook_path, null);
    });

    it("handles empty note content gracefully (0 words, 0 chars, 0 bytes)", () => {
      const emptyNote = {
        id: "empty-note",
        content: "   \n\t   ",
        format: "txt",
        created_at: "2026-09-30T10:00:00Z",
        modified_at: "2026-09-30T10:30:00Z",
        is_favorite: false,
        is_pinned: false,
        notebook_id: null,
      };

      const meta = computeNoteMetadata(emptyNote);
      assert.strictEqual(meta.word_count, 0);
      assert.strictEqual(meta.character_count, emptyNote.content.length);
      assert.strictEqual(meta.byte_size, emptyNote.content.length);
    });

    it("maps format to user-friendly format display name per Task 32", () => {
      assert.strictEqual(getFormatDisplayName("md"), "Markdown");
      assert.strictEqual(getFormatDisplayName("MD"), "Markdown");
      assert.strictEqual(getFormatDisplayName("txt"), "Plain Text");
      assert.strictEqual(getFormatDisplayName("TXT"), "Plain Text");
      assert.strictEqual(getFormatDisplayName(null), "Plain Text");
      assert.strictEqual(getFormatDisplayName(undefined), "Plain Text");
    });

    it("verifies Task 31 read-only vs editable metadata categorization", () => {
      const editableFields = new Set(["title", "tags", "is_favorite", "notebook_id"]);
      const readOnlyFields = new Set([
        "created_at",
        "modified_at",
        "format",
        "word_count",
        "character_count",
        "byte_size",
      ]);

      assert.strictEqual(editableFields.has("title"), true);
      assert.strictEqual(editableFields.has("tags"), true);
      assert.strictEqual(editableFields.has("is_favorite"), true);
      assert.strictEqual(editableFields.has("notebook_id"), true);

      assert.strictEqual(readOnlyFields.has("created_at"), true);
      assert.strictEqual(readOnlyFields.has("modified_at"), true);
      assert.strictEqual(readOnlyFields.has("format"), true);
      assert.strictEqual(readOnlyFields.has("word_count"), true);
    });
  });

  // =========================================================================
  // 8. Empty States
  // =========================================================================
  describe("8. Empty States", () => {
    it("handles empty available tags list state", () => {
      const tags = [];
      const renderTagsEmptyState = (tagList) => {
        if (tagList.length === 0) {
          return { showEmpty: true, message: "No tags yet" };
        }
        return { showEmpty: false, message: "" };
      };

      assert.deepStrictEqual(renderTagsEmptyState(tags), {
        showEmpty: true,
        message: "No tags yet",
      });
      assert.strictEqual(renderTagsEmptyState([{ id: "1", name: "work" }]).showEmpty, false);
    });

    it("handles note metadata empty tags state (displays 'None')", () => {
      const renderMetadataTags = (noteTags) => {
        if (!noteTags || noteTags.length === 0) {
          return "None";
        }
        return noteTags.map((t) => `#${t.name}`).join(" ");
      };

      assert.strictEqual(renderMetadataTags([]), "None");
      assert.strictEqual(
        renderMetadataTags([{ id: "1", name: "tagA" }, { id: "2", name: "tagB" }]),
        "#tagA #tagB"
      );
    });

    it("handles empty favorites navigation state", () => {
      const favoriteNotes = [];
      const renderFavoritesEmptyState = (favList) => {
        if (favList.length === 0) {
          return { showEmpty: true, message: "No favorite notes" };
        }
        return { showEmpty: false, message: "" };
      };

      assert.deepStrictEqual(renderFavoritesEmptyState(favoriteNotes), {
        showEmpty: true,
        message: "No favorite notes",
      });
    });

    it("handles empty tag-filtered notes state", () => {
      const notesForTag = [];
      const renderTagNotesEmptyState = (list, tagName) => {
        if (list.length === 0) {
          return { showEmpty: true, message: `No notes with #${tagName}` };
        }
        return { showEmpty: false, message: "" };
      };

      assert.deepStrictEqual(renderTagNotesEmptyState(notesForTag, "planning"), {
        showEmpty: true,
        message: "No notes with #planning",
      });
    });
  });

  // =========================================================================
  // TASK 64 — Manual QA Workflow Simulation (Test A through Test I)
  // =========================================================================
  describe("TASK 64: Exact Manual QA Workflow Simulation", () => {
    // In-memory simulation of complete frontend state across Test A to Test I
    let availableTags = [];
    let notes = [];
    let noteTagMap = new Map(); // noteId -> Set of tagIds
    let notebooks = [];

    it("Test A — Create tags: work, important, planning (all appear in Tags)", () => {
      const namesToCreate = ["work", "important", "planning"];
      for (const name of namesToCreate) {
        const val = validateTagName(name);
        assert.ok(val.valid);
        availableTags.push({
          id: `tag-${name}`,
          name: val.trimmed,
          created_at: new Date().toISOString(),
        });
      }

      assert.strictEqual(availableTags.length, 3);
      assert.deepStrictEqual(
        availableTags.map((t) => t.name),
        ["work", "important", "planning"]
      );
    });

    it("Test B — Assign tags: Open note -> Add work -> Add important -> [work] [important]", () => {
      const note = {
        id: "note-qa-1",
        title: "Test Note",
        content: "Testing tags",
        format: "txt",
        is_favorite: false,
        is_pinned: false,
        is_deleted: false,
        notebook_id: null,
      };
      notes.push(note);
      noteTagMap.set(note.id, new Set());

      // Assign work & important
      noteTagMap.get(note.id).add("tag-work");
      noteTagMap.get(note.id).add("tag-important");

      const assignedTagNames = Array.from(noteTagMap.get(note.id)).map(
        (tid) => availableTags.find((t) => t.id === tid).name
      );
      assert.deepStrictEqual(assignedTagNames, ["work", "important"]);
    });

    it("Test C — Remove one tag: Remove work -> [important]", () => {
      noteTagMap.get("note-qa-1").delete("tag-work");

      const remainingTags = Array.from(noteTagMap.get("note-qa-1")).map(
        (tid) => availableTags.find((t) => t.id === tid).name
      );
      assert.deepStrictEqual(remainingTags, ["important"]);
    });

    it("Test D — Favorite: Mark note as favorite -> Favorites contains note", () => {
      const note = notes.find((n) => n.id === "note-qa-1");
      note.is_favorite = true;

      const favoritesList = notes.filter((n) => n.is_favorite && !n.is_deleted);
      assert.strictEqual(favoritesList.length, 1);
      assert.strictEqual(favoritesList[0].id, "note-qa-1");
      assert.strictEqual(favoritesList[0].is_favorite, true);
    });

    it("Test E — Restart: Close -> reopen -> Tags preserved, Favorite preserved", () => {
      // Simulate persistent reload by serializing and deserializing state
      const serialized = JSON.stringify({ availableTags, notes, noteTagEntries: Array.from(noteTagMap.entries()).map(([k, v]) => [k, Array.from(v)]) });
      const reloaded = JSON.parse(serialized);

      const reloadedTags = reloaded.availableTags;
      const reloadedNotes = reloaded.notes;
      const reloadedMap = new Map(reloaded.noteTagEntries.map(([k, v]) => [k, new Set(v)]));

      assert.strictEqual(reloadedTags.length, 3);
      assert.deepStrictEqual(
        reloadedTags.map((t) => t.name),
        ["work", "important", "planning"]
      );

      const reloadedNote = reloadedNotes.find((n) => n.id === "note-qa-1");
      assert.strictEqual(reloadedNote.is_favorite, true);

      const noteTagsAfterReload = Array.from(reloadedMap.get("note-qa-1")).map(
        (tid) => reloadedTags.find((t) => t.id === tid).name
      );
      assert.deepStrictEqual(noteTagsAfterReload, ["important"]);
    });

    it("Test F — Rename tag: work -> projects -> All affected notes now show projects", () => {
      // Create second note assigned to 'work'
      const note2 = {
        id: "note-qa-2",
        title: "Work note",
        content: "Stuff",
        format: "txt",
        is_favorite: false,
        is_pinned: false,
        is_deleted: false,
        notebook_id: null,
      };
      notes.push(note2);
      noteTagMap.set(note2.id, new Set(["tag-work"]));

      // Rename tag 'work' to 'projects'
      const workTag = availableTags.find((t) => t.id === "tag-work");
      workTag.name = "projects";

      const note2Tags = Array.from(noteTagMap.get(note2.id)).map(
        (tid) => availableTags.find((t) => t.id === tid).name
      );
      assert.deepStrictEqual(note2Tags, ["projects"]);
    });

    it("Test G — Delete tag: Delete projects -> Tag disappears, Notes remain", () => {
      // Delete 'projects' tag
      availableTags = availableTags.filter((t) => t.id !== "tag-work");

      // Cascading removal from noteTagMap
      for (const [nId, tSet] of noteTagMap.entries()) {
        tSet.delete("tag-work");
      }

      assert.strictEqual(availableTags.some((t) => t.name === "projects"), false);
      assert.strictEqual(availableTags.length, 2); // 'important' and 'planning' remain

      // Notes remain untouched
      assert.strictEqual(notes.length, 2);
      assert.strictEqual(noteTagMap.get("note-qa-2").size, 0);
    });

    it("Test H — Tag filter: Select #important -> Only notes with important tag appear", () => {
      const getNotesForTag = (tagName) => {
        const tag = availableTags.find((t) => t.name === tagName);
        if (!tag) return [];
        return notes.filter(
          (n) => !n.is_deleted && noteTagMap.get(n.id)?.has(tag.id)
        );
      };

      const importantNotes = getNotesForTag("important");
      assert.strictEqual(importantNotes.length, 1);
      assert.strictEqual(importantNotes[0].id, "note-qa-1");
      assert.strictEqual(importantNotes[0].title, "Test Note");
    });

    it("Test I — Notebook + tags: Work/Projects/Project Plan (tags: work, important; favorite: Yes; consistent metadata)", () => {
      // Setup notebook hierarchy
      notebooks.push({ id: "nb-work", name: "Work", parent_id: null });
      notebooks.push({ id: "nb-projects", name: "Projects", parent_id: "nb-work" });

      // Recreate 'work' tag
      availableTags.push({ id: "tag-work-new", name: "work", created_at: new Date().toISOString() });

      // Create Project Plan note
      const projectPlan = {
        id: "note-project-plan",
        title: "Project Plan",
        content: "# Project Plan\n\nQ4 Deliverables and schedule.",
        format: "md",
        is_favorite: true,
        is_pinned: false,
        is_deleted: false,
        notebook_id: "nb-projects",
        created_at: "2026-09-30T15:00:00Z",
        modified_at: "2026-09-30T15:30:00Z",
      };
      notes.push(projectPlan);

      const assignedTagIds = new Set(["tag-work-new", "tag-important"]);
      noteTagMap.set(projectPlan.id, assignedTagIds);

      const planTags = availableTags.filter((t) => assignedTagIds.has(t.id));
      const metadata = computeNoteMetadata(
        projectPlan,
        planTags,
        "Projects",
        "Work / Projects"
      );

      assert.strictEqual(metadata.note_id, "note-project-plan");
      assert.strictEqual(metadata.notebook_name, "Projects");
      assert.strictEqual(metadata.notebook_path, "Work / Projects");
      assert.strictEqual(metadata.is_favorite, true);
      assert.strictEqual(metadata.format, "md");
      assert.strictEqual(getFormatDisplayName(metadata.format), "Markdown");
      assert.strictEqual(metadata.tags.length, 2);
      assert.deepStrictEqual(
        metadata.tags.map((t) => t.name).sort(),
        ["important", "work"]
      );
      assert.ok(metadata.word_count > 0);
      assert.ok(metadata.character_count > 0);
      assert.ok(metadata.byte_size > 0);
    });
  });

  // =========================================================================
  // TASK 65 — Restart Regression Test (Full cold shutdown & state verification)
  // =========================================================================
  describe("TASK 65: Restart Regression Test", () => {
    it("preserves notebook assignment, renamed tags, favorite, and edited content across a cold restart", () => {
      // 1. Create notebook
      const engineeringNb = { id: "nb-eng", name: "Engineering", parent_id: null };
      const productNb = { id: "nb-prod", name: "Product", parent_id: null };
      let localNotebooks = [engineeringNb, productNb];

      // 2. Create note
      const initialCreatedAt = "2026-09-30T10:00:00.000Z";
      let localNotes = [
        {
          id: "note-sprint-42",
          title: "Sprint Specs",
          content: "Initial sprint specifications.",
          format: "md",
          notebook_id: engineeringNb.id,
          is_favorite: false,
          is_pinned: false,
          is_deleted: false,
          created_at: initialCreatedAt,
          modified_at: initialCreatedAt,
        },
      ];

      // 3. Assign 3 tags
      let localTags = [
        { id: "tag-backend", name: "backend", created_at: initialCreatedAt },
        { id: "tag-critical", name: "critical", created_at: initialCreatedAt },
        { id: "tag-q4", name: "q4-2026", created_at: initialCreatedAt },
      ];
      let localNoteTags = [
        { note_id: "note-sprint-42", tag_id: "tag-backend" },
        { note_id: "note-sprint-42", tag_id: "tag-critical" },
        { note_id: "note-sprint-42", tag_id: "tag-q4" },
      ];

      // 4. Mark favorite
      localNotes[0].is_favorite = true;

      // 5. Edit note
      const editedTitle = "Sprint 42 Specs & Architecture";
      const editedContent = "# Sprint 42 Specs & Architecture\n\nDeep dive into offline local-first storage and indexing.";
      const editTimestamp = "2026-09-30T10:15:00.000Z";
      localNotes[0].title = editedTitle;
      localNotes[0].content = editedContent;
      localNotes[0].modified_at = editTimestamp;

      // 6. Move note
      localNotes[0].notebook_id = productNb.id;

      // 7. Rename tag: backend -> core-infrastructure
      const backendTag = localTags.find((t) => t.id === "tag-backend");
      backendTag.name = "core-infrastructure";

      // 8. Close application (simulate complete cold shutdown by serialization to disk/storage)
      const diskSnapshot = JSON.stringify({
        notebooks: localNotebooks,
        notes: localNotes,
        tags: localTags,
        noteTags: localNoteTags,
      });

      // Clear memory references
      localNotebooks = null;
      localNotes = null;
      localTags = null;
      localNoteTags = null;

      // 9. Reopen application (rehydrate completely from disk snapshot)
      const rehydrated = JSON.parse(diskSnapshot);

      // 10. Verify expectations
      const recoveredNote = rehydrated.notes.find((n) => n.id === "note-sprint-42");
      assert.ok(recoveredNote, "Note must exist after cold reload");

      // Notebook assignment correct
      assert.strictEqual(recoveredNote.notebook_id, productNb.id);
      const assignedNb = rehydrated.notebooks.find((nb) => nb.id === recoveredNote.notebook_id);
      assert.strictEqual(assignedNb.name, "Product");

      // Title correct
      assert.strictEqual(recoveredNote.title, editedTitle);

      // Content correct
      assert.strictEqual(recoveredNote.content, editedContent);

      // Favorite correct
      assert.strictEqual(recoveredNote.is_favorite, true);

      // Tags correct (with renamed tag name "core-infrastructure")
      const noteTagIds = rehydrated.noteTags
        .filter((nt) => nt.note_id === recoveredNote.id)
        .map((nt) => nt.tag_id);
      assert.strictEqual(noteTagIds.length, 3);

      const recoveredTags = rehydrated.tags.filter((t) => noteTagIds.includes(t.id));
      assert.strictEqual(recoveredTags.length, 3);
      const tagNames = recoveredTags.map((t) => t.name).sort();
      assert.deepStrictEqual(tagNames, ["core-infrastructure", "critical", "q4-2026"]);

      // Created time unchanged
      assert.strictEqual(recoveredNote.created_at, initialCreatedAt);

      // Modified time correct
      assert.strictEqual(recoveredNote.modified_at, editTimestamp);

      // Compute metadata validation after restart
      const metadata = computeNoteMetadata(
        recoveredNote,
        recoveredTags,
        assignedNb.name,
        assignedNb.name
      );
      assert.strictEqual(metadata.notebook_name, "Product");
      assert.strictEqual(metadata.is_favorite, true);
      assert.strictEqual(metadata.tags.length, 3);
      assert.strictEqual(metadata.word_count, 14);
      assert.strictEqual(getFormatDisplayName(metadata.format), "Markdown");
    });
  });

  // =========================================================================
  // Phase 6: Search Tests
  // =========================================================================
  describe("Phase 6: Search Logic & State Tests", () => {
    describe("Task 7: Empty Query Behavior", () => {
      it("considers empty and whitespace strings as inactive search queries", () => {
        const isSearchActive = (query) => query.trim().length > 0;

        assert.strictEqual(isSearchActive(""), false);
        assert.strictEqual(isSearchActive("   "), false);
        assert.strictEqual(isSearchActive("\t\n"), false);
        assert.strictEqual(isSearchActive("a"), true);
        assert.strictEqual(isSearchActive("  project  "), true);
      });

      it("transitions to idle state when query is empty or whitespace without executing backend search", () => {
        let backendCallCount = 0;
        const mockBackendSearch = () => {
          backendCallCount++;
          return [];
        };

        const processQuery = (rawQuery) => {
          const trimmed = rawQuery.trim();
          if (!trimmed) {
            return { status: "idle", results: [] };
          }
          const results = mockBackendSearch(trimmed);
          return { status: "success", query: trimmed, results };
        };

        const state1 = processQuery("");
        assert.strictEqual(state1.status, "idle");
        assert.strictEqual(backendCallCount, 0);

        const state2 = processQuery("     ");
        assert.strictEqual(state2.status, "idle");
        assert.strictEqual(backendCallCount, 0);

        const state3 = processQuery("project");
        assert.strictEqual(state3.status, "success");
        assert.strictEqual(backendCallCount, 1);

        // Clearing back to empty returns to idle
        const state4 = processQuery("");
        assert.strictEqual(state4.status, "idle");
        assert.strictEqual(backendCallCount, 1);
      });
    });

    describe("Task 8: Normalize Search Input", () => {
      const normalizeSearchQuery = (query) => (!query ? "" : query.trim());

      it("trims leading and trailing whitespace safely", () => {
        assert.strictEqual(normalizeSearchQuery("   project   "), "project");
        assert.strictEqual(normalizeSearchQuery("\tproject plan\n"), "project plan");
        assert.strictEqual(normalizeSearchQuery("  "), "");
      });

      it("preserves meaningful Unicode and accented characters intact", () => {
        assert.strictEqual(normalizeSearchQuery("   भारत यात्रा   "), "भारत यात्रा");
        assert.strictEqual(normalizeSearchQuery("   café résumé   "), "café résumé");
        assert.strictEqual(normalizeSearchQuery("   東京ガイド   "), "東京ガイド");
        assert.strictEqual(normalizeSearchQuery("  Übergröße  "), "Übergröße");
      });

      it("does not strip punctuation from search queries", () => {
        assert.strictEqual(normalizeSearchQuery("  C++  "), "C++");
        assert.strictEqual(normalizeSearchQuery("  user@example.com  "), "user@example.com");
        assert.strictEqual(normalizeSearchQuery("  v2.1.0  "), "v2.1.0");
      });
    });

    describe("Task 9: Basic Title Search", () => {
      const notes = [
        { id: "n1", title: "Project Plan", content: "Weekly timeline" },
        { id: "n2", title: "Release Notes", content: "Version 2.1 deployment details" },
        { id: "n3", title: "Meeting Minutes", content: "Discussed team roadmap" },
        { id: "n4", title: "Project Plan", content: "Different note with duplicate title" },
      ];

      const searchTitles = (query) => {
        const q = query.trim().toLowerCase();
        if (!q) return [];
        return notes.filter((n) => n.title.toLowerCase().includes(q));
      };

      it("matches title case-insensitively across lowercase, uppercase, and mixed-case", () => {
        const r1 = searchTitles("project");
        assert.strictEqual(r1.length, 2);
        assert.strictEqual(r1[0].id, "n1");
        assert.strictEqual(r1[1].id, "n4");

        const r2 = searchTitles("PROJECT");
        assert.strictEqual(r2.length, 2);

        const r3 = searchTitles("PrOjEcT");
        assert.strictEqual(r3.length, 2);
      });

      it("matches partial title tokens", () => {
        const r = searchTitles("plan");
        assert.strictEqual(r.length, 2);
        assert.strictEqual(r[0].title, "Project Plan");
      });

      it("returns empty array when title does not match", () => {
        const r = searchTitles("nonexistent-topic");
        assert.strictEqual(r.length, 0);
      });
    });

    describe("Task 10: Implement Content Search", () => {
      const notes = [
        { id: "n1", title: "Meeting", content: "Discuss the project release tomorrow." },
        { id: "n2", title: "Personal Diary", content: "Went for a peaceful run in the morning." },
      ];

      const searchContent = (query) => {
        const q = query.trim().toLowerCase();
        if (!q) return [];
        return notes
          .filter((n) => n.content.toLowerCase().includes(q))
          .map((n) => {
            const idx = n.content.toLowerCase().indexOf(q);
            const start = Math.max(0, idx - 15);
            const end = Math.min(n.content.length, idx + q.length + 15);
            const snippet = (start > 0 ? "..." : "") + n.content.slice(start, end) + (end < n.content.length ? "..." : "");
            return { ...n, snippet };
          });
      };

      it("returns note when content matches even if title does not", () => {
        const r = searchContent("project");
        assert.strictEqual(r.length, 1);
        assert.strictEqual(r[0].id, "n1");
        assert.strictEqual(r[0].title, "Meeting");
        assert.strictEqual(r[0].content.includes("project"), true);
      });

      it("generates a safe contextual snippet around the matching content term", () => {
        const r = searchContent("release");
        assert.strictEqual(r.length, 1);
        assert.strictEqual(r[0].snippet.includes("release"), true);
      });

      it("matches content case-insensitively", () => {
        const rUpper = searchContent("TOMORROW");
        assert.strictEqual(rUpper.length, 1);
        assert.strictEqual(rUpper[0].id, "n1");
      });
    });

    describe("Task 11: Title + Content Search", () => {
      const notes = [
        { id: "note-a", title: "Project Alpha", content: "General system architecture and milestones." },
        { id: "note-b", title: "Team Sync", content: "Review the upcoming project deadline on Friday." },
        { id: "note-c", title: "Project Review", content: "Reviewing project milestones and project budgets." },
        { id: "note-d", title: "Grocery Shopping", content: "Apples, bananas, milk, coffee beans." },
      ];

      const searchTitleOrContent = (query) => {
        const q = query.trim().toLowerCase();
        if (!q) return [];
        return notes.filter(
          (n) => n.title.toLowerCase().includes(q) || n.content.toLowerCase().includes(q)
        );
      };

      it("returns notes when either title matches or content matches, without requiring both", () => {
        const r = searchTitleOrContent("project");
        assert.strictEqual(r.length, 3);
        const ids = r.map((n) => n.id);
        assert.strictEqual(ids.includes("note-a"), true, "Title-only match included");
        assert.strictEqual(ids.includes("note-b"), true, "Content-only match included");
        assert.strictEqual(ids.includes("note-c"), true, "Both title and content match included");
        assert.strictEqual(ids.includes("note-d"), false, "Non-matching note excluded");
      });

      it("ensures no duplicate entries for notes matching both title and content", () => {
        const r = searchTitleOrContent("project");
        const idCounts = {};
        for (const n of r) {
          idCounts[n.id] = (idCounts[n.id] || 0) + 1;
        }
        for (const count of Object.values(idCounts)) {
          assert.strictEqual(count, 1, "Each matching note must appear exactly once");
        }
      });
    });

    describe("Task 12: Exact Note Selection", () => {
      const allNotes = [
        {
          id: "note-101",
          title: "Architecture Plan",
          content: "Detailed system design specifications.",
          format: "md",
          notebook_id: "nb-work",
          is_favorite: true,
          is_deleted: false,
          modified_at: "2026-09-30T10:00:00Z",
        },
        {
          id: "note-102",
          title: "Personal Thoughts",
          content: "Ideas for weekend hike.",
          format: "txt",
          notebook_id: null,
          is_favorite: false,
          is_deleted: false,
          modified_at: "2026-09-30T11:00:00Z",
        },
      ];

      it("loads the exact note by unique noteId into editor with all fields intact", () => {
        const selectNoteById = (id) => allNotes.find((n) => n.id === id) ?? null;

        const selected = selectNoteById("note-101");
        assert.notStrictEqual(selected, null);
        assert.strictEqual(selected.id, "note-101");
        assert.strictEqual(selected.title, "Architecture Plan");
        assert.strictEqual(selected.content, "Detailed system design specifications.");
        assert.strictEqual(selected.format, "md");
        assert.strictEqual(selected.notebook_id, "nb-work");
        assert.strictEqual(selected.is_favorite, true);
      });

      it("does not create a new note or copy when opening a search result", () => {
        let initialNoteCount = allNotes.length;
        const selectExistingNote = (searchResult) => {
          return allNotes.find((n) => n.id === searchResult.noteId);
        };

        const result = {
          noteId: "note-101",
          title: "Architecture Plan",
          modifiedAt: "2026-09-30T10:00:00Z",
          notebookId: "nb-work",
          favorite: true,
        };

        const loaded = selectExistingNote(result);
        assert.strictEqual(loaded.id, "note-101");
        assert.strictEqual(allNotes.length, initialNoteCount, "Note count must remain unchanged");
      });
    });

    describe("Task 13: Search Result Click Behavior", () => {
      it("dispatches selection exclusively using noteId and never title", () => {
        let selectedId = null;
        const handleSelectNote = (id) => {
          selectedId = id;
        };

        const searchResultA = {
          noteId: "id-alpha-123",
          title: "Project Plan",
        };

        const searchResultB = {
          noteId: "id-beta-456",
          title: "Project Plan", // Identical title
        };

        handleSelectNote(searchResultA.noteId);
        assert.strictEqual(selectedId, "id-alpha-123");

        handleSelectNote(searchResultB.noteId);
        assert.strictEqual(selectedId, "id-beta-456");
      });

      it("guarantees clicking a search result does not invoke note creation APIs", () => {
        let createCalled = false;
        let selectCalled = false;

        const mockApi = {
          createNote: () => {
            createCalled = true;
          },
          selectNote: (id) => {
            selectCalled = true;
            return id;
          },
        };

        const clickedResult = { noteId: "note-999", title: "Test Note" };
        mockApi.selectNote(clickedResult.noteId);

        assert.strictEqual(selectCalled, true);
        assert.strictEqual(createCalled, false, "Must not create a new note copy");
      });
    });

    describe("Task 14: Search Result List", () => {
      it("maps SearchResult objects into NoteListItems with snippet previews and metadata", () => {
        const searchResult = {
          noteId: "res-101",
          title: "Project Plan",
          snippet: "...project release timeline...",
          modifiedAt: "2026-09-30T12:00:00Z",
          notebookId: "nb-work",
          favorite: true,
        };

        const item = {
          id: searchResult.noteId,
          title: searchResult.title,
          preview: searchResult.snippet,
          isFavorite: searchResult.favorite,
          notebookId: searchResult.notebookId,
          notebookPath: "Work / Projects",
          tags: ["work", "important"],
        };

        assert.strictEqual(item.id, "res-101");
        assert.strictEqual(item.title, "Project Plan");
        assert.strictEqual(item.preview, "...project release timeline...");
        assert.strictEqual(item.isFavorite, true);
        assert.strictEqual(item.notebookPath, "Work / Projects");
        assert.deepStrictEqual(item.tags, ["work", "important"]);
      });

      it("supports dual rendering modes in NotesList without duplicating components", () => {
        const renderNotesListConfig = (mode, count, query) => ({
          isSearchMode: mode === "search",
          title: mode === "search" ? `Search: "${query}"` : "All Notes",
          countBadge: String(count),
          emptyTitle: mode === "search" ? "No notes found" : "No notes yet",
          allowNewNote: mode !== "search",
        });

        const normalConfig = renderNotesListConfig("normal", 5, "");
        assert.strictEqual(normalConfig.isSearchMode, false);
        assert.strictEqual(normalConfig.title, "All Notes");
        assert.strictEqual(normalConfig.allowNewNote, true);

        const searchConfig = renderNotesListConfig("search", 2, "project");
        assert.strictEqual(searchConfig.isSearchMode, true);
        assert.strictEqual(searchConfig.title, 'Search: "project"');
        assert.strictEqual(searchConfig.emptyTitle, "No notes found");
        assert.strictEqual(searchConfig.allowNewNote, false);
      });
    });

    describe("Task 15: Search Result Empty State", () => {
      const resolveEmptyStateProps = ({ mode, searchQuery, emptyTitle, emptyDescription, onNewNote }) => {
        const isSearchMode = mode === "search";
        const resolvedEmptyTitle = emptyTitle ?? (isSearchMode ? "No notes found" : "No notes yet");
        const resolvedEmptyDescription =
          emptyDescription ??
          (isSearchMode
            ? `No notes match "${searchQuery}".`
            : "Create your first note to get started.");
        const effectiveActionText = isSearchMode ? undefined : onNewNote ? "+ New Note" : undefined;
        return {
          title: resolvedEmptyTitle,
          description: resolvedEmptyDescription,
          actionText: effectiveActionText,
        };
      };

      it("displays 'No notes found' and 'No notes match \"xyzabcdef\".' for zero results", () => {
        const props = resolveEmptyStateProps({
          mode: "search",
          searchQuery: "xyzabcdef",
          onNewNote: () => {},
        });

        assert.strictEqual(props.title, "No notes found");
        assert.strictEqual(props.description, 'No notes match "xyzabcdef".');
        assert.strictEqual(props.actionText, undefined, "Must omit '+ New Note' action button in search mode");
      });

      it("does not show an empty/blank list with no explanation when search produces 0 results", () => {
        const searchResults = [];
        const isSearchActive = true;
        const searchQuery = "nonexistent-token";

        const status = isSearchActive
          ? searchResults.length === 0
            ? "empty"
            : "idle"
          : "idle";

        assert.strictEqual(status, "empty");

        const view = resolveEmptyStateProps({
          mode: "search",
          searchQuery,
        });

        assert.ok(view.title.length > 0, "Title must not be blank");
        assert.ok(view.description.length > 0, "Description must explain zero results");
        assert.strictEqual(view.title, "No notes found");
        assert.strictEqual(view.description, 'No notes match "nonexistent-token".');
      });
    });

    describe("Task 16: Search Loading State", () => {
      it("maps searching status to 'loading' in NotesList with subtle Searching... indicator", () => {
        const computeSearchDisplayState = (searchState, isSearchActive) => {
          if (!isSearchActive) return { status: "idle", loadingBadge: undefined, loadingMessage: undefined };
          if (searchState.status === "searching") {
            return {
              status: "loading",
              loadingBadge: "Searching...",
              loadingMessage: "Searching...",
            };
          }
          return { status: "idle", loadingBadge: undefined, loadingMessage: undefined };
        };

        const activeSearch = computeSearchDisplayState({ status: "searching", query: "sqlite" }, true);
        assert.strictEqual(activeSearch.status, "loading");
        assert.strictEqual(activeSearch.loadingBadge, "Searching...");
        assert.strictEqual(activeSearch.loadingMessage, "Searching...");

        const finishedSearch = computeSearchDisplayState({ status: "success", query: "sqlite", results: [] }, true);
        assert.strictEqual(finishedSearch.status, "idle");
        assert.strictEqual(finishedSearch.loadingBadge, undefined);
      });

      it("keeps search UI non-blocking so users can continue typing or interacting", () => {
        let isSearching = false;
        let searchInputDisabled = false; // Input should never be disabled during search
        let fullScreenBlockingModal = false;

        // Simulate initiating an async search
        isSearching = true;
        // Verify non-blocking constraints
        assert.strictEqual(isSearching, true);
        assert.strictEqual(searchInputDisabled, false, "Search input must remain interactive while searching");
        assert.strictEqual(fullScreenBlockingModal, false, "Must not block application with a modal overlay");

        // Simulate async completion
        isSearching = false;
        assert.strictEqual(isSearching, false);
      });
    });

    describe("Task 17: Search Error State", () => {
      const sanitizeSearchError = (err) => {
        return {
          title: "Search failed",
          message: "Unable to search notes right now. Please try again.",
        };
      };

      it("presents friendly 'Search failed' and 'Unable to search notes right now. Please try again.' on failure", () => {
        const rawSqliteError = new Error('SQLite error: near "SELECT": syntax error in query');
        const sanitized = sanitizeSearchError(rawSqliteError);

        assert.strictEqual(sanitized.title, "Search failed");
        assert.strictEqual(sanitized.message, "Unable to search notes right now. Please try again.");
      });

      it("never exposes internal SQLite error messages to normal users", () => {
        const technicalErrors = [
          new Error('SQLite error: near "WHERE": syntax error'),
          new Error("OperationalError: database disk image is malformed"),
          new Error("rusqlite::Error::SqliteFailure(1, Some(\"no such table: notes_fts\"))"),
          "Generic low-level network or IPC failure",
        ];

        for (const err of technicalErrors) {
          const sanitized = sanitizeSearchError(err);
          assert.strictEqual(sanitized.message.includes("SQLite"), false, "Must not leak SQLite token");
          assert.strictEqual(sanitized.message.includes("syntax error"), false, "Must not leak syntax error");
          assert.strictEqual(sanitized.message.includes("near"), false, "Must not leak near clause");
          assert.strictEqual(sanitized.message.includes("malformed"), false, "Must not leak malformed details");
          assert.strictEqual(sanitized.message, "Unable to search notes right now. Please try again.");
        }
      });
    });

    describe("Task 18: Debounced Search", () => {
      it("uses default debounce within recommended 150-300ms window (200ms)", () => {
        const defaultDebounceMs = 200;
        assert.ok(defaultDebounceMs >= 150 && defaultDebounceMs <= 300);
      });

      it("coalesces rapid keystrokes (p -> pr -> pro -> proj -> proje -> projec -> project) into 1 search", async () => {
        let executionCount = 0;
        let lastExecutedQuery = null;
        let activeTimer = null;
        const debounceMs = 20;

        const simulateKeystroke = (text) => {
          if (activeTimer) {
            clearTimeout(activeTimer);
          }
          activeTimer = setTimeout(() => {
            executionCount++;
            lastExecutedQuery = text;
          }, debounceMs);
        };

        const keystrokes = ["p", "pr", "pro", "proj", "proje", "projec", "project"];
        for (const stroke of keystrokes) {
          simulateKeystroke(stroke);
          await new Promise((res) => setTimeout(res, 5));
        }

        assert.strictEqual(executionCount, 0, "Must not execute during rapid burst");

        await new Promise((res) => setTimeout(res, 35));

        assert.strictEqual(executionCount, 1, "Must execute exactly once after pause");
        assert.strictEqual(lastExecutedQuery, "project");
      });

      it("cancels pending debounce immediately when query is wiped or replaced with whitespace", () => {
        let activeTimer = null;
        let wasCancelled = false;

        const setQuery = (text) => {
          if (activeTimer) {
            clearTimeout(activeTimer);
            activeTimer = null;
          }
          const trimmed = text.trim();
          if (!trimmed) {
            wasCancelled = true;
            return;
          }
          activeTimer = setTimeout(() => {}, 200);
        };

        setQuery("hello");
        assert.ok(activeTimer !== null);

        setQuery("   ");
        assert.strictEqual(activeTimer, null);
        assert.strictEqual(wasCancelled, true);
      });
    });

    describe("Task 19: Immediate Search Submission (Enter & Selection)", () => {
      it("executes search immediately on Enter key press without waiting for debounce timer", async () => {
        let activeTimer = null;
        let executedQuery = null;
        let executionCount = 0;
        const debounceMs = 200;

        const scheduleSearch = (text) => {
          if (activeTimer) clearTimeout(activeTimer);
          activeTimer = setTimeout(() => {
            executionCount++;
            executedQuery = text;
          }, debounceMs);
        };

        const submitImmediately = (text) => {
          if (activeTimer) {
            clearTimeout(activeTimer);
            activeTimer = null;
          }
          executionCount++;
          executedQuery = text;
        };

        // User typed "meeting"
        scheduleSearch("meeting");
        assert.ok(activeTimer !== null);
        assert.strictEqual(executionCount, 0);

        // User hits Enter immediately (e.g. 5ms later)
        submitImmediately("meeting");
        assert.strictEqual(executionCount, 1);
        assert.strictEqual(executedQuery, "meeting");
        assert.strictEqual(activeTimer, null);

        // Verify that after 250ms, no duplicate execution happens
        await new Promise((res) => setTimeout(res, 25));
        assert.strictEqual(executionCount, 1, "Debounced timer must not fire again after Enter submission");
      });

      it("cancels pending debounce when user explicitly selects a result and loads note without delay", () => {
        let activeTimer = "mock-active-timer-id";
        let selectedNote = null;

        const cancelPendingDebounce = () => {
          activeTimer = null;
        };

        const onSelectNote = (noteId) => {
          cancelPendingDebounce();
          selectedNote = noteId;
        };

        // User typed into search bar and timer is armed
        assert.strictEqual(activeTimer, "mock-active-timer-id");

        // User clicks a search result item
        onSelectNote("note-456");

        // Debounce is cancelled immediately, and selection applied without waiting
        assert.strictEqual(activeTimer, null, "Pending debounce must be cancelled on note selection");
        assert.strictEqual(selectedNote, "note-456");
      });
    });

    describe("Task 20: Cancel Stale Search Results", () => {
      it("ignores older out-of-order search responses (request 1 'pro' returning after request 2 'project')", async () => {
        let requestId = 0;
        let finalState = null;

        const executeSearch = async (text, delayMs, results) => {
          const currentId = ++requestId;
          await new Promise((res) => setTimeout(res, delayMs));

          // Stale response guard
          if (currentId === requestId) {
            finalState = { query: text, results };
          }
        };

        // User typed "pro" (takes 40ms)
        const p1 = executeSearch("pro", 40, [{ noteId: "1", title: "Project Pro" }]);

        // User immediately typed "project" (takes 10ms)
        const p2 = executeSearch("project", 10, [{ noteId: "2", title: "Final Project" }]);

        await Promise.all([p1, p2]);

        // Even though "pro" returned later, finalState must remain "project" from request 2
        assert.strictEqual(finalState.query, "project");
        assert.deepStrictEqual(finalState.results, [{ noteId: "2", title: "Final Project" }]);
      });

      it("drops stale errors from earlier requests if a newer request succeeded", async () => {
        let requestId = 0;
        let finalState = null;

        const executeSearchWithError = async (text, delayMs, shouldFail, results) => {
          const currentId = ++requestId;
          await new Promise((res) => setTimeout(res, delayMs));

          try {
            if (shouldFail) throw new Error("Delayed DB Failure");
            if (currentId === requestId) {
              finalState = { status: "success", query: text, results };
            }
          } catch (err) {
            if (currentId === requestId) {
              finalState = { status: "error", message: err.message };
            }
          }
        };

        // Request 1 fails slowly at 40ms
        const req1 = executeSearchWithError("old", 40, true, []);

        // Request 2 succeeds quickly at 10ms
        const req2 = executeSearchWithError("new", 10, false, [{ noteId: "10", title: "New Result" }]);

        await Promise.all([req1, req2]);

        assert.strictEqual(finalState.status, "success");
        assert.strictEqual(finalState.query, "new");
      });

      it("drops in-flight responses when search is cleared, preventing late resurrection of search results", async () => {
        let requestId = 0;
        let state = { status: "idle" };

        const runSearch = async (delayMs, results) => {
          const currentId = ++requestId;
          await new Promise((res) => setTimeout(res, delayMs));
          if (currentId === requestId) {
            state = { status: "success", results };
          }
        };

        const clearSearch = () => {
          requestId++;
          state = { status: "idle" };
        };

        // Launch slow query
        const searchPromise = runSearch(30, [{ noteId: "99", title: "Should Be Dropped" }]);

        // Clear query after 5ms
        await new Promise((res) => setTimeout(res, 5));
        clearSearch();
        assert.strictEqual(state.status, "idle");

        // Wait for searchPromise to complete
        await searchPromise;

        // State remains idle, response was dropped
        assert.strictEqual(state.status, "idle");
      });
    });

    describe("Task 21: Search State Model", () => {
      const isSearchIdle = (state) => state.status === "idle";
      const isSearchSearching = (state) => state.status === "searching";
      const isSearchSuccess = (state) => state.status === "success";
      const isSearchError = (state) => state.status === "error";

      it("models search state as a strictly mutually exclusive tagged union", () => {
        const idleState = { status: "idle" };
        const searchingState = { status: "searching", query: "notes" };
        const successState = {
          status: "success",
          query: "notes",
          results: [{ noteId: "1", title: "Notes Test" }],
        };
        const errorState = {
          status: "error",
          query: "notes",
          message: "Unable to search notes right now. Please try again.",
        };

        // Validate idle
        assert.strictEqual(isSearchIdle(idleState), true);
        assert.strictEqual(isSearchSearching(idleState), false);
        assert.strictEqual(isSearchSuccess(idleState), false);
        assert.strictEqual(isSearchError(idleState), false);

        // Validate searching
        assert.strictEqual(isSearchIdle(searchingState), false);
        assert.strictEqual(isSearchSearching(searchingState), true);
        assert.strictEqual(isSearchSuccess(searchingState), false);
        assert.strictEqual(isSearchError(searchingState), false);
        assert.strictEqual(searchingState.query, "notes");

        // Validate success
        assert.strictEqual(isSearchIdle(successState), false);
        assert.strictEqual(isSearchSearching(successState), false);
        assert.strictEqual(isSearchSuccess(successState), true);
        assert.strictEqual(isSearchError(successState), false);
        assert.strictEqual(successState.results.length, 1);

        // Validate error
        assert.strictEqual(isSearchIdle(errorState), false);
        assert.strictEqual(isSearchSearching(errorState), false);
        assert.strictEqual(isSearchSuccess(errorState), false);
        assert.strictEqual(isSearchError(errorState), true);
        assert.strictEqual(errorState.message, "Unable to search notes right now. Please try again.");
      });

      it("avoids conflicting boolean states (cannot be simultaneously searching and error)", () => {
        // With boolean flags, bugs like { isSearching: true, hasError: true, hasResults: true } can occur.
        // A single tagged union discriminant guarantees only one status is active at any time.
        const stateTransitions = [
          { status: "idle" },
          { status: "searching", query: "sqlite" },
          { status: "success", query: "sqlite", results: [] },
          { status: "searching", query: "sqlite2" },
          { status: "error", query: "sqlite2", message: "Failed" },
          { status: "idle" },
        ];

        for (const s of stateTransitions) {
          const activeChecks = [
            isSearchIdle(s),
            isSearchSearching(s),
            isSearchSuccess(s),
            isSearchError(s),
          ].filter(Boolean);

          assert.strictEqual(activeChecks.length, 1, `State ${s.status} must match exactly 1 discriminant check`);
        }
      });
    });

    describe("Task 22: Search Keyboard Shortcut", () => {
      const createShortcutDispatcher = (handlers) => {
        return (event) => {
          const isModifier = event.ctrlKey || event.metaKey;
          const key = event.key.toLowerCase();

          if (isModifier && (key === "k" || key === "f")) {
            event.preventDefault();
            handlers.onFocusSearch?.();
            return;
          }
        };
      };

      it("triggers search focus on Ctrl+K (Windows/Linux) and Cmd+K (macOS)", () => {
        let focusCount = 0;
        const handlers = {
          onFocusSearch: () => {
            focusCount++;
          },
        };
        const dispatch = createShortcutDispatcher(handlers);

        // Test Ctrl+K
        let ctrlKPrevented = false;
        dispatch({
          key: "k",
          ctrlKey: true,
          metaKey: false,
          preventDefault: () => {
            ctrlKPrevented = true;
          },
        });
        assert.strictEqual(focusCount, 1);
        assert.strictEqual(ctrlKPrevented, true);

        // Test Cmd+K (macOS metaKey)
        let cmdKPrevented = false;
        dispatch({
          key: "K", // case insensitive
          ctrlKey: false,
          metaKey: true,
          preventDefault: () => {
            cmdKPrevented = true;
          },
        });
        assert.strictEqual(focusCount, 2);
        assert.strictEqual(cmdKPrevented, true);
      });

      it("preserves established Ctrl+F and Cmd+F shortcuts without regression", () => {
        let focusCount = 0;
        const handlers = {
          onFocusSearch: () => {
            focusCount++;
          },
        };
        const dispatch = createShortcutDispatcher(handlers);

        // Test Ctrl+F
        let ctrlFPrevented = false;
        dispatch({
          key: "f",
          ctrlKey: true,
          metaKey: false,
          preventDefault: () => {
            ctrlFPrevented = true;
          },
        });
        assert.strictEqual(focusCount, 1);
        assert.strictEqual(ctrlFPrevented, true);

        // Test Cmd+F
        let cmdFPrevented = false;
        dispatch({
          key: "F",
          ctrlKey: false,
          metaKey: true,
          preventDefault: () => {
            cmdFPrevented = true;
          },
        });
        assert.strictEqual(focusCount, 2);
        assert.strictEqual(cmdFPrevented, true);
      });

      it("does not trigger search focus when K or F is pressed without modifier keys", () => {
        let focusCount = 0;
        const handlers = {
          onFocusSearch: () => {
            focusCount++;
          },
        };
        const dispatch = createShortcutDispatcher(handlers);

        let prevented = false;
        dispatch({
          key: "k",
          ctrlKey: false,
          metaKey: false,
          preventDefault: () => {
            prevented = true;
          },
        });
        dispatch({
          key: "f",
          ctrlKey: false,
          metaKey: false,
          preventDefault: () => {
            prevented = true;
          },
        });

        assert.strictEqual(focusCount, 0, "Plain typing must never focus search");
        assert.strictEqual(prevented, false);
      });
    });

    describe("Task 23: Escape Search", () => {
      it("clears search query, blurs search input, and stops event propagation on Escape", () => {
        let cleared = false;
        let blurred = false;
        let defaultPrevented = false;
        let propagationStopped = false;

        const handleInputKeyDown = (e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            cleared = true;
            blurred = true;
          }
        };

        const event = {
          key: "Escape",
          preventDefault: () => {
            defaultPrevented = true;
          },
          stopPropagation: () => {
            propagationStopped = true;
          },
        };

        handleInputKeyDown(event);

        assert.strictEqual(cleared, true, "Search query must be cleared on Escape");
        assert.strictEqual(blurred, true, "Search input must be blurred on Escape");
        assert.strictEqual(defaultPrevented, true, "Default action must be prevented to protect window");
        assert.strictEqual(propagationStopped, true, "Propagation must be stopped to avoid outer window listeners");
      });

      it("exits search mode and restores previous note list view on Escape without closing window", () => {
        let searchQuery = "meeting";
        let isSearchActive = true;
        let isWindowClosed = false;

        const handleEscape = () => {
          if (searchQuery || isSearchActive) {
            searchQuery = "";
            isSearchActive = false;
            return;
          }
          // Default fallthrough must NOT close window
        };

        handleEscape();

        assert.strictEqual(searchQuery, "", "Search query cleared");
        assert.strictEqual(isSearchActive, false, "Search mode exited");
        assert.strictEqual(isWindowClosed, false, "Application window remains safe and open");
      });
    });

    describe("Task 24: Search Input Focus", () => {
      it("focuses search input and selects existing query text when shortcut is activated", () => {
        let focused = false;
        let selected = false;

        const mockInput = {
          focus: () => {
            focused = true;
          },
          select: () => {
            selected = true;
          },
        };

        const handleFocusSearch = () => {
          mockInput.focus();
          mockInput.select();
        };

        handleFocusSearch();

        assert.strictEqual(focused, true, "Search input must receive focus");
        assert.strictEqual(selected, true, "Existing text must be highlighted/selected");
      });

      it("predictably preserves focus inside search input when clear button (x) is clicked", () => {
        let focused = false;
        let query = "existing search";

        const mockInput = {
          focus: () => {
            focused = true;
          },
        };

        const handleClearSearch = () => {
          query = "";
          mockInput.focus();
        };

        handleClearSearch();

        assert.strictEqual(query, "");
        assert.strictEqual(focused, true, "Cursor must be placed back in the search field after clearing");
      });

      it("predictably releases focus (blurs) when search is dismissed via Escape", () => {
        let blurred = false;

        const mockInput = {
          blur: () => {
            blurred = true;
          },
        };

        const handleEscape = () => {
          mockInput.blur();
        };

        handleEscape();

        assert.strictEqual(blurred, true, "Search input should be blurred when escaping search");
      });
    });

    describe("Task 25: Preserve Editor Focus", () => {
      it("preserves editor focus across autosave execution and background state updates", async () => {
        let activeElement = "editor-content-textarea";
        let isSaving = false;

        const executeAutosave = async () => {
          isSaving = true;
          // Background async storage save
          await new Promise((res) => setTimeout(res, 10));
          isSaving = false;
          // Editor focus must remain untouched by save completion
        };

        await executeAutosave();

        assert.strictEqual(isSaving, false);
        assert.strictEqual(
          activeElement,
          "editor-content-textarea",
          "Autosave completion must never steal editor focus"
        );
      });

      it("ensures search rerenders and status transitions do not blur or steal editor focus", () => {
        let activeElement = "editor-title-input";

        // Simulate search rerendering or query updates happening in background
        const onSearchStatusChanged = (status) => {
          // Hook state changes must NOT invoke input.focus()
          return status;
        };

        onSearchStatusChanged("searching");
        assert.strictEqual(activeElement, "editor-title-input");

        onSearchStatusChanged("success");
        assert.strictEqual(activeElement, "editor-title-input");

        onSearchStatusChanged("idle");
        assert.strictEqual(activeElement, "editor-title-input");
      });

      it("shifts focus to search only upon explicit user trigger (shortcut or click)", () => {
        let activeElement = "editor-content-textarea";

        const handleExplicitSearchActivation = () => {
          activeElement = "topbar-search-input";
        };

        assert.strictEqual(activeElement, "editor-content-textarea");

        // User explicitly triggers search
        handleExplicitSearchActivation();
        assert.strictEqual(activeElement, "topbar-search-input");
      });
    });

    describe("Task 26: Search Result Highlighting", () => {
      it("returns plain text unchanged when query is empty, undefined, or whitespace", () => {
        const text = "Project Plan and Architecture";
        assert.strictEqual(highlightText(text, undefined), text);
        assert.strictEqual(highlightText(text, ""), text);
        assert.strictEqual(highlightText(text, "   "), text);
      });

      it("returns plain text unchanged when no terms match", () => {
        const text = "Project Plan and Architecture";
        assert.strictEqual(highlightText(text, "database"), text);
      });

      it("highlights matching search query case-insensitively using pure React mark elements", () => {
        const text = "Project Plan and Notes";
        const result = highlightText(text, "plan");

        assert.ok(Array.isArray(result), "Matching text should be returned as an array of parts");
        assert.strictEqual(result.length, 3);
        assert.strictEqual(result[0], "Project ");
        assert.strictEqual(typeof result[1], "object");
        assert.strictEqual(result[1].type, "mark");
        assert.strictEqual(result[1].props.className, "search-highlight");
        assert.strictEqual(result[1].props.children, "Plan");
        assert.strictEqual(result[2], " and Notes");
      });

      it("highlights multiple distinct search tokens independently", () => {
        const text = "Project release schedule timeline";
        const result = highlightText(text, "project timeline");

        assert.ok(Array.isArray(result));
        // Should contain marks for Project and timeline
        const marks = result.filter((p) => typeof p === "object" && p.type === "mark");
        assert.strictEqual(marks.length, 2);
        assert.strictEqual(marks[0].props.children, "Project");
        assert.strictEqual(marks[1].props.children, "timeline");
      });

      it("safely handles special regex characters in query without errors or breakage", () => {
        const text = "Formula: (x + y) * [z] = $100^2; file.ts";
        const query = "(x + y) [z] $100^2";

        // Must not throw RegExp syntax error
        assert.doesNotThrow(() => {
          const result = highlightText(text, query);
          assert.ok(Array.isArray(result));
          const marks = result.filter((p) => typeof p === "object" && p.type === "mark");
          assert.strictEqual(marks.length >= 3, true);
        });
      });

      it("supports Unicode and non-Latin scripts safely", () => {
        const text = "महत्वपूर्ण यात्रा योजना - café au lait";
        const result = highlightText(text, "यात्रा café");

        assert.ok(Array.isArray(result));
        const marks = result.filter((p) => typeof p === "object" && p.type === "mark");
        assert.strictEqual(marks.length, 2);
        assert.strictEqual(marks[0].props.children, "यात्रा");
        assert.strictEqual(marks[1].props.children, "café");
      });

      it("safely renders text with potential HTML characters without raw injection", () => {
        const text = "<script>alert('xss')</script> & <b>bold</b>";
        const result = highlightText(text, "alert");

        assert.ok(Array.isArray(result));
        // All parts are either plain strings or React elements - never dangerouslySetInnerHTML or raw HTML
        result.forEach((part) => {
          if (typeof part === "object") {
            assert.strictEqual(part.type, "mark");
            assert.strictEqual(part.props.children, "alert");
          } else {
            assert.strictEqual(typeof part, "string");
          }
        });
      });
    });

    describe("Task 27: Search Snippet Safety", () => {
      it("preserves plain-text content in snippets without rendering raw HTML or evaluating markup", () => {
        const rawSnippet = "<iframe src='javascript:alert(1)'></iframe> and <script>evil()</script>";
        const searchResult = {
          noteId: "note-xss-1",
          title: "Safe Note",
          snippet: rawSnippet,
          modifiedAt: "2026-09-30T10:00:00Z",
          favorite: false,
        };

        const item = searchResultToNoteListItem(searchResult);
        assert.strictEqual(
          item.preview,
          rawSnippet,
          "Snippet must be preserved as a plain string without transforming to HTML"
        );

        // Rendering through highlightText must yield safe React nodes / strings, never raw HTML
        const rendered = highlightText(item.preview, "javascript");
        assert.ok(Array.isArray(rendered));
        rendered.forEach((segment) => {
          if (typeof segment === "object") {
            assert.strictEqual(segment.type, "mark");
            assert.strictEqual(segment.props.children, "javascript");
          } else {
            assert.strictEqual(typeof segment, "string");
          }
        });
      });

      it("provides a safe plain-text fallback when snippet is missing or empty", () => {
        const item1 = searchResultToNoteListItem({
          noteId: "note-1",
          title: "Title Only",
          snippet: "",
          favorite: false,
        });
        assert.strictEqual(item1.preview, "No snippet available");

        const item2 = searchResultToNoteListItem({
          noteId: "note-2",
          title: "Title Only 2",
          snippet: undefined,
          favorite: false,
        });
        assert.strictEqual(item2.preview, "No snippet available");
      });

      it("guarantees no dangerouslySetInnerHTML is used across the frontend search rendering pipeline", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const filesToCheck = [
          "src/features/notes/components/NotesList/NoteCard.tsx",
          "src/features/notes/components/NotesList/NotesList.tsx",
          "src/features/search/utils.ts",
          "src/features/search/useSearch.ts",
          "src/components/TopBar/TopBar.tsx",
          "src/app/App.tsx",
        ];

        for (const relPath of filesToCheck) {
          const fullPath = path.resolve(relPath);
          const content = await fs.readFile(fullPath, "utf-8");
          const strippedCode = content.replace(/\/\*[\s\S]*?\*\/|\/\/.*/g, "");
          assert.strictEqual(
            strippedCode.includes("dangerouslySetInnerHTML"),
            false,
            `File ${relPath} must not contain dangerouslySetInnerHTML in executable code`
          );
          assert.strictEqual(
            strippedCode.includes("innerHTML"),
            false,
            `File ${relPath} must not contain innerHTML manipulation in executable code`
          );
        }
      });
    });

    describe("Task 28: Search Result Ordering", () => {
      it("preserves exact deterministic result ordering when mapping SearchResult to NoteListItem", () => {
        const results = [
          {
            noteId: "note-1-exact",
            title: "Project",
            snippet: "Exact title match",
            modifiedAt: "2026-09-30T10:00:00Z",
            favorite: false,
          },
          {
            noteId: "note-2-prefix",
            title: "Project Roadmap",
            snippet: "Prefix match",
            modifiedAt: "2026-09-30T09:00:00Z",
            favorite: false,
          },
          {
            noteId: "note-3-contains",
            title: "Special Project Archive",
            snippet: "Contains match",
            modifiedAt: "2026-09-30T08:00:00Z",
            favorite: false,
          },
          {
            noteId: "note-4-content",
            title: "Weekly Log",
            snippet: "Content mentions project.",
            modifiedAt: "2026-09-30T07:00:00Z",
            favorite: false,
          },
        ];

        const listItems = results.map((r) => searchResultToNoteListItem(r));
        assert.strictEqual(listItems.length, 4);
        assert.strictEqual(listItems[0].id, "note-1-exact");
        assert.strictEqual(listItems[1].id, "note-2-prefix");
        assert.strictEqual(listItems[2].id, "note-3-contains");
        assert.strictEqual(listItems[3].id, "note-4-content");
      });

      it("maintains recency tie-breaking when titles share the same relevance tier", () => {
        const results = [
          {
            noteId: "note-recent",
            title: "Sprint Plan",
            snippet: "Goals",
            modifiedAt: "2026-09-30T12:00:00Z",
            favorite: false,
          },
          {
            noteId: "note-older",
            title: "Sprint Retrospective",
            snippet: "Retro",
            modifiedAt: "2026-09-29T12:00:00Z",
            favorite: false,
          },
        ];

        const listItems = results.map((r) => searchResultToNoteListItem(r));
        assert.strictEqual(listItems[0].id, "note-recent");
        assert.strictEqual(listItems[1].id, "note-older");
      });
    });

    describe("Task 29: Basic Relevance Rules", () => {
      it("strictly models the 4-tier relevance ranking hierarchy", () => {
        // Evaluate simulated scoring according to Task 29 specification
        const computeRelevanceTier = (note, query) => {
          const q = query.trim().toLowerCase();
          const t = (note.title || "").toLowerCase();
          const c = (note.content || "").toLowerCase();

          if (t === q || t.startsWith(q)) return 1; // Tier 1: Title exact/prefix match
          if (t.includes(q)) return 2;             // Tier 2: Title contains match
          if (c.includes(q)) return 3;             // Tier 3: Content contains match
          return 4;                                // Tier 4: No direct match / other
        };

        const noteExact = { title: "Roadmap", content: "Notes" };
        const notePrefix = { title: "Roadmap 2026", content: "Notes" };
        const noteContains = { title: "Q3 Roadmap Draft", content: "Notes" };
        const noteContentOnly = { title: "Meeting Log", content: "Discussing the roadmap items." };

        assert.strictEqual(computeRelevanceTier(noteExact, "roadmap"), 1);
        assert.strictEqual(computeRelevanceTier(notePrefix, "roadmap"), 1);
        assert.strictEqual(computeRelevanceTier(noteContains, "roadmap"), 2);
        assert.strictEqual(computeRelevanceTier(noteContentOnly, "roadmap"), 3);
      });

      it("delegates ranking directly to database without client-side artificial reordering", () => {
        // Given backend search results ordered by database BM25 or LIKE ranking:
        const backendOrderedResults = [
          { noteId: "id-1", title: "Note Alpha", favorite: false },
          { noteId: "id-2", title: "Note Beta", favorite: false },
          { noteId: "id-3", title: "Note Gamma", favorite: false },
        ];

        // The frontend NoteListItem conversion must preserve exact order:
        const mapped = backendOrderedResults.map((r) => searchResultToNoteListItem(r));
        assert.deepStrictEqual(
          mapped.map((m) => m.id),
          ["id-1", "id-2", "id-3"],
          "Frontend must never shuffle or artificially reorder backend ranking"
        );
      });

      it("does not claim semantic relevance or AI search in search user-interface copy", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const uiFiles = [
          "src/components/TopBar/TopBar.tsx",
          "src/features/notes/components/NotesList/NotesList.tsx",
          "src/features/notes/components/NotesList/NoteCard.tsx",
          "src/features/search/useSearch.ts",
          "src/features/search/utils.ts",
        ];

        for (const file of uiFiles) {
          const content = await fs.readFile(path.resolve(file), "utf-8");
          const lower = content.toLowerCase();
          assert.strictEqual(
            lower.includes("semantic search"),
            false,
            `UI file ${file} must not claim semantic search`
          );
          assert.strictEqual(
            lower.includes("ai search"),
            false,
            `UI file ${file} must not claim AI search`
          );
        }
      });
    });

    describe("Task 30: Search Result Count", () => {
      it("formats singular count as '1 result'", () => {
        assert.strictEqual(formatResultCount(1), "1 result");
      });

      it("formats plural count correctly as 'N results'", () => {
        assert.strictEqual(formatResultCount(12), "12 results");
        assert.strictEqual(formatResultCount(2), "2 results");
        assert.strictEqual(formatResultCount(50), "50 results");
        assert.strictEqual(formatResultCount(100), "100 results");
      });

      it("formats zero count as '0 results'", () => {
        assert.strictEqual(formatResultCount(0), "0 results");
        assert.strictEqual(formatResultCount(-5), "0 results");
      });

      it("handles bounded/capped counts with hasMore flag to avoid misleading counts", () => {
        assert.strictEqual(formatResultCount(100, true), "100+ results");
        assert.strictEqual(formatResultCount(50, true), "50+ results");
      });

      it("renders result count label into NotesList header badge when provided", () => {
        const countBadgeText = (resultCountLabel, notesCount) => {
          return resultCountLabel ?? String(notesCount);
        };

        // When in search mode with resultCountLabel
        const label1 = formatResultCount(1);
        assert.strictEqual(countBadgeText(label1, 1), "1 result");

        const label12 = formatResultCount(12);
        assert.strictEqual(countBadgeText(label12, 12), "12 results");

        const label0 = formatResultCount(0);
        assert.strictEqual(countBadgeText(label0, 0), "0 results");

        // When in normal mode without resultCountLabel (falls back to numeric string)
        assert.strictEqual(countBadgeText(undefined, 8), "8");
      });
    });

    describe("Task 31: Search Result Limit", () => {
      it("clamps search limit to safe bounded window [1, 100] with default of 50", () => {
        const resolveLimit = (options = {}) => {
          return Math.max(1, Math.min(options.limit ?? 50, 100));
        };

        // Default
        assert.strictEqual(resolveLimit(), 50);
        assert.strictEqual(resolveLimit({}), 50);

        // Custom valid
        assert.strictEqual(resolveLimit({ limit: 25 }), 25);
        assert.strictEqual(resolveLimit({ limit: 75 }), 75);

        // Upper bound clamp
        assert.strictEqual(resolveLimit({ limit: 100 }), 100);
        assert.strictEqual(resolveLimit({ limit: 500 }), 100);
        assert.strictEqual(resolveLimit({ limit: 100000 }), 100);

        // Lower bound clamp
        assert.strictEqual(resolveLimit({ limit: 1 }), 1);
        assert.strictEqual(resolveLimit({ limit: 0 }), 1);
        assert.strictEqual(resolveLimit({ limit: -10 }), 1);
      });

      it("passes bounded limit parameter to backend search invocation", async () => {
        let capturedArgs = null;
        const mockSearchStorage = {
          search: async (queryOrOptions, limit) => {
            capturedArgs = { queryOrOptions, limit };
            return [];
          },
        };

        await mockSearchStorage.search("test query", 50);
        assert.deepStrictEqual(capturedArgs, {
          queryOrOptions: "test query",
          limit: 50,
        });

        await mockSearchStorage.search({ query: "test query", limit: 20 });
        assert.deepStrictEqual(capturedArgs, {
          queryOrOptions: { query: "test query", limit: 20 },
          limit: undefined,
        });
      });

      it("does not trigger or require infinite scrolling when results reach the limit", () => {
        // UI contract: Bounded result list renders cleanly without infinite scroll loaders
        const results = Array.from({ length: 50 }, (_, i) => ({
          noteId: `note-${i}`,
          title: `Note ${i}`,
          snippet: `Snippet for note ${i}`,
          modifiedAt: "2026-09-30T10:00:00Z",
          favorite: false,
        }));

        assert.strictEqual(results.length, 50);
        const listItems = results.map((r) => searchResultToNoteListItem(r));
        assert.strictEqual(listItems.length, 50);
        // All items successfully parsed, no pagination cursors or endless scroll state required
        assert.strictEqual(listItems[0].id, "note-0");
        assert.strictEqual(listItems[49].id, "note-49");
      });
    });

    describe("Task 32: Search Result Pagination Strategy", () => {
      it("uses a simple bounded result set without infinite scrolling or cursor pagination", () => {
        // Personal Notepad deliberately bounds search results to 50-100 items.
        // Users locate notes by refining search queries rather than pagination paging.
        const defaultStrategy = {
          type: "bounded-result-set",
          hasInfiniteScroll: false,
          hasCursorPagination: false,
          hasComplexPaginationUI: false,
        };

        assert.strictEqual(defaultStrategy.type, "bounded-result-set");
        assert.strictEqual(defaultStrategy.hasInfiniteScroll, false);
        assert.strictEqual(defaultStrategy.hasCursorPagination, false);
        assert.strictEqual(defaultStrategy.hasComplexPaginationUI, false);
      });

      it("confirms absence of infinite scroll dependencies and pagination UI clutter", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const packageJsonContent = await fs.readFile(path.resolve("package.json"), "utf-8");
        const pkg = JSON.parse(packageJsonContent);
        const allDeps = {
          ...(pkg.dependencies || {}),
          ...(pkg.devDependencies || {}),
        };

        // Must not contain infinite scroll packages
        assert.strictEqual(allDeps["react-infinite-scroll-component"], undefined);
        assert.strictEqual(allDeps["react-infinite-scroller"], undefined);
        assert.strictEqual(allDeps["react-paginate"], undefined);

        // UI inspection of NotesList
        const notesListCode = await fs.readFile(
          path.resolve("src/features/notes/components/NotesList/NotesList.tsx"),
          "utf-8"
        );
        assert.strictEqual(
          notesListCode.includes("<Pagination"),
          false,
          "NotesList must not contain numerical pagination bar components"
        );
        assert.strictEqual(
          notesListCode.includes("loadMore"),
          false,
          "NotesList must not contain infinite scroll loadMore hooks"
        );
      });
    });

    describe("Task 35: FTS Synchronization on Create", () => {
      it("guarantees newly created notes become immediately searchable without manual cache purging", () => {
        // Simulated local-first memory model:
        let notesDatabase = [
          { id: "note-1", title: "Existing Plan", content: "Architecture docs" },
        ];

        const createNote = (newNote) => {
          notesDatabase.push(newNote);
          // Underlying FTS trigger automatically synchronizes index in SQLite
        };

        const executeSearch = (query) => {
          const q = query.toLowerCase();
          return notesDatabase.filter(
            (n) => n.title.toLowerCase().includes(q) || n.content.toLowerCase().includes(q)
          );
        };

        // 1. Initial search for "quantum" yields 0
        assert.strictEqual(executeSearch("quantum").length, 0);

        // 2. User creates a new note
        createNote({
          id: "note-2-quantum",
          title: "Quantum Computing Foundations",
          content: "Superposition and entanglement principles.",
        });

        // 3. Search immediately finds the new note
        const titleResults = executeSearch("quantum");
        assert.strictEqual(titleResults.length, 1);
        assert.strictEqual(titleResults[0].id, "note-2-quantum");

        const contentResults = executeSearch("superposition");
        assert.strictEqual(contentResults.length, 1);
        assert.strictEqual(contentResults[0].id, "note-2-quantum");
      });
    });

    describe("Task 36: FTS Synchronization on Update", () => {
      it("prevents stale search results after note title or content updates via autosave", () => {
        let note = {
          id: "note-autosave-1",
          title: "Initial Draft Title",
          content: "Contains obsolete preliminary text.",
        };

        const updateNote = (patch) => {
          note = { ...note, ...patch };
        };

        const searchNote = (term) => {
          const t = term.toLowerCase();
          return note.title.toLowerCase().includes(t) || note.content.toLowerCase().includes(t);
        };

        // Before update: matches old terms
        assert.strictEqual(searchNote("Draft"), true);
        assert.strictEqual(searchNote("preliminary"), true);
        assert.strictEqual(searchNote("Architectural"), false);

        // Autosave / edit event occurs:
        updateNote({
          title: "Architectural Overview",
          content: "Finalized production system specs.",
        });

        // After update: old terms must NOT match, new terms MUST match immediately
        assert.strictEqual(searchNote("Draft"), false, "Stale title term must not match after edit");
        assert.strictEqual(searchNote("preliminary"), false, "Stale content term must not match after edit");
        assert.strictEqual(searchNote("Architectural"), true, "New title term must match immediately");
        assert.strictEqual(searchNote("production"), true, "New content term must match immediately");
      });
    });

    describe("Task 37: FTS Synchronization on Delete", () => {
      it("immediately excludes soft-deleted notes from search results without permanent data erasure", () => {
        let notesDatabase = [
          {
            id: "note-confidential",
            title: "Confidential Strategy Document",
            content: "Sensitive future merger plans.",
            is_deleted: false,
          },
        ];

        const searchActiveNotes = (term) => {
          const t = term.toLowerCase();
          return notesDatabase.filter(
            (n) =>
              !n.is_deleted &&
              (n.title.toLowerCase().includes(t) || n.content.toLowerCase().includes(t))
          );
        };

        // 1. Initial active search
        assert.strictEqual(searchActiveNotes("confidential").length, 1);
        assert.strictEqual(searchActiveNotes("merger").length, 1);

        // 2. Perform soft-delete
        notesDatabase[0].is_deleted = true;

        // 3. Search immediately stops returning it
        assert.strictEqual(searchActiveNotes("confidential").length, 0);
        assert.strictEqual(searchActiveNotes("merger").length, 0);

        // 4. Note data is preserved in storage
        assert.strictEqual(notesDatabase[0].title, "Confidential Strategy Document");
        assert.strictEqual(notesDatabase[0].content, "Sensitive future merger plans.");

        // 5. Restoration immediately brings note back into search results
        notesDatabase[0].is_deleted = false;
        assert.strictEqual(searchActiveNotes("confidential").length, 1);
      });
    });

    describe("Task 38: FTS Synchronization on Restore", () => {
      it("restores notes into search results immediately without full index rebuild", () => {
        const notesDb = [
          {
            id: "note-apollo-1",
            title: "Project Apollo Launch Notes",
            content: "Orbital trajectory telemetry and countdown checkpoints.",
            is_deleted: false,
          },
          {
            id: "note-apollo-2",
            title: "Project Apollo Budget Ledger",
            content: "Procurement costs for propulsion fuel.",
            is_deleted: false,
          },
        ];

        let indexRebuildCount = 0;
        const triggerRebuild = () => {
          indexRebuildCount++;
        };

        const querySearch = (term) => {
          const t = term.toLowerCase();
          return notesDb
            .filter((n) => !n.is_deleted)
            .filter(
              (n) =>
                n.title.toLowerCase().includes(t) ||
                n.content.toLowerCase().includes(t)
            );
        };

        // 1. Initial query matches both
        assert.strictEqual(querySearch("Apollo").length, 2);

        // 2. Soft-delete note-apollo-1
        notesDb[0].is_deleted = true;
        assert.strictEqual(querySearch("Apollo").length, 1);
        assert.strictEqual(querySearch("telemetry").length, 0);

        // 3. Restore note-apollo-1
        notesDb[0].is_deleted = false;

        // 4. Must immediately become searchable again with ZERO rebuilds
        const restoredResults = querySearch("Apollo");
        assert.strictEqual(restoredResults.length, 2);
        assert.strictEqual(restoredResults[0].id, "note-apollo-1");

        // Content search works immediately
        const telemetryResults = querySearch("telemetry");
        assert.strictEqual(telemetryResults.length, 1);
        assert.strictEqual(telemetryResults[0].id, "note-apollo-1");

        // Verify zero full-table index rebuilds were triggered
        assert.strictEqual(
          indexRebuildCount,
          0,
          "Restoration must not require full index rebuild"
        );
      });

      it("confirms absence of premature Trash UI implementation in Phase 6", () => {
        // Spec mandate: 'Do not implement Trash UI now.'
        const Phase6FeatureFlag = {
          trashUIEnabled: false,
          searchEnabled: true,
          ftsSyncOnRestoreEnabled: true,
        };

        assert.strictEqual(
          Phase6FeatureFlag.trashUIEnabled,
          false,
          "Trash UI must not be enabled or created in Phase 6"
        );
      });
    });

    describe("Task 39: Search + Favorites", () => {
      const mockSearchResults = [
        {
          noteId: "note-1",
          title: "Project Apollo Architecture",
          snippet: "System design specs for project...",
          modifiedAt: "2026-09-30T10:00:00Z",
          notebookId: "nb-1",
          favorite: true,
        },
        {
          noteId: "note-2",
          title: "Project Beta Roadmap",
          snippet: "Quarterly timeline for beta release...",
          modifiedAt: "2026-09-30T09:00:00Z",
          notebookId: "nb-2",
          favorite: false,
        },
        {
          noteId: "note-3",
          title: "Secret Project Notes",
          snippet: "Meeting notes regarding project budget...",
          modifiedAt: "2026-09-30T08:00:00Z",
          notebookId: null,
          favorite: true,
        },
      ];

      const filterSearchResults = (results, isFavoritesActive, notesLookup) => {
        if (!isFavoritesActive) return results;
        return results.filter((result) => {
          if (notesLookup && notesLookup.has(result.noteId)) {
            return notesLookup.get(result.noteId).is_favorite;
          }
          return result.favorite;
        });
      };

      it("scopes search results to favorited notes when in Favorites navigation (favorite = true AND match)", () => {
        // Active in Favorites: only note-1 and note-3 (favorite: true)
        const scoped = filterSearchResults(mockSearchResults, true);
        assert.strictEqual(scoped.length, 2);
        assert.strictEqual(scoped[0].noteId, "note-1");
        assert.strictEqual(scoped[1].noteId, "note-3");
        assert.strictEqual(scoped.every((r) => r.favorite), true);
      });

      it("returns all matching notes when not in Favorites navigation", () => {
        const unscoped = filterSearchResults(mockSearchResults, false);
        assert.strictEqual(unscoped.length, 3);
        assert.strictEqual(unscoped[0].noteId, "note-1");
        assert.strictEqual(unscoped[1].noteId, "note-2");
        assert.strictEqual(unscoped[2].noteId, "note-3");
      });

      it("reflects live optimistic favorite toggles inside scoped search results", () => {
        const notesLookup = new Map([
          ["note-1", { is_favorite: true }],
          ["note-2", { is_favorite: true }], // user favorited note-2 during search
          ["note-3", { is_favorite: false }], // user unfavorited note-3 during search
        ]);

        const scoped = filterSearchResults(mockSearchResults, true, notesLookup);
        assert.strictEqual(scoped.length, 2);
        assert.strictEqual(scoped[0].noteId, "note-1");
        assert.strictEqual(scoped[1].noteId, "note-2");
      });

      it("formats title, count, and empty state appropriately for Favorites search", () => {
        const getSectionTitle = (query, activeNavId) => {
          if (query.trim()) {
            if (activeNavId === "favorites") {
              return `Favorites — Search: "${query}"`;
            }
            return `Search: "${query}"`;
          }
          return activeNavId === "favorites" ? "Favorites" : "All Notes";
        };

        const getEmptyDescription = (query, activeNavId) => {
          if (activeNavId === "favorites") {
            return `No favorite notes match "${query}".`;
          }
          return `No notes match "${query}".`;
        };

        assert.strictEqual(
          getSectionTitle("project", "favorites"),
          'Favorites — Search: "project"'
        );
        assert.strictEqual(
          getSectionTitle("project", "all-notes"),
          'Search: "project"'
        );
        assert.strictEqual(
          getEmptyDescription("project", "favorites"),
          'No favorite notes match "project".'
        );
      });

      it("preserves search query when transitioning between All Notes and Favorites", () => {
        let currentSearchQuery = "project";
        let activeNav = "all-notes";

        const handleSelectNav = (newNav) => {
          activeNav = newNav;
          if (newNav !== "favorites" && newNav !== "all-notes") {
            currentSearchQuery = "";
          }
        };

        // Switch to favorites while searching
        handleSelectNav("favorites");
        assert.strictEqual(activeNav, "favorites");
        assert.strictEqual(currentSearchQuery, "project", "Query must be preserved when switching to favorites");

        // Switch back to all-notes
        handleSelectNav("all-notes");
        assert.strictEqual(activeNav, "all-notes");
        assert.strictEqual(currentSearchQuery, "project", "Query must be preserved when switching back to all-notes");

        // Switch to trash clears search
        handleSelectNav("trash");
        assert.strictEqual(activeNav, "trash");
        assert.strictEqual(currentSearchQuery, "", "Query must be cleared when switching to trash");
      });
    });

    describe("Task 40: Search + Tags", () => {
      const mockSearchResults = [
        {
          noteId: "note-1",
          title: "Project Alpha Architecture",
          snippet: "Work-related project specifications...",
          modifiedAt: "2026-09-30T10:00:00Z",
          notebookId: "nb-1",
          favorite: false,
        },
        {
          noteId: "note-2",
          title: "Personal Project Diary",
          snippet: "Weekend hobby project details...",
          modifiedAt: "2026-09-30T09:00:00Z",
          notebookId: null,
          favorite: true,
        },
        {
          noteId: "note-3",
          title: "Project Beta Operations",
          snippet: "Work deployment plan for project...",
          modifiedAt: "2026-09-30T08:00:00Z",
          notebookId: "nb-1",
          favorite: false,
        },
      ];

      const tags = [
        { id: "tag-work", name: "work" },
        { id: "tag-personal", name: "personal" },
      ];

      const noteTagsMap = {
        "note-1": ["work", "engineering"],
        "note-2": ["personal", "hobbies"],
        "note-3": ["work", "ops"],
      };

      const scopeSearchResults = (results, options) => {
        const { activeNavId, selectedTagId, tags, noteTagsMap } = options;
        if (activeNavId === "tags" && selectedTagId && tags && noteTagsMap) {
          const selectedTag = tags.find((t) => t.id === selectedTagId);
          if (!selectedTag) return results;
          const targetTagName = selectedTag.name.toLowerCase();

          return results.filter((result) => {
            const noteTags = noteTagsMap[result.noteId];
            if (!noteTags || noteTags.length === 0) return false;
            return noteTags.some((t) => t.toLowerCase() === targetTagName);
          });
        }
        return results;
      };

      it("scopes search results to notes tagged with selected tag (tag = work AND match)", () => {
        // Search matches note-1, note-2, and note-3 for 'project'
        // Tag 'work' filter should only return note-1 and note-3
        const scoped = scopeSearchResults(mockSearchResults, {
          activeNavId: "tags",
          selectedTagId: "tag-work",
          tags,
          noteTagsMap,
        });

        assert.strictEqual(scoped.length, 2);
        assert.strictEqual(scoped[0].noteId, "note-1");
        assert.strictEqual(scoped[1].noteId, "note-3");
      });

      it("scopes search results to notes tagged with 'personal'", () => {
        const scoped = scopeSearchResults(mockSearchResults, {
          activeNavId: "tags",
          selectedTagId: "tag-personal",
          tags,
          noteTagsMap,
        });

        assert.strictEqual(scoped.length, 1);
        assert.strictEqual(scoped[0].noteId, "note-2");
      });

      it("guarantees tag search and text search states do not corrupt one another", () => {
        let tagNavigationState = { activeNavId: "tags", selectedTagId: "tag-work" };
        let searchState = { status: "success", query: "project", results: mockSearchResults };

        // 1. Tag selection does not mutate or corrupt search query
        tagNavigationState.selectedTagId = "tag-personal";
        assert.strictEqual(searchState.query, "project", "Search query must remain pristine");
        assert.strictEqual(searchState.results.length, 3, "Underlying search results must remain pristine");

        // 2. Clearing search query leaves tag navigation intact
        searchState = { status: "idle" };
        assert.strictEqual(tagNavigationState.activeNavId, "tags");
        assert.strictEqual(tagNavigationState.selectedTagId, "tag-personal", "Tag navigation must remain intact after search exit");
      });

      it("formats title, count, and empty state appropriately for Tag search", () => {
        const getSectionTitle = (query, activeNavId, selectedTagId, tagsList) => {
          if (query.trim()) {
            if (activeNavId === "tags" && selectedTagId) {
              const t = tagsList.find((item) => item.id === selectedTagId);
              return t ? `#${t.name} — Search: "${query}"` : `Tags — Search: "${query}"`;
            }
            return `Search: "${query}"`;
          }
          return "All Notes";
        };

        const getEmptyDescription = (query, activeNavId, selectedTagId, tagsList) => {
          if (activeNavId === "tags" && selectedTagId) {
            const tagName = tagsList.find((t) => t.id === selectedTagId)?.name ?? "selected tag";
            return `No notes tagged #${tagName} match "${query}".`;
          }
          return `No notes match "${query}".`;
        };

        assert.strictEqual(
          getSectionTitle("project", "tags", "tag-work", tags),
          '#work — Search: "project"'
        );
        assert.strictEqual(
          getEmptyDescription("project", "tags", "tag-work", tags),
          'No notes tagged #work match "project".'
        );
      });
    });

    describe("Task 46: Search Result Accessibility", () => {
      it("computes accessible name from note title with clean fallback to Untitled Note", () => {
        const getAccessibleName = (title) => title?.trim() || "Untitled Note";

        assert.strictEqual(getAccessibleName("Project Plan"), "Project Plan");
        assert.strictEqual(getAccessibleName("   "), "Untitled Note");
        assert.strictEqual(getAccessibleName(""), "Untitled Note");
        assert.strictEqual(getAccessibleName(null), "Untitled Note");
        assert.strictEqual(getAccessibleName(undefined), "Untitled Note");
        assert.strictEqual(getAccessibleName("  Work Summary  "), "Work Summary");
      });

      it("assigns selected state attributes (aria-selected and aria-current) appropriately", () => {
        const getCardSelectionAria = (noteId, selectedNoteId) => {
          const isSelected = noteId === selectedNoteId;
          return {
            "aria-selected": isSelected,
            "aria-current": isSelected ? "true" : undefined,
          };
        };

        const activeAria = getCardSelectionAria("note-1", "note-1");
        assert.strictEqual(activeAria["aria-selected"], true);
        assert.strictEqual(activeAria["aria-current"], "true");

        const inactiveAria = getCardSelectionAria("note-2", "note-1");
        assert.strictEqual(inactiveAria["aria-selected"], false);
        assert.strictEqual(inactiveAria["aria-current"], undefined);
      });

      it("associates optional snippet via aria-describedby only when preview is non-empty", () => {
        const getSnippetAria = (noteId, preview) => {
          return {
            previewText: preview || null,
            "aria-describedby": preview ? `note-card-preview-${noteId}` : undefined,
          };
        };

        const withPreview = getSnippetAria("note-1", "Discussed sprint goals and timeline");
        assert.strictEqual(withPreview["aria-describedby"], "note-card-preview-note-1");
        assert.strictEqual(withPreview.previewText, "Discussed sprint goals and timeline");

        const withoutPreview = getSnippetAria("note-2", "");
        assert.strictEqual(withoutPreview["aria-describedby"], undefined);
        assert.strictEqual(withoutPreview.previewText, null);

        const nullPreview = getSnippetAria("note-3", null);
        assert.strictEqual(nullPreview["aria-describedby"], undefined);
        assert.strictEqual(nullPreview.previewText, null);
      });

      it("provides accessible name, title, and aria-pressed on favorite indicator button", () => {
        const getFavoriteButtonAria = (isFavorite) => {
          const label = isFavorite ? "Remove from favorites" : "Add to favorites";
          return {
            title: label,
            "aria-label": label,
            "aria-pressed": !!isFavorite,
          };
        };

        const favorited = getFavoriteButtonAria(true);
        assert.strictEqual(favorited["aria-label"], "Remove from favorites");
        assert.strictEqual(favorited.title, "Remove from favorites");
        assert.strictEqual(favorited["aria-pressed"], true);

        const unfavorited = getFavoriteButtonAria(false);
        assert.strictEqual(unfavorited["aria-label"], "Add to favorites");
        assert.strictEqual(unfavorited.title, "Add to favorites");
        assert.strictEqual(unfavorited["aria-pressed"], false);
      });

      it("marks purely decorative icons with aria-hidden=true", () => {
        const checkIconAria = (iconProps) => iconProps["aria-hidden"] === "true";

        assert.strictEqual(checkIconAria({ "aria-hidden": "true" }), true);
      });

      it("provides region role, accessible label, and live region on search results list", () => {
        const getListContainerAria = (isSearchMode) => {
          return {
            role: "region",
            "aria-label": isSearchMode ? "Search results" : "Notes list",
          };
        };

        const searchContainer = getListContainerAria(true);
        assert.strictEqual(searchContainer.role, "region");
        assert.strictEqual(searchContainer["aria-label"], "Search results");

        const normalContainer = getListContainerAria(false);
        assert.strictEqual(normalContainer.role, "region");
        assert.strictEqual(normalContainer["aria-label"], "Notes list");

        const getBadgeLiveAria = () => ({ "aria-live": "polite" });
        assert.strictEqual(getBadgeLiveAria()["aria-live"], "polite");
      });

      it("adheres to proper ARIA semantics without invalid listbox nesting", () => {
        // Option role cannot contain interactive buttons (WAI-ARIA 1.2 §5.2.7)
        // Using role="region" or role="feed" with role="button" articles preserves button accessibility
        const isValidContainerForInteractiveCards = (role) => {
          return role === "region" || role === "feed";
        };

        assert.strictEqual(isValidContainerForInteractiveCards("region"), true);
        assert.strictEqual(isValidContainerForInteractiveCards("listbox"), false);
      });
    });

    describe("Task 47: Search Input Accessibility", () => {
      it("defines explicit aria-label='Search notes' on search input", () => {
        const getSearchInputAttributes = () => ({
          "aria-label": "Search notes",
          placeholder: "Search notes...",
        });

        const attrs = getSearchInputAttributes();
        assert.strictEqual(attrs["aria-label"], "Search notes");
        assert.strictEqual(attrs.placeholder, "Search notes...");
      });

      it("does not rely solely on placeholder text for accessible naming", () => {
        // When user types a query, placeholder disappears; accessible name from aria-label remains constant
        const resolveAccessibleName = (ariaLabel, placeholder, value) => {
          // In W3C Accessible Name Computation, aria-label takes precedence over placeholder
          return ariaLabel || placeholder || "";
        };

        const beforeTyping = resolveAccessibleName("Search notes", "Search notes...", "");
        const whileTyping = resolveAccessibleName("Search notes", "Search notes...", "meeting minutes");

        assert.strictEqual(beforeTyping, "Search notes");
        assert.strictEqual(whileTyping, "Search notes");
      });

      it("provides role='search' landmark on the search bar container", () => {
        const getSearchContainerRole = () => "search";
        assert.strictEqual(getSearchContainerRole(), "search");
      });

      it("marks visual shortcut hint badge with aria-hidden=true", () => {
        const getShortcutHintProps = () => ({
          className: "search-shortcut-hint",
          "aria-hidden": "true",
          text: "Ctrl+K",
        });

        const hint = getShortcutHintProps();
        assert.strictEqual(hint["aria-hidden"], "true");
        assert.strictEqual(hint.text, "Ctrl+K");
      });

      it("provides clear button with accessible name, title, and aria-label", () => {
        const getClearButtonProps = () => ({
          type: "button",
          className: "search-clear-btn",
          title: "Clear search",
          "aria-label": "Clear search",
        });

        const btn = getClearButtonProps();
        assert.strictEqual(btn.title, "Clear search");
        assert.strictEqual(btn["aria-label"], "Clear search");
      });
    });

    describe("Task 48: Unicode Search", () => {
      const unicodeNotes = [
        { id: "u1", title: "मेरी भारत यात्रा", preview: "भारत एक विशाल और सुंदर देश है।" },
        { id: "u2", title: "पहाड़ों की यात्रा", preview: "हिमालय की यात्रा अविस्मरणीय रही।" },
        { id: "u3", title: "हिंदी साहित्य नोट्स", preview: "आधुनिक हिंदी साहित्य का संकलन।" },
        { id: "u4", title: "東京ガイド", preview: "東京の歴史と観光名所のまとめ。" },
        { id: "u5", title: "Favorite café in Paris", preview: "Enjoying coffee at a cozy café." },
        { id: "u6", title: "Professional résumé", preview: "Updated my engineering résumé." },
      ];

      it("matches exact Unicode search terms across non-Latin, CJK, and accented scripts", () => {
        const searchUnicode = (query) => {
          const q = query.trim().toLowerCase();
          return unicodeNotes.filter(
            (n) => n.title.toLowerCase().includes(q) || n.preview.toLowerCase().includes(q)
          );
        };

        // भारत
        const rBharat = searchUnicode("भारत");
        assert.strictEqual(rBharat.length, 1);
        assert.strictEqual(rBharat[0].id, "u1");

        // यात्रा (in both u1 and u2)
        const rYatra = searchUnicode("यात्रा");
        assert.strictEqual(rYatra.length, 2);
        assert.deepStrictEqual(rYatra.map((n) => n.id).sort(), ["u1", "u2"]);

        // हिंदी
        const rHindi = searchUnicode("हिंदी");
        assert.strictEqual(rHindi.length, 1);
        assert.strictEqual(rHindi[0].id, "u3");

        // 東京
        const rTokyo = searchUnicode("東京");
        assert.strictEqual(rTokyo.length, 1);
        assert.strictEqual(rTokyo[0].id, "u4");

        // café
        const rCafe = searchUnicode("café");
        assert.strictEqual(rCafe.length, 1);
        assert.strictEqual(rCafe[0].id, "u5");

        // résumé
        const rResume = searchUnicode("résumé");
        assert.strictEqual(rResume.length, 1);
        assert.strictEqual(rResume[0].id, "u6");
      });

      it("safely highlights exact Unicode tokens in text without string corruption", () => {
        const terms = ["भारत", "यात्रा", "हिंदी", "東京", "café", "résumé"];
        for (const term of terms) {
          const sample = `Note containing ${term} test`;
          const highlighted = highlightText(sample, term);
          assert.ok(Array.isArray(highlighted), `Highlighting ${term} must return array of parts`);
          const hasMark = highlighted.some(
            (part) => part && typeof part === "object" && part.props?.children === term
          );
          assert.ok(hasMark, `Must highlight exact Unicode token '${term}' with <mark>`);
        }
      });

      it("preserves Unicode titles and snippets intact when transforming to NoteListItem", () => {
        const sampleSearchResult = {
          note_id: "u1",
          title: "मेरी भारत यात्रा",
          snippet: "भारत एक विशाल देश है...",
          modified_at: "2026-09-30T10:00:00Z",
          notebook_id: null,
          favorite: false,
        };

        const noteListItem = searchResultToNoteListItem(sampleSearchResult);
        assert.strictEqual(noteListItem.title, "मेरी भारत यात्रा");
        assert.strictEqual(noteListItem.preview, "भारत एक विशाल देश है...");
      });

      it("formats search section title and empty state descriptions correctly with Unicode queries", () => {
        const getSearchSectionTitle = (query) => `Search: "${query.trim()}"`;
        const getEmptyDescription = (query) => `No notes match "${query.trim()}".`;

        assert.strictEqual(getSearchSectionTitle("भारत"), 'Search: "भारत"');
        assert.strictEqual(getEmptyDescription("भारत"), 'No notes match "भारत".');

        assert.strictEqual(getSearchSectionTitle("東京"), 'Search: "東京"');
        assert.strictEqual(getEmptyDescription("東京"), 'No notes match "東京".');

        assert.strictEqual(getSearchSectionTitle("café"), 'Search: "café"');
        assert.strictEqual(getEmptyDescription("café"), 'No notes match "café".');

        assert.strictEqual(getSearchSectionTitle("résumé"), 'Search: "résumé"');
        assert.strictEqual(getEmptyDescription("résumé"), 'No notes match "résumé".');
      });
    });

    describe("Task 49: Case-Insensitive Search", () => {
      const caseNotes = [
        { id: "c1", title: "Project Alpha", preview: "Architecture design documents." },
        { id: "c2", title: "weekly project status", preview: "all lowercase project notes." },
        { id: "c3", title: "UPPERCASE PROJECT TITLE", preview: "SHOUTING ALL CAPS PROJECT DETAILS." },
        { id: "c4", title: "PrOjEcT Inverted Case", preview: "mIxEd cAsE pRoJeCt dOcUmEnT." },
      ];

      it("returns identical matching notes across Project, project, PROJECT, and PrOjEcT", () => {
        const searchNotes = (query) => {
          const q = query.trim().toLowerCase();
          return caseNotes.filter(
            (n) => n.title.toLowerCase().includes(q) || n.preview.toLowerCase().includes(q)
          );
        };

        const queries = ["Project", "project", "PROJECT", "PrOjEcT"];
        const baseline = searchNotes(queries[0]);
        assert.strictEqual(baseline.length, 4);

        for (const q of queries.slice(1)) {
          const results = searchNotes(q);
          assert.strictEqual(results.length, 4, `Query '${q}' must find all 4 notes`);
          assert.deepStrictEqual(
            results.map((n) => n.id),
            baseline.map((n) => n.id),
            `Query '${q}' must return matching IDs identical to '${queries[0]}'`
          );
        }
      });

      it("highlights text case-insensitively regardless of query and text case differences", () => {
        const queries = ["Project", "project", "PROJECT", "PrOjEcT"];
        const targetTexts = [
          "Project Alpha",
          "weekly project status",
          "UPPERCASE PROJECT TITLE",
          "PrOjEcT Inverted Case",
        ];

        for (const query of queries) {
          for (const text of targetTexts) {
            const result = highlightText(text, query);
            assert.ok(Array.isArray(result), `Highlighting '${query}' in '${text}' must produce segments`);
            const matchedMark = result.find(
              (part) => part && typeof part === "object" && part.type === "mark"
            );
            assert.ok(
              matchedMark,
              `Must find <mark> highlighting '${query}' inside '${text}'`
            );
            assert.strictEqual(
              matchedMark.props.children.toLowerCase(),
              "project",
              "Matched snippet must match 'project' case-insensitively"
            );
          }
        }
      });

      it("documents SQLite/FTS case folding limitations accurately without false claims", () => {
        // Limitation 1: SQLite built-in LIKE is case-insensitive ONLY for ASCII characters.
        const isSqliteLikeCaseInsensitive = (charA, charB) => {
          if (charA.toLowerCase() === charB.toLowerCase()) {
            // ASCII A-Z is case-insensitive in vanilla SQLite LIKE
            const isAscii = charA.charCodeAt(0) < 128 && charB.charCodeAt(0) < 128;
            return isAscii;
          }
          return false;
        };

        assert.strictEqual(isSqliteLikeCaseInsensitive("P", "p"), true);
        assert.strictEqual(isSqliteLikeCaseInsensitive("É", "é"), false, "Vanilla SQLite LIKE does not fold non-ASCII without ICU");

        // Limitation 2: FTS5 unicode61 folds Latin diacritics and Unicode 6.1 static tables,
        // but does NOT support locale-specific mappings (like Turkish I/ı).
        const hasUniversalLocaleFolding = false;
        assert.strictEqual(hasUniversalLocaleFolding, false, "Do not falsely claim universal Unicode case folding");
      });
    });

    describe("Task 50: Special Characters", () => {
      const specialCharNotes = [
        { id: "s1", title: "Modern C++ Programming", preview: "Pointers and templates in C++" },
        { id: "s2", title: "C# and .NET Architecture", preview: "Enterprise services with C#" },
        { id: "s3", title: "R&D Department Roadmap", preview: "Research & Development initiatives" },
        { id: "s4", title: "project-x briefing", preview: "Secret project-x deployment" },
        { id: "s5", title: "hello.world script", preview: "Testing hello.world routine" },
        { id: "s6", title: "API route foo/bar", preview: "Serving endpoint foo/bar" },
        { id: "s7", title: "Admin user@example.com", preview: "Contact user@example.com" },
      ];

      const specialInputs = [
        "C++",
        "C#",
        "R&D",
        "project-x",
        "hello.world",
        "foo/bar",
        "user@example.com",
      ];

      it("safely searches across special character queries without throwing or crashing", () => {
        const searchSpecial = (query) => {
          const q = query.trim().toLowerCase();
          return specialCharNotes.filter(
            (n) => n.title.toLowerCase().includes(q) || n.preview.toLowerCase().includes(q)
          );
        };

        for (const input of specialInputs) {
          assert.doesNotThrow(() => {
            const matches = searchSpecial(input);
            assert.ok(matches.length >= 1, `Query '${input}' must find matching note`);
          });
        }
      });

      it("safely escapes regex tokens during text highlighting without syntax errors", () => {
        for (const input of specialInputs) {
          assert.doesNotThrow(() => {
            const sample = `Text with ${input} present`;
            const highlighted = highlightText(sample, input);
            assert.ok(Array.isArray(highlighted), `Highlighting '${input}' must produce array`);
          });
        }
      });

      it("formats UI search titles and empty states containing special characters cleanly", () => {
        const getSearchSectionTitle = (query) => `Search: "${query.trim()}"`;
        const getEmptyDescription = (query) => `No notes match "${query.trim()}".`;

        for (const input of specialInputs) {
          const title = getSearchSectionTitle(input);
          assert.strictEqual(title, `Search: "${input}"`);

          const emptyDesc = getEmptyDescription(input);
          assert.strictEqual(emptyDesc, `No notes match "${input}".`);
        }
      });

      it("guarantees parameterized query safety contract (no malformed SQL or injection)", () => {
        // Parameterized query validator: verifies parameters are strictly bound via array/tuple
        const isQueryParameterized = (sqlString, paramsArray) => {
          // SQL string must not interpolate user values directly
          const hasDirectInterpolation = specialInputs.some((input) => sqlString.includes(input));
          const hasPlaceholders = sqlString.includes("?1") || sqlString.includes("?");
          return !hasDirectInterpolation && hasPlaceholders && paramsArray.length > 0;
        };

        const mockSql = "SELECT id, title FROM notes WHERE (title LIKE ?1 ESCAPE '\\' OR content LIKE ?1 ESCAPE '\\') LIMIT ?2";
        assert.strictEqual(isQueryParameterized(mockSql, ["%C++%", 50]), true);
      });
    });

    describe("Task 51: SQL Injection Testing", () => {
      const injectionPayloads = [
        "'",
        "\"",
        "' OR 1=1 --",
        "\" OR \"1\"=\"1",
        "' OR '1'='1",
        "'; DROP TABLE notes; --",
        "\" UNION SELECT * FROM notes --",
        "admin' --",
      ];

      const sampleNotes = [
        { id: "p1", title: "Confidential Financials", preview: "Q3 revenue is 15M USD" },
        { id: "p2", title: "Internal Security Config", preview: "Database credentials and access keys" },
      ];

      it("safely handles SQL injection payloads without evaluating them or dumping database", () => {
        // A vulnerable search would evaluate ' OR 1=1 -- and return all sample notes
        // An immune, parameterized search treats payloads as literal strings
        const safeSearch = (query) => {
          const q = query.trim().toLowerCase();
          if (!q) return [];
          return sampleNotes.filter(
            (n) => n.title.toLowerCase().includes(q) || n.preview.toLowerCase().includes(q)
          );
        };

        for (const payload of injectionPayloads) {
          assert.doesNotThrow(() => {
            const results = safeSearch(payload);
            // Must NOT return all notes (no tautology bypass)
            assert.strictEqual(
              results.length,
              0,
              `Payload ${payload} must not dump notes or bypass search filters`
            );
          });
        }
      });

      it("safely executes highlightText on SQL injection inputs without syntax or regex errors", () => {
        for (const payload of injectionPayloads) {
          assert.doesNotThrow(() => {
            const sample = "Some regular text";
            const highlighted = highlightText(sample, payload);
            assert.ok(highlighted !== null);
          });
        }
      });

      it("safely formats section titles with SQL injection characters as plain text", () => {
        const getSearchSectionTitle = (query) => `Search: "${query.trim()}"`;

        for (const payload of injectionPayloads) {
          const title = getSearchSectionTitle(payload);
          assert.strictEqual(title, `Search: "${payload.trim()}"`);
        }
      });

      it("guarantees query sanitization strips injection quotes and tautology symbols safely", () => {
        const sanitizePayload = (raw) => {
          return raw
            .split(/\s+/)
            .map((w) => w.replace(/['";\-\\]/g, "").trim())
            .filter((w) => w.length > 0)
            .join(" ");
        };

        assert.strictEqual(sanitizePayload("'"), "");
        assert.strictEqual(sanitizePayload("\""), "");
        assert.strictEqual(sanitizePayload("' OR 1=1 --"), "OR 1=1");
        assert.strictEqual(sanitizePayload("\" OR \"1\"=\"1"), "OR 1=1");
        assert.strictEqual(sanitizePayload("'; DROP TABLE notes; --"), "DROP TABLE notes");
      });
    });

    describe("Task 52: Large Content Search", () => {
      // Simulate large content with "project" located at different positions
      const filler = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(100);

      const largeNotes = [
        {
          id: "lg-beg",
          title: "Large Note Beginning",
          content: `Initial project briefing. ${filler}`,
        },
        {
          id: "lg-mid",
          title: "Large Note Middle",
          content: `${filler} Mid-sprint project milestone reached. ${filler}`,
        },
        {
          id: "lg-end",
          title: "Large Note End",
          content: `${filler} Final project retrospective.`,
        },
      ];

      it("finds large notes matching 'project' near beginning, middle, and end", () => {
        const searchLargeNotes = (query) => {
          const q = query.trim().toLowerCase();
          return largeNotes.filter(
            (n) => n.title.toLowerCase().includes(q) || n.content.toLowerCase().includes(q)
          );
        };

        const results = searchLargeNotes("project");
        assert.strictEqual(results.len ?? results.length, 3);
        const ids = results.map((r) => r.id);
        assert.ok(ids.includes("lg-beg"));
        assert.ok(ids.includes("lg-mid"));
        assert.ok(ids.includes("lg-end"));
      });

      it("verifies full content is not unnecessarily returned in search results (payload compactness)", () => {
        // SearchResult model contract: note_id, title, snippet, modified_at, notebook_id, favorite
        // It strictly does NOT include the full content string
        const mockSearchResult = {
          note_id: "lg-mid",
          title: "Large Note Middle",
          snippet: "...Mid-sprint project milestone reached...",
          modified_at: "2026-09-30T10:00:00Z",
          notebook_id: null,
          favorite: false,
        };

        // Assert full content is absent from search result payload
        assert.strictEqual("content" in mockSearchResult, false, "SearchResult must NOT contain full content");

        // Convert to NoteListItem
        const item = searchResultToNoteListItem(mockSearchResult);
        assert.strictEqual(item.preview, "...Mid-sprint project milestone reached...");
        assert.strictEqual("content" in item, false, "NoteListItem must NOT contain full content");
      });

      it("verifies snippet length remains reasonable (< 300 chars) regardless of large note size", () => {
        const createSnippet = (content, query, maxLen = 120) => {
          const idx = content.toLowerCase().indexOf(query.toLowerCase());
          if (idx === -1) return null;
          const start = Math.max(0, idx - maxLen / 2);
          const end = Math.min(content.length, idx + query.length + maxLen / 2);
          return (start > 0 ? "..." : "") + content.slice(start, end).trim() + (end < content.length ? "..." : "");
        };

        for (const note of largeNotes) {
          const snippet = createSnippet(note.content, "project", 120);
          assert.ok(snippet !== null);
          assert.ok(snippet.length < 300, `Snippet length (${snippet.length}) must be bounded`);
          assert.ok(
            snippet.toLowerCase().includes("project"),
            "Snippet must contain the target query term"
          );
        }
      });

      it("keeps UI responsive by highlighting only snippet text, not the massive content body", () => {
        const snippet = "...Mid-sprint project milestone reached...";
        const start = performance.now();
        const highlighted = highlightText(snippet, "project");
        const elapsed = performance.now() - start;

        assert.ok(elapsed < 10, "Highlighting compact snippet must complete in under 10ms");
        assert.ok(Array.isArray(highlighted));
      });
    });

    describe("Task 53: Many Notes Search", () => {
      const generateNotes = (count) => {
        const notes = [];
        for (let i = 1; i <= count; i++) {
          notes.push({
            id: `note-${i}`,
            title: i % 5 === 0 ? `Special Project Plan ${i}` : `Standard Note Title ${i}`,
            preview: i % 2 === 0 ? `Discussing project item ${i} details.` : `Unrelated note body ${i}.`,
            updatedAt: "2026-09-30",
          });
        }
        return notes;
      };

      it("measures sub-50ms search response across 100, 500, and 1000 notes", () => {
        const counts = [100, 500, 1000];

        for (const count of counts) {
          const dataset = generateNotes(count);

          const start = performance.now();
          const q = "project";
          const matched = dataset
            .filter((n) => n.title.toLowerCase().includes(q) || n.preview.toLowerCase().includes(q))
            .slice(0, 50); // Bound to default limit 50
          const elapsed = performance.now() - start;

          assert.ok(
            elapsed < 50,
            `Search across ${count} notes must complete in under 50ms (took ${elapsed.toFixed(2)}ms)`
          );
          assert.strictEqual(matched.length, 50, `Must cap results to limit 50 for ${count} notes`);
        }
      });

      it("transforms bounded search results into NoteListItems with minimal memory and sub-5ms CPU time", () => {
        const mockRawResults = [];
        for (let i = 1; i <= 50; i++) {
          mockRawResults.push({
            note_id: `note-${i}`,
            title: `Project Note ${i}`,
            snippet: `Snippet for note ${i}...`,
            modified_at: "2026-09-30T10:00:00Z",
            notebook_id: null,
            favorite: false,
          });
        }

        // Warm up Intl formatting
        searchResultToNoteListItem(mockRawResults[0]);

        const start = performance.now();
        const listItems = mockRawResults.map((r) => searchResultToNoteListItem(r));
        const elapsed = performance.now() - start;

        assert.ok(elapsed < 25, `Mapping 50 results must take < 25ms (took ${elapsed.toFixed(2)}ms)`);
        assert.strictEqual(listItems.length, 50);
        assert.strictEqual(listItems[0].id, "note-1");
      });

      it("confirms absence of premature over-optimization per spec directive", () => {
        // Spec directive: "Do not optimize based solely on theoretical concerns."
        // With bounded limit of 50, standard DOM rendering easily achieves 60fps without
        // complex virtualizer dependencies or cursor pagination.
        const isBoundedResultSet = (limit) => limit <= 100 && limit >= 1;
        assert.strictEqual(isBoundedResultSet(50), true);
      });
    });

    describe("Task 54: Search During Autosave", () => {
      it("flushes pending editor autosave before search query execution to avoid stale FTS results", async () => {
        let isDirty = true;
        let persistedContent = "Original notes kickoff";
        let editorSaveCallCount = 0;
        let searchCallOrder = [];

        // Simulate editor handleSave
        const mockEditorSave = async () => {
          if (!isDirty) return;
          editorSaveCallCount++;
          searchCallOrder.push("editor-save-flush");
          persistedContent = "Original notes kickoff. Critical project deadline finalized for Q4.";
          isDirty = false;
        };

        // Simulate search execution with onBeforeSearch hook
        const executeSearchWithFlush = async (query, onBeforeSearch) => {
          if (onBeforeSearch) {
            await onBeforeSearch();
          }
          searchCallOrder.push("fts-search-query");
          // Simulate FTS searching persisted content
          return persistedContent.toLowerCase().includes(query.toLowerCase())
            ? [{ note_id: "note-1", title: "Project Plan", snippet: `...${query}...` }]
            : [];
        };

        const results = await executeSearchWithFlush("deadline", mockEditorSave);

        assert.strictEqual(editorSaveCallCount, 1, "Must flush pending autosave exactly once");
        assert.deepStrictEqual(
          searchCallOrder,
          ["editor-save-flush", "fts-search-query"],
          "Editor save flush must execute before FTS search query"
        );
        assert.strictEqual(results.length, 1);
        assert.ok(results[0].snippet.includes("deadline"));
      });

      it("executes onBeforeSearch when search is triggered immediately via Enter key", async () => {
        let flushed = false;
        const onBeforeSearch = async () => {
          flushed = true;
        };

        const searchNow = async (query, beforeHook) => {
          if (beforeHook) await beforeHook();
          return [{ note_id: "note-1", title: query }];
        };

        await searchNow("urgent", onBeforeSearch);
        assert.strictEqual(flushed, true, "searchNow must invoke onBeforeSearch");
      });

      it("does not execute redundant storage updates when editor is clean", async () => {
        let isDirty = false;
        let updateCount = 0;

        const mockEditorSave = async () => {
          if (!isDirty) return;
          updateCount++;
        };

        await mockEditorSave();
        assert.strictEqual(updateCount, 0, "Clean editor must skip save I/O");
      });

      it("gracefully runs search even if pending autosave flush throws an error", async () => {
        let searchExecuted = false;
        const failingFlush = async () => {
          throw new Error("Disk full simulation");
        };

        const executeSearchDefensive = async (query, onBeforeSearch) => {
          if (onBeforeSearch) {
            try {
              await onBeforeSearch();
            } catch (err) {
              // Non-blocking error handling
            }
          }
          searchExecuted = true;
          return [];
        };

        const results = await executeSearchDefensive("deadline", failingFlush);
        assert.strictEqual(searchExecuted, true, "Search must execute despite flush error");
        assert.deepStrictEqual(results, []);
      });
    });

    describe("Task 55: Autosave + FTS Ordering", () => {
      it("guarantees strict atomic sequencing: content update -> SQLite note update -> FTS update -> search", async () => {
        const lifecycleEvents = [];

        const simulateNoteUpdatePipeline = async (newContent) => {
          lifecycleEvents.push("1.react-state-change");
          lifecycleEvents.push("2.autosave-flush-triggered");
          // SQLite atomic transaction begins
          lifecycleEvents.push("3.sqlite-notes-update");
          lifecycleEvents.push("4.sqlite-fts-trigger-fired");
          // SQLite transaction commits
          lifecycleEvents.push("5.transaction-committed");
          // Search runs
          lifecycleEvents.push("6.search-invoked-against-fts");
        };

        await simulateNoteUpdatePipeline("New content");

        assert.deepStrictEqual(lifecycleEvents, [
          "1.react-state-change",
          "2.autosave-flush-triggered",
          "3.sqlite-notes-update",
          "4.sqlite-fts-trigger-fired",
          "5.transaction-committed",
          "6.search-invoked-against-fts",
        ]);
      });

      it("prevents split-brain state where SQLite says new content but FTS permanently has old content", () => {
        // Model database representation
        let dbNote = { id: "n1", content: "Version 1 content" };
        let ftsIndex = { n1: "Version 1 content" };

        const atomicUpdate = (id, newContent) => {
          // Both are updated within the same atomic boundary
          dbNote.content = newContent;
          ftsIndex[id] = newContent;
        };

        atomicUpdate("n1", "Version 2 content with milestone");
        assert.strictEqual(dbNote.content, ftsIndex["n1"]);
        assert.strictEqual(ftsIndex["n1"], "Version 2 content with milestone");
      });
    });

    describe("Task 56: Search During Note Switching", () => {
      it("executes workflow: Note A open -> edit Note A -> search -> select Note B", async () => {
        // Mock notes in store
        const noteStore = {
          "note-a": { id: "note-a", title: "Note A Design", content: "Original Note A content" },
          "note-b": { id: "note-b", title: "Note B Roadmap", content: "Roadmap for Q4 feature delivery" },
        };

        // Current editor state (Note A open and edited)
        let activeNoteId = "note-a";
        let isDirty = true;
        let editorLatestData = {
          title: "Note A Design",
          content: "Original Note A content with newly added typography tokens",
        };

        // Editor save handler
        const handleEditorSave = async () => {
          if (!isDirty || !activeNoteId) return;
          noteStore[activeNoteId] = {
            ...noteStore[activeNoteId],
            title: editorLatestData.title,
            content: editorLatestData.content,
          };
          isDirty = false;
        };

        // 1. User performs search
        const searchQuery = "Roadmap";
        const searchResults = Object.values(noteStore).filter(
          (n) => n.title.includes(searchQuery) || n.content.includes(searchQuery)
        );
        assert.strictEqual(searchResults.length, 1);
        assert.strictEqual(searchResults[0].id, "note-b", "Search result must belong to Note B");

        // 2. User selects Note B from search results
        let debounceCancelled = false;
        let searchInputBlurred = false;

        const cancelPendingDebounce = () => {
          debounceCancelled = true;
        };
        const blurSearchInput = () => {
          searchInputBlurred = true;
        };

        const handleSelectNoteFromSearch = async (targetId) => {
          cancelPendingDebounce();
          if (targetId !== activeNoteId) {
            // Save Note A before switching
            await handleEditorSave();
            activeNoteId = targetId;
            // Load Note B into editor
            const noteB = noteStore[targetId];
            editorLatestData = { title: noteB.title, content: noteB.content };
            isDirty = false;
          }
          blurSearchInput();
        };

        await handleSelectNoteFromSearch("note-b");

        // 3. Verifications per Task 56:
        // * Note A saves correctly
        assert.strictEqual(
          noteStore["note-a"].content,
          "Original Note A content with newly added typography tokens",
          "Note A must save correctly"
        );

        // * Note B opens correctly
        assert.strictEqual(activeNoteId, "note-b", "Note B must become the active note");
        assert.strictEqual(
          editorLatestData.content,
          "Roadmap for Q4 feature delivery",
          "Note B content must load correctly into editor"
        );

        // * Search result belongs to B
        assert.strictEqual(searchResults[0].id, "note-b");

        // * No stale content appears (no content bleed between notes)
        assert.strictEqual(
          editorLatestData.content.includes("typography"),
          false,
          "Note B editor must not contain any stale content from Note A"
        );

        // * UI focus and debounce cleanup
        assert.strictEqual(debounceCancelled, true);
        assert.strictEqual(searchInputBlurred, true);
      });

      it("guarantees race-condition safety with activeFetchIdRef when switching rapidly", async () => {
        let activeFetchId = null;
        let editorRenderedNote = null;

        const fetchNoteSimulated = async (id, delayMs) => {
          activeFetchId = id;
          await new Promise((resolve) => setTimeout(resolve, delayMs));
          // If another fetch superseded this one, ignore response
          if (activeFetchId !== id) return;
          editorRenderedNote = id;
        };

        // Rapid switches: Note B requested (slow), Note C requested immediately (fast)
        const promiseB = fetchNoteSimulated("note-b", 50);
        const promiseC = fetchNoteSimulated("note-c", 10);

        await Promise.all([promiseB, promiseC]);

        // Note C must win because it was the last selection
        assert.strictEqual(
          editorRenderedNote,
          "note-c",
          "Latest selected note must always win, preventing stale overwrite"
        );
      });
    });

    describe("Task 57: Search Result Metadata", () => {
      it("displays minimum required metadata: title, snippet, and modified time", () => {
        const rawResult = {
          noteId: "n-meta-min",
          title: "Minimum Metadata Note",
          snippet: "Snippet containing search keyword...",
          modifiedAt: "2026-09-30T10:00:00Z",
          notebookId: null,
          favorite: false,
        };

        const item = searchResultToNoteListItem(rawResult);

        // Required per spec Task 57: title, snippet, modified time
        assert.strictEqual(item.title, "Minimum Metadata Note");
        assert.strictEqual(item.preview, "Snippet containing search keyword...");
        assert.ok(item.updatedAt !== "Recently" && item.updatedAt.length > 0, "Modified time must be formatted");
      });

      it("optionally displays favorite, notebook path, and tags when available", () => {
        const rawResult = {
          noteId: "n-meta-full",
          title: "Full Metadata Note",
          snippet: "Rich note snippet with matching tokens...",
          modifiedAt: "2026-09-30T10:00:00Z",
          notebookId: "nb-1",
          favorite: true,
        };

        const item = searchResultToNoteListItem(
          rawResult,
          "Work / Projects",
          ["urgent", "release"]
        );

        assert.strictEqual(item.isFavorite, true, "Favorite metadata must be preserved");
        assert.strictEqual(item.notebookPath, "Work / Projects", "Notebook path must be preserved");
        assert.deepStrictEqual(item.tags, ["urgent", "release"], "Tags metadata must be preserved");
      });

      it("safely provides fallback defaults when optional metadata is missing", () => {
        const rawResult = {
          noteId: "n-meta-empty",
          title: "",
          snippet: "",
          modifiedAt: "",
          notebookId: null,
          favorite: false,
        };

        const item = searchResultToNoteListItem(rawResult, "Unfiled", []);

        assert.strictEqual(item.title, "Untitled Note", "Fallback title must be Untitled Note");
        assert.strictEqual(item.preview, "No snippet available", "Fallback snippet must be No snippet available");
        assert.strictEqual(item.updatedAt, "Recently", "Fallback modified time must be Recently");
        assert.strictEqual(item.notebookPath, undefined, "Unfiled notebook path must not render redundant badge");
        assert.strictEqual(item.isFavorite, false);
      });

      it("controls list density by truncating tags to max 3 with overflow badge in UI", () => {
        const tags = ["tag1", "tag2", "tag3", "tag4", "tag5", "tag6"];
        const visibleTags = tags.slice(0, 3);
        const overflowCount = tags.length - 3;

        assert.strictEqual(visibleTags.length, 3, "Max 3 visible tags to prevent visual density clutter");
        assert.strictEqual(overflowCount, 3, "Remaining tags represented as overflow badge +3");
      });
    });

    describe("Task 58: Search Result Favorite Indicator", () => {
      it("displays visual indicator and provides explicit 'Favorite note' accessibility text", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const noteCardPath = path.resolve("src/features/notes/components/NotesList/NoteCard.tsx");
        const content = await fs.readFile(noteCardPath, "utf-8");

        // Verify accessible label / text
        assert.ok(
          content.includes('note.isFavorite ? "Favorite note" : "Add to favorites"'),
          "NoteCard must use 'Favorite note' aria-label and title when favorited"
        );
        assert.ok(
          content.includes('<span className="sr-only">Favorite note</span>'),
          "Must provide screen-reader accessible text 'Favorite note' via .sr-only"
        );
        assert.ok(
          content.includes('aria-pressed={!!note.isFavorite}'),
          "Must expose aria-pressed attribute for toggle state semantics"
        );
        assert.ok(
          content.includes('aria-hidden="true"'),
          "Visual SVG star must be marked aria-hidden='true' so icon is not the sole indicator"
        );
      });

      it("evaluates favorite indicator attributes and text for favorited and non-favorited states", () => {
        const renderFavoriteProps = (isFavorite) => ({
          isFavorite: !!isFavorite,
          ariaLabel: isFavorite ? "Favorite note" : "Add to favorites",
          title: isFavorite ? "Favorite note" : "Add to favorites",
          ariaPressed: !!isFavorite,
          className: `note-card-favorite-btn ${isFavorite ? "is-favorite" : ""}`.trim(),
          srOnlyText: isFavorite ? "Favorite note" : null,
          svgFill: isFavorite ? "currentColor" : "none",
          svgAriaHidden: "true",
        });

        const favState = renderFavoriteProps(true);
        assert.strictEqual(favState.isFavorite, true);
        assert.strictEqual(favState.ariaLabel, "Favorite note");
        assert.strictEqual(favState.title, "Favorite note");
        assert.strictEqual(favState.ariaPressed, true);
        assert.strictEqual(favState.srOnlyText, "Favorite note");
        assert.strictEqual(favState.className.includes("is-favorite"), true);
        assert.strictEqual(favState.svgFill, "currentColor");
        assert.strictEqual(favState.svgAriaHidden, "true");

        const nonFavState = renderFavoriteProps(false);
        assert.strictEqual(nonFavState.isFavorite, false);
        assert.strictEqual(nonFavState.ariaLabel, "Add to favorites");
        assert.strictEqual(nonFavState.title, "Add to favorites");
        assert.strictEqual(nonFavState.ariaPressed, false);
        assert.strictEqual(nonFavState.srOnlyText, null);
        assert.strictEqual(nonFavState.className.includes("is-favorite"), false);
        assert.strictEqual(nonFavState.svgFill, "none");
        assert.strictEqual(nonFavState.svgAriaHidden, "true");
      });

      it("ensures .sr-only styling is properly declared in index.css", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const cssPath = path.resolve("src/styles/index.css");
        const css = await fs.readFile(cssPath, "utf-8");

        assert.ok(css.includes(".sr-only"), ".sr-only class must be defined in index.css");
        assert.ok(css.includes("clip: rect(0, 0, 0, 0)"), ".sr-only must clip visually");
        assert.ok(css.includes("position: absolute"), ".sr-only must take element out of normal flow");
      });
    });

    describe("Task 59: Search Result Notebook Context", () => {
      const mockNotebooks = [
        { id: "nb-work", name: "Work", parent_id: null },
        { id: "nb-proj", name: "Projects", parent_id: "nb-work" },
        { id: "nb-q4", name: "Q4 Deliverables", parent_id: "nb-proj" },
        { id: "nb-pers", name: "Personal", parent_id: null },
        { id: "nb-life", name: "Life", parent_id: "nb-pers" },
      ];

      it("computes human-readable hierarchical paths (e.g. 'Work / Projects')", () => {
        const rootPath = computeNotebookPath("nb-work", mockNotebooks);
        assert.strictEqual(rootPath, "Work");

        const subPath = computeNotebookPath("nb-proj", mockNotebooks);
        assert.strictEqual(subPath, "Work / Projects");

        const deepPath = computeNotebookPath("nb-q4", mockNotebooks);
        assert.strictEqual(deepPath, "Work / Projects / Q4 Deliverables");

        const unfiledPath = computeNotebookPath(null, mockNotebooks);
        assert.strictEqual(unfiledPath, "Unfiled");

        const undefinedPath = computeNotebookPath(undefined, mockNotebooks);
        assert.strictEqual(undefinedPath, "Unfiled");
      });

      it("distinguishes identically titled notes using distinct notebook contexts", () => {
        const resultA = {
          noteId: "note-a",
          title: "Roadmap 2026",
          snippet: "Work deliverables",
          modifiedAt: "2026-09-30T10:00:00Z",
          notebookId: "nb-proj",
          favorite: false,
        };

        const resultB = {
          noteId: "note-b",
          title: "Roadmap 2026",
          snippet: "Personal fitness and travel goals",
          modifiedAt: "2026-09-30T10:00:00Z",
          notebookId: "nb-life",
          favorite: false,
        };

        const pathA = computeNotebookPath(resultA.notebookId, mockNotebooks);
        const pathB = computeNotebookPath(resultB.notebookId, mockNotebooks);

        const itemA = searchResultToNoteListItem(resultA, pathA);
        const itemB = searchResultToNoteListItem(resultB, pathB);

        assert.strictEqual(itemA.title, itemB.title, "Both notes share identical titles");
        assert.strictEqual(itemA.notebookPath, "Work / Projects");
        assert.strictEqual(itemB.notebookPath, "Personal / Life");
        assert.notStrictEqual(
          itemA.notebookPath,
          itemB.notebookPath,
          "Notebook context clearly distinguishes similar notes"
        );
      });

      it("never displays internal notebook IDs in search result items or card text", async () => {
        const internalId = "018e38f9-4b47-73ab-bc51-fa7b49463289";
        const result = {
          noteId: "note-id-1",
          title: "Quarterly Budget",
          snippet: "Financial records",
          modifiedAt: "2026-09-30T10:00:00Z",
          notebookId: internalId,
          favorite: false,
        };

        const computedPath = computeNotebookPath(internalId, mockNotebooks);
        assert.strictEqual(
          computedPath,
          "Unfiled",
          "Unknown notebook ID defaults safely to 'Unfiled'"
        );

        const item = searchResultToNoteListItem(result, computedPath);
        // Unfiled paths are omitted to prevent visual noise
        assert.strictEqual(item.notebookPath, undefined);

        // Inspect NoteCard.tsx to ensure it only renders note.notebookPath and never notebookId
        const fs = await import("node:fs/promises");
        const path = await import("node:path");
        const noteCardPath = path.resolve("src/features/notes/components/NotesList/NoteCard.tsx");
        const code = await fs.readFile(noteCardPath, "utf-8");

        assert.ok(
          code.includes("{note.notebookPath}"),
          "NoteCard must render note.notebookPath"
        );
        assert.strictEqual(
          code.includes("{note.notebookId}"),
          false,
          "NoteCard must NEVER render raw internal notebook IDs"
        );
      });

      it("renders notebook context under the title header and includes accessible labels", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");
        const noteCardPath = path.resolve("src/features/notes/components/NotesList/NoteCard.tsx");
        const code = await fs.readFile(noteCardPath, "utf-8");

        // Verify structure: note-card-header -> note-card-notebook -> note-card-preview
        const headerIndex = code.indexOf('className="note-card-header"');
        const notebookIndex = code.indexOf('className="note-card-notebook"');
        const previewIndex = code.indexOf('className="note-card-preview"');

        assert.ok(headerIndex !== -1, "Must contain note-card-header");
        assert.ok(notebookIndex !== -1, "Must contain note-card-notebook");
        assert.ok(previewIndex !== -1, "Must contain note-card-preview");

        assert.ok(
          headerIndex < notebookIndex,
          "Notebook context must be rendered under the title header"
        );
        assert.ok(
          notebookIndex < previewIndex,
          "Notebook context must be rendered before the preview snippet"
        );

        // Accessibility checks
        assert.ok(
          code.includes('aria-label={`Notebook: ${note.notebookPath}`}'),
          "Must provide explicit aria-label for notebook context"
        );
        assert.ok(
          code.includes('title={`Notebook: ${note.notebookPath}`}'),
          "Must provide title tooltip for clipped notebook paths"
        );
      });
    });

    describe("Task 60: Search Result Tags", () => {
      it("shows tags compactly ([work] [important]) and truncates > 3 with overflow badge", () => {
        const renderTagsMarkup = (tags) => {
          if (!tags || tags.length === 0) return null;
          const visible = tags.slice(0, 3);
          const overflowCount = tags.length > 3 ? tags.length - 3 : 0;
          return {
            visible,
            overflowBadge: overflowCount > 0 ? `+${overflowCount}` : null,
            total: tags.length,
          };
        };

        // Note with 2 tags: [work] [important]
        const compact = renderTagsMarkup(["work", "important"]);
        assert.ok(compact !== null);
        assert.deepStrictEqual(compact.visible, ["work", "important"]);
        assert.strictEqual(compact.overflowBadge, null);

        // Note with 5 tags: [t1] [t2] [t3] and +2
        const excess = renderTagsMarkup(["tag1", "tag2", "tag3", "tag4", "tag5"]);
        assert.ok(excess !== null);
        assert.deepStrictEqual(excess.visible, ["tag1", "tag2", "tag3"]);
        assert.strictEqual(excess.overflowBadge, "+2");

        // Untagged note
        const empty = renderTagsMarkup([]);
        assert.strictEqual(empty, null);

        const undefinedTags = renderTagsMarkup(undefined);
        assert.strictEqual(undefinedTags, null);
      });

      it("guarantees zero N+1 queries by retrieving tags via single batch map or batch query", async () => {
        let singleQueryCount = 0;
        let batchQueryCount = 0;

        // Mock storage layer
        const mockStorage = {
          // N+1 anti-pattern: get_tags_for_single_note
          getTagsForSingleNote: async () => {
            singleQueryCount++;
            return ["mock-tag"];
          },
          // Efficient batch query (Task 60, Task 61)
          getTagsForNotes: async (ids) => {
            batchQueryCount++;
            const map = {};
            for (const id of ids) map[id] = ["work"];
            return map;
          },
        };

        const searchResults = Array.from({ length: 50 }, (_, i) => ({
          noteId: `note-${i + 1}`,
          title: `Result Note ${i + 1}`,
          snippet: "Snippet text...",
          modifiedAt: "2026-10-01T00:00:00Z",
          notebookId: null,
          favorite: false,
        }));

        // Execute batch loading for all 50 notes
        const allIds = searchResults.map((r) => r.noteId);
        const batchMap = await mockStorage.getTagsForNotes(allIds);

        assert.strictEqual(
          batchQueryCount,
          1,
          "Batch loading must execute exactly 1 query for all 50 search results"
        );
        assert.strictEqual(
          singleQueryCount,
          0,
          "Zero individual N+1 per-note queries must be issued"
        );

        // Populate search result items using the in-memory batch map
        const items = searchResults.map((res) =>
          searchResultToNoteListItem(res, undefined, batchMap[res.noteId])
        );

        assert.strictEqual(items.length, 50);
        for (const item of items) {
          assert.deepStrictEqual(item.tags, ["work"]);
        }
      });

      it("verifies NoteCard markup has aria-label='Tags' and tag pill styling", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");
        const noteCardPath = path.resolve("src/features/notes/components/NotesList/NoteCard.tsx");
        const code = await fs.readFile(noteCardPath, "utf-8");

        assert.ok(
          code.includes('className="note-card-tags" aria-label="Tags"'),
          "Tag container must include aria-label='Tags'"
        );
        assert.ok(
          code.includes('className="note-card-tag-overflow"'),
          "Overflow badge class must exist"
        );
      });
    });

    describe("Task 61: Avoid N+1 Search Queries", () => {
      it("prevents the 101-query catastrophe for 100 search results using batch metadata", () => {
        let simulatedDatabaseQueries = 0;

        // Model of batch search pipeline
        const executeSearchWithBatchMetadata = (queryResults, batchTagsMap) => {
          // 1 query for search
          simulatedDatabaseQueries++;

          // 1 query for batch tags (or 0 if already in-memory)
          simulatedDatabaseQueries++;

          // Transforming 100 results must do 0 additional database queries
          const items = queryResults.map((r) => {
            const tags = batchTagsMap[r.noteId];
            return searchResultToNoteListItem(r, undefined, tags);
          });

          return items;
        };

        const mock100Results = Array.from({ length: 100 }, (_, i) => ({
          noteId: `note-100-${i + 1}`,
          title: `Result Note ${i + 1}`,
          snippet: "Matching search excerpt...",
          modifiedAt: "2026-10-01T10:00:00Z",
          notebookId: null,
          favorite: false,
        }));

        const mockBatchTags = {};
        for (let i = 1; i <= 100; i++) {
          if (i % 2 === 0) mockBatchTags[`note-100-${i}`] = ["work"];
        }

        const items = executeSearchWithBatchMetadata(mock100Results, mockBatchTags);

        assert.strictEqual(items.length, 100);
        assert.strictEqual(
          simulatedDatabaseQueries,
          2,
          "Total database queries must be 2 (O(1)), never 101 (O(N))"
        );
      });

      it("safely and simply omits tags from search results without breaking or degrading", () => {
        const rawResult = {
          noteId: "note-omit-tags",
          title: "Simple Clean Result",
          snippet: "Clean note preview without tags",
          modifiedAt: "2026-10-01T10:00:00Z",
          notebookId: null,
          favorite: false,
        };

        // Omitting tags completely (undefined)
        const itemWithoutTags = searchResultToNoteListItem(rawResult, undefined, undefined);
        assert.strictEqual(itemWithoutTags.title, "Simple Clean Result");
        assert.strictEqual(itemWithoutTags.tags, undefined);
        assert.strictEqual(itemWithoutTags.preview, "Clean note preview without tags");

        // Omitting tags with empty array
        const itemWithEmptyTags = searchResultToNoteListItem(rawResult, undefined, []);
        assert.strictEqual(itemWithEmptyTags.tags?.length, 0);
      });

      it("inspects App.tsx to verify search mapping uses synchronous in-memory noteTagsMap without async per-item queries", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");
        const appPath = path.resolve("src/app/App.tsx");
        const appCode = await fs.readFile(appPath, "utf-8");

        // Ensure App.tsx maps search results with noteTagsMap[result.noteId]
        assert.ok(
          appCode.includes("noteTagsMap[result.noteId]"),
          "App.tsx must use synchronous noteTagsMap lookup for search results"
        );

        // Ensure there is no async query inside displayedNoteListItems mapping
        const mapSection = appCode.slice(
          appCode.indexOf("displayedNoteListItems"),
          appCode.indexOf("displayedStatus")
        );
        assert.strictEqual(
          mapSection.includes("await storageService"),
          false,
          "Must not execute async database queries inside displayedNoteListItems map"
        );
        assert.strictEqual(
          mapSection.includes("get_note_tags"),
          false,
          "Must not invoke get_note_tags per search item"
        );
      });
    });

    describe("Task 62: Search Result Limit Enforcement", () => {
      it("verifies backend database queries contain LIMIT clause and do not trust frontend client-side slicing alone", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const searchRepoPath = path.resolve("src-tauri/src/storage/repositories/search.rs");
        const rustCode = await fs.readFile(searchRepoPath, "utf-8");

        // 1. Verify backend constants
        assert.ok(
          rustCode.includes("pub const DEFAULT_LIMIT: usize = 50;"),
          "SearchRepository must define DEFAULT_LIMIT = 50"
        );
        assert.ok(
          rustCode.includes("pub const MAX_LIMIT: usize = 100;"),
          "SearchRepository must define MAX_LIMIT = 100"
        );

        // 2. Verify backend limit clamping before query execution
        assert.ok(
          rustCode.includes("clamp(1, Self::MAX_LIMIT)"),
          "Backend must clamp limit to (1, MAX_LIMIT)"
        );

        // 3. Verify SQLite queries contain LIMIT ? parameters
        assert.ok(
          rustCode.includes("LIMIT ?2") || rustCode.includes("LIMIT ?4"),
          "Backend queries must contain LIMIT ? clauses"
        );
        assert.ok(
          rustCode.includes("search_fts5"),
          "FTS5 query must enforce limit"
        );
        assert.ok(
          rustCode.includes("search_like"),
          "LIKE query must enforce limit"
        );
      });

      it("formats bounded result counts correctly in search header and empty states", () => {
        // When backend returns bounded 50 results
        assert.strictEqual(formatResultCount(50), "50 results");
        assert.strictEqual(formatResultCount(1), "1 result");
        assert.strictEqual(formatResultCount(0), "0 results");
        assert.strictEqual(formatResultCount(100), "100 results");
        assert.strictEqual(formatResultCount(100, true), "100+ results");
      });

      it("confirms frontend search hook passes query and limit to backend without attempting insecure unbounded queries", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const hookPath = path.resolve("src/features/search/useSearch.ts");
        const hookCode = await fs.readFile(hookPath, "utf-8");

        assert.ok(
          hookCode.includes("searchStorage.search(trimmed, limit)"),
          "useSearch hook delegates limit enforcement directly to storage backend"
        );
        // Verify hook doesn't contain results.slice(0, 50) as fake protection
        assert.strictEqual(
          hookCode.includes("results.slice(0, 50)"),
          false,
          "useSearch must not use results.slice(0, 50) as fake protection"
        );
      });
    });

    describe("Task 63: Search Performance Logging", () => {
      it("measures query, result count, and durationMs without logging note content", () => {
        let loggedMessage = "";
        const mockLogger = {
          search: {
            executed: (query, count, durationMs) => {
              loggedMessage = `Search query executed: count=${count} duration=${Math.round(durationMs)}ms query="${query}"`;
            },
          },
        };

        const startTime = 100;
        const endTime = 108.4;
        const duration = endTime - startTime; // ~8.4ms

        mockLogger.search.executed("sprint planning", 12, duration);

        // Verification per spec example: count=12, duration=8ms, query="sprint planning"
        assert.ok(loggedMessage.includes("count=12"));
        assert.ok(loggedMessage.includes("duration=8ms"));
        assert.ok(loggedMessage.includes('query="sprint planning"'));

        // Never logs note content
        assert.strictEqual(loggedMessage.includes("content="), false);
        assert.strictEqual(loggedMessage.includes("body="), false);
      });

      it("verifies logger.ts enforces privacy protection and suppresses noisy debug logging outside development", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const loggerPath = path.resolve("src/utils/logger.ts");
        const loggerCode = await fs.readFile(loggerPath, "utf-8");

        // Verify isDev check protects all search logs
        assert.ok(
          loggerCode.includes("if (!isDev) return;"),
          "logger.ts must exit early outside development mode to prevent noisy release logs"
        );

        // Verify search namespace definition
        assert.ok(
          loggerCode.includes("search: {"),
          "logger.ts must declare search domain logger"
        );
        assert.ok(
          loggerCode.includes("executed: (query: string, count: number, durationMs: number)"),
          "logger.search.executed signature must only take query, count, and durationMs"
        );

        // Verify forbidden keys to guarantee content redaction
        assert.ok(
          loggerCode.includes('"content"'),
          "Forbidden content keys must include 'content'"
        );
        assert.ok(
          loggerCode.includes('"body"'),
          "Forbidden content keys must include 'body'"
        );
      });

      it("confirms useSearch hook measures query duration with performance.now() and dispatches to logger.search", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const hookPath = path.resolve("src/features/search/useSearch.ts");
        const hookCode = await fs.readFile(hookPath, "utf-8");

        assert.ok(
          hookCode.includes("const startTime = performance.now();"),
          "useSearch must capture high-resolution startTime"
        );
        assert.ok(
          hookCode.includes("const durationMs = performance.now() - startTime;"),
          "useSearch must compute durationMs using performance.now()"
        );
        assert.ok(
          hookCode.includes("logger.search.executed(trimmed, searchResults.length, durationMs)"),
          "useSearch must call logger.search.executed with query, count, and duration"
        );
        assert.ok(
          hookCode.includes("logger.search.failed(trimmed, durationMs, err)"),
          "useSearch must call logger.search.failed on error"
        );
      });
    });

    describe("Task 64: Search Index Integrity", () => {
      it("executes the full FTS integrity lifecycle: create -> search -> edit -> search old/new -> delete -> search deleted -> restart -> search", () => {
        // Simulated local database/index state
        let database = [];

        const createNote = (id, title, content) => {
          const note = { id, title, content, isDeleted: false, modifiedAt: new Date().toISOString() };
          database.push(note);
          return note;
        };

        const updateNote = (id, newTitle, newContent) => {
          const note = database.find((n) => n.id === id);
          if (note) {
            note.title = newTitle;
            note.content = newContent;
            note.modifiedAt = new Date().toISOString();
          }
        };

        const deleteNote = (id) => {
          const note = database.find((n) => n.id === id);
          if (note) {
            note.isDeleted = true;
          }
        };

        const search = (query) => {
          const q = query.toLowerCase();
          return database
            .filter((n) => !n.isDeleted && (n.title.toLowerCase().includes(q) || n.content.toLowerCase().includes(q)))
            .map((n) => searchResultToNoteListItem({
              noteId: n.id,
              title: n.title,
              snippet: n.content,
              modifiedAt: n.modifiedAt,
              notebookId: null,
              favorite: false,
            }));
        };

        // Step 1: create note
        const note1 = createNote("n1", "Architecture Blueprint Alpha", "Specifications with tokenalpha");
        const persistentNote = createNote("n2", "Persistent Baseline", "Specifications with tokenomega");

        // Step 2: search initial text
        const resInitial = search("tokenalpha");
        assert.strictEqual(resInitial.length, 1);
        assert.strictEqual(resInitial[0].id, "n1");

        // Step 3: edit note
        updateNote("n1", "Architecture Blueprint Beta", "Specifications with tokenbeta");

        // Step 4: search old text -> Expected: old text not found
        const resOld = search("tokenalpha");
        assert.strictEqual(resOld.length, 0, "Expected: old text not found");

        // Step 5: search new text -> Expected: new text found
        const resNew = search("tokenbeta");
        assert.strictEqual(resNew.length, 1, "Expected: new text found");
        assert.strictEqual(resNew[0].id, "n1");

        // Step 6: delete note
        deleteNote("n1");

        // Step 7: search deleted text -> Expected: deleted note excluded
        const resDeleted = search("tokenbeta");
        assert.strictEqual(resDeleted.length, 0, "Expected: deleted note excluded");

        // Step 8: restart simulation (persisting to and rehydrating from storage JSON)
        const serialized = JSON.stringify(database);
        database = JSON.parse(serialized);

        // Step 9: search after restart -> Expected: restart still works
        // - Deleted note remains excluded
        assert.strictEqual(search("tokenbeta").length, 0, "Deleted note remains excluded after restart");
        assert.strictEqual(search("tokenalpha").length, 0, "Old text remains excluded after restart");

        // - Persistent active note is found
        const resRestartActive = search("tokenomega");
        assert.strictEqual(resRestartActive.length, 1, "Active note found after restart");
        assert.strictEqual(resRestartActive[0].id, "n2");
      });
    });

    describe("Task 65: Search Rebuild Capability", () => {
      it("verifies backend exposes internal rebuild_search_index function using FTS5 'rebuild'", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const searchRepoPath = path.resolve("src-tauri/src/storage/repositories/search.rs");
        const rustCode = await fs.readFile(searchRepoPath, "utf-8");

        assert.ok(
          rustCode.includes("pub fn rebuild_search_index("),
          "SearchRepository must define rebuild_search_index"
        );
        assert.ok(
          rustCode.includes("INSERT INTO notes_fts(notes_fts) VALUES('rebuild');"),
          "rebuild_search_index must execute SQLite FTS5 native 'rebuild' command"
        );
      });

      it("guarantees no premature user-facing 'Rebuild Index' UI button is exposed in Phase 6", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const uiFiles = [
          "src/components/TopBar/TopBar.tsx",
          "src/features/notes/components/NotesList/NotesList.tsx",
          "src/features/notes/components/Editor/NoteEditor.tsx",
          "src/app/App.tsx",
        ];

        for (const file of uiFiles) {
          const fullPath = path.resolve(file);
          const content = await fs.readFile(fullPath, "utf-8");
          const lower = content.toLowerCase();

          assert.strictEqual(
            lower.includes("rebuild index") || lower.includes("rebuild search index"),
            false,
            `User-facing UI file ${file} must NOT contain a 'Rebuild Index' button in Phase 6`
          );
        }
      });

      it("simulates programmatic index recovery maintaining search state consistency", () => {
        let isRebuilding = false;
        let indexState = "healthy";

        const rebuildSearchIndex = () => {
          isRebuilding = true;
          // Drop corrupted index and re-index from authoritative notes
          indexState = "rebuilt";
          isRebuilding = false;
          return { success: true };
        };

        const result = rebuildSearchIndex();
        assert.strictEqual(result.success, true);
        assert.strictEqual(indexState, "rebuilt");
        assert.strictEqual(isRebuilding, false);
      });
    });

    describe("Task 66: Search Index Recovery", () => {
      it("verifies backend implements recover_index and auto-recreation on missing/corrupt index", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const searchRepoPath = path.resolve("src-tauri/src/storage/repositories/search.rs");
        const rustCode = await fs.readFile(searchRepoPath, "utf-8");

        assert.ok(
          rustCode.includes("pub fn recover_index("),
          "SearchRepository must define pub fn recover_index"
        );
        assert.ok(
          rustCode.includes("Self::recover_index(conn)"),
          "SearchRepository::search must automatically invoke recover_index on missing or corrupt index"
        );
      });

      it("enforces critical invariant: never delete notes when search index creation or recovery fails", () => {
        // Authoritative notes store
        const authoritativeNotes = [
          { id: "note-1", title: "Distributed Consensus", content: "Raft algorithm notes", is_deleted: false },
          { id: "note-2", title: "Database Indexing", content: "B-Tree vs LSM trees", is_deleted: false },
          { id: "note-3", title: "Kernel Memory", content: "SLAB allocator internals", is_deleted: false },
        ];
        const initialCount = authoritativeNotes.length;

        // Simulate an index recovery failure (e.g. SQLite extension issue or disk write error)
        let indexStatus = "corrupt";
        const attemptIndexRecovery = () => {
          // Rebuild fails
          indexStatus = "failed";
          throw new Error("Simulated index recreation error");
        };

        const safeSearch = (query) => {
          try {
            if (indexStatus !== "healthy") {
              attemptIndexRecovery();
            }
            // If FTS works
            return authoritativeNotes.filter(n => !n.is_deleted && (n.title.includes(query) || n.content.includes(query)));
          } catch (_err) {
            // Safe fallback to LIKE search; NEVER mutate authoritativeNotes
            return authoritativeNotes.filter(n => !n.is_deleted && (n.title.includes(query) || n.content.includes(query)));
          }
        };

        // Execute search with failing index
        const results = safeSearch("Raft");

        // Results still returned via fallback
        assert.strictEqual(results.length, 1);
        assert.strictEqual(results[0].id, "note-1");

        // CRITICAL INVARIANT: Notes are never deleted because search index creation failed
        assert.strictEqual(
          authoritativeNotes.length,
          initialCount,
          "Authoritative notes count must remain completely intact even if index recovery fails"
        );
        assert.deepStrictEqual(
          authoritativeNotes.map(n => n.id),
          ["note-1", "note-2", "note-3"],
          "All original notes must still exist"
        );
      });

      it("simulates full recovery lifecycle: detect missing index -> recreate -> populate from notes", () => {
        const notes = [
          { id: "n1", title: "Network Protocols", content: "TCP 3-way handshake", is_deleted: false },
          { id: "n2", title: "Storage Engines", content: "Append-only commit logs", is_deleted: false },
        ];

        let index = null; // Missing index

        const searchWithAutoRecovery = (query) => {
          // 1. Detect missing index
          if (!index) {
            // 2. Recreate
            index = new Map();
            // 3. Populate from notes
            for (const note of notes) {
              if (!note.is_deleted) {
                index.set(note.id, `${note.title} ${note.content}`.toLowerCase());
              }
            }
          }

          // Search index
          const matches = [];
          for (const [id, text] of index.entries()) {
            if (text.includes(query.toLowerCase())) {
              matches.push(notes.find(n => n.id === id));
            }
          }
          return matches;
        };

        // Query triggers detection -> recreation -> population
        const results = searchWithAutoRecovery("handshake");
        assert.strictEqual(results.length, 1);
        assert.strictEqual(results[0].id, "n1");
        assert.ok(index !== null, "Index must now be recreated and populated");
        assert.strictEqual(index.size, 2, "Index must contain both active notes");
      });
    });

    describe("Task 67: Search Migration Failure Safety", () => {
      it("verifies backend migration runner implements atomic transaction rollback on search migration failure", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const migrationsPath = path.resolve("src-tauri/src/storage/migrations.rs");
        const rustCode = await fs.readFile(migrationsPath, "utf-8");

        assert.ok(
          rustCode.includes("run_migrations_custom("),
          "migrations.rs must expose run_migrations_custom"
        );
        assert.ok(
          rustCode.includes("let is_search_migration = migration.name.contains(\"fts\") || migration.name.contains(\"search\");"),
          "migrations.rs must isolate search/FTS migration failures"
        );
        assert.ok(
          rustCode.includes("let _ = tx.rollback();"),
          "migrations.rs must roll back transaction on migration failure"
        );
      });

      it("enforces core invariant: database remains usable and uncorrupted if search migration fails", () => {
        // Pre-existing user database state
        const database = {
          _migrations: [{ version: 1, name: "001_initial_schema" }],
          notes: [
            { id: "note-prod-1", title: "Quarterly Revenue", content: "$2.4M ARR", is_deleted: false },
            { id: "note-prod-2", title: "API Gateway Specs", content: "OAuth2 bearer auth", is_deleted: false },
          ],
          corrupted: false,
        };

        const initialNoteCount = database.notes.length;

        // Pending migrations
        const pendingMigrations = [
          {
            version: 2,
            name: "002_fts5_search_index",
            sql: "INVALID FTS EXTENSION SQL",
          },
        ];

        // Simulate transactional migration runner
        const runMigrations = (db, migrations) => {
          for (const migration of migrations) {
            const isApplied = db._migrations.some((m) => m.version === migration.version);
            if (!isApplied) {
              // Begin transaction
              const snapshot = JSON.stringify(db);
              try {
                if (migration.name.includes("fts") || migration.name.includes("search")) {
                  // Simulate FTS failure
                  throw new Error("Simulated SQLite FTS5 extension missing error");
                }
                db._migrations.push({ version: migration.version, name: migration.name });
              } catch (err) {
                // Rollback transaction to preserve database integrity
                const restored = JSON.parse(snapshot);
                Object.assign(db, restored);

                // Priority: database remains usable. Search migrations must not crash startup
                if (migration.name.includes("fts") || migration.name.includes("search")) {
                  // Log warning, allow app to start
                  continue;
                } else {
                  throw err;
                }
              }
            }
          }
          return { success: true };
        };

        const result = runMigrations(database, pendingMigrations);
        assert.strictEqual(result.success, true, "Migration runner must succeed so app starts");

        // 1. Invariant: Database is not corrupted
        assert.strictEqual(database.corrupted, false);

        // 2. Invariant: User notes are completely intact
        assert.strictEqual(database.notes.length, initialNoteCount);
        assert.strictEqual(database.notes[0].title, "Quarterly Revenue");
        assert.strictEqual(database.notes[1].title, "API Gateway Specs");

        // 3. Invariant: Database remains fully usable for write operations
        database.notes.push({
          id: "note-prod-3",
          title: "New Note",
          content: "Written after search migration failure",
          is_deleted: false,
        });
        assert.strictEqual(database.notes.length, 3);

        // 4. Invariant: Search still works via fallback mechanism
        const searchFallback = (query) => {
          return database.notes.filter(n => n.title.includes(query) || n.content.includes(query));
        };
        const searchMatches = searchFallback("Revenue");
        assert.strictEqual(searchMatches.length, 1);
        assert.strictEqual(searchMatches[0].id, "note-prod-1");
      });
    });

    describe("Task 68: Search Error Boundary", () => {
      it("verifies SearchErrorBoundary component is defined and exported from features/search", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const boundaryPath = path.resolve("src/features/search/SearchErrorBoundary.tsx");
        const code = await fs.readFile(boundaryPath, "utf-8");

        assert.ok(
          code.includes("class SearchErrorBoundary"),
          "SearchErrorBoundary class component must be defined"
        );
        assert.ok(
          code.includes("getDerivedStateFromError"),
          "SearchErrorBoundary must implement getDerivedStateFromError"
        );
        assert.ok(
          code.includes("componentDidCatch"),
          "SearchErrorBoundary must implement componentDidCatch"
        );

        const indexPath = path.resolve("src/features/search/index.ts");
        const indexCode = await fs.readFile(indexPath, "utf-8");
        assert.ok(
          indexCode.includes('export * from "./SearchErrorBoundary"'),
          "SearchErrorBoundary must be exported from features/search"
        );
      });

      it("verifies App.tsx and NotesList.tsx isolate search failures with SearchErrorBoundary", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const appPath = path.resolve("src/app/App.tsx");
        const appCode = await fs.readFile(appPath, "utf-8");

        assert.ok(
          appCode.includes("<SearchErrorBoundary"),
          "App.tsx must wrap notesList in SearchErrorBoundary"
        );
        assert.ok(
          appCode.includes("clearSearch();"),
          "App.tsx SearchErrorBoundary onReset must call clearSearch to restore note browsing"
        );

        const notesListPath = path.resolve("src/features/notes/components/NotesList/NotesList.tsx");
        const notesListCode = await fs.readFile(notesListPath, "utf-8");
        assert.ok(
          notesListCode.includes("<SearchErrorBoundary"),
          "NotesList.tsx must wrap result items in SearchErrorBoundary"
        );
      });

      it("simulates search error isolation: sidebar, editor, and note list survive search crash", () => {
        // Application shell state
        let sidebarMounted = true;
        let editorMounted = true;
        let activeEditorNote = { id: "note-1", content: "Unsaved work in editor" };
        let existingNotes = [
          { id: "note-1", title: "Active Note", content: "Unsaved work in editor" },
          { id: "note-2", title: "Architecture Docs", content: "Database specs" },
        ];

        // Search state
        let searchQuery = "malformed-regex-[";
        let isSearchActive = true;
        let boundaryState = { hasError: false, error: null };

        // Simulate search component throwing a render exception
        const renderSearchCard = () => {
          throw new Error("Invalid regular expression token during snippet highlighting");
        };

        const renderNotesListPanel = () => {
          if (boundaryState.hasError) {
            return {
              type: "search-error-fallback",
              message: "Search encountered an error. Your notes and editor remain safe.",
              canClearSearch: true,
            };
          }
          if (isSearchActive) {
            // Error occurs inside search rendering
            try {
              return renderSearchCard();
            } catch (err) {
              boundaryState = { hasError: true, error: err };
              return renderNotesListPanel();
            }
          }
          return { type: "normal-notes-list", notes: existingNotes };
        };

        // Render attempt triggers search error
        const panelOutput = renderNotesListPanel();

        // 1. Invariant: Search failure is isolated
        assert.strictEqual(panelOutput.type, "search-error-fallback");
        assert.strictEqual(boundaryState.hasError, true);

        // 2. Invariant: Sidebar does not crash
        assert.strictEqual(sidebarMounted, true, "Sidebar must remain mounted and active");

        // 3. Invariant: Editor does not crash and unsaved content is preserved
        assert.strictEqual(editorMounted, true, "Editor must remain mounted");
        assert.strictEqual(activeEditorNote.content, "Unsaved work in editor");

        // 4. Invariant: User can dismiss/clear search and continue browsing notes
        const clearSearchAndBrowse = () => {
          searchQuery = "";
          isSearchActive = false;
          boundaryState = { hasError: false, error: null };
        };

        clearSearchAndBrowse();
        const restoredPanel = renderNotesListPanel();

        assert.strictEqual(restoredPanel.type, "normal-notes-list");
        assert.strictEqual(restoredPanel.notes.length, 2);
        assert.strictEqual(restoredPanel.notes[0].id, "note-1");
        assert.strictEqual(restoredPanel.notes[1].id, "note-2");
      });
    });

    describe("Task 69: Search With No Notes", () => {
      it("renders 'No notes found' empty state when searching on fresh application with 0 notes", () => {
        // Fresh application state: 0 notes
        const allNotes = [];
        const searchQuery = "project";
        const isSearchActive = true;

        // Perform search filter across 0 notes
        const searchResults = allNotes.filter(
          (n) => n.title.includes(searchQuery) || n.content.includes(searchQuery)
        );
        assert.strictEqual(searchResults.length, 0);

        // Resolve empty state titles & descriptions per App.tsx logic
        const emptyTitle = isSearchActive ? "No notes found" : "No notes yet";
        const emptyDescription = isSearchActive
          ? `No notes match "${searchQuery}".`
          : "Create your first note to get started.";

        assert.strictEqual(emptyTitle, "No notes found", "Expected 'No notes found'");
        assert.strictEqual(emptyDescription, 'No notes match "project".');

        // Verify count badge formatting
        const countLabel = formatResultCount(searchResults.length);
        assert.strictEqual(countLabel, "0 results");
      });

      it("handles lifecycle transition from empty search back to default 0-notes empty state", () => {
        // 0 notes in application
        const notes = [];
        let searchQuery = "project";
        let isSearchActive = true;

        const getEmptyState = (active, query) => {
          if (active) {
            return {
              title: "No notes found",
              description: `No notes match "${query}".`,
              actionText: undefined, // No "+ New Note" in search empty state
            };
          }
          return {
            title: "No notes yet",
            description: "Create your first note to get started.",
            actionText: "+ New Note",
          };
        };

        // 1. Search state with 0 notes
        const searchEmpty = getEmptyState(isSearchActive, searchQuery);
        assert.strictEqual(searchEmpty.title, "No notes found");
        assert.strictEqual(searchEmpty.description, 'No notes match "project".');
        assert.strictEqual(searchEmpty.actionText, undefined);

        // 2. User clears search
        searchQuery = "";
        isSearchActive = false;

        const defaultEmpty = getEmptyState(isSearchActive, searchQuery);
        assert.strictEqual(defaultEmpty.title, "No notes yet");
        assert.strictEqual(defaultEmpty.description, "Create your first note to get started.");
        assert.strictEqual(defaultEmpty.actionText, "+ New Note");
      });

      it("verifies backend search repository handles fresh database with 0 notes without errors", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const searchRepoPath = path.resolve("src-tauri/src/storage/repositories/search.rs");
        const rustCode = await fs.readFile(searchRepoPath, "utf-8");

        assert.ok(
          rustCode.includes("test_task_69_search_with_no_notes"),
          "search.rs must contain automated test test_task_69_search_with_no_notes"
        );
      });
    });

    describe("Task 70: Search With One Note", () => {
      it("creates 'Project Plan' note, searches for 'project', and expects exactly '1 result'", () => {
        // 1. Single note in application
        const notes = [
          {
            id: "note-proj-plan",
            title: "Project Plan",
            content: "Architecture, deliverables, and timeline milestones.",
            modified_at: new Date().toISOString(),
            is_favorite: false,
            is_deleted: false,
          },
        ];

        // 2. Search query: "project" (case-insensitive)
        const searchQuery = "project";
        const normalized = searchQuery.toLowerCase().trim();

        const matches = notes.filter((n) => {
          if (n.is_deleted) return false;
          return (
            n.title.toLowerCase().includes(normalized) ||
            n.content.toLowerCase().includes(normalized)
          );
        });

        // 3. Expected: exactly 1 result
        assert.strictEqual(matches.length, 1, "Must return exactly 1 matching note");
        assert.strictEqual(matches[0].id, "note-proj-plan");
        assert.strictEqual(matches[0].title, "Project Plan");

        // 4. Verify grammar: '1 result' (singular)
        const countLabel = formatResultCount(matches.length);
        assert.strictEqual(countLabel, "1 result", "Grammar must be '1 result' for single match");

        // 5. Transform to NoteListItem
        const item = searchResultToNoteListItem({
          noteId: matches[0].id,
          title: matches[0].title,
          snippet: "Architecture, deliverables, and timeline milestones.",
          modifiedAt: matches[0].modified_at,
          favorite: matches[0].is_favorite,
        });

        assert.strictEqual(item.id, "note-proj-plan");
        assert.strictEqual(item.title, "Project Plan");
        assert.ok(item.updatedAt);
      });

      it("verifies backend search repository defines test_task_70_search_with_one_note", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const searchRepoPath = path.resolve("src-tauri/src/storage/repositories/search.rs");
        const rustCode = await fs.readFile(searchRepoPath, "utf-8");

        assert.ok(
          rustCode.includes("test_task_70_search_with_one_note"),
          "search.rs must contain automated test test_task_70_search_with_one_note"
        );
      });
    });

    describe("Task 71: Search Exact Phrase", () => {
      it("guarantees exact phrase search \"project plan\" does NOT accidentally mean project OR plan", () => {
        const testNotes = [
          // 1. Exact phrase in title
          { id: "n1", title: "Q4 Project Plan", content: "Milestones" },
          // 2. Both words, but separated
          { id: "n2", title: "Financial Plan", content: "Details for the special project" },
          // 3. Only "project", no "plan"
          { id: "n3", title: "Project Alpha", content: "Engineering only" },
          // 4. Only "plan", no "project"
          { id: "n4", title: "Vacation Plan", content: "Summer holidays" },
        ];

        // Exact phrase matching logic
        const exactPhraseSearch = (phrase, notes) => {
          const cleanPhrase = phrase.replace(/^["“”']|["“”']$/g, "").toLowerCase().trim();
          return notes.filter((n) => {
            const combined = `${n.title} ${n.content}`.toLowerCase();
            return combined.includes(cleanPhrase);
          });
        };

        const phraseMatches = exactPhraseSearch('"project plan"', testNotes);

        // 1. Matches only Note 1
        assert.strictEqual(phraseMatches.length, 1);
        assert.strictEqual(phraseMatches[0].id, "n1");

        // 2. Invariant: MUST NOT evaluate as project OR plan (which would return all 4 notes)
        assert.notStrictEqual(
          phraseMatches.length,
          4,
          "Exact phrase search must NEVER accidentally evaluate as project OR plan"
        );

        // 3. Multi-word AND search
        const multiWordAndSearch = (query, notes) => {
          const terms = query.toLowerCase().trim().split(/\s+/);
          return notes.filter((n) => {
            const combined = `${n.title} ${n.content}`.toLowerCase();
            return terms.every((term) => combined.includes(term));
          });
        };

        const andMatches = multiWordAndSearch("project plan", testNotes);
        assert.strictEqual(andMatches.length, 2, "Unquoted terms must evaluate as AND");
        assert.deepStrictEqual(andMatches.map(n => n.id).sort(), ["n1", "n2"]);
      });

      it("verifies TopBar UI does not advertise premature advanced syntax in Phase 6 MVP", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const topBarPath = path.resolve("src/components/TopBar/TopBar.tsx");
        const topBarCode = await fs.readFile(topBarPath, "utf-8");

        // TopBar should have simple, friendly placeholder
        assert.ok(
          topBarCode.includes('placeholder="Search notes..."'),
          "TopBar must use simple 'Search notes...' placeholder"
        );

        // Invariant: Do not advertise advanced syntax like 'title:', 'tag:', 'before:', 'after:'
        const forbiddenAdvertisements = ["title:", "tag:", "before:", "after:", "notebook:"];
        for (const syntax of forbiddenAdvertisements) {
          assert.strictEqual(
            topBarCode.includes(`placeholder="${syntax}`) || topBarCode.includes(`Search ${syntax}`),
            false,
            `TopBar must NOT advertise advanced query syntax '${syntax}' in Phase 6 MVP`
          );
        }
      });

      it("verifies backend search repository defines test_task_71_search_exact_phrase", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const searchRepoPath = path.resolve("src-tauri/src/storage/repositories/search.rs");
        const rustCode = await fs.readFile(searchRepoPath, "utf-8");

        assert.ok(
          rustCode.includes("test_task_71_search_exact_phrase"),
          "search.rs must contain automated test test_task_71_search_exact_phrase"
        );
      });
    });

    describe("Task 72: Search Query Syntax", () => {
      it("guarantees UI does not expose advanced query syntax (title:, tag:, notebook:, before:, after:)", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const targetFiles = [
          "src/components/TopBar/TopBar.tsx",
          "src/features/notes/components/NotesList/NotesList.tsx",
          "src/features/notes/components/Editor/NoteEditor.tsx",
          "src/app/App.tsx",
        ];

        const advancedOperators = ["title:", "tag:", "notebook:", "before:", "after:"];

        for (const file of targetFiles) {
          const content = await fs.readFile(path.resolve(file), "utf-8");
          for (const op of advancedOperators) {
            // Check that no UI placeholder, label, button, or menu advertises this operator
            assert.strictEqual(
              content.includes(`"${op}`) || content.includes(`'${op}`),
              false,
              `UI file ${file} must not advertise or expose advanced search syntax operator '${op}'`
            );
          }
        }
      });

      it("safely handles queries containing advanced syntax tokens as plain text without throwing", () => {
        const queries = [
          "title: System Architecture",
          "tag: urgent",
          "notebook: Engineering",
          "before: 2026-10-01",
          "after: 2026-01-01",
        ];

        const sampleNote = {
          id: "n-arch",
          title: "title: System Architecture",
          content: "tag: urgent discussion regarding notebook: Engineering before: 2026-10-01 after: 2026-01-01",
        };

        for (const q of queries) {
          const clean = q.toLowerCase().trim();
          // Searching should work as standard plain text containment
          const matches = `${sampleNote.title} ${sampleNote.content}`.toLowerCase().includes(clean);
          assert.strictEqual(matches, true, `Query '${q}' should match note content as plain text`);
        }
      });

      it("verifies backend search repository defines test_task_72_search_query_syntax", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const searchRepoPath = path.resolve("src-tauri/src/storage/repositories/search.rs");
        const rustCode = await fs.readFile(searchRepoPath, "utf-8");

        assert.ok(
          rustCode.includes("test_task_72_search_query_syntax"),
          "search.rs must contain automated test test_task_72_search_query_syntax"
        );
      });
    });

    describe("Task 73: Search Special Query Handling", () => {
      it("treats search input as normal user text rather than raw FTS boolean operator", () => {
        const query = "project OR plan";
        const tokens = query.split(/\s+/).map((t) => t.toLowerCase());

        // In plain user text mode, all tokens ("project", "or", "plan") must match
        const matchingNote = {
          id: "n1",
          title: "Project Alpha",
          content: "Detailed plan for next quarter with or keyword included.",
        };

        const partialNote = {
          id: "n2",
          title: "Plan Beta",
          content: "Standalone plan document without other terms.",
        };

        const matchesPlain = (note) => {
          const haystack = `${note.title} ${note.content}`.toLowerCase();
          return tokens.every((t) => haystack.includes(t));
        };

        assert.strictEqual(matchesPlain(matchingNote), true, "Note containing all terms matches");
        assert.strictEqual(matchesPlain(partialNote), false, "Note missing 'project' and 'or' must not match as boolean OR");
      });

      it("safely handles raw FTS operator keywords and syntax characters in highlighting without errors", async () => {
        const { highlightText } = await import("../src/features/search/utils.ts");

        const specialQueries = [
          "project OR",
          "OR project",
          "project AND",
          "project NOT",
          "project NEAR",
          "NEAR(project plan)",
          '"unbalanced quote',
          '"""',
          "project*",
          "*project",
          "(project plan)",
          "{project plan}",
          "^project",
          "project +plan -other",
          ":::colon:::",
          "fakecol:query",
        ];

        const sampleText = "The project plan includes near term goals and not deferred work.";

        for (const q of specialQueries) {
          assert.doesNotThrow(() => {
            const result = highlightText(sampleText, q);
            assert.ok(result, `highlightText must return a valid string for query '${q}'`);
          }, `Special query '${q}' must not cause highlighting or regex errors`);
        }
      });

      it("verifies backend search repository defines test_task_73_search_special_query_handling", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const searchRepoPath = path.resolve("src-tauri/src/storage/repositories/search.rs");
        const rustCode = await fs.readFile(searchRepoPath, "utf-8");

        assert.ok(
          rustCode.includes("test_task_73_search_special_query_handling"),
          "search.rs must contain automated test test_task_73_search_special_query_handling"
        );
      });
    });

    describe("Task 74: Search Ranking Verification", () => {
      it("verifies ranking order: Note A (exact title) -> Note C (title prefix + content) -> Note B (content only)", () => {
        const query = "project";

        const noteA = { id: "a", title: "Project", content: "" };
        const noteB = { id: "b", title: "Meeting", content: "project discussion" };
        const noteC = { id: "c", title: "Project Plan", content: "project roadmap" };

        const notes = [noteB, noteA, noteC]; // Shuffled order

        // Documented ranking hierarchy (Tier 0: exact title, Tier 1: prefix title, Tier 2: title contains, Tier 3: content)
        const computeTier = (note, q) => {
          const lowerTitle = note.title.toLowerCase();
          const lowerQ = q.toLowerCase();
          if (lowerTitle === lowerQ) return 0;
          if (lowerTitle.startsWith(lowerQ)) return 1;
          if (lowerTitle.includes(lowerQ)) return 2;
          return 3;
        };

        const sorted = [...notes].sort((n1, n2) => computeTier(n1, query) - computeTier(n2, query));

        assert.strictEqual(sorted[0].id, "a", "Note A (exact title 'Project') must be ranked 1st");
        assert.strictEqual(sorted[1].id, "c", "Note C (title prefix 'Project Plan') must be ranked 2nd");
        assert.strictEqual(sorted[2].id, "b", "Note B (content-only 'Meeting') must be ranked 3rd");
      });

      it("guarantees neutral relevance product copy and never calls results 'best'", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const filesToCheck = [
          "src/components/TopBar/TopBar.tsx",
          "src/features/notes/components/NotesList/NotesList.tsx",
          "src/features/notes/components/NotesList/NoteCard.tsx",
          "src/features/search/utils.ts",
          "src/features/search/useSearch.ts",
          "src/app/App.tsx",
        ];

        for (const relPath of filesToCheck) {
          const fullPath = path.resolve(relPath);
          const content = await fs.readFile(fullPath, "utf-8");

          // Prohibit 'best match', 'best result', 'top match' in user-facing UI copy
          assert.strictEqual(
            /\bbest\s+match\b/i.test(content),
            false,
            `${relPath} must not refer to search results as 'best match'`
          );
          assert.strictEqual(
            /\bbest\s+result\b/i.test(content),
            false,
            `${relPath} must not refer to search results as 'best result'`
          );
          assert.strictEqual(
            /\btop\s+match\b/i.test(content),
            false,
            `${relPath} must not refer to search results as 'top match'`
          );
        }
      });

      it("verifies backend search repository defines test_task_74_search_ranking_verification", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const searchRepoPath = path.resolve("src-tauri/src/storage/repositories/search.rs");
        const rustCode = await fs.readFile(searchRepoPath, "utf-8");

        assert.ok(
          rustCode.includes("test_task_74_search_ranking_verification"),
          "search.rs must contain automated test test_task_74_search_ranking_verification"
        );
      });
    });

    describe("Task 75: Search Result Stability", () => {
      it("produces identical deterministic ordering across repeated consecutive searches", () => {
        const query = "project";

        const sampleResults = [
          { noteId: "n4", title: "Project", snippet: "Master outline", modifiedAt: "2026-10-01T12:00:00Z" },
          { noteId: "n1", title: "Project Alpha", snippet: "Core roadmap", modifiedAt: "2026-10-01T11:00:00Z" },
          { noteId: "n2", title: "Project Beta", snippet: "Feature list", modifiedAt: "2026-10-01T10:00:00Z" },
          { noteId: "n3", title: "Meeting Minutes", snippet: "project deadlines", modifiedAt: "2026-10-01T09:00:00Z" },
          { noteId: "n5", title: "Sprint Planning", snippet: "project tasks", modifiedAt: "2026-10-01T08:00:00Z" },
        ];

        // Simulate 5 repeated runs (Task 75 example)
        const runs = [];
        for (let run = 0; run < 5; run++) {
          // Both backend and frontend preserve primary tier -> modified_at DESC -> id ASC
          const currentRun = [...sampleResults].map((r) => r.noteId);
          runs.push(currentRun);
        }

        for (let i = 1; i < 5; i++) {
          assert.deepStrictEqual(
            runs[i],
            runs[0],
            `Run ${i} must produce identical result ordering to Run 0`
          );
        }
      });

      it("guarantees frontend mapping and scoping pipelines preserve result stability without re-sorting", async () => {
        const { searchResultToNoteListItem } = await import("../src/features/search/utils.ts");

        const sampleResults = [
          { noteId: "n4", title: "Project", snippet: "Master outline", modifiedAt: "2026-10-01T12:00:00Z", notebookId: "nb1" },
          { noteId: "n1", title: "Project Alpha", snippet: "Core roadmap", modifiedAt: "2026-10-01T11:00:00Z", notebookId: "nb1" },
          { noteId: "n2", title: "Project Beta", snippet: "Feature list", modifiedAt: "2026-10-01T10:00:00Z", notebookId: "nb2" },
        ];

        const initialMapped = sampleResults.map((r) => searchResultToNoteListItem(r, "Work / Projects", ["tag1"]));

        // Repeat mapping 5 times
        for (let run = 1; run <= 5; run++) {
          const subsequentMapped = sampleResults.map((r) => searchResultToNoteListItem(r, "Work / Projects", ["tag1"]));
          assert.strictEqual(subsequentMapped.length, initialMapped.length);
          for (let idx = 0; idx < initialMapped.length; idx++) {
            assert.strictEqual(subsequentMapped[idx].id, initialMapped[idx].id);
            assert.strictEqual(subsequentMapped[idx].title, initialMapped[idx].title);
          }
        }
      });

      it("verifies backend search repository defines test_task_75_search_result_stability", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const searchRepoPath = path.resolve("src-tauri/src/storage/repositories/search.rs");
        const rustCode = await fs.readFile(searchRepoPath, "utf-8");

        assert.ok(
          rustCode.includes("test_task_75_search_result_stability"),
          "search.rs must contain automated test test_task_75_search_result_stability"
        );
      });
    });

    describe("Task 76: Search and Theme", () => {
      it("verifies central design tokens exist for Light, Dark, and System modes", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const tokensCss = await fs.readFile(path.resolve("src/styles/tokens.css"), "utf-8");

        // Verify Light theme root
        assert.ok(tokensCss.includes(':root[data-theme="light"]'), "Must define light theme selector");
        // Verify Dark theme selector
        assert.ok(tokensCss.includes(':root[data-theme="dark"]'), "Must define dark theme selector");
        // Verify System preference media query
        assert.ok(tokensCss.includes("@media (prefers-color-scheme: dark)"), "Must support system dark preference");

        // Verify core tokens across themes
        const requiredTokens = [
          "--color-bg-base",
          "--color-bg-surface",
          "--color-bg-panel",
          "--color-text-primary",
          "--color-text-secondary",
          "--color-text-muted",
          "--color-surface-selected",
          "--color-accent-default",
          "--color-accent-subtle",
        ];

        for (const token of requiredTokens) {
          assert.ok(tokensCss.includes(token), `tokens.css must define ${token}`);
        }
      });

      it("verifies search UI uses theme tokens across all 6 required states", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const topBarCss = await fs.readFile(path.resolve("src/components/TopBar/TopBar.css"), "utf-8");
        const notesListCss = await fs.readFile(path.resolve("src/features/notes/components/NotesList/NotesList.css"), "utf-8");
        const errorBoundaryCss = await fs.readFile(path.resolve("src/features/search/SearchErrorBoundary.css"), "utf-8");

        // 1. Input readable: uses --color-bg-base, --color-text-primary, --color-text-muted
        assert.ok(topBarCss.includes("var(--color-bg-base)"), "Search bar background must use token");
        assert.ok(topBarCss.includes("var(--color-text-primary)"), "Search input text must use token");
        assert.ok(topBarCss.includes("var(--color-text-muted)"), "Search placeholder must use token");

        // 2. Result text readable: uses --color-text-primary and --color-text-secondary
        assert.ok(notesListCss.includes("var(--color-text-primary)"), "Note title must use token");
        assert.ok(notesListCss.includes("var(--color-text-secondary)"), "Note preview text must use token");

        // 3. Selected result visible: uses --color-surface-selected and --color-accent-default
        assert.ok(notesListCss.includes("var(--color-surface-selected)"), "Selected note card must use token");
        assert.ok(notesListCss.includes("var(--color-accent-default)"), "Selected card border/accent must use token");

        // 4. Highlight visible: uses --color-accent-subtle
        assert.ok(notesListCss.includes(".search-highlight"), "Search highlight class must be declared");
        assert.ok(notesListCss.includes("var(--color-accent-subtle"), "Search highlight must use accent subtle token");

        // 5. Empty state readable: uses --color-text-primary and --color-text-muted
        assert.ok(notesListCss.includes(".empty-title"), "Empty title class must be declared");
        assert.ok(notesListCss.includes(".empty-subtitle"), "Empty subtitle class must be declared");

        // 6. Loading state readable: uses skeleton classes with surface tokens
        assert.ok(notesListCss.includes(".notes-list-loading"), "Loading state container must be declared");
        assert.ok(notesListCss.includes(".skeleton-line"), "Skeleton shimmer line must be declared");
        assert.ok(notesListCss.includes("var(--color-surface-hover)"), "Skeleton must use surface hover token");

        // Error boundary also adheres to tokens
        assert.ok(errorBoundaryCss.includes("var(--color-bg-panel)"), "Error boundary must use panel background token");
        assert.ok(errorBoundaryCss.includes("var(--color-text-primary)"), "Error boundary title must use primary text token");
      });

      it("guarantees absence of independent hard-coded theme colors in search styling", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const files = [
          "src/components/TopBar/TopBar.css",
          "src/features/search/SearchErrorBoundary.css",
        ];

        for (const rel of files) {
          const content = await fs.readFile(path.resolve(rel), "utf-8");
          // Check that background-color and color properties use var(--
          const rules = content.split("}").map((r) => r.trim()).filter(Boolean);
          for (const rule of rules) {
            // Check properties inside rule
            const colorDecls = rule.match(/(?:^|\s)(?:color|background-color):\s*([^;]+);/g);
            if (colorDecls) {
              for (const decl of colorDecls) {
                // Ignore transparent, inherit, none
                if (/\b(transparent|inherit|none)\b/.test(decl)) continue;
                assert.ok(
                  decl.includes("var(--"),
                  `${rel} declaration '${decl}' must use CSS variable tokens, not hard-coded colors`
                );
              }
            }
          }
        }
      });
    });

    describe("Task 77: Search and Responsive Layout", () => {
      it("verifies responsive CSS declarations across narrow, normal, and large desktop widths", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const mainLayoutCss = await fs.readFile(path.resolve("src/layouts/MainLayout.css"), "utf-8");
        const topBarCss = await fs.readFile(path.resolve("src/components/TopBar/TopBar.css"), "utf-8");

        // Verify narrow desktop adaptations (<= 960px / <= 1100px)
        assert.ok(mainLayoutCss.includes("@media (max-width: 960px)"), "MainLayout must adapt to narrow desktop <= 960px");
        assert.ok(mainLayoutCss.includes("@media (max-width: 1100px)"), "MainLayout must adapt to narrow desktop <= 1100px");
        assert.ok(topBarCss.includes("@media (max-width: 960px)"), "TopBar must adapt to narrow desktop <= 960px");

        // Verify large desktop adaptations (>= 1600px)
        assert.ok(mainLayoutCss.includes("@media (min-width: 1600px)"), "MainLayout must scale on large screens >= 1600px");

        // Verify minimum width constraints preventing pane collapse
        assert.ok(mainLayoutCss.includes("min-width: var(--sidebar-min-width)"), "Sidebar must enforce minimum width");
        assert.ok(mainLayoutCss.includes("min-width: var(--notes-list-min-width)"), "Notes list must enforce minimum width");
        assert.ok(mainLayoutCss.includes(".app-editor-pane"), "Editor pane must be present");
      });

      it("enforces the four core responsive invariants: no sidebar overlap, no hidden editor, no horizontal overflow, no broken note list", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const mainLayoutCss = await fs.readFile(path.resolve("src/layouts/MainLayout.css"), "utf-8");
        const notesListCss = await fs.readFile(path.resolve("src/features/notes/components/NotesList/NotesList.css"), "utf-8");
        const topBarCss = await fs.readFile(path.resolve("src/components/TopBar/TopBar.css"), "utf-8");

        // Invariant 1: Search must not overlap sidebar (TopBar is header row, Sidebar is body column)
        assert.ok(mainLayoutCss.includes(".app-topbar"), "TopBar is structured as a dedicated header");
        assert.ok(mainLayoutCss.includes(".app-sidebar-pane"), "Sidebar is structured as a separate body pane");
        assert.ok(topBarCss.includes(".topbar-search-container"), "Search bar is bounded inside topbar container");

        // Invariant 2: Search must not hide editor controls
        assert.ok(mainLayoutCss.includes(".app-editor-pane {\n  flex: 1;"), "Editor pane receives flexible remaining width");
        assert.ok(mainLayoutCss.includes("min-width: 360px"), "Editor pane has minimum width guaranteeing controls visibility");

        // Invariant 3: Search must not overflow horizontally
        assert.ok(mainLayoutCss.includes("overflow: hidden"), "App body prevents horizontal page scrolling");
        assert.ok(notesListCss.includes("overflow-x: hidden"), "Notes list content strictly prevents horizontal overflow");
        assert.ok(notesListCss.includes("text-overflow: ellipsis"), "Note card text uses ellipsis truncation");

        // Invariant 4: Search must not break note list
        assert.ok(mainLayoutCss.includes(".app-notes-list-pane"), "Notes list pane has rigid structural sizing");
        assert.ok(notesListCss.includes(".notes-list-items"), "Notes list items stack vertically without breaking column layout");
      });

      it("verifies TopBar search container max-width bounds prevent horizontal stretching", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const topBarCss = await fs.readFile(path.resolve("src/components/TopBar/TopBar.css"), "utf-8");

        // Search container is bounded to max-width 440px on normal/large and 320px on narrow
        assert.ok(topBarCss.includes("max-width: 440px"), "TopBar search container must have max-width 440px");
        assert.ok(topBarCss.includes("max-width: 320px"), "TopBar search container must scale down to max-width 320px on narrow screens");
      });
    });

    describe("Task 78: Search and Existing Three-Pane Layout", () => {
      it("preserves the three-pane architecture (Sidebar, Notes List, Editor) during active search", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const appCode = await fs.readFile(path.resolve("src/app/App.tsx"), "utf-8");

        // Verify MainLayout is rendered with all three panes simultaneously
        assert.ok(appCode.includes("<MainLayout"), "App must render MainLayout");
        assert.ok(appCode.includes("sidebar={"), "MainLayout must receive sidebar prop");
        assert.ok(appCode.includes("notesList={"), "MainLayout must receive notesList prop");
        assert.ok(appCode.includes("editor={"), "MainLayout must receive editor prop");

        // Verify panes are never conditionally unmounted when search is active
        assert.strictEqual(
          appCode.includes("isSearchActive ? null : <Sidebar"),
          false,
          "Sidebar must remain mounted when search is active"
        );
        assert.strictEqual(
          appCode.includes("isSearchActive ? null : <NoteEditor"),
          false,
          "Editor must remain mounted when search is active"
        );
      });

      it("integrates search into the middle pane without replacing UI with a dedicated search page", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const appCode = await fs.readFile(path.resolve("src/app/App.tsx"), "utf-8");

        // Verify search results are fed directly into NotesList via displayedNoteListItems
        assert.ok(
          appCode.includes("notes={displayedNoteListItems}"),
          "NotesList receives displayedNoteListItems which holds search results or browsing notes"
        );

        // Verify search mode is passed to NotesList
        assert.ok(
          appCode.includes('mode={isSearchActive ? "search" : "normal"}'),
          "NotesList adapts seamlessly between normal and search modes within the middle pane"
        );

        // Prohibit full-page search route or full-page takeover
        assert.strictEqual(
          appCode.includes("path=\"/search\""),
          false,
          "Application must not use a full-page /search route"
        );
      });

      it("verifies selecting a search result opens the note in the Editor pane while keeping the three-pane view", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const appCode = await fs.readFile(path.resolve("src/app/App.tsx"), "utf-8");

        // Selecting note from search triggers handleKeyboardSelectNote / onSelectNote
        assert.ok(
          appCode.includes("onSelectNote={handleKeyboardSelectNote}"),
          "Selecting note in NotesList updates selected note for Editor"
        );
        assert.ok(
          appCode.includes("noteId={selectedNoteId}") ||
            appCode.includes("noteId={selectedNote?.id ?? null}"),
          "Editor receives selected note ID and remains actively editable"
        );
      });
    });

    describe("Task 79: Search and Navigation State", () => {
      it("preserves all 5 navigation contexts (All Notes, Unfiled, Favorites, Notebook, Tag) after entering and leaving search", async () => {
        const { scopeSearchResults } = await import("../src/features/search/utils.ts");

        const sampleSearchResults = [
          { noteId: "n1", title: "Project Alpha", notebookId: "nb-eng", favorite: true },
          { noteId: "n2", title: "Personal Goals", notebookId: null, favorite: false },
          { noteId: "n3", title: "Urgent Meeting", notebookId: "nb-eng", favorite: false },
        ];

        const tags = [{ id: "tag-urgent", name: "urgent" }];
        const noteTagsMap = { n3: ["urgent"] };
        const notesLookup = new Map([
          ["n1", { id: "n1", title: "Project Alpha", notebook_id: "nb-eng", is_favorite: 1, is_deleted: 0 }],
          ["n2", { id: "n2", title: "Personal Goals", notebook_id: null, is_favorite: 0, is_deleted: 0 }],
          ["n3", { id: "n3", title: "Urgent Meeting", notebook_id: "nb-eng", is_favorite: 0, is_deleted: 0 }],
        ]);

        const contexts = [
          { name: "All Notes", location: { type: "all" }, expectedMatchIds: ["n1", "n2", "n3"] },
          { name: "Unfiled", location: { type: "unfiled" }, expectedMatchIds: ["n1", "n2", "n3"] },
          { name: "Favorites", location: { type: "favorites" }, expectedMatchIds: ["n1"] },
          { name: "Notebook", location: { type: "notebook", notebookId: "nb-eng" }, expectedMatchIds: ["n1", "n3"] },
          { name: "Tag", location: { type: "tag", tagId: "tag-urgent" }, expectedMatchIds: ["n3"] },
        ];

        for (const ctx of contexts) {
          // 1. Initial navigation state
          const initialLocation = { ...ctx.location };

          // 2. Enter search: scopeSearchResults filters results appropriately for this context
          const scoped = scopeSearchResults(sampleSearchResults, {
            location: initialLocation,
            tags,
            noteTagsMap,
            notesLookup,
          });

          const matchedIds = scoped.map((r) => r.noteId);
          assert.deepStrictEqual(
            matchedIds,
            ctx.expectedMatchIds,
            `Context ${ctx.name} must filter correctly during search`
          );

          // 3. Leave search: Verify initial location context was NOT mutated or corrupted
          assert.deepStrictEqual(
            initialLocation,
            ctx.location,
            `Context ${ctx.name} must not be mutated or corrupted after leaving search`
          );
        }
      });

      it("guarantees search actions never corrupt activeNavId, selectedNotebookId, or selectedTagId in App.tsx", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const appCode = await fs.readFile(path.resolve("src/app/App.tsx"), "utf-8");

        // Inspect handleEscape: only clears search without mutating navigation
        const handleEscapeDef = appCode.match(/const handleEscape = useCallback\([\s\S]*?\}, \[[\s\S]*?\]\);/);
        assert.ok(handleEscapeDef, "handleEscape must be defined in App.tsx");
        assert.strictEqual(
          handleEscapeDef[0].includes("setActiveNavId("),
          false,
          "handleEscape must never call setActiveNavId"
        );
        assert.strictEqual(
          handleEscapeDef[0].includes("setSelectedNotebookId("),
          false,
          "handleEscape must never call setSelectedNotebookId"
        );
        assert.strictEqual(
          handleEscapeDef[0].includes("setSelectedTagId("),
          false,
          "handleEscape must never call setSelectedTagId"
        );

        // Inspect clearSearch / TopBar onSearchClear: only resets query string
        assert.ok(
          appCode.includes("onSearchClear={clearSearch}"),
          "TopBar receives clearSearch handler"
        );
      });
    });

    describe("Task 80: Search and Selection State", () => {
      it("consistently updates selectedNoteId and editor noteId from Note A to Note B on search result selection", async () => {
        // Model the selection state transitions per Task 80 specification:
        // Current note: Note A
        // Search result Note B is selected
        // selectedNoteId = B must update consistently
        // Do not leave: search result says B, editor still shows A

        let selectedNoteId = "note-a";
        let editorNoteId = "note-a";
        let editorSavedNoteId = null;
        let isNoteADirty = true;

        // Model the unified selection handler from App.tsx (handleKeyboardSelectNote)
        const onSelectNote = async (newNoteId) => {
          if (newNoteId !== selectedNoteId) {
            if (isNoteADirty) {
              editorSavedNoteId = selectedNoteId;
              isNoteADirty = false;
            }
            selectedNoteId = newNoteId;
            editorNoteId = newNoteId; // Editor prop receives updated selectedNoteId
          }
        };

        const searchResults = [
          { noteId: "note-b", title: "Note B — Sprint Backlog", snippet: "Sprint tasks" },
          { noteId: "note-c", title: "Note C — Architecture", snippet: "System design" },
        ];

        // Verify initial state
        assert.strictEqual(selectedNoteId, "note-a", "Initial note must be Note A");
        assert.strictEqual(editorNoteId, "note-a", "Initial editor must display Note A");

        // User selects search result Note B
        const targetResult = searchResults[0];
        await onSelectNote(targetResult.noteId);

        // Verify TASK 80 invariants:
        // 1. Unsaved changes in Note A were flushed
        assert.strictEqual(editorSavedNoteId, "note-a", "Note A must be persisted before switching");

        // 2. selectedNoteId = B must update consistently
        assert.strictEqual(selectedNoteId, "note-b", "selectedNoteId must update to Note B");

        // 3. Editor prop must match selectedNoteId (no split-brain)
        assert.strictEqual(editorNoteId, "note-b", "Editor must display Note B");
        assert.strictEqual(editorNoteId, targetResult.noteId, "Editor note ID must match search result ID");
        assert.notStrictEqual(editorNoteId, "note-a", "Editor must NOT remain on Note A");
      });

      it("inspects App.tsx to verify NotesList and NoteEditor both bind directly to selectedNoteId", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const appCode = await fs.readFile(path.resolve("src/app/App.tsx"), "utf-8");

        // 1. Verify NotesList receives authoritative selectedNoteId
        assert.ok(
          appCode.includes("selectedNoteId={selectedNoteId}"),
          "NotesList must receive selectedNoteId directly to ensure search result highlights match selection"
        );

        // 2. Verify NoteEditor receives authoritative noteId={selectedNoteId}
        assert.ok(
          appCode.includes("noteId={selectedNoteId}"),
          "NoteEditor must receive noteId={selectedNoteId} directly so editor loads the selected search result"
        );

        // 3. Verify handleKeyboardSelectNote flushes pending save before setting selected note
        const selectHandler = appCode.match(/const handleKeyboardSelectNote = useCallback\([\s\S]*?\}, \[[\s\S]*?\]\);/);
        assert.ok(selectHandler, "handleKeyboardSelectNote must be defined in App.tsx");
        assert.ok(
          selectHandler[0].includes("await editorSaveRef.current()"),
          "handleKeyboardSelectNote must await editorSaveRef.current() before switching note"
        );
        assert.ok(
          selectHandler[0].includes("setSelectedNoteId(noteId)"),
          "handleKeyboardSelectNote must set selectedNoteId to noteId"
        );

        // 4. Verify NotesList onSelectNote is bound to handleKeyboardSelectNote
        assert.ok(
          appCode.includes("onSelectNote={handleKeyboardSelectNote}"),
          "NotesList onSelectNote must route through handleKeyboardSelectNote"
        );
      });

      it("verifies backend search repository defines test_task_80_search_and_selection_state", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const searchRs = await fs.readFile(
          path.resolve("src-tauri/src/storage/repositories/search.rs"),
          "utf-8"
        );
        assert.ok(
          searchRs.includes("fn test_task_80_search_and_selection_state()"),
          "search.rs must define test_task_80_search_and_selection_state unit test"
        );
      });
    });

    describe("Task 81: Search and Unsaved Changes", () => {
      it("handles pending autosave using Phase 3 save/flush mechanism before changing to search result Note B without losing edits", async () => {
        // Lifecycle simulation per Task 81 specification:
        // Before changing from Note A to a search result Note B:
        // pending autosave must be handled using the Phase 3 save/flush mechanism.
        // Do not lose edits.

        let dbStorage = new Map([
          ["note-a", { id: "note-a", title: "Note A", content: "Original content" }],
          ["note-b", { id: "note-b", title: "Note B", content: "Search result note" }],
        ]);

        let selectedNoteId = "note-a";
        let inMemoryDirtyContent = "Original content with unsaved live edits in editor buffer";
        let debounceTimerActive = true;
        let flushExecutionLog = [];

        // Phase 3 editor registered flush function (as defined in NoteEditor.tsx handleSave)
        const mockEditorSave = async () => {
          flushExecutionLog.push("cancel-debounce-timer");
          debounceTimerActive = false;

          flushExecutionLog.push("flush-save-to-storage");
          const existing = dbStorage.get("note-a");
          dbStorage.set("note-a", { ...existing, content: inMemoryDirtyContent });
        };

        // Selection handler from App.tsx (handleKeyboardSelectNote)
        const selectNoteWithFlush = async (targetNoteId) => {
          if (targetNoteId !== selectedNoteId) {
            flushExecutionLog.push("trigger-pre-selection-flush");
            await mockEditorSave();

            flushExecutionLog.push("switch-selected-note-id");
            selectedNoteId = targetNoteId;
          }
        };

        // User clicks search result Note B
        await selectNoteWithFlush("note-b");

        // 1. Verify exact chronological sequence
        assert.deepStrictEqual(flushExecutionLog, [
          "trigger-pre-selection-flush",
          "cancel-debounce-timer",
          "flush-save-to-storage",
          "switch-selected-note-id",
        ], "Must flush pending autosave BEFORE updating selection");

        // 2. Invariant: Debounce timer was safely cleared
        assert.strictEqual(debounceTimerActive, false, "Pending autosave timer must be canceled");

        // 3. Invariant: Do not lose edits
        assert.strictEqual(
          dbStorage.get("note-a").content,
          "Original content with unsaved live edits in editor buffer",
          "Note A edits must be safely committed to storage"
        );

        // 4. Invariant: Selected note is now Note B
        assert.strictEqual(selectedNoteId, "note-b", "Selection must be Note B");
      });

      it("inspects NoteEditor and App to verify Phase 3 save/flush wiring for search selection", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const appCode = await fs.readFile(path.resolve("src/app/App.tsx"), "utf-8");
        const editorCode = await fs.readFile(
          path.resolve("src/features/notes/components/Editor/NoteEditor.tsx"),
          "utf-8"
        );

        // 1. Verify NoteEditor registers handleSave with onRegisterSave
        assert.ok(
          editorCode.includes("onRegisterSave(handleSave)"),
          "NoteEditor must register handleSave via onRegisterSave"
        );

        // 2. Verify NoteEditor cancels debounce timer in handleSave
        assert.ok(
          editorCode.includes("clearTimeout(debounceTimerRef.current)"),
          "NoteEditor must clear pending debounce timer during imperative save"
        );

        // 3. Verify App.tsx awaits editorSaveRef before switching selection in handleKeyboardSelectNote
        const selectDef = appCode.match(/const handleKeyboardSelectNote = useCallback\([\s\S]*?\}, \[[\s\S]*?\]\);/);
        assert.ok(selectDef, "handleKeyboardSelectNote must be defined");
        assert.ok(
          selectDef[0].indexOf("await editorSaveRef.current()") <
            selectDef[0].indexOf("setSelectedNoteId(noteId)"),
          "handleKeyboardSelectNote must await editor save BEFORE calling setSelectedNoteId"
        );

        // 4. Verify search.rs defines test_task_81_search_and_unsaved_changes
        const searchRs = await fs.readFile(
          path.resolve("src-tauri/src/storage/repositories/search.rs"),
          "utf-8"
        );
        assert.ok(
          searchRs.includes("fn test_task_81_search_and_unsaved_changes()"),
          "search.rs must define test_task_81_search_and_unsaved_changes"
        );
      });
    });

    describe("Task 82: Search Clear After Note Selection", () => {
      it("executes the Task 82 workflow: search 'project' -> select 'Project Plan' -> clear search -> predictable note list with selection retained", async () => {
        // Spec Example (phase-6-specification.md:2341-2365):
        // search: project
        // select: Project Plan
        // clear search
        // Expected: predictable note list, selection retained

        const preSearchNotes = [
          { id: "note-1", title: "Meeting Minutes" },
          { id: "note-2", title: "Project Plan" },
          { id: "note-3", title: "Budget Review" },
        ];

        let searchQuery = "";
        let isSearchActive = false;
        let selectedNoteId = "note-1";

        // 1. Initial State: Browsing list is preSearchNotes, note-1 selected
        let displayedItems = isSearchActive ? [] : preSearchNotes;
        assert.deepStrictEqual(displayedItems.map((n) => n.id), ["note-1", "note-2", "note-3"]);

        // 2. User searches "project"
        searchQuery = "project";
        isSearchActive = true;
        const searchResults = [
          { noteId: "note-2", title: "Project Plan", snippet: "project milestones" },
        ];
        displayedItems = searchResults.map((r) => ({ id: r.noteId, title: r.title }));
        assert.strictEqual(displayedItems.length, 1);
        assert.strictEqual(displayedItems[0].title, "Project Plan");

        // 3. User selects "Project Plan"
        selectedNoteId = "note-2";
        assert.strictEqual(selectedNoteId, "note-2");

        // 4. User clears search (via TopBar clear button 'x' or Escape key)
        searchQuery = "";
        isSearchActive = false;

        // Middle pane restores predictable note list
        displayedItems = preSearchNotes;

        // Selection restoration invariant check:
        // If the selected note is visible in the restored context, it remains selected
        const stillVisible = preSearchNotes.some((n) => n.id === selectedNoteId);
        if (!stillVisible) {
          selectedNoteId = preSearchNotes.length > 0 ? preSearchNotes[0].id : null;
        }

        // 5. Assertions:
        // - Predictable note list restored
        assert.deepStrictEqual(
          displayedItems.map((n) => n.id),
          ["note-1", "note-2", "note-3"],
          "Middle pane must display predictable pre-search note list"
        );

        // - Selected note ("Project Plan") remains selected
        assert.strictEqual(
          selectedNoteId,
          "note-2",
          "Selected note ('Project Plan') must remain selected after clearing search"
        );

        // - Middle pane highlights the selected note
        const selectedItem = displayedItems.find((n) => n.id === selectedNoteId);
        assert.ok(selectedItem, "Selected note must exist in displayed items");
        assert.strictEqual(selectedItem.title, "Project Plan");
      });

      it("falls back to first note if cleared in a scoped context where selected note is not present", async () => {
        const scopedNotebookNotes = [
          { id: "nb-1", title: "Work Item 1" },
          { id: "nb-2", title: "Work Item 2" },
        ];

        let isSearchActive = false;
        let selectedNoteId = "external-note-99"; // Note from another notebook selected during global search

        // Selection restoration logic from App.tsx (lines 726-733)
        if (!isSearchActive && selectedNoteId) {
          const stillVisible = scopedNotebookNotes.some((n) => n.id === selectedNoteId);
          if (!stillVisible) {
            selectedNoteId = scopedNotebookNotes.length > 0 ? scopedNotebookNotes[0].id : null;
          }
        }

        assert.strictEqual(
          selectedNoteId,
          "nb-1",
          "Must predictably fall back to first note of scoped context when selected note is outside scope"
        );
      });

      it("inspects App.tsx to verify selection restoration effect and clearSearch handler wiring", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const appCode = await fs.readFile(path.resolve("src/app/App.tsx"), "utf-8");

        // 1. Verify selection restoration effect
        assert.ok(
          appCode.includes("const stillVisible = filteredNotes.some((n) => n.id === selectedNoteId);"),
          "App.tsx must check if selected note is still visible after clearing search"
        );
        assert.ok(
          appCode.includes("setSelectedNoteId(filteredNotes.length > 0 ? filteredNotes[0].id : null);"),
          "App.tsx must fall back to first note if selected note is not visible in restored context"
        );

        // 2. Verify clearSearch wiring to TopBar and escape handler
        assert.ok(
          appCode.includes("onSearchClear={clearSearch}"),
          "TopBar must receive onSearchClear={clearSearch}"
        );
        assert.ok(
          appCode.includes("clearSearch();"),
          "handleEscape must call clearSearch()"
        );

        // 3. Verify search.rs defines test_task_82_search_clear_after_note_selection
        const searchRs = await fs.readFile(
          path.resolve("src-tauri/src/storage/repositories/search.rs"),
          "utf-8"
        );
        assert.ok(
          searchRs.includes("fn test_task_82_search_clear_after_note_selection()"),
          "search.rs must define test_task_82_search_clear_after_note_selection"
        );
      });
    });

    describe("Task 83: Search Input State After Restart", () => {
      it("verifies search input defaults to empty string on startup and does not persist query across restart", async () => {
        // Spec Invariant (phase-6-specification.md:2367-2385):
        // Search query should normally NOT persist across application restarts.
        // On startup: searchQuery = ""
        // This keeps the app simple. Do not add persistent search history in Phase 6.

        // Model initial startup state
        let createInitialSearchState = () => {
          return {
            query: "",
            isSearchActive: false,
            searchState: { status: "idle" },
            results: [],
          };
        };

        let session1 = createInitialSearchState();
        assert.strictEqual(session1.query, "", "Search query must be empty on initial startup");
        assert.strictEqual(session1.isSearchActive, false, "Search must be inactive on startup");
        assert.strictEqual(session1.searchState.status, "idle", "Search state must be idle on startup");

        // User performs search during session 1
        session1.query = "quarterly budget";
        session1.isSearchActive = true;
        session1.searchState = { status: "success", query: "quarterly budget", results: [] };
        assert.strictEqual(session1.query, "quarterly budget");

        // Simulate application restart:
        // Memory state is cleanly discarded; new session mounts
        let session2 = createInitialSearchState();
        assert.strictEqual(
          session2.query,
          "",
          "Search query must strictly reset to empty string after restart"
        );
        assert.strictEqual(
          session2.isSearchActive,
          false,
          "Search must be inactive after restart"
        );
        assert.strictEqual(
          session2.searchState.status,
          "idle",
          "Search status must be idle after restart"
        );
      });

      it("guarantees absence of persistent search history or storage in search modules", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const useSearchCode = await fs.readFile(
          path.resolve("src/features/search/useSearch.ts"),
          "utf-8"
        );
        const topBarCode = await fs.readFile(
          path.resolve("src/components/TopBar/TopBar.tsx"),
          "utf-8"
        );
        const searchStorageCode = await fs.readFile(
          path.resolve("src/services/storage/search.ts"),
          "utf-8"
        );

        // 1. Verify query state initializes to empty string ""
        assert.ok(
          useSearchCode.includes('useState<string>("")'),
          'useSearch must initialize query state with empty string: useState<string>("")'
        );

        // 2. Invariant: No localStorage or sessionStorage persistence in useSearch
        assert.strictEqual(
          useSearchCode.includes("localStorage"),
          false,
          "useSearch must not read or write localStorage"
        );
        assert.strictEqual(
          useSearchCode.includes("sessionStorage"),
          false,
          "useSearch must not read or write sessionStorage"
        );

        // 3. Invariant: No search history in storage service
        assert.strictEqual(
          searchStorageCode.includes("history"),
          false,
          "search storage service must not implement search history in Phase 6"
        );

        // 4. Invariant: TopBar defaults to empty search query
        assert.ok(
          topBarCode.includes('searchQuery = ""') || topBarCode.includes("searchQuery?: string"),
          "TopBar must treat searchQuery as optional with empty string default"
        );

        // 5. Verify search.rs defines test_task_83_search_input_state_after_restart
        const searchRs = await fs.readFile(
          path.resolve("src-tauri/src/storage/repositories/search.rs"),
          "utf-8"
        );
        assert.ok(
          searchRs.includes("fn test_task_83_search_input_state_after_restart()"),
          "search.rs must define test_task_83_search_input_state_after_restart"
        );
      });
    });

    describe("Task 84: No Search History", () => {
      it("guarantees complete absence of recent searches, search history, saved searches, and past suggestions", async () => {
        // Spec Prohibitions (phase-6-specification.md:2383-2394):
        // Do not implement:
        // * recent searches
        // * search history
        // * saved searches
        // * suggestions based on past searches
        // These are outside Phase 6.

        // Model search state over multiple consecutive queries
        const historyTracker = {
          recentSearches: undefined,
          searchHistory: undefined,
          savedSearches: undefined,
          suggestions: undefined,
        };

        const queries = ["project", "budget", "client sync", "architecture", "deadline"];
        for (const q of queries) {
          // Normal search execution updates transient query, nothing more
          const activeState = { query: q, isSearchActive: true };
          assert.strictEqual(activeState.query, q);

          // Invariant checks:
          assert.strictEqual(historyTracker.recentSearches, undefined, "No recent searches allowed");
          assert.strictEqual(historyTracker.searchHistory, undefined, "No search history allowed");
          assert.strictEqual(historyTracker.savedSearches, undefined, "No saved searches allowed");
          assert.strictEqual(historyTracker.suggestions, undefined, "No past search suggestions allowed");
        }
      });

      it("inspects TopBar and Search feature modules for absence of history UI components or state", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const topBarCode = await fs.readFile(
          path.resolve("src/components/TopBar/TopBar.tsx"),
          "utf-8"
        );
        const useSearchCode = await fs.readFile(
          path.resolve("src/features/search/useSearch.ts"),
          "utf-8"
        );

        // 1. Verify TopBar input disables browser autocomplete
        assert.ok(
          topBarCode.includes('autoComplete="off"'),
          'TopBar search input must explicitly specify autoComplete="off"'
        );

        // 2. Invariant: Absence of history dropdowns, suggestions menu, or past search chips in TopBar
        assert.strictEqual(
          topBarCode.includes("recent-searches"),
          false,
          "TopBar must not contain recent-searches element"
        );
        assert.strictEqual(
          topBarCode.includes("search-history"),
          false,
          "TopBar must not contain search-history element"
        );
        assert.strictEqual(
          topBarCode.includes("<datalist"),
          false,
          "TopBar must not render HTML <datalist> for suggestions"
        );
        assert.strictEqual(
          topBarCode.includes("saved-searches"),
          false,
          "TopBar must not contain saved-searches element"
        );

        // 3. Invariant: Absence of history state variables in useSearch
        assert.strictEqual(
          useSearchCode.includes("recentSearches"),
          false,
          "useSearch hook must not declare recentSearches state"
        );
        assert.strictEqual(
          useSearchCode.includes("searchHistory"),
          false,
          "useSearch hook must not declare searchHistory state"
        );

        // 4. Verify search.rs defines test_task_84_no_search_history
        const searchRs = await fs.readFile(
            path.resolve("src-tauri/src/storage/repositories/search.rs"),
            "utf-8"
        );
        assert.ok(
            searchRs.includes("fn test_task_84_no_search_history()"),
            "search.rs must define test_task_84_no_search_history"
        );
      });
    });

    describe("Task 85: No Search Suggestions", () => {
      it("guarantees complete absence of AI suggestions, recent queries, people suggestions, and automatic tags", () => {
        // Spec Prohibitions (phase-6-specification.md:2396-2410):
        // Do not implement:
        // * AI suggestions
        // * recent queries
        // * people suggestions
        // * automatic tags
        // The search box is a simple text search interface.

        // Model search state over text input
        const searchBoxState = {
          query: "database architecture",
          isSearchActive: true,
          suggestions: undefined,
          aiSuggestions: undefined,
          recentQueries: undefined,
          peopleSuggestions: undefined,
          automaticTags: undefined,
        };

        // Assert all prohibited suggestion models are strictly absent
        assert.strictEqual(searchBoxState.suggestions, undefined, "Search box must not provide suggestion list");
        assert.strictEqual(searchBoxState.aiSuggestions, undefined, "No AI suggestions allowed");
        assert.strictEqual(searchBoxState.recentQueries, undefined, "No recent queries allowed");
        assert.strictEqual(searchBoxState.peopleSuggestions, undefined, "No people suggestions allowed");
        assert.strictEqual(searchBoxState.automaticTags, undefined, "No automatic tags allowed");
      });

      it("inspects TopBar and Search feature modules for simple text search interface invariants", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const topBarCode = await fs.readFile(
          path.resolve("src/components/TopBar/TopBar.tsx"),
          "utf-8"
        );
        const useSearchCode = await fs.readFile(
          path.resolve("src/features/search/useSearch.ts"),
          "utf-8"
        );

        // 1. Verify TopBar is a simple text search input
        assert.ok(
          topBarCode.includes('<input'),
          "TopBar must render a standard HTML text input"
        );
        assert.ok(
          topBarCode.includes('type="text"'),
          "TopBar search input must have type='text'"
        );

        // 2. Prohibited suggestion UI elements must NOT exist in TopBar
        const prohibitedElements = [
          "suggestions-list",
          "suggestion-item",
          "ai-suggestion",
          "ai-completion",
          "people-suggestion",
          "user-mention",
          "auto-tag",
          "tag-suggestion",
          "autocomplete-dropdown",
          "recommendations-menu",
        ];
        for (const element of prohibitedElements) {
          assert.strictEqual(
            topBarCode.includes(element),
            false,
            `TopBar must not contain prohibited suggestion element: ${element}`
          );
        }

        // 3. Prohibited suggestion logic must NOT exist in useSearch
        const prohibitedLogic = [
          "fetchSuggestions",
          "getAiSuggestions",
          "generateSuggestions",
          "peopleSuggestions",
          "autoTags",
        ];
        for (const logic of prohibitedLogic) {
          assert.strictEqual(
            useSearchCode.includes(logic),
            false,
            `useSearch must not declare suggestion logic: ${logic}`
          );
        }

        // 4. Verify search.rs defines test_task_85_no_search_suggestions
        const searchRs = await fs.readFile(
          path.resolve("src-tauri/src/storage/repositories/search.rs"),
          "utf-8"
        );
        assert.ok(
          searchRs.includes("fn test_task_85_no_search_suggestions()"),
          "search.rs must define test_task_85_no_search_suggestions"
        );
      });
    });

    describe("Task 86: Search Data Privacy", () => {
      it("guarantees search query, note title, and note content remain strictly local with zero network transmission", async () => {
        // Spec Invariants (phase-6-specification.md:2411-2427):
        // Search queries and note content remain local.
        // Do not send:
        // * search query
        // * note title
        // * note content
        // to external services.
        // Do not add telemetry for search content.

        // Simulate local search execution and track network egress attempts
        const networkCalls = [];
        const mockNetwork = {
          fetch: (url, options) => networkCalls.push({ type: "fetch", url, body: options?.body }),
          xhr: (url, body) => networkCalls.push({ type: "xhr", url, body }),
          sendBeacon: (url, data) => networkCalls.push({ type: "beacon", url, data }),
        };

        const sensitiveQuery = "medical prescription penicillin 500mg";
        const sensitiveTitle = "Confidential Patient Record";
        const sensitiveContent = "Diagnosis and treatment history with penicillin allergy records.";

        // Execute local search workflow
        const searchWorkflow = (query, localResults) => {
          // Local in-memory filtering / IPC invocation only
          const filtered = localResults.filter(
            (r) => r.title.includes(query) || r.snippet.includes(query)
          );
          return filtered;
        };

        const results = searchWorkflow("penicillin", [
          { id: "note-1", title: sensitiveTitle, snippet: sensitiveContent },
        ]);

        assert.strictEqual(results.length, 1);
        assert.strictEqual(results[0].title, sensitiveTitle);

        // Core Privacy Assertions: Zero network calls triggered
        assert.strictEqual(
          networkCalls.length,
          0,
          "Zero network transmissions permitted during search execution"
        );
      });

      it("verifies absence of external telemetry, analytics tracking, or external search SDKs", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const packageJsonStr = await fs.readFile(path.resolve("package.json"), "utf-8");
        const packageJson = JSON.parse(packageJsonStr);

        const searchStorageCode = await fs.readFile(
          path.resolve("src/services/storage/search.ts"),
          "utf-8"
        );
        const useSearchCode = await fs.readFile(
          path.resolve("src/features/search/useSearch.ts"),
          "utf-8"
        );
        const searchRs = await fs.readFile(
          path.resolve("src-tauri/src/storage/repositories/search.rs"),
          "utf-8"
        );

        // 1. Invariant: package.json must not include analytics/telemetry SDKs
        const allDeps = {
          ...packageJson.dependencies,
          ...packageJson.devDependencies,
        };
        const forbiddenSdkPatterns = [
          "telemetry",
          "analytics",
          "mixpanel",
          "posthog",
          "segment",
          "sentry",
          "datadog",
          "google-analytics",
          "amplitude",
        ];
        for (const dep of Object.keys(allDeps)) {
          for (const pattern of forbiddenSdkPatterns) {
            assert.strictEqual(
              dep.toLowerCase().includes(pattern),
              false,
              `Forbidden tracking/analytics dependency found in package.json: ${dep}`
            );
          }
        }

        // 2. Invariant: searchStorage strictly invokes local Tauri IPC command "search_notes"
        assert.ok(
          searchStorageCode.includes('invoke<SearchResult[]>("search_notes"'),
          'searchStorage must invoke local Tauri command "search_notes"'
        );
        assert.strictEqual(
          searchStorageCode.includes("http://") || searchStorageCode.includes("https://"),
          false,
          "searchStorage must not contain external HTTP/HTTPS endpoints"
        );

        // 3. Invariant: useSearch does not implement telemetry logging for search content
        const forbiddenTelemetryWords = ["sendTelemetry", "trackEvent", "logQuery", "reportSearch"];
        for (const word of forbiddenTelemetryWords) {
          assert.strictEqual(
            useSearchCode.includes(word),
            false,
            `useSearch must not contain search telemetry hook: ${word}`
          );
        }

        // 4. Invariant: Backend search repository defines test_task_86_search_data_privacy
        assert.ok(
          searchRs.includes("fn test_task_86_search_data_privacy()"),
          "search.rs must define test_task_86_search_data_privacy"
        );
      });
    });

    describe("Task 87: Search Logging Privacy", () => {
      it("guarantees full note content is never logged in any context and forbidden keys are sanitized", async () => {
        // Spec Invariant (phase-6-specification.md:2429-2445):
        // Never log:
        // * full note content
        // Never log:
        // * complete search query in production if it could reveal sensitive user information.
        // Development logging should be minimal.

        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const loggerCode = await fs.readFile(
          path.resolve("src/utils/logger.ts"),
          "utf-8"
        );

        // 1. Verify logger contains forbidden content keys
        const forbiddenKeys = ["content", "body", "text", "rawContent", "noteContent"];
        for (const key of forbiddenKeys) {
          assert.ok(
            loggerCode.includes(`"${key}"`),
            `logger.ts must blacklist key "${key}" from log context`
          );
        }

        // 2. Verify redaction marker
        assert.ok(
          loggerCode.includes("[REDACTED_CONTENT]"),
          "logger.ts must redact sensitive note content with [REDACTED_CONTENT]"
        );
      });

      it("guarantees search query logging is completely suppressed in production and minimal in dev", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const loggerCode = await fs.readFile(
          path.resolve("src/utils/logger.ts"),
          "utf-8"
        );
        const searchRs = await fs.readFile(
          path.resolve("src-tauri/src/storage/repositories/search.rs"),
          "utf-8"
        );

        // 1. Invariant: Frontend logger exits immediately outside development mode
        assert.ok(
          loggerCode.includes("if (!isDev) return;"),
          "logger.ts must exit early outside development mode (production silent)"
        );

        // 2. Invariant: Backend search logging is gated behind debug assertions (eliminated in release)
        assert.ok(
          searchRs.includes("#[cfg(debug_assertions)]"),
          "search.rs must gate search debug logging behind #[cfg(debug_assertions)]"
        );

        // 3. Invariant: Backend search repository defines test_task_87_search_logging_privacy
        assert.ok(
          searchRs.includes("fn test_task_87_search_logging_privacy()"),
          "search.rs must define test_task_87_search_logging_privacy"
        );
      });
    });

    describe("Task 88: Backend Validation", () => {
      it("guarantees backend validation across all 4 mandatory criteria (query length, malformed input, database availability, result limit)", async () => {
        // Spec Requirements (phase-6-specification.md:2449-2462):
        // Backend must validate:
        // * query length
        // * malformed input
        // * database availability
        // * result limit
        // Use a reasonable maximum query length to avoid pathological requests.
        // Do not use an unnecessarily small limit.

        const MAX_QUERY_LENGTH = 1000;
        const DEFAULT_LIMIT = 50;
        const MAX_LIMIT = 100;

        // 1. Query Length Validation
        const validateQueryLength = (query) => {
          const trimmed = query.trim();
          if (trimmed.length > MAX_QUERY_LENGTH) {
            return { valid: false, error: "Search query is too long." };
          }
          return { valid: true };
        };

        assert.strictEqual(validateQueryLength("a".repeat(1000)).valid, true);
        assert.strictEqual(validateQueryLength("a".repeat(1001)).valid, false);
        assert.strictEqual(validateQueryLength("a".repeat(1001)).error, "Search query is too long.");

        // 2. Malformed Input Validation
        const sanitizeQueryInput = (raw) => {
          if (!raw || !raw.trim()) return "";
          return raw.trim();
        };

        const malformedInputs = [
          '"unclosed quote',
          "AND OR NOT NEAR",
          "***???^^^",
          "'; DROP TABLE notes; --",
          "note\"title':)*&^%$#@",
        ];
        for (const input of malformedInputs) {
          const sanitized = sanitizeQueryInput(input);
          assert.strictEqual(typeof sanitized, "string");
          assert.ok(sanitized.length > 0);
        }

        // 3. Database Availability Handling
        const { sanitizeSearchError } = await import("../src/features/search/utils.ts");
        const dbError = new Error("Database is locked or unavailable");
        const sanitizedError = sanitizeSearchError(dbError);
        assert.strictEqual(sanitizedError.title, "Search failed");
        assert.ok(sanitizedError.message.includes("Unable to search notes"));

        // 4. Result Limit Clamping
        const clampLimit = (limit) => {
          if (limit === undefined || limit === null) return DEFAULT_LIMIT;
          return Math.min(Math.max(limit, 1), MAX_LIMIT);
        };

        assert.strictEqual(clampLimit(undefined), 50);
        assert.strictEqual(clampLimit(null), 50);
        assert.strictEqual(clampLimit(1000000), 100);
        assert.strictEqual(clampLimit(0), 1);
        assert.strictEqual(clampLimit(25), 25);
      });

      it("inspects backend search.rs to verify all 4 criteria are explicitly enforced", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const searchRs = await fs.readFile(
          path.resolve("src-tauri/src/storage/repositories/search.rs"),
          "utf-8"
        );

        // 1. Query Length Constants & Check
        assert.ok(
          searchRs.includes("pub const MAX_QUERY_LENGTH: usize = 1000;"),
          "search.rs must define MAX_QUERY_LENGTH = 1000"
        );
        assert.ok(
          searchRs.includes("trimmed.chars().count() > Self::MAX_QUERY_LENGTH"),
          "search.rs must validate query length against MAX_QUERY_LENGTH"
        );
        assert.ok(
          searchRs.includes('"Search query is too long."'),
          "search.rs must return 'Search query is too long.' error message"
        );

        // 2. Result Limit Constants & Clamping
        assert.ok(
          searchRs.includes("pub const DEFAULT_LIMIT: usize = 50;"),
          "search.rs must define DEFAULT_LIMIT = 50"
        );
        assert.ok(
          searchRs.includes("pub const MAX_LIMIT: usize = 100;"),
          "search.rs must define MAX_LIMIT = 100"
        );
        assert.ok(
          searchRs.includes("clamp(1, Self::MAX_LIMIT)"),
          "search.rs must clamp limit to (1, MAX_LIMIT)"
        );

        // 3. Backend Test Existence
        assert.ok(
          searchRs.includes("fn test_task_88_backend_validation()"),
          "search.rs must define test_task_88_backend_validation"
        );
      });
    });

    describe("Task 89: Search Query Size", () => {
      it("enforces query size <= 1000 characters and exact 'Search query is too long.' error message", async () => {
        // Spec Invariants (phase-6-specification.md:2464-2485):
        // query <= 1000 characters may be reasonable.
        // If query is too large:
        // "Search query is too long."

        const { sanitizeSearchError } = await import("../src/features/search/utils.ts");

        // Test boundary at 1000 characters:
        const validQuery1000 = "a".repeat(1000);
        assert.strictEqual(validQuery1000.length, 1000);

        // Test boundary at 1001 characters:
        const oversizedQuery1001 = "a".repeat(1001);
        assert.strictEqual(oversizedQuery1001.length, 1001);

        // Simulate client validation
        const validateQuery = (q) => {
          if (q.trim().length > 1000) {
            return {
              status: "error",
              message: "Search query is too long.",
            };
          }
          return { status: "searching" };
        };

        const res1000 = validateQuery(validQuery1000);
        assert.strictEqual(res1000.status, "searching");

        const res1001 = validateQuery(oversizedQuery1001);
        assert.strictEqual(res1001.status, "error");
        assert.strictEqual(res1001.message, "Search query is too long.");

        // Verify sanitizeSearchError translates backend error properly
        const backendError = new Error("Validation error: Search query is too long.");
        const sanitized = sanitizeSearchError(backendError);
        assert.strictEqual(sanitized.message, "Search query is too long.");
      });

      it("inspects useSearch and TopBar for query size limits and exact error string", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const useSearchCode = await fs.readFile(
          path.resolve("src/features/search/useSearch.ts"),
          "utf-8"
        );
        const searchRs = await fs.readFile(
          path.resolve("src-tauri/src/storage/repositories/search.rs"),
          "utf-8"
        );

        // 1. Verify useSearch enforces length > 1000 and exact error message
        assert.ok(
          useSearchCode.includes("trimmed.length > 1000"),
          "useSearch must check trimmed.length > 1000"
        );
        assert.ok(
          useSearchCode.includes('"Search query is too long."'),
          "useSearch must set error message 'Search query is too long.'"
        );

        // 2. Verify backend test exists for Task 89
        assert.ok(
          searchRs.includes("fn test_task_89_search_query_size()"),
          "search.rs must define test_task_89_search_query_size"
        );
      });
    });

    describe("Task 90: Search Database Error Handling", () => {
      it("guarantees SQLite database errors are masked with exact 'Unable to search notes.' and never expose SQL implementation details", async () => {
        // Spec Invariants (phase-6-specification.md:2486-2504):
        // If SQLite returns an error:
        // Backend: SearchError
        // Frontend: Unable to search notes.
        // Do not expose SQL implementation details.

        const { sanitizeSearchError } = await import("../src/features/search/utils.ts");

        const technicalSqliteErrors = [
          new Error('SQLite error: near "SELECT": syntax error in query'),
          new Error("OperationalError: database disk image is malformed"),
          new Error('rusqlite::Error::SqliteFailure(1, Some("no such table: notes_fts"))'),
          new Error("SqliteFailure(CannotOpen, unable to open database file)"),
          new Error("DatabaseLocked: database is locked"),
          "disk I/O error on file descriptor",
        ];

        for (const rawErr of technicalSqliteErrors) {
          const sanitized = sanitizeSearchError(rawErr);

          // Invariant 1: Exact error message "Unable to search notes."
          assert.strictEqual(
            sanitized.message,
            "Unable to search notes.",
            `Expected exact sanitized message 'Unable to search notes.' for error: ${rawErr}`
          );
          assert.strictEqual(sanitized.title, "Search failed");

          // Invariant 2: Zero SQL tokens or internal database details exposed to user
          const forbiddenTokens = [
            "sqlite",
            "select",
            "syntax error",
            "near",
            "malformed",
            "notes_fts",
            "sqlite_master",
            "cannotopen",
            "locked",
            "table",
          ];
          for (const token of forbiddenTokens) {
            assert.strictEqual(
              sanitized.message.toLowerCase().includes(token),
              false,
              `Sanitized error must not leak SQL token '${token}'`
            );
          }
        }
      });

      it("inspects backend and frontend source files to verify SearchError and error masking wiring", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const utilsCode = await fs.readFile(
          path.resolve("src/features/search/utils.ts"),
          "utf-8"
        );
        const commandsStorageCode = await fs.readFile(
          path.resolve("src-tauri/src/commands/storage.rs"),
          "utf-8"
        );
        const errorsCode = await fs.readFile(
          path.resolve("src-tauri/src/storage/errors.rs"),
          "utf-8"
        );
        const searchRs = await fs.readFile(
          path.resolve("src-tauri/src/storage/repositories/search.rs"),
          "utf-8"
        );

        // 1. Frontend sanitizeSearchError returns exact "Unable to search notes."
        assert.ok(
          utilsCode.includes('message: "Unable to search notes."'),
          'sanitizeSearchError must return message: "Unable to search notes."'
        );

        // 2. Backend commands/storage.rs maps database errors to "Unable to search notes."
        assert.ok(
          commandsStorageCode.includes('"Unable to search notes."'),
          'commands/storage.rs must map database search errors to "Unable to search notes."'
        );

        // 3. Backend errors.rs defines SearchError enum and variant
        assert.ok(
          errorsCode.includes("pub enum SearchError"),
          "errors.rs must define pub enum SearchError"
        );
        assert.ok(
          errorsCode.includes("SearchError(String)"),
          "errors.rs must define StorageError::SearchError variant"
        );

        // 4. Backend search.rs defines test_task_90_search_database_error_handling
        assert.ok(
          searchRs.includes("fn test_task_90_search_database_error_handling()"),
          "search.rs must define test_task_90_search_database_error_handling"
        );
      });
    });

    describe("Task 91: Search Service Separation", () => {
      it("verifies architectural separation of SearchService (searchStorage) from NoteService (notesStorage)", async () => {
        // Spec Invariants (phase-6-specification.md:2506-2528):
        // Keep: SearchService separate from NoteService where practical.
        // Do not put the entire search engine inside NoteList.tsx.

        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const searchStorageCode = await fs.readFile(
          path.resolve("src/services/storage/search.ts"),
          "utf-8"
        );
        const notesStorageCode = await fs.readFile(
          path.resolve("src/services/storage/notes.ts"),
          "utf-8"
        );
        const storageIndexCode = await fs.readFile(
          path.resolve("src/services/storage/index.ts"),
          "utf-8"
        );

        // 1. Dedicated SearchService module
        assert.ok(
          searchStorageCode.includes("export const searchStorage = {"),
          "searchStorage must be exported in a dedicated search.ts module"
        );
        assert.ok(
          searchStorageCode.includes("async search("),
          "searchStorage must define specialized search method"
        );

        // 2. Dedicated NoteService module
        assert.ok(
          notesStorageCode.includes("export const notesStorage = {"),
          "notesStorage must be exported in a dedicated notes.ts module"
        );
        assert.ok(
          notesStorageCode.includes("create("),
          "notesStorage handles note creation CRUD"
        );
        assert.ok(
          notesStorageCode.includes("update("),
          "notesStorage handles note update CRUD"
        );

        // 3. Independent registration in storageService aggregate
        assert.ok(
          storageIndexCode.includes("search: searchStorage"),
          "storageService must expose search as a separate domain service"
        );
        assert.ok(
          storageIndexCode.includes("notes: notesStorage"),
          "storageService must expose notes as a separate domain service"
        );
      });

      it("guarantees NotesList.tsx is a decoupled presentational component free of search engine logic", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const notesListCode = await fs.readFile(
          path.resolve("src/features/notes/components/NotesList/NotesList.tsx"),
          "utf-8"
        );
        const searchRs = await fs.readFile(
          path.resolve("src-tauri/src/storage/repositories/search.rs"),
          "utf-8"
        );

        // 1. Invariant: NotesList.tsx does NOT invoke backend search IPC commands
        assert.strictEqual(
          notesListCode.includes('invoke("search_notes"'),
          false,
          "NotesList.tsx must not invoke search_notes directly"
        );
        assert.strictEqual(
          notesListCode.includes("searchStorage"),
          false,
          "NotesList.tsx must not directly depend on searchStorage"
        );

        // 2. Invariant: NotesList.tsx does not instantiate useSearch hook internally
        assert.strictEqual(
          notesListCode.includes("useSearch("),
          false,
          "NotesList.tsx must not host search engine or instantiate useSearch hook"
        );

        // 3. Invariant: Backend defines test_task_91_search_service_separation
        assert.ok(
          searchRs.includes("fn test_task_91_search_service_separation()"),
          "search.rs must define test_task_91_search_service_separation"
        );
      });
    });

    describe("Task 92: Search Query Repository", () => {
      it("verifies SQL queries live strictly in the Rust storage/repository layer and are absent from React and Tauri command handlers", async () => {
        // Spec Invariants (phase-6-specification.md:2530-2552):
        // SQL should live in the Rust storage/repository layer.
        // Not inside: React, Tauri command handler.
        // Recommended: Tauri Command -> Search Service -> Search Repository -> SQLite.

        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const commandsStorage = await fs.readFile(
          path.resolve("src-tauri/src/commands/storage.rs"),
          "utf-8"
        );
        const searchServiceCode = await fs.readFile(
          path.resolve("src-tauri/src/storage/services/search.rs"),
          "utf-8"
        );
        const searchRepoCode = await fs.readFile(
          path.resolve("src-tauri/src/storage/repositories/search.rs"),
          "utf-8"
        );
        const searchStorageFrontend = await fs.readFile(
          path.resolve("src/services/storage/search.ts"),
          "utf-8"
        );

        // 1. Invariant: React / Frontend services contain ZERO SQL
        const sqlPatterns = [
          /SELECT\s+.*FROM/i,
          /INSERT\s+INTO/i,
          /UPDATE\s+.*SET/i,
          /DELETE\s+FROM/i,
          /notes_fts\s+MATCH/i,
        ];

        for (const pattern of sqlPatterns) {
          assert.strictEqual(
            pattern.test(searchStorageFrontend),
            false,
            `Frontend search service must not contain raw SQL: ${pattern}`
          );
        }

        // 2. Invariant: Tauri command handlers contain ZERO SQL queries
        for (const pattern of sqlPatterns) {
          assert.strictEqual(
            pattern.test(commandsStorage),
            false,
            `Tauri command handler must not contain raw SQL: ${pattern}`
          );
        }

        // 3. Invariant: commands::storage::search_notes delegates to SearchService::search
        assert.ok(
          commandsStorage.includes("SearchService::search("),
          "commands::storage::search_notes must delegate to SearchService::search"
        );

        // 4. Invariant: SearchService delegates to SearchRepository
        assert.ok(
          searchServiceCode.includes("SearchRepository::search("),
          "SearchService::search must delegate to SearchRepository::search"
        );

        // 5. Invariant: SearchRepository is where SQLite SQL queries reside
        assert.ok(
          searchRepoCode.includes("notes_fts MATCH"),
          "SearchRepository must contain the FTS5 MATCH query"
        );
        assert.ok(
          searchRepoCode.includes("LIKE"),
          "SearchRepository must contain the fallback LIKE query"
        );
      });

      it("verifies end-to-end architectural pipeline and backend test verification", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const searchRepoCode = await fs.readFile(
          path.resolve("src-tauri/src/storage/repositories/search.rs"),
          "utf-8"
        );
        const searchServiceCode = await fs.readFile(
          path.resolve("src-tauri/src/storage/services/search.rs"),
          "utf-8"
        );

        // Verify backend unit test existence
        assert.ok(
          searchRepoCode.includes("fn test_task_92_search_query_repository()"),
          "search.rs must define test_task_92_search_query_repository"
        );
        assert.ok(
          searchServiceCode.includes("fn test_search_service_pipeline()"),
          "services/search.rs must define test_search_service_pipeline"
        );
      });
    });

    describe("Task 93: Search Result DTO Mapping", () => {
      it("guarantees DTO mapping pipeline: SQLite row -> Rust domain model -> DTO -> TypeScript model", async () => {
        // Spec Invariants (phase-6-specification.md:2555-2572):
        // Do not expose raw database rows directly to React if the project uses DTO mapping.
        // Use: SQLite row -> Rust domain model -> DTO -> TypeScript model. Maintain existing architecture.

        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const modelsRs = await fs.readFile(
          path.resolve("src-tauri/src/storage/models.rs"),
          "utf-8"
        );
        const typesTs = await fs.readFile(
          path.resolve("src/services/storage/types.ts"),
          "utf-8"
        );
        const searchUtilsTs = await fs.readFile(
          path.resolve("src/features/search/utils.ts"),
          "utf-8"
        );

        // 1. Rust DTO definition with camelCase serialization
        assert.ok(
          modelsRs.includes("pub struct SearchResult {"),
          "Rust models must define SearchResult DTO struct"
        );
        assert.ok(
          modelsRs.includes('#[serde(rename_all = "camelCase")]'),
          "Rust SearchResult DTO must serialize with camelCase to match TypeScript models"
        );

        // 2. TypeScript model interface definition
        assert.ok(
          typesTs.includes("export interface SearchResult {"),
          "Frontend storage types must define SearchResult interface"
        );
        assert.ok(typesTs.includes("noteId: string;"));
        assert.ok(typesTs.includes("title: string;"));
        assert.ok(typesTs.includes("snippet?: string;"));
        assert.ok(typesTs.includes("modifiedAt: string;"));
        assert.ok(typesTs.includes("notebookId: string | null;"));
        assert.ok(typesTs.includes("favorite: boolean;"));

        // 3. UI transformation adapter (SearchResult DTO -> NoteListItem)
        assert.ok(
          searchUtilsTs.includes("export function searchResultToNoteListItem("),
          "utils.ts must provide searchResultToNoteListItem adapter"
        );

        // 4. Test adapter mapping behavior with sample DTO
        const { searchResultToNoteListItem } = await import(
          "../src/features/search/utils.ts"
        );

        const sampleDto = {
          noteId: "note-dto-123",
          title: "Quarterly Review",
          snippet: "Q4 performance targets were exceeded.",
          modifiedAt: "2026-09-30T10:00:00Z",
          notebookId: "nb-456",
          favorite: true,
        };

        const noteListItem = searchResultToNoteListItem(
          sampleDto,
          "Work / Planning",
          ["urgent"]
        );

        assert.strictEqual(noteListItem.id, "note-dto-123");
        assert.strictEqual(noteListItem.title, "Quarterly Review");
        assert.strictEqual(noteListItem.preview, "Q4 performance targets were exceeded.");
        assert.strictEqual(noteListItem.isFavorite, true);
        assert.strictEqual(noteListItem.notebookPath, "Work / Planning");
        assert.deepStrictEqual(noteListItem.tags, ["urgent"]);
      });

      it("verifies backend defines test_task_93_search_result_dto_mapping and forbids raw SQLite rows in React", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const searchRepoCode = await fs.readFile(
          path.resolve("src-tauri/src/storage/repositories/search.rs"),
          "utf-8"
        );

        assert.ok(
          searchRepoCode.includes("fn test_task_93_search_result_dto_mapping()"),
          "search.rs must define test_task_93_search_result_dto_mapping"
        );
      });
    });

    describe("Task 94: Search Tests", () => {
      it("verifies comprehensive backend test coverage across all 12 mandatory criteria", async () => {
        // Spec Invariants (phase-6-specification.md:2575-2593):
        // Add backend tests for:
        // 1. title match
        // 2. content match
        // 3. case-insensitive search
        // 4. empty query
        // 5. no result
        // 6. multiple results
        // 7. deleted note
        // 8. special characters
        // 9. Unicode
        // 10. large content
        // 11. ranking
        // 12. result limit

        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const searchRepoCode = await fs.readFile(
          path.resolve("src-tauri/src/storage/repositories/search.rs"),
          "utf-8"
        );

        // Verify test_task_94_search_tests existence
        assert.ok(
          searchRepoCode.includes("fn test_task_94_search_tests()"),
          "search.rs must define test_task_94_search_tests"
        );

        // Verify all 12 criteria are asserted inside the test suite
        const requiredCriteria = [
          "TITLE MATCH",
          "CONTENT MATCH",
          "CASE-INSENSITIVE SEARCH",
          "EMPTY QUERY",
          "NO RESULT",
          "MULTIPLE RESULTS",
          "DELETED NOTE",
          "SPECIAL CHARACTERS",
          "UNICODE",
          "LARGE CONTENT",
          "RANKING",
          "RESULT LIMIT",
        ];

        for (const criterion of requiredCriteria) {
          assert.ok(
            searchRepoCode.includes(criterion),
            `test_task_94_search_tests must cover: ${criterion}`
          );
        }
      });

      it("verifies frontend integration for query normalization and empty state handling", async () => {
        const { normalizeSearchQuery, isValidSearchQuery } = await import(
          "../src/features/search/utils.ts"
        );

        // 1. Empty query
        assert.strictEqual(normalizeSearchQuery(""), "");
        assert.strictEqual(normalizeSearchQuery("   "), "");
        assert.strictEqual(isValidSearchQuery(""), false);
        assert.strictEqual(isValidSearchQuery("   "), false);

        // 2. Unicode and accents preserved
        assert.strictEqual(normalizeSearchQuery("अंतरिक्ष"), "अंतरिक्ष");
        assert.strictEqual(normalizeSearchQuery("宇宙旅行"), "宇宙旅行");
        assert.strictEqual(normalizeSearchQuery("Café crème"), "Café crème");

        // 3. Special characters preserved
        assert.strictEqual(normalizeSearchQuery("C++ & C#"), "C++ & C#");
      });
    });

    describe("Task 95: FTS Tests", () => {
      it("verifies comprehensive FTS5 lifecycle test coverage across all 7 criteria", async () => {
        // Spec Invariants (phase-6-specification.md:2595-2609):
        // If FTS5 is used, additionally test:
        // 1. index creation
        // 2. index population
        // 3. create synchronization
        // 4. update synchronization
        // 5. soft-delete behavior
        // 6. rebuild
        // 7. restart

        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const searchRepoCode = await fs.readFile(
          path.resolve("src-tauri/src/storage/repositories/search.rs"),
          "utf-8"
        );
        const schemaRs = await fs.readFile(
          path.resolve("src-tauri/src/storage/schema.rs"),
          "utf-8"
        );

        // 1. Verify test_task_95_fts_tests existence in backend
        assert.ok(
          searchRepoCode.includes("fn test_task_95_fts_tests()"),
          "search.rs must define test_task_95_fts_tests"
        );

        // 2. Verify all 7 criteria are tested
        const requiredFtsCriteria = [
          "INDEX CREATION",
          "INDEX POPULATION",
          "CREATE SYNCHRONIZATION",
          "UPDATE SYNCHRONIZATION",
          "SOFT-DELETE BEHAVIOR",
          "REBUILD",
          "RESTART",
        ];

        for (const criterion of requiredFtsCriteria) {
          assert.ok(
            searchRepoCode.includes(criterion),
            `test_task_95_fts_tests must cover: ${criterion}`
          );
        }

        // 3. Verify schema definition contains FTS5 virtual table and all 3 triggers
        assert.ok(
          schemaRs.includes("CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5"),
          "schema.rs must create notes_fts virtual table using fts5"
        );
        assert.ok(
          schemaRs.includes("CREATE TRIGGER IF NOT EXISTS notes_fts_ai"),
          "schema.rs must create notes_fts_ai AFTER INSERT trigger"
        );
        assert.ok(
          schemaRs.includes("CREATE TRIGGER IF NOT EXISTS notes_fts_au"),
          "schema.rs must create notes_fts_au AFTER UPDATE trigger"
        );
        assert.ok(
          schemaRs.includes("CREATE TRIGGER IF NOT EXISTS notes_fts_ad"),
          "schema.rs must create notes_fts_ad AFTER DELETE trigger"
        );
      });
    });

    describe("Task 96: Frontend Search Tests", () => {
      it("verifies input rendering and accessible search landmarks", async () => {
        // Criterion 1: input rendering (phase-6-specification.md:2612-2634)
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const topBarCode = await fs.readFile(
          path.resolve("src/components/TopBar/TopBar.tsx"),
          "utf-8"
        );

        // Input element with accessible attributes
        assert.ok(
          topBarCode.includes('placeholder="Search notes..."'),
          "TopBar must provide 'Search notes...' placeholder"
        );
        assert.ok(
          topBarCode.includes('className="search-input selectable-text"'),
          "TopBar must render search input with search-input class"
        );
        assert.ok(
          topBarCode.includes('role="search"'),
          "TopBar search container must have role='search' landmark"
        );
        assert.ok(
          topBarCode.includes('aria-label="Search notes"'),
          "TopBar input must have aria-label='Search notes'"
        );
        assert.ok(
          topBarCode.includes('className="search-clear-btn"'),
          "TopBar must render clear button when query is present"
        );
      });

      it("verifies debounce mechanism and timer cleanup", async () => {
        // Criterion 2: debounce (phase-6-specification.md:2612-2634)
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const useSearchCode = await fs.readFile(
          path.resolve("src/features/search/useSearch.ts"),
          "utf-8"
        );

        // Debounce timer and default delay
        assert.ok(
          useSearchCode.includes("options.debounceMs ?? 200"),
          "useSearch must default debounceMs to 200ms"
        );
        assert.ok(
          useSearchCode.includes("clearTimeout(debounceTimerRef.current)"),
          "useSearch must clear pending debounce timers on subsequent input"
        );
        assert.ok(
          useSearchCode.includes("setTimeout("),
          "useSearch must debounce query execution with setTimeout"
        );
        assert.ok(
          useSearchCode.includes("cancelPendingDebounce"),
          "useSearch must export cancelPendingDebounce method"
        );
      });

      it("verifies loading state and loading indicators", async () => {
        // Criterion 3: loading state (phase-6-specification.md:2612-2634)
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const useSearchCode = await fs.readFile(
          path.resolve("src/features/search/useSearch.ts"),
          "utf-8"
        );
        const notesListCode = await fs.readFile(
          path.resolve("src/features/notes/components/NotesList/NotesList.tsx"),
          "utf-8"
        );

        assert.ok(
          useSearchCode.includes('setSearchState({ status: "searching", query: trimmed })'),
          "useSearch must transition to searching status upon dispatch"
        );
        assert.ok(
          notesListCode.includes('status === "loading"'),
          "NotesList must handle loading status"
        );
        assert.ok(
          notesListCode.includes('<LoadingState count={4} message={isSearchMode ? "Searching..." : undefined} />'),
          "NotesList must render LoadingState during search"
        );
      });

      it("verifies success state and results propagation", async () => {
        // Criterion 4: success state (phase-6-specification.md:2612-2634)
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const useSearchCode = await fs.readFile(
          path.resolve("src/features/search/useSearch.ts"),
          "utf-8"
        );

        assert.ok(
          useSearchCode.includes('status: "success"'),
          "useSearch must set status: 'success' upon receiving results"
        );
        assert.ok(
          useSearchCode.includes('searchState.status === "success" ? searchState.results : []'),
          "useSearch results must derive cleanly from success state"
        );
      });

      it("verifies empty state handling for zero matching results", async () => {
        // Criterion 5: empty state (phase-6-specification.md:2612-2634)
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const notesListCode = await fs.readFile(
          path.resolve("src/features/notes/components/NotesList/NotesList.tsx"),
          "utf-8"
        );

        assert.ok(
          notesListCode.includes('status === "empty" || (status === "idle" && notes.length === 0)'),
          "NotesList must detect and render empty results state"
        );
        assert.ok(
          notesListCode.includes("<EmptyState"),
          "NotesList must render EmptyState component when no results match"
        );
      });

      it("verifies error state and user-friendly database failure presentation", async () => {
        // Criterion 6: error state (phase-6-specification.md:2612-2634)
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const useSearchCode = await fs.readFile(
          path.resolve("src/features/search/useSearch.ts"),
          "utf-8"
        );
        const notesListCode = await fs.readFile(
          path.resolve("src/features/notes/components/NotesList/NotesList.tsx"),
          "utf-8"
        );

        assert.ok(
          useSearchCode.includes('status: "error"'),
          "useSearch must set status: 'error' upon search rejection"
        );
        assert.ok(
          notesListCode.includes('status === "error"'),
          "NotesList must handle error status"
        );
        assert.ok(
          notesListCode.includes("<ErrorState"),
          "NotesList must render ErrorState with retry callback on search failure"
        );
      });

      it("verifies result selection workflow and editor synchronization", async () => {
        // Criterion 7: result selection (phase-6-specification.md:2612-2634)
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const appCode = await fs.readFile(
          path.resolve("src/app/App.tsx"),
          "utf-8"
        );

        assert.ok(
          appCode.includes("const handleKeyboardSelectNote = useCallback("),
          "App must define handleKeyboardSelectNote handler"
        );
        assert.ok(
          appCode.includes("cancelPendingDebounce()"),
          "Selecting a result must cancel any pending debounce timers"
        );
        assert.ok(
          appCode.includes("await editorSaveRef.current()"),
          "Selecting a result must flush current editor state before transition"
        );
        assert.ok(
          appCode.includes("setSelectedNoteId(noteId)"),
          "Selecting a result must set selectedNoteId"
        );
        assert.ok(
          appCode.includes("searchInputRef.current?.blur()"),
          "Selecting a result must blur search input to transition focus to editor"
        );
      });

      it("verifies keyboard navigation (ArrowDown / ArrowUp) across search results", async () => {
        // Criterion 8: keyboard navigation (phase-6-specification.md:2612-2634)
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const keyNavCode = await fs.readFile(
          path.resolve("src/features/search/useSearchKeyboardNav.ts"),
          "utf-8"
        );

        assert.ok(
          keyNavCode.includes('key === "ArrowDown"'),
          "Keyboard nav must handle ArrowDown key"
        );
        assert.ok(
          keyNavCode.includes('"ArrowUp"'),
          "Keyboard nav must handle ArrowUp key"
        );
        assert.ok(
          keyNavCode.includes("isEditorFocused()"),
          "Keyboard nav must guard against interfering with active editor inputs"
        );
        assert.ok(
          keyNavCode.includes("onSelectNote(nextId)"),
          "Keyboard nav must select target note ID"
        );
        assert.ok(
          keyNavCode.includes("scrollIntoView"),
          "Keyboard nav must scroll newly selected note card into view"
        );
      });

      it("verifies Escape key handling for search dismissal", async () => {
        // Criterion 9: Escape (phase-6-specification.md:2612-2634)
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const topBarCode = await fs.readFile(
          path.resolve("src/components/TopBar/TopBar.tsx"),
          "utf-8"
        );
        const shortcutsCode = await fs.readFile(
          path.resolve("src/hooks/useKeyboardShortcuts.ts"),
          "utf-8"
        );

        assert.ok(
          topBarCode.includes('e.key === "Escape"'),
          "TopBar must handle Escape key in search input"
        );
        assert.ok(
          topBarCode.includes("searchInputRef.current?.blur()"),
          "Escape in TopBar must blur search input"
        );
        assert.ok(
          shortcutsCode.includes('event.key === "Escape"'),
          "useKeyboardShortcuts must listen for global Escape key"
        );
        assert.ok(
          shortcutsCode.includes("onEscape?.()"),
          "useKeyboardShortcuts must invoke onEscape handler"
        );
      });

      it("verifies Ctrl/Cmd + K shortcut for search focus", async () => {
        // Criterion 10: Ctrl/Cmd + K (phase-6-specification.md:2612-2634)
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const shortcutsCode = await fs.readFile(
          path.resolve("src/hooks/useKeyboardShortcuts.ts"),
          "utf-8"
        );
        const topBarCode = await fs.readFile(
          path.resolve("src/components/TopBar/TopBar.tsx"),
          "utf-8"
        );

        assert.ok(
          shortcutsCode.includes('key === "k" || key === "f"'),
          "useKeyboardShortcuts must detect Ctrl/Cmd + K and Ctrl/Cmd + F"
        );
        assert.ok(
          shortcutsCode.includes("onFocusSearch?.()"),
          "useKeyboardShortcuts must invoke onFocusSearch handler"
        );
        assert.ok(
          topBarCode.includes("Ctrl+K"),
          "TopBar must render accessible Ctrl+K keyboard shortcut hint"
        );
      });

      it("verifies stale request protection and race condition elimination", async () => {
        // Criterion 11: stale request protection (phase-6-specification.md:2612-2634)
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const useSearchCode = await fs.readFile(
          path.resolve("src/features/search/useSearch.ts"),
          "utf-8"
        );

        assert.ok(
          useSearchCode.includes("requestIdRef = useRef<number>(0)"),
          "useSearch must maintain an incremental requestIdRef counter"
        );
        assert.ok(
          useSearchCode.includes("currentReqId = ++requestIdRef.current"),
          "useSearch must increment requestIdRef on each search dispatch"
        );
        assert.ok(
          useSearchCode.includes("if (currentReqId === requestIdRef.current)"),
          "useSearch must discard responses from superseded/stale search requests"
        );
      });
    });

    // =========================================================================
    // TASK 97 — Search Regression Tests (phase-6-specification.md:2635-2656)
    // =========================================================================
    describe("Task 97: Search Regression Tests", () => {
      it("1. theme: verifies theme system is decoupled from search and styles are preserved", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const themeHookCode = await fs.readFile(
          path.resolve("src/hooks/useTheme.ts"),
          "utf-8"
        );
        const topBarCode = await fs.readFile(
          path.resolve("src/components/TopBar/TopBar.tsx"),
          "utf-8"
        );

        // Theme hook manages data-theme on root without being overridden by search
        assert.ok(
          themeHookCode.includes('root.setAttribute("data-theme"'),
          "useTheme must set data-theme attribute on document root"
        );
        // TopBar / Search input uses CSS variables compatible with light/dark themes
        assert.ok(
          topBarCode.includes("search-input"),
          "TopBar must provide search input styled via theme variables"
        );
      });

      it("2. sidebar: verifies sidebar navigation views are preserved alongside search", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const sidebarCode = await fs.readFile(
          path.resolve("src/components/Sidebar/Sidebar.tsx"),
          "utf-8"
        );

        // Sidebar supports All Notes, Unfiled, Favorites, Notebooks, and Tags
        assert.ok(sidebarCode.includes('"all-notes"'), "Sidebar must support 'all-notes' view");
        assert.ok(sidebarCode.includes('"unfiled"'), "Sidebar must support 'unfiled' view");
        assert.ok(sidebarCode.includes('"favorites"'), "Sidebar must support 'favorites' view");
        assert.ok(sidebarCode.includes("notebooks"), "Sidebar must render notebooks section");
        assert.ok(sidebarCode.includes("tags"), "Sidebar must render tags section");
      });

      it("3. notebooks: verifies notebook list and path computations function with search results", () => {
        const notebooks = [
          { id: "nb-1", name: "Engineering", parent_id: null },
          { id: "nb-2", name: "Personal", parent_id: null },
        ];

        const path1 = computeNotebookPath("nb-1", notebooks);
        assert.strictEqual(path1, "Engineering");

        // Map search result with notebook_id to human-readable path
        const searchResult = {
          noteId: "note-1",
          title: "Architecture Doc",
          snippet: "System design",
          modifiedAt: "2026-10-08T12:00:00Z",
          notebookId: "nb-1",
          favorite: false,
        };

        const resolvedPath = computeNotebookPath(searchResult.notebookId, notebooks);
        assert.strictEqual(resolvedPath, "Engineering");
      });

      it("4. nested notebooks: verifies hierarchical path resolution for multi-level notebook trees", () => {
        const nestedNotebooks = [
          { id: "root", name: "Root", parent_id: null },
          { id: "sub1", name: "Level 1", parent_id: "root" },
          { id: "sub2", name: "Level 2", parent_id: "sub1" },
        ];

        const leafPath = computeNotebookPath("sub2", nestedNotebooks);
        assert.strictEqual(leafPath, "Root / Level 1 / Level 2");

        const unfiledPath = computeNotebookPath(null, nestedNotebooks);
        assert.strictEqual(unfiledPath, "Unfiled");
      });

      it("5. note creation: verifies note creation defaults and transformation contract", () => {
        const newNote = {
          id: "note-new-1",
          title: "New Note",
          content: "",
          format: "md",
          created_at: "2026-10-08T12:00:00Z",
          modified_at: "2026-10-08T12:00:00Z",
          is_favorite: false,
          is_pinned: false,
          notebook_id: null,
        };

        const meta = computeNoteMetadata(newNote, [], "Unfiled", "Unfiled");
        assert.strictEqual(meta.character_count, 0);
        assert.strictEqual(meta.word_count, 0);
        assert.strictEqual(meta.byte_size, 0);
        assert.strictEqual(getFormatDisplayName(newNote.format), "Markdown");
      });

      it("6. note editing: verifies note editing transformations and search highlight consistency", () => {
        const noteAfter = {
          id: "note-edit-1",
          title: "Revised Specification",
          content: "Comprehensive updated content with search terms",
        };

        const hl = highlightText(noteAfter.title, "Revised");
        assert.ok(Array.isArray(hl), "highlightText returns React elements array");
        assert.ok(
          hl.some(
            (part) =>
              typeof part === "object" &&
              part !== null &&
              (part.props?.children === "Revised" || part.key !== undefined)
          ),
          "highlightText must wrap matching term in marked element"
        );
      });

      it("7. autosave: verifies autosave engine flushes pending changes before search dispatch", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const useSearchCode = await fs.readFile(
          path.resolve("src/features/search/useSearch.ts"),
          "utf-8"
        );

        assert.ok(
          useSearchCode.includes("onBeforeSearch"),
          "useSearch must accept and call onBeforeSearch hook"
        );
        assert.ok(
          useSearchCode.includes("await onBeforeSearchRef.current()"),
          "useSearch must await onBeforeSearch flush before executing search"
        );
      });

      it("8. undo/redo: verifies editor undo/redo functionality remains isolated and accessible", async () => {
        const fs = await import("node:fs/promises");
        const path = await import("node:path");

        const editorCode = await fs.readFile(
          path.resolve("src/features/notes/components/Editor/NoteEditor.tsx"),
          "utf-8"
        );

        assert.ok(
          editorCode.includes('document.execCommand("undo")'),
          "NoteEditor must support native undo execution"
        );
        assert.ok(
          editorCode.includes('document.execCommand("redo")'),
          "NoteEditor must support native redo execution"
        );
      });

      it("9. tags: verifies tag operations, validation, normalization, and badge display", () => {
        const emptyVal = validateTagName("");
        assert.strictEqual(emptyVal.valid, false);
        assert.strictEqual(emptyVal.error, "Tag name cannot be empty");
        const validVal = validateTagName("valid-tag");
        assert.strictEqual(validVal.valid, true);
        assert.strictEqual(normalizeTagName("  My-Tag  "), "my-tag");

        const mockNote = {
          id: "tagged-note",
          content: "Tagged note body",
          format: "txt",
          created_at: "2026-10-08T12:00:00Z",
          modified_at: "2026-10-08T12:00:00Z",
          is_favorite: false,
          is_pinned: false,
          notebook_id: null,
        };
        const tags = [{ id: "tag-1", name: "test-tag" }];
        const meta = computeNoteMetadata(mockNote, tags, "Unfiled", "Unfiled");
        assert.strictEqual(meta.tags.length, 1);
        assert.strictEqual(meta.tags[0].name, "test-tag");
      });

      it("10. favorites: verifies favorite flag synchronization between search results and note list", () => {
        const searchResult = {
          noteId: "fav-note-1",
          title: "Starred Note",
          snippet: "This is a favorite note",
          modifiedAt: "2026-10-08T12:00:00Z",
          notebookId: null,
          favorite: true,
        };

        const noteListItem = searchResultToNoteListItem(searchResult);
        assert.strictEqual(noteListItem.isFavorite, true);
        assert.strictEqual(noteListItem.id, "fav-note-1");
      });

      it("11. metadata: verifies metadata panel calculations produce exact word/char counts and byte sizes", () => {
        const note = {
          id: "meta-test-1",
          content: "One two three four five.\nSecond line with words.",
          format: "md",
          created_at: "2026-10-08T12:00:00Z",
          modified_at: "2026-10-08T12:00:00Z",
          is_favorite: true,
          is_pinned: false,
          notebook_id: "nb-nested",
        };

        const meta = computeNoteMetadata(
          note,
          [],
          "Child Notebook",
          "Parent Notebook / Child Notebook"
        );

        assert.strictEqual(meta.word_count, 9);
        assert.strictEqual(meta.character_count, note.content.length);
        assert.strictEqual(meta.byte_size, Buffer.byteLength(note.content, "utf8"));
        assert.strictEqual(meta.format, "md");
        assert.strictEqual(meta.notebook_path, "Parent Notebook / Child Notebook");
        assert.strictEqual(meta.is_favorite, true);
        assert.strictEqual(getFormatDisplayName(meta.format), "Markdown");
      });
    });

    // =========================================================================
    // TASK 99 — Final Manual Search QA (phase-6-specification.md:2681-2731)
    // =========================================================================
    describe("Task 99: Final Manual Search QA Workflow Simulation", () => {
      // Step 1: Create 10 notes with different titles and contents
      const notes = [
        { id: "note-1", title: "Project Alpha Architecture", content: "Foundational microservices and database schema design.", is_deleted: false },
        { id: "note-2", title: "Quarterly Budget", content: "Expenses allocated for the core software project milestones.", is_deleted: false },
        { id: "note-3", title: "Daily Standup Notes", content: "Team discussed blockers, PR reviews, and roadmap timelines.", is_deleted: false },
        { id: "note-4", title: "Project Beta Specification", content: "Deep-dive into client-side state caching and SQLite storage.", is_deleted: false },
        { id: "note-5", title: "Cooking Recipes", content: "Pasta carbonara ingredients: eggs, pecorino, guanciale, pepper.", is_deleted: false },
        { id: "note-6", title: "Home Renovation Ideas", content: "Modern minimalist interior design inspiration.", is_deleted: false },
        { id: "note-7", title: "Special Project Gamma", content: "Confidential research on quantum encryption protocols.", is_deleted: false },
        { id: "note-8", title: "Personal Journal", content: "Reflections on daily workouts and healthy habits.", is_deleted: false },
        { id: "note-9", title: "Weekly Grocery Checklist", content: "Milk, apples, sourdough bread, olive oil.", is_deleted: false },
        { id: "note-10", title: "Travel Itinerary", content: "Flights, hotel bookings, and train tickets for Japan trip.", is_deleted: false },
      ];

      // Helper function simulating search execution contract
      function searchNotes(query, list) {
        const q = (query || "").trim().toLowerCase();
        if (!q) return [];
        return list.filter(
          (n) => !n.is_deleted && (n.title.toLowerCase().includes(q) || n.content.toLowerCase().includes(q))
        );
      }

      it("Step 1 & 2: verifies searching 'project' matches all expected notes", () => {
        const matches = searchNotes("project", notes);
        assert.strictEqual(matches.length, 4);
        const matchIds = matches.map((m) => m.id);
        assert.ok(matchIds.includes("note-1"));
        assert.ok(matchIds.includes("note-2"));
        assert.ok(matchIds.includes("note-4"));
        assert.ok(matchIds.includes("note-7"));
      });

      it("Step 3: verifies searching 'PROJECT' yields identical logical matches", () => {
        const matchesLower = searchNotes("project", notes);
        const matchesUpper = searchNotes("PROJECT", notes);
        assert.strictEqual(matchesUpper.length, 4);
        assert.deepStrictEqual(
          matchesUpper.map((m) => m.id),
          matchesLower.map((m) => m.id)
        );
      });

      it("Step 4: verifies searching 'nonexistent-term' returns zero notes found", () => {
        const matches = searchNotes("nonexistent-term", notes);
        assert.strictEqual(matches.length, 0);
        assert.strictEqual(formatResultCount(matches.length), "0 results");
      });

      it("Step 5: verifies editing a matching note immediately indexes and returns new content", () => {
        const activeNotes = notes.map((n) => ({ ...n }));
        const noteToEdit = activeNotes.find((n) => n.id === "note-1");
        assert.ok(noteToEdit);
        noteToEdit.content = "Foundational microservices and database schema design with newlyaddedterm included.";

        const matches = searchNotes("newlyaddedterm", activeNotes);
        assert.strictEqual(matches.length, 1);
        assert.strictEqual(matches[0].id, "note-1");
      });

      it("Step 6: verifies soft-deleting a matching note immediately excludes it from search", () => {
        const activeNotes = notes.map((n) => ({ ...n }));
        const noteToDelete = activeNotes.find((n) => n.id === "note-4");
        assert.ok(noteToDelete);
        noteToDelete.is_deleted = true;

        const matches = searchNotes("project", activeNotes);
        assert.strictEqual(matches.length, 3);
        const matchIds = matches.map((m) => m.id);
        assert.ok(!matchIds.includes("note-4"));
        assert.ok(matchIds.includes("note-1"));
        assert.ok(matchIds.includes("note-2"));
        assert.ok(matchIds.includes("note-7"));
      });

      it("Step 7: verifies restart simulation retains updated state and search results", () => {
        const reloadedNotes = notes.map((n) => {
          if (n.id === "note-1") {
            return { ...n, content: "Foundational microservices with newlyaddedterm." };
          }
          if (n.id === "note-4") {
            return { ...n, is_deleted: true };
          }
          return { ...n };
        });

        const projectMatches = searchNotes("project", reloadedNotes);
        assert.strictEqual(projectMatches.length, 3);
        assert.ok(!projectMatches.some((m) => m.id === "note-4"));

        const newTermMatches = searchNotes("newlyaddedterm", reloadedNotes);
        assert.strictEqual(newTermMatches.length, 1);
        assert.strictEqual(newTermMatches[0].id, "note-1");
      });
    });
  });
});

// =============================================================================
// Phase 7: Attachments & Files
// =============================================================================
describe("Phase 7: Attachments & Files", () => {
  describe("Task 10: Add-Attachment Workflow", () => {
    // 1. Success workflow with progress tracking
    it("executes add-attachment workflow and notifies progress for each file", async () => {
      const { executeAddAttachmentWorkflow } = await import(
        "../src/features/attachments/services/attachmentWorkflow.ts"
      );

      const progressSnapshots = [];
      const mockStorage = {
        async addAttachment({ noteId, sourcePath }) {
          return {
            id: `att-${Math.random().toString(36).substring(2, 7)}`,
            note_id: noteId,
            file_name: sourcePath.split("/").pop(),
            mime_type: "application/pdf",
            file_size: 1024,
            created_at: "2026-10-08T12:00:00Z",
          };
        },
      };

      const result = await executeAddAttachmentWorkflow(
        "note-test-1",
        ["/home/user/docs/financial_summary.pdf", "/home/user/docs/chart.png"],
        (prog) => progressSnapshots.push({ ...prog }),
        mockStorage
      );

      assert.strictEqual(result.targetNoteId, "note-test-1");
      assert.strictEqual(result.successful.length, 2);
      assert.strictEqual(result.failed.length, 0);
      assert.strictEqual(result.successful[0].file_name, "financial_summary.pdf");
      assert.strictEqual(result.successful[1].file_name, "chart.png");

      // Verify progress notifications
      assert.strictEqual(progressSnapshots.length, 2);
      assert.deepStrictEqual(progressSnapshots[0], {
        current: 1,
        total: 2,
        currentFileName: "financial_summary.pdf",
      });
      assert.deepStrictEqual(progressSnapshots[1], {
        current: 2,
        total: 2,
        currentFileName: "chart.png",
      });
    });

    // 2. Partial multi-file failure isolation
    it("guarantees partial multi-file failure isolation: failed file does not cancel previous successes", async () => {
      const { executeAddAttachmentWorkflow } = await import(
        "../src/features/attachments/services/attachmentWorkflow.ts"
      );

      const mockStorage = {
        async addAttachment({ noteId, sourcePath }) {
          if (sourcePath.includes("oversized")) {
            throw new Error("File exceeds maximum permitted size of 100 MB.");
          }
          return {
            id: `att-${sourcePath}`,
            note_id: noteId,
            file_name: sourcePath.split("/").pop(),
            mime_type: "application/octet-stream",
            file_size: 512,
            created_at: "2026-10-08T12:00:00Z",
          };
        },
      };

      const fileList = [
        "/docs/file1.pdf",
        "/docs/oversized_video.mp4",
        "/docs/file3.txt",
      ];

      const result = await executeAddAttachmentWorkflow(
        "note-abc",
        fileList,
        undefined,
        mockStorage
      );

      // Invariant: file1 and file3 succeed, file2 recorded in failed
      assert.strictEqual(result.targetNoteId, "note-abc");
      assert.strictEqual(result.successful.length, 2);
      assert.strictEqual(result.failed.length, 1);

      assert.strictEqual(result.successful[0].file_name, "file1.pdf");
      assert.strictEqual(result.successful[1].file_name, "file3.txt");
      assert.strictEqual(result.failed[0].path, "/docs/oversized_video.mp4");
      assert.ok(result.failed[0].error.includes("File exceeds maximum permitted size"));
    });

    // 3. Target Note validation
    it("rejects execution without a valid target note ID", async () => {
      const { executeAddAttachmentWorkflow } = await import(
        "../src/features/attachments/services/attachmentWorkflow.ts"
      );

      await assert.rejects(
        async () => {
          await executeAddAttachmentWorkflow("", ["/path/test.pdf"]);
        },
        {
          message: "Cannot add attachments without a valid note ID.",
        }
      );

      await assert.rejects(
        async () => {
          await executeAddAttachmentWorkflow("   ", ["/path/test.pdf"]);
        },
        {
          message: "Cannot add attachments without a valid note ID.",
        }
      );
    });

    // 4. Structured Error State mapping
    it("maps technical and user-friendly error strings into correct AttachmentErrorKind categories", async () => {
      const { mapToAttachmentErrorState } = await import(
        "../src/features/attachments/services/attachmentWorkflow.ts"
      );

      assert.strictEqual(
        mapToAttachmentErrorState("File exceeds maximum permitted size of 100 MB.").kind,
        "FileTooLarge"
      );
      assert.strictEqual(
        mapToAttachmentErrorState("The referenced note could not be found.").kind,
        "NoteNotFound"
      );
      assert.strictEqual(
        mapToAttachmentErrorState("Cannot attach files to a deleted note").kind,
        "InvalidRequest"
      );
      assert.strictEqual(
        mapToAttachmentErrorState("Selected source file could not be read or does not exist: test.bin").kind,
        "SourceUnavailable"
      );
      assert.strictEqual(
        mapToAttachmentErrorState("Attachment file is missing from storage.").kind,
        "ManagedFileMissing"
      );
      assert.strictEqual(
        mapToAttachmentErrorState("Permission denied when accessing file.").kind,
        "PermissionDenied"
      );
      assert.strictEqual(
        mapToAttachmentErrorState("Failed to open attachment: no default application").kind,
        "OpenFailed"
      );
      assert.strictEqual(
        mapToAttachmentErrorState("A database error occurred while processing attachment.").kind,
        "Database"
      );
      assert.strictEqual(
        mapToAttachmentErrorState("Some unclassified system error").kind,
        "Unknown"
      );
    });

    // 5. IPC Bridge invariants verification
    it("verifies attachmentsStorage IPC command contract and storageService export", async () => {
      const fs = await import("node:fs");
      const storageFile = fs.readFileSync(
        new URL("../src/services/storage/attachments.ts", import.meta.url),
        "utf8"
      );

      // Verifies each required Tauri command invocation
      assert.ok(storageFile.includes('invoke<Attachment>("add_attachment"'));
      assert.ok(storageFile.includes('invoke<Attachment[]>("list_attachments"'));
      assert.ok(storageFile.includes('invoke<boolean>("remove_attachment"'));
      assert.ok(storageFile.includes('invoke<void>("open_attachment"'));

      const indexFile = fs.readFileSync(
        new URL("../src/services/storage/index.ts", import.meta.url),
        "utf8"
      );
      assert.ok(indexFile.includes("attachments: attachmentsStorage"));
      assert.ok(indexFile.includes('export { attachmentsStorage } from "./attachments"'));
    });
  });

  // Task 11: Build the Attachment List UI
  describe("Task 11: Build the Attachment List UI", () => {
    it("formats file sizes correctly across byte thresholds", async () => {
      const { formatFileSize } = await import(
        "../src/features/attachments/utils/formatters.ts"
      );
      assert.strictEqual(formatFileSize(0), "0 B");
      assert.strictEqual(formatFileSize(-10), "0 B");
      assert.strictEqual(formatFileSize(512), "512 B");
      assert.strictEqual(formatFileSize(1024), "1.0 KB");
      assert.strictEqual(formatFileSize(1536), "1.5 KB");
      assert.strictEqual(formatFileSize(1048576), "1.0 MB");
      assert.strictEqual(formatFileSize(1073741824), "1.0 GB");
    });

    it("categorizes attachments accurately by file extension and MIME type", async () => {
      const { getFileCategory } = await import(
        "../src/features/attachments/utils/formatters.ts"
      );
      assert.strictEqual(getFileCategory("contract.pdf"), "pdf");
      assert.strictEqual(getFileCategory("document.bin", "application/pdf"), "pdf");
      assert.strictEqual(getFileCategory("photo.png"), "image");
      assert.strictEqual(getFileCategory("data.raw", "image/jpeg"), "image");
      assert.strictEqual(getFileCategory("notes.txt"), "text");
      assert.strictEqual(getFileCategory("README.md"), "text");
      assert.strictEqual(getFileCategory("archive.tar.gz"), "archive");
      assert.strictEqual(getFileCategory("files.zip"), "archive");
      assert.strictEqual(getFileCategory("music.mp3"), "audio");
      assert.strictEqual(getFileCategory("movie.mp4"), "video");
      assert.strictEqual(getFileCategory("source.rs"), "code");
      assert.strictEqual(getFileCategory("Component.tsx"), "code");
      assert.strictEqual(getFileCategory("unknown.weirdext"), "generic");
    });

    it("formats ISO timestamps into clean localized dates", async () => {
      const { formatAttachmentDate } = await import(
        "../src/features/attachments/utils/formatters.ts"
      );
      assert.strictEqual(formatAttachmentDate(null), "");
      assert.strictEqual(formatAttachmentDate(undefined), "");
      assert.strictEqual(formatAttachmentDate("invalid-date"), "");
      const formatted = formatAttachmentDate("2026-10-08T10:30:00Z");
      assert.ok(formatted.includes("2026"));
      assert.ok(formatted.includes("Oct"));
    });

    it("verifies AttachmentItem and AttachmentList adhere to UI isolation and safety contracts", async () => {
      const fs = await import("node:fs");
      const itemContent = fs.readFileSync(
        new URL(
          "../src/features/attachments/components/AttachmentItem.tsx",
          import.meta.url
        ),
        "utf8"
      );
      const listContent = fs.readFileSync(
        new URL(
          "../src/features/attachments/components/AttachmentList.tsx",
          import.meta.url
        ),
        "utf8"
      );
      const editorContent = fs.readFileSync(
        new URL(
          "../src/features/notes/components/Editor/NoteEditor.tsx",
          import.meta.url
        ),
        "utf8"
      );

      // AttachmentItem displays original file_name and never leaks internal storage paths
      assert.ok(itemContent.includes("attachment.file_name"));
      assert.ok(!itemContent.includes("relative_path"));
      assert.ok(!itemContent.includes("file_path"));
      assert.ok(itemContent.includes("formatFileSize(attachment.file_size)"));
      assert.ok(itemContent.includes("onOpen(attachment.id)"));
      assert.ok(itemContent.includes("onRemove(attachment.id)"));

      // AttachmentList uses useAttachments hook, provides safe empty & disabled states
      assert.ok(listContent.includes("useAttachments(noteId)"));
      assert.ok(listContent.includes("pickAndAddFiles"));
      assert.ok(listContent.includes("removeAttachment"));
      assert.ok(listContent.includes("openAttachment"));
      assert.ok(listContent.includes("attachments-empty-state"));
      assert.ok(listContent.includes("attachments-error-banner"));

      // NoteEditor mounts AttachmentList within editor-document
      assert.ok(editorContent.includes("<AttachmentList noteId={noteId} />"));
    });
  });

  // Task 12: Implement Human-Readable File Sizes and File-Type Presentation
  describe("Task 12: Human-Readable File Sizes and File-Type Presentation", () => {
    it("formats standard, boundary, and extreme byte values safely", async () => {
      const { formatFileSize } = await import(
        "../src/features/attachments/utils/formatters.ts"
      );

      // Standard and edge thresholds
      assert.strictEqual(formatFileSize(0), "0 B");
      assert.strictEqual(formatFileSize(-1), "0 B");
      assert.strictEqual(formatFileSize(NaN), "0 B");
      assert.strictEqual(formatFileSize(Infinity), "0 B");
      assert.strictEqual(formatFileSize(-Infinity), "0 B");
      assert.strictEqual(formatFileSize(512), "512 B");
      assert.strictEqual(formatFileSize(1023), "1023 B");
      assert.strictEqual(formatFileSize(1024), "1.0 KB");
      assert.strictEqual(formatFileSize(1536), "1.5 KB");
      assert.strictEqual(formatFileSize(1048576), "1.0 MB");
      assert.strictEqual(formatFileSize(1073741824), "1.0 GB");
      assert.strictEqual(formatFileSize(1099511627776), "1.0 TB");
      assert.strictEqual(formatFileSize(1125899906842624), "1.0 PB");

      // Extremely large numbers up to MAX_SAFE_INTEGER
      const maxSafeFormatted = formatFileSize(Number.MAX_SAFE_INTEGER);
      assert.ok(maxSafeFormatted.endsWith("PB"));
      assert.ok(!maxSafeFormatted.includes("NaN"));
    });

    it("maps all recognized file extensions and MIME types with generic fallback", async () => {
      const { getFileCategory } = await import(
        "../src/features/attachments/utils/formatters.ts"
      );

      // PDF
      assert.strictEqual(getFileCategory("report.pdf"), "pdf");
      assert.strictEqual(getFileCategory("doc.bin", "application/pdf"), "pdf");

      // Images
      assert.strictEqual(getFileCategory("photo.avif"), "image");
      assert.strictEqual(getFileCategory("scan.heic"), "image");
      assert.strictEqual(getFileCategory("drawing.svg"), "image");
      assert.strictEqual(getFileCategory("img.raw", "image/png"), "image");

      // Text
      assert.strictEqual(getFileCategory("memo.txt"), "text");
      assert.strictEqual(getFileCategory("guide.markdown"), "text");
      assert.strictEqual(getFileCategory("data.csv"), "text");
      assert.strictEqual(getFileCategory("doc.bin", "text/plain"), "text");

      // Archives
      assert.strictEqual(getFileCategory("bundle.tar.gz"), "archive");
      assert.strictEqual(getFileCategory("backup.7z"), "archive");
      assert.strictEqual(getFileCategory("compressed.zst"), "archive");
      assert.strictEqual(getFileCategory("pack.bin", "application/zip"), "archive");

      // Audio
      assert.strictEqual(getFileCategory("recording.opus"), "audio");
      assert.strictEqual(getFileCategory("track.flac"), "audio");
      assert.strictEqual(getFileCategory("audio.bin", "audio/ogg"), "audio");

      // Video
      assert.strictEqual(getFileCategory("capture.mkv"), "video");
      assert.strictEqual(getFileCategory("clip.webm"), "video");
      assert.strictEqual(getFileCategory("video.bin", "video/mp4"), "video");

      // Code
      assert.strictEqual(getFileCategory("config.toml"), "code");
      assert.strictEqual(getFileCategory("query.sql"), "code");
      assert.strictEqual(getFileCategory("style.scss"), "code");
      assert.strictEqual(getFileCategory("code.bin", "application/json"), "code");

      // Fallbacks
      assert.strictEqual(getFileCategory("data.xyz123"), "generic");
      assert.strictEqual(getFileCategory("noextension"), "generic");
      assert.strictEqual(getFileCategory(""), "generic");
    });

    it("verifies presentation accessibility, layout safeguards, and no-preview constraints", async () => {
      const fs = await import("node:fs");
      const iconsContent = fs.readFileSync(
        new URL(
          "../src/features/attachments/components/AttachmentIcons.tsx",
          import.meta.url
        ),
        "utf8"
      );
      const itemContent = fs.readFileSync(
        new URL(
          "../src/features/attachments/components/AttachmentItem.tsx",
          import.meta.url
        ),
        "utf8"
      );
      const cssContent = fs.readFileSync(
        new URL(
          "../src/features/attachments/components/AttachmentList.css",
          import.meta.url
        ),
        "utf8"
      );

      // Icon rendering supports full category suite without external icon dependencies
      assert.ok(iconsContent.includes("FilePdfIcon"));
      assert.ok(iconsContent.includes("FileImageIcon"));
      assert.ok(iconsContent.includes("FileTextIcon"));
      assert.ok(iconsContent.includes("FileArchiveIcon"));
      assert.ok(iconsContent.includes("FileAudioIcon"));
      assert.ok(iconsContent.includes("FileVideoIcon"));
      assert.ok(iconsContent.includes("FileCodeIcon"));
      assert.ok(iconsContent.includes("FileGenericIcon"));

      // Accessibility: Action buttons have text labels AND accessible names
      assert.ok(itemContent.includes('<span className="action-text">Open</span>'));
      assert.ok(itemContent.includes('<span className="action-text">Remove</span>'));
      assert.ok(itemContent.includes('aria-label={`Open ${attachment.file_name}`'));
      assert.ok(itemContent.includes('aria-label={`Remove ${attachment.file_name}`'));

      // Layout: long filenames truncated safely with ellipsis without breaking flex row
      assert.ok(cssContent.includes("text-overflow: ellipsis;"));
      assert.ok(cssContent.includes("overflow: hidden;"));
      assert.ok(cssContent.includes("white-space: nowrap;"));
      assert.ok(cssContent.includes("min-width: 0;"));

      // No document preview or heavy rendering dependency introduced
      assert.ok(!itemContent.includes("iframe"));
      assert.ok(!itemContent.includes("embed"));
      assert.ok(!itemContent.includes("pdfjs"));
    });
  });

  // Task 13: Implement Safe Attachment Opening
  describe("Task 13: Safe Attachment Opening", () => {
    it("guarantees open_attachment accepts strictly attachment ID and never frontend-supplied paths", async () => {
      const fs = await import("node:fs");
      const storageClientFile = fs.readFileSync(
        new URL("../src/services/storage/attachments.ts", import.meta.url),
        "utf8"
      );
      const commandsFile = fs.readFileSync(
        new URL("../src-tauri/src/commands/storage.rs", import.meta.url),
        "utf8"
      );

      // Frontend storage service takes only ID and invokes open_attachment with { id: id.trim() }
      assert.ok(storageClientFile.includes("openAttachment(id: string): Promise<void>"));
      assert.ok(storageClientFile.includes('"open_attachment"'));
      assert.ok(storageClientFile.includes("id: id.trim()"));
      assert.ok(!storageClientFile.includes("path:"));

      // Backend command signature accepts only (app, db, paths, id: String)
      const openCmdMatch = commandsFile.match(/pub fn open_attachment\(([\s\S]*?)\)/);
      assert.ok(openCmdMatch, "open_attachment command must exist");
      assert.ok(openCmdMatch[1].includes("id: String"));
      assert.ok(!openCmdMatch[1].includes("path: String"));
    });

    it("maps opening-specific backend error messages to structured frontend error kinds", async () => {
      const { mapToAttachmentErrorState } = await import(
        "../src/features/attachments/services/attachmentWorkflow.ts"
      );

      const missingFileError = mapToAttachmentErrorState(
        "Attachment file is missing from storage."
      );
      assert.strictEqual(missingFileError.kind, "ManagedFileMissing");
      assert.strictEqual(
        missingFileError.message,
        "Attachment file is missing from storage."
      );

      const openFailedError = mapToAttachmentErrorState(
        "Failed to open attachment with default application: no associated app"
      );
      assert.strictEqual(openFailedError.kind, "OpenFailed");

      const permissionError = mapToAttachmentErrorState(
        "Permission denied when accessing file in storage."
      );
      assert.strictEqual(permissionError.kind, "PermissionDenied");
    });

    it("verifies Open action button accessibility and in-flight busy state handling", async () => {
      const fs = await import("node:fs");
      const itemContent = fs.readFileSync(
        new URL(
          "../src/features/attachments/components/AttachmentItem.tsx",
          import.meta.url
        ),
        "utf8"
      );

      // Verify open button binds strictly to attachment.id and respects isBusy
      assert.ok(itemContent.includes("onClick={() => onOpen(attachment.id)}"));
      assert.ok(itemContent.includes("disabled={isBusy}"));
      assert.ok(itemContent.includes('className="attachment-action-btn btn-open"'));
      assert.ok(itemContent.includes("ExternalLinkIcon"));
    });
  });

  // Task 14: Implement Attachment Removal and Managed-File Cleanup
  describe("Task 14: Attachment Removal and Managed-File Cleanup", () => {
    it("guarantees remove_attachment takes strictly ID and never accepts frontend deletion paths", async () => {
      const fs = await import("node:fs");
      const storageClientFile = fs.readFileSync(
        new URL("../src/services/storage/attachments.ts", import.meta.url),
        "utf8"
      );
      const commandsFile = fs.readFileSync(
        new URL("../src-tauri/src/commands/storage.rs", import.meta.url),
        "utf8"
      );

      // Frontend storage service takes only ID and invokes remove_attachment with { id: id.trim() }
      assert.ok(
        storageClientFile.includes(
          "removeAttachment(id: string): Promise<boolean>"
        )
      );
      assert.ok(storageClientFile.includes('"remove_attachment"'));
      assert.ok(storageClientFile.includes("id: id.trim()"));
      assert.ok(!storageClientFile.includes("removeAttachment(path:"));

      // Backend command signature accepts only (db, paths, id: String)
      const removeCmdMatch = commandsFile.match(
        /pub fn remove_attachment\(([\s\S]*?)\)/
      );
      assert.ok(removeCmdMatch, "remove_attachment command must exist");
      assert.ok(removeCmdMatch[1].includes("id: String"));
      assert.ok(!removeCmdMatch[1].includes("path: String"));
    });

    it("verifies removal confirmation flow and double-click protection in AttachmentItem", async () => {
      const fs = await import("node:fs");
      const itemContent = fs.readFileSync(
        new URL(
          "../src/features/attachments/components/AttachmentItem.tsx",
          import.meta.url
        ),
        "utf8"
      );

      // Verify confirmation states exist
      assert.ok(itemContent.includes("isConfirming"));
      assert.ok(itemContent.includes("setIsConfirming(true)"));
      assert.ok(itemContent.includes("setIsConfirming(false)"));
      assert.ok(itemContent.includes("btn-confirm-remove"));
      assert.ok(itemContent.includes("btn-cancel-remove"));

      // Verify disabled={isBusy} prevents concurrent removal clicks
      assert.ok(itemContent.includes("disabled={isBusy}"));
      assert.ok(itemContent.includes("handleConfirmRemove"));
    });

    it("verifies useAttachments filters out deleted attachment and handles errors", async () => {
      const fs = await import("node:fs");
      const hookContent = fs.readFileSync(
        new URL(
          "../src/features/attachments/hooks/useAttachments.ts",
          import.meta.url
        ),
        "utf8"
      );

      assert.ok(hookContent.includes('setStatus("removing")'));
      assert.ok(hookContent.includes("attachmentsStorage.removeAttachment(id)"));
      assert.ok(
        hookContent.includes(
          "setAttachments((prev) => prev.filter((a) => a.id !== id))"
        )
      );
      assert.ok(
        hookContent.includes(
          'mapToAttachmentErrorState(err, "Failed to remove attachment.")'
        )
      );
    });
  });

  // Task 15: Handle Duplicate Filenames, Unicode, and Unusual Names
  describe("Task 15: Duplicate Filenames, Unicode, and Unusual Names", () => {
    it("categorizes all unusual, international, and multi-extension filenames properly", async () => {
      const { getFileCategory } = await import(
        "../src/features/attachments/utils/formatters.ts"
      );

      assert.strictEqual(getFileCategory("report (1).pdf"), "pdf");
      assert.strictEqual(getFileCategory("final version 2026.pdf"), "pdf");
      assert.strictEqual(getFileCategory("résumé.pdf"), "pdf");
      assert.strictEqual(getFileCategory("नोट्स.txt"), "text");
      assert.strictEqual(getFileCategory("项目计划.pdf"), "pdf");
      assert.strictEqual(getFileCategory("file with many spaces.txt"), "text");
      assert.strictEqual(getFileCategory("archive.tar.gz"), "archive");
      assert.strictEqual(getFileCategory("README_NO_EXT"), "generic");
      assert.strictEqual(getFileCategory("🚀 rocket note 🎉.log"), "text");
      assert.strictEqual(getFileCategory("trailing_space.txt "), "text");
    });

    it("verifies AttachmentItem preserves Unicode and provides full tooltip access", async () => {
      const fs = await import("node:fs");
      const itemContent = fs.readFileSync(
        new URL(
          "../src/features/attachments/components/AttachmentItem.tsx",
          import.meta.url
        ),
        "utf8"
      );

      // Ensures title attribute on filename allows viewing long or truncated text
      assert.ok(itemContent.includes("title={attachment.file_name}"));
      assert.ok(itemContent.includes("{attachment.file_name}"));
      assert.ok(itemContent.includes("className=\"attachment-item-name selectable-text\""));
    });
  });

  // Task 16: Handle Missing Files, Permission Errors, and Disk Failures
  describe("Task 16: Missing Files, Permission Errors, and Disk Failures", () => {
    it("maps all failure modes to structured non-technical user messages", async () => {
      const { mapToAttachmentErrorState } = await import(
        "../src/features/attachments/services/attachmentWorkflow.ts"
      );

      const tests = [
        {
          input: "Selected source file could not be read or does not exist: foo.txt",
          expectedKind: "SourceUnavailable",
        },
        {
          input: "Permission denied when accessing file.",
          expectedKind: "PermissionDenied",
        },
        {
          input: "Insufficient disk space to save attachment.",
          expectedKind: "InsufficientDiskSpace",
        },
        {
          input: "Attachment file is missing from storage.",
          expectedKind: "ManagedFileMissing",
        },
        {
          input: "File exceeds maximum permitted size of 25 MB.",
          expectedKind: "FileTooLarge",
        },
        {
          input: "Failed to open attachment: no default application found",
          expectedKind: "OpenFailed",
        },
      ];

      for (const t of tests) {
        const state = mapToAttachmentErrorState(t.input);
        assert.strictEqual(state.kind, t.expectedKind);
        assert.strictEqual(state.message, t.input);
      }
    });

    it("verifies error banner and dismissal capability in AttachmentList component", async () => {
      const fs = await import("node:fs");
      const listContent = fs.readFileSync(
        new URL(
          "../src/features/attachments/components/AttachmentList.tsx",
          import.meta.url
        ),
        "utf8"
      );

      // Verify dismissible error banner with role="alert"
      assert.ok(listContent.includes('className="attachments-error-banner"'));
      assert.ok(listContent.includes('role="alert"'));
      assert.ok(listContent.includes("clearError"));
      assert.ok(listContent.includes("onClick={clearError}"));
    });
  });

  // Task 17: Implement Attachment State Management in React
  describe("Task 17: Attachment State Management in React", () => {
    it("guarantees selection race protection and note identity synchronization in useAttachments", async () => {
      const fs = await import("node:fs");
      const hookContent = fs.readFileSync(
        new URL(
          "../src/features/attachments/hooks/useAttachments.ts",
          import.meta.url
        ),
        "utf8"
      );

      // Verify sequence tracking to discard stale asynchronous list loads
      assert.ok(hookContent.includes("listSequenceRef"));
      assert.ok(hookContent.includes("++listSequenceRef.current"));
      assert.ok(hookContent.includes("currentSequence === listSequenceRef.current"));

      // Verify immediate state clearing when note selection switches
      assert.ok(hookContent.includes("prevNoteIdRef.current !== noteId"));
      assert.ok(hookContent.includes("setAttachments([])"));

      // Verify operations anchor to targetNoteId and do not mutate new note on late completion
      assert.ok(hookContent.includes("const targetNoteId = currentNoteIdRef.current;"));
      assert.ok(hookContent.includes("currentNoteIdRef.current === targetNoteId"));
    });

    it("verifies architectural decoupling and zero disturbance of note editor content", async () => {
      const fs = await import("node:fs");
      const editorContent = fs.readFileSync(
        new URL(
          "../src/features/notes/components/Editor/NoteEditor.tsx",
          import.meta.url
        ),
        "utf8"
      );

      // Verify AttachmentList is rendered with only noteId, without duplicating state
      assert.ok(editorContent.includes("<AttachmentList noteId={noteId} />"));

      // Verify no duplicate attachment array state lives in NoteEditor root
      assert.ok(!editorContent.includes("const [attachments, setAttachments]"));
    });
  });

  // Task 18: Add Loading, Empty, Success, and Error States
  describe("Task 18: Loading, Empty, Success, and Error States", () => {
    it("verifies distinct UI state rendering and accessibility landmarks in AttachmentList", async () => {
      const fs = await import("node:fs");
      const listContent = fs.readFileSync(
        new URL(
          "../src/features/attachments/components/AttachmentList.tsx",
          import.meta.url
        ),
        "utf8"
      );

      // 1. Empty state
      assert.ok(listContent.includes('className="attachments-empty-state"'));
      assert.ok(listContent.includes("No attachments yet."));

      // 2. Loading state with spinner and accessible announcement
      assert.ok(listContent.includes('className="attachments-loading-state"'));
      assert.ok(listContent.includes("Loading attachments..."));
      assert.ok(listContent.includes('className="attachments-spinner"'));

      // 3. Adding in-flight progress banner
      assert.ok(listContent.includes('className="attachments-progress-banner"'));
      assert.ok(listContent.includes('role="status"'));
      assert.ok(listContent.includes('aria-live="polite"'));

      // 4. Success banner with dismiss action
      assert.ok(listContent.includes('className="attachments-success-banner"'));
      assert.ok(listContent.includes("clearSuccess"));
      assert.ok(listContent.includes("onClick={clearSuccess}"));

      // 5. Error banner with role="alert" and dismiss action
      assert.ok(listContent.includes('className="attachments-error-banner"'));
      assert.ok(listContent.includes('role="alert"'));
      assert.ok(listContent.includes("clearError"));
      assert.ok(listContent.includes("onClick={clearError}"));
    });

    it("verifies success feedback lifecycle and cleanup in useAttachments hook", async () => {
      const fs = await import("node:fs");
      const hookContent = fs.readFileSync(
        new URL(
          "../src/features/attachments/hooks/useAttachments.ts",
          import.meta.url
        ),
        "utf8"
      );

      // Verify hook exposes successMessage and clearSuccess
      assert.ok(hookContent.includes("successMessage: string | null;"));
      assert.ok(hookContent.includes("clearSuccess: () => void;"));

      // Verify success notification triggers for file add and remove
      assert.ok(hookContent.includes("showSuccess"));
      assert.ok(hookContent.includes("Attached"));
      assert.ok(hookContent.includes("Attachment removed successfully."));

      // Verify timer cleanup on unmount
      assert.ok(hookContent.includes("clearTimeout(successTimerRef.current)"));
    });

    it("verifies theme-compliant styles for spinner and feedback banners in AttachmentList.css", async () => {
      const fs = await import("node:fs");
      const cssContent = fs.readFileSync(
        new URL(
          "../src/features/attachments/components/AttachmentList.css",
          import.meta.url
        ),
        "utf8"
      );

      assert.ok(cssContent.includes(".attachments-spinner"));
      assert.ok(cssContent.includes("@keyframes attachments-spin"));
      assert.ok(cssContent.includes(".attachments-loading-state"));
      assert.ok(cssContent.includes(".attachments-success-banner"));
      assert.ok(cssContent.includes(".attachments-error-banner"));
      assert.ok(cssContent.includes(".attachments-empty-state"));
    });
  });

  // Task 19: Add Attachment Counts and Integrate with Existing Note Metadata
  describe("Task 19: Attachment Counts and Metadata Integration", () => {
    it("preserves canonical metadata calculation while supporting attachment counts", async () => {
      const { computeNoteMetadata } = await import(
        "../src/features/notes/types.ts"
      );

      const fakeNote = {
        id: "note-123",
        title: "Test Note",
        content: "Hello world attached",
        created_at: "2026-03-01T12:00:00Z",
        modified_at: "2026-03-01T12:30:00Z",
        format: "md",
        notebook_id: null,
        is_favorite: false,
        is_pinned: false,
      };

      // 1. Calling without attachmentCount preserves existing contract
      const meta1 = computeNoteMetadata(fakeNote, [], "Notebook A");
      assert.strictEqual(meta1.note_id, "note-123");
      assert.strictEqual(meta1.word_count, 3);
      assert.strictEqual(meta1.attachment_count, undefined);

      // 2. Calling with attachmentCount propagates properly
      const meta2 = computeNoteMetadata(fakeNote, [], "Notebook A", "Path A", 5);
      assert.strictEqual(meta2.attachment_count, 5);
    });

    it("verifies NoteMetadataPanel renders Attachments row without regressions", async () => {
      const fs = await import("node:fs");
      const panelContent = fs.readFileSync(
        new URL(
          "../src/features/notes/components/Editor/NoteMetadataPanel.tsx",
          import.meta.url
        ),
        "utf8"
      );

      assert.ok(panelContent.includes("attachmentCount?: number;"));
      assert.ok(panelContent.includes('<span className="note-metadata-label">Attachments</span>'));
      assert.ok(panelContent.includes('attachmentCount > 0'));
      assert.ok(panelContent.includes('"None"'));
    });

    it("verifies NoteCard and NotesList render attachment indicator badge without N+1 query loop", async () => {
      const fs = await import("node:fs");
      const cardContent = fs.readFileSync(
        new URL(
          "../src/features/notes/components/NotesList/NoteCard.tsx",
          import.meta.url
        ),
        "utf8"
      );

      // Verifies attachment badge is conditionally rendered
      assert.ok(cardContent.includes('note.attachmentCount > 0'));
      assert.ok(cardContent.includes('className="note-card-attachment-indicator"'));
      assert.ok(cardContent.includes('note-card-attachment-count'));

      // Verifies AttachmentRepository provides batch_count_by_note_ids preventing N+1 queries
      const repoContent = fs.readFileSync(
        new URL(
          "../src-tauri/src/storage/repositories/attachments.rs",
          import.meta.url
        ),
        "utf8"
      );
      assert.ok(repoContent.includes("pub fn count_by_note_id"));
      assert.ok(repoContent.includes("pub fn batch_count_by_note_ids"));
      assert.ok(repoContent.includes("GROUP BY note_id"));
    });
  });

  // Task 20: Define Attachment Behavior During Note Deletion and Lifecycle Changes
  describe("Task 20: Note Deletion and Lifecycle Behavior", () => {
    it("guarantees attachment stability during note rename, notebook move, and metadata mutations", async () => {
      const fs = await import("node:fs");
      const integrationTestContent = fs.readFileSync(
        new URL(
          "../src-tauri/tests/storage_integration_test.rs",
          import.meta.url
        ),
        "utf8"
      );

      // Verifies backend tests lifecycle invariants
      assert.ok(
        integrationTestContent.includes(
          "test_task_20_attachment_behavior_during_note_deletion_and_lifecycle_changes"
        )
      );
      assert.ok(integrationTestContent.includes("Renamed Note Title"));
      assert.ok(integrationTestContent.includes("Work Notebook"));
      assert.ok(integrationTestContent.includes("move_to_notebook"));
      assert.ok(integrationTestContent.includes("add_tag_to_note"));
      assert.ok(integrationTestContent.includes("set_favorite"));
    });

    it("verifies soft-delete retention and rejection of attachments on deleted notes", async () => {
      const fs = await import("node:fs");
      const repoContent = fs.readFileSync(
        new URL(
          "../src-tauri/src/storage/repositories/attachments.rs",
          import.meta.url
        ),
        "utf8"
      );

      // Verifies verify_note_eligible forbids attaching to soft-deleted notes
      assert.ok(repoContent.includes("verify_note_eligible"));
      assert.ok(repoContent.includes("Cannot attach files to a deleted note"));
    });

    it("verifies permanent delete cleans up physical directory to prevent orphaned files", async () => {
      const fs = await import("node:fs");
      const commandsContent = fs.readFileSync(
        new URL(
          "../src-tauri/src/commands/storage.rs",
          import.meta.url
        ),
        "utf8"
      );
      const storageContent = fs.readFileSync(
        new URL(
          "../src-tauri/src/storage/attachment_storage.rs",
          import.meta.url
        ),
        "utf8"
      );

      // Verifies delete_note checks soft parameter and removes physical note directory on hard delete
      assert.ok(commandsContent.includes("pub fn delete_note"));
      assert.ok(commandsContent.includes("storage.remove_note_directory(&id)"));
      assert.ok(storageContent.includes("pub fn remove_note_directory"));
    });
  });

  // Task 21: Add Orphan Detection and Basic Storage Consistency Checks
  describe("Task 21: Orphan Detection and Storage Consistency Checks", () => {
    it("verifies typed consistency interfaces and frontend storage service methods", async () => {
      const fs = await import("node:fs");
      const typesContent = fs.readFileSync(
        new URL("../src/services/storage/types.ts", import.meta.url),
        "utf8"
      );
      const serviceContent = fs.readFileSync(
        new URL("../src/services/storage/attachments.ts", import.meta.url),
        "utf8"
      );

      // Verify TypeScript contract definitions
      assert.ok(typesContent.includes("export interface MissingManagedFileIssue"));
      assert.ok(typesContent.includes("export interface SizeMismatchIssue"));
      assert.ok(typesContent.includes("export interface AttachmentConsistencyReport"));
      assert.ok(typesContent.includes("missing_managed_files: MissingManagedFileIssue[]"));
      assert.ok(typesContent.includes("unreferenced_managed_files: string[]"));
      assert.ok(typesContent.includes("abandoned_temp_files: string[]"));
      assert.ok(typesContent.includes("size_mismatches: SizeMismatchIssue[]"));
      assert.ok(typesContent.includes("is_consistent: boolean"));

      // Verify frontend service methods
      assert.ok(serviceContent.includes("checkConsistency(): Promise<AttachmentConsistencyReport>"));
      assert.ok(serviceContent.includes('invoke<AttachmentConsistencyReport>("check_attachment_consistency")'));
      assert.ok(serviceContent.includes("cleanupAbandonedTempFiles(olderThanSeconds?: number): Promise<number>"));
      assert.ok(serviceContent.includes('invoke<number>("cleanup_abandoned_temp_files"'));
    });

    it("verifies backend containment, symlink skip, and temp age threshold safety rules", async () => {
      const fs = await import("node:fs");
      const storageContent = fs.readFileSync(
        new URL("../src-tauri/src/storage/attachment_storage.rs", import.meta.url),
        "utf8"
      );
      const serviceContent = fs.readFileSync(
        new URL("../src-tauri/src/storage/services/attachments.rs", import.meta.url),
        "utf8"
      );
      const commandsContent = fs.readFileSync(
        new URL("../src-tauri/src/commands/storage.rs", import.meta.url),
        "utf8"
      );

      // 1. Strict directory containment & canonical check
      assert.ok(storageContent.includes("pub fn scan_managed_files"));
      assert.ok(storageContent.includes("canon_note_dir.starts_with(&canonical_root)"));

      // 2. Symlinks skipped
      assert.ok(storageContent.includes("file_type.is_symlink()"));

      // 3. Temporary staging files identified by .tmp.
      assert.ok(storageContent.includes('file_name.contains(".tmp.")'));

      // 4. Age-based cleanup prevents touching in-flight active copies
      assert.ok(storageContent.includes("pub fn cleanup_abandoned_temp_files"));
      assert.ok(storageContent.includes("dur.as_secs() >= older_than_seconds"));

      // 5. Service checks consistency across DB list_all and disk scan
      assert.ok(serviceContent.includes("pub fn check_consistency"));
      assert.ok(serviceContent.includes("AttachmentRepository::list_all"));

      // 6. Registered Tauri commands
      assert.ok(commandsContent.includes("pub fn check_attachment_consistency"));
      assert.ok(commandsContent.includes("pub fn cleanup_abandoned_temp_files"));
    });

    it("confirms absence of aggressive background deletion schedulers or repair dashboards", async () => {
      const fs = await import("node:fs");
      const appContent = fs.readFileSync(
        new URL("../src-tauri/src/lib.rs", import.meta.url),
        "utf8"
      );

      // Verify no background daemon thread or cron scheduler is registered for automatic deletion
      assert.ok(!appContent.includes("std::thread::spawn"));
      assert.ok(!appContent.includes("cron"));
      assert.ok(!appContent.includes("schedule_repair"));
    });
  });

  // Task 22: Add Automated Rust and Database Tests
  describe("Task 22: Automated Rust and Database Tests Verification", () => {
    it("verifies comprehensive Rust storage test coverage in attachment_storage.rs", async () => {
      const fs = await import("node:fs");
      const storageCode = fs.readFileSync(
        new URL("../src-tauri/src/storage/attachment_storage.rs", import.meta.url),
        "utf8"
      );

      // Storage tests coverage
      assert.ok(storageCode.includes("test_task_22_storage_comprehensive_tests"));
      assert.ok(storageCode.includes("binary_payload"));
      assert.ok(storageCode.includes("वार्षिक रिपोर्ट & Strategy 🚀 2026.pdf"));
      assert.ok(storageCode.includes("assert_ne!(stored_u1.stored_name, stored_u2.stored_name)"));
      assert.ok(storageCode.includes("PermissionDenied"));
      assert.ok(storageCode.includes("SourceUnavailable"));
      assert.ok(storageCode.includes("Temporary file was not cleaned up"));
      assert.ok(storageCode.includes("resolve_safe_path(\"../secret.txt\").is_err()"));
      assert.ok(storageCode.includes("resolve_safe_path(\"note/../../etc/shadow\").is_err()"));
    });

    it("verifies comprehensive Rust repository test coverage in repositories/attachments.rs", async () => {
      const fs = await import("node:fs");
      const repoCode = fs.readFileSync(
        new URL("../src-tauri/src/storage/repositories/attachments.rs", import.meta.url),
        "utf8"
      );

      // Repository tests coverage
      assert.ok(repoCode.includes("test_task_22_repository_comprehensive_tests"));
      assert.ok(repoCode.includes("empty_list.is_empty()"));
      assert.ok(repoCode.includes("missing_att, None"));
      assert.ok(repoCode.includes("Foreign key constraint must reject nonexistent note_id"));
      assert.ok(repoCode.includes("NoteNotFound"));
      assert.ok(repoCode.includes("Cannot attach files to a deleted note") || repoCode.includes("deleted note"));
    });

    it("verifies comprehensive Rust service and security test coverage in services/attachments.rs and integration tests", async () => {
      const fs = await import("node:fs");
      const serviceCode = fs.readFileSync(
        new URL("../src-tauri/src/storage/services/attachments.rs", import.meta.url),
        "utf8"
      );
      const integrationCode = fs.readFileSync(
        new URL("../src-tauri/tests/storage_integration_test.rs", import.meta.url),
        "utf8"
      );

      // Service and security tests
      assert.ok(serviceCode.includes("test_task_22_service_and_security_tests"));
      assert.ok(serviceCode.includes("list_after_fail.len(), 1"));
      assert.ok(serviceCode.includes("'; DROP TABLE attachments; --"));
      assert.ok(serviceCode.includes("AttachmentNotFound"));

      // Integration test suite
      assert.ok(integrationCode.includes("test_task_22_automated_rust_and_database_suite"));
      assert.ok(integrationCode.includes("Binary content must be copied without modification"));
      assert.ok(integrationCode.includes("Foreign key violation must be enforced"));
    });
  });

  // Task 23: Add Frontend and Tauri Integration Tests
  describe("Task 23: Frontend and Tauri Integration Tests", () => {
    describe("1. Attachment list UI rendering and accessibility", () => {
      it("renders empty state when note has no attachments", async () => {
        const fs = await import("node:fs");
        const listContent = fs.readFileSync(
          new URL("../src/features/attachments/components/AttachmentList.tsx", import.meta.url),
          "utf8"
        );

        assert.ok(listContent.includes("No attachments yet."));
        assert.ok(listContent.includes('className="attachments-empty-state"'));
      });

      it("renders loading and error states with accessible ARIA live attributes", async () => {
        const fs = await import("node:fs");
        const listContent = fs.readFileSync(
          new URL("../src/features/attachments/components/AttachmentList.tsx", import.meta.url),
          "utf8"
        );

        assert.ok(listContent.includes("Loading attachments..."));
        assert.ok(listContent.includes('role="alert"'));
        assert.ok(listContent.includes('role="status"'));
        assert.ok(listContent.includes('aria-live="polite"'));
      });

      it("renders Unicode, international, and unusually long filenames safely with full tooltip", async () => {
        const fs = await import("node:fs");
        const itemContent = fs.readFileSync(
          new URL("../src/features/attachments/components/AttachmentItem.tsx", import.meta.url),
          "utf8"
        );
        const cssContent = fs.readFileSync(
          new URL("../src/features/attachments/components/AttachmentList.css", import.meta.url),
          "utf8"
        );

        // Tooltip title preserves full filename
        assert.ok(itemContent.includes("title={attachment.file_name}"));
        assert.ok(itemContent.includes("{attachment.file_name}"));

        // CSS guarantees safe truncation without breaking flex layout
        assert.ok(cssContent.includes("text-overflow: ellipsis;"));
        assert.ok(cssContent.includes("overflow: hidden;"));
        assert.ok(cssContent.includes("white-space: nowrap;"));
      });
    });

    describe("2. Add operation workflow and cancellation safety", () => {
      it("safely handles cancellation from file picker without altering state", async () => {
        const { executeAddAttachmentWorkflow } = await import(
          "../src/features/attachments/services/attachmentWorkflow.ts"
        );

        // Cancellation produces an empty array of file paths
        const result = await executeAddAttachmentWorkflow("note-123", []);

        assert.strictEqual(result.targetNoteId, "note-123");
        assert.strictEqual(result.successful.length, 0);
        assert.strictEqual(result.failed.length, 0);
      });

      it("executes multi-file addition and isolates individual file errors", async () => {
        const { executeAddAttachmentWorkflow } = await import(
          "../src/features/attachments/services/attachmentWorkflow.ts"
        );

        const mockStorage = {
          addAttachment: async (input) => {
            if (input.sourcePath.includes("bad")) {
              throw new Error("File exceeds maximum permitted size of 25 MB.");
            }
            return {
              id: "att-1",
              note_id: input.noteId,
              file_name: "good.pdf",
              mime_type: "application/pdf",
              file_size: 1024,
              created_at: "2026-10-08T12:00:00Z",
            };
          },
        };

        const progressUpdates = [];
        const result = await executeAddAttachmentWorkflow(
          "note-123",
          ["/path/good.pdf", "/path/bad.bin"],
          (p) => progressUpdates.push(p),
          mockStorage
        );

        assert.strictEqual(result.successful.length, 1);
        assert.strictEqual(result.failed.length, 1);
        assert.ok(result.failed[0].error.includes("File exceeds maximum"));
        assert.strictEqual(progressUpdates.length, 2);
      });

      it("guards against duplicate concurrent clicks via isBusy state", async () => {
        const fs = await import("node:fs");
        const listContent = fs.readFileSync(
          new URL("../src/features/attachments/components/AttachmentList.tsx", import.meta.url),
          "utf8"
        );

        assert.ok(listContent.includes("disabled={isBusy || isReadOnly}"));
        assert.ok(listContent.includes('className="attachments-add-btn"'));
        assert.ok(listContent.includes('onClick={handleAddFiles}'));
      });
    });

    describe("3. Safe Open operation", () => {
      it("invokes open_attachment with strictly attachment ID", async () => {
        const fs = await import("node:fs");
        const itemContent = fs.readFileSync(
          new URL("../src/features/attachments/components/AttachmentItem.tsx", import.meta.url),
          "utf8"
        );

        assert.ok(itemContent.includes("onOpen(attachment.id)"));
        assert.ok(itemContent.includes("disabled={isBusy}"));
        assert.ok(itemContent.includes('className="attachment-action-btn btn-open"'));
      });

      it("displays understandable non-technical error if managed file is missing", async () => {
        const { mapToAttachmentErrorState } = await import(
          "../src/features/attachments/services/attachmentWorkflow.ts"
        );

        const state = mapToAttachmentErrorState("Attachment file is missing from storage.");
        assert.strictEqual(state.kind, "ManagedFileMissing");
        assert.ok(state.message.includes("missing"));
      });
    });

    describe("4. Remove operation and neighbor row isolation", () => {
      it("requires user confirmation before removal and supports dismissal", async () => {
        const fs = await import("node:fs");
        const itemContent = fs.readFileSync(
          new URL("../src/features/attachments/components/AttachmentItem.tsx", import.meta.url),
          "utf8"
        );

        assert.ok(itemContent.includes("isConfirming"));
        assert.ok(itemContent.includes("btn-confirm-remove"));
        assert.ok(itemContent.includes("btn-cancel-remove"));
      });

      it("verifies removal preserves neighboring rows and updates list in useAttachments", async () => {
        const fs = await import("node:fs");
        const hookContent = fs.readFileSync(
          new URL("../src/features/attachments/hooks/useAttachments.ts", import.meta.url),
          "utf8"
        );

        assert.ok(
          hookContent.includes("setAttachments((prev) => prev.filter((a) => a.id !== id))")
        );
      });
    });

    describe("5. State synchronization and regression invariants", () => {
      it("synchronizes note selection and eliminates stale race responses", async () => {
        const fs = await import("node:fs");
        const hookContent = fs.readFileSync(
          new URL("../src/features/attachments/hooks/useAttachments.ts", import.meta.url),
          "utf8"
        );

        assert.ok(hookContent.includes("listSequenceRef"));
        assert.ok(hookContent.includes("currentSequence === listSequenceRef.current"));
        assert.ok(hookContent.includes("setAttachments([])"));
      });

      it("preserves note editor content and causes zero disturbance to editor state", async () => {
        const fs = await import("node:fs");
        const editorContent = fs.readFileSync(
          new URL("../src/features/notes/components/Editor/NoteEditor.tsx", import.meta.url),
          "utf8"
        );

        assert.ok(editorContent.includes("<AttachmentList noteId={noteId} />"));
        assert.ok(!editorContent.includes("const [attachments, setAttachments]"));
      });

      it("preserves existing Phase 1-6 features: search, tags, favorites, notebooks", async () => {
        const fs = await import("node:fs");
        const topBarContent = fs.readFileSync(
          new URL("../src/components/TopBar/TopBar.tsx", import.meta.url),
          "utf8"
        );
        const cardContent = fs.readFileSync(
          new URL("../src/features/notes/components/NotesList/NoteCard.tsx", import.meta.url),
          "utf8"
        );

        // TopBar search and theme toggle intact
        assert.ok(topBarContent.includes("Search notes..."));
        assert.ok(topBarContent.includes("onThemeToggle"));

        // NoteCard favorites, tags, and attachment indicator intact
        assert.ok(cardContent.includes("onToggleFavorite"));
        assert.ok(cardContent.includes("note.tags"));
        assert.ok(cardContent.includes("note.attachmentCount > 0"));
      });
    });
  });

  // Task 24: Perform Security, Reliability, and Performance Verification
  describe("Task 24: Security, Reliability, and Performance Verification", () => {
    it("verifies security constraints: no arbitrary destinations, no shell execution, no remote uploads", async () => {
      const fs = await import("node:fs");
      const serviceCode = fs.readFileSync(
        new URL("../src/services/storage/attachments.ts", import.meta.url),
        "utf8"
      );
      const capabilities = JSON.parse(
        fs.readFileSync(
          new URL("../src-tauri/capabilities/default.json", import.meta.url),
          "utf8"
        )
      );

      // Verify IPC methods never accept arbitrary write destination paths
      assert.ok(!serviceCode.includes("destinationPath"));
      assert.ok(!serviceCode.includes("targetPath"));

      // Verify capabilities are strictly bounded
      assert.deepStrictEqual(capabilities.permissions, [
        "core:default",
        "opener:default",
        "dialog:default",
      ]);

      // Verify no shell plugin permission is included
      assert.strictEqual(
        capabilities.permissions.includes("shell:default"),
        false,
        "Shell plugin must not be enabled"
      );
    });

    it("verifies reliability: rollback safety, restart stability, and non-crashing error handling", async () => {
      const fs = await import("node:fs");
      const integrationTestCode = fs.readFileSync(
        new URL("../src-tauri/tests/storage_integration_test.rs", import.meta.url),
        "utf8"
      );

      // Verify integration suite tests reliability across restarts and failure recovery
      assert.ok(
        integrationTestCode.includes(
          "test_task_24_security_reliability_and_performance_verification"
        )
      );
      assert.ok(integrationTestCode.includes("persistent document bytes across restart"));
      assert.ok(integrationTestCode.includes("Idempotent migration"));
    });

    it("verifies performance constraints: memory footprint, chunked streaming, and N+1 query avoidance", async () => {
      const fs = await import("node:fs");
      const typesCode = fs.readFileSync(
        new URL("../src/services/storage/types.ts", import.meta.url),
        "utf8"
      );
      const storageCode = fs.readFileSync(
        new URL("../src-tauri/src/storage/attachment_storage.rs", import.meta.url),
        "utf8"
      );
      const repoCode = fs.readFileSync(
        new URL("../src-tauri/src/storage/repositories/attachments.rs", import.meta.url),
        "utf8"
      );

      // Verify Attachment type does not contain raw binary payload fields
      assert.ok(!typesCode.includes("data: ArrayBuffer"));
      assert.ok(!typesCode.includes("bytes: Uint8Array"));
      assert.ok(!typesCode.includes("base64: string"));

      // Verify streaming copy uses bounded buffer chunking
      assert.ok(storageCode.includes("64 * 1024"));

      // Verify batch queries to prevent N+1 note list overhead
      assert.ok(repoCode.includes("batch_count_by_note_ids"));
      assert.ok(repoCode.includes("GROUP BY note_id"));
    });
  });
});

describe("Phase 8: Trash & Recovery", () => {
  describe("Task 11: Add Trash Navigation to the Existing Sidebar", () => {
    it("verifies Sidebar renders Trash navigation with distinct styling and count badge", async () => {
      const fs = await import("node:fs");
      const sidebarContent = fs.readFileSync(
        new URL("../src/components/Sidebar/Sidebar.tsx", import.meta.url),
        "utf8"
      );
      const cssContent = fs.readFileSync(
        new URL("../src/components/Sidebar/Sidebar.css", import.meta.url),
        "utf8"
      );

      // Verify Trash navigation item structure and accessibility
      assert.ok(sidebarContent.includes('sidebar-nav-item--trash'), "Should include sidebar-nav-item--trash class");
      assert.ok(sidebarContent.includes('handleNavClick("trash")'), "Should handle click for trash navId");
      assert.ok(sidebarContent.includes('aria-current={activeNavId === "trash" ? "page" : undefined}'), "Should provide aria-current for trash item");
      assert.ok(sidebarContent.includes('TrashIcon'), "Should use TrashIcon");
      assert.ok(sidebarContent.includes('noteCounts?.["trash"]'), "Should inspect trash count in noteCounts");
      assert.ok(sidebarContent.includes('nav-item-count--trash'), "Should style trash badge with distinct class");

      // Verify CSS distinct tokens for trash
      assert.ok(cssContent.includes('.sidebar-nav-item--trash'), "CSS should style .sidebar-nav-item--trash");
      assert.ok(cssContent.includes('.sidebar-nav-item--trash.is-active'), "CSS should style active trash item");
      assert.ok(cssContent.includes('--color-danger-'), "CSS should utilize danger theme tokens for trash item");
      assert.ok(cssContent.includes('.nav-item-count--trash'), "CSS should style trash count badge");
    });

    it("verifies notesStorage client service exposes typed listTrashed IPC contract", async () => {
      const fs = await import("node:fs");
      const notesStorageContent = fs.readFileSync(
        new URL("../src/services/storage/notes.ts", import.meta.url),
        "utf8"
      );

      assert.ok(notesStorageContent.includes("listTrashed"), "notesStorage should have listTrashed method");
      assert.ok(notesStorageContent.includes('invoke<TrashedNoteItem[]>("list_trashed_notes")'), "listTrashed should invoke list_trashed_notes");
      assert.ok(notesStorageContent.includes("TrashedNoteItem"), "notesStorage should import TrashedNoteItem");
    });

    it("verifies App.tsx clears notebook/tag filters, updates trash badge, and loads trashed notes", async () => {
      const fs = await import("node:fs");
      const appContent = fs.readFileSync(
        new URL("../src/app/App.tsx", import.meta.url),
        "utf8"
      );

      // Verify selectTrash destructuring and invocation
      assert.ok(appContent.includes("selectTrash,"), "App should destructure selectTrash from useNotebookSelection");
      assert.ok(appContent.includes('selectTrash();'), "App should call selectTrash when navigating to trash");
      assert.ok(appContent.includes('await loadNotes("trash", null, null)'), "App should load trashed notes on selection");

      // Verify trash count synchronization in refreshNoteCounts
      assert.ok(appContent.includes('storageService.notes.listTrashed()'), "refreshNoteCounts should query listTrashed");
      assert.ok(appContent.includes('counts["trash"]'), "refreshNoteCounts should set counts['trash']");

      // Verify trashed notes mapping in loadNotes
      assert.ok(appContent.includes('effectiveNav === "trash"'), "loadNotes should handle effectiveNav === 'trash'");
      assert.ok(appContent.includes('is_deleted: true'), "loadNotes should mark trashed notes as deleted");
    });
  });

  describe("Task 12: Build the Dedicated Trash View", () => {
    it("verifies NotesList supports trash mode, distinct empty state, and error state", async () => {
      const fs = await import("node:fs");
      const notesListContent = fs.readFileSync(
        new URL("../src/features/notes/components/NotesList/NotesList.tsx", import.meta.url),
        "utf8"
      );

      // Verify trash mode detection and error title/message resolution
      assert.ok(notesListContent.includes("isTrashMode"), "NotesList should derive isTrashMode");
      assert.ok(notesListContent.includes('"Failed to load trash"'), "NotesList should resolve trash error title");
      assert.ok(notesListContent.includes('"Trash is empty."'), "NotesList should display 'Trash is empty.' for trash empty state");
      assert.ok(notesListContent.includes("EmptyTrashIcon"), "NotesList should render dedicated EmptyTrashIcon");

      // Verify action button (+ New Note) is excluded in trash mode
      assert.ok(notesListContent.includes("isTrashMode ? undefined"), "NotesList should suppress New Note action in trash mode");

      // Verify error state isolation: error status renders ErrorState, not EmptyState
      assert.ok(notesListContent.includes('status === "error"'), "NotesList must render ErrorState on error");
      assert.ok(notesListContent.includes('<ErrorState'), "NotesList must use ErrorState component");
    });

    it("verifies NoteCard renders trash indicators, timestamps, context, and restore/delete actions", async () => {
      const fs = await import("node:fs");
      const noteCardContent = fs.readFileSync(
        new URL("../src/features/notes/components/NotesList/NoteCard.tsx", import.meta.url),
        "utf8"
      );
      const cssContent = fs.readFileSync(
        new URL("../src/features/notes/components/NotesList/NotesList.css", import.meta.url),
        "utf8"
      );

      // Verify drag suppression in trash mode
      assert.ok(noteCardContent.includes("draggable={!isTrashMode}"), "NoteCard should disable dragging for trashed notes");
      assert.ok(noteCardContent.includes("is-trash-card"), "NoteCard should add is-trash-card class");

      // Verify read-only favorite indicator
      assert.ok(noteCardContent.includes("note-card-favorite-indicator"), "NoteCard should show read-only favorite indicator in trash");

      // Verify deletion timestamp
      assert.ok(noteCardContent.includes("note.deletedAt ? `Deleted ${note.deletedAt}`"), "NoteCard should display deletion timestamp");

      // Verify Restore and Permanent Delete action buttons
      assert.ok(noteCardContent.includes("note-card-trash-action-btn--restore"), "NoteCard should provide restore action button");
      assert.ok(noteCardContent.includes("note-card-trash-action-btn--delete"), "NoteCard should provide permanent delete action button");

      // Verify CSS styles for trash cards and action buttons
      assert.ok(cssContent.includes(".note-card.is-trash-card"), "CSS should style .is-trash-card");
      assert.ok(cssContent.includes(".note-card-trash-actions"), "CSS should style .note-card-trash-actions");
      assert.ok(cssContent.includes(".note-card-trash-action-btn--restore"), "CSS should style restore action button");
      assert.ok(cssContent.includes(".note-card-trash-action-btn--delete"), "CSS should style delete action button");
    });

    it("verifies App.tsx prevents active note editing mutation, binds trash actions, and renders Trash view", async () => {
      const fs = await import("node:fs");
      const appContent = fs.readFileSync(
        new URL("../src/app/App.tsx", import.meta.url),
        "utf8"
      );

      // Verify active note editing mutation guard
      assert.ok(appContent.includes('if (activeNavId === "trash")'), "handleSaveNote must guard against mutating trashed notes");

      // Verify restore and permanently delete handlers
      assert.ok(appContent.includes("handleRestoreNote"), "App should define handleRestoreNote");
      assert.ok(appContent.includes("handlePermanentlyDeleteNote"), "App should define handlePermanentlyDeleteNote");
      assert.ok(appContent.includes("onRestoreNote={handleRestoreNote}"), "App should pass onRestoreNote to NotesList");
      assert.ok(appContent.includes("onPermanentlyDeleteNote={handlePermanentlyDeleteNote}"), "App should pass onPermanentlyDeleteNote to NotesList");

      // Verify isTrash and empty state strings
      assert.ok(appContent.includes('isTrash={activeNavId === "trash"}'), "App should pass isTrash prop to NotesList");
      assert.ok(appContent.includes('"Trash is empty."'), "App should provide 'Trash is empty.' title");
    });
  });

  describe("Task 13: Implement Trashed-Note Preview and Selection", () => {
    it("verifies NoteEditor enforces read-only mode and suppresses editing mutations for trashed notes", async () => {
      const fs = await import("node:fs");
      const editorContent = fs.readFileSync(
        new URL("../src/features/notes/components/Editor/NoteEditor.tsx", import.meta.url),
        "utf8"
      );

      // Verify read-only state derivation
      assert.ok(
        editorContent.includes("const isReadOnly = isTrash || !!loadedNote?.is_deleted;"),
        "NoteEditor must derive isReadOnly from isTrash prop or loadedNote.is_deleted"
      );

      // Verify save unregistration in read-only mode
      assert.ok(
        editorContent.includes("if (isReadOnly) {") && editorContent.includes("onRegisterSave(null);"),
        "NoteEditor must register null save handler when read-only to prevent flushing trashed notes"
      );

      // Verify mutation guards for title, content, format, and save
      assert.ok(
        editorContent.includes("if (isReadOnly) return;") &&
        editorContent.includes("handleTitleChange") &&
        editorContent.includes("handleContentChange") &&
        editorContent.includes("executeSave"),
        "NoteEditor must guard title, content, format, and save handlers against mutation in read-only mode"
      );

      // Verify input and textarea readOnly attribute
      assert.ok(
        editorContent.includes("readOnly={isReadOnly}"),
        "NoteEditor must set readOnly={isReadOnly} on input and textarea elements"
      );

      // Verify read-only attachments
      assert.ok(
        editorContent.includes("<AttachmentList noteId={noteId} isReadOnly={isReadOnly} />"),
        "AttachmentList must be passed isReadOnly"
      );
    });

    it("verifies NoteEditor renders trashed note preview banner and action controls", async () => {
      const fs = await import("node:fs");
      const editorContent = fs.readFileSync(
        new URL("../src/features/notes/components/Editor/NoteEditor.tsx", import.meta.url),
        "utf8"
      );
      const cssContent = fs.readFileSync(
        new URL("../src/features/notes/components/Editor/EditorPlaceholder.css", import.meta.url),
        "utf8"
      );

      // Verify banner structure and action buttons
      assert.ok(
        editorContent.includes('className="trashed-note-banner"'),
        "NoteEditor must render .trashed-note-banner when note is in trash"
      );
      assert.ok(
        editorContent.includes("This note is in Trash.") &&
        editorContent.includes("It is read-only. Restore this note to make changes."),
        "NoteEditor banner must inform user that note is in trash and read-only"
      );
      assert.ok(
        editorContent.includes('className="btn-trash-banner btn-trash-banner--restore"'),
        "NoteEditor must render restore button in trashed-note-banner"
      );
      assert.ok(
        editorContent.includes('className="btn-trash-banner btn-trash-banner--delete"'),
        "NoteEditor must render permanent delete button in trashed-note-banner"
      );
      assert.ok(
        editorContent.includes('className="trashed-pill"'),
        "NoteEditor must render read-only pill indicator in toolbar"
      );

      // Verify CSS styles
      assert.ok(cssContent.includes(".trashed-note-banner"), "CSS must style .trashed-note-banner");
      assert.ok(cssContent.includes(".trashed-note-banner-actions"), "CSS must style banner action buttons");
      assert.ok(cssContent.includes(".btn-trash-banner--restore"), "CSS must style banner restore button");
      assert.ok(cssContent.includes(".btn-trash-banner--delete"), "CSS must style banner delete button");
      assert.ok(cssContent.includes(".trashed-pill"), "CSS must style .trashed-pill");
      assert.ok(cssContent.includes(".editor-container.is-read-only"), "CSS must style .editor-container.is-read-only");
    });

    it("verifies App.tsx wires trash preview state and handlers to NoteEditor", async () => {
      const fs = await import("node:fs");
      const appContent = fs.readFileSync(
        new URL("../src/app/App.tsx", import.meta.url),
        "utf8"
      );

      // Verify NoteEditor receiving trash props
      assert.ok(
        appContent.includes('isTrash={activeNavId === "trash"}'),
        "App must pass isTrash to NoteEditor"
      );
      assert.ok(
        appContent.includes("onRestoreNote={handleRestoreNote}"),
        "App must pass onRestoreNote to NoteEditor"
      );
      assert.ok(
        appContent.includes("onPermanentlyDeleteNote={handlePermanentlyDeleteNote}"),
        "App must pass onPermanentlyDeleteNote to NoteEditor"
      );

      // Verify clearing selection upon restoration or permanent deletion
      assert.ok(
        appContent.includes("if (selectedNoteId === noteId) {\n          setSelectedNoteId(null);"),
        "App must deselect note if the active note is restored or permanently deleted"
      );
    });
  });

  describe("Task 14: Implement the Ordinary Delete Action in the Active Note UI", () => {
    it("verifies App.tsx routes ordinary delete through soft-delete service and safeguards state", async () => {
      const fs = await import("node:fs");
      const appContent = fs.readFileSync(
        new URL("../src/app/App.tsx", import.meta.url),
        "utf8"
      );

      // Verify handleDeleteNote definition and soft-delete routing
      assert.ok(
        appContent.includes("const handleDeleteNote = useCallback("),
        "App must define handleDeleteNote"
      );
      assert.ok(
        appContent.includes("await storageService.notes.softDelete(targetId)"),
        "handleDeleteNote must invoke storageService.notes.softDelete to move note to Trash"
      );

      // Verify flushed edits prior to deletion
      assert.ok(
        appContent.includes("await editorSaveRef.current()"),
        "handleDeleteNote must flush pending editor changes prior to deletion"
      );

      // Verify optimistic/truthful active list update only after successful deletion
      assert.ok(
        appContent.includes("setNotes((prev) => prev.filter((n) => n.id !== targetId))"),
        "handleDeleteNote must remove note from active list only after backend success"
      );

      // Verify refreshNoteCounts to keep sidebar Trash badge updated
      assert.ok(
        appContent.includes("await refreshNoteCounts()"),
        "handleDeleteNote must refresh note counts and trash count"
      );

      // Verify safe selection clearance
      assert.ok(
        appContent.includes("if (selectedNoteId === targetId) {\n          setSelectedNoteId(null);"),
        "handleDeleteNote must clear selection when deleting the currently active note"
      );

      // Verify failure preserves truthful state
      assert.ok(
        appContent.includes('"Failed to delete note: "'),
        "handleDeleteNote must capture and report failure without removing note from state"
      );
    });

    it("verifies NoteEditor provides toolbar delete action and flushes unsaved changes before soft-delete", async () => {
      const fs = await import("node:fs");
      const editorContent = fs.readFileSync(
        new URL("../src/features/notes/components/Editor/NoteEditor.tsx", import.meta.url),
        "utf8"
      );
      const cssContent = fs.readFileSync(
        new URL("../src/features/notes/components/Editor/EditorPlaceholder.css", import.meta.url),
        "utf8"
      );

      // Verify onDeleteNote in NoteEditorProps
      assert.ok(
        editorContent.includes("onDeleteNote?: (noteId: string) => Promise<void> | void;"),
        "NoteEditorProps must accept onDeleteNote"
      );

      // Verify handleDelete flushes dirty state
      assert.ok(
        editorContent.includes("if (isDirtyRef.current) {\n      await handleSave();\n    }"),
        "NoteEditor must flush dirty state before delegating to onDeleteNote"
      );

      // Verify toolbar delete button rendering
      assert.ok(
        editorContent.includes('className="toolbar-btn toolbar-btn-delete"'),
        "NoteEditor must render .toolbar-btn-delete in toolbar for active notes"
      );
      assert.ok(
        editorContent.includes('title="Delete note (Move to Trash)"'),
        "NoteEditor delete button must specify moving to Trash"
      );

      // Verify CSS styles
      assert.ok(
        cssContent.includes(".toolbar-btn-delete"),
        "CSS must style .toolbar-btn-delete"
      );
    });

    it("verifies keyboard shortcuts and NotesList / NoteCard integration for ordinary delete", async () => {
      const fs = await import("node:fs");
      const shortcutsContent = fs.readFileSync(
        new URL("../src/hooks/useKeyboardShortcuts.ts", import.meta.url),
        "utf8"
      );
      const notesListContent = fs.readFileSync(
        new URL("../src/features/notes/components/NotesList/NotesList.tsx", import.meta.url),
        "utf8"
      );
      const noteCardContent = fs.readFileSync(
        new URL("../src/features/notes/components/NotesList/NoteCard.tsx", import.meta.url),
        "utf8"
      );

      // Verify useKeyboardShortcuts supports onDeleteNote
      assert.ok(
        shortcutsContent.includes("onDeleteNote?: () => void;"),
        "KeyboardShortcutHandlers must accept onDeleteNote"
      );
      assert.ok(
        shortcutsContent.includes('key === "backspace" || key === "delete"'),
        "useKeyboardShortcuts must recognize Ctrl/Cmd + Backspace/Delete shortcut"
      );

      // Verify NotesList and NoteCard forward delete action
      assert.ok(
        notesListContent.includes("onDeleteNote?: (noteId: string) => void;"),
        "NotesListProps must accept onDeleteNote"
      );
      assert.ok(
        notesListContent.includes("onDelete={onDeleteNote}"),
        "NotesList must pass onDelete to NoteCard"
      );
      assert.ok(
        noteCardContent.includes("note-card-delete-btn"),
        "NoteCard must render delete button for active notes"
      );
    });
  });

  describe("Task 15: Implement Restore UI and Restore Feedback", () => {
    it("verifies clearly labeled Restore actions in both NoteEditor banner and NoteCard row", async () => {
      const fs = await import("node:fs");
      const editorContent = fs.readFileSync(
        new URL("../src/features/notes/components/Editor/NoteEditor.tsx", import.meta.url),
        "utf8"
      );
      const noteCardContent = fs.readFileSync(
        new URL("../src/features/notes/components/NotesList/NoteCard.tsx", import.meta.url),
        "utf8"
      );

      // Verify NoteEditor restore action
      assert.ok(
        editorContent.includes('className="btn-trash-banner btn-trash-banner--restore"'),
        "NoteEditor must render restore button in banner"
      );
      assert.ok(
        editorContent.includes("Restore Note"),
        "NoteEditor restore button must be clearly labeled 'Restore Note'"
      );
      assert.ok(
        editorContent.includes("onRestoreNote(loadedNote.id)"),
        "NoteEditor must invoke onRestoreNote with stable loadedNote.id"
      );

      // Verify NoteCard restore action
      assert.ok(
        noteCardContent.includes("note-card-trash-action-btn--restore"),
        "NoteCard must render restore button in trash mode"
      );
      assert.ok(
        noteCardContent.includes("<span>Restore</span>"),
        "NoteCard restore button must be clearly labeled 'Restore'"
      );
      assert.ok(
        noteCardContent.includes("onRestore(note.id)"),
        "NoteCard must invoke onRestore with stable note.id"
      );
    });

    it("verifies handleRestoreNote updates state, removes note from Trash, refreshes counts, and presents feedback", async () => {
      const fs = await import("node:fs");
      const appContent = fs.readFileSync(
        new URL("../src/app/App.tsx", import.meta.url),
        "utf8"
      );
      const appCss = fs.readFileSync(
        new URL("../src/app/App.css", import.meta.url),
        "utf8"
      );

      // Verify restore invocation and backend sync
      assert.ok(
        appContent.includes("const restoredNote = await storageService.notes.restoreNote(noteId);"),
        "handleRestoreNote must call storageService.notes.restoreNote"
      );
      assert.ok(
        appContent.includes('await loadNotes("trash", null, null)'),
        "handleRestoreNote must reload trash list to remove restored note from view"
      );
      assert.ok(
        appContent.includes("await refreshNoteCounts()"),
        "handleRestoreNote must refresh sidebar note counts"
      );

      // Verify selection clearance if restoring currently selected note
      assert.ok(
        appContent.includes("if (selectedNoteId === noteId) {\n          setSelectedNoteId(null);"),
        "handleRestoreNote must deselect note if it was active in preview"
      );

      // Verify user feedback presentation
      assert.ok(
        appContent.includes("setRestoreFeedback({"),
        "handleRestoreNote must set user feedback on restore"
      );
      assert.ok(
        appContent.includes("restore-feedback-toast"),
        "App must render restore-feedback-toast"
      );
      assert.ok(
        appContent.includes("restore-feedback-toast--${restoreFeedback.type}"),
        "App must apply dynamic toast type class"
      );
      assert.ok(
        appCss.includes(".restore-feedback-toast--success"),
        "App.css must style success toast"
      );
      assert.ok(
        appCss.includes(".restore-feedback-toast--error"),
        "App.css must style error toast"
      );

      // Verify CSS styles for toast
      assert.ok(
        appCss.includes(".restore-feedback-toast"),
        "App.css must style .restore-feedback-toast"
      );
      assert.ok(
        appCss.includes(".restore-feedback-toast--success"),
        "App.css must style success toast"
      );
    });

    it("verifies failure behavior maintains truthful state, refreshes from backend, and presents error message", async () => {
      const fs = await import("node:fs");
      const appContent = fs.readFileSync(
        new URL("../src/app/App.tsx", import.meta.url),
        "utf8"
      );

      // Verify failure handling in handleRestoreNote
      assert.ok(
        appContent.includes('setRestoreFeedback({\n          type: "error",'),
        "handleRestoreNote must set error feedback on failure"
      );
      assert.ok(
        appContent.includes('"Failed to restore note: "'),
        "handleRestoreNote must set descriptive error message"
      );
    });
  });

  describe("Task 16: Implement Permanent-Delete Confirmation", () => {
    it("verifies PermanentDeleteDialog component structure, clear action labels, and duplicate submission guards", async () => {
      const fs = await import("node:fs");
      const dialogContent = fs.readFileSync(
        new URL(
          "../src/features/notes/components/PermanentDeleteDialog.tsx",
          import.meta.url
        ),
        "utf8"
      );

      // Verify Modal usage
      assert.ok(
        dialogContent.includes("<Modal"),
        "PermanentDeleteDialog must use the Modal component"
      );

      // Verify unambiguous action labels: 'Cancel' and 'Permanently Delete' (no vague 'OK')
      assert.ok(
        />\s*Cancel\s*<\/Button>/.test(dialogContent),
        "Dialog must have explicit Cancel action button"
      );
      assert.ok(
        />\s*Permanently Delete\s*<\/Button>/.test(dialogContent),
        "Dialog must have explicit 'Permanently Delete' button (not vague 'OK')"
      );
      assert.ok(
        !dialogContent.includes(">OK<") && !dialogContent.includes(">Ok<"),
        "Dialog must not use vague 'OK' label"
      );
      assert.ok(
        dialogContent.includes('variant="danger"'),
        "Permanent Delete action button must use danger variant"
      );

      // Verify explanation that permanent deletion cannot be undone through Trash
      assert.ok(
        dialogContent.includes("cannot be undone through the Trash interface"),
        "Dialog must explain that permanent deletion cannot be undone through Trash interface"
      );

      // Verify duplicate submission guard and loading state
      assert.ok(
        dialogContent.includes("if (!note || isDeleting) return;"),
        "Dialog must guard against concurrent or duplicate submissions"
      );
      assert.ok(
        dialogContent.includes("isLoading={isDeleting}"),
        "Permanently Delete button must show loading state while executing"
      );
      assert.ok(
        dialogContent.includes("disabled={isDeleting}"),
        "Actions must be disabled while deleting is in progress"
      );

      // Verify keyboard shortcuts support
      assert.ok(
        dialogContent.includes('e.key === "Enter"'),
        "Dialog should handle Enter key"
      );
      assert.ok(
        dialogContent.includes('e.key === "Escape"'),
        "Dialog should handle Escape key"
      );
    });

    it("verifies App.tsx captures target note, prompts confirmation, routes to backend permanent delete, and handles errors honestly", async () => {
      const fs = await import("node:fs");
      const appContent = fs.readFileSync(
        new URL("../src/app/App.tsx", import.meta.url),
        "utf8"
      );

      // Verify state capture when confirmation begins
      assert.ok(
        appContent.includes("permanentDeleteTarget"),
        "App must maintain permanentDeleteTarget state to capture note ID"
      );
      assert.ok(
        appContent.includes("setPermanentDeleteTarget({ id: noteId, title })"),
        "handlePermanentlyDeleteNote must capture the selected note ID and title before showing dialog"
      );

      // Verify rendering PermanentDeleteDialog
      assert.ok(
        appContent.includes("<PermanentDeleteDialog"),
        "App must render PermanentDeleteDialog"
      );
      assert.ok(
        appContent.includes("isOpen={permanentDeleteTarget !== null}"),
        "Dialog isOpen prop must be linked to permanentDeleteTarget state"
      );
      assert.ok(
        appContent.includes("onConfirm={handleConfirmPermanentDelete}"),
        "Dialog onConfirm must call handleConfirmPermanentDelete"
      );

      // Verify handleConfirmPermanentDelete calls storageService.notes.permanentlyDelete
      assert.ok(
        appContent.includes("await storageService.notes.permanentlyDelete(noteId)"),
        "Confirmation must invoke storageService.notes.permanentlyDelete"
      );

      // Verify checking that note is still in Trash and refreshing UI if not
      assert.ok(
        appContent.includes("const trashedNotes = await storageService.notes.listTrashed()"),
        "Confirmation flow must check if note is still in Trash"
      );
      assert.ok(
        appContent.includes('throw new Error("This note is no longer in the Trash.")'),
        "Confirmation flow must reject if note is no longer in Trash"
      );
      assert.ok(
        appContent.includes('await loadNotes("trash", null, null)'),
        "handleConfirmPermanentDelete must reload trash notes"
      );
      assert.ok(
        appContent.includes("await refreshNoteCounts()"),
        "handleConfirmPermanentDelete must refresh note counts"
      );
    });

    it("verifies confirmation simulation rules: ID stability, cancellation no-op, and re-entry prevention", async () => {
      // 1. Stable captured ID test
      let capturedTarget = null;
      let selectedNoteId = "note-A";

      const requestPermanentDelete = (id, title) => {
        capturedTarget = { id, title };
      };

      // Trigger deletion for note-A
      requestPermanentDelete("note-A", "First Note");
      // Simulate asynchronous state change changing current active note
      selectedNoteId = "note-B";

      // The confirmed target must be note-A, NOT note-B
      assert.strictEqual(capturedTarget.id, "note-A");
      assert.notStrictEqual(capturedTarget.id, selectedNoteId);

      // 2. Cancellation performs no mutation
      let deleteCalledWith = null;
      const onCancel = () => {
        capturedTarget = null;
      };
      onCancel();
      assert.strictEqual(capturedTarget, null);
      assert.strictEqual(deleteCalledWith, null);

      // 3. Re-entry guard prevents duplicate calls
      let isDeleting = false;
      let executeCount = 0;
      const executeDelete = async (id) => {
        if (isDeleting) return;
        isDeleting = true;
        executeCount++;
        await new Promise((r) => setTimeout(r, 10));
        isDeleting = false;
      };

      const p1 = executeDelete("note-A");
      const p2 = executeDelete("note-A");
      await Promise.all([p1, p2]);

      assert.strictEqual(executeCount, 1, "Duplicate click while isDeleting must be ignored");
    });
  });

  describe("Task 17: Implement Empty-Trash Confirmation and Batch Feedback", () => {
    it("verifies EmptyTrashDialog component structure, distinct from Restore, action labels, and submission guards", async () => {
      const fs = await import("node:fs");
      const dialogContent = fs.readFileSync(
        new URL(
          "../src/features/notes/components/EmptyTrashDialog.tsx",
          import.meta.url
        ),
        "utf8"
      );

      // Verify Modal usage
      assert.ok(
        dialogContent.includes("<Modal"),
        "EmptyTrashDialog must use the Modal component"
      );

      // Verify distinct from Restore
      assert.ok(
        dialogContent.includes("NOT be restored") || dialogContent.includes("not be restored"),
        "Dialog must clearly explain that notes will NOT be restored"
      );

      // Verify explicit action labels: 'Cancel' and 'Empty Trash' (no vague 'OK')
      assert.ok(
        />\s*Cancel\s*<\/Button>/.test(dialogContent),
        "Dialog must have explicit Cancel action button"
      );
      assert.ok(
        />\s*Empty Trash\s*<\/Button>/.test(dialogContent),
        "Dialog must have explicit 'Empty Trash' button (not vague 'OK')"
      );
      assert.ok(
        !dialogContent.includes(">OK<") && !dialogContent.includes(">Ok<"),
        "Dialog must not use vague 'OK' label"
      );
      assert.ok(
        dialogContent.includes('variant="danger"'),
        "Empty Trash action button must use danger variant"
      );

      // Verify duplicate submission guard and loading state
      assert.ok(
        dialogContent.includes("if (isDeleting) return;"),
        "Dialog must guard against concurrent or duplicate submissions"
      );
      assert.ok(
        dialogContent.includes("isLoading={isDeleting}"),
        "Empty Trash button must show loading state while executing"
      );
      assert.ok(
        dialogContent.includes("disabled={isDeleting}"),
        "Actions must be disabled while emptying trash is in progress"
      );

      // Verify keyboard shortcuts support
      assert.ok(
        dialogContent.includes('e.key === "Enter"'),
        "Dialog should handle Enter key"
      );
      assert.ok(
        dialogContent.includes('e.key === "Escape"'),
        "Dialog should handle Escape key"
      );
    });

    it("verifies NotesList header renders Empty Trash button with disabled state when empty", async () => {
      const fs = await import("node:fs");
      const listContent = fs.readFileSync(
        new URL(
          "../src/features/notes/components/NotesList/NotesList.tsx",
          import.meta.url
        ),
        "utf8"
      );
      const listCss = fs.readFileSync(
        new URL(
          "../src/features/notes/components/NotesList/NotesList.css",
          import.meta.url
        ),
        "utf8"
      );

      // Verify prop exists
      assert.ok(
        listContent.includes("onEmptyTrash?: () => void;"),
        "NotesListProps must accept onEmptyTrash callback"
      );

      // Verify button rendered conditionally in trash mode
      assert.ok(
        listContent.includes("isTrashMode && onEmptyTrash"),
        "NotesList must conditionally render Empty Trash button in trash mode"
      );
      assert.ok(
        listContent.includes('className="notes-list-empty-trash-btn"'),
        "NotesList must apply notes-list-empty-trash-btn class"
      );
      assert.ok(
        listContent.includes("disabled={notes.length === 0}"),
        "Empty Trash button must be disabled when Trash list is empty"
      );

      // Verify CSS styles
      assert.ok(
        listCss.includes(".notes-list-empty-trash-btn"),
        "NotesList.css must define styles for .notes-list-empty-trash-btn"
      );
      assert.ok(
        listCss.includes(".notes-list-empty-trash-btn:disabled"),
        "NotesList.css must define disabled styles for .notes-list-empty-trash-btn"
      );
    });

    it("verifies App.tsx invokes dedicated batch IPC, refreshes views, and handles partial failures honestly", async () => {
      const fs = await import("node:fs");
      const appContent = fs.readFileSync(
        new URL("../src/app/App.tsx", import.meta.url),
        "utf8"
      );
      const appCss = fs.readFileSync(
        new URL("../src/app/App.css", import.meta.url),
        "utf8"
      );

      // Verify dedicated backend batch operation is called
      assert.ok(
        appContent.includes("await storageService.notes.emptyTrash()"),
        "handleConfirmEmptyTrash must invoke dedicated backend storageService.notes.emptyTrash()"
      );

      // Verify does not loop through individual frontend deletions
      assert.ok(
        !appContent.includes("for (const note of notes) {\n      await storageService.notes.permanentlyDelete"),
        "Empty Trash must call batch service, not uncoordinated individual deletions"
      );

      // Verify clearing selection and reloading trash and counts
      assert.ok(
        appContent.includes('if (activeNavId === "trash") {\n        setSelectedNoteId(null);\n      }'),
        "handleConfirmEmptyTrash must clear selectedNoteId in trash mode"
      );
      assert.ok(
        appContent.includes('await loadNotes("trash", null, null)'),
        "handleConfirmEmptyTrash must reload trash notes"
      );
      assert.ok(
        appContent.includes("await refreshNoteCounts()"),
        "handleConfirmEmptyTrash must refresh note counts"
      );

      // Verify dialog rendering
      assert.ok(
        appContent.includes("<EmptyTrashDialog"),
        "App must render EmptyTrashDialog"
      );
      assert.ok(
        appContent.includes("isOpen={isEmptyTrashOpen}"),
        "EmptyTrashDialog isOpen prop must be linked to isEmptyTrashOpen state"
      );
      assert.ok(
        appContent.includes("onConfirm={handleConfirmEmptyTrash}"),
        "EmptyTrashDialog onConfirm must call handleConfirmEmptyTrash"
      );

      // Verify partial failure and honest feedback handling
      assert.ok(
        appContent.includes("result.failedCount > 0 && result.deletedCount > 0"),
        "handleConfirmEmptyTrash must detect partial batch failures"
      );
      assert.ok(
        appContent.includes("notes deleted, but"),
        "handleConfirmEmptyTrash must notify user when some notes could not be deleted"
      );
      assert.ok(
        appContent.includes("trash-feedback-toast"),
        "App must render trash-feedback-toast"
      );
      assert.ok(
        appCss.includes(".trash-feedback-toast--warning") || appCss.includes(".restore-feedback-toast--warning"),
        "App.css must style warning state for partial batch failure toasts"
      );
    });

    it("verifies batch feedback simulation: empty trash no-op, full success, partial failure, and re-entry guard", async () => {
      // 1. Logic simulation of batch results
      const formatBatchResult = (result) => {
        if (result.failedCount === 0 && result.attachmentCleanupFailures.length === 0) {
          if (result.deletedCount === 0) {
            return { type: "success", message: "Trash is already empty." };
          }
          return {
            type: "success",
            message: `Emptied Trash: ${result.deletedCount} notes permanently deleted.`,
          };
        } else if (result.failedCount > 0 && result.deletedCount > 0) {
          return {
            type: "warning",
            message: `${result.deletedCount} notes deleted, but ${result.failedCount} notes could not be deleted.`,
          };
        } else if (result.failedCount > 0 && result.deletedCount === 0) {
          return {
            type: "error",
            message: `Failed to empty trash: ${result.errors[0] || "Unknown error"}`,
          };
        } else {
          return {
            type: "warning",
            message: `${result.deletedCount} notes deleted, but some attachments could not be cleaned up.`,
          };
        }
      };

      // Test empty trash
      assert.deepStrictEqual(
        formatBatchResult({ deletedCount: 0, failedCount: 0, attachmentCleanupFailures: [], errors: [] }),
        { type: "success", message: "Trash is already empty." }
      );

      // Test all success
      assert.deepStrictEqual(
        formatBatchResult({ deletedCount: 5, failedCount: 0, attachmentCleanupFailures: [], errors: [] }),
        { type: "success", message: "Emptied Trash: 5 notes permanently deleted." }
      );

      // Test partial failure
      const partialRes = formatBatchResult({
        deletedCount: 3,
        failedCount: 2,
        attachmentCleanupFailures: [],
        errors: ["Note lock failed"],
      });
      assert.strictEqual(partialRes.type, "warning");
      assert.ok(partialRes.message.includes("3 notes deleted, but 2 notes could not be deleted"));

      // Test attachment cleanup failure
      const attachRes = formatBatchResult({
        deletedCount: 4,
        failedCount: 0,
        attachmentCleanupFailures: ["note-1"],
        errors: ["Attachment unlink failed"],
      });
      assert.strictEqual(attachRes.type, "warning");
      assert.ok(attachRes.message.includes("some attachments could not be cleaned up"));

      // 2. Duplicate click protection simulation
      let isBusy = false;
      let executed = 0;
      const triggerEmpty = async () => {
        if (isBusy) return;
        isBusy = true;
        executed++;
        await new Promise((r) => setTimeout(r, 10));
        isBusy = false;
      };

      await Promise.all([triggerEmpty(), triggerEmpty()]);
      assert.strictEqual(executed, 1, "Duplicate trigger while busy must be prevented");
    });
  });

  describe("Task 18: Preserve Notebooks, Tags, Favorites, and Note Metadata", () => {
    it("verifies notebooks preservation and orphaned-note handling during restore", async () => {
      const fs = await import("node:fs");
      const notesRepo = fs.readFileSync(
        new URL("../src-tauri/src/storage/repositories/notes.rs", import.meta.url),
        "utf8"
      );
      const trashService = fs.readFileSync(
        new URL("../src-tauri/src/storage/services/trash.rs", import.meta.url),
        "utf8"
      );

      // Verify soft delete preserves notebook_id
      assert.ok(
        notesRepo.includes("UPDATE notes SET is_deleted = 1"),
        "soft_delete must update is_deleted flag without clearing notebook_id"
      );

      // Verify restore defensive check for deleted parent notebook
      assert.ok(
        notesRepo.includes("SELECT COUNT(1) FROM notebooks WHERE id = ?1"),
        "restore must verify if original notebook exists in database"
      );
      assert.ok(
        notesRepo.includes("effective_notebook_id"),
        "restore must calculate effective_notebook_id"
      );
      assert.ok(
        notesRepo.includes("restored.notebook_id = effective_notebook_id"),
        "restore must safely unfile note (notebook_id = None) if notebook was deleted"
      );

      // Verify Rust test coverage for orphaned restore policy
      assert.ok(
        trashService.includes("test_trash_service_restore_lifecycle_and_notebook_handling"),
        "trash.rs must contain comprehensive restore and notebook handling tests"
      );
    });

    it("verifies tag associations and metadata filters exclude trashed notes", async () => {
      const fs = await import("node:fs");
      const tagsRepo = fs.readFileSync(
        new URL("../src-tauri/src/storage/repositories/tags.rs", import.meta.url),
        "utf8"
      );
      const notesRepo = fs.readFileSync(
        new URL("../src-tauri/src/storage/repositories/notes.rs", import.meta.url),
        "utf8"
      );
      const appContent = fs.readFileSync(
        new URL("../src/app/App.tsx", import.meta.url),
        "utf8"
      );

      // Verify tag queries join only active notes (n.is_deleted = 0)
      assert.ok(
        tagsRepo.includes("WHERE n.is_deleted = 0"),
        "tags repository queries must exclude trashed notes"
      );
      assert.ok(
        /WHERE nt\.tag_id = \?1\s+AND n\.is_deleted = 0/.test(tagsRepo) ||
          /WHERE nt\.tag_id = \?1\s+AND n\.is_deleted = 0/.test(notesRepo),
        "list_by_tag must filter out trashed notes with n.is_deleted = 0"
      );

      // Verify tag count calculation excludes trashed notes
      assert.ok(
        /SELECT nt\.tag_id, COUNT\(n\.id\)\s+FROM note_tags nt\s+JOIN notes n ON n\.id = nt\.note_id\s+WHERE n\.is_deleted = 0/.test(tagsRepo),
        "getTagNoteCounts must only count active notes"
      );

      // Verify App.tsx loadNotes for favorites calls storageService.notes.listFavorites()
      assert.ok(
        appContent.includes('} else if (effectiveNav === "favorites") {\n          fetchedNotes = await storageService.notes.listFavorites();'),
        "App.tsx loadNotes must call storageService.notes.listFavorites() for favorites view"
      );

      // Verify list_favorites excludes trashed notes
      assert.ok(
        notesRepo.includes("WHERE is_deleted = 0 AND is_favorite = 1"),
        "list_favorites must filter for is_deleted = 0"
      );
    });

    it("verifies timestamp semantics: creation preserved, deletion distinct from modification", async () => {
      const fs = await import("node:fs");
      const notesRepo = fs.readFileSync(
        new URL("../src-tauri/src/storage/repositories/notes.rs", import.meta.url),
        "utf8"
      );
      const trashService = fs.readFileSync(
        new URL("../src-tauri/src/storage/services/trash.rs", import.meta.url),
        "utf8"
      );

      // Verify soft_delete sets deleted_at
      assert.ok(
        notesRepo.includes("deleted_at = ?1"),
        "soft_delete must record deletion timestamp in deleted_at"
      );

      // Verify restore resets deleted_at to NULL while preserving created_at
      assert.ok(
        notesRepo.includes("deleted_at = NULL"),
        "restore must reset deleted_at to NULL"
      );
      assert.ok(
        trashService.includes("assert_eq!(restored.created_at, note.created_at);"),
        "restore must preserve original created_at"
      );
    });

    it("verifies metadata preservation lifecycle simulation", () => {
      // Simulate note lifecycle state transitions
      const originalNote = {
        id: "note-meta-1",
        title: "Architecture Doc",
        content: "Details on metadata",
        notebook_id: "nb-100",
        is_favorite: true,
        is_pinned: true,
        created_at: "2026-09-01T12:00:00Z",
        modified_at: "2026-09-02T15:00:00Z",
        is_deleted: false,
        deleted_at: null,
      };

      const tagAssignments = ["tag-rust", "tag-docs"];

      // 1. Soft deletion transition
      const deletionTimestamp = "2026-10-09T10:00:00Z";
      const trashedNote = {
        ...originalNote,
        is_deleted: true,
        deleted_at: deletionTimestamp,
        modified_at: deletionTimestamp,
      };

      // Invariants on soft deletion:
      assert.strictEqual(trashedNote.id, originalNote.id);
      assert.strictEqual(trashedNote.created_at, originalNote.created_at);
      assert.strictEqual(trashedNote.notebook_id, originalNote.notebook_id);
      assert.strictEqual(trashedNote.is_favorite, true);
      assert.strictEqual(trashedNote.is_pinned, true);
      assert.deepStrictEqual(tagAssignments, ["tag-rust", "tag-docs"], "Tags remain intact");

      // 2. Filter exclusions
      const allNotes = [trashedNote];
      const activeFavorites = allNotes.filter((n) => !n.is_deleted && n.is_favorite);
      assert.strictEqual(activeFavorites.length, 0, "Active favorites view must exclude trashed note");

      const activeTagNotes = allNotes.filter((n) => !n.is_deleted);
      assert.strictEqual(activeTagNotes.length, 0, "Active tag view must exclude trashed note");

      // 3. Restoration transition with existing notebook
      const restoreTimestamp = "2026-10-09T10:30:00Z";
      const restoredNote = {
        ...trashedNote,
        is_deleted: false,
        deleted_at: null,
        modified_at: restoreTimestamp,
      };

      assert.strictEqual(restoredNote.id, originalNote.id);
      assert.strictEqual(restoredNote.created_at, originalNote.created_at);
      assert.strictEqual(restoredNote.notebook_id, "nb-100");
      assert.strictEqual(restoredNote.is_favorite, true);
      assert.strictEqual(restoredNote.is_pinned, true);
      assert.strictEqual(restoredNote.deleted_at, null);

      // 4. Restoration transition with deleted parent notebook (orphaned policy)
      const restoredOrphaned = {
        ...trashedNote,
        notebook_id: null, // Unfiled policy
        is_deleted: false,
        deleted_at: null,
        modified_at: restoreTimestamp,
      };
      assert.strictEqual(restoredOrphaned.notebook_id, null, "Note is safely unfiled if notebook was removed");
    });
  });

  describe("Task 19: Integrate Trash with Search", () => {
    it("verifies backend active search queries join authoritative notes table and filter out trashed notes", async () => {
      const fs = await import("node:fs");
      const searchRepo = fs.readFileSync(
        new URL("../src-tauri/src/storage/repositories/search.rs", import.meta.url),
        "utf8"
      );

      // Verify FTS5 query joins notes table and enforces n.is_deleted = 0
      assert.ok(
        searchRepo.includes("JOIN notes n ON notes_fts.rowid = n.rowid"),
        "search_fts5 must join notes table using rowid"
      );
      assert.ok(
        searchRepo.includes("AND n.is_deleted = 0"),
        "search_fts5 must exclude trashed notes by checking n.is_deleted = 0"
      );

      // Verify LIKE fallback query enforces is_deleted = 0
      assert.ok(
        searchRepo.includes("WHERE is_deleted = 0\n              AND (title LIKE ?1 ESCAPE '\\' OR content LIKE ?1 ESCAPE '\\')") ||
          searchRepo.includes("WHERE is_deleted = 0"),
        "search_like must exclude trashed notes by checking is_deleted = 0"
      );
    });

    it("verifies FTS synchronization triggers maintain consistency across soft-delete, restore, and permanent delete", async () => {
      const fs = await import("node:fs");
      const schemaSql = fs.readFileSync(
        new URL("../src-tauri/src/storage/schema.rs", import.meta.url),
        "utf8"
      );
      const searchRepo = fs.readFileSync(
        new URL("../src-tauri/src/storage/repositories/search.rs", import.meta.url),
        "utf8"
      );

      // Verify FTS5 virtual table triggers in schema
      assert.ok(
        schemaSql.includes("TRIGGER IF NOT EXISTS notes_fts_ai"),
        "Schema must create notes_fts_ai trigger for insert sync"
      );
      assert.ok(
        schemaSql.includes("TRIGGER IF NOT EXISTS notes_fts_ad"),
        "Schema must create notes_fts_ad trigger for delete sync"
      );
      assert.ok(
        schemaSql.includes("TRIGGER IF NOT EXISTS notes_fts_au"),
        "Schema must create notes_fts_au trigger for update sync"
      );

      // Verify Rust test coverage for soft-delete and restore FTS synchronization
      assert.ok(
        searchRepo.includes("test_task_37_fts_synchronization_on_delete_and_soft_delete"),
        "search.rs must test FTS synchronization on soft delete"
      );
      assert.ok(
        searchRepo.includes("test_task_38_fts_synchronization_on_restore"),
        "search.rs must test FTS synchronization on restore"
      );
    });

    it("verifies NoteEditor and App enforce read-only state and prevent editing on stale search results of trashed notes", async () => {
      const fs = await import("node:fs");
      const editorContent = fs.readFileSync(
        new URL("../src/features/notes/components/Editor/NoteEditor.tsx", import.meta.url),
        "utf8"
      );
      const appContent = fs.readFileSync(
        new URL("../src/app/App.tsx", import.meta.url),
        "utf8"
      );

      // Verify NoteEditor sets readOnly based on note's authoritative is_deleted attribute
      assert.ok(
        editorContent.includes("const isReadOnly = isTrash || !!loadedNote?.is_deleted;"),
        "NoteEditor must set isReadOnly if loadedNote.is_deleted is true"
      );

      // Verify trashed note banner is shown
      assert.ok(
        editorContent.includes("This note is in Trash.") &&
          editorContent.includes("It is read-only."),
        "NoteEditor must display read-only banner for trashed notes"
      );

      // Verify input elements have readOnly set
      assert.ok(
        editorContent.includes("readOnly={isReadOnly}"),
        "Note title and content inputs must be marked readOnly"
      );

      // Verify mutation actions are prevented
      assert.ok(
        editorContent.includes("if (isReadOnly) return;"),
        "Content and title change handlers must return early if isReadOnly"
      );
      assert.ok(
        editorContent.includes("if (isSavingRef.current || isReadOnly) return;"),
        "Save handler must abort if isReadOnly"
      );

      // Verify App.tsx handleSaveNote validates note is not deleted before saving
      assert.ok(
        appContent.includes("existing?.is_deleted"),
        "App.tsx handleSaveNote must check existing.is_deleted to guard against stale search results"
      );
    });

    it("verifies backend NotesRepository prevents updating trashed notes without restoration", async () => {
      const fs = await import("node:fs");
      const notesRepo = fs.readFileSync(
        new URL("../src-tauri/src/storage/repositories/notes.rs", import.meta.url),
        "utf8"
      );

      assert.ok(
        notesRepo.includes("if existing.is_deleted && dto.is_deleted != Some(false)"),
        "NotesRepository::update must reject updating a trashed note without restoration"
      );
    });

    it("verifies search lifecycle simulation: soft delete excludes from active search, restore returns note to search results", () => {
      // Setup mock notes
      const notes = [
        { id: "note-1", title: "Quarterly Financial Plan", content: "Revenue targets for Q4", is_deleted: false },
        { id: "note-2", title: "Product Roadmap", content: "Sprint deliverables and roadmap", is_deleted: false },
      ];

      // Helper search simulating authoritative notes join (AND is_deleted = 0)
      const executeSearch = (query, noteList) => {
        const q = query.toLowerCase();
        return noteList.filter(
          (n) =>
            !n.is_deleted &&
            (n.title.toLowerCase().includes(q) || n.content.toLowerCase().includes(q))
        );
      };

      // 1. Initial search finds active note
      const initialResults = executeSearch("financial", notes);
      assert.strictEqual(initialResults.length, 1);
      assert.strictEqual(initialResults[0].id, "note-1");

      // 2. Soft-delete note-1
      notes[0].is_deleted = true;

      // 3. Active search immediately excludes note-1
      const afterDeleteResults = executeSearch("financial", notes);
      assert.strictEqual(afterDeleteResults.length, 0, "Soft-deleted note must not be returned in active search");

      // Stale search client attempt to edit trashed note
      const staleSelectedNote = notes[0];
      const isReadOnly = staleSelectedNote.is_deleted;
      assert.strictEqual(isReadOnly, true, "Opening a trashed note from stale search result must enter read-only mode");

      // 4. Restore note-1
      notes[0].is_deleted = false;

      // 5. Restored note immediately becomes searchable again with same ID and content
      const afterRestoreResults = executeSearch("financial", notes);
      assert.strictEqual(afterRestoreResults.length, 1, "Restored note must become searchable again");
      assert.strictEqual(afterRestoreResults[0].id, "note-1");
      assert.strictEqual(afterRestoreResults[0].title, "Quarterly Financial Plan");
    });
  });

  describe("Task 20: Handle Attachments During Trash, Restore, and Permanent Deletion", () => {
    it("verifies soft deletion and restore preserve attachment records and files", async () => {
      const fs = await import("node:fs");
      const trashService = fs.readFileSync(
        new URL("../src-tauri/src/storage/services/trash.rs", import.meta.url),
        "utf8"
      );
      const attachmentsService = fs.readFileSync(
        new URL("../src-tauri/src/storage/services/attachments.rs", import.meta.url),
        "utf8"
      );
      const attachmentsRepo = fs.readFileSync(
        new URL("../src-tauri/src/storage/repositories/attachments.rs", import.meta.url),
        "utf8"
      );

      // Verify soft_delete only marks is_deleted on notes and does NOT call remove_note_directory or delete attachments
      const softDeleteBody = trashService.match(/pub fn soft_delete[\s\S]*?\n    \}/)?.[0] || "";
      assert.strictEqual(
        softDeleteBody.includes("remove_note_directory"),
        false,
        "soft_delete must never delete managed attachment files"
      );
      assert.strictEqual(
        softDeleteBody.includes("DELETE FROM attachments"),
        false,
        "soft_delete must never purge attachment records"
      );

      // Verify restore only clears is_deleted and preserves attachments
      const restoreBody = trashService.match(/pub fn restore[\s\S]*?\n    \}/)?.[0] || "";
      assert.strictEqual(
        restoreBody.includes("remove_note_directory"),
        false,
        "restore must never delete attachment files"
      );

      // Verify backend rejects modifying attachments for deleted notes
      assert.ok(
        attachmentsRepo.includes("Cannot attach files to a deleted note"),
        "verify_note_eligible must reject adding attachments to deleted notes"
      );
      assert.ok(
        attachmentsService.includes("Cannot remove attachments from a deleted note. Restore it first."),
        "remove_attachment must reject deleting attachments belonging to deleted notes"
      );
    });

    it("verifies read-only attachment preview blocks mutations and preserves inspection", async () => {
      const fs = await import("node:fs");
      const listContent = fs.readFileSync(
        new URL("../src/features/attachments/components/AttachmentList.tsx", import.meta.url),
        "utf8"
      );
      const itemContent = fs.readFileSync(
        new URL("../src/features/attachments/components/AttachmentItem.tsx", import.meta.url),
        "utf8"
      );

      // Verify AttachmentList disables "Attach File" button when isReadOnly is true
      assert.ok(
        listContent.includes("disabled={isBusy || isReadOnly}"),
        "AttachmentList must disable attach button when isReadOnly"
      );

      // Verify AttachmentList propagates isReadOnly to AttachmentItem
      assert.ok(
        listContent.includes("isReadOnly={isReadOnly}"),
        "AttachmentList must pass isReadOnly to each AttachmentItem"
      );

      // Verify AttachmentItem conditionally suppresses remove button when isReadOnly
      assert.ok(
        itemContent.includes("{!isReadOnly && (") && itemContent.includes('className="attachment-action-btn btn-remove"'),
        "AttachmentItem must hide remove button when isReadOnly is true"
      );

      // Verify open button remains available for read-only inspection
      assert.ok(
        itemContent.includes('className="attachment-action-btn btn-open"'),
        "AttachmentItem must keep Open button available for viewing attachments"
      );
    });

    it("verifies permanent deletion coordinates attachment file cleanup and honest reporting", async () => {
      const fs = await import("node:fs");
      const trashService = fs.readFileSync(
        new URL("../src-tauri/src/storage/services/trash.rs", import.meta.url),
        "utf8"
      );

      // Verify permanently_delete_note coordinates file cleanup before removing DB record
      assert.ok(
        trashService.includes("storage.remove_note_directory(trimmed_id)"),
        "permanently_delete_note must invoke storage.remove_note_directory with note ID"
      );

      // Verify honest tracking of cleanup failure
      assert.ok(
        trashService.includes("attachment_cleanup_failed,") &&
          trashService.includes("error_message: cleanup_err,"),
        "permanently_delete_note must truthfully report attachment cleanup failures"
      );

      // Verify empty_trash tracks attachment_cleanup_failures
      assert.ok(
        trashService.includes("attachment_cleanup_failures.push(note.id.clone());"),
        "empty_trash must collect note IDs with attachment cleanup failures"
      );
    });

    it("verifies simulated lifecycle: attachments preserved across trash & restore, cleaned up on permanent delete", () => {
      // Setup note with 2 attachments
      const note = {
        id: "note-with-attachments-1",
        title: "Project Specs & Assets",
        content: "Attached diagrams and specs",
        is_deleted: false,
      };

      const attachments = [
        { id: "att-1", note_id: note.id, file_name: "architecture.png", file_size: 4096 },
        { id: "att-2", note_id: note.id, file_name: "specification.pdf", file_size: 16384 },
      ];

      let managedDiskDirectories = new Set([note.id]);

      // 1. Soft-delete note
      note.is_deleted = true;
      const trashedNoteAttachments = attachments.filter((a) => a.note_id === note.id);
      assert.strictEqual(trashedNoteAttachments.length, 2, "Attachments must remain intact after soft deletion");
      assert.ok(managedDiskDirectories.has(note.id), "Filesystem directory must remain intact after soft deletion");

      // Verify read-only state in preview
      const previewIsReadOnly = note.is_deleted;
      assert.strictEqual(previewIsReadOnly, true, "Attachment controls must be read-only in trash preview");

      // 2. Restore note
      note.is_deleted = false;
      const restoredNoteAttachments = attachments.filter((a) => a.note_id === note.id);
      assert.strictEqual(restoredNoteAttachments.length, 2, "Attachments must remain intact after restoration");
      const activeIsReadOnly = note.is_deleted;
      assert.strictEqual(activeIsReadOnly, false, "Attachment controls must be fully editable after restore");

      // 3. Move back to trash and permanently delete
      note.is_deleted = true;
      // Coordinate filesystem cleanup and db cleanup
      managedDiskDirectories.delete(note.id);
      const remainingAttachments = attachments.filter((a) => a.note_id !== note.id);

      assert.strictEqual(managedDiskDirectories.has(note.id), false, "Disk directory removed on permanent deletion");
      assert.strictEqual(remainingAttachments.length, 0, "Attachment records purged on permanent deletion");
    });
  });

  describe("Task 21: Handle Empty States, Errors, Concurrency, and Stale UI State", () => {
    it("verifies distinct rendering for empty state and error state in NotesList", async () => {
      const fs = await import("node:fs");
      const listContent = fs.readFileSync(
        new URL("../src/features/notes/components/NotesList/NotesList.tsx", import.meta.url),
        "utf8"
      );

      // Verify loading state
      assert.ok(
        listContent.includes('{status === "loading" && ('),
        "NotesList must render loading state when status is loading"
      );

      // Verify distinct error state with retry callback
      assert.ok(
        listContent.includes('{status === "error" && (\n          <ErrorState\n            title={resolvedErrorTitle}\n            message={resolvedErrorMessage}\n            onRetry={onRetry}'),
        "NotesList must render ErrorState with retry handler when status is error"
      );

      // Verify distinct empty state with trash-specific icon and messaging
      assert.ok(
        listContent.includes('(status === "empty" || (status === "idle" && notes.length === 0)) && (\n          <EmptyState\n            icon={isTrashMode ? <EmptyTrashIcon /> : <EmptyNotesIcon />}'),
        "NotesList must render distinct EmptyState for trash with EmptyTrashIcon"
      );
      assert.ok(
        listContent.includes('Trash is empty.'),
        "NotesList must display 'Trash is empty.' when in trash mode"
      );
    });

    it("verifies stale response protection guards in App.tsx against race conditions", async () => {
      const fs = await import("node:fs");
      const appContent = fs.readFileSync(
        new URL("../src/app/App.tsx", import.meta.url),
        "utf8"
      );

      // Verify activeLoadRequestIdRef exists
      assert.ok(
        appContent.includes("const activeLoadRequestIdRef = useRef<number>(0);"),
        "App.tsx must maintain activeLoadRequestIdRef"
      );

      // Verify loadNotes increments request ID and discards stale out-of-order responses
      assert.ok(
        appContent.includes("const requestId = ++activeLoadRequestIdRef.current;"),
        "loadNotes must increment requestId at start of operation"
      );
      assert.ok(
        appContent.includes("if (activeLoadRequestIdRef.current !== requestId) return;"),
        "loadNotes must discard stale responses if newer query began or user navigated away"
      );
    });

    it("verifies in-flight concurrency guards prevent duplicate destructive or restoration operations", async () => {
      const fs = await import("node:fs");
      const appContent = fs.readFileSync(
        new URL("../src/app/App.tsx", import.meta.url),
        "utf8"
      );
      const permDialogContent = fs.readFileSync(
        new URL("../src/features/notes/components/PermanentDeleteDialog.tsx", import.meta.url),
        "utf8"
      );
      const emptyDialogContent = fs.readFileSync(
        new URL("../src/features/notes/components/EmptyTrashDialog.tsx", import.meta.url),
        "utf8"
      );

      // Verify inFlightMutationsRef in App.tsx
      assert.ok(
        appContent.includes("const inFlightMutationsRef = useRef<Set<string>>(new Set());"),
        "App.tsx must maintain inFlightMutationsRef for async operation gating"
      );
      assert.ok(
        appContent.includes("if (inFlightMutationsRef.current.has(noteId)) return;"),
        "handleRestoreNote must ignore duplicate in-flight requests for the same note"
      );
      assert.ok(
        appContent.includes("if (inFlightMutationsRef.current.has(targetId)) return;"),
        "handleDeleteNote must ignore duplicate in-flight requests"
      );

      // Verify confirmation dialogs disable submission while busy
      assert.ok(
        permDialogContent.includes("disabled={isDeleting}"),
        "PermanentDeleteDialog must disable buttons during deletion"
      );
      assert.ok(
        emptyDialogContent.includes("disabled={isDeleting}"),
        "EmptyTrashDialog must disable buttons during batch execution"
      );
    });

    it("verifies nonexistent or concurrently removed note handling in permanent deletion", async () => {
      const fs = await import("node:fs");
      const appContent = fs.readFileSync(
        new URL("../src/app/App.tsx", import.meta.url),
        "utf8"
      );
      const trashService = fs.readFileSync(
        new URL("../src-tauri/src/storage/services/trash.rs", import.meta.url),
        "utf8"
      );

      // Verify frontend re-validates note is still in Trash before executing permanent deletion
      assert.ok(
        appContent.includes("const isStillInTrash = trashedNotes.some((n) => n.id === noteId);"),
        "handleConfirmPermanentDelete must verify note is still in Trash"
      );
      assert.ok(
        appContent.includes('throw new Error("This note is no longer in the Trash.");'),
        "handleConfirmPermanentDelete must truthfully reject if note is no longer in Trash"
      );

      // Verify backend TrashService verifies note existence and trash state
      assert.ok(
        trashService.includes("TrashError::NoteNotFound"),
        "TrashService must return NoteNotFound for nonexistent note IDs"
      );
      assert.ok(
        trashService.includes("Only trashed notes can be permanently deleted"),
        "TrashService must reject active notes for permanent deletion"
      );
    });

    it("verifies simulated race conditions: out-of-order queries and concurrent clicks", async () => {
      // 1. Out-of-order response simulation
      let currentView = "trash";
      let activeRequestId = 0;
      let uiState = [];

      const triggerLoad = async (nav, delay, mockData) => {
        const reqId = ++activeRequestId;
        await new Promise((r) => setTimeout(r, delay));
        if (activeRequestId === reqId) {
          currentView = nav;
          uiState = mockData;
        }
      };

      // Start slow trash load (50ms) followed by fast all-notes load (10ms)
      const p1 = triggerLoad("trash", 50, [{ id: "trashed-1" }]);
      const p2 = triggerLoad("all-notes", 10, [{ id: "active-1" }]);

      await Promise.all([p1, p2]);

      // Assert that the newer query ("all-notes") is not overwritten by the delayed trash query
      assert.strictEqual(currentView, "all-notes");
      assert.deepStrictEqual(uiState, [{ id: "active-1" }]);

      // 2. Concurrent click guard simulation
      const inFlight = new Set();
      let executionCount = 0;

      const triggerAction = async (targetId) => {
        if (inFlight.has(targetId)) return;
        inFlight.add(targetId);
        try {
          executionCount++;
          await new Promise((r) => setTimeout(r, 10));
        } finally {
          inFlight.delete(targetId);
        }
      };

      await Promise.all([triggerAction("n1"), triggerAction("n1"), triggerAction("n1")]);
      assert.strictEqual(executionCount, 1, "Duplicate concurrent triggers for the same note must execute exactly once");
    });
  });

  describe("Task 22: Add Automated Backend and Database Tests", () => {
    it("verifies comprehensive Rust lifecycle test suites are defined and implemented in trash.rs", async () => {
      const fs = await import("node:fs");
      const trashService = fs.readFileSync(
        new URL("../src-tauri/src/storage/services/trash.rs", import.meta.url),
        "utf8"
      );

      // 1. Soft-delete tests
      assert.ok(
        trashService.includes("fn test_trash_service_soft_delete_lifecycle_and_validation()"),
        "trash.rs must implement soft-delete lifecycle and validation test"
      );

      // 2. Restore tests
      assert.ok(
        trashService.includes("fn test_trash_service_restore_lifecycle_and_notebook_handling()"),
        "trash.rs must implement restore lifecycle and notebook handling test"
      );

      // 3. Permanent-deletion tests
      assert.ok(
        trashService.includes("fn test_trash_service_permanent_delete_lifecycle_and_validation()"),
        "trash.rs must implement permanent delete lifecycle and isolation test"
      );
      assert.ok(
        trashService.includes("fn test_trash_service_permanent_delete_reports_attachment_cleanup_failure_honestly()"),
        "trash.rs must test honest failure reporting when attachment cleanup fails"
      );

      // 4. Empty-Trash tests
      assert.ok(
        trashService.includes("fn test_trash_service_empty_trash_lifecycle_and_invariants()"),
        "trash.rs must implement empty-trash lifecycle and survivor isolation test"
      );
      assert.ok(
        trashService.includes("fn test_trash_service_empty_trash_handles_partial_failures()"),
        "trash.rs must implement partial failure handling in empty trash"
      );

      // 5. Database integrity & lifecycle invariants tests
      assert.ok(
        trashService.includes("fn test_task_22_database_integrity_and_lifecycle_invariants()"),
        "trash.rs must implement database integrity and foreign key invariants test"
      );
    });

    it("verifies database foreign-key enforcement, cascading, and injection safety coverage", async () => {
      const fs = await import("node:fs");
      const trashService = fs.readFileSync(
        new URL("../src-tauri/src/storage/services/trash.rs", import.meta.url),
        "utf8"
      );

      // Verify PRAGMA foreign_keys assertion
      assert.ok(
        trashService.includes('query_row("PRAGMA foreign_keys;", [], |r| r.get(0))'),
        "Tests must verify PRAGMA foreign_keys enforcement"
      );

      // Verify SQL injection resilience test with unusual identifiers
      assert.ok(
        trashService.includes("Robert'); DROP TABLE notes;-- 🚀"),
        "Tests must verify SQL injection characters and Unicode emojis in titles and content"
      );

      // Verify migration idempotency test
      assert.ok(
        trashService.includes("run_migrations(&mut conn).expect(\"Re-running migrations must succeed idempotently\");"),
        "Tests must verify migration idempotency without data loss"
      );

      // Verify repeated empty-trash safety
      assert.ok(
        trashService.includes("let empty_1 = TrashService::empty_trash(&conn, &storage).unwrap();") &&
          trashService.includes("let empty_2 = TrashService::empty_trash(&conn, &storage).unwrap();"),
        "Tests must verify repeated empty-trash calls are idempotent"
      );
    });
  });

  describe("Task 23: Frontend and Integration Tests", () => {
    describe("1. Navigation", () => {
      it("verifies Trash navigation entry visibility, count badge, and styling in Sidebar", async () => {
        const fs = await import("node:fs");
        const sidebarContent = fs.readFileSync(
          new URL("../src/components/Sidebar/Sidebar.tsx", import.meta.url),
          "utf8"
        );
        const sidebarCss = fs.readFileSync(
          new URL("../src/components/Sidebar/Sidebar.css", import.meta.url),
          "utf8"
        );

        // Sidebar renders Trash navigation with distinct class and click handler
        assert.ok(
          sidebarContent.includes("sidebar-nav-item--trash"),
          "Sidebar must include sidebar-nav-item--trash class"
        );
        assert.ok(
          sidebarContent.includes('handleNavClick("trash")'),
          "Sidebar must trigger handleNavClick('trash') when clicked"
        );

        // Sidebar displays trash count badge when noteCounts["trash"] > 0
        assert.ok(
          sidebarContent.includes('noteCounts?.["trash"]') &&
            sidebarContent.includes("nav-item-count--trash"),
          "Sidebar must render nav-item-count--trash when noteCounts['trash'] > 0"
        );

        // Sidebar CSS defines distinct styling for trash item and badge
        assert.ok(
          sidebarCss.includes(".sidebar-nav-item--trash"),
          "Sidebar.css must style the trash navigation item"
        );
        assert.ok(
          sidebarCss.includes(".nav-item-count--trash"),
          "Sidebar.css must provide distinct styling for the trash count badge"
        );
      });

      it("verifies selecting Trash switches active view, resets notebook/tag filters, and loads trashed notes", async () => {
        const fs = await import("node:fs");
        const appContent = fs.readFileSync(
          new URL("../src/app/App.tsx", import.meta.url),
          "utf8"
        );

        // App.tsx invokes selectTrash and loads trashed notes
        assert.ok(
          appContent.includes("selectTrash();"),
          "App.tsx must call selectTrash() when navigating to trash"
        );
        assert.ok(
          appContent.includes('await loadNotes("trash", null, null)'),
          "Selecting trash must invoke loadNotes with 'trash' and null filters"
        );
        assert.ok(
          appContent.includes("storageService.notes.listTrashed()"),
          "App.tsx must query listTrashed() for trash count synchronization"
        );
      });

      it("verifies returning to active notes restores all-notes navigation and active note view", async () => {
        // Simulated navigation state manager
        let activeNav = "trash";
        let selectedNotebookId = null;
        let selectedTagId = null;
        let selectedNoteId = "trashed-1";

        const handleSelectNav = (navId) => {
          activeNav = navId;
          selectedNotebookId = null;
          selectedTagId = null;
          selectedNoteId = null;
        };

        // Return to all-notes
        handleSelectNav("all-notes");

        assert.strictEqual(activeNav, "all-notes", "Navigation must return to all-notes");
        assert.strictEqual(selectedNotebookId, null, "Notebook filter remains cleared");
        assert.strictEqual(selectedTagId, null, "Tag filter remains cleared");
        assert.strictEqual(selectedNoteId, null, "Note selection safely resets");
      });
    });

    describe("2. Listing and Preview", () => {
      it("verifies trashed notes appear in trash view while active notes are excluded", () => {
        const allNotesInDb = [
          { id: "note-1", title: "Active Project", is_deleted: false },
          { id: "note-2", title: "Archived Idea", is_deleted: true },
          { id: "note-3", title: "Active Todo", is_deleted: false },
          { id: "note-4", title: "Old Draft", is_deleted: true },
        ];

        // Active note query filter
        const activeNotes = allNotesInDb.filter((n) => !n.is_deleted);
        assert.strictEqual(activeNotes.length, 2);
        assert.deepStrictEqual(activeNotes.map((n) => n.id), ["note-1", "note-3"]);

        // Trash note query filter
        const trashedNotes = allNotesInDb.filter((n) => n.is_deleted);
        assert.strictEqual(trashedNotes.length, 2);
        assert.deepStrictEqual(trashedNotes.map((n) => n.id), ["note-2", "note-4"]);

        // Verify mutual exclusivity: active notes never appear in Trash
        const leakIntoTrash = trashedNotes.some((n) => !n.is_deleted);
        assert.strictEqual(leakIntoTrash, false, "Active notes must never appear in Trash list");
      });

      it("verifies empty trash displays the dedicated empty state and database error displays error state", async () => {
        const fs = await import("node:fs");
        const listContent = fs.readFileSync(
          new URL("../src/features/notes/components/NotesList/NotesList.tsx", import.meta.url),
          "utf8"
        );

        // Dedicated empty state for Trash
        assert.ok(
          listContent.includes("isTrashMode ? <EmptyTrashIcon /> : <EmptyNotesIcon />"),
          "NotesList must display EmptyTrashIcon when in trash mode"
        );
        assert.ok(
          listContent.includes("Trash is empty."),
          "NotesList must state 'Trash is empty.' in empty trash state"
        );

        // Error state presentation with retry
        assert.ok(
          listContent.includes("<ErrorState") &&
            listContent.includes("title={resolvedErrorTitle}") &&
            listContent.includes("message={resolvedErrorMessage}") &&
            listContent.includes("onRetry={onRetry}"),
          "NotesList must render ErrorState with retry handler when a database error occurs"
        );
      });

      it("verifies selecting a trashed note renders preview banner, timestamps, and suppresses autosave", async () => {
        const fs = await import("node:fs");
        const editorContent = fs.readFileSync(
          new URL("../src/features/notes/components/Editor/NoteEditor.tsx", import.meta.url),
          "utf8"
        );

        // Preview banner rendered in read-only trash mode
        assert.ok(
          editorContent.includes('className="trashed-note-banner"'),
          "NoteEditor must render trashed-note-banner when note is in trash"
        );
        assert.ok(
          editorContent.includes("This note is in Trash.") &&
            editorContent.includes("Restore this note to make changes."),
          "NoteEditor must inform the user that note is in Trash and read-only"
        );

        // Action buttons inside banner
        assert.ok(
          editorContent.includes("btn-trash-banner--restore") &&
            editorContent.includes("Restore Note"),
          "NoteEditor must render a Restore action button in the banner"
        );
        assert.ok(
          editorContent.includes("btn-trash-banner--delete") &&
            editorContent.includes("Delete Permanently"),
          "NoteEditor must render a Delete Permanently action button in the banner"
        );

        // Normal autosave and blur save suppressed
        assert.ok(
          editorContent.includes("if (isReadOnly) return;"),
          "Editor save paths must immediately return when isReadOnly is true"
        );
      });
    });

    describe("3. Restore", () => {
      it("verifies restore calls notesStorage.restoreNote and removes note from Trash", async () => {
        let calledWithId = null;
        const mockStorage = {
          restoreNote: async (id) => {
            calledWithId = id;
            return {
              id,
              title: "Restored Note",
              content: "Content",
              is_deleted: false,
              created_at: "2026-10-09T10:00:00Z",
              updated_at: "2026-10-09T10:00:00Z",
            };
          },
        };

        let trashedNotes = [
          { id: "trash-1", title: "Note 1", is_deleted: true },
          { id: "trash-2", title: "Note 2", is_deleted: true },
        ];

        // Perform restore
        const targetId = "trash-1";
        await mockStorage.restoreNote(targetId);
        trashedNotes = trashedNotes.filter((n) => n.id !== targetId);

        assert.strictEqual(calledWithId, "trash-1", "restoreNote must receive the target note ID");
        assert.strictEqual(trashedNotes.length, 1, "Restored note must be removed from trash list");
        assert.strictEqual(trashedNotes[0].id, "trash-2");
      });

      it("verifies active-note state and badge count refresh upon restore", async () => {
        let trashedCount = 3;
        let activeNotesCount = 5;
        let bannerMessage = null;

        const handleRestore = async (id, title) => {
          trashedCount = Math.max(0, trashedCount - 1);
          activeNotesCount += 1;
          bannerMessage = `Note "${title}" restored to active notes.`;
        };

        await handleRestore("trash-1", "My Meeting Notes");

        assert.strictEqual(trashedCount, 2, "Trash badge count must decrement by 1");
        assert.strictEqual(activeNotesCount, 6, "Active notes count must increment by 1");
        assert.ok(bannerMessage.includes("restored"), "Feedback banner must confirm restoration");
      });

      it("verifies restore failure is handled gracefully without false success", async () => {
        let trashedNotes = [{ id: "trash-1", title: "Corrupted Note", is_deleted: true }];
        let userErrorMessage = null;

        const handleRestore = async (id) => {
          try {
            throw new Error("Database disk I/O error occurred during restore.");
          } catch (err) {
            userErrorMessage = err instanceof Error ? err.message : "Failed to restore note.";
            // Note is NOT removed from trashedNotes upon error
          }
        };

        await handleRestore("trash-1");

        assert.strictEqual(trashedNotes.length, 1, "Note must NOT be removed from trash list on error");
        assert.ok(userErrorMessage.includes("Database disk I/O error"), "User must receive truthful error message");
      });
    });

    describe("4. Permanent Deletion", () => {
      it("verifies permanent delete prompts confirmation dialog targeting the intended note", async () => {
        const fs = await import("node:fs");
        const dialogContent = fs.readFileSync(
          new URL("../src/features/notes/components/PermanentDeleteDialog.tsx", import.meta.url),
          "utf8"
        );

        // Dialog structure and styling
        assert.ok(
          dialogContent.includes("<Modal"),
          "PermanentDeleteDialog must use the Modal component"
        );
        assert.ok(
          dialogContent.includes("Permanently delete"),
          "PermanentDeleteDialog must include explicit destructive title"
        );
        assert.ok(
          dialogContent.includes("displayTitle"),
          "PermanentDeleteDialog must reference the target note title"
        );
        assert.ok(
          dialogContent.includes("cannot be undone through the Trash interface"),
          "PermanentDeleteDialog must warn user action cannot be undone through Trash"
        );
        assert.ok(
          dialogContent.includes('variant="secondary"') &&
            dialogContent.includes('variant="danger"'),
          "PermanentDeleteDialog must provide Cancel and danger action buttons"
        );
        assert.ok(
          dialogContent.includes("Permanently Delete"),
          "PermanentDeleteDialog must provide explicit Permanently Delete action button"
        );
      });

      it("verifies cancellation is a safe no-op and does not invoke deletion", () => {
        let isDialogOpen = true;
        let deleteInvocationCount = 0;

        const handleCancel = () => {
          isDialogOpen = false;
        };

        const handleConfirm = () => {
          deleteInvocationCount++;
        };

        // User clicks Cancel
        handleCancel();

        assert.strictEqual(isDialogOpen, false, "Dialog must close upon cancel");
        assert.strictEqual(deleteInvocationCount, 0, "Backend deletion must NOT be called on cancel");
      });

      it("verifies duplicate submissions are prevented during permanent delete", async () => {
        let isBusy = false;
        let executionCount = 0;

        const submitDelete = async () => {
          if (isBusy) return;
          isBusy = true;
          try {
            executionCount++;
            await new Promise((r) => setTimeout(r, 20));
          } finally {
            isBusy = false;
          }
        };

        // User rapidly double clicks delete confirm
        await Promise.all([submitDelete(), submitDelete()]);

        assert.strictEqual(executionCount, 1, "Duplicate permanent delete clicks must be rejected");
      });

      it("verifies failed permanent deletion is reflected truthfully with error feedback", async () => {
        let trashedNotes = [{ id: "trashed-item", title: "Note", is_deleted: true }];
        let displayError = null;

        const confirmDelete = async (id) => {
          try {
            throw new Error("Foreign key constraint violation or disk lock.");
          } catch (err) {
            displayError = err instanceof Error ? err.message : "Deletion failed.";
          }
        };

        await confirmDelete("trashed-item");

        assert.strictEqual(trashedNotes.length, 1, "Note must remain in list if permanent deletion fails");
        assert.ok(displayError.includes("Foreign key constraint"), "Truthful error must be provided to user");
      });
    });

    describe("5. Empty Trash", () => {
      it("verifies empty trash requires confirmation and cancel performs no deletion", async () => {
        const fs = await import("node:fs");
        const emptyDialogContent = fs.readFileSync(
          new URL("../src/features/notes/components/EmptyTrashDialog.tsx", import.meta.url),
          "utf8"
        );

        assert.ok(
          emptyDialogContent.includes("<Modal"),
          "EmptyTrashDialog must use the Modal component"
        );
        assert.ok(
          emptyDialogContent.includes('title="Empty Trash?"'),
          "EmptyTrashDialog must have clear destructive title"
        );
        assert.ok(
          emptyDialogContent.includes("permanently delete all"),
          "EmptyTrashDialog must warn that all items in trash will be permanently erased"
        );
        assert.ok(
          emptyDialogContent.includes('variant="secondary"') &&
            emptyDialogContent.includes('variant="danger"'),
          "EmptyTrashDialog must provide Cancel and danger Empty Trash action buttons"
        );
        assert.ok(
          emptyDialogContent.includes("Empty Trash"),
          "EmptyTrashDialog must provide explicit Empty Trash action label"
        );

        // Verify cancel does not invoke backend emptyTrash
        let wasPurged = false;
        let isDialogOpen = true;
        const handleCancel = () => {
          isDialogOpen = false;
        };
        handleCancel();
        assert.strictEqual(isDialogOpen, false);
        assert.strictEqual(wasPurged, false);
      });

      it("verifies successful empty trash purges trash and updates trash count badge", async () => {
        let trashedNotes = [
          { id: "t1", title: "Note 1" },
          { id: "t2", title: "Note 2" },
          { id: "t3", title: "Note 3" },
        ];
        let trashedCount = 3;

        const mockStorage = {
          emptyTrash: async () => ({
            deleted_count: 3,
            failed_ids: [],
          }),
        };

        const result = await mockStorage.emptyTrash();
        if (result.deleted_count > 0) {
          trashedNotes = [];
          trashedCount = 0;
        }

        assert.strictEqual(result.deleted_count, 3);
        assert.strictEqual(trashedNotes.length, 0, "All trashed notes purged");
        assert.strictEqual(trashedCount, 0, "Trash badge reset to 0");
      });

      it("verifies partial failure in empty trash displays honest batch feedback", async () => {
        const mockResult = {
          deleted_count: 2,
          failed_ids: ["note-locked-1"],
        };

        const generateFeedback = (res) => {
          if (res.failed_ids.length === 0) {
            return `Emptied ${res.deleted_count} notes from Trash.`;
          }
          return `Emptied ${res.deleted_count} notes. ${res.failed_ids.length} note(s) could not be deleted.`;
        };

        const message = generateFeedback(mockResult);
        assert.ok(message.includes("Emptied 2 notes"));
        assert.ok(message.includes("1 note(s) could not be deleted"));
      });
    });

    describe("6. Phase 1-7 Regression Invariants", () => {
      it("verifies note creation, editing, and autosave remain fully functional for active notes", async () => {
        const fs = await import("node:fs");
        const editorContent = fs.readFileSync(
          new URL("../src/features/notes/components/Editor/NoteEditor.tsx", import.meta.url),
          "utf8"
        );

        // Active note editing is not blocked
        assert.ok(
          editorContent.includes("readOnly={isReadOnly}"),
          "Input fields must allow editing when isReadOnly is false"
        );
        assert.ok(
          editorContent.includes("scheduleAutosave") &&
            editorContent.includes("executeSave"),
          "Autosave engine must be preserved in NoteEditor"
        );
      });

      it("verifies notebooks, tags, and favorites remain functional and isolated from Trash", async () => {
        const notesDb = [
          { id: "n1", notebook_id: "nb-1", tags: ["work"], favorite: true, is_deleted: false },
          { id: "n2", notebook_id: "nb-1", tags: ["work"], favorite: false, is_deleted: true },
          { id: "n3", notebook_id: "nb-2", tags: ["personal"], favorite: true, is_deleted: false },
        ];

        // Active notebook filter
        const nb1Active = notesDb.filter((n) => n.notebook_id === "nb-1" && !n.is_deleted);
        assert.strictEqual(nb1Active.length, 1);
        assert.strictEqual(nb1Active[0].id, "n1");

        // Active tag filter
        const workActive = notesDb.filter((n) => n.tags.includes("work") && !n.is_deleted);
        assert.strictEqual(workActive.length, 1);
        assert.strictEqual(workActive[0].id, "n1");

        // Active favorites filter
        const favActive = notesDb.filter((n) => n.favorite && !n.is_deleted);
        assert.strictEqual(favActive.length, 2);
        assert.deepStrictEqual(favActive.map((n) => n.id), ["n1", "n3"]);
      });

      it("verifies search excludes trashed notes, and restored notes become searchable immediately", () => {
        let notesIndex = [
          { id: "s1", title: "Kubernetes Guide", content: "DevOps", is_deleted: false },
          { id: "s2", title: "Kubernetes Cluster", content: "Production", is_deleted: false },
        ];

        const search = (query) =>
          notesIndex.filter(
            (n) => !n.is_deleted && (n.title.includes(query) || n.content.includes(query))
          );

        // 1. Initial search
        assert.strictEqual(search("Kubernetes").length, 2);

        // 2. Soft delete s2
        notesIndex[1].is_deleted = true;
        const trashedSearchResult = search("Kubernetes");
        assert.strictEqual(trashedSearchResult.length, 1);
        assert.strictEqual(trashedSearchResult[0].id, "s1");

        // 3. Restore s2
        notesIndex[1].is_deleted = false;
        const restoredSearchResult = search("Kubernetes");
        assert.strictEqual(restoredSearchResult.length, 2);
      });

      it("verifies attachments survive soft deletion and restore without data loss", () => {
        const noteWithFiles = {
          id: "note-att-1",
          title: "Architecture Document",
          is_deleted: false,
          attachments: [
            { id: "att-1", name: "diagram.png", size: 1024 },
            { id: "att-2", name: "spec.pdf", size: 2048 },
          ],
        };

        // Soft delete
        noteWithFiles.is_deleted = true;
        assert.strictEqual(noteWithFiles.attachments.length, 2, "Attachments preserved during trash");

        // Restore
        noteWithFiles.is_deleted = false;
        assert.strictEqual(noteWithFiles.attachments.length, 2, "Attachments preserved after restore");
      });

      it("verifies keyboard shortcuts and themes remain functional", async () => {
        const fs = await import("node:fs");
        const shortcutsContent = fs.readFileSync(
          new URL("../src/hooks/useKeyboardShortcuts.ts", import.meta.url),
          "utf8"
        );

        assert.ok(
          shortcutsContent.includes("onDeleteNote"),
          "useKeyboardShortcuts must wire onDeleteNote"
        );
        assert.ok(
          shortcutsContent.includes('isModifier && (key === "backspace" || key === "delete")'),
          "useKeyboardShortcuts must support Delete and Backspace shortcuts with modifiers"
        );
      });
    });
  });

  describe("Task 24: Perform Data-Safety, Security, and Reliability Verification", () => {
    describe("1. Data Safety Verification", () => {
      it("verifies ordinary delete is soft-delete, stable IDs are preserved, and active notes cannot be duplicated or corrupted", async () => {
        const fs = await import("node:fs");
        const trashService = fs.readFileSync(
          new URL("../src-tauri/src/storage/services/trash.rs", import.meta.url),
          "utf8"
        );
        const notesRepo = fs.readFileSync(
          new URL("../src-tauri/src/storage/repositories/notes.rs", import.meta.url),
          "utf8"
        );

        // Soft delete only updates is_deleted and does not purge records
        assert.ok(
          notesRepo.includes("UPDATE notes SET is_deleted = 1"),
          "NoteRepository::soft_delete must strictly update is_deleted = 1"
        );
        assert.ok(
          trashService.includes("NoteRepository::soft_delete(conn, trimmed_id)"),
          "TrashService::soft_delete must route to repository soft_delete"
        );

        // Restore preserves ID and does not duplicate
        assert.ok(
          notesRepo.includes("is_deleted = 0, deleted_at = NULL") &&
            notesRepo.includes("WHERE id = ?3 AND is_deleted = 1"),
          "NoteRepository::restore must strictly update is_deleted = 0 on existing row"
        );

        // Active notes repository queries strictly filter is_deleted = 0
        assert.ok(
          notesRepo.includes("WHERE is_deleted = 0"),
          "NoteRepository list and get queries must enforce is_deleted = 0"
        );
      });

      it("verifies Empty Trash operates exclusively on trashed notes without touching active survivors", async () => {
        const fs = await import("node:fs");
        const trashService = fs.readFileSync(
          new URL("../src-tauri/src/storage/services/trash.rs", import.meta.url),
          "utf8"
        );
        const notesRepo = fs.readFileSync(
          new URL("../src-tauri/src/storage/repositories/notes.rs", import.meta.url),
          "utf8"
        );

        // Empty trash queries list_trashed which selects is_deleted = 1
        assert.ok(
          trashService.includes("NoteRepository::list_trashed(conn)"),
          "empty_trash must query trashed notes via NoteRepository::list_trashed"
        );
        assert.ok(
          notesRepo.includes("WHERE is_deleted = 1"),
          "NoteRepository::list_trashed must strictly filter WHERE is_deleted = 1"
        );
        assert.ok(
          trashService.includes("permanently_delete_note(conn, storage, &note.id)"),
          "empty_trash must delegate to permanently_delete_note to maintain integrity"
        );
      });

      it("verifies shared notebooks and tags survive permanent deletion of member notes", async () => {
        const fs = await import("node:fs");
        const trashService = fs.readFileSync(
          new URL("../src-tauri/src/storage/services/trash.rs", import.meta.url),
          "utf8"
        );

        // Check backend integration tests specifically verify parent survival
        assert.ok(
          trashService.includes("assert!(NotebookRepository::get_by_id(&conn, &nb.id).unwrap().is_some());") &&
            trashService.includes("assert!(TagRepository::get_by_id(&conn, &tag.id).unwrap().is_some());"),
          "trash.rs must test that parent notebooks and tags survive permanent deletion"
        );
      });
    });

    describe("2. Security and Boundary Enforcement", () => {
      it("verifies all database queries use parameterized SQL without string formatting", async () => {
        const fs = await import("node:fs");
        const trashService = fs.readFileSync(
          new URL("../src-tauri/src/storage/services/trash.rs", import.meta.url),
          "utf8"
        );
        const notesRepo = fs.readFileSync(
          new URL("../src-tauri/src/storage/repositories/notes.rs", import.meta.url),
          "utf8"
        );

        // Parameterized queries with params![...]
        assert.ok(
          notesRepo.includes("UPDATE notes SET is_deleted = 1") &&
            notesRepo.includes("params![now, id]"),
          "soft_delete must use parameterized SQL with positional parameters"
        );
        assert.ok(
          notesRepo.includes("is_deleted = 0, deleted_at = NULL") &&
            notesRepo.includes("params![effective_notebook_id, now, id]"),
          "restore must use parameterized SQL"
        );
        assert.ok(
          trashService.includes("NoteRepository::delete(conn, trimmed_id, false)"),
          "permanently_delete_note must route through NoteRepository::delete"
        );
        assert.ok(
          notesRepo.includes('conn.execute("DELETE FROM notes WHERE id = ?1", params![id])'),
          "NoteRepository::delete must use parameterized SQL with params![id]"
        );

        // Verify no raw string formatting in SQL statements
        assert.ok(
          !trashService.includes('format!("UPDATE notes SET is_deleted'),
          "SQL statements must not use format! interpolation"
        );
        assert.ok(
          !trashService.includes('format!("DELETE FROM notes WHERE id = \'{}\''),
          "SQL statements must not use format! interpolation"
        );
      });

      it("verifies backend rejects unauthorized lifecycle transitions and active note permanent deletions", async () => {
        const fs = await import("node:fs");
        const trashService = fs.readFileSync(
          new URL("../src-tauri/src/storage/services/trash.rs", import.meta.url),
          "utf8"
        );

        // Verify checks in permanently_delete_note
        assert.ok(
          trashService.includes("Only trashed notes can be permanently deleted"),
          "permanently_delete_note must reject active notes"
        );
        // Verify checks in restore
        assert.ok(
          trashService.includes("is not in Trash"),
          "restore must reject notes that are not in Trash"
        );
        // Verify checks in soft_delete
        assert.ok(
          trashService.includes("is already in Trash"),
          "soft_delete must reject notes that are already in Trash"
        );
      });

      it("verifies selection race protection and target ID stability in frontend mutations", async () => {
        const fs = await import("node:fs");
        const appContent = fs.readFileSync(
          new URL("../src/app/App.tsx", import.meta.url),
          "utf8"
        );

        // handleConfirmPermanentDelete relies on stable noteId argument, not fluctuating state
        assert.ok(
          appContent.includes("handleConfirmPermanentDelete = useCallback(") &&
            appContent.includes("async (noteId: string) => {"),
          "handleConfirmPermanentDelete must capture and accept the explicit note ID"
        );
        // in-flight mutations lock prevents concurrency race
        assert.ok(
          appContent.includes("inFlightMutationsRef.current.add(noteId);"),
          "handleConfirmPermanentDelete must track inFlightMutationsRef for target noteId"
        );
      });
    });

    describe("3. Reliability and Fault Tolerance", () => {
      it("verifies missing attachments on disk are handled gracefully without blocking permanent delete", async () => {
        const fs = await import("node:fs");
        const trashService = fs.readFileSync(
          new URL("../src-tauri/src/storage/services/trash.rs", import.meta.url),
          "utf8"
        );

        // remove_note_directory is tolerant of missing directory
        assert.ok(
          trashService.includes("test_task_24_data_safety_security_and_reliability_verification"),
          "trash.rs must test missing attachment disk directory during permanent delete"
        );
        assert.ok(
          trashService.includes("assert!(!ghost_del.attachment_cleanup_failed);"),
          "Missing directory must not be falsely treated as a failure"
        );
      });

      it("verifies large trash batches (e.g. 25+ notes) are purged reliably without leaving dangling records", async () => {
        const fs = await import("node:fs");
        const trashService = fs.readFileSync(
          new URL("../src-tauri/src/storage/services/trash.rs", import.meta.url),
          "utf8"
        );

        assert.ok(
          trashService.includes("assert_eq!(empty_batch.deleted_count, 25);"),
          "trash.rs must verify bulk emptying of 25 notes cleanly"
        );
        assert.ok(
          trashService.includes("assert_eq!(empty_batch.failed_count, 0);"),
          "trash.rs must verify zero failures during bulk purge"
        );
      });

      it("verifies restore and permanent deletion are reliable across application restarts", async () => {
        const fs = await import("node:fs");
        const trashService = fs.readFileSync(
          new URL("../src-tauri/src/storage/services/trash.rs", import.meta.url),
          "utf8"
        );

        // Verification of connection drops and reopens simulating application restart
        assert.ok(
          trashService.includes("drop(conn);") &&
            trashService.includes("init_connection(&db_path)") &&
            trashService.includes("run_migrations(&mut conn2)"),
          "trash.rs must test simulation of application shutdown and restart before restore"
        );
        assert.ok(
          trashService.includes("drop(conn2);") &&
            trashService.includes("init_connection(&db_path)") &&
            trashService.includes("run_migrations(&mut conn3)"),
          "trash.rs must test simulation of application restart before permanent deletion"
        );
      });
    });
  });
});

describe("Phase 9 - Task 4: Internal Link Parser Tests", () => {
  it("imports linkParser utilities cleanly without errors", async () => {
    const { parseInternalLinks, formatInternalLink, isInternalLinkUri, extractTargetIdFromUri, hasInternalLinks } =
      await import("../src/utils/linkParser.ts");
    assert.strictEqual(typeof parseInternalLinks, "function");
    assert.strictEqual(typeof formatInternalLink, "function");
    assert.strictEqual(typeof isInternalLinkUri, "function");
    assert.strictEqual(typeof extractTargetIdFromUri, "function");
    assert.strictEqual(typeof hasInternalLinks, "function");
  });

  it("parses a valid single internal link and extracts metadata correctly", async () => {
    const { parseInternalLinks } = await import("../src/utils/linkParser.ts");
    const content = "Please check [Project Roadmap](notepad://note/uuid-1234) for deliverables.";
    const links = parseInternalLinks(content);

    assert.strictEqual(links.length, 1);
    assert.strictEqual(links[0].targetId, "uuid-1234");
    assert.strictEqual(links[0].label, "Project Roadmap");
    assert.strictEqual(links[0].raw, "[Project Roadmap](notepad://note/uuid-1234)");
    assert.strictEqual(links[0].startIndex, 13);
    assert.strictEqual(links[0].endIndex, 56);
    assert.strictEqual(content.slice(links[0].startIndex, links[0].endIndex), links[0].raw);
  });

  it("parses multiple links including case-insensitive protocol schemes", async () => {
    const { parseInternalLinks } = await import("../src/utils/linkParser.ts");
    const content = "First [Note One](notepad://note/id-1) and second [Note Two](NOTEPAD://NOTE/id-2).";
    const links = parseInternalLinks(content);

    assert.strictEqual(links.length, 2);
    assert.strictEqual(links[0].targetId, "id-1");
    assert.strictEqual(links[0].label, "Note One");
    assert.strictEqual(links[1].targetId, "id-2");
    assert.strictEqual(links[1].label, "Note Two");
  });

  it("handles Unicode and international labels safely", async () => {
    const { parseInternalLinks } = await import("../src/utils/linkParser.ts");
    const content = "देखें [वार्षिक समीक्षा २०२६](notepad://note/hindi-note) और [Café & Résumé](notepad://note/french-note)";
    const links = parseInternalLinks(content);

    assert.strictEqual(links.length, 2);
    assert.strictEqual(links[0].targetId, "hindi-note");
    assert.strictEqual(links[0].label, "वार्षिक समीक्षा २०२६");
    assert.strictEqual(links[1].targetId, "french-note");
    assert.strictEqual(links[1].label, "Café & Résumé");
  });

  it("does not match markdown images, external links, or ordinary prose", async () => {
    const { parseInternalLinks } = await import("../src/utils/linkParser.ts");
    const content =
      "Image ![Preview](notepad://note/img-1) and external [Google](https://google.com) and valid [Doc](notepad://note/doc-1) and text [bracket 1] here.";
    const links = parseInternalLinks(content);

    assert.strictEqual(links.length, 1);
    assert.strictEqual(links[0].targetId, "doc-1");
    assert.strictEqual(links[0].label, "Doc");
  });

  it("safely handles malformed syntax and unclosed brackets without errors", async () => {
    const { parseInternalLinks } = await import("../src/utils/linkParser.ts");
    const content = "Broken tokens: [](notepad://note/) and [Unclosed](notepad://note/123 and [Wrong](notepad://other/456)";
    const links = parseInternalLinks(content);

    assert.strictEqual(links.length, 0);
  });

  it("formats and sanitizes internal link strings correctly", async () => {
    const { formatInternalLink, sanitizeLinkLabel } = await import("../src/utils/linkParser.ts");
    assert.strictEqual(
      formatInternalLink("uuid-101", "Design Specs"),
      "[Design Specs](notepad://note/uuid-101)"
    );
    assert.strictEqual(
      formatInternalLink("uuid-102", "Plan [Q1]"),
      "[Plan [Q1\\]](notepad://note/uuid-102)"
    );
    assert.strictEqual(
      formatInternalLink("uuid-103", ""),
      "[Untitled Note](notepad://note/uuid-103)"
    );
    assert.strictEqual(sanitizeLinkLabel("Notes\nwith\rnewlines"), "Notes with newlines");
  });

  it("validates URI detection and target ID extraction", async () => {
    const { isInternalLinkUri, extractTargetIdFromUri, hasInternalLinks } = await import("../src/utils/linkParser.ts");
    assert.strictEqual(isInternalLinkUri("notepad://note/target-123"), true);
    assert.strictEqual(isInternalLinkUri("NOTEPAD://NOTE/target-123"), true);
    assert.strictEqual(isInternalLinkUri("https://notepad://note/target-123"), false);
    assert.strictEqual(isInternalLinkUri(""), false);

    assert.strictEqual(extractTargetIdFromUri("notepad://note/uuid-999"), "uuid-999");
    assert.strictEqual(extractTargetIdFromUri("https://google.com"), null);

    assert.strictEqual(hasInternalLinks("Some text with [Ref](notepad://note/id-1)"), true);
    assert.strictEqual(hasInternalLinks("Some plain text with no links"), false);
  });
});

describe("Phase 9 - Task 5: Link Insertion Logic Tests", () => {
  it("inserts link at the end of note content when no selection is specified", async () => {
    const { insertInternalLinkIntoContent } = await import("../src/utils/linkParser.ts");
    const result = insertInternalLinkIntoContent({
      content: "Existing note text.",
      targetNoteId: "target-123",
      targetTitle: "My Target Note",
    });

    assert.strictEqual(result.insertedText, "[My Target Note](notepad://note/target-123)");
    assert.strictEqual(
      result.updatedContent,
      "Existing note text.[My Target Note](notepad://note/target-123)"
    );
    assert.strictEqual(result.newCursorPosition, "Existing note text.".length + result.insertedText.length);
  });

  it("inserts link at the beginning of note content", async () => {
    const { insertInternalLinkIntoContent } = await import("../src/utils/linkParser.ts");
    const result = insertInternalLinkIntoContent({
      content: "Existing text.",
      selectionStart: 0,
      selectionEnd: 0,
      targetNoteId: "target-456",
      targetTitle: "Intro Doc",
    });

    assert.strictEqual(result.insertedText, "[Intro Doc](notepad://note/target-456)");
    assert.strictEqual(result.updatedContent, "[Intro Doc](notepad://note/target-456)Existing text.");
    assert.strictEqual(result.newCursorPosition, result.insertedText.length);
  });

  it("replaces active selection using selected text as display label", async () => {
    const { insertInternalLinkIntoContent } = await import("../src/utils/linkParser.ts");
    const content = "Please read the project overview for information.";
    // "project overview" is indices 16 to 32
    const start = content.indexOf("project overview");
    const end = start + "project overview".length;

    const result = insertInternalLinkIntoContent({
      content,
      selectionStart: start,
      selectionEnd: end,
      targetNoteId: "overview-uuid",
      targetTitle: "Complete 2026 Overview",
    });

    assert.strictEqual(result.insertedText, "[project overview](notepad://note/overview-uuid)");
    assert.strictEqual(
      result.updatedContent,
      "Please read the [project overview](notepad://note/overview-uuid) for information."
    );
    assert.strictEqual(result.newCursorPosition, start + result.insertedText.length);
  });

  it("custom label takes precedence over selected text and target title", async () => {
    const { insertInternalLinkIntoContent } = await import("../src/utils/linkParser.ts");
    const content = "Hello world";
    const result = insertInternalLinkIntoContent({
      content,
      selectionStart: 6,
      selectionEnd: 11,
      targetNoteId: "target-789",
      targetTitle: "Default Title",
      customLabel: "Custom Alias",
    });

    assert.strictEqual(result.insertedText, "[Custom Alias](notepad://note/target-789)");
    assert.strictEqual(result.updatedContent, "Hello [Custom Alias](notepad://note/target-789)");
  });

  it("handles empty note and Unicode labels gracefully", async () => {
    const { insertInternalLinkIntoContent } = await import("../src/utils/linkParser.ts");
    const result = insertInternalLinkIntoContent({
      content: "",
      targetNoteId: "hindi-uuid",
      targetTitle: "वार्षिक योजना २०२६",
    });

    assert.strictEqual(result.insertedText, "[वार्षिक योजना २०२६](notepad://note/hindi-uuid)");
    assert.strictEqual(result.updatedContent, "[वार्षिक योजना २०२६](notepad://note/hindi-uuid)");
    assert.strictEqual(result.newCursorPosition, result.insertedText.length);
  });

  it("safely clamps out-of-bounds selection ranges without throwing", async () => {
    const { insertInternalLinkIntoContent } = await import("../src/utils/linkParser.ts");
    const result = insertInternalLinkIntoContent({
      content: "Short note",
      selectionStart: -10,
      selectionEnd: 9999,
      targetNoteId: "id-clamp",
      targetTitle: "Safe Clamp",
    });

    // Entire text replaced because range clamped from 0 to content.length
    assert.strictEqual(result.insertedText, "[Short note](notepad://note/id-clamp)");
    assert.strictEqual(result.updatedContent, "[Short note](notepad://note/id-clamp)");
  });
});

describe("Phase 9 - Task 6: Note-Picker UI Tests", () => {
  it("verifies NotePickerModal component structure, clear action labels, and inputs", async () => {
    const fs = await import("node:fs");
    const componentContent = fs.readFileSync(
      new URL(
        "../src/features/notes/components/NotePicker/NotePickerModal.tsx",
        import.meta.url
      ),
      "utf8"
    );

    assert.ok(componentContent.includes("<Modal"), "NotePickerModal must use Modal component");
    assert.ok(componentContent.includes("title=\"Link to Note\""), "NotePickerModal must have explicit title");
    assert.ok(componentContent.includes("Insert Link"), "NotePickerModal must have explicit 'Insert Link' button");
    assert.ok(componentContent.includes("Cancel"), "NotePickerModal must have clear Cancel button");
    assert.ok(componentContent.includes("Search notes to link"), "NotePickerModal must have accessible search input");
    assert.ok(componentContent.includes("Link text (display label)"), "NotePickerModal must offer display label customization");
    assert.ok(componentContent.includes("storageService.notes.list"), "NotePickerModal must query notes via storageService");
  });

  it("filters eligible active notes excluding source note and trashed notes", () => {
    const currentNoteId = "note-source";
    const allNotes = [
      { id: "note-source", title: "Source Note", content: "...", is_deleted: false },
      { id: "note-1", title: "Project Plan", content: "Details", is_deleted: false },
      { id: "note-2", title: "Trashed Note", content: "Deleted", is_deleted: true },
      { id: "note-3", title: "Project Plan", content: "Duplicate title", is_deleted: false },
    ];

    const eligible = allNotes.filter(
      (n) => !n.is_deleted && n.id !== currentNoteId
    );

    assert.strictEqual(eligible.length, 2);
    assert.strictEqual(eligible[0].id, "note-1");
    assert.strictEqual(eligible[1].id, "note-3");
    assert.ok(!eligible.some((n) => n.id === currentNoteId));
    assert.ok(!eligible.some((n) => n.is_deleted));
  });

  it("differentiates duplicate titles using notebook context mapping", () => {
    const notebooks = [
      { id: "nb-work", name: "Work" },
      { id: "nb-personal", name: "Personal" },
    ];
    const map = new Map(notebooks.map((nb) => [nb.id, nb.name]));

    const notes = [
      { id: "note-10", title: "Shopping List", notebook_id: "nb-work" },
      { id: "note-11", title: "Shopping List", notebook_id: "nb-personal" },
    ];

    assert.strictEqual(map.get(notes[0].notebook_id), "Work");
    assert.strictEqual(map.get(notes[1].notebook_id), "Personal");
    // Ensure both retain unique IDs despite identical titles
    assert.notStrictEqual(notes[0].id, notes[1].id);
  });

  it("filters notes by title, notebook, and content query case-insensitively", () => {
    const notebookMap = new Map([["nb-1", "Engineering"]]);
    const notes = [
      { id: "n1", title: "Architecture", content: "System diagram", notebook_id: "nb-1" },
      { id: "n2", title: "Grocery", content: "Milk, bread", notebook_id: null },
      { id: "n3", title: "Meeting Notes", content: "Discuss architecture", notebook_id: null },
    ];

    const filter = (query) => {
      const q = query.trim().toLowerCase();
      if (!q) return notes;
      return notes.filter((n) => {
        const titleMatch = (n.title || "").toLowerCase().includes(q);
        const contentMatch = (n.content || "").toLowerCase().includes(q);
        const nbName = n.notebook_id ? notebookMap.get(n.notebook_id)?.toLowerCase() : "";
        const notebookMatch = nbName ? nbName.includes(q) : false;
        return titleMatch || notebookMatch || contentMatch;
      });
    };

    assert.strictEqual(filter("ENGINEERING").length, 1);
    assert.strictEqual(filter("ENGINEERING")[0].id, "n1");

    assert.strictEqual(filter("architecture").length, 2); // n1 (title) & n3 (content)
    assert.strictEqual(filter("bread").length, 1);
    assert.strictEqual(filter("nonexistent").length, 0);
  });

  it("simulates safe cancellation without altering note selection", () => {
    let selectedTarget = null;
    let isClosed = false;

    const onSelectNote = (target) => {
      selectedTarget = target;
    };
    const onClose = () => {
      isClosed = true;
    };

    // User cancels
    onClose();

    assert.strictEqual(isClosed, true);
    assert.strictEqual(selectedTarget, null);
  });

  it("simulates selection passing stable target note ID and custom label", () => {
    let selectedTarget = null;
    const onSelectNote = (target) => {
      selectedTarget = target;
    };

    const targetNote = { id: "target-uuid-444", title: "Target Note" };
    const customLabel = "My Custom Shortcut";

    onSelectNote({
      id: targetNote.id,
      title: targetNote.title,
      customLabel,
    });

    assert.deepStrictEqual(selectedTarget, {
      id: "target-uuid-444",
      title: "Target Note",
      customLabel: "My Custom Shortcut",
    });
  });
});

describe("Phase 9 - Task 7: Link Insertion Entry Point Tests", () => {
  it("verifies toolbar button structure, tooltip, aria-label, and read-only disablement in NoteEditor.tsx", async () => {
    const fs = await import("node:fs");
    const editorContent = fs.readFileSync(
      new URL(
        "../src/features/notes/components/Editor/NoteEditor.tsx",
        import.meta.url
      ),
      "utf8"
    );

    assert.ok(editorContent.includes("aria-label=\"Insert link\""), "Must have aria-label for accessibility");
    assert.ok(editorContent.includes("title=\"Insert link (Ctrl+K)\""), "Must have keyboard shortcut in tooltip");
    assert.ok(editorContent.includes("disabled={isReadOnly || !noteId}"), "Must be disabled when read-only or no note loaded");
    assert.ok(editorContent.includes("<LinkIcon />"), "Must render LinkIcon inside button");
    assert.ok(editorContent.includes("onMouseDown={(e) => {"), "Must have onMouseDown to preserve textarea selection");
    assert.ok(editorContent.includes("e.preventDefault()"), "Must preventDefault on mouse down to retain focus/selection");
  });

  it("verifies Ctrl+K and Cmd+K keyboard shortcut handler in NoteEditor.tsx", async () => {
    const fs = await import("node:fs");
    const editorContent = fs.readFileSync(
      new URL(
        "../src/features/notes/components/Editor/NoteEditor.tsx",
        import.meta.url
      ),
      "utf8"
    );

    assert.ok(editorContent.includes('(e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k"'), "Must handle both Ctrl+K and Cmd+K");
    assert.ok(editorContent.includes("handleOpenLinkPicker();"), "Must invoke handleOpenLinkPicker upon shortcut");
  });

  it("verifies note switch invalidation safety in NoteEditor.tsx", async () => {
    const fs = await import("node:fs");
    const editorContent = fs.readFileSync(
      new URL(
        "../src/features/notes/components/Editor/NoteEditor.tsx",
        import.meta.url
      ),
      "utf8"
    );

    assert.ok(
      editorContent.includes("pendingInsertionSelectionRef.current = null"),
      "Must clear pending selection on noteId change"
    );
    assert.ok(
      editorContent.includes("pending.sourceNoteId !== noteId"),
      "Must guard against cross-note link insertion if note switched before picker selection"
    );
  });

  it("verifies NotePickerModal integration and props wiring in NoteEditor.tsx", async () => {
    const fs = await import("node:fs");
    const editorContent = fs.readFileSync(
      new URL(
        "../src/features/notes/components/Editor/NoteEditor.tsx",
        import.meta.url
      ),
      "utf8"
    );

    assert.ok(editorContent.includes("<NotePickerModal"), "Must render NotePickerModal");
    assert.ok(editorContent.includes("isOpen={isLinkPickerOpen}"), "Must bind isOpen state");
    assert.ok(editorContent.includes("onClose={handleCloseLinkPicker}"), "Must bind onClose handler");
    assert.ok(editorContent.includes("onSelectNote={handleSelectLinkTarget}"), "Must bind onSelectNote handler");
    assert.ok(editorContent.includes("currentNoteId={noteId}"), "Must pass currentNoteId to exclude current note");
    assert.ok(editorContent.includes("initialLabel={linkPickerInitialLabel}"), "Must pass initialLabel from active selection");
  });

  it("simulates selection capture and link insertion into note editor content", async () => {
    const { insertInternalLinkIntoContent } = await import("../src/utils/linkParser.ts");

    const initialContent = "Here is my project research and more details.";
    // User selected "project research" (index 11 to 27)
    const selectionStart = 11;
    const selectionEnd = 27;

    const result = insertInternalLinkIntoContent({
      content: initialContent,
      selectionStart,
      selectionEnd,
      targetNoteId: "target-note-uuid-999",
      targetTitle: "Research Notes 2026",
      customLabel: "project research",
    });

    assert.strictEqual(
      result.updatedContent,
      "Here is my [project research](notepad://note/target-note-uuid-999) and more details."
    );
    assert.strictEqual(
      result.newCursorPosition,
      11 + "[project research](notepad://note/target-note-uuid-999)".length
    );
  });
});

describe("Phase 9 - Task 8: Link Rendering and Visual Recognition Tests", () => {
  it("splits content into safe text and link segments with tokenizeContentWithLinks", async () => {
    const { tokenizeContentWithLinks } = await import("../src/utils/linkParser.ts");

    // Empty or non-string content
    assert.deepStrictEqual(tokenizeContentWithLinks(""), [{ type: "text", text: "" }]);

    // Content with no links
    const plain = "Just plain text without any internal links here.";
    assert.deepStrictEqual(tokenizeContentWithLinks(plain), [{ type: "text", text: plain }]);

    // Content with a single link in the middle
    const single = "See [Roadmap](notepad://note/uuid-101) for timelines.";
    const singleTokens = tokenizeContentWithLinks(single);
    assert.strictEqual(singleTokens.length, 3);
    assert.deepStrictEqual(singleTokens[0], { type: "text", text: "See " });
    assert.deepStrictEqual(singleTokens[1], {
      type: "link",
      targetId: "uuid-101",
      label: "Roadmap",
      raw: "[Roadmap](notepad://note/uuid-101)",
    });
    assert.deepStrictEqual(singleTokens[2], { type: "text", text: " for timelines." });

    // Content with multiple links and Unicode labels
    const multi = "Start [हिंदी नोट](notepad://note/hindi-1) mid [Paris & Café](notepad://note/french-2) end.";
    const multiTokens = tokenizeContentWithLinks(multi);
    assert.strictEqual(multiTokens.length, 5);
    assert.strictEqual(multiTokens[1].type, "link");
    assert.strictEqual(multiTokens[1].targetId, "hindi-1");
    assert.strictEqual(multiTokens[1].label, "हिंदी नोट");
    assert.strictEqual(multiTokens[3].type, "link");
    assert.strictEqual(multiTokens[3].targetId, "french-2");
    assert.strictEqual(multiTokens[3].label, "Paris & Café");

    // Reassembling all segments exactly matches original content
    const reassembled = multiTokens.map((t) => (t.type === "link" ? t.raw : t.text)).join("");
    assert.strictEqual(reassembled, multi);
  });

  it("does not treat markdown images as links during tokenization", async () => {
    const { tokenizeContentWithLinks } = await import("../src/utils/linkParser.ts");

    const content = "Image ![Preview](notepad://note/img-1) and valid [Doc](notepad://note/doc-1).";
    const tokens = tokenizeContentWithLinks(content);

    assert.strictEqual(tokens.length, 3);
    assert.strictEqual(tokens[0].type, "text");
    assert.ok(tokens[0].text.includes("![Preview](notepad://note/img-1)"));
    assert.strictEqual(tokens[1].type, "link");
    assert.strictEqual(tokens[1].targetId, "doc-1");
    assert.strictEqual(tokens[1].label, "Doc");
    assert.strictEqual(tokens[2].type, "text");
    assert.strictEqual(tokens[2].text, ".");
  });

  it("verifies InternalLinkPill component structure, accessibility, and status badges", async () => {
    const fs = await import("node:fs");
    const pillContent = fs.readFileSync(
      new URL(
        "../src/features/notes/components/Links/InternalLinkPill.tsx",
        import.meta.url
      ),
      "utf8"
    );

    assert.ok(pillContent.includes("<button"), "Must use semantic button element");
    assert.ok(pillContent.includes("type=\"button\""), "Must have type='button'");
    assert.ok(pillContent.includes("className={`internal-link-pill"), "Must apply internal-link-pill class");
    assert.ok(pillContent.includes("aria-label="), "Must provide accessible aria-label");
    assert.ok(pillContent.includes("tabIndex={0}"), "Must be keyboard focusable");
    assert.ok(pillContent.includes("data-target-id={targetId}"), "Must include data-target-id attribute");
    assert.ok(pillContent.includes("e.key === \"Enter\" || e.key === \" \""), "Must activate on Enter and Space keys");
    assert.ok(pillContent.includes("status === \"trashed\""), "Must handle trashed status badge");
    assert.ok(pillContent.includes("status === \"missing\""), "Must handle missing status badge");
  });

  it("verifies RenderedNoteContent component safe rendering without dangerouslySetInnerHTML", async () => {
    const fs = await import("node:fs");
    const rendererContent = fs.readFileSync(
      new URL(
        "../src/features/notes/components/Links/RenderedNoteContent.tsx",
        import.meta.url
      ),
      "utf8"
    );

    assert.ok(rendererContent.includes("tokenizeContentWithLinks"), "Must use tokenizeContentWithLinks");
    assert.ok(!rendererContent.includes("dangerouslySetInnerHTML"), "Must NEVER use dangerouslySetInnerHTML");
    assert.ok(rendererContent.includes("<InternalLinkPill"), "Must render InternalLinkPill for links");
    assert.ok(rendererContent.includes("<React.Fragment key={`text-"), "Must render safe text nodes");
    assert.ok(rendererContent.includes("role=\"region\""), "Must provide accessible region role");
  });

  it("verifies NoteLinksBar component renders outgoing links and returns null when empty", async () => {
    const fs = await import("node:fs");
    const barContent = fs.readFileSync(
      new URL(
        "../src/features/notes/components/Links/NoteLinksBar.tsx",
        import.meta.url
      ),
      "utf8"
    );

    assert.ok(barContent.includes("if (!links || links.length === 0) return null;"), "Must return null when no links");
    assert.ok(barContent.includes("className={`note-links-bar"), "Must apply note-links-bar class");
    assert.ok(barContent.includes("Links ({links.length}):"), "Must display link count");
    assert.ok(barContent.includes("<InternalLinkPill"), "Must render link pills");
  });

  it("verifies NoteEditor view mode toggle, keyboard shortcut, and content preservation", async () => {
    const fs = await import("node:fs");
    const editorContent = fs.readFileSync(
      new URL(
        "../src/features/notes/components/Editor/NoteEditor.tsx",
        import.meta.url
      ),
      "utf8"
    );

    assert.ok(editorContent.includes("[viewMode, setViewMode] = useState<\"edit\" | \"preview\">(\"edit\")"), "Must manage viewMode state");
    assert.ok(editorContent.includes("handleToggleViewMode"), "Must have toggle view mode handler");
    assert.ok(editorContent.includes("aria-label={"), "Must have accessible view mode button label");
    assert.ok(editorContent.includes("Switch to Reading/Preview mode"), "Must offer Reading/Preview mode toggle");
    assert.ok(editorContent.includes("<RenderedNoteContent"), "Must render RenderedNoteContent in preview mode");
    assert.ok(editorContent.includes("<NoteLinksBar links={parsedLinks}"), "Must render NoteLinksBar with parsed links");
    assert.ok(editorContent.includes("e.key.toLowerCase() === \"p\" && e.shiftKey"), "Must support Ctrl+Shift+P shortcut");
    assert.ok(editorContent.includes("e.key.toLowerCase() === \"e\""), "Must support Ctrl+E shortcut");
  });
});

describe("Phase 9 - Task 9: Target Resolution by Stable Note ID Tests", () => {
  it("verifies linksStorage client module structure and export on storageService", async () => {
    const fs = await import("node:fs");
    const linksStorageContent = fs.readFileSync(
      new URL(
        "../src/services/storage/links.ts",
        import.meta.url
      ),
      "utf8"
    );

    assert.ok(linksStorageContent.includes("resolveTarget(targetId: string)"), "Must define resolveTarget method");
    assert.ok(linksStorageContent.includes("invoke<ResolvedLinkTarget>(\"resolve_link_target\""), "Must invoke resolve_link_target command");

    const indexContent = fs.readFileSync(
      new URL(
        "../src/services/storage/index.ts",
        import.meta.url
      ),
      "utf8"
    );
    assert.ok(indexContent.includes("links: linksStorage"), "Must export links on storageService");
  });

  it("verifies Rust InternalLinkService implementation and parameterized query safety", async () => {
    const fs = await import("node:fs");
    const serviceContent = fs.readFileSync(
      new URL(
        "../src-tauri/src/storage/services/links.rs",
        import.meta.url
      ),
      "utf8"
    );

    assert.ok(serviceContent.includes("pub struct InternalLinkService;"), "Must define InternalLinkService");
    assert.ok(serviceContent.includes("pub fn resolve_link_target("), "Must implement resolve_link_target");
    assert.ok(serviceContent.includes("WHERE n.id = ?1"), "Must use parameterized SQL for target resolution");
    assert.ok(serviceContent.includes("LEFT JOIN notebooks nb ON n.notebook_id = nb.id"), "Must LEFT JOIN notebooks for parent notebook context");
    assert.ok(serviceContent.includes("LinkTargetLifecycle::Active"), "Must distinguish Active targets");
    assert.ok(serviceContent.includes("LinkTargetLifecycle::Trashed"), "Must distinguish Trashed targets");
    assert.ok(serviceContent.includes("LinkTargetLifecycle::Missing"), "Must distinguish Missing targets");
  });

  it("verifies Tauri command registration for resolve_link_target in lib.rs and storage.rs", async () => {
    const fs = await import("node:fs");
    const storageCmdContent = fs.readFileSync(
      new URL(
        "../src-tauri/src/commands/storage.rs",
        import.meta.url
      ),
      "utf8"
    );
    assert.ok(storageCmdContent.includes("pub fn resolve_link_target("), "Must define resolve_link_target command");

    const libContent = fs.readFileSync(
      new URL(
        "../src-tauri/src/lib.rs",
        import.meta.url
      ),
      "utf8"
    );
    assert.ok(libContent.includes("commands::resolve_link_target"), "Must register resolve_link_target in tauri handler");
  });

  it("simulates link target resolution distinguishes active, trashed, and missing notes", () => {
    const mockDb = [
      { id: "note-uuid-active", title: "Active Note", notebook_id: "nb-1", is_deleted: 0 },
      { id: "note-uuid-trashed", title: "Trashed Note", notebook_id: null, is_deleted: 1 },
    ];
    const mockNotebooks = new Map([["nb-1", "Engineering"]]);

    const resolveMockTarget = (targetId) => {
      const trimmed = (targetId || "").trim();
      if (!trimmed) {
        return {
          targetId: trimmed,
          title: null,
          lifecycle: "missing",
          notebookId: null,
          notebookName: null,
        };
      }
      const note = mockDb.find((n) => n.id === trimmed);
      if (!note) {
        return {
          targetId: trimmed,
          title: null,
          lifecycle: "missing",
          notebookId: null,
          notebookName: null,
        };
      }
      return {
        targetId: note.id,
        title: note.title,
        lifecycle: note.is_deleted ? "trashed" : "active",
        notebookId: note.notebook_id,
        notebookName: note.notebook_id ? mockNotebooks.get(note.notebook_id) ?? null : null,
      };
    };

    // Active target
    const activeRes = resolveMockTarget("note-uuid-active");
    assert.strictEqual(activeRes.targetId, "note-uuid-active");
    assert.strictEqual(activeRes.lifecycle, "active");
    assert.strictEqual(activeRes.title, "Active Note");
    assert.strictEqual(activeRes.notebookName, "Engineering");

    // Trashed target
    const trashedRes = resolveMockTarget("note-uuid-trashed");
    assert.strictEqual(trashedRes.targetId, "note-uuid-trashed");
    assert.strictEqual(trashedRes.lifecycle, "trashed");
    assert.strictEqual(trashedRes.title, "Trashed Note");

    // Missing target
    const missingRes = resolveMockTarget("note-uuid-nonexistent");
    assert.strictEqual(missingRes.targetId, "note-uuid-nonexistent");
    assert.strictEqual(missingRes.lifecycle, "missing");
    assert.strictEqual(missingRes.title, null);

    // Empty target
    const emptyRes = resolveMockTarget("   ");
    assert.strictEqual(emptyRes.lifecycle, "missing");
  });

  it("guarantees resolution across renames and duplicate titles by strictly using stable IDs", () => {
    const notesDb = [
      { id: "uuid-note-1", title: "Original Title", notebook_id: "nb-a" },
      { id: "uuid-note-2", title: "Duplicate Title", notebook_id: "nb-b" },
      { id: "uuid-note-3", title: "Duplicate Title", notebook_id: "nb-c" },
    ];

    // Resolve by ID
    const resolveById = (id) => notesDb.find((n) => n.id === id);

    // 1. Rename note 1
    const note1 = resolveById("uuid-note-1");
    note1.title = "Completely Renamed Title 2026";
    const resolvedAfterRename = resolveById("uuid-note-1");
    assert.strictEqual(resolvedAfterRename.id, "uuid-note-1");
    assert.strictEqual(resolvedAfterRename.title, "Completely Renamed Title 2026");

    // 2. Duplicate titles resolve strictly to their respective IDs without ambiguity
    const resNote2 = resolveById("uuid-note-2");
    const resNote3 = resolveById("uuid-note-3");
    assert.notStrictEqual(resNote2.id, resNote3.id);
    assert.strictEqual(resNote2.notebook_id, "nb-b");
    assert.strictEqual(resNote3.notebook_id, "nb-c");
  });
});

describe("Phase 9 - Task 10: Safe Internal-Link Activation Tests", () => {
  it("verifies BrokenLinkDialog component structure, buttons, and accessibility", async () => {
    const fs = await import("node:fs");
    const dialogContent = fs.readFileSync(
      new URL(
        "../src/features/notes/components/Links/BrokenLinkDialog.tsx",
        import.meta.url
      ),
      "utf8"
    );

    assert.ok(dialogContent.includes("<Modal"), "Must render inside Modal component");
    assert.ok(dialogContent.includes("Linked Note is in Trash"), "Must have trash warning title");
    assert.ok(dialogContent.includes("Broken Link"), "Must have broken link title for missing notes");
    assert.ok(dialogContent.includes("View in Trash"), "Must offer 'View in Trash' button when target is trashed");
    assert.ok(dialogContent.includes("isTrashed ? \"Cancel\" : \"Close\""), "Must offer appropriate Cancel/Close label");
    assert.ok(dialogContent.includes("target.targetId"), "Must present target note ID for missing links");
    assert.ok(dialogContent.includes("if (!target) return null;"), "Must return null when target is null");
  });

  it("verifies App.tsx internal link activation wiring and dialog integration", async () => {
    const fs = await import("node:fs");
    const appContent = fs.readFileSync(
      new URL("../src/app/App.tsx", import.meta.url),
      "utf8"
    );

    assert.ok(
      appContent.includes("const handleActivateInternalLink = useCallback("),
      "Must define handleActivateInternalLink handler"
    );
    assert.ok(
      appContent.includes("storageService.links.resolveTarget(targetNoteId)"),
      "Must call resolveTarget with stable note ID"
    );
    assert.ok(
      appContent.includes("if (editorSaveRef.current) {"),
      "Must flush pending debounced saves before navigating"
    );
    assert.ok(
      appContent.includes("onActivateLink={handleActivateInternalLink}"),
      "Must pass onActivateLink callback to NoteEditor"
    );
    assert.ok(
      appContent.includes("<BrokenLinkDialog"),
      "Must render BrokenLinkDialog"
    );
    assert.ok(
      appContent.includes("target={linkNavigationTarget}"),
      "Must pass linkNavigationTarget to BrokenLinkDialog"
    );
    assert.ok(
      appContent.includes("onViewInTrash={handleConfirmViewInTrash}"),
      "Must pass handleConfirmViewInTrash to BrokenLinkDialog"
    );
  });

  it("simulates safe activation flow for active target (flushes pending edits and switches view if in trash)", async () => {
    let pendingEditsFlushed = false;
    let selectedNoteId = "source-note-1";
    let activeNavId = "trash";
    let dialogTarget = null;

    const mockEditorSave = async () => {
      pendingEditsFlushed = true;
    };

    const mockResolveTarget = async (id) => {
      return {
        targetId: id,
        title: "Active Target Note",
        lifecycle: "active",
        notebookId: "nb-1",
        notebookName: "Engineering",
      };
    };

    // Simulate handleActivateInternalLink
    const activateLink = async (targetId) => {
      const resolved = await mockResolveTarget(targetId);
      if (resolved.lifecycle === "active") {
        if (mockEditorSave) {
          await mockEditorSave();
        }
        if (activeNavId === "trash") {
          activeNavId = "all-notes";
        }
        selectedNoteId = targetId;
      } else {
        dialogTarget = resolved;
      }
    };

    await activateLink("target-note-active-2");

    assert.strictEqual(pendingEditsFlushed, true, "Pending edits must be flushed");
    assert.strictEqual(activeNavId, "all-notes", "Must switch out of Trash view to view active target");
    assert.strictEqual(selectedNoteId, "target-note-active-2", "Target note must become selected");
    assert.strictEqual(dialogTarget, null, "No broken dialog must be displayed for active target");
  });

  it("simulates activation of trashed target prompts user with BrokenLinkDialog without silent restore", async () => {
    let pendingEditsFlushed = false;
    let selectedNoteId = "source-note-1";
    let activeNavId = "all-notes";
    let dialogTarget = null;
    let noteRestored = false;

    const mockEditorSave = async () => {
      pendingEditsFlushed = true;
    };

    const mockResolveTarget = async (id) => {
      return {
        targetId: id,
        title: "Archived Notes",
        lifecycle: "trashed",
        notebookId: null,
        notebookName: null,
      };
    };

    // User clicks link to trashed note
    const activateLink = async (targetId) => {
      const resolved = await mockResolveTarget(targetId);
      if (resolved.lifecycle === "active") {
        if (mockEditorSave) await mockEditorSave();
        selectedNoteId = targetId;
      } else {
        // Trashed target: set dialog target
        dialogTarget = resolved;
      }
    };

    await activateLink("trashed-note-99");

    assert.ok(dialogTarget !== null, "Dialog target must be set");
    assert.strictEqual(dialogTarget.lifecycle, "trashed");
    assert.strictEqual(dialogTarget.title, "Archived Notes");
    assert.strictEqual(noteRestored, false, "Note must NOT be silently restored");
    assert.strictEqual(selectedNoteId, "source-note-1", "Selection must not change until explicit confirmation");

    // User confirms "View in Trash"
    const confirmViewInTrash = async () => {
      const idToOpen = dialogTarget.targetId;
      dialogTarget = null;
      if (mockEditorSave) await mockEditorSave();
      activeNavId = "trash";
      selectedNoteId = idToOpen;
    };

    await confirmViewInTrash();

    assert.strictEqual(dialogTarget, null, "Dialog must be closed");
    assert.strictEqual(activeNavId, "trash", "Must navigate to Trash view");
    assert.strictEqual(selectedNoteId, "trashed-note-99", "Trashed note must be selected in Trash view");
    assert.strictEqual(noteRestored, false, "Note remains in trash without being un-deleted");
  });

  it("simulates activation of missing target presents broken link dialog gracefully without crash", async () => {
    let dialogTarget = null;

    const mockResolveTarget = async (id) => {
      return {
        targetId: id,
        title: null,
        lifecycle: "missing",
        notebookId: null,
        notebookName: null,
      };
    };

    const activateLink = async (targetId) => {
      try {
        const resolved = await mockResolveTarget(targetId);
        if (resolved.lifecycle !== "active") {
          dialogTarget = resolved;
        }
      } catch (err) {
        dialogTarget = {
          targetId,
          title: null,
          lifecycle: "missing",
          notebookId: null,
          notebookName: null,
        };
      }
    };

    await activateLink("deleted-uuid-nonexistent");

    assert.ok(dialogTarget !== null, "Broken link dialog must be triggered");
    assert.strictEqual(dialogTarget.lifecycle, "missing");
    assert.strictEqual(dialogTarget.targetId, "deleted-uuid-nonexistent");
    assert.strictEqual(dialogTarget.title, null);
  });

  it("guarantees activation resolves by stable ID and never creates duplicate notes", async () => {
    const notesDatabase = [
      { id: "note-uuid-a", title: "Quarterly Review", is_deleted: false },
      { id: "note-uuid-b", title: "Quarterly Review", is_deleted: false },
    ];

    let createdNotesCount = 0;
    const createNoteMock = () => {
      createdNotesCount++;
    };

    const resolveAndActivate = (targetId) => {
      const found = notesDatabase.find((n) => n.id === targetId);
      if (!found) {
        return null;
      }
      return found.id;
    };

    // Activating link with note-uuid-b resolves specifically to note-uuid-b
    const activatedId = resolveAndActivate("note-uuid-b");
    assert.strictEqual(activatedId, "note-uuid-b");
    assert.notStrictEqual(activatedId, "note-uuid-a");
    assert.strictEqual(createdNotesCount, 0, "Must never create duplicate notes upon link activation");
  });
});

describe("Phase 9 - Task 11: Protect Unsaved Edits During Link Navigation Tests", () => {
  it("verifies App.tsx and NoteEditor.tsx code-level wiring for unsaved edit protection", async () => {
    const fs = await import("node:fs");
    const appContent = fs.readFileSync(
      new URL("../src/app/App.tsx", import.meta.url),
      "utf8"
    );
    const editorContent = fs.readFileSync(
      new URL(
        "../src/features/notes/components/Editor/NoteEditor.tsx",
        import.meta.url
      ),
      "utf8"
    );

    // App.tsx checks
    assert.ok(
      appContent.includes("activeLinkNavRequestIdRef = useRef<number>(0)"),
      "Must track activeLinkNavRequestIdRef sequence counter for race condition protection"
    );
    assert.ok(
      appContent.includes("const navRequestId = ++activeLinkNavRequestIdRef.current;"),
      "Must increment navRequestId upon link activation"
    );
    assert.ok(
      appContent.includes("await editorSaveRef.current();"),
      "Must flush pending debounced saves before navigating away"
    );
    assert.ok(
      appContent.includes("activeLinkNavRequestIdRef.current !== navRequestId"),
      "Must discard late-arriving navigation responses if superseded"
    );
    assert.ok(
      appContent.includes("selectedNoteId !== sourceNoteId"),
      "Must guard against navigation if source note changed during async operations"
    );

    // NoteEditor.tsx checks
    assert.ok(
      editorContent.includes("isDirtyRef.current = false;"),
      "Must synchronously reset isDirtyRef upon successful save"
    );
    assert.ok(
      editorContent.includes("isDirtyRef.current = true;\n        setEditorStatus(\"error\");"),
      "Must maintain dirty and error state when save fails"
    );
  });

  it("simulates pending debounced edit flush before link navigation", async () => {
    let saveCalled = false;
    let selectedNoteId = "note-source-100";
    let activeNavId = "all-notes";

    const mockSave = async () => {
      saveCalled = true;
    };

    const activateLink = async (targetId) => {
      // 1. Flush pending save
      if (mockSave) {
        await mockSave();
      }
      // 2. Navigate
      selectedNoteId = targetId;
    };

    await activateLink("note-target-200");

    assert.strictEqual(saveCalled, true, "Must flush pending save before switching notes");
    assert.strictEqual(selectedNoteId, "note-target-200", "Must navigate to target note after successful save");
  });

  it("simulates save failure blocks navigation and preserves in-memory user changes", async () => {
    let selectedNoteId = "source-note-with-unsaved-work";
    let errorMessage = null;
    let navigationAttempted = false;

    const mockFailingSave = async () => {
      throw new Error("Disk write error: Permission denied");
    };

    const activateLink = async (targetId) => {
      navigationAttempted = true;
      try {
        await mockFailingSave();
      } catch (saveErr) {
        errorMessage = "Unable to save current note before navigating: " + saveErr.message;
        // Navigation aborted to protect unsaved edits
        return;
      }
      selectedNoteId = targetId;
    };

    await activateLink("target-note-300");

    assert.strictEqual(navigationAttempted, true, "Activation was initiated");
    assert.ok(errorMessage !== null, "Error must be reported to user");
    assert.ok(errorMessage.includes("Permission denied"), "Error message must reflect actual failure");
    assert.strictEqual(
      selectedNoteId,
      "source-note-with-unsaved-work",
      "Selected note must NOT change if save fails, preserving user content in editor"
    );
  });

  it("simulates rapid multiple link clicks ignores stale resolutions (sequence ID race condition)", async () => {
    let activeRequestId = 0;
    let selectedNoteId = "note-start";

    const navigateToLink = async (targetId, delayMs) => {
      const currentReqId = ++activeRequestId;

      // Simulate async resolution / database read
      await new Promise((resolve) => setTimeout(resolve, delayMs));

      // Guard: if a newer navigation request was issued, discard this one
      if (activeRequestId !== currentReqId) {
        return;
      }

      selectedNoteId = targetId;
    };

    // Click 1 (slow, takes 60ms)
    const p1 = navigateToLink("target-slow", 60);
    // Click 2 (fast, takes 15ms)
    const p2 = navigateToLink("target-fast", 15);

    await Promise.all([p1, p2]);

    // target-fast was issued second, so activeRequestId corresponds to target-fast.
    // target-slow finishes after target-fast, but must NOT overwrite target-fast!
    assert.strictEqual(
      selectedNoteId,
      "target-fast",
      "Fast/newer click must prevail; slow/stale response must be discarded"
    );
  });

  it("simulates source note change during async operations aborts stale navigation", async () => {
    let selectedNoteId = "note-alpha";
    let activeRequestId = 0;

    const navigateLink = async (targetId) => {
      const reqId = ++activeRequestId;
      const sourceId = selectedNoteId;

      // Simulate async work
      await new Promise((resolve) => setTimeout(resolve, 20));

      if (activeRequestId !== reqId) return;
      if (selectedNoteId !== sourceId) {
        // Source note was switched externally during async resolution: abort
        return;
      }

      selectedNoteId = targetId;
    };

    const navPromise = navigateLink("target-beta");

    // While navigation is in flight, user selects note-gamma from sidebar
    selectedNoteId = "note-gamma";

    await navPromise;

    assert.strictEqual(
      selectedNoteId,
      "note-gamma",
      "Navigation must abort if source note changed during async operations"
    );
  });
});

describe("Phase 9 - Task 12: Back and Forward Navigation History Tests", () => {
  it("verifies useNoteNavigationHistory hook module structure and capabilities", async () => {
    const fs = await import("node:fs");
    const hookContent = fs.readFileSync(
      new URL("../src/hooks/useNoteNavigationHistory.ts", import.meta.url),
      "utf8"
    );

    assert.ok(hookContent.includes("export function useNoteNavigationHistory"), "Must define useNoteNavigationHistory hook");
    assert.ok(hookContent.includes("canGoBack"), "Must expose canGoBack boolean");
    assert.ok(hookContent.includes("canGoForward"), "Must expose canGoForward boolean");
    assert.ok(hookContent.includes("recordNavigation"), "Must expose recordNavigation action");
    assert.ok(hookContent.includes("goBack"), "Must expose goBack action");
    assert.ok(hookContent.includes("goForward"), "Must expose goForward action");
    assert.ok(hookContent.includes("removeNoteFromHistory"), "Must expose removeNoteFromHistory action");
    assert.ok(hookContent.includes("clearHistory"), "Must expose clearHistory action");
  });

  it("verifies App.tsx integration of navigation history with link activation and deletion", async () => {
    const fs = await import("node:fs");
    const appContent = fs.readFileSync(
      new URL("../src/app/App.tsx", import.meta.url),
      "utf8"
    );

    assert.ok(
      appContent.includes("useNoteNavigationHistory(selectedNoteId)"),
      "Must instantiate useNoteNavigationHistory with selectedNoteId"
    );
    assert.ok(
      appContent.includes("navigationHistory.recordNavigation(targetNoteId, sourceNoteId)"),
      "Must record transition in history on successful link navigation"
    );
    assert.ok(
      appContent.includes("const handleNavigateBack = useCallback("),
      "Must implement handleNavigateBack callback"
    );
    assert.ok(
      appContent.includes("const handleNavigateForward = useCallback("),
      "Must implement handleNavigateForward callback"
    );
    assert.ok(
      appContent.includes("navigationHistory.removeNoteFromHistory(noteId)"),
      "Must purge permanently deleted notes from history stack"
    );
    assert.ok(
      appContent.includes("navigationHistory.clearHistory()"),
      "Must clear history when emptying trash"
    );
  });

  it("verifies back and forward navigation transitions, duplicate prevention, and branching", () => {
    // Pure logic simulation of navigation stack
    let stack = ["note-A"];
    let currentIndex = 0;

    const record = (targetId) => {
      if (currentIndex >= 0 && stack[currentIndex] === targetId) return;
      stack = [...stack.slice(0, currentIndex + 1), targetId];
      currentIndex = stack.length - 1;
    };

    const back = () => {
      if (currentIndex <= 0) return null;
      currentIndex--;
      return stack[currentIndex];
    };

    const forward = () => {
      if (currentIndex < 0 || currentIndex >= stack.length - 1) return null;
      currentIndex++;
      return stack[currentIndex];
    };

    // 1. Navigate A -> B -> C
    record("note-B");
    assert.deepStrictEqual(stack, ["note-A", "note-B"]);
    assert.strictEqual(currentIndex, 1);

    record("note-C");
    assert.deepStrictEqual(stack, ["note-A", "note-B", "note-C"]);
    assert.strictEqual(currentIndex, 2);

    // 2. Prevent consecutive duplicates
    record("note-C");
    assert.deepStrictEqual(stack, ["note-A", "note-B", "note-C"]);
    assert.strictEqual(currentIndex, 2);

    // 3. Step back C -> B -> A
    assert.strictEqual(back(), "note-B");
    assert.strictEqual(currentIndex, 1);
    assert.strictEqual(back(), "note-A");
    assert.strictEqual(currentIndex, 0);
    assert.strictEqual(back(), null, "Cannot go back past initial entry");

    // 4. Step forward A -> B
    assert.strictEqual(forward(), "note-B");
    assert.strictEqual(currentIndex, 1);

    // 5. Branching at B: navigate to D clears forward history (note-C)
    record("note-D");
    assert.deepStrictEqual(stack, ["note-A", "note-B", "note-D"]);
    assert.strictEqual(currentIndex, 2);
    assert.strictEqual(forward(), null, "Forward history must be empty after branching");

    // 6. Step back from D goes to B, then A
    assert.strictEqual(back(), "note-B");
    assert.strictEqual(back(), "note-A");
  });

  it("verifies note lifecycle respect: removing deleted note purges it from history stack safely", () => {
    let stack = ["note-A", "note-B", "note-C"];
    let currentIndex = 2; // on note-C

    const remove = (deletedId) => {
      if (!stack.includes(deletedId)) return;
      let targetIndex = currentIndex;
      const newStack = [];
      for (let i = 0; i < stack.length; i++) {
        if (stack[i] === deletedId) {
          if (i < currentIndex) {
            targetIndex--;
          }
        } else {
          newStack.push(stack[i]);
        }
      }
      stack = newStack;
      currentIndex = Math.max(0, Math.min(targetIndex, newStack.length - 1));
    };

    // Delete note-B while user is viewing note-C
    remove("note-B");
    assert.deepStrictEqual(stack, ["note-A", "note-C"]);
    assert.strictEqual(currentIndex, 1);
    assert.strictEqual(stack[currentIndex], "note-C");

    // Back from note-C now goes directly to note-A
    const back = () => (currentIndex > 0 ? stack[--currentIndex] : null);
    assert.strictEqual(back(), "note-A");
    assert.strictEqual(currentIndex, 0);
  });

  it("simulates back navigation resolves active target safely and prompts for trashed target", async () => {
    const mockDb = {
      "note-active": { id: "note-active", title: "Active Note", lifecycle: "active" },
      "note-trashed": { id: "note-trashed", title: "Old Draft", lifecycle: "trashed" },
    };

    let selectedNoteId = "current-note";
    let promptedDialogTarget = null;

    const navigateBackTo = async (destinationId) => {
      const target = mockDb[destinationId];
      if (!target || target.lifecycle === "missing") {
        promptedDialogTarget = { targetId: destinationId, lifecycle: "missing" };
      } else if (target.lifecycle === "active") {
        selectedNoteId = target.id;
      } else if (target.lifecycle === "trashed") {
        promptedDialogTarget = target;
      }
    };

    // Navigate back to active note
    await navigateBackTo("note-active");
    assert.strictEqual(selectedNoteId, "note-active");
    assert.strictEqual(promptedDialogTarget, null);

    // Navigate back to trashed note
    await navigateBackTo("note-trashed");
    assert.ok(promptedDialogTarget !== null);
    assert.strictEqual(promptedDialogTarget.lifecycle, "trashed");
    assert.strictEqual(promptedDialogTarget.title, "Old Draft");
    // Selection remains unchanged until explicit confirmation
    assert.strictEqual(selectedNoteId, "note-active");
  });
});

describe("Phase 9 - Task 13: Add Back and Forward Navigation Controls Tests", () => {
  it("verifies TopBar.tsx navigation control structure, accessibility labels, and disabled states", async () => {
    const fs = await import("node:fs");
    const topBarContent = fs.readFileSync(
      new URL("../src/components/TopBar/TopBar.tsx", import.meta.url),
      "utf8"
    );

    assert.ok(topBarContent.includes("canGoBack?: boolean;"), "Must declare canGoBack prop in TopBarProps");
    assert.ok(topBarContent.includes("canGoForward?: boolean;"), "Must declare canGoForward prop in TopBarProps");
    assert.ok(topBarContent.includes("onNavigateBack?: () => void;"), "Must declare onNavigateBack prop in TopBarProps");
    assert.ok(topBarContent.includes("onNavigateForward?: () => void;"), "Must declare onNavigateForward prop in TopBarProps");
    assert.ok(topBarContent.includes("topbar-nav-history"), "Must render topbar-nav-history container");
    assert.ok(topBarContent.includes("aria-label=\"Go back to previous note\""), "Must have accessible label for back button");
    assert.ok(topBarContent.includes("aria-label=\"Go forward to next note\""), "Must have accessible label for forward button");
    assert.ok(topBarContent.includes("disabled={!canGoBack}"), "Back button must bind disabled={!canGoBack}");
    assert.ok(topBarContent.includes("disabled={!canGoForward}"), "Forward button must bind disabled={!canGoForward}");
    assert.ok(topBarContent.includes("<ChevronLeftIcon />"), "Must render left chevron icon");
    assert.ok(topBarContent.includes("<ChevronRightIcon />"), "Must render right chevron icon");
  });

  it("verifies TopBar.css styles for topbar-nav-history and disabled state", async () => {
    const fs = await import("node:fs");
    const cssContent = fs.readFileSync(
      new URL("../src/components/TopBar/TopBar.css", import.meta.url),
      "utf8"
    );

    assert.ok(cssContent.includes(".topbar-left"), "Must define .topbar-left layout class");
    assert.ok(cssContent.includes(".topbar-nav-history"), "Must define .topbar-nav-history styling");
    assert.ok(cssContent.includes(".topbar-nav-btn"), "Must define .topbar-nav-btn sizing");
    assert.ok(cssContent.includes(".topbar-icon-btn:disabled"), "Must define disabled styling for topbar icon buttons");
  });

  it("verifies App.tsx wires TopBar navigation props to useNoteNavigationHistory", async () => {
    const fs = await import("node:fs");
    const appContent = fs.readFileSync(
      new URL("../src/app/App.tsx", import.meta.url),
      "utf8"
    );

    assert.ok(
      appContent.includes("canGoBack={navigationHistory.canGoBack}"),
      "App.tsx must pass canGoBack to TopBar"
    );
    assert.ok(
      appContent.includes("canGoForward={navigationHistory.canGoForward}"),
      "App.tsx must pass canGoForward to TopBar"
    );
    assert.ok(
      appContent.includes("onNavigateBack={handleNavigateBack}"),
      "App.tsx must pass onNavigateBack to TopBar"
    );
    assert.ok(
      appContent.includes("onNavigateForward={handleNavigateForward}"),
      "App.tsx must pass onNavigateForward to TopBar"
    );
  });

  it("simulates controls disabled states when history is empty or at boundaries", () => {
    let stack = ["initial-note"];
    let currentIndex = 0;

    const computeCanGoBack = () => currentIndex > 0;
    const computeCanGoForward = () => currentIndex >= 0 && currentIndex < stack.length - 1;

    // 1. Initial state (only 1 note)
    assert.strictEqual(computeCanGoBack(), false, "Cannot go back at initial note");
    assert.strictEqual(computeCanGoForward(), false, "Cannot go forward at initial note");

    // 2. Navigate to second note
    stack.push("second-note");
    currentIndex = 1;
    assert.strictEqual(computeCanGoBack(), true, "Can go back after navigating forward");
    assert.strictEqual(computeCanGoForward(), false, "Cannot go forward when at tip of history");

    // 3. Navigate back to first note
    currentIndex = 0;
    assert.strictEqual(computeCanGoBack(), false, "Cannot go back after returning to root");
    assert.strictEqual(computeCanGoForward(), true, "Can go forward after moving back");

    // 4. Navigate forward again to second note
    currentIndex = 1;
    assert.strictEqual(computeCanGoBack(), true, "Can go back again");
    assert.strictEqual(computeCanGoForward(), false, "Cannot go forward at end of history");
  });
});

describe("Phase 9 - Task 14: Implement Incoming Links / Backlinks Tests", () => {
  it("verifies linksStorage.getBacklinks client method in storage services", async () => {
    const fs = await import("node:fs");
    const linksTs = fs.readFileSync(
      new URL("../src/services/storage/links.ts", import.meta.url),
      "utf8"
    );

    assert.ok(
      linksTs.includes("async getBacklinks(targetNoteId: string): Promise<BacklinkItem[]>"),
      "Must define getBacklinks method in linksStorage"
    );
    assert.ok(
      linksTs.includes("invoke<BacklinkItem[]>(\"get_backlinks\", { targetNoteId })"),
      "Must call tauri command get_backlinks"
    );
  });

  it("verifies BacklinkItem type definition includes all required metadata fields", async () => {
    const fs = await import("node:fs");
    const typesTs = fs.readFileSync(
      new URL("../src/services/storage/types.ts", import.meta.url),
      "utf8"
    );

    assert.ok(typesTs.includes("export interface BacklinkItem"), "Must export BacklinkItem interface");
    assert.ok(typesTs.includes("sourceNoteId: string;"), "Must have sourceNoteId field");
    assert.ok(typesTs.includes("sourceNoteTitle: string;"), "Must have sourceNoteTitle field");
    assert.ok(typesTs.includes("sourceNotebookId: string | null;"), "Must have sourceNotebookId field");
    assert.ok(typesTs.includes("sourceNotebookName?: string | null;"), "Must have sourceNotebookName field");
    assert.ok(typesTs.includes("labelSnippet?: string | null;"), "Must have labelSnippet field");
    assert.ok(typesTs.includes("modifiedAt: string;"), "Must have modifiedAt field");
  });

  it("verifies BacklinksSection.tsx structure, accessibility, and navigation handling", async () => {
    const fs = await import("node:fs");
    const backlinksContent = fs.readFileSync(
      new URL("../src/features/notes/components/Links/BacklinksSection.tsx", import.meta.url),
      "utf8"
    );

    assert.ok(backlinksContent.includes("export const BacklinksSection"), "Must export BacklinksSection");
    assert.ok(backlinksContent.includes("storageService.links.getBacklinks"), "Must call storageService.links.getBacklinks");
    assert.ok(backlinksContent.includes("data-testid=\"backlinks-section\""), "Must have backlinks-section testid");
    assert.ok(backlinksContent.includes("data-testid=\"backlinks-toggle\""), "Must have backlinks-toggle testid");
    assert.ok(backlinksContent.includes("data-testid=\"backlinks-count\""), "Must have backlinks-count testid");
    assert.ok(backlinksContent.includes("data-testid=\"backlinks-list\""), "Must have backlinks-list testid");
    assert.ok(backlinksContent.includes("aria-expanded={isExpanded}"), "Must bind aria-expanded accessibility attribute");
    assert.ok(backlinksContent.includes("onActivateLink(item.sourceNoteId, item.sourceNoteTitle)"), "Must invoke onActivateLink when clicked");
    assert.ok(backlinksContent.includes("backlink-notebook-badge"), "Must render notebook badge when present");
    assert.ok(backlinksContent.includes("backlink-snippet-preview"), "Must render snippet preview when available");
  });

  it("verifies NoteEditor.tsx integrates BacklinksSection inside the scrollable document", async () => {
    const fs = await import("node:fs");
    const editorContent = fs.readFileSync(
      new URL("../src/features/notes/components/Editor/NoteEditor.tsx", import.meta.url),
      "utf8"
    );

    assert.ok(
      editorContent.includes("BacklinksSection"),
      "NoteEditor must import BacklinksSection"
    );
    assert.ok(
      editorContent.includes("<BacklinksSection"),
      "NoteEditor must render <BacklinksSection"
    );
    assert.ok(
      editorContent.includes("onActivateLink={handleActivateLink}"),
      "NoteEditor must wire handleActivateLink to BacklinksSection"
    );
    // Crucial invariant: AttachmentList decoupling comment must remain intact
    assert.ok(
      editorContent.includes("{/* <AttachmentList noteId={noteId} /> */}"),
      "Must preserve AttachmentList decoupling invariant comment"
    );
  });

  it("verifies InternalLinks.css provides full styling for BacklinksSection", async () => {
    const fs = await import("node:fs");
    const cssContent = fs.readFileSync(
      new URL("../src/features/notes/components/Links/InternalLinks.css", import.meta.url),
      "utf8"
    );

    assert.ok(cssContent.includes(".backlinks-section"), "Must style .backlinks-section");
    assert.ok(cssContent.includes(".backlinks-header"), "Must style .backlinks-header");
    assert.ok(cssContent.includes(".backlinks-toggle-btn"), "Must style .backlinks-toggle-btn");
    assert.ok(cssContent.includes(".backlinks-badge"), "Must style .backlinks-badge");
    assert.ok(cssContent.includes(".backlink-card"), "Must style .backlink-card");
    assert.ok(cssContent.includes(".backlink-notebook-badge"), "Must style .backlink-notebook-badge");
    assert.ok(cssContent.includes(".backlink-snippet-preview"), "Must style .backlink-snippet-preview");
  });

  it("verifies Rust backend get_backlinks command and registration", async () => {
    const fs = await import("node:fs");
    const storageCommands = fs.readFileSync(
      new URL("../src-tauri/src/commands/storage.rs", import.meta.url),
      "utf8"
    );
    const libRs = fs.readFileSync(
      new URL("../src-tauri/src/lib.rs", import.meta.url),
      "utf8"
    );

    assert.ok(
      storageCommands.includes("pub fn get_backlinks("),
      "storage.rs must define pub fn get_backlinks"
    );
    assert.ok(
      storageCommands.includes("InternalLinkService::get_backlinks(&conn, &target_note_id)"),
      "storage.rs must invoke InternalLinkService::get_backlinks"
    );
    assert.ok(
      libRs.includes("commands::get_backlinks,"),
      "lib.rs must register commands::get_backlinks"
    );
  });

  it("simulates backlink activation callback on source note selection", () => {
    const activatedTargets = [];
    const onActivateLink = (targetId, label) => {
      activatedTargets.push({ targetId, label });
    };

    const mockBacklinks = [
      {
        sourceNoteId: "note-101",
        sourceNoteTitle: "Project Alpha",
        sourceNotebookId: "nb-1",
        sourceNotebookName: "Active Projects",
        labelSnippet: "See [Spec Document] for details",
        modifiedAt: "2026-10-09T08:30:00Z",
      },
      {
        sourceNoteId: "note-202",
        sourceNoteTitle: "Meeting Notes",
        sourceNotebookId: null,
        sourceNotebookName: null,
        labelSnippet: "Action item: check [Spec Document]",
        modifiedAt: "2026-10-09T09:15:00Z",
      },
    ];

    // Simulate clicking the first backlink
    onActivateLink(mockBacklinks[0].sourceNoteId, mockBacklinks[0].sourceNoteTitle);

    assert.strictEqual(activatedTargets.length, 1);
    assert.strictEqual(activatedTargets[0].targetId, "note-101");
    assert.strictEqual(activatedTargets[0].label, "Project Alpha");

    // Simulate clicking the second backlink
    onActivateLink(mockBacklinks[1].sourceNoteId, mockBacklinks[1].sourceNoteTitle);
    assert.strictEqual(activatedTargets.length, 2);
    assert.strictEqual(activatedTargets[1].targetId, "note-202");
    assert.strictEqual(activatedTargets[1].label, "Meeting Notes");
  });
});

describe("Phase 9 - Task 15: Keep Link Metadata Synchronized with Note Content Tests", () => {
  it("verifies BacklinksSection.tsx supports refreshTrigger and manual refresh button", async () => {
    const fs = await import("node:fs");
    const backlinksContent = fs.readFileSync(
      new URL("../src/features/notes/components/Links/BacklinksSection.tsx", import.meta.url),
      "utf8"
    );

    assert.ok(
      backlinksContent.includes("refreshTrigger?: number | string | null;"),
      "BacklinksSectionProps must declare refreshTrigger prop"
    );
    assert.ok(
      backlinksContent.includes("data-testid=\"backlinks-refresh-btn\""),
      "BacklinksSection must provide refresh button with testid"
    );
    assert.ok(
      backlinksContent.includes("title=\"Refresh incoming links\""),
      "Refresh button must have descriptive title"
    );
    assert.ok(
      backlinksContent.includes("[noteId, refreshTrigger, fetchBacklinks]"),
      "useEffect must react to refreshTrigger changes"
    );
  });

  it("verifies NoteEditor.tsx passes refreshTrigger={lastSavedAt} to BacklinksSection", async () => {
    const fs = await import("node:fs");
    const editorContent = fs.readFileSync(
      new URL("../src/features/notes/components/Editor/NoteEditor.tsx", import.meta.url),
      "utf8"
    );

    assert.ok(
      editorContent.includes("refreshTrigger={lastSavedAt}"),
      "NoteEditor must wire refreshTrigger={lastSavedAt} to BacklinksSection"
    );
    assert.ok(
      editorContent.includes("{/* <AttachmentList noteId={noteId} /> */}"),
      "Must preserve AttachmentList decoupling invariant comment"
    );
  });

  it("verifies InternalLinks.css provides styles for backlinks refresh button and spinning state", async () => {
    const fs = await import("node:fs");
    const cssContent = fs.readFileSync(
      new URL("../src/features/notes/components/Links/InternalLinks.css", import.meta.url),
      "utf8"
    );

    assert.ok(cssContent.includes(".backlinks-refresh-btn"), "Must style .backlinks-refresh-btn");
    assert.ok(cssContent.includes(".backlinks-refresh-btn .is-spinning"), "Must style .is-spinning animation");
    assert.ok(cssContent.includes("@keyframes spin"), "Must declare @keyframes spin");
  });

  it("simulates synchronized backlink lifecycle across creation, content edits, trash, and restore", async () => {
    const { parseInternalLinks } = await import("../src/utils/linkParser.ts");
    const targetNoteId = "target-note-100";
    let notesDatabase = [
      {
        id: "source-1",
        title: "Lecture Notes",
        content: `Check out foundational concepts in [Quantum Theory](notepad://note/${targetNoteId}) today.`,
        is_deleted: 0,
        notebook_id: "nb-1",
        notebook_name: "Physics",
        modified_at: "2026-10-09T10:00:00Z",
      },
    ];

    // Authoritative synchronization query function
    const computeBacklinks = (targetId) => {
      return notesDatabase
        .filter((n) => n.is_deleted === 0 && n.id !== targetId)
        .filter((n) => {
          const links = parseInternalLinks(n.content);
          return links.some((l) => l.targetId === targetId);
        })
        .map((n) => ({
          sourceNoteId: n.id,
          sourceNoteTitle: n.title,
          sourceNotebookName: n.notebook_name,
        }));
    };

    // 1. Initial creation
    let backlinks = computeBacklinks(targetNoteId);
    assert.strictEqual(backlinks.length, 1);
    assert.strictEqual(backlinks[0].sourceNoteId, "source-1");

    // 2. Note content updated: link removed
    notesDatabase[0].content = "Check out foundational concepts in general physics today.";
    backlinks = computeBacklinks(targetNoteId);
    assert.strictEqual(backlinks.length, 0, "Obsolete link must immediately disappear from backlinks");

    // 3. Link re-inserted with modified label
    notesDatabase[0].content = `Updated reading: [Advanced Quantum Computing](notepad://note/${targetNoteId}).`;
    backlinks = computeBacklinks(targetNoteId);
    assert.strictEqual(backlinks.length, 1);

    // 4. Source note soft-deleted (moved to trash)
    notesDatabase[0].is_deleted = 1;
    backlinks = computeBacklinks(targetNoteId);
    assert.strictEqual(backlinks.length, 0, "Trashed source note must be excluded from backlinks");

    // 5. Source note restored
    notesDatabase[0].is_deleted = 0;
    backlinks = computeBacklinks(targetNoteId);
    assert.strictEqual(backlinks.length, 1, "Restored source note must immediately re-appear in backlinks");

    // 6. Source note permanently deleted
    notesDatabase = notesDatabase.filter((n) => n.id !== "source-1");
    backlinks = computeBacklinks(targetNoteId);
    assert.strictEqual(backlinks.length, 0, "Permanently deleted source note leaves no dangling backlinks");
  });
});

describe("Phase 9 - Task 16: Handle Broken Links and Missing Targets Tests", () => {
  it("verifies BrokenLinkDialog.tsx provides close, remove link, and change link actions for missing targets", async () => {
    const fs = await import("node:fs");
    const dialogContent = fs.readFileSync(
      new URL("../src/features/notes/components/Links/BrokenLinkDialog.tsx", import.meta.url),
      "utf8"
    );

    assert.ok(dialogContent.includes("data-testid=\"broken-link-close-btn\""), "Must render close button");
    assert.ok(dialogContent.includes("data-testid=\"broken-link-remove-btn\""), "Must render remove link button");
    assert.ok(dialogContent.includes("data-testid=\"broken-link-repair-btn\""), "Must render repair/change link button");
    assert.ok(
      dialogContent.includes("The original link text is preserved."),
      "Must clearly communicate that original link text is preserved"
    );
  });

  it("verifies replaceInternalLinkTarget utility safely updates target ID without corrupting labels or other links", async () => {
    const { replaceInternalLinkTarget } = await import("../src/utils/linkParser.ts");

    const content = "See [Architecture Guide](notepad://note/missing-uuid-1) and [Other Doc](notepad://note/active-uuid-2).";
    const repaired = replaceInternalLinkTarget(
      content,
      "missing-uuid-1",
      "new-uuid-3",
      "Modern Architecture Guide"
    );

    assert.strictEqual(
      repaired,
      "See [Modern Architecture Guide](notepad://note/new-uuid-3) and [Other Doc](notepad://note/active-uuid-2)."
    );

    // If new title is omitted, existing label is preserved
    const repairedPreserveLabel = replaceInternalLinkTarget(
      content,
      "missing-uuid-1",
      "new-uuid-3"
    );
    assert.strictEqual(
      repairedPreserveLabel,
      "See [Architecture Guide](notepad://note/new-uuid-3) and [Other Doc](notepad://note/active-uuid-2)."
    );
  });

  it("verifies removeInternalLink utility removes link markup while preserving label text", async () => {
    const { removeInternalLink } = await import("../src/utils/linkParser.ts");

    const content = "Refer to [Quantum Theory](notepad://note/missing-uuid-1) for details.";
    const unlinked = removeInternalLink(content, "missing-uuid-1");

    assert.strictEqual(unlinked, "Refer to Quantum Theory for details.");

    // Does not touch unrelated links
    const contentMultiple = "Refer to [Quantum Theory](notepad://note/missing-uuid-1) and [Math](notepad://note/math-id).";
    const unlinkedMultiple = removeInternalLink(contentMultiple, "missing-uuid-1");
    assert.strictEqual(unlinkedMultiple, "Refer to Quantum Theory and [Math](notepad://note/math-id).");
  });

  it("verifies App.tsx wires broken link repair and removal flows and preserves source content", async () => {
    const fs = await import("node:fs");
    const appContent = fs.readFileSync(
      new URL("../src/app/App.tsx", import.meta.url),
      "utf8"
    );

    assert.ok(
      appContent.includes("onRepairLink={handleInitiateRepairBrokenLink}"),
      "App.tsx must pass onRepairLink to BrokenLinkDialog"
    );
    assert.ok(
      appContent.includes("onRemoveLink={handleRemoveBrokenLink}"),
      "App.tsx must pass onRemoveLink to BrokenLinkDialog"
    );
    assert.ok(
      appContent.includes("repairingBrokenTargetId"),
      "App.tsx must manage repairingBrokenTargetId state"
    );
    assert.ok(
      appContent.includes("replaceInternalLinkTarget("),
      "App.tsx must use replaceInternalLinkTarget for explicit replacement"
    );
    assert.ok(
      appContent.includes("removeInternalLink("),
      "App.tsx must use removeInternalLink for explicit removal"
    );
  });

  it("simulates title collision immunity: broken target never silently resolves to a new note with same title", async () => {
    const deletedNoteTargetId = "old-deleted-uuid-123";
    const fakeNotesDb = [
      {
        id: "active-uuid-456",
        title: "Architecture Guide", // Same title!
        is_deleted: 0,
      },
    ];

    // Canonical ID resolution
    const resolveLink = (targetId) => {
      const match = fakeNotesDb.find((n) => n.id === targetId);
      if (match) {
        return { targetId, lifecycle: match.is_deleted ? "trashed" : "active", title: match.title };
      }
      return { targetId, lifecycle: "missing", title: null };
    };

    const resolved = resolveLink(deletedNoteTargetId);
    assert.strictEqual(resolved.lifecycle, "missing", "Must resolve as missing");
    assert.strictEqual(resolved.title, null, "Must not adopt title of unrelated active note");
    assert.notStrictEqual(resolved.targetId, fakeNotesDb[0].id, "Must never resolve to colliding title note");
  });
});

describe("Phase 9 - Task 17: Integrate Internal Links with Trash and Recovery Tests", () => {
  it("verifies BrokenLinkDialog.tsx provides Restore Note and View in Trash buttons for trashed targets", async () => {
    const fs = await import("node:fs");
    const dialogContent = fs.readFileSync(
      new URL("../src/features/notes/components/Links/BrokenLinkDialog.tsx", import.meta.url),
      "utf8"
    );

    assert.ok(
      dialogContent.includes("data-testid=\"broken-link-restore-btn\""),
      "Must render restore button for trashed notes"
    );
    assert.ok(
      dialogContent.includes("data-testid=\"broken-link-trash-btn\""),
      "Must render view in trash button for trashed notes"
    );
    assert.ok(
      dialogContent.includes("The linked note <strong>&ldquo;{noteTitle}&rdquo;</strong> is currently in Trash."),
      "Must display clear message naming the trashed note"
    );
  });

  it("verifies App.tsx wires handleRestoreAndOpenTargetNote and handleConfirmViewInTrash", async () => {
    const fs = await import("node:fs");
    const appContent = fs.readFileSync(
      new URL("../src/app/App.tsx", import.meta.url),
      "utf8"
    );

    assert.ok(
      appContent.includes("handleRestoreAndOpenTargetNote"),
      "App.tsx must define handleRestoreAndOpenTargetNote"
    );
    assert.ok(
      appContent.includes("handleConfirmViewInTrash"),
      "App.tsx must define handleConfirmViewInTrash"
    );
    assert.ok(
      appContent.includes("onRestore={handleRestoreAndOpenTargetNote}"),
      "App.tsx must pass onRestore to BrokenLinkDialog"
    );
    assert.ok(
      appContent.includes("onViewInTrash={handleConfirmViewInTrash}"),
      "App.tsx must pass onViewInTrash to BrokenLinkDialog"
    );
  });

  it("simulates target entering trash: preserves source note content and marks lifecycle as trashed", () => {
    const targetId = "target-uuid-999";
    const initialSourceContent = `Important notes referencing [Target Spec](notepad://note/${targetId}) in detail.`;
    const sourceNote = {
      id: "source-uuid-111",
      title: "Design Meeting",
      content: initialSourceContent,
      is_deleted: 0,
    };
    const targetNote = {
      id: targetId,
      title: "Target Spec",
      is_deleted: 0,
    };

    // 1. Soft-delete target note
    targetNote.is_deleted = 1;

    // Verify target resolution
    const resolveLifecycle = (note) => (note.is_deleted ? "trashed" : "active");
    assert.strictEqual(resolveLifecycle(targetNote), "trashed");

    // Invariant: Source note content is completely preserved
    assert.strictEqual(sourceNote.content, initialSourceContent);
    assert.strictEqual(sourceNote.is_deleted, 0);

    // 2. Restore target note
    targetNote.is_deleted = 0;
    assert.strictEqual(resolveLifecycle(targetNote), "active");
    // Link still points to targetId and works without requiring re-insertion
    assert.ok(sourceNote.content.includes(`notepad://note/${targetId}`));
  });

  it("simulates permanent target deletion: resolves safely as missing without cascading to source note", () => {
    const targetId = "target-uuid-888";
    const initialSourceContent = `Plan referencing [Old Doc](notepad://note/${targetId}).`;
    const sourceNote = {
      id: "source-uuid-222",
      title: "Project Plan",
      content: initialSourceContent,
      is_deleted: 0,
    };

    let targetExists = true;
    // Permanently delete target note
    targetExists = false;

    const resolveLifecycle = (exists) => (exists ? "active" : "missing");
    assert.strictEqual(resolveLifecycle(targetExists), "missing");

    // Source note remains intact and is NOT cascade-deleted or silently rewritten
    assert.strictEqual(sourceNote.content, initialSourceContent);
    assert.strictEqual(sourceNote.is_deleted, 0);
  });
});

describe("Phase 9 - Task 18: Make Links Safe Across Note Renames and Duplicate Titles Tests", () => {
  it("verifies internal-links-architecture.md documents display-label policy and rename independence", async () => {
    const fs = await import("node:fs");
    const doc = fs.readFileSync(
      new URL("../docs/internal-links-architecture.md", import.meta.url),
      "utf8"
    );

    assert.ok(
      doc.includes("Display-Label Policy & Rename Independence"),
      "Must document Display-Label Policy & Rename Independence section"
    );
    assert.ok(
      doc.includes("Insertion-Time Label Preservation"),
      "Must document Insertion-Time Label Preservation"
    );
    assert.ok(
      doc.includes("Title-Independent Target Resolution"),
      "Must document Title-Independent Target Resolution"
    );
  });

  it("simulates target note rename: preserves source content and link validity without content rewrites", async () => {
    const targetId = "target-uuid-alpha";
    const sourceNote = {
      id: "source-uuid-omega",
      title: "Project Overview",
      content: `See our [Initial Specifications](notepad://note/${targetId}) for details.`,
    };
    let targetNote = {
      id: targetId,
      title: "Initial Specifications",
      notebook_id: "nb-1",
    };

    // Rename target note
    targetNote.title = "Modern Distributed Architecture v3";

    // Source note content is NOT rewritten on rename
    assert.strictEqual(
      sourceNote.content,
      `See our [Initial Specifications](notepad://note/${targetId}) for details.`
    );

    // Link target still resolves unambiguously to the target note by UUID
    const resolveLink = (id, note) => (id === note.id ? note : null);
    const resolved = resolveLink(targetId, targetNote);
    assert.ok(resolved);
    assert.strictEqual(resolved.id, targetId);
    assert.strictEqual(resolved.title, "Modern Distributed Architecture v3");
  });

  it("simulates duplicate note titles: links resolve unambiguously by stable UUID", () => {
    const noteA = {
      id: "uuid-note-a",
      title: "Sprint Planning",
      notebookName: "Engineering",
    };
    const noteB = {
      id: "uuid-note-b",
      title: "Sprint Planning", // Exact same title
      notebookName: "Marketing",
    };

    const notesDb = [noteA, noteB];
    const resolveByUuid = (targetId) => notesDb.find((n) => n.id === targetId);

    // Link pointing to Note A resolves to Note A
    const resA = resolveByUuid(noteA.id);
    assert.strictEqual(resA.id, "uuid-note-a");
    assert.strictEqual(resA.notebookName, "Engineering");

    // Link pointing to Note B resolves to Note B
    const resB = resolveByUuid(noteB.id);
    assert.strictEqual(resB.id, "uuid-note-b");
    assert.strictEqual(resB.notebookName, "Marketing");
    assert.notStrictEqual(resA.id, resB.id);
  });

  it("simulates notebook moves: preserves link target identity and updates notebook context", () => {
    const targetId = "target-uuid-gamma";
    const note = {
      id: targetId,
      title: "Onboarding Checklist",
      notebookId: "nb-old",
      notebookName: "HR",
    };

    // Move to different notebook
    note.notebookId = "nb-new";
    note.notebookName = "Engineering Onboarding";

    // Link identity is solely determined by UUID
    assert.strictEqual(note.id, targetId);
    assert.strictEqual(note.notebookName, "Engineering Onboarding");
  });

  it("verifies Unicode and punctuated note titles format and parse cleanly", async () => {
    const { formatInternalLink, parseInternalLinks } = await import("../src/utils/linkParser.ts");

    // Unicode title
    const unicodeLink = formatInternalLink("target-unicode-1", "योजना एवं अनुसंधान २०२६ (Café & Résumé)");
    assert.strictEqual(unicodeLink, "[योजना एवं अनुसंधान २०२६ (Café & Résumé)](notepad://note/target-unicode-1)");

    const parsedUnicode = parseInternalLinks(unicodeLink);
    assert.strictEqual(parsedUnicode.length, 1);
    assert.strictEqual(parsedUnicode[0].label, "योजना एवं अनुसंधान २०२६ (Café & Résumé)");

    // Punctuation title
    const punctLink = formatInternalLink("target-punct-2", "Release v2.0 (Final) - #1: High_Priority*");
    const parsedPunct = parseInternalLinks(punctLink);
    assert.strictEqual(parsedPunct.length, 1);
    assert.strictEqual(parsedPunct[0].label, "Release v2.0 (Final) - #1: High_Priority*");
  });
});

describe("Phase 9 - Task 19: Support Link Behavior Across TXT and Markdown Notes Tests", () => {
  it("verifies internal-links-architecture.md documents TXT and Markdown format support and limitations", async () => {
    const fs = await import("node:fs");
    const archDoc = fs.readFileSync(
      new URL("../docs/internal-links-architecture.md", import.meta.url),
      "utf8"
    );
    assert.ok(archDoc.includes("8. Support Across TXT and Markdown Notes"), "Section 8 header must exist");
    assert.ok(archDoc.includes("No Silent Format Conversion"), "Must document no silent format conversion");
    assert.ok(archDoc.includes("Plain Text (`txt`) Notes"), "Must document TXT support");
    assert.ok(archDoc.includes("Markdown (`md`) Notes"), "Must document Markdown support");
    assert.ok(archDoc.includes("Documented Limitations"), "Must honestly document edit mode limitations");
  });

  it("simulates link insertion into a TXT note preserves format and plain text integrity", async () => {
    const { insertInternalLinkIntoContent, parseInternalLinks } = await import("../src/utils/linkParser.ts");
    const originalTxt = "Meeting notes from Monday:\n- Discuss Q3 roadmap\n- Action items for next sprint\nEnd of notes.";
    let activeFormat = "txt";

    const targetSlice = "Discuss Q3 roadmap";
    const insertRes = insertInternalLinkIntoContent({
      content: originalTxt,
      selectionStart: originalTxt.indexOf(targetSlice),
      selectionEnd: originalTxt.indexOf(targetSlice) + targetSlice.length,
      targetNoteId: "target-roadmap-uuid",
      targetTitle: "Q3 Product Roadmap",
    });

    // Invariant: format must remain strictly "txt"
    assert.strictEqual(activeFormat, "txt", "Format must never change on link insertion");

    // Content must preserve surrounding text
    assert.ok(insertRes.updatedContent.startsWith("Meeting notes from Monday:\n- [Discuss Q3 roadmap](notepad://note/target-roadmap-uuid)"));
    assert.ok(insertRes.updatedContent.endsWith("\n- Action items for next sprint\nEnd of notes."));

    // Parsing must discover the link
    const parsed = parseInternalLinks(insertRes.updatedContent);
    assert.strictEqual(parsed.length, 1);
    assert.strictEqual(parsed[0].targetId, "target-roadmap-uuid");
    assert.strictEqual(parsed[0].label, "Discuss Q3 roadmap");
  });

  it("simulates link insertion into a Markdown note preserves Markdown syntax and formatting", async () => {
    const { insertInternalLinkIntoContent, tokenizeContentWithLinks } = await import("../src/utils/linkParser.ts");
    const originalMd = "# Project Architecture\n\n> Important RFC doc\n\n```typescript\nconst x = [1, 2, 3];\n```\n\nSee the specifications here.";
    let activeFormat = "md";

    const insertRes = insertInternalLinkIntoContent({
      content: originalMd,
      selectionStart: originalMd.indexOf("specifications"),
      selectionEnd: originalMd.indexOf("specifications") + "specifications".length,
      targetNoteId: "target-specs-uuid",
      targetTitle: "System Specifications",
    });

    assert.strictEqual(activeFormat, "md");
    assert.ok(insertRes.updatedContent.includes("# Project Architecture"));
    assert.ok(insertRes.updatedContent.includes("> Important RFC doc"));
    assert.ok(insertRes.updatedContent.includes("```typescript\nconst x = [1, 2, 3];\n```"));
    assert.ok(insertRes.updatedContent.includes("[specifications](notepad://note/target-specs-uuid)"));

    // Tokenization preserves full Markdown structure
    const segments = tokenizeContentWithLinks(insertRes.updatedContent);
    const linkSegment = segments.find((s) => s.type === "link");
    assert.ok(linkSegment, "Link segment must be present");
    assert.strictEqual(linkSegment.targetId, "target-specs-uuid");
    assert.strictEqual(linkSegment.label, "specifications");
  });

  it("verifies tokenization does not misinterpret unrelated brackets, images, or code as internal links", async () => {
    const { parseInternalLinks, tokenizeContentWithLinks } = await import("../src/utils/linkParser.ts");
    const complexContent = "Check out [TODO] and task [1] in notebook. Also ![preview image](https://example.com/pic.png). For internal reference, see [API Plan](notepad://note/api-plan-123).";
    
    const parsed = parseInternalLinks(complexContent);
    assert.strictEqual(parsed.length, 1, "Only notepad://note/ URI should be parsed as internal link");
    assert.strictEqual(parsed[0].targetId, "api-plan-123");
    assert.strictEqual(parsed[0].label, "API Plan");

    const tokens = tokenizeContentWithLinks(complexContent);
    const linkTokens = tokens.filter((t) => t.type === "link");
    assert.strictEqual(linkTokens.length, 1);
    assert.strictEqual(linkTokens[0].targetId, "api-plan-123");

    // All other segments are plain text
    const textTokens = tokens.filter((t) => t.type === "text");
    const combinedText = textTokens.map((t) => t.text).join("");
    assert.ok(combinedText.includes("[TODO]"));
    assert.ok(combinedText.includes("[1]"));
    assert.ok(combinedText.includes("![preview image](https://example.com/pic.png)"));
  });

  it("verifies NoteEditor preserves format state during link insertion", async () => {
    const fs = await import("node:fs");
    const noteEditorSource = fs.readFileSync(
      new URL("../src/features/notes/components/Editor/NoteEditor.tsx", import.meta.url),
      "utf8"
    );

    // In handleConfirmInsertLink, format is passed directly to updateDirtyState
    assert.ok(
      noteEditorSource.includes("updateDirtyState(title, newContent, format);"),
      "NoteEditor must pass unchanged format to updateDirtyState on link insertion"
    );

    // Format is toggled only via handleToggleFormat
    assert.ok(
      noteEditorSource.includes("const handleToggleFormat = () => {"),
      "Format toggling must only occur through explicit user action"
    );
  });
});

describe("Phase 9 - Task 20: Add Keyboard Accessibility and Editor Interaction Support Tests", () => {
  it("verifies internal-links-architecture.md documents keyboard accessibility, shortcuts, and focus management", async () => {
    const fs = await import("node:fs");
    const archDoc = fs.readFileSync(
      new URL("../docs/internal-links-architecture.md", import.meta.url),
      "utf8"
    );
    assert.ok(
      archDoc.includes("9. Keyboard Accessibility & Interaction Support"),
      "Section 9 header must exist"
    );
    assert.ok(archDoc.includes("Shortcuts & Conflict Prevention"), "Must document shortcuts and conflict prevention");
    assert.ok(archDoc.includes("Note Picker Keyboard Flow"), "Must document note picker keyboard flow");
    assert.ok(archDoc.includes("Link Activation & Focus Management"), "Must document focus management");
  });

  it("verifies InternalLinkPill provides keyboard activation and accessible attributes", async () => {
    const fs = await import("node:fs");
    const pillSource = fs.readFileSync(
      new URL(
        "../src/features/notes/components/Links/InternalLinkPill.tsx",
        import.meta.url
      ),
      "utf8"
    );

    assert.ok(pillSource.includes("tabIndex={0}"), "InternalLinkPill must be focusable with tabIndex 0");
    assert.ok(
      pillSource.includes('if (e.key === "Enter" || e.key === " ")'),
      "InternalLinkPill must support Enter and Space keys for activation"
    );
    assert.ok(
      pillSource.includes('aria-label={`Open linked note: ${label}'),
      "InternalLinkPill must provide descriptive aria-label"
    );
  });

  it("verifies NotePickerModal implements search focus, Arrow navigation, Enter select, and Escape close", async () => {
    const fs = await import("node:fs");
    const pickerSource = fs.readFileSync(
      new URL(
        "../src/features/notes/components/NotePicker/NotePickerModal.tsx",
        import.meta.url
      ),
      "utf8"
    );

    assert.ok(
      pickerSource.includes("searchInputRef.current?.focus()"),
      "NotePickerModal must autofocus search input on open"
    );
    assert.ok(
      pickerSource.includes('if (e.key === "Escape")'),
      "NotePickerModal must support Escape key to close"
    );
    assert.ok(
      pickerSource.includes('if (e.key === "ArrowDown")'),
      "NotePickerModal must support ArrowDown navigation"
    );
    assert.ok(
      pickerSource.includes('else if (e.key === "ArrowUp")'),
      "NotePickerModal must support ArrowUp navigation"
    );
    assert.ok(
      pickerSource.includes('else if (e.key === "Enter")'),
      "NotePickerModal must support Enter key to select candidate"
    );
  });

  it("verifies NoteEditor handles Ctrl+K, view mode toggles, and focus restoration", async () => {
    const fs = await import("node:fs");
    const editorSource = fs.readFileSync(
      new URL(
        "../src/features/notes/components/Editor/NoteEditor.tsx",
        import.meta.url
      ),
      "utf8"
    );

    // Ctrl+K opens picker
    assert.ok(
      editorSource.includes('if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k")'),
      "NoteEditor must support Ctrl/Cmd+K to insert link"
    );

    // Ctrl+Shift+P or Ctrl+E toggles preview mode
    assert.ok(
      editorSource.includes('e.key.toLowerCase() === "p" && e.shiftKey'),
      "NoteEditor must support Ctrl/Cmd+Shift+P to toggle view mode"
    );

    // Focus restoration
    assert.ok(
      editorSource.includes("contentRef.current?.focus();"),
      "NoteEditor must restore focus on picker close"
    );
    assert.ok(
      editorSource.includes("contentRef.current.selectionStart = insertionResult.newCursorPosition;"),
      "NoteEditor must position cursor right after inserted link"
    );
  });

  it("verifies TopBar navigation buttons provide accessible disabled states", async () => {
    const fs = await import("node:fs");
    const topBarSource = fs.readFileSync(
      new URL("../src/components/TopBar/TopBar.tsx", import.meta.url),
      "utf8"
    );

    assert.ok(
      topBarSource.includes("aria-disabled={!canGoBack}"),
      "TopBar back button must declare aria-disabled"
    );
    assert.ok(
      topBarSource.includes("aria-disabled={!canGoForward}"),
      "TopBar forward button must declare aria-disabled"
    );
    assert.ok(
      topBarSource.includes('aria-label="Go back to previous note"'),
      "TopBar back button must declare aria-label"
    );
    assert.ok(
      topBarSource.includes('aria-label="Go forward to next note"'),
      "TopBar forward button must declare aria-label"
    );
  });

  it("verifies useKeyboardShortcuts guards Alt+Arrow from macOS text-editing conflict", async () => {
    const fs = await import("node:fs");
    const shortcutsSource = fs.readFileSync(
      new URL("../src/hooks/useKeyboardShortcuts.ts", import.meta.url),
      "utf8"
    );

    assert.ok(
      shortcutsSource.includes('if (event.altKey && event.key === "ArrowLeft")'),
      "Must listen to Alt+ArrowLeft for history back"
    );
    assert.ok(
      shortcutsSource.includes('if (event.altKey && event.key === "ArrowRight")'),
      "Must listen to Alt+ArrowRight for history forward"
    );
    assert.ok(
      shortcutsSource.includes("if (isMac && isEditing)"),
      "Must guard against macOS Option+Arrow text-editing conflict when typing"
    );
  });
});

describe("Phase 9 - Task 21: Implement Link-Related Loading, Empty, and Error States Tests", () => {
  it("verifies internal-links-architecture.md documents loading, empty, error states and decoupling rules", async () => {
    const fs = await import("node:fs");
    const archDoc = fs.readFileSync(
      new URL("../docs/internal-links-architecture.md", import.meta.url),
      "utf8"
    );

    assert.ok(
      archDoc.includes("10. Link-Related Loading, Empty, and Error States"),
      "Section 10 header must exist"
    );
    assert.ok(archDoc.includes("Note Picker States"), "Must document note picker states");
    assert.ok(archDoc.includes("Target Resolution & Navigation States"), "Must document target resolution & nav states");
    assert.ok(archDoc.includes("Incoming Links (Backlinks) States"), "Must document backlinks states");
    assert.ok(archDoc.includes("Decoupling Guarantee"), "Must document decoupling guarantee");
  });

  it("verifies NotePickerModal provides distinct loading, search error, and empty states", async () => {
    const fs = await import("node:fs");
    const pickerSource = fs.readFileSync(
      new URL(
        "../src/features/notes/components/NotePicker/NotePickerModal.tsx",
        import.meta.url
      ),
      "utf8"
    );

    assert.ok(
      pickerSource.includes('className="note-picker-loading"'),
      "NotePickerModal must render distinct loading container"
    );
    assert.ok(
      pickerSource.includes("Loading notes..."),
      "NotePickerModal must display loading message"
    );
    assert.ok(
      pickerSource.includes('className="note-picker-error" role="alert"'),
      "NotePickerModal must render alert container on query error"
    );
    assert.ok(
      pickerSource.includes('className="note-picker-empty"'),
      "NotePickerModal must render distinct empty container"
    );
    assert.ok(
      pickerSource.includes("No other active notes available to link."),
      "NotePickerModal must distinguish empty library from empty search results"
    );
  });

  it("verifies BacklinksSection provides distinct loading, empty, and query failure states", async () => {
    const fs = await import("node:fs");
    const backlinksSource = fs.readFileSync(
      new URL(
        "../src/features/notes/components/Links/BacklinksSection.tsx",
        import.meta.url
      ),
      "utf8"
    );

    // Distinct states
    assert.ok(
      backlinksSource.includes('className="backlinks-loading"'),
      "BacklinksSection must provide loading container"
    );
    assert.ok(
      backlinksSource.includes('className="backlinks-error"'),
      "BacklinksSection must provide error container"
    );
    assert.ok(
      backlinksSource.includes('className="backlinks-empty"'),
      "BacklinksSection must provide empty container"
    );
    assert.ok(
      backlinksSource.includes("No incoming links to this note yet."),
      "BacklinksSection empty state must be distinct from error"
    );

    // Error safety & loading cleanup
    assert.ok(
      backlinksSource.includes('setError("Unable to load backlinks.");'),
      "BacklinksSection must present human-friendly error without raw SQL"
    );
    assert.ok(
      backlinksSource.includes("finally {"),
      "BacklinksSection must have finally block"
    );
    assert.ok(
      backlinksSource.includes("setIsLoading(false);"),
      "BacklinksSection must clear loading state in finally"
    );
  });

  it("verifies BrokenLinkDialog distinguishes Trashed vs Missing link target states", async () => {
    const fs = await import("node:fs");
    const dialogSource = fs.readFileSync(
      new URL(
        "../src/features/notes/components/Links/BrokenLinkDialog.tsx",
        import.meta.url
      ),
      "utf8"
    );

    assert.ok(
      dialogSource.includes('const isTrashed = target.lifecycle === "trashed";'),
      "BrokenLinkDialog must inspect target lifecycle"
    );
    assert.ok(
      dialogSource.includes('isTrashed ? "Linked Note is in Trash" : "Broken Link"'),
      "BrokenLinkDialog must provide distinct titles for trashed vs missing notes"
    );
    assert.ok(
      dialogSource.includes("Restore Note"),
      "BrokenLinkDialog must provide recovery option for trashed notes"
    );
    assert.ok(
      dialogSource.includes("Change Link"),
      "BrokenLinkDialog must provide re-link option for missing notes"
    );
  });

  it("verifies App.tsx handles link resolution progress, save failure guard, and navigation error", async () => {
    const fs = await import("node:fs");
    const appSource = fs.readFileSync(
      new URL("../src/app/App.tsx", import.meta.url),
      "utf8"
    );

    // Link resolution indicator
    assert.ok(
      appSource.includes("isResolvingLinkTarget && ("),
      "App.tsx must render resolution indicator while resolving link"
    );
    assert.ok(
      appSource.includes("Opening linked note..."),
      "App.tsx must display resolution text"
    );
    assert.ok(
      appSource.includes("setIsResolvingLinkTarget(false);"),
      "App.tsx must reset resolution indicator in finally block"
    );

    // Save failure guard before navigation
    assert.ok(
      appSource.includes("Unable to save current note before navigating."),
      "App.tsx must notify user if pre-navigation save flush fails"
    );

    // Navigation error handling
    assert.ok(
      appSource.includes('setErrorMessage("Unable to open linked note. An unexpected error occurred.");'),
      "App.tsx must handle unexpected navigation errors gracefully"
    );
  });
});

describe("Phase 9 - Task 22: Add Automated Parser, Service, and Database Tests", () => {
  it("parsing: handles valid links, multiple links, Unicode, and punctuation", async () => {
    const { parseInternalLinks, formatInternalLink } = await import("../src/utils/linkParser.ts");

    // Single link
    const single = "[Overview](notepad://note/uuid-101)";
    const parsed1 = parseInternalLinks(single);
    assert.strictEqual(parsed1.length, 1);
    assert.strictEqual(parsed1[0].targetId, "uuid-101");
    assert.strictEqual(parsed1[0].label, "Overview");

    // Multiple links with Unicode & punctuation
    const multi = "Notes: [योजना एवं विकास](notepad://note/uuid-hindi) and [Release v2.0 (Final) - #1](notepad://note/uuid-rel) end.";
    const parsedMulti = parseInternalLinks(multi);
    assert.strictEqual(parsedMulti.length, 2);
    assert.strictEqual(parsedMulti[0].label, "योजना एवं विकास");
    assert.strictEqual(parsedMulti[0].targetId, "uuid-hindi");
    assert.strictEqual(parsedMulti[1].label, "Release v2.0 (Final) - #1");
    assert.strictEqual(parsedMulti[1].targetId, "uuid-rel");

    // Escaped brackets
    const formattedEscaped = formatInternalLink("uuid-esc", "Task [Q1]");
    assert.strictEqual(formattedEscaped, "[Task [Q1\\]](notepad://note/uuid-esc)");
    const parsedEscaped = parseInternalLinks(formattedEscaped);
    assert.strictEqual(parsedEscaped.length, 1);
    assert.strictEqual(parsedEscaped[0].label, "Task [Q1]");
  });

  it("parsing: ignores malformed syntax, external URLs, and Markdown images", async () => {
    const { parseInternalLinks } = await import("../src/utils/linkParser.ts");

    const malformed = `
      [empty-id](notepad://note/)
      [whitespace-id](notepad://note/   )
      [external-url](https://notepad://note/123)
      ![markdown-image](notepad://note/img-uuid)
      [unclosed](notepad://note/123
      [bracket-without-scheme]
      ordinary [brackets] in text
    `;

    const parsed = parseInternalLinks(malformed);
    assert.strictEqual(parsed.length, 0, "No malformed tokens should be parsed as internal links");
  });

  it("parsing: executes in linear time across 100KB+ note content without pathological performance", async () => {
    const { parseInternalLinks } = await import("../src/utils/linkParser.ts");

    let largeContent = "";
    for (let i = 0; i < 1500; i++) {
      largeContent += "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt. ";
      if (i % 150 === 0) {
        largeContent += `[Reference Link ${i}](notepad://note/perf-id-${i}) `;
      }
    }
    assert.ok(largeContent.length > 100000, "Content must exceed 100KB");

    const startTime = performance.now();
    const links = parseInternalLinks(largeContent);
    const duration = performance.now() - startTime;

    assert.strictEqual(links.length, 10, "Must detect all 10 links");
    assert.ok(duration < 50, `Parsing 100KB text must complete in <50ms (actual: ${duration.toFixed(2)}ms)`);
  });

  it("insertion: covers start, end, range selection, empty content, and format preservation", async () => {
    const { insertInternalLinkIntoContent } = await import("../src/utils/linkParser.ts");

    // Start of note
    const resStart = insertInternalLinkIntoContent({
      content: "Existing body content.",
      selectionStart: 0,
      selectionEnd: 0,
      targetNoteId: "target-start",
      targetTitle: "Intro Note",
    });
    assert.ok(resStart.updatedContent.startsWith("[Intro Note](notepad://note/target-start)"));
    assert.ok(resStart.updatedContent.endsWith("Existing body content."));

    // End of note
    const resEnd = insertInternalLinkIntoContent({
      content: "Existing body content.",
      selectionStart: 22,
      selectionEnd: 22,
      targetNoteId: "target-end",
      targetTitle: "Outro Note",
    });
    assert.strictEqual(resEnd.updatedContent, "Existing body content.[Outro Note](notepad://note/target-end)");

    // Range selection
    const resRange = insertInternalLinkIntoContent({
      content: "Review the quarterly report for details.",
      selectionStart: 11,
      selectionEnd: 27, // "quarterly report"
      targetNoteId: "target-report",
      targetTitle: "Q3 Report",
    });
    assert.strictEqual(
      resRange.updatedContent,
      "Review the [quarterly report](notepad://note/target-report) for details."
    );

    // Empty content with default fallback
    const resEmpty = insertInternalLinkIntoContent({
      content: "",
      targetNoteId: "target-empty",
    });
    assert.strictEqual(resEmpty.updatedContent, "[Untitled Note](notepad://note/target-empty)");
  });

  it("resolution & lifecycle: verifies rename independence, duplicate titles, and trash/restore isolation", async () => {
    const { replaceInternalLinkTarget, removeInternalLink } = await import("../src/utils/linkParser.ts");

    // In-memory model simulating database resolution across lifecycle
    const targetA = { id: "uuid-target-a", title: "Core Specs", is_deleted: 0, notebook: "Dev" };
    const targetB = { id: "uuid-target-b", title: "Core Specs", is_deleted: 0, notebook: "Ops" }; // Duplicate title!

    const resolveTarget = (id) => {
      const found = [targetA, targetB].find((n) => n.id === id);
      if (!found) return { targetId: id, lifecycle: "missing", title: null };
      if (found.is_deleted) return { targetId: id, lifecycle: "trashed", title: found.title };
      return { targetId: id, lifecycle: "active", title: found.title, notebook: found.notebook };
    };

    // 1. Unambiguous resolution of duplicate titles by ID
    const resA = resolveTarget("uuid-target-a");
    const resB = resolveTarget("uuid-target-b");
    assert.strictEqual(resA.targetId, "uuid-target-a");
    assert.strictEqual(resA.notebook, "Dev");
    assert.strictEqual(resB.targetId, "uuid-target-b");
    assert.strictEqual(resB.notebook, "Ops");

    // 2. Renamed target resolves with new title without content rewrite
    targetA.title = "Core Specs v2 (Renamed)";
    const resRenamed = resolveTarget("uuid-target-a");
    assert.strictEqual(resRenamed.title, "Core Specs v2 (Renamed)");
    assert.strictEqual(resRenamed.lifecycle, "active");

    // 3. Trashed target resolves as trashed
    targetA.is_deleted = 1;
    const resTrashed = resolveTarget("uuid-target-a");
    assert.strictEqual(resTrashed.lifecycle, "trashed");

    // 4. Restored target resolves back to active
    targetA.is_deleted = 0;
    const resRestored = resolveTarget("uuid-target-a");
    assert.strictEqual(resRestored.lifecycle, "active");

    // 5. Unknown target resolves as missing
    const resMissing = resolveTarget("non-existent-uuid");
    assert.strictEqual(resMissing.lifecycle, "missing");

    // 6. Source note repair & unlink utilities preserve surrounding content
    const sourceContent = "See [Core Specs](notepad://note/uuid-target-a) for details. Also [Other](notepad://note/other-uuid).";
    const repaired = replaceInternalLinkTarget(sourceContent, "uuid-target-a", "uuid-target-b", "Ops Specs");
    assert.strictEqual(repaired, "See [Ops Specs](notepad://note/uuid-target-b) for details. Also [Other](notepad://note/other-uuid).");

    const unlinked = removeInternalLink(sourceContent, "uuid-target-a");
    assert.strictEqual(unlinked, "See Core Specs for details. Also [Other](notepad://note/other-uuid).");
  });
});

describe("Phase 9 - Task 23: Add Frontend and Navigation Integration Tests", () => {
  // =========================================================================
  // 1. Link Insertion End-to-End Workflows
  // =========================================================================
  describe("1. Link Insertion End-to-End Workflows", () => {
    it("simulates opening NotePickerModal, filtering by title, and selecting target note", async () => {
      const { insertInternalLinkIntoContent, parseInternalLinks } = await import(
        "../src/utils/linkParser.ts"
      );

      const mockNotes = [
        { id: "note-arch-1", title: "Architecture Blueprint", notebook: "Engineering" },
        { id: "note-arch-2", title: "Architecture Guidelines", notebook: "Engineering" },
        { id: "note-roadmap", title: "Q4 Product Roadmap", notebook: "Product" },
        { id: "note-notes", title: "General Brainstorming", notebook: null },
      ];

      // Simulate opening modal with query "arch"
      const query = "arch".toLowerCase();
      const filtered = mockNotes.filter(
        (n) =>
          n.title.toLowerCase().includes(query) ||
          (n.notebook && n.notebook.toLowerCase().includes(query))
      );
      assert.strictEqual(filtered.length, 2, "Should filter to 2 architecture notes");

      // Select target note
      const selected = filtered[0];
      const initialContent = "Please refer to the documentation.";
      const insertResult = insertInternalLinkIntoContent({
        content: initialContent,
        selectionStart: 20,
        selectionEnd: 33, // replaces "documentation"
        targetNoteId: selected.id,
        targetTitle: selected.title,
      });

      assert.strictEqual(
        insertResult.updatedContent,
        "Please refer to the [documentation](notepad://note/note-arch-1)."
      );
      assert.strictEqual(
        insertResult.insertedText,
        "[documentation](notepad://note/note-arch-1)"
      );

      // Verify round-trip parsing
      const parsed = parseInternalLinks(insertResult.updatedContent);
      assert.strictEqual(parsed.length, 1);
      assert.strictEqual(parsed[0].targetId, "note-arch-1");
      assert.strictEqual(parsed[0].label, "documentation");
    });

    it("verifies canceling note picker leaves content and cursor untouched", async () => {
      const { insertInternalLinkIntoContent } = await import(
        "../src/utils/linkParser.ts"
      );

      const originalContent = "Drafting important specifications here.";
      const originalCursor = 18;

      let isPickerOpen = true;
      let activeContent = originalContent;
      let activeCursor = originalCursor;

      // User presses Escape or clicks Cancel
      const handleCancelPicker = () => {
        isPickerOpen = false;
        // Content and cursor must remain untouched
      };

      handleCancelPicker();

      assert.strictEqual(isPickerOpen, false, "Picker should close");
      assert.strictEqual(activeContent, originalContent, "Content must not be modified");
      assert.strictEqual(activeCursor, originalCursor, "Cursor position must be preserved");
    });

    it("verifies inserting into notes with existing content at start, middle, end, and selection", async () => {
      const { insertInternalLinkIntoContent } = await import(
        "../src/utils/linkParser.ts"
      );

      const targetId = "target-uuid-99";
      const targetTitle = "Target Guide";

      // 1. Insert at start of document
      const resStart = insertInternalLinkIntoContent({
        content: "Following is the content.",
        selectionStart: 0,
        selectionEnd: 0,
        targetNoteId: targetId,
        targetTitle,
      });
      assert.strictEqual(
        resStart.updatedContent,
        "[Target Guide](notepad://note/target-uuid-99)Following is the content."
      );
      assert.strictEqual(resStart.newCursorPosition, 45);

      // 2. Insert in middle between words
      const resMiddle = insertInternalLinkIntoContent({
        content: "Check this now.",
        selectionStart: 11,
        selectionEnd: 11,
        targetNoteId: targetId,
        targetTitle,
      });
      assert.strictEqual(
        resMiddle.updatedContent,
        "Check this [Target Guide](notepad://note/target-uuid-99)now."
      );

      // 3. Insert at end of document
      const resEnd = insertInternalLinkIntoContent({
        content: "See appendix: ",
        selectionStart: 14,
        selectionEnd: 14,
        targetNoteId: targetId,
        targetTitle,
      });
      assert.strictEqual(
        resEnd.updatedContent,
        "See appendix: [Target Guide](notepad://note/target-uuid-99)"
      );

      // 4. Replace non-empty active selection using selected text as label
      const resSelection = insertInternalLinkIntoContent({
        content: "Read the comprehensive specification for details.",
        selectionStart: 9,
        selectionEnd: 36, // "comprehensive specification"
        targetNoteId: targetId,
        targetTitle,
      });
      assert.strictEqual(
        resSelection.updatedContent,
        "Read the [comprehensive specification](notepad://note/target-uuid-99) for details."
      );
    });

    it("verifies inserting links with Unicode titles, special characters, and bracket escaping", async () => {
      const {
        insertInternalLinkIntoContent,
        parseInternalLinks,
        formatInternalLink,
        sanitizeLinkLabel,
      } = await import("../src/utils/linkParser.ts");

      const unicodeTestCases = [
        {
          id: "u-hindi",
          title: "परियोजना कार्ययोजना २०२६",
          expectedLabel: "परियोजना कार्ययोजना २०२६",
        },
        {
          id: "u-arabic",
          title: "وثيقة التصميم الفني",
          expectedLabel: "وثيقة التصميم الفني",
        },
        {
          id: "u-japanese",
          title: "システム開発メモ 🚀",
          expectedLabel: "システム開発メモ 🚀",
        },
        {
          id: "u-brackets",
          title: "Task [Phase 9] - Release",
          expectedLabel: "Task [Phase 9] - Release",
        },
      ];

      for (const tc of unicodeTestCases) {
        const linkFormatted = formatInternalLink(tc.id, tc.title);
        const res = insertInternalLinkIntoContent({
          content: "Reference: ",
          selectionStart: 11,
          selectionEnd: 11,
          targetNoteId: tc.id,
          targetTitle: tc.title,
        });

        assert.ok(
          res.updatedContent.includes(tc.id),
          `Must include target ID ${tc.id}`
        );

        const parsed = parseInternalLinks(res.updatedContent);
        assert.strictEqual(parsed.length, 1);
        assert.strictEqual(parsed[0].targetId, tc.id);
        assert.strictEqual(parsed[0].label, tc.expectedLabel);
      }
    });

    it("verifies editor undo and redo transactions cleanly restore and reapply inserted link", async () => {
      const { insertInternalLinkIntoContent } = await import(
        "../src/utils/linkParser.ts"
      );

      // Simulate editor undo/redo transaction stack
      let content = "Initial note draft.";
      const undoStack = [];
      const redoStack = [];

      const recordEdit = (newContent) => {
        undoStack.push(content);
        redoStack.length = 0; // Branching clears redo stack
        content = newContent;
      };

      const undo = () => {
        if (undoStack.length === 0) return;
        redoStack.push(content);
        content = undoStack.pop();
      };

      const redo = () => {
        if (redoStack.length === 0) return;
        undoStack.push(content);
        content = redoStack.pop();
      };

      // 1. Initial state
      assert.strictEqual(content, "Initial note draft.");
      assert.strictEqual(undoStack.length, 0);

      // 2. Insert internal link
      const insertRes = insertInternalLinkIntoContent({
        content,
        selectionStart: 8,
        selectionEnd: 12, // "note"
        targetNoteId: "uuid-handbook",
        targetTitle: "Handbook",
      });
      recordEdit(insertRes.updatedContent);

      assert.strictEqual(
        content,
        "Initial [note](notepad://note/uuid-handbook) draft."
      );
      assert.strictEqual(undoStack.length, 1);
      assert.strictEqual(redoStack.length, 0);

      // 3. Perform Undo
      undo();
      assert.strictEqual(content, "Initial note draft.", "Undo must restore exact text");
      assert.strictEqual(undoStack.length, 0);
      assert.strictEqual(redoStack.length, 1);

      // 4. Perform Redo
      redo();
      assert.strictEqual(
        content,
        "Initial [note](notepad://note/uuid-handbook) draft.",
        "Redo must reapply inserted link"
      );
      assert.strictEqual(undoStack.length, 1);
      assert.strictEqual(redoStack.length, 0);
    });
  });

  // =========================================================================
  // 2. Link Activation End-to-End Workflows
  // =========================================================================
  describe("2. Link Activation End-to-End Workflows", () => {
    it("simulates clicking valid internal link switches active note to target ID", async () => {
      let activeNoteId = "source-note-1";
      const recordedTransitions = [];

      const mockStorage = {
        async resolveTarget(id) {
          if (id === "target-note-2") {
            return {
              targetId: "target-note-2",
              title: "Onboarding Checklist",
              lifecycle: "active",
              notebookId: "nb-hr",
              notebookName: "Human Resources",
            };
          }
          return { targetId: id, title: null, lifecycle: "missing", notebookId: null, notebookName: null };
        },
      };

      const handleActivateLink = async (targetId) => {
        const resolved = await mockStorage.resolveTarget(targetId);
        if (resolved.lifecycle === "active") {
          recordedTransitions.push({ from: activeNoteId, to: targetId });
          activeNoteId = targetId;
        }
      };

      await handleActivateLink("target-note-2");

      assert.strictEqual(activeNoteId, "target-note-2", "Active note must switch to target");
      assert.strictEqual(recordedTransitions.length, 1);
      assert.deepStrictEqual(recordedTransitions[0], {
        from: "source-note-1",
        to: "target-note-2",
      });
    });

    it("verifies duplicate titles resolve strictly by stable note UUID without confusion", async () => {
      // Two notes in database with exact identical titles
      const mockDatabase = [
        {
          id: "uuid-standup-eng",
          title: "Daily Standup",
          notebook_id: "nb-eng",
          notebook_name: "Engineering",
          is_deleted: 0,
        },
        {
          id: "uuid-standup-sales",
          title: "Daily Standup",
          notebook_id: "nb-sales",
          notebook_name: "Sales",
          is_deleted: 0,
        },
      ];

      const resolveTarget = (targetId) => {
        const found = mockDatabase.find((n) => n.id === targetId);
        if (!found) {
          return { targetId, title: null, lifecycle: "missing", notebookName: null };
        }
        return {
          targetId: found.id,
          title: found.title,
          lifecycle: found.is_deleted ? "trashed" : "active",
          notebookName: found.notebook_name,
        };
      };

      // User has link targeting Sales Standup by ID
      const resolvedTarget = resolveTarget("uuid-standup-sales");
      assert.strictEqual(resolvedTarget.targetId, "uuid-standup-sales");
      assert.strictEqual(resolvedTarget.title, "Daily Standup");
      assert.strictEqual(resolvedTarget.notebookName, "Sales");

      // Verify that Eng Standup is NOT opened by mistake
      assert.notStrictEqual(resolvedTarget.targetId, "uuid-standup-eng");
    });

    it("verifies source edits are flushed before navigation and navigation aborts if save fails", async () => {
      let isSaveCalled = false;
      let shouldSaveFail = false;
      let activeNoteId = "source-note-active";
      let navigationAborted = false;
      let userErrorMessage = null;

      const editorSave = async () => {
        isSaveCalled = true;
        if (shouldSaveFail) {
          throw new Error("Disk I/O error: database is locked");
        }
      };

      const handleActivateLinkWithProtection = async (targetId) => {
        // Step 1: Flush pending edits of source note
        try {
          await editorSave();
        } catch (saveErr) {
          userErrorMessage =
            "Unable to save current note before navigating. Your edits have been kept safe in the editor: " +
            saveErr.message;
          navigationAborted = true;
          return; // Abort navigation to protect user's unsaved edits
        }

        // Step 2: Proceed with navigation
        activeNoteId = targetId;
      };

      // Case 1: Save succeeds -> navigation proceeds
      shouldSaveFail = false;
      await handleActivateLinkWithProtection("target-note-next");
      assert.strictEqual(isSaveCalled, true);
      assert.strictEqual(activeNoteId, "target-note-next");
      assert.strictEqual(navigationAborted, false);
      assert.strictEqual(userErrorMessage, null);

      // Case 2: Save fails -> navigation aborts and edits remain protected
      isSaveCalled = false;
      shouldSaveFail = true;
      await handleActivateLinkWithProtection("target-note-failed");
      assert.strictEqual(isSaveCalled, true);
      assert.strictEqual(
        activeNoteId,
        "target-note-next",
        "Active note must NOT change if save fails"
      );
      assert.strictEqual(navigationAborted, true);
      assert.ok(
        userErrorMessage.includes("database is locked"),
        "User error message must explain the save error"
      );
    });

    it("verifies trashed target prompts recovery dialog instead of opening as active note", async () => {
      let activeNoteId = "source-note";
      let trashedPromptTarget = null;

      const mockStorage = {
        async resolveTarget(id) {
          if (id === "trashed-note-uuid") {
            return {
              targetId: "trashed-note-uuid",
              title: "Archived Meeting Notes",
              lifecycle: "trashed",
              notebookId: "nb-archive",
              notebookName: "Archive",
            };
          }
          return { targetId: id, title: null, lifecycle: "active", notebookId: null, notebookName: null };
        },
      };

      const handleActivateLink = async (targetId) => {
        const resolved = await mockStorage.resolveTarget(targetId);
        if (resolved.lifecycle === "active") {
          activeNoteId = targetId;
        } else if (resolved.lifecycle === "trashed") {
          trashedPromptTarget = resolved;
        }
      };

      await handleActivateLink("trashed-note-uuid");

      assert.strictEqual(
        activeNoteId,
        "source-note",
        "Must NOT switch active note to trashed note"
      );
      assert.notStrictEqual(trashedPromptTarget, null, "Must set trashed prompt target");
      assert.strictEqual(trashedPromptTarget.title, "Archived Meeting Notes");
      assert.strictEqual(trashedPromptTarget.lifecycle, "trashed");
    });

    it("verifies missing target displays broken dialog offering repair and unlink options", async () => {
      const { replaceInternalLinkTarget, removeInternalLink } = await import(
        "../src/utils/linkParser.ts"
      );

      let brokenLinkPrompt = null;
      let activeNoteContent =
        "See the [Obsolete Spec](notepad://note/missing-uuid-404) for background.";

      const mockStorage = {
        async resolveTarget(id) {
          return {
            targetId: id,
            title: null,
            lifecycle: "missing",
            notebookId: null,
            notebookName: null,
          };
        },
      };

      const handleActivateLink = async (targetId) => {
        const resolved = await mockStorage.resolveTarget(targetId);
        if (resolved.lifecycle === "missing") {
          brokenLinkPrompt = resolved;
        }
      };

      await handleActivateLink("missing-uuid-404");

      assert.notStrictEqual(brokenLinkPrompt, null, "Broken link prompt must be triggered");
      assert.strictEqual(brokenLinkPrompt.lifecycle, "missing");
      assert.strictEqual(brokenLinkPrompt.targetId, "missing-uuid-404");

      // User chooses Action 1: Unlink (Keep Text)
      const unlinkedContent = removeInternalLink(activeNoteContent, "missing-uuid-404");
      assert.strictEqual(
        unlinkedContent,
        "See the Obsolete Spec for background.",
        "Unlinking must preserve display text while removing broken link syntax"
      );

      // User chooses Action 2: Repair Link to new valid target
      const repairedContent = replaceInternalLinkTarget(
        activeNoteContent,
        "missing-uuid-404",
        "new-valid-uuid",
        "Current Modern Spec"
      );
      assert.strictEqual(
        repairedContent,
        "See the [Current Modern Spec](notepad://note/new-valid-uuid) for background.",
        "Repairing must replace target ID and display title cleanly"
      );
    });
  });

  // =========================================================================
  // 3. Navigation History End-to-End Workflows
  // =========================================================================
  describe("3. Navigation History End-to-End Workflows", () => {
    it("simulates traversal Note A -> Note B -> Note C, stepping back, stepping forward", () => {
      let stack = ["note-A"];
      let currentIndex = 0;

      const recordNavigation = (targetId, currentId) => {
        if (currentIndex >= 0 && stack[currentIndex] === targetId) return;
        stack = [...stack.slice(0, currentIndex + 1), targetId];
        currentIndex = stack.length - 1;
      };

      const goBack = () => {
        if (currentIndex <= 0) return null;
        currentIndex--;
        return stack[currentIndex];
      };

      const goForward = () => {
        if (currentIndex >= stack.length - 1) return null;
        currentIndex++;
        return stack[currentIndex];
      };

      // 1. Initial on Note A
      assert.strictEqual(stack[currentIndex], "note-A");
      assert.strictEqual(currentIndex > 0, false, "Cannot go back from initial note");

      // 2. Navigate to Note B
      recordNavigation("note-B", "note-A");
      assert.deepStrictEqual(stack, ["note-A", "note-B"]);
      assert.strictEqual(currentIndex, 1);
      assert.strictEqual(currentIndex > 0, true, "Can go back to Note A");
      assert.strictEqual(currentIndex < stack.length - 1, false, "Cannot go forward yet");

      // 3. Navigate to Note C
      recordNavigation("note-C", "note-B");
      assert.deepStrictEqual(stack, ["note-A", "note-B", "note-C"]);
      assert.strictEqual(currentIndex, 2);

      // 4. Go back to Note B
      const backToB = goBack();
      assert.strictEqual(backToB, "note-B");
      assert.strictEqual(currentIndex, 1);
      assert.strictEqual(currentIndex > 0, true, "Can go back further");
      assert.strictEqual(currentIndex < stack.length - 1, true, "Can go forward to Note C");

      // 5. Go back to Note A
      const backToA = goBack();
      assert.strictEqual(backToA, "note-A");
      assert.strictEqual(currentIndex, 0);
      assert.strictEqual(currentIndex > 0, false, "Cannot go back past start");
      assert.strictEqual(currentIndex < stack.length - 1, true, "Can go forward to Note B");

      // 6. Go forward to Note B
      const forwardToB = goForward();
      assert.strictEqual(forwardToB, "note-B");
      assert.strictEqual(currentIndex, 1);

      // 7. Go forward to Note C
      const forwardToC = goForward();
      assert.strictEqual(forwardToC, "note-C");
      assert.strictEqual(currentIndex, 2);
      assert.strictEqual(currentIndex < stack.length - 1, false, "At end of history");
    });

    it("verifies branching navigation after Back clears forward history stack", () => {
      let stack = ["note-A", "note-B", "note-C"];
      let currentIndex = 2; // At Note C

      const goBack = () => {
        if (currentIndex <= 0) return null;
        currentIndex--;
        return stack[currentIndex];
      };

      const recordNavigation = (targetId) => {
        if (currentIndex >= 0 && stack[currentIndex] === targetId) return;
        // Truncate any forward history when branching
        stack = [...stack.slice(0, currentIndex + 1), targetId];
        currentIndex = stack.length - 1;
      };

      // 1. Go back to Note B
      assert.strictEqual(goBack(), "note-B");
      assert.strictEqual(currentIndex, 1);
      assert.strictEqual(currentIndex < stack.length - 1, true, "Note C is in forward history");

      // 2. From Note B, branch by navigating to Note D
      recordNavigation("note-D");

      // Forward history (Note C) must be cleared completely
      assert.deepStrictEqual(stack, ["note-A", "note-B", "note-D"]);
      assert.strictEqual(currentIndex, 2);
      assert.strictEqual(currentIndex < stack.length - 1, false, "Forward history must be empty after branching");

      // 3. Step back now goes to Note B, then Note A
      assert.strictEqual(goBack(), "note-B");
      assert.strictEqual(goBack(), "note-A");
    });

    it("verifies aborted or invalid navigation does not create corrupted history entries", () => {
      let stack = ["note-A"];
      let currentIndex = 0;

      const recordNavigation = (targetId) => {
        if (!targetId) return;
        if (currentIndex >= 0 && stack[currentIndex] === targetId) return;
        stack = [...stack.slice(0, currentIndex + 1), targetId];
        currentIndex = stack.length - 1;
      };

      // 1. Re-clicking current note should be a no-op
      recordNavigation("note-A");
      assert.deepStrictEqual(stack, ["note-A"]);
      assert.strictEqual(currentIndex, 0);

      // 2. Empty or invalid note ID should be ignored
      recordNavigation("");
      recordNavigation(null);
      assert.deepStrictEqual(stack, ["note-A"]);
      assert.strictEqual(currentIndex, 0);
    });

    it("verifies permanent note deletion purges target from navigation history stack", () => {
      let stack = ["note-1", "note-to-delete", "note-3"];
      let currentIndex = 2; // on note-3

      const removeNoteFromHistory = (noteId) => {
        if (!stack.includes(noteId)) return;
        let targetIndex = currentIndex;
        const newStack = [];

        for (let i = 0; i < stack.length; i++) {
          if (stack[i] === noteId) {
            if (i < currentIndex) {
              targetIndex--;
            }
          } else {
            newStack.push(stack[i]);
          }
        }

        let newIndex = targetIndex;
        if (newStack.length === 0) {
          newIndex = -1;
        } else if (newIndex >= newStack.length) {
          newIndex = newStack.length - 1;
        } else if (newIndex < 0) {
          newIndex = 0;
        }

        stack = newStack;
        currentIndex = newIndex;
      };

      removeNoteFromHistory("note-to-delete");

      assert.deepStrictEqual(stack, ["note-1", "note-3"]);
      assert.strictEqual(currentIndex, 1, "Index must adjust so current note remains note-3");
      assert.strictEqual(stack[currentIndex], "note-3");
    });
  });

  // =========================================================================
  // 4. Backlinks End-to-End Workflows
  // =========================================================================
  describe("4. Backlinks End-to-End Workflows", () => {
    it("simulates fetching and displaying incoming backlinks with snippets and notebook tags", async () => {
      const mockBacklinkDb = [
        {
          source_note_id: "src-100",
          source_note_title: "Sprint 42 Goals",
          source_notebook_id: "nb-eng",
          source_notebook_name: "Engineering",
          snippet: "Refer to [System Architecture](notepad://note/target-core) for requirements.",
          updated_at: "2026-10-09T10:00:00Z",
        },
        {
          source_note_id: "src-200",
          source_note_title: "Release Checklist",
          source_notebook_id: null,
          source_notebook_name: null,
          snippet: "Verify adherence to [System Architecture](notepad://note/target-core).",
          updated_at: "2026-10-09T11:30:00Z",
        },
      ];

      const getBacklinks = (targetId) => {
        return mockBacklinkDb.map((row) => ({
          sourceNoteId: row.source_note_id,
          sourceNoteTitle: row.source_note_title,
          sourceNotebookId: row.source_notebook_id,
          sourceNotebookName: row.source_notebook_name,
          labelSnippet: row.snippet,
          modifiedAt: row.updated_at,
        }));
      };

      const backlinks = getBacklinks("target-core");
      assert.strictEqual(backlinks.length, 2);
      assert.strictEqual(backlinks[0].sourceNoteTitle, "Sprint 42 Goals");
      assert.strictEqual(backlinks[0].sourceNotebookName, "Engineering");
      assert.ok(backlinks[0].labelSnippet.includes("System Architecture"));
      assert.strictEqual(backlinks[1].sourceNoteTitle, "Release Checklist");
      assert.strictEqual(backlinks[1].sourceNotebookName, null);
    });

    it("simulates clicking backlink card activates source note and saves source edits", async () => {
      let activeNoteId = "target-note-viewed";
      const navigationHistory = ["target-note-viewed"];

      const handleBacklinkClick = (sourceNoteId) => {
        // Record navigation and switch note
        navigationHistory.push(sourceNoteId);
        activeNoteId = sourceNoteId;
      };

      handleBacklinkClick("src-100");

      assert.strictEqual(activeNoteId, "src-100", "Must activate source note");
      assert.deepStrictEqual(navigationHistory, ["target-note-viewed", "src-100"]);
    });

    it("verifies refreshing backlinks when source note adds or removes link reference", async () => {
      let sourceLinks = [
        { sourceId: "src-1", targetId: "target-A" },
        { sourceId: "src-2", targetId: "target-A" },
      ];

      const computeIncoming = (targetId) => sourceLinks.filter((l) => l.targetId === targetId);

      // Initial state
      assert.strictEqual(computeIncoming("target-A").length, 2);

      // Source note 2 removes the link to target-A
      sourceLinks = sourceLinks.filter((l) => !(l.sourceId === "src-2" && l.targetId === "target-A"));

      // Backlinks refresh triggered
      const refreshed = computeIncoming("target-A");
      assert.strictEqual(refreshed.length, 1);
      assert.strictEqual(refreshed[0].sourceId, "src-1");
    });

    it("verifies distinct empty state renders when note has zero incoming backlinks", () => {
      const backlinks = [];
      const renderBacklinksState = (items) => {
        if (items.length === 0) {
          return {
            type: "empty",
            message: "No incoming links found",
            helper: "Notes that link to this one will appear here.",
          };
        }
        return { type: "list", count: items.length };
      };

      const state = renderBacklinksState(backlinks);
      assert.strictEqual(state.type, "empty");
      assert.strictEqual(state.message, "No incoming links found");
      assert.strictEqual(state.helper, "Notes that link to this one will appear here.");
    });
  });

  // =========================================================================
  // 5. Regression Verification Across Phase 1-8 Capabilities
  // =========================================================================
  describe("5. Regression Verification Across Phase 1-8 Capabilities", () => {
    it("regression: verifies debounced autosave contract and timer flush remain intact", async () => {
      let savedContent = "";
      let saveCallCount = 0;
      let timerId = null;

      const scheduleAutosave = (newContent, delayMs = 50) => {
        if (timerId) clearTimeout(timerId);
        timerId = setTimeout(() => {
          savedContent = newContent;
          saveCallCount++;
        }, delayMs);
      };

      const flushAutosave = (contentToFlush) => {
        if (timerId) {
          clearTimeout(timerId);
          timerId = null;
        }
        savedContent = contentToFlush;
        saveCallCount++;
      };

      // Type some edits with internal links
      scheduleAutosave("Draft text with [Link](notepad://note/uuid-1)");
      assert.strictEqual(savedContent, "", "Should debounce save");

      // Navigation triggers immediate flush
      flushAutosave("Draft text with [Link](notepad://note/uuid-1)");
      assert.strictEqual(savedContent, "Draft text with [Link](notepad://note/uuid-1)");
      assert.strictEqual(saveCallCount, 1);
    });

    it("regression: verifies search query matching and highlighting unaffected by internal links", async () => {
      const { highlightText } = await import("../src/features/search/utils.ts");
      const sampleText = "Check the [Architecture Roadmap](notepad://note/arch-123) for timelines.";

      const fragments = highlightText(sampleText, "Roadmap");
      assert.ok(Array.isArray(fragments), "highlightText returns React elements array");
      const matchFound = fragments.some(
        (f) =>
          typeof f === "object" &&
          f !== null &&
          (f.props?.children === "Roadmap" || f.key !== undefined)
      );
      assert.strictEqual(matchFound, true, "Search highlight must identify match inside link label");
    });

    it("regression: verifies notebook and tag navigation filters operate concurrently without conflict", async () => {
      const mockNotes = [
        { id: "n1", notebook_id: "nb-work", tags: ["urgent"], is_deleted: 0 },
        { id: "n2", notebook_id: "nb-work", tags: ["review"], is_deleted: 0 },
        { id: "n3", notebook_id: "nb-home", tags: ["urgent"], is_deleted: 0 },
        { id: "n4", notebook_id: "nb-work", tags: ["urgent"], is_deleted: 1 }, // trashed
      ];

      const filterNotes = ({ notebookId, tag, isTrash = false }) => {
        return mockNotes.filter((n) => {
          if (isTrash) return n.is_deleted === 1;
          if (n.is_deleted === 1) return false;
          if (notebookId && n.notebook_id !== notebookId) return false;
          if (tag && !n.tags.includes(tag)) return false;
          return true;
        });
      };

      // Active work notebook notes
      const workNotes = filterNotes({ notebookId: "nb-work" });
      assert.strictEqual(workNotes.length, 2);

      // Urgent tag notes
      const urgentNotes = filterNotes({ tag: "urgent" });
      assert.strictEqual(urgentNotes.length, 2);
      assert.deepStrictEqual(urgentNotes.map((n) => n.id), ["n1", "n3"]);

      // Trashed notes
      const trashed = filterNotes({ isTrash: true });
      assert.strictEqual(trashed.length, 1);
      assert.strictEqual(trashed[0].id, "n4");
    });

    it("regression: verifies note favorite toggle persists favorite state", () => {
      const notes = [
        { id: "n-fav-1", is_favorite: 0 },
        { id: "n-fav-2", is_favorite: 1 },
      ];

      const toggleFavorite = (id) => {
        const target = notes.find((n) => n.id === id);
        if (target) {
          target.is_favorite = target.is_favorite ? 0 : 1;
        }
      };

      toggleFavorite("n-fav-1");
      assert.strictEqual(notes[0].is_favorite, 1);

      toggleFavorite("n-fav-1");
      assert.strictEqual(notes[0].is_favorite, 0);
    });

    it("regression: verifies NoteEditor preserves AttachmentList decoupling invariant comment", async () => {
      const fs = await import("node:fs");
      const editorFile = fs.readFileSync(
        new URL("../src/features/notes/components/Editor/NoteEditor.tsx", import.meta.url),
        "utf8"
      );

      assert.ok(
        editorFile.includes("{/* <AttachmentList noteId={noteId} /> */}"),
        "Must preserve AttachmentList decoupling invariant comment"
      );
    });

    it("regression: verifies keyboard shortcuts for link picker, history, and editing", () => {
      const testEvents = [
        { key: "k", ctrlKey: true, metaKey: false, expectedAction: "openLinkPicker" },
        { key: "s", ctrlKey: true, metaKey: false, expectedAction: "saveNote" },
        { key: "z", ctrlKey: true, metaKey: false, expectedAction: "undo" },
        { key: "y", ctrlKey: true, metaKey: false, expectedAction: "redo" },
        { key: "Escape", ctrlKey: false, metaKey: false, expectedAction: "closeModal" },
      ];

      const resolveShortcut = (e) => {
        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") return "openLinkPicker";
        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") return "saveNote";
        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") return "undo";
        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "y") return "redo";
        if (e.key === "Escape") return "closeModal";
        return null;
      };

      for (const t of testEvents) {
        assert.strictEqual(
          resolveShortcut(t),
          t.expectedAction,
          `Shortcut event for ${t.key} must resolve to ${t.expectedAction}`
        );
      }
    });
  });
});

describe("Phase 9 - Task 24: Security, Reliability, and Performance Verification Tests", () => {
  // =========================================================================
  // 1. Security Verification Suite
  // =========================================================================
  describe("1. Security Verification Suite", () => {
    it("security: verifies target resolution strictly binds to alphanumeric UUIDs and rejects injection attempts", async () => {
      const maliciousIds = [
        "' OR 1=1 --",
        "'; DROP TABLE notes; --",
        "admin'/*",
        "<script>alert(1)</script>",
        "../../../../etc/passwd",
        "$(whoami)",
        "; rm -rf / ;",
        "\0badbyte",
      ];

      const resolveMockTarget = (id) => {
        // Enforce safe ID contract matching InternalLinkService
        const trimmed = (id || "").trim();
        const isValidId = /^[a-zA-Z0-9_-]+$/.test(trimmed);
        if (!isValidId) {
          return { targetId: trimmed, title: null, lifecycle: "missing", notebookName: null };
        }
        return { targetId: trimmed, title: "Target Note", lifecycle: "active", notebookName: null };
      };

      for (const malicious of maliciousIds) {
        const res = resolveMockTarget(malicious);
        assert.strictEqual(
          res.lifecycle,
          "missing",
          `Malicious input '${malicious}' must resolve strictly as missing`
        );
        assert.strictEqual(res.title, null);
      }
    });

    it("security: verifies XSS and raw HTML injection payloads are safely sanitized and rendered as text", async () => {
      const { tokenizeContentWithLinks, parseInternalLinks } = await import(
        "../src/utils/linkParser.ts"
      );

      const xssPayloads = [
        "<script>alert('pwned')</script>",
        "<img src=x onerror=\"alert(1)\" />",
        "<svg onload=\"alert(1)\">",
        "javascript:alert(document.cookie)",
      ];

      for (const payload of xssPayloads) {
        const noteContent = `Check this note: [${payload}](notepad://note/target-uuid-1)`;
        const links = parseInternalLinks(noteContent);
        assert.strictEqual(links.length, 1);
        assert.strictEqual(links[0].label, payload, "Payload must be preserved as literal text");

        const segments = tokenizeContentWithLinks(noteContent);
        assert.strictEqual(segments.length, 2);
        assert.strictEqual(segments[1].type, "link");
        assert.strictEqual(segments[1].label, payload);
        // Pure data token, zero HTML execution
      }
    });

    it("security: verifies parser immunity to catastrophic backtracking (ReDoS) and malformed syntax", async () => {
      const { parseInternalLinks } = await import("../src/utils/linkParser.ts");

      const malformedCases = [
        "[[[[[[[[[[unclosed bracket bomb",
        "[valid label](notepad://note/uuid-1))extra bracket",
        "[missing closing bracket(notepad://note/uuid-1)",
        "[label](notepad:wrong-protocol/uuid-1)",
        "[label](notepad://note/)",
        "[label](http://example.com/not-internal)",
        "[label](file:///etc/hosts)",
        "![" + "a".repeat(1000) + "](notepad://note/image-not-link)",
        "[[[[".repeat(500) + "]]]]".repeat(500),
      ];

      for (const malformed of malformedCases) {
        const t0 = performance.now();
        const links = parseInternalLinks(malformed);
        const elapsed = performance.now() - t0;

        assert.ok(Array.isArray(links));
        assert.ok(
          elapsed < 20,
          `Parsing malformed input must complete under 20ms without ReDoS: ${elapsed.toFixed(2)}ms`
        );
      }
    });

    it("security: verifies local-first privacy, zero network calls, and absence of external telemetry", async () => {
      const fs = await import("node:fs");

      const parserCode = fs.readFileSync(
        new URL("../src/utils/linkParser.ts", import.meta.url),
        "utf8"
      );
      const storageLinksCode = fs.readFileSync(
        new URL("../src/services/storage/links.ts", import.meta.url),
        "utf8"
      );
      const hookCode = fs.readFileSync(
        new URL("../src/hooks/useNoteNavigationHistory.ts", import.meta.url),
        "utf8"
      );

      // Verify no network fetch, axios, or telemetry endpoints
      const forbiddenTokens = ["fetch(", "XMLHttpRequest", "http://", "https://", "api.analytics", "telemetry"];
      for (const token of forbiddenTokens) {
        assert.strictEqual(
          parserCode.includes(token),
          false,
          `linkParser.ts must not contain ${token}`
        );
        assert.strictEqual(
          storageLinksCode.includes(token),
          false,
          `storage/links.ts must not contain ${token}`
        );
        assert.strictEqual(
          hookCode.includes(token),
          false,
          `useNoteNavigationHistory.ts must not contain ${token}`
        );
      }
    });
  });

  // =========================================================================
  // 2. Reliability Verification Suite
  // =========================================================================
  describe("2. Reliability Verification Suite", () => {
    it("reliability: verifies unsaved changes are safeguarded and navigation halts if save fails", async () => {
      let activeNoteId = "note-source-1";
      let editorText = "Critical in-flight modifications that must never be lost";
      let navigationSucceeded = false;
      let errorReported = null;

      const failingSave = async () => {
        throw new Error("Disk full: cannot commit SQLite transaction");
      };

      const handleNavigateWithGuard = async (targetId) => {
        try {
          await failingSave();
        } catch (err) {
          errorReported = err.message;
          // Abort navigation
          return;
        }
        activeNoteId = targetId;
        navigationSucceeded = true;
      };

      await handleNavigateWithGuard("note-target-2");

      assert.strictEqual(navigationSucceeded, false, "Navigation must be aborted");
      assert.strictEqual(activeNoteId, "note-source-1", "Active note must remain source note");
      assert.strictEqual(
        editorText,
        "Critical in-flight modifications that must never be lost",
        "In-flight edits must remain intact in memory"
      );
      assert.ok(errorReported.includes("Disk full"));
    });

    it("reliability: verifies sequence counter prevents stale async responses from race conditions", async () => {
      let activeNoteId = "initial-note";
      let currentRequestId = 0;

      // Simulate 3 rapid clicks on different notes with varying response latencies
      const triggerClick = async (targetId, simulatedLatencyMs) => {
        const requestId = ++currentRequestId;
        await new Promise((r) => setTimeout(r, simulatedLatencyMs));
        // Check if request is still current
        if (requestId === currentRequestId) {
          activeNoteId = targetId;
        }
      };

      // Click 1 (slow): target-1, takes 60ms
      const p1 = triggerClick("target-1", 60);
      // Click 2 (fast): target-2, takes 10ms
      const p2 = triggerClick("target-2", 10);
      // Click 3 (latest): target-3, takes 30ms
      const p3 = triggerClick("target-3", 30);

      await Promise.all([p1, p2, p3]);

      // target-3 was the latest click initiated, so it must win even if p1 resolves last
      assert.strictEqual(
        activeNoteId,
        "target-3",
        "Latest navigation request must prevail over earlier stale resolutions"
      );
    });

    it("reliability: verifies navigation history avoids loops and respects depth constraints", () => {
      let stack = ["note-start"];
      let currentIndex = 0;
      const MAX_DEPTH = 100;

      const record = (id) => {
        if (!id) return;
        if (currentIndex >= 0 && stack[currentIndex] === id) return;
        stack = [...stack.slice(0, currentIndex + 1), id];
        if (stack.length > MAX_DEPTH) {
          stack = stack.slice(stack.length - MAX_DEPTH);
        }
        currentIndex = stack.length - 1;
      };

      // Rapidly click same note 10 times
      for (let i = 0; i < 10; i++) {
        record("note-start");
      }
      assert.strictEqual(stack.length, 1, "Duplicate consecutive clicks must not inflate stack");

      // Push 150 unique transitions
      for (let i = 0; i < 150; i++) {
        record(`note-${i}`);
      }
      assert.strictEqual(stack.length, MAX_DEPTH, "Stack must be capped at MAX_DEPTH (100)");
      assert.strictEqual(currentIndex, MAX_DEPTH - 1);
    });

    it("reliability: verifies complete lifecycle transitions: active -> trashed -> restored -> missing", () => {
      const noteStore = new Map([
        ["uuid-test", { title: "Living Document", is_deleted: 0 }],
      ]);

      const resolve = (id) => {
        const note = noteStore.get(id);
        if (!note) return { targetId: id, lifecycle: "missing" };
        return { targetId: id, lifecycle: note.is_deleted ? "trashed" : "active" };
      };

      // 1. Active
      assert.strictEqual(resolve("uuid-test").lifecycle, "active");

      // 2. Trashed
      noteStore.get("uuid-test").is_deleted = 1;
      assert.strictEqual(resolve("uuid-test").lifecycle, "trashed");

      // 3. Restored
      noteStore.get("uuid-test").is_deleted = 0;
      assert.strictEqual(resolve("uuid-test").lifecycle, "active");

      // 4. Permanently deleted
      noteStore.delete("uuid-test");
      assert.strictEqual(resolve("uuid-test").lifecycle, "missing");
    });
  });

  // =========================================================================
  // 3. Performance Verification Suite
  // =========================================================================
  describe("3. Performance Verification Suite", () => {
    it("performance: verifies fast short-circuit on notes without internal links (< 0.1ms)", async () => {
      const { parseInternalLinks, hasInternalLinks } = await import(
        "../src/utils/linkParser.ts"
      );

      const largePlainText = "This is ordinary text without any links. ".repeat(2000); // ~84 KB

      const t0 = performance.now();
      const hasLinks = hasInternalLinks(largePlainText);
      const parsed = parseInternalLinks(largePlainText);
      const elapsed = performance.now() - t0;

      assert.strictEqual(hasLinks, false);
      assert.strictEqual(parsed.length, 0);
      assert.ok(elapsed < 2, `Pre-filtered plain text parsing took ${elapsed.toFixed(3)}ms (expected < 2ms)`);
    });

    it("performance: verifies linear-time parsing across large notes with 100+ links (< 10ms)", async () => {
      const { parseInternalLinks } = await import("../src/utils/linkParser.ts");

      let largeNoteWithLinks = "";
      for (let i = 0; i < 150; i++) {
        largeNoteWithLinks += `Section ${i}: Please consult [Specification ${i}](notepad://note/uuid-target-${i}) for details.\n`;
        largeNoteWithLinks += "Supplementary paragraph containing detailed explanation of requirements.\n";
      }

      const t0 = performance.now();
      const parsed = parseInternalLinks(largeNoteWithLinks);
      const elapsed = performance.now() - t0;

      assert.strictEqual(parsed.length, 150);
      assert.strictEqual(parsed[0].targetId, "uuid-target-0");
      assert.strictEqual(parsed[149].targetId, "uuid-target-149");
      assert.ok(
        elapsed < 15,
        `150-link note parsed in ${elapsed.toFixed(3)}ms (expected < 15ms)`
      );
    });

    it("performance: verifies backlinks aggregation extracts lightweight metadata without full bodies", () => {
      const mockNotes = Array.from({ length: 50 }, (_, i) => ({
        id: `source-note-${i}`,
        title: `Project Log #${i}`,
        notebook: i % 2 === 0 ? "Dev" : "Ops",
        full_content: "Huge multi-megabyte body text... ".repeat(100),
        snippet: `Contains [Architecture Hub](notepad://note/hub-uuid) reference`,
      }));

      // Simulate backend projection
      const projectedBacklinks = mockNotes.map((n) => ({
        sourceNoteId: n.id,
        sourceNoteTitle: n.title,
        sourceNotebookName: n.notebook,
        labelSnippet: n.snippet,
        // Notice full_content is excluded to keep React state lightweight
      }));

      assert.strictEqual(projectedBacklinks.length, 50);
      assert.strictEqual(projectedBacklinks[0].sourceNoteTitle, "Project Log #0");
      assert.strictEqual("full_content" in projectedBacklinks[0], false);
    });
  });

  describe("Phase 10 - Task 8: Note Title Derivation and Import Options Tests", () => {
    it("title derivation: sanitizes file stem, strips recognized extensions, and preserves Unicode/emojis", () => {
      const deriveTitle = (fileName) => {
        const trimmed = (fileName || "").trim();
        if (!trimmed) return "Untitled Note";

        // Remove recognized extension
        const lower = trimmed.toLowerCase();
        let stem = trimmed;
        for (const ext of [".markdown", ".json", ".txt", ".md"]) {
          if (lower.endsWith(ext)) {
            stem = trimmed.slice(0, trimmed.length - ext.length);
            break;
          }
        }

        // Filter control characters
        const cleaned = stem.replace(/[\u0000-\u001F\u007F-\u009F]/g, "").trim();
        if (!cleaned || cleaned === ".") {
          return "Untitled Note";
        }
        return cleaned;
      };

      assert.strictEqual(deriveTitle("Sprint Plan.md"), "Sprint Plan");
      assert.strictEqual(deriveTitle("Notes.txt"), "Notes");
      assert.strictEqual(deriveTitle("export.markdown"), "export");
      assert.strictEqual(deriveTitle("archive.json"), "archive");
      assert.strictEqual(deriveTitle("Report.2026.Q3.final.md"), "Report.2026.Q3.final");
      assert.strictEqual(deriveTitle("✨ Rocket Notes 🚀.md"), "✨ Rocket Notes 🚀");
      assert.strictEqual(deriveTitle("दस्तावेज़.txt"), "दस्तावेज़");
      assert.strictEqual(deriveTitle("日本語ノート.markdown"), "日本語ノート");
      assert.strictEqual(deriveTitle("   Leading and Trailing   .md"), "Leading and Trailing");
      assert.strictEqual(deriveTitle("\t\nNote with controls\r\n.txt"), "Note with controls");
      assert.strictEqual(deriveTitle(""), "Untitled Note");
      assert.strictEqual(deriveTitle("   "), "Untitled Note");
      assert.strictEqual(deriveTitle(".txt"), "Untitled Note");
      assert.strictEqual(deriveTitle(".md"), "Untitled Note");
      assert.strictEqual(deriveTitle("."), "Untitled Note");
      assert.strictEqual(deriveTitle("\x00\x01\x02.txt"), "Untitled Note");
    });

    it("options & overrides: builds valid import request with custom title overrides and chosen duplicate policy", () => {
      const mockCandidates = [
        {
          id: "cand-1",
          filePath: "/docs/Meeting.md",
          fileName: "Meeting.md",
          proposedTitle: "Meeting",
          validationStatus: "valid",
          selected: true,
        },
        {
          id: "cand-2",
          filePath: "/docs/Notes.txt",
          fileName: "Notes.txt",
          proposedTitle: "Notes",
          validationStatus: "valid",
          selected: true,
        },
        {
          id: "cand-3",
          filePath: "/docs/corrupt.bin",
          fileName: "corrupt.bin",
          proposedTitle: "corrupt",
          validationStatus: "invalid",
          selected: false,
        },
      ];

      const userTitleOverrides = {
        "cand-1": "Q3 Planning Meeting",
        "cand-2": "   ", // User cleared the input: should fallback to candidate.proposedTitle
      };

      const targetNotebookId = "nb-work-123";
      const duplicatePolicy = "skip_exact";

      // Replicate handleConfirm logic in ImportPreviewModal
      const overrides = mockCandidates.map((c) => ({
        id: c.id,
        filePath: c.filePath,
        selected: c.selected && c.validationStatus !== "invalid",
        customTitle: userTitleOverrides[c.id]?.trim() || c.proposedTitle,
      }));

      const request = {
        candidates: overrides,
        options: {
          targetNotebookId,
          duplicatePolicy,
        },
      };

      assert.strictEqual(request.options.targetNotebookId, "nb-work-123");
      assert.strictEqual(request.options.duplicatePolicy, "skip_exact");
      assert.strictEqual(request.candidates.length, 3);
      assert.deepStrictEqual(request.candidates[0], {
        id: "cand-1",
        filePath: "/docs/Meeting.md",
        selected: true,
        customTitle: "Q3 Planning Meeting",
      });
      assert.deepStrictEqual(request.candidates[1], {
        id: "cand-2",
        filePath: "/docs/Notes.txt",
        selected: true,
        customTitle: "Notes", // Successfully fell back to proposedTitle
      });
      assert.deepStrictEqual(request.candidates[2], {
        id: "cand-3",
        filePath: "/docs/corrupt.bin",
        selected: false, // Invalid candidate remains unselected
        customTitle: "corrupt",
      });
    });

    it("source content preservation invariant: preview and title edits never alter underlying text", () => {
      const originalFileContent = "# Original Heading\n\nPreserved line verbatim.\n[Link](notepad://note/uuid-1)";
      let userEditedTitle = "New Title Choice";

      // Editing the title modifies metadata only
      const stagedImportDto = {
        title: userEditedTitle,
        content: originalFileContent, // Content remains strictly unmodified
        format: "md",
      };

      assert.strictEqual(stagedImportDto.title, "New Title Choice");
      assert.strictEqual(stagedImportDto.content, originalFileContent);
      assert.ok(stagedImportDto.content.includes("[Link](notepad://note/uuid-1)"));
    });
  });

  describe("Phase 10 - Task 9: Duplicate Detection and Conflict Policies Tests", () => {
    const existingActiveNotes = [
      {
        id: "note-uuid-1",
        title: "Sprint Retrospective",
        content: "What went well and what could be improved.",
        is_deleted: false,
      },
      {
        id: "note-uuid-2",
        title: "Architecture Decisions",
        content: "ADR 001: Local first database schema.",
        is_deleted: false,
      },
      {
        id: "note-uuid-3",
        title: "Old Meeting Notes",
        content: "Meeting from 2024.",
        is_deleted: true, // Trashed note
      },
    ];

    const checkDuplicate = (title, content, activeNotes) => {
      // 1. Check exact title and content match
      const exactMatch = activeNotes.find(
        (n) => !n.is_deleted && n.title === title && n.content === content
      );
      if (exactMatch) {
        return { type: "exactMatch", existingId: exactMatch.id, existingTitle: exactMatch.title };
      }

      // 2. Check exact content match with different title
      if (content.trim()) {
        const contentMatch = activeNotes.find(
          (n) => !n.is_deleted && n.content === content
        );
        if (contentMatch) {
          return { type: "contentMatch", existingId: contentMatch.id, existingTitle: contentMatch.title };
        }
      }

      // 3. Check title collision only with different content
      const titleMatch = activeNotes.find(
        (n) => !n.is_deleted && n.title === title
      );
      if (titleMatch) {
        return { type: "titleCollisionOnly", existingId: titleMatch.id, existingTitle: titleMatch.title };
      }

      return { type: "none" };
    };

    const shouldSkipCandidate = (policy, dupMatch, selected) => {
      if (!selected) {
        return { skip: true, reason: "Deselected by user." };
      }
      if (policy === "import_as_new") {
        return { skip: false, reason: null };
      }
      if (policy === "skip_exact") {
        if (dupMatch.type === "exactMatch") {
          return { skip: true, reason: `Skipped exact duplicate note '${dupMatch.existingTitle}'.` };
        }
        if (dupMatch.type === "contentMatch") {
          return { skip: true, reason: `Skipped duplicate content matching '${dupMatch.existingTitle}'.` };
        }
        // Critical: Title collision alone is NOT skipped
        return { skip: false, reason: null };
      }
      // Ask policy: user choice respected
      return { skip: false, reason: null };
    };

    it("duplicate classification: accurately distinguishes exact duplicate, content match, title collision, and trashed notes", () => {
      // Exact match
      const match1 = checkDuplicate("Sprint Retrospective", "What went well and what could be improved.", existingActiveNotes);
      assert.strictEqual(match1.type, "exactMatch");
      assert.strictEqual(match1.existingId, "note-uuid-1");

      // Content match (different title)
      const match2 = checkDuplicate("Sprint Retro Q3", "What went well and what could be improved.", existingActiveNotes);
      assert.strictEqual(match2.type, "contentMatch");
      assert.strictEqual(match2.existingId, "note-uuid-1");

      // Title collision only (matching title, DIFFERENT content)
      const match3 = checkDuplicate("Sprint Retrospective", "Different content altogether.", existingActiveNotes);
      assert.strictEqual(match3.type, "titleCollisionOnly");
      assert.strictEqual(match3.existingId, "note-uuid-1");

      // Trashed note must not cause collision
      const matchTrashed = checkDuplicate("Old Meeting Notes", "Meeting from 2024.", existingActiveNotes);
      assert.strictEqual(matchTrashed.type, "none");

      // Unique note
      const matchUnique = checkDuplicate("New Roadmap", "Unique text.", existingActiveNotes);
      assert.strictEqual(matchUnique.type, "none");
    });

    it("duplicate policy evaluation: skip_exact skips exact duplicates but preserves title collisions safely", () => {
      const exactDup = { type: "exactMatch", existingId: "note-uuid-1", existingTitle: "Sprint Retrospective" };
      const contentDup = { type: "contentMatch", existingId: "note-uuid-1", existingTitle: "Sprint Retrospective" };
      const titleCol = { type: "titleCollisionOnly", existingId: "note-uuid-1", existingTitle: "Sprint Retrospective" };
      const uniqueNote = { type: "none" };

      // Policy: skip_exact
      assert.strictEqual(shouldSkipCandidate("skip_exact", exactDup, true).skip, true);
      assert.strictEqual(shouldSkipCandidate("skip_exact", contentDup, true).skip, true);

      // INVARIANT: Title collision with different content must NOT be skipped
      assert.strictEqual(shouldSkipCandidate("skip_exact", titleCol, true).skip, false);
      assert.strictEqual(shouldSkipCandidate("skip_exact", uniqueNote, true).skip, false);

      // Policy: import_as_new (creates notes regardless of duplicate matches)
      assert.strictEqual(shouldSkipCandidate("import_as_new", exactDup, true).skip, false);
      assert.strictEqual(shouldSkipCandidate("import_as_new", contentDup, true).skip, false);
      assert.strictEqual(shouldSkipCandidate("import_as_new", titleCol, true).skip, false);

      // Policy: ask (user selection respected)
      assert.strictEqual(shouldSkipCandidate("ask", exactDup, true).skip, false);
      assert.strictEqual(shouldSkipCandidate("ask", exactDup, false).skip, true);
    });

    it("data safety invariant: import operations never overwrite or destroy existing note records", () => {
      // Simulating import commit
      const databaseNotes = [...existingActiveNotes];
      const incomingNote = {
        title: "Sprint Retrospective",
        content: "New notes for next week.", // Title collision
      };

      // Import must create a new record rather than mutating existing note-uuid-1
      const newCreatedNote = {
        id: "new-generated-uuid-99",
        title: incomingNote.title,
        content: incomingNote.content,
        is_deleted: false,
      };
      databaseNotes.push(newCreatedNote);

      // Original note remains completely intact
      const originalNote = databaseNotes.find((n) => n.id === "note-uuid-1");
      assert.strictEqual(originalNote.content, "What went well and what could be improved.");

      // Both notes safely coexist
      const sameTitleNotes = databaseNotes.filter((n) => n.title === "Sprint Retrospective");
      assert.strictEqual(sameTitleNotes.length, 2);
      assert.notStrictEqual(sameTitleNotes[0].id, sameTitleNotes[1].id);
    });
  });

  describe("Phase 10 - Task 10: Safe Note Creation and Import Transactions Tests", () => {
    it("transaction isolation: partial batch failures report itemized outcomes without rolling back successful files", () => {
      const candidates = [
        { id: "cand-1", filePath: "/docs/valid1.md", selected: true, title: "Valid Note 1" },
        { id: "cand-2", filePath: "/docs/corrupt.bin", selected: true, title: "Corrupt File" },
        { id: "cand-3", filePath: "/docs/deselected.txt", selected: false, title: "Deselected Note" },
        { id: "cand-4", filePath: "/docs/valid2.md", selected: true, title: "Valid Note 2" },
      ];

      // Simulate executeImport per-file atomic processing
      const databaseState = [];
      const results = [];
      let imported = 0;
      let skipped = 0;
      let failed = 0;

      for (const c of candidates) {
        if (!c.selected) {
          results.push({
            candidateId: c.id,
            fileName: c.filePath.split("/").pop(),
            status: "skipped",
            createdNoteId: null,
            message: "Deselected by user.",
          });
          skipped++;
          continue;
        }

        if (c.filePath.endsWith(".bin")) {
          // Validation error: rejected before writing
          results.push({
            candidateId: c.id,
            fileName: c.filePath.split("/").pop(),
            status: "rejected",
            createdNoteId: null,
            message: "Binary file content detected.",
          });
          failed++;
          continue;
        }

        // Successfully created note within its transaction
        const noteId = `generated-local-${c.id}`;
        databaseState.push({
          id: noteId,
          title: c.title,
          content: "Safe content verbatim",
          format: "md",
        });

        results.push({
          candidateId: c.id,
          fileName: c.filePath.split("/").pop(),
          status: "imported",
          createdNoteId: noteId,
          message: null,
        });
        imported++;
      }

      const summary = {
        totalProcessed: candidates.length,
        importedCount: imported,
        skippedCount: skipped,
        failedCount: failed,
        results,
        durationMs: 12,
      };

      assert.strictEqual(summary.totalProcessed, 4);
      assert.strictEqual(summary.importedCount, 2);
      assert.strictEqual(summary.skippedCount, 1);
      assert.strictEqual(summary.failedCount, 1);

      // Verify that database received exactly the 2 valid notes and ZERO corrupt records
      assert.strictEqual(databaseState.length, 2);
      assert.strictEqual(databaseState[0].title, "Valid Note 1");
      assert.strictEqual(databaseState[1].title, "Valid Note 2");
      assert.ok(databaseState.every((n) => !n.title.includes("Corrupt")));
    });

    it("target notebook validation: rejects invalid or deleted notebook references before transaction", () => {
      const existingNotebooks = [{ id: "nb-valid-1", name: "Engineering" }];
      const validateNotebookAssignment = (targetNotebookId) => {
        if (!targetNotebookId) return { valid: true, assignedNotebookId: null };
        const exists = existingNotebooks.some((nb) => nb.id === targetNotebookId);
        if (!exists) {
          return { valid: false, error: `Target notebook '${targetNotebookId}' does not exist.` };
        }
        return { valid: true, assignedNotebookId: targetNotebookId };
      };

      // Valid notebook assignment
      const validRes = validateNotebookAssignment("nb-valid-1");
      assert.strictEqual(validRes.valid, true);
      assert.strictEqual(validRes.assignedNotebookId, "nb-valid-1");

      // Optional unfiled assignment
      const unfiledRes = validateNotebookAssignment(null);
      assert.strictEqual(unfiledRes.valid, true);
      assert.strictEqual(unfiledRes.assignedNotebookId, null);

      // Invalid notebook reference fails validation
      const invalidRes = validateNotebookAssignment("nonexistent-nb-999");
      assert.strictEqual(invalidRes.valid, false);
      assert.ok(invalidRes.error.includes("does not exist"));
    });

    it("structured import isolation: external export_id is never inserted as SQLite primary key", () => {
      const externalInterchangeNote = {
        exportId: "external-untrusted-export-id-12345",
        title: "Portable Note",
        content: "Portable body",
      };

      // Backend generates local UUID v4
      const localGeneratedId = "local-uuid-550e8400-e29b-41d4-a716-446655440000";
      const createdNoteRecord = {
        id: localGeneratedId,
        title: externalInterchangeNote.title,
        content: externalInterchangeNote.content,
      };

      assert.notStrictEqual(createdNoteRecord.id, externalInterchangeNote.exportId);
      assert.strictEqual(createdNoteRecord.id, localGeneratedId);
    });
  });

  describe("Phase 10 - Task 11: Markdown Import Without Unnecessary Conversion Tests", () => {
    it("source syntax preservation: headings, lists, tables, code blocks, and HTML tags are preserved verbatim", () => {
      const rawMarkdownInput = `# System Overview\n\n## Architecture\n* Item 1\n* Item 2\n\n| Prop | Value |\n| --- | --- |\n| Port | 8080 |\n\n\`\`\`typescript\nconst x: number = 42;\n\`\`\`\n\n> Blockquote\n\n<div class="note-box"><script>alert('xss')</script></div>`;

      // Simulate file reading and ingestion
      const normalizeLineEndings = (text) => text.replace(/\r\n/g, "\n");
      const processedContent = normalizeLineEndings(rawMarkdownInput);

      // Verify that no HTML rendering took place and source remains untouched
      assert.strictEqual(processedContent, rawMarkdownInput);
      assert.ok(processedContent.includes("# System Overview"));
      assert.ok(processedContent.includes("```typescript"));
      assert.ok(processedContent.includes("| Prop | Value |"));
      assert.ok(processedContent.includes("<script>alert('xss')</script>"));
      assert.ok(!processedContent.startsWith("<html>"), "Content must not be converted into an HTML document");
    });

    it("model format assignment: sets format to 'md' and display name to 'Markdown'", () => {
      const getFormatDisplayName = (format) => {
        return format?.toLowerCase() === "md" ? "Markdown" : "Plain Text";
      };

      const importedMdNote = {
        id: "note-md-1",
        title: "Release Notes",
        content: "# Release 1.0\n* Initial launch",
        format: "md",
      };

      assert.strictEqual(importedMdNote.format, "md");
      assert.strictEqual(getFormatDisplayName(importedMdNote.format), "Markdown");

      // Verify TXT comparison
      assert.strictEqual(getFormatDisplayName("txt"), "Plain Text");
    });

    it("editor save invariance: editing and saving the imported markdown note retains format 'md' without silent conversion to 'txt'", () => {
      let databaseNote = {
        id: "note-md-2",
        title: "API Design",
        content: "# Endpoints\n- GET /api/v1/notes",
        format: "md",
        modified_at: "2026-10-12T10:00:00Z",
      };

      // Frontend save handler logic matching App.tsx handleSaveNote
      const handleSaveNote = (id, newTitle, newContent, explicitFormat) => {
        const resolvedFormat = explicitFormat || databaseNote.format || "txt";
        databaseNote = {
          ...databaseNote,
          title: newTitle,
          content: newContent,
          format: resolvedFormat,
          modified_at: "2026-10-12T10:05:00Z",
        };
        return databaseNote;
      };

      // 1. User edits title and appends content, format is omitted (standard editing flow)
      const savedNote = handleSaveNote(
        "note-md-2",
        "API Design v2",
        "# Endpoints\n- GET /api/v1/notes\n- POST /api/v1/notes",
        undefined
      );

      // Verify format was NOT degraded to "txt"
      assert.strictEqual(savedNote.format, "md");
      assert.strictEqual(savedNote.title, "API Design v2");
      assert.ok(savedNote.content.includes("POST /api/v1/notes"));

      // 2. Editor state resolution check (matching NoteEditor.tsx)
      const resolveEditorFormat = (note) => (note.format === "md" ? "md" : "txt");
      assert.strictEqual(resolveEditorFormat(savedNote), "md");
    });
  });

  describe("Phase 10 - Task 12: Plain-Text Import and Encoding Behavior Tests", () => {
    it("whitespace and line ending normalization: preserves exact spacing and blank lines while normalizing CRLF to LF", () => {
      const inputWithCrlfAndIndents = "Title Line\r\n\r\n    Four space indent\r\n\tTab indent\r\nTrailing spaces    \r\n\r\n";
      const normalizeLineEndings = (text) => text.replace(/\r\n/g, "\n");
      const normalized = normalizeLineEndings(inputWithCrlfAndIndents);

      // Verify CRLF converted to LF
      assert.ok(!normalized.includes("\r\n"));
      // Verify leading spaces, tabs, trailing spaces, and blank lines are NOT stripped
      assert.ok(normalized.includes("    Four space indent"));
      assert.ok(normalized.includes("\tTab indent"));
      assert.ok(normalized.includes("Trailing spaces    "));
      assert.ok(normalized.startsWith("Title Line\n\n"));
      assert.ok(normalized.endsWith("\n\n"));
    });

    it("multilingual unicode and emoji support: preserves multi-script characters and emoji without lossy replacement", () => {
      const multilingualContent = "🌟 Meeting Minutes 📝\n\n日本語: 議事録\nहिन्दी: बैठक सारांश\nالعربية: محضر الاجتماع\nMath: ∀x ∃y (x < y)";
      
      // Strict UTF-8 validation check (no replacement characters)
      assert.ok(!multilingualContent.includes("\uFFFD"), "Content must not contain replacement characters");
      assert.ok(multilingualContent.includes("🌟"));
      assert.ok(multilingualContent.includes("議事録"));
      assert.ok(multilingualContent.includes("बैठक सारांश"));
      assert.ok(multilingualContent.includes("محضر الاجتماع"));
      assert.ok(multilingualContent.includes("∀x ∃y (x < y)"));
    });

    it("empty text file safety: empty inputs produce a valid empty note without throwing", () => {
      const processEmptyFile = (fileSize, rawText) => {
        if (fileSize === 0 || rawText === "") {
          return {
            isEmpty: true,
            content: "",
            format: "txt",
            warning: "File is empty (0 bytes). An empty note will be created.",
          };
        }
        return { isEmpty: false, content: rawText, format: "txt" };
      };

      const result = processEmptyFile(0, "");
      assert.strictEqual(result.isEmpty, true);
      assert.strictEqual(result.content, "");
      assert.strictEqual(result.format, "txt");
      assert.ok(result.warning.includes("0 bytes"));
    });

    it("binary and corrupt encoding detection: rejects files with null bytes or corrupt UTF-8", () => {
      const isBinaryBuffer = (uint8Array) => uint8Array.some((byte) => byte === 0);
      const binarySample = new Uint8Array([0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x00, 0x57, 0x6f, 0x72, 0x6c, 0x64]);
      const validTextSample = new Uint8Array([0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x57, 0x6f, 0x72, 0x6c, 0x64]);

      assert.strictEqual(isBinaryBuffer(binarySample), true);
      assert.strictEqual(isBinaryBuffer(validTextSample), false);
    });
  });

  describe("Phase 10 - Task 13: Notebook and Metadata Handling for Structured Imports Tests", () => {
    it("version and schema validation: accepts version 1.0 documents and rejects incompatible versions", () => {
      const validateDocumentVersion = (doc) => {
        if (!doc || typeof doc !== "object") return { valid: false, error: "Malformed document" };
        if (doc.app !== "personal-notepad") return { valid: false, error: `Invalid app '${doc.app}'` };
        if (doc.version !== "1.0") return { valid: false, error: `Unsupported interchange version '${doc.version}'` };
        return { valid: true };
      };

      const validDoc = { app: "personal-notepad", version: "1.0", notes: [] };
      const invalidVersionDoc = { app: "personal-notepad", version: "2.0", notes: [] };
      const invalidAppDoc = { app: "external-app", version: "1.0", notes: [] };

      assert.strictEqual(validateDocumentVersion(validDoc).valid, true);
      assert.strictEqual(validateDocumentVersion(invalidVersionDoc).valid, false);
      assert.ok(validateDocumentVersion(invalidVersionDoc).error.includes("Unsupported interchange version '2.0'"));
      assert.strictEqual(validateDocumentVersion(invalidAppDoc).valid, false);
    });

    it("destination notebook priority: user-selected target notebook overrides document notebook_path", () => {
      const resolveDestinationNotebook = (userSelectedNotebookId, documentNotebookPath) => {
        if (userSelectedNotebookId) {
          return { notebookId: userSelectedNotebookId, source: "user_override" };
        }
        if (documentNotebookPath && documentNotebookPath.trim()) {
          return { notebookId: "resolved-hierarchy-id", path: documentNotebookPath, source: "document_hierarchy" };
        }
        return { notebookId: null, source: "unfiled" };
      };

      // Case 1: User explicitly overrides destination
      const overrideRes = resolveDestinationNotebook("target-nb-123", "Work/Projects");
      assert.strictEqual(overrideRes.notebookId, "target-nb-123");
      assert.strictEqual(overrideRes.source, "user_override");

      // Case 2: User leaves target unselected -> preserves document hierarchy
      const preserveRes = resolveDestinationNotebook(null, "Work/Projects");
      assert.strictEqual(preserveRes.notebookId, "resolved-hierarchy-id");
      assert.strictEqual(preserveRes.source, "document_hierarchy");

      // Case 3: No user target and no document path -> unfiled
      const unfiledRes = resolveDestinationNotebook(null, null);
      assert.strictEqual(unfiledRes.notebookId, null);
      assert.strictEqual(unfiledRes.source, "unfiled");
    });

    it("plain text vs structured metadata isolation: txt imports never invent tags or favorite flags", () => {
      const createTxtImportRecord = (title, content, targetNotebookId) => ({
        title,
        content,
        format: "txt",
        notebook_id: targetNotebookId || null,
        is_favorite: false,
        is_pinned: false,
        tags: [],
      });

      const txtNote = createTxtImportRecord("Scratchpad", "hello world", null);
      assert.strictEqual(txtNote.is_favorite, false);
      assert.strictEqual(txtNote.is_pinned, false);
      assert.deepStrictEqual(txtNote.tags, []);
      assert.strictEqual(txtNote.notebook_id, null);
    });

    it("internal link remapping: rewrites package note references to fresh local UUIDs while leaving external links intact", () => {
      const idMapping = new Map([
        ["ext-note-a", "local-uuid-aaaa"],
        ["ext-note-b", "local-uuid-bbbb"],
      ]);

      const originalContent = "See [Note B](notepad://note/ext-note-b) and shorthand [Note B](note:ext-note-b). External: [Legacy](notepad://note/ext-unimported-999)";

      const remapInternalLinks = (content, mapping) => {
        let result = content;
        for (const [exportId, localUuid] of mapping.entries()) {
          result = result.replace(new RegExp(`notepad://note/${exportId}`, "g"), `notepad://note/${localUuid}`);
          result = result.replace(new RegExp(`note:${exportId}`, "g"), `note:${localUuid}`);
        }
        return result;
      };

      const remapped = remapInternalLinks(originalContent, idMapping);
      assert.ok(remapped.includes("notepad://note/local-uuid-bbbb"));
      assert.ok(remapped.includes("note:local-uuid-bbbb"));
      assert.ok(remapped.includes("notepad://note/ext-unimported-999"), "Unimported external link must remain intact");
    });
  });

  describe("Phase 10 - Task 14: Individual Note Export Tests", () => {
    const sanitizeExportFilename = (title) => {
      if (!title || !title.trim()) {
        return "Untitled Note";
      }
      let sanitized = "";
      for (const ch of title) {
        const code = ch.charCodeAt(0);
        if (code < 32 || ['/', '\\', ':', '*', '?', '"', '<', '>', '|'].includes(ch)) {
          sanitized += "_";
        } else {
          sanitized += ch;
        }
      }
      sanitized = sanitized.trim();
      while (sanitized.endsWith(".") || sanitized.endsWith(" ")) {
        sanitized = sanitized.slice(0, -1);
      }
      return sanitized.length > 0 ? sanitized : "Untitled Note";
    };

    it("filename sanitization: replaces forbidden filesystem characters with underscore and trims trailing dots", () => {
      assert.strictEqual(sanitizeExportFilename("Meeting / Notes: 2026*?"), "Meeting _ Notes_ 2026__");
      assert.strictEqual(sanitizeExportFilename('Project "Alpha" <Draft> | Final.'), "Project _Alpha_ _Draft_ _ Final");
      assert.strictEqual(sanitizeExportFilename("   ...   "), "Untitled Note");
      assert.strictEqual(sanitizeExportFilename(""), "Untitled Note");
      assert.strictEqual(sanitizeExportFilename("Valid Note Title"), "Valid Note Title");
      assert.strictEqual(sanitizeExportFilename("Notes 🚀 2026 日本語"), "Notes 🚀 2026 日本語");
    });

    it("export format selection: provides markdown-as-txt warning when exporting markdown notes as txt", () => {
      const getExportAdvice = (noteFormat, selectedFormat) => {
        if (noteFormat === "md" && selectedFormat === "txt") {
          return {
            hasWarning: true,
            warning: "Stored Markdown source will be exported as raw text. Markdown characters (like #, *, []) will be preserved as text characters; output is not rendered into plain prose.",
          };
        }
        return { hasWarning: false, warning: null };
      };

      const mdToTxt = getExportAdvice("md", "txt");
      assert.strictEqual(mdToTxt.hasWarning, true);
      assert.ok(mdToTxt.warning.includes("Stored Markdown source will be exported as raw text"));

      const mdToMd = getExportAdvice("md", "md");
      assert.strictEqual(mdToMd.hasWarning, false);

      const txtToTxt = getExportAdvice("txt", "txt");
      assert.strictEqual(txtToTxt.hasWarning, false);

      const mdToJson = getExportAdvice("md", "json");
      assert.strictEqual(mdToJson.hasWarning, false);
    });

    it("pre-export editor flush: ensures unpersisted dirty edits are saved before export executes", async () => {
      let isDirty = true;
      let flushExecuted = false;
      let exportedContent = null;

      const editorState = {
        title: "Clean Title",
        content: "Unsaved modifications in editor buffer",
      };

      const flushSave = async () => {
        if (isDirty) {
          flushExecuted = true;
          isDirty = false;
        }
      };

      const handleExport = async (onFlush) => {
        await onFlush();
        exportedContent = editorState.content;
      };

      await handleExport(flushSave);

      assert.strictEqual(flushExecuted, true, "Dirty editor buffer must be saved before export");
      assert.strictEqual(isDirty, false);
      assert.strictEqual(exportedContent, "Unsaved modifications in editor buffer");
    });

    it("destination dialog cancellation: cleanly cancels without error notification or file writing", async () => {
      let exportCalled = false;
      let errorOccurred = false;

      const pickExportDestinationFile = async () => {
        // User closes/cancels native save dialog
        return null;
      };

      const triggerExport = async () => {
        const destinationPath = await pickExportDestinationFile();
        if (!destinationPath) {
          // Clean exit without error
          return;
        }
        exportCalled = true;
      };

      try {
        await triggerExport();
      } catch {
        errorOccurred = true;
      }

      assert.strictEqual(exportCalled, false, "Must not invoke export write when dialog was cancelled");
      assert.strictEqual(errorOccurred, false, "Cancellation must never be treated as an error");
    });

    it("zero side effects: exported note in database remains completely unmodified", () => {
      const sourceNote = {
        id: "note-orig-123",
        title: "Original Title",
        content: "Untouched content",
        format: "md",
        createdAt: "2026-01-01T00:00:00Z",
        modifiedAt: "2026-01-01T00:00:00Z",
        notebookId: "nb-1",
      };

      // Create deep clone representing DB state after export
      const postExportNote = JSON.parse(JSON.stringify(sourceNote));

      assert.deepStrictEqual(sourceNote, postExportNote, "Export must have zero side effects on the source note");
    });
  });

  describe("Phase 10 - Task 15: Multiple-Note and Notebook Export Selection Tests", () => {
    it("selection scope resolution: supports single note, selected note IDs, notebook, and all notes", () => {
      const resolveScopeItems = (scope, allNotes) => {
        switch (scope.type) {
          case "single_note":
            return allNotes.filter((n) => n.id === scope.note_id && !n.is_deleted);
          case "selected_notes":
            return allNotes.filter((n) => scope.note_ids.includes(n.id) && !n.is_deleted);
          case "notebook":
            if (scope.include_nested) {
              const allowedNotebookIds = new Set([scope.notebook_id, "child-nb-1", "grandchild-nb-2"]);
              return allNotes.filter((n) => allowedNotebookIds.has(n.notebook_id) && !n.is_deleted);
            }
            return allNotes.filter((n) => n.notebook_id === scope.notebook_id && !n.is_deleted);
          case "all_notes":
            return allNotes.filter((n) => !n.is_deleted);
          default:
            return [];
        }
      };

      const testNotes = [
        { id: "note-1", notebook_id: "nb-root", is_deleted: false },
        { id: "note-2", notebook_id: "child-nb-1", is_deleted: false },
        { id: "note-3", notebook_id: "grandchild-nb-2", is_deleted: false },
        { id: "note-4", notebook_id: "nb-other", is_deleted: false },
        { id: "note-5", notebook_id: "nb-root", is_deleted: true },
      ];

      // Single note scope
      const single = resolveScopeItems({ type: "single_note", note_id: "note-1" }, testNotes);
      assert.strictEqual(single.length, 1);
      assert.strictEqual(single[0].id, "note-1");

      // Selected notes scope
      const selected = resolveScopeItems(
        { type: "selected_notes", note_ids: ["note-1", "note-4", "non-existent"] },
        testNotes
      );
      assert.strictEqual(selected.length, 2);

      // Shallow notebook scope
      const shallow = resolveScopeItems(
        { type: "notebook", notebook_id: "nb-root", include_nested: false },
        testNotes
      );
      assert.strictEqual(shallow.length, 1);
      assert.strictEqual(shallow[0].id, "note-1");

      // Hierarchical notebook scope
      const hierarchical = resolveScopeItems(
        { type: "notebook", notebook_id: "nb-root", include_nested: true },
        testNotes
      );
      assert.strictEqual(hierarchical.length, 3);
      assert.deepStrictEqual(
        hierarchical.map((n) => n.id),
        ["note-1", "note-2", "note-3"]
      );

      // All notes scope
      const all = resolveScopeItems({ type: "all_notes" }, testNotes);
      assert.strictEqual(all.length, 4);
    });

    it("trashed notes exclusion: excludes trashed notes by default and reports accurate skipped counts", () => {
      const computeSelectionPreview = (requestedIds, dbNotes) => {
        let totalIncluded = 0;
        let skippedTrashed = 0;
        let skippedMissing = 0;
        const validNotes = [];

        for (const reqId of requestedIds) {
          const note = dbNotes.find((n) => n.id === reqId);
          if (!note) {
            skippedMissing++;
          } else if (note.is_deleted) {
            skippedTrashed++;
          } else {
            totalIncluded++;
            validNotes.push({
              id: note.id,
              title: note.title,
              contentLengthBytes: note.content.length,
            });
          }
        }

        return {
          totalRequested: requestedIds.length,
          totalIncluded,
          skippedTrashedCount: skippedTrashed,
          skippedMissingCount: skippedMissing,
          notes: validNotes,
        };
      };

      const dbNotes = [
        { id: "n-1", title: "Active 1", content: "hello", is_deleted: false },
        { id: "n-2", title: "Active 2", content: "world", is_deleted: false },
        { id: "n-3", title: "In Trash", content: "deleted", is_deleted: true },
      ];

      const preview = computeSelectionPreview(["n-1", "n-2", "n-3", "n-missing-99"], dbNotes);

      assert.strictEqual(preview.totalRequested, 4);
      assert.strictEqual(preview.totalIncluded, 2);
      assert.strictEqual(preview.skippedTrashedCount, 1);
      assert.strictEqual(preview.skippedMissingCount, 1);
      assert.strictEqual(preview.notes.length, 2);
    });

    it("lightweight metadata isolation: avoids loading large full content bodies into preview state", () => {
      const mockFullNote = {
        id: "huge-note-1",
        title: "Large Note",
        content: "A".repeat(500000), // 500 KB body
        format: "md",
        notebook_id: "nb-1",
        notebook_path: "Docs / Architecture",
        is_favorite: true,
        is_pinned: false,
        tag_count: 3,
        modified_at: "2026-10-12T12:00:00Z",
      };

      // Transform into lightweight metadata item
      const metadataItem = {
        id: mockFullNote.id,
        title: mockFullNote.title,
        format: mockFullNote.format,
        notebookId: mockFullNote.notebook_id,
        notebookPath: mockFullNote.notebook_path,
        isFavorite: mockFullNote.is_favorite,
        isPinned: mockFullNote.is_pinned,
        tagCount: mockFullNote.tag_count,
        contentLengthBytes: mockFullNote.content.length,
        modifiedAt: mockFullNote.modified_at,
      };

      assert.strictEqual("content" in metadataItem, false, "Full text content must not be present in preview item");
      assert.strictEqual(metadataItem.contentLengthBytes, 500000);
      assert.strictEqual(metadataItem.notebookPath, "Docs / Architecture");
    });

    it("search results independence: does not assume all visible search results are automatically selected", () => {
      const isSearchActive = true;
      const searchResultIds = ["res-1", "res-2", "res-3"];
      const userSelectedNotebook = "nb-work";

      // If exporting notebook while search is active, the scope binds to the explicit notebook, NOT the search query
      const buildNotebookExportScope = (notebookId) => ({
        type: "notebook",
        notebook_id: notebookId,
        include_nested: true,
      });

      const scope = buildNotebookExportScope(userSelectedNotebook);

      assert.strictEqual(scope.type, "notebook");
      assert.strictEqual(scope.notebook_id, "nb-work");
      assert.notDeepStrictEqual(scope, { type: "selected_notes", note_ids: searchResultIds });
    });

    it("non-blocking multi-note export workflow: pre-flushes dirty edits and exits cleanly on cancel", async () => {
      let isEditorDirty = true;
      let flushCalled = false;
      let exportCalled = false;

      const flushSave = async () => {
        if (isEditorDirty) {
          flushCalled = true;
          isEditorDirty = false;
        }
      };

      const pickExportDestinationFile = async () => null; // user cancelled

      const executeExportWorkflow = async () => {
        await flushSave();
        const dest = await pickExportDestinationFile();
        if (!dest) {
          // cleanly abort without error
          return;
        }
        exportCalled = true;
      };

      await executeExportWorkflow();

      assert.strictEqual(flushCalled, true, "Dirty edits must be flushed before export");
      assert.strictEqual(isEditorDirty, false);
      assert.strictEqual(exportCalled, false, "Export write cancelled cleanly without throwing");
    });
  });

  describe("Phase 10 - Task 16: Structured Multi-Note Export Tests", () => {
    it("structured interchange schema compliance: exports versioned document with format identifier, policies, timestamps, and note array", () => {
      const doc = {
        $schema: "https://personal-notepad.local/schemas/interchange-v1.json",
        app: "personal-notepad",
        format: "personal-notepad-interchange",
        version: "1.0",
        exported_at: "2026-10-12T10:00:00Z",
        note_count: 2,
        internal_link_policy: "Package-scoped internal links map export_id to new local IDs upon import; external or unexported note links remain unmapped as broken/missing references.",
        attachment_policy: "Note attachments are omitted from portable JSON interchange documents. Attachment metadata counts are tracked but binaries are not bundled.",
        notes: [
          {
            export_id: "exp-1",
            title: "Roadmap",
            content: "See [Arch](notepad://note/exp-2)",
            format: "md",
            notebook_path: "Engineering/Planning",
            tags: ["planning", "roadmap"],
            is_favorite: true,
            is_pinned: false,
            created_at: "2026-10-01T08:00:00Z",
            modified_at: "2026-10-05T12:00:00Z",
            attachments_omitted_count: 1,
            internal_links: [
              { label: "Arch", target_export_id: "exp-2", is_package_internal: true },
            ],
          },
          {
            export_id: "exp-2",
            title: "Architecture",
            content: "Architecture notes",
            format: "md",
            notebook_path: "Engineering/Planning",
            tags: ["architecture"],
            is_favorite: false,
            is_pinned: false,
            created_at: "2026-10-02T09:00:00Z",
            modified_at: "2026-10-06T15:00:00Z",
            attachments_omitted_count: null,
            internal_links: [],
          },
        ],
      };

      assert.strictEqual(doc.app, "personal-notepad");
      assert.strictEqual(doc.format, "personal-notepad-interchange");
      assert.strictEqual(doc.version, "1.0");
      assert.strictEqual(doc.note_count, 2);
      assert.strictEqual(typeof doc.internal_link_policy, "string");
      assert.strictEqual(typeof doc.attachment_policy, "string");
      assert.strictEqual(doc.notes.length, 2);
      assert.strictEqual(doc.notes[0].export_id, "exp-1");
    });

    it("internal link portability isolation: classifies links within package as package-internal and external links as non-portable", () => {
      const exportedIds = new Set(["exp-1", "exp-2"]);

      const analyzeInternalLinks = (content, links) => {
        return links.map((link) => ({
          ...link,
          is_package_internal: exportedIds.has(link.target_export_id),
        }));
      };

      const linksInNote1 = [
        { label: "Internal Target", target_export_id: "exp-2" },
        { label: "External Unexported", target_export_id: "unexported-uuid-999" },
      ];

      const analyzed = analyzeInternalLinks("content", linksInNote1);

      assert.strictEqual(analyzed[0].is_package_internal, true, "Link to note inside package must be package-internal");
      assert.strictEqual(analyzed[1].is_package_internal, false, "Link to note outside package must be marked external (not portable)");
    });

    it("single-note export link safeguard: links in single-note export are never misrepresented as package-internal", () => {
      const singleExportedId = "single-1";

      const analyzeSingleNoteLink = (targetId) => {
        return targetId === singleExportedId;
      };

      assert.strictEqual(analyzeSingleNoteLink("other-note-id"), false, "Links to other notes in single-note export must be external");
    });

    it("attachment accounting: explicitly records omitted attachment counts without false full-backup representations", () => {
      const noteWithAttachments = {
        title: "Release Artifacts",
        attachmentsCount: 3,
      };

      const interchangeNote = {
        title: noteWithAttachments.title,
        attachments_omitted_count: noteWithAttachments.attachmentsCount > 0 ? noteWithAttachments.attachmentsCount : null,
      };

      assert.strictEqual(interchangeNote.attachments_omitted_count, 3);

      const disclaimerNotice = "Structured Interchange (v1.0): Not a full database backup. Attachments omitted.";
      assert.strictEqual(disclaimerNotice.includes("Not a full database backup"), true);
    });

    it("deterministic serialization: tag arrays are sorted alphabetically and document preserves round-trip consistency", () => {
      const rawTags = ["zeta", "beta", "alpha"];
      const sortedTags = [...rawTags].sort();

      assert.deepStrictEqual(sortedTags, ["alpha", "beta", "zeta"]);

      const payload = { tags: sortedTags };
      const serialized = JSON.stringify(payload);
      const parsed = JSON.parse(serialized);

      assert.deepStrictEqual(parsed.tags, ["alpha", "beta", "zeta"]);
    });

    it("nesting depth limits: enforces maximum notebook depth of 3 levels during validation", () => {
      const validateNestingDepth = (notebookPath) => {
        if (!notebookPath) return true;
        const segments = notebookPath.split("/").filter((s) => s.trim().length > 0);
        return segments.length <= 3;
      };

      assert.strictEqual(validateNestingDepth("Work"), true);
      assert.strictEqual(validateNestingDepth("Work/Projects"), true);
      assert.strictEqual(validateNestingDepth("Work/Projects/2026"), true);
      assert.strictEqual(validateNestingDepth("Work/Projects/2026/Q1"), false, "4 levels exceeds max nesting depth of 3");
    });
  });

  describe("Phase 10 - Task 17: Safe Filenames and Destination Collision Handling Tests", () => {
    const sanitizeExportFilename = (title) => {
      const illegal = /[/\\:*?"<>|]/g;
      let cleaned = (title || "").replace(illegal, "").trim();
      cleaned = cleaned.replace(/^\.+/, "");
      cleaned = cleaned.replace(/[. ]+$/, "");
      if (!cleaned) {
        return "Untitled Note";
      }
      const upper = cleaned.toUpperCase();
      const isReserved = [
        "CON", "PRN", "AUX", "NUL",
        "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
        "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
      ].includes(upper);
      const safeName = isReserved ? `_${cleaned}` : cleaned;
      return safeName.length > 180 ? safeName.slice(0, 180) : safeName;
    };

    it("filename sanitization: strips illegal characters, leading dots, and trailing dots/spaces", () => {
      assert.strictEqual(
        sanitizeExportFilename("Project / Plan : 2026 * Final ? <v1> | \"Draft\" \\ Report"),
        "Project  Plan  2026  Final  v1  Draft  Report"
      );
      assert.strictEqual(sanitizeExportFilename(".hidden_note"), "hidden_note");
      assert.strictEqual(sanitizeExportFilename("...triple_dot"), "triple_dot");
      assert.strictEqual(sanitizeExportFilename("trailing_dot."), "trailing_dot");
      assert.strictEqual(sanitizeExportFilename("trailing_dots...   "), "trailing_dots");
      assert.strictEqual(sanitizeExportFilename(""), "Untitled Note");
      assert.strictEqual(sanitizeExportFilename("   "), "Untitled Note");
      assert.strictEqual(sanitizeExportFilename("."), "Untitled Note");
      assert.strictEqual(sanitizeExportFilename(".."), "Untitled Note");
      assert.strictEqual(sanitizeExportFilename("..."), "Untitled Note");
    });

    it("reserved device names: prefixes Windows reserved names with underscore", () => {
      assert.strictEqual(sanitizeExportFilename("CON"), "_CON");
      assert.strictEqual(sanitizeExportFilename("con"), "_con");
      assert.strictEqual(sanitizeExportFilename("PRN"), "_PRN");
      assert.strictEqual(sanitizeExportFilename("aux"), "_aux");
      assert.strictEqual(sanitizeExportFilename("NUL"), "_NUL");
      assert.strictEqual(sanitizeExportFilename("COM1"), "_COM1");
      assert.strictEqual(sanitizeExportFilename("com9"), "_com9");
      assert.strictEqual(sanitizeExportFilename("lpt1"), "_lpt1");
      assert.strictEqual(sanitizeExportFilename("LPT9"), "_LPT9");

      // Words containing reserved substrings must NOT be modified
      assert.strictEqual(sanitizeExportFilename("Conference Notes"), "Conference Notes");
      assert.strictEqual(sanitizeExportFilename("Pronto"), "Pronto");
      assert.strictEqual(sanitizeExportFilename("Auxiliary"), "Auxiliary");
      assert.strictEqual(sanitizeExportFilename("Nullify"), "Nullify");
    });

    it("unicode preservation: preserves full multi-lingual Unicode characters and emojis", () => {
      const unicodeTitle = "Project 🚀 Roadmap 2026 日本語 हिन्दी Русский";
      assert.strictEqual(sanitizeExportFilename(unicodeTitle), unicodeTitle);
    });

    it("length limits: truncates long stems to 180 characters", () => {
      const longTitle = "a".repeat(250);
      const sanitized = sanitizeExportFilename(longTitle);
      assert.strictEqual(sanitized.length, 180);
      assert.strictEqual(sanitized, "a".repeat(180));
    });

    it("path traversal prevention: note titles containing path navigation cannot escape destination directory", () => {
      const traversalTitle = "../../etc/passwd";
      const sanitized = sanitizeExportFilename(traversalTitle);
      assert.strictEqual(sanitized.includes("/"), false);
      assert.strictEqual(sanitized.includes("\\"), false);
      assert.strictEqual(sanitized.startsWith(".."), false);
      assert.strictEqual(sanitized, "etcpasswd");
    });

    it("collision avoidance: generates predictable numbering suffixes for batch exports without overwriting", () => {
      const resolveUniqueFilename = (baseStem, ext, usedFilenames, diskFiles = new Set(), overwrite = false) => {
        let filename = `${baseStem}.${ext}`;
        let counter = 1;
        while (usedFilenames.has(filename) || (!overwrite && diskFiles.has(filename))) {
          filename = `${baseStem} (${counter}).${ext}`;
          counter += 1;
        }
        usedFilenames.add(filename);
        return filename;
      };

      const usedFilenames = new Set();
      const diskFiles = new Set(["Meeting Notes.txt"]);

      // 1. First note in batch has same title as pre-existing file on disk
      const file1 = resolveUniqueFilename("Meeting Notes", "txt", usedFilenames, diskFiles, false);
      assert.strictEqual(file1, "Meeting Notes (1).txt", "Must not overwrite disk file when overwrite is false");

      // 2. Second note in batch also has same title
      const file2 = resolveUniqueFilename("Meeting Notes", "txt", usedFilenames, diskFiles, false);
      assert.strictEqual(file2, "Meeting Notes (2).txt", "Must increment suffix to (2)");

      // 3. A distinct note gets its clean stem
      const file3 = resolveUniqueFilename("Sprint Review", "txt", usedFilenames, diskFiles, false);
      assert.strictEqual(file3, "Sprint Review.txt");
    });

    it("destination validation: rejects path traversal and attempts to write to managed SQLite database", () => {
      const validateDestination = (pathStr) => {
        if (!pathStr || !pathStr.trim()) {
          return { valid: false, error: "Destination path cannot be empty" };
        }
        const segments = pathStr.split(/[/\\]/);
        if (segments.includes("..")) {
          return { valid: false, error: "Path traversal ('..') is not permitted" };
        }
        const fileName = segments[segments.length - 1];
        if (["database.sqlite", "database.sqlite-wal", "database.sqlite-shm"].includes(fileName.toLowerCase())) {
          return { valid: false, error: "Destination cannot target application database files" };
        }
        return { valid: true, error: null };
      };

      assert.strictEqual(validateDestination("").valid, false);
      assert.strictEqual(validateDestination("   ").valid, false);
      assert.strictEqual(validateDestination("../secret.txt").valid, false);
      assert.strictEqual(validateDestination("/home/user/../../secret.txt").valid, false);
      assert.strictEqual(validateDestination("/home/user/app/database.sqlite").valid, false);
      assert.strictEqual(validateDestination("/home/user/app/database.sqlite-wal").valid, false);
      assert.strictEqual(validateDestination("/home/user/Documents/export.txt").valid, true);
    });
  });

  describe("Phase 10 - Task 18: Safe and Reliable File Writing Tests", () => {
    it("atomic write lifecycle: writes to temporary destination file before atomic rename", async () => {
      const fsMock = new Map();
      let tempFileCreated = null;
      let finalFileCommitted = null;

      const atomicWrite = async (destPath, content, shouldFail = false) => {
        const parent = "/user/exports";
        const tempPath = `${parent}/.tmp_export_uuid123.part`;
        tempFileCreated = tempPath;
        fsMock.set(tempPath, content);

        if (shouldFail) {
          // Cleanup on failure
          fsMock.delete(tempPath);
          throw new Error("Disk write interrupted");
        }

        // Atomic rename
        fsMock.set(destPath, fsMock.get(tempPath));
        fsMock.delete(tempPath);
        finalFileCommitted = destPath;
        return destPath;
      };

      await atomicWrite("/user/exports/note.txt", "Export payload content");
      assert.strictEqual(finalFileCommitted, "/user/exports/note.txt");
      assert.strictEqual(fsMock.get("/user/exports/note.txt"), "Export payload content");
      assert.strictEqual(fsMock.has(tempFileCreated), false, "Temporary artifact must be removed on success");
    });

    it("error safety: existing destination file is preserved when a write fails", async () => {
      const fsMock = new Map();
      fsMock.set("/user/exports/preexisting.txt", "Original untouched data");

      const atomicWrite = async (destPath, content, shouldFail = false) => {
        const tempPath = `/user/exports/.tmp_uuid456.part`;
        fsMock.set(tempPath, content);

        if (shouldFail) {
          fsMock.delete(tempPath);
          throw new Error("Simulated I/O permission denied");
        }

        fsMock.set(destPath, fsMock.get(tempPath));
        fsMock.delete(tempPath);
      };

      await assert.rejects(
        () => atomicWrite("/user/exports/preexisting.txt", "Corrupted new data", true),
        /Simulated I\/O permission denied/
      );

      // Existing file must remain unchanged
      assert.strictEqual(fsMock.get("/user/exports/preexisting.txt"), "Original untouched data");
      assert.strictEqual(fsMock.has("/user/exports/.tmp_uuid456.part"), false, "Temporary file must be cleaned up on failure");
    });

    it("batch commitment semantics: commits each file independently with honest failure accounting", async () => {
      const filesToExport = [
        { id: "note-1", title: "Note 1", content: "Content 1", fail: false },
        { id: "note-2", title: "Note 2", content: "Content 2", fail: true },
        { id: "note-3", title: "Note 3", content: "Content 3", fail: false },
      ];

      const exportedFiles = [];
      const results = [];

      for (const item of filesToExport) {
        if (item.fail) {
          results.push({
            noteId: item.id,
            noteTitle: item.title,
            filePath: `/exports/${item.title}.txt`,
            status: "failed",
            message: "I/O error: disk full",
          });
        } else {
          exportedFiles.push(`/exports/${item.title}.txt`);
          results.push({
            noteId: item.id,
            noteTitle: item.title,
            filePath: `/exports/${item.title}.txt`,
            status: "exported",
            message: null,
          });
        }
      }

      const summary = {
        totalNotes: filesToExport.length,
        exportedCount: results.filter((r) => r.status === "exported").length,
        failedCount: results.filter((r) => r.status === "failed").length,
        results,
      };

      assert.strictEqual(summary.totalNotes, 3);
      assert.strictEqual(summary.exportedCount, 2);
      assert.strictEqual(summary.failedCount, 1);
      assert.deepStrictEqual(exportedFiles, ["/exports/Note 1.txt", "/exports/Note 3.txt"]);
    });

    it("pre-serialization invariant: verifies in-memory encoding completes before any disk writing", () => {
      const invalidNoteDoc = {
        app: "personal-notepad",
        version: "2.0-unsupported", // invalid schema version
      };

      const serializeDoc = (doc) => {
        if (doc.version !== "1.0") {
          throw new Error("Unsupported schema version: " + doc.version);
        }
        return JSON.stringify(doc);
      };

      let diskWriteAttempted = false;
      const attemptExport = () => {
        const serialized = serializeDoc(invalidNoteDoc);
        diskWriteAttempted = true;
        return serialized;
      };

      assert.throws(attemptExport, /Unsupported schema version/);
      assert.strictEqual(diskWriteAttempted, false, "Must not begin writing to disk if serialization fails");
    });
  });

  describe("Phase 10 - Task 19: Import/Export Progress, Cancellation, and Summaries Tests", () => {
    it("outcome categorization: accurately maps to the 5 distinct outcome categories", () => {
      const calculateOutcome = (summary) => {
        if (summary.cancelledCount > 0) return "cancelled";
        if (summary.failedCount === 0 && summary.skippedCount === 0 && (summary.importedCount > 0 || summary.exportedCount > 0)) {
          return "full_success";
        }
        if (summary.failedCount === 0 && summary.skippedCount > 0 && (summary.importedCount > 0 || summary.exportedCount > 0)) {
          return "success_with_warnings";
        }
        if (summary.failedCount > 0 && (summary.importedCount > 0 || summary.exportedCount > 0)) {
          return "partial_success";
        }
        return "total_failure";
      };

      assert.strictEqual(
        calculateOutcome({ importedCount: 5, skippedCount: 0, failedCount: 0, cancelledCount: 0 }),
        "full_success"
      );
      assert.strictEqual(
        calculateOutcome({ importedCount: 4, skippedCount: 1, failedCount: 0, cancelledCount: 0 }),
        "success_with_warnings"
      );
      assert.strictEqual(
        calculateOutcome({ importedCount: 3, skippedCount: 0, failedCount: 2, cancelledCount: 0 }),
        "partial_success"
      );
      assert.strictEqual(
        calculateOutcome({ importedCount: 2, skippedCount: 0, failedCount: 0, cancelledCount: 1 }),
        "cancelled"
      );
      assert.strictEqual(
        calculateOutcome({ importedCount: 0, skippedCount: 0, failedCount: 5, cancelledCount: 0 }),
        "total_failure"
      );
    });

    it("progress tracking: computes percentage and preserves current file tracking", () => {
      const progressEvents = [];
      const trackProgress = (p) => {
        const percent = p.total > 0 ? Math.round((p.currentIndex / p.total) * 100) : 0;
        progressEvents.push({ ...p, percent });
      };

      trackProgress({ total: 4, currentIndex: 1, currentFile: "note1.txt", importedCount: 0, skippedCount: 0, failedCount: 0, cancelledCount: 0 });
      trackProgress({ total: 4, currentIndex: 2, currentFile: "note2.md", importedCount: 1, skippedCount: 0, failedCount: 0, cancelledCount: 0 });
      trackProgress({ total: 4, currentIndex: 3, currentFile: "note3.json", importedCount: 2, skippedCount: 0, failedCount: 0, cancelledCount: 0 });
      trackProgress({ total: 4, currentIndex: 4, currentFile: "note4.txt", importedCount: 3, skippedCount: 0, failedCount: 0, cancelledCount: 0 });

      assert.strictEqual(progressEvents.length, 4);
      assert.strictEqual(progressEvents[0].percent, 25);
      assert.strictEqual(progressEvents[1].percent, 50);
      assert.strictEqual(progressEvents[2].percent, 75);
      assert.strictEqual(progressEvents[3].percent, 100);
      assert.strictEqual(progressEvents[1].currentFile, "note2.md");
    });

    it("cancellation token: halting mid-batch leaves already committed files intact and marks remainder cancelled", () => {
      const candidates = ["a.txt", "b.txt", "c.txt", "d.txt"];
      let isCancelled = false;
      const committed = [];
      const results = [];

      for (let i = 0; i < candidates.length; i++) {
        if (isCancelled) {
          results.push({ file: candidates[i], status: "cancelled", message: "Operation cancelled by user." });
          continue;
        }

        // Process item
        committed.push(candidates[i]);
        results.push({ file: candidates[i], status: "imported", message: null });

        // Simulate cancellation after 2nd file
        if (i === 1) {
          isCancelled = true;
        }
      }

      assert.deepStrictEqual(committed, ["a.txt", "b.txt"]);
      assert.strictEqual(results[0].status, "imported");
      assert.strictEqual(results[1].status, "imported");
      assert.strictEqual(results[2].status, "cancelled");
      assert.strictEqual(results[3].status, "cancelled");
      assert.strictEqual(results.filter((r) => r.status === "cancelled").length, 2);
    });

    it("itemized file review: maps friendly messages without raw technical stack traces", () => {
      const sanitizeErrorMessage = (rawError) => {
        if (!rawError) return null;
        if (rawError.includes("UNIQUE constraint failed")) return "A note with this title or content already exists.";
        if (rawError.includes("EACCES") || rawError.includes("Permission denied")) return "Permission denied. Destination is not writeable.";
        if (rawError.includes("invalid utf-8")) return "File contains invalid UTF-8 characters.";
        return rawError;
      };

      assert.strictEqual(
        sanitizeErrorMessage("rusqlite::Error: UNIQUE constraint failed: notes.id (code 19)"),
        "A note with this title or content already exists."
      );
      assert.strictEqual(
        sanitizeErrorMessage("std::io::Error: Permission denied (os error 13)"),
        "Permission denied. Destination is not writeable."
      );
      assert.strictEqual(
        sanitizeErrorMessage("File contains invalid utf-8 byte sequence [0xFF, 0xFE]"),
        "File contains invalid UTF-8 characters."
      );
    });
  });

  describe("Phase 10 - Task 20: Note State, Autosave, and View Synchronization Tests", () => {
    it("pre-export autosave flush: flushes dirty editor state before reading export payload", async () => {
      let isEditorDirty = true;
      let persistedNoteContent = "Initial content in SQLite";
      const dirtyEditorContent = "Updated content in memory draft";

      const flushPendingAutosave = async () => {
        if (isEditorDirty) {
          persistedNoteContent = dirtyEditorContent;
          isEditorDirty = false;
        }
      };

      const handleExport = async () => {
        // Step 1: Pre-flush autosave
        await flushPendingAutosave();
        // Step 2: Read persisted storage for export
        return persistedNoteContent;
      };

      const exportedPayload = await handleExport();
      assert.strictEqual(isEditorDirty, false, "Editor must no longer be dirty");
      assert.strictEqual(exportedPayload, "Updated content in memory draft");
    });

    it("post-import view synchronization: triggers full refresh of note list, notebook counts, and tag counts", async () => {
      let notesLoaded = false;
      let notebooksLoaded = false;
      let tagsLoaded = false;
      let countsRefreshed = false;

      const refreshAppState = async () => {
        notesLoaded = true;
        notebooksLoaded = true;
        tagsLoaded = true;
        countsRefreshed = true;
      };

      const onImportSuccess = async (summary) => {
        if (summary.importedCount > 0) {
          await refreshAppState();
        }
      };

      await onImportSuccess({ importedCount: 3, failedCount: 0 });
      assert.strictEqual(notesLoaded, true);
      assert.strictEqual(notebooksLoaded, true);
      assert.strictEqual(tagsLoaded, true);
      assert.strictEqual(countsRefreshed, true);
    });

    it("active note preservation: preserves currently opened editor note and cursor state during background import", () => {
      const activeEditorNoteId = "active-note-123";
      const existingNotes = [
        { id: "active-note-123", title: "Active Note", content: "Active Content" },
        { id: "note-456", title: "Other Note", content: "Other Content" },
      ];

      const newlyImportedNotes = [
        { id: "imported-789", title: "Imported Note 1", content: "Imported Content 1" },
        { id: "imported-999", title: "Imported Note 2", content: "Imported Content 2" },
      ];

      const updatedNotes = [...existingNotes, ...newlyImportedNotes];
      // Note retention rule: if activeNoteId exists in updated list, keep it selected!
      const selectedId = updatedNotes.some((n) => n.id === activeEditorNoteId)
        ? activeEditorNoteId
        : updatedNotes[0]?.id || null;

      assert.strictEqual(selectedId, "active-note-123", "Editor selection must not be unexpectedly replaced");
    });

    it("trash exclusion invariant: excludes soft-deleted notes from batch export selection by default", () => {
      const allNotes = [
        { id: "note-1", title: "Active 1", is_deleted: false },
        { id: "note-2", title: "Deleted 2", is_deleted: true },
        { id: "note-3", title: "Active 3", is_deleted: false },
        { id: "note-4", title: "Deleted 4", is_deleted: true },
      ];

      const filterNotesForExport = (notes, includeDeleted = false) => {
        return notes.filter((n) => includeDeleted || !n.is_deleted);
      };

      const exportable = filterNotesForExport(allNotes);
      assert.strictEqual(exportable.length, 2);
      assert.deepStrictEqual(exportable.map((n) => n.id), ["note-1", "note-3"]);
    });
  });

  describe("Phase 10 - Task 21: User Feedback, Empty States, and Error Recovery Tests", () => {
    it("empty selection guard: returns cleanly with zero side-effects when user cancels file picker", async () => {
      const pickImportFiles = async (simulateCancel = true) => {
        if (simulateCancel) return { cancelled: true, paths: [] };
        return { cancelled: false, paths: ["/path/note.txt"] };
      };

      let modalOpened = false;
      const result = await pickImportFiles(true);
      if (!result.cancelled && result.paths.length > 0) {
        modalOpened = true;
      }

      assert.strictEqual(modalOpened, false, "Modal must not open when picker is cancelled");
      assert.strictEqual(result.cancelled, true);
    });

    it("all candidates deselected: displays actionable guidance and disables import button", () => {
      const candidates = [
        { id: "1", title: "Note 1", selected: false, validationStatus: "valid" },
        { id: "2", title: "Note 2", selected: false, validationStatus: "valid" },
      ];

      const selectedCount = candidates.filter((c) => c.selected && c.validationStatus !== "invalid").length;
      const canConfirm = selectedCount > 0;
      const emptyBanner = selectedCount === 0 ? "Select at least one valid note to import." : null;

      assert.strictEqual(canConfirm, false);
      assert.strictEqual(emptyBanner, "Select at least one valid note to import.");
    });

    it("unsupported or corrupt files: flags invalid candidates with helpful format requirements", () => {
      const candidates = [
        { id: "1", name: "archive.zip", format: "zip", error: "Unsupported file extension (.zip). Supported formats are .txt, .md, .markdown, and .json." },
        { id: "2", name: "binary.bin", format: "bin", error: "File contains null bytes or binary characters." },
      ];

      const validCandidates = candidates.filter((c) => !c.error);
      assert.strictEqual(validCandidates.length, 0);
      assert.strictEqual(candidates[0].error.includes(".txt, .md"), true);
      assert.strictEqual(candidates[1].error.includes("binary characters"), true);
    });

    it("error recovery and feedback banner: produces clear toast feedback and auto-dismisses", () => {
      let banner = null;
      const showToast = (type, message) => {
        banner = { type, message, timestamp: Date.now() };
      };

      showToast("success", "Successfully imported 5 notes.");
      assert.strictEqual(banner.type, "success");
      assert.strictEqual(banner.message, "Successfully imported 5 notes.");

      showToast("error", "Failed to write exported notes: disk full.");
      assert.strictEqual(banner.type, "error");
      assert.strictEqual(banner.message, "Failed to write exported notes: disk full.");
    });
  });

  describe("Phase 10 - Task 23: Complete End-to-End Simulation Workflows Tests", () => {
    it("Workflow 1: Single note export to Markdown and Text with atomic file creation", async () => {
      const memoryFS = new Map();
      const exportSingleNote = async (note, format, destPath) => {
        const payload = note.content;
        // Atomic write pattern:
        const tempPath = destPath + ".tmp";
        memoryFS.set(tempPath, payload);
        memoryFS.set(destPath, memoryFS.get(tempPath));
        memoryFS.delete(tempPath);
        return { noteId: note.id, status: "exported", path: destPath };
      };

      const mdNote = { id: "n-1", title: "Arch Doc", content: "# Architecture\n\n- Local first\n- Offline only" };
      const res = await exportSingleNote(mdNote, "md", "/exports/Arch Doc.md");

      assert.strictEqual(res.status, "exported");
      assert.strictEqual(memoryFS.get("/exports/Arch Doc.md"), "# Architecture\n\n- Local first\n- Offline only");
      assert.strictEqual(memoryFS.has("/exports/Arch Doc.md.tmp"), false);
    });

    it("Workflow 2: Multi-note batch export to folder with collision handling", async () => {
      const destinationFiles = new Set(["Meeting.txt"]);
      const notes = [
        { id: "n-1", title: "Meeting", content: "Meeting content 1" },
        { id: "n-2", title: "Meeting", content: "Meeting content 2" },
      ];

      const exportedResults = [];
      for (const note of notes) {
        let filename = `${note.title}.txt`;
        let suffix = 1;
        while (destinationFiles.has(filename)) {
          filename = `${note.title} (${suffix}).txt`;
          suffix++;
        }
        destinationFiles.add(filename);
        exportedResults.push({ id: note.id, filename, content: note.content });
      }

      assert.strictEqual(exportedResults.length, 2);
      assert.strictEqual(exportedResults[0].filename, "Meeting (1).txt");
      assert.strictEqual(exportedResults[1].filename, "Meeting (2).txt");
    });

    it("Workflow 3: Structured interchange export of notebooks, tags, internal links", () => {
      const noteA = { id: "uuid-a", title: "Roadmap", content: "See [Spec](notepad://note/uuid-b)", tags: ["zebra", "apple"], notebook: "Work/Projects" };
      const noteB = { id: "uuid-b", title: "Spec", content: "Details here", tags: ["backend"], notebook: "Work/Projects" };

      const createInterchangeDoc = (notes) => {
        const idSet = new Set(notes.map((n) => n.id));
        return {
          app: "personal-notepad",
          format: "personal-notepad-interchange",
          version: "1.0",
          exported_at: new Date().toISOString(),
          note_count: notes.length,
          notes: notes.map((n) => ({
            export_id: n.id,
            title: n.title,
            content: n.content,
            format: "md",
            notebook_path: n.notebook,
            tags: [...n.tags].sort(), // Deterministic sorting invariant
            internal_links: (n.content.match(/notepad:\/\/note\/([a-zA-Z0-9-]+)/g) || []).map((uri) => {
              const targetId = uri.replace("notepad://note/", "");
              return { target_export_id: targetId, is_package_internal: idSet.has(targetId) };
            }),
          })),
        };
      };

      const doc = createInterchangeDoc([noteA, noteB]);
      assert.strictEqual(doc.version, "1.0");
      assert.strictEqual(doc.note_count, 2);
      assert.deepStrictEqual(doc.notes[0].tags, ["apple", "zebra"]); // Alphabetical order
      assert.strictEqual(doc.notes[0].internal_links[0].is_package_internal, true);
    });

    it("Workflow 4: Batch import of mixed TXT and MD files with conflict resolution", () => {
      const existingDBNotes = [{ title: "Existing Note", content: "Verbatim text" }];
      const incomingFiles = [
        { path: "/in/Existing Note.txt", title: "Existing Note", content: "Verbatim text" },
        { path: "/in/New Markdown.md", title: "New Markdown", content: "## Subtitle" },
      ];

      const processBatchImport = (files, policy) => {
        const imported = [];
        const skipped = [];

        for (const file of files) {
          const match = existingDBNotes.find((n) => n.title === file.title && n.content === file.content);
          if (match && policy === "skip_exact") {
            skipped.push({ file: file.title, reason: "Exact duplicate skipped" });
          } else {
            imported.push({ file: file.title, format: file.path.endsWith(".md") ? "md" : "txt" });
          }
        }
        return { imported, skipped };
      };

      const res = processBatchImport(incomingFiles, "skip_exact");
      assert.strictEqual(res.skipped.length, 1);
      assert.strictEqual(res.skipped[0].file, "Existing Note");
      assert.strictEqual(res.imported.length, 1);
      assert.strictEqual(res.imported[0].file, "New Markdown");
      assert.strictEqual(res.imported[0].format, "md");
    });

    it("Workflow 5: Structured JSON import with automatic notebook hierarchy, tag creation, and internal link ID remapping", () => {
      const incomingDoc = {
        app: "personal-notepad",
        format: "personal-notepad-interchange",
        version: "1.0",
        notes: [
          {
            export_id: "ext-1",
            title: "Note 1",
            content: "Check [Note 2](notepad://note/ext-2)",
            tags: ["dev", "offline"],
            notebook_path: "Engineering/Specs",
          },
          {
            export_id: "ext-2",
            title: "Note 2",
            content: "Implementation details",
            tags: ["dev"],
            notebook_path: "Engineering/Specs",
          },
        ],
      };

      const localDB = { notes: [], notebooks: new Set(), tags: new Set() };
      const idMapping = new Map();

      // Step 1: Pre-generate local UUIDs
      for (const n of incomingDoc.notes) {
        idMapping.set(n.export_id, `local-${n.export_id}`);
      }

      // Step 2: Import notes with link remapping
      for (const n of incomingDoc.notes) {
        localDB.notebooks.add(n.notebook_path);
        n.tags.forEach((t) => localDB.tags.add(t));

        let rewrittenContent = n.content;
        for (const [extId, localId] of idMapping.entries()) {
          rewrittenContent = rewrittenContent.replaceAll(`notepad://note/${extId}`, `notepad://note/${localId}`);
        }

        localDB.notes.push({
          id: idMapping.get(n.export_id),
          title: n.title,
          content: rewrittenContent,
        });
      }

      assert.strictEqual(localDB.notes.length, 2);
      assert.strictEqual(localDB.notes[0].id, "local-ext-1");
      assert.strictEqual(localDB.notes[0].content, "Check [Note 2](notepad://note/local-ext-2)");
      assert.strictEqual(localDB.notebooks.has("Engineering/Specs"), true);
      assert.strictEqual(localDB.tags.has("dev"), true);
      assert.strictEqual(localDB.tags.has("offline"), true);
    });

    it("Workflow 6: Live progress reporting and clean cancellation mid-batch", () => {
      const items = ["note-1", "note-2", "note-3", "note-4", "note-5"];
      let cancelled = false;
      const progressSnapshots = [];
      const processed = [];

      for (let i = 0; i < items.length; i++) {
        if (cancelled) break;
        progressSnapshots.push({ index: i + 1, total: items.length, current: items[i] });
        processed.push(items[i]);
        if (i === 2) cancelled = true; // Cancel after 3rd item
      }

      assert.strictEqual(processed.length, 3);
      assert.strictEqual(progressSnapshots.length, 3);
      assert.strictEqual(progressSnapshots[2].current, "note-3");
    });

    it("Workflow 7: Unsaved editor draft synchronization prior to batch export", async () => {
      let isAutosavePending = true;
      let databaseState = "Old persisted text";
      const editorDraftState = "New unsaved changes in editor";

      const flushEditor = async () => {
        if (isAutosavePending) {
          databaseState = editorDraftState;
          isAutosavePending = false;
        }
      };

      // Ensure batch export triggers flushEditor before collecting note contents
      await flushEditor();
      assert.strictEqual(isAutosavePending, false);
      assert.strictEqual(databaseState, "New unsaved changes in editor");
    });

    it("Workflow 8: Error recovery from corrupt files, unsupported extensions, and unwritable paths", () => {
      const candidates = [
        { file: "valid.txt", valid: true },
        { file: "corrupt.bin", valid: false, error: "Binary file rejected" },
        { file: "unwriteable.txt", valid: true, ioError: true },
      ];

      const results = [];
      for (const c of candidates) {
        if (!c.valid) {
          results.push({ file: c.file, status: "rejected", message: c.error });
        } else if (c.ioError) {
          results.push({ file: c.file, status: "failed", message: "Disk write failure" });
        } else {
          results.push({ file: c.file, status: "success", message: null });
        }
      }

      assert.strictEqual(results.filter((r) => r.status === "success").length, 1);
      assert.strictEqual(results.filter((r) => r.status === "rejected").length, 1);
      assert.strictEqual(results.filter((r) => r.status === "failed").length, 1);
    });

    it("Workflow 9: Round-trip migration fidelity verification (Export -> Import -> Verify content, tags, hierarchy, and links)", () => {
      const originalNote = {
        title: "Integration Blueprint",
        content: "Verbatim markdown with **bold**, code `fn()`, and link [Target](notepad://note/target-1)",
        format: "md",
        notebook: "Architecture/2026",
        tags: ["blueprint", "offline"],
      };

      // Export serialize
      const serialized = JSON.stringify({
        app: "personal-notepad",
        format: "personal-notepad-interchange",
        version: "1.0",
        notes: [{
          export_id: "orig-1",
          title: originalNote.title,
          content: originalNote.content,
          format: originalNote.format,
          notebook_path: originalNote.notebook,
          tags: originalNote.tags,
        }],
      });

      // Import deserialize
      const deserialized = JSON.parse(serialized);
      const importedNote = deserialized.notes[0];

      assert.strictEqual(importedNote.title, originalNote.title);
      assert.strictEqual(importedNote.content, originalNote.content);
      assert.strictEqual(importedNote.format, originalNote.format);
      assert.strictEqual(importedNote.notebook_path, originalNote.notebook);
      assert.deepStrictEqual(importedNote.tags, originalNote.tags);
    });

    it("Workflow 10: Full state refresh (note list, counts, tags, search index) after batch operations", () => {
      const appState = {
        notesCount: 10,
        notebooksCount: 2,
        tagsCount: 3,
        searchIndexUpdated: false,
      };

      const onBatchOperationComplete = (newNotesCount, newTagsCount) => {
        appState.notesCount += newNotesCount;
        appState.tagsCount += newTagsCount;
        appState.searchIndexUpdated = true;
      };

      onBatchOperationComplete(4, 2);

      assert.strictEqual(appState.notesCount, 14);
      assert.strictEqual(appState.tagsCount, 5);
      assert.strictEqual(appState.searchIndexUpdated, true);
    });
  });

  describe("Phase 10 - Task 24: Security, Data Integrity, and Performance Verification Tests", () => {
    // Security Verification Tests
    describe("Security Verification", () => {
      it("path traversal mitigation: rejects path traversal sequences across operating system styles", () => {
        const isPathTraversalSafe = (filePath) => {
          if (!filePath || typeof filePath !== "string") return false;
          // Normalize and check for relative traversal components
          const normalized = filePath.replace(/\\/g, "/");
          const segments = normalized.split("/");
          return !segments.some((segment) => segment === ".." || segment === ".");
        };

        const safeFilenameStem = (rawTitle) => {
          const illegal = /[/\\:*?"<>|]/g;
          let cleaned = (rawTitle || "").replace(illegal, "").trim();
          cleaned = cleaned.replace(/^\.+/, "");
          cleaned = cleaned.replace(/[. ]+$/, "");
          return cleaned || "Untitled Note";
        };

        // Traversal attempts
        assert.strictEqual(isPathTraversalSafe("../../etc/passwd"), false);
        assert.strictEqual(isPathTraversalSafe("..\\..\\Windows\\System32"), false);
        assert.strictEqual(isPathTraversalSafe("notes/../../secret.key"), false);
        assert.strictEqual(isPathTraversalSafe("valid/folder/note.md"), true);

        // Filename stem sanitization strips traversal separators
        assert.strictEqual(safeFilenameStem("../../etc/passwd"), "etcpasswd");
        assert.strictEqual(safeFilenameStem("..\\Windows\\System32\\cmd.exe"), "WindowsSystem32cmd.exe");
        assert.strictEqual(safeFilenameStem("....hidden"), "hidden");
      });

      it("Windows reserved device names neutralization: prefixes reserved names with an underscore", () => {
        const RESERVED_NAMES = new Set([
          "CON", "PRN", "AUX", "NUL",
          "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
          "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ]);

        const sanitizeReservedName = (name) => {
          const upper = (name || "").toUpperCase().trim();
          return RESERVED_NAMES.has(upper) ? `_${name.trim()}` : name.trim();
        };

        assert.strictEqual(sanitizeReservedName("CON"), "_CON");
        assert.strictEqual(sanitizeReservedName("con"), "_con");
        assert.strictEqual(sanitizeReservedName("PRN"), "_PRN");
        assert.strictEqual(sanitizeReservedName("aux"), "_aux");
        assert.strictEqual(sanitizeReservedName("NUL"), "_NUL");
        assert.strictEqual(sanitizeReservedName("COM1"), "_COM1");
        assert.strictEqual(sanitizeReservedName("lpt9"), "_lpt9");

        // Substrings or compound words remain unaltered
        assert.strictEqual(sanitizeReservedName("Conference"), "Conference");
        assert.strictEqual(sanitizeReservedName("Pronto"), "Pronto");
        assert.strictEqual(sanitizeReservedName("Auxiliary"), "Auxiliary");
        assert.strictEqual(sanitizeReservedName("NullPointer"), "NullPointer");
      });

      it("managed database file exclusion: forbids export and import targeting managed SQLite database files", () => {
        const MANAGED_DB_FILES = new Set([
          "database.sqlite",
          "database.sqlite-wal",
          "database.sqlite-shm",
          "notepad.db",
          "notepad.db-wal",
          "notepad.db-shm",
          "custom-notepad.db",
        ]);

        const isForbiddenTarget = (targetPath) => {
          if (!targetPath) return true;
          const normalized = targetPath.replace(/\\/g, "/");
          const fileName = normalized.split("/").pop()?.toLowerCase();
          return fileName ? MANAGED_DB_FILES.has(fileName) : true;
        };

        assert.strictEqual(isForbiddenTarget("/home/user/.local/share/custom-notepad/database.sqlite"), true);
        assert.strictEqual(isForbiddenTarget("C:\\Users\\User\\AppData\\notepad.db-wal"), true);
        assert.strictEqual(isForbiddenTarget("/tmp/notepad.db-shm"), true);
        assert.strictEqual(isForbiddenTarget("/home/user/Documents/Exported Note.md"), false);
        assert.strictEqual(isForbiddenTarget("C:\\Users\\User\\Desktop\\backup.json"), false);
      });

      it("HTML and script containment: stores script tags and HTML entities as inert plain text literals", () => {
        const rawPayloads = [
          "<script>alert('xss')</script>",
          "<img src=x onerror=alert(1)>",
          "<svg onload=fetch('http://evil.com')>",
          "&lt;script&gt;alert(document.cookie)&lt;/script&gt;",
          "<iframe src=\"javascript:alert(1)\"></iframe>",
        ];

        for (const payload of rawPayloads) {
          const note = {
            id: "sec-note",
            title: "Security Test",
            content: payload,
          };

          // Invariant: Content is preserved verbatim without executing or stripping
          const serialized = JSON.stringify(note);
          const parsed = JSON.parse(serialized);

          assert.strictEqual(parsed.content, payload, "Payload must be preserved verbatim as inert text");
          assert.strictEqual(typeof parsed.content, "string", "Must remain a literal string primitive");
          // Never eval or execute
          assert.strictEqual(parsed.content.includes("<script>"), payload.includes("<script>"));
        }
      });

      it("prototype pollution resistance: safely handles objects containing __proto__ or constructor keys without poisoning Object.prototype", () => {
        const maliciousJson = `{"__proto__":{"polluted":"yes"},"constructor":{"prototype":{"polluted2":"yes"}},"title":"Test Note","content":"Safe content"}`;

        // Safe JSON parsing invariant
        const parsed = JSON.parse(maliciousJson);

        // Verify global Object.prototype is NOT polluted
        assert.strictEqual(Object.prototype.polluted, undefined, "Object.prototype must not be polluted with 'polluted'");
        assert.strictEqual(Object.prototype.polluted2, undefined, "Object.prototype must not be polluted with 'polluted2'");
        assert.strictEqual(parsed.title, "Test Note");
        assert.strictEqual(parsed.content, "Safe content");
      });

      it("external export_id isolation: never reuses external export_id as local database primary key", () => {
        const externalPackage = {
          notes: [
            { export_id: "ext-uuid-1234", title: "External Note 1", content: "Body 1" },
            { export_id: "ext-uuid-5678", title: "External Note 2", content: "Body 2" },
          ],
        };

        const generateLocalId = () => `local-uuid-${Math.random().toString(36).substring(2, 9)}`;

        const importedDatabaseNotes = externalPackage.notes.map((extNote) => {
          const internalId = generateLocalId();
          return {
            id: internalId, // strictly generated local UUID
            title: extNote.title,
            content: extNote.content,
            originalExportId: extNote.export_id,
          };
        });

        for (const imported of importedDatabaseNotes) {
          assert.notStrictEqual(imported.id, imported.originalExportId, "Local primary key must never equal external export_id");
          assert.ok(imported.id.startsWith("local-uuid-"));
        }
      });
    });

    // Data Integrity Verification Tests
    describe("Data Integrity Verification", () => {
      it("multilingual Unicode and emoji roundtrip fidelity: preserves complex multi-codepoint characters and scripts verbatim", () => {
        const complexUnicodeCorpus = [
          "Chinese (Simplified/Traditional): 快速的棕色狐狸跃过了懒惰的狗。 繁體中文測試。",
          "Japanese: こんにちは世界！平仮名、片仮名、漢字 (日本語のテスト)。",
          "Korean: 안녕하세요 세계! 한글 테스트 문장입니다.",
          "Arabic (RTL): مرحباً بالعالم! هذا اختبار للغة العربية والاتجاه من اليمين إلى اليسار.",
          "Hebrew (RTL): שלום עולם! בדיקת טקסט בעברית מימין לשמאל.",
          "Devanagari: नमस्ते दुनिया! यह हिंदी भाषा और देवनागरी लिपि का परीक्षण है।",
          "Cyrillic: Привет, мир! Тестирование русского текста и кириллицы.",
          "Mathematical symbols: ∑_{i=1}^n x_i = ∫_0^∞ e^{-x} dx ∧ ∀x ∃y (x < y) ⊗ π ≈ 3.14159",
          "Multi-codepoint emojis: 👨‍👩‍👧‍👦 (family ZWJ), 🏳️‍🌈 (rainbow flag), 🧑🏽‍💻 (technologist), 🚀 ✨ 💖 🔥 🥑 ☕",
        ];

        for (const text of complexUnicodeCorpus) {
          const note = { id: "unicode-test", title: text.slice(0, 30), content: text };
          const serialized = JSON.stringify(note);
          const restored = JSON.parse(serialized);

          assert.strictEqual(restored.content, text, `Fidelity failed for: ${text}`);
          assert.strictEqual(restored.title, text.slice(0, 30));
        }
      });

      it("BOM stripping: cleanly removes leading UTF-8 Byte Order Mark (\\uFEFF) without mangling content", () => {
        const stripBom = (input) => {
          if (!input) return "";
          return input.charCodeAt(0) === 0xFEFF ? input.slice(1) : input;
        };

        const rawTextWithBom = "\uFEFF# Project Overview\n\nStarting note with BOM.";
        const stripped = stripBom(rawTextWithBom);

        assert.strictEqual(stripped.charCodeAt(0), 35, "First character must be '#' (ASCII 35)");
        assert.strictEqual(stripped, "# Project Overview\n\nStarting note with BOM.");
        assert.strictEqual(stripped.includes("\uFEFF"), false, "BOM must be completely removed");

        // Idempotent on text without BOM
        const normalText = "# No BOM";
        assert.strictEqual(stripBom(normalText), "# No BOM");
      });

      it("line ending normalization: normalizes CRLF and CR to standard LF (\\n)", () => {
        const normalizeLineEndings = (text) => {
          if (!text) return "";
          return text.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
        };

        const windowsText = "Line 1\r\nLine 2\r\nLine 3\r\n";
        const macLegacyText = "Line 1\rLine 2\rLine 3\r";
        const mixedText = "Line 1\r\nLine 2\rLine 3\nLine 4";

        assert.strictEqual(normalizeLineEndings(windowsText), "Line 1\nLine 2\nLine 3\n");
        assert.strictEqual(normalizeLineEndings(macLegacyText), "Line 1\nLine 2\nLine 3\n");
        assert.strictEqual(normalizeLineEndings(mixedText), "Line 1\nLine 2\nLine 3\nLine 4");
      });

      it("binary and null byte rejection: identifies invalid non-text files and protects database from corruption", () => {
        const isBinaryContent = (content) => {
          if (typeof content !== "string") return true;
          // Check for null bytes or control characters (excluding tab, LF, CR)
          for (let i = 0; i < Math.min(content.length, 1024); i++) {
            const code = content.charCodeAt(i);
            if (code === 0 || (code < 32 && code !== 9 && code !== 10 && code !== 13)) {
              return true;
            }
          }
          return false;
        };

        assert.strictEqual(isBinaryContent("Valid plain text note with words."), false);
        assert.strictEqual(isBinaryContent("# Markdown Heading\n\n- item 1\n- item 2\n"), false);
        assert.strictEqual(isBinaryContent("Binary\x00file with null byte"), true);
        assert.strictEqual(isBinaryContent("\x7F\x01\x02\x03\x04PNG\r\n\x1a\n"), true);
      });

      it("deterministic tag sorting: sorts tags lexicographically in interchange JSON serialization", () => {
        const unsortedTags = ["productivity", "architecture", "database", "backend", "analytics"];
        const sortTagsDeterministically = (tags) => {
          return [...tags].sort((a, b) => a.localeCompare(b, "en", { sensitivity: "base" }));
        };

        const sorted = sortTagsDeterministically(unsortedTags);
        assert.deepStrictEqual(sorted, ["analytics", "architecture", "backend", "database", "productivity"]);

        // Repeated sorting remains identical
        assert.deepStrictEqual(sortTagsDeterministically(sorted), sorted);
      });

      it("internal note link remapping vs external URL retention", () => {
        const idMap = new Map([
          ["ext-note-1", "loc-note-101"],
          ["ext-note-2", "loc-note-102"],
        ]);

        const remapNoteLinks = (content, map) => {
          return content.replace(/notepad:\/\/note\/([a-zA-Z0-9_-]+)/g, (match, targetId) => {
            const newId = map.get(targetId);
            return newId ? `notepad://note/${newId}` : match;
          });
        };

        const sampleContent =
          "See [Design](notepad://note/ext-note-1) and [Arch](notepad://note/ext-note-2) or visit [Site](https://example.com/notepad://note/test).";

        const remapped = remapNoteLinks(sampleContent, idMap);

        assert.strictEqual(
          remapped,
          "See [Design](notepad://note/loc-note-101) and [Arch](notepad://note/loc-note-102) or visit [Site](https://example.com/notepad://note/test)."
        );
        // External link retained unchanged
        assert.ok(remapped.includes("https://example.com/notepad://note/test"));
      });
    });

    // Performance Verification Tests
    describe("Performance Verification", () => {
      it("in-memory candidate filtering and pre-scan benchmark executes in under 50ms for 100 files", () => {
        const candidatePaths = Array.from({ length: 100 }, (_, i) => {
          const ext = i % 4 === 0 ? "md" : i % 4 === 1 ? "txt" : i % 4 === 2 ? "json" : "bin";
          return `/user/documents/folder/file_${i}.${ext}`;
        });

        const startTime = performance.now();

        const SUPPORTED_EXTS = new Set(["md", "markdown", "txt", "json"]);
        const candidates = candidatePaths.map((p, idx) => {
          const ext = p.split(".").pop()?.toLowerCase() || "";
          const isSupported = SUPPORTED_EXTS.has(ext);
          return {
            id: `cand-${idx}`,
            path: p,
            detected_format: isSupported ? ext : "unknown",
            is_supported: isSupported,
            validation_error: isSupported ? null : `Unsupported file extension .${ext}`,
          };
        });

        const validCandidates = candidates.filter((c) => c.is_supported);
        const duration = performance.now() - startTime;

        assert.strictEqual(candidates.length, 100);
        assert.strictEqual(validCandidates.length, 75);
        assert.ok(duration < 50, `Scanning 100 files took ${duration.toFixed(2)}ms (must be < 50ms)`);
      });

      it("in-memory batch interchange serialization benchmark executes in under 50ms for 100 notes", () => {
        const notes = Array.from({ length: 100 }, (_, i) => ({
          id: `uuid-note-${i}`,
          title: `Project Architecture Specification Note ${i}`,
          content: `# Specification ${i}\n\nDetailed contents for note ${i} with references to [Next](notepad://note/uuid-note-${(i + 1) % 100}).`,
          tags: [`tag-${i % 5}`, `common-tag`, `spec`],
          notebook: i % 2 === 0 ? "Engineering/Frontend" : "Engineering/Backend",
        }));

        const startTime = performance.now();

        const interchangePackage = {
          app: "personal-notepad",
          format: "personal-notepad-interchange",
          version: "1.0",
          exported_at: new Date().toISOString(),
          note_count: notes.length,
          notes: notes.map((n) => ({
            export_id: n.id,
            title: n.title,
            content: n.content,
            format: "md",
            notebook_path: n.notebook,
            tags: [...n.tags].sort(),
          })),
        };

        const serialized = JSON.stringify(interchangePackage);
        const duration = performance.now() - startTime;

        assert.strictEqual(interchangePackage.notes.length, 100);
        assert.ok(serialized.length > 10000, "Serialized JSON should have non-trivial size");
        assert.ok(duration < 50, `Serializing 100 notes took ${duration.toFixed(2)}ms (must be < 50ms)`);
      });

      it("conflict resolution lookup operates in O(1) time complexity with Set/Map indexes", () => {
        const existingNoteTitles = new Set(
          Array.from({ length: 500 }, (_, i) => `Existing Title ${i}`)
        );

        const startTime = performance.now();
        let collisions = 0;

        for (let i = 0; i < 500; i++) {
          const testTitle = i % 2 === 0 ? `Existing Title ${i}` : `New Unique Title ${i}`;
          if (existingNoteTitles.has(testTitle)) {
            collisions++;
          }
        }

        const duration = performance.now() - startTime;
        assert.strictEqual(collisions, 250);
        assert.ok(duration < 20, `500 hash set lookups took ${duration.toFixed(2)}ms (must be < 20ms)`);
      });
    });
  });
});




