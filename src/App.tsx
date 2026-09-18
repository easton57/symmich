import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";

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

function App() {
  const [loggedIn, setLoggedIn] = useState(false);
  const [serverUrl, setServerUrl] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [albumName, setAlbumName] = useState("");
  const [localFolder, setLocalFolder] = useState("");
  const [albumId, setAlbumId] = useState("");
  const [scanFrequency, setScanFrequency] = useState("30");
  const [images, setImages] = useState<string[]>([]);
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
        if (s) setLoggedIn(true);
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
      if (!e.payload.running && e.payload.total !== undefined) {
        setMessage(
          e.payload.error
            ? `Scan failed: ${e.payload.error}`
            : `Scan finished: ${e.payload.uploaded}/${e.payload.total} uploaded`,
        );
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

  async function handleLogin(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    try {
      await invoke("login", { url: serverUrl, email, password });
      setPassword("");
      notice("Logged in");
      setLoggedIn(true);
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
        },
      });
      notice("Settings saved");
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
    } catch (err) {
      fail(err);
    }
  }

  async function handleFindAlbum() {
    try {
      const id = await invoke<string>("find_album_by_name");
      setAlbumId(id);
      notice(`Album found (${id})`);
    } catch (err) {
      fail(err);
    }
  }

  async function handleFindImages() {
    try {
      const found = await invoke<string[]>("find_images");
      setImages(found);
      notice(`Found ${found.length} images in folder`);
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
      setDiff(null);
    } catch (err) {
      fail(err);
    }
  }

  if (!loggedIn) {
    return (
      <main className="container">
        <h1>Symmich</h1>
        <form className="form" onSubmit={handleLogin}>
          <input
            value={serverUrl}
            onChange={(e) => setServerUrl(e.currentTarget.value)}
            placeholder="Server URL (e.g. https://immich.example.com)"
          />
          <input
            value={email}
            onChange={(e) => setEmail(e.currentTarget.value)}
            placeholder="Email"
            type="email"
          />
          <input
            value={password}
            onChange={(e) => setPassword(e.currentTarget.value)}
            placeholder="Password"
            type="password"
          />
          <button type="submit">Login</button>
        </form>
        {error && <p className="error">{error}</p>}
      </main>
    );
  }

  return (
    <main className="container">
      <h1>Symmich</h1>

      <section className="card">
        <h2>Settings</h2>
        <div className="form">
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
          <input
            value={albumName}
            onChange={(e) => setAlbumName(e.currentTarget.value)}
            placeholder="Album name"
          />
          <div className="row">
            <input
              value={localFolder}
              onChange={(e) => setLocalFolder(e.currentTarget.value)}
              placeholder="Local folder with images"
            />
            <button onClick={handleBrowseFolder}>Browse…</button>
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
          <button onClick={handleSaveSettings}>Save settings</button>
        </div>
      </section>

      <section className="card">
        <h2>Sync</h2>
        <div className="row">
          <button onClick={handleScanNow} disabled={scanning}>
            {scanning ? "Scanning…" : "Scan now"}
          </button>
        </div>
        {scanning && progress && (
          <p className="message">
            Uploading {progress.current}/{progress.total}: {progress.file}
          </p>
        )}
        <p>
          Status: {scanning ? "Running" : "Idle"}
          {lastRun && ` · Last scan: ${new Date(lastRun).toLocaleString()}`}
        </p>
        {scanError && <p className="error">{scanError}</p>}
      </section>

      <section className="card">
        <h2>Album</h2>
        <p>Album ID: {albumId || "none (created automatically on scan)"}</p>
        <div className="row">
          <button onClick={handleCreateAlbum}>Create album</button>
          <button onClick={handleFindAlbum}>Find album by name</button>
        </div>
      </section>

      <section className="card">
        <h2>Two-way sync</h2>
        <div className="row">
          <button onClick={handlePullAlbum}>Pull album to folder</button>
          <button onClick={handleSyncDiff}>Check local ↔ remote</button>
        </div>
        {diff && (
          <div className="diff">
            <p>
              Local: {diff.local_count} · Remote: {diff.remote_count} · Synced:{" "}
              {diff.synced.length}
            </p>
            {diff.conflicts.length > 0 && (
              <div>
                <h3>Conflicts ({diff.conflicts.length})</h3>
                <ul className="file-list">
                  {diff.conflicts.map((c) => (
                    <li key={c.file}>
                      {c.file}
                      <div className="row">
                        <button
                          onClick={() => handleResolveConflict(c.file, "local")}
                        >
                          Keep local
                        </button>
                        <button
                          onClick={() => handleResolveConflict(c.file, "remote")}
                        >
                          Keep remote
                        </button>
                      </div>
                    </li>
                  ))}
                </ul>
              </div>
            )}
            {diff.local_only.length > 0 && (
              <p>
                Upload needed ({diff.local_only.length}):{" "}
                {diff.local_only.slice(0, 8).join(", ")}
                {diff.local_only.length > 8 && "…"}
              </p>
            )}
            {diff.remote_only.length > 0 && (
              <p>
                Download needed ({diff.remote_only.length}):{" "}
                {diff.remote_only.slice(0, 8).join(", ")}
                {diff.remote_only.length > 8 && "…"}
              </p>
            )}
          </div>
        )}
      </section>

      <section className="card">
        <h2>Images in folder</h2>
        <div className="row">
          <button onClick={handleFindImages}>List images</button>
        </div>
        <ul className="file-list">
          {images.map((image) => (
            <li key={image}>{image}</li>
          ))}
        </ul>
      </section>

      {message && <p className="message">{message}</p>}
      {error && <p className="error">{error}</p>}

      <button className="logout" onClick={handleLogout}>
        Logout
      </button>
    </main>
  );
}

export default App;