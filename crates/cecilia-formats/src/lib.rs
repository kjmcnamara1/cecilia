//! File format import/export for cecilia.

mod binary;
pub mod gabc;
pub mod gp;
pub mod gp_write;
pub mod gpif;
pub mod gpif_write;
pub mod gpx;
pub mod midi;
pub mod musicxml;
pub mod native;

use std::path::Path;

use cecilia_core::Score;

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

/// Formats cecilia can identify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Guitar Pro 3, 4 or 5 binary file.
    GuitarPro3To5,
    /// Guitar Pro 6 (`.gpx`).
    Gpx,
    /// Guitar Pro 7/8 (`.gp`).
    Gp7,
    /// Standard MIDI File.
    Midi,
    /// MusicXML, uncompressed.
    MusicXml,
    /// MusicXML, compressed (`.mxl`).
    Mxl,
    /// cecilia's own format.
    Cecilia,
    /// Gregorio chant notation.
    Gabc,
}

/// Identifies a file's format from its magic bytes, falling back to the extension.
pub fn detect(bytes: &[u8], path: Option<&Path>) -> Option<Format> {
    if bytes.len() > 20 && bytes[1..].starts_with(b"FICHIER GUITAR") {
        return Some(Format::GuitarPro3To5);
    }
    if bytes.starts_with(b"BCFZ") || bytes.starts_with(b"BCFS") {
        return Some(Format::Gpx);
    }
    if native::is_cecilia(bytes) {
        return Some(Format::Cecilia);
    }
    if bytes.starts_with(b"MThd") {
        return Some(Format::Midi);
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]);
    if head.contains("<score-partwise") {
        return Some(Format::MusicXml);
    }
    if gabc::is_gabc(bytes) {
        return Some(Format::Gabc);
    }
    let ext = path?.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "gp3" | "gp4" | "gp5" => Some(Format::GuitarPro3To5),
        "gpx" => Some(Format::Gpx),
        "gp" => Some(Format::Gp7),
        "mid" | "midi" => Some(Format::Midi),
        "musicxml" | "xml" => Some(Format::MusicXml),
        "mxl" => Some(Format::Mxl),
        "cc" => Some(Format::Cecilia),
        "gabc" => Some(Format::Gabc),
        _ => None,
    }
}

/// Reads a score from bytes, detecting the format. Readers of multi-staff instruments give
/// one track each ([`Score::merge_staves`]).
pub fn read_bytes(bytes: &[u8], path: Option<&Path>) -> Result<Score> {
    match detect(bytes, path) {
        Some(Format::GuitarPro3To5) => gp::read(bytes),
        Some(Format::Gpx) => gpif::read_gpx(bytes),
        Some(Format::Gp7) => gpif::read_gp7(bytes),
        Some(Format::Midi) => midi::read_midi(bytes),
        Some(Format::MusicXml) => musicxml::read_musicxml(bytes),
        Some(Format::Mxl) => musicxml::read_mxl(bytes),
        Some(Format::Cecilia) => native::read_cecilia(bytes),
        Some(Format::Gabc) => gabc::read_gabc(bytes),
        None => Err(Error::Unsupported("unrecognised file".into())),
    }
}

/// Reads a score from a file, detecting the format.
pub fn read_file(path: &Path) -> Result<Score> {
    let bytes = std::fs::read(path)?;
    read_bytes(&bytes, Some(path))
}

/// Output formats, chosen by file extension. Guitar Pro and MusicXML writers store a grand
/// staff as two staves of one instrument ([`Score::split_grand`]).
pub fn write_bytes(score: &Score, extension: &str) -> Result<Vec<u8>> {
    // Formats keep a velocity per note: they get the beats' dynamics.
    let mut applied = score.clone();
    applied.apply_dynamics();
    let score = &applied;
    match extension.to_ascii_lowercase().as_str() {
        "gp5" => gp_write::write_gp5(score),
        "gp" => gpif_write::write_gp7(score),
        "gpx" => gpif_write::write_gpx(score),
        "mid" | "midi" => midi::write_midi(score),
        "musicxml" | "xml" => musicxml::write_musicxml(score),
        "mxl" => musicxml::write_mxl(score),
        "cc" => native::write_cecilia(score),
        "gabc" => gabc::write_gabc(score),
        other => Err(Error::Unsupported(format!("cannot write .{other} files"))),
    }
}

/// Writes a score to a file in the format given by its extension.
pub fn write_file(score: &Score, path: &Path) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    let bytes = write_bytes(score, ext)?;
    std::fs::write(path, bytes)?;
    Ok(())
}
