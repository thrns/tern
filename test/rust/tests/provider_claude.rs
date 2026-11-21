use std::ffi::OsString;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use tern_core::error::ErrorKind;
use tern_core::procenv::ProcEnv;
use tern_core::providers::claude::{ClaudeParser, ClaudeProvider, version_at_least};
use tern_core::providers::{
    AgentEvent, EventParser, ExitInfo, FailureKind, ImageSupport, McpInjection, Provider,
    TurnContext, TurnRequest,
};
use tern_core::settings::AiMode;

static SCRATCH_COUNTER: AtomicU32 = AtomicU32::new(0);
static SPAWN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

const ALLOWED_MCP_TOOLS: &str =
    "mcp__tern__search_files,mcp__tern__read_file,mcp__tern__calculate,mcp__tern__take_screenshot";

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/providers/fake")
}

fn scratch_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tern-claude-{label}-{}-{}",
        std::process::id(),
        SCRATCH_COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn write_executable(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    let mut file = std::fs::File::create(&path).expect("create script");
    file.write_all(body.as_bytes()).expect("write script");
    drop(file);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

fn request(prompt: &str) -> TurnRequest {
    TurnRequest {
        prompt: prompt.to_string(),
        attachments: Vec::new(),
        resume_session_id: None,
        model: None,
    }
}

fn context(mode: AiMode, version: Option<&str>) -> TurnContext {
    TurnContext {
        binary: PathBuf::from("/opt/bin/claude"),
        version: version.map(str::to_string),
        workspace_dir: PathBuf::from("/ws/conv-1"),
        work_dir: Some(PathBuf::from("/Users/me/project")),
        ai_mode: mode,
        mcp_server_path: PathBuf::from("/app/tern-mcp"),
        home_dir: PathBuf::from("/home/tern"),
        system_prompt_path: PathBuf::from("/home/tern/prompts/system.md"),
        env: vec![(OsString::from("HOME"), OsString::from("/Users/me"))],
    }
}

fn args_of(spec: &tern_core::providers::CommandSpec) -> Vec<String> {
    spec.args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

fn parse(lines: &[&str]) -> Vec<AgentEvent> {
    let mut parser = ClaudeParser::default();
    lines
        .iter()
        .flat_map(|line| parser.parse_line(line))
        .collect()
}

fn exit_code(code: i32) -> ExitInfo {
    ExitInfo {
        code: Some(code),
        signal: None,
        timed_out: false,
        cancelled: false,
    }
}

fn delta(text: &str) -> String {
    format!(
        r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{text}"}}}}}}"#
    )
}

#[test]
fn identity_and_capabilities() {
    let provider = ClaudeProvider;
    assert_eq!(provider.id(), "claude");
    assert_eq!(provider.display_name(), "Claude Code");
    assert_eq!(provider.binary_name(), "claude");
    let caps = provider.capabilities();
    assert_eq!(caps.images, ImageSupport::ViaPath);
    assert!(caps.resume);
    assert!(caps.streaming_text);
    assert!(caps.prompt_via_stdin);
    assert_eq!(caps.mcp_injection, McpInjection::CliFlag);
}

#[test]
fn read_only_command_snapshot() {
    let spec = ClaudeProvider
        .build_command(
            &request("what is 2+2"),
            &context(AiMode::ReadOnly, Some("2.1.289 (Claude Code)")),
        )
        .expect("command builds");
    assert_eq!(spec.program, PathBuf::from("/opt/bin/claude"));
    assert_eq!(spec.cwd, PathBuf::from("/ws/conv-1"));
    assert_eq!(spec.stdin, Some(b"what is 2+2".to_vec()));
    assert_eq!(
        spec.env,
        vec![(OsString::from("HOME"), OsString::from("/Users/me"))]
    );
    assert_eq!(
        args_of(&spec),
        strings(&[
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            "--permission-mode",
            "dontAsk",
            "--permission-prompts",
            "none",
            "--tools",
            "Read,Glob,Grep",
            "--strict-mcp-config",
            "--mcp-config",
            "/ws/conv-1/mcp.json",
            "--allowedTools",
            ALLOWED_MCP_TOOLS,
            "--append-system-prompt-file",
            "/home/tern/prompts/system.md",
        ])
    );
}

#[test]
fn allow_edits_command_snapshot() {
    let spec = ClaudeProvider
        .build_command(
            &request("fix the typo"),
            &context(AiMode::AllowEdits, Some("2.1.300")),
        )
        .expect("command builds");
    assert_eq!(spec.cwd, PathBuf::from("/Users/me/project"));
    assert_eq!(
        args_of(&spec),
        strings(&[
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            "--permission-mode",
            "acceptEdits",
            "--permission-prompts",
            "none",
            "--tools",
            "Read,Glob,Grep,Edit,Write",
            "--strict-mcp-config",
            "--mcp-config",
            "/ws/conv-1/mcp.json",
            "--allowedTools",
            ALLOWED_MCP_TOOLS,
            "--append-system-prompt-file",
            "/home/tern/prompts/system.md",
            "--add-dir",
            "/ws/conv-1",
        ])
    );
}

#[test]
fn allow_edits_without_work_dir_is_an_invalid_input_error() {
    let mut ctx = context(AiMode::AllowEdits, None);
    ctx.work_dir = None;
    let err = ClaudeProvider
        .build_command(&request("hi"), &ctx)
        .expect_err("work dir is required");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

#[test]
fn resume_and_model_flags_come_last() {
    let mut turn = request("again");
    turn.model = Some("sonnet".to_string());
    turn.resume_session_id = Some("sess-42".to_string());
    let spec = ClaudeProvider
        .build_command(&turn, &context(AiMode::ReadOnly, Some("2.1.289")))
        .expect("command builds");
    let args = args_of(&spec);
    let tail = &args[args.len() - 4..];
    assert_eq!(tail, strings(&["--model", "sonnet", "--resume", "sess-42"]));
}

#[test]
fn old_version_omits_permission_prompts_flag() {
    for version in [Some("2.1.258"), Some("1.9.999"), Some("unknown"), None] {
        let spec = ClaudeProvider
            .build_command(&request("hi"), &context(AiMode::ReadOnly, version))
            .expect("command builds");
        let args = args_of(&spec);
        assert!(
            !args.iter().any(|arg| arg == "--permission-prompts"),
            "{version:?}: {args:?}"
        );
        assert!(args.iter().any(|arg| arg == "dontAsk"));
    }
}

#[test]
fn image_attachments_add_trailing_lines_in_read_only_mode() {
    let mut turn = request("describe these");
    turn.attachments = vec![
        PathBuf::from("/ws/conv-1/attachments/att-1-shot.PNG"),
        PathBuf::from("/ws/conv-1/attachments/att-2-notes.txt"),
        PathBuf::from("/ws/conv-1/attachments/att-3-photo.jpeg"),
    ];
    let spec = ClaudeProvider
        .build_command(&turn, &context(AiMode::ReadOnly, None))
        .expect("command builds");
    assert_eq!(
        spec.stdin,
        Some(
            b"describe these\n\nAttached image: ./attachments/att-1-shot.PNG (open it with the Read tool)\nAttached image: ./attachments/att-3-photo.jpeg (open it with the Read tool)"
                .to_vec()
        )
    );
}

#[test]
fn image_attachments_use_absolute_paths_in_allow_edits_mode() {
    let mut turn = request("describe");
    turn.attachments = vec![PathBuf::from("/ws/conv-1/attachments/att-1-shot.png")];
    let spec = ClaudeProvider
        .build_command(&turn, &context(AiMode::AllowEdits, None))
        .expect("command builds");
    assert_eq!(
        spec.stdin,
        Some(
            b"describe\n\nAttached image: /ws/conv-1/attachments/att-1-shot.png (open it with the Read tool)"
                .to_vec()
        )
    );
}

#[test]
fn version_at_least_parses_the_first_triple() {
    assert!(version_at_least("2.1.300 (Claude Code)", (2, 1, 259)));
    assert!(version_at_least("2.1.259", (2, 1, 259)));
    assert!(version_at_least("v3.0.0", (2, 1, 259)));
    assert!(version_at_least("claude 2.2.0-beta.1", (2, 1, 259)));
    assert!(version_at_least("2.1.259.4", (2, 1, 259)));
    assert!(!version_at_least("2.1.258", (2, 1, 259)));
    assert!(!version_at_least("2.0.999 (Claude Code)", (2, 1, 259)));
    assert!(!version_at_least("1.99.99", (2, 1, 259)));
    assert!(!version_at_least("2.1", (2, 1, 0)));
    assert!(!version_at_least("", (0, 0, 0)));
    assert!(!version_at_least("no digits here", (0, 0, 0)));
    assert!(version_at_least("build 7 version 2.1.300", (2, 1, 259)));
}

#[test]
fn text_turn_maps_to_expected_events() {
    let init =
        r#"{"type":"system","subtype":"init","session_id":"s-1","tools":["Glob","Grep","Read"]}"#;
    let result = r#"{"type":"result","subtype":"success","is_error":false,"result":"hello fixture","session_id":"s-1","usage":{"input_tokens":12,"output_tokens":5},"total_cost_usd":0.0123}"#;
    let events = parse(&[
        init,
        r#"{"type":"stream_event","event":{"type":"message_start"}}"#,
        &delta("hello "),
        &delta("fixture"),
        &delta(""),
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hello fixture"}]}}"#,
        result,
    ]);
    assert_eq!(
        events,
        vec![
            AgentEvent::SessionStarted {
                session_id: "s-1".to_string()
            },
            AgentEvent::TextDelta {
                text: "hello ".to_string()
            },
            AgentEvent::TextDelta {
                text: "fixture".to_string()
            },
            AgentEvent::TextFinal {
                text: "hello fixture".to_string()
            },
            AgentEvent::Usage {
                input_tokens: Some(12),
                output_tokens: Some(5),
                cost_usd: Some(0.0123)
            },
            AgentEvent::Completed,
        ]
    );
}

#[test]
fn fake_hello_fixture_parses_like_a_claude_turn() {
    let text = std::fs::read_to_string(fixture_dir().join("hello.jsonl")).expect("fixture");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        parse(&lines),
        vec![
            AgentEvent::SessionStarted {
                session_id: "fake-session-1".to_string()
            },
            AgentEvent::TextDelta {
                text: "Hello".to_string()
            },
            AgentEvent::TextDelta {
                text: ", ".to_string()
            },
            AgentEvent::TextDelta {
                text: "world".to_string()
            },
            AgentEvent::TextFinal {
                text: "Hello, world".to_string()
            },
            AgentEvent::Usage {
                input_tokens: Some(1),
                output_tokens: Some(3),
                cost_usd: Some(0.0)
            },
            AgentEvent::Completed,
        ]
    );
}

#[test]
fn tool_use_and_tool_result_map_to_tool_events() {
    let events = parse(&[
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Reading"},{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/tmp/a.txt"}},{"type":"tool_use","id":"t2","name":"Glob","input":{"pattern":"*.md"}}]}}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"abc"}]}}"#,
        r#"{"type":"user","message":{"content":"plain user text"}}"#,
    ]);
    assert_eq!(
        events,
        vec![
            AgentEvent::Tool {
                name: "Read".to_string(),
                detail: r#"{"file_path":"/tmp/a.txt"}"#.to_string(),
                finished: false
            },
            AgentEvent::Tool {
                name: "Glob".to_string(),
                detail: r#"{"pattern":"*.md"}"#.to_string(),
                finished: false
            },
            AgentEvent::Tool {
                name: String::new(),
                detail: "result".to_string(),
                finished: true
            },
        ]
    );
}

#[test]
fn huge_tool_input_is_truncated_to_200_chars() {
    let huge = "é".repeat(5000);
    let line = format!(
        r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","name":"Write","input":{{"content":"{huge}"}}}}]}}}}"#
    );
    let events = parse(&[&line]);
    assert_eq!(events.len(), 1);
    match &events[0] {
        AgentEvent::Tool {
            name,
            detail,
            finished,
        } => {
            assert_eq!(name, "Write");
            assert_eq!(detail.chars().count(), 200);
            assert!(detail.starts_with(r#"{"content":"é"#));
            assert!(!finished);
        }
        other => panic!("unexpected event {other:?}"),
    }
}

#[test]
fn tool_use_without_input_has_empty_detail() {
    let events = parse(&[
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read"}]}}"#,
    ]);
    assert_eq!(
        events,
        vec![AgentEvent::Tool {
            name: "Read".to_string(),
            detail: String::new(),
            finished: false
        }]
    );
}

#[test]
fn api_retry_becomes_a_warning() {
    let events = parse(&[
        r#"{"type":"system","subtype":"api_retry","attempt":2,"max_retries":5,"error":"overloaded"}"#,
        r#"{"type":"system","subtype":"api_retry"}"#,
    ]);
    assert_eq!(
        events,
        vec![
            AgentEvent::Warning {
                message: "Retrying (attempt 2)".to_string()
            },
            AgentEvent::Warning {
                message: "Retrying".to_string()
            },
        ]
    );
}

#[test]
fn init_without_session_id_emits_nothing() {
    assert!(parse(&[r#"{"type":"system","subtype":"init","tools":[]}"#]).is_empty());
}

#[test]
fn unknown_types_and_subtypes_are_ignored() {
    let events = parse(&[
        r#"{"type":"rate_limit_event","info":{}}"#,
        r#"{"type":"system","subtype":"hook_started"}"#,
        r#"{"type":"stream_event","event":{"delta":{"type":"input_json_delta","partial_json":"{"}}}"#,
        r#"{"no_type":true}"#,
        "42",
        r#""text""#,
        "",
    ]);
    assert!(events.is_empty());
}

#[test]
fn result_with_non_string_result_completes_without_text_final() {
    let events = parse(&[
        r#"{"type":"result","subtype":"success","is_error":false,"result":{"structured":true},"usage":{"input_tokens":1}}"#,
    ]);
    assert_eq!(
        events,
        vec![
            AgentEvent::Usage {
                input_tokens: Some(1),
                output_tokens: None,
                cost_usd: None
            },
            AgentEvent::Completed,
        ]
    );
}

#[test]
fn result_without_usage_emits_no_usage_event() {
    let events = parse(&[r#"{"type":"result","is_error":false,"result":"ok"}"#]);
    assert_eq!(
        events,
        vec![
            AgentEvent::TextFinal {
                text: "ok".to_string()
            },
            AgentEvent::Completed,
        ]
    );
}

#[test]
fn error_result_fails_with_the_message_and_is_terminal() {
    let mut parser = ClaudeParser::default();
    let events = parser.parse_line(
        r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"Something broke","total_cost_usd":0.5}"#,
    );
    assert_eq!(
        events,
        vec![
            AgentEvent::Usage {
                input_tokens: None,
                output_tokens: None,
                cost_usd: Some(0.5)
            },
            AgentEvent::Failed {
                kind: FailureKind::Unknown,
                message: "Something broke".to_string()
            },
        ]
    );
    assert!(parser.finish(&exit_code(1), "").is_empty());
}

#[test]
fn error_result_without_a_message_names_the_subtype() {
    let events =
        parse(&[r#"{"type":"result","subtype":"error_max_turns","is_error":true,"result":null}"#]);
    assert_eq!(
        events,
        vec![AgentEvent::Failed {
            kind: FailureKind::Unknown,
            message: "claude turn failed (error_max_turns)".to_string()
        }]
    );
}

#[test]
fn authentication_failure_is_classified_as_not_logged_in() {
    let events = parse(&[
        r#"{"type":"system","subtype":"init","session_id":"s-9"}"#,
        r#"{"type":"assistant","error":"authentication_failed","message":{"content":[{"type":"text","text":"Failed to authenticate: OAuth session expired and could not be refreshed"}]}}"#,
        r#"{"type":"result","subtype":"success","is_error":true,"result":"Failed to authenticate: OAuth session expired and could not be refreshed","total_cost_usd":0,"usage":{"input_tokens":0,"output_tokens":0}}"#,
    ]);
    assert_eq!(
        events,
        vec![
            AgentEvent::SessionStarted {
                session_id: "s-9".to_string()
            },
            AgentEvent::Usage {
                input_tokens: Some(0),
                output_tokens: Some(0),
                cost_usd: Some(0.0)
            },
            AgentEvent::Failed {
                kind: FailureKind::NotLoggedIn,
                message: "Failed to authenticate: OAuth session expired and could not be refreshed"
                    .to_string()
            },
        ]
    );
}

#[test]
fn rate_limit_error_result_is_classified_as_rate_limited() {
    let events = parse(&[
        r#"{"type":"result","is_error":true,"result":"API Error: 429 Too Many Requests"}"#,
    ]);
    assert_eq!(
        events,
        vec![AgentEvent::Failed {
            kind: FailureKind::RateLimited,
            message: "API Error: 429 Too Many Requests".to_string()
        }]
    );
}

#[test]
fn finish_uses_exit_rules_when_no_result_line_arrived() {
    let mut parser = ClaudeParser::default();
    assert_eq!(
        parser.finish(&exit_code(0), ""),
        vec![AgentEvent::Completed]
    );

    let mut parser = ClaudeParser::default();
    let cancelled = ExitInfo {
        code: None,
        signal: Some(15),
        timed_out: false,
        cancelled: true,
    };
    assert_eq!(
        parser.finish(&cancelled, ""),
        vec![AgentEvent::Failed {
            kind: FailureKind::Cancelled,
            message: "turn cancelled".to_string()
        }]
    );

    let mut parser = ClaudeParser::default();
    let events = parser.finish(&exit_code(1), "Error: please run claude login");
    assert!(matches!(
        events.as_slice(),
        [AgentEvent::Failed {
            kind: FailureKind::NotLoggedIn,
            ..
        }]
    ));
}

#[test]
fn finish_uses_the_last_error_event_for_classification() {
    let mut parser = ClaudeParser::default();
    parser
        .parse_line(r#"{"type":"system","subtype":"api_retry","attempt":3,"error":"rate_limit"}"#);
    let events = parser.finish(&exit_code(1), "");
    assert!(matches!(
        events.as_slice(),
        [AgentEvent::Failed {
            kind: FailureKind::RateLimited,
            ..
        }]
    ));
}

#[test]
fn twenty_one_consecutive_invalid_lines_warn_once() {
    let mut parser = ClaudeParser::default();
    let mut warnings = 0;
    for _ in 0..40 {
        for event in parser.parse_line("not json") {
            assert!(matches!(event, AgentEvent::Warning { .. }));
            warnings += 1;
        }
    }
    assert_eq!(warnings, 1);
}

#[test]
fn twenty_invalid_lines_do_not_warn_and_valid_lines_reset_the_streak() {
    let mut parser = ClaudeParser::default();
    for _ in 0..20 {
        assert!(parser.parse_line("garbage").is_empty());
    }
    assert!(parser.parse_line(r#"{"type":"noop"}"#).is_empty());
    for _ in 0..20 {
        assert!(parser.parse_line("garbage").is_empty());
    }
    assert_eq!(parser.parse_line("garbage").len(), 1);
}

#[test]
fn detect_reports_version_and_login_state() {
    let _guard = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch_dir("detect-ok");
    let binary = write_executable(
        &dir,
        "claude",
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo '2.1.300 (Claude Code)'; exit 0; fi\nif [ \"$1\" = \"auth\" ]; then echo '{\"loggedIn\":true}'; exit 0; fi\nexit 2\n",
    );
    let env = ProcEnv::from_parts(
        dir.display().to_string(),
        &[],
        PathBuf::from("/nonexistent-home"),
    );
    let detection = ClaudeProvider.detect(&env);
    assert!(detection.installed);
    assert_eq!(detection.binary_path, Some(binary));
    assert_eq!(detection.version.as_deref(), Some("2.1.300 (Claude Code)"));
    assert_eq!(detection.logged_in, Some(true));
    assert_eq!(detection.note, None);
}

#[test]
fn detect_reports_logged_out_when_auth_status_exits_nonzero() {
    let _guard = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch_dir("detect-out");
    write_executable(
        &dir,
        "claude",
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo '2.1.289 (Claude Code)'; exit 0; fi\necho '{\"loggedIn\":false}'\nexit 1\n",
    );
    let env = ProcEnv::from_parts(
        dir.display().to_string(),
        &[],
        PathBuf::from("/nonexistent-home"),
    );
    let detection = ClaudeProvider.detect(&env);
    assert!(detection.installed);
    assert_eq!(detection.version.as_deref(), Some("2.1.289 (Claude Code)"));
    assert_eq!(detection.logged_in, Some(false));
    assert_eq!(detection.note, None);
}

#[test]
fn detect_reports_not_installed_when_binary_is_missing() {
    let dir = scratch_dir("detect-missing");
    let env = ProcEnv::from_parts(
        dir.display().to_string(),
        &[],
        PathBuf::from("/nonexistent-home"),
    );
    let detection = ClaudeProvider.detect(&env);
    assert!(!detection.installed);
    assert_eq!(detection.binary_path, None);
    assert_eq!(detection.version, None);
    assert_eq!(detection.logged_in, None);
}

#[test]
fn detect_times_out_hung_commands_and_leaves_login_unknown() {
    let _guard = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch_dir("detect-hang");
    write_executable(&dir, "claude", "#!/bin/sh\n/bin/sleep 30\n");
    let env = ProcEnv::from_parts(
        dir.display().to_string(),
        &[],
        PathBuf::from("/nonexistent-home"),
    );
    let started = std::time::Instant::now();
    let detection = ClaudeProvider.detect(&env);
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
    assert!(detection.installed);
    assert_eq!(detection.version, None);
    assert_eq!(detection.logged_in, None);
    assert!(detection.note.is_some());
}
