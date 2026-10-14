use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::storage::errors::BackupError;
use crate::storage::migrations::MIGRATIONS;
use crate::storage::models::{
    BackupAttachmentsSummary, BackupCompatibilityStatus, BackupDatabaseInfo,
    BackupManifest, BackupScopeDeclaration, BackupType, BackupValidationIssue,
    BackupValidationIssueSeverity, BackupValidationResult, BACKUP_FORMAT_IDENTIFIER,
    CURRENT_BACKUP_FORMAT_VERSION,
};
use crate::storage::services::attachment_capture::CapturedAttachment;

/// Current maximum application database schema version supported by this build.
pub const CURRENT_APP_SCHEMA_VERSION: i32 = 4;

/// Maximum allowed archive entry count to prevent decompression bomb denial-of-service.
pub const MAX_ARCHIVE_ENTRY_COUNT: usize = 10_000;

/// Maximum total uncompressed archive size (5 GiB limit).
pub const MAX_TOTAL_UNCOMPRESSED_BYTES: u64 = 5 * 1024 * 1024 * 1024;

/// Service responsible for constructing, serializing, inspecting, and verifying backup manifests (Phase 11, Task 7).
pub struct ManifestManager;

impl ManifestManager {
    /// Constructs an authoritative `BackupManifest` from consistent database snapshot metadata and captured attachments.
    pub fn build_manifest(
        backup_type: BackupType,
        database: BackupDatabaseInfo,
        captured_attachments: &[CapturedAttachment],
        attachments_summary: BackupAttachmentsSummary,
    ) -> BackupManifest {
        let app_version = env!("CARGO_PKG_VERSION").to_string();
        let created_at = chrono::Utc::now().to_rfc3339();

        let mut checksums = HashMap::new();
        checksums.insert("database.sqlite".to_string(), database.sha256.clone());

        let mut attachments = Vec::with_capacity(captured_attachments.len());
        for captured in captured_attachments {
            checksums.insert(
                captured.archive_entry_path.clone(),
                captured.item.sha256.clone(),
            );
            attachments.push(captured.item.clone());
        }

        BackupManifest {
            format_identifier: BACKUP_FORMAT_IDENTIFIER.to_string(),
            format_version: CURRENT_BACKUP_FORMAT_VERSION,
            app_version,
            created_at,
            backup_type,
            database,
            attachments_summary,
            attachments,
            scope: BackupScopeDeclaration {
                includes_settings: true,
                includes_trash: true,
                includes_attachments: true,
            },
            checksums,
        }
    }

    /// Efficiently extracts and deserializes only the `manifest.json` header from an archive
    /// without decompressing payload entries or recomputing cryptographic hashes.
    pub fn read_manifest_quick(archive_path: &Path) -> Result<BackupManifest, BackupError> {
        if !archive_path.is_file() {
            return Err(BackupError::DestinationUnavailable(format!(
                "Archive not found at '{}'",
                archive_path.display()
            )));
        }

        let file = File::open(archive_path).map_err(BackupError::Io)?;
        let mut archive = ZipArchive::new(file).map_err(|e| {
            BackupError::ArchiveReadFailed(format!("Failed to open ZIP archive: {e}"))
        })?;

        let mut manifest_entry = archive.by_name("manifest.json").map_err(|e| {
            BackupError::ManifestInvalid(format!("Archive does not contain manifest.json: {e}"))
        })?;

        let mut manifest_bytes = Vec::new();
        manifest_entry
            .read_to_end(&mut manifest_bytes)
            .map_err(BackupError::Io)?;

        let manifest: BackupManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|e| BackupError::ManifestInvalid(format!("Malformed manifest.json: {e}")))?;

        Ok(manifest)
    }

    /// Inspects and verifies a backup archive (`.pnbackup` or `.zip`) against its own manifest and entry checksums.
    ///
    /// Validation Invariants:
    /// 1. Reads and parses `manifest.json`.
    /// 2. Validates format identifier and version compatibility.
    /// 3. Validates archive security: rejects path traversal (`..`), absolute paths, and duplicate entries.
    /// 4. Checks that required `database.sqlite` entry exists and recalculates its SHA-256 hash.
    /// 5. For every attachment declared in the manifest, verifies presence, size, and recalculates its SHA-256 hash.
    /// 6. Evaluates schema version compatibility against current application migration version.
    /// 7. Emits structured issues with severity and determines overall validity.
    pub fn validate_archive(archive_path: &Path) -> Result<BackupValidationResult, BackupError> {
        if !archive_path.is_file() {
            return Err(BackupError::DestinationUnavailable(format!(
                "Backup archive not found at '{}'",
                archive_path.display()
            )));
        }

        let file = File::open(archive_path).map_err(BackupError::Io)?;
        let mut archive = ZipArchive::new(file).map_err(|e| {
            BackupError::ArchiveReadFailed(format!("Failed to open ZIP archive: {e}"))
        })?;

        let mut issues = Vec::new();

        // 1. Check entry count limits (decompression safety)
        if archive.len() > MAX_ARCHIVE_ENTRY_COUNT {
            issues.push(BackupValidationIssue {
                severity: BackupValidationIssueSeverity::Error,
                code: "ARCHIVE_ENTRY_LIMIT_EXCEEDED".to_string(),
                message: format!(
                    "Archive contains {} entries, which exceeds safety limit of {}.",
                    archive.len(),
                    MAX_ARCHIVE_ENTRY_COUNT
                ),
            });
        }

        // 2. Scan entries for path safety and duplicates
        let mut entry_names = HashSet::new();
        let mut total_uncompressed_bytes = 0u64;

        for i in 0..archive.len() {
            let entry = match archive.by_index(i) {
                Ok(e) => e,
                Err(e) => {
                    issues.push(BackupValidationIssue {
                        severity: BackupValidationIssueSeverity::Error,
                        code: "ARCHIVE_ENTRY_CORRUPT".to_string(),
                        message: format!("Entry at index {i} could not be read: {e}"),
                    });
                    continue;
                }
            };

            let name = entry.name().to_string();
            total_uncompressed_bytes += entry.size();

            // Zip Slip defense
            if name.starts_with('/')
                || name.starts_with('\\')
                || name.contains("..")
                || name.contains(":")
            {
                issues.push(BackupValidationIssue {
                    severity: BackupValidationIssueSeverity::Error,
                    code: "UNSAFE_ARCHIVE_PATH".to_string(),
                    message: format!("Archive contains unsafe path sequence: '{name}'"),
                });
            }

            if !entry_names.insert(name.clone()) {
                issues.push(BackupValidationIssue {
                    severity: BackupValidationIssueSeverity::Error,
                    code: "DUPLICATE_ARCHIVE_ENTRY".to_string(),
                    message: format!("Duplicate entry detected in archive: '{name}'"),
                });
            }
        }

        if total_uncompressed_bytes > MAX_TOTAL_UNCOMPRESSED_BYTES {
            issues.push(BackupValidationIssue {
                severity: BackupValidationIssueSeverity::Error,
                code: "ARCHIVE_SIZE_LIMIT_EXCEEDED".to_string(),
                message: format!(
                    "Archive total uncompressed size ({total_uncompressed_bytes} bytes) exceeds safety limit.",
                ),
            });
        }

        // 3. Read and deserialize manifest.json
        let manifest_bytes = match archive.by_name("manifest.json") {
            Ok(mut entry) => {
                let mut bytes = Vec::new();
                if let Err(e) = entry.read_to_end(&mut bytes) {
                    issues.push(BackupValidationIssue {
                        severity: BackupValidationIssueSeverity::Error,
                        code: "MANIFEST_READ_FAILED".to_string(),
                        message: format!("Failed to read manifest.json from archive: {e}"),
                    });
                    Vec::new()
                } else {
                    bytes
                }
            }
            Err(_) => {
                issues.push(BackupValidationIssue {
                    severity: BackupValidationIssueSeverity::Error,
                    code: "MANIFEST_MISSING".to_string(),
                    message: "Archive does not contain a 'manifest.json' file.".to_string(),
                });
                Vec::new()
            }
        };

        let manifest: Option<BackupManifest> = if !manifest_bytes.is_empty() {
            match serde_json::from_slice::<BackupManifest>(&manifest_bytes) {
                Ok(m) => Some(m),
                Err(e) => {
                    issues.push(BackupValidationIssue {
                        severity: BackupValidationIssueSeverity::Error,
                        code: "MANIFEST_PARSE_ERROR".to_string(),
                        message: format!("manifest.json is malformed or invalid JSON: {e}"),
                    });
                    None
                }
            }
        } else {
            None
        };

        // 4. Validate manifest semantics and version compatibility if present
        let mut compatibility = BackupCompatibilityStatus::Compatible;

        if let Some(ref m) = manifest {
            if m.format_identifier != BACKUP_FORMAT_IDENTIFIER {
                issues.push(BackupValidationIssue {
                    severity: BackupValidationIssueSeverity::Error,
                    code: "INVALID_FORMAT_IDENTIFIER".to_string(),
                    message: format!(
                        "Invalid format identifier '{}'. Expected '{}'.",
                        m.format_identifier, BACKUP_FORMAT_IDENTIFIER
                    ),
                });
                compatibility = BackupCompatibilityStatus::Corrupted;
            }

            if m.format_version > CURRENT_BACKUP_FORMAT_VERSION {
                issues.push(BackupValidationIssue {
                    severity: BackupValidationIssueSeverity::Error,
                    code: "UNSUPPORTED_FORMAT_VERSION".to_string(),
                    message: format!(
                        "Backup format version {} is newer than supported version {}.",
                        m.format_version, CURRENT_BACKUP_FORMAT_VERSION
                    ),
                });
                compatibility = BackupCompatibilityStatus::IncompatibleVersion;
            }

            // Schema version check
            let max_supported_schema = MIGRATIONS.iter().map(|mig| mig.version).max().unwrap_or(CURRENT_APP_SCHEMA_VERSION);
            if m.database.schema_version > max_supported_schema {
                issues.push(BackupValidationIssue {
                    severity: BackupValidationIssueSeverity::Error,
                    code: "INCOMPATIBLE_SCHEMA_VERSION".to_string(),
                    message: format!(
                        "Backup database schema (v{}) exceeds current application support (v{}).",
                        m.database.schema_version, max_supported_schema
                    ),
                });
                compatibility = BackupCompatibilityStatus::IncompatibleVersion;
            } else if m.database.schema_version < max_supported_schema {
                issues.push(BackupValidationIssue {
                    severity: BackupValidationIssueSeverity::Warning,
                    code: "SCHEMA_MIGRATION_REQUIRED".to_string(),
                    message: format!(
                        "Backup was created with an older database schema (v{}). It will be migrated to v{} during restore.",
                        m.database.schema_version, max_supported_schema
                    ),
                });
                compatibility = BackupCompatibilityStatus::RequiresMigration;
            }

            // 5. Recalculate checksum for database.sqlite
            match archive.by_name("database.sqlite") {
                Ok(mut db_entry) => {
                    let mut hasher = Sha256::new();
                    let mut buf = [0u8; 65536];
                    let mut read_bytes = 0u64;

                    loop {
                        match db_entry.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                hasher.update(&buf[..n]);
                                read_bytes += n as u64;
                            }
                            Err(e) => {
                                issues.push(BackupValidationIssue {
                                    severity: BackupValidationIssueSeverity::Error,
                                    code: "DATABASE_READ_ERROR".to_string(),
                                    message: format!("Error reading database.sqlite: {e}"),
                                });
                                break;
                            }
                        }
                    }

                    let recalculated_hash = format!("{:x}", hasher.finalize());
                    if recalculated_hash != m.database.sha256 {
                        issues.push(BackupValidationIssue {
                            severity: BackupValidationIssueSeverity::Error,
                            code: "DATABASE_CHECKSUM_MISMATCH".to_string(),
                            message: format!(
                                "Database checksum mismatch: expected {}, got {}.",
                                m.database.sha256, recalculated_hash
                            ),
                        });
                        compatibility = BackupCompatibilityStatus::Corrupted;
                    }

                    if read_bytes != m.database.file_size {
                        issues.push(BackupValidationIssue {
                            severity: BackupValidationIssueSeverity::Warning,
                            code: "DATABASE_SIZE_MISMATCH".to_string(),
                            message: format!(
                                "Database size mismatch: expected {} bytes, got {} bytes.",
                                m.database.file_size, read_bytes
                            ),
                        });
                    }
                }
                Err(_) => {
                    issues.push(BackupValidationIssue {
                        severity: BackupValidationIssueSeverity::Error,
                        code: "DATABASE_ENTRY_MISSING".to_string(),
                        message: "Archive does not contain required 'database.sqlite' file.".to_string(),
                    });
                    compatibility = BackupCompatibilityStatus::Corrupted;
                }
            }

            // 6. Recalculate checksums for all declared attachments
            for att in &m.attachments {
                let entry_path = format!("attachments/{}", att.relative_path);
                match archive.by_name(&entry_path) {
                    Ok(mut att_entry) => {
                        let mut hasher = Sha256::new();
                        let mut buf = [0u8; 65536];
                        let mut read_bytes = 0u64;

                        loop {
                            match att_entry.read(&mut buf) {
                                Ok(0) => break,
                                Ok(n) => {
                                    hasher.update(&buf[..n]);
                                    read_bytes += n as u64;
                                }
                                Err(e) => {
                                    issues.push(BackupValidationIssue {
                                        severity: BackupValidationIssueSeverity::Error,
                                        code: "ATTACHMENT_READ_ERROR".to_string(),
                                        message: format!("Error reading '{}': {e}", att.file_name),
                                    });
                                    break;
                                }
                            }
                        }

                        let hash_hex = format!("{:x}", hasher.finalize());
                        if hash_hex != att.sha256 {
                            issues.push(BackupValidationIssue {
                                severity: BackupValidationIssueSeverity::Error,
                                code: "ATTACHMENT_CHECKSUM_MISMATCH".to_string(),
                                message: format!(
                                    "Checksum mismatch for attachment '{}'. Expected {}, got {}.",
                                    att.file_name, att.sha256, hash_hex
                                ),
                            });
                            compatibility = BackupCompatibilityStatus::Corrupted;
                        }

                        if read_bytes != att.file_size {
                            issues.push(BackupValidationIssue {
                                severity: BackupValidationIssueSeverity::Warning,
                                code: "ATTACHMENT_SIZE_MISMATCH".to_string(),
                                message: format!(
                                    "Attachment '{}' size mismatch: expected {} B, got {} B.",
                                    att.file_name, att.file_size, read_bytes
                                ),
                            });
                        }
                    }
                    Err(_) => {
                        issues.push(BackupValidationIssue {
                            severity: BackupValidationIssueSeverity::Error,
                            code: "ATTACHMENT_ENTRY_MISSING".to_string(),
                            message: format!(
                                "Required attachment '{}' ('{}') is missing from archive.",
                                att.file_name, entry_path
                            ),
                        });
                        compatibility = BackupCompatibilityStatus::Corrupted;
                    }
                }
            }
        } else {
            compatibility = BackupCompatibilityStatus::Corrupted;
        }

        let is_valid = manifest.is_some()
            && !issues.iter().any(|i| i.severity == BackupValidationIssueSeverity::Error);

        Ok(BackupValidationResult {
            is_valid,
            compatibility,
            manifest,
            issues,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;
    use uuid::Uuid;
    use crate::storage::models::BackupAttachmentItem;

    #[test]
    fn test_task_7_manifest_generation_and_checksum_verification() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_man_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).expect("Create temp dir");
        let archive_path = temp_dir.join("test_backup.pnbackup");

        // 1. Prepare dummy db content and attachment content
        let db_bytes = b"SQLite format 3\0 dummy database test content";
        let mut db_hasher = Sha256::new();
        db_hasher.update(db_bytes);
        let db_sha256 = format!("{:x}", db_hasher.finalize());

        let att_bytes = b"dummy attachment bytes";
        let mut att_hasher = Sha256::new();
        att_hasher.update(att_bytes);
        let att_sha256 = format!("{:x}", att_hasher.finalize());

        let note_id = Uuid::new_v4().to_string();
        let att_item = BackupAttachmentItem {
            id: "att-1".to_string(),
            note_id: note_id.clone(),
            file_name: "test.png".to_string(),
            relative_path: format!("{note_id}/test.png"),
            file_size: att_bytes.len() as u64,
            mime_type: Some("image/png".to_string()),
            sha256: att_sha256.clone(),
        };

        let captured = vec![CapturedAttachment {
            item: att_item.clone(),
            source_path: temp_dir.join("test.png"),
            archive_entry_path: format!("attachments/{note_id}/test.png"),
        }];

        let db_info = BackupDatabaseInfo {
            entry_path: "database.sqlite".to_string(),
            file_size: db_bytes.len() as u64,
            sha256: db_sha256.clone(),
            schema_version: 4,
            note_count: 5,
            notebook_count: 1,
            tag_count: 2,
            trash_count: 0,
        };

        let summary = BackupAttachmentsSummary {
            total_files: 1,
            total_bytes: att_bytes.len() as u64,
        };

        // 2. Build manifest
        let manifest = ManifestManager::build_manifest(
            BackupType::Manual,
            db_info,
            &captured,
            summary,
        );

        assert_eq!(manifest.format_identifier, BACKUP_FORMAT_IDENTIFIER);
        assert_eq!(manifest.format_version, CURRENT_BACKUP_FORMAT_VERSION);
        assert_eq!(manifest.database.sha256, db_sha256);
        assert_eq!(manifest.attachments.len(), 1);
        assert_eq!(manifest.checksums.len(), 2);

        // 3. Write a valid ZIP archive
        {
            let file = File::create(&archive_path).expect("Create archive");
            let mut zip = ZipWriter::new(file);
            let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

            // Add manifest.json
            zip.start_file("manifest.json", options).expect("Add manifest.json");
            let manifest_json = serde_json::to_vec_pretty(&manifest).expect("Serialize manifest");
            zip.write_all(&manifest_json).expect("Write manifest");

            // Add database.sqlite
            zip.start_file("database.sqlite", options).expect("Add database.sqlite");
            zip.write_all(db_bytes).expect("Write database");

            // Add attachment
            zip.start_file(format!("attachments/{note_id}/test.png"), options).expect("Add attachment");
            zip.write_all(att_bytes).expect("Write attachment");

            zip.finish().expect("Finish zip");
        }

        // 4. Validate valid archive
        let valid_res = ManifestManager::validate_archive(&archive_path)
            .expect("Validate valid archive");

        assert!(valid_res.is_valid, "Valid archive must pass validation");
        assert_eq!(valid_res.compatibility, BackupCompatibilityStatus::Compatible);
        assert!(valid_res.issues.is_empty(), "Valid archive should have no issues");
        assert!(valid_res.manifest.is_some());

        // 5. Test Corrupted Checksum Detection
        let corrupt_archive_path = temp_dir.join("corrupt_backup.pnbackup");
        {
            let file = File::create(&corrupt_archive_path).expect("Create corrupt archive");
            let mut zip = ZipWriter::new(file);
            let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

            zip.start_file("manifest.json", options).expect("Add manifest.json");
            let manifest_json = serde_json::to_vec_pretty(&manifest).expect("Serialize manifest");
            zip.write_all(&manifest_json).expect("Write manifest");

            // Intentionally write altered database bytes
            zip.start_file("database.sqlite", options).expect("Add database.sqlite");
            zip.write_all(b"TAMPERED_DATABASE_BYTES").expect("Write corrupt database");

            zip.finish().expect("Finish zip");
        }

        let corrupt_res = ManifestManager::validate_archive(&corrupt_archive_path)
            .expect("Validate corrupt archive");

        assert!(!corrupt_res.is_valid, "Tampered archive must fail validation");
        assert_eq!(corrupt_res.compatibility, BackupCompatibilityStatus::Corrupted);
        assert!(
            corrupt_res.issues.iter().any(|i| i.code == "DATABASE_CHECKSUM_MISMATCH"),
            "Must report DATABASE_CHECKSUM_MISMATCH issue"
        );

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
