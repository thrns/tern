use std::path::PathBuf;
use std::sync::Arc;

use tern_core::core::Core;
use tern_core::error::CoreError;
use tern_core::settings as core_settings;

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    #[error("{kind}: {message}")]
    Failed { kind: String, message: String },
}

impl From<CoreError> for FfiError {
    fn from(err: CoreError) -> Self {
        FfiError::Failed {
            kind: format!("{:?}", err.kind),
            message: err.message,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum AiMode {
    ReadOnly,
    AllowEdits,
}

impl From<core_settings::AiMode> for AiMode {
    fn from(mode: core_settings::AiMode) -> Self {
        match mode {
            core_settings::AiMode::ReadOnly => AiMode::ReadOnly,
            core_settings::AiMode::AllowEdits => AiMode::AllowEdits,
        }
    }
}

impl From<AiMode> for core_settings::AiMode {
    fn from(mode: AiMode) -> Self {
        match mode {
            AiMode::ReadOnly => core_settings::AiMode::ReadOnly,
            AiMode::AllowEdits => core_settings::AiMode::AllowEdits,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ProviderSettings {
    pub id: String,
    pub enabled: bool,
    pub binary_path: Option<String>,
    pub model: Option<String>,
}

impl From<core_settings::ProviderSettings> for ProviderSettings {
    fn from(provider: core_settings::ProviderSettings) -> Self {
        Self {
            id: provider.id,
            enabled: provider.enabled,
            binary_path: provider.binary_path,
            model: provider.model,
        }
    }
}

impl From<ProviderSettings> for core_settings::ProviderSettings {
    fn from(provider: ProviderSettings) -> Self {
        Self {
            id: provider.id,
            enabled: provider.enabled,
            binary_path: provider.binary_path,
            model: provider.model,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Settings {
    pub hotkey_panel: String,
    pub hotkey_capture_ask: String,
    pub default_provider: String,
    pub providers: Vec<ProviderSettings>,
    pub ai_mode: AiMode,
    pub work_dir: Option<String>,
    pub allow_ai_screenshots: bool,
    pub index_roots: Vec<String>,
    pub index_exclude_globs: Vec<String>,
    pub max_content_file_bytes: u64,
    pub capture_retention_days: u32,
    pub turn_timeout_secs: u32,
    pub launch_at_login: bool,
}

impl From<core_settings::Settings> for Settings {
    fn from(settings: core_settings::Settings) -> Self {
        Self {
            hotkey_panel: settings.hotkey_panel,
            hotkey_capture_ask: settings.hotkey_capture_ask,
            default_provider: settings.default_provider,
            providers: settings.providers.into_iter().map(Into::into).collect(),
            ai_mode: settings.ai_mode.into(),
            work_dir: settings.work_dir,
            allow_ai_screenshots: settings.allow_ai_screenshots,
            index_roots: settings.index_roots,
            index_exclude_globs: settings.index_exclude_globs,
            max_content_file_bytes: settings.max_content_file_bytes,
            capture_retention_days: settings.capture_retention_days,
            turn_timeout_secs: settings.turn_timeout_secs,
            launch_at_login: settings.launch_at_login,
        }
    }
}

impl From<Settings> for core_settings::Settings {
    fn from(settings: Settings) -> Self {
        Self {
            hotkey_panel: settings.hotkey_panel,
            hotkey_capture_ask: settings.hotkey_capture_ask,
            default_provider: settings.default_provider,
            providers: settings.providers.into_iter().map(Into::into).collect(),
            ai_mode: settings.ai_mode.into(),
            work_dir: settings.work_dir,
            allow_ai_screenshots: settings.allow_ai_screenshots,
            index_roots: settings.index_roots,
            index_exclude_globs: settings.index_exclude_globs,
            max_content_file_bytes: settings.max_content_file_bytes,
            capture_retention_days: settings.capture_retention_days,
            turn_timeout_secs: settings.turn_timeout_secs,
            launch_at_login: settings.launch_at_login,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum FailureKind {
    NotInstalled,
    NotLoggedIn,
    RateLimited,
    Timeout,
    Cancelled,
    ProcessCrashed,
    ParseError,
    PermissionDenied,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum AskEvent {
    Started {
        conversation_id: String,
        provider_id: String,
    },
    SessionBound {
        session_id: String,
    },
    TextDelta {
        text: String,
    },
    TextFinal {
        text: String,
    },
    Tool {
        name: String,
        detail: String,
        finished: bool,
    },
    Usage {
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        cost_usd: Option<f64>,
    },
    Warning {
        message: String,
    },
    Completed,
    Failed {
        kind: FailureKind,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum IndexState {
    Idle,
    Scanning { done: u64 },
    Watching,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct IndexStatus {
    pub state: IndexState,
    pub roots: Vec<String>,
    pub files: u64,
    pub content_indexed: u64,
    pub db_bytes: u64,
    pub last_error: Option<String>,
}

#[uniffi::export(with_foreign)]
pub trait CoreListener: Send + Sync {
    fn on_ask_event(&self, ask_id: String, event: AskEvent);
    fn on_index_status(&self, status: IndexStatus);
    fn on_ai_capture(&self, path: String);
}

#[derive(uniffi::Object)]
pub struct TernCore {
    inner: Core,
    #[allow(dead_code)]
    listener: Arc<dyn CoreListener>,
}

#[uniffi::export]
impl TernCore {
    #[uniffi::constructor]
    pub fn new(
        home_dir: Option<String>,
        listener: Arc<dyn CoreListener>,
    ) -> Result<Arc<Self>, FfiError> {
        let inner = Core::open(home_dir.map(PathBuf::from))?;
        Ok(Arc::new(Self { inner, listener }))
    }

    pub fn version(&self) -> String {
        Core::version().to_string()
    }

    pub fn settings(&self) -> Settings {
        self.inner.settings().into()
    }

    pub fn update_settings(&self, settings: Settings) -> Result<(), FfiError> {
        self.inner.update_settings(settings.into())?;
        Ok(())
    }

    pub fn trim_memory(&self) {}

    pub fn shutdown(&self) {}
}
