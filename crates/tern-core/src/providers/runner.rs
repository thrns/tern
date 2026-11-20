use super::{AgentEvent, CommandSpec, EventParser, ExitInfo, FailureKind};
use crate::error::{CoreError, ErrorKind};
use crossbeam_channel::{Receiver, RecvTimeoutError, SendError, Sender};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const TERM_AFTER: Duration = Duration::from_secs(2);
const KILL_AFTER: Duration = Duration::from_secs(5);
const STDOUT_GRACE: Duration = Duration::from_secs(2);
const STDERR_GRACE: Duration = Duration::from_millis(500);
const FINISH_POLL: Duration = Duration::from_millis(5);
const EVENT_QUEUE_CAPACITY: usize = 1024;
const STDERR_TAIL_BYTES: usize = 8 * 1024;
const STDERR_CHUNK_BYTES: usize = 4096;
const LINE_BUFFER_KEEP_BYTES: usize = 64 * 1024;
const FAILURE_DETAIL_CHARS: usize = 500;
const TRUNCATION_MARKER: &str = "\n\n[truncated]";
const NOT_LOGGED_IN_PHRASES: [&str; 6] = [
    "not logged in",
    "login",
    "log in",
    "unauthorized",
    "401",
    "authentication",
];
const RATE_LIMIT_MARKERS: [&str; 3] = ["rate limit", "429", "quota"];

type SharedParser = Arc<Mutex<Box<dyn EventParser>>>;
type SharedTail = Arc<Mutex<Vec<u8>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunLimits {
    pub total: Duration,
    pub idle: Duration,
    pub max_line_bytes: usize,
    pub max_final_text_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct RunHandle {
    pub cancel: Arc<AtomicBool>,
    pub pid: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StopCause {
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, Copy)]
struct Stop {
    cause: StopCause,
    begun: Instant,
    termed: bool,
    killed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineRead {
    Line,
    Oversize,
    Eof,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn signal_group(group: libc::pid_t, signal: libc::c_int) {
    if group > 1 {
        unsafe {
            libc::kill(-group, signal);
        }
    }
}

fn group_of(child: &Child) -> Result<libc::pid_t, CoreError> {
    libc::pid_t::try_from(child.id()).map_err(|_| {
        CoreError::new(
            ErrorKind::Internal,
            format!("process id {} does not fit a pid", child.id()),
        )
    })
}

fn abort_child(child: &mut Child, err: CoreError) -> CoreError {
    if let Ok(group) = group_of(child) {
        signal_group(group, libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
    err
}

fn spawn_thread<F, T>(name: &str, work: F) -> Result<JoinHandle<T>, CoreError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    thread::Builder::new()
        .name(name.to_string())
        .spawn(work)
        .map_err(|err| CoreError::new(ErrorKind::Io, format!("cannot start thread {name}: {err}")))
}

fn spawn_child(spec: &CommandSpec) -> Result<Child, CoreError> {
    if !spec.cwd.is_dir() {
        return Err(CoreError::new(
            ErrorKind::Io,
            format!("working directory {} does not exist", spec.cwd.display()),
        ));
    }
    let stdin = if spec.stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    };
    Command::new(&spec.program)
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .env_clear()
        .envs(spec.env.iter().map(|(key, value)| (key, value)))
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|err| {
            let kind = if err.kind() == std::io::ErrorKind::NotFound {
                ErrorKind::ProviderNotInstalled
            } else {
                ErrorKind::Provider
            };
            CoreError::new(
                kind,
                format!("cannot start {}: {err}", spec.program.display()),
            )
        })
}

fn note_activity(activity: &AtomicU64, started: Instant) {
    let millis = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    activity.store(millis, Ordering::Relaxed);
}

fn read_capped_line<R: BufRead>(
    reader: &mut R,
    line: &mut Vec<u8>,
    cap: usize,
    mut on_bytes: impl FnMut(),
) -> std::io::Result<LineRead> {
    line.clear();
    let mut oversize = false;
    loop {
        let (consumed, complete) = {
            let chunk = match reader.fill_buf() {
                Ok(chunk) => chunk,
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(err),
            };
            if chunk.is_empty() {
                return Ok(if oversize {
                    LineRead::Oversize
                } else if line.is_empty() {
                    LineRead::Eof
                } else {
                    LineRead::Line
                });
            }
            on_bytes();
            let (take, complete) = match chunk.iter().position(|byte| *byte == b'\n') {
                Some(index) => (index + 1, true),
                None => (chunk.len(), false),
            };
            if !oversize {
                let body = &chunk[..if complete { take - 1 } else { take }];
                if line.len() + body.len() > cap {
                    oversize = true;
                    line.clear();
                } else {
                    line.extend_from_slice(body);
                }
            }
            (take, complete)
        };
        reader.consume(consumed);
        if complete {
            return Ok(if oversize {
                LineRead::Oversize
            } else {
                LineRead::Line
            });
        }
    }
}

fn parse_raw_line(parser: &SharedParser, raw: &[u8]) -> Vec<AgentEvent> {
    let decoded = String::from_utf8_lossy(raw);
    let text = decoded.trim_end_matches('\r');
    if text.trim().is_empty() {
        return Vec::new();
    }
    lock(parser).parse_line(text)
}

fn read_stdout(
    stdout: ChildStdout,
    parser: SharedParser,
    events: Sender<AgentEvent>,
    activity: Arc<AtomicU64>,
    started: Instant,
    max_line_bytes: usize,
) {
    let mut reader = BufReader::new(stdout);
    let mut line = Vec::new();
    loop {
        let outcome = read_capped_line(&mut reader, &mut line, max_line_bytes, || {
            note_activity(&activity, started)
        });
        let batch = match outcome {
            Ok(LineRead::Line) => parse_raw_line(&parser, &line),
            Ok(LineRead::Oversize) => vec![AgentEvent::Warning {
                message: format!("dropped a stdout line longer than {max_line_bytes} bytes"),
            }],
            Ok(LineRead::Eof) => break,
            Err(err) => {
                log::warn!("stdout read failed: {err}");
                break;
            }
        };
        for event in batch {
            if events.send(event).is_err() {
                return;
            }
        }
        line.shrink_to(LINE_BUFFER_KEEP_BYTES);
    }
}

fn drain_stderr(mut stderr: ChildStderr, tail: SharedTail) {
    let mut chunk = [0u8; STDERR_CHUNK_BYTES];
    loop {
        match stderr.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                let mut kept = lock(&tail);
                kept.extend_from_slice(&chunk[..count]);
                if kept.len() > STDERR_TAIL_BYTES {
                    let excess = kept.len() - STDERR_TAIL_BYTES;
                    kept.drain(..excess);
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
}

#[cfg(target_os = "macos")]
fn guard_broken_pipe(pipe: &ChildStdin) {
    use std::os::fd::AsRawFd;
    unsafe {
        libc::fcntl(pipe.as_raw_fd(), libc::F_SETNOSIGPIPE, 1);
    }
}

#[cfg(not(target_os = "macos"))]
fn guard_broken_pipe(_pipe: &ChildStdin) {
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGPIPE);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
    }
}

fn feed_stdin(mut pipe: ChildStdin, bytes: Vec<u8>) {
    guard_broken_pipe(&pipe);
    let _ = pipe.write_all(&bytes);
}

fn cut_at_boundary(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn last_chars(text: &str, count: usize) -> &str {
    match text.char_indices().rev().nth(count.saturating_sub(1)) {
        Some((index, _)) => &text[index..],
        None => text,
    }
}

struct Dispatcher {
    on_event: Box<dyn Fn(AgentEvent) + Send>,
    max_text_bytes: usize,
    text: String,
    text_truncated: bool,
    final_seen: bool,
    done: bool,
}

impl Dispatcher {
    fn new(on_event: Box<dyn Fn(AgentEvent) + Send>, max_text_bytes: usize) -> Self {
        Self {
            on_event,
            max_text_bytes,
            text: String::new(),
            text_truncated: false,
            final_seen: false,
            done: false,
        }
    }

    fn emit(&self, event: AgentEvent) {
        (self.on_event)(event);
    }

    fn remember(&mut self, delta: &str) {
        if self.text_truncated {
            return;
        }
        let room = self.max_text_bytes.saturating_sub(self.text.len());
        if delta.len() <= room {
            self.text.push_str(delta);
        } else {
            self.text.push_str(cut_at_boundary(delta, room));
            self.text_truncated = true;
        }
    }

    fn limit(&self, text: String) -> String {
        if text.len() <= self.max_text_bytes {
            return text;
        }
        let mut limited = cut_at_boundary(&text, self.max_text_bytes).to_string();
        limited.push_str(TRUNCATION_MARKER);
        limited
    }

    fn flush_text(&mut self) {
        if self.final_seen || (self.text.is_empty() && !self.text_truncated) {
            return;
        }
        self.final_seen = true;
        let mut text = std::mem::take(&mut self.text);
        if self.text_truncated {
            text.push_str(TRUNCATION_MARKER);
        }
        self.emit(AgentEvent::TextFinal { text });
    }

    fn deliver(&mut self, event: AgentEvent) {
        if self.done {
            return;
        }
        match event {
            AgentEvent::TextDelta { text } => {
                self.remember(&text);
                self.emit(AgentEvent::TextDelta { text });
            }
            AgentEvent::TextFinal { text } => {
                self.final_seen = true;
                let text = self.limit(text);
                self.emit(AgentEvent::TextFinal { text });
            }
            AgentEvent::Completed => {
                self.flush_text();
                self.done = true;
                self.emit(AgentEvent::Completed);
            }
            failed @ AgentEvent::Failed { .. } => {
                self.done = true;
                self.emit(failed);
            }
            other => self.emit(other),
        }
    }

    fn deliver_finish(&mut self, events: Vec<AgentEvent>, exit: &ExitInfo) {
        let interrupted = exit.cancelled || exit.timed_out;
        for event in events {
            if interrupted && matches!(event, AgentEvent::Completed) {
                continue;
            }
            self.deliver(event);
        }
    }

    fn ensure_terminal(&mut self, exit: &ExitInfo, stderr_tail: &str) {
        if !self.done {
            self.deliver(classify_failure(exit, stderr_tail, None));
        }
    }
}

struct Turn {
    child: Child,
    group: libc::pid_t,
    cancel: Arc<AtomicBool>,
    limits: RunLimits,
    started: Instant,
    activity: Arc<AtomicU64>,
    events: Receiver<AgentEvent>,
    parser: SharedParser,
    stderr_tail: SharedTail,
    stdout_thread: JoinHandle<()>,
    stderr_thread: JoinHandle<()>,
    dispatcher: Dispatcher,
}

impl Turn {
    fn stop_cause(&self, now: Instant) -> Option<StopCause> {
        if self.cancel.load(Ordering::Acquire) {
            return Some(StopCause::Cancelled);
        }
        let elapsed = now.duration_since(self.started);
        if elapsed >= self.limits.total {
            return Some(StopCause::TimedOut);
        }
        let last_activity = Duration::from_millis(self.activity.load(Ordering::Relaxed));
        if elapsed.saturating_sub(last_activity) >= self.limits.idle {
            return Some(StopCause::TimedOut);
        }
        None
    }

    fn enforce(&self, stop: &mut Option<Stop>, now: Instant) {
        match stop {
            None => {
                if let Some(cause) = self.stop_cause(now) {
                    signal_group(self.group, libc::SIGINT);
                    *stop = Some(Stop {
                        cause,
                        begun: now,
                        termed: false,
                        killed: false,
                    });
                }
            }
            Some(stop) => {
                let waited = now.duration_since(stop.begun);
                if waited >= KILL_AFTER {
                    if !stop.killed {
                        signal_group(self.group, libc::SIGKILL);
                        stop.killed = true;
                        stop.termed = true;
                    }
                } else if waited >= TERM_AFTER && !stop.termed {
                    signal_group(self.group, libc::SIGTERM);
                    stop.termed = true;
                }
            }
        }
    }

    fn wait_for_exit(&mut self, stop: &mut Option<Stop>) -> Option<ExitStatus> {
        let mut closed = false;
        let mut next_check = Instant::now();
        loop {
            if closed {
                thread::sleep(POLL_INTERVAL);
            } else {
                match self.events.recv_timeout(POLL_INTERVAL) {
                    Ok(event) => self.dispatcher.deliver(event),
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => closed = true,
                }
            }
            let now = Instant::now();
            if now < next_check {
                continue;
            }
            next_check = now + POLL_INTERVAL;
            match self.child.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) => self.enforce(stop, now),
                Err(err) => {
                    log::warn!("cannot poll provider process: {err}");
                    signal_group(self.group, libc::SIGKILL);
                    return None;
                }
            }
        }
    }

    fn drain_stdout(&mut self) {
        loop {
            match self.events.recv_timeout(STDOUT_GRACE) {
                Ok(event) => self.dispatcher.deliver(event),
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => return,
            }
        }
    }

    fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&lock(&self.stderr_tail)).into_owned()
    }

    fn run(mut self) {
        let mut stop: Option<Stop> = None;
        let status = self.wait_for_exit(&mut stop);
        signal_group(self.group, libc::SIGKILL);
        self.drain_stdout();
        let deadline = Instant::now() + STDERR_GRACE;
        while !self.stderr_thread.is_finished() && Instant::now() < deadline {
            thread::sleep(FINISH_POLL);
        }
        let exit = ExitInfo {
            code: status.and_then(|status| status.code()),
            signal: status.and_then(|status| status.signal()),
            timed_out: matches!(
                stop,
                Some(Stop {
                    cause: StopCause::TimedOut,
                    ..
                })
            ),
            cancelled: matches!(
                stop,
                Some(Stop {
                    cause: StopCause::Cancelled,
                    ..
                })
            ),
        };
        let stderr_tail = self.stderr_text();
        let finished = lock(&self.parser).finish(&exit, &stderr_tail);
        self.dispatcher.deliver_finish(finished, &exit);
        self.dispatcher.ensure_terminal(&exit, &stderr_tail);
        if self.stdout_thread.is_finished() && self.stdout_thread.join().is_err() {
            log::warn!("provider stdout reader panicked");
        }
        if self.stderr_thread.is_finished() && self.stderr_thread.join().is_err() {
            log::warn!("provider stderr reader panicked");
        }
    }
}

fn start_workers(
    child: &mut Child,
    stdin_bytes: Option<Vec<u8>>,
    parser: Box<dyn EventParser>,
    limits: RunLimits,
    on_event: Box<dyn Fn(AgentEvent) + Send>,
    cancel: Arc<AtomicBool>,
) -> Result<Sender<Child>, CoreError> {
    let group = group_of(child)?;
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        return Err(CoreError::new(
            ErrorKind::Internal,
            "provider output pipes are missing",
        ));
    };
    let started = Instant::now();
    let activity = Arc::new(AtomicU64::new(0));
    let parser: SharedParser = Arc::new(Mutex::new(parser));
    let stderr_tail: SharedTail = Arc::new(Mutex::new(Vec::new()));
    let (event_sender, event_receiver) = crossbeam_channel::bounded(EVENT_QUEUE_CAPACITY);

    let stdout_thread = {
        let parser = Arc::clone(&parser);
        let activity = Arc::clone(&activity);
        let max_line_bytes = limits.max_line_bytes;
        spawn_thread("tern-ask-stdout", move || {
            read_stdout(
                stdout,
                parser,
                event_sender,
                activity,
                started,
                max_line_bytes,
            )
        })?
    };
    let stderr_thread = {
        let tail = Arc::clone(&stderr_tail);
        spawn_thread("tern-ask-stderr", move || drain_stderr(stderr, tail))?
    };
    if let (Some(pipe), Some(bytes)) = (child.stdin.take(), stdin_bytes) {
        spawn_thread("tern-ask-stdin", move || feed_stdin(pipe, bytes))?;
    }

    let (handoff_sender, handoff_receiver) = crossbeam_channel::bounded::<Child>(1);
    let dispatcher = Dispatcher::new(on_event, limits.max_final_text_bytes);
    spawn_thread("tern-ask-waiter", move || {
        if let Ok(child) = handoff_receiver.recv() {
            Turn {
                child,
                group,
                cancel,
                limits,
                started,
                activity,
                events: event_receiver,
                parser,
                stderr_tail,
                stdout_thread,
                stderr_thread,
                dispatcher,
            }
            .run();
        }
    })?;
    Ok(handoff_sender)
}

pub fn run_turn(
    spec: CommandSpec,
    parser: Box<dyn EventParser>,
    limits: RunLimits,
    on_event: Box<dyn Fn(AgentEvent) + Send>,
) -> Result<RunHandle, CoreError> {
    let mut child = spawn_child(&spec)?;
    let pid = child.id();
    let cancel = Arc::new(AtomicBool::new(false));
    let handoff = match start_workers(
        &mut child,
        spec.stdin,
        parser,
        limits,
        on_event,
        Arc::clone(&cancel),
    ) {
        Ok(handoff) => handoff,
        Err(err) => return Err(abort_child(&mut child, err)),
    };
    match handoff.send(child) {
        Ok(()) => Ok(RunHandle { cancel, pid }),
        Err(SendError(mut child)) => Err(abort_child(
            &mut child,
            CoreError::new(ErrorKind::Internal, "provider waiter thread stopped early"),
        )),
    }
}

pub fn classify_failure(
    exit: &ExitInfo,
    stderr_tail: &str,
    last_error: Option<&str>,
) -> AgentEvent {
    if exit.cancelled {
        return failed(FailureKind::Cancelled, "turn cancelled".to_string());
    }
    if exit.timed_out {
        return failed(FailureKind::Timeout, "turn timed out".to_string());
    }
    if exit.code == Some(0) {
        return AgentEvent::Completed;
    }
    let stderr_detail = last_chars(stderr_tail.trim(), FAILURE_DETAIL_CHARS);
    let detail = if stderr_detail.is_empty() {
        last_chars(last_error.unwrap_or_default().trim(), FAILURE_DETAIL_CHARS)
    } else {
        stderr_detail
    };
    let describe = |fallback: String| {
        if detail.is_empty() {
            fallback
        } else {
            detail.to_string()
        }
    };
    if let Some(signal) = exit.signal {
        return failed(
            FailureKind::ProcessCrashed,
            describe(format!("provider process killed by signal {signal}")),
        );
    }
    let haystack = normalize_for_matching(stderr_tail, last_error);
    if NOT_LOGGED_IN_PHRASES
        .iter()
        .any(|phrase| contains_whole_phrase(&haystack, phrase))
    {
        return failed(
            FailureKind::NotLoggedIn,
            describe("provider is not logged in".to_string()),
        );
    }
    if RATE_LIMIT_MARKERS
        .iter()
        .any(|marker| haystack.contains(marker))
    {
        return failed(
            FailureKind::RateLimited,
            describe("provider rate limit reached".to_string()),
        );
    }
    let fallback = match exit.code {
        Some(code) => format!("provider exited with code {code}"),
        None => "provider exited abnormally".to_string(),
    };
    failed(FailureKind::Unknown, describe(fallback))
}

fn failed(kind: FailureKind, message: String) -> AgentEvent {
    AgentEvent::Failed { kind, message }
}

fn normalize_for_matching(stderr_tail: &str, last_error: Option<&str>) -> String {
    format!("{stderr_tail}\n{}", last_error.unwrap_or_default())
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn contains_whole_phrase(haystack: &str, phrase: &str) -> bool {
    haystack.match_indices(phrase).any(|(start, matched)| {
        let before = haystack[..start].chars().next_back();
        let after = haystack[start + matched.len()..].chars().next();
        !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
    })
}
