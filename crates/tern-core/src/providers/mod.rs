pub mod agy;
pub mod claude;
pub mod codex;
pub mod fake;
pub mod gemini;
pub mod runner;

use crate::error::CoreError;
use crate::procenv::ProcEnv;
use crate::settings::AiMode;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;

pub type ProviderId = &'static str;

pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn display_name(&self) -> &'static str;
    fn binary_name(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;
    fn detect(&self, env: &ProcEnv) -> Detection;
    fn build_command(
        &self,
        turn: &TurnRequest,
        ctx: &TurnContext,
    ) -> Result<CommandSpec, CoreError>;
    fn new_parser(&self) -> Box<dyn EventParser>;
}

pub trait EventParser: Send {
    fn parse_line(&mut self, line: &str) -> Vec<AgentEvent>;
    fn finish(&mut self, exit: &ExitInfo, stderr_tail: &str) -> Vec<AgentEvent>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub images: ImageSupport,
    pub resume: bool,
    pub streaming_text: bool,
    pub prompt_via_stdin: bool,
    pub mcp_injection: McpInjection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSupport {
    Native,
    ViaPath,
    Unverified,
    No,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpInjection {
    CliFlag,
    ConfigOverride,
    WorkspaceFile,
    None,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Detection {
    pub installed: bool,
    pub binary_path: Option<PathBuf>,
    pub version: Option<String>,
    pub logged_in: Option<bool>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRequest {
    pub prompt: String,
    pub attachments: Vec<PathBuf>,
    pub resume_session_id: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TurnContext {
    pub binary: PathBuf,
    pub version: Option<String>,
    pub workspace_dir: PathBuf,
    pub work_dir: Option<PathBuf>,
    pub ai_mode: AiMode,
    pub mcp_server_path: PathBuf,
    pub home_dir: PathBuf,
    pub system_prompt_path: PathBuf,
    pub env: Vec<(OsString, OsString)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub env: Vec<(OsString, OsString)>,
    pub stdin: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitInfo {
    pub code: Option<i32>,
    pub signal: Option<i32>,
    pub timed_out: bool,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    SessionStarted {
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

#[derive(Clone)]
pub struct ProviderRegistry {
    providers: Vec<Arc<dyn Provider>>,
}

impl ProviderRegistry {
    pub fn new(providers: Vec<Arc<dyn Provider>>) -> Self {
        Self { providers }
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Provider>> {
        self.providers
            .iter()
            .find(|provider| provider.id() == id)
            .cloned()
    }

    pub fn all(&self) -> &[Arc<dyn Provider>] {
        &self.providers
    }
}
