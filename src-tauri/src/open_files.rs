//! "Open with Personal Notepad": text and Markdown files handed to the app by the desktop
//! (`personal-notepad file.md`, the file manager's "Open With", or a double click).
//!
//! * The launch arguments are parsed into file paths (plain paths and `file://` URIs).
//! * A second launch while the app is already running forwards its paths to the running instance
//!   over a private Unix socket and exits, so the file opens in the existing window.
//! * Paths wait in [`PendingOpenFiles`] until the frontend is ready (and unlocked), then each file
//!   becomes an *external document*: it is listed in the sidebar's External section, never in the
//!   notes list, and edits are written back to the file itself.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;
use serde::Serialize;

use crate::storage::models::{ImportFormat, Note};
use crate::storage::services::ImportExportService;

/// Must match `identifier` in `tauri.conf.json` (checked by a test) so a second launch can find
/// the data directory of the running instance before any Tauri code has started.
pub const APP_IDENTIFIER: &str = "com.personalnotepad.app";
/// Never open more than this many files from one launch.
pub const MAX_LAUNCH_FILES: usize = 20;

static SOCKET_TO_CLEAN: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Remembers the listener socket so it can be removed when the app closes.
pub fn remember_socket_for_cleanup(path: PathBuf) {
    let _ = SOCKET_TO_CLEAN.set(path);
}

/// Removes the listener socket file (called when the window is destroyed). A leftover file would
/// be harmless, the next launch replaces it, but there is no reason to leave it behind.
pub fn remove_socket_file() {
    if let Some(path) = SOCKET_TO_CLEAN.get() {
        let _ = std::fs::remove_file(path);
    }
}

/// Event sent to the window when new paths were queued by a second launch.
pub const OPEN_FILES_QUEUED_EVENT: &str = "open-files-queued";

// ---------------------------------------------------------------------------------------------
// Launch arguments
// ---------------------------------------------------------------------------------------------

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Decodes `%XX` escapes (as used in `file://` URIs). Invalid escapes are kept literally.
fn percent_decode(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                out.push(hi << 4 | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// Turns one command-line argument into a path: a plain path, or a `file://` URI.
fn argument_to_path(arg: &str, cwd: &Path) -> Option<PathBuf> {
    let trimmed = arg.trim();
    if trimmed.is_empty() {
        return None;
    }
    let raw: PathBuf = if let Some(rest) = trimmed
        .strip_prefix("file://")
        .or_else(|| trimmed.strip_prefix("FILE://"))
    {
        // file:///home/me/a.md  or  file://localhost/home/me/a.md
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        if !rest.starts_with('/') {
            return None;
        }
        let decoded = percent_decode(rest);
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            PathBuf::from(std::ffi::OsString::from_vec(decoded))
        }
        #[cfg(not(unix))]
        {
            PathBuf::from(String::from_utf8_lossy(&decoded).into_owned())
        }
    } else if trimmed.contains("://") {
        // some other URI scheme (http:, trash:, ...): not a local file
        return None;
    } else {
        PathBuf::from(trimmed)
    };

    Some(if raw.is_absolute() {
        raw
    } else {
        cwd.join(raw)
    })
}

/// Extracts the file paths from the process arguments (`args[0]` is the program name).
/// Options such as `--foo` are ignored; everything after a lone `--` is treated as a path.
pub fn parse_launch_paths<I>(args: I, cwd: &Path) -> Vec<PathBuf>
where
    I: IntoIterator<Item = String>,
{
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut only_paths = false;
    for arg in args.into_iter().skip(1) {
        if !only_paths {
            if arg == "--" {
                only_paths = true;
                continue;
            }
            if arg.starts_with('-') {
                continue;
            }
        }
        if let Some(path) = argument_to_path(&arg, cwd) {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
        if paths.len() >= MAX_LAUNCH_FILES {
            break;
        }
    }
    paths
}

// ---------------------------------------------------------------------------------------------
// Pending queue and report types
// ---------------------------------------------------------------------------------------------

/// Paths waiting to be opened once the window is ready and the app is unlocked.
#[derive(Debug, Default)]
pub struct PendingOpenFiles(Mutex<Vec<PathBuf>>);

impl PendingOpenFiles {
    pub fn new(initial: Vec<PathBuf>) -> Self {
        Self(Mutex::new(initial))
    }

    pub fn push_all(&self, paths: Vec<PathBuf>) {
        if let Ok(mut queue) = self.0.lock() {
            for path in paths {
                if !queue.contains(&path) && queue.len() < MAX_LAUNCH_FILES {
                    queue.push(path);
                }
            }
        }
    }

    pub fn take_all(&self) -> Vec<PathBuf> {
        self.0
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.0.lock().map(|queue| queue.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedFile {
    pub path: String,
    pub file_name: String,
    pub note_id: String,
    pub title: String,
    pub format: String,
    /// False when the file was already open as an identical note and that note was reused.
    pub created: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedFile {
    pub path: String,
    pub file_name: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenFilesReport {
    pub opened: Vec<OpenedFile>,
    pub failed: Vec<FailedFile>,
}

// ---------------------------------------------------------------------------------------------
// Turning files into notes
// ---------------------------------------------------------------------------------------------

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file")
        .to_string()
}

/// Drops the technical prefixes of the shared import errors so the message reads naturally.
fn friendly_error(message: &str) -> String {
    message
        .strip_prefix("Validation failed: ")
        .or_else(|| message.strip_prefix("Cannot read file: "))
        .unwrap_or(message)
        .replace("can be imported", "can be opened")
}

/// Prefix of the ids of external documents, so they can never be mistaken for a note id.
pub const EXTERNAL_ID_PREFIX: &str = "ext-";

pub fn is_external_id(id: &str) -> bool {
    id.starts_with(EXTERNAL_ID_PREFIX)
}

/// A file opened from outside the notebook ("Open with"). The file itself stays the single source
/// of truth: only its location is remembered, nothing is copied into the notes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalDocument {
    pub id: String,
    pub path: String,
    pub file_name: String,
    pub format: String,
    pub opened_at: String,
    /// Pinned documents stay in the External list after a restart; the others are session-only.
    pub pinned: bool,
}

fn format_for(path: &Path) -> &'static str {
    // .md/.markdown are Markdown; every other text file (.txt, .log, .json, ...) is plain text
    match ImportExportService::detect_format_from_path(path) {
        Some(ImportFormat::Md) => "md",
        _ => "txt",
    }
}

pub fn ensure_external_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS external_documents (
            id TEXT PRIMARY KEY NOT NULL,
            path TEXT NOT NULL UNIQUE,
            opened_at TEXT NOT NULL,
            pinned INTEGER NOT NULL DEFAULT 0
        );",
    )?;
    // Lists created before pinning existed
    let has_pinned = conn
        .prepare("SELECT 1 FROM pragma_table_info('external_documents') WHERE name = 'pinned'")?
        .exists([])?;
    if !has_pinned {
        conn.execute(
            "ALTER TABLE external_documents ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    Ok(())
}

/// Forgets the external documents that were not pinned (called once at startup).
pub fn prune_unpinned_external(conn: &Connection) {
    if ensure_external_table(conn).is_ok() {
        let _ = conn.execute("DELETE FROM external_documents WHERE pinned = 0", []);
    }
}

pub fn set_external_pinned(conn: &Connection, id: &str, pinned: bool) -> Result<bool, String> {
    ensure_external_table(conn).map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE external_documents SET pinned = ?1 WHERE id = ?2",
        rusqlite::params![pinned as i64, id],
    )
    .map(|n| n > 0)
    .map_err(|e| e.to_string())
}

fn external_from_row(
    id: String,
    path: String,
    opened_at: String,
    pinned: bool,
) -> ExternalDocument {
    let path_buf = PathBuf::from(&path);
    ExternalDocument {
        id,
        file_name: display_name(&path_buf),
        format: format_for(&path_buf).to_string(),
        path,
        opened_at,
        pinned,
    }
}

pub fn list_external_documents(conn: &Connection) -> Result<Vec<ExternalDocument>, String> {
    ensure_external_table(conn).map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, path, opened_at, pinned FROM external_documents ORDER BY pinned DESC, opened_at DESC, rowid DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(external_from_row(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get::<_, i64>(3)? != 0,
            ))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn get_external_document(
    conn: &Connection,
    id: &str,
) -> Result<Option<ExternalDocument>, String> {
    ensure_external_table(conn).map_err(|e| e.to_string())?;
    match conn.query_row(
        "SELECT id, path, opened_at, pinned FROM external_documents WHERE id = ?1",
        [id],
        |row| {
            Ok(external_from_row(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get::<_, i64>(3)? != 0,
            ))
        },
    ) {
        Ok(doc) => Ok(Some(doc)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

/// Forgets the document. The file on disk is never touched.
pub fn close_external_document(conn: &Connection, id: &str) -> Result<bool, String> {
    ensure_external_table(conn).map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM external_documents WHERE id = ?1", [id])
        .map(|n| n > 0)
        .map_err(|e| e.to_string())
}

fn register_external(conn: &Connection, canonical: &Path) -> Result<ExternalDocument, String> {
    ensure_external_table(conn).map_err(|e| e.to_string())?;
    let path = canonical.to_string_lossy().into_owned();
    let now = chrono::Utc::now().to_rfc3339();
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM external_documents WHERE path = ?1",
            [&path],
            |row| row.get(0),
        )
        .ok();
    let id = match existing {
        Some(id) => {
            conn.execute(
                "UPDATE external_documents SET opened_at = ?1 WHERE id = ?2",
                rusqlite::params![now, id],
            )
            .map_err(|e| e.to_string())?;
            id
        }
        None => {
            let id = format!("{EXTERNAL_ID_PREFIX}{}", uuid::Uuid::new_v4());
            conn.execute(
                "INSERT INTO external_documents (id, path, opened_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, path, now],
            )
            .map_err(|e| e.to_string())?;
            id
        }
    };
    let pinned = conn
        .query_row(
            "SELECT pinned FROM external_documents WHERE id = ?1",
            [&id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
        != 0;
    Ok(external_from_row(id, path, now, pinned))
}

fn read_external_text(path: &Path) -> Result<String, String> {
    let validated = ImportExportService::read_and_validate_file(path, ImportFormat::Txt)
        .map_err(|error| friendly_error(&error.to_string()))?;
    Ok(validated.text)
}

/// The document as a note, so the editor can show it like any other note. Read fresh from disk.
pub fn load_external_note(conn: &Connection, id: &str) -> Result<Option<Note>, String> {
    let Some(doc) = get_external_document(conn, id)? else {
        return Ok(None);
    };
    let content = read_external_text(Path::new(&doc.path))?;
    Ok(Some(external_note(&doc, content)))
}

fn external_note(doc: &ExternalDocument, content: String) -> Note {
    let modified = std::fs::metadata(&doc.path)
        .and_then(|m| m.modified())
        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
        .unwrap_or_else(|_| doc.opened_at.clone());
    Note {
        id: doc.id.clone(),
        title: ImportExportService::derive_proposed_title(&doc.file_name),
        content,
        format: doc.format.clone(),
        notebook_id: None,
        created_at: doc.opened_at.clone(),
        modified_at: modified,
        is_favorite: false,
        is_pinned: false,
        is_deleted: false,
        deleted_at: None,
    }
}

/// Saves edited content back into the file, keeping its BOM and Windows line endings.
pub fn save_external_note(conn: &Connection, id: &str, content: &str) -> Result<Note, String> {
    let doc = get_external_document(conn, id)?
        .ok_or_else(|| "This document is no longer open.".to_string())?;
    let path = PathBuf::from(&doc.path);
    let existing = std::fs::read(&path).map_err(|_| {
        "The file was moved or deleted, so it cannot be saved. Close it and reopen the file."
            .to_string()
    })?;
    let had_bom = existing.starts_with(&[0xEF, 0xBB, 0xBF]);
    let crlf = existing.windows(2).any(|w| w == b"\r\n");

    let normalized = content.replace("\r\n", "\n");
    let mut bytes: Vec<u8> = Vec::with_capacity(normalized.len() + 3);
    if had_bom {
        bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    if crlf {
        bytes.extend_from_slice(normalized.replace('\n', "\r\n").as_bytes());
    } else {
        bytes.extend_from_slice(normalized.as_bytes());
    }
    ImportExportService::write_file_atomically(&path, &bytes)
        .map_err(|error| format!("Could not save the file: {error}"))?;
    Ok(external_note(&doc, normalized))
}

fn open_one(conn: &Connection, path: &Path) -> Result<OpenedFile, String> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| "The file does not exist or cannot be reached.".to_string())?;
    // Only checks that it is a readable text file; the content itself is read when it is shown
    read_external_text(&canonical)?;
    let doc = register_external(conn, &canonical)?;
    Ok(OpenedFile {
        path: doc.path,
        file_name: doc.file_name,
        note_id: doc.id,
        title: ImportExportService::derive_proposed_title(&display_name(&canonical)),
        format: doc.format,
        created: true,
    })
}

/// Opens every path as an external document (never as a note in the main list). One bad file never stops the others.
pub fn open_files_as_notes(conn: &Connection, paths: &[PathBuf]) -> OpenFilesReport {
    let mut report = OpenFilesReport::default();
    for path in paths {
        match open_one(conn, path) {
            Ok(opened) => report.opened.push(opened),
            Err(message) => report.failed.push(FailedFile {
                path: path.to_string_lossy().into_owned(),
                file_name: display_name(path),
                message,
            }),
        }
    }
    report
}

// ---------------------------------------------------------------------------------------------
// Second launch -> running instance (Unix socket)
// ---------------------------------------------------------------------------------------------

/// Where the running instance keeps its data, found the same way Tauri finds it.
pub fn default_data_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    let dir = base.join(APP_IDENTIFIER);
    // Honour the isolated test root in debug builds, exactly like the application does
    crate::storage::paths::resolve_app_data_root(dir).ok()
}

/// Unix socket paths are limited to about 108 bytes, which a long home or data directory can
/// exceed. The socket therefore lives in the (short, private) runtime directory and is named after
/// a hash of the data directory, so separate data directories never share a socket.
pub fn socket_path(data_dir: &Path) -> PathBuf {
    use sha2::{Digest, Sha256};
    let normalized = std::fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
    let digest = Sha256::digest(normalized.to_string_lossy().as_bytes());
    let short: String = digest
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect();

    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute() && dir.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    runtime_dir.join(format!("personal-notepad-{short}.sock"))
}

/// Wire format: one line per message. `OPEN <absolute path>` or `FOCUS`.
pub fn encode_message(paths: &[PathBuf]) -> String {
    if paths.is_empty() {
        return "FOCUS\n".to_string();
    }
    paths
        .iter()
        .filter_map(|p| p.to_str())
        .filter(|p| !p.contains('\n'))
        .map(|p| format!("OPEN {p}\n"))
        .collect()
}

/// Paths contained in a received message (a `FOCUS` message has none).
pub fn decode_message(message: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for line in message.lines() {
        if let Some(rest) = line.strip_prefix("OPEN ") {
            let path = PathBuf::from(rest);
            if path.is_absolute() && !paths.contains(&path) && paths.len() < MAX_LAUNCH_FILES {
                paths.push(path);
            }
        }
    }
    paths
}

/// For a second launch: if another instance is running, hand it the files (or ask it to come to
/// the front) and report true, so this process can exit without opening a window.
#[cfg(unix)]
pub fn forward_launch(data_dir: &Path, paths: &[PathBuf]) -> bool {
    let lock =
        crate::storage::StoragePaths::from_root(data_dir.to_path_buf()).app_instance_lock_file();
    let Some(running_pid) = std::fs::read_to_string(lock)
        .ok()
        .and_then(|text| text.trim().parse::<u32>().ok())
    else {
        return false;
    };
    if running_pid == std::process::id() || !Path::new(&format!("/proc/{running_pid}")).exists() {
        return false;
    }
    // The running instance may still be starting up and not listening yet: retry briefly
    for attempt in 0..8 {
        if ipc::forward_to_running_instance(data_dir, paths) {
            return true;
        }
        if attempt < 7 {
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
    }
    false
}

#[cfg(unix)]
pub mod ipc {
    use super::*;
    use std::io::{Read, Write};
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::time::Duration;

    /// Largest message the listener accepts (an attacker cannot make it buffer without limit).
    const MAX_MESSAGE_BYTES: u64 = 64 * 1024;

    /// Sends the paths (or a "bring your window to the front" request) to the running instance.
    /// Returns true when the message was delivered.
    pub fn forward_to_running_instance(data_dir: &Path, paths: &[PathBuf]) -> bool {
        let socket = socket_path(data_dir);
        // Only talk to a socket that belongs to this user (the fallback directory may be shared)
        let owned_by_me = std::fs::symlink_metadata(&socket)
            .ok()
            .zip(std::fs::metadata("/proc/self").ok())
            .is_some_and(|(socket_meta, me)| {
                socket_meta.file_type().is_socket() && socket_meta.uid() == me.uid()
            });
        if !owned_by_me {
            return false;
        }
        let Ok(mut stream) = UnixStream::connect(&socket) else {
            return false;
        };
        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
        let message = encode_message(paths);
        stream.write_all(message.as_bytes()).is_ok() && stream.flush().is_ok()
    }

    /// Starts listening for messages from later launches. `on_message` runs on a background
    /// thread with the paths (empty for a focus request).
    pub fn start_listener<F>(data_dir: &Path, on_message: F) -> std::io::Result<()>
    where
        F: Fn(Vec<PathBuf>) + Send + 'static,
    {
        let socket = socket_path(data_dir);
        // A socket file left behind by a crashed instance would block binding
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket)?;
        // Keep the socket private even for the instant before the permissions below are applied
        let _ = std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600));

        std::thread::Builder::new()
            .name("open-files-listener".to_string())
            .spawn(move || {
                for connection in listener.incoming() {
                    let Ok(stream) = connection else { continue };
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                    let mut text = String::new();
                    if stream
                        .take(MAX_MESSAGE_BYTES)
                        .read_to_string(&mut text)
                        .is_ok()
                    {
                        on_message(decode_message(&text));
                    }
                }
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use crate::storage::paths::StoragePaths;
    use std::fs;
    use uuid::Uuid;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_plain_paths_uris_and_ignores_flags() {
        let cwd = Path::new("/home/me/work");
        let paths = parse_launch_paths(
            args(&[
                "personal-notepad",
                "--some-flag",
                "/tmp/a.md",
                "notes/b.txt",
                "file:///home/me/My%20Notes/c%C3%A9.md",
                "file://localhost/tmp/d.txt",
                "/tmp/a.md",
                "https://example.com/x.md",
                "trash:///old.txt",
                "",
            ]),
            cwd,
        );
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/tmp/a.md"),
                PathBuf::from("/home/me/work/notes/b.txt"),
                PathBuf::from("/home/me/My Notes/cé.md"),
                PathBuf::from("/tmp/d.txt"),
            ],
            "flags, remote URIs and duplicates are dropped; relative paths use the working dir"
        );
    }

    #[test]
    fn double_dash_makes_everything_after_it_a_path_and_count_is_capped() {
        let cwd = Path::new("/");
        let paths = parse_launch_paths(args(&["np", "--", "-weird-name.txt"]), cwd);
        assert_eq!(paths, vec![PathBuf::from("/-weird-name.txt")]);

        let many: Vec<String> = std::iter::once("np".to_string())
            .chain((0..100).map(|i| format!("/tmp/f{i}.txt")))
            .collect();
        assert_eq!(parse_launch_paths(many, cwd).len(), MAX_LAUNCH_FILES);
        assert!(parse_launch_paths(args(&["np"]), cwd).is_empty());
    }

    #[test]
    fn percent_decoding_keeps_invalid_escapes() {
        assert_eq!(percent_decode("a%20b"), b"a b");
        assert_eq!(percent_decode("100%"), b"100%");
        assert_eq!(percent_decode("%zz%4"), b"%zz%4");
        assert_eq!(percent_decode("%E2%9C%93"), "✓".as_bytes());
    }

    #[test]
    fn pending_queue_dedupes_drains_and_is_bounded() {
        let queue = PendingOpenFiles::new(vec![PathBuf::from("/a")]);
        queue.push_all(vec![PathBuf::from("/a"), PathBuf::from("/b")]);
        assert_eq!(queue.len(), 2);
        assert_eq!(
            queue.take_all(),
            vec![PathBuf::from("/a"), PathBuf::from("/b")]
        );
        assert!(queue.is_empty());
        queue.push_all((0..100).map(|i| PathBuf::from(format!("/f{i}"))).collect());
        assert_eq!(queue.len(), MAX_LAUNCH_FILES);
    }

    #[test]
    fn message_round_trip_and_rejects_unsafe_input() {
        let paths = vec![PathBuf::from("/tmp/a b.md"), PathBuf::from("/tmp/c.txt")];
        assert_eq!(decode_message(&encode_message(&paths)), paths);
        assert_eq!(encode_message(&[]), "FOCUS\n");
        assert!(decode_message("FOCUS\n").is_empty());
        // relative paths, unknown verbs and a path with a newline cannot be smuggled in
        assert!(decode_message("OPEN relative.md\nRUN /bin/sh\n").is_empty());
        assert_eq!(encode_message(&[PathBuf::from("/tmp/x\ny")]), "");
    }

    struct TestEnv {
        conn: Connection,
        dir: PathBuf,
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn setup() -> TestEnv {
        let dir = std::env::temp_dir().join(format!("pn_open_files_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(dir.clone());
        paths.ensure_directories().unwrap();
        let mut conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut conn).unwrap();
        TestEnv { conn, dir }
    }

    #[test]
    fn opened_files_become_external_documents_not_notes() {
        let env = setup();
        let md = env.dir.join("Work Report.md");
        let txt = env.dir.join("todo.txt");
        let json = env.dir.join("data.json");
        fs::write(&md, "\u{feff}# Heading\r\n\r\nbody").unwrap();
        fs::write(&txt, "plain text").unwrap();
        fs::write(&json, "{\"notepad_version\": 1, \"notes\": []}").unwrap();

        let report = open_files_as_notes(&env.conn, &[md.clone(), txt, json]);
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        let formats: Vec<_> = report
            .opened
            .iter()
            .map(|o| (o.title.as_str(), o.format.as_str()))
            .collect();
        assert_eq!(
            formats,
            vec![("Work Report", "md"), ("todo", "txt"), ("data", "txt")]
        );
        assert!(report.opened.iter().all(|o| is_external_id(&o.note_id)));

        let notes: i64 = env
            .conn
            .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(notes, 0, "nothing lands in the main notes list");
        assert_eq!(list_external_documents(&env.conn).unwrap().len(), 3);

        let note = load_external_note(&env.conn, &report.opened[0].note_id)
            .unwrap()
            .unwrap();
        assert_eq!(note.content, "# Heading\n\nbody", "BOM removed, LF endings");
        assert_eq!(note.format, "md");

        // JSON is just text: shown verbatim, never interpreted
        let json_note = load_external_note(&env.conn, &report.opened[2].note_id)
            .unwrap()
            .unwrap();
        assert_eq!(json_note.content, "{\"notepad_version\": 1, \"notes\": []}");
    }

    #[test]
    fn reopening_a_file_keeps_one_entry_and_close_forgets_it_without_touching_the_file() {
        let env = setup();
        let file = env.dir.join("plan.md");
        fs::write(&file, "version one").unwrap();

        let first = open_files_as_notes(&env.conn, std::slice::from_ref(&file));
        let again = open_files_as_notes(&env.conn, std::slice::from_ref(&file));
        assert_eq!(first.opened[0].note_id, again.opened[0].note_id);
        assert_eq!(list_external_documents(&env.conn).unwrap().len(), 1);

        assert!(close_external_document(&env.conn, &first.opened[0].note_id).unwrap());
        assert!(list_external_documents(&env.conn).unwrap().is_empty());
        assert!(!close_external_document(&env.conn, "ext-missing").unwrap());
        assert_eq!(fs::read_to_string(&file).unwrap(), "version one");
    }

    #[test]
    fn pinned_documents_survive_a_restart_and_the_rest_are_cleared() {
        let env = setup();
        let keep = env.dir.join("keep.md");
        let temp = env.dir.join("temp.txt");
        fs::write(&keep, "a").unwrap();
        fs::write(&temp, "b").unwrap();
        let report = open_files_as_notes(&env.conn, &[keep, temp]);
        let keep_id = report.opened[0].note_id.clone();

        assert!(set_external_pinned(&env.conn, &keep_id, true).unwrap());
        let listed = list_external_documents(&env.conn).unwrap();
        assert_eq!(listed[0].id, keep_id, "pinned documents are listed first");
        assert!(listed[0].pinned && !listed[1].pinned);

        prune_unpinned_external(&env.conn);
        let after = list_external_documents(&env.conn).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].id, keep_id);
        assert!(!set_external_pinned(&env.conn, "ext-nope", true).unwrap());
    }

    #[test]
    fn saving_writes_back_to_the_file_and_keeps_bom_and_crlf() {
        let env = setup();
        let file = env.dir.join("win.txt");
        fs::write(&file, "\u{feff}one\r\ntwo").unwrap();
        let id = open_files_as_notes(&env.conn, std::slice::from_ref(&file)).opened[0]
            .note_id
            .clone();

        let saved = save_external_note(&env.conn, &id, "one\ntwo\nthree").unwrap();
        assert_eq!(saved.content, "one\ntwo\nthree");
        assert_eq!(
            fs::read(&file).unwrap(),
            "\u{feff}one\r\ntwo\r\nthree".as_bytes()
        );

        let unix = env.dir.join("unix.md");
        fs::write(&unix, "a\nb").unwrap();
        let id = open_files_as_notes(&env.conn, std::slice::from_ref(&unix)).opened[0]
            .note_id
            .clone();
        save_external_note(&env.conn, &id, "a\nb\nc").unwrap();
        assert_eq!(fs::read_to_string(&unix).unwrap(), "a\nb\nc");

        fs::remove_file(&unix).unwrap();
        assert!(save_external_note(&env.conn, &id, "x")
            .unwrap_err()
            .contains("moved or deleted"));
        assert!(save_external_note(&env.conn, "ext-nope", "x").is_err());
    }

    #[test]
    fn bad_files_are_reported_without_stopping_the_good_ones() {
        let env = setup();
        let good = env.dir.join("ok.txt");
        let binary = env.dir.join("image.txt");
        fs::write(&good, "fine").unwrap();
        fs::write(&binary, [0u8, 159, 146, 150, 0, 1, 2]).unwrap();
        let folder = env.dir.join("a-folder");
        fs::create_dir_all(&folder).unwrap();

        let report = open_files_as_notes(
            &env.conn,
            &[binary, env.dir.join("missing.md"), folder, good],
        );
        assert_eq!(report.opened.len(), 1);
        assert_eq!(report.opened[0].title, "ok");
        assert_eq!(report.failed.len(), 3);
        assert!(report.failed.iter().any(|f| f.file_name == "image.txt"));
        assert!(report
            .failed
            .iter()
            .any(|f| f.message.contains("does not exist")));

        let binary_message = &report
            .failed
            .iter()
            .find(|f| f.file_name == "image.txt")
            .unwrap()
            .message;
        assert!(
            !binary_message.starts_with("Validation failed"),
            "message reads naturally: {binary_message}"
        );
        assert!(binary_message.contains("Only text files can be opened"));
    }

    #[cfg(unix)]
    #[test]
    fn a_second_launch_delivers_its_files_to_the_running_instance() {
        use std::sync::mpsc;
        use std::time::Duration;
        let dir = std::env::temp_dir().join(format!("pn_sock_{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();

        // Nobody is listening yet
        assert!(!ipc::forward_to_running_instance(
            &dir,
            &[PathBuf::from("/tmp/x.md")]
        ));

        let (tx, rx) = mpsc::channel();
        ipc::start_listener(&dir, move |paths| {
            let _ = tx.send(paths);
        })
        .unwrap();
        let mode = fs::metadata(socket_path(&dir)).unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            mode.permissions().mode() & 0o777,
            0o600,
            "only the owner may connect"
        );

        assert!(ipc::forward_to_running_instance(
            &dir,
            &[PathBuf::from("/tmp/a b.md"), PathBuf::from("/tmp/c.txt")]
        ));
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(3)).unwrap(),
            vec![PathBuf::from("/tmp/a b.md"), PathBuf::from("/tmp/c.txt")]
        );
        assert!(ipc::forward_to_running_instance(&dir, &[]));
        assert!(
            rx.recv_timeout(Duration::from_secs(3)).unwrap().is_empty(),
            "focus request has no paths"
        );

        // The socket path stays short and distinct per data directory, even for very long ones
        let long_dir = dir.join("a".repeat(120));
        assert!(socket_path(&long_dir).to_string_lossy().len() < 100);
        assert_ne!(socket_path(&dir), socket_path(&long_dir));

        // A stale socket from a crashed instance does not block a new listener
        let (tx2, _rx2) = mpsc::channel();
        assert!(ipc::start_listener(&dir, move |p| {
            let _ = tx2.send(p);
        })
        .is_ok());
        let _ = fs::remove_file(socket_path(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn app_identifier_matches_tauri_config() {
        let config = include_str!("../tauri.conf.json");
        assert!(
            config.contains(&format!("\"identifier\": \"{APP_IDENTIFIER}\"")),
            "APP_IDENTIFIER must equal the identifier in tauri.conf.json"
        );
    }
}
