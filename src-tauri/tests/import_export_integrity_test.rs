mod common;

use std::fs;
use std::path::Path;

use common::TestDatabase;
use personal_notepad_lib::storage::{
    crypto::generate_dek,
    errors::StorageError,
    models::{
        CreateNoteDto, CreateNotebookDto, DuplicatePolicy, ExportBatchRequest, ExportFormat,
        ExportScope, ExportSingleNoteRequest, ImportCandidateOverride, ImportItemStatus,
        ImportOptions, ImportRequest, InterchangeDocument, OperationOutcome, ValidationStatus,
    },
    repositories::{NoteRepository, NotebookRepository},
    services::{ImportExportService, TrashService},
};

/// 1. Valid single-file import for plain text (.txt) and Markdown (.md).
#[test]
fn test_import_single_text_and_markdown_files() {
    let mut test_db = TestDatabase::new();
    let temp_root = test_db.root().join("import_test_single");
    fs::create_dir_all(&temp_root).expect("create temp import dir");

    let txt_path = temp_root.join("MeetingNotes.txt");
    fs::write(
        &txt_path,
        "Discussion on Q4 roadmap and delivery goals.\nSecond line notes.",
    )
    .expect("write txt");

    let md_path = temp_root.join("ArchitectureGuide.md");
    let md_content =
        "# System Design\n\n- Component A\n- Component B\n\n```rust\nfn main() {}\n```\n";
    fs::write(&md_path, md_content).expect("write md");

    let paths = vec![
        txt_path.to_string_lossy().to_string(),
        md_path.to_string_lossy().to_string(),
    ];

    // Scan candidates
    let preview = {
        let conn = test_db.connection();
        ImportExportService::scan_candidates(conn, &paths).expect("scan candidates")
    };
    assert_eq!(preview.total_files, 2);
    assert_eq!(preview.valid_files, 2);
    assert_eq!(preview.invalid_files, 0);

    // Execute import
    let summary = {
        let conn = test_db.connection_mut();
        ImportExportService::execute_import(
            conn,
            ImportRequest {
                operation_id: None,
                candidates: preview
                    .candidates
                    .into_iter()
                    .map(|c| ImportCandidateOverride {
                        id: c.id,
                        file_path: c.file_path,
                        custom_title: None,
                        selected: true,
                    })
                    .collect(),
                options: ImportOptions {
                    target_notebook_id: None,
                    duplicate_policy: DuplicatePolicy::ImportAsNew,
                },
            },
        )
        .expect("execute import")
    };

    assert_eq!(summary.imported_count, 2);
    assert_eq!(summary.failed_count, 0);
    assert_eq!(summary.skipped_count, 0);

    // Verify notes in database
    let conn = test_db.connection();
    let notes = NoteRepository::list_active(conn).expect("list active notes");
    assert_eq!(notes.len(), 2);

    let txt_note = notes
        .iter()
        .find(|n| n.title == "MeetingNotes")
        .expect("find txt note");
    assert_eq!(txt_note.format, "txt");
    assert!(txt_note.content.contains("Discussion on Q4 roadmap"));

    let md_note = notes
        .iter()
        .find(|n| n.title == "ArchitectureGuide")
        .expect("find md note");
    assert_eq!(md_note.format, "md");
    assert_eq!(md_note.content, md_content);
}

/// 2. Valid multi-file batch import assigned to target notebook.
#[test]
fn test_import_batch_with_target_notebook_assignment() {
    let mut test_db = TestDatabase::new();
    let temp_root = test_db.root().join("import_test_batch");
    fs::create_dir_all(&temp_root).expect("create temp dir");

    let target_nb = {
        let conn = test_db.connection();
        NotebookRepository::create(
            conn,
            CreateNotebookDto {
                name: "Imported Specifications".to_string(),
                parent_id: None,
            },
        )
        .expect("create notebook")
    };

    let mut paths = Vec::new();
    for i in 1..=5 {
        let p = temp_root.join(format!("Module_{}.md", i));
        fs::write(&p, format!("# Module {}\nSpecification body.", i)).expect("write module");
        paths.push(p.to_string_lossy().to_string());
    }

    let preview = {
        let conn = test_db.connection();
        ImportExportService::scan_candidates(conn, &paths).expect("scan batch")
    };
    assert_eq!(preview.valid_files, 5);

    let summary = {
        let conn = test_db.connection_mut();
        ImportExportService::execute_import(
            conn,
            ImportRequest {
                operation_id: None,
                candidates: preview
                    .candidates
                    .into_iter()
                    .map(|c| ImportCandidateOverride {
                        id: c.id,
                        file_path: c.file_path,
                        custom_title: None,
                        selected: true,
                    })
                    .collect(),
                options: ImportOptions {
                    target_notebook_id: Some(target_nb.id.clone()),
                    duplicate_policy: DuplicatePolicy::ImportAsNew,
                },
            },
        )
        .expect("execute import batch")
    };

    assert_eq!(summary.imported_count, 5);

    // Verify all 5 notes are filed into target notebook
    let conn = test_db.connection();
    let nb_notes = NoteRepository::list_by_notebook(conn, &target_nb.id).expect("list by notebook");
    assert_eq!(nb_notes.len(), 5);
    for n in &nb_notes {
        assert_eq!(n.notebook_id.as_deref(), Some(target_nb.id.as_str()));
    }

    // Attempting import with non-existent target notebook fails safely
    let bad_req = ImportRequest {
        operation_id: None,
        candidates: vec![],
        options: ImportOptions {
            target_notebook_id: Some("non-existent-notebook-uuid".to_string()),
            duplicate_policy: DuplicatePolicy::ImportAsNew,
        },
    };
    let err = ImportExportService::execute_import(test_db.connection_mut(), bad_req);
    assert!(matches!(err, Err(StorageError::Validation(_))));
}

/// 3. Empty input files (0 bytes) are supported and create notes with empty content.
#[test]
fn test_import_empty_files_supported() {
    let mut test_db = TestDatabase::new();
    let temp_root = test_db.root().join("import_test_empty");
    fs::create_dir_all(&temp_root).expect("create temp dir");

    let empty_txt = temp_root.join("EmptyDocument.txt");
    fs::write(&empty_txt, b"").expect("write empty txt");

    let empty_md = temp_root.join("EmptyScratchpad.md");
    fs::write(&empty_md, b"").expect("write empty md");

    let paths = vec![
        empty_txt.to_string_lossy().to_string(),
        empty_md.to_string_lossy().to_string(),
    ];

    let preview = {
        let conn = test_db.connection();
        ImportExportService::scan_candidates(conn, &paths).expect("scan empty files")
    };
    assert_eq!(preview.valid_files, 2);
    assert_eq!(preview.total_size_bytes, 0);

    let summary = {
        let conn = test_db.connection_mut();
        ImportExportService::execute_import(
            conn,
            ImportRequest {
                operation_id: None,
                candidates: preview
                    .candidates
                    .into_iter()
                    .map(|c| ImportCandidateOverride {
                        id: c.id,
                        file_path: c.file_path,
                        custom_title: None,
                        selected: true,
                    })
                    .collect(),
                options: ImportOptions {
                    target_notebook_id: None,
                    duplicate_policy: DuplicatePolicy::ImportAsNew,
                },
            },
        )
        .expect("import empty files")
    };

    assert_eq!(summary.imported_count, 2);

    let conn = test_db.connection();
    let notes = NoteRepository::list_active(conn).expect("list active");
    assert_eq!(notes.len(), 2);
    for n in notes {
        assert_eq!(n.content, "");
    }
}

/// 4. Multilingual Unicode content and filenames preserve fidelity.
#[test]
fn test_import_multilingual_unicode_content_and_filenames() {
    let mut test_db = TestDatabase::new();
    let temp_root = test_db.root().join("import_test_unicode");
    fs::create_dir_all(&temp_root).expect("create temp dir");

    let test_cases = vec![
        (
            "परियोजना_योजना.md",
            "# परियोजना रूपरेखा\n\nमहत्वपूर्ण मील के पत्थर।",
            "md",
        ),
        (
            "日本語_仕様書.txt",
            "アーキテクチャ概要\nマイクロサービス設計。",
            "txt",
        ),
        ("ملاحظات_المشروع.md", "# وثائق المشروع\n\nتفاصيل مهمة.", "md"),
        (
            "Accents_Café_Élégant.md",
            "# Café & Résumé\n\nDélicieux et soigné.",
            "md",
        ),
        (
            "🚀_Launch_Roadmap_🎯.txt",
            "Launch Day Checklist: 100% Ready 🌟",
            "txt",
        ),
    ];

    let mut paths = Vec::new();
    for (name, content, _) in &test_cases {
        let p = temp_root.join(name);
        fs::write(&p, content.as_bytes()).expect("write unicode file");
        paths.push(p.to_string_lossy().to_string());
    }

    let preview = {
        let conn = test_db.connection();
        ImportExportService::scan_candidates(conn, &paths).expect("scan unicode files")
    };
    assert_eq!(preview.valid_files, 5);

    let summary = {
        let conn = test_db.connection_mut();
        ImportExportService::execute_import(
            conn,
            ImportRequest {
                operation_id: None,
                candidates: preview
                    .candidates
                    .into_iter()
                    .map(|c| ImportCandidateOverride {
                        id: c.id,
                        file_path: c.file_path,
                        custom_title: None,
                        selected: true,
                    })
                    .collect(),
                options: ImportOptions {
                    target_notebook_id: None,
                    duplicate_policy: DuplicatePolicy::ImportAsNew,
                },
            },
        )
        .expect("execute unicode import")
    };

    assert_eq!(summary.imported_count, 5);

    let conn = test_db.connection();
    let notes = NoteRepository::list_active(conn).expect("list active");
    assert_eq!(notes.len(), 5);

    for (name, expected_content, expected_format) in test_cases {
        let stem = Path::new(name).file_stem().unwrap().to_str().unwrap();
        let note = notes
            .iter()
            .find(|n| n.title == stem)
            .unwrap_or_else(|| panic!("Note with title '{}' not found", stem));
        assert_eq!(note.format, expected_format);
        assert_eq!(note.content, expected_content);
    }
}

/// 5. Oversized files exceeding MAX_INDIVIDUAL_FILE_SIZE (10 MB) are rejected safely.
#[test]
fn test_import_file_size_limits_and_oversized_rejection() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let temp_root = test_db.root().join("import_test_limits");
    fs::create_dir_all(&temp_root).expect("create temp dir");

    // File slightly larger than 10MB limit: 10 * 1024 * 1024 + 1024
    let oversized_path = temp_root.join("OversizedDoc.txt");
    let file = fs::File::create(&oversized_path).expect("create oversized file");
    file.set_len(10 * 1024 * 1024 + 1024)
        .expect("set oversized len");

    let paths = vec![oversized_path.to_string_lossy().to_string()];
    let preview = ImportExportService::scan_candidates(conn, &paths).expect("scan oversized file");

    assert_eq!(preview.total_files, 1);
    assert_eq!(preview.valid_files, 0);
    assert_eq!(preview.invalid_files, 1);
    assert_eq!(
        preview.candidates[0].validation_status,
        ValidationStatus::Invalid
    );
    assert!(preview.candidates[0]
        .error_message
        .as_ref()
        .unwrap()
        .contains("exceeds"));
}

/// 6. Unsupported file extensions and invalid binary encoding are rejected safely.
#[test]
fn test_import_unsupported_extensions_and_invalid_encoding() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let temp_root = test_db.root().join("import_test_unsupported");
    fs::create_dir_all(&temp_root).expect("create temp dir");

    let bin_path = temp_root.join("binary_blob.bin");
    fs::write(&bin_path, &[0xDE, 0xAD, 0xBE, 0xEF]).expect("write bin");

    let exe_path = temp_root.join("executable.exe");
    fs::write(&exe_path, &[0x4D, 0x5A, 0x90, 0x00]).expect("write exe");

    let pdf_path = temp_root.join("manual.pdf");
    fs::write(&pdf_path, b"%PDF-1.4").expect("write pdf");

    // File with .txt extension but invalid non-UTF8 bytes
    let bad_utf8_path = temp_root.join("invalid_utf8.txt");
    fs::write(&bad_utf8_path, &[0xFF, 0xFE, 0xFD, 0xFC]).expect("write bad utf8");

    let paths = vec![
        bin_path.to_string_lossy().to_string(),
        exe_path.to_string_lossy().to_string(),
        pdf_path.to_string_lossy().to_string(),
        bad_utf8_path.to_string_lossy().to_string(),
    ];

    let preview = ImportExportService::scan_candidates(conn, &paths).expect("scan unsupported");
    assert_eq!(preview.total_files, 4);
    assert_eq!(preview.valid_files, 0);
    assert_eq!(preview.invalid_files, 4);

    for c in preview.candidates {
        assert_eq!(c.validation_status, ValidationStatus::Invalid);
        assert!(c.error_message.is_some());
    }
}

/// 7. Duplicate note conflict handling: SkipExact and ImportAsNew policies, and deselection.
#[test]
fn test_duplicate_note_policies_skip_and_import_as_new() {
    let mut test_db = TestDatabase::new();
    let temp_root = test_db.root().join("import_test_duplicates");
    fs::create_dir_all(&temp_root).expect("create temp dir");

    // Existing active note
    let existing_note = {
        let conn = test_db.connection();
        NoteRepository::create(
            conn,
            CreateNoteDto {
                title: Some("Sprint Brief".to_string()),
                content: Some("Original sprint brief content.".to_string()),
                format: Some("md".to_string()),
                ..Default::default()
            },
        )
        .expect("create existing note")
    };

    let candidate_file = temp_root.join("Sprint Brief.md");
    fs::write(&candidate_file, "Original sprint brief content.")
        .expect("write exact duplicate file");

    let paths = vec![candidate_file.to_string_lossy().to_string()];

    // Scan detects duplicate
    let preview = {
        let conn = test_db.connection();
        ImportExportService::scan_candidates(conn, &paths).expect("scan duplicate")
    };
    assert_eq!(preview.duplicate_files, 1);
    assert!(preview.candidates[0].is_duplicate);

    // 1. Policy: SkipExact -> Skips import without changing existing note
    {
        let conn = test_db.connection_mut();
        let summary_skip = ImportExportService::execute_import(
            conn,
            ImportRequest {
                operation_id: None,
                candidates: vec![ImportCandidateOverride {
                    id: preview.candidates[0].id.clone(),
                    file_path: candidate_file.to_string_lossy().to_string(),
                    custom_title: None,
                    selected: true,
                }],
                options: ImportOptions {
                    target_notebook_id: None,
                    duplicate_policy: DuplicatePolicy::SkipExact,
                },
            },
        )
        .expect("execute skip duplicate");

        assert_eq!(summary_skip.imported_count, 0);
        assert_eq!(summary_skip.skipped_count, 1);
        assert_eq!(summary_skip.results[0].status, ImportItemStatus::Skipped);
    }

    // 2. Policy: ImportAsNew -> Imports as new note with distinct UUID
    {
        let conn = test_db.connection_mut();
        let summary_new = ImportExportService::execute_import(
            conn,
            ImportRequest {
                operation_id: None,
                candidates: vec![ImportCandidateOverride {
                    id: preview.candidates[0].id.clone(),
                    file_path: candidate_file.to_string_lossy().to_string(),
                    custom_title: None,
                    selected: true,
                }],
                options: ImportOptions {
                    target_notebook_id: None,
                    duplicate_policy: DuplicatePolicy::ImportAsNew,
                },
            },
        )
        .expect("execute import as new");

        assert_eq!(summary_new.imported_count, 1);
        assert_ne!(
            summary_new.results[0].created_note_id.as_deref(),
            Some(existing_note.id.as_str())
        );

        let count = NoteRepository::count_active(conn).unwrap();
        assert_eq!(count, 2);
    }

    // 3. User deselection: candidate marked selected = false is skipped
    {
        let conn = test_db.connection_mut();
        let summary_deselected = ImportExportService::execute_import(
            conn,
            ImportRequest {
                operation_id: None,
                candidates: vec![ImportCandidateOverride {
                    id: preview.candidates[0].id.clone(),
                    file_path: candidate_file.to_string_lossy().to_string(),
                    custom_title: None,
                    selected: false,
                }],
                options: ImportOptions {
                    target_notebook_id: None,
                    duplicate_policy: DuplicatePolicy::ImportAsNew,
                },
            },
        )
        .expect("execute deselected import");

        assert_eq!(summary_deselected.imported_count, 0);
        assert_eq!(summary_deselected.skipped_count, 1);
        assert!(summary_deselected.results[0]
            .message
            .as_ref()
            .unwrap()
            .contains("Deselected"));
    }
}

/// 8. Single note export across TXT, MD, JSON with destination overwrite control.
#[test]
fn test_export_single_note_formats_and_overwrite_policy() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let temp_root = test_db.root().join("export_test_single");
    fs::create_dir_all(&temp_root).expect("create temp export dir");

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Project Specification".to_string()),
            content: Some(
                "# Title\n\nBody paragraph with [Link](notepad://note/target-uuid).".to_string(),
            ),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    // 1. Export as Markdown (.md)
    let md_dest = temp_root.join("spec.md");
    let res_md = ImportExportService::export_single_note(
        conn,
        ExportSingleNoteRequest {
            note_id: note.id.clone(),
            destination_path: md_dest.to_string_lossy().to_string(),
            format: ExportFormat::Md,
            overwrite: false,
        },
    )
    .expect("export md");
    assert!(md_dest.exists());
    assert_eq!(res_md.file_path, md_dest.to_string_lossy());
    let md_text = fs::read_to_string(&md_dest).unwrap();
    assert_eq!(md_text, note.content);

    // 2. Export with overwrite = false when file exists fails with Conflict
    let err_conflict = ImportExportService::export_single_note(
        conn,
        ExportSingleNoteRequest {
            note_id: note.id.clone(),
            destination_path: md_dest.to_string_lossy().to_string(),
            format: ExportFormat::Md,
            overwrite: false,
        },
    );
    assert!(matches!(err_conflict, Err(StorageError::Conflict(_))));

    // 3. Export with overwrite = true succeeds
    let res_overwrite = ImportExportService::export_single_note(
        conn,
        ExportSingleNoteRequest {
            note_id: note.id.clone(),
            destination_path: md_dest.to_string_lossy().to_string(),
            format: ExportFormat::Md,
            overwrite: true,
        },
    );
    assert!(res_overwrite.is_ok());

    // 4. Export as JSON interchange document (.json)
    let json_dest = temp_root.join("spec.json");
    ImportExportService::export_single_note(
        conn,
        ExportSingleNoteRequest {
            note_id: note.id.clone(),
            destination_path: json_dest.to_string_lossy().to_string(),
            format: ExportFormat::Json,
            overwrite: false,
        },
    )
    .expect("export json");

    let json_text = fs::read_to_string(&json_dest).unwrap();
    let doc: InterchangeDocument = serde_json::from_str(&json_text).expect("parse json");
    assert_eq!(doc.app, "personal-notepad");
    assert_eq!(doc.note_count, 1);
    assert_eq!(doc.notes[0].title, "Project Specification");
}

/// 9. Batch export handles notebook hierarchy, duplicate filename disambiguation, and excludes trashed notes.
#[test]
fn test_batch_export_notebook_hierarchy_and_name_collisions() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let temp_root = test_db.root().join("export_test_batch");
    fs::create_dir_all(&temp_root).expect("create temp dir");

    let nb = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Finance".to_string(),
            parent_id: None,
        },
    )
    .expect("create notebook");

    // Note 1 in notebook
    let _note1 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Audit Report".to_string()),
            content: Some("First audit report.".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb.id.clone()),
            ..Default::default()
        },
    )
    .expect("create note1");

    // Note 2 in same notebook with identical title -> collision candidate
    let _note2 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Audit Report".to_string()),
            content: Some("Second audit report with same title.".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb.id.clone()),
            ..Default::default()
        },
    )
    .expect("create note2");

    // Trashed note in same notebook -> must be excluded from export
    let trashed = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Old Audit".to_string()),
            content: Some("Trashed note content.".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb.id.clone()),
            ..Default::default()
        },
    )
    .expect("create trashed note");
    TrashService::soft_delete(conn, &trashed.id).expect("soft delete");

    let export_dir = temp_root.join("batch_out");
    let summary = ImportExportService::export_notes_batch(
        conn,
        ExportBatchRequest {
            operation_id: None,
            scope: Some(ExportScope::Notebook {
                notebook_id: nb.id.clone(),
                include_nested: true,
            }),
            note_ids: vec![],
            format: ExportFormat::Md,
            destination_dir: export_dir.to_string_lossy().to_string(),
            destination_file: None,
            include_metadata: true,
            overwrite: false,
        },
    )
    .expect("export batch");

    assert_eq!(summary.exported_count, 2);
    assert_eq!(summary.skipped_count, 1); // 1 trashed note skipped

    // Verify collision resolution with (1) suffix
    assert!(export_dir.join("Audit Report.md").exists());
    assert!(export_dir.join("Audit Report (1).md").exists());
    assert!(!export_dir.join("Old Audit.md").exists());
}

/// 10. Content round-tripping: exporting note to disk and importing it back preserves content verbatim.
#[test]
fn test_content_roundtrip_markdown_and_json() {
    let mut test_db = TestDatabase::new();
    let temp_root = test_db.root().join("roundtrip_test");
    fs::create_dir_all(&temp_root).expect("create temp dir");

    let complex_markdown = r#"# Comprehensive Specification

| Parameter | Type | Required | Description |
|---|---|---|---|
| `id` | UUID | Yes | Primary key identifier |
| `name` | String | Yes | Unicode entity title |

- [x] Initial design validated
- [ ] Production testing underway

```rust
pub fn calculate_hash(data: &[u8]) -> [u8; 32] {
    use sha2::{Sha256, Digest};
    Sha256::digest(data).into()
}
```

See [System Diagram](notepad://note/sys-123) for details.
"#;

    let original_note = {
        let conn = test_db.connection();
        NoteRepository::create(
            conn,
            CreateNoteDto {
                title: Some("Complex Roundtrip Note".to_string()),
                content: Some(complex_markdown.to_string()),
                format: Some("md".to_string()),
                ..Default::default()
            },
        )
        .expect("create original note")
    };

    // Export note
    let export_file = temp_root.join("roundtrip_export.md");
    {
        let conn = test_db.connection();
        ImportExportService::export_single_note(
            conn,
            ExportSingleNoteRequest {
                note_id: original_note.id.clone(),
                destination_path: export_file.to_string_lossy().to_string(),
                format: ExportFormat::Md,
                overwrite: true,
            },
        )
        .expect("export for roundtrip");
    }

    // Import exported file back
    let paths = vec![export_file.to_string_lossy().to_string()];
    let preview = {
        let conn = test_db.connection();
        ImportExportService::scan_candidates(conn, &paths).expect("scan exported file")
    };
    assert_eq!(preview.valid_files, 1);

    let summary = {
        let conn = test_db.connection_mut();
        ImportExportService::execute_import(
            conn,
            ImportRequest {
                operation_id: None,
                candidates: vec![ImportCandidateOverride {
                    id: preview.candidates[0].id.clone(),
                    file_path: export_file.to_string_lossy().to_string(),
                    custom_title: Some("Reimported Roundtrip Note".to_string()),
                    selected: true,
                }],
                options: ImportOptions {
                    target_notebook_id: None,
                    duplicate_policy: DuplicatePolicy::ImportAsNew,
                },
            },
        )
        .expect("execute roundtrip import")
    };

    assert_eq!(summary.imported_count, 1);
    let reimported_id = summary.results[0].created_note_id.as_ref().unwrap();

    let conn = test_db.connection();
    let reimported_note = NoteRepository::get_by_id(conn, reimported_id)
        .unwrap()
        .unwrap();

    // Verify verbatim roundtrip content
    assert_eq!(reimported_note.content, complex_markdown);
    assert_eq!(reimported_note.content, original_note.content);
}

/// 11. Cooperative cancellation during batch import stops subsequent items safely.
#[test]
fn test_cooperative_cancellation_during_batch_import() {
    let mut test_db = TestDatabase::new();
    let temp_root = test_db.root().join("cancellation_test");
    fs::create_dir_all(&temp_root).expect("create temp dir");

    let op_id = format!("op-cancel-{}", uuid::Uuid::new_v4());

    let mut paths = Vec::new();
    for i in 1..=4 {
        let p = temp_root.join(format!("Cancel_{}.txt", i));
        fs::write(&p, format!("Body {}", i)).expect("write file");
        paths.push(p.to_string_lossy().to_string());
    }

    let preview = {
        let conn = test_db.connection();
        ImportExportService::scan_candidates(conn, &paths).expect("scan for cancel")
    };
    assert_eq!(preview.valid_files, 4);

    // Cancel before or during execution
    ImportExportService::cancel_operation(&op_id);

    let summary = {
        let conn = test_db.connection_mut();
        ImportExportService::execute_import(
            conn,
            ImportRequest {
                operation_id: Some(op_id.clone()),
                candidates: preview
                    .candidates
                    .into_iter()
                    .map(|c| ImportCandidateOverride {
                        id: c.id,
                        file_path: c.file_path,
                        custom_title: None,
                        selected: true,
                    })
                    .collect(),
                options: ImportOptions {
                    target_notebook_id: None,
                    duplicate_policy: DuplicatePolicy::ImportAsNew,
                },
            },
        )
        .expect("execute cancelled import")
    };

    assert_eq!(summary.calculate_outcome(), OperationOutcome::Cancelled);
    assert_eq!(summary.cancelled_count, 4);
    assert_eq!(summary.imported_count, 0);

    // Clean up cancellation state
    ImportExportService::clear_operation(Some(&op_id));
}

/// 12. Authenticated encryption boundaries for import and export.
#[test]
fn test_encrypted_notes_import_and_export_boundaries() {
    let mut test_db = TestDatabase::new();
    let temp_root = test_db.root().join("crypto_import_export");
    fs::create_dir_all(&temp_root).expect("create temp dir");

    let dek = generate_dek();

    // 1. Create encrypted note
    let note = {
        let conn = test_db.connection();
        NoteRepository::create_with_key(
            conn,
            CreateNoteDto {
                title: Some("Confidential Secret".to_string()),
                content: Some("Top secret project details.".to_string()),
                format: Some("md".to_string()),
                ..Default::default()
            },
            Some(&dek),
        )
        .expect("create encrypted note")
    };

    // 2. Export encrypted note with active DEK -> writes plaintext to file
    let export_path = temp_root.join("secret_export.md");
    {
        let conn = test_db.connection();
        let res = ImportExportService::export_single_note_with_key(
            conn,
            ExportSingleNoteRequest {
                note_id: note.id.clone(),
                destination_path: export_path.to_string_lossy().to_string(),
                format: ExportFormat::Md,
                overwrite: true,
            },
            Some(&dek),
        )
        .expect("export encrypted note with DEK");
        assert_eq!(
            res.status,
            personal_notepad_lib::storage::models::ExportItemStatus::Exported
        );
    }

    assert!(export_path.exists());
    let exported_text = fs::read_to_string(&export_path).unwrap();
    assert_eq!(exported_text, "Top secret project details.");

    // 3. Import plaintext file into encrypted vault with DEK
    let import_file = temp_root.join("new_secret.txt");
    fs::write(&import_file, "Classified incoming content.").expect("write secret file");
    let paths = vec![import_file.to_string_lossy().to_string()];

    let preview = {
        let conn = test_db.connection();
        ImportExportService::scan_candidates_with_key(conn, &paths, Some(&dek))
            .expect("scan with DEK")
    };
    assert_eq!(preview.valid_files, 1);

    let summary = {
        let conn = test_db.connection_mut();
        ImportExportService::execute_import_with_key(
            conn,
            ImportRequest {
                operation_id: None,
                candidates: vec![ImportCandidateOverride {
                    id: preview.candidates[0].id.clone(),
                    file_path: import_file.to_string_lossy().to_string(),
                    custom_title: Some("Encrypted Imported Secret".to_string()),
                    selected: true,
                }],
                options: ImportOptions {
                    target_notebook_id: None,
                    duplicate_policy: DuplicatePolicy::ImportAsNew,
                },
            },
            Some(&dek),
        )
        .expect("import with DEK")
    };

    assert_eq!(summary.imported_count, 1);
    let new_note_id = summary.results[0].created_note_id.as_ref().unwrap();

    // Verify newly imported note is stored encrypted
    let conn = test_db.connection();
    let loaded_with_key = NoteRepository::get_by_id_with_key(conn, new_note_id, Some(&dek))
        .unwrap()
        .unwrap();
    assert_eq!(loaded_with_key.content, "Classified incoming content.");
    assert_eq!(loaded_with_key.title, "Encrypted Imported Secret");
}
