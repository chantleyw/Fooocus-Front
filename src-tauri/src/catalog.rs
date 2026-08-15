//! The downloadable model catalog.
//!
//! Every URL here is taken from the Fooocus source itself — `modules/config.py`
//! and `launch.py` for the support models, `presets/*.json` for the checkpoints
//! and their companions. These are the exact files Fooocus would fetch on
//! demand; the catalog simply lets the user fetch them deliberately, with
//! visible progress, instead of discovering a silent multi-gigabyte download
//! halfway through a generation.
//!
//! Sizes are never hardcoded. They are read from the server before a download
//! starts, so the numbers shown are always the real ones.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;

use crate::install::InstallInfo;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    /// Stable id, used as the download job key.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Name the file must have on disk. Occasionally differs from the URL's
    /// last segment — Fooocus saves `fooocus_expansion.bin` as
    /// `pytorch_model.bin`, and `vaeapp_sd15.pt` as `vaeapp_sd15.pth`.
    pub filename: String,
    /// Model category id, matching `install::CATEGORIES`.
    pub category: String,
    pub url: String,
    pub description: String,
    /// Shown as chips in the UI.
    pub tags: Vec<String>,
    /// Required for core Fooocus features; the UI groups these first.
    pub essential: bool,
    /// Filled in per-install: is the file already on disk, and where would it go.
    pub installed: bool,
    pub target_path: String,
    /// Real on-disk size when installed.
    pub installed_size: Option<u64>,
}

/// (id, name, filename, category, url, description, tags, essential)
type Row = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
    bool,
);

const HF: &str = "https://huggingface.co";

#[rustfmt::skip]
const ENTRIES: &[Row] = &[
    // ---------------------------------------------------------------- checkpoints
    ("juggernaut-xl-v8", "Juggernaut XL v8", "juggernautXL_v8Rundiffusion.safetensors", "checkpoints",
     "https://huggingface.co/lllyasviel/fav_models/resolve/main/fav/juggernautXL_v8Rundiffusion.safetensors",
     "The Fooocus default. A well-rounded photorealistic SDXL model that handles people, products and scenes without much prompt effort.",
     &["Photoreal", "General purpose", "Default"], true),

    ("realistic-stock-photo-v2", "Realistic Stock Photo v2.0", "realisticStockPhoto_v20.safetensors", "checkpoints",
     "https://huggingface.co/lllyasviel/fav_models/resolve/main/fav/realisticStockPhoto_v20.safetensors",
     "Tuned for clean commercial photography — even lighting, natural skin, stock-image framing. Used by the Realistic preset.",
     &["Photoreal", "Preset: realistic"], false),

    ("anima-pencil-xl-v5", "AnimaPencil XL v5.0", "animaPencilXL_v500.safetensors", "checkpoints",
     "https://huggingface.co/mashb1t/fav_models/resolve/main/fav/animaPencilXL_v500.safetensors",
     "Anime and illustration model with strong line work and clean colour. Used by the Anime preset.",
     &["Anime", "Illustration", "Preset: anime"], false),

    ("pony-diffusion-v6-xl", "Pony Diffusion V6 XL", "ponyDiffusionV6XL.safetensors", "checkpoints",
     "https://huggingface.co/mashb1t/fav_models/resolve/main/fav/ponyDiffusionV6XL.safetensors",
     "Stylised character and creature model with its own scoring-tag prompt style. Pair it with the matching VAE. Used by the Pony preset.",
     &["Stylised", "Characters", "Preset: pony_v6"], false),

    ("playground-v25", "Playground v2.5 (1024px Aesthetic)", "playground-v2.5-1024px-aesthetic.fp16.safetensors", "checkpoints",
     "https://huggingface.co/mashb1t/fav_models/resolve/main/fav/playground-v2.5-1024px-aesthetic.fp16.safetensors",
     "Strong aesthetic bias with vivid colour and contrast. Good for posters and album-art looks.",
     &["Aesthetic", "Vivid", "Preset: playground_v2.5"], false),

    ("sdxl-base-10", "Stable Diffusion XL Base 1.0", "sd_xl_base_1.0_0.9vae.safetensors", "checkpoints",
     "https://huggingface.co/stabilityai/stable-diffusion-xl-base-1.0/resolve/main/sd_xl_base_1.0_0.9vae.safetensors",
     "The original SDXL base model from Stability AI. A neutral, unopinionated starting point. Used by the SAI preset.",
     &["Base model", "Neutral", "Preset: sai"], false),

    ("sdxl-refiner-10", "Stable Diffusion XL Refiner 1.0", "sd_xl_refiner_1.0_0.9vae.safetensors", "checkpoints",
     "https://huggingface.co/stabilityai/stable-diffusion-xl-refiner-1.0/resolve/main/sd_xl_refiner_1.0_0.9vae.safetensors",
     "Second-stage model that polishes detail on top of SDXL Base. Only useful when a refiner is selected.",
     &["Refiner", "Preset: sai"], false),

    // ----------------------------------------------------------------------- vae
    ("pony-vae", "Pony Diffusion V6 XL VAE", "ponyDiffusionV6XL_vae.safetensors", "vae",
     "https://huggingface.co/mashb1t/fav_models/resolve/main/fav/ponyDiffusionV6XL_vae.safetensors",
     "The decoder Pony Diffusion V6 XL expects. Without it that checkpoint's colours come out washed.",
     &["Companion to Pony V6"], false),

    // --------------------------------------------------------------------- loras
    ("lora-offset", "SDXL Offset Noise", "sd_xl_offset_example-lora_1.0.safetensors", "loras",
     "https://huggingface.co/stabilityai/stable-diffusion-xl-base-1.0/resolve/main/sd_xl_offset_example-lora_1.0.safetensors",
     "Stability's offset-noise LoRA. Deepens blacks and widens dynamic range. Enabled by default in Fooocus.",
     &["Contrast", "Default"], true),

    ("lora-film-photography", "SDXL Film Photography Style", "SDXL_FILM_PHOTOGRAPHY_STYLE_V1.safetensors", "loras",
     "https://huggingface.co/mashb1t/fav_models/resolve/main/fav/SDXL_FILM_PHOTOGRAPHY_STYLE_V1.safetensors",
     "Analogue film grain, colour cast and falloff. Used by the Realistic preset.",
     &["Film", "Preset: realistic"], false),

    ("lora-lcm", "LCM LoRA (SDXL)", "sdxl_lcm_lora.safetensors", "loras",
     "https://huggingface.co/lllyasviel/misc/resolve/main/sdxl_lcm_lora.safetensors",
     "Latent Consistency LoRA. Powers the Extreme Speed performance mode, generating in around 8 steps.",
     &["Speed", "Performance mode"], false),

    ("lora-lightning", "SDXL Lightning 4-step", "sdxl_lightning_4step_lora.safetensors", "loras",
     "https://huggingface.co/mashb1t/misc/resolve/main/sdxl_lightning_4step_lora.safetensors",
     "Powers the Lightning performance mode. Four steps per image with surprisingly little quality loss.",
     &["Speed", "Performance mode"], false),

    ("lora-hyper-sd", "Hyper-SD 4-step", "sdxl_hyper_sd_4step_lora.safetensors", "loras",
     "https://huggingface.co/mashb1t/misc/resolve/main/sdxl_hyper_sd_4step_lora.safetensors",
     "Powers the Hyper-SD performance mode. Another four-step accelerator, with a different look to Lightning.",
     &["Speed", "Performance mode"], false),

    // ------------------------------------------------------------------- inpaint
    ("inpaint-head", "Inpaint Head", "fooocus_inpaint_head.pth", "inpaint",
     "https://huggingface.co/lllyasviel/fooocus_inpaint/resolve/main/fooocus_inpaint_head.pth",
     "Required by every inpaint and outpaint operation. Small, and there is no reason not to have it.",
     &["Required for inpaint"], true),

    ("inpaint-v26", "Inpaint Patch v2.6", "inpaint_v26.fooocus.patch", "inpaint",
     "https://huggingface.co/lllyasviel/fooocus_inpaint/resolve/main/inpaint_v26.fooocus.patch",
     "The current inpaint model. Best results for redrawing a masked region or extending an image.",
     &["Current", "Required for inpaint"], true),

    ("inpaint-v25", "Inpaint Patch v2.5", "inpaint_v25.fooocus.patch", "inpaint",
     "https://huggingface.co/lllyasviel/fooocus_inpaint/resolve/main/inpaint_v25.fooocus.patch",
     "Previous-generation inpaint model, kept for comparison against v2.6.",
     &["Legacy"], false),

    ("inpaint-v1", "Inpaint Patch v1", "inpaint.fooocus.patch", "inpaint",
     "https://huggingface.co/lllyasviel/fooocus_inpaint/resolve/main/inpaint.fooocus.patch",
     "The original inpaint model. Only needed to reproduce older results.",
     &["Legacy"], false),

    // ---------------------------------------------------------------- controlnet
    ("cn-canny", "ControlNet Canny", "control-lora-canny-rank128.safetensors", "controlnet",
     "https://huggingface.co/lllyasviel/misc/resolve/main/control-lora-canny-rank128.safetensors",
     "Copies the hard edges of a reference image. Backs the PyraCanny option in Image Prompt.",
     &["Image Prompt", "Edges"], false),

    ("cn-cpds", "ControlNet CPDS", "fooocus_xl_cpds_128.safetensors", "controlnet",
     "https://huggingface.co/lllyasviel/misc/resolve/main/fooocus_xl_cpds_128.safetensors",
     "Copies structure and depth rather than edges. Backs the CPDS option in Image Prompt.",
     &["Image Prompt", "Structure"], false),

    ("ip-negative", "Image Prompt Negative", "fooocus_ip_negative.safetensors", "controlnet",
     "https://huggingface.co/lllyasviel/misc/resolve/main/fooocus_ip_negative.safetensors",
     "Required alongside the IP-Adapters. Supplies the negative side of image prompting.",
     &["Image Prompt", "Required"], false),

    ("ip-adapter-plus", "IP-Adapter Plus", "ip-adapter-plus_sdxl_vit-h.bin", "controlnet",
     "https://huggingface.co/lllyasviel/misc/resolve/main/ip-adapter-plus_sdxl_vit-h.bin",
     "Transfers the overall style and content of a reference image. Backs the Image Prompt option.",
     &["Image Prompt", "Style transfer"], false),

    ("ip-adapter-face", "IP-Adapter Plus Face", "ip-adapter-plus-face_sdxl_vit-h.bin", "controlnet",
     "https://huggingface.co/lllyasviel/misc/resolve/main/ip-adapter-plus-face_sdxl_vit-h.bin",
     "Face-focused IP-Adapter. Backs the FaceSwap option in Image Prompt.",
     &["Image Prompt", "FaceSwap"], false),

    // --------------------------------------------------------------- clip vision
    ("clip-vision-h", "CLIP Vision ViT-H", "clip_vision_vit_h.safetensors", "clip_vision",
     "https://huggingface.co/lllyasviel/misc/resolve/main/clip_vision_vit_h.safetensors",
     "The image encoder every IP-Adapter depends on. Needed before any Image Prompt mode will run.",
     &["Image Prompt", "Required"], false),

    // ------------------------------------------------------------------ upscaler
    ("upscaler", "Fooocus Upscaler", "fooocus_upscaler_s409985e5.bin", "upscale_models",
     "https://huggingface.co/lllyasviel/misc/resolve/main/fooocus_upscaler_s409985e5.bin",
     "Used by every Upscale and Vary operation. Small and essential.",
     &["Required for upscale"], true),

    // ------------------------------------------------------------ safety checker
    ("safety-checker", "Safety Checker", "stable-diffusion-safety-checker.bin", "safety_checker",
     "https://huggingface.co/mashb1t/misc/resolve/main/stable-diffusion-safety-checker.bin",
     "Optional NSFW classifier. Only used when the image censor is switched on in settings.",
     &["Optional"], false),

    // ----------------------------------------------------------------------- sam
    ("sam-vit-b", "Segment Anything ViT-B", "sam_vit_b_01ec64.pth", "sam",
     "https://huggingface.co/mashb1t/misc/resolve/main/sam_vit_b_01ec64.pth",
     "Smallest and fastest detection model for describe-to-mask inpainting.",
     &["Enhance", "Fast"], false),

    ("sam-vit-l", "Segment Anything ViT-L", "sam_vit_l_0b3195.pth", "sam",
     "https://huggingface.co/mashb1t/misc/resolve/main/sam_vit_l_0b3195.pth",
     "Mid-size detection model. A reasonable balance of accuracy and speed.",
     &["Enhance", "Balanced"], false),

    ("sam-vit-h", "Segment Anything ViT-H", "sam_vit_h_4b8939.pth", "sam",
     "https://huggingface.co/mashb1t/misc/resolve/main/sam_vit_h_4b8939.pth",
     "Largest and most accurate detection model, and the slowest to run.",
     &["Enhance", "Accurate"], false),

    // ---------------------------------------------------------------- vae approx
    ("vae-approx-xl", "Preview Decoder (SDXL)", "xlvaeapp.pth", "vae_approx",
     "https://huggingface.co/lllyasviel/misc/resolve/main/xlvaeapp.pth",
     "Draws the live step-by-step preview while an SDXL image generates.",
     &["Preview", "Required"], true),

    ("vae-approx-sd15", "Preview Decoder (SD 1.5)", "vaeapp_sd15.pth", "vae_approx",
     "https://huggingface.co/lllyasviel/misc/resolve/main/vaeapp_sd15.pt",
     "Preview decoder for SD 1.5 latents.",
     &["Preview"], false),

    ("vae-interposer", "XL to v1 Interposer", "xl-to-v1_interposer-v4.0.safetensors", "vae_approx",
     "https://huggingface.co/mashb1t/misc/resolve/main/xl-to-v1_interposer-v4.0.safetensors",
     "Converts SDXL latents to SD 1.5 space so previews work across model families.",
     &["Preview"], false),

    // ----------------------------------------------------------- prompt expansion
    ("prompt-expansion", "Fooocus Prompt Expansion (V2)", "pytorch_model.bin", "prompt_expansion",
     "https://huggingface.co/lllyasviel/misc/resolve/main/fooocus_expansion.bin",
     "The GPT-2 model behind the Fooocus V2 style, which turns a short prompt into a richly detailed one.",
     &["Fooocus V2", "Required"], true),
];

/// Build the catalog for a given installation, marking what is already present
/// and where each missing file would land.
///
/// Anything a preset references that the built-in list does not know about is
/// appended, so a newer Fooocus that ships a new default model still shows up.
pub fn build(info: &InstallInfo) -> Vec<CatalogEntry> {
    let mut entries: Vec<CatalogEntry> = ENTRIES
        .iter()
        .map(|(id, name, filename, category, url, description, tags, essential)| {
            let target = target_path(info, category, filename);
            let meta = std::fs::metadata(&target).ok();
            CatalogEntry {
                id: (*id).to_string(),
                name: (*name).to_string(),
                filename: (*filename).to_string(),
                category: (*category).to_string(),
                url: (*url).to_string(),
                description: (*description).to_string(),
                tags: tags.iter().map(|t| (*t).to_string()).collect(),
                essential: *essential,
                installed: meta.is_some(),
                installed_size: meta.as_ref().map(std::fs::Metadata::len),
                target_path: target,
            }
        })
        .collect();

    let known: BTreeSet<String> = entries.iter().map(|e| e.url.clone()).collect();
    entries.extend(from_presets(info, &known));
    entries
}

/// Resolve where a catalog file belongs, using the install's configured paths.
fn target_path(info: &InstallInfo, category: &str, filename: &str) -> String {
    let dir = info
        .model_paths
        .get(category)
        .and_then(|paths| paths.first())
        .cloned()
        .unwrap_or_else(|| {
            Path::new(&info.fooocus_dir)
                .join("models")
                .join(category)
                .display()
                .to_string()
        });
    Path::new(&dir).join(filename).display().to_string()
}

/// Scrape `presets/*.json` for download URLs the built-in list does not cover.
fn from_presets(info: &InstallInfo, known: &BTreeSet<String>) -> Vec<CatalogEntry> {
    // Preset key -> our category id.
    const SECTIONS: &[(&str, &str)] = &[
        ("checkpoint_downloads", "checkpoints"),
        ("lora_downloads", "loras"),
        ("embeddings_downloads", "embeddings"),
        ("vae_downloads", "vae"),
    ];

    let dir = Path::new(&info.fooocus_dir).join("presets");
    let mut seen: BTreeSet<String> = known.clone();
    let mut extra = Vec::new();

    for preset in &info.presets {
        let path = dir.join(format!("{preset}.json"));
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };

        for (key, category) in SECTIONS {
            let Some(map) = json.get(key).and_then(|v| v.as_object()) else {
                continue;
            };
            for (filename, url) in map {
                let Some(url) = url.as_str() else { continue };
                if !seen.insert(url.to_string()) {
                    continue;
                }

                let target = target_path(info, category, filename);
                let meta = std::fs::metadata(&target).ok();
                extra.push(CatalogEntry {
                    id: format!("preset-{}", slug(filename)),
                    name: filename.trim_end_matches(".safetensors").to_string(),
                    filename: filename.clone(),
                    category: (*category).to_string(),
                    url: url.to_string(),
                    description: format!("Referenced by the {preset} preset in your Fooocus install."),
                    tags: vec![format!("Preset: {preset}")],
                    essential: false,
                    installed: meta.is_some(),
                    installed_size: meta.as_ref().map(std::fs::Metadata::len),
                    target_path: target,
                });
            }
        }
    }

    extra
}

fn slug(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

/// Guard against a malformed catalog entry pointing somewhere unexpected.
pub fn is_huggingface(url: &str) -> bool {
    url.starts_with(HF)
}
