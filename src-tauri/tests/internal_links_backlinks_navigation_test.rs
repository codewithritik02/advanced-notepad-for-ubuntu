mod common;

use common::TestDatabase;
use personal_notepad_lib::storage::{
    crypto::generate_dek,
    models::{CreateNoteDto, CreateNotebookDto, LinkTargetLifecycle, UpdateNoteDto},
    repositories::{NoteRepository, NotebookRepository},
    services::{
        links::{InternalLinkService, LinkParser},
        TrashService,
    },
};

/// 1. Creating two notes and inserting an internal link resolves target note accurately.
#[test]
fn test_internal_link_creation_and_resolution() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let notebook = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Engineering".to_string(),
            parent_id: None,
        },
    )
    .expect("create notebook");

    let target_note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("System Architecture Specification".to_string()),
            content: Some("Core system architecture diagrams and constraints.".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(notebook.id.clone()),
            ..Default::default()
        },
    )
    .expect("create target note");

    let link_token = LinkParser::format_link(&target_note.id, Some("Architecture Spec"));
    assert_eq!(
        link_token,
        format!("[Architecture Spec](notepad://note/{})", target_note.id)
    );

    let source_content = format!(
        "Refer to the foundational document: {}\nMake sure to review all sections.",
        link_token
    );

    let source_note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Implementation Plan".to_string()),
            content: Some(source_content),
            format: Some("md".to_string()),
            notebook_id: Some(notebook.id.clone()),
            ..Default::default()
        },
    )
    .expect("create source note");

    // Parse links from source note content
    let parsed = LinkParser::parse_internal_links(&source_note.content);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].target_id, target_note.id);
    assert_eq!(parsed[0].label, "Architecture Spec");
    assert_eq!(parsed[0].raw, link_token);

    // Resolve target note via domain service
    let resolved = InternalLinkService::resolve_link_target(conn, &target_note.id)
        .expect("resolve target note");
    assert_eq!(resolved.target_id, target_note.id);
    assert_eq!(resolved.lifecycle, LinkTargetLifecycle::Active);
    assert_eq!(
        resolved.title.as_deref(),
        Some("System Architecture Specification")
    );
    assert_eq!(resolved.notebook_id.as_deref(), Some(notebook.id.as_str()));
    assert_eq!(resolved.notebook_name.as_deref(), Some("Engineering"));
}

/// 2. Links survive target note renaming because resolution is ID-based.
#[test]
fn test_link_stability_across_target_renaming() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let target_note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("V1 API Specification".to_string()),
            content: Some("Initial API endpoints draft.".to_string()),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("create target note");

    let source_note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Client Integration Guide".to_string()),
            content: Some(format!(
                "Read the docs at [API Docs](notepad://note/{}) before implementing.",
                target_note.id
            )),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("create source note");

    // Rename target note
    let updated_target = NoteRepository::update(
        conn,
        &target_note.id,
        UpdateNoteDto {
            title: Some("V2 High-Performance API Specification".to_string()),
            ..Default::default()
        },
    )
    .expect("rename target note");

    assert_eq!(
        updated_target.title,
        "V2 High-Performance API Specification"
    );
    assert_eq!(updated_target.id, target_note.id);

    // Source note's link URI has not changed, but resolving it reflects the new title
    let resolved = InternalLinkService::resolve_link_target(conn, &target_note.id)
        .expect("resolve renamed target");
    assert_eq!(resolved.lifecycle, LinkTargetLifecycle::Active);
    assert_eq!(
        resolved.title.as_deref(),
        Some("V2 High-Performance API Specification")
    );

    // Backlinks from source still work
    let backlinks = InternalLinkService::get_backlinks(conn, &target_note.id)
        .expect("get backlinks after rename");
    assert_eq!(backlinks.len(), 1);
    assert_eq!(backlinks[0].source_note_id, source_note.id);
    assert_eq!(backlinks[0].source_note_title, "Client Integration Guide");
}

/// 3. Duplicate note titles resolve unambiguously because links target stable UUIDs.
#[test]
fn test_duplicate_titles_resolve_unambiguously_by_uuid() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let nb1 = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Frontend Team".to_string(),
            parent_id: None,
        },
    )
    .expect("create nb1");

    let nb2 = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Backend Team".to_string(),
            parent_id: None,
        },
    )
    .expect("create nb2");

    // Two different notes with identical title "Weekly Standup Notes"
    let note1 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Weekly Standup Notes".to_string()),
            content: Some("React component sprint discussion.".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb1.id.clone()),
            ..Default::default()
        },
    )
    .expect("create note1");

    let note2 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Weekly Standup Notes".to_string()),
            content: Some("SQLite indexing sprint discussion.".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb2.id.clone()),
            ..Default::default()
        },
    )
    .expect("create note2");

    assert_ne!(note1.id, note2.id);
    assert_eq!(note1.title, note2.title);

    // Resolve note 1
    let res1 = InternalLinkService::resolve_link_target(conn, &note1.id).expect("resolve note1");
    assert_eq!(res1.target_id, note1.id);
    assert_eq!(res1.title.as_deref(), Some("Weekly Standup Notes"));
    assert_eq!(res1.notebook_name.as_deref(), Some("Frontend Team"));

    // Resolve note 2
    let res2 = InternalLinkService::resolve_link_target(conn, &note2.id).expect("resolve note2");
    assert_eq!(res2.target_id, note2.id);
    assert_eq!(res2.title.as_deref(), Some("Weekly Standup Notes"));
    assert_eq!(res2.notebook_name.as_deref(), Some("Backend Team"));
}

/// 4. Backlinks reflect newly inserted links and are ordered by modified_at DESC.
#[test]
fn test_backlinks_reflection_and_ordering() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let target = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Target Note Hub".to_string()),
            content: Some("Central topic hub note.".to_string()),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("create target");

    // Initially no backlinks
    let initial_backlinks =
        InternalLinkService::get_backlinks(conn, &target.id).expect("get initial backlinks");
    assert_eq!(initial_backlinks.len(), 0);

    // Source note 1 links to target
    let source1 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Source Alpha".to_string()),
            content: Some(format!(
                "Context before link: [Hub Reference](notepad://note/{}) Context after link.",
                target.id
            )),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("create source1");

    let backlinks_after_s1 =
        InternalLinkService::get_backlinks(conn, &target.id).expect("backlinks after s1");
    assert_eq!(backlinks_after_s1.len(), 1);
    assert_eq!(backlinks_after_s1[0].source_note_id, source1.id);
    assert_eq!(backlinks_after_s1[0].source_note_title, "Source Alpha");
    assert!(backlinks_after_s1[0]
        .label_snippet
        .as_ref()
        .unwrap()
        .contains("Hub Reference"));

    // Source note 2 links to target (created later, modified later)
    let source2 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Source Beta".to_string()),
            content: Some(format!(
                "Another reference to the [Main Hub](notepad://note/{}) from Beta.",
                target.id
            )),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("create source2");

    let backlinks_after_s2 =
        InternalLinkService::get_backlinks(conn, &target.id).expect("backlinks after s2");
    assert_eq!(backlinks_after_s2.len(), 2);

    // Ordered by modified_at DESC: source2 should be first
    assert_eq!(backlinks_after_s2[0].source_note_id, source2.id);
    assert_eq!(backlinks_after_s2[0].source_note_title, "Source Beta");
    assert_eq!(backlinks_after_s2[1].source_note_id, source1.id);
    assert_eq!(backlinks_after_s2[1].source_note_title, "Source Alpha");
}

/// 5. Removing an internal link from source note content immediately updates backlinks.
#[test]
fn test_link_removal_immediately_updates_backlinks() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let target = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Shared Target".to_string()),
            content: Some("Information target.".to_string()),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("create target");

    let source = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Source Document".to_string()),
            content: Some(format!(
                "Linking to [Target](notepad://note/{}) here.",
                target.id
            )),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("create source");

    assert_eq!(
        InternalLinkService::get_backlinks(conn, &target.id)
            .unwrap()
            .len(),
        1
    );

    // Edit source note to remove the link
    NoteRepository::update(
        conn,
        &source.id,
        UpdateNoteDto {
            content: Some("Now this document no longer references any target note.".to_string()),
            ..Default::default()
        },
    )
    .expect("update source to remove link");

    // Backlinks for target must now be empty
    let backlinks =
        InternalLinkService::get_backlinks(conn, &target.id).expect("get backlinks after removal");
    assert_eq!(
        backlinks.len(),
        0,
        "Removed link must immediately disappear from backlinks"
    );
}

/// 6. Broken links and missing targets are handled safely without crashing.
#[test]
fn test_broken_link_and_nonexistent_target_handling() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let nonexistent_id = "nonexistent-uuid-999-000-111";

    // Resolving non-existent target returns typed Missing state
    let resolved = InternalLinkService::resolve_link_target(conn, nonexistent_id)
        .expect("resolve missing target without panic");
    assert_eq!(resolved.target_id, nonexistent_id);
    assert_eq!(resolved.lifecycle, LinkTargetLifecycle::Missing);
    assert_eq!(resolved.title, None);
    assert_eq!(resolved.notebook_id, None);
    assert_eq!(resolved.notebook_name, None);

    // Querying backlinks for nonexistent note returns empty vector safely
    let backlinks = InternalLinkService::get_backlinks(conn, nonexistent_id)
        .expect("get backlinks for nonexistent note");
    assert!(backlinks.is_empty());
}

/// 7. Target note lifecycle transitions: Active -> Trashed -> Restored -> Permanently Deleted.
#[test]
fn test_target_lifecycle_transitions_active_trashed_restored_permanently_deleted() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let target = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Lifecycle Target".to_string()),
            content: Some("Target lifecycle test.".to_string()),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("create target");

    let source = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Referencing Note".to_string()),
            content: Some(format!(
                "Points to [Lifecycle](notepad://note/{})",
                target.id
            )),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("create source");

    // 1. Target is Active
    let res_active = InternalLinkService::resolve_link_target(conn, &target.id).unwrap();
    assert_eq!(res_active.lifecycle, LinkTargetLifecycle::Active);
    assert_eq!(res_active.title.as_deref(), Some("Lifecycle Target"));

    // Backlinks show 1 incoming link
    let backlinks_active = InternalLinkService::get_backlinks(conn, &target.id).unwrap();
    assert_eq!(backlinks_active.len(), 1);

    // 2. Soft-delete target note (move to Trash)
    TrashService::soft_delete(conn, &target.id).expect("soft delete target");

    let res_trashed = InternalLinkService::resolve_link_target(conn, &target.id).unwrap();
    assert_eq!(
        res_trashed.lifecycle,
        LinkTargetLifecycle::Trashed,
        "Trashed target must report Trashed lifecycle"
    );
    assert_eq!(res_trashed.title.as_deref(), Some("Lifecycle Target"));

    // 3. Restore target note from Trash
    TrashService::restore(conn, &target.id).expect("restore target");

    let res_restored = InternalLinkService::resolve_link_target(conn, &target.id).unwrap();
    assert_eq!(
        res_restored.lifecycle,
        LinkTargetLifecycle::Active,
        "Restored target must report Active lifecycle"
    );

    // 4. Source note moved to Trash: source must disappear from target's backlinks
    TrashService::soft_delete(conn, &source.id).expect("soft delete source");
    let backlinks_source_trashed = InternalLinkService::get_backlinks(conn, &target.id).unwrap();
    assert_eq!(
        backlinks_source_trashed.len(),
        0,
        "Trashed source note must be excluded from target's backlinks"
    );

    // Restore source note: reappears in target's backlinks
    TrashService::restore(conn, &source.id).expect("restore source");
    let backlinks_source_restored = InternalLinkService::get_backlinks(conn, &target.id).unwrap();
    assert_eq!(backlinks_source_restored.len(), 1);

    // 5. Permanently delete target note
    TrashService::soft_delete(conn, &target.id).expect("trash target again");
    let paths = test_db.paths().clone();
    let storage = personal_notepad_lib::storage::AttachmentStorage::new(paths);
    TrashService::permanently_delete_note(conn, &storage, &target.id)
        .expect("permanently delete target");

    let res_deleted = InternalLinkService::resolve_link_target(conn, &target.id).unwrap();
    assert_eq!(
        res_deleted.lifecycle,
        LinkTargetLifecycle::Missing,
        "Permanently deleted target must report Missing lifecycle"
    );
    assert_eq!(res_deleted.title, None);
}

/// 8. Self-referencing links are excluded from incoming backlinks to avoid reflexive cycles.
#[test]
fn test_self_referencing_links_excluded_from_backlinks() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let self_note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Recursive Note".to_string()),
            content: Some("Placeholder before self-linking.".to_string()),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("create self note");

    // Add a link pointing to itself
    NoteRepository::update(
        conn,
        &self_note.id,
        UpdateNoteDto {
            content: Some(format!(
                "This note links back to itself: [Self](notepad://note/{}) in the content.",
                self_note.id
            )),
            ..Default::default()
        },
    )
    .expect("update with self link");

    let backlinks = InternalLinkService::get_backlinks(conn, &self_note.id)
        .expect("get backlinks for self note");
    assert_eq!(
        backlinks.len(),
        0,
        "Self-referencing links must be strictly excluded from backlinks"
    );
}

/// 9. Malformed syntax, unsupported protocols, and path traversal defense.
#[test]
fn test_malformed_syntax_and_path_traversal_defense() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    // 1. Markdown image syntax must NOT be parsed as internal navigation link
    let image_content = "Here is an image: ![Diagram](notepad://note/target-uuid-123)";
    let parsed_images = LinkParser::parse_internal_links(image_content);
    assert!(
        parsed_images.is_empty(),
        "Markdown image syntax must not be parsed as internal link"
    );

    // 2. Multiline bracket syntax must NOT be parsed as link
    let multiline_content = "Broken [Label\nwith newline](notepad://note/target-uuid-123)";
    let parsed_multiline = LinkParser::parse_internal_links(multiline_content);
    assert!(parsed_multiline.is_empty());

    // 3. Unclosed bracket or unclosed parenthesis
    let unclosed_bracket = "Broken [Unclosed bracket(notepad://note/target-uuid-123)";
    assert!(LinkParser::parse_internal_links(unclosed_bracket).is_empty());

    let unclosed_paren = "Broken [Unclosed paren](notepad://note/target-uuid-123";
    assert!(LinkParser::parse_internal_links(unclosed_paren).is_empty());

    // 4. External protocols and schemes
    assert!(!LinkParser::is_internal_link_uri(
        "https://example.com/note/123"
    ));
    assert!(!LinkParser::is_internal_link_uri("http://localhost:3000"));
    assert!(!LinkParser::is_internal_link_uri("file:///etc/passwd"));
    assert!(!LinkParser::is_internal_link_uri("data:text/html,test"));
    assert!(LinkParser::is_internal_link_uri(
        "notepad://note/valid-uuid"
    ));
    assert!(LinkParser::is_internal_link_uri(
        "NOTEPAD://NOTE/VALID-UUID"
    ));

    // 5. Path traversal attempts through target IDs
    let malicious_ids = vec![
        "../../etc/passwd",
        "..\\..\\windows\\system32",
        "/etc/shadow",
        "valid/../../traversal",
        "",
        "   ",
        "\t\n",
    ];

    for bad_id in malicious_ids {
        let res =
            InternalLinkService::resolve_link_target(conn, bad_id).expect("resolve bad ID safely");
        assert_eq!(
            res.lifecycle,
            LinkTargetLifecycle::Missing,
            "Malicious target ID '{}' must be resolved as Missing safely",
            bad_id
        );

        let backlinks = InternalLinkService::get_backlinks(conn, bad_id)
            .expect("get backlinks for bad ID safely");
        assert!(backlinks.is_empty());
    }
}

/// 10. Database restart preserves link content, target resolution, and backlink queries.
#[test]
fn test_restart_and_reopen_persists_link_content_and_backlinks() {
    let mut test_db = TestDatabase::new();

    let target_id;
    let source_id;

    {
        let conn = test_db.connection();
        let target = NoteRepository::create(
            conn,
            CreateNoteDto {
                title: Some("Persistent Target Note".to_string()),
                content: Some("Content that will survive restart.".to_string()),
                format: Some("md".to_string()),
                ..Default::default()
            },
        )
        .expect("create target");
        target_id = target.id;

        let source = NoteRepository::create(
            conn,
            CreateNoteDto {
                title: Some("Persistent Source Note".to_string()),
                content: Some(format!(
                    "Link surviving restart: [Target](notepad://note/{})",
                    target_id
                )),
                format: Some("md".to_string()),
                ..Default::default()
            },
        )
        .expect("create source");
        source_id = source.id;

        let backlinks = InternalLinkService::get_backlinks(conn, &target_id).unwrap();
        assert_eq!(backlinks.len(), 1);
    }

    // Close and reopen SQLite database
    test_db.close_and_reopen();

    {
        let conn = test_db.connection();

        // Target note resolves actively
        let res = InternalLinkService::resolve_link_target(conn, &target_id)
            .expect("resolve target after reopen");
        assert_eq!(res.lifecycle, LinkTargetLifecycle::Active);
        assert_eq!(res.title.as_deref(), Some("Persistent Target Note"));

        // Source note content intact
        let source_note = NoteRepository::get_by_id(conn, &source_id)
            .expect("get source note after reopen")
            .expect("source note exists");
        assert!(source_note.content.contains(&target_id));

        // Backlinks query returns identical result
        let backlinks = InternalLinkService::get_backlinks(conn, &target_id)
            .expect("get backlinks after reopen");
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].source_note_id, source_id);
        assert_eq!(backlinks[0].source_note_title, "Persistent Source Note");
    }
}

/// 11. Multilingual Unicode, BiDi text, emojis, and escaped brackets in link labels.
#[test]
fn test_multilingual_unicode_and_special_character_labels() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let target = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("अंतर्राष्ट्रीय दस्तावेज़ 🚀 (Global Spec)".to_string()),
            content: Some("Multilingual documentation content.".to_string()),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("create target");

    // Link with Hindi, Japanese, Arabic, and Emoji labels
    let test_cases = vec![
        ("परियोजना योजना 2026", "Hindi roadmap"),
        ("設計仕様書・アーキテクチャ", "Japanese design spec"),
        ("ملاحظات_المشروع_الهام", "Arabic project notes"),
        ("🚀 Production Launch Plan 🎯", "Emoji label"),
        (r"Feature \[Beta\] Specification", "Escaped brackets label"),
    ];

    for (label, description) in test_cases {
        let safe_label = LinkParser::sanitize_label(label);
        let link_token = LinkParser::format_link(&target.id, Some(&safe_label));
        let content = format!(
            "Preamble for {}: {} Postscript content for boundary test.",
            description, link_token
        );

        let parsed = LinkParser::parse_internal_links(&content);
        assert_eq!(parsed.len(), 1, "Failed parsing link with label: {}", label);
        assert_eq!(parsed[0].target_id, target.id);

        let source = NoteRepository::create(
            conn,
            CreateNoteDto {
                title: Some(format!("Source - {}", description)),
                content: Some(content),
                format: Some("md".to_string()),
                ..Default::default()
            },
        )
        .expect("create source note");

        // Backlink snippet extraction must not panic or slice across UTF-8 char boundaries
        let backlinks = InternalLinkService::get_backlinks(conn, &target.id)
            .expect("get backlinks for Unicode note");
        assert!(
            backlinks.iter().any(|b| b.source_note_id == source.id),
            "Source '{}' missing from backlinks",
            description
        );
    }
}

/// 12. Encrypted notes support link target resolution and backlink queries with active DEK.
#[test]
fn test_encrypted_notes_link_resolution_and_backlinks() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let dek = generate_dek();

    // Create encrypted target note
    let target = NoteRepository::create_with_key(
        conn,
        CreateNoteDto {
            title: Some("Confidential Architecture".to_string()),
            content: Some("Top secret system design.".to_string()),
            format: Some("md".to_string()),
            ..Default::default()
        },
        Some(&dek),
    )
    .expect("create encrypted target");

    // Create encrypted source note linking to target
    let source = NoteRepository::create_with_key(
        conn,
        CreateNoteDto {
            title: Some("Confidential Roadmap".to_string()),
            content: Some(format!(
                "Classified link to [Secret Architecture](notepad://note/{}) here.",
                target.id
            )),
            format: Some("md".to_string()),
            ..Default::default()
        },
        Some(&dek),
    )
    .expect("create encrypted source");

    // Target resolution with active DEK
    let res_with_key =
        InternalLinkService::resolve_link_target_with_key(conn, &target.id, Some(&dek))
            .expect("resolve encrypted target with DEK");
    assert_eq!(res_with_key.lifecycle, LinkTargetLifecycle::Active);
    assert_eq!(
        res_with_key.title.as_deref(),
        Some("Confidential Architecture")
    );

    // Backlinks discovery with active DEK
    let backlinks_with_key =
        InternalLinkService::get_backlinks_with_key(conn, &target.id, Some(&dek))
            .expect("get backlinks with DEK");
    assert_eq!(backlinks_with_key.len(), 1);
    assert_eq!(backlinks_with_key[0].source_note_id, source.id);
    assert_eq!(
        backlinks_with_key[0].source_note_title,
        "Confidential Roadmap"
    );
    assert!(backlinks_with_key[0]
        .label_snippet
        .as_ref()
        .unwrap()
        .contains("Secret Architecture"));

    // Backlinks discovery without DEK does not parse ciphertext as plaintext
    let backlinks_without_key =
        InternalLinkService::get_backlinks(conn, &target.id).expect("get backlinks without DEK");
    // Plaintext query cannot find the link in encrypted ciphertext
    assert_eq!(backlinks_without_key.len(), 0);
}

/// 13. Multiple internal links support fan-out (1 hub -> N targets) and fan-in (N sources -> 1 target).
#[test]
fn test_multiple_links_fan_out_and_fan_in() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    // 1. Fan-out: 1 hub note links to 5 distinct target notes
    let mut targets = Vec::new();
    for i in 1..=5 {
        let t = NoteRepository::create(
            conn,
            CreateNoteDto {
                title: Some(format!("Module {}", i)),
                content: Some(format!("Module {} documentation.", i)),
                format: Some("md".to_string()),
                ..Default::default()
            },
        )
        .expect("create module target");
        targets.push(t);
    }

    let mut hub_content = String::from("# Project Modules Overview\n\n");
    for (i, t) in targets.iter().enumerate() {
        hub_content.push_str(&format!(
            "- [Module {} Spec](notepad://note/{})\n",
            i + 1,
            t.id
        ));
    }

    let hub_note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Project Hub".to_string()),
            content: Some(hub_content),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("create hub note");

    let parsed_hub_links = LinkParser::parse_internal_links(&hub_note.content);
    assert_eq!(parsed_hub_links.len(), 5);

    // Each target note must have exactly 1 incoming backlink from the hub
    for t in &targets {
        let backlinks = InternalLinkService::get_backlinks(conn, &t.id).unwrap();
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].source_note_id, hub_note.id);
        assert_eq!(backlinks[0].source_note_title, "Project Hub");
    }

    // 2. Fan-in: 5 distinct source notes link to a single central note
    let central_note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Central Index".to_string()),
            content: Some("All systems index.".to_string()),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("create central note");

    for i in 1..=5 {
        NoteRepository::create(
            conn,
            CreateNoteDto {
                title: Some(format!("Contributor {}", i)),
                content: Some(format!(
                    "Contribution {} points to [Index](notepad://note/{})",
                    i, central_note.id
                )),
                format: Some("md".to_string()),
                ..Default::default()
            },
        )
        .expect("create contributor note");
    }

    let central_backlinks = InternalLinkService::get_backlinks(conn, &central_note.id).unwrap();
    assert_eq!(central_backlinks.len(), 5);
}
