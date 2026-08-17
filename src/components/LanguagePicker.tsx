import { useCallback, useEffect, useState } from "react";
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
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    api.translationStatus().then(setStatus).catch(() => setStatus(null));
  }, []);

  useEffect(() => {
    api.translationLanguages().then(setLanguages).catch(() => setLanguages([]));
  }, []);

  useEffect(refresh, [refresh, install]);

  const downloads = jobs.filter((job) => job.id.startsWith(JOB_PREFIX));
  const active = downloads.filter(
    (job) => job.state === "downloading" || job.state === "queued",
  );
  const failed = downloads.filter((job) => job.state === "failed");

  // The queue is the source of truth while a download runs; poll status only
  // to catch the transition to ready.
  useEffect(() => {
    if (active.length === 0) return;
    const timer = setInterval(refresh, 1500);
    return () => clearInterval(timer);
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
      return;
    }

    await saveSettings({ promptLanguage: code, translatePrompts: true });

    if (!ready && !deferInstall) await installTranslation();
  }

  async function installTranslation() {
    setBusy(true);
    setError(null);
    try {
      await api.installTranslation();
      refresh();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    setBusy(true);
    setError(null);
    try {
      await api.removeTranslation();
      await saveSettings({ promptLanguage: null, translatePrompts: false });
      refresh();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="stack">
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
          {languages.map((language) => (
            <option key={language.code} value={language.code}>
              {language.nativeName} — {language.name}
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
        <div className="stack-tight">
          <ProgressBar value={expected > 0 ? downloaded / expected : 0} />
          <p className="section-hint">
            Downloading the {language?.name ?? "translation"} model — {formatBytes(downloaded)}
            {expected > 0 && ` of ${formatBytes(expected)}`}. This happens once for this language.
          </p>
        </div>
      )}

      {!isEnglish && active.length === 0 && ready && (
        <div className="row">
          <Chip tone="success">
            <Check size={13} /> Ready
          </Chip>
          {!compact && status && status.bytesOnDisk > 0 && (
            <span className="section-hint">{formatBytes(status.bytesOnDisk)} on disk</span>
          )}
        </div>
      )}

      {!isEnglish && active.length === 0 && !ready && !busy && (
        deferInstall ? (
          <p className="section-hint">
            The {language?.name ?? "translation"} model — about 300 MB — downloads once Fooocus
            itself is installed, since it runs inside Fooocus's own Python.
          </p>
        ) : (
          <div className="row">
            <button type="button" className="button" onClick={() => void installTranslation()}>
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

      {!compact && ready && (
        <div className="row">
          <button type="button" className="button ghost" disabled={busy} onClick={() => void remove()}>
            <Trash2 size={14} /> Remove model and reclaim space
          </button>
        </div>
      )}

      {!compact && (
        <p className="section-hint">
          <Languages size={13} /> Each language has its own model, of about 300 MB, because a
          model trained on one language translates it far better than a general one — enough
          that a shared model turned a fox into a red thief. Some languages share a model, so
          switching is sometimes free.
        </p>
      )}
    </div>
  );
}
