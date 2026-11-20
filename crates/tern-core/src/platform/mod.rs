use crate::error::{CoreError, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(not(target_os = "macos"))]
pub mod unsupported;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const PNG_HEADER_LEN: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode {
    Region,
    Window,
    FullScreen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureInfo {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
}

pub trait Platform: Send + Sync {
    fn capture(&self, mode: CaptureMode, dest: &Path) -> Result<Option<CaptureInfo>, CoreError>;
    fn downscale_png(&self, src: &Path, dest: &Path, max_edge: u32) -> Result<(), CoreError>;
    fn set_background_priority(&self);
    fn login_shell_path(&self, timeout: Duration) -> Result<String, CoreError>;
    fn spotlight_names(
        &self,
        query: &str,
        limit: usize,
        timeout: Duration,
    ) -> Result<Vec<PathBuf>, CoreError>;
    fn screen_capture_allowed(&self) -> Option<bool>;
}

pub fn png_dimensions(header: &[u8]) -> Result<(u32, u32), CoreError> {
    if header.len() < PNG_HEADER_LEN {
        return Err(CoreError::new(
            ErrorKind::Capture,
            format!(
                "PNG data is {} bytes, need at least {PNG_HEADER_LEN}",
                header.len()
            ),
        ));
    }
    if header[..PNG_SIGNATURE.len()] != PNG_SIGNATURE || &header[12..16] != b"IHDR" {
        return Err(CoreError::new(
            ErrorKind::Capture,
            "data does not start with a PNG signature and IHDR chunk",
        ));
    }
    let width = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);
    let height = u32::from_be_bytes([header[20], header[21], header[22], header[23]]);
    Ok((width, height))
}

pub fn read_png_dimensions(path: &Path) -> Result<(u32, u32), CoreError> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|err| {
        CoreError::new(
            ErrorKind::Capture,
            format!("cannot open {}: {err}", path.display()),
        )
    })?;
    let mut header = Vec::with_capacity(PNG_HEADER_LEN);
    file.take(PNG_HEADER_LEN as u64)
        .read_to_end(&mut header)
        .map_err(|err| {
            CoreError::new(
                ErrorKind::Capture,
                format!("cannot read {}: {err}", path.display()),
            )
        })?;
    png_dimensions(&header).map_err(|err| {
        CoreError::new(
            ErrorKind::Capture,
            format!("{}: {}", path.display(), err.message),
        )
    })
}

#[cfg(target_os = "macos")]
pub fn current() -> Arc<dyn Platform> {
    Arc::new(macos::MacPlatform)
}

#[cfg(not(target_os = "macos"))]
pub fn current() -> Arc<dyn Platform> {
    Arc::new(unsupported::UnsupportedPlatform)
}
