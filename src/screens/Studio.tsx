import { useEffect, useState } from "react";
import {
  Brush,
  ChevronDown,
  Dice5,
  ImageIcon,
  Maximize2,
  Sliders,
  SkipForward,
  Sparkles,
  Square,
  Wand2,
} from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";

import { api, errorMessage, type BridgeOptions } from "../lib/api";
import { useStore } from "../store";
import { Banner, Chip, EmptyState, ProgressBar } from "../components/ui";
import { ImagePrompt } from "./ImagePrompt";
import { Inpaint } from "./Inpaint";
import { StartButton } from "./Launcher";
import { Upscale } from "./Upscale";

type Mode = "generate" | "inpaint" | "imagePrompt" | "upscale" | "fooocus";

const MODES: { id: Mode; label: string; icon: typeof Wand2; ready: boolean }[] = [
  { id: "generate", label: "Generate", icon: Sparkles, ready: true },
  { id: "inpaint", label: "Inpaint & Outpaint", icon: Brush, ready: true },
  { id: "imagePrompt", label: "Image Prompt", icon: ImageIcon, ready: true },
  { id: "upscale", label: "Upscale & Vary", icon: Maximize2, ready: true },
  { id: "fooocus", label: "Fooocus UI", icon: Sliders, ready: true },
];

export function Studio() {
  const { status } = useStore();
  const [mode, setMode] = useState<Mode>("generate");

  // Not running yet: show startup progress instead of dead controls.
  if (status.state !== "ready" || !status.url) {
    return (
      <div className="screen">
        <div className="studio">
          <div className="studio-overlay">
            <div className="studio-overlay-inner">
              {status.state === "starting" ? (
                <>
                  <EmptyState icon={<Wand2 size={22} />} title={status.stage}>
                    Fooocus is loading its models. The first start after an update takes longest.
                  </EmptyState>
                  <div style={{ marginTop: 4 }}>
                    <ProgressBar value={status.progress} indeterminate={status.progress === 0} />
                  </div>
                </>
              ) : (
                <EmptyState
                  icon={<Wand2 size={22} />}
                  title="Fooocus isn't running"
                  action={<StartButton />}
                >
                  Start it to generate images. It runs hidden in the background.
                </EmptyState>
              )}
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="screen">
      <nav className="mode-bar">
        {MODES.map(({ id, label, icon: Icon, ready }) => (
          <button
            key={id}
            className={`mode-btn${mode === id ? " active" : ""}`}
            onClick={() => setMode(id)}
            disabled={!ready}
            title={
              id === "fooocus"
                ? "The original Fooocus interface, with every option it offers"
                : ready
                  ? undefined
                  : "Not built yet — use the Fooocus UI tab for now"
            }
          >
            <Icon size={15} />
            {label}
          </button>
        ))}
      </nav>

      {mode === "fooocus" ? (
        <div className="studio">
          <iframe src={status.url} title="Fooocus" allow="clipboard-read; clipboard-write" />
        </div>
      ) : (
        <ModeSurface mode={mode} />
      )}
    </div>
  );
}

/** Loads the shared bridge options once, then hands them to the active mode. */
function ModeSurface({ mode }: { mode: Mode }) {
  const [options, setOptions] = useState<BridgeOptions | null>(null);
  const [error, setError] = useState<string | null>(null);

  // The bridge only answers once Fooocus has finished importing, which can lag
  // the "ready" status slightly. Retry until it responds.
  useEffect(() => {
    let cancelled = false;

    void (async () => {
      for (let attempt = 0; attempt < 30 && !cancelled; attempt++) {
        try {
          const loaded = await api.bridgeOptions();
          if (!cancelled) setOptions(loaded);
          return;
        } catch {
          await new Promise((resolve) => setTimeout(resolve, 1000));
        }
      }
      if (!cancelled) setError("The bridge did not respond. Try restarting Fooocus.");
    })();

    return () => {
      cancelled = true;
    };
  }, []);

  if (error) {
    return (
      <div className="screen-body">
        <Banner tone="danger">{error}</Banner>
      </div>
    );
  }

  if (!options) {
    return (
      <div className="screen-body">
        <EmptyState icon={<Sparkles size={22} />} title="Connecting to Fooocus…">
          Waiting for the generation bridge to come up.
        </EmptyState>
      </div>
    );
  }

  if (mode === "inpaint") return <Inpaint options={options} />;
  if (mode === "upscale") return <Upscale options={options} />;
  if (mode === "imagePrompt") return <ImagePrompt options={options} />;
  return <NativeStudio options={options} />;
}

// ------------------------------------------------------------------- native UI

function NativeStudio({ options }: { options: BridgeOptions }) {
  const {
    install,
    generating: busy,
    genPercent: percent,
    genStage: stage,
    genPreview: preview,
    genResults: results,
    genError,
    beginGeneration,
    failGeneration,
  } = useStore();

  const [error, setError] = useState<string | null>(null);

  const [prompt, setPrompt] = useState(options.defaults.prompt);
  const [negative, setNegative] = useState(options.defaults.negativePrompt);
  const [aspect, setAspect] = useState(options.defaults.aspectRatio);
  const [performance, setPerformance] = useState(options.defaults.performance);
  const [count, setCount] = useState(options.defaults.imageNumber);
  const [seed, setSeed] = useState<number>(0);
  const [randomSeed, setRandomSeed] = useState(true);
  const [showAdvanced, setShowAdvanced] = useState(false);

  // Applied per job rather than by restarting Fooocus with --preset.
  const [preset, setPreset] = useState("");
  const [presetModel, setPresetModel] = useState<string | null>(null);
  const [presetStyles, setPresetStyles] = useState<string[] | null>(null);
  const [presetLoras, setPresetLoras] = useState<[boolean, string, number][] | null>(null);
  const [presetRefiner, setPresetRefiner] = useState<{ name: string; switch: number } | null>(null);

  /** Load a preset and apply the parts our controls own. */
  async function applyPreset(name: string) {
    setPreset(name);

    if (!name) {
      setPresetModel(null);
      setPresetStyles(null);
      setPresetLoras(null);
      setPresetRefiner(null);
      return;
    }

    try {
      const data = await api.readPreset(name);

      setPresetModel(data.default_model ?? null);
      setPresetStyles(data.default_styles ?? null);
      setPresetRefiner(
        data.default_refiner && data.default_refiner !== "None"
          ? { name: data.default_refiner, switch: data.default_refiner_switch ?? 0.5 }
          : null,
      );

      // Presets store LoRAs as either [enabled, name, weight] or [name, weight].
      const loras = data.default_loras?.map((entry) =>
        entry.length === 3
          ? (entry as [boolean, string, number])
          : ([true, entry[0], entry[1]] as [boolean, string, number]),
      );
      setPresetLoras(loras ?? null);

      if (data.default_performance) setPerformance(data.default_performance);
      if (data.default_aspect_ratio) setAspect(data.default_aspect_ratio);
      if (data.default_prompt_negative !== undefined) setNegative(data.default_prompt_negative);
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function generate() {
    setError(null);
    beginGeneration();

    try {
      await api.bridgeGenerate({
        prompt,
        negative_prompt: negative,
        styles: presetStyles ?? options?.defaults.styles ?? [],
        performance,
        aspect_ratio: aspect,
        image_number: count,
        seed: randomSeed ? Math.floor(Math.random() * 2 ** 31) : seed,
        disable_seed_increment: false,
        ...(presetModel ? { base_model_name: presetModel } : {}),
        ...(presetLoras ? { loras: presetLoras } : {}),
        ...(presetRefiner
          ? {
              refiner_model_name: presetRefiner.name,
              refiner_switch: presetRefiner.switch,
            }
          : {}),
      });
    } catch (err) {
      failGeneration(errorMessage(err));
      setError(errorMessage(err));
    }
  }

  return (
    <div className="studio-native">
      <aside className="studio-panel">
        <div className="field">
          <label className="field-label" htmlFor="prompt">
            Prompt
          </label>
          <textarea
            id="prompt"
            className="textarea prompt-input"
            rows={5}
            value={prompt}
            placeholder="Describe the image you want"
            onChange={(event) => setPrompt(event.target.value)}
          />
        </div>

        <div className="field">
          <label className="field-label" htmlFor="preset">
            Preset
          </label>
          <select
            id="preset"
            className="select"
            value={preset}
            onChange={(event) => void applyPreset(event.target.value)}
          >
            <option value="">None — use current settings</option>
            {(install?.presets ?? []).map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
          {presetModel && (
            <span className="field-hint truncate">
              Using {presetModel}
              {presetStyles?.length ? ` · ${presetStyles.length} styles` : ""}
            </span>
          )}
        </div>

        <div className="field">
          <label className="field-label" htmlFor="aspect">
            Aspect ratio
          </label>
          <select
            id="aspect"
            className="select"
            value={aspect}
            onChange={(event) => setAspect(event.target.value)}
          >
            {options.aspectRatios.map((ratio) => (
              <option key={ratio.value} value={ratio.value}>
                {ratio.label}
              </option>
            ))}
          </select>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="performance">
            Performance
          </label>
          <select
            id="performance"
            className="select"
            value={performance}
            onChange={(event) => setPerformance(event.target.value)}
          >
            {options.performances.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
          <span className="field-hint">
            Speed modes use fewer steps. They need their matching LoRA installed.
          </span>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="count">
            Images: {count}
          </label>
          <input
            id="count"
            type="range"
            min={1}
            max={Math.min(options.defaults.maxImageNumber, 16)}
            value={count}
            onChange={(event) => setCount(Number(event.target.value))}
          />
        </div>

        <button className="disclosure" onClick={() => setShowAdvanced((open) => !open)}>
          <Sliders size={14} />
          Advanced
          <ChevronDown
            size={14}
            style={{
              marginLeft: "auto",
              transform: showAdvanced ? "rotate(180deg)" : undefined,
              transition: "transform 0.15s",
            }}
          />
        </button>

        {showAdvanced && (
          <>
            <div className="field">
              <label className="field-label" htmlFor="negative">
                Negative prompt
              </label>
              <textarea
                id="negative"
                className="textarea"
                rows={3}
                value={negative}
                placeholder="What to avoid"
                onChange={(event) => setNegative(event.target.value)}
              />
            </div>

            <div className="field">
              <label className="field-label">Seed</label>
              <label className="switch" style={{ marginBottom: 6 }}>
                <input
                  type="checkbox"
                  checked={randomSeed}
                  onChange={(event) => setRandomSeed(event.target.checked)}
                />
                <span className="field-hint">Random each time</span>
              </label>
              {!randomSeed && (
                <div style={{ display: "flex", gap: 6 }}>
                  <input
                    className="input mono"
                    type="number"
                    value={seed}
                    onChange={(event) => setSeed(Number(event.target.value))}
                  />
                  <button
                    className="btn btn-icon"
                    title="Randomise"
                    onClick={() => setSeed(Math.floor(Math.random() * 2 ** 31))}
                  >
                    <Dice5 size={15} />
                  </button>
                </div>
              )}
            </div>
          </>
        )}

        <div className="studio-actions">
          {busy ? (
            <>
              <button className="btn" onClick={() => void api.bridgeStop(true)}>
                <SkipForward size={15} />
                Skip
              </button>
              <button
                className="btn btn-danger"
                style={{ flex: 1 }}
                onClick={() => void api.bridgeStop(false)}
              >
                <Square size={15} />
                Stop
              </button>
            </>
          ) : (
            <button
              className="btn btn-primary btn-lg"
              style={{ width: "100%" }}
              onClick={() => void generate()}
              disabled={!prompt.trim()}
            >
              <Sparkles size={16} />
              Generate
            </button>
          )}
        </div>

        {(error || genError) && <Banner tone="danger">{error ?? genError}</Banner>}
      </aside>

      <section className="studio-canvas">
        {busy && (
          <div className="studio-progress">
            <div className="status-stage">{stage}</div>
            <ProgressBar value={percent} indeterminate={percent === 0} />
          </div>
        )}

        <div className="studio-stage">
          {preview ? (
            <img className="studio-image" src={preview} alt="Preview" />
          ) : results.length > 0 ? (
            <div className={`result-grid${results.length === 1 ? " single" : ""}`}>
              {results.map((path) => (
                <img key={path} className="studio-image" src={convertFileSrc(path)} alt="" />
              ))}
            </div>
          ) : (
            <EmptyState icon={<ImageIcon size={22} />} title="Nothing generated yet">
              Write a prompt and hit Generate. The image builds here step by step as it renders.
            </EmptyState>
          )}
        </div>

        {results.length > 0 && !busy && (
          <div className="studio-footer">
            <Chip tone="success">
              {results.length} {results.length === 1 ? "image" : "images"} saved
            </Chip>
            <button
              className="btn btn-sm"
              onClick={() => useStore.getState().setScreen("gallery")}
            >
              Open in Gallery
            </button>
          </div>
        )}
      </section>
    </div>
  );
}
