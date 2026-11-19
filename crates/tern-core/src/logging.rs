use crate::error::{CoreError, ErrorKind};
use log::{LevelFilter, Log, Metadata, Record};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_LOG_BYTES: u64 = 1024 * 1024;
const TERN_LOG_ENV: &str = "TERN_LOG";

struct Sink {
    path: PathBuf,
    file: File,
    written: u64,
}

static SINK: Mutex<Option<Sink>> = Mutex::new(None);
static LOGGER: FileLogger = FileLogger;

pub struct FileLogger;

impl FileLogger {
    pub fn init(path: &Path, level: LevelFilter) -> Result<(), CoreError> {
        let mut guard = SINK
            .lock()
            .map_err(|_| CoreError::new(ErrorKind::Internal, "logger state is poisoned"))?;
        if guard.is_some() {
            return Ok(());
        }
        let file = open_append(path)
            .map_err(|err| CoreError::new(ErrorKind::Io, format!("{}: {err}", path.display())))?;
        let written = file.metadata().map_or(0, |metadata| metadata.len());
        log::set_logger(&LOGGER)
            .map_err(|err| CoreError::new(ErrorKind::Internal, err.to_string()))?;
        log::set_max_level(level);
        *guard = Some(Sink {
            path: path.to_path_buf(),
            file,
            written,
        });
        Ok(())
    }
}

pub fn level_from_env() -> LevelFilter {
    match std::env::var(TERN_LOG_ENV) {
        Ok(value) if value.eq_ignore_ascii_case("debug") => LevelFilter::Debug,
        _ => LevelFilter::Info,
    }
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {} {}: {}\n",
            format_timestamp(now_since_epoch()),
            record.level(),
            record.target(),
            record.args()
        );
        if let Ok(mut guard) = SINK.lock()
            && let Some(sink) = guard.as_mut()
        {
            sink.write_line(&line);
        }
    }

    fn flush(&self) {
        if let Ok(mut guard) = SINK.lock()
            && let Some(sink) = guard.as_mut()
        {
            let _ = sink.file.flush();
        }
    }
}

impl Sink {
    fn write_line(&mut self, line: &str) {
        if self.written > MAX_LOG_BYTES {
            self.rotate();
        }
        if self.file.write_all(line.as_bytes()).is_ok() {
            self.written += line.len() as u64;
        }
    }

    fn rotate(&mut self) {
        let first = numbered(&self.path, 1);
        let second = numbered(&self.path, 2);
        let _ = fs::rename(&first, &second);
        let _ = fs::rename(&self.path, &first);
        if let Ok(file) = open_append(&self.path) {
            self.file = file;
            self.written = 0;
        }
    }
}

fn open_append(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

fn numbered(path: &Path, index: u32) -> PathBuf {
    let mut name = OsString::from(path.as_os_str());
    name.push(format!(".{index}"));
    PathBuf::from(name)
}

fn now_since_epoch() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

fn format_timestamp(since_epoch: Duration) -> String {
    let total_seconds = since_epoch.as_secs();
    let millis = since_epoch.subsec_millis();
    let days = (total_seconds / 86_400) as i64;
    let seconds_of_day = total_seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        seconds_of_day / 3_600,
        (seconds_of_day % 3_600) / 60,
        seconds_of_day % 60
    )
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let shifted = days_since_epoch + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = (shifted - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    let year = year_of_era as i64 + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
