use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tern_core::error::{CoreError, ErrorKind};
use tern_core::procenv::ProcEnv;
use tern_core::providers::runner::{RunHandle, RunLimits, classify_failure, run_turn};
use tern_core::providers::{
    AgentEvent, Capabilities, CommandSpec, Detection, EventParser, ExitInfo, FailureKind,
    ImageSupport, McpInjection, Provider, ProviderId, ProviderRegistry, TurnContext, TurnRequest,
};

static SCRATCH_COUNTER: AtomicU32 = AtomicU32::new(0);

const RUN_CEILING: Duration = Duration::from_secs(60);
const DEATH_CEILING: Duration = Duration::from_secs(10);
const GRANDCHILD_SCRIPT: &str = "sleep 100 &\necho $! > \"$1\"\necho delta:ready\nwait\n";

struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!(
            "tern-runner-{label}-{}-{}",
            std::process::id(),
            SCRATCH_COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Scratch { dir }
    }

    fn write(&self, name: &str, body: &str, mode: u32) -> PathBuf {
        let path = self.dir.join(name);
        std::fs::write(&path, body).expect("write file");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).expect("chmod");
        path
    }

    fn spec(&self, body: &str, args: &[&Path]) -> CommandSpec {
        let script = self.write("fake-cli.sh", body, 0o755);
        let mut all_args: Vec<OsString> = vec![script.into_os_string()];
        all_args.extend(args.iter().map(|arg| arg.as_os_str().to_os_string()));
        CommandSpec {
            program: PathBuf::from("/bin/sh"),
            args: all_args,
            cwd: self.dir.clone(),
            env: vec![(OsString::from("PATH"), OsString::from("/usr/bin:/bin"))],
            stdin: None,
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FinishMode {
    Classify,
    Silent,
}

#[derive(Clone, Default)]
struct Probe {
    longest_line: Arc<AtomicUsize>,
    stderr_tail: Arc<Mutex<Option<String>>>,
    exit: Arc<Mutex<Option<ExitInfo>>>,
}

struct ScriptParser {
    mode: FinishMode,
    probe: Probe,
}

impl ScriptParser {
    fn classifying() -> Box<dyn EventParser> {
        Box::new(ScriptParser {
            mode: FinishMode::Classify,
            probe: Probe::default(),
        })
    }

    fn silent() -> Box<dyn EventParser> {
        Box::new(ScriptParser {
            mode: FinishMode::Silent,
            probe: Probe::default(),
        })
    }

    fn probed(mode: FinishMode, probe: &Probe) -> Box<dyn EventParser> {
        Box::new(ScriptParser {
            mode,
            probe: probe.clone(),
        })
    }
}

impl EventParser for ScriptParser {
    fn parse_line(&mut self, line: &str) -> Vec<AgentEvent> {
        self.probe
            .longest_line
            .fetch_max(line.len(), Ordering::SeqCst);
        if let Some(text) = line.strip_prefix("delta:") {
            return vec![AgentEvent::TextDelta {
                text: text.to_string(),
            }];
        }
        if let Some(text) = line.strip_prefix("final:") {
            return vec![AgentEvent::TextFinal {
                text: text.to_string(),
            }];
        }
        if let Some(id) = line.strip_prefix("session:") {
            return vec![AgentEvent::SessionStarted {
                session_id: id.to_string(),
            }];
        }
        if let Some(message) = line.strip_prefix("warn:") {
            return vec![AgentEvent::Warning {
                message: message.to_string(),
            }];
        }
        if line == "complete" {
            return vec![AgentEvent::Completed];
        }
        Vec::new()
    }

    fn finish(&mut self, exit: &ExitInfo, stderr_tail: &str) -> Vec<AgentEvent> {
        *self.probe.stderr_tail.lock().expect("probe lock") = Some(stderr_tail.to_string());
        *self.probe.exit.lock().expect("probe lock") = Some(*exit);
        match self.mode {
            FinishMode::Classify => vec![classify_failure(exit, stderr_tail, None)],
            FinishMode::Silent => Vec::new(),
        }
    }
}

fn limits() -> RunLimits {
    RunLimits {
        total: Duration::from_secs(60),
        idle: Duration::from_secs(60),
        max_line_bytes: 1024 * 1024,
        max_final_text_bytes: 1024 * 1024,
    }
}

struct Session {
    handle: RunHandle,
    events: Receiver<AgentEvent>,
    seen: Vec<AgentEvent>,
}

impl Session {
    fn start(spec: CommandSpec, parser: Box<dyn EventParser>, limits: RunLimits) -> Session {
        let (sender, events) = mpsc::channel();
        let handle = run_turn(
            spec,
            parser,
            limits,
            Box::new(move |event| {
                let _ = sender.send(event);
            }),
        )
        .expect("run_turn");
        Session {
            handle,
            events,
            seen: Vec::new(),
        }
    }

    fn wait_for(&mut self, matches: impl Fn(&AgentEvent) -> bool) {
        let deadline = Instant::now() + RUN_CEILING;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(remaining) {
                Ok(event) => {
                    let hit = matches(&event);
                    self.seen.push(event);
                    if hit {
                        return;
                    }
                }
                Err(err) => panic!(
                    "expected event never arrived ({err}); seen: {:?}",
                    self.seen
                ),
            }
        }
    }

    fn cancel(&self) {
        self.handle.cancel.store(true, Ordering::SeqCst);
    }

    fn finish(mut self) -> Vec<AgentEvent> {
        let deadline = Instant::now() + RUN_CEILING;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(remaining) {
                Ok(event) => self.seen.push(event),
                Err(RecvTimeoutError::Disconnected) => return self.seen,
                Err(RecvTimeoutError::Timeout) => {
                    panic!("run did not finish; seen: {:?}", self.seen)
                }
            }
        }
    }
}

fn run(spec: CommandSpec, parser: Box<dyn EventParser>, limits: RunLimits) -> Vec<AgentEvent> {
    Session::start(spec, parser, limits).finish()
}

fn run_script(body: &str) -> Vec<AgentEvent> {
    let scratch = Scratch::new("script");
    run(
        scratch.spec(body, &[]),
        ScriptParser::classifying(),
        limits(),
    )
}

fn delta(text: &str) -> AgentEvent {
    AgentEvent::TextDelta {
        text: text.to_string(),
    }
}

fn final_text(text: &str) -> AgentEvent {
    AgentEvent::TextFinal {
        text: text.to_string(),
    }
}

fn failure_kind(events: &[AgentEvent]) -> FailureKind {
    match events.last() {
        Some(AgentEvent::Failed { kind, .. }) => *kind,
        other => panic!("expected a final Failed event, got {other:?} in {events:?}"),
    }
}

fn failure_message(events: &[AgentEvent]) -> String {
    match events.last() {
        Some(AgentEvent::Failed { message, .. }) => message.clone(),
        other => panic!("expected a final Failed event, got {other:?} in {events:?}"),
    }
}

fn assert_one_terminal_event_at_end(events: &[AgentEvent]) {
    let terminals = events
        .iter()
        .filter(|event| matches!(event, AgentEvent::Completed | AgentEvent::Failed { .. }))
        .count();
    assert_eq!(
        terminals, 1,
        "expected exactly one terminal event: {events:?}"
    );
    assert!(
        matches!(
            events.last(),
            Some(AgentEvent::Completed | AgentEvent::Failed { .. })
        ),
        "terminal event must be last: {events:?}"
    );
}

fn read_pid(path: &Path) -> libc::pid_t {
    let deadline = Instant::now() + DEATH_CEILING;
    loop {
        if let Ok(text) = std::fs::read_to_string(path)
            && let Ok(pid) = text.trim().parse::<libc::pid_t>()
        {
            return pid;
        }
        assert!(Instant::now() < deadline, "pid file never appeared");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn is_zombie(pid: libc::pid_t) -> bool {
    Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .trim_start()
                .starts_with('Z')
        })
        .unwrap_or(false)
}

fn process_alive(pid: libc::pid_t) -> bool {
    let rc = unsafe { libc::kill(pid, 0) };
    if rc != 0 {
        return std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH);
    }
    !is_zombie(pid)
}

fn wait_until_dead(pid: libc::pid_t) -> bool {
    let deadline = Instant::now() + DEATH_CEILING;
    while Instant::now() < deadline {
        if !process_alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    !process_alive(pid)
}

fn process_group_of(pid: libc::pid_t) -> libc::pid_t {
    unsafe { libc::getpgid(pid) }
}

fn assert_grandchild_dies(session_end: impl FnOnce(&mut Session), tight: RunLimits) {
    let scratch = Scratch::new("grandchild");
    let pid_file = scratch.dir.join("grandchild.pid");
    let spec = scratch.spec(GRANDCHILD_SCRIPT, &[&pid_file]);
    let mut session = Session::start(spec, ScriptParser::classifying(), tight);
    session.wait_for(|event| matches!(event, AgentEvent::TextDelta { .. }));
    let grandchild = read_pid(&pid_file);
    let leader = libc::pid_t::try_from(session.handle.pid).expect("pid fits");
    assert_ne!(grandchild, leader, "grandchild must be a separate process");
    assert!(
        process_alive(grandchild),
        "grandchild must be running before the turn is stopped"
    );
    assert_eq!(
        process_group_of(grandchild),
        leader,
        "grandchild must share the leader's process group"
    );
    session_end(&mut session);
    let events = session.finish();
    assert_one_terminal_event_at_end(&events);
    assert!(
        wait_until_dead(grandchild),
        "grandchild {grandchild} survived the end of the turn"
    );
    assert!(
        wait_until_dead(leader),
        "direct child {leader} survived the end of the turn"
    );
}

#[test]
fn normal_lines_stream_in_order_and_final_text_is_synthesized() {
    let events = run_script(
        "echo session:abc\necho delta:Hello\necho delta:', '\necho delta:world\necho ignored\n",
    );
    assert_eq!(
        events,
        vec![
            AgentEvent::SessionStarted {
                session_id: "abc".to_string()
            },
            delta("Hello"),
            delta(", "),
            delta("world"),
            final_text("Hello, world"),
            AgentEvent::Completed,
        ]
    );
}

#[test]
fn parser_final_text_is_not_duplicated() {
    let events = run_script("echo delta:a\necho final:ab\n");
    assert_eq!(
        events,
        vec![delta("a"), final_text("ab"), AgentEvent::Completed]
    );
}

#[test]
fn zero_output_process_completes_without_text() {
    let events = run_script("exit 0\n");
    assert_eq!(events, vec![AgentEvent::Completed]);
}

#[test]
fn events_after_the_terminal_event_are_dropped() {
    let events = run_script("echo delta:x\necho complete\necho delta:late\n");
    assert_eq!(
        events,
        vec![delta("x"), final_text("x"), AgentEvent::Completed]
    );
}

#[test]
fn runner_supplies_terminal_event_when_parser_finish_is_silent() {
    let scratch = Scratch::new("silent");
    let ok = run(
        scratch.spec("echo delta:hi\n", &[]),
        ScriptParser::silent(),
        limits(),
    );
    assert_eq!(
        ok,
        vec![delta("hi"), final_text("hi"), AgentEvent::Completed]
    );
    let failed = run(
        scratch.spec("echo boom >&2\nexit 3\n", &[]),
        ScriptParser::silent(),
        limits(),
    );
    assert_eq!(failure_kind(&failed), FailureKind::Unknown);
    assert!(failure_message(&failed).contains("boom"));
}

#[test]
fn oversize_line_is_skipped_with_a_warning_and_never_reaches_the_parser() {
    let scratch = Scratch::new("oversize");
    let probe = Probe::default();
    let script =
        "echo delta:before\nhead -c 3000000 /dev/zero | tr '\\0' 'x'\necho\necho delta:after\n";
    let mut capped = limits();
    capped.max_line_bytes = 1024;
    let events = run(
        scratch.spec(script, &[]),
        ScriptParser::probed(FinishMode::Classify, &probe),
        capped,
    );
    assert_eq!(events.len(), 5, "{events:?}");
    assert_eq!(events[0], delta("before"));
    assert!(
        matches!(&events[1], AgentEvent::Warning { message } if message.contains("1024")),
        "{events:?}"
    );
    assert_eq!(events[2], delta("after"));
    assert_eq!(events[3], final_text("beforeafter"));
    assert_eq!(events[4], AgentEvent::Completed);
    assert!(probe.longest_line.load(Ordering::SeqCst) <= 1024);
}

#[test]
fn line_exactly_at_the_cap_is_kept_and_one_byte_over_is_dropped() {
    let mut capped = limits();
    capped.max_line_bytes = 10;
    let scratch = Scratch::new("cap-edge");
    let events = run(
        scratch.spec("echo delta:abcd\necho delta:abcde\n", &[]),
        ScriptParser::classifying(),
        capped,
    );
    assert_eq!(events.len(), 4, "{events:?}");
    assert_eq!(events[0], delta("abcd"));
    assert!(matches!(events[1], AgentEvent::Warning { .. }));
    assert_eq!(events[2], final_text("abcd"));
    assert_eq!(events[3], AgentEvent::Completed);
}

#[test]
fn carriage_returns_are_stripped_and_blank_lines_are_skipped() {
    let events = run_script("printf 'delta:a\\r\\n\\n   \\ndelta:b\\r\\n'\n");
    assert_eq!(
        events,
        vec![
            delta("a"),
            delta("b"),
            final_text("ab"),
            AgentEvent::Completed
        ]
    );
}

#[test]
fn line_without_trailing_newline_is_still_parsed() {
    let events = run_script("printf 'delta:tail'\n");
    assert_eq!(
        events,
        vec![delta("tail"), final_text("tail"), AgentEvent::Completed]
    );
}

#[test]
fn non_utf8_output_is_decoded_lossily() {
    let events = run_script("printf 'delta:caf\\351\\n'\n");
    assert_eq!(
        events,
        vec![
            delta("caf\u{fffd}"),
            final_text("caf\u{fffd}"),
            AgentEvent::Completed
        ]
    );
}

#[test]
fn synthesized_final_text_is_truncated_on_a_char_boundary() {
    let scratch = Scratch::new("truncate-synth");
    let mut capped = limits();
    capped.max_final_text_bytes = 9;
    let events = run(
        scratch.spec("echo delta:ééééééé\n", &[]),
        ScriptParser::classifying(),
        capped,
    );
    assert_eq!(events.len(), 3, "{events:?}");
    assert_eq!(events[1], final_text("éééé\n\n[truncated]"));
    assert_eq!(events[2], AgentEvent::Completed);
}

#[test]
fn parser_final_text_is_truncated_on_a_char_boundary() {
    let scratch = Scratch::new("truncate-final");
    let mut capped = limits();
    capped.max_final_text_bytes = 10;
    let events = run(
        scratch.spec("echo final:ééééééé\n", &[]),
        ScriptParser::classifying(),
        capped,
    );
    assert_eq!(
        events,
        vec![final_text("ééééé\n\n[truncated]"), AgentEvent::Completed]
    );
}

#[test]
fn text_within_the_final_limit_is_not_marked_truncated() {
    let scratch = Scratch::new("truncate-none");
    let mut capped = limits();
    capped.max_final_text_bytes = 5;
    let events = run(
        scratch.spec("echo delta:12345\n", &[]),
        ScriptParser::classifying(),
        capped,
    );
    assert_eq!(
        events,
        vec![delta("12345"), final_text("12345"), AgentEvent::Completed]
    );
}

#[test]
fn total_timeout_fails_with_timeout() {
    let scratch = Scratch::new("total-timeout");
    let mut short = limits();
    short.total = Duration::from_millis(400);
    let started = Instant::now();
    let events = run(
        scratch.spec("echo delta:start\nexec sleep 60\n", &[]),
        ScriptParser::classifying(),
        short,
    );
    assert!(started.elapsed() < Duration::from_secs(30));
    assert_eq!(events.first(), Some(&delta("start")));
    assert_eq!(failure_kind(&events), FailureKind::Timeout);
    assert_one_terminal_event_at_end(&events);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::TextFinal { .. })),
        "a failed turn must not emit final text: {events:?}"
    );
}

#[test]
fn idle_timeout_fails_with_timeout_long_before_the_total_limit() {
    let scratch = Scratch::new("idle-timeout");
    let mut short = limits();
    short.idle = Duration::from_millis(400);
    let started = Instant::now();
    let events = run(
        scratch.spec("echo delta:start\nexec sleep 60\n", &[]),
        ScriptParser::classifying(),
        short,
    );
    assert!(started.elapsed() < Duration::from_secs(30));
    assert_eq!(failure_kind(&events), FailureKind::Timeout);
    assert_one_terminal_event_at_end(&events);
}

#[test]
fn steady_output_does_not_trip_the_idle_timeout() {
    let scratch = Scratch::new("steady");
    let mut short = limits();
    short.idle = Duration::from_millis(1500);
    let script = "i=0\nwhile [ $i -lt 8 ]; do echo delta:x; sleep 0.3; i=$((i+1)); done\n";
    let events = run(
        scratch.spec(script, &[]),
        ScriptParser::classifying(),
        short,
    );
    assert_eq!(events.last(), Some(&AgentEvent::Completed), "{events:?}");
    assert_eq!(events.len(), 10, "{events:?}");
}

#[test]
fn cancel_stops_the_process_and_reports_cancelled() {
    let scratch = Scratch::new("cancel");
    let mut session = Session::start(
        scratch.spec("echo delta:ready\nexec sleep 60\n", &[]),
        ScriptParser::classifying(),
        limits(),
    );
    session.wait_for(|event| matches!(event, AgentEvent::TextDelta { .. }));
    let cancelled_at = Instant::now();
    session.cancel();
    let events = session.finish();
    assert!(cancelled_at.elapsed() < Duration::from_secs(10));
    assert_eq!(
        events,
        vec![
            delta("ready"),
            AgentEvent::Failed {
                kind: FailureKind::Cancelled,
                message: "turn cancelled".to_string(),
            }
        ]
    );
}

#[test]
fn cancel_escalates_to_sigterm_when_sigint_is_ignored() {
    let scratch = Scratch::new("cancel-term");
    let script = "trap '' INT\necho delta:ready\nwhile :; do sleep 1; done\n";
    let mut session = Session::start(
        scratch.spec(script, &[]),
        ScriptParser::classifying(),
        limits(),
    );
    session.wait_for(|event| matches!(event, AgentEvent::TextDelta { .. }));
    let cancelled_at = Instant::now();
    session.cancel();
    let events = session.finish();
    let waited = cancelled_at.elapsed();
    assert_eq!(failure_kind(&events), FailureKind::Cancelled);
    assert!(
        waited >= Duration::from_millis(1900),
        "SIGTERM must wait for the 2 s grace period, took {waited:?}"
    );
    assert!(waited < Duration::from_secs(20), "took {waited:?}");
}

#[test]
fn cancel_escalates_to_sigkill_when_sigint_and_sigterm_are_ignored() {
    let scratch = Scratch::new("cancel-kill");
    let script = "trap '' INT TERM\necho delta:ready\nwhile :; do sleep 1; done\n";
    let mut session = Session::start(
        scratch.spec(script, &[]),
        ScriptParser::classifying(),
        limits(),
    );
    session.wait_for(|event| matches!(event, AgentEvent::TextDelta { .. }));
    let cancelled_at = Instant::now();
    session.cancel();
    let events = session.finish();
    let waited = cancelled_at.elapsed();
    assert_eq!(failure_kind(&events), FailureKind::Cancelled);
    assert!(
        waited >= Duration::from_millis(4900),
        "SIGKILL must wait for the 5 s grace period, took {waited:?}"
    );
    assert!(waited < Duration::from_secs(25), "took {waited:?}");
}

#[test]
fn cancel_kills_grandchildren_not_just_the_direct_child() {
    assert_grandchild_dies(|session| session.cancel(), limits());
}

#[test]
fn timeout_kills_grandchildren_not_just_the_direct_child() {
    let mut short = limits();
    short.idle = Duration::from_millis(500);
    assert_grandchild_dies(|_| {}, short);
}

#[test]
fn grandchild_left_behind_by_a_normal_exit_is_killed_and_the_turn_still_ends() {
    let scratch = Scratch::new("straggler");
    let pid_file = scratch.dir.join("straggler.pid");
    let script = "sleep 100 &\necho $! > \"$1\"\necho delta:done\nexit 0\n";
    let started = Instant::now();
    let events = run(
        scratch.spec(script, &[&pid_file]),
        ScriptParser::classifying(),
        limits(),
    );
    assert!(started.elapsed() < Duration::from_secs(30));
    assert_eq!(
        events,
        vec![delta("done"), final_text("done"), AgentEvent::Completed]
    );
    let straggler = read_pid(&pid_file);
    assert!(
        wait_until_dead(straggler),
        "straggler {straggler} outlived the turn"
    );
}

#[test]
fn child_leads_its_own_process_group() {
    let scratch = Scratch::new("group");
    let mut session = Session::start(
        scratch.spec("echo delta:ready\nexec sleep 60\n", &[]),
        ScriptParser::classifying(),
        limits(),
    );
    session.wait_for(|event| matches!(event, AgentEvent::TextDelta { .. }));
    let pid = libc::pid_t::try_from(session.handle.pid).expect("pid fits");
    assert_eq!(process_group_of(pid), pid);
    assert_ne!(process_group_of(pid), process_group_of(0));
    session.cancel();
    session.finish();
}

#[test]
fn non_zero_exit_with_401_in_stderr_is_not_logged_in() {
    let events = run_script("echo 'HTTP 401 from upstream' >&2\nexit 1\n");
    assert_eq!(failure_kind(&events), FailureKind::NotLoggedIn);
    assert!(failure_message(&events).contains("401"));
    assert_one_terminal_event_at_end(&events);
}

#[test]
fn login_hint_in_stderr_is_not_logged_in() {
    let events = run_script("echo 'Please run /login' >&2\nexit 1\n");
    assert_eq!(failure_kind(&events), FailureKind::NotLoggedIn);
}

#[test]
fn rate_limit_in_stderr_is_rate_limited() {
    let events = run_script("echo 'error: 429 Too Many Requests' >&2\nexit 1\n");
    assert_eq!(failure_kind(&events), FailureKind::RateLimited);
}

#[test]
fn other_non_zero_exit_is_unknown_and_carries_the_stderr_tail() {
    let events = run_script("echo 'kaboom happened' >&2\nexit 3\n");
    assert_eq!(failure_kind(&events), FailureKind::Unknown);
    assert!(failure_message(&events).contains("kaboom happened"));
}

#[test]
fn non_zero_exit_without_stderr_names_the_exit_code() {
    let events = run_script("exit 7\n");
    assert_eq!(failure_kind(&events), FailureKind::Unknown);
    assert!(failure_message(&events).contains('7'));
}

#[test]
fn bare_auth_in_stderr_is_not_mistaken_for_a_login_problem() {
    let events = run_script("echo 'oauth refresh failed for author' >&2\nexit 1\n");
    assert_eq!(failure_kind(&events), FailureKind::Unknown);
    assert!(failure_message(&events).contains("oauth refresh failed"));
}

#[test]
fn death_by_signal_is_process_crashed() {
    let events = run_script("echo delta:partial\nkill -9 $$\n");
    assert_eq!(failure_kind(&events), FailureKind::ProcessCrashed);
    assert_one_terminal_event_at_end(&events);
}

#[test]
fn death_by_signal_with_login_text_in_stderr_is_still_process_crashed() {
    let events = run_script("echo 'HTTP 401, please login' >&2\nkill -9 $$\n");
    assert_eq!(failure_kind(&events), FailureKind::ProcessCrashed);
    assert_one_terminal_event_at_end(&events);
}

#[test]
fn timeout_with_login_text_in_stderr_still_reports_timeout() {
    let scratch = Scratch::new("timeout-login");
    let mut short = limits();
    short.idle = Duration::from_millis(400);
    let probe = Probe::default();
    let events = run(
        scratch.spec(
            "echo 'Not logged in, please login' >&2\necho delta:start\nexec sleep 60\n",
            &[],
        ),
        ScriptParser::probed(FinishMode::Classify, &probe),
        short,
    );
    let tail = probe
        .stderr_tail
        .lock()
        .expect("probe lock")
        .clone()
        .expect("finish was called");
    assert!(tail.contains("please login"), "stderr was {tail:?}");
    assert_eq!(failure_kind(&events), FailureKind::Timeout);
    assert_one_terminal_event_at_end(&events);
}

#[test]
fn cancel_with_login_text_in_stderr_still_reports_cancelled() {
    let scratch = Scratch::new("cancel-login");
    let mut session = Session::start(
        scratch.spec(
            "echo 'HTTP 401 unauthorized' >&2\necho delta:ready\nexec sleep 60\n",
            &[],
        ),
        ScriptParser::classifying(),
        limits(),
    );
    session.wait_for(|event| matches!(event, AgentEvent::TextDelta { .. }));
    session.cancel();
    let events = session.finish();
    assert_eq!(failure_kind(&events), FailureKind::Cancelled);
    assert_one_terminal_event_at_end(&events);
}

#[test]
fn stderr_noise_on_a_successful_exit_still_completes() {
    let events = run_script("echo 'warning: something odd' >&2\necho delta:fine\n");
    assert_eq!(
        events,
        vec![delta("fine"), final_text("fine"), AgentEvent::Completed]
    );
}

#[test]
fn parser_receives_only_the_last_8_kib_of_stderr_and_exit_details() {
    let scratch = Scratch::new("stderr-tail");
    let probe = Probe::default();
    let script = "head -c 100000 /dev/zero | tr '\\0' 'a' >&2\necho ENDMARK >&2\nexit 2\n";
    let events = run(
        scratch.spec(script, &[]),
        ScriptParser::probed(FinishMode::Classify, &probe),
        limits(),
    );
    let tail = probe
        .stderr_tail
        .lock()
        .expect("probe lock")
        .clone()
        .expect("finish was called");
    assert!(tail.len() <= 8 * 1024, "tail was {} bytes", tail.len());
    assert!(tail.len() >= 8 * 1024 - 8, "tail was {} bytes", tail.len());
    assert!(tail.trim_end().ends_with("ENDMARK"));
    let exit = probe
        .exit
        .lock()
        .expect("probe lock")
        .expect("finish was called");
    assert_eq!(exit.code, Some(2));
    assert_eq!(exit.signal, None);
    assert!(!exit.timed_out && !exit.cancelled);
    assert_eq!(failure_kind(&events), FailureKind::Unknown);
    let message = failure_message(&events);
    assert_eq!(message.chars().count(), 500);
    assert!(message.ends_with("ENDMARK"));
}

#[test]
fn exit_info_reports_cancellation_and_signal() {
    let scratch = Scratch::new("exit-info");
    let probe = Probe::default();
    let mut session = Session::start(
        scratch.spec("echo delta:ready\nexec sleep 60\n", &[]),
        ScriptParser::probed(FinishMode::Classify, &probe),
        limits(),
    );
    session.wait_for(|event| matches!(event, AgentEvent::TextDelta { .. }));
    session.cancel();
    session.finish();
    let exit = probe
        .exit
        .lock()
        .expect("probe lock")
        .expect("finish was called");
    assert!(exit.cancelled);
    assert!(!exit.timed_out);
    assert_eq!(exit.code, None);
    assert!(
        matches!(exit.signal, Some(libc::SIGINT | libc::SIGTERM)),
        "unexpected signal {:?}",
        exit.signal
    );
}

#[test]
fn cancelled_exit_with_code_zero_is_not_reported_as_completed() {
    let scratch = Scratch::new("graceful-cancel");
    let script = "trap 'exit 0' INT\necho delta:ready\nwhile :; do sleep 0.1; done\n";
    let mut session = Session::start(
        scratch.spec(script, &[]),
        ScriptParser::classifying(),
        limits(),
    );
    session.wait_for(|event| matches!(event, AgentEvent::TextDelta { .. }));
    session.cancel();
    let events = session.finish();
    assert_eq!(failure_kind(&events), FailureKind::Cancelled);
    assert_one_terminal_event_at_end(&events);
}

#[test]
fn missing_binary_is_reported_synchronously() {
    let scratch = Scratch::new("missing");
    let mut spec = scratch.spec("exit 0\n", &[]);
    spec.program = scratch.dir.join("no-such-cli");
    let (sender, receiver) = mpsc::channel::<AgentEvent>();
    let result = run_turn(
        spec,
        ScriptParser::classifying(),
        limits(),
        Box::new(move |event| {
            let _ = sender.send(event);
        }),
    );
    let err = result.expect_err("a missing binary must fail");
    assert_eq!(err.kind, ErrorKind::ProviderNotInstalled);
    assert!(err.message.contains("no-such-cli"));
    assert!(matches!(
        receiver.try_recv(),
        Err(TryRecvError::Disconnected)
    ));
}

#[test]
fn non_executable_binary_is_a_provider_error() {
    let scratch = Scratch::new("not-exec");
    let mut spec = scratch.spec("exit 0\n", &[]);
    spec.program = scratch.write("plain.txt", "data", 0o644);
    let result = run_turn(
        spec,
        ScriptParser::classifying(),
        limits(),
        Box::new(|_| {}),
    );
    let err = result.expect_err("a non-executable file must fail");
    assert_eq!(err.kind, ErrorKind::Provider);
}

#[test]
fn missing_working_directory_is_an_io_error_not_a_missing_provider() {
    let scratch = Scratch::new("no-cwd");
    let mut spec = scratch.spec("exit 0\n", &[]);
    spec.cwd = scratch.dir.join("gone");
    let result = run_turn(
        spec,
        ScriptParser::classifying(),
        limits(),
        Box::new(|_| {}),
    );
    let err = result.expect_err("a missing cwd must fail");
    assert_eq!(err.kind, ErrorKind::Io);
    assert!(err.message.contains("gone"));
}

#[test]
fn stdin_is_written_in_full_and_closed() {
    let scratch = Scratch::new("stdin");
    let mut spec = scratch.spec("cat\n", &[]);
    spec.stdin = Some(b"delta:from stdin\n".to_vec());
    let events = run(spec, ScriptParser::classifying(), limits());
    assert_eq!(
        events,
        vec![
            delta("from stdin"),
            final_text("from stdin"),
            AgentEvent::Completed
        ]
    );
}

#[test]
fn large_stdin_is_not_a_problem_when_the_child_never_reads_it() {
    let scratch = Scratch::new("stdin-epipe");
    let mut spec = scratch.spec("echo delta:early\nexit 0\n", &[]);
    spec.stdin = Some(vec![b'z'; 4 * 1024 * 1024]);
    let events = run(spec, ScriptParser::classifying(), limits());
    assert_eq!(
        events,
        vec![delta("early"), final_text("early"), AgentEvent::Completed]
    );
}

#[test]
fn stdin_is_null_when_the_spec_has_none() {
    let events = run_script("cat\necho delta:after-cat\n");
    assert_eq!(
        events,
        vec![
            delta("after-cat"),
            final_text("after-cat"),
            AgentEvent::Completed
        ]
    );
}

#[test]
fn child_environment_is_exactly_the_spec_environment() {
    let scratch = Scratch::new("env");
    let mut spec = scratch.spec(
        "echo \"delta:home=${HOME:-unset} user=${USER:-unset} foo=${TERN_TEST_FOO:-unset}\"\n",
        &[],
    );
    spec.env
        .push((OsString::from("TERN_TEST_FOO"), OsString::from("bar")));
    let events = run(spec, ScriptParser::classifying(), limits());
    assert_eq!(
        events.first(),
        Some(&delta("home=unset user=unset foo=bar")),
        "{events:?}"
    );
}

#[test]
fn working_directory_is_applied() {
    let scratch = Scratch::new("cwd");
    let events = run(
        scratch.spec("echo \"delta:$(pwd -P)\"\n", &[]),
        ScriptParser::classifying(),
        limits(),
    );
    let expected = scratch.dir.canonicalize().expect("canonicalize");
    assert_eq!(
        events.first(),
        Some(&delta(&expected.display().to_string()))
    );
}

#[test]
fn chatty_output_is_delivered_completely_and_in_order_under_back_pressure() {
    let scratch = Scratch::new("chatty");
    let total_lines = 100_000usize;
    let script = format!("yes delta:x | head -n {total_lines}\n");
    let (sender, receiver) = mpsc::channel::<AgentEvent>();
    let delivered = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&delivered);
    let handle = run_turn(
        scratch.spec(&script, &[]),
        ScriptParser::classifying(),
        limits(),
        Box::new(move |event| {
            let count = counter.fetch_add(1, Ordering::SeqCst);
            if count.is_multiple_of(1000) {
                std::thread::sleep(Duration::from_millis(2));
            }
            let _ = sender.send(event);
        }),
    )
    .expect("run_turn");
    assert!(handle.pid > 0);
    let mut deltas = 0usize;
    let mut tail = Vec::new();
    loop {
        match receiver.recv_timeout(RUN_CEILING) {
            Ok(AgentEvent::TextDelta { text }) => {
                assert_eq!(text, "x");
                deltas += 1;
            }
            Ok(other) => tail.push(other),
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => panic!("chatty run stalled after {deltas} deltas"),
        }
    }
    assert_eq!(deltas, total_lines);
    assert_eq!(tail.len(), 2, "{tail:?}");
    assert_eq!(tail[0], final_text(&"x".repeat(total_lines)));
    assert_eq!(tail[1], AgentEvent::Completed);
}

fn exit_info(code: Option<i32>, signal: Option<i32>, timed_out: bool, cancelled: bool) -> ExitInfo {
    ExitInfo {
        code,
        signal,
        timed_out,
        cancelled,
    }
}

fn classified_kind(exit: &ExitInfo, stderr: &str, last_error: Option<&str>) -> FailureKind {
    match classify_failure(exit, stderr, last_error) {
        AgentEvent::Failed { kind, .. } => kind,
        other => panic!("expected Failed for {stderr:?} / {last_error:?}, got {other:?}"),
    }
}

#[test]
fn classify_failure_completes_only_on_a_clean_exit() {
    assert_eq!(
        classify_failure(&exit_info(Some(0), None, false, false), "noise", None),
        AgentEvent::Completed
    );
    assert_eq!(
        classified_kind(&exit_info(Some(0), None, false, true), "", None),
        FailureKind::Cancelled
    );
    assert_eq!(
        classified_kind(&exit_info(Some(0), None, true, false), "", None),
        FailureKind::Timeout
    );
}

#[test]
fn classify_failure_cancelled_wins_over_every_other_signal() {
    let noisy = "HTTP 401 not logged in please login, 429 rate limit";
    for exit in [
        exit_info(None, Some(2), false, true),
        exit_info(Some(1), None, false, true),
        exit_info(None, Some(9), true, true),
    ] {
        assert_eq!(
            classified_kind(&exit, noisy, Some("401 unauthorized")),
            FailureKind::Cancelled
        );
    }
}

#[test]
fn classify_failure_timeout_wins_over_signal_and_text() {
    let timed_out = exit_info(None, Some(15), true, false);
    for stderr in [
        "",
        "waiting for login",
        "Please log in to continue",
        "HTTP 401",
        "429 rate limit",
    ] {
        assert_eq!(
            classified_kind(&timed_out, stderr, None),
            FailureKind::Timeout,
            "{stderr}"
        );
    }
    assert_eq!(
        classified_kind(&timed_out, "", Some("Not logged in")),
        FailureKind::Timeout
    );
}

#[test]
fn classify_failure_signal_wins_over_text() {
    let crashed = exit_info(None, Some(11), false, false);
    for stderr in ["", "HTTP 401", "Not logged in", "429 rate limit"] {
        assert_eq!(
            classified_kind(&crashed, stderr, None),
            FailureKind::ProcessCrashed,
            "{stderr}"
        );
    }
    assert_eq!(
        classified_kind(&crashed, "", Some("please login")),
        FailureKind::ProcessCrashed
    );
}

#[test]
fn classify_failure_matches_login_phrases_as_whole_words_ignoring_case() {
    let plain = exit_info(Some(1), None, false, false);
    for stderr in [
        "Error: Not logged in",
        "NOT LOGGED IN",
        "please run /login first",
        "Login required",
        "login: failed",
        "please log in",
        "Please LOG IN again",
        "please log   in",
        "please log\nin",
        "Unauthorized",
        "error:unauthorized",
        "HTTP 401",
        "status (401)",
        "401: bad key",
        "authentication failed",
        "Authentication required",
    ] {
        assert_eq!(
            classified_kind(&plain, stderr, None),
            FailureKind::NotLoggedIn,
            "{stderr}"
        );
    }
}

#[test]
fn classify_failure_ignores_login_phrases_inside_other_words() {
    let plain = exit_info(Some(1), None, false, false);
    for stderr in [
        "auth failed",
        "AUTH error",
        "oauth token expired",
        "author unknown",
        "authenticated as bob",
        "unauthenticated",
        "HTTP 4010",
        "port 14012",
        "id 401a",
        "loginctl exited",
        "blogin failed",
        "login_failed",
        "backlog in progress",
        "Logged in as bob",
        "catalog in use",
    ] {
        assert_eq!(
            classified_kind(&plain, stderr, None),
            FailureKind::Unknown,
            "{stderr}"
        );
    }
}

#[test]
fn classify_failure_detects_rate_limits_after_login_text_is_ruled_out() {
    let plain = exit_info(Some(1), None, false, false);
    for stderr in [
        "rate limit exceeded",
        "HTTP 429",
        "quota exhausted",
        "Rate Limit",
    ] {
        assert_eq!(
            classified_kind(&plain, stderr, None),
            FailureKind::RateLimited,
            "{stderr}"
        );
    }
    assert_eq!(
        classified_kind(&plain, "HTTP 429 after HTTP 401", None),
        FailureKind::NotLoggedIn
    );
}

#[test]
fn classify_failure_reads_the_last_error_as_well_as_stderr() {
    let plain = exit_info(Some(1), None, false, false);
    match classify_failure(&plain, "", Some("API error 401: bad key")) {
        AgentEvent::Failed { kind, message } => {
            assert_eq!(kind, FailureKind::NotLoggedIn);
            assert!(message.contains("bad key"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        classified_kind(&plain, "", Some("429 slow down")),
        FailureKind::RateLimited
    );
    assert_eq!(
        classified_kind(&plain, "plain noise", Some("Please log in")),
        FailureKind::NotLoggedIn
    );
    assert_eq!(
        classified_kind(&plain, "something else", None),
        FailureKind::Unknown
    );
}

#[test]
fn classify_failure_unknown_message_is_the_last_500_chars_of_stderr() {
    let plain = ExitInfo {
        code: Some(9),
        signal: None,
        timed_out: false,
        cancelled: false,
    };
    let stderr = format!("{}{}", "é".repeat(700), "END");
    match classify_failure(&plain, &stderr, None) {
        AgentEvent::Failed { kind, message } => {
            assert_eq!(kind, FailureKind::Unknown);
            assert_eq!(message.chars().count(), 500);
            assert!(message.ends_with("END"));
        }
        other => panic!("{other:?}"),
    }
    match classify_failure(&plain, "  \n", None) {
        AgentEvent::Failed { message, .. } => assert!(message.contains('9')),
        other => panic!("{other:?}"),
    }
}

struct StubProvider {
    id: ProviderId,
}

impl Provider for StubProvider {
    fn id(&self) -> ProviderId {
        self.id
    }

    fn display_name(&self) -> &'static str {
        "Stub"
    }

    fn binary_name(&self) -> &'static str {
        "stub"
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
        Detection::default()
    }

    fn build_command(
        &self,
        _turn: &TurnRequest,
        _ctx: &TurnContext,
    ) -> Result<CommandSpec, CoreError> {
        Err(CoreError::new(ErrorKind::Unsupported, "stub"))
    }

    fn new_parser(&self) -> Box<dyn EventParser> {
        ScriptParser::classifying()
    }
}

fn stub(id: ProviderId) -> Arc<dyn Provider> {
    Arc::new(StubProvider { id })
}

#[test]
fn registry_returns_providers_by_id_in_registration_order() {
    let registry = ProviderRegistry::new(vec![stub("claude"), stub("codex"), stub("agy")]);
    let ids: Vec<&str> = registry
        .all()
        .iter()
        .map(|provider| provider.id())
        .collect();
    assert_eq!(ids, vec!["claude", "codex", "agy"]);
    assert_eq!(
        registry.get("codex").map(|provider| provider.id()),
        Some("codex")
    );
    assert!(registry.get("gemini").is_none());
    assert!(registry.get("").is_none());
}

#[test]
fn empty_registry_has_no_providers() {
    let registry = ProviderRegistry::new(Vec::new());
    assert!(registry.all().is_empty());
    assert!(registry.get("claude").is_none());
}

#[test]
fn registry_get_returns_the_first_provider_registered_under_an_id() {
    let first = stub("dup");
    let second = stub("dup");
    let registry = ProviderRegistry::new(vec![Arc::clone(&first), second]);
    let found = registry.get("dup").expect("dup is registered");
    assert!(Arc::ptr_eq(&found, &first));
}
