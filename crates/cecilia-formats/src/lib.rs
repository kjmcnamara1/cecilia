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

use std::io::Read;
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

/// Most bytes read from the entries of one archive (a guard against zip bombs).
pub(crate) const MAX_ARCHIVE_BYTES: u64 = 256 << 20;

/// Reads an archive entry to its end, charging `budget` (bytes left for the whole archive);
/// an entry that would overrun it is an error rather than an allocation.
pub(crate) fn read_entry(
    entry: impl std::io::Read,
    budget: &mut u64,
    what: &str,
) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    entry.take(*budget + 1).read_to_end(&mut data)?;
    if data.len() as u64 > *budget {
        return Err(Error::Unsupported(format!(
            "{what} is too large (archives are limited to {} MiB)",
            MAX_ARCHIVE_BYTES >> 20
        )));
    }
    *budget -= data.len() as u64;
    Ok(data)
}

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
        "cc" | "ccc" | "ccm" => Some(Format::Cecilia),
        "gabc" => Some(Format::Gabc),
        _ => None,
    }
}

/// Reads a score from bytes, detecting the format. Readers of multi-staff instruments give
/// one track each ([`Score::merge_staves`]).
pub fn read_bytes(bytes: &[u8], path: Option<&Path>) -> Result<Score> {
    let format = detect(bytes, path);
    let mut score = read_format(bytes, format)?;
    lyrics_to_text(&mut score, format == Some(Format::GuitarPro3To5));
    Ok(score)
}

/// Gives the tracks' lyrics their text ([`cecilia_core::gabcplus`]), which the syllables
/// on the beats are then laid out from: a Guitar Pro 3–5 file's lyric lines (`lines`), else
/// the syllables the file put on the beats.
pub fn lyrics_to_text(score: &mut Score, lines: bool) {
    use cecilia_core::gabcplus;
    if lines && let Some((t, text)) = gabcplus::text_from_lines(score) {
        score.tracks[t].lyrics_text = text;
    }
    gabcplus::ensure_text(score);
    for t in 0..score.tracks.len() {
        if !score.tracks[t].lyrics_text.is_empty() && score.tracks[t].chant.is_none() {
            gabcplus::apply(score, t);
        }
    }
}

fn read_format(bytes: &[u8], format: Option<Format>) -> Result<Score> {
    match format {
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
        "cc" | "ccc" | "ccm" => native::write_cecilia(score),
        "gabc" => gabc::write_gabc(score),
        other => Err(Error::Unsupported(format!("cannot write .{other} files"))),
    }
}

/// Voices with notes that a file of this extension cannot store, summed over the score: `.gp5`
/// keeps two voices per staff, `.gp` / `.gpx` four. Writing drops them, so callers can warn.
pub fn voices_lost(score: &Score, extension: &str) -> usize {
    let limit = match extension.to_ascii_lowercase().as_str() {
        "gp5" => 2,
        "gp" | "gpx" => 4,
        _ => return 0,
    };
    let split = score.split_grand();
    split
        .tracks
        .iter()
        .flat_map(|t| &t.bars)
        .flat_map(|b| b.voices.iter().skip(limit))
        .filter(|v| v.beats.iter().any(|b| !b.notes.is_empty()))
        .count()
}

/// Verse rows of lyrics that a file of this extension cannot store: `.gp5` keeps five lyric
/// lines. Writing drops the extra rows, so callers can warn.
pub fn lyric_rows_lost(score: &Score, extension: &str) -> usize {
    if !extension.eq_ignore_ascii_case("gp5") {
        return 0;
    }
    cecilia_core::gabcplus::gp_lyrics(score).map_or(0, |(_, g)| g.dropped)
}

/// True when writing a score with audio sync to a file of this extension drops the audio and
/// its sync: `.gabc` keeps only the chant (use `.ccc`), so callers can warn.
pub fn audio_lost(score: &Score, extension: &str) -> bool {
    extension.eq_ignore_ascii_case("gabc") && score.audio.is_some()
}

/// Writes a score to a file in the format given by its extension.
pub fn write_file(score: &Score, path: &Path) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    let bytes = write_bytes(score, ext)?;
    write_atomic(path, &bytes)
}

/// Writes `bytes` to `path` through a sibling temporary file renamed over it, so a failed or
/// interrupted write never leaves a truncated file. A symlink is followed (the file it points
/// to is replaced) and an existing file keeps its permissions.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    // Follow symlinks to the real target; a missing file is created at `path` itself.
    let target = match std::fs::canonicalize(path) {
        Ok(real) => real,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => path.to_path_buf(),
        Err(e) => return Err(e.into()),
    };
    let name = target.file_name().ok_or_else(|| {
        Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path has no file name",
        ))
    })?;
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(format!(".{}.tmp", std::process::id()));
    let tmp = target.with_file_name(tmp_name);
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        if let Ok(meta) = std::fs::metadata(&target) {
            file.set_permissions(meta.permissions())?;
        }
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, &target)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cecilia-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn read_entry_stops_at_the_budget() {
        let mut budget = 10;
        assert_eq!(
            read_entry(&[1u8; 10][..], &mut budget, "x").unwrap().len(),
            10
        );
        assert_eq!(budget, 0);
        let mut budget = 10;
        assert!(read_entry(&[1u8; 11][..], &mut budget, "x").is_err());
    }

    #[test]
    fn atomic_write_replaces_a_file_and_leaves_no_temporary() {
        let dir = scratch("atomic-plain");
        let file = dir.join("a.txt");
        write_atomic(&file, b"one").unwrap();
        write_atomic(&file, b"two").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_replaces_keeps_mode_and_follows_symlinks() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = scratch("atomic");
        let real = dir.join("real.txt");
        std::fs::write(&real, b"old").unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o640)).unwrap();
        let link = dir.join("link.txt");
        symlink(&real, &link).unwrap();
        write_atomic(&link, b"new").unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(std::fs::read(&real).unwrap(), b"new");
        let mode = std::fs::metadata(&real).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
        // No temporary file is left behind, and a new file is created.
        let fresh = dir.join("fresh.txt");
        write_atomic(&fresh, b"x").unwrap();
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(names.len(), 3);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn failed_write_leaves_no_temp_file() {
        let dir = scratch("atomic-fail");
        // The target is a directory: the rename fails and the temp file is removed.
        let target = dir.join("d");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("keep"), b"1").unwrap();
        assert!(write_atomic(&target, b"x").is_err());
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(names.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
