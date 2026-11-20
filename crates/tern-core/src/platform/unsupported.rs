use super::{CaptureInfo, CaptureMode, Platform};
use crate::error::{CoreError, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct UnsupportedPlatform;

fn unsupported(operation: &str) -> CoreError {
    CoreError::new(
        ErrorKind::Unsupported,
        format!("{operation} is not supported on this platform"),
    )
}

impl Platform for UnsupportedPlatform {
    fn capture(&self, _mode: CaptureMode, _dest: &Path) -> Result<Option<CaptureInfo>, CoreError> {
        Err(unsupported("screen capture"))
    }

    fn downscale_png(&self, _src: &Path, _dest: &Path, _max_edge: u32) -> Result<(), CoreError> {
        Err(unsupported("image downscaling"))
    }

    fn set_background_priority(&self) {}

    fn login_shell_path(&self, _timeout: Duration) -> Result<String, CoreError> {
        Err(unsupported("login shell PATH lookup"))
    }

    fn spotlight_names(
        &self,
        _query: &str,
        _limit: usize,
        _timeout: Duration,
    ) -> Result<Vec<PathBuf>, CoreError> {
        Err(unsupported("Spotlight search"))
    }

    fn screen_capture_allowed(&self) -> Option<bool> {
        None
    }
}
