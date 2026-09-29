# Shared Logic & Types

This directory contains cross-platform business logic, models, validation rules, and utilities that are strictly operating system-independent.

## Architectural Boundary
- Code in this directory **must not** contain OS-specific APIs (`linux`, `windows`, etc.).
- Both the Linux build and future Windows distribution consume identical shared logic from here.
- Any platform divergences must be handled behind platform abstractions located under `platform/` or native Tauri plugins.
