import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ---------------------------------------------------------------------- types

export interface BatFile {
  name: string;
  path: string;
  label: string;
  description: string;
  args: string[];
  autoUpdates: boolean;
}

export interface InstallInfo {
  root: string;
  fooocusDir: string;
  python: string;
  version: string | null;
  bats: BatFile[];
  presets: string[];
  outputsDir: string;
  modelPaths: Record<string, string[]>;
}

export interface ModelFile {
  name: string;
  path: string;
  size: number;
  category: string;
}

export interface ModelCategory {
  id: string;
  label: string;
  description: string;
  paths: string[];
  files: ModelFile[];
  totalSize: number;
}

export interface CatalogEntry {
  id: string;
  name: string;
  filename: string;
  category: string;
  url: string;
  description: string;
  tags: string[];
  essential: boolean;
  installed: boolean;
  targetPath: string;
  installedSize: number | null;
}

export type RunState = "stopped" | "starting" | "ready" | "stopping" | "crashed";

export interface StatusPayload {
  state: RunState;
  port: number | null;
  url: string | null;
  progress: number;
  stage: string;
  /** What the current stage is doing right now, e.g. which package is downloading. */
  detail: string | null;
  exitCode: number | null;
}

export interface LogPayload {
  line: string;
  stream: "stdout" | "stderr";
  transient: boolean;
}

export type JobState =
  | "queued"
  | "downloading"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled";

export interface Job {
  id: string;
  name: string;
  filename: string;
  category: string;
  url: string;
  target: string;
  state: JobState;
  downloaded: number;
  total: number | null;
  speed: number;
  error: string | null;
}

export interface GalleryImage {
  name: string;
  path: string;
  day: string;
  size: number;
  modified: number;
}

export interface AspectRatio {
  /** Exact string Fooocus expects, HTML markup and all. */
  value: string;
  /** Same thing with the markup stripped, for display. */
  label: string;
}

export interface BridgeOptions {
  styles: string[];
  aspectRatios: AspectRatio[];
  /** Upscale and Vary methods, minus "Disabled". */
  uovMethods: string[];
  /** Image prompt modes: ImagePrompt, PyraCanny, CPDS, FaceSwap. */
  ipTypes: string[];
  /** Per-type [stopAt, weight] starting points Fooocus itself uses. */
  ipDefaults: Record<string, [number, number]>;
  /** How many image prompt slots this install allows. */
  ipSlotCount: number;
  /** LoRA limits, read from the install rather than assumed. */
  maxLoraNumber: number;
  loraMinWeight: number;
  loraMaxWeight: number;
  performances: string[];
  baseModels: string[];
  outputFormats: string[];
  defaults: {
    prompt: string;
    negativePrompt: string;
    styles: string[];
    performance: string;
    aspectRatio: string;
    imageNumber: number;
    baseModel: string;
    outputFormat: string;
    sharpness: number;
    cfgScale: number;
    maxImageNumber: number;
  };
}

/** What the UI sends to start a job. Anything omitted uses the install's default.
 *  Field names are snake_case because they map onto Fooocus's own argument names. */
export interface GenerateOptions {
  prompt: string;
  negative_prompt: string;
  styles: string[];
  performance: string;
  aspect_ratio: string;
  image_number: number;
  seed: number;
  disable_seed_increment: boolean;
  base_model_name?: string;
  refiner_model_name?: string;
  refiner_switch?: number;
  loras?: [boolean, string, number][];

  /** Which image tool consumes `input_image`. Images are base64 PNG. */
  current_tab?: "uov" | "inpaint" | "ip";
  input_image?: string;
  /** Upscale & Vary method, e.g. "Upscale (2x)". Only used when tab is 'uov'. */
  uov_method?: string;
  mask_image?: string;
  inpaint_additional_prompt?: string;
  inpaint_strength?: number;
  inpaint_respective_field?: number;
  outpaint_selections?: ("Left" | "Right" | "Top" | "Bottom")[];
  /** Image prompt slots, used when tab is 'ip'. Images are base64 PNG. */
  ip_slots?: { image: string; type: string; stop: number; weight: number }[];
}

/** The subset of a preset file we apply to the controls. */
export interface Preset {
  default_model?: string;
  default_refiner?: string;
  default_refiner_switch?: number;
  default_loras?: [boolean, string, number][] | [string, number][];
  default_styles?: string[];
  default_performance?: string;
  default_prompt?: string;
  default_prompt_negative?: string;
  default_aspect_ratio?: string;
}

export interface BridgeEvent {
  kind: "queued" | "preview" | "results" | "finish";
  index: number;
  jobId: string;
  /** preview only */
  percentage?: number;
  title?: string;
  /** base64 PNG, preview only */
  image?: string | null;
  /** finish only: absolute paths of the finished images */
  images?: string[];
}

export interface CivitaiFile {
  name: string;
  sizeKb: number;
  downloadUrl: string;
  sha256: string | null;
}

export interface CivitaiVersion {
  id: number;
  name: string;
  baseModel: string;
  /** False when this version's base model is not something Fooocus can run. */
  compatible: boolean;
  file: CivitaiFile | null;
  image: string | null;
  /** True when this file is already present in the target folder. */
  installed: boolean;
}

export interface CivitaiModel {
  id: number;
  name: string;
  kind: string;
  /** Our model category, or null when Fooocus has no use for this type. */
  category: string | null;
  creator: string | null;
  nsfw: boolean;
  downloads: number;
  thumbsUp: number;
  tags: string[];
  versions: CivitaiVersion[];
}

export interface CivitaiSearchParams {
  query?: string;
  types?: string;
  sort?: string;
  cursor?: string;
  allBaseModels?: boolean;
  nsfw?: boolean;
  hiddenTags?: string[];
}

export interface CivitaiResults {
  items: CivitaiModel[];
  nextCursor: string | null;
}

export interface ReleasePackage {
  version: string;
  filename: string;
  url: string;
  size: number;
  requiredSpace: number;
  /** True when the GitHub API was unreachable and we used the known-good URL. */
  fallback: boolean;
}

export interface PackageDrift {
  name: string;
  expected: string;
  /** null when the package is missing entirely. */
  installed: string | null;
}

export type GpuVendor = "nvidia" | "amd" | "intelArc" | "cpu";

export interface GpuInfo {
  vendor: GpuVendor;
  name: string;
  adapters: string[];
  /** Why CPU was chosen, when an adapter exists but cannot be used. */
  note: string | null;
}

export type InstallPhase =
  | "downloading"
  | "extracting"
  | "configuring"
  | "finalizing"
  | "complete"
  | "failed"
  | "cancelled";

export interface InstallProgress {
  phase: InstallPhase;
  progress: number;
  bytes: number;
  total: number | null;
  speed: number;
  message: string;
  error: string | null;
  installRoot: string | null;
}

export interface Settings {
  installRoot: string | null;
  lastBat: string | null;
  lastPreset: string | null;
  autoStart: boolean;
  stopOnExit: boolean;
  gpuVendor: GpuVendor | null;
  /** Simultaneous downloads. 0 means the built-in default. */
  maxConcurrentDownloads: number;
  /** Language prompts are written in. null means English. */
  promptLanguage: string | null;
  /** Translate prompts before generating. */
  translatePrompts: boolean;
}

/** How well the model assigned to a language actually translates it.
 *  "dedicated" is a model trained on exactly this pair and is the good case;
 *  "family" is a language-family model, a little behind; "broad" is the
 *  hundred-language catch-all, used only where nothing better exists. */
export type TranslationQuality = "dedicated" | "family" | "broad";

export interface Language {
  code: string;
  name: string;
  nativeName: string;
  /** Hugging Face model serving this language, e.g. "opus-mt-de-en".
   *  Several languages share one, so switching between them may need no
   *  download at all. */
  model: string;
  quality: TranslationQuality;
}

export interface TranslatedPrompt {
  /** "prompt" or "negative_prompt". */
  field: string;
  original: string;
  /** Absent when translation failed and the original was sent unchanged. */
  translated?: string;
  error?: string;
}

export interface TranslationStatus {
  /** Every file of the selected language's model is on disk. */
  modelReady: boolean;
  /** SentencePiece and sacremoses are vendored and importable. */
  runtimeReady: boolean;
  /** Bytes used by every downloaded model, not only the selected one. */
  bytesOnDisk: number;
  /** Model files still to fetch for the selected language. */
  missing: string[];
  /** Language to translate from, or null when translation should not run.
   *  Resolved by the backend, so the UI never decides this for itself. */
  activeLanguage: string | null;
  /** Model serving the selected language, present even before it is
   *  downloaded so the UI can name what it is about to fetch. */
  activeModel: string | null;
  activeQuality: TranslationQuality | null;
}

// ------------------------------------------------------------------- commands

export const api = {
  getInstall: () => invoke<InstallInfo | null>("get_install"),
  setInstallRoot: (path: string) => invoke<InstallInfo>("set_install_root", { path }),
  findFooocusPackage: () => invoke<ReleasePackage>("find_fooocus_package"),
  checkFreeSpace: (path: string) => invoke<number | null>("check_free_space", { path }),
  suggestInstallLocation: () => invoke<string>("suggest_install_location"),
  detectGpu: () => invoke<GpuInfo>("detect_gpu"),
  configureGpu: (vendor: GpuVendor) => invoke<void>("configure_gpu", { vendor }),
  checkPackages: () => invoke<PackageDrift[]>("check_packages"),
  repairPackages: () => invoke<void>("repair_packages"),
  installFooocus: (pkg: ReleasePackage, dest: string, vendor: GpuVendor) =>
    invoke<void>("install_fooocus", { package: pkg, dest, vendor }),
  cancelInstall: () => invoke<void>("cancel_install"),

  scanModels: () => invoke<ModelCategory[]>("scan_models"),
  getCatalog: () => invoke<CatalogEntry[]>("get_catalog"),
  probeSize: (url: string) => invoke<number | null>("probe_size", { url }),

  startFooocus: (bat: string, preset: string | null) =>
    invoke<StatusPayload>("start_fooocus", { bat, preset }),
  stopFooocus: () => invoke<void>("stop_fooocus"),
  getStatus: () => invoke<StatusPayload>("get_status"),

  readPreset: (name: string) => invoke<Preset>("read_preset", { name }),
  bridgeOptions: () => invoke<BridgeOptions>("bridge_options"),
  bridgeReady: () => invoke<boolean>("bridge_ready"),
  bridgeGenerate: (options: GenerateOptions) =>
    invoke<{ jobId: string }>("bridge_generate", { options }),
  bridgeStop: (skip: boolean) => invoke<void>("bridge_stop", { skip }),

  civitaiSearch: (params: CivitaiSearchParams) =>
    invoke<CivitaiResults>("civitai_search", { params }),
  civitaiHasKey: () => invoke<boolean>("civitai_has_key"),
  secureStorageAvailable: () => invoke<boolean>("secure_storage_available"),
  civitaiHiddenTags: () => invoke<string[]>("civitai_hidden_tags"),
  civitaiSetHiddenTags: (tags: string[]) =>
    invoke<void>("civitai_set_hidden_tags", { tags }),
  civitaiSetKey: (key: string) => invoke<boolean>("civitai_set_key", { key }),
  civitaiDownload: (args: {
    versionId: number;
    name: string;
    filename: string;
    category: string;
    url: string;
  }) => invoke<void>("civitai_download", args),

  startDownload: (id: string) => invoke<void>("start_download", { id }),
  /** Resume or retry an existing job. Works for Civitai downloads too, which
   *  start_download cannot handle because they are not in the catalog. */
  resumeDownload: (id: string) => invoke<void>("resume_download", { id }),
  pauseDownload: (id: string) => invoke<void>("pause_download", { id }),
  cancelDownload: (id: string) => invoke<void>("cancel_download", { id }),
  clearFinishedDownloads: () => invoke<void>("clear_finished_downloads"),
  getDownloads: () => invoke<Job[]>("get_downloads"),

  translationLanguages: () => invoke<Language[]>("translation_languages"),
  translationStatus: () => invoke<TranslationStatus>("translation_status"),
  /** Codes whose model is already downloaded, for marking them in the picker. */
  translationInstalled: () => invoke<string[]>("translation_installed"),
  /** Vendors the Python package, then queues the model. Returns how many
   *  files were queued, which is 0 when everything was already present. */
  installTranslation: () => invoke<number>("install_translation"),
  /** Deletes one language's model, or all of them when no code is given.
   *  Returns the bytes reclaimed. */
  removeTranslation: (code?: string) => invoke<number>("remove_translation", { code }),
  translatePrompt: (text: string) => invoke<string>("translate_prompt", { text }),

  listOutputs: (limit?: number) => invoke<GalleryImage[]>("list_outputs", { limit }),

  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  readFooocusConfig: () => invoke<string>("read_fooocus_config"),
  writeFooocusConfig: (contents: string) => invoke<void>("write_fooocus_config", { contents }),
};

// --------------------------------------------------------------------- events

export const events = {
  onLog: (handler: (payload: LogPayload) => void): Promise<UnlistenFn> =>
    listen<LogPayload>("fooocus://log", (event) => handler(event.payload)),

  onStatus: (handler: (payload: StatusPayload) => void): Promise<UnlistenFn> =>
    listen<StatusPayload>("fooocus://status", (event) => handler(event.payload)),

  onDownload: (handler: (payload: Job) => void): Promise<UnlistenFn> =>
    listen<Job>("download://progress", (event) => handler(event.payload)),

  onBridge: (handler: (payload: BridgeEvent) => void): Promise<UnlistenFn> =>
    listen<BridgeEvent>("bridge://event", (event) => handler(event.payload)),

  onInstall: (handler: (payload: InstallProgress) => void): Promise<UnlistenFn> =>
    listen<InstallProgress>("install://progress", (event) => handler(event.payload)),

  /** Fires when a prompt was translated on its way to Fooocus, so the user can
   *  see the English that was actually sent rather than having their words
   *  silently rewritten. */
  onTranslated: (handler: (payload: TranslatedPrompt) => void): Promise<UnlistenFn> =>
    listen<TranslatedPrompt>("prompt://translated", (event) => handler(event.payload)),
};

/** Tauri errors arrive as plain strings; normalise anything else defensively. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}
