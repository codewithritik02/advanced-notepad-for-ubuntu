//! ZIP export: every note gets its own folder holding the note file and an `assets/` folder with
//! the files attached to it, so a note and its images always travel together.
//!
//! ```text
//! export.zip
//! ├── Meeting notes/
//! │   ├── Meeting notes.md          (the note, in its own format; image links point to assets/)
//! │   └── assets/photo.png
//! └── Shopping list/
//!     └── Shopping list.txt
//! ```

use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::storage::attachment_storage::AttachmentStorage;
use crate::storage::crypto::SecretBytes;
use crate::storage::errors::StorageError;
use crate::storage::models::{
    Attachment, ExportBatchRequest, ExportFileResult, ExportItemStatus, ExportProgress,
    ExportScope, ExportSummary,
};
use crate::storage::repositories::{AttachmentRepository, NoteRepository, SearchRepository};
use crate::storage::services::attachments::AttachmentService;
use crate::storage::services::import_export::{ImportExportService, MAX_BATCH_FILE_COUNT};

/// Largest single attachment that is put into an archive.
const MAX_ZIP_ATTACHMENT_BYTES: u64 = 100 * 1024 * 1024;

/// A name that is unique (ignoring case) among `used`, adding " (2)", " (3)", ... when needed.
fn unique_name(base: &str, used: &mut std::collections::HashSet<String>) -> String {
    let mut candidate = base.to_string();
    let mut counter = 2;
    while !used.insert(candidate.to_lowercase()) {
        candidate = match base.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() && ext.len() <= 8 => {
                format!("{stem} ({counter}).{ext}")
            }
            _ => format!("{base} ({counter})"),
        };
        counter += 1;
    }
    candidate
}

/// Percent-encodes what would break a Markdown link target.
fn link_target(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 8);
    for ch in name.chars() {
        match ch {
            ' ' => out.push_str("%20"),
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            '%' => out.push_str("%25"),
            _ => out.push(ch),
        }
    }
    out
}

/// Points `notepad://attachment/<id>` references at the exported `assets/<file>` copies.
pub fn rewrite_attachment_links(content: &str, assets: &[(String, String)]) -> String {
    let mut result = content.to_string();
    for (id, file_name) in assets {
        result = result.replace(
            &format!("notepad://attachment/{id}"),
            &format!("assets/{}", link_target(file_name)),
        );
    }
    result
}

fn zip_error(error: zip::result::ZipError) -> StorageError {
    StorageError::Validation(format!("Could not write the ZIP archive: {error}"))
}

struct TempFileGuard {
    path: PathBuf,
    committed: bool,
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

impl ImportExportService {
    /// Exports the selected notes into one ZIP archive (a folder per note, attachments in
    /// `assets/`). Trashed notes are skipped like in every other export.
    pub fn export_notes_zip_with_key<F>(
        conn: &Connection,
        storage: &AttachmentStorage,
        request: ExportBatchRequest,
        dek: Option<&SecretBytes>,
        mut on_progress: F,
    ) -> Result<ExportSummary, StorageError>
    where
        F: FnMut(&ExportProgress),
    {
        if SearchRepository::is_encryption_enabled(conn) && dek.is_none() {
            return Err(StorageError::Crypto(
                "Application is locked. Please unlock first.".to_string(),
            ));
        }
        let started = std::time::Instant::now();
        let operation_id = request.operation_id.clone();

        let scope = request.scope.clone().unwrap_or(ExportScope::SelectedNotes {
            note_ids: request.note_ids.clone(),
        });
        let preview = Self::resolve_export_selection_with_key(conn, &scope, dek)?;
        let total = preview.notes.len();
        if total > MAX_BATCH_FILE_COUNT {
            Self::clear_operation(operation_id.as_deref());
            return Err(StorageError::Validation(format!(
                "Export batch size ({total}) exceeds maximum allowed limit of {MAX_BATCH_FILE_COUNT} notes."
            )));
        }

        // Destination: a .zip file
        let mut target = match request.destination_file.as_deref() {
            Some(file) if !file.trim().is_empty() => PathBuf::from(file),
            _ => Path::new(&request.destination_dir).join("export.zip"),
        };
        if !target
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
        {
            let mut name = target.as_os_str().to_os_string();
            name.push(".zip");
            target = PathBuf::from(name);
        }
        Self::validate_destination_path(&target)?;
        if target.is_dir() {
            return Err(StorageError::Validation(format!(
                "Destination path '{}' is a directory, not a file",
                target.display()
            )));
        }
        if target.exists() && !request.overwrite {
            return Err(StorageError::Conflict(format!(
                "Destination file '{}' already exists and overwrite was not permitted",
                target.display()
            )));
        }
        let parent = target
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        std::fs::create_dir_all(&parent)?;

        let temp_path = parent.join(format!(".tmp_export_{}.part", uuid::Uuid::new_v4()));
        let mut guard = TempFileGuard {
            path: temp_path.clone(),
            committed: false,
        };
        let file = std::fs::File::create(&temp_path)?;
        let mut writer = zip::ZipWriter::new(std::io::BufWriter::new(file));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .large_file(true);

        let target_str = target.to_string_lossy().to_string();
        let skipped = preview.skipped_trashed_count + preview.skipped_missing_count;
        let mut results = Vec::new();
        let (mut exported, mut failed, mut cancelled) = (0usize, 0usize, 0usize);
        let mut used_folders = std::collections::HashSet::new();
        let mut is_cancelled = false;

        for (idx, meta) in preview.notes.iter().enumerate() {
            if is_cancelled || Self::is_operation_cancelled(operation_id.as_deref()) {
                is_cancelled = true;
                cancelled += 1;
                results.push(ExportFileResult {
                    note_id: meta.id.clone(),
                    note_title: meta.title.clone(),
                    file_path: String::new(),
                    status: ExportItemStatus::Cancelled,
                    message: Some("Export cancelled by user.".to_string()),
                });
            } else {
                match Self::write_note_folder(
                    conn,
                    storage,
                    dek,
                    &mut writer,
                    options,
                    &meta.id,
                    &mut used_folders,
                ) {
                    Ok(skipped_assets) => {
                        exported += 1;
                        results.push(ExportFileResult {
                            note_id: meta.id.clone(),
                            note_title: meta.title.clone(),
                            file_path: target_str.clone(),
                            status: ExportItemStatus::Exported,
                            message: (skipped_assets > 0).then(|| {
                                format!(
                                    "{skipped_assets} attachment(s) could not be read and were left out."
                                )
                            }),
                        });
                    }
                    Err(error) => {
                        failed += 1;
                        results.push(ExportFileResult {
                            note_id: meta.id.clone(),
                            note_title: meta.title.clone(),
                            file_path: String::new(),
                            status: ExportItemStatus::Failed,
                            message: Some(error.user_friendly_message()),
                        });
                    }
                }
            }
            on_progress(&ExportProgress {
                operation_id: operation_id.clone(),
                total,
                current_index: idx + 1,
                current_title: meta.title.clone(),
                exported_count: exported,
                skipped_count: skipped,
                failed_count: failed,
                cancelled_count: cancelled,
            });
        }

        let mut inner = writer.finish().map_err(zip_error)?;
        inner.flush()?;
        drop(inner);

        if is_cancelled || exported == 0 {
            // nothing worth keeping: the temporary file is removed by the guard
        } else {
            if target.exists() {
                let _ = std::fs::remove_file(&target);
            }
            std::fs::rename(&temp_path, &target)?;
            guard.committed = true;
        }
        Self::clear_operation(operation_id.as_deref());

        let mut summary = ExportSummary {
            total_notes: total,
            exported_count: exported,
            skipped_count: skipped,
            failed_count: failed,
            cancelled_count: cancelled,
            outcome: None,
            results,
            duration_ms: started.elapsed().as_millis() as u64,
        };
        summary.outcome = Some(summary.calculate_outcome());
        Ok(summary)
    }

    /// Writes one note folder. Returns how many attachments could not be read.
    fn write_note_folder<W: Write + std::io::Seek>(
        conn: &Connection,
        storage: &AttachmentStorage,
        dek: Option<&SecretBytes>,
        writer: &mut zip::ZipWriter<W>,
        options: zip::write::SimpleFileOptions,
        note_id: &str,
        used_folders: &mut std::collections::HashSet<String>,
    ) -> Result<usize, StorageError> {
        let note = NoteRepository::get_by_id_with_key(conn, note_id, dek)?
            .ok_or_else(|| StorageError::Validation("The note no longer exists.".to_string()))?;
        let folder = unique_name(&Self::sanitize_filename(&note.title), used_folders);
        let ext = if note.format == "md" { "md" } else { "txt" };

        let attachments: Vec<Attachment> =
            AttachmentRepository::list_by_note_id_with_key(conn, &note.id, dek)?;
        let mut used_assets = std::collections::HashSet::new();
        let mut assets: Vec<(String, String)> = Vec::new(); // (attachment id, file name in assets/)
        let mut asset_bytes: Vec<(String, Vec<u8>)> = Vec::new();
        let mut skipped_assets = 0;
        for attachment in &attachments {
            match AttachmentService::read_attachment_bytes_with_key(
                conn,
                storage,
                &attachment.id,
                dek,
                MAX_ZIP_ATTACHMENT_BYTES,
            ) {
                Ok(bytes) => {
                    let name = unique_name(
                        &Self::sanitize_filename(&attachment.file_name),
                        &mut used_assets,
                    );
                    assets.push((attachment.id.clone(), name.clone()));
                    asset_bytes.push((name, bytes));
                }
                Err(_) => skipped_assets += 1,
            }
        }

        let content = rewrite_attachment_links(&note.content, &assets);
        writer
            .start_file(format!("{folder}/{folder}.{ext}"), options)
            .map_err(zip_error)?;
        writer.write_all(content.as_bytes())?;
        for (name, bytes) in asset_bytes {
            writer
                .start_file(format!("{folder}/assets/{name}"), options)
                .map_err(zip_error)?;
            writer.write_all(&bytes)?;
        }
        Ok(skipped_assets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_pointed_at_the_assets_folder() {
        let assets = vec![("a1".to_string(), "my photo (1).png".to_string())];
        let out = rewrite_attachment_links(
            "![pic|300](notepad://attachment/a1) and [x](notepad://note/n1)",
            &assets,
        );
        assert_eq!(
            out,
            "![pic|300](assets/my%20photo%20%281%29.png) and [x](notepad://note/n1)"
        );
    }

    #[test]
    fn names_stay_unique_ignoring_case() {
        let mut used = std::collections::HashSet::new();
        assert_eq!(unique_name("Note", &mut used), "Note");
        assert_eq!(unique_name("note", &mut used), "note (2)");
        assert_eq!(unique_name("a.png", &mut used), "a.png");
        assert_eq!(unique_name("A.png", &mut used), "A (2).png");
    }

    fn setup() -> (Connection, AttachmentStorage, PathBuf) {
        use crate::storage::database::init_connection;
        use crate::storage::migrations::run_migrations;
        use crate::storage::paths::StoragePaths;
        let base = std::env::temp_dir().join(format!("pn_zip_{}", uuid::Uuid::new_v4()));
        let paths = StoragePaths::from_root(base.clone());
        paths.ensure_directories().unwrap();
        let mut conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut conn).unwrap();
        std::fs::create_dir_all(base.join("sources")).unwrap();
        (conn, AttachmentStorage::new(paths), base)
    }

    fn make_note(
        conn: &Connection,
        dek: Option<&SecretBytes>,
        title: &str,
        content: &str,
        format: &str,
    ) -> String {
        use crate::storage::models::CreateNoteDto;
        NoteRepository::create_with_key(
            conn,
            CreateNoteDto {
                title: Some(title.to_string()),
                content: Some(content.to_string()),
                format: Some(format.to_string()),
                ..Default::default()
            },
            dek,
        )
        .unwrap()
        .id
    }

    fn zip_request(note_ids: Vec<String>, dest: &Path) -> ExportBatchRequest {
        ExportBatchRequest {
            operation_id: None,
            scope: None,
            note_ids,
            format: crate::storage::models::ExportFormat::Zip,
            destination_dir: String::new(),
            destination_file: Some(dest.to_string_lossy().to_string()),
            include_metadata: true,
            overwrite: true,
        }
    }

    fn read_zip(path: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
        use std::io::Read;
        let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        let mut files = std::collections::BTreeMap::new();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).unwrap();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            files.insert(entry.name().to_string(), bytes);
        }
        files
    }

    fn run_zip_export(dek: Option<&SecretBytes>) {
        let (conn, storage, base) = setup();
        let png: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3, 4];
        let image_path = base.join("sources").join("my photo.png");
        std::fs::write(&image_path, &png).unwrap();
        let pdf_path = base.join("sources").join("report.pdf");
        std::fs::write(&pdf_path, b"%PDF-1.4 test").unwrap();

        let with_images = make_note(&conn, dek, "Trip: Day 1", "", "md");
        let att = AttachmentService::add_attachment_with_key(
            &conn,
            &storage,
            &with_images,
            &image_path,
            dek,
        )
        .unwrap();
        AttachmentService::add_attachment_with_key(&conn, &storage, &with_images, &pdf_path, dek)
            .unwrap();
        NoteRepository::update_with_key(
            &conn,
            &with_images,
            crate::storage::models::UpdateNoteDto {
                content: Some(format!(
                    "# Day 1\n![photo|300](notepad://attachment/{})\ntext",
                    att.id
                )),
                ..Default::default()
            },
            dek,
        )
        .unwrap();
        let plain = make_note(&conn, dek, "Shopping", "milk", "txt");
        let same_title = make_note(&conn, dek, "Shopping", "eggs", "md");

        let dest = base.join("out").join("export.zip");
        let summary = ImportExportService::export_notes_zip_with_key(
            &conn,
            &storage,
            zip_request(vec![with_images, plain, same_title], &dest),
            dek,
            |_| {},
        )
        .unwrap();
        assert_eq!(summary.exported_count, 3, "{:?}", summary.results);
        assert_eq!(summary.failed_count, 0);

        let files = read_zip(&dest);
        let names: Vec<&str> = files.keys().map(String::as_str).collect();
        assert!(names.contains(&"Trip Day 1/Trip Day 1.md"), "{names:?}");
        assert!(
            names.contains(&"Trip Day 1/assets/my photo.png"),
            "{names:?}"
        );
        assert!(names.contains(&"Trip Day 1/assets/report.pdf"), "{names:?}");
        assert!(names.contains(&"Shopping/Shopping.txt"), "{names:?}");
        assert!(names.contains(&"Shopping (2)/Shopping (2).md"), "{names:?}");
        assert_eq!(
            files["Trip Day 1/assets/my photo.png"], png,
            "attachment bytes are intact"
        );
        let text = String::from_utf8(files["Trip Day 1/Trip Day 1.md"].clone()).unwrap();
        assert!(
            text.contains("![photo|300](assets/my%20photo.png)"),
            "{text}"
        );
        assert!(!text.contains("notepad://attachment"));
        assert_eq!(files["Shopping/Shopping.txt"], b"milk");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn zip_export_puts_each_note_and_its_assets_in_its_own_folder() {
        run_zip_export(None);
    }

    #[test]
    fn zip_export_decrypts_notes_and_attachments_in_memory() {
        let dek = crate::storage::crypto::generate_dek();
        run_zip_export(Some(&dek));
    }

    #[test]
    fn zip_export_leaves_no_partial_file_when_every_note_fails_or_is_cancelled() {
        let (conn, storage, base) = setup();
        let dest = base.join("none.zip");
        let summary = ImportExportService::export_notes_zip_with_key(
            &conn,
            &storage,
            zip_request(vec!["missing".to_string()], &dest),
            None,
            |_| {},
        )
        .unwrap();
        assert_eq!(summary.exported_count, 0);
        assert!(!dest.exists());
        let leftovers = std::fs::read_dir(&base)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(".tmp_export_"))
            .count();
        assert_eq!(leftovers, 0);
        let _ = std::fs::remove_dir_all(&base);
    }
}
