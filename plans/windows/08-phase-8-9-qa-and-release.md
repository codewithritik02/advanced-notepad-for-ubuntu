# Phase 8 & 9 — QA aur Release

---

## PHASE 8 — Manual QA aur Hardening (1 din)

**Goal:** Asli Windows par poora user journey verify; `docs/development/phase-14-*` aur `phase-15-*` jaisi QA discipline Windows ke liye dohrani.
Test matrix detail: `09-test-matrix.md`.

### 8.1 Environments
- [ ] Windows 11 x64 (clean VM, snapshot "fresh")
- [ ] Windows 10 22H2 x64 (WebView2 maybe missing, W-12 verify)
- [ ] Standard user (non-admin) account
- [ ] Display scaling 100%, 125%, 150%, multi-monitor (agar possible)
- [ ] Windows Defender ON (default)

### 8.2 Installer lifecycle (S1 area)
- [ ] Fresh install (per-user, no admin prompt)
- [ ] Start menu entry, desktop shortcut option, app icon
- [ ] Upgrade (0.2.0-beta1 → beta2): notes untouched
- [ ] Reinstall same version
- [ ] Uninstall: data folder safe (W-13), Start menu clean
- [ ] Silent install/uninstall (`/S`) — data safe
- [ ] Install path with space aur Unicode username (`C:\Users\रितिक\...`)
- [ ] Install without internet (WebView2 present / absent)

### 8.3 Core functionality (Windows par re-run)
Notes CRUD, autosave, undo/redo, notebooks (7 levels), tags, favorites, pin, hidden, trash, search, links + Linked-from, attachments (add/open/remove/large file), import (`txt/md/json`), export (`txt/md/json/zip`), backups (manual/scheduled/verify/restore), password lock/unlock, zoom, themes, back/forward.

### 8.4 Windows-specific scenarios
- [ ] Instance: dusri baar launch, kill via Task Manager → relaunch (W-01, W-11)
- [ ] Open with: single/multi/Unicode/UNC/long path (W-02/04/05)
- [ ] Logoff/shutdown ke dauran app open (WAL recovery next boot)
- [ ] Defender/indexer ke saath bade attachments aur backup (W-06)
- [ ] OneDrive-backed Documents folder se import/export
- [ ] Read-only file / locked file open
- [ ] Reserved names export/import roundtrip (W-08)
- [ ] Cross-OS: Linux backup/zip → Windows restore aur ulta

### 8.5 Performance aur reliability
- [ ] Startup time (cold/warm), 10k notes dataset (existing `large_datasets_and_resource_limits_test` equivalent manual)
- [ ] Memory 1 ghante idle + use
- [ ] Long session (8h) resource leak check

### 8.6 Accessibility / UX
- [ ] Keyboard shortcuts (`Ctrl+...`), IME input (Hindi/Japanese), High Contrast mode, Narrator basic
- [ ] Windows dark mode vs app theme

### 8.7 Security checks (Windows)
- [ ] Data folder ACL sirf current user
- [ ] Encryption: password, wrong password, lock, auto-lock; plaintext notes/attachments disk par temp mein na rahen (`decrypted` dir cleanup)
- [ ] `argv` injection: crafted file args (`--help`, very long path, 1000 args)
- [ ] CSP/IPC: `http://ipc.localhost` ke alawa koi network connection nahi (offline verify: firewall block, Wireshark/netstat)

### 8.8 Bug triage
- [ ] Har bug `02-bug-register.md` mein (W-21+), severity ke saath
- [ ] **S1/S2 sab fix** release se pehle; S3 documented known-issues; S4 backlog

### Exit criteria
- 09-test-matrix ke saare "must" rows pass.
- Koi open S1/S2 bug nahi.
- QA report: `plans/windows/QA-REPORT.md` (naya, local).

---

## PHASE 9 — Signing, Docs, Release (0.5 din + cert ka wait)

### 9.1 Code signing (W-18, D-06)
- [ ] Beta: unsigned release, README mein SmartScreen steps
- [ ] Stable: Azure Trusted Signing / OV cert → CI secret → `signCommand` Tauri `bundle.windows`
- [ ] Signed `.exe` ka timestamp (RFC 3161) zaroori taaki cert expiry ke baad bhi valid

### 9.2 Release checklist
- [ ] Version bump `0.2.0` (package.json, Cargo.toml, tauri.conf.json, Cargo.lock/package-lock sync)
- [ ] `npm run release:metadata` → Windows artifact + checksums
- [ ] CHANGELOG entry, README platform table: Windows → "Beta (0.2.0)"
- [ ] Release notes: known issues (SmartScreen, WebView2, Windows 10 note)
- [ ] Tag `v0.2.0` → release workflow → draft → artifacts verify → **manually publish**
- [ ] Downloaded artifact ka SHA-256 verify
- [ ] Post-release: fresh VM par release artifact se install test

### 9.3 Docs (public: README/CONTRIBUTING; local: docs/)
- [ ] Windows install/uninstall/upgrade/reinstall commands
- [ ] "Where data is stored" table final
- [ ] Windows dev setup in CONTRIBUTING
- [ ] `docs/development/phase-16-*.md` (local) — Windows verification report, Phase 15 ki tarah

### 9.4 Post-release
- [ ] Issue template mein "OS: Windows/Linux" field
- [ ] Crash/log location docs: `%APPDATA%\...\logs\`
- [ ] Beta feedback 1–2 hafte → `0.2.1` fixes

---

## PHASE 10 — Optional / Future
- Windows ARM64 target (`aarch64-pc-windows-msvc`)
- MSI (WiX) for enterprise
- Microsoft Store (MSIX)
- Linux + Windows single-instance ko plugin par unify (D-02 revisit)
- Auto-update (Tauri updater, signature keys)
- Portable (zip) build
