import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";
import {
  ActivityIcon,
  AlertTriangleIcon,
  CheckCircleIcon,
  DownloadCloudIcon,
  FolderIcon,
  ImageIcon,
  LayersIcon,
  LockIcon,
  LogOutIcon,
  MailIcon,
  RefreshCwIcon,
  ServerIcon,
  SettingsIcon,
  UploadCloudIcon,
} from "./icons";

type CloseBehavior = "hide" | "quit_confirm";
type Tab = "overview" | "activity" | "settings";

interface SessionInfo {
  server_url: string;
  email: string;
}

interface Settings {
  server_url: string;
  email: string;
  album_name: string;
  local_folder: string;
  album_id?: string | null;
  scan_frequency_minutes: number;
  close_behavior: CloseBehavior;
}

interface ScanStatus {
  running: boolean;
  last_run?: string | null;
  uploaded?: number;
  total?: number;
  error?: string | null;
}

interface ScanProgress {
  current: number;
  total: number;
  file: string;
}

interface Conflict {
  file: string;
  local_checksum: string;
  remote_checksum: string;
}

interface SyncDiff {
  local_count: number;
  remote_count: number;
  local_only: string[];
  remote_only: string[];
  synced: string[];
  conflicts: Conflict[];
}

interface PullResult {
  downloaded: number;
  skipped: number;
  total: number;
}

function timeAgo(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return iso;
  const seconds = Math.round((Date.now() - then) / 1000);
  if (seconds < 10) return "just now";
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.round(hours / 24);
  return `${days}d ago`;
}

function App() {
  const [loggedIn, setLoggedIn] = useState(false);
  const [tab, setTab] = useState<Tab>("overview");

  const [serverUrl, setServerUrl] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [albumName, setAlbumName] = useState("");
  const [localFolder, setLocalFolder] = useState("");
  const [albumId, setAlbumId] = useState("");
  const [scanFrequency, setScanFrequency] = useState("30");
  const [closeBehavior, setCloseBehavior] = useState<CloseBehavior>("hide");
  const [images, setImages] = useState<string[] | null>(null);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  const [scanning, setScanning] = useState(false);
  const [lastRun, setLastRun] = useState("");
  const [scanError, setScanError] = useState("");
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [diff, setDiff] = useState<SyncDiff | null>(null);

  useEffect(() => {
    invoke<SessionInfo | null>("get_session")
      .then((s) => {
        if (s) {
          setLoggedIn(true);
          refreshActivity();
        }
      })
      .catch(() => {});

    invoke<Settings>("get_settings")
      .then((s) => {
        setServerUrl(s.server_url);
        setEmail(s.email);
        setAlbumName(s.album_name);
        setLocalFolder(s.local_folder);
        setAlbumId(s.album_id ?? "");
        setScanFrequency(String(s.scan_frequency_minutes ?? 0));
        setCloseBehavior(s.close_behavior ?? "hide");
      })
      .catch(() => {});

    invoke<ScanStatus>("get_scan_status")
      .then((s) => {
        setScanning(s.running);
        setLastRun(s.last_run ?? "");
        setScanError(s.error ?? "");
      })
      .catch(() => {});

    const unlistenStatus = listen<ScanStatus>("scan-status", (e) => {
      setScanning(e.payload.running);
      if (e.payload.last_run) setLastRun(e.payload.last_run);
      if (e.payload.error) setScanError(e.payload.error);
      else if (!e.payload.running) setScanError("");
      if (!e.payload.running && e.payload.total !== undefined) {
        setMessage(
          e.payload.error
            ? `Scan failed: ${e.payload.error}`
            : `Scan finished: ${e.payload.uploaded}/${e.payload.total} uploaded`,
        );
        setProgress(null);
        refreshActivity();
      }
    });
    const unlistenProgress = listen<ScanProgress>("scan-progress", (e) => {
      setProgress(e.payload);
    });

    return () => {
      unlistenStatus.then((fn) => fn());
      unlistenProgress.then((fn) => fn());
    };
  }, []);

  function notice(text: string) {
    setMessage(text);
    setError("");
  }

  function fail(err: unknown) {
    setError(String(err));
    setMessage("");
  }

  // Silently refreshes the Overview/Activity numbers (local images, synced,
  // conflicts). Errors are swallowed since this runs opportunistically
  // (e.g. before an album exists) and shouldn't surface as user-facing banners.
  function refreshActivity() {
    invoke<string[]>("find_images")
      .then(setImages)
      .catch(() => {});
    invoke<SyncDiff>("sync_diff")
      .then(setDiff)
      .catch(() => {});
  }

  async function handleLogin(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    try {
      await invoke("login", { url: serverUrl, email, password });
      setPassword("");
      notice("Logged in");
      setLoggedIn(true);
      refreshActivity();
    } catch (err) {
      fail(err);
    }
  }

  async function handleLogout() {
    try {
      await invoke("logout");
      setLoggedIn(false);
      notice("Logged out");
    } catch (err) {
      fail(err);
    }
  }

  async function handleSaveSettings() {
    try {
      await invoke("save_settings", {
        settings: {
          server_url: serverUrl,
          email,
          album_name: albumName,
          local_folder: localFolder,
          album_id: albumId || null,
          scan_frequency_minutes: Number(scanFrequency) || 0,
          close_behavior: closeBehavior,
        },
      });
      notice("Settings saved");
      refreshActivity();
    } catch (err) {
      fail(err);
    }
  }

  async function handleBrowseFolder() {
    try {
      const selected = await open({ directory: true });
      if (typeof selected === "string") {
        setLocalFolder(selected);
      }
    } catch (err) {
      fail(err);
    }
  }

  async function handleScanNow() {
    setScanError("");
    try {
      await invoke("run_scan");
    } catch (err) {
      fail(err);
    }
  }

  async function handleCreateAlbum() {
    try {
      const id = await invoke<string>("create_album");
      setAlbumId(id);
      notice(`Album created (${id})`);
      refreshActivity();
    } catch (err) {
      fail(err);
    }
  }

  async function handleFindAlbum() {
    try {
      const id = await invoke<string>("find_album_by_name");
      setAlbumId(id);
      notice(`Album found (${id})`);
      refreshActivity();
    } catch (err) {
      fail(err);
    }
  }

  async function handleFindImages() {
    try {
      const found = await invoke<string[]>("find_images");
      setImages(found);
      notice(`${found.length} image${found.length === 1 ? "" : "s"} pending upload`);
    } catch (err) {
      fail(err);
    }
  }

  async function handlePullAlbum() {
    setScanError("");
    try {
      const result = await invoke<PullResult>("pull_album");
      notice(
        `Pull complete: ${result.downloaded} downloaded, ${result.skipped} already present`,
      );
      refreshActivity();
    } catch (err) {
      fail(err);
    }
  }

  async function handleSyncDiff() {
    setScanError("");
    try {
      const result = await invoke<SyncDiff>("sync_diff");
      setDiff(result);
      notice(
        `Local ${result.local_count} · Remote ${result.remote_count} · Synced ${result.synced.length}`,
      );
    } catch (err) {
      fail(err);
    }
  }

  async function handleResolveConflict(file: string, keep: string) {
    try {
      const result = await invoke<{ action: string; file: string }>(
        "resolve_conflict",
        { file, keep },
      );
      notice(`Conflict resolved (${result.action}): ${result.file}`);
      refreshActivity();
    } catch (err) {
      fail(err);
    }
  }

  const conflictCount = diff?.conflicts.length ?? 0;
  const pendingUpload = diff?.local_only.length ?? 0;
  const pendingDownload = diff?.remote_only.length ?? 0;

  const statusVariant = scanning ? "running" : scanError ? "error" : "idle";
  const statusLabel = scanning ? "Syncing" : scanError ? "Error" : "Idle";
  const progressPct = useMemo(
    () =>
      progress ? Math.min(100, Math.round((progress.current / Math.max(1, progress.total)) * 100)) : 0,
    [progress],
  );

  if (!loggedIn) {
    return (
      <div className="login-shell">
        <div className="login-card">
          <div className="login-brand">
            <div className="login-mark">S</div>
          </div>
          <h1>Symmich</h1>
          <p className="subtitle">Sign in to your Immich server</p>
          <form className="form" onSubmit={handleLogin}>
            <div className="input-icon">
              <ServerIcon size={16} />
              <input
                value={serverUrl}
                onChange={(e) => setServerUrl(e.currentTarget.value)}
                placeholder="Server URL (e.g. https://immich.example.com)"
              />
            </div>
            <div className="input-icon">
              <MailIcon size={16} />
              <input
                value={email}
                onChange={(e) => setEmail(e.currentTarget.value)}
                placeholder="Email"
                type="email"
              />
            </div>
            <div className="input-icon">
              <LockIcon size={16} />
              <input
                value={password}
                onChange={(e) => setPassword(e.currentTarget.value)}
                placeholder="Password"
                type="password"
              />
            </div>
            <button type="submit" className="btn btn-primary btn-block">
              Login
            </button>
          </form>
          {error && <p className="banner banner-error">{error}</p>}
        </div>
      </div>
    );
  }

  return (
    <div className="app-shell">
      <header className="app-header">
        <div className="brand">
          <div className="brand-mark">S</div>
          <div>
            <h1>Symmich</h1>
            <p className="subtitle">{serverUrl || "Immich desktop sync"}</p>
          </div>
        </div>
        <div className="header-actions">
          <span className="status-pill">
            <span className={`status-dot status-dot--${statusVariant}`} />
            {statusLabel}
          </span>
          <button className="icon-btn" onClick={handleLogout} title="Logout">
            <LogOutIcon size={16} />
          </button>
        </div>
      </header>

      <nav className="tab-bar">
        <button
          className={`tab-btn ${tab === "overview" ? "tab-btn--active" : ""}`}
          onClick={() => setTab("overview")}
        >
          <LayersIcon size={16} />
          Overview
        </button>
        <button
          className={`tab-btn ${tab === "activity" ? "tab-btn--active" : ""}`}
          onClick={() => setTab("activity")}
        >
          <ActivityIcon size={16} />
          Activity
          {conflictCount > 0 && <span className="tab-badge">{conflictCount}</span>}
        </button>
        <button
          className={`tab-btn ${tab === "settings" ? "tab-btn--active" : ""}`}
          onClick={() => setTab("settings")}
        >
          <SettingsIcon size={16} />
          Settings
        </button>
      </nav>

      <div className="tab-content">
        {message && <p className="banner banner-info">{message}</p>}
        {error && <p className="banner banner-error">{error}</p>}

        {tab === "overview" && (
          <>
            <section className="card hero-card">
              <div className="hero-top">
                <div>
                  <h2>Sync</h2>
                  <p className="card-desc">Upload new local images to your Immich album.</p>
                </div>
                <button
                  className={`btn btn-primary btn-icon ${scanning ? "btn-loading" : ""}`}
                  onClick={handleScanNow}
                  disabled={scanning}
                >
                  <RefreshCwIcon size={16} className={scanning ? "spin" : ""} />
                  {scanning ? "Scanning…" : "Scan now"}
                </button>
              </div>

              {scanning && progress && (
                <div className="progress">
                  <div className="progress-track">
                    <div className="progress-fill" style={{ width: `${progressPct}%` }} />
                  </div>
                  <div className="progress-label">
                    <span>{progress.file}</span>
                    <span>
                      {progress.current}/{progress.total}
                    </span>
                  </div>
                </div>
              )}

              <div className="stat-grid">
                <div className="stat-tile">
                  <span className="stat-value">{images === null ? "—" : images.length}</span>
                  <span className="stat-label">Pending upload</span>
                </div>
                <div className="stat-tile">
                  <span className="stat-value">{diff === null ? "—" : diff.synced.length}</span>
                  <span className="stat-label">Synced</span>
                </div>
                <div className={`stat-tile ${conflictCount > 0 ? "stat-tile--warning" : ""}`}>
                  <span className="stat-value">{diff === null ? "—" : conflictCount}</span>
                  <span className="stat-label">Conflicts</span>
                </div>
              </div>

              <p className="meta-line">
                Last scan: <strong>{lastRun ? timeAgo(lastRun) : "never"}</strong>
                {scanFrequency !== "0" && <> · Auto every {scanFrequency}m</>}
              </p>
              {scanError && <p className="banner banner-error">{scanError}</p>}
            </section>

            <section className="card">
              <div className="card-title-row">
                <LayersIcon size={16} />
                <h2>Album</h2>
              </div>
              <p className="card-desc">
                Target album: <span className="mono">{albumName || "not set"}</span>
              </p>
              <p className="meta-line">
                Album ID:{" "}
                <span className="mono">{albumId || "none (created automatically on scan)"}</span>
              </p>
              <div className="row">
                <button className="btn" onClick={handleCreateAlbum}>
                  Create album
                </button>
                <button className="btn" onClick={handleFindAlbum}>
                  Find album by name
                </button>
              </div>
            </section>
          </>
        )}

        {tab === "activity" && (
          <>
            <section className="card">
              <div className="card-title-row">
                <ActivityIcon size={16} />
                <h2>Local ↔ Remote</h2>
              </div>
              <p className="card-desc">Pull remote assets down, or compare local and remote state.</p>
              <div className="row">
                <button className="btn btn-icon" onClick={handlePullAlbum}>
                  <DownloadCloudIcon size={16} />
                  Pull album to folder
                </button>
                <button className="btn btn-icon" onClick={handleSyncDiff}>
                  <RefreshCwIcon size={16} />
                  Check local ↔ remote
                </button>
              </div>

              {diff && (
                <div className="diff">
                  <div className="stat-grid stat-grid--4">
                    <div className="stat-tile">
                      <span className="stat-value">{diff.local_count}</span>
                      <span className="stat-label">Local</span>
                    </div>
                    <div className="stat-tile">
                      <span className="stat-value">{diff.remote_count}</span>
                      <span className="stat-label">Remote</span>
                    </div>
                    <div className="stat-tile">
                      <span className="stat-value">{pendingUpload}</span>
                      <span className="stat-label">To upload</span>
                    </div>
                    <div className="stat-tile">
                      <span className="stat-value">{pendingDownload}</span>
                      <span className="stat-label">To download</span>
                    </div>
                  </div>

                  {diff.conflicts.length > 0 ? (
                    <div>
                      <h3>
                        <AlertTriangleIcon size={14} /> Conflicts ({diff.conflicts.length})
                      </h3>
                      <ul className="conflict-list">
                        {diff.conflicts.map((c) => (
                          <li key={c.file} className="conflict-item">
                            <span className="file-name">{c.file}</span>
                            <div className="conflict-actions">
                              <button
                                className="btn"
                                onClick={() => handleResolveConflict(c.file, "local")}
                              >
                                Keep local
                              </button>
                              <button
                                className="btn"
                                onClick={() => handleResolveConflict(c.file, "remote")}
                              >
                                Keep remote
                              </button>
                            </div>
                          </li>
                        ))}
                      </ul>
                    </div>
                  ) : (
                    <p className="empty-state">
                      <CheckCircleIcon size={14} /> No conflicts — everything checked out.
                    </p>
                  )}
                </div>
              )}
            </section>

            <section className="card">
              <div className="card-title-row">
                <ImageIcon size={16} />
                <h2>Images in folder</h2>
              </div>
              <p className="card-desc">Local files not yet uploaded to the album.</p>
              <div className="row">
                <button className="btn" onClick={handleFindImages}>
                  List images
                </button>
              </div>
              {images && images.length > 0 ? (
                <ul className="file-list">
                  {images.map((image) => (
                    <li key={image}>{image}</li>
                  ))}
                </ul>
              ) : (
                <p className="empty-state">
                  <CheckCircleIcon size={14} />
                  {images === null ? "No images listed yet." : "Nothing pending — all uploaded."}
                </p>
              )}
            </section>
          </>
        )}

        {tab === "settings" && (
          <section className="card">
            <div className="card-title-row">
              <SettingsIcon size={16} />
              <h2>Settings</h2>
            </div>
            <p className="card-desc">Server connection, target album, and local folder.</p>
            <div className="form">
              <span className="field-group-label">Connection</span>
              <input
                value={serverUrl}
                onChange={(e) => setServerUrl(e.currentTarget.value)}
                placeholder="Server URL"
              />
              <input
                value={email}
                onChange={(e) => setEmail(e.currentTarget.value)}
                placeholder="Email"
                type="email"
              />

              <span className="field-group-label">Sync</span>
              <input
                value={albumName}
                onChange={(e) => setAlbumName(e.currentTarget.value)}
                placeholder="Album name"
              />
              <div className="form-row">
                <input
                  value={localFolder}
                  onChange={(e) => setLocalFolder(e.currentTarget.value)}
                  placeholder="Local folder with images"
                />
                <button className="btn btn-icon" onClick={handleBrowseFolder}>
                  <FolderIcon size={16} />
                  Browse…
                </button>
              </div>
              <label className="field-label">
                Scan frequency (minutes, 0 = manual only)
                <input
                  value={scanFrequency}
                  onChange={(e) => setScanFrequency(e.currentTarget.value)}
                  type="number"
                  min={0}
                />
              </label>

              <span className="field-group-label">Behavior</span>
              <label className="field-label">
                On window close
                <select
                  value={closeBehavior}
                  onChange={(e) => setCloseBehavior(e.currentTarget.value as CloseBehavior)}
                >
                  <option value="hide">Hide to tray</option>
                  <option value="quit_confirm">Ask before quitting</option>
                </select>
              </label>

              <button className="btn btn-primary" onClick={handleSaveSettings}>
                Save settings
              </button>
            </div>
          </section>
        )}
      </div>

      <footer className="status-bar">
        <span className="status-bar-item">
          <span className={`status-dot status-dot--${statusVariant}`} />
          {statusLabel}
        </span>
        <span className="status-bar-sep" />
        <span className="status-bar-item">
          <UploadCloudIcon size={13} />
          {diff ? `${pendingUpload} to upload` : "— to upload"}
        </span>
        <span className="status-bar-item">
          <DownloadCloudIcon size={13} />
          {diff ? `${pendingDownload} to download` : "— to download"}
        </span>
        {conflictCount > 0 && (
          <span className="status-bar-item status-bar-item--warning">
            <AlertTriangleIcon size={13} />
            {conflictCount} conflict{conflictCount === 1 ? "" : "s"}
          </span>
        )}
        <span className="status-bar-spacer" />
        <span className="status-bar-item status-bar-muted">
          {lastRun ? `Last sync ${timeAgo(lastRun)}` : "Never synced"}
        </span>
      </footer>
    </div>
  );
}

export default App;
