import { useEffect, useRef } from "react";

import { api } from "./lib/api";
import { attachEventListeners, useStore } from "./store";
import { Sidebar } from "./components/Sidebar";
import { Downloads } from "./screens/Downloads";
import { Gallery } from "./screens/Gallery";
import { Launcher } from "./screens/Launcher";
import { Models } from "./screens/Models";
import { Settings } from "./screens/Settings";
import { Setup } from "./screens/Setup";
import { Studio } from "./screens/Studio";

export default function App() {
  const { screen, install, settings, loading, bootstrap } = useStore();
  const autoStarted = useRef(false);

  useEffect(() => {
    let detach: (() => void) | undefined;

    void (async () => {
      detach = await attachEventListeners();
      await bootstrap();
    })();

    return () => detach?.();
  }, [bootstrap]);

  // Honour the auto-start preference exactly once per session.
  useEffect(() => {
    if (autoStarted.current || !install || !settings?.autoStart) return;

    const bat = settings.lastBat ?? install.bats[0]?.name;
    if (!bat) return;

    autoStarted.current = true;
    void api.startFooocus(bat, settings.lastPreset ?? null).catch(() => {
      // A failed auto-start is surfaced by the status event; nothing to do here.
    });
  }, [install, settings]);

  if (loading && !install) {
    return (
      <div className="setup">
        <div style={{ color: "var(--text-muted)" }}>Looking for your Fooocus installation…</div>
      </div>
    );
  }

  if (!install) return <Setup />;

  return (
    <div className="app">
      <Sidebar />
      {screen === "launcher" && <Launcher />}
      {screen === "studio" && <Studio />}
      {screen === "models" && <Models />}
      {screen === "downloads" && <Downloads />}
      {screen === "gallery" && <Gallery />}
      {screen === "settings" && <Settings />}
    </div>
  );
}
