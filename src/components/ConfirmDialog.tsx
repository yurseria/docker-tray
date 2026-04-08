interface Props {
  message: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({ message, onConfirm, onCancel }: Props) {
  return (
    <div className="confirm-overlay" onClick={onCancel}>
      <div className="confirm-dialog" onClick={(e) => e.stopPropagation()}>
        <p className="confirm-message">{message}</p>
        <div className="confirm-actions">
          <button className="confirm-btn cancel" onClick={onCancel}>
            Cancel
          </button>
          <button className="confirm-btn danger" onClick={onConfirm}>
            Remove
          </button>
        </div>
      </div>
    </div>
  );
}
