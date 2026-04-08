import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Props {
  containerId: string;
  containerName: string;
}

export function LogWindow({ containerId, containerName }: Props) {
  const [logs, setLogs] = useState<string[]>([]);
  const [autoScroll, setAutoScroll] = useState(true);
  const [lineWrap, setLineWrap] = useState(true);
  const [loading, setLoading] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);

  const fetchLogs = useCallback(async () => {
    setLoading(true);
    try {
      const lines = await invoke<string[]>("get_container_logs", {
        id: containerId,
        tail: "200",
      });
      setLogs(lines);
    } finally {
      setLoading(false);
    }
  }, [containerId]);

  useEffect(() => {
    fetchLogs();
    const interval = setInterval(fetchLogs, 5000);
    return () => clearInterval(interval);
  }, [fetchLogs]);

  useEffect(() => {
    if (autoScroll) {
      bottomRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [logs, autoScroll]);

  return (
    <div className="log-window">
      <div className="log-window-header">
        <span className="log-window-title">{containerName}</span>
        <span className="log-window-id">{containerId}</span>
        <div className="log-window-controls">
          <button
            className={`log-toggle-btn ${lineWrap ? "active" : ""}`}
            onClick={() => setLineWrap((v) => !v)}
            title="Line Wrap"
          >
            <i className="ri-text-wrap" /> Wrap
          </button>
          <button
            className={`log-toggle-btn ${autoScroll ? "active" : ""}`}
            onClick={() => setAutoScroll((v) => !v)}
            title="Auto-scroll"
          >
            <i className="ri-arrow-down-line" /> Auto
          </button>
          <button
            className="log-toggle-btn"
            onClick={fetchLogs}
            title="Refresh"
          >
            <i className={loading ? "ri-loader-4-line" : "ri-refresh-line"} />
          </button>
        </div>
      </div>
      <div className={`log-window-content ${lineWrap ? "wrap" : "nowrap"}`}>
        {logs.map((line, i) => (
          <div key={i} className="log-window-line">
            <span className="log-line-num">{i + 1}</span>
            <span className="log-line-text">{line}</span>
          </div>
        ))}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
