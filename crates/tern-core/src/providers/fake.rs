use super::runner::classify_failure;
use super::{
    AgentEvent, Capabilities, CommandSpec, Detection, EventParser, ExitInfo, FailureKind,
    ImageSupport, McpInjection, Provider, ProviderId, TurnContext, TurnRequest,
};
use crate::error::{CoreError, ErrorKind};
use crate::procenv::ProcEnv;
use serde_json::Value;
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
        Box::new(FakeParser::default())
    }
}

fn required_env(ctx: &TurnContext, key: &str) -> Result<OsString, CoreError> {
    ctx.env
        .iter()
        .find(|(name, _)| name.as_os_str() == OsStr::new(key))
        .map(|(_, value)| value.clone())
        .ok_or_else(|| CoreError::new(ErrorKind::Config, format!("{key} is not set")))
}

#[derive(Default)]
struct FakeParser {
    terminal: bool,
}

impl EventParser for FakeParser {
    fn parse_line(&mut self, line: &str) -> Vec<AgentEvent> {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        match value["type"].as_str() {
            Some("system") if value["subtype"] == "init" => value["session_id"]
                .as_str()
                .map(|session_id| {
                    vec![AgentEvent::SessionStarted {
                        session_id: session_id.to_string(),
                    }]
                })
                .unwrap_or_default(),
            Some("stream_event") if value["event"]["delta"]["type"] == "text_delta" => {
                value["event"]["delta"]["text"]
                    .as_str()
                    .map(|text| {
                        vec![AgentEvent::TextDelta {
                            text: text.to_string(),
                        }]
                    })
                    .unwrap_or_default()
            }
            Some("result") => self.result_events(&value),
            _ => Vec::new(),
        }
    }

    fn finish(&mut self, exit: &ExitInfo, stderr_tail: &str) -> Vec<AgentEvent> {
        if self.terminal {
            return Vec::new();
        }
        vec![classify_failure(exit, stderr_tail, None)]
    }
}

impl FakeParser {
    fn result_events(&mut self, value: &Value) -> Vec<AgentEvent> {
        self.terminal = true;
        let result = value["result"].as_str().unwrap_or_default().to_string();
        if value["is_error"] == true {
            vec![AgentEvent::Failed {
                kind: FailureKind::Unknown,
                message: result,
            }]
        } else {
            vec![
                AgentEvent::TextFinal { text: result },
                AgentEvent::Completed,
            ]
        }
    }
}
