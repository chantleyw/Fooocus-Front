import { Plus, Trash2 } from "lucide-react";

import { useStore } from "../store";

export interface LoraSlot {
  name: string;
  weight: number;
}

/**
 * LoRA selection, limited to what is actually installed.
 *
 * Fooocus takes a fixed number of (enabled, name, weight) triples; the bridge
 * pads the unused slots, so this only has to describe the ones in use.
 */
export function LoraPicker({
  slots,
  onChange,
  max,
  minWeight,
  maxWeight,
}: {
  slots: LoraSlot[];
  onChange: (next: LoraSlot[]) => void;
  max: number;
  minWeight: number;
  maxWeight: number;
}) {
  const { models, setScreen } = useStore();

  // Only real files: the placeholder notes Fooocus ships in the folder are
  // filtered out by the scanner already.
  const available = models.find((category) => category.id === "loras")?.files ?? [];
  const unused = available.filter((file) => !slots.some((slot) => slot.name === file.name));

  function update(index: number, patch: Partial<LoraSlot>) {
    onChange(slots.map((slot, i) => (i === index ? { ...slot, ...patch } : slot)));
  }

  if (available.length === 0) {
    return (
      <div className="field">
        <label className="field-label">LoRAs</label>
        <span className="field-hint">
          None installed yet. LoRAs are small add-ons that push a checkpoint toward a style or
          subject.
        </span>
        <button className="btn btn-sm" onClick={() => setScreen("models")}>
          Find some
        </button>
      </div>
    );
  }

  return (
    <div className="field">
      <label className="field-label">
        LoRAs
        {slots.length > 0 && (
          <span style={{ fontWeight: 400, color: "var(--text-faint)" }}>
            {" "}
            · {slots.length} of {max}
          </span>
        )}
      </label>

      {slots.map((slot, index) => (
        <div className="lora-slot" key={`${slot.name}-${index}`}>
          <div style={{ display: "flex", gap: 7, alignItems: "center" }}>
            <select
              className="select"
              value={slot.name}
              onChange={(event) => update(index, { name: event.target.value })}
            >
              <option value={slot.name}>{slot.name}</option>
              {unused.map((file) => (
                <option key={file.path} value={file.name}>
                  {file.name}
                </option>
              ))}
            </select>
            <button
              className="btn btn-ghost btn-icon"
              title="Remove"
              onClick={() => onChange(slots.filter((_, i) => i !== index))}
            >
              <Trash2 size={14} />
            </button>
          </div>

          <label className="field-label" style={{ fontWeight: 500, marginTop: 6 }}>
            Weight: {slot.weight.toFixed(2)}
          </label>
          <input
            type="range"
            min={minWeight}
            max={maxWeight}
            step={0.05}
            value={slot.weight}
            onChange={(event) => update(index, { weight: Number(event.target.value) })}
          />
        </div>
      ))}

      {slots.length < max && unused.length > 0 && (
        <button
          className="btn btn-sm"
          onClick={() => onChange([...slots, { name: unused[0].name, weight: 1 }])}
        >
          <Plus size={14} />
          Add a LoRA
        </button>
      )}

      {slots.length === 0 && (
        <span className="field-hint">
          Stack up to {max}. Negative weights push away from a style rather than toward it.
        </span>
      )}
    </div>
  );
}
