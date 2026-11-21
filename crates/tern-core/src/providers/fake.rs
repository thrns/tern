use super::claude::ClaudeParser;
use super::{
    Capabilities, CommandSpec, Detection, EventParser, ImageSupport, McpInjection, Provider,
    ProviderId, TurnContext, TurnRequest,
};
use crate::error::{CoreError, ErrorKind};
use crate::procenv::ProcEnv;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

const SCRIPT_ENV: &str = "TERN_FAKE_PROVIDER";
const FIXTURE_ENV: &str = "TERN_FAKE_FIXTURE";
const SHELL: &str = "/bin/sh";

pub struct FakeProvider;

impl Provider for FakeProvider {
    fn id(&self) -> ProviderId {
        "fake"
    }

    fn display_name(&self) -> &'static str {
        "Fake"
    }

    fn binary_name(&self) -> &'static str {
        "sh"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            images: ImageSupport::No,
            resume: false,
            streaming_text: true,
            prompt_via_stdin: false,
            mcp_injection: McpInjection::None,
        }
    }

    fn detect(&self, _env: &ProcEnv) -> Detection {
        Detection {
            installed: true,
            binary_path: Some(PathBuf::from(SHELL)),
            version: None,
            logged_in: Some(true),
            note: None,
        }
    }

    fn build_command(
        &self,
        _turn: &TurnRequest,
        ctx: &TurnContext,
    ) -> Result<CommandSpec, CoreError> {
        let script = required_env(ctx, SCRIPT_ENV)?;
        let fixture = required_env(ctx, FIXTURE_ENV)?;
        Ok(CommandSpec {
            program: PathBuf::from(SHELL),
            args: vec![script, fixture],
            cwd: ctx.workspace_dir.clone(),
            env: ctx.env.clone(),
            stdin: None,
        })
    }

    fn new_parser(&self) -> Box<dyn EventParser> {
        Box::new(ClaudeParser::default())
    }
}

fn required_env(ctx: &TurnContext, key: &str) -> Result<OsString, CoreError> {
    ctx.env
        .iter()
        .find(|(name, _)| name.as_os_str() == OsStr::new(key))
        .map(|(_, value)| value.clone())
        .ok_or_else(|| CoreError::new(ErrorKind::Config, format!("{key} is not set")))
}
