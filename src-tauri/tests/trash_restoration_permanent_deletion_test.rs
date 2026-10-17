mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use common::TestDatabase;
use personal_notepad_lib::storage::{
    crypto::generate_dek,
    errors::TrashError,
    models::{CreateNoteDto, CreateNotebookDto, LinkTargetLifecycle, NoteLifecycle},
    repositories::{
        AttachmentRepository, NoteRepository, NotebookRepository, SearchRepository, TagRepository,
    },
    services::{links::InternalLinkService, AttachmentService, TrashService},
    AttachmentStorage,
};

/// 1. Moving an active note to Trash excludes it from normal lists, notebook views, and search.
#[test]
fn test_soft_delete_and_exclusion_from_normal_views() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let nb = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Work Notebook".to_string(),
            parent_id: None,
        },
    )
    .expect("create notebook");

    let tag = TagRepository::create(conn, "urgent").expect("create tag");

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("System Architecture Plan".to_string()),
            content: Some("Kubernetes cluster design and service mesh notes.".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(true),
        },
    )
    .expect("create note");
    TagRepository::add_tag_to_note(conn, &note.id, &tag.id).expect("add tag to note");

    assert_eq!(note.lifecycle(), NoteLifecycle::Active);
    assert!(!note.is_deleted);
    assert_eq!(note.deleted_at, None);

    // Initial counts
    assert_eq!(NoteRepository::count_active(conn).unwrap(), 1);
    assert_eq!(NoteRepository::count_trashed(conn).unwrap(), 0);

    // Search matches before soft-delete
    let search_before = SearchRepository::search(conn, "Kubernetes", Some(10)).unwrap();
    assert_eq!(search_before.len(), 1);
    assert_eq!(search_before[0].note_id, note.id);

    // Execute soft delete
    let trashed = TrashService::soft_delete(conn, &note.id).expect("soft delete note");

    // Invariants on returned trashed note
    assert_eq!(trashed.id, note.id);
    assert_eq!(trashed.title, "System Architecture Plan");
    assert_eq!(
        trashed.content,
        "Kubernetes cluster design and service mesh notes."
    );
    assert_eq!(trashed.format, "md");
    assert_eq!(trashed.notebook_id, Some(nb.id.clone()));
    assert_eq!(trashed.created_at, note.created_at);
    assert!(trashed.is_favorite);
    assert!(trashed.is_pinned);
    assert!(trashed.is_deleted);
    assert!(trashed.deleted_at.is_some());
    assert_eq!(trashed.lifecycle(), NoteLifecycle::Trashed);

    // Counts updated
    assert_eq!(NoteRepository::count_active(conn).unwrap(), 0);
    assert_eq!(NoteRepository::count_trashed(conn).unwrap(), 1);

    // Excluded from normal active list
    let active_notes = NoteRepository::list_active(conn).unwrap();
    assert!(!active_notes.iter().any(|n| n.id == note.id));

    // Excluded from notebook view
    let nb_notes = NoteRepository::list_by_notebook(conn, &nb.id).unwrap();
    assert!(!nb_notes.iter().any(|n| n.id == note.id));

    // Excluded from favorites list
    let fav_notes = NoteRepository::list_favorites(conn).unwrap();
    assert!(!fav_notes.iter().any(|n| n.id == note.id));

    // Excluded from pinned notes in active list
    assert!(!active_notes.iter().any(|n| n.is_pinned && n.id == note.id));

    // Excluded from search
    let search_after = SearchRepository::search(conn, "Kubernetes", Some(10)).unwrap();
    assert_eq!(search_after.len(), 0);

    // Appears in trash list
    let trashed_notes = NoteRepository::list_trashed(conn).unwrap();
    assert_eq!(trashed_notes.len(), 1);
    assert_eq!(trashed_notes[0].id, note.id);
    assert_eq!(trashed_notes[0].title, "System Architecture Plan");

    // Tag associations remain intact in SQLite
    let tags_after = TagRepository::get_tags_for_note(conn, &note.id).unwrap();
    assert_eq!(tags_after.len(), 1);
    assert_eq!(tags_after[0].id, tag.id);
}

/// 2. Restoring a note restores content, metadata, relationships, and index visibility.
#[test]
fn test_restore_note_content_and_metadata_preservation() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let nb = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Research".to_string(),
            parent_id: None,
        },
    )
    .expect("create notebook");

    let tag = TagRepository::create(conn, "ml-ai").expect("create tag");

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Transformer Architecture".to_string()),
            content: Some("Attention mechanisms and positional encodings.".to_string()),
            format: Some("txt".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(true),
        },
    )
    .expect("create note");
    TagRepository::add_tag_to_note(conn, &note.id, &tag.id).expect("add tag to note");

    // Move to Trash
    TrashService::soft_delete(conn, &note.id).expect("trash note");
    assert_eq!(NoteRepository::count_active(conn).unwrap(), 0);
    assert_eq!(NoteRepository::count_trashed(conn).unwrap(), 1);

    // Restore note
    let restored = TrashService::restore(conn, &note.id).expect("restore note");

    // Metadata preservation invariants
    assert_eq!(restored.id, note.id);
    assert_eq!(restored.title, "Transformer Architecture");
    assert_eq!(
        restored.content,
        "Attention mechanisms and positional encodings."
    );
    assert_eq!(restored.format, "txt");
    assert_eq!(restored.created_at, note.created_at);
    assert_eq!(restored.notebook_id, Some(nb.id.clone()));
    assert!(restored.is_favorite);
    assert!(restored.is_pinned);
    assert!(!restored.is_deleted);
    assert_eq!(restored.deleted_at, None);
    assert_eq!(restored.lifecycle(), NoteLifecycle::Active);

    // Counts updated
    assert_eq!(NoteRepository::count_active(conn).unwrap(), 1);
    assert_eq!(NoteRepository::count_trashed(conn).unwrap(), 0);

    // Reappears in active list
    let active = NoteRepository::list_active(conn).unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, note.id);

    // Reappears in notebook view
    let nb_list = NoteRepository::list_by_notebook(conn, &nb.id).unwrap();
    assert_eq!(nb_list.len(), 1);
    assert_eq!(nb_list[0].id, note.id);

    // Reappears in favorites list
    let favs = NoteRepository::list_favorites(conn).unwrap();
    assert_eq!(favs.len(), 1);
    assert_eq!(favs[0].id, note.id);

    // Reappears in active list with pinned state preserved
    assert!(active.iter().any(|n| n.is_pinned && n.id == note.id));

    // Reappears in search results
    let search_res = SearchRepository::search(conn, "Transformer", Some(10)).unwrap();
    assert_eq!(search_res.len(), 1);
    assert_eq!(search_res[0].note_id, note.id);

    // Tags preserved
    let tags = TagRepository::get_tags_for_note(conn, &note.id).unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].id, tag.id);
}

/// 3. Restoring a note defensively unfiles it if its parent notebook was deleted while trashed.
#[test]
fn test_restore_defensively_unfiles_if_notebook_was_deleted() {
    let mut test_db = TestDatabase::new();

    let nb = NotebookRepository::create(
        test_db.connection(),
        CreateNotebookDto {
            name: "Temporary Notebook".to_string(),
            parent_id: None,
        },
    )
    .expect("create notebook");

    let note = NoteRepository::create(
        test_db.connection(),
        CreateNoteDto {
            title: Some("Orphan Candidate Note".to_string()),
            content: Some("Note content awaiting restoration.".to_string()),
            notebook_id: Some(nb.id.clone()),
            ..Default::default()
        },
    )
    .expect("create note");

    // Trash the note
    TrashService::soft_delete(test_db.connection(), &note.id).expect("trash note");

    // Delete the notebook while the note is in Trash
    NotebookRepository::delete(test_db.connection_mut(), &nb.id)
        .expect("delete notebook while note trashed");

    // Restore the note: should succeed and defensively unfile the note instead of crashing
    let restored = TrashService::restore(test_db.connection(), &note.id).expect("restore note");
    assert_eq!(restored.id, note.id);
    assert!(!restored.is_deleted);
    assert_eq!(
        restored.notebook_id, None,
        "Note must be safely unfiled (notebook_id = None) when notebook was deleted"
    );

    // Note is active and unfiled
    let active = NoteRepository::list_active(test_db.connection()).unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].notebook_id, None);
}

/// 4. Restoring a note containing attachments preserves disk files, SQLite rows, and access.
#[test]
fn test_restore_note_containing_attachments() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Note with Attachments".to_string()),
            content: Some("Project documentation with attached blueprints.".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let src1 = test_db.root().join("blueprint.pdf");
    let src2 = test_db.root().join("mockup.png");
    let pdf_bytes = b"%PDF-1.4 architectural blueprint";
    let png_bytes = b"\x89PNG\r\n\x1a\n ui mockup";
    fs::write(&src1, pdf_bytes).unwrap();
    fs::write(&src2, png_bytes).unwrap();

    let att1 = AttachmentService::add_attachment(conn, &storage, &note.id, &src1).unwrap();
    let att2 = AttachmentService::add_attachment(conn, &storage, &note.id, &src2).unwrap();

    // Soft-delete the note
    TrashService::soft_delete(conn, &note.id).expect("trash note");

    // While in Trash, removing an attachment is prohibited
    let rm_err = AttachmentService::remove_attachment(conn, &storage, &att1.id).unwrap_err();
    assert!(rm_err
        .user_friendly_message()
        .contains("Cannot remove attachments from a deleted note. Restore it first."));

    // Both files remain present on disk and registered in SQLite
    assert!(storage.file_exists(&att1.relative_path));
    assert!(storage.file_exists(&att2.relative_path));

    // Restore the note
    TrashService::restore(conn, &note.id).expect("restore note");

    // Attachments listed and accessible
    let att_list = AttachmentService::list_attachments(conn, &note.id).unwrap();
    assert_eq!(att_list.len(), 2);

    let path1 = AttachmentService::get_attachment_for_open(conn, &storage, &att1.id).unwrap();
    let path2 = AttachmentService::get_attachment_for_open(conn, &storage, &att2.id).unwrap();
    assert_eq!(fs::read(path1).unwrap(), pdf_bytes);
    assert_eq!(fs::read(path2).unwrap(), png_bytes);
}

/// 5. Restoring a note referenced by internal links restores link lifecycle and backlink discovery.
#[test]
fn test_restore_note_referenced_by_internal_links() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    // Note B: target note
    let note_b = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Target Roadmap Note".to_string()),
            content: Some("Phase 14 execution milestones.".to_string()),
            ..Default::default()
        },
    )
    .expect("create note B");

    // Note A: referencing note
    let note_a_content = format!(
        "Please review [Project Roadmap](notepad://note/{}) for details.",
        note_b.id
    );
    let note_a = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Referencing Note".to_string()),
            content: Some(note_a_content),
            ..Default::default()
        },
    )
    .expect("create note A");

    // 5a. Initial state: Note B is active
    let target_initial = InternalLinkService::resolve_link_target(conn, &note_b.id).unwrap();
    assert_eq!(target_initial.lifecycle, LinkTargetLifecycle::Active);
    assert_eq!(target_initial.title.as_deref(), Some("Target Roadmap Note"));

    let backlinks_initial = InternalLinkService::get_backlinks(conn, &note_b.id).unwrap();
    assert_eq!(backlinks_initial.len(), 1);
    assert_eq!(backlinks_initial[0].source_note_id, note_a.id);

    // 5b. Soft-delete Note B
    TrashService::soft_delete(conn, &note_b.id).expect("trash note B");

    let target_trashed = InternalLinkService::resolve_link_target(conn, &note_b.id).unwrap();
    assert_eq!(target_trashed.lifecycle, LinkTargetLifecycle::Trashed);

    // 5c. Restore Note B
    TrashService::restore(conn, &note_b.id).expect("restore note B");

    let target_restored = InternalLinkService::resolve_link_target(conn, &note_b.id).unwrap();
    assert_eq!(target_restored.lifecycle, LinkTargetLifecycle::Active);
    assert_eq!(
        target_restored.title.as_deref(),
        Some("Target Roadmap Note")
    );

    let backlinks_restored = InternalLinkService::get_backlinks(conn, &note_b.id).unwrap();
    assert_eq!(backlinks_restored.len(), 1);
    assert_eq!(backlinks_restored[0].source_note_id, note_a.id);
}

/// 6. Permanently deleting a single note cascades deletions across SQLite, FTS, disk files, and links.
#[test]
fn test_permanently_delete_single_note_lifecycle_and_cascades() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Doomed Note For Permanent Deletion".to_string()),
            content: Some("UniqueKeywordSecret content to verify FTS purge.".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let tag = TagRepository::create(conn, "doomed-tag").unwrap();
    TagRepository::add_tag_to_note(conn, &note.id, &tag.id).unwrap();

    let src = test_db.root().join("attachment.txt");
    fs::write(&src, b"Doomed attachment bytes").unwrap();
    let att = AttachmentService::add_attachment(conn, &storage, &note.id, &src).unwrap();
    assert!(storage.file_exists(&att.relative_path));

    // Note B refers to Doomed Note
    let link_content = format!("Reference to [Doomed](notepad://note/{})", note.id);
    let _referencing = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Referencer".to_string()),
            content: Some(link_content),
            ..Default::default()
        },
    )
    .unwrap();

    // 6a. Attempting permanent deletion on ACTIVE note MUST fail with InvalidState
    let active_del_err =
        TrashService::permanently_delete_note(conn, &storage, &note.id).unwrap_err();
    match active_del_err {
        TrashError::InvalidState(msg) => {
            assert!(msg.contains("Only trashed notes can be permanently deleted"));
        }
        other => panic!("expected InvalidState for active note deletion, got {other:?}"),
    }

    // 6b. Move note to Trash first
    TrashService::soft_delete(conn, &note.id).expect("trash note");

    // 6c. Execute permanent delete
    let perm_res = TrashService::permanently_delete_note(conn, &storage, &note.id)
        .expect("permanently delete note");
    assert!(perm_res.success);
    assert!(!perm_res.attachment_cleanup_failed);
    assert!(perm_res.error_message.is_none());

    // Note row in notes table is gone
    assert!(NoteRepository::get_by_id(conn, &note.id).unwrap().is_none());

    // Note tags cascaded
    let tags_after = TagRepository::get_tags_for_note(conn, &note.id).unwrap();
    assert!(tags_after.is_empty());

    // Attachments cascaded in SQLite
    let atts_after = AttachmentRepository::list_by_note_id(conn, &note.id).unwrap();
    assert!(atts_after.is_empty());

    // Attachment files and note folder removed from disk
    assert!(!storage.file_exists(&att.relative_path));
    let note_dir = test_db.paths().attachments_dir.join(&note.id);
    assert!(!note_dir.exists());

    // FTS search no longer matches
    let fts_res = SearchRepository::search(conn, "UniqueKeywordSecret", Some(10)).unwrap();
    assert!(fts_res.is_empty());

    // Links pointing to permanently deleted note resolve as Missing
    let target_res = InternalLinkService::resolve_link_target(conn, &note.id).unwrap();
    assert_eq!(target_res.lifecycle, LinkTargetLifecycle::Missing);
}

/// 7. Emptying Trash permanently removes all trashed notes and files while isolating active survivors.
#[test]
fn test_empty_trash_lifecycle_and_survivor_isolation() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    // Survivor note (active)
    let survivor = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Survivor Note".to_string()),
            content: Some("I will survive empty trash.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    let surv_src = test_db.root().join("surv_att.txt");
    fs::write(&surv_src, b"survivor bytes").unwrap();
    let surv_att =
        AttachmentService::add_attachment(conn, &storage, &survivor.id, &surv_src).unwrap();

    // Doomed notes (trashed)
    let doomed1 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Doomed Note 1".to_string()),
            content: Some("First doomed note.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    let d1_src = test_db.root().join("d1_att.txt");
    fs::write(&d1_src, b"doomed 1 bytes").unwrap();
    let d1_att = AttachmentService::add_attachment(conn, &storage, &doomed1.id, &d1_src).unwrap();

    let doomed2 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Doomed Note 2".to_string()),
            content: Some("Second doomed note without attachments.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // Trash both doomed notes
    TrashService::soft_delete(conn, &doomed1.id).unwrap();
    TrashService::soft_delete(conn, &doomed2.id).unwrap();

    assert_eq!(NoteRepository::count_active(conn).unwrap(), 1);
    assert_eq!(NoteRepository::count_trashed(conn).unwrap(), 2);

    // Empty trash
    let result = TrashService::empty_trash(conn, &storage).expect("empty trash");
    assert_eq!(result.deleted_count, 2);
    assert_eq!(result.failed_count, 0);
    assert!(result.attachment_cleanup_failures.is_empty());
    assert!(result.errors.is_empty());

    // Trash is now empty
    assert_eq!(NoteRepository::count_trashed(conn).unwrap(), 0);
    assert_eq!(NoteRepository::count_active(conn).unwrap(), 1);

    // Doomed notes and their files are gone
    assert!(NoteRepository::get_by_id(conn, &doomed1.id)
        .unwrap()
        .is_none());
    assert!(NoteRepository::get_by_id(conn, &doomed2.id)
        .unwrap()
        .is_none());
    assert!(!storage.file_exists(&d1_att.relative_path));

    // Survivor note is completely unaffected
    let surv_fetched = NoteRepository::get_by_id(conn, &survivor.id)
        .unwrap()
        .unwrap();
    assert_eq!(surv_fetched.title, "Survivor Note");
    assert_eq!(surv_fetched.lifecycle(), NoteLifecycle::Active);
    assert!(storage.file_exists(&surv_att.relative_path));

    // Calling empty_trash again is idempotent and returns 0 deleted count
    let repeat_res = TrashService::empty_trash(conn, &storage).unwrap();
    assert_eq!(repeat_res.deleted_count, 0);
    assert_eq!(repeat_res.failed_count, 0);
}

/// 8. State machine validations: repeated deletion, repeated restore, and invalid transitions.
#[test]
fn test_repeated_deletion_and_restore_state_machines() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("State Transition Note".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 8a. Restoring an active note fails with InvalidState
    let restore_active_err = TrashService::restore(conn, &note.id).unwrap_err();
    assert!(matches!(restore_active_err, TrashError::InvalidState(_)));

    // 8b. Permanently deleting an active note fails with InvalidState
    let perm_del_active_err =
        TrashService::permanently_delete_note(conn, &storage, &note.id).unwrap_err();
    assert!(matches!(perm_del_active_err, TrashError::InvalidState(_)));

    // 8c. First soft delete succeeds
    TrashService::soft_delete(conn, &note.id).expect("first soft delete");

    // 8d. Repeated soft delete on already trashed note fails with InvalidState
    let repeat_soft_del_err = TrashService::soft_delete(conn, &note.id).unwrap_err();
    assert!(matches!(repeat_soft_del_err, TrashError::InvalidState(_)));

    // 8e. First restore succeeds
    TrashService::restore(conn, &note.id).expect("first restore");

    // 8f. Repeated restore on already restored note fails with InvalidState
    let repeat_restore_err = TrashService::restore(conn, &note.id).unwrap_err();
    assert!(matches!(repeat_restore_err, TrashError::InvalidState(_)));

    // 8g. Soft delete then permanent delete succeeds
    TrashService::soft_delete(conn, &note.id).expect("soft delete for perm del");
    let perm_res =
        TrashService::permanently_delete_note(conn, &storage, &note.id).expect("perm del");
    assert!(perm_res.success);

    // 8h. Repeated permanent delete on gone note fails with NoteNotFound
    let repeat_perm_del_err =
        TrashService::permanently_delete_note(conn, &storage, &note.id).unwrap_err();
    assert!(matches!(repeat_perm_del_err, TrashError::NoteNotFound(_)));
}

/// 9. Rejection of invalid, whitespace, path-traversal, and missing note IDs.
#[test]
fn test_invalid_and_missing_note_ids() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let invalid_ids = [
        ("", "empty"),
        ("   ", "whitespace"),
        ("../traversal", "relative parent traversal"),
        ("..\\windows", "backslash traversal"),
        ("note/subpath", "slash separator"),
    ];

    for (bad_id, desc) in invalid_ids {
        let err_soft = TrashService::soft_delete(conn, bad_id).unwrap_err();
        assert!(
            matches!(err_soft, TrashError::InvalidRequest(_)),
            "soft_delete failed on {desc}"
        );

        let err_restore = TrashService::restore(conn, bad_id).unwrap_err();
        assert!(
            matches!(err_restore, TrashError::InvalidRequest(_)),
            "restore failed on {desc}"
        );

        let err_perm = TrashService::permanently_delete_note(conn, &storage, bad_id).unwrap_err();
        assert!(
            matches!(err_perm, TrashError::InvalidRequest(_)),
            "permanently_delete failed on {desc}"
        );
    }

    // Missing UUID (well-formed format but nonexistent)
    let missing_uuid = "00000000-0000-0000-0000-000000000000";
    assert!(matches!(
        TrashService::soft_delete(conn, missing_uuid).unwrap_err(),
        TrashError::NoteNotFound(_)
    ));
    assert!(matches!(
        TrashService::restore(conn, missing_uuid).unwrap_err(),
        TrashError::NoteNotFound(_)
    ));
    assert!(matches!(
        TrashService::permanently_delete_note(conn, &storage, missing_uuid).unwrap_err(),
        TrashError::NoteNotFound(_)
    ));
}

/// 10. Filesystem failure during attachment cleanup is reported honestly without corrupting database deletion.
#[test]
fn test_filesystem_failure_during_attachment_cleanup_reported_honestly() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Locked Directory Note".to_string()),
            content: Some("Testing honest reporting of partial disk failure.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let src = test_db.root().join("file.txt");
    fs::write(&src, b"locked file bytes").unwrap();
    AttachmentService::add_attachment(conn, &storage, &note.id, &src).unwrap();

    let note_dir = storage.get_note_dir(&note.id).unwrap();
    assert!(note_dir.exists());

    // Trash note
    TrashService::soft_delete(conn, &note.id).unwrap();

    // Make note directory read-only so remove_dir_all fails on POSIX
    fs::set_permissions(&note_dir, fs::Permissions::from_mode(0o555)).unwrap();

    let res = TrashService::permanently_delete_note(conn, &storage, &note.id).unwrap();

    // Database deletion still succeeded
    assert!(res.success);
    // But attachment cleanup failure is reported honestly!
    assert!(
        res.attachment_cleanup_failed,
        "Must honestly report that attachment cleanup failed"
    );
    assert!(
        res.error_message.is_some(),
        "Must supply error message for cleanup failure"
    );

    // Note record is deleted from DB
    assert!(NoteRepository::get_by_id(conn, &note.id).unwrap().is_none());

    // Restore permissions so test fixture can clean up
    let _ = fs::set_permissions(&note_dir, fs::Permissions::from_mode(0o755));
}

/// 11. Database restart persists Trash and active states across application reboots.
#[test]
fn test_database_restart_preserves_trash_and_active_states() {
    let mut test_db = TestDatabase::new();

    let n_active = NoteRepository::create(
        test_db.connection(),
        CreateNoteDto {
            title: Some("Active Persistent Note".to_string()),
            content: Some("Content that stays active.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let n_trashed = NoteRepository::create(
        test_db.connection(),
        CreateNoteDto {
            title: Some("Trashed Persistent Note".to_string()),
            content: Some("Content that stays trashed.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    let trashed_original = TrashService::soft_delete(test_db.connection(), &n_trashed.id).unwrap();

    // Simulate restart
    test_db.close_and_reopen();
    let conn = test_db.connection();

    // Active note remains active
    let active_list = NoteRepository::list_active(conn).unwrap();
    assert_eq!(active_list.len(), 1);
    assert_eq!(active_list[0].id, n_active.id);
    assert!(!active_list[0].is_deleted);
    assert_eq!(active_list[0].deleted_at, None);

    // Trashed note remains trashed with identical deletion timestamp
    let trashed_list = NoteRepository::list_trashed(conn).unwrap();
    assert_eq!(trashed_list.len(), 1);
    assert_eq!(trashed_list[0].id, n_trashed.id);
    assert!(trashed_list[0].is_deleted);
    assert_eq!(trashed_list[0].deleted_at, trashed_original.deleted_at);

    // Trashed note can be restored after restart
    let restored = TrashService::restore(conn, &n_trashed.id).expect("restore after restart");
    assert_eq!(restored.id, n_trashed.id);
    assert!(!restored.is_deleted);
    assert_eq!(NoteRepository::count_active(conn).unwrap(), 2);
}

/// 12. Bulk deletion with mixed attachment and content states via empty_trash.
#[test]
fn test_bulk_deletion_with_mixed_attachment_states() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    // 2 active notes
    let _a1 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Active 1".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    let _a2 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Active 2".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 5 trashed notes with various states
    let t1 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("T1 with PDF".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    let f1 = test_db.root().join("t1.pdf");
    fs::write(&f1, b"%PDF sample").unwrap();
    AttachmentService::add_attachment(conn, &storage, &t1.id, &f1).unwrap();

    let t2 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("T2 plain text".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    let t3 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("T3 plain text".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let t4 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("T4 multi attachments".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    let f4a = test_db.root().join("t4a.txt");
    let f4b = test_db.root().join("t4b.txt");
    fs::write(&f4a, b"a").unwrap();
    fs::write(&f4b, b"b").unwrap();
    AttachmentService::add_attachment(conn, &storage, &t4.id, &f4a).unwrap();
    AttachmentService::add_attachment(conn, &storage, &t4.id, &f4b).unwrap();

    let t5 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("T5 plain text".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    for id in [&t1.id, &t2.id, &t3.id, &t4.id, &t5.id] {
        TrashService::soft_delete(conn, id).unwrap();
    }

    assert_eq!(NoteRepository::count_trashed(conn).unwrap(), 5);
    assert_eq!(NoteRepository::count_active(conn).unwrap(), 2);

    let empty_res = TrashService::empty_trash(conn, &storage).expect("empty trash batch");
    assert_eq!(empty_res.deleted_count, 5);
    assert_eq!(empty_res.failed_count, 0);
    assert!(empty_res.attachment_cleanup_failures.is_empty());

    assert_eq!(NoteRepository::count_trashed(conn).unwrap(), 0);
    assert_eq!(NoteRepository::count_active(conn).unwrap(), 2);
}

/// 13. Encrypted note trash, restore, and permanent deletion with active key.
#[test]
fn test_encrypted_note_trash_restore_and_permanent_delete() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let dek = generate_dek();

    // Create encrypted note
    let note = NoteRepository::create_with_key(
        conn,
        CreateNoteDto {
            title: Some("Confidential R&D Blueprint".to_string()),
            content: Some("Secret encrypted algorithm details.".to_string()),
            ..Default::default()
        },
        Some(&dek),
    )
    .expect("create encrypted note");

    // Soft delete with key
    let trashed = TrashService::soft_delete_with_key(conn, &note.id, Some(&dek))
        .expect("trash encrypted note");
    assert!(trashed.is_deleted);
    assert_eq!(trashed.title, "Confidential R&D Blueprint");
    assert_eq!(trashed.content, "Secret encrypted algorithm details.");

    // Restore with key
    let restored =
        TrashService::restore_with_key(conn, &note.id, Some(&dek)).expect("restore encrypted note");
    assert!(!restored.is_deleted);
    assert_eq!(restored.title, "Confidential R&D Blueprint");
    assert_eq!(restored.content, "Secret encrypted algorithm details.");

    // Trash again then permanently delete
    TrashService::soft_delete_with_key(conn, &note.id, Some(&dek)).unwrap();
    let perm = TrashService::permanently_delete_note(conn, &storage, &note.id).unwrap();
    assert!(perm.success);
    assert!(NoteRepository::get_by_id(conn, &note.id).unwrap().is_none());
}
