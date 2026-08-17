import { useEffect, useState } from "react";
import {
  AlertTriangle,
  Aperture,
  ArrowLeft,
  Check,
  Download,
  FolderOpen,
  HardDrive,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";

import {
  api,
  errorMessage,
  events,
  type GpuInfo,
  type GpuVendor,
  type InstallProgress,
  type ReleasePackage,
} from "../lib/api";

import { formatBytes, formatEta, formatSpeed } from "../lib/format";
import { useStore } from "../store";
import { Banner, Chip, ProgressBar } from "../components/ui";

/** What each graphics stack means for the install, in plain language. */
const GPUS: { id: GpuVendor; label: string; detail: string; untested?: boolean }[] = [
  {
    id: "nvidia",
    label: "NVIDIA",
    detail: "Ready to go. The standard package already includes what your card needs.",
  },
  {
    id: "intelArc",
    label: "Intel Arc",
    detail:
      "Needs Intel's own graphics packages, which Fooocus does not document. The setup installs them for you — about 2 GB extra.",
  },
  {
    id: "amd",
    label: "AMD",
    detail:
      "Needs DirectML in place of the default packages, following the official Fooocus instructions. Roughly 3x slower than an equivalent NVIDIA card.",
    untested: true,
  },
  {
    id: "cpu",
    label: "Integrated graphics or none",
    detail:
      "Also the right answer for integrated graphics, which share system memory. Works, but expect several minutes per image rather than seconds.",
  },
];

type Mode = "choose" | "install";

/** Shown on first run, or whenever the saved installation cannot be found. */
export function Setup() {
  const [mode, setMode] = useState<Mode>("choose");

  return (
    <div className="setup">
      {mode === "choose" ? (
        <ChooseRoute onInstall={() => setMode("install")} />
      ) : (
        <InstallWizard onBack={() => setMode("choose")} />
      )}
    </div>
  );
}

// ------------------------------------------------------------------ route picker

function ChooseRoute({ onInstall }: { onInstall: () => void }) {
  const { chooseInstall, error } = useStore();
  const [busy, setBusy] = useState(false);

  async function pickExisting() {
    const chosen = await open({ directory: true, title: "Select your Fooocus folder" });
    if (typeof chosen !== "string") return;

    setBusy(true);
    try {
      await chooseInstall(chosen);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="card setup-card">
      <div className="setup-mark">
        <Aperture size={26} />
      </div>

      <h1 className="screen-title">Welcome</h1>
      <p className="screen-subtitle">
        This app runs Fooocus for you — hidden, with everything on one screen. First, let's find it.
      </p>

      <div className="route-list">
        <button className="route" onClick={onInstall}>
          <span className="route-icon accent">
            <Download size={19} />
          </span>
          <span>
            <span className="route-title">Install Fooocus for me</span>
            <span className="route-desc">
              Downloads the official Windows package and sets it up. No git, no Python, nothing to
              configure — takes about 2 GB.
            </span>
          </span>
        </button>

        <button className="route" onClick={() => void pickExisting()} disabled={busy}>
          <span className="route-icon">
            <FolderOpen size={19} />
          </span>
          <span>
            <span className="route-title">I already have Fooocus</span>
            <span className="route-desc">
              Point at the folder holding your <span className="mono">run.bat</span> files. Nothing
              in it gets modified.
            </span>
          </span>
        </button>
      </div>

      {error && (
        <div style={{ marginTop: 14, textAlign: "left" }}>
          <Banner tone="danger">{error}</Banner>
        </div>
      )}
    </div>
  );
}

// --------------------------------------------------------------- install wizard

function InstallWizard({ onBack }: { onBack: () => void }) {
  const { chooseInstall, setJustInstalled, setScreen } = useStore();

  const [pkg, setPkg] = useState<ReleasePackage | null>(null);
  const [dest, setDest] = useState("");
  const [free, setFree] = useState<number | null>(null);
  const [progress, setProgress] = useState<InstallProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const [gpu, setGpu] = useState<GpuInfo | null>(null);
  const [vendor, setVendor] = useState<GpuVendor | null>(null);

  // Resolve the package, a default location and the graphics card in parallel.
  useEffect(() => {
    void (async () => {
      try {
        const [found, suggested, detected] = await Promise.all([
          api.findFooocusPackage(),
          api.suggestInstallLocation(),
          api.detectGpu(),
        ]);
        setPkg(found);
        setDest(suggested);
        setGpu(detected);
        setVendor(detected.vendor);
      } catch (err) {
        setError(errorMessage(err));
      }
    })();
  }, []);

  // Keep the free-space readout in step with the chosen folder.
  useEffect(() => {
    if (!dest) return;
    api.checkFreeSpace(dest).then(setFree).catch(() => setFree(null));
  }, [dest]);

  useEffect(() => {
    const unlisten = events.onInstall(async (payload) => {
      setProgress(payload);
      if (payload.phase === "complete" && payload.installRoot) {
        await chooseInstall(payload.installRoot);
        // Fooocus is installed but has no models yet. Send the user somewhere
        // that says so, rather than letting the first Generate kick off a
        // silent multi-gigabyte download that looks like a hang.
        setJustInstalled(true);
        setScreen("models");
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [chooseInstall, setJustInstalled, setScreen]);

  async function pickFolder() {
    const chosen = await open({ directory: true, title: "Where should Fooocus be installed?" });
    if (typeof chosen === "string") setDest(chosen);
  }

  async function start() {
    if (!pkg || !dest || !vendor) return;
    setError(null);
    try {
      await api.installFooocus(pkg, dest, vendor);
      setProgress({
        phase: "downloading",
        progress: 0,
        bytes: 0,
        total: pkg.size,
        speed: 0,
        message: "Starting download",
        error: null,
        installRoot: null,
      });
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  const running =
    progress !== null &&
    ["downloading", "extracting", "configuring", "finalizing"].includes(progress.phase);
  const notEnoughSpace = pkg !== null && free !== null && free < pkg.requiredSpace;

  // ------------------------------------------------------------ running state

  if (running) {
    return (
      <div className="card setup-card">
        <div className="setup-mark">
          <Download size={26} />
        </div>

        <h1 className="screen-title">{progress.message}</h1>
        <p className="screen-subtitle">
          {progress.phase === "downloading"
            ? "You can leave this running. An interrupted download resumes where it stopped."
            : progress.phase === "extracting"
              ? "Unpacking the archive. This part is quick but disk-heavy."
              : progress.phase === "configuring"
                ? "Installing the graphics packages your card needs. This is the fiddly part you would otherwise do by hand."
                : "Almost there."}
        </p>

        <div style={{ marginTop: 22 }}>
          <ProgressBar
            value={progress.progress}
            indeterminate={progress.phase !== "downloading"}
          />
          <div className="status-meta">
            {progress.phase === "configuring" ? (
              <span className="mono truncate" style={{ maxWidth: "100%" }}>
                {progress.message}
              </span>
            ) : progress.phase === "downloading" && progress.total ? (
              <>
                <span>
                  {formatBytes(progress.bytes)} of {formatBytes(progress.total)} ·{" "}
                  {Math.round(progress.progress * 100)}%
                </span>
                <span>
                  {formatSpeed(progress.speed)} ·{" "}
                  {formatEta(progress.bytes, progress.total, progress.speed)} left
                </span>
              </>
            ) : (
              <span>{formatBytes(progress.bytes)} written</span>
            )}
          </div>
        </div>

        <button
          className="btn"
          style={{ marginTop: 20, width: "100%" }}
          onClick={() => void api.cancelInstall()}
        >
          Cancel
        </button>
      </div>
    );
  }

  // ------------------------------------------------------------ terminal states

  if (progress?.phase === "failed" || progress?.phase === "cancelled") {
    const failed = progress.phase === "failed";
    return (
      <div className="card setup-card">
        <div className="setup-mark" style={{ background: failed ? "var(--danger)" : undefined }}>
          <AlertTriangle size={26} />
        </div>

        <h1 className="screen-title">{failed ? "Installation failed" : "Installation cancelled"}</h1>
        <p className="screen-subtitle">
          {failed
            ? "Nothing was left in a broken state."
            : "Any partly downloaded file was kept, so starting again resumes rather than restarting."}
        </p>

        {progress.error && (
          <div style={{ marginTop: 14, textAlign: "left" }}>
            <Banner tone="danger">{progress.error}</Banner>
          </div>
        )}

        <div style={{ display: "flex", gap: 8, marginTop: 20 }}>
          <button className="btn" style={{ flex: 1 }} onClick={onBack}>
            <ArrowLeft size={15} />
            Back
          </button>
          <button className="btn btn-primary" style={{ flex: 1 }} onClick={() => void start()}>
            Try again
          </button>
        </div>
      </div>
    );
  }

  // ------------------------------------------------------------- configuration

  return (
    <div className="card setup-card">
      <div className="setup-mark">
        <Download size={26} />
      </div>

      <h1 className="screen-title">Install Fooocus</h1>
      <p className="screen-subtitle">
        {pkg
          ? `Version ${pkg.version}, the official Windows package — ${formatBytes(pkg.size)} to download.`
          : "Looking up the latest release…"}
      </p>

      <div className="field" style={{ marginTop: 20, textAlign: "left" }}>
        <label className="field-label">Graphics card</label>
        {gpu && (
          <span className="field-hint">
            {gpu.adapters.length > 0
              ? `Detected: ${gpu.name}`
              : "Could not detect a graphics card — please choose below."}
            {gpu.note && ` ${gpu.note}`}
          </span>
        )}
        <div className="gpu-list">
          {GPUS.map((entry) => (
            <button
              key={entry.id}
              className={`gpu-option${vendor === entry.id ? " selected" : ""}`}
              onClick={() => setVendor(entry.id)}
            >
              <span className="profile-radio" />
              <span style={{ minWidth: 0 }}>
                <span className="profile-name">
                  {entry.label}
                  {gpu?.vendor === entry.id && <Chip tone="accent">Detected</Chip>}
                  {entry.untested && <Chip tone="warning">Untested</Chip>}
                </span>
                <span className="profile-desc" style={{ display: "block" }}>
                  {entry.detail}
                </span>
              </span>
            </button>
          ))}
        </div>
      </div>

      <div className="field" style={{ marginTop: 16, textAlign: "left" }}>
        <label className="field-label">Install location</label>
        <div style={{ display: "flex", gap: 8 }}>
          <input
            className="input mono"
            value={dest}
            onChange={(event) => setDest(event.target.value)}
            spellCheck={false}
          />
          <button className="btn" onClick={() => void pickFolder()}>
            <FolderOpen size={15} />
          </button>
        </div>
        {free !== null && pkg && (
          <span className={`field-hint${notEnoughSpace ? " danger" : ""}`}>
            <HardDrive size={12} style={{ verticalAlign: -2, marginRight: 4 }} />
            {formatBytes(free)} free · about {formatBytes(pkg.requiredSpace)} needed for the
            download and extracted files
          </span>
        )}
      </div>

      <ul className="setup-steps">
        <li>
          <Check size={14} style={{ flexShrink: 0, marginTop: 3 }} />
          <span>
            Includes its own Python and every dependency. Git, Python and pip are not needed.
          </span>
        </li>
        <li>
          <Check size={14} style={{ flexShrink: 0, marginTop: 3 }} />
          <span>
            The 2 GB archive is deleted once extracted, so it does not sit on your disk.
          </span>
        </li>
        <li>
          <Check size={14} style={{ flexShrink: 0, marginTop: 3 }} />
          <span>
            Models are separate. Once this finishes you can pick exactly which ones you want from
            the Models tab, or let the first launch fetch the defaults.
          </span>
        </li>
      </ul>

      {pkg?.fallback && (
        <div style={{ marginTop: 14, textAlign: "left" }}>
          <Banner tone="info">
            GitHub's release listing could not be reached, so the last known-good package is being
            used. This is fine — it is the same file the Fooocus README links to.
          </Banner>
        </div>
      )}

      {GPUS.find((entry) => entry.id === vendor)?.untested && (
        <div style={{ marginTop: 14, textAlign: "left" }}>
          <Banner tone="warning" icon={<AlertTriangle size={15} />}>
            This path follows the official Fooocus instructions for AMD cards, but nobody has run
            it yet — the app was developed on Intel Arc. It should work. If it does not, Settings
            has a <strong>Restore pinned versions</strong> button that undoes package changes, and
            you can switch to CPU there too. Please do report back either way.
          </Banner>
        </div>
      )}

      {notEnoughSpace && (
        <div style={{ marginTop: 14, textAlign: "left" }}>
          <Banner tone="warning" icon={<AlertTriangle size={15} />}>
            There may not be enough space on that drive. Choose somewhere roomier, or free some up.
          </Banner>
        </div>
      )}

      {error && (
        <div style={{ marginTop: 14, textAlign: "left" }}>
          <Banner tone="danger">{error}</Banner>
        </div>
      )}

      <div style={{ display: "flex", gap: 8, marginTop: 20 }}>
        <button className="btn" onClick={onBack}>
          <ArrowLeft size={15} />
          Back
        </button>
        <button
          className="btn btn-primary btn-lg"
          style={{ flex: 1 }}
          onClick={() => void start()}
          disabled={!pkg || !dest || !vendor}
        >
          <Download size={16} />
          Install Fooocus
        </button>
      </div>
    </div>
  );
}
