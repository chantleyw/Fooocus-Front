import { useEffect, useState } from "react";
import { ImagePlus, Maximize2, Sparkles, SkipForward, Square, Upload } from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";

import { api, errorMessage, type BridgeOptions } from "../lib/api";
import { useStore } from "../store";
import { Banner, Chip, ProgressBar } from "../components/ui";
import { RecentStrip, useImageSource } from "./imageInput";

/**
 * What each Fooocus method actually does, in plain language. Keyed by the exact
 * strings from `modules/flags.py`, with a fallback so a method added by a newer
 * Fooocus still appears rather than vanishing from the list.
 */
const DESCRIPTIONS: Record<string, string> = {
  "Vary (Subtle)": "Regenerates at the same size with small changes. Keeps the composition.",
  "Vary (Strong)": "Regenerates with substantial freedom. Same subject, noticeably different image.",
  "Upscale (1.5x)": "Enlarges by half again, redrawing detail as it goes.",
  "Upscale (2x)": "Doubles the size, redrawing detail. The slowest option, and the sharpest.",
  "Upscale (Fast 2x)": "Doubles the size with a straight upscale. Fast, but adds no new detail.",
};

export function Upscale({ options }: { options: BridgeOptions }) {
  const {
    images,
    generating: busy,
    genPercent: percent,
    genStage: stage,
    genPreview: preview,
    genResults: results,
    beginGeneration,
    failGeneration,
    refreshGallery,
  } = useStore();

  const image = useImageSource();
  const [method, setMethod] = useState(options.uovMethods[0] ?? "Upscale (2x)");
  const [prompt, setPrompt] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refreshGallery();
  }, [refreshGallery]);

  async function generate() {
    if (!image.data) return;

    setError(null);
    beginGeneration();

    try {
      await api.bridgeGenerate({
        prompt,
        negative_prompt: options.defaults.negativePrompt,
        styles: options.defaults.styles,
        performance: options.defaults.performance,
        aspect_ratio: options.defaults.aspectRatio,
        image_number: 1,
        seed: Math.floor(Math.random() * 2 ** 31),
        disable_seed_increment: false,
        current_tab: "uov",
        uov_method: method,
        input_image: image.data,
      });
    } catch (err) {
      failGeneration(errorMessage(err));
      setError(errorMessage(err));
    }
  }

  const problem = error ?? image.error;

  // ------------------------------------------------------------- no image yet

  if (!image.source) {
    return (
      <div className="studio-native">
        <aside className="studio-panel">
          <h2 className="section-title">Upscale & Vary</h2>
          <p className="field-hint">
            Enlarge an image with new detail, or generate variations of it. Start by choosing a
            picture.
          </p>

          <button
            className="btn btn-primary"
            onClick={() => void image.pickFile()}
            disabled={image.busy}
          >
            <Upload size={15} />
            Choose an image
          </button>

          {problem && <Banner tone="danger">{problem}</Banner>}
        </aside>

        <section className="studio-canvas">
          <div className="drop-zone">
            <div className="drop-inner">
              <ImagePlus size={30} style={{ color: "var(--text-faint)" }} />
              <div>
                <div className="empty-title">Start from an image</div>
                <p className="field-hint" style={{ marginTop: 4 }}>
                  Pick a file, or start from something you generated earlier.
                </p>
              </div>
              <button className="btn btn-primary" onClick={() => void image.pickFile()}>
                <Upload size={15} />
                Choose an image
              </button>
            </div>
          </div>

          <RecentStrip images={images} onPick={(path) => void image.loadFrom(path)} />
        </section>
      </div>
    );
  }

  // ---------------------------------------------------------------- with image

  return (
    <div className="studio-native">
      <aside className="studio-panel">
        <div className="field">
          <label className="field-label" htmlFor="uov-method">
            What would you like to do?
          </label>
          <select
            id="uov-method"
            className="select"
            value={method}
            onChange={(event) => setMethod(event.target.value)}
          >
            {options.uovMethods.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
          <span className="field-hint">
            {DESCRIPTIONS[method] ?? "Applies this Fooocus method to the image."}
          </span>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="uov-prompt">
            Prompt <span style={{ fontWeight: 400, color: "var(--text-faint)" }}>(optional)</span>
          </label>
          <textarea
            id="uov-prompt"
            className="textarea prompt-input"
            rows={4}
            value={prompt}
            placeholder="Guide the new detail, or leave blank to keep it faithful"
            onChange={(event) => setPrompt(event.target.value)}
          />
          <span className="field-hint">
            Ignored by Fast 2x, which does not redraw anything.
          </span>
        </div>

        <button className="btn btn-sm" onClick={() => void image.pickFile()}>
          <Upload size={14} />
          Use a different image
        </button>

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
            >
              <Maximize2 size={16} />
              {method.startsWith("Vary") ? "Create variation" : "Upscale"}
            </button>
          )}
        </div>

        {problem && <Banner tone="danger">{problem}</Banner>}
      </aside>

      <section className="studio-canvas">
        {busy && (
          <div className="studio-progress">
            <div className="status-stage">{stage}</div>
            <ProgressBar value={percent} indeterminate={percent === 0} />
          </div>
        )}

        <div className="studio-stage">
          <img
            className="studio-image"
            src={
              busy && preview
                ? preview
                : !busy && results.length > 0
                  ? convertFileSrc(results[0])
                  : image.source
            }
            alt=""
          />
        </div>

        <div className="studio-footer">
          {!busy && results.length > 0 ? (
            <>
              <Chip tone="success">
                <Sparkles size={12} />
                Done
              </Chip>
              <button className="btn btn-sm" onClick={() => void image.loadFrom(results[0])}>
                Use this as the input
              </button>
            </>
          ) : (
            <Chip>Showing the original</Chip>
          )}
        </div>
      </section>
    </div>
  );
}
