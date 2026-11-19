#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Io,
    Config,
    Index,
    Database,
    Provider,
    ProviderNotInstalled,
    ProviderBusy,
    Capture,
    Permission,
    InvalidInput,
    Unsupported,
    Mcp,
    Internal,
}

#[derive(Debug, thiserror::Error)]
#[error("{kind:?}: {message}")]
pub struct CoreError {
    pub kind: ErrorKind,
    pub message: String,
}

impl CoreError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl From<std::io::Error> for CoreError {
    fn from(err: std::io::Error) -> Self {
        Self::new(ErrorKind::Io, err.to_string())
    }
}

impl From<rusqlite::Error> for CoreError {
    fn from(err: rusqlite::Error) -> Self {
        Self::new(ErrorKind::Database, err.to_string())
    }
}

impl From<serde_json::Error> for CoreError {
    fn from(err: serde_json::Error) -> Self {
        Self::new(ErrorKind::Config, err.to_string())
    }
}

impl From<toml::de::Error> for CoreError {
    fn from(err: toml::de::Error) -> Self {
        Self::new(ErrorKind::Config, err.to_string())
    }
}
