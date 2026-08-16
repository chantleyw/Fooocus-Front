import { useEffect, useRef, useState } from "react";
import { AlertTriangle, Eraser, Play, RefreshCw, Square, Wand2 } from "lucide-react";

import { api, errorMessage } from "../lib/api";
import { useStore } from "../store";
import { Banner, Chip, ProgressBar, ScreenHeader } from "../components/ui";

export function Launcher() {
  const {
    install,
    status,
    logs,
    clearLogs,
    setScreen,
    selectedBat,
    selectedPreset,
    setSelectedBat,
    setSelectedPreset,
  } = useStore();

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const logRef = useRef<HTMLDivElement>(null);

  const bat = selectedBat ?? "";
  const preset = selectedPreset ?? "";

  // Keep the log pinned to the newest line while it streams.
  useEffect(() => {
    const el = logRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [logs]);

  if (!install) return null;

  const running = status.state === "starting" || status.state === "ready";
  const selected = install.bats.find((b) => b.name === bat);

  async function start() {
    setBusy(true);
    setError(null);
    try {
      clearLogs();
      await api.startFooocus(bat, preset || null);
      // Follow the action: the Studio shows startup progress and then becomes
      // the app itself once Fooocus is ready.
      setScreen("studio");
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  async function stop() {
    setBusy(true);
    try {
      await api.stopFooocus();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="screen">
      <ScreenHeader
        title="Launcher"
        subtitle="Choose how Fooocus starts. It runs hidden in the background — everything appears here."
        actions={
          running ? (
            <>
              {status.state === "ready" && (
                <button className="btn" onClick={() => setScreen("studio")}>
                  <Wand2 size={15} />
                  Open Studio
                </button>
              )}
              <button className="btn btn-danger" onClick={stop} disabled={busy}>
                <Square size={15} />
                Stop
              </button>
            </>
          ) : (
            <button className="btn btn-primary btn-lg" onClick={start} disabled={busy || !bat}>
              <Play size={16} />
              Start Fooocus
            </button>
          )
        }
      />

      <div className="screen-body">
        {error && (
          <div style={{ marginBottom: 16 }}>
            <Banner tone="danger" icon={<AlertTriangle size={15} />}>
              {error}
            </Banner>
          </div>
        )}

        <div className="launcher">
          <section>
            <h2 className="section-title">Launch profile</h2>
            <p className="section-hint">
              Each profile comes from a <span className="mono">.bat</span> file in your Fooocus
              folder. The flags it carries are passed through exactly as written.
            </p>

            <div className="profile-list">
              {install.bats.map((profile) => (
                <button
                  key={profile.name}
                  className={`profile${bat === profile.name ? " selected" : ""}`}
                  onClick={() => setSelectedBat(profile.name)}
                  disabled={running}
                >
                  <span className="profile-radio" />
                  <span style={{ minWidth: 0 }}>
                    <span className="profile-name">
                      {profile.label}
                      <Chip>{profile.name}</Chip>
                      {profile.autoUpdates && <Chip tone="warning">Auto-updates</Chip>}
                    </span>
                    <span className="profile-desc" style={{ display: "block" }}>
                      {profile.description}
                    </span>
                    <span className="profile-args mono" style={{ display: "block" }}>
                      {profile.args.join(" ")}
                    </span>
                  </span>
                </button>
              ))}
            </div>

            <div className="field" style={{ marginTop: 18, maxWidth: 320 }}>
              <label className="field-label" htmlFor="preset">
                Preset
              </label>
              <select
                id="preset"
                className="select"
                value={preset}
                onChange={(event) => setSelectedPreset(event.target.value)}
                disabled={running}
              >
                <option value="">Use the profile's own preset</option>
                {install.presets.map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </select>
              <span className="field-hint">
                Presets pick the checkpoint, LoRAs and default settings. Choosing one here overrides
                whatever the launch profile sets.
              </span>
            </div>
          </section>

          <aside className="card status-card">
            <div className="status-heading">
              <span className={`run-dot ${status.state}`} />
              <h2 className="section-title" style={{ margin: 0 }}>
                {status.state === "ready"
                  ? "Fooocus is running"
                  : status.state === "starting"
                    ? "Starting up"
                    : status.state === "crashed"
                      ? "Stopped unexpectedly"
                      : "Not running"}
              </h2>
            </div>

            {status.state === "crashed" && (
              <Banner tone="danger" icon={<AlertTriangle size={15} />}>
                Fooocus exited on its own
                {status.exitCode !== null && ` with code ${status.exitCode}`}. The log below usually
                says why.
              </Banner>
            )}

            {running && (
              <>
                <div className="status-stage">{status.stage}</div>
                <ProgressBar
                  value={status.progress}
                  indeterminate={status.state === "starting" && status.progress === 0}
                />
                <div className="status-meta">
                  <span>{Math.round(status.progress * 100)}%</span>
                  {status.port && <span className="mono">127.0.0.1:{status.port}</span>}
                </div>

                {/* Long stages download gigabytes. Without this the bar looks
                    frozen for as long as that takes. */}
                {status.detail && (
                  <div className="status-detail truncate" title={status.detail}>
                    {status.detail}
                  </div>
                )}
              </>
            )}

            {!running && status.state !== "crashed" && (
              <p className="field-hint" style={{ marginTop: 4 }}>
                {selected
                  ? `Ready to start with ${selected.label}. No console window will appear.`
                  : "Select a launch profile to begin."}
              </p>
            )}

            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                marginTop: 18,
              }}
            >
              <h3 className="section-title" style={{ margin: 0 }}>
                Output
              </h3>
              <button className="btn btn-ghost btn-sm" onClick={clearLogs} disabled={!logs.length}>
                <Eraser size={13} />
                Clear
              </button>
            </div>

            <div className="log" ref={logRef}>
              {logs.length === 0 ? (
                <span style={{ color: "var(--text-faint)" }}>
                  Console output from Fooocus appears here, including any models it downloads on
                  first run.
                </span>
              ) : (
                logs.map((line) => (
                  <div key={line.id} className={`log-line ${line.stream}`}>
                    {line.text}
                  </div>
                ))
              )}
            </div>
          </aside>
        </div>
      </div>
    </div>
  );
}

/** Small helper used by the Studio screen's empty state. */
export function StartButton() {
  const { install, selectedBat, selectedPreset } = useStore();
  const [busy, setBusy] = useState(false);

  if (!install) return null;
  const bat = selectedBat ?? install.bats[0]?.name;
  if (!bat) return null;

  return (
    <button
      className="btn btn-primary"
      disabled={busy}
      onClick={async () => {
        setBusy(true);
        try {
          await api.startFooocus(bat, selectedPreset || null);
        } finally {
          setBusy(false);
        }
      }}
    >
      {busy ? <RefreshCw size={15} className="spin" /> : <Play size={15} />}
      Start Fooocus
    </button>
  );
}
