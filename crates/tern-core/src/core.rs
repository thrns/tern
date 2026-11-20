use crate::error::{CoreError, ErrorKind};
use crate::logging::{FileLogger, level_from_env};
use crate::paths::TernPaths;
use crate::settings::Settings;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Core {
    paths: TernPaths,
    settings: Mutex<Settings>,
}

impl Core {
    pub fn open(home: Option<PathBuf>) -> Result<Core, CoreError> {
        let paths = TernPaths::resolve(home.as_deref())?;
        paths.ensure_dirs()?;
        FileLogger::init(&paths.app_log_file(), level_from_env())?;
        let (settings, _notice) = Settings::load(&paths.config_file())?;
        Ok(Core {
            paths,
            settings: Mutex::new(settings),
        })
    }

    pub fn settings(&self) -> Settings {
        self.settings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn update_settings(&self, settings: Settings) -> Result<(), CoreError> {
        let mut current = self
            .settings
            .lock()
            .map_err(|_| CoreError::new(ErrorKind::Internal, "settings state is poisoned"))?;
        settings.validate()?;
        settings.save(&self.paths.config_file())?;
        *current = settings;
        Ok(())
    }

    pub fn version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}
