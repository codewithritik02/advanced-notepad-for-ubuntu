use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::storage::attachment_storage::AttachmentStorage;
use crate::storage::crypto::kdf::{derive_kek, unwrap_dek};
use crate::storage::crypto::metadata::{
    EncryptionMetadata, MigrationStatus, ENCRYPTION_METADATA_KEY,
};
use crate::storage::crypto::{
    decrypt_file_to_file, decrypt_string, encrypt_file_to_file, encrypt_string,
    is_encrypted_string, SecretBytes,
};
use crate::storage::errors::CryptoError;
use crate::storage::models::BackupRestoreRequest;
use crate::storage::repositories::{SearchRepository, SettingsRepository};
use crate::storage::services::RestoreService;
use crate::storage::StoragePaths;

/// Detailed operational summary of a completed safe migration (Phase 12, Task 17).
#[derive(Debug, Clone)]
pub struct MigrationReport {
    pub notes_migrated: usize,
    pub attachments_migrated: usize,
    pub checkpoint_path: PathBuf,
}

/// Detailed operational summary of a completed safe disable-encryption workflow (Phase 12, Task 21).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisableEncryptionReport {
    pub notes_decrypted: usize,
    pub attachments_decrypted: usize,
    pub checkpoint_path: String,
}

/// Durable marker payload tracking multi-step encryption transitions across crashes (Phase 12, Task 18).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EncryptionTransitionMarker {
    pub operation_id: String,
    pub operation_type: String,
    pub stage: MigrationStatus,
    pub checkpoint_path: String,
    pub started_at: String,
    pub updated_at: String,
}

/// Recovery decision made at startup when inspecting transition markers and metadata (Phase 12, Task 18).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupRecoveryDecision {
    /// No interrupted operation was detected. Startup can proceed normally.
    None,
    /// An unstarted operation (SetupRequested/CheckpointCreated) was detected before any data modification.
    /// Safely cleaned up and storage remains unencrypted.
    CleanedUpUnstarted,
    /// An operation that completed full encryption and verification was detected.
    /// Automatically finalized to Active state.
    AutoFinalizedVerified,
    /// Storage is in an ambiguous or interrupted state (MigrationInProgress, StagingPrepared, Failed).
    /// Normal operations are locked; user must make a recovery decision (Rollback or Resume).
    RestrictedRecoveryRequired {
        stage: MigrationStatus,
        checkpoint_path: Option<String>,
    },
}

/// Domain service responsible for safe, stateful migration of unencrypted storage
/// into authenticated AES-256-GCM ciphertext without data loss or ambiguous states.
pub struct MigrationService;

impl MigrationService {
    /// Validates pre-migration database health and attachments accessibility.
    pub fn validate_pre_migration(
        conn: &Connection,
        paths: &StoragePaths,
    ) -> Result<(), CryptoError> {
        let integrity: String = conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(|e| {
                CryptoError::MigrationError(format!("Pre-migration integrity check failed: {e}"))
            })?;

        if integrity != "ok" {
            return Err(CryptoError::MigrationError(format!(
                "Cannot migrate database with integrity errors: {integrity}"
            )));
        }

        std::fs::create_dir_all(&paths.attachments_dir).map_err(|e| {
            CryptoError::MigrationError(format!("Cannot access attachments directory: {e}"))
        })?;

        Ok(())
    }

    /// Creates a verified, pre-migration recovery checkpoint in `checkpoints/` before any mutations.
    pub fn create_recovery_checkpoint(
        conn: &Connection,
        paths: &StoragePaths,
    ) -> Result<PathBuf, CryptoError> {
        let checkpoint = RestoreService::create_recovery_checkpoint(conn, paths).map_err(|e| {
            CryptoError::MigrationError(format!("Pre-migration recovery checkpoint failed: {e}"))
        })?;

        let checkpoint_path = PathBuf::from(checkpoint.file_path);
        if !checkpoint_path.is_file() {
            return Err(CryptoError::MigrationError(format!(
                "Checkpoint file '{:?}' was not created on disk",
                checkpoint_path
            )));
        }

        Ok(checkpoint_path)
    }

    /// Migrates any existing plaintext notes into versioned `ENC$v1$` authenticated ciphertext envelopes.
    pub fn migrate_notes(conn: &Connection, dek: &SecretBytes) -> Result<usize, CryptoError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, title, content FROM notes WHERE title NOT LIKE 'ENC$v1$%' OR content NOT LIKE 'ENC$v1%'",
            )
            .map_err(|e| CryptoError::MigrationError(format!("Failed to prepare unencrypted notes query: {e}")))?;

        let note_rows: Vec<(String, String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| {
                CryptoError::MigrationError(format!("Failed to query unencrypted notes: {e}"))
            })?
            .filter_map(|res| res.ok())
            .collect();

        let mut count = 0;
        for (id, plain_title, plain_content) in note_rows {
            let enc_title = if is_encrypted_string(&plain_title) {
                plain_title
            } else {
                let aad = format!("notes:{id}:title");
                encrypt_string(&plain_title, dek, aad.as_bytes())?
            };

            let enc_content = if is_encrypted_string(&plain_content) {
                plain_content
            } else {
                let aad = format!("notes:{id}:content");
                encrypt_string(&plain_content, dek, aad.as_bytes())?
            };

            conn.execute(
                "UPDATE notes SET title = ?1, content = ?2 WHERE id = ?3",
                params![enc_title, enc_content, id],
            )
            .map_err(|e| {
                CryptoError::MigrationError(format!("Failed to update encrypted note {id}: {e}"))
            })?;

            count += 1;
        }

        Ok(count)
    }

    /// Migrates existing plaintext attachment records in SQLite and attachment files on disk.
    pub fn migrate_attachments(
        conn: &Connection,
        paths: &StoragePaths,
        dek: &SecretBytes,
    ) -> Result<usize, CryptoError> {
        let mut stmt = conn
            .prepare("SELECT id, note_id, file_name, relative_path, mime_type FROM attachments")
            .map_err(|e| {
                CryptoError::MigrationError(format!("Failed to prepare attachments query: {e}"))
            })?;

        let att_rows: Vec<(String, String, String, String, String)> = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .map_err(|e| CryptoError::MigrationError(format!("Failed to query attachments: {e}")))?
            .filter_map(|res| res.ok())
            .collect();

        let mut count = 0;
        for (att_id, note_id, file_name, relative_path, mime_type) in att_rows {
            // 1. Encrypt database metadata fields
            let enc_file_name = if is_encrypted_string(&file_name) {
                file_name
            } else {
                let aad = format!("attachments:{att_id}:file_name");
                encrypt_string(&file_name, dek, aad.as_bytes())?
            };

            let enc_mime_type = if is_encrypted_string(&mime_type) {
                mime_type
            } else {
                let aad = format!("attachments:{att_id}:mime_type");
                encrypt_string(&mime_type, dek, aad.as_bytes())?
            };

            conn.execute(
                "UPDATE attachments SET file_name = ?1, mime_type = ?2 WHERE id = ?3",
                params![enc_file_name, enc_mime_type, att_id],
            )
            .map_err(|e| {
                CryptoError::MigrationError(format!(
                    "Failed to update attachment metadata {att_id}: {e}"
                ))
            })?;

            // 2. Encrypt disk file if unencrypted
            let disk_path =
                StoragePaths::sanitize_subpath(&paths.attachments_dir, Path::new(&relative_path))
                    .map_err(|e| {
                    CryptoError::MigrationError(format!(
                        "Invalid attachment path {relative_path}: {e}"
                    ))
                })?;

            if disk_path.is_file() && !AttachmentStorage::is_encrypted_file(&disk_path) {
                let stored_name = relative_path
                    .split('/')
                    .next_back()
                    .unwrap_or(&relative_path);
                let aad = format!("attachments:{}:{stored_name}", note_id.trim());

                let staging_path = disk_path.with_extension(format!("mig_tmp.{}", Uuid::new_v4()));
                if let Err(e) = encrypt_file_to_file(&disk_path, &staging_path, dek, aad.as_bytes())
                {
                    let _ = std::fs::remove_file(&staging_path);
                    return Err(e);
                }

                // Verify staging file envelope before replacing original
                if !AttachmentStorage::is_encrypted_file(&staging_path) {
                    let _ = std::fs::remove_file(&staging_path);
                    return Err(CryptoError::MigrationError(format!(
                        "Staged encrypted attachment '{:?}' failed envelope verification",
                        staging_path
                    )));
                }

                // Atomically replace original plaintext file
                std::fs::rename(&staging_path, &disk_path).map_err(|e| {
                    let _ = std::fs::remove_file(&staging_path);
                    CryptoError::MigrationError(format!(
                        "Failed to replace attachment '{:?}' with encrypted payload: {e}",
                        disk_path
                    ))
                })?;

                count += 1;
            }
        }

        Ok(count)
    }

    /// Verifies post-migration storage consistency: 100% of notes and attachments must be encrypted and decryptable.
    pub fn verify_post_migration(
        conn: &Connection,
        paths: &StoragePaths,
        dek: &SecretBytes,
    ) -> Result<(), CryptoError> {
        // 1. Verify no notes remain unencrypted
        let unencrypted_notes_count: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM notes WHERE title NOT LIKE 'ENC$v1$%' OR content NOT LIKE 'ENC$v1%'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| CryptoError::MigrationError(format!("Post-migration note check query failed: {e}")))?;

        if unencrypted_notes_count != 0 {
            return Err(CryptoError::MigrationError(format!(
                "Verification failed: {unencrypted_notes_count} notes remain unencrypted in database"
            )));
        }

        // 2. Test decrypting all notes with DEK
        let mut note_stmt = conn
            .prepare("SELECT id, title, content FROM notes")
            .map_err(|e| {
                CryptoError::MigrationError(format!("Prepare verify notes query failed: {e}"))
            })?;

        let note_rows: Vec<(String, String, String)> = note_stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| CryptoError::MigrationError(format!("Query verify notes failed: {e}")))?
            .filter_map(|res| res.ok())
            .collect();

        for (id, enc_title, enc_content) in note_rows {
            let title_aad = format!("notes:{id}:title");
            decrypt_string(&enc_title, dek, title_aad.as_bytes()).map_err(|e| {
                CryptoError::MigrationError(format!(
                    "Post-migration decryption failed for note {id} title: {e}"
                ))
            })?;

            let content_aad = format!("notes:{id}:content");
            decrypt_string(&enc_content, dek, content_aad.as_bytes()).map_err(|e| {
                CryptoError::MigrationError(format!(
                    "Post-migration decryption failed for note {id} content: {e}"
                ))
            })?;
        }

        // 3. Verify no attachment records in SQLite remain unencrypted
        let unencrypted_att_count: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM attachments WHERE file_name NOT LIKE 'ENC$v1$%'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| {
                CryptoError::MigrationError(format!("Post-migration attachment check failed: {e}"))
            })?;

        if unencrypted_att_count != 0 {
            return Err(CryptoError::MigrationError(format!(
                "Verification failed: {unencrypted_att_count} attachments remain unencrypted in database"
            )));
        }

        // 4. Verify all attachment files on disk are authenticated envelopes
        let mut att_stmt = conn
            .prepare("SELECT relative_path FROM attachments")
            .map_err(|e| {
                CryptoError::MigrationError(format!("Prepare verify att disk query failed: {e}"))
            })?;

        let att_paths: Vec<String> = att_stmt
            .query_map([], |r| r.get(0))
            .map_err(|e| CryptoError::MigrationError(format!("Query verify att disk failed: {e}")))?
            .filter_map(|res| res.ok())
            .collect();

        for rel_path in att_paths {
            let disk_path =
                StoragePaths::sanitize_subpath(&paths.attachments_dir, Path::new(&rel_path))
                    .map_err(|e| CryptoError::MigrationError(e.to_string()))?;
            if disk_path.is_file() && !AttachmentStorage::is_encrypted_file(&disk_path) {
                return Err(CryptoError::MigrationError(format!(
                    "Verification failed: attachment file '{rel_path}' on disk is not encrypted"
                )));
            }
        }

        Ok(())
    }

    /// Executes the full safe migration workflow from start to finish.
    pub fn execute_full_migration(
        conn: &Connection,
        paths: &StoragePaths,
        dek: &SecretBytes,
    ) -> Result<MigrationReport, CryptoError> {
        // 1. Pre-migration integrity validation
        Self::validate_pre_migration(conn, paths)?;

        // 2. Pre-migration recovery checkpoint creation
        let checkpoint_path = Self::create_recovery_checkpoint(conn, paths)?;

        // 3. Migrate database notes
        let notes_migrated = Self::migrate_notes(conn, dek)?;

        // 4. Migrate database attachment records & files on disk
        let attachments_migrated = Self::migrate_attachments(conn, paths, dek)?;

        // 5. Purge unencrypted FTS search index
        if let Err(e) = SearchRepository::purge_plaintext_index(conn) {
            eprintln!("[Migration] Warning: Failed to purge unencrypted FTS index: {e}");
        }

        // 6. Post-migration verification
        Self::verify_post_migration(conn, paths, dek)?;

        Ok(MigrationReport {
            notes_migrated,
            attachments_migrated,
            checkpoint_path,
        })
    }

    /// Atomically persists the durable transition marker to disk (Phase 12, Task 18).
    pub fn save_transition_marker(
        paths: &StoragePaths,
        marker: &EncryptionTransitionMarker,
    ) -> Result<(), CryptoError> {
        let marker_file = paths.encryption_migration_marker_file();
        let tmp_file = paths
            .root
            .join(format!(".encryption_marker_{}.tmp", Uuid::new_v4()));

        let json = serde_json::to_string_pretty(marker).map_err(|e| {
            CryptoError::MigrationError(format!("Failed to serialize transition marker: {e}"))
        })?;

        {
            let mut file = std::fs::File::create(&tmp_file).map_err(|e| {
                CryptoError::MigrationError(format!(
                    "Failed to create transition marker temp file: {e}"
                ))
            })?;
            file.write_all(json.as_bytes()).map_err(|e| {
                CryptoError::MigrationError(format!("Failed to write transition marker: {e}"))
            })?;
            file.flush().map_err(|e| {
                CryptoError::MigrationError(format!("Failed to flush transition marker: {e}"))
            })?;
            file.sync_all().map_err(|e| {
                CryptoError::MigrationError(format!(
                    "Failed to sync transition marker to disk: {e}"
                ))
            })?;
        }

        std::fs::rename(&tmp_file, &marker_file).map_err(|e| {
            let _ = std::fs::remove_file(&tmp_file);
            CryptoError::MigrationError(format!(
                "Failed to atomically commit transition marker: {e}"
            ))
        })?;

        Ok(())
    }

    /// Reads and parses the durable transition marker from disk if present (Phase 12, Task 18).
    pub fn read_transition_marker(
        paths: &StoragePaths,
    ) -> Result<Option<EncryptionTransitionMarker>, CryptoError> {
        let marker_file = paths.encryption_migration_marker_file();
        if !marker_file.is_file() {
            return Ok(None);
        }

        let bytes = std::fs::read(&marker_file).map_err(|e| {
            CryptoError::MigrationError(format!("Failed to read transition marker: {e}"))
        })?;

        let marker: EncryptionTransitionMarker = match serde_json::from_slice(&bytes) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[Migration] Warning: Corrupt transition marker detected: {e}");
                return Ok(None);
            }
        };

        Ok(Some(marker))
    }

    /// Removes the durable transition marker from disk upon successful transition finalization or rollback.
    pub fn remove_transition_marker(paths: &StoragePaths) {
        let marker_file = paths.encryption_migration_marker_file();
        if marker_file.exists() {
            let _ = std::fs::remove_file(&marker_file);
        }
    }

    /// Detects interrupted encryption operations at application startup and determines the safe recovery decision (Phase 12, Task 18).
    pub fn detect_and_recover_at_startup(
        conn: &Connection,
        paths: &StoragePaths,
    ) -> Result<StartupRecoveryDecision, CryptoError> {
        let marker_opt = Self::read_transition_marker(paths)?;
        let meta_opt = SettingsRepository::get(conn, ENCRYPTION_METADATA_KEY)
            .ok()
            .flatten();

        let meta = match meta_opt {
            Some(raw) => EncryptionMetadata::from_json(&raw).ok(),
            None => None,
        };

        // Determine effective stage from marker and metadata
        let effective_stage = if let Some(ref m) = marker_opt {
            Some(m.stage.clone())
        } else {
            meta.as_ref().map(|em| em.migration_status.clone())
        };

        let checkpoint_path = marker_opt.as_ref().map(|m| m.checkpoint_path.clone());

        match effective_stage {
            Some(MigrationStatus::Verified) => {
                // If the operation already reached Verified, all rows and files were verified 100% ciphertext.
                // Auto-finalize to Active.
                if let Some(mut em) = meta {
                    em.migration_status = MigrationStatus::Active;
                    em.updated_at = chrono::Utc::now().to_rfc3339();
                    if let Ok(json) = em.to_json() {
                        let _ = SettingsRepository::set(conn, ENCRYPTION_METADATA_KEY, &json);
                    }
                }
                Self::remove_transition_marker(paths);
                Ok(StartupRecoveryDecision::AutoFinalizedVerified)
            }
            Some(MigrationStatus::SetupRequested) | Some(MigrationStatus::CheckpointCreated) => {
                // Interrupted prior to any authoritative database row modification.
                // Storage is 100% intact and unencrypted. Clean up marker and unstarted metadata.
                let _ = SettingsRepository::delete(conn, ENCRYPTION_METADATA_KEY);
                Self::remove_transition_marker(paths);
                Ok(StartupRecoveryDecision::CleanedUpUnstarted)
            }
            Some(MigrationStatus::MigrationInProgress)
            | Some(MigrationStatus::StagingPrepared)
            | Some(MigrationStatus::Failed(_)) => {
                // Interrupted during mutation: storage state is ambiguous.
                // Enter restricted recovery mode requiring user decision (Rollback or Resume).
                let stage = effective_stage.unwrap();
                Ok(StartupRecoveryDecision::RestrictedRecoveryRequired {
                    stage,
                    checkpoint_path,
                })
            }
            _ => {
                // Clean state: remove stray marker if any exists
                Self::remove_transition_marker(paths);
                Ok(StartupRecoveryDecision::None)
            }
        }
    }

    /// Safely rolls back an interrupted or corrupt migration using the pre-migration recovery checkpoint (Phase 12, Task 18).
    pub fn rollback_interrupted_migration(
        conn: &mut Connection,
        paths: &StoragePaths,
    ) -> Result<String, CryptoError> {
        let marker = Self::read_transition_marker(paths)?;
        let checkpoint_path = if let Some(ref m) = marker {
            PathBuf::from(&m.checkpoint_path)
        } else {
            let checkpoints = RestoreService::list_recovery_checkpoints(paths).map_err(|e| {
                CryptoError::MigrationError(format!("Failed to list checkpoints: {e}"))
            })?;
            if checkpoints.is_empty() {
                return Err(CryptoError::MigrationError(
                    "Cannot roll back: no pre-migration recovery checkpoint found on disk."
                        .to_string(),
                ));
            }
            PathBuf::from(&checkpoints[0].file_path)
        };

        if !checkpoint_path.is_file() {
            return Err(CryptoError::MigrationError(format!(
                "Pre-migration checkpoint archive '{:?}' not found on disk.",
                checkpoint_path
            )));
        }

        let request = BackupRestoreRequest {
            file_path: checkpoint_path.to_string_lossy().to_string(),
            operation_id: Some(format!("rollback_{}", Uuid::new_v4())),
        };

        RestoreService::execute_restore::<fn(crate::storage::models::BackupProgressEvent)>(
            conn, paths, request, None,
        )
        .map_err(|e| CryptoError::MigrationError(format!("Rollback restore failed: {e}")))?;

        let _ = SettingsRepository::delete(conn, ENCRYPTION_METADATA_KEY);
        Self::remove_transition_marker(paths);

        Ok(format!(
            "Successfully rolled back storage to pre-migration checkpoint: {}",
            checkpoint_path.display()
        ))
    }

    /// Resumes an interrupted migration from its current stage using the user's master password (Phase 12, Task 18).
    pub fn resume_interrupted_migration(
        conn: &Connection,
        paths: &StoragePaths,
        password: &str,
    ) -> Result<(EncryptionMetadata, SecretBytes), CryptoError> {
        let raw_meta = SettingsRepository::get(conn, ENCRYPTION_METADATA_KEY)
            .map_err(|e| CryptoError::InvalidMetadata(e.to_string()))?
            .ok_or(CryptoError::NotEnabled)?;

        let mut metadata = EncryptionMetadata::from_json(&raw_meta)?;

        let kek = derive_kek(password, &metadata.kdf)?;
        let dek = unwrap_dek(&metadata.wrapped_dek, &kek)?;

        if let Ok(Some(mut m)) = Self::read_transition_marker(paths) {
            m.stage = MigrationStatus::MigrationInProgress;
            m.updated_at = chrono::Utc::now().to_rfc3339();
            let _ = Self::save_transition_marker(paths, &m);
        }

        Self::migrate_notes(conn, &dek)?;

        if let Ok(Some(mut m)) = Self::read_transition_marker(paths) {
            m.stage = MigrationStatus::StagingPrepared;
            m.updated_at = chrono::Utc::now().to_rfc3339();
            let _ = Self::save_transition_marker(paths, &m);
        }
        Self::migrate_attachments(conn, paths, &dek)?;

        let _ = SearchRepository::purge_plaintext_index(conn);
        Self::verify_post_migration(conn, paths, &dek)?;

        metadata.migration_status = MigrationStatus::Active;
        metadata.updated_at = chrono::Utc::now().to_rfc3339();
        SettingsRepository::set(conn, ENCRYPTION_METADATA_KEY, &metadata.to_json()?)
            .map_err(|e| CryptoError::InvalidMetadata(e.to_string()))?;

        Self::remove_transition_marker(paths);

        Ok((metadata, dek))
    }

    /// Decrypts all encrypted notes in the database back to standard plaintext UTF-8 strings (Phase 12, Task 21).
    pub fn decrypt_all_notes(conn: &Connection, dek: &SecretBytes) -> Result<usize, CryptoError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, title, content FROM notes WHERE title LIKE 'ENC$v1$%' OR content LIKE 'ENC$v1$%'",
            )
            .map_err(|e| CryptoError::MigrationError(format!("Failed to prepare encrypted notes query: {e}")))?;

        let note_rows: Vec<(String, String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| {
                CryptoError::MigrationError(format!("Failed to query encrypted notes: {e}"))
            })?
            .filter_map(|res| res.ok())
            .collect();

        let mut count = 0;
        for (id, enc_title, enc_content) in note_rows {
            let plain_title = if is_encrypted_string(&enc_title) {
                let aad = format!("notes:{id}:title");
                decrypt_string(&enc_title, dek, aad.as_bytes())?
            } else {
                enc_title
            };

            let plain_content = if is_encrypted_string(&enc_content) {
                let aad = format!("notes:{id}:content");
                decrypt_string(&enc_content, dek, aad.as_bytes())?
            } else {
                enc_content
            };

            conn.execute(
                "UPDATE notes SET title = ?1, content = ?2 WHERE id = ?3",
                params![plain_title, plain_content, id],
            )
            .map_err(|e| {
                CryptoError::MigrationError(format!("Failed to update decrypted note {id}: {e}"))
            })?;

            count += 1;
        }

        Ok(count)
    }

    /// Decrypts all encrypted attachment metadata in SQLite and attachment payload files on disk (Phase 12, Task 21).
    pub fn decrypt_all_attachments(
        conn: &Connection,
        paths: &StoragePaths,
        dek: &SecretBytes,
    ) -> Result<usize, CryptoError> {
        let mut stmt = conn
            .prepare("SELECT id, note_id, file_name, relative_path, mime_type, file_size FROM attachments")
            .map_err(|e| CryptoError::MigrationError(format!("Failed to prepare attachments query: {e}")))?;

        let att_rows: Vec<(String, String, String, String, String, i64)> = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })
            .map_err(|e| CryptoError::MigrationError(format!("Failed to query attachments: {e}")))?
            .filter_map(|res| res.ok())
            .collect();

        let mut count = 0;
        for (att_id, note_id, file_name, relative_path, mime_type, mut file_size) in att_rows {
            // 1. Decrypt SQLite metadata fields
            let plain_file_name = if is_encrypted_string(&file_name) {
                let aad = format!("attachments:{att_id}:file_name");
                decrypt_string(&file_name, dek, aad.as_bytes())?
            } else {
                file_name
            };

            let plain_mime_type = if is_encrypted_string(&mime_type) {
                let aad = format!("attachments:{att_id}:mime_type");
                decrypt_string(&mime_type, dek, aad.as_bytes())?
            } else {
                mime_type
            };

            // 2. Decrypt disk file if encrypted (starts with 0x01)
            let disk_path =
                StoragePaths::sanitize_subpath(&paths.attachments_dir, Path::new(&relative_path))
                    .map_err(|e| {
                    CryptoError::MigrationError(format!(
                        "Invalid attachment path {relative_path}: {e}"
                    ))
                })?;

            if disk_path.is_file() && AttachmentStorage::is_encrypted_file(&disk_path) {
                let stored_name = relative_path
                    .split('/')
                    .next_back()
                    .unwrap_or(&relative_path);
                let aad = format!("attachments:{}:{stored_name}", note_id.trim());

                let staging_path = disk_path.with_extension(format!("dec_tmp.{}", Uuid::new_v4()));
                if let Err(e) = decrypt_file_to_file(&disk_path, &staging_path, dek, aad.as_bytes())
                {
                    let _ = std::fs::remove_file(&staging_path);
                    return Err(e);
                }

                // Verify staging file is no longer encrypted
                if AttachmentStorage::is_encrypted_file(&staging_path) {
                    let _ = std::fs::remove_file(&staging_path);
                    return Err(CryptoError::MigrationError(format!(
                        "Staged decrypted attachment '{:?}' still appears encrypted",
                        staging_path
                    )));
                }

                if let Ok(meta) = std::fs::metadata(&staging_path) {
                    file_size = meta.len() as i64;
                }

                // Atomically replace original encrypted file with decrypted file
                std::fs::rename(&staging_path, &disk_path).map_err(|e| {
                    let _ = std::fs::remove_file(&staging_path);
                    CryptoError::MigrationError(format!(
                        "Failed to replace attachment '{:?}' with decrypted payload: {e}",
                        disk_path
                    ))
                })?;

                count += 1;
            }

            conn.execute(
                "UPDATE attachments SET file_name = ?1, mime_type = ?2, file_size = ?3 WHERE id = ?4",
                params![plain_file_name, plain_mime_type, file_size, att_id],
            )
            .map_err(|e| {
                CryptoError::MigrationError(format!("Failed to update decrypted attachment metadata {att_id}: {e}"))
            })?;
        }

        Ok(count)
    }

    /// Verifies post-decryption storage consistency: 0% of notes and attachments are encrypted (Phase 12, Task 21).
    pub fn verify_post_decryption(
        conn: &Connection,
        paths: &StoragePaths,
    ) -> Result<(), CryptoError> {
        let enc_notes_count: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM notes WHERE title LIKE 'ENC$v1$%' OR content LIKE 'ENC$v1$%'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| {
                CryptoError::MigrationError(format!("Post-decryption note check failed: {e}"))
            })?;

        if enc_notes_count != 0 {
            return Err(CryptoError::MigrationError(format!(
                "Decryption verification failed: {enc_notes_count} notes remain encrypted"
            )));
        }

        let mut stmt = conn
            .prepare("SELECT relative_path FROM attachments")
            .map_err(|e| {
                CryptoError::MigrationError(format!("Prepare verify attachments failed: {e}"))
            })?;

        let rel_paths: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .map_err(|e| CryptoError::MigrationError(format!("Query attachments failed: {e}")))?
            .filter_map(|res| res.ok())
            .collect();

        for rel in rel_paths {
            let disk_path = StoragePaths::sanitize_subpath(&paths.attachments_dir, Path::new(&rel))
                .map_err(|e| CryptoError::MigrationError(e.to_string()))?;
            if disk_path.is_file() && AttachmentStorage::is_encrypted_file(&disk_path) {
                return Err(CryptoError::MigrationError(format!(
                    "Decryption verification failed: attachment file '{rel}' remains encrypted on disk"
                )));
            }
        }

        Ok(())
    }

    /// Disables encryption completely, decrypting all database notes and attachments on disk (Phase 12, Task 21).
    pub fn disable_encryption(
        conn: &Connection,
        paths: &StoragePaths,
        password: &str,
    ) -> Result<DisableEncryptionReport, CryptoError> {
        // 1. Authenticate credential and recover DEK
        let meta_json = SettingsRepository::get(conn, ENCRYPTION_METADATA_KEY)
            .map_err(|e| {
                CryptoError::MigrationError(format!("Failed to query encryption metadata: {e}"))
            })?
            .ok_or(CryptoError::NotEnabled)?;

        let metadata = EncryptionMetadata::from_json(&meta_json)?;
        if !metadata.enabled {
            return Err(CryptoError::NotEnabled);
        }

        let kek = derive_kek(password, &metadata.kdf)?;
        let dek = unwrap_dek(&metadata.wrapped_dek, &kek)?;

        // 2. Validate pre-decryption database integrity
        Self::validate_pre_migration(conn, paths)?;

        // 3. Create verified recovery checkpoint
        let checkpoint_path = Self::create_recovery_checkpoint(conn, paths)?;

        // Durable operation marker: CheckpointCreated
        let op_id = Uuid::new_v4().to_string();
        let mut marker = EncryptionTransitionMarker {
            operation_id: op_id.clone(),
            operation_type: "disable_encryption".to_string(),
            stage: MigrationStatus::CheckpointCreated,
            checkpoint_path: checkpoint_path.to_string_lossy().to_string(),
            started_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        Self::save_transition_marker(paths, &marker)?;

        // Transition marker: MigrationInProgress
        marker.stage = MigrationStatus::MigrationInProgress;
        marker.updated_at = chrono::Utc::now().to_rfc3339();
        Self::save_transition_marker(paths, &marker)?;

        // 4. Decrypt all notes
        let notes_decrypted = Self::decrypt_all_notes(conn, &dek)?;

        // 5. Decrypt all attachments
        let attachments_decrypted = Self::decrypt_all_attachments(conn, paths, &dek)?;

        // Transition marker: StagingPrepared
        marker.stage = MigrationStatus::StagingPrepared;
        marker.updated_at = chrono::Utc::now().to_rfc3339();
        Self::save_transition_marker(paths, &marker)?;

        // 6. Verify post-decryption
        Self::verify_post_decryption(conn, paths)?;

        // 7. Remove encryption metadata from settings table
        SettingsRepository::delete(conn, ENCRYPTION_METADATA_KEY).map_err(|e| {
            CryptoError::MigrationError(format!("Failed to delete encryption metadata: {e}"))
        })?;

        // 8. Rebuild plaintext FTS index and triggers
        SearchRepository::rebuild_search_index(conn).map_err(|e| {
            CryptoError::MigrationError(format!("Failed to rebuild search index: {e}"))
        })?;

        // 9. Remove transition marker
        Self::remove_transition_marker(paths);

        Ok(DisableEncryptionReport {
            notes_decrypted,
            attachments_decrypted,
            checkpoint_path: checkpoint_path.to_string_lossy().to_string(),
        })
    }
}
