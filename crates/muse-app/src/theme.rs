//! Interface colours: a preset (`[theme] preset = "dark"` or `"light"`) with any colour
//! overridden in `config.toml`, and the palette tracks are coloured from.

use serde::Deserialize;

/// A colour with alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// Parses `#rgb`, `#rrggbb` or `#rrggbbaa` (the `#` is optional).
    pub fn parse(s: &str) -> Option<Self> {
        let h = s.trim().trim_start_matches('#');
        let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
        let nibble = |i: usize| {
            u8::from_str_radix(h.get(i..i + 1)?, 16)
                .ok()
                .map(|v| v * 17)
        };
        match h.len() {
            3 => Some(Self::rgb(nibble(0)?, nibble(1)?, nibble(2)?)),
            6 => Some(Self::rgb(byte(0)?, byte(2)?, byte(4)?)),
            8 => Some(Self {
                a: byte(6)?,
                ..Self::rgb(byte(0)?, byte(2)?, byte(4)?)
            }),
            _ => None,
        }
    }

    /// `#rrggbb` (with `aa` when not opaque).
    pub fn hex(self) -> String {
        let mut s = format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b);
        if self.a != 255 {
            s.push_str(&format!("{:02x}", self.a));
        }
        s
    }

    /// Mixes towards `other` by `t` (0 = self, 1 = other).
    pub fn mix(self, other: Rgba, t: f32) -> Rgba {
        let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgba {
            r: m(self.r, other.r),
            g: m(self.g, other.g),
            b: m(self.b, other.b),
            a: m(self.a, other.a),
        }
    }

    fn to_hsl(self) -> (f32, f32, f32) {
        let (r, g, b) = (
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
        );
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;
        if max == min {
            return (0.0, 0.0, l);
        }
        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };
        let h = if max == r {
            (g - b) / d + if g < b { 6.0 } else { 0.0 }
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        (h * 60.0, s, l)
    }

    fn from_hsl(h: f32, s: f32, l: f32) -> Rgba {
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let hp = h.rem_euclid(360.0) / 60.0;
        let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
        let (r, g, b) = match hp as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = l - c / 2.0;
        let v = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
        Rgba::rgb(v(r), v(g), v(b))
    }
}

/// The interface colours.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Around the engraved pages.
    pub paper: Rgba,
    /// The engraved pages themselves.
    pub page: Rgba,
    /// The engraved notation.
    pub ink: Rgba,
    /// Tabline, status line.
    pub chrome: Rgba,
    pub chrome_text: Rgba,
    /// Secondary text.
    pub dim: Rgba,
    /// Highlights: the active tab, focus borders.
    pub accent: Rgba,
    /// The playhead (red bar).
    pub playhead: Rgba,
    pub selection: Rgba,
    /// Buffer tabs: normal and hovered (the active tab uses `paper`).
    pub tab: Rgba,
    pub tab_hover: Rgba,
    /// The toolbar, and a button under the mouse or switched on.
    pub toolbar: Rgba,
    pub button_hover: Rgba,
    /// The tracks panel and waveform bar.
    pub panel: Rgba,
    /// A bar without notes in the tracks panel.
    pub empty_bar: Rgba,
    /// The command palette, pickers, which-key and tooltips.
    pub popover: Rgba,
    /// Error messages.
    pub error: Rgba,
    /// Terminal score views: lyrics, and bar/section headings and beat text.
    pub lyrics: Rgba,
    pub marker: Rgba,
    /// Waveforms, the loop region on them, and sync points.
    pub waveform: Rgba,
    pub loop_region: Rgba,
    pub sync_point: Rgba,
    pub mode_normal: Rgba,
    pub mode_insert: Rgba,
    pub mode_visual: Rgba,
    pub mode_command: Rgba,
    pub mode_picker: Rgba,
    pub mode_tracks: Rgba,
    /// File-type icons in the tabline.
    pub icon_guitar_pro: Rgba,
    pub icon_musicxml: Rgba,
    pub icon_midi: Rgba,
    pub icon_chant: Rgba,
    pub icon_muse: Rgba,
    /// Colours given to tracks in order (a track can have its own with `:track-color`).
    pub track_colors: Vec<Rgba>,
}

/// The built-in themes.
pub const PRESETS: &[&str] = &[
    "dark",
    "light",
    "nord",
    "gruvbox",
    "solarized-dark",
    "solarized-light",
];

/// The colours a theme sets, by their names in config.toml.
pub const COLOR_NAMES: &[&str] = &[
    "paper",
    "page",
    "ink",
    "chrome",
    "chrome_text",
    "dim",
    "accent",
    "playhead",
    "selection",
    "tab",
    "tab_hover",
    "toolbar",
    "button_hover",
    "panel",
    "empty_bar",
    "popover",
    "error",
    "lyrics",
    "marker",
    "waveform",
    "loop_region",
    "sync_point",
    "mode_normal",
    "mode_insert",
    "mode_visual",
    "mode_command",
    "mode_picker",
    "mode_tracks",
    "icon_guitar_pro",
    "icon_musicxml",
    "icon_midi",
    "icon_chant",
    "icon_muse",
];

fn hex(s: &str) -> Rgba {
    Rgba::parse(s).expect("valid built-in colour")
}

impl Theme {
    pub fn dark() -> Self {
        let accent = Rgba::rgb(250, 190, 40);
        Self {
            paper: Rgba::rgb(232, 233, 236),
            page: Rgba::rgb(255, 255, 255),
            ink: Rgba::rgb(0, 0, 0),
            chrome: Rgba::rgb(40, 42, 48),
            chrome_text: Rgba::rgb(225, 225, 230),
            dim: Rgba::rgb(150, 152, 160),
            accent,
            playhead: Rgba::rgb(225, 40, 40),
            selection: Rgba::rgb(150, 90, 220),
            tab: Rgba::rgb(52, 55, 63),
            tab_hover: Rgba::rgb(72, 75, 86),
            toolbar: Rgba::rgb(46, 48, 55),
            button_hover: Rgba::rgb(72, 75, 86),
            panel: Rgba::rgb(30, 32, 37),
            empty_bar: Rgba::rgb(55, 58, 66),
            popover: Rgba::rgb(36, 38, 44),
            error: Rgba::rgb(255, 110, 100),
            lyrics: Rgba::rgb(110, 200, 130),
            marker: Rgba::rgb(90, 190, 210),
            waveform: Rgba::rgb(110, 170, 230),
            loop_region: Rgba::rgb(60, 190, 110).with_alpha(50),
            sync_point: accent,
            mode_normal: Rgba::rgb(90, 150, 230),
            mode_insert: Rgba::rgb(240, 180, 60),
            mode_visual: Rgba::rgb(190, 120, 220),
            mode_command: Rgba::rgb(100, 190, 120),
            mode_picker: Rgba::rgb(80, 190, 200),
            mode_tracks: Rgba::rgb(230, 120, 80),
            icon_guitar_pro: Rgba::rgb(240, 140, 50),
            icon_musicxml: Rgba::rgb(80, 150, 230),
            icon_midi: Rgba::rgb(90, 190, 110),
            icon_chant: Rgba::rgb(180, 110, 210),
            icon_muse: Rgba::rgb(60, 190, 190),
            track_colors: spectrum(accent, 8, 0.62),
        }
    }

    pub fn light() -> Self {
        let accent = Rgba::rgb(214, 140, 0);
        Self {
            paper: Rgba::rgb(250, 250, 251),
            chrome: Rgba::rgb(236, 237, 241),
            chrome_text: Rgba::rgb(36, 38, 44),
            dim: Rgba::rgb(110, 114, 124),
            accent,
            playhead: Rgba::rgb(215, 30, 30),
            selection: Rgba::rgb(140, 80, 210),
            tab: Rgba::rgb(220, 221, 227),
            tab_hover: Rgba::rgb(205, 207, 214),
            toolbar: Rgba::rgb(244, 244, 247),
            button_hover: Rgba::rgb(218, 219, 225),
            panel: Rgba::rgb(246, 246, 248),
            empty_bar: Rgba::rgb(222, 223, 228),
            popover: Rgba::rgb(252, 252, 253),
            error: Rgba::rgb(200, 40, 30),
            lyrics: Rgba::rgb(40, 140, 80),
            marker: Rgba::rgb(30, 130, 150),
            waveform: Rgba::rgb(40, 110, 200),
            loop_region: Rgba::rgb(40, 160, 90).with_alpha(50),
            sync_point: accent,
            mode_normal: Rgba::rgb(60, 120, 210),
            mode_insert: Rgba::rgb(220, 150, 30),
            mode_visual: Rgba::rgb(160, 90, 200),
            mode_command: Rgba::rgb(60, 160, 90),
            mode_picker: Rgba::rgb(40, 160, 175),
            mode_tracks: Rgba::rgb(210, 100, 60),
            track_colors: spectrum(accent, 8, 0.48),
            ..Self::dark()
        }
    }

    pub fn nord() -> Self {
        let accent = hex("#88c0d0");
        Self {
            paper: hex("#d8dee9"),
            page: hex("#f2f4f8"),
            ink: hex("#2e3440"),
            chrome: hex("#2e3440"),
            chrome_text: hex("#eceff4"),
            dim: hex("#9aa5b8"),
            accent,
            playhead: hex("#bf616a"),
            selection: hex("#b48ead"),
            tab: hex("#3b4252"),
            tab_hover: hex("#434c5e"),
            toolbar: hex("#323946"),
            button_hover: hex("#434c5e"),
            panel: hex("#272c36"),
            empty_bar: hex("#3b4252"),
            popover: hex("#3b4252"),
            error: hex("#bf616a"),
            lyrics: hex("#a3be8c"),
            marker: hex("#88c0d0"),
            waveform: hex("#81a1c1"),
            loop_region: hex("#a3be8c").with_alpha(50),
            sync_point: hex("#ebcb8b"),
            mode_normal: hex("#81a1c1"),
            mode_insert: hex("#ebcb8b"),
            mode_visual: hex("#b48ead"),
            mode_command: hex("#a3be8c"),
            mode_picker: hex("#88c0d0"),
            mode_tracks: hex("#d08770"),
            icon_guitar_pro: hex("#d08770"),
            icon_musicxml: hex("#81a1c1"),
            icon_midi: hex("#a3be8c"),
            icon_chant: hex("#b48ead"),
            icon_muse: hex("#8fbcbb"),
            track_colors: spectrum(accent, 8, 0.65),
        }
    }

    pub fn gruvbox() -> Self {
        let accent = hex("#fabd2f");
        Self {
            paper: hex("#ebdbb2"),
            page: hex("#fbf1c7"),
            ink: hex("#282828"),
            chrome: hex("#282828"),
            chrome_text: hex("#ebdbb2"),
            dim: hex("#a89984"),
            accent,
            playhead: hex("#fb4934"),
            selection: hex("#d3869b"),
            tab: hex("#3c3836"),
            tab_hover: hex("#504945"),
            toolbar: hex("#32302f"),
            button_hover: hex("#504945"),
            panel: hex("#1d2021"),
            empty_bar: hex("#3c3836"),
            popover: hex("#3c3836"),
            error: hex("#fb4934"),
            lyrics: hex("#b8bb26"),
            marker: hex("#8ec07c"),
            waveform: hex("#83a598"),
            loop_region: hex("#b8bb26").with_alpha(50),
            sync_point: hex("#fabd2f"),
            mode_normal: hex("#83a598"),
            mode_insert: hex("#fabd2f"),
            mode_visual: hex("#d3869b"),
            mode_command: hex("#b8bb26"),
            mode_picker: hex("#8ec07c"),
            mode_tracks: hex("#fe8019"),
            icon_guitar_pro: hex("#fe8019"),
            icon_musicxml: hex("#83a598"),
            icon_midi: hex("#b8bb26"),
            icon_chant: hex("#d3869b"),
            icon_muse: hex("#8ec07c"),
            track_colors: spectrum(accent, 8, 0.6),
        }
    }

    pub fn solarized_dark() -> Self {
        let accent = hex("#b58900");
        Self {
            paper: hex("#eee8d5"),
            page: hex("#fdf6e3"),
            ink: hex("#002b36"),
            chrome: hex("#002b36"),
            chrome_text: hex("#93a1a1"),
            dim: hex("#657b83"),
            accent,
            playhead: hex("#dc322f"),
            selection: hex("#6c71c4"),
            tab: hex("#073642"),
            tab_hover: hex("#0b4553"),
            toolbar: hex("#03303b"),
            button_hover: hex("#0b4553"),
            panel: hex("#00212b"),
            empty_bar: hex("#073642"),
            popover: hex("#073642"),
            error: hex("#dc322f"),
            lyrics: hex("#859900"),
            marker: hex("#2aa198"),
            waveform: hex("#268bd2"),
            loop_region: hex("#859900").with_alpha(50),
            sync_point: hex("#b58900"),
            mode_normal: hex("#268bd2"),
            mode_insert: hex("#b58900"),
            mode_visual: hex("#d33682"),
            mode_command: hex("#859900"),
            mode_picker: hex("#2aa198"),
            mode_tracks: hex("#cb4b16"),
            icon_guitar_pro: hex("#cb4b16"),
            icon_musicxml: hex("#268bd2"),
            icon_midi: hex("#859900"),
            icon_chant: hex("#6c71c4"),
            icon_muse: hex("#2aa198"),
            track_colors: spectrum(accent, 8, 0.55),
        }
    }

    pub fn solarized_light() -> Self {
        Self {
            chrome: hex("#eee8d5"),
            chrome_text: hex("#586e75"),
            dim: hex("#93a1a1"),
            tab: hex("#e4ddc8"),
            tab_hover: hex("#d9d2bd"),
            toolbar: hex("#f5efdc"),
            button_hover: hex("#e4ddc8"),
            panel: hex("#f5efdc"),
            empty_bar: hex("#e4ddc8"),
            popover: hex("#fdf6e3"),
            track_colors: spectrum(hex("#b58900"), 8, 0.45),
            ..Self::solarized_dark()
        }
    }

    /// A built-in theme by name.
    pub fn preset(name: &str) -> Option<Self> {
        Some(match name {
            "dark" => Self::dark(),
            "light" => Self::light(),
            "nord" => Self::nord(),
            "gruvbox" => Self::gruvbox(),
            "solarized-dark" => Self::solarized_dark(),
            "solarized-light" => Self::solarized_light(),
            _ => return None,
        })
    }

    /// A colour by its config name (see [`COLOR_NAMES`]).
    pub fn color_mut(&mut self, name: &str) -> Option<&mut Rgba> {
        Some(match name {
            "paper" => &mut self.paper,
            "page" => &mut self.page,
            "ink" => &mut self.ink,
            "chrome" => &mut self.chrome,
            "chrome_text" => &mut self.chrome_text,
            "dim" => &mut self.dim,
            "accent" => &mut self.accent,
            "playhead" => &mut self.playhead,
            "selection" => &mut self.selection,
            "tab" => &mut self.tab,
            "tab_hover" => &mut self.tab_hover,
            "toolbar" => &mut self.toolbar,
            "button_hover" => &mut self.button_hover,
            "panel" => &mut self.panel,
            "empty_bar" => &mut self.empty_bar,
            "popover" => &mut self.popover,
            "error" => &mut self.error,
            "lyrics" => &mut self.lyrics,
            "marker" => &mut self.marker,
            "waveform" => &mut self.waveform,
            "loop_region" => &mut self.loop_region,
            "sync_point" => &mut self.sync_point,
            "mode_normal" => &mut self.mode_normal,
            "mode_insert" => &mut self.mode_insert,
            "mode_visual" => &mut self.mode_visual,
            "mode_command" => &mut self.mode_command,
            "mode_picker" => &mut self.mode_picker,
            "mode_tracks" => &mut self.mode_tracks,
            "icon_guitar_pro" => &mut self.icon_guitar_pro,
            "icon_musicxml" => &mut self.icon_musicxml,
            "icon_midi" => &mut self.icon_midi,
            "icon_chant" => &mut self.icon_chant,
            "icon_muse" => &mut self.icon_muse,
            _ => return None,
        })
    }

    /// The colour of a mode: its status-line label, and the edit cursor while in it.
    pub fn mode_color(&self, mode: crate::config::Mode) -> Rgba {
        use crate::config::Mode;
        match mode {
            Mode::Normal => self.mode_normal,
            Mode::Insert => self.mode_insert,
            Mode::Visual => self.mode_visual,
            Mode::Command => self.mode_command,
            Mode::Picker => self.mode_picker,
            Mode::Tracks => self.mode_tracks,
        }
    }

    /// The colour of track `i` without a colour of its own.
    pub fn track_color(&self, i: usize) -> Rgba {
        match self.track_colors.len() {
            0 => self.accent,
            n => self.track_colors[i % n],
        }
    }

    /// The colour of a file-type icon (`kind` from [`crate::file_kind`]).
    pub fn file_icon(&self, kind: &str) -> Rgba {
        match kind {
            "guitar-pro" => self.icon_guitar_pro,
            "musicxml" => self.icon_musicxml,
            "midi" => self.icon_midi,
            "chant" => self.icon_chant,
            "muse" => self.icon_muse,
            _ => self.dim,
        }
    }

    /// Text that reads on top of `bg`.
    pub fn text_on(&self, bg: Rgba) -> Rgba {
        let luma = 0.299 * bg.r as f32 + 0.587 * bg.g as f32 + 0.114 * bg.b as f32;
        if luma > 150.0 {
            Rgba::rgb(20, 20, 24)
        } else {
            Rgba::rgb(245, 245, 248)
        }
    }

    /// The theme a config asks for, and anything in it that couldn't be used. A theme file
    /// (`file`) is read first; the preset and colours given in `[theme]` itself win over it.
    pub fn from_config(c: &ThemeConfig) -> (Self, Vec<String>) {
        let mut errors = Vec::new();
        let from_file = match &c.file {
            Some(path) => {
                let path = theme_path(path);
                match std::fs::read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|t| toml::from_str::<ThemeConfig>(&t).map_err(|e| e.to_string()))
                {
                    Ok(f) => Some(f),
                    Err(e) => {
                        errors.push(format!("theme.file: {}: {e}", path.display()));
                        None
                    }
                }
            }
            None => None,
        };
        let preset = c
            .preset
            .as_deref()
            .or(from_file.as_ref().and_then(|f| f.preset.as_deref()))
            .unwrap_or("dark");
        let mut t = Self::preset(preset).unwrap_or_else(|| {
            errors.push(format!(
                "theme.preset: unknown preset '{preset}' ({})",
                PRESETS.join(", ")
            ));
            Self::dark()
        });
        let accent_before = t.accent;
        let layers = from_file.iter().chain([c]);
        let mut track_colors = None;
        for layer in layers {
            for (name, value) in &layer.colors {
                let Some(slot) = t.color_mut(name) else {
                    errors.push(format!("theme.{name}: not a theme colour"));
                    continue;
                };
                match Rgba::parse(value) {
                    Some(color) => *slot = color,
                    None => {
                        errors.push(format!("theme.{name}: '{value}' is not a colour (#rrggbb)"))
                    }
                }
            }
            if layer.track_colors.is_some() {
                track_colors = layer.track_colors.clone();
            }
        }
        match track_colors {
            Some(list) => {
                let colors: Vec<Rgba> = list
                    .iter()
                    .filter_map(|v| {
                        let c = Rgba::parse(v);
                        if c.is_none() {
                            errors.push(format!("theme.track_colors: '{v}' is not a colour"));
                        }
                        c
                    })
                    .collect();
                if !colors.is_empty() {
                    t.track_colors = colors;
                }
            }
            // A new accent shifts the default spectrum with it.
            None if t.accent != accent_before => {
                let (_, _, l) = t.track_colors[0].to_hsl();
                t.track_colors = spectrum(t.accent, 8, l);
            }
            None => {}
        }
        (t, errors)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

/// Where a theme file is: `~/…`, absolute, or relative to muse's config folder.
pub fn theme_path(path: &std::path::Path) -> std::path::PathBuf {
    if let Ok(rest) = path.strip_prefix("~") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    if path.is_absolute() {
        return path.to_path_buf();
    }
    directories::ProjectDirs::from("", "", "muse")
        .map(|d| d.config_dir().join(path))
        .unwrap_or_else(|| path.to_path_buf())
}

/// `n` colours around the colour wheel starting at `from`'s hue, at lightness `l`.
pub fn spectrum(from: Rgba, n: usize, l: f32) -> Vec<Rgba> {
    let (h, s, _) = from.to_hsl();
    let s = s.clamp(0.55, 0.85);
    (0..n)
        .map(|i| Rgba::from_hsl(h + i as f32 * 360.0 / n as f32, s, l))
        .collect()
}

/// `[theme]` in config.toml (or a theme file): a preset, colours overriding it by name
/// (`chrome = "#282a30"`, see [`COLOR_NAMES`]) and the track palette.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub preset: Option<String>,
    /// A theme file with the same keys, applied under the ones here.
    pub file: Option<std::path::PathBuf>,
    pub track_colors: Option<Vec<String>>,
    #[serde(flatten)]
    pub colors: std::collections::BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_parse_and_presets_take_overrides() {
        assert_eq!(Rgba::parse("#fa0"), Some(Rgba::rgb(255, 170, 0)));
        assert_eq!(Rgba::parse("102030"), Some(Rgba::rgb(16, 32, 48)));
        assert_eq!(Rgba::parse("#10203080").unwrap().a, 128);
        assert_eq!(Rgba::parse("#12"), None);
        assert_eq!(Rgba::rgb(16, 32, 48).hex(), "#102030");
        let c: ThemeConfig =
            toml::from_str("preset = \"light\"\nplayhead = \"#00ff00\"\ndim = \"nope\"").unwrap();
        let (t, errors) = Theme::from_config(&c);
        assert_eq!(t.playhead, Rgba::rgb(0, 255, 0));
        assert_eq!(t.chrome, Theme::light().chrome);
        assert_eq!(errors.len(), 1);
        // Tracks cycle through the palette; it follows the accent unless given.
        assert_eq!(t.track_color(8), t.track_color(0));
        let c: ThemeConfig = toml::from_str("accent = \"#3080ff\"").unwrap();
        let (t, _) = Theme::from_config(&c);
        assert_ne!(t.track_colors, Theme::dark().track_colors);
        let c: ThemeConfig = toml::from_str("track_colors = [\"#ff0000\", \"#00ff00\"]").unwrap();
        let (t, _) = Theme::from_config(&c);
        assert_eq!(t.track_color(3), Rgba::rgb(0, 255, 0));
        // Unknown names are reported.
        let c: ThemeConfig = toml::from_str("chrom = \"#000000\"").unwrap();
        assert_eq!(Theme::from_config(&c).1.len(), 1);
    }

    #[test]
    fn every_preset_and_colour_name_works() {
        for p in PRESETS {
            let mut t = Theme::preset(p).unwrap();
            for name in COLOR_NAMES {
                assert!(t.color_mut(name).is_some(), "{name}");
            }
        }
        assert!(Theme::preset("nope").is_none());
    }

    #[test]
    fn a_theme_file_sits_under_the_config() {
        let dir = std::env::temp_dir().join(format!("muse-theme-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("mine.toml");
        std::fs::write(
            &file,
            "preset = \"nord\"\naccent = \"#ff0000\"\nchrome = \"#010203\"\n",
        )
        .unwrap();
        let c: ThemeConfig = toml::from_str(&format!(
            "file = \"{}\"\nchrome = \"#0a0b0c\"\n",
            file.display()
        ))
        .unwrap();
        let (t, errors) = Theme::from_config(&c);
        assert!(errors.is_empty(), "{errors:?}");
        // The file's preset and colours, then the config's own on top.
        assert_eq!(t.paper, Theme::nord().paper);
        assert_eq!(t.accent, Rgba::rgb(255, 0, 0));
        assert_eq!(t.chrome, Rgba::rgb(10, 11, 12));
        let c: ThemeConfig = toml::from_str("file = \"/nonexistent/x.toml\"").unwrap();
        assert_eq!(Theme::from_config(&c).1.len(), 1);
        std::fs::remove_dir_all(dir).ok();
    }
}
