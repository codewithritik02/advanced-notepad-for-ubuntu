# Tests

## Current suites

- `frontend.test.mjs` contains the existing Node-based frontend logic, simulation, and source-contract tests.
- `frontend.test.mjs` also executes the pure guarded-close coordinator to verify pending-save ordering and failure behavior.
- `../src-tauri/tests/` contains Rust integration and backend workflow tests.
- `../src-tauri/tests/startup_shutdown_recovery_test.rs` drives the production lifecycle entry point through fresh, existing, migrated, invalid, interrupted, and encrypted storage states.
- `../src-tauri/tests/sqlite_schema_migrations_integrity_test.rs` verifies the real schema, all supported upgrade paths, constraints, rollback, contention, reopen, and corruption behavior.
- Real rendered React component tests and desktop end-to-end automation have not been added yet; do not report the existing simulations as those test types.

## Isolated Rust fixtures

Rust integration tests should import `src-tauri/tests/common/mod.rs` and use:

- `TestAppDir` for a unique application-data tree beneath the system temporary directory.
- `TestAppDir::open_migrated_database()` for SQLite initialized by the real migration runner.
- `TestDatabase` for note, notebook, tag, attachment, and reopen helpers.
- `lock_process_global_test_state()` only when a test changes production process-global state such as the security audit sink or backup/restore concurrency flags.

Each root uses the reserved `personal-notepad-test-` prefix and an exact safety marker. Cleanup refuses paths without both safeguards, removes only the owned root, and surfaces cleanup failures. Do not construct test storage from the normal application-data resolver.

Debug desktop tests may set `PERSONAL_NOTEPAD_TEST_DATA_DIR` to a root created by the canonical fixture. The Tauri application independently validates that it is a marked direct child of the system temporary directory. Release builds ignore this override.

Run the infrastructure checks with:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test test_infrastructure_test
cargo test --manifest-path src-tauri/Cargo.toml --test startup_shutdown_recovery_test
cargo test --manifest-path src-tauri/Cargo.toml --test sqlite_schema_migrations_integrity_test
```
