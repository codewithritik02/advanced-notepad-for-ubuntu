# Changelog

All notable changes to **Personal Notepad** are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.1.0] - 2026-10-17

### Added
- **Production Ubuntu Packaging**:
  - Pinned Debian deliverable `personal-notepad_0.1.0_amd64.deb` targeted for Ubuntu 24.04 LTS and 26.04 LTS (`amd64`).
  - FreeDesktop desktop launcher (`Personal Notepad.desktop`) with `Utility;TextEditor;` categories and MIME associations for `text/plain` and `text/markdown`.
  - Complete FreeDesktop icon hierarchy (`32x32`, `64x64`, `128x128`, `256x256`, `512x512`).
  - Strict filesystem separation: Binaries and desktop assets install to `/usr/bin/` and `/usr/share/`, while user notes, database, attachments, and backups strictly reside in `$XDG_DATA_HOME/com.personalnotepad.app/`.
- **Local-First Core Storage**:
  - Embedded SQLite database engine (`rusqlite`) with WAL (Write-Ahead Logging) journal mode.
  - Automated schema migration runner (`_migrations` table).
  - High-performance SQLite FTS5 full-text search index with automated trigger synchronization.
- **Privacy & Security Boundaries**:
  - 100% offline runtime operation with zero telemetry, zero cloud accounts, and zero remote network requests.
  - Phase 12 optional zero-knowledge master password encryption using Argon2id key derivation and AES-256-GCM authenticated ciphertext envelopes.
  - Automatic memory zeroization on app lock and ephemeral decrypted attachment cache cleanup.
  - Comprehensive executable blocking policy (`BLOCKED_EXECUTABLE_EXTENSIONS`) and canonical path traversal defense for attachment handling.
- **Native Desktop Integrations**:
  - Native file pickers via `tauri-plugin-dialog` for attachments, backup archive export/import, and notes import.
  - System default viewer dispatch via `tauri-plugin-opener` (`xdg-open`).
  - Safe failure recovery: Interrupted installations, schema upgrade errors, or disk space exhaustion never purge or corrupt user notes.

### Known Issues & Limitations
- Packaged `.deb` deliverable is currently compiled exclusively for `x86_64` (`amd64`). ARM64 (`aarch64`) packaging is planned for a future release.
- Windows distribution is planned under Phase 16.
