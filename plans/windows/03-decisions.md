# 03 — Technical Decisions

Format: **Decision · Options · Choice · Why · Revisit when**

## D-01 — Instance lock mechanism
- **Options:** (a) PID file + `/proc` (current) · (b) PID file + `sysinfo` crate se process check · (c) **OS exclusive file lock** (`File::try_lock`, Rust ≥ 1.89, ya `fs4`)
- **Choice:** (c). Local rustc 1.99.1 hai, to std API available. CI mein `dtolnay/rust-toolchain@stable` use ho raha hai, to bhi theek. `rust-version` Cargo.toml mein pin kar do.
- **Why:** Crash par OS lock chhod deta hai (stale PID nahi), PID reuse bug nahi, Linux + Windows dono par ek hi code. PID file sirf diagnostics ke liye (existing `forward_launch` isko padhta hai).
- **Careful:** Lock file ko DB ke saath **same directory** mein rakho, lekin ek alag file (`.app_instance.lock`); `.app_instance.pid` ko existing Linux forwarding ke liye maintain rakho. Network drive par advisory lock unreliable, docs mein likhna.
- **Revisit:** Agar Rust MSRV kam rakhni pade.

## D-02 — Single-instance / "Open with" forwarding on Windows
- **Options:** (a) `tauri-plugin-single-instance` · (b) Windows named pipe khud (`interprocess` crate) · (c) Local TCP loopback
- **Choice:** (a) sirf `cfg(windows)` ke liye. Linux ka existing Unix socket **touch nahi**.
- **Why:** Official, maintained, kam code, security (DACL etc.) plugin handle karta hai. Callback mein `(app, argv, cwd)` milta hai, jo seedha `parse_launch_paths` mein ja sakta hai.
- **Why not (c):** Loopback TCP port par koi bhi local process connect kar sakta hai, attack surface.
- **Revisit:** Phase 10, Linux + Windows dono ko plugin par unify karna (code kam hoga, lekin Linux regression risk).

## D-03 — Windows target aur architecture
- **Choice:** `x86_64-pc-windows-msvc` only (v0.2.0). ARM64 baad mein.
- **Why:** MSVC target Tauri ka officially supported hai aur WebView2 ke saath best. GNU target nahi.

## D-04 — Installer format
- **Options:** NSIS `.exe` · WiX `.msi` · MSIX
- **Choice:** **NSIS** pehle. MSI optional Phase 10.
- **Why:** NSIS Tauri mein cross-platform build hota hai (Linux se bhi), per-user install support, small. MSI enterprise ke liye.
- **Install mode:** `currentUser` (admin rights nahi chahiye, per-user `%LOCALAPPDATA%\Programs\Personal Notepad`). Why: notes app ko admin ki zaroorat nahi, aur UAC prompt kam.
- **Languages:** English (default); baaki baad mein.

## D-05 — WebView2 install strategy
- **Options:** `downloadBootstrapper` (default) · `embedBootstrapper` · `offlineInstaller` · `skip`
- **Choice:** `embedBootstrapper` (chhota overhead, internet chahiye sirf tab jab WebView2 missing ho). Documented limitation: Windows 10 par WebView2 absent + offline machine → ek baar internet ya offline installer.
- **Why:** README "fully offline" claim ko sach rakhne ke liye; lekin 120 MB offline installer har user par thopna bhaari. **Optional** ek alag "offline" artifact release karo (`-offline-setup.exe`).
- **Revisit:** Agar user feedback mein install failures aayein.

## D-06 — Code signing
- **Options:** OV/EV certificate · Azure Trusted Signing · Unsigned beta
- **Choice:** v0.2.0-beta **unsigned** + clear docs; v0.2.x par signing (Azure Trusted Signing ya OV cert).
- **Why:** Cost/time (certificate validation days le sakti hai). Open-source project; SHA-256 checksums se integrity.
- **Revisit:** Pehle 50 downloads ke SmartScreen feedback ke baad.

## D-07 — Windows par frontend / UI changes
- **Choice:** Koi bada UI change nahi. Sirf verify: fonts, scrollbars, title bar, DPI scaling (125%/150%), dark mode.
- **Native title bar** rakho (custom decorations nahi).

## D-08 — Data directory: Roaming vs Local
- **Options:** (a) `%APPDATA%` Roaming (Tauri default, README mein likha) · (b) `%LOCALAPPDATA%` Local
- **Recommendation:** **(b) Local**, kyunki SQLite WAL + attachments Roaming par sync/network share par corrupt ho sakte hain.
- **Impact:** `resolve_app_data_root` mein Windows branch, `app.path().app_local_data_dir()` use karo. README table update (`%LOCALAPPDATA%\com.personalnotepad.app\`).
- **Careful:** Linux ka path **na badle**. Abhi koi Windows user nahi, to migration ki zaroorat nahi. **Release se pehle** decide karo, baad mein badalna migration maangta hai.
- **Status:** **FINAL: Local (`%LOCALAPPDATA%`)** (2026-11-17). Linux unchanged. Implement Phase 1/4.

## D-09 — Filesystem retry helper
- **Choice:** `src-tauri/src/storage/fs_util.rs` (ya `platform/` module) mein `rename_with_retry`, `remove_file_with_retry`, `remove_dir_all_with_retry`, `canonicalize_clean`. Linux par thin wrapper (retry 1 baar, behaviour same).
- **Why:** Ek jagah testable logic, 12+ call sites ek hi tareeke se fix.

## D-10 — Windows-only code kahan rakhein
- **Choice:** Rust: `src-tauri/src/platform/{mod.rs, unix.rs, windows.rs}` ya `#[cfg]` modules; shared code `cfg` se mukt rahe jahan mumkin ho. Repo-level: `platform/windows/` mein sirf installer assets (NSIS hooks, manifest, docs).
- **Why:** Aapki existing docs ka rule: "Linux-specific packaging behavior must remain isolated from shared application code."

## D-11 — Test policy
- **Choice:** Har naya platform fix ke saath unit test (pure-logic parts Linux par bhi chalein, jaise `file:///C:/x` parsing function ko cfg ke bina testable banao), plus Windows CI par full suite.
- **Windows-only behaviour** (rename retry on sharing violation) ke liye Windows-only integration test.

## D-12 — Versioning aur artifact naming
- **Choice:** Windows release `0.2.0` (Cargo.toml, package.json, tauri.conf.json teeno sync, `generate-release-metadata.mjs` already check karta hai). Artifact: `Personal-Notepad_0.2.0_x64-setup.exe` + `SHA256SUMS.txt`. Linux: `personal-notepad_0.2.0_amd64.deb`.
- **Why:** Release metadata script ko extend karna padega (Linux-only filename assumptions? Phase 6 mein check).
