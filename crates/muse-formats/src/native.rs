//! The native `.muse` format: a zip archive holding a versioned manifest and the full score
//! model as JSON, so nothing is lost when saving.

use std::io::{Read, Write};

use muse_core::Score;

use crate::{Error, Result};

/// Version of the score model stored in `score.json`; bump when its schema changes and add a
/// migration in [`read_muse`].
pub const FORMAT_VERSION: u32 = 1;

fn err(message: impl Into<String>) -> Error {
    Error::Parse {
        offset: 0,
        location: Some(".muse".into()),
        message: message.into(),
    }
}

pub fn write_muse(score: &Score) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(score).map_err(|e| err(e.to_string()))?;
    let manifest = format!(
        "format = \"muse\"\nformat_version = {FORMAT_VERSION}\napp_version = \"{}\"\n",
        env!("CARGO_PKG_VERSION")
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let zerr = |e: zip::result::ZipError| err(e.to_string());
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("manifest.toml", options).map_err(zerr)?;
    zip.write_all(manifest.as_bytes())?;
    zip.start_file("score.json", options).map_err(zerr)?;
    zip.write_all(&json)?;
    if let Some(audio) = &score.audio {
        if let Some(data) = &audio.embedded {
            let name = audio.file.as_deref().unwrap_or("audio");
            let base = name.rsplit('/').next().unwrap_or(name);
            let stored = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file(format!("audio/{base}"), stored)
                .map_err(zerr)?;
            zip.write_all(data)?;
        }
    }
    Ok(zip.finish().map_err(zerr)?.into_inner())
}

pub fn read_muse(bytes: &[u8]) -> Result<Score> {
    let mut zip =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| err(e.to_string()))?;
    let mut manifest = String::new();
    zip.by_name("manifest.toml")
        .map_err(|e| err(e.to_string()))?
        .read_to_string(&mut manifest)?;
    let version = manifest
        .lines()
        .find_map(|l| l.trim().strip_prefix("format_version"))
        .and_then(|v| v.trim().trim_start_matches('=').trim().parse::<u32>().ok())
        .ok_or_else(|| err("manifest has no format_version"))?;
    if version > FORMAT_VERSION {
        return Err(err(format!(
            "file format version {version} is newer than this muse (supports {FORMAT_VERSION})"
        )));
    }
    let mut json = Vec::new();
    zip.by_name("score.json")
        .map_err(|e| err(e.to_string()))?
        .read_to_end(&mut json)?;
    let mut score: Score = serde_json::from_slice(&json).map_err(|e| err(e.to_string()))?;
    if let Some(audio) = score.audio.as_mut() {
        let base = audio
            .file
            .as_deref()
            .map(|f| f.rsplit('/').next().unwrap_or(f).to_string());
        if let Some(base) = base {
            if let Ok(mut entry) = zip.by_name(&format!("audio/{base}")) {
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                audio.embedded = Some(std::sync::Arc::new(data));
            }
        }
    }
    Ok(score)
}

/// Whether bytes look like a `.muse` archive (a zip with a muse manifest).
pub fn is_muse(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK")
        && zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .is_ok_and(|z| z.index_for_name("manifest.toml").is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use muse_core::*;

    #[test]
    fn round_trips_exactly() {
        let score = Score {
            tempo: 133,
            meta: Metadata {
                title: "Ünïcödé".into(),
                ..Metadata::default()
            },
            master_bars: vec![MasterBar {
                tempo_changes: vec![TempoChange {
                    tick: 480,
                    bpm: 99.5,
                }],
                ..MasterBar::default()
            }],
            tracks: vec![Track {
                name: "t".into(),
                bars: vec![Bar::default()],
                ..Track::default()
            }],
            ..Score::default()
        };
        let bytes = write_muse(&score).unwrap();
        assert!(is_muse(&bytes));
        assert_eq!(read_muse(&bytes).unwrap(), score);
    }

    #[test]
    fn rejects_newer_versions() {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("manifest.toml", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"format_version = 999\n").unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        assert!(read_muse(&bytes).is_err());
    }
}
