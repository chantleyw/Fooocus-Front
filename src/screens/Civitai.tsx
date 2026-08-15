import { useCallback, useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  Check,
  Download,
  ExternalLink,
  Eye,
  EyeOff,
  Key,
  Search,
  SlidersHorizontal,
  ThumbsUp,
  X,
} from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";

import { api, errorMessage, type CivitaiModel, type CivitaiVersion } from "../lib/api";
import { formatBytes, formatSpeed } from "../lib/format";
import { useStore } from "../store";
import { Banner, Chip, EmptyState, ProgressBar } from "../components/ui";

/** Civitai's type values, limited to the ones Fooocus can use. */
const TYPES = [
  { id: "", label: "Everything" },
  { id: "Checkpoint", label: "Checkpoints" },
  { id: "LORA", label: "LoRAs" },
  { id: "TextualInversion", label: "Embeddings" },
  { id: "VAE", label: "VAE" },
];

const SORTS = ["Highest Rated", "Most Downloaded", "Newest"];

/** The topics Civitai offers as one-click toggles on their own site. */
const PRESET_TAGS = ["anime", "furry", "gore", "political"];

export function Civitai() {
  const { jobs, refreshModels } = useStore();

  const [query, setQuery] = useState("");
  const [type, setType] = useState("");
  const [sort, setSort] = useState(SORTS[0]);
  const [allBaseModels, setAllBaseModels] = useState(false);
  const [nsfw, setNsfw] = useState(false);

  const [items, setItems] = useState<CivitaiModel[]>([]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [hasKey, setHasKey] = useState(true);
  const [keyInput, setKeyInput] = useState("");
  const [keyBusy, setKeyBusy] = useState(false);
  const [keyError, setKeyError] = useState<string | null>(null);

  /** Chosen version per model, defaulting to the newest compatible one. */
  const [picked, setPicked] = useState<Record<number, number>>({});

  // Content controls, mirroring Civitai's own. Persisted so they stick.
  const [hiddenTags, setHiddenTags] = useState<string[]>([]);
  const [showControls, setShowControls] = useState(false);
  const [tagInput, setTagInput] = useState("");

  useEffect(() => {
    api.civitaiHasKey().then(setHasKey).catch(() => setHasKey(false));
    api.civitaiHiddenTags().then(setHiddenTags).catch(() => setHiddenTags([]));
  }, []);

  function updateTags(next: string[]) {
    setHiddenTags(next);
    void api.civitaiSetHiddenTags(next);
  }

  function toggleTag(tag: string) {
    updateTags(
      hiddenTags.includes(tag)
        ? hiddenTags.filter((t) => t !== tag)
        : [...hiddenTags, tag],
    );
  }

  const load = useCallback(
    async (append: string | null = null) => {
      setLoading(true);
      setError(null);
      try {
        const results = await api.civitaiSearch({
          query,
          types: type || undefined,
          sort,
          allBaseModels,
          nsfw,
          hiddenTags,
          cursor: append ?? undefined,
        });

        setItems((current) => (append ? [...current, ...results.items] : results.items));
        setCursor(results.nextCursor);
      } catch (err) {
        setError(errorMessage(err));
      } finally {
        setLoading(false);
      }
    },
    [query, type, sort, allBaseModels, nsfw, hiddenTags],
  );

  // Debounce so typing a search does not fire a request per keystroke.
  const timer = useRef<number | null>(null);
  useEffect(() => {
    if (timer.current) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => void load(), 350);
    return () => {
      if (timer.current) window.clearTimeout(timer.current);
    };
  }, [load]);

  async function saveKey() {
    setKeyBusy(true);
    setKeyError(null);
    try {
      const ok = await api.civitaiSetKey(keyInput);
      if (ok) {
        setHasKey(keyInput.trim().length > 0);
        setKeyInput("");
      } else {
        setKeyError("Civitai rejected that key. Check you copied all of it.");
      }
    } catch (err) {
      setKeyError(errorMessage(err));
    } finally {
      setKeyBusy(false);
    }
  }

  async function download(model: CivitaiModel, version: CivitaiVersion) {
    if (!version.file || !model.category) return;

    setError(null);
    try {
      await api.civitaiDownload({
        versionId: version.id,
        name: `${model.name} · ${version.name}`,
        filename: version.file.name,
        category: model.category,
        url: version.file.downloadUrl,
      });
      void refreshModels();
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  return (
    <div className="screen-body">
      {!hasKey && (
        <div className="card" style={{ marginBottom: 16 }}>
          <div style={{ display: "flex", gap: 11, alignItems: "flex-start" }}>
            <Key size={17} style={{ marginTop: 2, color: "var(--accent)" }} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div className="field-label">A Civitai API key is needed to download</div>
              <p className="field-hint" style={{ marginTop: 3 }}>
                Browsing works without one. Downloads do not — Civitai returns 401 without a key.
                It is free: create one under Account settings → API Keys, then paste it here. It is
                stored only on this machine.
              </p>

              <div style={{ display: "flex", gap: 8, marginTop: 11 }}>
                <input
                  className="input mono"
                  type="password"
                  placeholder="Paste your API key"
                  value={keyInput}
                  onChange={(event) => setKeyInput(event.target.value)}
                />
                <button
                  className="btn btn-primary"
                  onClick={() => void saveKey()}
                  disabled={keyBusy || !keyInput.trim()}
                >
                  {keyBusy ? "Checking…" : "Save"}
                </button>
                <button
                  className="btn"
                  onClick={() => void openUrl("https://civitai.com/user/account")}
                  title="Open Civitai account settings"
                >
                  <ExternalLink size={15} />
                </button>
              </div>

              {keyError && (
                <div style={{ marginTop: 10 }}>
                  <Banner tone="danger">{keyError}</Banner>
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      <div className="toolbar">
        <div className="search">
          <Search size={15} />
          <input
            className="input"
            placeholder="Search Civitai"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </div>

        <select className="select" style={{ width: "auto" }} value={type}
          onChange={(event) => setType(event.target.value)}>
          {TYPES.map((entry) => (
            <option key={entry.id} value={entry.id}>
              {entry.label}
            </option>
          ))}
        </select>

        <select className="select" style={{ width: "auto" }} value={sort}
          onChange={(event) => setSort(event.target.value)}>
          {SORTS.map((name) => (
            <option key={name} value={name}>
              {name}
            </option>
          ))}
        </select>

        <button
          className={`btn btn-sm${allBaseModels ? "" : " btn-primary"}`}
          onClick={() => setAllBaseModels((v) => !v)}
          title="Fooocus only runs SDXL-based models"
        >
          {allBaseModels ? "All base models" : "SDXL only"}
        </button>

        <button
          className={`btn btn-sm${nsfw ? " btn-danger" : ""}`}
          onClick={() => setNsfw((v) => !v)}
          title="Show adult content"
        >
          {nsfw ? <Eye size={14} /> : <EyeOff size={14} />}
          {nsfw ? "NSFW shown" : "NSFW hidden"}
        </button>

        <button
          className={`btn btn-sm${hiddenTags.length ? " btn-primary" : ""}`}
          onClick={() => setShowControls((v) => !v)}
          title="Hide topics you would rather not see"
        >
          <SlidersHorizontal size={14} />
          Content
          {hiddenTags.length > 0 && ` (${hiddenTags.length})`}
        </button>
      </div>

      {showControls && (
        <div className="card" style={{ marginBottom: 16 }}>
          <div className="field-label">Content controls</div>
          <p className="field-hint" style={{ marginTop: 3, marginBottom: 12 }}>
            Hide models tagged with these topics. Civitai's tags are applied by
            uploaders, so this reduces rather than eliminates what gets through.
          </p>

          <div className="content-toggles">
            {PRESET_TAGS.map((tag) => (
              <label className="switch" key={tag}>
                <input
                  type="checkbox"
                  checked={hiddenTags.includes(tag)}
                  onChange={() => toggleTag(tag)}
                />
                <span style={{ textTransform: "capitalize" }}>Hide {tag}</span>
              </label>
            ))}
          </div>

          <div className="field" style={{ marginTop: 14 }}>
            <label className="field-label" htmlFor="hide-tag">
              Hidden tags
            </label>
            <form
              style={{ display: "flex", gap: 8 }}
              onSubmit={(event) => {
                event.preventDefault();
                const tag = tagInput.trim().toLowerCase();
                if (tag && !hiddenTags.includes(tag)) updateTags([...hiddenTags, tag]);
                setTagInput("");
              }}
            >
              <input
                id="hide-tag"
                className="input"
                placeholder="Add a tag to hide, e.g. horror"
                value={tagInput}
                onChange={(event) => setTagInput(event.target.value)}
              />
              <button className="btn" type="submit" disabled={!tagInput.trim()}>
                Add
              </button>
            </form>
          </div>

          {hiddenTags.length > 0 && (
            <div className="catalog-tags" style={{ marginTop: 12 }}>
              {hiddenTags.map((tag) => (
                <button key={tag} className="chip removable" onClick={() => toggleTag(tag)}>
                  {tag}
                  <X size={11} />
                </button>
              ))}
            </div>
          )}
        </div>
      )}

      {allBaseModels && (
        <div style={{ marginBottom: 14 }}>
          <Banner tone="warning" icon={<AlertTriangle size={15} />}>
            Showing every base model. Fooocus can only run SDXL-based ones — anything else will
            download fine and then fail to load. Incompatible versions are marked.
          </Banner>
        </div>
      )}

      {error && (
        <div style={{ marginBottom: 14 }}>
          <Banner tone="danger">{error}</Banner>
        </div>
      )}

      <div className="catalog-grid">
        {items.map((model) => {
          const versionId = picked[model.id] ?? model.versions[0]?.id;
          const version = model.versions.find((v) => v.id === versionId) ?? model.versions[0];
          const job = jobs.find((j) => j.id === `civitai-${version?.id}`);
          const active = job?.state === "downloading" || job?.state === "queued";
          const done = job?.state === "completed";

          return (
            <article className="catalog-card" key={model.id}>
              {version?.image && (
                <img className="civitai-preview" src={version.image} alt="" loading="lazy" />
              )}

              <div className="catalog-head">
                <div style={{ minWidth: 0 }}>
                  <div className="catalog-name">{model.name}</div>
                  <div className="catalog-file">
                    {model.kind}
                    {model.creator && ` · by ${model.creator}`}
                  </div>
                </div>
                {version?.installed && (
                  <Chip tone="success">
                    <Check size={12} />
                    Installed
                  </Chip>
                )}
                {model.nsfw && <Chip tone="danger">NSFW</Chip>}
              </div>

              <div className="catalog-tags">
                <Chip tone={version?.compatible ? "success" : "warning"}>
                  {version?.baseModel ?? "Unknown"}
                </Chip>
                <Chip>
                  <Download size={11} />
                  {model.downloads.toLocaleString()}
                </Chip>
                <Chip>
                  <ThumbsUp size={11} />
                  {model.thumbsUp.toLocaleString()}
                </Chip>
                {version?.file && <Chip>{formatBytes(version.file.sizeKb * 1024)}</Chip>}
              </div>

              {model.versions.length > 1 && (
                <select
                  className="select"
                  value={versionId}
                  onChange={(event) =>
                    setPicked((current) => ({
                      ...current,
                      [model.id]: Number(event.target.value),
                    }))
                  }
                >
                  {model.versions.map((v) => (
                    <option key={v.id} value={v.id}>
                      {v.name} — {v.baseModel}
                      {v.compatible ? "" : " (incompatible)"}
                    </option>
                  ))}
                </select>
              )}

              {version && !version.installed &&
                model.versions.some((v) => v.installed) && (
                  <p className="field-hint">
                    You already have another version of this model.
                  </p>
                )}

              {version && !version.compatible && (
                <p className="field-hint" style={{ color: "var(--warning)" }}>
                  {version.baseModel} is not SDXL-based, so Fooocus cannot load it.
                </p>
              )}

              {!model.category && (
                <p className="field-hint" style={{ color: "var(--warning)" }}>
                  Fooocus has no folder for {model.kind} files.
                </p>
              )}

              {job && active && (
                <div>
                  <ProgressBar
                    value={job.total ? job.downloaded / job.total : 0}
                    indeterminate={job.state === "queued" || !job.total}
                  />
                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      marginTop: 5,
                      fontSize: 11.5,
                      color: "var(--text-muted)",
                    }}
                  >
                    <span>
                      {job.state === "queued"
                        ? "Queued"
                        : `${formatBytes(job.downloaded)}${
                            job.total ? ` of ${formatBytes(job.total)}` : ""
                          }`}
                    </span>
                    {job.speed > 0 && <span>{formatSpeed(job.speed)}</span>}
                  </div>
                </div>
              )}

              <div className="catalog-foot">
                <button
                  className="btn btn-ghost btn-sm"
                  onClick={() => void openUrl(`https://civitai.com/models/${model.id}`)}
                  title="Open on Civitai"
                >
                  <ExternalLink size={14} />
                  View
                </button>

                {version?.installed || done ? (
                  <Chip tone="success">
                    <Check size={12} />
                    {version?.installed && !done ? "Already installed" : "Downloaded"}
                  </Chip>
                ) : active ? (
                  <button
                    className="btn btn-sm"
                    onClick={() => void api.cancelDownload(job!.id)}
                  >
                    <X size={14} />
                    Cancel
                  </button>
                ) : (
                  <button
                    className="btn btn-primary btn-sm"
                    disabled={!version?.file || !model.category || !version.compatible}
                    onClick={() => version && void download(model, version)}
                    title={
                      !model.category
                        ? "Fooocus cannot use this type of file"
                        : !version?.compatible
                          ? "Not an SDXL model"
                          : undefined
                    }
                  >
                    <Download size={14} />
                    Download
                  </button>
                )}
              </div>
            </article>
          );
        })}
      </div>

      {loading && (
        <EmptyState icon={<Search size={22} />} title="Searching Civitai…" />
      )}

      {!loading && items.length === 0 && (
        <EmptyState icon={<Search size={22} />} title="Nothing found">
          Try a different search, or switch off the SDXL-only filter to see everything.
        </EmptyState>
      )}

      {!loading && cursor && (
        <div style={{ display: "flex", justifyContent: "center", marginTop: 18 }}>
          <button className="btn" onClick={() => void load(cursor)}>
            Load more
          </button>
        </div>
      )}
    </div>
  );
}
