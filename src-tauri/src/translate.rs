//! Prompt translation.
//!
//! SDXL understands English far better than anything else, so a non-English
//! speaker typing in their own language gets noticeably worse images rather
//! than an error. Translating the prompt on the way through is what actually
//! closes that gap; translating the interface alone would not.
//!
//! One model is downloaded for the language the user picks, from the Helsinki
//! OPUS-MT family (Apache-2.0), at roughly 300 MB. Nothing ships in the
//! installer, and an English user downloads nothing at all.
//!
//! # Why one model per language rather than one for all of them
//!
//! `opus-mt-mul-en` translates a hundred languages with a single download,
//! which looks like the obvious choice for an app that wants to work in any
//! language without an unbounded disk bill. It was measured against the
//! dedicated pairs and lost on every axis that matters:
//!
//! - **Accuracy.** On the same prompt it dropped "snowy" from a German forest,
//!   turned a fox into "the Red Cross" in Chinese and "a red thief" in Hindi,
//!   hallucinated freely in Vietnamese, and emitted nothing but full stops in
//!   Korean. Around half the languages were unusable.
//! - **Speed.** Measured cold, one call each: 567 ms against 283 ms for
//!   German, and 2140 ms against 277 ms for Korean — the catch-all was slower
//!   precisely where it was worst, because it kept generating rubbish tokens.
//! - **Size.** `mul-en` is 298.8 MB, `opus-mt-de-en` is 286.8 MB. There is no
//!   saving, because someone writing prompts in German downloads exactly one
//!   model either way.
//!
//! Warm, a dedicated pair translates a prompt in 260–340 ms, with a one-off
//! cost of about four seconds the first time the model is loaded.
//!
//! A single multilingual model only wins for a user who needs several
//! languages at once, which is not what writing prompts looks like. So the
//! language chosen at setup decides the model, and `mul-en` survives only
//! where nothing better exists.
//!
//! Translation runs inside the Fooocus process via the bridge, pinned to the
//! CPU so it never competes with SDXL for VRAM. See `fooocus_bridge.py`.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::error::{AppError, Result};

const OWNER: &str = "Helsinki-NLP";

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Files the Marian model and its tokenizer actually need.
///
/// These repositories also carry `tf_model.h5`, a TensorFlow copy of the same
/// weights. Fetching it would double the transfer for nothing, so the list is
/// explicit rather than "everything in the repo".
const FILES: &[&str] = &[
    "config.json",
    "generation_config.json",
    "tokenizer_config.json",
    "vocab.json",
    "source.spm",
    "target.spm",
    "pytorch_model.bin",
];

/// Packages the Fooocus environment is missing.
///
/// Marian's tokenizer is SentencePiece-backed, and transformers warns that
/// sacremoses is wanted for Marian normalisation. Everything else — torch,
/// transformers, tokenizers, numpy, protobuf — is already present.
const VENDOR_PACKAGES: &[&str] = &["sentencepiece", "sacremoses"];

/// How well the assigned model handles this language.
///
/// Carried through to the UI so a user on a weaker model is told, rather than
/// left to conclude the app is bad at their language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    /// A model trained on exactly this pair. The good case.
    Dedicated,
    /// A language-family model. Clearly better than the catch-all, a little
    /// behind a dedicated pair.
    Family,
    /// The hundred-language catch-all, used only where nothing else exists.
    Broad,
}

/// (code, English name, native name, model, quality)
///
/// Native names are shown first in the picker, so someone who cannot read the
/// English name can still find their own language.
pub const LANGUAGES: &[(&str, &str, &str, &str, Quality)] = &[
    ("ar", "Arabic", "العربية", "opus-mt-ar-en", Quality::Dedicated),
    ("bg", "Bulgarian", "български", "opus-mt-bg-en", Quality::Dedicated),
    ("cs", "Czech", "čeština", "opus-mt-cs-en", Quality::Dedicated),
    ("da", "Danish", "dansk", "opus-mt-da-en", Quality::Dedicated),
    ("de", "German", "Deutsch", "opus-mt-de-en", Quality::Dedicated),
    ("el", "Greek", "Ελληνικά", "opus-mt-grk-en", Quality::Family),
    ("es", "Spanish", "español", "opus-mt-es-en", Quality::Dedicated),
    ("et", "Estonian", "eesti", "opus-mt-et-en", Quality::Dedicated),
    ("fa", "Persian", "فارسی", "opus-mt-mul-en", Quality::Broad),
    ("fi", "Finnish", "suomi", "opus-mt-fi-en", Quality::Dedicated),
    ("fr", "French", "français", "opus-mt-fr-en", Quality::Dedicated),
    ("he", "Hebrew", "עברית", "opus-mt-sem-en", Quality::Family),
    ("hi", "Hindi", "हिन्दी", "opus-mt-hi-en", Quality::Dedicated),
    ("hu", "Hungarian", "magyar", "opus-mt-hu-en", Quality::Dedicated),
    ("id", "Indonesian", "Bahasa Indonesia", "opus-mt-id-en", Quality::Dedicated),
    ("it", "Italian", "italiano", "opus-mt-it-en", Quality::Dedicated),
    ("ja", "Japanese", "日本語", "opus-mt-ja-en", Quality::Dedicated),
    ("ko", "Korean", "한국어", "opus-mt-ko-en", Quality::Dedicated),
    ("nl", "Dutch", "Nederlands", "opus-mt-nl-en", Quality::Dedicated),
    ("no", "Norwegian", "norsk", "opus-mt-gem-en", Quality::Family),
    ("pl", "Polish", "polski", "opus-mt-pl-en", Quality::Dedicated),
    ("pt", "Portuguese", "português", "opus-mt-roa-en", Quality::Family),
    ("ro", "Romanian", "română", "opus-mt-roa-en", Quality::Family),
    ("ru", "Russian", "русский", "opus-mt-ru-en", Quality::Dedicated),
    ("sv", "Swedish", "svenska", "opus-mt-sv-en", Quality::Dedicated),
    ("th", "Thai", "ไทย", "opus-mt-th-en", Quality::Dedicated),
    ("tr", "Turkish", "Türkçe", "opus-mt-tr-en", Quality::Dedicated),
    ("uk", "Ukrainian", "українська", "opus-mt-uk-en", Quality::Dedicated),
    ("vi", "Vietnamese", "Tiếng Việt", "opus-mt-vi-en", Quality::Dedicated),
    ("zh", "Chinese", "中文", "opus-mt-zh-en", Quality::Dedicated),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Language {
    pub code: String,
    pub name: String,
    pub native_name: String,
    pub model: String,
    pub quality: Quality,
}

pub fn languages() -> Vec<Language> {
    LANGUAGES
        .iter()
        .map(|(code, name, native, model, quality)| Language {
            code: (*code).to_string(),
            name: (*name).to_string(),
            native_name: (*native).to_string(),
            model: (*model).to_string(),
            quality: *quality,
        })
        .collect()
}

fn entry(code: &str) -> Option<&'static (&'static str, &'static str, &'static str, &'static str, Quality)> {
    LANGUAGES.iter().find(|(c, _, _, _, _)| *c == code)
}

/// Whether a language code is one we offer. Guards against a stale settings
/// file naming a language a later build no longer lists.
pub fn is_supported(code: &str) -> bool {
    entry(code).is_some()
}

/// The model a language is translated by. Several languages share one — both
/// Portuguese and Romanian use the Romance model — so a user switching between
/// them downloads nothing the second time.
pub fn model_for(code: &str) -> Option<&'static str> {
    entry(code).map(|(_, _, _, model, _)| *model)
}

pub fn quality_for(code: &str) -> Option<Quality> {
    entry(code).map(|(_, _, _, _, quality)| *quality)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Every file of the active language's model present on disk.
    pub model_ready: bool,
    /// Vendored Python packages available.
    pub runtime_ready: bool,
    /// Bytes used by every model on disk, so the UI can offer to reclaim them.
    pub bytes_on_disk: u64,
    /// Files still missing for the active language, for a precise message
    /// rather than a bare "not installed".
    pub missing: Vec<String>,
    /// The language prompts should currently be translated from, or `None`
    /// when nothing should happen. Decided here rather than in the UI so
    /// there is one answer to "is translation on", not two that can disagree.
    pub active_language: Option<String>,
    /// The model serving the active language.
    pub active_model: Option<String>,
    /// How good that pairing is, so the UI can warn where it is not ideal.
    pub active_quality: Option<Quality>,
}

fn translate_root(app: &AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::msg(e.to_string()))?
        .join("translate");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Where a language's model lives: our data directory, keyed by model name so
/// languages sharing a model share the download. Never the Fooocus folder.
pub fn model_dir(app: &AppHandle, code: &str) -> Result<PathBuf> {
    let model = model_for(code)
        .ok_or_else(|| AppError::msg(format!("no translation model for language '{code}'")))?;
    let dir = translate_root(app)?.join(model);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Where vendored Python packages go.
///
/// Deliberately separate from the Fooocus environment. Its package set is
/// pinned and fragile — Gradio in particular — so we add to `sys.path` at
/// runtime rather than installing anything into it.
pub fn vendor_dir(app: &AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::msg(e.to_string()))?
        .join("vendor");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn file_url(model: &str, file: &str) -> String {
    format!("https://huggingface.co/{OWNER}/{model}/resolve/main/{file}")
}

/// A partially-downloaded file leaves a `.part` behind, which must not count
/// as present — the queue renames it only on completion.
fn is_complete(dir: &Path, file: &str) -> bool {
    let path = dir.join(file);
    path.is_file() && std::fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false)
}

pub fn missing_files(app: &AppHandle, code: &str) -> Result<Vec<String>> {
    let dir = model_dir(app, code)?;
    Ok(FILES
        .iter()
        .filter(|file| !is_complete(&dir, file))
        .map(|file| (*file).to_string())
        .collect())
}

pub fn model_ready(app: &AppHandle, code: &str) -> bool {
    missing_files(app, code).map(|m| m.is_empty()).unwrap_or(false)
}

/// Every vendored package importable from our vendor directory.
///
/// Checked by looking for the directories rather than by importing, so this
/// stays cheap and does not need a Python process.
pub fn runtime_ready(app: &AppHandle) -> bool {
    let Ok(dir) = vendor_dir(app) else {
        return false;
    };
    VENDOR_PACKAGES
        .iter()
        .all(|package| dir.join(package).is_dir())
}

/// Bytes used by every downloaded model, not just the active one — someone
/// who has switched languages twice should be told the true figure.
fn bytes_on_disk(app: &AppHandle) -> u64 {
    let Ok(root) = translate_root(app) else {
        return 0;
    };
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };

    entries
        .filter_map(std::result::Result::ok)
        .flat_map(|entry| {
            FILES
                .iter()
                .filter_map(move |file| std::fs::metadata(entry.path().join(file)).ok())
        })
        .map(|m| m.len())
        .sum()
}

pub fn status(app: &AppHandle, active: Option<&str>) -> Result<Status> {
    let missing = match active {
        Some(code) => missing_files(app, code)?,
        // Nothing selected means nothing to fetch, rather than everything.
        None => Vec::new(),
    };

    Ok(Status {
        model_ready: active.is_some() && missing.is_empty(),
        runtime_ready: runtime_ready(app),
        bytes_on_disk: bytes_on_disk(app),
        missing,
        active_language: active.map(str::to_string),
        active_model: active.and_then(model_for).map(str::to_string),
        active_quality: active.and_then(quality_for),
    })
}

/// Download whatever is missing for a language, through the normal queue.
///
/// Reusing the queue means translation downloads are resumable and survive a
/// restart exactly as model downloads do, rather than being a second, weaker
/// implementation of the same thing.
pub fn install_model(
    app: &AppHandle,
    manager: &std::sync::Arc<crate::downloads::DownloadManager>,
    code: &str,
) -> Result<usize> {
    let model = model_for(code)
        .ok_or_else(|| AppError::msg(format!("no translation model for language '{code}'")))?;
    let dir = model_dir(app, code)?;
    let missing = missing_files(app, code)?;

    for file in &missing {
        crate::downloads::enqueue(
            app,
            manager,
            // Keyed by model, not language, so two languages sharing a model
            // cannot queue the same file twice.
            format!("translate:{model}:{file}"),
            format!("Translation · {model} · {file}"),
            file.clone(),
            "translation".to_string(),
            file_url(model, file),
            dir.join(file).display().to_string(),
            // Hugging Face serves these anonymously and rejects an
            // unexpected bearer token.
            None,
        )?;
    }

    Ok(missing.len())
}

/// Vendor the missing Python packages using the install's own Python.
///
/// `--target` keeps them out of the Fooocus environment entirely. Using
/// `python_embeded` rather than any Python on PATH guarantees the wheel ABI
/// matches the interpreter that will import it.
pub fn install_runtime(app: &AppHandle, install_root: &Path) -> Result<()> {
    let python = install_root.join("python_embeded/python.exe");
    if !python.is_file() {
        return Err(AppError::msg(
            "Fooocus's bundled Python was not found, so the translation runtime cannot be installed",
        ));
    }

    let target = vendor_dir(app)?;

    let mut command = Command::new(&python);
    command.args(["-m", "pip", "install"]);
    command.args(VENDOR_PACKAGES);
    command.args([
        "--target",
        &target.display().to_string(),
        "--no-cache-dir",
        "--disable-pip-version-check",
    ]);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command.output()?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let tail: String = detail.lines().rev().take(4).collect::<Vec<_>>().join(" ");
        return Err(AppError::msg(format!(
            "installing the translation runtime failed: {tail}"
        )));
    }

    if !runtime_ready(app) {
        return Err(AppError::msg(
            "the translation runtime reported success but is not importable from the vendor directory",
        ));
    }

    Ok(())
}

/// Delete every downloaded model, for someone who wants the space back.
pub fn remove_models(app: &AppHandle) -> Result<u64> {
    let freed = bytes_on_disk(app);
    let root = translate_root(app)?;

    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.filter_map(std::result::Result::ok) {
            if entry.path().is_dir() {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }

    Ok(freed)
}
