//! File format import/export for muse.

mod binary;
pub mod gp;

use std::path::Path;

use muse_core::Score;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported format: {0}")]
    Unsupported(String),
    #[error("parse error at byte {offset}{}: {message}", location.as_deref().map(|l| format!(" ({l})")).unwrap_or_default())]
    Parse {
        offset: usize,
        location: Option<String>,
        message: String,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

/// Formats muse can identify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Guitar Pro 3, 4 or 5 binary file.
    GuitarPro3To5,
    /// Guitar Pro 6 (`.gpx`).
    Gpx,
    /// Guitar Pro 7/8 (`.gp`).
    Gp7,
}

/// Identifies a file's format from its magic bytes, falling back to the extension.
pub fn detect(bytes: &[u8], path: Option<&Path>) -> Option<Format> {
    if bytes.len() > 20 && bytes[1..].starts_with(b"FICHIER GUITAR") {
        return Some(Format::GuitarPro3To5);
    }
    if bytes.starts_with(b"BCFZ") || bytes.starts_with(b"BCFS") {
        return Some(Format::Gpx);
    }
    let ext = path?.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "gp3" | "gp4" | "gp5" => Some(Format::GuitarPro3To5),
        "gpx" => Some(Format::Gpx),
        "gp" => Some(Format::Gp7),
        _ => None,
    }
}

/// Reads a score from bytes, detecting the format.
pub fn read_bytes(bytes: &[u8], path: Option<&Path>) -> Result<Score> {
    match detect(bytes, path) {
        Some(Format::GuitarPro3To5) => gp::read(bytes),
        Some(other) => Err(Error::Unsupported(format!(
            "{other:?} (planned for milestone 2)"
        ))),
        None => Err(Error::Unsupported("unrecognised file".into())),
    }
}

/// Reads a score from a file, detecting the format.
pub fn read_file(path: &Path) -> Result<Score> {
    let bytes = std::fs::read(path)?;
    read_bytes(&bytes, Some(path))
}
