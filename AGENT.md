# Project: Symmich — Immich Desktop Sync Client

## Stack
- **Backend:** Rust (Tauri)
- **Frontend:** Next.js (TypeScript)
- **Desktop:** Tauri v2

## Project Structure (Target)

```
symmich/
├── package.json            # Next.js frontend
├── src/                    # React pages/components
├── public/
├── src-tauri/              # Rust backend
│   ├── Cargo.toml
│   ├── src/
│   └── tauri.conf.json
└── AGENT.md
```

## Key Conventions

- Rust backend lives in `src-tauri/`, not at root.
- Frontend at root via `npm create tauri-app`.
- All Tauri commands exposed to the frontend go through `#[tauri::command]` in Rust.
- Use `tracing` for logging, not `println!`.
- Prefer `anyhow::Result` for fallible functions, `thiserror::Error` enums for public error types.
- Keep sync engine decoupled from Tauri — pure Rust library code under `src-tauri/src/sync/`.

## Before Scaffolding

Move existing `Cargo.toml` and `src/` into `src-tauri/` before running `npm create tauri-app` to avoid conflicts. The scaffolding tool expects an empty or absent Rust project at root.

## Sync Architecture

1. **ImmichApi** — HTTP client wrapping the Immich REST API.
2. **Scanner** — walks local directories, hashes files, extracts EXIF/metadata.
3. **SyncEngine** — compares scanner results vs server state, produces upload queue.
4. **Watcher** — uses `notify` crate to watch directories in real-time.
5. **Tauri commands** — `start_sync`, `pause_sync`, `get_status`, etc.
