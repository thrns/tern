use super::runner::classify_failure;
use super::{
    AgentEvent, Capabilities, CommandSpec, Detection, EventParser, ExitInfo, FailureKind,
    ImageSupport, McpInjection, Provider, ProviderId, TurnContext, TurnRequest,
};
use crate::error::{CoreError, ErrorKind};
use crate::procenv::ProcEnv;
use crate::settings::AiMode;
use serde_json::Value;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

const DETECT_TIMEOUT: Duration = Duration::from_secs(3);
const NONZERO_EXIT_MARKER: &str = " exited with ";
const PERMISSION_PROMPTS_MIN_VERSION: (u32, u32, u32) = (2, 1, 259);
const READ_ONLY_TOOLS: &str = "Read,Glob,Grep";
const ALLOW_EDITS_TOOLS: &str = "Read,Glob,Grep,Edit,Write";
const ALLOWED_MCP_TOOLS: &str =
    "mcp__tern__search_files,mcp__tern__read_file,mcp__tern__calculate,mcp__tern__take_screenshot";
const ATTACHMENTS_DIR: &str = "attachments";
const IMAGE_EXTENSIONS: [&str; 8] = ["png", "jpg", "jpeg", "gif", "webp", "heic", "bmp", "tiff"];
const TOOL_DETAIL_CHARS: usize = 200;
const MAX_CONSECUTIVE_INVALID_LINES: u32 = 20;

pub struct ClaudeProvider;

impl Provider for ClaudeProvider {
    fn id(&self) -> ProviderId {
        "claude"
    }

    fn display_name(&self) -> &'static str {
        "Claude Code"
    }

    fn binary_name(&self) -> &'static str {
        "claude"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            images: ImageSupport::ViaPath,
            resume: true,
            streaming_text: true,
            prompt_via_stdin: true,
            mcp_injection: McpInjection::CliFlag,
        }
    }

    fn detect(&self, env: &ProcEnv) -> Detection {
        let Some(binary) = env.find_binary(self.binary_name(), None) else {
            return Detection::default();
        };
        let mut notes = Vec::new();
        let version = match env.run_version(&binary, &["--version"], DETECT_TIMEOUT) {
            Ok(version) if !version.is_empty() => Some(version),
            Ok(_) => {
                notes.push("claude --version printed nothing".to_string());
                None
            }
            Err(err) => {
                notes.push(err.message);
                None
            }
        };
        let logged_in = match env.run_version(&binary, &["auth", "status"], DETECT_TIMEOUT) {
            Ok(_) => Some(true),
            Err(err) if err.message.contains(NONZERO_EXIT_MARKER) => Some(false),
            Err(err) => {
                notes.push(err.message);
                None
            }
        };
        Detection {
            installed: true,
            binary_path: Some(binary),
            version,
            logged_in,
            note: if notes.is_empty() {
                None
            } else {
                Some(notes.join("; "))
            },
        }
    }

    fn build_command(
        &self,
        turn: &TurnRequest,
        ctx: &TurnContext,
    ) -> Result<CommandSpec, CoreError> {
        let allow_edits = ctx.ai_mode == AiMode::AllowEdits;
        let cwd = if allow_edits {
            ctx.work_dir.clone().ok_or_else(|| {
                CoreError::new(
                    ErrorKind::InvalidInput,
                    "claude allow-edits mode needs a work directory",
                )
            })?
        } else {
            ctx.workspace_dir.clone()
        };
        let mut args: Vec<OsString> = Vec::new();
        push_flags(
            &mut args,
            &[
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--include-partial-messages",
                "--permission-mode",
                if allow_edits {
                    "acceptEdits"
                } else {
                    "dontAsk"
                },
            ],
        );
        let has_permission_prompts_flag = ctx
            .version
            .as_deref()
            .is_some_and(|version| version_at_least(version, PERMISSION_PROMPTS_MIN_VERSION));
        if has_permission_prompts_flag {
            push_flags(&mut args, &["--permission-prompts", "none"]);
        }
        push_flags(
            &mut args,
            &[
                "--tools",
                if allow_edits {
                    ALLOW_EDITS_TOOLS
                } else {
                    READ_ONLY_TOOLS
                },
                "--strict-mcp-config",
                "--mcp-config",
            ],
        );
        args.push(ctx.workspace_dir.join("mcp.json").into_os_string());
        push_flags(&mut args, &["--allowedTools", ALLOWED_MCP_TOOLS]);
        args.push(OsString::from("--append-system-prompt-file"));
        args.push(ctx.system_prompt_path.clone().into_os_string());
        if allow_edits {
            args.push(OsString::from("--add-dir"));
            args.push(ctx.workspace_dir.clone().into_os_string());
        }
        if let Some(model) = &turn.model {
            push_flags(&mut args, &["--model", model]);
        }
        if let Some(session_id) = &turn.resume_session_id {
            push_flags(&mut args, &["--resume", session_id]);
        }
        Ok(CommandSpec {
            program: ctx.binary.clone(),
            args,
            cwd,
            env: ctx.env.clone(),
            stdin: Some(prompt_with_images(turn, ctx, allow_edits)?.into_bytes()),
        })
    }

    fn new_parser(&self) -> Box<dyn EventParser> {
        Box::new(ClaudeParser::default())
    }
}

fn push_flags(args: &mut Vec<OsString>, flags: &[&str]) {
    args.extend(flags.iter().map(OsString::from));
}

fn is_image(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

fn prompt_with_images(
    turn: &TurnRequest,
    ctx: &TurnContext,
    absolute_paths: bool,
) -> Result<String, CoreError> {
    let mut lines = Vec::new();
    for attachment in turn.attachments.iter().filter(|path| is_image(path)) {
        let file_name = attachment.file_name().ok_or_else(|| {
            CoreError::new(
                ErrorKind::InvalidInput,
                format!("attachment has no file name: {}", attachment.display()),
            )
        })?;
        let reference: PathBuf = if absolute_paths {
            ctx.workspace_dir.join(ATTACHMENTS_DIR).join(file_name)
        } else {
            PathBuf::from(".").join(ATTACHMENTS_DIR).join(file_name)
        };
        lines.push(format!(
            "Attached image: {} (open it with the Read tool)",
            reference.display()
        ));
    }
    let mut text = turn.prompt.clone();
    if !lines.is_empty() {
        text.push_str("\n\n");
        text.push_str(&lines.join("\n"));
    }
    Ok(text)
}

pub fn version_at_least(version: &str, min: (u32, u32, u32)) -> bool {
    version
        .split(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .find_map(parse_triple)
        .is_some_and(|found| found >= min)
}

fn parse_triple(segment: &str) -> Option<(u32, u32, u32)> {
    let mut parts = segment.trim_matches('.').split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

#[derive(Debug, Default)]
pub struct ClaudeParser {
    terminal: bool,
    invalid_streak: u32,
    last_error: Option<String>,
}

impl EventParser for ClaudeParser {
    fn parse_line(&mut self, line: &str) -> Vec<AgentEvent> {
        if line.trim().is_empty() {
            return Vec::new();
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return self.invalid_line();
        };
        self.invalid_streak = 0;
        match value["type"].as_str() {
            Some("system") => self.system_events(&value),
            Some("stream_event") => text_delta_events(&value),
            Some("assistant") => self.assistant_events(&value),
            Some("user") => user_events(&value),
            Some("result") => self.result_events(&value),
            _ => Vec::new(),
        }
    }

    fn finish(&mut self, exit: &ExitInfo, stderr_tail: &str) -> Vec<AgentEvent> {
        if self.terminal {
            return Vec::new();
        }
        vec![classify_failure(
            exit,
            stderr_tail,
            self.last_error.as_deref(),
        )]
    }
}

impl ClaudeParser {
    fn invalid_line(&mut self) -> Vec<AgentEvent> {
        self.invalid_streak = self.invalid_streak.saturating_add(1);
        if self.invalid_streak == MAX_CONSECUTIVE_INVALID_LINES + 1 {
            return vec![AgentEvent::Warning {
                message: format!(
                    "More than {MAX_CONSECUTIVE_INVALID_LINES} consecutive provider output lines were not JSON and were ignored"
                ),
            }];
        }
        Vec::new()
    }

    fn remember_error(&mut self, value: &Value) {
        if let Some(error) = value["error"].as_str() {
            self.last_error = Some(error.to_string());
        }
    }

    fn system_events(&mut self, value: &Value) -> Vec<AgentEvent> {
        match value["subtype"].as_str() {
            Some("init") => value["session_id"]
                .as_str()
                .map(|session_id| {
                    vec![AgentEvent::SessionStarted {
                        session_id: session_id.to_string(),
                    }]
                })
                .unwrap_or_default(),
            Some("api_retry") => {
                self.remember_error(value);
                let message = match value["attempt"].as_u64() {
                    Some(attempt) => format!("Retrying (attempt {attempt})"),
                    None => "Retrying".to_string(),
                };
                vec![AgentEvent::Warning { message }]
            }
            _ => Vec::new(),
        }
    }

    fn assistant_events(&mut self, value: &Value) -> Vec<AgentEvent> {
        self.remember_error(value);
        value["message"]["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|block| block["type"] == "tool_use")
            .map(|block| AgentEvent::Tool {
                name: block["name"].as_str().unwrap_or_default().to_string(),
                detail: tool_detail(&block["input"]),
                finished: false,
            })
            .collect()
    }

    fn result_events(&mut self, value: &Value) -> Vec<AgentEvent> {
        self.terminal = true;
        let mut events = Vec::new();
        let is_error = value["is_error"] == true;
        let text = value["result"].as_str();
        if !is_error && let Some(text) = text {
            events.push(AgentEvent::TextFinal {
                text: text.to_string(),
            });
        }
        if let Some(usage) = usage_event(value) {
            events.push(usage);
        }
        if is_error {
            events.push(self.error_result_event(value, text));
        } else {
            events.push(AgentEvent::Completed);
        }
        events
    }

    fn error_result_event(&self, value: &Value, text: Option<&str>) -> AgentEvent {
        let message = match text.map(str::trim) {
            Some(text) if !text.is_empty() => text.to_string(),
            _ => format!(
                "claude turn failed ({})",
                value["subtype"].as_str().unwrap_or("unknown")
            ),
        };
        let evidence = match &self.last_error {
            Some(error) => format!("{error} {message}"),
            None => message.clone(),
        };
        let exit = ExitInfo {
            code: Some(1),
            signal: None,
            timed_out: false,
            cancelled: false,
        };
        let kind = match classify_failure(&exit, "", Some(&evidence)) {
            AgentEvent::Failed { kind, .. } => kind,
            _ => FailureKind::Unknown,
        };
        AgentEvent::Failed { kind, message }
    }
}

fn text_delta_events(value: &Value) -> Vec<AgentEvent> {
    let delta = &value["event"]["delta"];
    if delta["type"] != "text_delta" {
        return Vec::new();
    }
    match delta["text"].as_str() {
        Some(text) if !text.is_empty() => vec![AgentEvent::TextDelta {
            text: text.to_string(),
        }],
        _ => Vec::new(),
    }
}

fn user_events(value: &Value) -> Vec<AgentEvent> {
    value["message"]["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|block| block["type"] == "tool_result")
        .map(|_| AgentEvent::Tool {
            name: String::new(),
            detail: "result".to_string(),
            finished: true,
        })
        .collect()
}

fn usage_event(value: &Value) -> Option<AgentEvent> {
    let input_tokens = value["usage"]["input_tokens"].as_u64();
    let output_tokens = value["usage"]["output_tokens"].as_u64();
    let cost_usd = value["total_cost_usd"].as_f64();
    if input_tokens.is_none() && output_tokens.is_none() && cost_usd.is_none() {
        return None;
    }
    Some(AgentEvent::Usage {
        input_tokens,
        output_tokens,
        cost_usd,
    })
}

fn tool_detail(input: &Value) -> String {
    if input.is_null() {
        return String::new();
    }
    serde_json::to_string(input)
        .unwrap_or_default()
        .chars()
        .take(TOOL_DETAIL_CHARS)
        .collect()
}
