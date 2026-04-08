import { useCallback, useEffect, useState } from "react";
import { useDocker } from "./hooks/useDocker";
import { ContainersTab } from "./components/ContainersTab";
import { ImagesTab } from "./components/ImagesTab";
import { VolumesTab } from "./components/VolumesTab";
import { NetworksTab } from "./components/NetworksTab";
import { Settings, getSettings } from "./components/Settings";
import { PullImageDialog } from "./components/PullImageDialog";
import { CreateContainerDialog } from "./components/CreateContainerDialog";
import { ComposeDialog } from "./components/ComposeDialog";
import type { Tab } from "./types";
import "./App.css";

type DialogType = "pull" | "create" | "compose" | null;

const TABS: { key: Tab; label: string; iconClass: string }[] = [
  { key: "containers", label: "Containers", iconClass: "ri-instance-line" },
  { key: "images", label: "Images", iconClass: "ri-stack-line" },
  { key: "volumes", label: "Volumes", iconClass: "ri-hard-drive-3-line" },
  { key: "networks", label: "Networks", iconClass: "ri-share-line" },
];

function App() {
  const [activeTab, setActiveTab] = useState<Tab>("containers");
  const [showSettings, setShowSettings] = useState(false);
  const [dialog, setDialog] = useState<DialogType>(null);
  const [createImage, setCreateImage] = useState<string | undefined>();

  const docker = useDocker();
  const { fetchContainers, fetchImages, fetchVolumes, fetchNetworks, ping } = docker;

  const fetchCurrentTab = useCallback(() => {
    switch (activeTab) {
      case "containers": fetchContainers(); break;
      case "images": fetchImages(); break;
      case "volumes": fetchVolumes(); break;
      case "networks": fetchNetworks(); break;
    }
  }, [activeTab, fetchContainers, fetchImages, fetchVolumes, fetchNetworks]);

  useEffect(() => {
    ping();
    fetchCurrentTab();
  }, [activeTab, ping, fetchCurrentTab]);

  useEffect(() => {
    const ms = getSettings().refreshInterval * 1000;
    const interval = setInterval(fetchCurrentTab, ms);
    return () => clearInterval(interval);
  }, [fetchCurrentTab]);

  if (!docker.connected) {
    return (
      <div className="app">
        <div className="disconnected">
          <i className="ri-server-line disconnected-icon" />
          <div className="disconnected-text">Docker not available</div>
          <div className="disconnected-hint">
            Make sure Docker Desktop is running
          </div>
          <button className="retry-btn" onClick={docker.ping}>
            <i className="ri-refresh-line" /> Retry
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="app">
      <div className="titlebar">
        <span className="titlebar-text">Docker Tray</span>
        {docker.loading && <span className="loading-indicator" />}
        <button
          className="settings-btn"
          onClick={() => setDialog("pull")}
          title="Pull Image"
        >
          <i className="ri-download-line" />
        </button>
        <button
          className="settings-btn"
          onClick={() => setDialog("create")}
          title="Create Container"
        >
          <i className="ri-add-box-line" />
        </button>
        <button
          className="settings-btn"
          onClick={() => setDialog("compose")}
          title="Compose Up"
        >
          <i className="ri-file-upload-line" />
        </button>
        <button
          className="settings-btn"
          onClick={() => setShowSettings((v) => !v)}
          title="Settings"
        >
          <i className="ri-settings-3-line" />
        </button>
      </div>

      {showSettings ? (
        <Settings onClose={() => setShowSettings(false)} />
      ) : (
        <>
          <nav className="tabs">
            {TABS.map((tab) => (
              <button
                key={tab.key}
                className={`tab ${activeTab === tab.key ? "active" : ""}`}
                onClick={() => setActiveTab(tab.key)}
              >
                <i className={`tab-icon ${tab.iconClass}`} />
                <span className="tab-label">{tab.label}</span>
              </button>
            ))}
          </nav>

          <div className="content">
            {docker.error && (
              <div className="error-banner">
                <i className="ri-error-warning-line" /> {docker.error}
              </div>
            )}

            {activeTab === "containers" && (
              <ContainersTab
                groups={docker.containers}
                onStart={docker.startContainer}
                onStop={docker.stopContainer}
                onRestart={docker.restartContainer}
                onRemove={docker.removeContainer}
              />
            )}
            {activeTab === "images" && (
              <ImagesTab
                images={docker.images}
                onRemove={docker.removeImage}
                onCreateContainer={(img) => { setCreateImage(img); setDialog("create"); }}
              />
            )}
            {activeTab === "volumes" && (
              <VolumesTab volumes={docker.volumes} onRemove={docker.removeVolume} />
            )}
            {activeTab === "networks" && (
              <NetworksTab networks={docker.networks} onRemove={docker.removeNetwork} />
            )}
          </div>
        </>
      )}
      {dialog === "pull" && (
        <PullImageDialog onPull={docker.pullImage} onClose={() => setDialog(null)} />
      )}
      {dialog === "create" && (
        <CreateContainerDialog
          defaultImage={createImage}
          onSubmit={docker.createContainer}
          onClose={() => { setDialog(null); setCreateImage(undefined); }}
        />
      )}
      {dialog === "compose" && (
        <ComposeDialog onCompose={docker.composeUp} onClose={() => setDialog(null)} />
      )}
    </div>
  );
}

export default App;
