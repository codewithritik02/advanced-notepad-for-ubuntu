mod common;

use common::TestDatabase;
use personal_notepad_lib::storage::{
    errors::StorageError,
    limits::MAX_TAG_NAME_CHARS,
    models::{CreateNoteDto, UpdateNoteDto, UpdateTagDto},
    repositories::{NoteRepository, NotebookRepository, TagRepository},
};

/// 1. Tag creation, whitespace trimming, Unicode support, length validation, and renaming.
#[test]
fn test_tag_creation_validation_and_renaming() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    // 1a. Normal creation with leading/trailing whitespace trimming
    let tag =
        TagRepository::create(conn, "  rust-development  ").expect("create tag with whitespace");
    assert_eq!(tag.name, "rust-development");
    assert!(!tag.id.is_empty());
    assert!(!tag.created_at.is_empty());

    // 1b. Unicode and special character tag creation
    let unicode_tag = TagRepository::create(conn, "यात्रा 📁 & R&D").expect("create unicode tag");
    assert_eq!(unicode_tag.name, "यात्रा 📁 & R&D");

    // 1c. Empty or whitespace-only name fails validation
    let empty_err = TagRepository::create(conn, "   ").unwrap_err();
    match empty_err {
        StorageError::Validation(msg) => assert!(msg.to_lowercase().contains("tag name")),
        other => panic!("expected Validation error for empty tag name, got {other:?}"),
    }

    // 1d. Name exceeding MAX_TAG_NAME_CHARS (100) fails validation
    let valid_long_name = "a".repeat(MAX_TAG_NAME_CHARS);
    let long_tag = TagRepository::create(conn, &valid_long_name).expect("tag at exact max length");
    assert_eq!(long_tag.name.len(), MAX_TAG_NAME_CHARS);

    let too_long_name = "a".repeat(MAX_TAG_NAME_CHARS + 1);
    let too_long_err = TagRepository::create(conn, &too_long_name).unwrap_err();
    match too_long_err {
        StorageError::Validation(msg) => assert!(msg.to_lowercase().contains("exceed")),
        other => panic!("expected Validation error for over-length tag name, got {other:?}"),
    }

    // 1e. Successful tag renaming
    let renamed = TagRepository::update(
        conn,
        &tag.id,
        UpdateTagDto {
            name: "systems-programming".to_string(),
        },
    )
    .expect("rename tag");
    assert_eq!(renamed.id, tag.id);
    assert_eq!(renamed.name, "systems-programming");
    assert_eq!(renamed.created_at, tag.created_at);

    // Verify lookup by new name works and old name no longer matches
    assert!(TagRepository::get_by_name(conn, "rust-development")
        .unwrap()
        .is_none());
    assert!(TagRepository::get_by_name(conn, "systems-programming")
        .unwrap()
        .is_some());

    // 1f. Rename to existing name causes Conflict error
    let tag2 = TagRepository::create(conn, "embedded").expect("create second tag");
    let conflict_err = TagRepository::update(
        conn,
        &tag2.id,
        UpdateTagDto {
            name: "Systems-Programming".to_string(), // case-insensitive conflict with renamed tag
        },
    )
    .unwrap_err();

    match conflict_err {
        StorageError::Conflict(msg) => assert!(msg.contains("already exists")),
        other => panic!("expected Conflict error on rename collision, got {other:?}"),
    }

    // 1g. Renaming tag to same name (or same name different case) for itself succeeds
    let self_rename = TagRepository::update(
        conn,
        &renamed.id,
        UpdateTagDto {
            name: "SYSTEMS-PROGRAMMING".to_string(),
        },
    )
    .expect("case update on same tag");
    assert_eq!(self_rename.name, "SYSTEMS-PROGRAMMING");
}

/// 2. Tag uniqueness and case-insensitivity policy.
#[test]
fn test_tag_uniqueness_and_case_insensitivity() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    // Create initial tag
    let original = TagRepository::create(conn, "Productivity").expect("create tag");
    assert_eq!(original.name, "Productivity");

    // Creating same tag with different casing returns existing tag idempotently
    let lower = TagRepository::create(conn, "productivity").expect("idempotent lowercase");
    assert_eq!(lower.id, original.id);
    assert_eq!(lower.name, original.name);

    let upper = TagRepository::create(conn, "PRODUCTIVITY").expect("idempotent uppercase");
    assert_eq!(upper.id, original.id);

    // Verify database only contains 1 tag record
    let all_tags = TagRepository::list(conn).expect("list tags");
    assert_eq!(all_tags.len(), 1);
    assert_eq!(all_tags[0].id, original.id);
}

/// 3. Many-to-many relationship: multiple tags on one note, and one tag on multiple notes.
#[test]
fn test_multi_tag_to_single_note_and_single_tag_to_multi_notes() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    // Create 3 tags
    let t_alpha = TagRepository::create(conn, "alpha").unwrap();
    let t_beta = TagRepository::create(conn, "beta").unwrap();
    let t_gamma = TagRepository::create(conn, "gamma").unwrap();

    // Create 3 notes
    let n1 = test_db.create_note("Note 1", "Content 1", None);
    let n2 = test_db.create_note("Note 2", "Content 2", None);
    let n3 = test_db.create_note("Note 3", "Content 3", None);

    // 3a. Assign multiple tags to Note 1
    TagRepository::add_tag_to_note(conn, &n1.id, &t_alpha.id).unwrap();
    TagRepository::add_tag_to_note(conn, &n1.id, &t_beta.id).unwrap();
    TagRepository::add_tag_to_note(conn, &n1.id, &t_gamma.id).unwrap();

    // Adding duplicate tag association is safely ignored (idempotent)
    TagRepository::add_tag_to_note(conn, &n1.id, &t_beta.id).unwrap();

    let n1_tags = TagRepository::get_tags_for_note(conn, &n1.id).unwrap();
    assert_eq!(n1_tags.len(), 3);
    assert_eq!(n1_tags[0].name, "alpha");
    assert_eq!(n1_tags[1].name, "beta");
    assert_eq!(n1_tags[2].name, "gamma");

    // 3b. Assign one tag ("alpha") to multiple notes (Note 1, Note 2, Note 3)
    TagRepository::add_tag_to_note(conn, &n2.id, &t_alpha.id).unwrap();
    TagRepository::add_tag_to_note(conn, &n3.id, &t_alpha.id).unwrap();

    // Verify tag note counts
    let counts = TagRepository::get_tag_note_counts(conn).unwrap();
    assert_eq!(counts.get(&t_alpha.id).copied(), Some(3));
    assert_eq!(counts.get(&t_beta.id).copied(), Some(1));
    assert_eq!(counts.get(&t_gamma.id).copied(), Some(1));

    // Verify list_by_tag returns all 3 notes for "alpha"
    let alpha_notes = NoteRepository::list_by_tag(conn, &t_alpha.id).unwrap();
    assert_eq!(alpha_notes.len(), 3);
    let returned_ids: Vec<String> = alpha_notes.into_iter().map(|n| n.id).collect();
    assert!(returned_ids.contains(&n1.id));
    assert!(returned_ids.contains(&n2.id));
    assert!(returned_ids.contains(&n3.id));

    // 3c. Batch lookup: get_tags_for_notes avoids N+1 queries
    let batch_tags =
        TagRepository::get_tags_for_notes(conn, &[n1.id.clone(), n2.id.clone()]).unwrap();
    assert_eq!(
        batch_tags.get(&n1.id).unwrap(),
        &vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()]
    );
    assert_eq!(batch_tags.get(&n2.id).unwrap(), &vec!["alpha".to_string()]);
}

/// 4. Tag removal and atomic replacement via set_tags_for_note.
#[test]
fn test_tag_removal_and_atomic_set_for_note() {
    let mut test_db = TestDatabase::new();
    let conn = test_db.connection_mut();

    let t1 = TagRepository::create(conn, "dev").unwrap();
    let t2 = TagRepository::create(conn, "ops").unwrap();
    let t3 = TagRepository::create(conn, "security").unwrap();
    let t4 = TagRepository::create(conn, "audit").unwrap();

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Server Setup".to_string()),
            content: Some("Run deploy".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // Add T1, T2, T3
    TagRepository::add_tag_to_note(conn, &note.id, &t1.id).unwrap();
    TagRepository::add_tag_to_note(conn, &note.id, &t2.id).unwrap();
    TagRepository::add_tag_to_note(conn, &note.id, &t3.id).unwrap();

    // Remove single tag T2
    TagRepository::remove_tag_from_note(conn, &note.id, &t2.id).unwrap();
    let remaining = TagRepository::get_tags_for_note(conn, &note.id).unwrap();
    assert_eq!(remaining.len(), 2);
    let remaining_names: Vec<&str> = remaining.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(remaining_names, vec!["dev", "security"]);

    // Removing non-existent tag is a safe no-op
    TagRepository::remove_tag_from_note(conn, &note.id, &t2.id).unwrap();

    // Atomic replacement: replace all tags on note with [T3, T4]
    let updated_tags = TagRepository::set_tags_for_note(
        conn,
        &note.id,
        &[t3.id.clone(), t4.id.clone(), t3.id.clone()], // duplicate in input handled
    )
    .unwrap();

    assert_eq!(updated_tags.len(), 2);
    let updated_names: Vec<&str> = updated_tags.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(updated_names, vec!["audit", "security"]);

    // Clear all tags atomically by passing empty list
    let cleared_tags = TagRepository::set_tags_for_note(conn, &note.id, &[]).unwrap();
    assert!(cleared_tags.is_empty());
    assert!(TagRepository::get_tags_for_note(conn, &note.id)
        .unwrap()
        .is_empty());
}

/// 5. Favorites lifecycle: set, toggle, list_favorites, and aggregate note counts.
#[test]
fn test_favorites_lifecycle_toggle_and_filtering() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let n1 = test_db.create_note("Note 1", "Content 1", None);
    let n2 = test_db.create_note("Note 2", "Content 2", None);
    let n3 = test_db.create_note("Note 3", "Content 3", None);

    assert!(!n1.is_favorite);
    assert!(!n2.is_favorite);
    assert!(!n3.is_favorite);

    // Set favorite explicitly
    let n1_fav = NoteRepository::set_favorite(conn, &n1.id, true).expect("set favorite");
    assert!(n1_fav.is_favorite);

    // Toggle favorite on n2 (false -> true)
    let n2_fav = NoteRepository::toggle_favorite(conn, &n2.id).expect("toggle favorite on");
    assert!(n2_fav.is_favorite);

    // Toggle favorite on n1 (true -> false)
    let n1_unfav = NoteRepository::toggle_favorite(conn, &n1.id).expect("toggle favorite off");
    assert!(!n1_unfav.is_favorite);

    // Re-enable favorite on n1
    NoteRepository::set_favorite(conn, &n1.id, true).unwrap();

    // Verify list_favorites only returns n1 and n2
    let favs = NoteRepository::list_favorites(conn).expect("list favorites");
    assert_eq!(favs.len(), 2);
    let fav_ids: Vec<String> = favs.into_iter().map(|n| n.id).collect();
    assert!(fav_ids.contains(&n1.id));
    assert!(fav_ids.contains(&n2.id));
    assert!(!fav_ids.contains(&n3.id));

    // Verify aggregate counts DTO reflects favorites count accurately
    let counts = NoteRepository::get_note_counts(conn).expect("get note counts");
    assert_eq!(counts.total, 3);
    assert_eq!(counts.favorites, 2);
    assert_eq!(counts.unfiled, 3);
    assert_eq!(counts.trash, 0);
}

/// 6. Pinned notes sorting and toggle behavior.
#[test]
fn test_pinned_notes_sort_order_and_toggle() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    // Create 4 notes
    let n1 = test_db.create_note("Alpha", "Content 1", None);
    let n2 = test_db.create_note("Beta", "Content 2", None);
    let n3 = test_db.create_note("Gamma", "Content 3", None);
    let n4 = test_db.create_note("Delta", "Content 4", None);

    // Initially none are pinned
    assert!(!n1.is_pinned);
    assert!(!n2.is_pinned);
    assert!(!n3.is_pinned);
    assert!(!n4.is_pinned);

    // Pin N2 and N4
    let n2_pinned = NoteRepository::set_pinned(conn, &n2.id, true).expect("pin note 2");
    assert!(n2_pinned.is_pinned);

    let n4_pinned = NoteRepository::toggle_pinned(conn, &n4.id).expect("toggle pin note 4");
    assert!(n4_pinned.is_pinned);

    // Listing active notes must place pinned notes first:
    // Pinned: [N4, N2] (or [N2, N4] based on modified_at DESC), followed by unpinned [N3, N1]
    let active_notes = NoteRepository::list_active(conn).expect("list active notes");
    assert_eq!(active_notes.len(), 4);
    assert!(active_notes[0].is_pinned);
    assert!(active_notes[1].is_pinned);
    assert!(!active_notes[2].is_pinned);
    assert!(!active_notes[3].is_pinned);

    let pinned_ids = [active_notes[0].id.clone(), active_notes[1].id.clone()];
    assert!(pinned_ids.contains(&n2.id));
    assert!(pinned_ids.contains(&n4.id));

    let unpinned_ids = [active_notes[2].id.clone(), active_notes[3].id.clone()];
    assert!(unpinned_ids.contains(&n1.id));
    assert!(unpinned_ids.contains(&n3.id));

    // Unpin N2 via toggle
    let n2_unpinned = NoteRepository::toggle_pinned(conn, &n2.id).expect("unpin note 2");
    assert!(!n2_unpinned.is_pinned);

    let active_after = NoteRepository::list_active(conn).expect("list active notes after unpin");
    assert_eq!(active_after[0].id, n4.id); // Only N4 remains pinned
    assert!(active_after[0].is_pinned);
    assert!(!active_after[1].is_pinned);
    assert!(!active_after[2].is_pinned);
    assert!(!active_after[3].is_pinned);
}

/// 7. Combined metadata filters: notebooks, tags, favorites, and pinned state.
#[test]
fn test_combined_metadata_filters() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    // Create Notebooks
    let nb_eng = test_db.create_notebook("Engineering", None);
    let nb_design = test_db.create_notebook("Design", None);

    // Create Tags
    let tag_urgent = TagRepository::create(conn, "urgent").unwrap();
    let tag_roadmap = TagRepository::create(conn, "roadmap").unwrap();

    // Note 1: In Engineering, Pinned, Favorite, Tagged "urgent"
    let n1 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Engine Architecture".to_string()),
            content: Some("Core system specs".to_string()),
            notebook_id: Some(nb_eng.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    TagRepository::add_tag_to_note(conn, &n1.id, &tag_urgent.id).unwrap();

    // Note 2: In Engineering, Not Pinned, Favorite, Tagged "roadmap"
    let n2 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Q4 Deliverables".to_string()),
            content: Some("Timeline".to_string()),
            notebook_id: Some(nb_eng.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(false),
            ..Default::default()
        },
    )
    .unwrap();
    TagRepository::add_tag_to_note(conn, &n2.id, &tag_roadmap.id).unwrap();

    // Note 3: In Engineering, Not Pinned, Not Favorite, Tagged "urgent"
    let n3 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Bug Triage".to_string()),
            content: Some("Fix memory leak".to_string()),
            notebook_id: Some(nb_eng.id.clone()),
            is_favorite: Some(false),
            is_pinned: Some(false),
            ..Default::default()
        },
    )
    .unwrap();
    TagRepository::add_tag_to_note(conn, &n3.id, &tag_urgent.id).unwrap();

    // Note 4: In Design, Not Pinned, Favorite, Tagged "urgent"
    let n4 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Figma UI Kit".to_string()),
            content: Some("Components".to_string()),
            notebook_id: Some(nb_design.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(false),
            ..Default::default()
        },
    )
    .unwrap();
    TagRepository::add_tag_to_note(conn, &n4.id, &tag_urgent.id).unwrap();

    // 7a. Query Engineering notebook: returns N1 (pinned), then N2, N3
    let eng_notes = NoteRepository::list_by_notebook(conn, &nb_eng.id).unwrap();
    assert_eq!(eng_notes.len(), 3);
    assert_eq!(eng_notes[0].id, n1.id);
    assert!(eng_notes[0].is_pinned);

    // 7b. Query by "urgent" tag: returns N1 (pinned), followed by unpinned N3 and N4
    let urgent_notes = NoteRepository::list_by_tag(conn, &tag_urgent.id).unwrap();
    assert_eq!(urgent_notes.len(), 3);
    assert_eq!(urgent_notes[0].id, n1.id);
    assert!(urgent_notes[0].is_pinned);
    let urgent_unpinned: Vec<String> = urgent_notes[1..].iter().map(|n| n.id.clone()).collect();
    assert!(urgent_unpinned.contains(&n3.id));
    assert!(urgent_unpinned.contains(&n4.id));

    // 7c. Query favorites: returns N1 (pinned), followed by N2 and N4
    let fav_notes = NoteRepository::list_favorites(conn).unwrap();
    assert_eq!(fav_notes.len(), 3);
    assert_eq!(fav_notes[0].id, n1.id);
    assert!(fav_notes[0].is_pinned);
    let fav_unpinned: Vec<String> = fav_notes[1..].iter().map(|n| n.id.clone()).collect();
    assert!(fav_unpinned.contains(&n2.id));
    assert!(fav_unpinned.contains(&n4.id));
    assert!(!fav_unpinned.contains(&n3.id));

    // 7d. Query aggregate counts
    let counts = NoteRepository::get_note_counts(conn).unwrap();
    assert_eq!(counts.total, 4);
    assert_eq!(counts.favorites, 3);
    assert_eq!(counts.unfiled, 0);
    assert_eq!(counts.by_notebook.get(&nb_eng.id).copied(), Some(3));
    assert_eq!(counts.by_notebook.get(&nb_design.id).copied(), Some(1));
}

/// 8. Metadata persistence across content updates and concurrent autosave simulation.
#[test]
fn test_metadata_persistence_across_content_edits_and_autosave() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let tag1 = TagRepository::create(conn, "release-candidate").unwrap();
    let tag2 = TagRepository::create(conn, "v1.0").unwrap();

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Release Notes".to_string()),
            content: Some("Initial draft".to_string()),
            is_favorite: Some(true),
            is_pinned: Some(true),
            ..Default::default()
        },
    )
    .unwrap();

    TagRepository::add_tag_to_note(conn, &note.id, &tag1.id).unwrap();
    TagRepository::add_tag_to_note(conn, &note.id, &tag2.id).unwrap();

    // Simulate autosave updating content only
    let updated = NoteRepository::update(
        conn,
        &note.id,
        UpdateNoteDto {
            content: Some("Updated draft with new features and fixes".to_string()),
            ..Default::default()
        },
    )
    .expect("autosave update");

    // Title, favorite, pinned state, and tags must remain untouched
    assert_eq!(updated.title, "Release Notes");
    assert_eq!(updated.content, "Updated draft with new features and fixes");
    assert!(updated.is_favorite);
    assert!(updated.is_pinned);

    let tags = TagRepository::get_tags_for_note(conn, &note.id).unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].name, "release-candidate");
    assert_eq!(tags[1].name, "v1.0");

    // Simulate metadata update (toggling favorite) without corrupting content
    let unfav = NoteRepository::set_favorite(conn, &note.id, false).unwrap();
    assert!(!unfav.is_favorite);
    assert!(unfav.is_pinned);
    assert_eq!(unfav.content, "Updated draft with new features and fixes");
}

/// 9. Soft-delete preserves metadata, excludes from active queries, and restore recovers fully.
#[test]
fn test_soft_delete_preserves_metadata_and_restore_recovers() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let tag = TagRepository::create(conn, "important-client").unwrap();
    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Contract Draft".to_string()),
            content: Some("Confidential terms".to_string()),
            is_favorite: Some(true),
            is_pinned: Some(true),
            ..Default::default()
        },
    )
    .unwrap();

    TagRepository::add_tag_to_note(conn, &note.id, &tag.id).unwrap();

    // Verify initial state
    assert_eq!(
        TagRepository::get_tag_note_counts(conn)
            .unwrap()
            .get(&tag.id)
            .copied(),
        Some(1)
    );
    assert_eq!(NoteRepository::list_favorites(conn).unwrap().len(), 1);
    assert_eq!(NoteRepository::list_by_tag(conn, &tag.id).unwrap().len(), 1);

    // 9a. Soft delete note to Trash
    let trashed = NoteRepository::soft_delete(conn, &note.id).expect("soft delete note");
    assert!(trashed.is_deleted);
    assert!(trashed.deleted_at.is_some());

    // Trashed note MUST be excluded from active queries
    assert!(NoteRepository::list_active(conn).unwrap().is_empty());
    assert!(NoteRepository::list_favorites(conn).unwrap().is_empty());
    assert!(NoteRepository::list_by_tag(conn, &tag.id)
        .unwrap()
        .is_empty());

    // Active tag count must ignore trashed note
    let trashed_tag_counts = TagRepository::get_tag_note_counts(conn).unwrap();
    assert_eq!(trashed_tag_counts.get(&tag.id).copied(), None);

    // Aggregate counts reflect 1 in trash and 0 in active/favorites
    let counts = NoteRepository::get_note_counts(conn).unwrap();
    assert_eq!(counts.total, 0);
    assert_eq!(counts.favorites, 0);
    assert_eq!(counts.trash, 1);

    // BUT underlying metadata is safely preserved intact!
    let direct_note = NoteRepository::get_by_id(conn, &note.id).unwrap().unwrap();
    assert!(direct_note.is_favorite);
    assert!(direct_note.is_pinned);
    let note_tags = TagRepository::get_tags_for_note(conn, &note.id).unwrap();
    assert_eq!(note_tags.len(), 1);
    assert_eq!(note_tags[0].name, "important-client");

    // 9b. Restore note back to active
    let restored = NoteRepository::restore(conn, &note.id).expect("restore note");
    assert!(!restored.is_deleted);
    assert!(restored.deleted_at.is_none());
    assert!(restored.is_favorite);
    assert!(restored.is_pinned);

    // Note reappears in active queries with metadata intact
    assert_eq!(NoteRepository::list_active(conn).unwrap().len(), 1);
    let restored_favs = NoteRepository::list_favorites(conn).unwrap();
    assert_eq!(restored_favs.len(), 1);
    assert_eq!(restored_favs[0].id, note.id);

    let restored_tag_notes = NoteRepository::list_by_tag(conn, &tag.id).unwrap();
    assert_eq!(restored_tag_notes.len(), 1);
    assert_eq!(restored_tag_notes[0].id, note.id);

    let restored_tag_counts = TagRepository::get_tag_note_counts(conn).unwrap();
    assert_eq!(restored_tag_counts.get(&tag.id).copied(), Some(1));
}

/// 10. Permanent deletion cascades junction records without orphan records or tag loss.
#[test]
fn test_permanent_delete_cascades_and_cleans_dependent_metadata() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let tag1 = TagRepository::create(conn, "project-x").unwrap();
    let tag2 = TagRepository::create(conn, "milestone-1").unwrap();

    let n1 = test_db.create_note("Note 1", "Content 1", None);
    let n2 = test_db.create_note("Note 2", "Content 2", None);

    TagRepository::add_tag_to_note(conn, &n1.id, &tag1.id).unwrap();
    TagRepository::add_tag_to_note(conn, &n1.id, &tag2.id).unwrap();
    TagRepository::add_tag_to_note(conn, &n2.id, &tag2.id).unwrap();

    // Verify initial note_tags count = 3
    let junction_count: i64 = conn
        .query_row("SELECT COUNT(1) FROM note_tags", [], |r| r.get(0))
        .unwrap();
    assert_eq!(junction_count, 3);

    // 10a. Permanently delete Note 1
    let deleted = NoteRepository::delete(conn, &n1.id, false).expect("permanently delete note 1");
    assert!(deleted);

    // Verify junction rows for Note 1 were cleaned up by ON DELETE CASCADE
    let junction_after_note: i64 = conn
        .query_row("SELECT COUNT(1) FROM note_tags", [], |r| r.get(0))
        .unwrap();
    assert_eq!(junction_after_note, 1); // Only N2-tag2 remains

    // Tags themselves still exist
    assert!(TagRepository::get_by_id(conn, &tag1.id).unwrap().is_some());
    assert!(TagRepository::get_by_id(conn, &tag2.id).unwrap().is_some());

    // 10b. Deleting a tag cleans up associated note_tags rows without deleting the note
    let tag_deleted = TagRepository::delete(conn, &tag2.id).expect("delete tag 2");
    assert!(tag_deleted);

    let junction_after_tag: i64 = conn
        .query_row("SELECT COUNT(1) FROM note_tags", [], |r| r.get(0))
        .unwrap();
    assert_eq!(junction_after_tag, 0);

    // Note 2 is unaffected and still exists
    let n2_check = NoteRepository::get_by_id(conn, &n2.id).unwrap();
    assert!(n2_check.is_some());
}

/// 11. Note metadata computation and metrics aggregation.
#[test]
fn test_note_metadata_aggregation_and_metrics() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    // Create notebook hierarchy: Projects -> Mobile
    let nb_parent = test_db.create_notebook("Projects", None);
    let nb_child = test_db.create_notebook("Mobile", Some(&nb_parent.id));

    // Create tags
    let t1 = TagRepository::create(conn, "swift").unwrap();
    let t2 = TagRepository::create(conn, "ios").unwrap();

    let note_content = "Antigravity makes engineering pleasant and resilient. Ten words here.";
    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Architecture Document".to_string()),
            content: Some(note_content.to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb_child.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(true),
        },
    )
    .unwrap();

    TagRepository::add_tag_to_note(conn, &note.id, &t1.id).unwrap();
    TagRepository::add_tag_to_note(conn, &note.id, &t2.id).unwrap();

    // Fetch NoteMetadata
    let metadata = NoteRepository::get_metadata(conn, &note.id)
        .expect("get metadata")
        .expect("metadata exists");

    assert_eq!(metadata.note_id, note.id);
    assert_eq!(metadata.format, "md");
    assert_eq!(metadata.format_display(), "Markdown");
    assert!(metadata.is_favorite);
    assert!(metadata.is_pinned);
    assert_eq!(metadata.notebook_id.as_deref(), Some(nb_child.id.as_str()));
    assert_eq!(metadata.notebook_path.as_deref(), Some("Projects / Mobile"));
    assert_eq!(metadata.word_count, 9);
    assert_eq!(metadata.character_count, note_content.chars().count());
    assert_eq!(metadata.byte_size, note_content.len());

    assert_eq!(metadata.tags.len(), 2);
    let tag_names: Vec<&str> = metadata.tags.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(tag_names, vec!["ios", "swift"]); // Sorted alphabetically

    // Verify get_notebook_path helper returns exact hierarchy path
    let path = NoteRepository::get_notebook_path(conn, &note.id).expect("get path");
    assert_eq!(path, "Projects / Mobile");

    // Test unfiled note returns "Unfiled"
    let unfiled_note = test_db.create_note("Scratch", "Quick note", None);
    let unfiled_path = NoteRepository::get_notebook_path(conn, &unfiled_note.id).unwrap();
    assert_eq!(unfiled_path, "Unfiled");
}

/// 12. Full persistence of metadata across database close and reopen.
#[test]
fn test_database_restart_and_persistence_of_all_metadata() {
    let mut test_db = TestDatabase::new();

    let tag_dev_id: String;
    let tag_qa_id: String;
    let note_pinned_id: String;
    let note_fav_id: String;
    let nb_id: String;

    {
        let conn = test_db.connection();

        let nb = NotebookRepository::create(
            conn,
            personal_notepad_lib::storage::models::CreateNotebookDto {
                name: "Work".to_string(),
                parent_id: None,
            },
        )
        .unwrap();
        nb_id = nb.id;

        let tag_dev = TagRepository::create(conn, "developer").unwrap();
        let tag_qa = TagRepository::create(conn, "quality-assurance").unwrap();
        tag_dev_id = tag_dev.id;
        tag_qa_id = tag_qa.id;

        let n_pinned = NoteRepository::create(
            conn,
            CreateNoteDto {
                title: Some("Pinned Note".to_string()),
                content: Some("Important contents".to_string()),
                notebook_id: Some(nb_id.clone()),
                is_pinned: Some(true),
                is_favorite: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        note_pinned_id = n_pinned.id;

        let n_fav = NoteRepository::create(
            conn,
            CreateNoteDto {
                title: Some("Favorite Note".to_string()),
                content: Some("Starred contents".to_string()),
                notebook_id: Some(nb_id.clone()),
                is_pinned: Some(false),
                is_favorite: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        note_fav_id = n_fav.id;

        TagRepository::add_tag_to_note(conn, &note_pinned_id, &tag_dev_id).unwrap();
        TagRepository::add_tag_to_note(conn, &note_pinned_id, &tag_qa_id).unwrap();
        TagRepository::add_tag_to_note(conn, &note_fav_id, &tag_dev_id).unwrap();
    }

    // Close SQLite connection and reopen from the same isolated on-disk database file
    test_db.close_and_reopen();

    {
        let conn = test_db.connection();

        // Verify tags persisted
        let tags = TagRepository::list(conn).unwrap();
        assert_eq!(tags.len(), 2);
        let tag_names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(tag_names, vec!["developer", "quality-assurance"]);

        // Verify tag counts persisted
        let tag_counts = TagRepository::get_tag_note_counts(conn).unwrap();
        assert_eq!(tag_counts.get(&tag_dev_id).copied(), Some(2));
        assert_eq!(tag_counts.get(&tag_qa_id).copied(), Some(1));

        // Verify pinned ordering persisted
        let active = NoteRepository::list_active(conn).unwrap();
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].id, note_pinned_id);
        assert!(active[0].is_pinned);
        assert!(!active[1].is_pinned);

        // Verify favorites list persisted
        let favs = NoteRepository::list_favorites(conn).unwrap();
        assert_eq!(favs.len(), 1);
        assert_eq!(favs[0].id, note_fav_id);

        // Verify note metadata persisted
        let meta = NoteRepository::get_metadata(conn, &note_pinned_id)
            .unwrap()
            .unwrap();
        assert_eq!(meta.tags.len(), 2);
        assert!(meta.is_pinned);
        assert!(!meta.is_favorite);
        assert_eq!(meta.notebook_path.as_deref(), Some("Work"));
    }
}
