import { useEffect, useState } from "react";
import {
  AlertTriangle,
  ImagePlus,
  Plus,
  SkipForward,
  Sparkles,
  Square,
  Trash2,
  Upload,
} from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import { api, errorMessage, type BridgeOptions } from "../lib/api";
import { useStore } from "../store";
import { Banner, Chip, ProgressBar } from "../components/ui";

/**
 * What each mode does, and which catalog entries it needs on disk.
 *
 * Fooocus downloads these silently on first use, which from the UI looks like a
 * hang for several gigabytes. Checking up front and saying so is the whole
 * point of listing the requirements here.
 */
const MODES: Record<string, { label: string; hint: string; requires: string[] }> = {
  ImagePrompt: {
    label: "Image Prompt",
    hint: "Borrows the overall style, colour and content of the reference.",
    requires: ["clip-vision-h", "ip-adapter-plus", "ip-negative"],
  },
  PyraCanny: {
    label: "PyraCanny",
    hint: "Follows the hard edges of the reference. Good for keeping a composition or pose.",
    requires: ["cn-canny"],
  },
  CPDS: {
    label: "CPDS",
    hint: "Follows depth and structure rather than edges. Looser than PyraCanny.",
    requires: ["cn-cpds"],
  },
  FaceSwap: {
    label: "FaceSwap",
    hint: "Carries the face from the reference into the new image.",
    requires: ["clip-vision-h", "ip-adapter-face", "ip-negative"],
  },
};

interface Slot {
  id: number;
  source: string;
  data: string;
  type: string;
  stop: number;
  weight: number;
}

let nextSlotId = 1;

export function ImagePrompt({ options }: { options: BridgeOptions }) {
  const {
    images,
    catalog,
    generating: busy,
    genPercent: percent,
    genStage: stage,
    genPreview: preview,
    genResults: results,
    beginGeneration,
    failGeneration,
    refreshGallery,
    setScreen,
  } = useStore();

  const [slots, setSlots] = useState<Slot[]>([]);
  const [prompt, setPrompt] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refreshGallery();
  }, [refreshGallery]);

  /** Catalog entries needed by the modes in use that are not yet downloaded. */
  const missing = [...new Set(slots.flatMap((slot) => MODES[slot.type]?.requires ?? []))]
    .map((id) => catalog.find((entry) => entry.id === id))
    .filter((entry) => entry && !entry.installed);

  async function addSlot(path?: string) {
    let chosen = path;
    if (!chosen) {
      const picked = await open({
        title: "Choose a reference image",
        filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp"] }],
      });
      if (typeof picked !== "string") return;
      chosen = picked;
    }

    try {
      const response = await fetch(convertFileSrc(chosen));
      const blob = await response.blob();

      const uri = await new Promise<string>((resolve) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.readAsDataURL(blob);
      });

      const type = options.ipTypes[0] ?? "ImagePrompt";
      const [stop, weight] = options.ipDefaults[type] ?? [0.5, 0.6];

      setSlots((current) => [
        ...current,
        { id: nextSlotId++, source: uri, data: uri.split(",")[1] ?? "", type, stop, weight },
      ]);
    } catch (err) {
      setError(`Could not open that image: ${errorMessage(err)}`);
    }
  }

  function update(id: number, patch: Partial<Slot>) {
    setSlots((current) =>
      current.map((slot) => {
        if (slot.id !== id) return slot;

        // Switching mode resets the sliders to that mode's own defaults, which
        // differ a lot — FaceSwap wants 0.9/0.75, ImagePrompt 0.5/0.6.
        if (patch.type && patch.type !== slot.type) {
          const [stop, weight] = options.ipDefaults[patch.type] ?? [slot.stop, slot.weight];
          return { ...slot, ...patch, stop, weight };
        }
        return { ...slot, ...patch };
      }),
    );
  }

  async function generate() {
    if (slots.length === 0) return;

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
        current_tab: "ip",
        ip_slots: slots.map((slot) => ({
          image: slot.data,
          type: slot.type,
          stop: slot.stop,
          weight: slot.weight,
        })),
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
          <label className="field-label" htmlFor="ip-prompt">
            Prompt
          </label>
          <textarea
            id="ip-prompt"
            className="textarea prompt-input"
            rows={4}
            value={prompt}
            placeholder="Describe the image you want, guided by the references"
            onChange={(event) => setPrompt(event.target.value)}
          />
        </div>

        <div className="field">
          <label className="field-label">
            References ({slots.length} of {options.ipSlotCount})
          </label>
          <span className="field-hint">
            Each reference guides generation differently. Combine several to control style and
            composition at once.
          </span>
        </div>

        {slots.map((slot) => {
          const mode = MODES[slot.type];
          return (
            <div className="ip-slot" key={slot.id}>
              <div className="ip-slot-head">
                <img className="ip-thumb" src={slot.source} alt="" />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <select
                    className="select"
                    value={slot.type}
                    onChange={(event) => update(slot.id, { type: event.target.value })}
                  >
                    {options.ipTypes.map((name) => (
                      <option key={name} value={name}>
                        {MODES[name]?.label ?? name}
                      </option>
                    ))}
                  </select>
                </div>
                <button
                  className="btn btn-ghost btn-icon"
                  title="Remove this reference"
                  onClick={() =>
                    setSlots((current) => current.filter((entry) => entry.id !== slot.id))
                  }
                >
                  <Trash2 size={14} />
                </button>
              </div>

              <p className="field-hint">{mode?.hint}</p>

              <label className="field-label" style={{ fontWeight: 500 }}>
                Weight: {slot.weight.toFixed(2)}
              </label>
              <input
                type="range"
                min={0}
                max={2}
                step={0.05}
                value={slot.weight}
                onChange={(event) => update(slot.id, { weight: Number(event.target.value) })}
              />

              <label className="field-label" style={{ fontWeight: 500 }}>
                Stop at: {slot.stop.toFixed(2)}
              </label>
              <input
                type="range"
                min={0}
                max={1}
                step={0.05}
                value={slot.stop}
                onChange={(event) => update(slot.id, { stop: Number(event.target.value) })}
              />
              <span className="field-hint">
                How far through generation this reference keeps applying. Lower values let the
                prompt take over sooner.
              </span>
            </div>
          );
        })}

        {slots.length < options.ipSlotCount && (
          <button className="btn" onClick={() => void addSlot()}>
            <Plus size={15} />
            Add a reference
          </button>
        )}

        {missing.length > 0 && (
          <Banner tone="warning" icon={<AlertTriangle size={15} />}>
            <div style={{ marginBottom: 8 }}>
              {missing.length === 1 ? "This mode needs a model" : "These modes need models"} that
              isn&apos;t downloaded yet: {missing.map((entry) => entry!.name).join(", ")}. Fooocus
              would fetch it mid-generation with no progress shown.
            </div>
            <button className="btn btn-sm" onClick={() => setScreen("models")}>
              Download first
            </button>
          </Banner>
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
              disabled={slots.length === 0}
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

        <div className="studio-stage">
          {busy && preview ? (
            <img className="studio-image" src={preview} alt="Preview" />
          ) : !busy && results.length > 0 ? (
            <img className="studio-image" src={convertFileSrc(results[0])} alt="" />
          ) : slots.length === 0 ? (
            <div className="drop-inner">
              <ImagePlus size={30} style={{ color: "var(--text-faint)" }} />
              <div>
                <div className="empty-title">Guide generation with a reference</div>
                <p className="field-hint" style={{ marginTop: 4 }}>
                  Add an image to borrow its style, composition, depth or face.
                </p>
              </div>
              <button className="btn btn-primary" onClick={() => void addSlot()}>
                <Upload size={15} />
                Add a reference
              </button>
            </div>
          ) : (
            <div className="ip-preview-grid">
              {slots.map((slot) => (
                <figure key={slot.id} className="ip-preview">
                  <img src={slot.source} alt="" />
                  <figcaption>
                    <Chip tone="accent">{MODES[slot.type]?.label ?? slot.type}</Chip>
                  </figcaption>
                </figure>
              ))}
            </div>
          )}
        </div>

        {images.length > 0 && slots.length < options.ipSlotCount && (
          <div className="studio-footer" style={{ gap: 12, overflowX: "auto" }}>
            <Chip>Use a recent image</Chip>
            {images.slice(0, 12).map((entry) => (
              <button
                key={entry.path}
                className="thumb"
                style={{ width: 58, height: 58, flexShrink: 0 }}
                onClick={() => void addSlot(entry.path)}
                title={entry.name}
              >
                <img src={convertFileSrc(entry.path)} alt="" loading="lazy" />
              </button>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
