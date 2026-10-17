mod common;

use common::TestDatabase;
use personal_notepad_lib::storage::{
    models::{CreateNoteDto, CreateNotebookDto, NoteLifecycle, UpdateNoteDto},
    repositories::{
        AttachmentRepository, NoteRepository, NotebookRepository, SearchRepository, TagRepository,
    },
    services::AttachmentService,
    AttachmentStorage,
};
use std::fs;

#[test]
fn test_note_selection_loading_and_not_found_transitions() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    // 1. Initial State: Non-existent note returns None (frontend maps to 404 / NotFound state)
    let missing_id = "00000000-0000-0000-0000-000000000000";
    let missing_note = NoteRepository::get_by_id(conn, missing_id).expect("query should succeed");
    assert!(missing_note.is_none(), "Non-existent note must return None");

    // 2. Create Note: Transitions to valid persisted note
    let created = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("State Transition Note".to_string()),
            content: Some("Initial content before editing".to_string()),
            format: Some("md".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .expect("creation should succeed");

    // 3. Select & Load: Returns exact persisted fields
    let loaded = NoteRepository::get_by_id(conn, &created.id)
        .expect("query should succeed")
        .expect("note must exist");
    assert_eq!(loaded.id, created.id);
    assert_eq!(loaded.title, "State Transition Note");
    assert_eq!(loaded.content, "Initial content before editing");
    assert!(!loaded.is_deleted);
}

#[test]
fn test_editor_save_transitions_monotonic_timestamps_and_error_isolation() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let created = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Original Title".to_string()),
            content: Some("Original Body".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .expect("note created");

    let initial_created_at = created.created_at.clone();
    let initial_modified_at = created.modified_at.clone();

    // Small sleep to ensure clock ticks forward for monotonic timestamp verification
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Save Edit 1: Transitions dirty -> saving -> saved
    let updated = NoteRepository::update(
        conn,
        &created.id,
        UpdateNoteDto {
            title: Some("Updated Title 1".to_string()),
            content: Some("Updated Content 1".to_string()),
            format: Some("md".to_string()),
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
            is_deleted: None,
        },
    )
    .expect("first update succeeds");

    assert_eq!(updated.title, "Updated Title 1");
    assert_eq!(updated.content, "Updated Content 1");
    assert_eq!(updated.format, "md");
    assert_eq!(
        updated.created_at, initial_created_at,
        "created_at must never change on edit"
    );
    assert!(
        updated.modified_at >= initial_modified_at,
        "modified_at must advance monotonically"
    );

    // Save Edit 2 with Validation Failure: Title > 500 characters
    let long_title = "A".repeat(501);
    let fail_result = NoteRepository::update(
        conn,
        &created.id,
        UpdateNoteDto {
            title: Some(long_title),
            content: Some("Attempted payload".to_string()),
            format: None,
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
            is_deleted: None,
        },
    );

    assert!(
        fail_result.is_err(),
        "Exceeding title limit must fail validation"
    );

    // Verify existing database state is preserved without partial mutation
    let reloaded = NoteRepository::get_by_id(conn, &created.id)
        .expect("get succeeds")
        .expect("note exists");
    assert_eq!(
        reloaded.title, "Updated Title 1",
        "Database title must remain unchanged after failed save"
    );
    assert_eq!(
        reloaded.content, "Updated Content 1",
        "Database content must remain unchanged after failed save"
    );
}

#[test]
fn test_sidebar_notebook_selection_note_movement_and_unfiling_transitions() {
    let mut test_db = TestDatabase::new();
    let conn = test_db.connection_mut();

    // Create Notebook A and Notebook B
    let nb_a = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Work Notebook".to_string(),
            parent_id: None,
        },
    )
    .expect("create notebook A");

    let nb_b = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Archive Notebook".to_string(),
            parent_id: None,
        },
    )
    .expect("create notebook B");

    // Create note in Notebook A
    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Project Plan".to_string()),
            content: Some("Drafting Q4 plan".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb_a.id.clone()),
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .expect("create note in nb A");

    assert_eq!(note.notebook_id, Some(nb_a.id.clone()));

    // Move note from Notebook A to Notebook B
    let moved = NoteRepository::update(
        conn,
        &note.id,
        UpdateNoteDto {
            title: None,
            content: None,
            format: None,
            notebook_id: Some(nb_b.id.clone()),
            is_favorite: None,
            is_pinned: None,
            is_deleted: None,
        },
    )
    .expect("move note to nb B");

    assert_eq!(moved.notebook_id, Some(nb_b.id.clone()));

    // Delete Notebook B: Verify note is unfiled to root (notebook_id = None) rather than deleted
    NotebookRepository::delete(conn, &nb_b.id).expect("delete notebook B");

    let unfiled = NoteRepository::get_by_id(conn, &note.id)
        .expect("get note")
        .expect("note must still exist");
    assert_eq!(
        unfiled.notebook_id, None,
        "Note must be unfiled to None when parent notebook is removed"
    );
}

#[test]
fn test_tag_assignment_removal_and_favorite_toggle_transitions() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Tagged Note".to_string()),
            content: Some("Testing tag associations".to_string()),
            format: Some("md".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .expect("create note");

    let tag1 = TagRepository::create(conn, "frontend").expect("create tag 1");
    let tag2 = TagRepository::create(conn, "accessibility").expect("create tag 2");

    // Associate tags with note
    TagRepository::add_tag_to_note(conn, &note.id, &tag1.id).expect("add tag 1");
    TagRepository::add_tag_to_note(conn, &note.id, &tag2.id).expect("add tag 2");

    let tags = TagRepository::get_tags_for_note(conn, &note.id).expect("get tags");
    assert_eq!(tags.len(), 2);

    // Remove one tag
    TagRepository::remove_tag_from_note(conn, &note.id, &tag2.id).expect("remove tag 2");
    let updated_tags = TagRepository::get_tags_for_note(conn, &note.id).expect("get updated tags");
    assert_eq!(updated_tags.len(), 1);
    assert_eq!(updated_tags[0].name, "frontend");

    // Toggle favorite state
    assert!(!note.is_favorite);
    let favorited = NoteRepository::update(
        conn,
        &note.id,
        UpdateNoteDto {
            title: None,
            content: None,
            format: None,
            notebook_id: None,
            is_favorite: Some(true),
            is_pinned: None,
            is_deleted: None,
        },
    )
    .expect("toggle favorite true");
    assert!(favorited.is_favorite);

    let unfavorited = NoteRepository::update(
        conn,
        &note.id,
        UpdateNoteDto {
            title: None,
            content: None,
            format: None,
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: None,
            is_deleted: None,
        },
    )
    .expect("toggle favorite false");
    assert!(!unfavorited.is_favorite);
}

#[test]
fn test_trash_soft_delete_restore_and_permanent_delete_lifecycle_transitions() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Trash Transition Note".to_string()),
            content: Some("Lifecycle test".to_string()),
            format: Some("md".to_string()),
            notebook_id: None,
            is_favorite: Some(true),
            is_pinned: Some(true),
        },
    )
    .expect("note created");

    // State 1: Active
    assert_eq!(note.lifecycle(), NoteLifecycle::Active);
    assert!(!note.is_deleted);
    let active_list = NoteRepository::list(conn, false).expect("list");
    assert!(active_list.iter().any(|n| n.id == note.id));

    // State 2: Soft delete -> Trashed
    NoteRepository::delete(conn, &note.id, true).expect("soft delete");
    let trashed_note = NoteRepository::get_by_id(conn, &note.id)
        .expect("get")
        .expect("exists");
    assert_eq!(trashed_note.lifecycle(), NoteLifecycle::Trashed);
    assert!(trashed_note.is_deleted, "Note must have is_deleted = true");

    // Trashed note must disappear from active lists
    let active_after_trash = NoteRepository::list(conn, false).expect("list");
    assert!(!active_after_trash.iter().any(|n| n.id == note.id));

    // Trashed note appears in trash list
    let trash_list = NoteRepository::list_trashed(conn).expect("list trash");
    assert!(trash_list.iter().any(|n| n.id == note.id));

    // State 3: Restore from Trash
    NoteRepository::restore(conn, &note.id).expect("restore");
    let restored = NoteRepository::get_by_id(conn, &note.id)
        .expect("get")
        .expect("exists");
    assert_eq!(restored.lifecycle(), NoteLifecycle::Active);
    assert!(
        !restored.is_deleted,
        "Restored note must have is_deleted = false"
    );

    // State 4: Permanent Delete
    NoteRepository::delete(conn, &note.id, false).expect("hard delete");
    let gone = NoteRepository::get_by_id(conn, &note.id).expect("get");
    assert!(gone.is_none(), "Permanently deleted note must return None");
}

#[test]
fn test_search_index_live_synchronization_and_trash_exclusion_transitions() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Quantum Computing Overview".to_string()),
            content: Some("Superposition and entanglement principles.".to_string()),
            format: Some("md".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .expect("create");

    // Search query matches active note
    let results = SearchRepository::search(conn, "Quantum", Some(10)).expect("search");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].note_id, note.id);

    // Edit content -> FTS search reflects updated keyword
    NoteRepository::update(
        conn,
        &note.id,
        UpdateNoteDto {
            title: Some("Quantum Computing Advanced".to_string()),
            content: Some("Qubits, decoherence, and error mitigation algorithms.".to_string()),
            format: None,
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
            is_deleted: None,
        },
    )
    .expect("update");

    let updated_results =
        SearchRepository::search(conn, "decoherence", Some(10)).expect("search updated");
    assert_eq!(updated_results.len(), 1);
    assert_eq!(updated_results[0].note_id, note.id);

    // Move to Trash -> Immediate exclusion from search
    NoteRepository::delete(conn, &note.id, true).expect("soft delete");
    let trash_search =
        SearchRepository::search(conn, "decoherence", Some(10)).expect("search trashed");
    assert!(
        trash_search.is_empty(),
        "Trashed notes must be excluded from search results"
    );
}

#[test]
fn test_attachment_lifecycle_add_list_delete_transitions() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Note with Attachments".to_string()),
            content: Some("Testing attachment transitions".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
        },
    )
    .expect("note");

    // 1. Initial State: Attachment count is 0
    let initial_attachments = AttachmentRepository::list_by_note_id(conn, &note.id).expect("get");
    assert_eq!(initial_attachments.len(), 0);

    // 2. Add Attachment
    let source_dir = test_db.root().join("source_files");
    fs::create_dir_all(&source_dir).expect("create source dir");
    let file_path = source_dir.join("report.pdf");
    fs::write(&file_path, b"%PDF-1.7 Report content for PDF test").expect("write source file");

    let attachment = AttachmentService::add_attachment(conn, &storage, &note.id, &file_path)
        .expect("add attachment");

    assert_eq!(attachment.note_id, note.id);
    assert_eq!(attachment.file_name, "report.pdf");

    // 3. Count is now 1
    let attachments_after_add = AttachmentRepository::list_by_note_id(conn, &note.id).expect("get");
    assert_eq!(attachments_after_add.len(), 1);

    // 4. Remove Attachment
    AttachmentRepository::delete(conn, &attachment.id).expect("delete attachment");

    let final_attachments = AttachmentRepository::list_by_note_id(conn, &note.id).expect("get");
    assert_eq!(final_attachments.len(), 0);
}
