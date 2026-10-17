use serde::{Deserialize, Serialize};
use tauri::State;

use crate::storage::crypto::{ChangePasswordDto, CryptoSession, SetupEncryptionDto, UnlockDto};
use crate::storage::{Database, StoragePaths};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionStatusDto {
    pub enabled: bool,
    pub is_locked: bool,
    pub algorithm: Option<String>,
    pub kdf_algorithm: Option<String>,
    pub format_version: Option<u32>,
    pub created_at: Option<String>,
    pub migration_status: Option<String>,
    pub is_interrupted: bool,
    pub recovery_checkpoint_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionSetupResult {
    pub success: bool,
    pub format_version: u32,
    pub algorithm: String,
    pub message: String,
}

#[tauri::command]
pub fn get_encryption_status(
    paths: State<'_, StoragePaths>,
    session: State<'_, CryptoSession>,
) -> Result<EncryptionStatusDto, String> {
    let enabled = session.is_encryption_enabled();
    let is_locked = session.is_locked();
    let is_interrupted = session.is_interrupted();
    let migration_status = session.get_migration_status().map(|s| format!("{:?}", s));

    let recovery_checkpoint_path = if is_interrupted {
        crate::storage::crypto::MigrationService::read_transition_marker(&paths)
            .ok()
            .flatten()
            .map(|m| m.checkpoint_path)
            .or_else(|| {
                crate::storage::services::RestoreService::list_recovery_checkpoints(&paths)
                    .ok()
                    .and_then(|list| list.into_iter().next().map(|c| c.file_path))
            })
    } else {
        None
    };

    Ok(EncryptionStatusDto {
        enabled,
        is_locked,
        algorithm: if enabled {
            Some("AES-256-GCM".to_string())
        } else {
            None
        },
        kdf_algorithm: if enabled {
            Some("argon2id".to_string())
        } else {
            None
        },
        format_version: if enabled { Some(1) } else { None },
        created_at: None,
        migration_status,
        is_interrupted,
        recovery_checkpoint_path,
    })
}

#[tauri::command]
pub fn setup_encryption(
    db: State<'_, Database>,
    paths: State<'_, StoragePaths>,
    session: State<'_, CryptoSession>,
    dto: SetupEncryptionDto,
) -> Result<EncryptionSetupResult, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    let meta = session
        .setup_encryption(&conn, &paths, dto)
        .map_err(|e| e.user_friendly_message())?;

    Ok(EncryptionSetupResult {
        success: true,
        format_version: meta.format_version,
        algorithm: meta.cipher_algorithm,
        message: "Encryption has been successfully enabled. A verified pre-migration backup checkpoint was saved."
            .to_string(),
    })
}

#[tauri::command]
pub fn unlock_application(
    session: State<'_, CryptoSession>,
    dto: UnlockDto,
) -> Result<bool, String> {
    session
        .unlock(dto)
        .map(|_| true)
        .map_err(|e| e.user_friendly_message())
}

#[tauri::command]
pub fn lock_application(
    paths: State<'_, StoragePaths>,
    session: State<'_, CryptoSession>,
) -> Result<bool, String> {
    session.lock();
    let storage = crate::storage::attachment_storage::AttachmentStorage::new((*paths).clone());
    let _ = storage.cleanup_decrypted_attachments();
    Ok(true)
}

#[tauri::command]
pub fn change_encryption_password(
    db: State<'_, Database>,
    session: State<'_, CryptoSession>,
    dto: ChangePasswordDto,
) -> Result<bool, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    session
        .change_password(&conn, dto)
        .map(|_| true)
        .map_err(|e| e.user_friendly_message())
}

#[tauri::command]
pub fn rollback_interrupted_migration(
    db: State<'_, Database>,
    paths: State<'_, StoragePaths>,
    session: State<'_, CryptoSession>,
) -> Result<String, String> {
    let mut conn = db.conn.lock().map_err(|e| e.to_string())?;
    session
        .rollback_interrupted_migration(&mut conn, &paths)
        .map_err(|e| e.user_friendly_message())
}

#[tauri::command]
pub fn resume_interrupted_migration(
    db: State<'_, Database>,
    paths: State<'_, StoragePaths>,
    session: State<'_, CryptoSession>,
    dto: UnlockDto,
) -> Result<bool, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    session
        .resume_interrupted_migration(&conn, &paths, &dto.password)
        .map(|_| true)
        .map_err(|e| e.user_friendly_message())
}

#[tauri::command]
pub fn disable_encryption(
    db: State<'_, Database>,
    paths: State<'_, StoragePaths>,
    session: State<'_, CryptoSession>,
    dto: UnlockDto,
) -> Result<bool, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    session
        .disable_encryption(&conn, &paths, &dto.password)
        .map(|_| true)
        .map_err(|e| e.user_friendly_message())
}

#[tauri::command]
pub fn get_security_diagnostics(
    limit: Option<usize>,
) -> Result<Vec<crate::storage::crypto::SecurityLogEntry>, String> {
    Ok(crate::storage::crypto::get_security_audit_log(
        limit.unwrap_or(50),
    ))
}
