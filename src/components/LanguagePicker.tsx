import { useCallback, useEffect, useRef, useState } from "react";
import { AlertTriangle, Check, Download, Languages, Trash2 } from "lucide-react";

import {
  api,
  errorMessage,
  type Language,
  type TranslationStatus,
} from "../lib/api";
import { useStore } from "../store";
import { Banner, Chip, ProgressBar } from "./ui";

/** Job ids the translation download uses, so its progress can be picked out
 *  of the shared queue without a second progress system. */
const JOB_PREFIX = "translate:";

function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 MB";
  const mb = bytes / (1024 * 1024);
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.round(mb)} MB`;
}

interface Props {
  /** Setup shows a leaner version: no removal, no size reclaim. */
  compact?: boolean;
  /** Record the choice without downloading anything yet.
   *
   *  Used during Setup, where Fooocus does not exist yet — the runtime is
   *  installed with its bundled Python, so there is nothing to install into
   *  until the main install has finished. Setup starts the download itself
   *  once that is done. */
  deferInstall?: boolean;
}

/**
 * Choosing the language prompts are written in, and installing what that
 * needs.
 *
 * English is the default and costs nothing — no model, no download, no
 * runtime. Everything here only happens once someone picks another language.
 */
export function LanguagePicker({ compact = false, deferInstall = false }: Props) {
  const { settings, saveSettings, jobs, install } = useStore();

  const [languages, setLanguages] = useState<Language[]>([]);
  const [status, setStatus] = useState<TranslationStatus | null>(null);
  /** Codes already downloaded, marked in the dropdown so the ones that cost
   *  nothing are visible without selecting them one at a time. */
  const [installed, setInstalled] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** The download has finished but the status call has not confirmed it yet.
   *  Without this the screen briefly offers to download what just arrived. */
  const [verifying, setVerifying] = useState(false);

  const refresh = useCallback(async () => {
    await Promise.all([
      api.translationStatus().then(setStatus).catch(() => setStatus(null)),
      api.translationInstalled().then(setInstalled).catch(() => setInstalled([])),
    ]);
  }, []);

  useEffect(() => {
    api.translationLanguages().then(setLanguages).catch(() => setLanguages([]));
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh, install]);

  const downloads = jobs.filter((job) => job.id.startsWith(JOB_PREFIX));
  const active = downloads.filter(
    (job) => job.state === "downloading" || job.state === "queued",
  );
  const failed = downloads.filter((job) => job.state === "failed");

  // The queue is the source of truth while a download runs; poll status only
  // to catch the transition to ready.
  //
  // The poll runs faster than it needs to for its own sake. It exists so the
  // moment the last file lands is noticed promptly — the queue reports the job
  // finished before the status call has seen the files on disk, and until it
  // does the screen still offers to download what has just arrived.
  useEffect(() => {
    if (active.length === 0) return;
    const timer = setInterval(refresh, 600);
    return () => clearInterval(timer);
  }, [active.length, refresh]);

  // Refresh once more when the last download finishes.
  //
  // Without this the interval above is torn down on the same render that ends
  // the download, so nothing ever asks again and the screen sat showing a
  // download button for a model that was already on disk.
  const wasDownloading = useRef(false);
  useEffect(() => {
    if (active.length > 0) {
      wasDownloading.current = true;
      return;
    }
    if (wasDownloading.current) {
      wasDownloading.current = false;
      setVerifying(true);
      void refresh().finally(() => setVerifying(false));
    }
  }, [active.length, refresh]);

  const selected = settings?.promptLanguage ?? "";
  const isEnglish = selected === "";
  const ready = status?.modelReady === true && status?.runtimeReady === true;
  const language = languages.find((entry) => entry.code === selected) ?? null;

  const downloaded = downloads.reduce((sum, job) => sum + job.downloaded, 0);
  const expected = downloads.reduce((sum, job) => sum + (job.total ?? 0), 0);

  async function choose(code: string) {
    setError(null);

    // Turning translation off keeps the language, so switching back does not
    // mean picking it again.
    if (code === "") {
      await saveSettings({ promptLanguage: null, translatePrompts: false });
      await refresh();
      return;
    }

    await saveSettings({ promptLanguage: code, translatePrompts: true });

    // Ask about the language just chosen rather than trusting what was on
    // screen, which describes the one before it. Skipping this left an
    // uninstalled language claiming to be ready — and, because the decision
    // below reads the same value, quietly skipped downloading it.
    const next = await api.translationStatus().catch(() => null);
    setStatus(next);
    if (next && next.modelReady && next.runtimeReady) return;

    if (!deferInstall) await installTranslation();
  }

  async function installTranslation() {
    setBusy(true);
    setError(null);
    try {
      await api.installTranslation();
      await refresh();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  }

  /** Remove the selected language's model and fall back to English. */
  async function remove() {
    if (!selected) return;

    setBusy(true);
    setError(null);
    try {
      await api.removeTranslation(selected);
      await saveSettings({ promptLanguage: null, translatePrompts: false });
      await refresh();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    // Inline layout to match the rest of the screens, which do the same rather
    // than carrying utility classes the stylesheet does not define.
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <div className="field">
        <label className="field-label" htmlFor="prompt-language">
          Language you write prompts in
        </label>
        <select
          id="prompt-language"
          className="input"
          value={selected}
          disabled={busy}
          onChange={(event) => void choose(event.target.value)}
        >
          <option value="">English — no translation needed</option>
          {languages.map((entry) => (
            <option key={entry.code} value={entry.code}>
              {installed.includes(entry.code) ? "✓ " : ""}
              {entry.nativeName} — {entry.name}
            </option>
          ))}
        </select>
      </div>

      {isEnglish ? (
        <p className="section-hint">
          Prompts go to Fooocus exactly as you type them. Nothing is downloaded.
        </p>
      ) : (
        <p className="section-hint">
          Your prompt is translated into English before generating, and the English is shown
          alongside it so you can see what was actually sent. Stable Diffusion understands
          English far better than other languages, which is why this matters more than
          translating the buttons.
        </p>
      )}

      {!isEnglish && language && language.quality !== "dedicated" && (
        <Banner tone="warning" icon={<AlertTriangle size={15} />}>
          {language.quality === "family" ? (
            <>
              {language.name} has no model of its own, so it is translated by a
              wider {language.model.replace("opus-mt-", "").replace("-en", "")} model. It works,
              but expect it to be a little rougher than the languages that have a dedicated one.
            </>
          ) : (
            <>
              {language.name} is only covered by the general hundred-language model, which is
              markedly weaker — it can drop or mistake words. Check the English shown alongside
              your prompt before relying on it.
            </>
          )}
        </Banner>
      )}

      {!isEnglish && active.length > 0 && (
        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          <ProgressBar value={expected > 0 ? downloaded / expected : 0} />
          <p className="section-hint">
            Downloading the {language?.name ?? "translation"} model — {formatBytes(downloaded)}
            {expected > 0 && ` of ${formatBytes(expected)}`}. This happens once for this language.
          </p>
        </div>
      )}

      {!isEnglish && active.length === 0 && ready && (
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <Chip tone="success">
            <Check size={13} /> Ready
          </Chip>
          {!compact && status && status.bytesOnDisk > 0 && (
            <span className="section-hint">{formatBytes(status.bytesOnDisk)} on disk</span>
          )}
        </div>
      )}

      {!isEnglish && verifying && <p className="section-hint">Finishing…</p>}

      {!isEnglish && active.length === 0 && !ready && !busy && !verifying && (
        deferInstall ? (
          <p className="section-hint">
            The {language?.name ?? "translation"} model — about 300 MB — downloads once Fooocus
            itself is installed, since it runs inside Fooocus's own Python.
          </p>
        ) : (
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <button type="button" className="btn btn-primary" onClick={() => void installTranslation()}>
              <Download size={14} /> Download {language?.name ?? "translation"} model
            </button>
            <span className="section-hint">About 300 MB, once, for {language?.name ?? "this language"}.</span>
          </div>
        )
      )}

      {busy && <p className="section-hint">Preparing the translation runtime…</p>}

      {failed.length > 0 && (
        <Banner tone="danger" icon={<AlertTriangle size={15} />}>
          {failed.length === 1
            ? "Part of the translation model failed to download."
            : `${failed.length} parts of the translation model failed to download.`}{" "}
          Retry them from the Downloads screen.
        </Banner>
      )}

      {error && (
        <Banner tone="danger" icon={<AlertTriangle size={15} />}>
          {error}
        </Banner>
      )}

      {/* Only for a language that is actually installed. On English there is
          nothing to remove, and offering it there was just clutter. */}
      {!compact && !isEnglish && ready && active.length === 0 && (
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <button type="button" className="btn btn-ghost btn-sm" disabled={busy} onClick={() => void remove()}>
            <Trash2 size={14} /> Uninstall {language?.name ?? "language"}
          </button>
        </div>
      )}

      {!compact && isEnglish && (
        <p className="section-hint">
          <Languages size={13} /> Languages are downloaded one at a time, about 300 MB each, and
          only when you pick one.
        </p>
      )}
    </div>
  );
}
