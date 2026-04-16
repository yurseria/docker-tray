import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

interface Props {
  onCompose: (filePath: string) => Promise<string>;
  onClose: () => void;
}

interface ServiceStatus {
  name: string;
  status: "waiting" | "in-progress" | "done" | "error";
  detail: string;
}

function parseProgressLine(line: string): { name: string; detail: string } | null {
  // docker compose stderr patterns:
  //  " Container myapp-db-1  Creating"
  //  " Container myapp-db-1  Started"
  //  " Network myapp_default  Creating"
  const match = line.match(/(?:Container|Network|Volume|Image)\s+(\S+)\s+(.+)/i);
  if (match) {
    return { name: match[1], detail: match[2].trim() };
  }
  // Pulling lines: " db Pulling", " db Pull complete"
  const pullMatch = line.match(/^\s*(\S+)\s+(Pull.*)$/i);
  if (pullMatch) {
    return { name: pullMatch[1], detail: pullMatch[2].trim() };
  }
  return null;
}

function statusFromDetail(detail: string): ServiceStatus["status"] {
  const d = detail.toLowerCase();
  if (d.includes("started") || d.includes("running") || d.includes("created") || d.includes("complete")) return "done";
  return "in-progress";
}

export function ComposeDialog({ onCompose, onClose }: Props) {
  const [filePath, setFilePath] = useState("");
  const [loading, setLoading] = useState(false);
  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => { if (e.key === "Escape" && !loading) onClose(); };
    document.addEventListener("keydown", handleKey);
    return () => document.removeEventListener("keydown", handleKey);
  }, [onClose, loading]);
  const [error, setError] = useState<string | null>(null);
  const [services, setServices] = useState<ServiceStatus[]>([]);
  const unlistenRef = useRef<UnlistenFn | null>(null);

  useEffect(() => {
    return () => { unlistenRef.current?.(); };
  }, []);

  const handleBrowse = async () => {
    try {
      const path = await invoke<string | null>("pick_yaml_file");
      const win = getCurrentWebviewWindow();
      await win.show();
      await win.setFocus();
      if (path) setFilePath(path);
    } catch {
      const win = getCurrentWebviewWindow();
      await win.show();
      await win.setFocus();
    }
  };

  const handleSubmit = async () => {
    if (!filePath.trim()) return;
    setLoading(true);
    setError(null);
    setServices([]);

    // Listen for progress events
    unlistenRef.current = await listen<string>("compose-progress", (event) => {
      const line = event.payload;
      if (line === "done" || line === "error") return;
      const parsed = parseProgressLine(line);
      if (!parsed) return;
      setServices((prev) => {
        const idx = prev.findIndex((s) => s.name === parsed.name);
        const entry: ServiceStatus = {
          name: parsed.name,
          status: statusFromDetail(parsed.detail),
          detail: parsed.detail,
        };
        if (idx >= 0) {
          const next = [...prev];
          next[idx] = entry;
          return next;
        }
        return [...prev, entry];
      });
    });

    try {
      await onCompose(filePath.trim());
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      unlistenRef.current?.();
      unlistenRef.current = null;
      setLoading(false);
    }
  };

  return (
    <div className="confirm-overlay">
      <div className="modal-dialog" onClick={(e) => e.stopPropagation()}>
        <h3 className="modal-title">Compose Up</h3>
        <div className="modal-field">
          <label className="modal-label">docker-compose.yaml</label>
          <div className="modal-browse">
            <input
              className="modal-input"
              placeholder="/path/to/docker-compose.yaml"
              value={filePath}
              onChange={(e) => setFilePath(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleSubmit()}
              disabled={loading}
            />
            <button className="confirm-btn cancel" onClick={handleBrowse} disabled={loading}>
              Browse
            </button>
          </div>
        </div>
        {services.length > 0 && (
          <div className="compose-progress">
            {services.map((s) => (
              <div key={s.name} className={`compose-progress-item ${s.status}`}>
                <span className="compose-progress-icon">
                  {s.status === "done" ? "\u2713" : s.status === "error" ? "\u2717" : "\u25CB"}
                </span>
                <span className="compose-progress-name">{s.name}</span>
                <span className="compose-progress-detail">{s.detail}</span>
              </div>
            ))}
          </div>
        )}
        {error && <div className="modal-error">{error}</div>}
        <div className="confirm-actions">
          <button className="confirm-btn cancel" onClick={onClose} disabled={loading}>
            Cancel
          </button>
          <button
            className="confirm-btn primary"
            onClick={handleSubmit}
            disabled={loading || !filePath.trim()}
          >
            {loading ? "Running..." : "Up"}
          </button>
        </div>
      </div>
    </div>
  );
}
