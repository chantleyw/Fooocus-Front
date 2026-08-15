import { useMemo, useState } from "react";
import {
  Check,
  ChevronDown,
  ChevronRight,
  Download,
  FolderOpen,
  Package,
  RefreshCw,
  Search,
} from "lucide-react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

import { api, errorMessage, type CatalogEntry } from "../lib/api";
import { formatBytes } from "../lib/format";
import { useStore } from "../store";
import { Banner, Chip, EmptyState, ScreenHeader } from "../components/ui";

type Tab = "installed" | "available";

export function Models() {
  const { models, catalog, jobs, refreshModels, setScreen } = useStore();

  const [tab, setTab] = useState<Tab>("installed");
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useState<Record<string, boolean>>({ checkpoints: true });
  const [error, setError] = useState<string | null>(null);

  const installedCount = models.reduce((sum, category) => sum + category.files.length, 0);
  const installedSize = models.reduce((sum, category) => sum + category.totalSize, 0);
  const missing = catalog.filter((entry) => !entry.installed);

  const search = query.trim().toLowerCase();

  const filteredCategories = useMemo(
    () =>
      models
        .map((category) => ({
          ...category,
          files: search
            ? category.files.filter((file) => file.name.toLowerCase().includes(search))
            : category.files,
        }))
        .filter((category) => category.files.length > 0 || !search),
    [models, search],
  );

  const filteredCatalog = useMemo(() => {
    const matches = search
      ? catalog.filter(
          (entry) =>
            entry.name.toLowerCase().includes(search) ||
            entry.filename.toLowerCase().includes(search) ||
            entry.description.toLowerCase().includes(search) ||
            entry.tags.some((tag) => tag.toLowerCase().includes(search)),
        )
      : catalog;

    // Missing essentials first, then the rest of the missing, then installed.
    return [...matches].sort((a, b) => {
      const rank = (entry: CatalogEntry) =>
        entry.installed ? 2 : entry.essential ? 0 : 1;
      return rank(a) - rank(b) || a.name.localeCompare(b.name);
    });
  }, [catalog, search]);

  async function download(id: string) {
    setError(null);
    try {
      await api.startDownload(id);
      setScreen("downloads");
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  return (
    <div className="screen">
      <ScreenHeader
        title="Models"
        subtitle={`${installedCount} files installed · ${formatBytes(installedSize)} on disk${
          missing.length ? ` · ${missing.length} available to download` : ""
        }`}
        actions={
          <button className="btn" onClick={() => void refreshModels()}>
            <RefreshCw size={15} />
            Rescan
          </button>
        }
      />

      <div className="screen-body">
        {error && (
          <div style={{ marginBottom: 14 }}>
            <Banner tone="danger">{error}</Banner>
          </div>
        )}

        <div className="toolbar">
          <div className="filter-group">
            <button
              className={`filter-btn${tab === "installed" ? " active" : ""}`}
              onClick={() => setTab("installed")}
            >
              Installed
            </button>
            <button
              className={`filter-btn${tab === "available" ? " active" : ""}`}
              onClick={() => setTab("available")}
            >
              Available
            </button>
          </div>

          <div className="search">
            <Search size={15} />
            <input
              className="input"
              placeholder={tab === "installed" ? "Search your models" : "Search the catalog"}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>
        </div>

        {tab === "installed" ? (
          installedCount === 0 ? (
            <EmptyState
              icon={<Package size={22} />}
              title="No model files found"
              action={
                <button className="btn btn-primary" onClick={() => setTab("available")}>
                  <Download size={15} />
                  Browse available models
                </button>
              }
            >
              Nothing was found in the folders your <span className="mono">config.txt</span> points
              at.
            </EmptyState>
          ) : (
            filteredCategories.map((category) => {
              const open = expanded[category.id] ?? false;
              return (
                <div className="category" key={category.id}>
                  <button
                    className="category-header"
                    onClick={() =>
                      setExpanded((current) => ({ ...current, [category.id]: !open }))
                    }
                  >
                    {open ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
                    <span style={{ minWidth: 0 }}>
                      <span className="category-title" style={{ display: "block" }}>
                        {category.label}
                      </span>
                      <span className="category-desc" style={{ display: "block" }}>
                        {category.description}
                      </span>
                    </span>
                    <span className="category-meta">
                      {category.files.length > 0 && (
                        <Chip>{formatBytes(category.totalSize)}</Chip>
                      )}
                      <Chip tone={category.files.length ? "accent" : "default"}>
                        {category.files.length} {category.files.length === 1 ? "file" : "files"}
                      </Chip>
                    </span>
                  </button>

                  {open && (
                    <div className="category-body">
                      {category.files.length === 0 ? (
                        <p className="field-hint" style={{ padding: "10px 0" }}>
                          Empty. Fooocus will download what it needs here on demand, or you can
                          fetch it from the Available tab.
                        </p>
                      ) : (
                        category.files.map((file) => (
                          <div className="file-row" key={file.path}>
                            <div style={{ flex: 1, minWidth: 0 }}>
                              <div className="file-name truncate">{file.name}</div>
                              <div className="file-path truncate">{file.path}</div>
                            </div>
                            <Chip>{formatBytes(file.size)}</Chip>
                            <button
                              className="btn btn-ghost btn-icon"
                              title="Show in File Explorer"
                              onClick={() => void revealItemInDir(file.path)}
                            >
                              <FolderOpen size={15} />
                            </button>
                          </div>
                        ))
                      )}
                    </div>
                  )}
                </div>
              );
            })
          )
        ) : (
          <>
            <p className="section-hint" style={{ marginBottom: 14 }}>
              Every file here is one Fooocus would otherwise fetch silently mid-generation. The
              links come from your install's own <span className="mono">config.py</span> and
              presets, so they always match your version.
            </p>

            <div className="catalog-grid">
              {filteredCatalog.map((entry) => {
                const job = jobs.find((candidate) => candidate.id === entry.id);
                const active = job?.state === "downloading" || job?.state === "queued";

                return (
                  <article
                    className={`catalog-card${entry.installed ? " installed" : ""}`}
                    key={entry.id}
                  >
                    <div className="catalog-head">
                      <div style={{ minWidth: 0 }}>
                        <div className="catalog-name">{entry.name}</div>
                        <div className="catalog-file mono">{entry.filename}</div>
                      </div>
                      {entry.installed ? (
                        <Chip tone="success">
                          <Check size={12} />
                          Installed
                        </Chip>
                      ) : entry.essential ? (
                        <Chip tone="accent">Essential</Chip>
                      ) : null}
                    </div>

                    <p className="catalog-desc">{entry.description}</p>

                    <div className="catalog-tags">
                      {entry.tags.map((tag) => (
                        <Chip key={tag}>{tag}</Chip>
                      ))}
                    </div>

                    <div className="catalog-foot">
                      <span style={{ fontSize: 12, color: "var(--text-muted)" }}>
                        {entry.installed
                          ? formatBytes(entry.installedSize)
                          : active
                            ? "Downloading…"
                            : "Size shown once started"}
                      </span>

                      {entry.installed ? (
                        <button
                          className="btn btn-ghost btn-sm"
                          onClick={() => void revealItemInDir(entry.targetPath)}
                        >
                          <FolderOpen size={14} />
                          Show
                        </button>
                      ) : (
                        <button
                          className="btn btn-primary btn-sm"
                          disabled={active}
                          onClick={() => void download(entry.id)}
                        >
                          <Download size={14} />
                          {active ? "Queued" : "Download"}
                        </button>
                      )}
                    </div>
                  </article>
                );
              })}
            </div>

            {filteredCatalog.length === 0 && (
              <EmptyState icon={<Search size={22} />} title="Nothing matches that search" />
            )}
          </>
        )}
      </div>
    </div>
  );
}
