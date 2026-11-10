//! Interface colors: a preset (`[theme] preset = "dark"`, a `themes/*.toml` file) with any color
//! overridden in `config.toml`, and the palette tracks are colored from.

use serde::{Deserialize, Serialize};

/// A color with alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

    /// `#RRGGBB` (with `AA` when not opaque).
    pub fn hex(self) -> String {
        let mut s = format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b);
        if self.a != 255 {
            s.push_str(&format!("{:02X}", self.a));
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

/// The interface colors. `Default` is blank: a theme is built from its layers (`from_config`).
#[derive(Debug, Clone, PartialEq, Default)]
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
    /// The status line's second sections (layout, progress), as lualine's section b.
    pub status_b: Rgba,
    /// The toolbar, and a button under the mouse or switched on.
    pub toolbar: Rgba,
    pub button_hover: Rgba,
    /// A capo on the fretboard.
    pub capo: Rgba,
    /// The tracks panel and waveform bar.
    pub panel: Rgba,
    /// A bar without notes in the tracks panel.
    pub empty_bar: Rgba,
    /// Staff lines of a bar that doesn't hold its full count (`ui.highlight_incomplete_bars`),
    /// drawn on the page: red enough to read on every preset's light page.
    pub incomplete_bar: Rgba,
    /// The command palette, pickers, which-key and tooltips.
    pub popover: Rgba,
    /// Error messages.
    pub error: Rgba,
    /// Terminal score views: lyrics, and bar/section headings and beat text.
    pub lyrics: Rgba,
    pub marker: Rgba,
    /// The lyrics text area: the words, and with GABC shown the words dimmed, the GABC
    /// groups (bold), the `[labels]` (bold) and the `(bar:beat)` anchors.
    pub lyrics_text: Rgba,
    pub lyrics_text_dim: Rgba,
    pub lyrics_gabc: Rgba,
    pub lyrics_label: Rgba,
    pub lyrics_anchor: Rgba,
    /// GABC in the text area: clefs (purple), divisions (red), notes (blue), shape modifiers
    /// (yellow), attachment modifiers (cyan), space modifiers (gray) and markup tags (green).
    pub lyrics_clef: Rgba,
    pub lyrics_division: Rgba,
    pub lyrics_note: Rgba,
    pub lyrics_shape: Rgba,
    pub lyrics_attach: Rgba,
    pub lyrics_space: Rgba,
    pub lyrics_markup: Rgba,
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
    pub icon_cecilia: Rgba,
    /// The status line's `MODERN` chip (a chant score's `CHANT` chip uses `icon_chant`).
    pub score_modern: Rgba,
    /// The outline of the edit cursor; `None` contrasts with the page (see [`Theme::cursor_border`]).
    pub cursor_border: Option<Rgba>,
    /// Colors given to tracks in order (a track can have its own with `:track-color`).
    pub track_colors: Vec<Rgba>,
    /// Colors of the voices in order: the selected voice in the editing palette's voice buttons.
    pub voice_colors: Vec<Rgba>,
}

impl Theme {
    /// The color of a piece of the lyrics text area's text, and whether it is bold.
    pub fn lyrics_color(&self, k: cecilia_core::gabcplus::Hl) -> (Rgba, bool) {
        use cecilia_core::gabcplus::Hl::*;
        match k {
            Text | Space => (self.lyrics_text, false),
            Paren => (self.lyrics_text_dim, false),
            Clef => (self.lyrics_clef, false),
            Division => (self.lyrics_division, false),
            Note => (self.lyrics_note, false),
            Shape => (self.lyrics_shape, false),
            Attach => (self.lyrics_attach, false),
            Gap => (self.lyrics_space, false),
            Markup => (self.lyrics_markup, false),
            Other => (self.lyrics_gabc, false),
            Label => (self.lyrics_label, true),
            Anchor => (self.lyrics_anchor, false),
        }
    }
}

/// The built-in themes: `themes/<name>.toml` in the source tree, in the format of `[theme]`
/// in config.toml. `dark` holds every color; the others start from the theme their `preset`
/// names (`dark` when absent) and could set only what differs.
const BUILTIN: &[(&str, &str)] = &[
    ("dark", include_str!("../themes/dark.toml")),
    ("light", include_str!("../themes/light.toml")),
    ("cecilia-dark", include_str!("../themes/cecilia-dark.toml")),
    (
        "cecilia-light",
        include_str!("../themes/cecilia-light.toml"),
    ),
    ("cecilia", include_str!("../themes/cecilia.toml")),
    ("nord", include_str!("../themes/nord.toml")),
    ("nord-dark", include_str!("../themes/nord-dark.toml")),
    ("nord-light", include_str!("../themes/nord-light.toml")),
    ("gruvbox", include_str!("../themes/gruvbox.toml")),
    (
        "solarized-dark",
        include_str!("../themes/solarized-dark.toml"),
    ),
    (
        "solarized-light",
        include_str!("../themes/solarized-light.toml"),
    ),
    (
        "catppuccin-latte",
        include_str!("../themes/catppuccin-latte.toml"),
    ),
    (
        "catppuccin-frappe",
        include_str!("../themes/catppuccin-frappe.toml"),
    ),
    (
        "catppuccin-macchiato",
        include_str!("../themes/catppuccin-macchiato.toml"),
    ),
    (
        "catppuccin-mocha",
        include_str!("../themes/catppuccin-mocha.toml"),
    ),
];

/// The folder of the user's own themes, `<name>.toml`: they can start from any theme and
/// shadow a built-in one of the same name.
pub fn user_themes_dir() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("", "", "cecilia").map(|d| d.config_dir().join("themes"))
}

/// The names `theme.preset` takes: the built-in themes, then the user's.
pub fn preset_names() -> Vec<String> {
    let mut names: Vec<String> = BUILTIN.iter().map(|(n, _)| n.to_string()).collect();
    let user = user_themes_dir().and_then(|d| std::fs::read_dir(d).ok());
    let mut mine: Vec<String> = user
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let path = e.ok()?.path();
            (path.extension()? == "toml").then(|| path.file_stem()?.to_str().map(String::from))?
        })
        .filter(|n| !names.contains(n))
        .collect();
    mine.sort();
    names.extend(mine);
    names
}

/// The layers a theme is made of, base first: what its `preset` names, and so on up to `dark`.
fn preset_layers(name: &str, dir: Option<&std::path::Path>) -> Result<Vec<ThemeConfig>, String> {
    let mut layers = Vec::new();
    let mut name = name.to_string();
    loop {
        if layers.len() > 8 || name.contains(['/', '\\']) || name.starts_with('.') {
            return Err(format!(
                "theme '{name}' is not a theme (or its presets loop)"
            ));
        }
        let user = dir.map(|d| d.join(format!("{name}.toml")));
        let text = match user.as_ref().map(std::fs::read_to_string) {
            Some(Ok(text)) => text,
            _ => match BUILTIN.iter().find(|(n, _)| *n == name) {
                Some((_, text)) => text.to_string(),
                None => return Err(format!("unknown theme '{name}'")),
            },
        };
        let layer: ThemeConfig =
            toml::from_str(&text).map_err(|e| format!("theme '{name}': {e}"))?;
        let parent = layer.preset.clone();
        layers.push(layer);
        if parent.is_empty() || parent == name {
            break;
        }
        name = parent;
    }
    layers.reverse();
    Ok(layers)
}

/// The colors a theme sets, by their names in config.toml.
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
    "status_b",
    "toolbar",
    "button_hover",
    "capo",
    "panel",
    "empty_bar",
    "incomplete_bar",
    "popover",
    "error",
    "lyrics",
    "marker",
    "lyrics_text",
    "lyrics_text_dim",
    "lyrics_gabc",
    "lyrics_label",
    "lyrics_anchor",
    "lyrics_clef",
    "lyrics_division",
    "lyrics_note",
    "lyrics_shape",
    "lyrics_attach",
    "lyrics_space",
    "lyrics_markup",
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
    "icon_cecilia",
    "score_modern",
];

impl Theme {
    /// A color by its config name (see [`COLOR_NAMES`]).
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
            "status_b" => &mut self.status_b,
            "toolbar" => &mut self.toolbar,
            "button_hover" => &mut self.button_hover,
            "capo" => &mut self.capo,
            "panel" => &mut self.panel,
            "empty_bar" => &mut self.empty_bar,
            "incomplete_bar" => &mut self.incomplete_bar,
            "popover" => &mut self.popover,
            "error" => &mut self.error,
            "lyrics" => &mut self.lyrics,
            "marker" => &mut self.marker,
            "lyrics_text" => &mut self.lyrics_text,
            "lyrics_text_dim" => &mut self.lyrics_text_dim,
            "lyrics_gabc" => &mut self.lyrics_gabc,
            "lyrics_label" => &mut self.lyrics_label,
            "lyrics_anchor" => &mut self.lyrics_anchor,
            "lyrics_clef" => &mut self.lyrics_clef,
            "lyrics_division" => &mut self.lyrics_division,
            "lyrics_note" => &mut self.lyrics_note,
            "lyrics_shape" => &mut self.lyrics_shape,
            "lyrics_attach" => &mut self.lyrics_attach,
            "lyrics_space" => &mut self.lyrics_space,
            "lyrics_markup" => &mut self.lyrics_markup,
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
            "icon_cecilia" => &mut self.icon_cecilia,
            "score_modern" => &mut self.score_modern,
            _ => return None,
        })
    }

    /// The outline drawn around the edit cursor: the theme's `cursor_border`, else near-black
    /// on a light page and near-white on a dark one.
    pub fn cursor_border(&self) -> Rgba {
        self.cursor_border
            .unwrap_or_else(|| self.text_on(self.page))
    }

    /// The color of a mode: its status-line label, and the edit cursor while in it.
    pub fn mode_color(&self, mode: crate::config::Mode) -> Rgba {
        use crate::config::Mode;
        match mode {
            Mode::Normal => self.mode_normal,
            Mode::Insert => self.mode_insert,
            Mode::Visual => self.mode_visual,
            Mode::Command => self.mode_command,
            Mode::Picker => self.mode_picker,
            Mode::Tracks | Mode::Properties | Mode::Palette => self.mode_tracks,
            Mode::Sync => self.sync_point,
        }
    }

    /// The color of track `i` without a color of its own.
    pub fn track_color(&self, i: usize) -> Rgba {
        match self.track_colors.len() {
            0 => self.accent,
            n => self.track_colors[i % n],
        }
    }

    /// The color of voice `i` (0-based); voices past the palette cycle through it.
    pub fn voice_color(&self, i: usize) -> Rgba {
        match self.voice_colors.len() {
            0 => self.accent,
            n => self.voice_colors[i % n],
        }
    }

    /// The color of a file-type icon (`kind` from [`crate::file_kind`]).
    pub fn file_icon(&self, kind: &str) -> Rgba {
        match kind {
            "guitar-pro" => self.icon_guitar_pro,
            "musicxml" => self.icon_musicxml,
            "midi" => self.icon_midi,
            "chant" => self.icon_chant,
            "cecilia" => self.icon_cecilia,
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

    /// The built-in dark theme.
    pub fn dark() -> Self {
        Self::from_config(&ThemeConfig::default()).0
    }

    /// Lays one theme layer over the colors so far, reporting what couldn't be used.
    fn apply(&mut self, layer: &ThemeConfig, errors: &mut Vec<String>) {
        let accent_before = self.accent;
        for (name, value) in layer.colors.iter().filter(|(_, v)| !v.is_empty()) {
            let color = Rgba::parse(value);
            if color.is_none() {
                errors.push(format!("theme.{name}: '{value}' is not a color (#rrggbb)"));
                continue;
            }
            if name == "cursor_border" {
                self.cursor_border = color;
            } else if let Some(slot) = self.color_mut(name) {
                *slot = color.unwrap();
            } else {
                errors.push(format!("theme.{name}: not a theme color"));
            }
        }
        let mut palette = |key: &str, list: &[String]| {
            let colors: Vec<Rgba> = list
                .iter()
                .filter_map(|v| {
                    let c = Rgba::parse(v);
                    if c.is_none() {
                        errors.push(format!("theme.{key}: '{v}' is not a color"));
                    }
                    c
                })
                .collect();
            (!colors.is_empty()).then_some(colors)
        };
        let tracks = palette("track_colors", &layer.track_colors);
        let voices = palette("voice_colors", &layer.voice_colors);
        match tracks {
            Some(colors) => self.track_colors = colors,
            // A new accent shifts the default spectrum with it.
            None if self.accent != accent_before => {
                let l = self.track_colors.first().map_or(0.55, |c| c.to_hsl().2);
                self.track_colors = spectrum(self.accent, 8, l);
            }
            None => {}
        }
        if let Some(colors) = voices {
            self.voice_colors = colors;
        }
    }

    /// The theme a config asks for, and anything in it that couldn't be used. A theme file
    /// (`file`) is read first; the preset and colors given in `[theme]` itself win over it.
    pub fn from_config(c: &ThemeConfig) -> (Self, Vec<String>) {
        let mut errors = Vec::new();
        let from_file = match c.file.as_str() {
            "" => None,
            path => {
                let path = theme_path(std::path::Path::new(path));
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
        };
        // "dark" is the default, so it can't tell "unset" from "dark": a preset other than it
        // wins, then the theme file's.
        let preset = [
            Some(c.preset.as_str()),
            from_file.as_ref().map(|f| f.preset.as_str()),
        ]
        .into_iter()
        .flatten()
        .find(|p| !matches!(*p, "dark" | ""))
        .unwrap_or("dark");
        let mut layers = preset_layers(preset, user_themes_dir().as_deref()).unwrap_or_else(|e| {
            errors.push(format!("theme.preset: {e} ({})", preset_names().join(", ")));
            preset_layers("dark", None).expect("built-in dark theme")
        });
        layers.extend(from_file);
        layers.push(c.clone());
        let mut t = Self::default();
        for layer in &layers {
            t.apply(layer, &mut errors);
        }
        (t, errors)
    }
}

/// Where a theme file is: `~/…`, absolute, or relative to cecilia's config folder.
pub fn theme_path(path: &std::path::Path) -> std::path::PathBuf {
    if let Ok(rest) = path.strip_prefix("~") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    if path.is_absolute() {
        return path.to_path_buf();
    }
    directories::ProjectDirs::from("", "", "cecilia")
        .map(|d| d.config_dir().join(path))
        .unwrap_or_else(|| path.to_path_buf())
}

/// The default colors of voices 1 to 4: blue, orange, green, purple. They read on the dark and
/// the light interface presets alike.
pub fn default_voice_colors() -> Vec<Rgba> {
    vec![
        Rgba::rgb(70, 140, 230),
        Rgba::rgb(235, 140, 50),
        Rgba::rgb(60, 170, 100),
        Rgba::rgb(170, 100, 210),
    ]
}

/// `n` colors around the color wheel starting at `from`'s hue, at lightness `l`.
pub fn spectrum(from: Rgba, n: usize, l: f32) -> Vec<Rgba> {
    let (h, s, _) = from.to_hsl();
    let s = s.clamp(0.55, 0.85);
    (0..n)
        .map(|i| Rgba::from_hsl(h + i as f32 * 360.0 / n as f32, s, l))
        .collect()
}

/// `[theme]` in config.toml (or a theme file): a preset, colors overriding it by name
/// (`chrome = "#282a30"`, see [`COLOR_NAMES`]) and the track palette.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ThemeConfig {
    /// The color preset (empty: "dark").
    pub preset: String,
    /// A theme file with the same keys, applied under the ones here (empty: none).
    pub file: String,
    /// The colors of tracks 1, 2, … (empty: the built-in palette).
    pub track_colors: Vec<String>,
    /// The colors of voices 1, 2, … (the voice buttons of the editing palette; empty: built-in).
    pub voice_colors: Vec<String>,
    #[serde(flatten)]
    pub colors: std::collections::BTreeMap<String, String>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            preset: "dark".into(),
            file: String::new(),
            track_colors: Vec::new(),
            voice_colors: Vec::new(),
            colors: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(name: &str) -> Theme {
        let c = ThemeConfig {
            preset: name.into(),
            ..Default::default()
        };
        let (t, errors) = Theme::from_config(&c);
        assert!(errors.is_empty(), "{name}: {errors:?}");
        t
    }

    #[test]
    fn colors_parse_and_presets_take_overrides() {
        assert_eq!(Rgba::parse("#fa0"), Some(Rgba::rgb(255, 170, 0)));
        assert_eq!(Rgba::parse("102030"), Some(Rgba::rgb(16, 32, 48)));
        assert_eq!(Rgba::parse("#10203080").unwrap().a, 128);
        assert_eq!(Rgba::parse("#12"), None);
        assert_eq!(Rgba::rgb(16, 32, 48).hex(), "#102030");
        let c: ThemeConfig =
            toml::from_str("preset = \"light\"\nplayhead = \"#00ff00\"\ndim = \"nope\"").unwrap();
        let (t, errors) = Theme::from_config(&c);
        assert_eq!(t.playhead, Rgba::rgb(0, 255, 0));
        assert_eq!(t.chrome, preset("light").chrome);
        assert_eq!(errors.len(), 1);
        // Tracks cycle through the palette; it follows the accent unless given.
        assert_eq!(t.track_color(8), t.track_color(0));
        let c: ThemeConfig = toml::from_str("accent = \"#3080ff\"").unwrap();
        let (t, _) = Theme::from_config(&c);
        assert_ne!(t.track_colors, Theme::dark().track_colors);
        let c: ThemeConfig = toml::from_str("track_colors = [\"#ff0000\", \"#00ff00\"]").unwrap();
        let (t, _) = Theme::from_config(&c);
        assert_eq!(t.track_color(3), Rgba::rgb(0, 255, 0));
        // Voices cycle through their palette, which can be replaced.
        assert_eq!(t.voice_color(4), t.voice_color(0));
        assert_ne!(t.voice_color(0), t.voice_color(1));
        let c: ThemeConfig = toml::from_str("voice_colors = [\"#ff0000\", \"#00ff00\"]").unwrap();
        let (t, _) = Theme::from_config(&c);
        assert_eq!(t.voice_color(1), Rgba::rgb(0, 255, 0));
        // Unknown names are reported.
        let c: ThemeConfig = toml::from_str("chrom = \"#000000\"").unwrap();
        assert_eq!(Theme::from_config(&c).1.len(), 1);
    }

    #[test]
    fn cursor_border_is_set_or_contrasts_with_the_page() {
        let (t, e) = Theme::from_config(&toml::from_str("").unwrap());
        assert!(e.is_empty());
        assert_eq!(t.cursor_border(), Rgba::rgb(20, 20, 24));
        let c: ThemeConfig = toml::from_str("page = \"#101010\"").unwrap();
        assert_eq!(
            Theme::from_config(&c).0.cursor_border(),
            Rgba::rgb(245, 245, 248)
        );
        let c: ThemeConfig = toml::from_str("cursor_border = \"#ff0000\"").unwrap();
        assert_eq!(
            Theme::from_config(&c).0.cursor_border(),
            Rgba::rgb(255, 0, 0)
        );
        let c: ThemeConfig = toml::from_str("cursor_border = \"nope\"").unwrap();
        assert_eq!(Theme::from_config(&c).1.len(), 1);
    }

    #[test]
    fn every_preset_and_color_name_works() {
        for (p, _) in BUILTIN {
            let mut t = preset(p);
            for name in COLOR_NAMES {
                assert!(t.color_mut(name).is_some(), "{name}");
            }
            // The lyrics text area's colors read on the panel; the dimmed words less so.
            let luma = |c: Rgba| 0.299 * c.r as f32 + 0.587 * c.g as f32 + 0.114 * c.b as f32;
            let contrast = |c: Rgba| (luma(c) - luma(t.panel)).abs();
            for c in [
                t.lyrics_text,
                t.lyrics_gabc,
                t.lyrics_label,
                t.lyrics_anchor,
            ] {
                assert!(contrast(c) > 50.0, "{p}: {c:?} on {:?}", t.panel);
            }
            // The MODERN and CHANT chips stand out from the status line and from each other.
            let dist = |a: Rgba, b: Rgba| {
                (a.r as i32 - b.r as i32).abs()
                    + (a.g as i32 - b.g as i32).abs()
                    + (a.b as i32 - b.b as i32).abs()
            };
            assert_ne!(t.score_modern, t.icon_chant, "{p}");
            assert!(
                dist(t.score_modern, t.chrome) > 100,
                "{p}: modern on chrome"
            );
            assert!(dist(t.icon_chant, t.chrome) > 100, "{p}: chant on chrome");
            assert!(dist(t.score_modern, t.icon_chant) > 100, "{p}");
            assert!(contrast(t.lyrics_text_dim) < contrast(t.lyrics_text), "{p}");
            assert!(
                contrast(t.lyrics_gabc) >= contrast(t.lyrics_text_dim),
                "{p}"
            );
        }
        // Every built-in theme file holds only known colors and parses.
        for (p, text) in BUILTIN {
            let c: ThemeConfig = toml::from_str(text).unwrap();
            assert!(
                c.colors.keys().all(|k| COLOR_NAMES.contains(&k.as_str())),
                "{p}"
            );
        }
        // An unknown theme is reported and falls back to dark.
        let c: ThemeConfig = toml::from_str("preset = \"nope\"").unwrap();
        let (t, errors) = Theme::from_config(&c);
        assert_eq!(errors.len(), 1);
        assert_eq!(t, Theme::dark());
    }

    #[test]
    fn cecilia_themes_resolve_and_split_ui_from_score() {
        let luma = |c: Rgba| 0.299 * c.r as f32 + 0.587 * c.g as f32 + 0.114 * c.b as f32;
        let by_name = |n: &str| preset(n);
        let dark = by_name("cecilia-dark");
        assert!(luma(dark.page) < 80.0 && luma(dark.chrome) < 80.0 && luma(dark.ink) > 200.0);
        let hybrid = by_name("cecilia");
        assert!(luma(hybrid.chrome) < 80.0 && luma(hybrid.panel) < 80.0);
        assert!(luma(hybrid.page) > 220.0 && luma(hybrid.paper) > 180.0);
        assert_eq!(hybrid.ink, Rgba::rgb(0, 0, 0));
        let light = by_name("cecilia-light");
        assert!(luma(light.chrome) > 200.0 && luma(light.page) > 220.0);
        assert_eq!(light.ink, Rgba::rgb(0, 0, 0));
    }

    #[test]
    fn a_theme_file_sits_under_the_config() {
        let dir = std::env::temp_dir().join(format!("cecilia-theme-{}", std::process::id()));
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
        // The file's preset and colors, then the config's own on top.
        assert_eq!(t.paper, preset("nord").paper);
        assert_eq!(t.accent, Rgba::rgb(255, 0, 0));
        assert_eq!(t.chrome, Rgba::rgb(10, 11, 12));
        let c: ThemeConfig = toml::from_str("file = \"/nonexistent/x.toml\"").unwrap();
        assert_eq!(Theme::from_config(&c).1.len(), 1);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn user_themes_inherit_from_any_theme() {
        let dir = std::env::temp_dir().join(format!("cecilia-themes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("mine.toml"),
            "preset = \"nord\"\nchrome = \"#010203\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("plain.toml"), "ink = \"#102030\"\n").unwrap();
        std::fs::write(dir.join("a.toml"), "preset = \"b\"\n").unwrap();
        std::fs::write(dir.join("b.toml"), "preset = \"a\"\n").unwrap();
        let build = |name: &str| {
            let layers = preset_layers(name, Some(&dir))?;
            let mut t = Theme::default();
            layers.iter().for_each(|l| t.apply(l, &mut Vec::new()));
            Ok::<_, String>(t)
        };
        let mine = build("mine").unwrap();
        assert_eq!(mine.chrome, Rgba::rgb(1, 2, 3));
        assert_eq!(mine.paper, preset("nord").paper);
        // Without a `preset`, a theme starts from dark.
        let plain = build("plain").unwrap();
        assert_eq!(plain.ink, Rgba::rgb(16, 32, 48));
        assert_eq!(plain.chrome, Theme::dark().chrome);
        assert!(build("a").is_err() && build("nope").is_err() && build("../x").is_err());
        std::fs::remove_dir_all(dir).ok();
    }
}
