import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { LogWindow } from "./components/LogWindow";
import { FileExplorer } from "./components/FileExplorer";
import { applyScale, getSettings } from "./components/Settings";
import "remixicon/fonts/remixicon.css";
import "./App.css";

// Apply saved UI scale on startup
applyScale(getSettings().uiScale);

function Router() {
  const hash = window.location.hash;
  const logMatch = hash.match(/^#\/logs\/([^/]+)\/(.+)$/);
  const fileMatch = hash.match(/^#\/files\/([^/]+)\/(.+)$/);

  if (logMatch) {
    return <LogWindow containerId={logMatch[1]} containerName={decodeURIComponent(logMatch[2])} />;
  }
  if (fileMatch) {
    return <FileExplorer containerId={fileMatch[1]} containerName={decodeURIComponent(fileMatch[2])} />;
  }

  return <App />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Router />
  </React.StrictMode>,
);
