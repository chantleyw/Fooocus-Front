import { useCallback, useEffect, useRef, useState } from "react";
import { ImagePlus, SkipForward, Sparkles, Square, Upload } from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";

import { api, errorMessage, type BridgeOptions } from "../lib/api";
import { useStore } from "../store";
import { Banner, Chip, ProgressBar } from "../components/ui";
import { MaskEditor, type MaskEditorHandle } from "../components/MaskEditor";
import { RecentStrip, useImageSource } from "./imageInput";

/** Fooocus's own inpaint methods, which change how the masked area is treated. */
const METHODS = [
  {
    id: "Inpaint or Outpaint (default)",
    label: "Inpaint",
    hint: "Replace the masked area, guided by your prompt.",
  },
  {
    id: "Improve Detail (face, hand, eyes, etc.)",
    label: "Improve detail",
    hint: "Refine what is already there rather than replacing it. Good for faces and hands.",
  },
  {
    id: "Modify Content (add objects, change background, etc.)",
    label: "Modify content",
    hint: "Add or substantially change what occupies the masked area.",
  },
];

const OUTPAINT_SIDES = ["Left", "Right", "Top", "Bottom"] as const;

export function Inpaint({ options }: { options: BridgeOptions }) {
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
  const [prompt, setPrompt] = useState("");
  const [method, setMethod] = useState(METHODS[0].id);
  const [strength, setStrength] = useState(1);
  const [outpaint, setOutpaint] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  const editor = useRef<MaskEditorHandle | null>(null);
  const onEditorReady = useCallback((handle: MaskEditorHandle) => {
    editor.current = handle;
  }, []);

  useEffect(() => {
    void refreshGallery();
  }, [refreshGallery]);

  async function generate() {
    const mask = editor.current?.getMask();
    if (!image.data) return;

    if (!mask && outpaint.length === 0) {
      setError("Paint the area you want to change first, or choose a direction to extend.");
      return;
    }

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
        current_tab: "inpaint",
        input_image: image.data,
        mask_image: mask ?? undefined,
        inpaint_additional_prompt: method === METHODS[0].id ? "" : prompt,
        inpaint_strength: strength,
        outpaint_selections: outpaint as ("Left" | "Right" | "Top" | "Bottom")[],
      });
    } catch (err) {
      failGeneration(errorMessage(err));
      setError(errorMessage(err));
    }
  }

  // ------------------------------------------------------------- no image yet

  if (!image.source) {
    return (
      <div className="studio-native">
        <aside className="studio-panel">
          <h2 className="section-title">Inpaint & outpaint</h2>
          <p className="field-hint">
            Choose an image, paint over what you want changed, and describe the result. Zoom right
            in for precise edges.
          </p>

          <button className="btn btn-primary" onClick={() => void image.pickFile()} disabled={image.busy}>
            <Upload size={15} />
            Choose an image
          </button>

          {error && <Banner tone="danger">{error}</Banner>}
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

  const selectedMethod = METHODS.find((entry) => entry.id === method);

  return (
    <div className="studio-native">
      <aside className="studio-panel">
        <div className="field">
          <label className="field-label" htmlFor="inpaint-prompt">
            What should appear there?
          </label>
          <textarea
            id="inpaint-prompt"
            className="textarea prompt-input"
            rows={4}
            value={prompt}
            placeholder="Describe what belongs in the masked area"
            onChange={(event) => setPrompt(event.target.value)}
          />
        </div>

        <div className="field">
          <label className="field-label" htmlFor="method">
            Method
          </label>
          <select
            id="method"
            className="select"
            value={method}
            onChange={(event) => setMethod(event.target.value)}
          >
            {METHODS.map((entry) => (
              <option key={entry.id} value={entry.id}>
                {entry.label}
              </option>
            ))}
          </select>
          <span className="field-hint">{selectedMethod?.hint}</span>
        </div>

        <div className="field">
          <label className="field-label">Extend the image</label>
          <div style={{ display: "flex", gap: 5, flexWrap: "wrap" }}>
            {OUTPAINT_SIDES.map((side) => (
              <button
                key={side}
                className={`btn btn-sm${outpaint.includes(side) ? " btn-primary" : ""}`}
                onClick={() =>
                  setOutpaint((current) =>
                    current.includes(side)
                      ? current.filter((entry) => entry !== side)
                      : [...current, side],
                  )
                }
              >
                {side}
              </button>
            ))}
          </div>
          <span className="field-hint">
            Outpainting grows the canvas in the chosen directions. No mask needed.
          </span>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="strength">
            Strength: {strength.toFixed(2)}
          </label>
          <input
            id="strength"
            type="range"
            min={0.1}
            max={1}
            step={0.05}
            value={strength}
            onChange={(event) => setStrength(Number(event.target.value))}
          />
          <span className="field-hint">
            Lower values keep more of the original underneath the mask.
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
              <Sparkles size={16} />
              Generate
            </button>
          )}
        </div>

        {error && <Banner tone="danger">{error}</Banner>}
      </aside>

      <section className="studio-canvas">
        {busy && (
          <div className="studio-progress">
            <div className="status-stage">{stage}</div>
            <ProgressBar value={percent} indeterminate={percent === 0} />
          </div>
        )}

        {busy && preview ? (
          <div className="studio-stage">
            <img className="studio-image" src={preview} alt="Preview" />
          </div>
        ) : !busy && results.length > 0 ? (
          <>
            <div className="studio-stage">
              <img className="studio-image" src={convertFileSrc(results[0])} alt="" />
            </div>
            <div className="studio-footer">
              <Chip tone="success">Done</Chip>
              <button className="btn btn-sm" onClick={() => void image.loadFrom(results[0])}>
                Edit this result
              </button>
            </div>
          </>
        ) : (
          <MaskEditor src={image.source} onReady={onEditorReady} />
        )}
      </section>
    </div>
  );
}
