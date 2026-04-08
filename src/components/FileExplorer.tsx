import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface FileEntry {
  name: string;
  is_dir: boolean;
  size: string;
  modified: string;
  permissions: string;
}

interface Props {
  containerId: string;
  containerName: string;
}

export function FileExplorer({ containerId, containerName }: Props) {
  const [currentPath, setCurrentPath] = useState("/");
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<{ name: string; content: string } | null>(null);

  const fetchFiles = useCallback(async (path: string) => {
    setLoading(true);
    setError(null);
    try {
      const entries = await invoke<FileEntry[]>("list_container_files", {
        containerId,
        path,
      });
      setFiles(entries);
      setCurrentPath(path);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [containerId]);

  useEffect(() => {
    fetchFiles("/");
  }, [fetchFiles]);

  const navigate = (name: string) => {
    const next = currentPath === "/" ? `/${name}` : `${currentPath}/${name}`;
    fetchFiles(next);
  };

  const goUp = () => {
    if (currentPath === "/") return;
    const parent = currentPath.split("/").slice(0, -1).join("/") || "/";
    fetchFiles(parent);
  };

  const goTo = (path: string) => {
    fetchFiles(path);
  };

  const openFile = async (name: string) => {
    const filePath = currentPath === "/" ? `/${name}` : `${currentPath}/${name}`;
    try {
      const content = await invoke<string>("read_container_file", {
        containerId,
        path: filePath,
      });
      setPreview({ name, content });
    } catch (e) {
      setError(`Cannot read file: ${e}`);
    }
  };

  const saveFile = async (name: string) => {
    const containerPath = currentPath === "/" ? `/${name}` : `${currentPath}/${name}`;
    const downloads = `${await getDownloadsPath()}/${name}`;
    try {
      await invoke("save_from_container", {
        containerId,
        containerPath,
        hostPath: downloads,
      });
      alert(`Saved to ${downloads}`);
    } catch (e) {
      setError(`Failed to save: ${e}`);
    }
  };

  const importFile = async () => {
    try {
      // Use Tauri's native file picker via a Rust command
      const selected = await invoke<string | null>("pick_file_for_import");
      if (!selected) return;
      await invoke("import_to_container", {
        containerId,
        hostPath: selected,
        containerPath: currentPath,
      });
      fetchFiles(currentPath);
    } catch (e) {
      setError(`Failed to import: ${e}`);
    }
  };

  // Breadcrumb segments
  const pathSegments = currentPath.split("/").filter(Boolean);

  return (
    <div className="file-explorer">
      <div className="fe-header">
        <span className="fe-title">{containerName}</span>
        <div className="fe-actions">
          <button className="log-toggle-btn" onClick={importFile} title="Import file">
            <i className="ri-upload-2-line" /> Import
          </button>
          <button className="log-toggle-btn" onClick={() => fetchFiles(currentPath)} title="Refresh">
            <i className="ri-refresh-line" />
          </button>
        </div>
      </div>

      <div className="fe-breadcrumb">
        <button className="fe-bc-btn" onClick={goUp} disabled={currentPath === "/"}>
          <i className="ri-arrow-up-line" />
        </button>
        <button className="fe-bc-seg" onClick={() => goTo("/")}>
          /
        </button>
        {pathSegments.map((seg, i) => {
          const path = "/" + pathSegments.slice(0, i + 1).join("/");
          return (
            <span key={path}>
              <button className="fe-bc-seg" onClick={() => goTo(path)}>
                {seg}
              </button>
              {i < pathSegments.length - 1 && <span className="fe-bc-sep">/</span>}
            </span>
          );
        })}
      </div>

      {error && <div className="error-banner"><i className="ri-error-warning-line" /> {error}</div>}

      {preview ? (
        <div className="fe-preview">
          <div className="fe-preview-header">
            <span className="fe-preview-name">{preview.name}</span>
            <div className="fe-preview-actions">
              <button className="log-toggle-btn" onClick={() => saveFile(preview.name)}>
                <i className="ri-download-2-line" /> Save
              </button>
              <button className="log-toggle-btn" onClick={() => setPreview(null)}>
                <i className="ri-close-line" /> Close
              </button>
            </div>
          </div>
          <pre className="fe-preview-content">{preview.content}</pre>
        </div>
      ) : (
        <div className="fe-list">
          {loading && files.length === 0 && <div className="empty">Loading...</div>}
          {!loading && files.length === 0 && <div className="empty">Empty directory</div>}
          {files.map((f) => (
            <div
              key={f.name}
              className={`fe-item ${f.is_dir ? "fe-dir" : "fe-file"}`}
              onClick={() => (f.is_dir ? navigate(f.name) : openFile(f.name))}
            >
              <i className={f.is_dir ? "ri-folder-line fe-icon" : "ri-file-line fe-icon"} />
              <span className="fe-name">{f.name}</span>
              <span className="fe-size">{f.is_dir ? "" : f.size}</span>
              <span className="fe-modified">{f.modified}</span>
              {!f.is_dir && (
                <button
                  className="action-btn"
                  title="Save to Downloads"
                  onClick={(e) => { e.stopPropagation(); saveFile(f.name); }}
                >
                  <i className="ri-download-2-line" />
                </button>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

async function getDownloadsPath(): Promise<string> {
  const home = await invoke<string | null>("get_home_dir").catch(() => null);
  return home ? `${home}/Downloads` : "/tmp";
}
