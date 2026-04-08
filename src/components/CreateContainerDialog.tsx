import { useState } from "react";

interface Props {
  defaultImage?: string;
  onSubmit: (input: {
    name?: string;
    image: string;
    ports: { host: string; container: string }[];
    volumes: { host: string; container: string }[];
    env: string[];
    auto_start: boolean;
  }) => Promise<void>;
  onClose: () => void;
}

export function CreateContainerDialog({ defaultImage, onSubmit, onClose }: Props) {
  const [name, setName] = useState("");
  const [image, setImage] = useState(defaultImage || "");
  const [portsStr, setPortsStr] = useState("");
  const [volumesStr, setVolumesStr] = useState("");
  const [envStr, setEnvStr] = useState("");
  const [autoStart, setAutoStart] = useState(true);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async () => {
    if (!image.trim()) return;
    setLoading(true);
    setError(null);
    try {
      const ports = portsStr
        .split("\n")
        .map((l) => l.trim())
        .filter(Boolean)
        .map((l) => {
          const [host, container] = l.split(":");
          return { host: host.trim(), container: (container || host).trim() };
        });

      const volumes = volumesStr
        .split("\n")
        .map((l) => l.trim())
        .filter(Boolean)
        .map((l) => {
          const idx = l.indexOf(":");
          if (idx === -1) return { host: l, container: l };
          return { host: l.slice(0, idx), container: l.slice(idx + 1) };
        });

      const env = envStr
        .split("\n")
        .map((l) => l.trim())
        .filter(Boolean);

      await onSubmit({
        name: name.trim() || undefined,
        image: image.trim(),
        ports,
        volumes,
        env,
        auto_start: autoStart,
      });
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="confirm-overlay" onClick={onClose}>
      <div className="modal-dialog wide" onClick={(e) => e.stopPropagation()}>
        <h3 className="modal-title">Create Container</h3>

        <div className="modal-field">
          <label className="modal-label">Name (optional)</label>
          <input
            className="modal-input"
            placeholder="my-container"
            value={name}
            onChange={(e) => setName(e.target.value)}
            autoFocus
          />
        </div>

        <div className="modal-field">
          <label className="modal-label">Image</label>
          <input
            className="modal-input"
            placeholder="nginx:latest"
            value={image}
            onChange={(e) => setImage(e.target.value)}
          />
        </div>

        <div className="modal-field">
          <label className="modal-label">Ports (host:container, one per line)</label>
          <textarea
            className="modal-textarea"
            placeholder={"8080:80\n3000:3000"}
            value={portsStr}
            onChange={(e) => setPortsStr(e.target.value)}
            rows={2}
          />
        </div>

        <div className="modal-field">
          <label className="modal-label">Volumes (host:container, one per line)</label>
          <textarea
            className="modal-textarea"
            placeholder={"/host/path:/container/path"}
            value={volumesStr}
            onChange={(e) => setVolumesStr(e.target.value)}
            rows={2}
          />
        </div>

        <div className="modal-field">
          <label className="modal-label">Environment (KEY=VALUE, one per line)</label>
          <textarea
            className="modal-textarea"
            placeholder={"NODE_ENV=production"}
            value={envStr}
            onChange={(e) => setEnvStr(e.target.value)}
            rows={2}
          />
        </div>

        <label className="modal-checkbox">
          <input
            type="checkbox"
            checked={autoStart}
            onChange={(e) => setAutoStart(e.target.checked)}
          />
          Start after creation
        </label>

        {error && <div className="modal-error">{error}</div>}

        <div className="confirm-actions">
          <button className="confirm-btn cancel" onClick={onClose} disabled={loading}>
            Cancel
          </button>
          <button
            className="confirm-btn primary"
            onClick={handleSubmit}
            disabled={loading || !image.trim()}
          >
            {loading ? "Creating..." : "Create"}
          </button>
        </div>
      </div>
    </div>
  );
}
