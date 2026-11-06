# Suggested Libraries & Implementation Steps

## Rust Backend Libraries

### Core

| Purpose | Crate |
| --- | --- |
| HTTP client | `reqwest` |
| JSON | `serde` + `serde_json` |
| Async runtime | `tokio` |
| Desktop framework | `tauri` (v2) |
| Error handling | `anyhow` + `thiserror` |

### File Sync

| Purpose | Crate |
| --- | --- |
| File watcher | `notify` |
| Content hashing | `sha2` or `blake3` |
| MIME detection | `infer` |
| EXIF data | `kamadak-exif` |
| Image thumbnails | `image` |
| Video metadata | `mp4parse` or `ffmpeg-next` |

### Utility

| Purpose | Crate |
| --- | --- |
| Config storage | `directories` |
| Logging | `tracing` |
| Date/time | `chrono` |
| UUID | `uuid` |
| Tauri plugins | `tauri-plugin-dialog`, `tauri-plugin-fs`, `tauri-plugin-shell` |

## Implementation Steps

1. [x] **Scaffold** — `npm create tauri-app . -- --template next-ts` (after moving Rust into `src-tauri/`)
2. [x] **Immich API client** — Wrap `/api/auth`, `/api/albums`, `/api/assets` endpoints
3. [x] **File scanner** — Walk directories, hash files, extract metadata
4. [x] **Sync engine** — Compare local vs server state, build upload queue
5. [x] **File watcher** — `notify` crate for real-time monitoring
6. [x] **Tauri commands** — Expose sync status, progress, and controls to the React frontend
7. [x] **Frontend UI** — Show sync progress, album selection, settings
