//! Civitai browsing.
//!
//! Requests go through Rust rather than the webview so the API key never
//! reaches the frontend, and so we are not at the mercy of Civitai's CORS
//! headers. Searching is anonymous; downloading requires a key, which Civitai
//! has enforced since 2024 (an unauthenticated download URL returns 401).

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

const API: &str = "https://civitai.com/api/v1";

/// Hosts we will accept a download from.
pub const DOWNLOAD_HOSTS: &[&str] = &["civitai.com", "civitai-delivery-worker-prod.5ac0637cfd0766c97916cefa3764fbdf.r2.cloudflarestorage.com"];

/// Base models Fooocus can actually run. Fooocus is SDXL-only, and a large
/// share of Civitai is SD 1.5 — which downloads happily and then fails to
/// load. Filtering by default is the single biggest usability win over the
/// website.
pub const SDXL_BASE_MODELS: &[&str] = &[
    "SDXL 1.0",
    "SDXL 1.0 LCM",
    "SDXL Turbo",
    "SDXL Lightning",
    "SDXL Hyper",
    "SDXL Distilled",
    "Pony",
    "Illustrious",
    "NoobAI",
];

/// Civitai model type -> our model category (and therefore target folder).
pub fn category_for_type(kind: &str) -> Option<&'static str> {
    match kind.to_ascii_lowercase().as_str() {
        "checkpoint" => Some("checkpoints"),
        "lora" | "locon" | "lycoris" | "dora" => Some("loras"),
        "textualinversion" => Some("embeddings"),
        "vae" => Some("vae"),
        "controlnet" => Some("controlnet"),
        "upscaler" => Some("upscale_models"),
        _ => None,
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchParams {
    pub query: Option<String>,
    /// Civitai type, e.g. "Checkpoint" or "LORA".
    pub types: Option<String>,
    pub sort: Option<String>,
    pub period: Option<String>,
    pub cursor: Option<String>,
    /// When false, restrict to base models Fooocus can run.
    pub all_base_models: Option<bool>,
    pub nsfw: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CivitaiFile {
    pub name: String,
    pub size_kb: f64,
    pub download_url: String,
    /// Civitai's own SHA256, when published, for later verification.
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CivitaiVersion {
    pub id: u64,
    pub name: String,
    pub base_model: String,
    /// False when this version's base model is not SDXL-compatible.
    pub compatible: bool,
    pub file: Option<CivitaiFile>,
    pub image: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CivitaiModel {
    pub id: u64,
    pub name: String,
    pub kind: String,
    /// Our category, or None when Fooocus has no use for this type.
    pub category: Option<String>,
    pub creator: Option<String>,
    pub nsfw: bool,
    pub downloads: u64,
    pub thumbs_up: u64,
    pub versions: Vec<CivitaiVersion>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub items: Vec<CivitaiModel>,
    pub next_cursor: Option<String>,
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent("FooocusFrontend")
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(60))
        .build()?)
}

pub async fn search(params: SearchParams, api_key: Option<&str>) -> Result<SearchResults> {
    let mut url = reqwest::Url::parse(&format!("{API}/models"))
        .map_err(|e| AppError::msg(e.to_string()))?;

    {
        let mut q = url.query_pairs_mut();
        q.append_pair("limit", "24");

        if let Some(text) = params.query.as_deref().filter(|t| !t.trim().is_empty()) {
            q.append_pair("query", text.trim());
        }
        if let Some(types) = params.types.as_deref().filter(|t| !t.is_empty()) {
            q.append_pair("types", types);
        }
        q.append_pair("sort", params.sort.as_deref().unwrap_or("Highest Rated"));
        if let Some(period) = params.period.as_deref() {
            q.append_pair("period", period);
        }
        if let Some(cursor) = params.cursor.as_deref() {
            q.append_pair("cursor", cursor);
        }
        if !params.all_base_models.unwrap_or(false) {
            for base in SDXL_BASE_MODELS {
                q.append_pair("baseModels", base);
            }
        }
        // Civitai treats this as "include NSFW", not "only NSFW".
        q.append_pair("nsfw", if params.nsfw.unwrap_or(false) { "true" } else { "false" });
    }

    let mut request = client()?.get(url);
    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }

    let response = request.send().await?;
    if !response.status().is_success() {
        return Err(AppError::msg(format!(
            "Civitai search failed ({})",
            response.status()
        )));
    }

    let body: serde_json::Value = response.json().await?;
    Ok(parse(&body, params.nsfw.unwrap_or(false)))
}

/// Translate Civitai's payload into just what the UI needs.
///
/// Their schema is loose — fields go missing on older models — so everything
/// is read defensively rather than deserialised into a rigid struct.
fn parse(body: &serde_json::Value, allow_nsfw: bool) -> SearchResults {
    let items = body
        .get("items")
        .and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|raw| parse_model(raw, allow_nsfw))
        .collect();

    SearchResults {
        items,
        next_cursor: body
            .pointer("/metadata/nextCursor")
            .and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                _ => None,
            }),
    }
}

fn parse_model(raw: &serde_json::Value, allow_nsfw: bool) -> Option<CivitaiModel> {
    let nsfw = raw.get("nsfw").and_then(serde_json::Value::as_bool).unwrap_or(false);
    if nsfw && !allow_nsfw {
        return None;
    }

    let kind = raw.get("type")?.as_str()?.to_string();
    let stats = raw.get("stats");

    let versions: Vec<CivitaiVersion> = raw
        .get("modelVersions")
        .and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|v| parse_version(v, allow_nsfw))
        .collect();

    if versions.is_empty() {
        return None;
    }

    Some(CivitaiModel {
        id: raw.get("id")?.as_u64()?,
        name: raw.get("name")?.as_str()?.to_string(),
        category: category_for_type(&kind).map(str::to_string),
        kind,
        creator: raw
            .pointer("/creator/username")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        nsfw,
        downloads: stats
            .and_then(|s| s.get("downloadCount"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        thumbs_up: stats
            .and_then(|s| s.get("thumbsUpCount"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        versions,
    })
}

fn parse_version(raw: &serde_json::Value, allow_nsfw: bool) -> Option<CivitaiVersion> {
    let base_model = raw
        .get("baseModel")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();

    // Prefer the primary weights file; Civitai attaches config and preview
    // files to the same version.
    let file = raw
        .get("files")
        .and_then(|v| v.as_array())
        .and_then(|files| {
            files
                .iter()
                .find(|f| {
                    f.get("primary").and_then(serde_json::Value::as_bool).unwrap_or(false)
                })
                .or_else(|| files.first())
        })
        .and_then(|f| {
            Some(CivitaiFile {
                name: f.get("name")?.as_str()?.to_string(),
                size_kb: f.get("sizeKB").and_then(serde_json::Value::as_f64).unwrap_or(0.0),
                download_url: f.get("downloadUrl")?.as_str()?.to_string(),
                sha256: f
                    .pointer("/hashes/SHA256")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            })
        });

    // nsfwLevel 1 is "safe"; anything higher is progressively less so.
    let image = raw
        .get("images")
        .and_then(|v| v.as_array())
        .and_then(|images| {
            images
                .iter()
                .find(|i| {
                    allow_nsfw
                        || i.get("nsfwLevel")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0)
                            <= 1
                })
                .and_then(|i| i.get("url"))
                .and_then(|u| u.as_str())
                .map(str::to_string)
        });

    Some(CivitaiVersion {
        id: raw.get("id")?.as_u64()?,
        name: raw
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Version")
            .to_string(),
        compatible: SDXL_BASE_MODELS.contains(&base_model.as_str()),
        base_model,
        file,
        image,
    })
}

/// Check a key by asking for something only an authenticated caller can get.
pub async fn verify_key(api_key: &str) -> Result<bool> {
    let response = client()?
        .get(format!("{API}/models?limit=1"))
        .bearer_auth(api_key)
        .send()
        .await?;
    Ok(response.status().is_success())
}
