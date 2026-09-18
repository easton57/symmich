# Auth Implementation Plan

**Server:** https://immich.example.com (Immich, not Nextcloud)
**Frontend:** TS (React) via Tauri; all HTTP requests go straight from Rust.
**Auth flow:** Email + password against Immich `/api/auth/login`. No OAuth.

## Key fact that shapes the design

Immich's `POST /api/auth/login` response does **not** include an `expires_in` field.
The `accessToken` is a JWT that carries its own expiry inside the token payload (the
`exp` claim). So "auto re-login on expiry" means decoding that `exp`, or re-logging in
when a call returns HTTP 401 — not reading an expiry field.

Because we auto re-login, Rust must keep the credentials stored (encrypted) at rest so
it can silently authenticate again once the token expires.

## Login endpoint

`POST {server_url}/api/auth/login`

**Request body:**
```json
{ "email": "<email>", "password": "<password>" }
```

**Response (200):**
```json
{
  "accessToken": "<JWT>",
  "userId": "...",
  "userEmail": "...",
  "name": "...",
  "profileImagePath": "...",
  "isAdmin": true,
  "shouldChangePassword": false
}
```

The `accessToken` must be sent as `Authorization: Bearer <token>` on every later call.

---

## Files & changes

### 1. `src-tauri/Cargo.toml`
```toml
reqwest = { version = "0.12", features = ["json"] }
# credential storage: keyring (OS keychain) OR encrypted file — see section below
keyring = "3"
```

### 2. Session state (`src-tauri/src/lib.rs`)
```rust
struct Session {
    server_url: String,
    email: String,
    token: String,
    token_expiry: Option<i64>,   // epoch seconds from JWT `exp` claim
    password: String,            // kept to re-login; stored encrypted on disk
}
```
Held in Tauri managed state: `app.manage(Mutex<Session>)` inside `run()`.

### 3. Commands (`src-tauri/src/lib.rs`)
All registered in `invoke_handler`.

- **`login(email, password, server_url) -> UserInfo`**
  - `POST {server}/api/auth/login` with body `{ email, password }`.
  - Read `accessToken`, `userId`, `name`, `profileImagePath` from the response.
  - Decode the JWT middle segment (base64) to extract the `exp` claim.
  - Persist token + email + password (encrypted) for remember-me; store in state.

- **`logout()`**
  - Clear in-memory state and delete persisted credentials.

- **`get_session() -> Option<SessionInfo>`**
  - Frontend calls on startup to detect an existing logged-in session.

- **Private helper `authenticated_client() -> Result<reqwest::Client>`**
  - Used by every other command (Pull Folders, Push Image, etc.).
  - If token exists and unexpired → reuse it.
  - If expired → silently re-login with stored credentials, update state, continue.
  - Returns a `reqwest::Client` with `.bearer_auth(&token)`.

### 4. `src/App.tsx`
- Remove the `greet` demo.
- On mount, call `get_session()`:
  - Logged in → show main app.
  - Not logged in → show email/password/server form.
- Form calls `invoke("login", { email, password, serverUrl })`.

### 5. Persistence layout (ties into Save/Load + Encrypt preferences)
- Store prefs + credentials under `app_config_dir()`:
  - macOS: `~/Library/Application Support/com.symmich.app/`
- Prefs: plain serde JSON file.
- Credentials: encrypted at rest (see decision below).

---

## Open decision: credential storage

**Option A — `keyring` crate (recommended)**
Use the OS keychain (Keychain on macOS, Secret Service on Linux).
Store the plaintext password there, never write it to disk. Safer, less code,
and your "Encrypt password" feature becomes mostly unnecessary (OS handles it).

**Option B — encrypted config file:**
Write the password to a JSON file, encrypted with a key you manage
(e.g. derived from a master password). Single file, but you own the crypto —
more code and more risk.

> **Choice: A — implemented.** The `keyring` crate stores the password under
> service `com.eseidel.symmich`, keyed by email. `Session::load` restores it on
> startup; `login`/`logout` write/delete the entry. `get_session()` tells the
> frontend whether a restorable session exists so it can skip the login form.

---

## Suggested sequence

1. Add `reqwest` dep and a minimal `login` command (returns token + user info, no
   persistence yet) to get the round-trip working against the live server.
2. Add session state + `get_session()` + `logout()`.
3. Add `authenticated_client()` + re-login-on-expiry.
4. Wire remember-me (keyring or encrypted file).
5. Rework `App.tsx` to replace the greeting demo with login.