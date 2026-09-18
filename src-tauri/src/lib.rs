use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};

const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "bmp", "heic", "heif", "tiff", "svg",
];

const KEYRING_SERVICE: &str = "com.eseidel.symmich";

fn save_password_to_keyring(email: &str, password: &str) {
    match keyring::Entry::new(KEYRING_SERVICE, email) {
        Ok(entry) => {
            if let Err(e) = entry.set_password(password) {
                tracing::warn!("failed to save credentials to keyring: {}", e);
            }
        }
        Err(e) => tracing::warn!("failed to access keyring: {}", e),
    }
}

fn load_password_from_keyring(email: &str) -> Option<String> {
    if email.is_empty() {
        return None;
    }
    keyring::Entry::new(KEYRING_SERVICE, email)
        .ok()?
        .get_password()
        .ok()
}

fn delete_password_from_keyring(email: &str) {
    if email.is_empty() {
        return;
    }
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, email) {
        let _ = entry.delete_credential();
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct Settings {
    server_url: String,
    email: String,
    album_name: String,
    local_folder: String,
    album_id: Option<String>,
    scan_frequency_minutes: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            email: String::new(),
            album_name: String::new(),
            local_folder: String::new(),
            album_id: None,
            scan_frequency_minutes: 30,
        }
    }
}

impl Settings {
    fn load(app: &AppHandle) -> Self {
        let path = settings_path(app);
        fs::read_to_string(path)
            .ok()
            .and_then(|contents| serde_json::from_str(&contents).ok())
            .unwrap_or_default()
    }

    fn save(&self, app: &AppHandle) -> Result<(), String> {
        let path = settings_path(app);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }
}

fn settings_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("settings.json"))
        .unwrap_or_else(|_| std::path::PathBuf::from("settings.json"))
}

#[derive(Serialize, Clone, Default)]
struct ScanStatus {
    running: bool,
    last_run: Option<String>,
    uploaded: usize,
    total: usize,
    error: Option<String>,
}

#[derive(Serialize, Clone)]
struct ScanResult {
    uploaded: usize,
    total: usize,
}

#[derive(Serialize, Clone)]
struct PullResult {
    downloaded: usize,
    skipped: usize,
    total: usize,
}

#[derive(Serialize, Clone, Default)]
struct SyncDiff {
    local_count: usize,
    remote_count: usize,
    local_only: Vec<String>,
    remote_only: Vec<String>,
    synced: Vec<String>,
    conflicts: Vec<Conflict>,
}

#[derive(Serialize, Clone)]
struct Conflict {
    file: String,
    local_checksum: String,
    remote_checksum: String,
}

#[derive(Serialize, Clone)]
struct ResolveResult {
    action: String,
    file: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct PulledAsset {
    asset_id: String,
    file_name: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
struct Manifest {
    assets: Vec<PulledAsset>,
}

impl Manifest {
    fn load(folder: &str) -> Self {
        let path = Path::new(folder).join(".symmich.json");
        fs::read_to_string(path)
            .ok()
            .and_then(|contents| serde_json::from_str(&contents).ok())
            .unwrap_or_default()
    }

    fn save(&self, folder: &str) -> Result<(), String> {
        let path = Path::new(folder).join(".symmich.json");
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }

    fn contains_asset(&self, asset_id: &str) -> bool {
        self.assets.iter().any(|a| a.asset_id == asset_id)
    }

    fn contains_file(&self, file_name: &str) -> bool {
        self.assets.iter().any(|a| a.file_name == file_name)
    }
}

struct ScanSnapshot {
    album_name: String,
    album_id: Option<String>,
    local_folder: String,
}

struct Session {
    server_url: String,
    email: String,
    token: String,
    password: String,
    album_name: String,
    album_id: Option<String>,
    local_folder: String,
    scan_frequency_minutes: u64,
    scan_running: bool,
    last_scan: Option<String>,
    last_scan_count: usize,
    last_scan_total: usize,
    last_scan_error: Option<String>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            email: String::new(),
            token: String::new(),
            password: String::new(),
            album_name: String::new(),
            album_id: None,
            local_folder: String::new(),
            scan_frequency_minutes: 30,
            scan_running: false,
            last_scan: None,
            last_scan_count: 0,
            last_scan_total: 0,
            last_scan_error: None,
        }
    }
}

impl Session {
    fn load(app: &AppHandle) -> Self {
        let settings = Settings::load(app);
        let password = load_password_from_keyring(&settings.email).unwrap_or_default();
        Self {
            server_url: settings.server_url,
            email: settings.email,
            token: String::new(),
            password,
            album_name: settings.album_name,
            album_id: settings.album_id,
            local_folder: settings.local_folder,
            scan_frequency_minutes: settings.scan_frequency_minutes,
            scan_running: false,
            last_scan: None,
            last_scan_count: 0,
            last_scan_total: 0,
            last_scan_error: None,
        }
    }
}

fn persist_settings(session: &Session, app: &AppHandle) -> Result<(), String> {
    settings_from_session(session).save(app)
}

fn settings_from_session(session: &Session) -> Settings {
    Settings {
        server_url: session.server_url.clone(),
        email: session.email.clone(),
        album_name: session.album_name.clone(),
        local_folder: session.local_folder.clone(),
        album_id: session.album_id.clone(),
        scan_frequency_minutes: session.scan_frequency_minutes,
    }
}

fn server_url(app: &AppHandle) -> String {
    let state = app.state::<Mutex<Session>>();
    let value = match state.lock() {
        Ok(session) => session.server_url.trim_end_matches('/').to_string(),
        Err(_) => String::new(),
    };
    value
}

// ---------- Commands: settings ----------

#[derive(Serialize, Clone)]
struct SessionInfo {
    server_url: String,
    email: String,
}

#[tauri::command]
fn get_session(state: State<'_, Mutex<Session>>) -> Result<Option<SessionInfo>, String> {
    let session = state.lock().map_err(|e| e.to_string())?;
    if session.password.is_empty() {
        Ok(None)
    } else {
        Ok(Some(SessionInfo {
            server_url: session.server_url.clone(),
            email: session.email.clone(),
        }))
    }
}

#[tauri::command]
fn get_settings(state: State<'_, Mutex<Session>>) -> Result<Settings, String> {
    let session = state.lock().map_err(|e| e.to_string())?;
    Ok(settings_from_session(&session))
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<'_, Mutex<Session>>,
    settings: Settings,
) -> Result<(), String> {
    let mut session = state.lock().map_err(|e| e.to_string())?;
    session.server_url = settings.server_url.clone();
    session.email = settings.email.clone();
    session.album_name = settings.album_name.clone();
    session.local_folder = settings.local_folder.clone();
    session.album_id = settings.album_id.clone();
    session.scan_frequency_minutes = settings.scan_frequency_minutes;
    settings.save(&app)
}

#[tauri::command]
fn get_scan_status(state: State<'_, Mutex<Session>>) -> Result<ScanStatus, String> {
    let session = state.lock().map_err(|e| e.to_string())?;
    Ok(ScanStatus {
        running: session.scan_running,
        last_run: session.last_scan.clone(),
        uploaded: session.last_scan_count,
        total: session.last_scan_total,
        error: session.last_scan_error.clone(),
    })
}

// ---------- Auth ----------

fn token_expiry(token: &str) -> Option<i64> {
    let mut parts = token.split('.');
    parts.next()?;
    let payload = parts.next()?;
    let decoded = BASE64.decode(payload.as_bytes()).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    value.get("exp").and_then(|e| e.as_i64())
}

fn token_expired(token: &str) -> bool {
    match token_expiry(token) {
        Some(exp) => {
            let now = chrono::Utc::now().timestamp();
            exp - 60 <= now
        }
        None => false,
    }
}

async fn relogin(app: &AppHandle) -> Result<String, String> {
    let (server_url, email, password) = {
        let state = app.state::<Mutex<Session>>();
        let session = state.lock().map_err(|e| e.to_string())?;
        (
            session.server_url.clone(),
            session.email.clone(),
            session.password.clone(),
        )
    };

    if email.is_empty() || password.is_empty() {
        return Err("Not logged in".to_string());
    }

    let client = reqwest::Client::new();
    let url = format!("{}/api/auth/login", server_url.trim_end_matches('/'));
    let response = client
        .post(&url)
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    let body = response.bytes().await.map_err(|e| e.to_string())?;
    let parsed: serde_json::Value =
        serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);

    if !status.is_success() {
        return Err(format!("Re-login failed with status {}: {}", status, parsed));
    }

    let token = parsed
        .get("accessToken")
        .and_then(|t| t.as_str())
        .unwrap_or_default()
        .to_string();
    if token.is_empty() {
        return Err("Re-login returned no token".to_string());
    }

    let state = app.state::<Mutex<Session>>();
    let mut session = state.lock().map_err(|e| e.to_string())?;
    session.token = token.clone();
    Ok(token)
}

async fn valid_token(app: &AppHandle) -> Result<String, String> {
    let token = {
        let state = app.state::<Mutex<Session>>();
        let session = state.lock().map_err(|e| e.to_string())?;
        session.token.clone()
    };

    if token.is_empty() || token_expired(&token) {
        return relogin(app).await;
    }
    Ok(token)
}

async fn api_json_request(
    app: &AppHandle,
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<(reqwest::StatusCode, serde_json::Value), String> {
    let client = reqwest::Client::new();
    let url = format!("{}{}", server_url(app), path);

    for _ in 0..2 {
        let token = valid_token(app).await?;
        let mut request = client.request(method.clone(), &url).bearer_auth(&token);
        if let Some(b) = &body {
            request = request.json(b);
        }

        let response = request.send().await.map_err(|e| e.to_string())?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        let parsed: serde_json::Value =
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);

        if status == reqwest::StatusCode::UNAUTHORIZED {
            relogin(app).await?;
            continue;
        }

        return Ok((status, parsed));
    }

    Err("Unauthorized".to_string())
}

#[tauri::command]
async fn login(
    app: AppHandle,
    state: State<'_, Mutex<Session>>,
    url: String,
    email: String,
    password: String,
) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();
    let full_url = format!("{}/api/auth/login", url.trim_end_matches('/'));
    let payload = serde_json::json!({
        "email": email,
        "password": password
    });

    let response = client
        .post(&full_url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    let body = response.bytes().await.map_err(|e| e.to_string())?;
    let parsed: serde_json::Value =
        serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);

    if !status.is_success() {
        return Err(format!("Login failed with status {}: {}", status, parsed));
    }

    let token = parsed
        .get("accessToken")
        .and_then(|t| t.as_str())
        .unwrap_or_default()
        .to_string();

    save_password_to_keyring(&email, &password);

    let mut session = state.lock().map_err(|e| e.to_string())?;
    session.server_url = url;
    session.email = email;
    session.password = password;
    session.token = token;
    persist_settings(&session, &app)?;

    Ok(parsed)
}

#[tauri::command]
async fn logout(app: AppHandle) -> Result<(), String> {
    let token = valid_token(&app).await?;
    let client = reqwest::Client::new();
    let full_url = format!("{}/api/auth/logout", server_url(&app));

    let response = client
        .post(&full_url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("Logout failed with status {}", status));
    }

    let state = app.state::<Mutex<Session>>();
    let mut session = state.lock().map_err(|e| e.to_string())?;
    delete_password_from_keyring(&session.email);
    session.token = String::new();
    session.password = String::new();

    Ok(())
}

// ---------- File scanning ----------

#[tauri::command]
fn list_files_in_directory(path: &str) -> Result<Vec<String>, String> {
    let mut file_list = Vec::new();
    let entries = fs::read_dir(path).map_err(|e| e.to_string())?;

    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_file() {
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                file_list.push(file_name.to_string());
            }
        }
    }

    Ok(file_list)
}

fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| IMAGE_EXTENSIONS.iter().any(|i| ext.eq_ignore_ascii_case(i)))
        .unwrap_or(false)
}

fn scan_images(folder: &str) -> Result<Vec<String>, String> {
    let manifest = Manifest::load(folder);
    let mut images = Vec::new();
    let entries = fs::read_dir(folder).map_err(|e| e.to_string())?;

    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if manifest.contains_file(file_name) {
            continue;
        }
        if is_image_path(&path) {
            if let Some(p) = path.to_str() {
                images.push(p.to_string());
            }
        }
    }

    images.sort();
    Ok(images)
}

fn scan_all_images(folder: &str) -> Result<Vec<String>, String> {
    let mut images = Vec::new();
    let entries = fs::read_dir(folder).map_err(|e| e.to_string())?;

    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_file() && is_image_path(&path) {
            if let Some(p) = path.to_str() {
                images.push(p.to_string());
            }
        }
    }

    images.sort();
    Ok(images)
}

#[tauri::command]
fn find_images(state: State<'_, Mutex<Session>>) -> Result<Vec<String>, String> {
    let folder = state.lock().map_err(|e| e.to_string())?.local_folder.clone();
    scan_images(&folder)
}

fn file_sha1_hex(path: &str) -> Result<String, String> {
    let data = fs::read(path).map_err(|e| e.to_string())?;
    let mut hasher = sha1_smol::Sha1::new();
    hasher.update(&data);
    Ok(hasher.digest().to_string())
}

fn asset_checksum_hex(asset: &serde_json::Value) -> Option<String> {
    let checksum = asset.get("checksum").and_then(|c| c.as_str())?;
    let bytes = BASE64.decode(checksum.as_bytes()).ok()?;
    Some(hex::encode(bytes))
}

// ---------- Albums ----------

async fn find_album_api(app: &AppHandle, album_name: &str) -> Result<Option<String>, String> {
    let (status, parsed) =
        api_json_request(app, reqwest::Method::GET, "/api/albums", None).await?;
    if !status.is_success() {
        return Err(format!("Fetching albums failed with status {}: {}", status, parsed));
    }

    let albums = parsed
        .as_array()
        .ok_or_else(|| "Unexpected response format".to_string())?;

    Ok(albums
        .iter()
        .find(|a| a.get("albumName").and_then(|n| n.as_str()) == Some(album_name))
        .and_then(|a| a.get("id").and_then(|i| i.as_str()))
        .map(|s| s.to_string()))
}

async fn create_album_api(app: &AppHandle, album_name: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "albumName": album_name,
        "description": "Created via Symmich desktop sync client"
    });
    let (status, parsed) =
        api_json_request(app, reqwest::Method::POST, "/api/albums", Some(body)).await?;
    if !status.is_success() {
        return Err(format!("Creating album failed with status {}: {}", status, parsed));
    }

    parsed
        .get("id")
        .and_then(|i| i.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "No album id in response".to_string())
}

async fn resolve_album(app: &AppHandle, album_name: &str) -> Result<String, String> {
    if let Some(id) = find_album_api(app, album_name).await? {
        return Ok(id);
    }
    create_album_api(app, album_name).await
}

#[tauri::command]
async fn create_album(
    app: AppHandle,
    state: State<'_, Mutex<Session>>,
) -> Result<String, String> {
    let album_name = {
        let session = state.lock().map_err(|e| e.to_string())?;
        session.album_name.clone()
    };
    if album_name.trim().is_empty() {
        return Err("Album name is empty".to_string());
    }

    let album_id = create_album_api(&app, &album_name).await?;

    let mut session = state.lock().map_err(|e| e.to_string())?;
    session.album_id = Some(album_id.clone());
    persist_settings(&session, &app)?;

    Ok(album_id)
}

#[tauri::command]
async fn find_album_by_name(
    app: AppHandle,
    state: State<'_, Mutex<Session>>,
) -> Result<String, String> {
    let album_name = {
        let session = state.lock().map_err(|e| e.to_string())?;
        session.album_name.clone()
    };
    if album_name.trim().is_empty() {
        return Err("Album name is empty".to_string());
    }

    let album_id = find_album_api(&app, &album_name)
        .await?
        .ok_or_else(|| format!("No album named '{}' found", album_name))?;

    let mut session = state.lock().map_err(|e| e.to_string())?;
    session.album_id = Some(album_id.clone());
    persist_settings(&session, &app)?;

    Ok(album_id)
}

// ---------- Upload ----------

fn asset_metadata(path: &str) -> Result<(String, String), String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    let file_name = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("image")
        .to_string();

    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok());

    let created_at = modified
        .and_then(|d| {
            chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, d.subsec_nanos())
        })
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default();

    let device_asset_id = format!(
        "{}-{}-{}",
        file_name,
        meta.len(),
        modified.map(|d| d.as_secs()).unwrap_or(0)
    );

    Ok((created_at, device_asset_id))
}

async fn upload_asset_form(image_path: &str) -> Result<reqwest::multipart::Form, String> {
    let (created_at, device_asset_id) = asset_metadata(image_path)?;
    let part = reqwest::multipart::Part::file(image_path)
        .await
        .map_err(|e| e.to_string())?;
    Ok(reqwest::multipart::Form::new()
        .part("assetData", part)
        .text("deviceAssetId", device_asset_id)
        .text("deviceId", "symmich-desktop")
        .text("assetType", "IMAGE")
        .text("fileCreatedAt", created_at))
}

async fn upload_asset(app: &AppHandle, image_path: &str) -> Result<reqwest::Response, String> {
    let client = reqwest::Client::new();
    let upload_url = format!("{}/api/assets", server_url(app));

    for _ in 0..2 {
        let token = valid_token(app).await?;
        let form = upload_asset_form(image_path).await?;
        let response = client
            .post(&upload_url)
            .bearer_auth(&token)
            .multipart(form)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            relogin(app).await?;
            continue;
        }

        return Ok(response);
    }

    Err("Unauthorized".to_string())
}

async fn upload_image(app: &AppHandle, album_id: &str, image_path: &str) -> Result<(), String> {
    let file_name = Path::new(image_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("image")
        .to_string();

    let response = upload_asset(app, image_path).await?;
    let status = response.status();
    let body = response.bytes().await.map_err(|e| e.to_string())?;
    let parsed: serde_json::Value =
        serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);

    if !status.is_success() {
        return Err(format!("Upload failed with status {}: {}", status, parsed));
    }

    let asset_id = parsed
        .get("id")
        .and_then(|i| i.as_str())
        .ok_or_else(|| format!("No asset id returned for {}", file_name))?
        .to_string();

    let add_body = serde_json::json!({ "ids": [asset_id] });
    let (add_status, add_parsed) = api_json_request(
        app,
        reqwest::Method::PUT,
        &format!("/api/albums/{}/assets", album_id),
        Some(add_body),
    )
    .await?;

    if !add_status.is_success() {
        return Err(format!(
            "Adding to album failed with status {}: {}",
            add_status, add_parsed
        ));
    }

    Ok(())
}

// ---------- Pull / two-way sync ----------

fn ext_from_asset(asset: &serde_json::Value) -> String {
    if let Some(name) = asset.get("originalFileName").and_then(|n| n.as_str()) {
        if let Some(ext) = Path::new(name).extension().and_then(|e| e.to_str()) {
            return format!(".{}", ext.to_lowercase());
        }
    }
    match asset.get("mimeType").and_then(|m| m.as_str()) {
        Some("image/png") => ".png".to_string(),
        Some("image/webp") => ".webp".to_string(),
        Some("image/gif") => ".gif".to_string(),
        Some("image/heic") => ".heic".to_string(),
        Some("image/heif") => ".heif".to_string(),
        _ => ".jpg".to_string(),
    }
}

async fn download_asset(app: &AppHandle, asset_id: &str) -> Result<Vec<u8>, String> {
    let token = valid_token(app).await?;
    let client = reqwest::Client::new();
    let url = format!("{}/api/assets/{}/original", server_url(app), asset_id);

    let response = client
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;

    if status == reqwest::StatusCode::UNAUTHORIZED {
        let token = relogin(app).await?;
        let response = client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("Download failed with status {}", status));
        }
        return Ok(bytes.to_vec());
    }

    if !status.is_success() {
        return Err(format!("Download failed with status {}", status));
    }

    Ok(bytes.to_vec())
}

#[tauri::command]
async fn pull_album(app: AppHandle) -> Result<PullResult, String> {
    let (album_id, local_folder) = {
        let state = app.state::<Mutex<Session>>();
        let session = state.lock().map_err(|e| e.to_string())?;
        (session.album_id.clone(), session.local_folder.clone())
    };

    let album_id = album_id.ok_or_else(|| "No album selected. Create or find an album first.".to_string())?;
    if local_folder.trim().is_empty() {
        return Err("No local folder set. Add one in Settings.".to_string());
    }
    fs::create_dir_all(&local_folder).map_err(|e| e.to_string())?;

    let (status, parsed) = api_json_request(
        &app,
        reqwest::Method::GET,
        &format!("/api/albums/{}", album_id),
        None,
    )
    .await?;
    if !status.is_success() {
        return Err(format!("Fetching album failed with status {}: {}", status, parsed));
    }

    let assets = parsed
        .get("assets")
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();

    let mut manifest = Manifest::load(&local_folder);
    let mut downloaded = 0;
    let mut skipped = 0;
    let total = assets.len();

    for asset in &assets {
        let asset_id = asset.get("id").and_then(|i| i.as_str()).unwrap_or("");
        if asset_id.is_empty() || manifest.contains_asset(asset_id) {
            skipped += 1;
            continue;
        }

        let file_name = format!("{}{}", asset_id, ext_from_asset(asset));
        let bytes = download_asset(&app, asset_id).await?;
        fs::write(Path::new(&local_folder).join(&file_name), bytes).map_err(|e| e.to_string())?;

        manifest.assets.push(PulledAsset {
            asset_id: asset_id.to_string(),
            file_name: file_name.clone(),
        });
        downloaded += 1;

        let _ = app.emit(
            "scan-progress",
            serde_json::json!({ "current": downloaded, "total": total, "file": file_name }),
        );
    }

    manifest.save(&local_folder)?;

    let result = PullResult {
        downloaded,
        skipped,
        total,
    };
    let _ = app.emit("pull-status", &result);
    Ok(result)
}

// ---------- Local <-> Remote diff & conflict resolution ----------

#[tauri::command]
async fn sync_diff(app: AppHandle) -> Result<SyncDiff, String> {
    let (album_id, local_folder) = {
        let state = app.state::<Mutex<Session>>();
        let session = state.lock().map_err(|e| e.to_string())?;
        (session.album_id.clone(), session.local_folder.clone())
    };

    let album_id = album_id.ok_or_else(|| "No album selected. Create or find an album first.".to_string())?;
    if local_folder.trim().is_empty() {
        return Err("No local folder set. Add one in Settings.".to_string());
    }

    let (status, parsed) = api_json_request(
        &app,
        reqwest::Method::GET,
        &format!("/api/albums/{}", album_id),
        None,
    )
    .await?;
    if !status.is_success() {
        return Err(format!("Fetching album failed with status {}: {}", status, parsed));
    }

    let assets = parsed
        .get("assets")
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();

    let mut remote_by_checksum: HashMap<String, &serde_json::Value> = HashMap::new();
    let mut remote_name_to_checksum: HashMap<String, String> = HashMap::new();
    for asset in &assets {
        if let Some(checksum) = asset_checksum_hex(asset) {
            remote_by_checksum.insert(checksum.clone(), asset);
            if let Some(name) = asset.get("originalFileName").and_then(|n| n.as_str()) {
                if let Some(base) = Path::new(name).file_name().and_then(|f| f.to_str()) {
                    remote_name_to_checksum.insert(base.to_string(), checksum);
                }
            }
        }
    }

    let local_files = scan_all_images(&local_folder)?;
    let mut synced = Vec::new();
    let mut local_only = Vec::new();
    let mut conflicts = Vec::new();
    let mut matched_remote = std::collections::HashSet::new();

    for file in &local_files {
        let local_checksum = file_sha1_hex(file)?;
        let file_name = Path::new(file)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        if remote_by_checksum.contains_key(&local_checksum) {
            matched_remote.insert(local_checksum);
            synced.push(file_name);
            continue;
        }

        if let Some(remote_checksum) = remote_name_to_checksum.get(&file_name) {
            if *remote_checksum != local_checksum {
                conflicts.push(Conflict {
                    file: file_name,
                    local_checksum,
                    remote_checksum: remote_checksum.clone(),
                });
                continue;
            }
        }

        local_only.push(file_name);
    }

    let mut remote_only = Vec::new();
    for asset in &assets {
        let Some(checksum) = asset_checksum_hex(asset) else {
            continue;
        };
        if matched_remote.contains(&checksum) {
            continue;
        }
        if let Some(name) = asset.get("originalFileName").and_then(|n| n.as_str()) {
            if let Some(base) = Path::new(name).file_name().and_then(|f| f.to_str()) {
                remote_only.push(base.to_string());
                continue;
            }
        }
        if let Some(id) = asset.get("id").and_then(|i| i.as_str()) {
            remote_only.push(format!("{}{}", id, ext_from_asset(asset)));
        }
    }

    Ok(SyncDiff {
        local_count: local_files.len(),
        remote_count: assets.len(),
        local_only,
        remote_only,
        synced,
        conflicts,
    })
}

#[tauri::command]
async fn resolve_conflict(
    app: AppHandle,
    file: String,
    keep: String,
) -> Result<ResolveResult, String> {
    let album_id = {
        let state = app.state::<Mutex<Session>>();
        let session = state.lock().map_err(|e| e.to_string())?;
        session.album_id.clone()
    };
    let album_id = album_id.ok_or_else(|| "No album selected. Create or find an album first.".to_string())?;

    let (status, parsed) = api_json_request(
        &app,
        reqwest::Method::GET,
        &format!("/api/albums/{}", album_id),
        None,
    )
    .await?;
    if !status.is_success() {
        return Err(format!("Fetching album failed with status {}: {}", status, parsed));
    }

    let assets = parsed
        .get("assets")
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();

    let asset = assets
        .iter()
        .find(|a| {
            a.get("originalFileName")
                .and_then(|n| n.as_str())
                .map(|n| Path::new(n).file_name().and_then(|f| f.to_str()).unwrap_or(n) == file)
                .unwrap_or(false)
        })
        .ok_or_else(|| format!("No remote asset named '{}' found", file))?;

    let asset_id = asset
        .get("id")
        .and_then(|i| i.as_str())
        .ok_or_else(|| "Missing asset id".to_string())?;

    let local_folder = {
        let state = app.state::<Mutex<Session>>();
        let session = state.lock().map_err(|e| e.to_string())?;
        session.local_folder.clone()
    };

    if keep == "remote" {
        let bytes = download_asset(&app, asset_id).await?;
        fs::write(Path::new(&local_folder).join(&file), bytes).map_err(|e| e.to_string())?;
        Ok(ResolveResult {
            action: "remote".to_string(),
            file,
        })
    } else {
        let body = serde_json::json!({ "ids": [asset_id] });
        let (status, parsed) = api_json_request(
            &app,
            reqwest::Method::DELETE,
            &format!("/api/albums/{}/assets", album_id),
            Some(body),
        )
        .await?;
        if !status.is_success() {
            return Err(format!(
                "Removing asset from album failed with status {}: {}",
                status, parsed
            ));
        }
        Ok(ResolveResult {
            action: "local".to_string(),
            file,
        })
    }
}

// ---------- Scan orchestration ----------

fn emit_status(app: &AppHandle, status: &ScanStatus) {
    let _ = app.emit("scan-status", status);
}

fn emit_progress(app: &AppHandle, current: usize, total: usize, file: &str) {
    let _ = app.emit(
        "scan-progress",
        serde_json::json!({ "current": current, "total": total, "file": file }),
    );
}

#[tauri::command]
async fn run_scan(app: AppHandle) -> Result<ScanResult, String> {
    run_scan_impl(&app).await
}

async fn run_scan_impl(app: &AppHandle) -> Result<ScanResult, String> {
    let snapshot = {
        let state = app.state::<Mutex<Session>>();
        let session = state.lock().map_err(|e| e.to_string())?;
        if session.scan_running {
            return Err("A scan is already running".to_string());
        }
        ScanSnapshot {
            album_name: session.album_name.clone(),
            album_id: session.album_id.clone(),
            local_folder: session.local_folder.clone(),
        }
    };

    if snapshot.local_folder.trim().is_empty() {
        return Err("No local folder set. Add one in Settings.".to_string());
    }

    {
        let state = app.state::<Mutex<Session>>();
        let mut session = state.lock().map_err(|e| e.to_string())?;
        session.scan_running = true;
        session.last_scan_error = None;
    }
    emit_status(
        app,
        &ScanStatus {
            running: true,
            ..Default::default()
        },
    );

    let outcome: Result<ScanResult, String> = async {
        let album_id = match snapshot.album_id {
            Some(id) if !id.is_empty() => id,
            _ => {
                let name = snapshot.album_name.trim().to_string();
                if name.is_empty() {
                    return Err("No album selected and album name is empty".to_string());
                }
                let id = resolve_album(app, &name).await?;
                let state = app.state::<Mutex<Session>>();
                let mut session = state.lock().map_err(|e| e.to_string())?;
                session.album_id = Some(id.clone());
                persist_settings(&session, app)?;
                id
            }
        };

        let images = scan_images(&snapshot.local_folder)?;
        let total = images.len();
        let mut uploaded = 0;

        for (index, image) in images.iter().enumerate() {
            if let Err(e) = upload_image(app, &album_id, image).await {
                return Err(format!("{}: {}", image, e));
            }
            uploaded += 1;
            emit_progress(app, index + 1, total, image);
        }

        Ok(ScanResult { uploaded, total })
    }
    .await;

    let now = chrono::Utc::now().to_rfc3339();
    let (uploaded, total, error) = match &outcome {
        Ok(result) => (result.uploaded, result.total, None),
        Err(e) => (0, 0, Some(e.clone())),
    };

    {
        let state = app.state::<Mutex<Session>>();
        let mut session = state.lock().map_err(|e| e.to_string())?;
        session.scan_running = false;
        session.last_scan = Some(now.clone());
        session.last_scan_count = uploaded;
        session.last_scan_total = total;
        session.last_scan_error = error.clone();
    }

    emit_status(
        app,
        &ScanStatus {
            running: false,
            last_run: Some(now),
            uploaded,
            total,
            error: error.clone(),
        },
    );

    outcome
}

// ---------- File watcher + auto scan ----------

fn build_watcher(app: &AppHandle, folder: &str) -> Result<notify::RecommendedWatcher, String> {
    use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};

    let app = app.clone();
    let mut last_trigger = std::time::Instant::now();

    let mut watcher = RecommendedWatcher::new(
        move |result: Result<notify::Event, notify::Error>| {
            if let Ok(event) = result {
                let relevant = event.paths.iter().any(|p| is_image_path(p));
                if relevant && last_trigger.elapsed() >= Duration::from_secs(10) {
                    last_trigger = std::time::Instant::now();
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = run_scan_impl(&app).await;
                    });
                }
            }
        },
        Config::default(),
    )
    .map_err(|e| e.to_string())?;

    watcher
        .watch(Path::new(folder), RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;

    Ok(watcher)
}

async fn auto_scan_loop(app: AppHandle) {
    let mut watcher: Option<notify::RecommendedWatcher> = None;
    let mut watched_folder: Option<String> = None;

    loop {
        let (folder, frequency) = {
            let state = app.state::<Mutex<Session>>();
            let value = match state.lock() {
                Ok(session) => (session.local_folder.clone(), session.scan_frequency_minutes),
                Err(_) => (String::new(), 0),
            };
            value
        };

        let folder = if folder.trim().is_empty() {
            None
        } else {
            Some(folder)
        };

        if folder != watched_folder {
            watcher.take();
            watched_folder = folder.clone();
            if let Some(f) = folder {
                match build_watcher(&app, &f) {
                    Ok(w) => watcher = Some(w),
                    Err(e) => {
                        tracing::error!("failed to start file watcher: {}", e);
                        watched_folder = None;
                    }
                }
            }
        }

        let delay = if frequency > 0 {
            frequency * 60
        } else {
            60
        };
        tokio::time::sleep(Duration::from_secs(delay)).await;

        if frequency > 0 {
            let _ = run_scan_impl(&app).await;
        }
    }
}

// ---------- Tray icon ----------

fn setup_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show Symmich", true, None::<&str>)?;
    let scan = MenuItem::with_id(app, "scan", "Scan now", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &scan, &quit])?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| "no default window icon".to_string())?;

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("Symmich")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            match event.id.as_ref() {
                "show" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "scan" => {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = run_scan_impl(&app).await;
                    });
                }
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let session = Session::load(app.handle());
            app.manage(Mutex::new(session));

            if let Err(e) = setup_tray(app) {
                tracing::error!("failed to create tray icon: {}", e);
            }

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                auto_scan_loop(app_handle).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            login,
            logout,
            get_session,
            get_settings,
            save_settings,
            get_scan_status,
            list_files_in_directory,
            find_images,
            create_album,
            find_album_by_name,
            run_scan,
            pull_album,
            sync_diff,
            resolve_conflict
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}