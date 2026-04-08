import { useEffect, useRef, useState } from "react";
import { ConfirmDialog } from "./ConfirmDialog";

export interface MenuItem {
  label: string;
  danger?: boolean;
  confirm?: string;
  onClick: () => void;
}

interface Props {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
}

export function ContextMenu({ x, y, items, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [confirmItem, setConfirmItem] = useState<MenuItem | null>(null);

  useEffect(() => {
    const handleClick = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        onClose();
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [onClose]);

  if (confirmItem) {
    return (
      <ConfirmDialog
        message={confirmItem.confirm!}
        onConfirm={() => {
          confirmItem.onClick();
          onClose();
        }}
        onCancel={onClose}
      />
    );
  }

  return (
    <div ref={ref} className="context-menu" style={{ top: y, left: x }}>
      {items.map((item, i) => (
        <button
          key={i}
          className={`context-menu-item ${item.danger ? "danger" : ""}`}
          onClick={() => {
            if (item.confirm) {
              setConfirmItem(item);
              return;
            }
            item.onClick();
            onClose();
          }}
        >
          {item.label}
        </button>
      ))}
    </div>
  );
}
