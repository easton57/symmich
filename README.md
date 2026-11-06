# Symmich

Symmich is an unofficial desktop sync client for Immich.

## Features

- [x] One Way Sync
- [x] Two Way Sync
- [x] Tray Icon
- [x] Conflict Resolution
- [x] Folder to Remote Album (Local folder creates an album in immich)
- [x] Simple UI to sign in, resolve conflicts, and check sync status

## Libraries

[Tray Icon](https://github.com/tauri-apps/tray-icon)  
[Reqwest](https://docs.rs/reqwest/latest/reqwest/)

## Installing

Installers for Windows, macOS and Linux (.deb, .rpm, AppImage) are built by
GitHub Actions whenever a `v*.*.*` tag is pushed, and can be downloaded from
that workflow run's artifacts.

The macOS DMG is not signed or notarized, so Gatekeeper will warn on first
launch. Right-click the app and choose **Open**, or run
`xattr -dr com.apple.quarantine /Applications/Symmich.app`.

## Development

Requires Node.js 20+ and a Rust toolchain, plus the
[Tauri system dependencies](https://v2.tauri.app/start/prerequisites/) for
your platform.

```bash
npm install
npm run tauri dev
```

## License

[MIT](LICENSE)
