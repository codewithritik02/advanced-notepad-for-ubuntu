# Phase 6 & 7 — Installer aur CI/CD

---

## PHASE 6 — Installer / Packaging (0.5–1 din)
**Bugs:** W-10, W-12, W-13, W-16 · **Decisions:** D-03, D-04, D-05, D-12

**Goal:** Ek signed-ready NSIS installer jo per-user install, upgrade, uninstall data-safely karta hai.

### Sub-phase 6.1 — `tauri.conf.json` (Windows config)
Option: alag file `src-tauri/tauri.windows.conf.json` (Tauri platform-specific merge) taaki Linux config touch na ho.
- [ ] `bundle.targets`: Linux `["deb"]`, Windows `["nsis"]`
- [ ] `bundle.windows.nsis`:
  - `installMode: "currentUser"`
  - `languages: ["English"]`
  - `displayLanguageSelector: false`
  - `installerIcon: "icons/icon.ico"`
  - (optional) `installerHooks` NSIS hook file `platform/windows/installer-hooks.nsh`
- [ ] `bundle.windows.webviewInstallMode`: `{ "type": "embedBootstrapper" }` (D-05)
- [ ] `bundle.windows.allowDowngrades`: `false`
- [ ] `bundle.fileAssociations` (W-16): `txt` (text/plain), `md` (text/markdown), role `Editor`, naam "Personal Notepad document"
- [ ] `productName`, `publisher`, `copyright` already set, check
- [ ] `identifier` same rakho (`com.personalnotepad.app`) taaki data path stable

### Sub-phase 6.2 — Windows manifest aur exe metadata
- [ ] Executable version info (Tauri `build.rs`/`tauri-build` windows resource: FileDescription, ProductName)
- [ ] `longPathAware` manifest (W-08), DPI awareness (`PerMonitorV2`) check — Tauri default sahi hota hai, verify
- [ ] Icon: `icons/icon.ico` mein sizes 16/24/32/48/64/128/256 hain? (`ico` inspect karo)

### Sub-phase 6.3 — Uninstall data safety (W-13)
- [ ] NSIS template check: "Delete the application data" checkbox default aur silent `/S` behaviour
- [ ] Agar template data delete kar sakta hai → custom hook se disable ya checkbox hata do. **Policy: app data kabhi delete nahi**; uninstall ke baad data path user ko message mein dikhao.
- [ ] Test: install → notes banao → uninstall → data folder maujood → reinstall → notes wapas

### Sub-phase 6.4 — Scripts aur release metadata
- [ ] `package.json`:
  ```json
  "package:windows": "tauri build --bundles nsis",
  "build:desktop": ... (Windows par bhi chale, `--debug --no-bundle` OK)
  ```
- [ ] `scripts/generate-release-metadata.mjs`: Windows artifact support (`*.exe`), SHA-256, naming (D-12), Linux-only assumptions (`.deb` regex) nikalna
- [ ] Artifact: `Personal-Notepad_<version>_x64-setup.exe`, `SHA256SUMS.txt`

### Sub-phase 6.5 — Docs
- [ ] `platform/windows/README.md` rewrite (actual config + build steps)
- [ ] `README.md`: Windows install steps, SmartScreen note, WebView2 note, data location, uninstall behaviour
- [ ] `CHANGELOG.md`: Windows beta entry
- [ ] `CONTRIBUTING.md`: Windows dev setup section

### Exit criteria
- `npm run package:windows` Windows par `.exe` banata hai.
- Fresh VM par install → app chalu → notes kaam.
- Uninstall data nahi hatata.

---

## PHASE 7 — CI/CD (0.5 din)
**Bugs:** W-15, W-19

**Goal:** Har PR par Windows verification, tag par installer artifact.

### Sub-phase 7.1 — CI matrix
`.github/workflows/ci.yml`:
- [ ] `rust-quality` ko matrix: `os: [ubuntu-latest, windows-latest]`
  - Ubuntu: current steps (apt deps, cargo-audit, fmt, clippy, tests)
  - Windows: Rust toolchain + `cargo check/clippy/test --locked`, apt step skip (`if: runner.os == 'Linux'`)
  - `cargo fmt`, `cargo audit` sirf Ubuntu par (duplicate kaam avoid)
- [ ] `frontend-quality` Windows par bhi (typecheck, lint, test, build); browser regression test Windows par `continue-on-error: true` shuru mein
- [ ] Cache: `Swatinem/rust-cache@v2` (cargo registry + target), OS-specific key
- [ ] Defender slowness ke liye test timeout (`timeout-minutes: 30`)

### Sub-phase 7.2 — Release workflow (naya `.github/workflows/release.yml`)
- [ ] Trigger: tag `v*`
- [ ] Jobs: `build-linux-deb` (ubuntu), `build-windows-nsis` (windows-latest)
- [ ] Steps: checkout, setup node/rust, `npm ci`, `npm run package:<os>`, `generate-release-metadata`, artifacts upload
- [ ] Draft GitHub Release create (manual publish, kabhi auto-publish nahi)
- [ ] Windows signing step placeholder (secrets se, Phase 9)
- [ ] Permissions minimal (`contents: write` sirf release job mein)

### Sub-phase 7.3 — Branch protection (manual, repo settings)
- [ ] Windows job ko required check banao jab stable ho

### Sub-phase 7.4 — Time/cost
- [ ] Pehle run ke timings note karo; target: Windows PR job < 20 min
- [ ] Full LTO release build sirf release workflow mein

### Exit criteria
- PR par dono OS ke jobs green.
- Tag push par draft release mein `.deb` + `.exe` + `SHA256SUMS.txt`.
