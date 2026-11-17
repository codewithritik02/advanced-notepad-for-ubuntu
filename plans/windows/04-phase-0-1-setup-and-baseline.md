# Phase 0 & 1 — Environment aur Baseline

---

## PHASE 0 — Environment + Tooling (0.5 din)

**Goal:** Ek aisa Windows environment jahan build aur test chal sakein.

### 0.1 Windows machine ka option choose karo
| Option | Pros | Cons |
|---|---|---|
| A. GitHub Actions `windows-latest` | Free (public repo), reproducible | Debugging slow (push → wait) |
| B. Windows 11 VM (VirtualBox/KVM) is Ubuntu par | Interactive debugging, GUI QA | Setup time, 8 GB RAM + 60 GB disk |
| C. Real Windows PC (dost/laptop) | Best QA | Access chahiye |
| D. Cross-compile Linux → Windows (`cargo-xwin`) | Local compile check | **Tests/QA nahi chalte**, sirf compile |

**Recommendation:** A + B dono. CI se pehla compile signal, VM se manual QA (Phase 8).
Is machine ke specs (8 cores, 14 GB RAM, 189 GB free) VM ke liye kaafi hain.

### 0.2 Windows dev setup checklist (VM par)
- [ ] Windows 11 x64 (clean install, updates)
- [ ] **Visual Studio Build Tools 2022** → "Desktop development with C++" workload (MSVC + Windows SDK)
- [ ] Rust via `rustup` (stable, target `x86_64-pc-windows-msvc`)
- [ ] Node.js 22 LTS (CI ke same)
- [ ] WebView2 Runtime (Win 11 mein already; Win 10 par install)
- [ ] Git for Windows, `git config --global core.autocrlf false`
- [ ] NSIS (Tauri khud download kar leta hai build time par, par offline ho to manual)
- [ ] Google Chrome (browser regression test ke liye, optional)

### 0.3 Ubuntu par local compile check (optional, jaldi signal)
```bash
rustup target add x86_64-pc-windows-msvc
cargo install --locked cargo-xwin
cargo xwin check --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
```
Ye sirf `cfg(windows)` compile errors dikhayega (jaise W-09). Runtime ya tests nahi.

### 0.4 Baseline snapshot (Linux)
- [ ] `npm ci && npm run typecheck && npm run lint && npm test`
- [ ] `npm run lint:rust && npm run test:rust` → sab green (reference ke liye pass/fail counts note karo)
- [ ] `git tag pre-windows-baseline` (local tag)

### Exit criteria Phase 0
- Windows environment se `rustc --version`, `node --version`, `cl.exe` kaam karte hain.
- Linux baseline green aur numbers recorded.

---

## PHASE 1 — Pehla Windows Compile + Baseline (0.5 din)

**Goal:** Windows par `cargo check`, `cargo build`, `npm run build` pass karwana aur **asli bugs ki list** pakadna.

### Sub-phase 1.1 — Repo hygiene (Linux par karo, commit karo)
- [ ] `.gitattributes` add (**tracked**):
  ```
  * text=auto eol=lf
  *.png binary
  *.ico binary
  *.icns binary
  *.sqlite binary
  *.pnbackup binary
  ```
- [ ] `rust-version = "1.89"` (ya jo MSRV decide ho) `Cargo.toml` mein
- [ ] `git add --renormalize .` aur diff dekho (sirf line endings badlein, content nahi)
- [ ] Bug: W-14

### Sub-phase 1.2 — Compile check
- [ ] Windows par: `cargo check --manifest-path src-tauri/Cargo.toml --all-targets --locked`
- [ ] Har compile error/warning ko `02-bug-register.md` mein **W-21+** ke taur par log karo
- [ ] Expected suspects: `lib.rs:70-78` socket functions (W-09), unused imports/functions under `cfg(not(unix))`, `open_files.rs` unused `default_data_dir`

### Sub-phase 1.3 — Minimal compile fixes
- [ ] Sirf compile fix, **behaviour change nahi** (behaviour Phase 2–4 mein)
- [ ] Unused code ko `#[cfg(unix)]` ke peeche rakho, ya Windows stub no-ops
- [ ] Linux par dobara `cargo clippy --all-targets --locked` aur tests green

### Sub-phase 1.4 — Frontend build on Windows
- [ ] `npm ci && npm run build` (Vite + tsc)
- [ ] `npm run lint`, `npm run typecheck`, `npm test` Windows par; failures W-xx mein log (W-14, W-15)

### Sub-phase 1.5 — Debug run (smoke)
- [ ] `npm run tauri dev` ya `npm run build:desktop` → app window khulti hai?
- [ ] Note create/edit/save, restart, notes persist?
- [ ] DPI 100/125/150% par UI theek?
- [ ] Data path confirm: `%APPDATA%\com.personalnotepad.app\` (ya Local, D-08)

### Sub-phase 1.6 — Decisions finalize
- [ ] D-08 (Roaming vs Local) final
- [ ] D-05 (WebView2 mode) final
- [ ] Bug register update (confidence "L" wale ko V/false-alarm mark karo)

### Exit criteria Phase 1
- `cargo check --all-targets` Windows par pass.
- `npm run build` Windows par pass.
- App Windows par launch hoti hai aur basic note CRUD kaam karta hai (with known issues listed).
- Bug register updated, Linux tests unaffected.
- Commits: `chore: add gitattributes`, `fix: compile on windows target` (alag).
