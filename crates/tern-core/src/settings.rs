use crate::error::{CoreError, ErrorKind};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::env;
use std::fmt::Display;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const HOME_ENV: &str = "HOME";
const TMP_SUFFIX: &str = ".tmp";
const BROKEN_SUFFIX_PREFIX: &str = ".broken-";
const MIN_CONTENT_FILE_BYTES: u64 = 64 * 1024;
const MAX_CONTENT_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MIN_TURN_TIMEOUT_SECS: u32 = 30;
const MAX_TURN_TIMEOUT_SECS: u32 = 3600;
const MIN_RETENTION_DAYS: u32 = 1;
const MAX_RETENTION_DAYS: u32 = 90;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub hotkey_panel: String,
    pub hotkey_capture_ask: String,
    pub default_provider: String,
    pub providers: Vec<ProviderSettings>,
    pub ai_mode: AiMode,
    pub work_dir: Option<String>,
    pub allow_ai_screenshots: bool,
    pub index_roots: Vec<String>,
    pub index_exclude_globs: Vec<String>,
    pub max_content_file_bytes: u64,
    pub capture_retention_days: u32,
    pub turn_timeout_secs: u32,
    pub launch_at_login: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSettings {
    pub id: String,
    pub enabled: bool,
    pub binary_path: Option<String>,
    pub model: Option<String>,
}

impl ProviderSettings {
    fn with_enabled(id: &str, enabled: bool) -> Self {
        Self {
            id: id.to_string(),
            enabled,
            binary_path: None,
            model: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiMode {
    ReadOnly,
    AllowEdits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotkey {
    pub modifiers: Vec<Modifier>,
    pub key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Modifier {
    Cmd,
    Alt,
    Ctrl,
    Shift,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey_panel: "alt+space".to_string(),
            hotkey_capture_ask: "alt+shift+space".to_string(),
            default_provider: "claude".to_string(),
            providers: vec![
                ProviderSettings::with_enabled("claude", true),
                ProviderSettings::with_enabled("codex", true),
                ProviderSettings::with_enabled("agy", true),
                ProviderSettings::with_enabled("gemini", false),
            ],
            ai_mode: AiMode::ReadOnly,
            work_dir: None,
            allow_ai_screenshots: false,
            index_roots: Vec::new(),
            index_exclude_globs: Vec::new(),
            max_content_file_bytes: 2_097_152,
            capture_retention_days: 7,
            turn_timeout_secs: 600,
            launch_at_login: false,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Result<(Settings, Option<String>), CoreError> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Settings::default(), None));
            }
            Err(err) => {
                return Err(CoreError::new(
                    ErrorKind::Io,
                    format!("cannot read {}: {err}", path.display()),
                ));
            }
        };
        match parse_settings(&bytes) {
            Ok(settings) => Ok((settings, None)),
            Err(()) => {
                let broken = sibling_with_suffix(path, &broken_suffix())?;
                fs::rename(path, &broken).map_err(|err| {
                    CoreError::new(
                        ErrorKind::Io,
                        format!(
                            "cannot move broken settings {} aside: {err}",
                            path.display()
                        ),
                    )
                })?;
                let warning = format!(
                    "settings file could not be parsed and was moved to {}; default settings are in use",
                    broken.display()
                );
                log::warn!("{warning}");
                Ok((Settings::default(), Some(warning)))
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        let text = toml::to_string_pretty(self)
            .map_err(|err| config_error(format!("cannot serialize settings: {err}")))?;
        let tmp = sibling_with_suffix(path, TMP_SUFFIX)?;
        if let Err(err) = write_synced(&tmp, text.as_bytes()) {
            let _ = fs::remove_file(&tmp);
            return Err(err);
        }
        if let Err(err) = fs::rename(&tmp, path) {
            let _ = fs::remove_file(&tmp);
            return Err(CoreError::new(
                ErrorKind::Io,
                format!("cannot replace {}: {err}", path.display()),
            ));
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        let home = env_home()?;
        self.validate_with_home(&home)
    }

    pub fn validate_with_home(&self, home: &Path) -> Result<(), CoreError> {
        let panel = parse_hotkey(&self.hotkey_panel)?;
        let capture = parse_hotkey(&self.hotkey_capture_ask)?;
        if panel == capture {
            return Err(config_error(
                "hotkey_panel and hotkey_capture_ask must be different",
            ));
        }
        self.validate_providers()?;
        self.validate_ai_mode()?;
        self.validate_index_roots(home)?;
        check_range(
            "max_content_file_bytes",
            self.max_content_file_bytes,
            MIN_CONTENT_FILE_BYTES,
            MAX_CONTENT_FILE_BYTES,
        )?;
        check_range(
            "turn_timeout_secs",
            self.turn_timeout_secs,
            MIN_TURN_TIMEOUT_SECS,
            MAX_TURN_TIMEOUT_SECS,
        )?;
        check_range(
            "capture_retention_days",
            self.capture_retention_days,
            MIN_RETENTION_DAYS,
            MAX_RETENTION_DAYS,
        )
    }

    pub fn provider(&self, id: &str) -> Option<&ProviderSettings> {
        self.providers.iter().find(|provider| provider.id == id)
    }

    fn validate_providers(&self) -> Result<(), CoreError> {
        let mut ids = HashSet::new();
        for provider in &self.providers {
            if !ids.insert(provider.id.as_str()) {
                return Err(config_error(format!(
                    "duplicate provider id '{}'",
                    provider.id
                )));
            }
        }
        match self.provider(&self.default_provider) {
            None => Err(config_error(format!(
                "default provider '{}' is not in the provider list",
                self.default_provider
            ))),
            Some(provider) if !provider.enabled => Err(config_error(format!(
                "default provider '{}' is disabled",
                self.default_provider
            ))),
            Some(_) => Ok(()),
        }
    }

    fn validate_ai_mode(&self) -> Result<(), CoreError> {
        if self.ai_mode != AiMode::AllowEdits {
            return Ok(());
        }
        match &self.work_dir {
            None => Err(config_error("ai_mode allow_edits requires work_dir")),
            Some(dir) if Path::new(dir).is_dir() => Ok(()),
            Some(dir) => Err(config_error(format!(
                "work_dir '{dir}' is not an existing directory"
            ))),
        }
    }

    fn validate_index_roots(&self, home: &Path) -> Result<(), CoreError> {
        if self.index_roots.is_empty() {
            return Ok(());
        }
        let home = fs::canonicalize(home).map_err(|err| {
            CoreError::new(
                ErrorKind::Io,
                format!("cannot resolve home directory: {err}"),
            )
        })?;
        let mut roots: Vec<PathBuf> = Vec::with_capacity(self.index_roots.len());
        for raw in &self.index_roots {
            let path = Path::new(raw);
            if !path.is_absolute() {
                return Err(config_error(format!(
                    "index root '{raw}' must be an absolute path"
                )));
            }
            let root = fs::canonicalize(path).map_err(|err| {
                config_error(format!("index root '{raw}' is not accessible: {err}"))
            })?;
            if !root.is_dir() {
                return Err(config_error(format!(
                    "index root '{raw}' is not a directory"
                )));
            }
            if root.parent().is_none() {
                return Err(config_error("the filesystem root cannot be an index root"));
            }
            if root == home {
                return Err(config_error(format!(
                    "index root '{raw}' cannot be the home directory"
                )));
            }
            roots.push(root);
        }
        for (index, root) in roots.iter().enumerate() {
            for other in &roots[index + 1..] {
                if root.starts_with(other) || other.starts_with(root) {
                    return Err(config_error(format!(
                        "index roots overlap: '{}' and '{}'",
                        root.display(),
                        other.display()
                    )));
                }
            }
        }
        Ok(())
    }
}

pub fn parse_hotkey(spec: &str) -> Result<Hotkey, CoreError> {
    let tokens: Vec<&str> = spec.split('+').collect();
    let Some((key, modifier_tokens)) = tokens.split_last() else {
        return Err(hotkey_error(spec, "the hotkey is empty".to_string()));
    };
    let mut modifiers: Vec<Modifier> = Vec::with_capacity(modifier_tokens.len());
    for token in modifier_tokens {
        let modifier = parse_modifier(token)
            .ok_or_else(|| hotkey_error(spec, format!("'{token}' is not a modifier")))?;
        if modifiers.contains(&modifier) {
            return Err(hotkey_error(
                spec,
                format!("modifier '{token}' appears twice"),
            ));
        }
        modifiers.push(modifier);
    }
    if modifiers.is_empty() {
        return Err(hotkey_error(
            spec,
            "at least one modifier is required".to_string(),
        ));
    }
    if !is_valid_key(key) {
        return Err(hotkey_error(
            spec,
            format!("'{key}' is not a supported key"),
        ));
    }
    modifiers.sort();
    Ok(Hotkey {
        modifiers,
        key: key.to_string(),
    })
}

fn parse_modifier(token: &str) -> Option<Modifier> {
    match token {
        "cmd" => Some(Modifier::Cmd),
        "alt" | "option" => Some(Modifier::Alt),
        "ctrl" => Some(Modifier::Ctrl),
        "shift" => Some(Modifier::Shift),
        _ => None,
    }
}

fn is_valid_key(token: &str) -> bool {
    if matches!(token, "space" | "return" | "tab") {
        return true;
    }
    if let Some(number) = token.strip_prefix('f') {
        return !number.starts_with('0')
            && number.bytes().all(|byte| byte.is_ascii_digit())
            && matches!(number.parse::<u8>(), Ok(1..=12));
    }
    let mut chars = token.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some(character), None) if character.is_ascii_lowercase() || character.is_ascii_digit()
    )
}

fn parse_settings(bytes: &[u8]) -> Result<Settings, ()> {
    let text = std::str::from_utf8(bytes).map_err(|_| ())?;
    toml::from_str(text).map_err(|_| ())
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    let mut file = File::create(path).map_err(|err| {
        CoreError::new(
            ErrorKind::Io,
            format!("cannot create {}: {err}", path.display()),
        )
    })?;
    file.write_all(bytes).map_err(|err| {
        CoreError::new(
            ErrorKind::Io,
            format!("cannot write {}: {err}", path.display()),
        )
    })?;
    file.sync_all().map_err(|err| {
        CoreError::new(
            ErrorKind::Io,
            format!("cannot sync {}: {err}", path.display()),
        )
    })
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> Result<PathBuf, CoreError> {
    let file_name = path.file_name().ok_or_else(|| {
        CoreError::new(
            ErrorKind::InvalidInput,
            format!("settings path has no file name: {}", path.display()),
        )
    })?;
    let mut name = file_name.to_os_string();
    name.push(suffix);
    Ok(path.with_file_name(name))
}

fn broken_suffix() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    format!("{BROKEN_SUFFIX_PREFIX}{seconds}")
}

fn env_home() -> Result<PathBuf, CoreError> {
    env::var_os(HOME_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| config_error(format!("{HOME_ENV} is not set")))
}

fn check_range<T>(name: &str, value: T, min: T, max: T) -> Result<(), CoreError>
where
    T: Copy + PartialOrd + Display,
{
    if value < min || value > max {
        return Err(config_error(format!(
            "{name} must be between {min} and {max}, got {value}"
        )));
    }
    Ok(())
}

fn config_error(message: impl Into<String>) -> CoreError {
    CoreError::new(ErrorKind::Config, message)
}

fn hotkey_error(spec: &str, reason: String) -> CoreError {
    CoreError::new(
        ErrorKind::InvalidInput,
        format!("invalid hotkey '{spec}': {reason}"),
    )
}
