mod common;

use common::TestDatabase;
use personal_notepad_lib::storage::{
    limits::{MAX_NOTEBOOK_NAME_CHARS, MAX_NOTEBOOK_NESTING_DEPTH},
    models::{CreateNoteDto, CreateNotebookDto, UpdateNotebookDto},
    repositories::{NoteRepository, NotebookRepository},
    StorageError,
};
use rusqlite::Connection;
use std::thread;
use std::time::Duration;

fn assert_database_healthy(connection: &Connection) {
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("PRAGMA integrity_check query");
    assert_eq!(integrity, "ok", "SQLite database must pass integrity check");

    let foreign_key_violations: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .expect("foreign key check");
    assert_eq!(
        foreign_key_violations, 0,
        "No foreign key violations allowed"
    );
}

#[test]
fn create_rename_and_query_notebooks_lifecycle() {
    let mut db = TestDatabase::new();
    let conn = db.connection_mut();

    let initial_name = "Engineering Specs";
    let notebook = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: initial_name.to_string(),
            parent_id: None,
        },
    )
    .expect("create notebook must succeed");

    assert!(!notebook.id.is_empty(), "Notebook ID must be generated");
    assert_eq!(notebook.name, initial_name);
    assert_eq!(notebook.parent_id, None);
    assert_eq!(
        notebook.created_at, notebook.modified_at,
        "created_at and modified_at must match on fresh notebook"
    );

    // Sleep briefly to ensure distinct timestamp on rename
    thread::sleep(Duration::from_millis(15));
    let updated_name = "Core Systems Engineering";
    let renamed = NotebookRepository::update(
        conn,
        &notebook.id,
        UpdateNotebookDto {
            name: updated_name.to_string(),
        },
    )
    .expect("rename notebook must succeed");

    assert_eq!(renamed.id, notebook.id);
    assert_eq!(renamed.name, updated_name);
    assert_eq!(renamed.parent_id, None);
    assert_eq!(renamed.created_at, notebook.created_at);
    assert!(
        renamed.modified_at >= notebook.created_at,
        "modified_at must advance on rename"
    );

    // Verify retrieval by ID
    let queried = NotebookRepository::get_by_id(conn, &notebook.id)
        .expect("get notebook by id")
        .expect("notebook must exist");
    assert_eq!(queried, renamed);

    // Verify listing includes the renamed notebook
    let list = NotebookRepository::list(conn).expect("list notebooks");
    assert!(list
        .iter()
        .any(|nb| nb.id == notebook.id && nb.name == updated_name));

    assert_database_healthy(conn);
}

#[test]
fn nested_notebook_hierarchy_depth_and_path_resolution() {
    let mut db = TestDatabase::new();
    let conn = db.connection_mut();

    // Level 1: Root notebook
    let root = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Work".to_string(),
            parent_id: None,
        },
    )
    .expect("create root notebook");
    assert_eq!(NotebookRepository::get_depth(conn, &root.id).unwrap(), 1);
    assert_eq!(
        NotebookRepository::get_hierarchy_path(conn, &root.id).unwrap(),
        "Work"
    );

    // Level 2: Child notebook
    let child = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Projects".to_string(),
            parent_id: Some(root.id.clone()),
        },
    )
    .expect("create child notebook");
    assert_eq!(NotebookRepository::get_depth(conn, &child.id).unwrap(), 2);
    assert_eq!(
        NotebookRepository::get_hierarchy_path(conn, &child.id).unwrap(),
        "Work / Projects"
    );

    // Level 3: Grandchild notebook (boundary limit = MAX_NOTEBOOK_NESTING_DEPTH = 3)
    let grandchild = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Desktop App".to_string(),
            parent_id: Some(child.id.clone()),
        },
    )
    .expect("create grandchild notebook at max depth");
    assert_eq!(
        NotebookRepository::get_depth(conn, &grandchild.id).unwrap(),
        3
    );
    assert_eq!(
        NotebookRepository::get_hierarchy_path(conn, &grandchild.id).unwrap(),
        "Work / Projects / Desktop App"
    );

    // Level 4: Exceeding nesting depth must be rejected
    let overflow_res = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Level 4 Overflow".to_string(),
            parent_id: Some(grandchild.id.clone()),
        },
    );
    match overflow_res {
        Err(StorageError::InvalidHierarchy(msg)) => {
            assert!(
                msg.contains(&format!(
                    "maximum nesting depth of {MAX_NOTEBOOK_NESTING_DEPTH} levels exceeded"
                )),
                "Error must cite nesting depth boundary: {msg}"
            );
        }
        other => panic!("expected InvalidHierarchy on depth overflow, got: {other:?}"),
    }

    assert_database_healthy(conn);
}

#[test]
fn assign_and_move_notes_between_notebooks_and_unfiled() {
    let mut db = TestDatabase::new();
    let conn = db.connection_mut();

    let nb_alpha = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Notebook Alpha".to_string(),
            parent_id: None,
        },
    )
    .expect("create notebook alpha");

    let nb_beta = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Notebook Beta".to_string(),
            parent_id: None,
        },
    )
    .expect("create notebook beta");

    // 1. Create Note 1 assigned to Alpha
    let note1 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Note in Alpha".to_string()),
            content: Some("Original content".to_string()),
            notebook_id: Some(nb_alpha.id.clone()),
            ..Default::default()
        },
    )
    .expect("create note 1 in alpha");
    assert_eq!(note1.notebook_id, Some(nb_alpha.id.clone()));

    // 2. Create Note 2 unfiled (notebook_id: None)
    let note2 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Unfiled Note".to_string()),
            content: Some("Loose note".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .expect("create note 2 unfiled");
    assert_eq!(note2.notebook_id, None);

    // 3. Move Note 1 from Alpha to Beta
    thread::sleep(Duration::from_millis(15));
    let moved_to_beta = NoteRepository::move_to_notebook(conn, &note1.id, Some(&nb_beta.id))
        .expect("move note 1 to beta");
    assert_eq!(moved_to_beta.notebook_id, Some(nb_beta.id.clone()));
    assert_eq!(moved_to_beta.title, note1.title);
    assert_eq!(moved_to_beta.content, note1.content);
    assert!(moved_to_beta.modified_at >= note1.modified_at);

    // Verify in database
    let read_moved = NoteRepository::get_by_id(conn, &note1.id)
        .expect("query note 1")
        .expect("note 1 must exist");
    assert_eq!(read_moved.notebook_id, Some(nb_beta.id.clone()));

    // 4. Move Note 1 from Beta to Unfiled (notebook_id: None)
    let moved_to_unfiled =
        NoteRepository::move_to_notebook(conn, &note1.id, None).expect("move note 1 to unfiled");
    assert_eq!(moved_to_unfiled.notebook_id, None);

    let read_unfiled = NoteRepository::get_by_id(conn, &note1.id)
        .expect("query note 1")
        .expect("note 1 must exist");
    assert_eq!(read_unfiled.notebook_id, None);

    // 5. Move Note 2 from Unfiled to Alpha
    let note2_to_alpha = NoteRepository::move_to_notebook(conn, &note2.id, Some(&nb_alpha.id))
        .expect("move note 2 to alpha");
    assert_eq!(note2_to_alpha.notebook_id, Some(nb_alpha.id.clone()));

    // 6. Invariant: Attempting to move note to non-existent notebook must be rejected safely
    let missing_nb_id = "00000000-0000-0000-0000-000000000000";
    let invalid_move = NoteRepository::move_to_notebook(conn, &note1.id, Some(missing_nb_id));
    match invalid_move {
        Err(StorageError::Validation(msg)) => {
            assert!(msg.contains("does not exist"));
        }
        other => {
            panic!("expected Validation error when moving to missing notebook, got: {other:?}")
        }
    }

    // Verify note 1 assignment was not corrupted by the failed move
    let read_after_invalid = NoteRepository::get_by_id(conn, &note1.id)
        .expect("query note 1")
        .expect("note 1 must exist");
    assert_eq!(read_after_invalid.notebook_id, None);

    assert_database_healthy(conn);
}

#[test]
fn move_notebook_and_defend_against_cycles_and_invalid_hierarchies() {
    let mut db = TestDatabase::new();
    let conn = db.connection_mut();

    let root_a = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Tree A".to_string(),
            parent_id: None,
        },
    )
    .expect("create tree a");

    let root_b = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Tree B".to_string(),
            parent_id: None,
        },
    )
    .expect("create tree b");

    let child = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Subproject".to_string(),
            parent_id: Some(root_a.id.clone()),
        },
    )
    .expect("create subproject under tree a");

    let grandchild = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Module".to_string(),
            parent_id: Some(child.id.clone()),
        },
    )
    .expect("create module under subproject");

    // 1. Valid move: Move Child from Tree A to Tree B
    let moved_child = NotebookRepository::move_notebook(conn, &child.id, Some(&root_b.id))
        .expect("move child to root b");
    assert_eq!(moved_child.parent_id, Some(root_b.id.clone()));
    assert_eq!(
        NotebookRepository::get_hierarchy_path(conn, &child.id).unwrap(),
        "Tree B / Subproject"
    );
    assert_eq!(
        NotebookRepository::get_hierarchy_path(conn, &grandchild.id).unwrap(),
        "Tree B / Subproject / Module"
    );

    // 2. Valid move: Promote Child to root level (parent_id: None)
    let promoted =
        NotebookRepository::move_notebook(conn, &child.id, None).expect("promote child to root");
    assert_eq!(promoted.parent_id, None);
    assert_eq!(
        NotebookRepository::get_hierarchy_path(conn, &child.id).unwrap(),
        "Subproject"
    );
    assert_eq!(
        NotebookRepository::get_hierarchy_path(conn, &grandchild.id).unwrap(),
        "Subproject / Module"
    );

    // 3. Invariant: Self-parenting must fail
    let self_parent = NotebookRepository::move_notebook(conn, &child.id, Some(&child.id));
    match self_parent {
        Err(StorageError::InvalidHierarchy(msg)) => {
            assert!(msg.contains("A notebook cannot be its own parent"));
        }
        other => panic!("expected InvalidHierarchy for self-parenting, got: {other:?}"),
    }

    // 4. Invariant: Non-existent target parent must fail
    let ghost_parent = NotebookRepository::move_notebook(conn, &child.id, Some("ghost-nb-id"));
    match ghost_parent {
        Err(StorageError::InvalidHierarchy(msg)) => {
            assert!(msg.contains("does not exist"));
        }
        other => panic!("expected InvalidHierarchy for non-existent parent, got: {other:?}"),
    }

    // 5. Invariant: Cycle prevention: Cannot move a parent into its own descendant
    // Subproject has child Module. Attempting to move Subproject into Module would form cycle.
    let cycle_attempt = NotebookRepository::move_notebook(conn, &child.id, Some(&grandchild.id));
    match cycle_attempt {
        Err(StorageError::InvalidHierarchy(msg)) => {
            assert!(
                msg.contains("hierarchy cycle"),
                "Must reject cycle attempt: {msg}"
            );
        }
        other => panic!("expected InvalidHierarchy on cycle attempt, got: {other:?}"),
    }

    // 6. Invariant: Depth overflow via move
    // Move Subproject (height 2 with Module) under Tree A (depth 1) -> Total depth 3 (OK)
    let re_nested = NotebookRepository::move_notebook(conn, &child.id, Some(&root_a.id))
        .expect("re-nest under tree a");
    assert_eq!(re_nested.parent_id, Some(root_a.id.clone()));

    // Create another deep node under Module -> Module is now level 3
    // Trying to move a 2-level subtree under a level-2 parent would exceed depth 3
    let deep_target = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Deep Target".to_string(),
            parent_id: Some(root_b.id.clone()),
        },
    )
    .expect("create deep target under root b");

    // Subproject has height 2 (Subproject -> Module). Moving Subproject under deep_target (depth 2)
    // would produce depth 2 + 2 = 4 (> 3) -> MUST FAIL!
    let depth_overflow_move =
        NotebookRepository::move_notebook(conn, &child.id, Some(&deep_target.id));
    match depth_overflow_move {
        Err(StorageError::InvalidHierarchy(msg)) => {
            assert!(
                msg.contains("exceed the maximum nesting depth"),
                "Must reject move that overflows depth: {msg}"
            );
        }
        other => panic!("expected InvalidHierarchy on depth overflow move, got: {other:?}"),
    }

    assert_database_healthy(conn);
}

#[test]
fn delete_empty_notebook_and_delete_notebook_with_notes_unfiles_notes_safely() {
    let mut db = TestDatabase::new();
    let conn = db.connection_mut();

    // 1. Delete empty notebook
    let empty_nb = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Empty Notebook".to_string(),
            parent_id: None,
        },
    )
    .expect("create empty notebook");

    let deleted = NotebookRepository::delete(conn, &empty_nb.id).expect("delete empty notebook");
    assert!(deleted);
    assert!(NotebookRepository::get_by_id(conn, &empty_nb.id)
        .unwrap()
        .is_none());

    // 2. Delete notebook containing multiple notes
    let parent_nb = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Department Notes".to_string(),
            parent_id: None,
        },
    )
    .expect("create department notebook");

    let note_a = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Q1 Strategy".to_string()),
            content: Some("Strategy documentation".to_string()),
            notebook_id: Some(parent_nb.id.clone()),
            ..Default::default()
        },
    )
    .expect("create note a");

    let note_b = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Q2 Roadmap".to_string()),
            content: Some("Roadmap documentation".to_string()),
            notebook_id: Some(parent_nb.id.clone()),
            ..Default::default()
        },
    )
    .expect("create note b");

    // Delete the parent notebook
    let nb_deleted =
        NotebookRepository::delete(conn, &parent_nb.id).expect("delete notebook containing notes");
    assert!(nb_deleted);
    assert!(NotebookRepository::get_by_id(conn, &parent_nb.id)
        .unwrap()
        .is_none());

    // CRITICAL SPEC INVARIANT: Notes MUST NOT be deleted or moved to trash!
    // Their notebook_id must be safely set to NULL (unfiled) with content completely intact.
    let preserved_a = NoteRepository::get_active_by_id(conn, &note_a.id)
        .expect("query note a")
        .expect("note a must survive notebook deletion");
    assert_eq!(
        preserved_a.notebook_id, None,
        "Note A notebook_id must be safely set to None (unfiled)"
    );
    assert_eq!(preserved_a.title, "Q1 Strategy");
    assert_eq!(preserved_a.content, "Strategy documentation");
    assert!(!preserved_a.is_deleted);

    let preserved_b = NoteRepository::get_active_by_id(conn, &note_b.id)
        .expect("query note b")
        .expect("note b must survive notebook deletion");
    assert_eq!(
        preserved_b.notebook_id, None,
        "Note B notebook_id must be safely set to None (unfiled)"
    );
    assert_eq!(preserved_b.title, "Q2 Roadmap");
    assert_eq!(preserved_b.content, "Roadmap documentation");
    assert!(!preserved_b.is_deleted);

    assert_database_healthy(conn);
}

#[test]
fn delete_notebook_is_blocked_when_child_subnotebooks_exist() {
    let mut db = TestDatabase::new();
    let conn = db.connection_mut();

    let parent = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Headquarters".to_string(),
            parent_id: None,
        },
    )
    .expect("create headquarters notebook");

    let child = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Finance Branch".to_string(),
            parent_id: Some(parent.id.clone()),
        },
    )
    .expect("create finance branch");

    // Attempting to delete parent while child exists must fail with DeleteBlocked
    let del_attempt = NotebookRepository::delete(conn, &parent.id);
    match del_attempt {
        Err(StorageError::DeleteBlocked(msg)) => {
            assert!(
                msg.contains("contains sub-notebooks"),
                "Error must indicate sub-notebook blockage: {msg}"
            );
        }
        other => panic!("expected DeleteBlocked error, got: {other:?}"),
    }

    // Both notebooks must remain completely intact
    assert!(NotebookRepository::get_by_id(conn, &parent.id)
        .unwrap()
        .is_some());
    assert!(NotebookRepository::get_by_id(conn, &child.id)
        .unwrap()
        .is_some());

    // Once the child is deleted, deleting the parent must succeed
    NotebookRepository::delete(conn, &child.id).expect("delete child first");
    assert!(NotebookRepository::get_by_id(conn, &child.id)
        .unwrap()
        .is_none());

    let parent_deleted = NotebookRepository::delete(conn, &parent.id).expect("delete parent now");
    assert!(parent_deleted);
    assert!(NotebookRepository::get_by_id(conn, &parent.id)
        .unwrap()
        .is_none());

    assert_database_healthy(conn);
}

#[test]
fn notebook_assignments_and_hierarchy_persist_across_database_reopen() {
    let mut db = TestDatabase::new();

    let root_name = "Global Architecture";
    let child_name = "Storage Engine";

    let (root_id, child_id, note_root_id, note_child_id, note_unfiled_id) = {
        let conn = db.connection_mut();
        let root = NotebookRepository::create(
            conn,
            CreateNotebookDto {
                name: root_name.to_string(),
                parent_id: None,
            },
        )
        .expect("create root");

        let child = NotebookRepository::create(
            conn,
            CreateNotebookDto {
                name: child_name.to_string(),
                parent_id: Some(root.id.clone()),
            },
        )
        .expect("create child");

        let note_r = NoteRepository::create(
            conn,
            CreateNoteDto {
                title: Some("Root Overview Note".to_string()),
                notebook_id: Some(root.id.clone()),
                ..Default::default()
            },
        )
        .expect("create note in root");

        let note_c = NoteRepository::create(
            conn,
            CreateNoteDto {
                title: Some("Child Deep Dive Note".to_string()),
                notebook_id: Some(child.id.clone()),
                ..Default::default()
            },
        )
        .expect("create note in child");

        let note_u = NoteRepository::create(
            conn,
            CreateNoteDto {
                title: Some("Independent Unfiled Note".to_string()),
                notebook_id: None,
                ..Default::default()
            },
        )
        .expect("create unfiled note");

        (root.id, child.id, note_r.id, note_c.id, note_u.id)
    };

    // Close and reopen SQLite database to simulate clean restart
    db.close_and_reopen();
    let conn = db.connection();

    assert_database_healthy(conn);

    // Verify notebooks persisted
    let root_restored = NotebookRepository::get_by_id(conn, &root_id)
        .unwrap()
        .expect("root notebook must exist after reopen");
    assert_eq!(root_restored.name, root_name);
    assert_eq!(root_restored.parent_id, None);

    let child_restored = NotebookRepository::get_by_id(conn, &child_id)
        .unwrap()
        .expect("child notebook must exist after reopen");
    assert_eq!(child_restored.name, child_name);
    assert_eq!(child_restored.parent_id, Some(root_id.clone()));

    // Verify hierarchy path
    assert_eq!(
        NotebookRepository::get_hierarchy_path(conn, &child_id).unwrap(),
        "Global Architecture / Storage Engine"
    );

    // Verify note assignments
    let note_r_restored = NoteRepository::get_by_id(conn, &note_root_id)
        .unwrap()
        .unwrap();
    assert_eq!(note_r_restored.notebook_id, Some(root_id));

    let note_c_restored = NoteRepository::get_by_id(conn, &note_child_id)
        .unwrap()
        .unwrap();
    assert_eq!(note_c_restored.notebook_id, Some(child_id));

    let note_u_restored = NoteRepository::get_by_id(conn, &note_unfiled_id)
        .unwrap()
        .unwrap();
    assert_eq!(note_u_restored.notebook_id, None);
}

#[test]
fn notebook_unicode_special_characters_and_length_limits() {
    let mut db = TestDatabase::new();
    let conn = db.connection_mut();

    let complex_names = [
        "व्यक्तिगत परियोजनाएं 📁",             // Hindi Devanagari
        "日本語ノート 🌸",                  // Japanese
        "مشاريع العمل والأبحاث 📚",          // Arabic RTL
        "Geschäftsbücher & Pläne",          // German umlauts & ampersand
        "2026 — Goals & Targets 🎯",        // Em-dash & numbers
        "Project / Architecture / Spec",    // Forward slashes (path delimiters)
        "Quotes: \"Work\" & 'Life'",        // Single & double quotes
        "SQL: '; DROP TABLE notebooks; --", // SQL injection payload
    ];

    for name in complex_names {
        let created = NotebookRepository::create(
            conn,
            CreateNotebookDto {
                name: name.to_string(),
                parent_id: None,
            },
        )
        .unwrap_or_else(|e| panic!("failed to create notebook '{name}': {e}"));

        assert_eq!(created.name, name);

        let retrieved = NotebookRepository::get_by_id(conn, &created.id)
            .unwrap()
            .expect("notebook must exist");
        assert_eq!(
            retrieved.name, name,
            "Notebook name with complex Unicode / special characters must roundtrip byte-for-byte"
        );
    }

    // 1. Validation failure: Empty notebook name
    let empty_err = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "   ".to_string(),
            parent_id: None,
        },
    );
    match empty_err {
        Err(StorageError::Validation(msg)) => {
            assert!(msg.contains("cannot be empty"));
        }
        other => panic!("expected Validation error for empty notebook name, got: {other:?}"),
    }

    // 2. Validation failure: Notebook name exceeding MAX_NOTEBOOK_NAME_CHARS (200 characters)
    let oversize_name = "N".repeat(MAX_NOTEBOOK_NAME_CHARS + 1);
    let oversize_err = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: oversize_name,
            parent_id: None,
        },
    );
    match oversize_err {
        Err(StorageError::Validation(msg)) => {
            assert!(
                msg.contains(&format!(
                    "maximum allowed limit of {MAX_NOTEBOOK_NAME_CHARS} characters"
                )),
                "Must cite character limit: {msg}"
            );
        }
        other => panic!("expected Validation error for oversize notebook name, got: {other:?}"),
    }

    assert_database_healthy(conn);
}
