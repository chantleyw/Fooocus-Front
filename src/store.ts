import { create } from "zustand";

import {
  api,
  errorMessage,
  events,
  type CatalogEntry,
  type GalleryImage,
  type InstallInfo,
  type Job,
  type ModelCategory,
  type Settings,
  type StatusPayload,
} from "./lib/api";

/** How many log lines we keep. Enough to diagnose a failed start, bounded so a
 *  chatty download progress bar cannot grow the DOM without limit. */
const LOG_LIMIT = 500;

export type ScreenId = "launcher" | "studio" | "models" | "downloads" | "gallery" | "settings";

interface LogLine {
  id: number;
  text: string;
  stream: "stdout" | "stderr";
}

interface AppStore {
  screen: ScreenId;
  install: InstallInfo | null;
  settings: Settings | null;
  status: StatusPayload;
  logs: LogLine[];
  models: ModelCategory[];
  catalog: CatalogEntry[];
  jobs: Job[];
  images: GalleryImage[];
  loading: boolean;
  error: string | null;

  /** Launch selection. Lives here rather than in the Launcher component so it
   *  survives navigating away and back. */
  selectedBat: string | null;
  selectedPreset: string | null;

  /** True just after a from-scratch install, so the Models screen can prompt
   *  for the essentials before the user hits a silent multi-gigabyte fetch. */
  justInstalled: boolean;

  /** Live generation state. Also kept here so switching screens mid-render
   *  does not lose the progress bar and preview of a job still running. */
  generating: boolean;
  genPercent: number;
  genStage: string;
  genPreview: string | null;
  genResults: string[];
  genError: string | null;

  setScreen: (screen: ScreenId) => void;
  setError: (error: string | null) => void;
  setSelectedBat: (bat: string) => void;
  setSelectedPreset: (preset: string) => void;

  setJustInstalled: (value: boolean) => void;
  downloadEssentials: () => Promise<number>;
  beginGeneration: () => void;
  failGeneration: (message: string) => void;

  bootstrap: () => Promise<void>;
  chooseInstall: (path: string) => Promise<void>;
  refreshModels: () => Promise<void>;
  refreshGallery: () => Promise<void>;
  saveSettings: (patch: Partial<Settings>) => Promise<void>;
  clearLogs: () => void;
}

const IDLE_STATUS: StatusPayload = {
  state: "stopped",
  port: null,
  url: null,
  progress: 0,
  stage: "Not running",
  exitCode: null,
};

let logCounter = 0;

export const useStore = create<AppStore>((set, get) => ({
  screen: "launcher",
  install: null,
  settings: null,
  status: IDLE_STATUS,
  logs: [],
  models: [],
  catalog: [],
  jobs: [],
  images: [],
  loading: true,
  error: null,
  selectedBat: null,
  selectedPreset: null,
  justInstalled: false,
  generating: false,
  genPercent: 0,
  genStage: "",
  genPreview: null,
  genResults: [],
  genError: null,

  setJustInstalled: (value) => set({ justInstalled: value }),

  /** Queue every essential model that is not already on disk. */
  downloadEssentials: async () => {
    const missing = get().catalog.filter((entry) => entry.essential && !entry.installed);
    for (const entry of missing) {
      try {
        await api.startDownload(entry.id);
      } catch (error) {
        set({ error: errorMessage(error) });
      }
    }
    return missing.length;
  },

  beginGeneration: () =>
    set({
      generating: true,
      genPercent: 0,
      genStage: "Queued",
      genPreview: null,
      genError: null,
    }),

  failGeneration: (message) => set({ generating: false, genError: message }),

  setScreen: (screen) => set({ screen }),
  setError: (error) => set({ error }),
  clearLogs: () => set({ logs: [] }),

  setSelectedBat: (bat) => {
    set({ selectedBat: bat });
    void get().saveSettings({ lastBat: bat });
  },

  setSelectedPreset: (preset) => {
    set({ selectedPreset: preset });
    void get().saveSettings({ lastPreset: preset || null });
  },

  /** Load everything the shell needs, then wire up the live event streams. */
  bootstrap: async () => {
    set({ loading: true });
    try {
      const [install, settings, status, jobs] = await Promise.all([
        api.getInstall(),
        api.getSettings(),
        api.getStatus(),
        api.getDownloads(),
      ]);
      // Restore the remembered launch profile, falling back to one that
      // actually exists in this install.
      const bat =
        install?.bats.find((b) => b.name === settings.lastBat)?.name ??
        install?.bats[0]?.name ??
        null;

      set({
        install,
        settings,
        status,
        jobs,
        error: null,
        selectedBat: bat,
        selectedPreset: settings.lastPreset ?? "",
      });

      if (install) {
        await Promise.all([get().refreshModels(), get().refreshGallery()]);
      }
    } catch (error) {
      set({ error: errorMessage(error) });
    } finally {
      set({ loading: false });
    }
  },

  chooseInstall: async (path) => {
    set({ loading: true });
    try {
      const install = await api.setInstallRoot(path);
      set({ install, error: null });
      await Promise.all([get().refreshModels(), get().refreshGallery()]);
    } catch (error) {
      set({ error: errorMessage(error) });
    } finally {
      set({ loading: false });
    }
  },

  refreshModels: async () => {
    try {
      const [models, catalog] = await Promise.all([api.scanModels(), api.getCatalog()]);
      set({ models, catalog });
    } catch (error) {
      set({ error: errorMessage(error) });
    }
  },

  refreshGallery: async () => {
    try {
      set({ images: await api.listOutputs(300) });
    } catch (error) {
      set({ error: errorMessage(error) });
    }
  },

  saveSettings: async (patch) => {
    const current = get().settings;
    if (!current) return;

    const next = { ...current, ...patch };
    set({ settings: next });
    try {
      await api.saveSettings(next);
    } catch (error) {
      set({ error: errorMessage(error) });
    }
  },
}));

/** Subscribe to backend events once, at app start. */
export async function attachEventListeners(): Promise<() => void> {
  const unlisten = await Promise.all([
    events.onStatus((status) => {
      useStore.setState({ status });

      // A finished run may have produced new images; a fresh start may have
      // pulled models down on its own.
      if (status.state === "ready") {
        void useStore.getState().refreshModels();
      }
    }),

    events.onLog(({ line, stream, transient }) => {
      useStore.setState((state) => {
        const logs = state.logs.slice();
        const previous = logs[logs.length - 1];

        // Carriage-return progress bars replace the line they overwrote
        // rather than filling the log with thousands of near-identical rows.
        if (transient && previous && previous.stream === stream) {
          logs[logs.length - 1] = { ...previous, text: line };
        } else {
          logs.push({ id: logCounter++, text: line, stream });
        }

        return { logs: logs.slice(-LOG_LIMIT) };
      });
    }),

    // Registered app-wide, not per-screen: a job keeps running while the user
    // browses Models or Gallery, and its progress must survive that.
    events.onBridge((event) => {
      if (event.kind === "preview") {
        useStore.setState({
          generating: true,
          genPercent: (event.percentage ?? 0) / 100,
          genStage: event.title || "",
          ...(event.image ? { genPreview: `data:image/png;base64,${event.image}` } : {}),
        });
      } else if (event.kind === "finish") {
        useStore.setState({
          generating: false,
          genPercent: 1,
          genStage: "Done",
          genPreview: null,
          ...(event.images?.length ? { genResults: event.images } : {}),
        });
        void useStore.getState().refreshGallery();
      }
    }),

    events.onDownload((job) => {
      useStore.setState((state) => {
        const jobs = state.jobs.slice();
        const index = jobs.findIndex((existing) => existing.id === job.id);
        if (index >= 0) jobs[index] = job;
        else jobs.push(job);
        return { jobs };
      });

      // A completed file changes what the Model Manager should show.
      if (job.state === "completed") {
        void useStore.getState().refreshModels();
      }
    }),
  ]);

  return () => unlisten.forEach((fn) => fn());
}

/** Downloads that are still doing something, for the sidebar badge. */
export function activeJobCount(jobs: Job[]): number {
  return jobs.filter((job) => job.state === "downloading" || job.state === "queued").length;
}
