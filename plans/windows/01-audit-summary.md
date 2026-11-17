# 01 — Audit Summary (2026-11-17, v0.1.6)

## 1. Project kya hai

| Item | Detail |
|---|---|
| App | Personal Notepad — offline, local-first notes |
| Stack | Tauri 2, Rust (edition 2021, ~9.7k lines in `src-tauri/src`), React 19 + Vite 8 + TypeScript 6 |
| Storage | SQLite (`rusqlite` `bundled`), attachments on disk, optional encryption (Argon2id + AES-256-GCM) |
| Plugins | `tauri-plugin-opener`, `tauri-plugin-dialog` |
| Local toolchain | rustc 1.99.1, Linux (Ubuntu), x86_64-unknown-linux-gnu |
| Release profile | `lto=true`, `codegen-units=1`, `opt-level=3`, `panic=abort`, `strip=true` |

## 2. Linux ke liye jo ban chuka hai

- `.deb` packaging (`package:linux`), `desktop-template.desktop`, dependency list (libsoup, glib, cairo, ...).
- CI: `ubuntu-latest` par frontend quality + rust quality (fmt, clippy, cargo-audit, tests).
- Docs: 60+ development docs (`docs/`), Phase 1–15 specs. `docs/` bhi gitignored hai.
- Features: notes, notebooks (7 levels), tags, trash, backups, encryption, import/export, internal links, "Open with".
- Test suites: 25+ Rust integration test files, frontend tests, browser editor regression test.
- Release metadata script: `scripts/generate-release-metadata.mjs` (checksums, provenance).

## 3. Windows ke liye abhi kya hai

| Cheez | Status |
|---|---|
| `platform/windows/README.md` | Sirf placeholder, koi code nahi |
| `tauri.conf.json` bundle targets | `["deb"]` only |
| `icons/icon.ico` | **Maujood hai** (ready) |
| CSP mein `http://ipc.localhost`, `http://asset.localhost` | **Pehle se hai** (Windows WebView2 ke liye zaroori) |
| CI Windows job | Nahi hai |
| Windows par koi build/test run | **Kabhi nahi hua** (NOT VERIFIED) |
| Rust Windows target (is machine par) | Installed nahi |
| `.gitattributes` | Nahi hai (line-ending risk, bug W-14) |
| README claim | "Windows: In progress", data path `%APPDATA%\com.personalnotepad.app\` |

## 4. Kya portable hai (achhi khabar)

- Poora React/TypeScript frontend. Shortcuts `ctrlKey || metaKey` use karte hain, Windows par sahi.
- SQLite bundled, to system library nahi chahiye (sirf MSVC C compiler build time par).
- Crypto crates (`aes-gcm`, `argon2`, `sha2`, `rand`, `zeroize`) pure Rust, portable.
- Path traversal guards pehle se `/` aur `\` dono reject karte hain (`attachment_storage.rs:107,505`, `paths.rs:161,179`).
- Backslash normalization import/export mein hai (`attachment_capture.rs:116`, `paths.rs:210`).
- Atomic file write pattern (temp + rename) use ho raha hai; Windows par `std::fs::rename` existing file replace karta hai (agar target open na ho).
- Executable-extension blocklist mein Windows ke saare dangerous types (`exe, bat, cmd, msi, scr, ps1, ...`) pehle se hain.
- Data dir Tauri ke `app_data_dir()` se aata hai, jo Windows par automatically sahi path deta hai.

## 5. Kya Linux-only hai (problem areas)

Detail `02-bug-register.md` mein. Short list:

1. **`/proc/<pid>` se instance lock** → Windows par lock kaam hi nahi karta (data safety).
2. **Unix domain socket IPC** (`open_files::ipc`, `forward_launch`) → `#[cfg(unix)]`; Windows par "second launch → running window" aur "Open with" ka forwarding nahi.
3. **`default_data_dir()`** sirf `XDG_DATA_HOME`/`HOME` padhta hai → Windows par `None`.
4. **`file://` URI parsing** Unix-style, `file:///C:/...` galat.
5. **Unix permissions (`0o600`, `PermissionsExt`)** sirf `cfg(unix)` mein, theek, lekin Windows ke liye koi equivalent decision nahi.
6. **Tests** mein `/tmp`, `/home/...`, `std::os::unix`, `/bin/sh` hardcoded.
7. **File locking semantics**: Windows par open file ko rename/delete nahi kar sakte (DB corrupt isolation, attachment cleanup).
8. **Packaging/CI/scripts**: sab Linux-centric.

## 6. Confidence levels (honest note)

- **Verified by reading code:** W-01, W-02, W-03, W-04, W-07, W-09 (file:line diye gaye hain).
- **Likely, par Windows par compile/run karke confirm karna hai:** W-05, W-06, W-08, W-10, W-11, W-12, W-13.
- Windows par kuch bhi chalaya nahi gaya, isliye ye audit **static analysis** hai. Phase 1 ke baad ye list update hogi aur naye bugs add honge.
