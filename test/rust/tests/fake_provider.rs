use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;
use tern_core::error::ErrorKind;
use tern_core::providers::fake::FakeProvider;
use tern_core::providers::runner::{RunLimits, run_turn};
use tern_core::providers::{AgentEvent, Provider, TurnContext, TurnRequest};
use tern_core::settings::AiMode;

const EVENT_TIMEOUT: Duration = Duration::from_secs(30);

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/providers/fake")
}

fn turn() -> TurnRequest {
    TurnRequest {
        prompt: "say hello".to_string(),
        attachments: Vec::new(),
        resume_session_id: None,
        model: None,
    }
}

fn context(env: Vec<(OsString, OsString)>) -> TurnContext {
    TurnContext {
        binary: PathBuf::from("/bin/sh"),
        version: None,
        workspace_dir: fixture_dir(),
        work_dir: None,
        ai_mode: AiMode::ReadOnly,
        mcp_server_path: PathBuf::from("/nonexistent/tern-mcp"),
        home_dir: fixture_dir(),
        system_prompt_path: PathBuf::from("/nonexistent/system.md"),
        env,
    }
}

fn replay_env() -> Vec<(OsString, OsString)> {
    vec![
        (
            OsString::from("TERN_FAKE_PROVIDER"),
            fixture_dir().join("replay.sh").into_os_string(),
        ),
        (
            OsString::from("TERN_FAKE_FIXTURE"),
            fixture_dir().join("hello.jsonl").into_os_string(),
        ),
        (OsString::from("PATH"), OsString::from("/usr/bin:/bin")),
    ]
}

fn limits() -> RunLimits {
    RunLimits {
        total: Duration::from_secs(60),
        idle: Duration::from_secs(60),
        max_line_bytes: 1024 * 1024,
        max_final_text_bytes: 1024 * 1024,
    }
}

fn run_fake_turn(env: Vec<(OsString, OsString)>) -> Vec<AgentEvent> {
    let provider = FakeProvider;
    let spec = provider
        .build_command(&turn(), &context(env))
        .expect("fake command builds");
    let (sender, receiver) = mpsc::channel();
    let _handle = run_turn(
        spec,
        provider.new_parser(),
        limits(),
        Box::new(move |event| {
            let _ = sender.send(event);
        }),
    )
    .expect("fake turn starts");
    let mut events = Vec::new();
    loop {
        match receiver.recv_timeout(EVENT_TIMEOUT) {
            Ok(event) => events.push(event),
            Err(RecvTimeoutError::Disconnected) => return events,
            Err(RecvTimeoutError::Timeout) => {
                panic!("fake turn did not finish; seen: {events:?}")
            }
        }
    }
}

#[test]
fn hello_fixture_replays_as_a_full_turn() {
    let events = run_fake_turn(replay_env());
    assert_eq!(
        events,
        vec![
            AgentEvent::SessionStarted {
                session_id: "fake-session-1".to_string(),
            },
            AgentEvent::TextDelta {
                text: "Hello".to_string(),
            },
            AgentEvent::TextDelta {
                text: ", ".to_string(),
            },
            AgentEvent::TextDelta {
                text: "world".to_string(),
            },
            AgentEvent::TextFinal {
                text: "Hello, world".to_string(),
            },
            AgentEvent::Usage {
                input_tokens: Some(1),
                output_tokens: Some(3),
                cost_usd: Some(0.0),
            },
            AgentEvent::Completed,
        ]
    );
}

#[test]
fn build_command_fails_with_config_error_when_script_env_is_missing() {
    let env: Vec<_> = replay_env()
        .into_iter()
        .filter(|(key, _)| key != "TERN_FAKE_PROVIDER")
        .collect();
    let err = FakeProvider
        .build_command(&turn(), &context(env))
        .expect_err("missing script must fail");
    assert_eq!(err.kind, ErrorKind::Config);
    assert!(
        err.message.contains("TERN_FAKE_PROVIDER"),
        "{}",
        err.message
    );
}

#[test]
fn build_command_fails_with_config_error_when_fixture_env_is_missing() {
    let env: Vec<_> = replay_env()
        .into_iter()
        .filter(|(key, _)| key != "TERN_FAKE_FIXTURE")
        .collect();
    let err = FakeProvider
        .build_command(&turn(), &context(env))
        .expect_err("missing fixture must fail");
    assert_eq!(err.kind, ErrorKind::Config);
    assert!(err.message.contains("TERN_FAKE_FIXTURE"), "{}", err.message);
}

#[test]
fn command_runs_sh_with_script_then_fixture() {
    let spec = FakeProvider
        .build_command(&turn(), &context(replay_env()))
        .expect("fake command builds");
    assert_eq!(spec.program, PathBuf::from("/bin/sh"));
    assert_eq!(
        spec.args,
        vec![
            fixture_dir().join("replay.sh").into_os_string(),
            fixture_dir().join("hello.jsonl").into_os_string(),
        ]
    );
    assert_eq!(spec.stdin, None);
}
