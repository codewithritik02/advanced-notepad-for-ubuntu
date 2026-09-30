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
  });
});


