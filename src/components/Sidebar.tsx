import {
  Download,
  Images,
  LayoutGrid,
  Package,
  Settings as SettingsIcon,
  Aperture,
  Wand2,
} from "lucide-react";

import { activeJobCount, useStore, type ScreenId } from "../store";
import { ProgressBar } from "./ui";

const NAV: { id: ScreenId; label: string; icon: typeof LayoutGrid }[] = [
  { id: "launcher", label: "Launcher", icon: LayoutGrid },
  { id: "studio", label: "Studio", icon: Wand2 },
  { id: "models", label: "Models", icon: Package },
  { id: "downloads", label: "Downloads", icon: Download },
  { id: "gallery", label: "Gallery", icon: Images },
  { id: "settings", label: "Settings", icon: SettingsIcon },
];

const STATE_LABEL: Record<string, string> = {
  stopped: "Stopped",
  starting: "Starting",
  ready: "Running",
  stopping: "Stopping",
  crashed: "Stopped unexpectedly",
};

export function Sidebar() {
  const { screen, setScreen, install, status, jobs, generating, genPercent, genStage } =
    useStore();
  const active = activeJobCount(jobs);

  return (
    <nav className="sidebar">
      <div className="brand">
        <div className="brand-mark">
          <Aperture size={17} />
        </div>
        <div style={{ minWidth: 0 }}>
          <div className="brand-name">Fooocus</div>
          <div className="brand-version truncate">
            {install?.version ? `v${install.version}` : "Not configured"}
          </div>
        </div>
      </div>

      {NAV.map(({ id, label, icon: Icon }) => (
        <button
          key={id}
          className={`nav-item${screen === id ? " active" : ""}`}
          onClick={() => setScreen(id)}
        >
          <Icon size={17} strokeWidth={2} />
          {label}
          {id === "downloads" && active > 0 && <span className="nav-badge">{active}</span>}
        </button>
      ))}

      <div className="nav-spacer" />

      {/* Visible from every screen, so leaving the Studio never hides a job. */}
      {generating && (
        <button className="gen-pill" onClick={() => setScreen("studio")}>
          <div className="gen-pill-head">
            <Aperture size={13} />
            <span className="truncate">{genStage || "Generating"}</span>
            <span className="gen-pill-percent">{Math.round(genPercent * 100)}%</span>
          </div>
          <ProgressBar value={genPercent} indeterminate={genPercent === 0} />
        </button>
      )}

      <button className="run-pill" onClick={() => setScreen("launcher")}>
        <span className={`run-dot ${status.state}`} />
        <span className="run-pill-text" style={{ textAlign: "left" }}>
          <span className="run-pill-state">{STATE_LABEL[status.state] ?? status.state}</span>
          <span className="run-pill-stage" style={{ display: "block" }}>
            {status.stage}
          </span>
        </span>
      </button>
    </nav>
  );
}
