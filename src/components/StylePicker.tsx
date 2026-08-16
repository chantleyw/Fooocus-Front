import { useMemo, useState } from "react";
import { Check, Search, X } from "lucide-react";

import { matches } from "../lib/search";
import { Chip } from "./ui";

/**
 * Fooocus ships 279 styles. Presented as a flat list they are unusable, so the
 * ones that actually change how the app behaves are pulled to the top and the
 * rest are searchable.
 */
const FOOOCUS_PREFIX = "Fooocus";

export function StylePicker({
  all,
  selected,
  onChange,
}: {
  all: string[];
  selected: string[];
  onChange: (next: string[]) => void;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");

  // Fooocus's own styles are the ones with real behavioural effect (V2 rewrites
  // the prompt, Enhance and Sharp change sampling), so they lead.
  const ordered = useMemo(() => {
    const own = all.filter((name) => name.startsWith(FOOOCUS_PREFIX));
    const rest = all.filter((name) => !name.startsWith(FOOOCUS_PREFIX));
    return [...own, ...rest];
  }, [all]);

  const visible = useMemo(
    () => (query.trim() ? ordered.filter((name) => matches(name, query)) : ordered),
    [ordered, query],
  );

  function toggle(name: string) {
    onChange(
      selected.includes(name)
        ? selected.filter((entry) => entry !== name)
        : [...selected, name],
    );
  }

  return (
    <div className="field">
      <label className="field-label">
        Styles
        {selected.length > 0 && (
          <span style={{ fontWeight: 400, color: "var(--text-faint)" }}>
            {" "}
            · {selected.length} selected
          </span>
        )}
      </label>

      {selected.length > 0 && (
        <div className="catalog-tags" style={{ marginBottom: 2 }}>
          {selected.map((name) => (
            <button key={name} className="chip removable" onClick={() => toggle(name)}>
              {name}
              <X size={11} />
            </button>
          ))}
        </div>
      )}

      <button className="btn btn-sm" onClick={() => setOpen((value) => !value)}>
        {open ? "Done" : selected.length ? "Change styles" : "Choose styles"}
      </button>

      {open && (
        <div className="style-picker">
          <div className="search" style={{ marginBottom: 8 }}>
            <Search size={14} />
            <input
              className="input"
              placeholder={`Search ${all.length} styles`}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              autoFocus
            />
          </div>

          <div className="style-list">
            {visible.map((name) => {
              const on = selected.includes(name);
              return (
                <button
                  key={name}
                  className={`style-row${on ? " selected" : ""}`}
                  onClick={() => toggle(name)}
                >
                  <span className="style-check">{on && <Check size={12} />}</span>
                  <span className="truncate">{name}</span>
                </button>
              );
            })}

            {visible.length === 0 && (
              <p className="field-hint" style={{ padding: "10px 4px" }}>
                Nothing matches that.
              </p>
            )}
          </div>

          {selected.length > 0 && (
            <button
              className="btn btn-ghost btn-sm"
              style={{ marginTop: 8 }}
              onClick={() => onChange([])}
            >
              Clear all
            </button>
          )}
        </div>
      )}

      {selected.length === 0 && (
        <span className="field-hint">
          No styles applied. <Chip>Fooocus V2</Chip> rewrites short prompts into richer ones.
        </span>
      )}
    </div>
  );
}
