use serde::{Serialize, Serializer};

/// Every failure that can cross the Rust -> JS boundary.
///
/// Tauri commands must return something `Serialize`, so we flatten to a
/// plain string message rather than exposing the underlying error types.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Message(String),

    #[error("no Fooocus installation found at {0}")]
    InstallNotFound(String),

    #[error("Fooocus is already running")]
    AlreadyRunning,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("could not parse {file}: {source}")]
    Json {
        file: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
}

impl AppError {
    pub fn msg(m: impl Into<String>) -> Self {
        AppError::Message(m.into())
    }
}

impl Serialize for AppError {
    // Fully qualified: the `Result` alias below shadows the std one in this module.
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
