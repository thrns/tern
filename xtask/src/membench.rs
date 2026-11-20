use std::env;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::{XtaskError, bundle, workspace_root};

const SAMPLES_PER_PHASE: usize = 10;
const SAMPLE_INTERVAL: Duration = Duration::from_millis(100);
const PHASE_TIMEOUT: Duration = Duration::from_secs(60);
const EXIT_TIMEOUT: Duration = Duration::from_secs(10);
const BYTES_PER_MB: f64 = 1024.0 * 1024.0;
const APP_PROCESS_NAME: &str = "Tern";
const SHUTDOWN_PHASE: &str = "shutdown";
const MEMBENCH_USAGE: &str = "usage: xtask membench";
const BUDGETS_RELATIVE_PATH: &str = "test/bench/budgets.toml";
const REPORT_RELATIVE_DIR: &str = "target/membench";
const SUMMARY_ENV: &str = "GITHUB_STEP_SUMMARY";
const STDERR_TAIL_BYTES: usize = 4096;

#[derive(Debug)]
pub enum MembenchError {
    AlreadyRunning { pids: Vec<i32> },
    Io { context: String, source: io::Error },
    Rusage { pid: i32, source: io::Error },
    Budgets { path: PathBuf, message: String },
    EarlyExit { status: String, stderr: String },
    PhaseTimeout { waited: Duration },
    Protocol { line: String, message: String },
    Report { path: PathBuf, message: String },
    Breach { names: Vec<String> },
}

impl fmt::Display for MembenchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MembenchError::AlreadyRunning { pids } => {
                let listed: Vec<String> = pids.iter().map(i32::to_string).collect();
                write!(
                    f,
                    "a Tern process is already running (pid {}). Quit Tern from its menu bar item, then run membench again.",
                    listed.join(", ")
                )
            }
            MembenchError::Io { context, source } => write!(f, "{context}: {source}"),
            MembenchError::Rusage { pid, source } => {
                write!(f, "proc_pid_rusage failed for pid {pid}: {source}")
            }
            MembenchError::Budgets { path, message } => {
                write!(f, "invalid budgets file {}: {message}", path.display())
            }
            MembenchError::EarlyExit { status, stderr } => {
                write!(f, "app exited before the benchmark finished ({status})")?;
                if !stderr.trim().is_empty() {
                    write!(f, "\napp stderr:\n{}", stderr.trim_end())?;
                }
                Ok(())
            }
            MembenchError::PhaseTimeout { waited } => write!(
                f,
                "no phase line received from the app within {} s",
                waited.as_secs()
            ),
            MembenchError::Protocol { line, message } => {
                write!(f, "unexpected app output '{line}': {message}")
            }
            MembenchError::Report { path, message } => {
                write!(f, "cannot write report {}: {message}", path.display())
            }
            MembenchError::Breach { names } => {
                write!(f, "memory budget breached: {}", names.join(", "))
            }
        }
    }
}

fn io_error(context: impl Into<String>) -> impl FnOnce(io::Error) -> MembenchError {
    let context = context.into();
    move |source| MembenchError::Io { context, source }
}

#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub t_ms: u64,
    pub footprint: u64,
    pub peak: u64,
}

pub fn footprint(pid: i32) -> Result<(u64, u64), MembenchError> {
    let mut info = std::mem::MaybeUninit::<libc::rusage_info_v4>::zeroed();
    let buffer = info.as_mut_ptr().cast::<libc::rusage_info_t>();
    let status = unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V4, buffer) };
    if status != 0 {
        return Err(MembenchError::Rusage {
            pid,
            source: io::Error::last_os_error(),
        });
    }
    let info = unsafe { info.assume_init() };
    Ok((info.ri_phys_footprint, info.ri_lifetime_max_phys_footprint))
}

fn running_app_pids() -> Vec<i32> {
    let capacity = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    let Ok(capacity) = usize::try_from(capacity) else {
        return Vec::new();
    };
    if capacity == 0 {
        return Vec::new();
    }
    let slots = capacity * 2;
    let mut pids = vec![0 as libc::pid_t; slots];
    let bytes = i32::try_from(slots * std::mem::size_of::<libc::pid_t>()).unwrap_or(i32::MAX);
    let returned = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    let Ok(returned) = usize::try_from(returned) else {
        return Vec::new();
    };
    pids.truncate(returned);
    pids.into_iter()
        .filter(|pid| *pid > 0 && process_name(*pid).as_deref() == Some(APP_PROCESS_NAME))
        .collect()
}

fn process_name(pid: i32) -> Option<String> {
    let mut buffer = [0u8; 256];
    let length = unsafe {
        libc::proc_name(
            pid,
            buffer.as_mut_ptr().cast(),
            u32::try_from(buffer.len()).unwrap_or(0),
        )
    };
    let length = usize::try_from(length).ok().filter(|length| *length > 0)?;
    Some(String::from_utf8_lossy(&buffer[..length]).into_owned())
}

#[derive(Debug, Deserialize)]
struct BudgetFile {
    budget: Vec<Budget>,
}

#[derive(Debug, Deserialize)]
struct Budget {
    name: String,
    phase: String,
    process: String,
    metric: String,
    target_mb: f64,
    limit_mb: f64,
}

fn load_budgets(path: &Path) -> Result<Vec<Budget>, MembenchError> {
    let text = fs::read_to_string(path).map_err(io_error(format!(
        "cannot read budgets file {}",
        path.display()
    )))?;
    let file: BudgetFile = toml::from_str(&text).map_err(|source| MembenchError::Budgets {
        path: path.to_path_buf(),
        message: source.to_string(),
    })?;
    for budget in &file.budget {
        if budget.process != "app" {
            return Err(MembenchError::Budgets {
                path: path.to_path_buf(),
                message: format!(
                    "budget '{}' has unsupported process '{}'",
                    budget.name, budget.process
                ),
            });
        }
        if budget.metric != "footprint" && budget.metric != "peak" {
            return Err(MembenchError::Budgets {
                path: path.to_path_buf(),
                message: format!(
                    "budget '{}' has unsupported metric '{}'",
                    budget.name, budget.metric
                ),
            });
        }
    }
    Ok(file.budget)
}

#[derive(Debug, Deserialize)]
struct PhaseLine {
    phase: String,
    ready: bool,
}

#[derive(Debug, Serialize)]
struct PhaseResult {
    phase: String,
    median_footprint_bytes: u64,
    max_peak_bytes: u64,
    samples: Vec<SampleRecord>,
}

#[derive(Debug, Serialize)]
struct SampleRecord {
    t_ms: u64,
    footprint: u64,
    peak: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Verdict {
    Pass,
    Fail,
    Skipped,
}

impl Verdict {
    fn label(self) -> &'static str {
        match self {
            Verdict::Pass => "pass",
            Verdict::Fail => "FAIL",
            Verdict::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Serialize)]
struct BudgetResult {
    name: String,
    phase: String,
    process: String,
    metric: String,
    target_mb: f64,
    limit_mb: f64,
    measured_mb: Option<f64>,
    verdict: Verdict,
}

#[derive(Debug, Serialize)]
struct Report {
    phases: Vec<PhaseResult>,
    budgets: Vec<BudgetResult>,
}

struct TempHome {
    path: PathBuf,
}

impl TempHome {
    fn create() -> Result<Self, MembenchError> {
        let path = env::temp_dir().join(format!("tern-membench-{}", std::process::id()));
        if path.exists() {
            fs::remove_dir_all(&path).map_err(io_error(format!(
                "cannot clear temporary home {}",
                path.display()
            )))?;
        }
        fs::create_dir_all(&path).map_err(io_error(format!(
            "cannot create temporary home {}",
            path.display()
        )))?;
        Ok(Self { path })
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct AppProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: mpsc::Receiver<String>,
    stderr_path: PathBuf,
}

impl AppProcess {
    fn launch(binary: &Path, home: &TempHome) -> Result<Self, MembenchError> {
        let stderr_path = home.path.join("app-stderr.log");
        let stderr_file = File::create(&stderr_path)
            .map_err(io_error(format!("cannot create {}", stderr_path.display())))?;
        let mut child = Command::new(binary)
            .env("TERN_HOME", &home.path)
            .env("TERN_BENCH", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr_file))
            .spawn()
            .map_err(io_error(format!("cannot start {}", binary.display())))?;
        let stdout = child.stdout.take().ok_or_else(|| MembenchError::Io {
            context: "cannot capture app stdout".to_string(),
            source: io::Error::other("stdout was not piped"),
        })?;
        let stdin = child.stdin.take();
        let (sender, lines) = mpsc::sync_channel(16);
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            lines,
            stderr_path,
        })
    }

    fn pid(&self) -> i32 {
        i32::try_from(self.child.id()).unwrap_or(0)
    }

    fn next_line(&mut self) -> Result<String, MembenchError> {
        match self.lines.recv_timeout(PHASE_TIMEOUT) {
            Ok(line) => Ok(line),
            Err(RecvTimeoutError::Timeout) => {
                self.terminate();
                Err(MembenchError::PhaseTimeout {
                    waited: PHASE_TIMEOUT,
                })
            }
            Err(RecvTimeoutError::Disconnected) => Err(self.early_exit()),
        }
    }

    fn early_exit(&mut self) -> MembenchError {
        let status = match self.child.wait() {
            Ok(status) => status.to_string(),
            Err(source) => format!("status unavailable: {source}"),
        };
        MembenchError::EarlyExit {
            status,
            stderr: self.stderr_tail(),
        }
    }

    fn stderr_tail(&self) -> String {
        let text = fs::read(&self.stderr_path)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default();
        let start = text.len().saturating_sub(STDERR_TAIL_BYTES);
        let boundary = (start..=text.len())
            .find(|index| text.is_char_boundary(*index))
            .unwrap_or(text.len());
        text[boundary..].to_string()
    }

    fn continue_scenario(&mut self) -> Result<(), MembenchError> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(self.early_exit());
        };
        let written = stdin.write_all(b"\n").and_then(|()| stdin.flush());
        match written {
            Ok(()) => Ok(()),
            Err(_) => Err(self.early_exit()),
        }
    }

    fn finish(&mut self) -> Result<(), MembenchError> {
        self.stdin = None;
        let deadline = Instant::now() + EXIT_TIMEOUT;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return Ok(()),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
                Ok(None) => {
                    self.terminate();
                    return Ok(());
                }
                Err(source) => {
                    return Err(MembenchError::Io {
                        context: "cannot wait for app exit".to_string(),
                        source,
                    });
                }
            }
        }
    }

    fn terminate(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for AppProcess {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            self.terminate();
        }
    }
}

fn sample_window(pid: i32, started: Instant) -> Result<Vec<Sample>, MembenchError> {
    let mut samples = Vec::with_capacity(SAMPLES_PER_PHASE);
    for index in 0..SAMPLES_PER_PHASE {
        if index > 0 {
            thread::sleep(SAMPLE_INTERVAL);
        }
        let (current, peak) = footprint(pid)?;
        samples.push(Sample {
            t_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            footprint: current,
            peak,
        });
    }
    Ok(samples)
}

fn median(values: &mut [u64]) -> u64 {
    values.sort_unstable();
    match values.len() {
        0 => 0,
        length if length % 2 == 1 => values[length / 2],
        length => {
            let low = values[length / 2 - 1];
            let high = values[length / 2];
            low + (high - low) / 2
        }
    }
}

fn summarize(phase: &str, samples: &[Sample]) -> PhaseResult {
    let mut footprints: Vec<u64> = samples.iter().map(|sample| sample.footprint).collect();
    PhaseResult {
        phase: phase.to_string(),
        median_footprint_bytes: median(&mut footprints),
        max_peak_bytes: samples.iter().map(|sample| sample.peak).max().unwrap_or(0),
        samples: samples
            .iter()
            .map(|sample| SampleRecord {
                t_ms: sample.t_ms,
                footprint: sample.footprint,
                peak: sample.peak,
            })
            .collect(),
    }
}

fn drive(app: &mut AppProcess) -> Result<Vec<PhaseResult>, MembenchError> {
    let started = Instant::now();
    let mut results = Vec::new();
    loop {
        let line = app.next_line()?;
        let parsed: PhaseLine =
            serde_json::from_str(&line).map_err(|source| MembenchError::Protocol {
                line: line.clone(),
                message: source.to_string(),
            })?;
        if !parsed.ready {
            return Err(MembenchError::Protocol {
                line,
                message: "phase line has ready=false".to_string(),
            });
        }
        if parsed.phase == SHUTDOWN_PHASE {
            app.finish()?;
            return Ok(results);
        }
        let samples = sample_window(app.pid(), started)?;
        results.push(summarize(&parsed.phase, &samples));
        app.continue_scenario()?;
    }
}

fn evaluate(budgets: &[Budget], phases: &[PhaseResult]) -> Vec<BudgetResult> {
    budgets
        .iter()
        .map(|budget| {
            let measured = phases
                .iter()
                .find(|phase| phase.phase == budget.phase)
                .map(|phase| match budget.metric.as_str() {
                    "peak" => phase.max_peak_bytes,
                    _ => phase.median_footprint_bytes,
                });
            let measured_mb = measured.map(|bytes| bytes as f64 / BYTES_PER_MB);
            let verdict = match measured_mb {
                None => Verdict::Skipped,
                Some(value) if value > budget.limit_mb => Verdict::Fail,
                Some(_) => Verdict::Pass,
            };
            BudgetResult {
                name: budget.name.clone(),
                phase: budget.phase.clone(),
                process: budget.process.clone(),
                metric: budget.metric.clone(),
                target_mb: budget.target_mb,
                limit_mb: budget.limit_mb,
                measured_mb,
                verdict,
            }
        })
        .collect()
}

fn render_table(results: &[BudgetResult]) -> String {
    let mut table = String::from(
        "| Budget | Phase | Metric | Measured (MB) | Target (MB) | Limit (MB) | Result |\n\
         |---|---|---|---:|---:|---:|---|\n",
    );
    for result in results {
        let measured = result
            .measured_mb
            .map_or_else(|| "-".to_string(), |value| format!("{value:.1}"));
        table.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            result.name,
            result.phase,
            result.metric,
            measured,
            result.target_mb,
            result.limit_mb,
            result.verdict.label()
        ));
    }
    table
}

fn write_report(root: &Path, report: &Report) -> Result<PathBuf, MembenchError> {
    let directory = root.join(REPORT_RELATIVE_DIR);
    fs::create_dir_all(&directory).map_err(io_error(format!(
        "cannot create report directory {}",
        directory.display()
    )))?;
    let path = directory.join("report.json");
    let json = serde_json::to_string_pretty(report).map_err(|source| MembenchError::Report {
        path: path.clone(),
        message: source.to_string(),
    })?;
    fs::write(&path, json).map_err(|source| MembenchError::Report {
        path: path.clone(),
        message: source.to_string(),
    })?;
    Ok(path)
}

fn append_summary(table: &str) -> Result<(), MembenchError> {
    let Some(path) = env::var_os(SUMMARY_ENV).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let path = PathBuf::from(path);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(io_error(format!(
            "cannot open job summary {}",
            path.display()
        )))?;
    writeln!(file, "### Memory benchmark\n\n{table}").map_err(io_error(format!(
        "cannot write job summary {}",
        path.display()
    )))
}

fn execute(root: &Path, budgets: &[Budget]) -> Result<(), MembenchError> {
    let binary = root.join("dist/Tern.app/Contents/MacOS/Tern");
    let home = TempHome::create()?;
    let mut app = AppProcess::launch(&binary, &home)?;
    let phases = drive(&mut app)?;
    drop(app);
    let results = evaluate(budgets, &phases);
    let table = render_table(&results);
    println!("{table}");
    let report = Report {
        phases,
        budgets: results,
    };
    let report_path = write_report(root, &report)?;
    println!("report: {}", report_path.display());
    append_summary(&table)?;
    let breaches: Vec<String> = report
        .budgets
        .iter()
        .filter(|result| result.verdict == Verdict::Fail)
        .map(|result| result.name.clone())
        .collect();
    if breaches.is_empty() {
        Ok(())
    } else {
        Err(MembenchError::Breach { names: breaches })
    }
}

pub fn run(args: &[String]) -> Result<(), XtaskError> {
    if let Some(extra) = args.first() {
        return Err(XtaskError::Usage(format!(
            "membench: unknown argument '{extra}'\n{MEMBENCH_USAGE}"
        )));
    }
    let pids = running_app_pids();
    if !pids.is_empty() {
        return Err(XtaskError::Membench(MembenchError::AlreadyRunning { pids }));
    }
    let root = workspace_root();
    let budgets = load_budgets(&root.join(BUDGETS_RELATIVE_PATH)).map_err(XtaskError::Membench)?;
    bundle(&["--release".to_string()])?;
    execute(&root, &budgets).map_err(XtaskError::Membench)
}
