# Symmich

Symmich is an unofficial desktop sync client for Immich.

## Features

- [x] One Way Sync
- [x] Two Way Sync
- [x] Tray Icon
- [x] Conflict Resolution
- [x] Folder to Remote Album (Local folder creates an album in immich)
- [x] Simple UI to sign in, resolve conflicts, and check sync status
- [x] Remembers your password so it signs in by itself
- [x] Optional start at login, quietly in the tray

## Libraries

[Tray Icon](https://github.com/tauri-apps/tray-icon)  
[Reqwest](https://docs.rs/reqwest/latest/reqwest/)

## Installing

Installers for Windows, macOS and Linux (.deb, .rpm, AppImage) are built by
GitHub Actions whenever a `v*.*.*` tag is pushed, and published on the
[Releases](https://github.com/easton57/symmich/releases) page.

## Security

- The server URL must use `https://`, so your password and photos are
  encrypted on the way. Plain `http://` is only accepted for servers on your
  home network (private addresses, `localhost`, `.local`/`.lan` names).
- If "Remember my password" is on, your password is kept in the system's
  password store (Windows Credential Manager, macOS Keychain, or the Secret
  Service on Linux), never in a plain file. Changing the server or account
  in Settings forgets it and asks you to sign in again, and signing out removes
  it even when the server can't be reached.
- Photos pulled from the server are only written inside your sync folder, under
  plain image file names.

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
