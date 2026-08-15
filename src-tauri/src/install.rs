//! Discovery and inspection of a Fooocus installation.
//!
//! Everything here is read-only. We never assume the stock folder layout:
//! `config.txt` holds absolute paths for every model category and is the
//! source of truth, so a user who relocated their checkpoints still gets a
//! correct picture. The stock layout is only the fallback.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

/// Model file extensions we surface in the manager. Everything else in a
/// model folder (`put_loras_here`, `.txt` notes, `.yaml` configs) is noise.
const MODEL_EXTENSIONS: &[&str] = &["safetensors", "ckpt", "pth", "pt", "bin", "patch", "onnx"];

/// The model categories Fooocus knows about, paired with the `config.txt` key
/// that overrides their location and the stock folder name under `models/`.
pub const CATEGORIES: &[(&str, &str, &str)] = &[
    // (id, config.txt key, stock folder under models/)
    ("checkpoints", "path_checkpoints", "checkpoints"),
    ("loras", "path_loras", "loras"),
    ("embeddings", "path_embeddings", "embeddings"),
    ("vae", "path_vae", "vae"),
    ("vae_approx", "path_vae_approx", "vae_approx"),
    ("upscale_models", "path_upscale_models", "upscale_models"),
    ("inpaint", "path_inpaint", "inpaint"),
    ("controlnet", "path_controlnet", "controlnet"),
    ("clip_vision", "path_clip_vision", "clip_vision"),
    ("safety_checker", "path_safety_checker", "safety_checker"),
    ("sam", "path_sam", "sam"),
    (
        "prompt_expansion",
        "path_fooocus_expansion",
        "prompt_expansion/fooocus_expansion",
    ),
];

/// Human-facing label and blurb for each category, shown in the Model Manager.
pub fn category_meta(id: &str) -> (&'static str, &'static str) {
    match id {
        "checkpoints" => (
            "Checkpoints",
            "Base models. These determine the overall look and quality of everything you generate.",
        ),
        "loras" => (
            "LoRAs",
            "Small style and subject add-ons layered on top of a checkpoint, with adjustable strength.",
        ),
        "embeddings" => (
            "Embeddings",
            "Textual inversions. Compact concepts you trigger by name inside a prompt.",
        ),
        "vae" => (
            "VAE",
            "Decoders that convert the model's latent output into pixels. Affects colour and fine detail.",
        ),
        "vae_approx" => (
            "VAE (preview)",
            "Lightweight decoders used to draw the fast step-by-step preview while generating.",
        ),
        "upscale_models" => (
            "Upscalers",
            "Used by the Upscale and Vary tools to enlarge an image without losing detail.",
        ),
        "inpaint" => (
            "Inpaint",
            "Specialised patches that let Fooocus redraw a masked region of an existing image.",
        ),
        "controlnet" => (
            "ControlNet & Image Prompt",
            "Guides generation from a reference image: pose, edges, depth, or overall style.",
        ),
        "clip_vision" => (
            "CLIP Vision",
            "Image encoders required by Image Prompt and the face-swap tools.",
        ),
        "safety_checker" => (
            "Safety Checker",
            "Optional NSFW classifier applied to finished images.",
        ),
        "sam" => (
            "Segment Anything",
            "Detection models that turn a text description into an inpaint mask automatically.",
        ),
        "prompt_expansion" => (
            "Prompt Expansion",
            "The GPT-2 model behind Fooocus V2, which enriches short prompts automatically.",
        ),
        _ => ("Other", "Additional model files."),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatFile {
    /// File name, e.g. `run_arc.bat`.
    pub name: String,
    pub path: String,
    /// Friendly name derived from the file, e.g. "Arc / DirectML".
    pub label: String,
    pub description: String,
    /// The python arguments the bat passes, already split.
    pub args: Vec<String>,
    /// True when the bat routes through `entry_with_update.py`, which git-pulls
    /// Fooocus before launching.
    pub auto_updates: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub category: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCategory {
    pub id: String,
    pub label: String,
    pub description: String,
    /// Every directory Fooocus scans for this category (config.txt allows lists).
    pub paths: Vec<String>,
    pub files: Vec<ModelFile>,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallInfo {
    /// Folder holding the `.bat` files and `python_embeded`.
    pub root: String,
    /// The `Fooocus` subfolder holding `launch.py`.
    pub fooocus_dir: String,
    pub python: String,
    pub version: Option<String>,
    pub bats: Vec<BatFile>,
    pub presets: Vec<String>,
    pub outputs_dir: String,
    /// Resolved directory per category id, taken from config.txt where present.
    pub model_paths: BTreeMap<String, Vec<String>>,
}

/// Resolve a user-supplied folder into a real installation.
///
/// Accepts either the launcher folder itself (`...\Fooocus_win64_2-5-0`) or a
/// parent that contains it — picking `D:\AI\Fooocus` in a folder dialog is the
/// obvious thing to do, and it should just work.
pub fn resolve_install(path: &Path) -> Result<PathBuf> {
    if is_install_root(path) {
        return Ok(path.to_path_buf());
    }

    // Look one level down for the real root.
    if let Ok(entries) = std::fs::read_dir(path) {
        let mut candidates: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && is_install_root(p))
            .collect();
        // Newest-looking name wins when several versions sit side by side.
        candidates.sort();
        if let Some(found) = candidates.pop() {
            return Ok(found);
        }
    }

    Err(AppError::InstallNotFound(path.display().to_string()))
}

/// A root is valid when it has both the embedded interpreter and `launch.py`.
fn is_install_root(path: &Path) -> bool {
    path.join("python_embeded/python.exe").is_file() && path.join("Fooocus/launch.py").is_file()
}

/// Read everything we need to know about an installation in one pass.
pub fn inspect(root: &Path) -> Result<InstallInfo> {
    if !is_install_root(root) {
        return Err(AppError::InstallNotFound(root.display().to_string()));
    }

    let fooocus_dir = root.join("Fooocus");
    let config = read_config(&fooocus_dir)?;

    let mut model_paths = BTreeMap::new();
    for (id, key, stock) in CATEGORIES {
        let dirs = config_paths(&config, key)
            .unwrap_or_else(|| vec![fooocus_dir.join("models").join(stock)]);
        model_paths.insert(
            id.to_string(),
            dirs.iter().map(|p| p.display().to_string()).collect(),
        );
    }

    let outputs_dir = config_paths(&config, "path_outputs")
        .and_then(|v| v.into_iter().next())
        .unwrap_or_else(|| fooocus_dir.join("outputs"));

    Ok(InstallInfo {
        root: root.display().to_string(),
        fooocus_dir: fooocus_dir.display().to_string(),
        python: root.join("python_embeded/python.exe").display().to_string(),
        version: read_version(&fooocus_dir),
        bats: read_bats(root),
        presets: read_presets(&fooocus_dir),
        outputs_dir: outputs_dir.display().to_string(),
        model_paths,
    })
}

/// `config.txt` is JSON, but it is written by Fooocus and may be absent on a
/// fresh install — treat a missing file as "use every default".
fn read_config(fooocus_dir: &Path) -> Result<serde_json::Value> {
    let path = fooocus_dir.join("config.txt");
    if !path.is_file() {
        return Ok(serde_json::Value::Object(Default::default()));
    }
    let raw = std::fs::read_to_string(&path)?;
    serde_json::from_str(&raw).map_err(|source| AppError::Json {
        file: path.display().to_string(),
        source,
    })
}

/// Values in config.txt are either a single path or an array of paths.
fn config_paths(config: &serde_json::Value, key: &str) -> Option<Vec<PathBuf>> {
    match config.get(key)? {
        serde_json::Value::String(s) => Some(vec![PathBuf::from(s)]),
        serde_json::Value::Array(items) => {
            let paths: Vec<PathBuf> = items
                .iter()
                .filter_map(|v| v.as_str())
                .map(PathBuf::from)
                .collect();
            (!paths.is_empty()).then_some(paths)
        }
        _ => None,
    }
}

fn read_version(fooocus_dir: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(fooocus_dir.join("fooocus_version.py")).ok()?;
    // The file is a one-liner: version = '2.5.0'
    let start = raw.find(['\'', '"'])?;
    let quote = raw.as_bytes()[start] as char;
    let rest = &raw[start + 1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

/// Parse every `.bat` in the root into a launch profile.
fn read_bats(root: &Path) -> Vec<BatFile> {
    let mut bats: Vec<BatFile> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("bat"))
        })
        .filter_map(|p| parse_bat(&p))
        .collect();

    bats.sort_by(|a, b| a.name.cmp(&b.name));
    bats
}

/// Extract the python invocation from a Fooocus launch script.
///
/// The stock bats are two lines: a `python.exe -s <entry>.py <flags>` call and
/// a `pause`. We keep the flags (they are the whole point of picking one bat
/// over another) and drop everything else — notably `pause`, which would block
/// forever behind a hidden window waiting for a keypress that can never come.
fn parse_bat(path: &Path) -> Option<BatFile> {
    let raw = std::fs::read_to_string(path).ok()?;
    let name = path.file_name()?.to_string_lossy().to_string();

    let line = raw
        .lines()
        .map(str::trim)
        .find(|l| l.to_ascii_lowercase().contains("python.exe"))?;

    let tokens = split_command(line);
    // Drop the interpreter itself and its own flags; keep from the .py onward.
    let script_at = tokens
        .iter()
        .position(|t| t.to_ascii_lowercase().ends_with(".py"))?;
    let args: Vec<String> = tokens[script_at..].to_vec();

    let auto_updates = args
        .first()
        .is_some_and(|s| s.to_ascii_lowercase().contains("entry_with_update"));

    let (label, description) = describe_bat(&name, &args, auto_updates);

    Some(BatFile {
        name,
        path: path.display().to_string(),
        label,
        description,
        args,
        auto_updates,
    })
}

/// Minimal command-line splitter honouring double quotes.
fn split_command(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;

    for ch in line.chars() {
        match ch {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Turn a bat's file name and flags into something a human can choose between.
fn describe_bat(name: &str, args: &[String], auto_updates: bool) -> (String, String) {
    let preset = args
        .iter()
        .position(|a| a == "--preset")
        .and_then(|i| args.get(i + 1))
        .cloned();

    let stem = name.trim_end_matches(".bat").trim_end_matches(".BAT");
    let label = match (stem, preset.as_deref()) {
        ("run", _) => "Default".to_string(),
        ("run_arc", _) => "Intel Arc / AMD (DirectML)".to_string(),
        (_, Some(p)) => {
            let mut c = p.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => p.to_string(),
            }
        }
        (s, None) => s.replace('_', " "),
    };

    let mut notes: Vec<String> = Vec::new();
    if let Some(p) = &preset {
        notes.push(format!("Starts with the {p} preset."));
    }
    if args.iter().any(|a| a == "--attention-split") {
        notes.push("Tuned for Intel Arc and AMD cards using split attention and bf16.".into());
    }
    notes.push(if auto_updates {
        "Checks for a Fooocus update before starting.".into()
    } else {
        "Starts immediately without checking for updates.".into()
    });

    (label, notes.join(" "))
}

fn read_presets(fooocus_dir: &Path) -> Vec<String> {
    let mut presets: Vec<String> = std::fs::read_dir(fooocus_dir.join("presets"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
        .collect();
    presets.sort();
    presets
}

/// Walk every model directory and report what is actually on disk.
pub fn scan_models(info: &InstallInfo) -> Vec<ModelCategory> {
    CATEGORIES
        .iter()
        .map(|(id, _, _)| {
            let (label, description) = category_meta(id);
            let paths = info.model_paths.get(*id).cloned().unwrap_or_default();

            let mut files: Vec<ModelFile> = paths
                .iter()
                .flat_map(|dir| scan_dir(Path::new(dir), id))
                .collect();
            files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

            let total_size = files.iter().map(|f| f.size).sum();

            ModelCategory {
                id: id.to_string(),
                label: label.to_string(),
                description: description.to_string(),
                paths,
                files,
                total_size,
            }
        })
        .collect()
}

fn scan_dir(dir: &Path, category: &str) -> Vec<ModelFile> {
    walkdir::WalkDir::new(dir)
        .max_depth(3)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map(str::to_ascii_lowercase)
                .is_some_and(|x| MODEL_EXTENSIONS.contains(&x.as_str()))
        })
        .map(|e| ModelFile {
            name: e.file_name().to_string_lossy().to_string(),
            path: e.path().display().to_string(),
            size: e.metadata().map(|m| m.len()).unwrap_or(0),
            category: category.to_string(),
        })
        .collect()
}
