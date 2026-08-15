//! Reading generated images out of the Fooocus outputs folder.
//!
//! Fooocus writes to `outputs/YYYY-MM-DD/`, so the folder name is the date and
//! we get grouping for free without parsing any metadata.

use std::path::Path;

use serde::Serialize;

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryImage {
    pub name: String,
    pub path: String,
    /// The `YYYY-MM-DD` folder the image sits in.
    pub day: String,
    pub size: u64,
    /// Seconds since the Unix epoch, for sorting newest-first.
    pub modified: u64,
}

/// List the most recent images, newest first.
pub fn list(outputs_dir: &str, limit: usize) -> Vec<GalleryImage> {
    let root = Path::new(outputs_dir);
    if !root.is_dir() {
        return Vec::new();
    }

    let mut images: Vec<GalleryImage> = walkdir::WalkDir::new(root)
        .max_depth(2)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map(str::to_ascii_lowercase)
                .is_some_and(|x| IMAGE_EXTENSIONS.contains(&x.as_str()))
        })
        .map(|e| {
            let meta = e.metadata().ok();
            let day = e
                .path()
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            GalleryImage {
                name: e.file_name().to_string_lossy().to_string(),
                path: e.path().display().to_string(),
                day,
                size: meta.as_ref().map(std::fs::Metadata::len).unwrap_or(0),
                modified: meta
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            }
        })
        .collect();

    images.sort_by(|a, b| b.modified.cmp(&a.modified));
    images.truncate(limit);
    images
}
