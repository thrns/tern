use crate::error::{CoreError, ErrorKind};
use std::env;
use std::fs::DirBuilder;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

const HOME_ENV: &str = "HOME";
const TERN_HOME_ENV: &str = "TERN_HOME";
const DEFAULT_HOME_SUFFIX: &str = "Library/Application Support/Tern";
const PRIVATE_DIR_MODE: u32 = 0o700;
const SOCKET_PRIMARY_LIMIT: usize = 100;
const SOCKET_FILE_NAME: &str = "s";
const MAX_ID_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TernPaths {
    pub home: PathBuf,
}

impl TernPaths {
    pub fn resolve(override_home: Option<&Path>) -> Result<Self, CoreError> {
        if let Some(path) = override_home {
            return Ok(Self {
                home: path.to_path_buf(),
            });
        }
        if let Some(path) = env::var_os(TERN_HOME_ENV).filter(|value| !value.is_empty()) {
            return Ok(Self {
                home: PathBuf::from(path),
            });
        }
        let user_home = env::var_os(HOME_ENV)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                CoreError::new(
                    ErrorKind::Config,
                    format!("{HOME_ENV} is not set; set {TERN_HOME_ENV} to choose the Tern home"),
                )
            })?;
        Ok(Self {
            home: PathBuf::from(user_home).join(DEFAULT_HOME_SUFFIX),
        })
    }

    pub fn ensure_dirs(&self) -> Result<(), CoreError> {
        let socket = self.socket_path();
        let run_dir = self.run_dir();
        let mut dirs = vec![
            self.home.clone(),
            self.captures_dir(),
            self.workspaces_dir(),
            self.prompts_dir(),
            self.logs_dir(),
            run_dir.clone(),
        ];
        if let Some(parent) = socket.parent()
            && parent != run_dir
        {
            dirs.push(parent.to_path_buf());
        }
        for dir in &dirs {
            create_private_dir(dir)?;
        }
        Ok(())
    }

    pub fn config_file(&self) -> PathBuf {
        self.home.join("config.toml")
    }

    pub fn index_db(&self) -> PathBuf {
        self.home.join("index.db")
    }

    pub fn state_db(&self) -> PathBuf {
        self.home.join("state.db")
    }

    pub fn captures_dir(&self) -> PathBuf {
        self.home.join("captures")
    }

    pub fn workspaces_dir(&self) -> PathBuf {
        self.home.join("workspaces")
    }

    pub fn workspace_dir(&self, conversation_id: &str) -> PathBuf {
        self.workspaces_dir().join(conversation_id)
    }

    pub fn prompts_dir(&self) -> PathBuf {
        self.home.join("prompts")
    }

    pub fn system_prompt_file(&self) -> PathBuf {
        self.prompts_dir().join("system.md")
    }

    pub fn run_dir(&self) -> PathBuf {
        self.home.join("run")
    }

    pub fn socket_path(&self) -> PathBuf {
        let primary = self.run_dir().join(SOCKET_FILE_NAME);
        if primary.as_os_str().as_bytes().len() <= SOCKET_PRIMARY_LIMIT {
            return primary;
        }
        env::temp_dir()
            .join(format!("tern-{}", current_uid()))
            .join(SOCKET_FILE_NAME)
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.home.join("logs")
    }

    pub fn app_log_file(&self) -> PathBuf {
        self.logs_dir().join("tern.log")
    }

    pub fn mcp_log_file(&self) -> PathBuf {
        self.logs_dir().join("tern-mcp.log")
    }
}

pub fn validate_id(id: &str) -> Result<(), CoreError> {
    let length_ok = (1..=MAX_ID_LEN).contains(&id.len());
    let characters_ok = id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    if length_ok && characters_ok {
        Ok(())
    } else {
        Err(CoreError::new(
            ErrorKind::InvalidInput,
            format!("id must be 1 to {MAX_ID_LEN} ASCII letters, digits or hyphens"),
        ))
    }
}

fn create_private_dir(path: &Path) -> Result<(), CoreError> {
    DirBuilder::new()
        .recursive(true)
        .mode(PRIVATE_DIR_MODE)
        .create(path)
        .map_err(|err| CoreError::new(ErrorKind::Io, format!("{}: {err}", path.display())))
}

fn current_uid() -> libc::uid_t {
    unsafe { libc::getuid() }
}
