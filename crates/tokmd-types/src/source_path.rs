//! Admission of native source paths to UTF-8 receipt and model APIs.

use std::fmt;
use std::path::Path;

/// A native path component cannot be represented in a UTF-8 receipt.
#[derive(Debug)]
pub struct UnsupportedSourcePath {
    component: Vec<u8>,
    length: usize,
}

impl fmt::Display for UnsupportedSourcePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Unsupported source path: non-UTF-8 component (hex ")?;
        for byte in &self.component {
            write!(f, "{byte:02x}")?;
        }
        if self.length > self.component.len() {
            write!(f, "..., {} bytes", self.length)?;
        }
        write!(f, "). Rename the component to valid UTF-8 and retry")
    }
}

impl std::error::Error for UnsupportedSourcePath {}

/// Borrow a representable native path without transforming its identity.
/// On failure, identify the offending component without exposing its parent path.
/// The diagnostic is capped for untrusted in-memory inputs.
pub fn validate_source_path(path: &Path) -> Result<&str, UnsupportedSourcePath> {
    path.to_str().ok_or_else(|| {
        let bad = path
            .components()
            .map(|part| part.as_os_str())
            .find(|part| part.to_str().is_none())
            .unwrap_or(path.as_os_str());
        let bytes = bad.as_encoded_bytes();
        UnsupportedSourcePath {
            component: bytes.iter().take(768).copied().collect(),
            length: bytes.len(),
        }
    })
}
