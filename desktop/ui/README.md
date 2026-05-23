# AkaDesk Desktop UI Skeleton

This directory is intentionally limited to the M1 baseline:

- `src/app/` owns application-level composition.
- `src/pages/` hosts route-level pages.
- `src/components/` holds shared UI pieces.
- `src/features/` groups runtime status, session, diagnostics, and settings flows.
- `src/stores/` and `src/services/` are the only places that should centralize state and IPC integration.

Tooling manifests can be finalized in M2 when the Tauri shell and IPC surface are wired together.
