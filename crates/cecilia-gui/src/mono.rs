//! The font of the lyrics text area (`ui.lyrics_font`): the app's own text font (Source Sans 3,
//! the default), or an installed font of the family named, found in the usual font folders by
//! its file name and its name table. Glyphs are drawn as outlines, like all other text.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use ttf_parser::{Face, GlyphId, OutlineBuilder, name_id};
use vello::kurbo::{Affine, BezPath};

use cecilia_engrave::fonts::{BOLD as TEXT_BOLD, TEXT};

/// One face of a font, with its glyph outlines cached.
pub struct MonoFont {
    face: Face<'static>,
    cache: Mutex<HashMap<u16, Option<BezPath>>>,
    /// Faces to take characters this one lacks from (the app's text font).
    fallback: Option<&'static MonoFont>,
}

struct Builder(BezPath);

impl OutlineBuilder for Builder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((x as f64, y as f64));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to((x1 as f64, y1 as f64), (x as f64, y as f64));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.curve_to(
            (x1 as f64, y1 as f64),
            (x2 as f64, y2 as f64),
            (x as f64, y as f64),
        );
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}

impl MonoFont {
    fn new(data: &'static [u8], index: u32, fallback: Option<&'static MonoFont>) -> Option<Self> {
        Some(Self {
            face: Face::parse(data, index).ok()?,
            cache: Mutex::new(HashMap::new()),
            fallback,
        })
    }

    /// The face that draws a character: this one, else the fallback.
    fn with(&self, ch: char) -> (&MonoFont, Option<GlyphId>) {
        match self.face.glyph_index(ch) {
            Some(g) => (self, Some(g)),
            None => match self.fallback {
                Some(f) => (f, f.face.glyph_index(ch)),
                None => (self, None),
            },
        }
    }

    fn scale(&self, size: f64) -> f64 {
        size / self.face.units_per_em() as f64
    }

    /// Advance width of a character at an em size.
    pub fn advance(&self, ch: char, size: f64) -> f64 {
        let (f, g) = self.with(ch);
        let g = g.or_else(|| f.face.glyph_index('?'));
        let adv = g.and_then(|g| f.face.glyph_hor_advance(g)).unwrap_or(0);
        adv as f64 * f.scale(size)
    }

    pub fn text_width(&self, text: &str, size: f64) -> f64 {
        text.chars().map(|c| self.advance(c, size)).sum()
    }

    /// Outlines of a line of text from its baseline's left end at (x, y), y down.
    pub fn text_path(&self, text: &str, x: f64, y: f64, size: f64) -> BezPath {
        let mut out = BezPath::new();
        let mut pen = x;
        for c in text.chars() {
            let (f, g) = self.with(c);
            if let Some(g) = g {
                let unit = {
                    let mut cache = f.cache.lock().unwrap();
                    cache
                        .entry(g.0)
                        .or_insert_with(|| {
                            let mut b = Builder(BezPath::new());
                            f.face.outline_glyph(g, &mut b)?;
                            Some(b.0)
                        })
                        .clone()
                };
                if let Some(unit) = unit {
                    let s = f.scale(size);
                    out.extend(
                        Affine::translate((pen, y)) * Affine::scale_non_uniform(s, -s) * unit,
                    );
                }
            }
            pen += self.advance(c, size);
        }
        out
    }
}

/// A family name as compared: lower case, without spaces, dashes or underscores.
fn key(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The app's text font (Source Sans 3), regular or bold.
fn bundled(bold: bool) -> &'static MonoFont {
    static FONTS: OnceLock<[MonoFont; 2]> = OnceLock::new();
    let f = FONTS.get_or_init(|| {
        [
            MonoFont::new(TEXT, 0, None).expect("bundled text font"),
            MonoFont::new(TEXT_BOLD, 0, None).expect("bundled bold text font"),
        ]
    });
    &f[usize::from(bold)]
}

/// The regular or bold face of a font family: the app's text font (Source Sans 3, also for an
/// empty name), an installed font of that family, or (none found) the app's text font.
pub fn font(family: &str, bold: bool) -> &'static MonoFont {
    static LOADED: OnceLock<Mutex<HashMap<(String, bool), &'static MonoFont>>> = OnceLock::new();
    let k = key(family);
    if k.is_empty() || k == "sourcesans3" || k == "sourcesans" {
        return bundled(bold);
    }
    let loaded = LOADED.get_or_init(Default::default);
    if let Some(f) = loaded.lock().unwrap().get(&(k.clone(), bold)) {
        return f;
    }
    let found = find(&k, bold)
        .and_then(|(index, data)| {
            MonoFont::new(
                Box::leak(data.into_boxed_slice()),
                index,
                Some(bundled(bold)),
            )
        })
        .map(|f| &*Box::leak(Box::new(f)))
        .unwrap_or_else(|| bundled(bold));
    loaded.lock().unwrap().insert((k, bold), found);
    found
}

/// Folders fonts are installed in.
fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let home = cecilia_app::home_dir();
    if let Some(d) = std::env::var_os("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(d).join("fonts"));
    }
    if let Some(h) = &home {
        dirs.push(h.join(".local/share/fonts"));
        dirs.push(h.join(".fonts"));
        dirs.push(h.join("Library/Fonts"));
    }
    for d in [
        "/usr/share/fonts",
        "/usr/local/share/fonts",
        "/Library/Fonts",
        "/System/Library/Fonts",
    ] {
        dirs.push(PathBuf::from(d));
    }
    // The Windows folder is wherever the system says (not always C:).
    let windir = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot"));
    dirs.push(windir.map_or_else(
        || PathBuf::from("C:\\Windows\\Fonts"),
        |w| PathBuf::from(w).join("Fonts"),
    ));
    if let Some(d) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(d).join("Microsoft\\Windows\\Fonts"));
    }
    dirs
}

/// The bytes of an installed font of family `k` ([`key`] form): files whose name starts with the
/// family are read, and the one whose name table gives the family and the weight wins.
fn find(k: &str, bold: bool) -> Option<(u32, Vec<u8>)> {
    let mut files = Vec::new();
    let mut stack = font_dirs();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if is_font(&p) && file_key(&p).starts_with(k) {
                files.push(p);
            }
        }
    }
    files.sort();
    // (distance from the wanted weight, face index in the file, file bytes)
    let mut best: Option<(u32, u32, Vec<u8>)> = None;
    for p in files {
        let Ok(data) = std::fs::read(&p) else {
            continue;
        };
        // A .ttc holds several faces (often regular, bold, italic ...).
        let faces = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
        for index in 0..faces {
            let Ok(face) = Face::parse(&data, index) else {
                continue;
            };
            let family = face
                .names()
                .into_iter()
                .filter(|n| {
                    n.name_id == name_id::TYPOGRAPHIC_FAMILY || n.name_id == name_id::FAMILY
                })
                .filter_map(|n| n.to_string())
                .any(|n| key(&n) == k);
            if !family || face.is_italic() {
                continue;
            }
            let want = if bold { 700 } else { 400 };
            let off = (face.weight().to_number() as i32 - want).unsigned_abs();
            if best.as_ref().is_none_or(|(b, _, _)| off < *b) {
                best = Some((off, index, data.clone()));
            }
        }
    }
    best.map(|(_, i, d)| (i, d))
}

fn is_font(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc"))
}

fn file_key(p: &Path) -> String {
    key(p.file_stem().and_then(|s| s.to_str()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_font_is_the_default_and_unknown_families_fall_back() {
        assert!(std::ptr::eq(
            font("No Such Family 123", false),
            bundled(false)
        ));
        assert!(std::ptr::eq(font("", false), bundled(false)));
        assert!(std::ptr::eq(font("Source Sans 3", true), bundled(true)));
        let t = font("Source Sans 3", false);
        assert!(t.text_width("abc", 14.0) > 0.0);
        assert!(!t.text_path("a", 0.0, 0.0, 14.0).elements().is_empty());
    }
}
