import { useEffect, useState } from "react";
import { AlertTriangle, Check, FolderOpen, Save } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

import { api, errorMessage, events, type GpuInfo, type GpuVendor } from "../lib/api";
import { useStore } from "../store";
import { Banner, Chip, ProgressBar, ScreenHeader } from "../components/ui";

const GPU_LABELS: Record<GpuVendor, string> = {
  nvidia: "NVIDIA",
  intelArc: "Intel Arc",
  amd: "AMD",
  cpu: "CPU only",
};

export function Settings() {
  const { install, settings, chooseInstall, saveSettings, bootstrap } = useStore();

  const [config, setConfig] = useState("");
  const [original, setOriginal] = useState("");
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [gpu, setGpu] = useState<GpuInfo | null>(null);
  const [configuring, setConfiguring] = useState(false);
  const [configureMessage, setConfigureMessage] = useState("");

  useEffect(() => {
    api.detectGpu().then(setGpu).catch(() => setGpu(null));
  }, []);

  // The configuration runs in the backend and reports through the same channel
  // the installer uses.
  useEffect(() => {
    const unlisten = events.onInstall((payload) => {
      if (payload.phase !== "configuring") return;

      setConfigureMessage(payload.message);
      if (payload.progress >= 1) {
        setConfiguring(false);
        void bootstrap();
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [bootstrap]);

  async function reconfigure(vendor: GpuVendor) {
    setError(null);
    setConfiguring(true);
    setConfigureMessage("Starting…");
    try {
      await api.configureGpu(vendor);
    } catch (err) {
      setError(errorMessage(err));
      setConfiguring(false);
    }
  }

  useEffect(() => {
    if (!install) return;
    api
      .readFooocusConfig()
      .then((text) => {
        setConfig(text);
        setOriginal(text);
      })
      .catch((err) => setError(errorMessage(err)));
  }, [install]);

  async function pickFolder() {
    const chosen = await open({ directory: true, title: "Select your Fooocus folder" });
    if (typeof chosen === "string") await chooseInstall(chosen);
  }

  async function saveConfig() {
    setError(null);
    try {
      await api.writeFooocusConfig(config);
      setOriginal(config);
      setSaved(true);
      setTimeout(() => setSaved(false), 2500);
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  const dirty = config !== original;

  return (
    <div className="screen">
      <ScreenHeader title="Settings" subtitle="Installation, startup behaviour, and Fooocus paths" />

      <div className="screen-body" style={{ maxWidth: 780 }}>
        {error && (
          <div style={{ marginBottom: 16 }}>
            <Banner tone="danger" icon={<AlertTriangle size={15} />}>
              {error}
            </Banner>
          </div>
        )}

        <section className="section">
          <h2 className="section-title">Installation</h2>
          <p className="section-hint">
            The folder holding your <span className="mono">.bat</span> files and{" "}
            <span className="mono">python_embeded</span>.
          </p>

          <div className="card">
            <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div className="mono truncate">{install?.root ?? "Not selected"}</div>
                {install && (
                  <div style={{ display: "flex", gap: 6, marginTop: 8 }}>
                    {install.version && <Chip tone="accent">Fooocus {install.version}</Chip>}
                    <Chip>{install.bats.length} launch profiles</Chip>
                    <Chip>{install.presets.length} presets</Chip>
                  </div>
                )}
              </div>
              <button className="btn" onClick={() => void pickFolder()}>
                <FolderOpen size={15} />
                Change
              </button>
            </div>
          </div>
        </section>

        <section className="section">
          <h2 className="section-title">Graphics card</h2>
          <p className="section-hint">
            Determines which packages Fooocus uses and which flags it launches with. Re-running
            this replaces the graphics packages inside your Fooocus folder, so only do it if
            generation is failing or you have changed card.
          </p>

          <div className="card">
            <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 12 }}>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 13 }}>{gpu?.name ?? "Detecting…"}</div>
                <div style={{ display: "flex", gap: 6, marginTop: 8, flexWrap: "wrap" }}>
                  {settings?.gpuVendor ? (
                    <Chip tone="accent">Configured for {GPU_LABELS[settings.gpuVendor]}</Chip>
                  ) : (
                    <Chip tone="warning">Not configured by this app</Chip>
                  )}
                  {gpu && <Chip>Detected {GPU_LABELS[gpu.vendor]}</Chip>}
                </div>
              </div>
            </div>

            <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
              {(["nvidia", "intelArc", "amd", "cpu"] as GpuVendor[]).map((id) => (
                <button
                  key={id}
                  className={`btn btn-sm${settings?.gpuVendor === id ? " btn-primary" : ""}`}
                  onClick={() => void reconfigure(id)}
                  disabled={configuring}
                >
                  {GPU_LABELS[id]}
                </button>
              ))}
            </div>

            {configuring && (
              <div style={{ marginTop: 12 }}>
                <ProgressBar value={0} indeterminate />
                <div className="mono truncate" style={{ marginTop: 8, fontSize: 11.5 }}>
                  {configureMessage}
                </div>
              </div>
            )}
          </div>
        </section>

        <section className="section">
          <h2 className="section-title">Behaviour</h2>

          <div className="card" style={{ display: "flex", flexDirection: "column", gap: 16 }}>
            <label className="switch">
              <input
                type="checkbox"
                checked={settings?.autoStart ?? false}
                onChange={(event) => void saveSettings({ autoStart: event.target.checked })}
              />
              <span>
                <span className="field-label" style={{ display: "block" }}>
                  Start Fooocus when this app opens
                </span>
                <span className="field-hint">
                  Uses your last launch profile. Loading models takes a minute or two, so this gets
                  it out of the way.
                </span>
              </span>
            </label>

            <label className="switch">
              <input
                type="checkbox"
                checked={settings?.stopOnExit ?? true}
                onChange={(event) => void saveSettings({ stopOnExit: event.target.checked })}
              />
              <span>
                <span className="field-label" style={{ display: "block" }}>
                  Stop Fooocus when this app closes
                </span>
                <span className="field-hint">
                  Recommended. Because Fooocus runs hidden, leaving it running would hold your
                  graphics memory with nothing visible to close.
                </span>
              </span>
            </label>
          </div>
        </section>

        <section className="section">
          <div
            style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}
          >
            <h2 className="section-title">Fooocus paths</h2>
            <div style={{ display: "flex", gap: 8 }}>
              {install && (
                <button
                  className="btn btn-sm"
                  onClick={() => void revealItemInDir(`${install.fooocusDir}\\config.txt`)}
                >
                  <FolderOpen size={14} />
                  Show file
                </button>
              )}
              <button className="btn btn-primary btn-sm" onClick={saveConfig} disabled={!dirty}>
                {saved ? <Check size={14} /> : <Save size={14} />}
                {saved ? "Saved" : "Save"}
              </button>
            </div>
          </div>
          <p className="section-hint">
            This is your Fooocus <span className="mono">config.txt</span>. Change these to keep
            models on another drive. It must stay valid JSON — saving is blocked otherwise, so a
            typo cannot stop Fooocus from starting.
          </p>

          <textarea
            className="textarea"
            rows={16}
            spellCheck={false}
            value={config}
            onChange={(event) => setConfig(event.target.value)}
          />
          {dirty && (
            <p className="field-hint" style={{ marginTop: 8 }}>
              Restart Fooocus for path changes to take effect.
            </p>
          )}
        </section>
      </div>
    </div>
  );
}
