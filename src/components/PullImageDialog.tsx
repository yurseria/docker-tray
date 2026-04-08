import { useState } from "react";

interface Props {
  onPull: (image: string) => Promise<void>;
  onClose: () => void;
}

export function PullImageDialog({ onPull, onClose }: Props) {
  const [image, setImage] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async () => {
    if (!image.trim()) return;
    setLoading(true);
    setError(null);
    try {
      await onPull(image.trim());
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="confirm-overlay" onClick={onClose}>
      <div className="modal-dialog" onClick={(e) => e.stopPropagation()}>
        <h3 className="modal-title">Pull Image</h3>
        <div className="modal-field">
          <label className="modal-label">Image</label>
          <input
            className="modal-input"
            placeholder="nginx:latest"
            value={image}
            onChange={(e) => setImage(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleSubmit()}
            autoFocus
          />
        </div>
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
            {loading ? "Pulling..." : "Pull"}
          </button>
        </div>
      </div>
    </div>
  );
}
