import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

interface Props {
  onCompose: (filePath: string) => Promise<string>;
  onClose: () => void;
}

export function ComposeDialog({ onCompose, onClose }: Props) {
  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("keydown", handleKey);
    return () => document.removeEventListener("keydown", handleKey);
  }, [onClose]);
  const [filePath, setFilePath] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleBrowse = async () => {
    try {
      const path = await invoke<string | null>("pick_yaml_file");
      // Re-show window after osascript file picker steals focus
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
    try {
      await onCompose(filePath.trim());
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
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
            />
            <button className="confirm-btn cancel" onClick={handleBrowse}>
              Browse
            </button>
          </div>
        </div>
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
