# Services Layer

Defines interfaces and client adaptors between the React UI layer and external/native environments (such as Tauri IPC invocation, system dialogs, file drop events).
Components must communicate through services rather than invoking native runtime APIs directly.
