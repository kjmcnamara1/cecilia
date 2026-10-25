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
    /// Behind the engraved score (the paper).
    pub paper: Rgba,
    /// Tabline, status line, panels.
    pub chrome: Rgba,
    pub chrome_text: Rgba,
    /// Secondary text.
    pub dim: Rgba,
    /// Highlights: the active tab, focus borders.
    pub accent: Rgba,
    /// Edit cursor (filled translucently) and its outline.
    pub cursor: Rgba,
    pub cursor_outline: Rgba,
    /// The playhead (red bar).
    pub playhead: Rgba,
    pub selection: Rgba,
    /// Buffer tabs: normal and hovered (the active tab uses `paper`).
    pub tab: Rgba,
    pub tab_hover: Rgba,
    /// The tracks panel.
    pub panel: Rgba,
    /// A bar without notes in the tracks panel.
    pub empty_bar: Rgba,
    pub mode_normal: Rgba,
    pub mode_insert: Rgba,
    pub mode_visual: Rgba,
    pub mode_command: Rgba,
    pub mode_picker: Rgba,
    pub mode_tracks: Rgba,
    /// Colours given to tracks in order (a track can have its own with `:track-color`).
    pub track_colors: Vec<Rgba>,
}

impl Theme {
    pub fn dark() -> Self {
        let accent = Rgba::rgb(250, 190, 40);
        Self {
            paper: Rgba::rgb(232, 233, 236),
            chrome: Rgba::rgb(40, 42, 48),
            chrome_text: Rgba::rgb(225, 225, 230),
            dim: Rgba::rgb(150, 152, 160),
            accent,
            cursor: Rgba::rgb(250, 190, 40),
            cursor_outline: Rgba::rgb(230, 140, 0),
            playhead: Rgba::rgb(225, 40, 40),
            selection: Rgba::rgb(150, 90, 220),
            tab: Rgba::rgb(52, 55, 63),
            tab_hover: Rgba::rgb(72, 75, 86),
            panel: Rgba::rgb(30, 32, 37),
            empty_bar: Rgba::rgb(55, 58, 66),
            mode_normal: Rgba::rgb(90, 150, 230),
            mode_insert: Rgba::rgb(240, 180, 60),
            mode_visual: Rgba::rgb(190, 120, 220),
            mode_command: Rgba::rgb(100, 190, 120),
            mode_picker: Rgba::rgb(80, 190, 200),
            mode_tracks: Rgba::rgb(230, 120, 80),
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
            cursor: Rgba::rgb(250, 190, 40),
            cursor_outline: Rgba::rgb(200, 120, 0),
            playhead: Rgba::rgb(215, 30, 30),
            selection: Rgba::rgb(140, 80, 210),
            tab: Rgba::rgb(220, 221, 227),
            tab_hover: Rgba::rgb(205, 207, 214),
            panel: Rgba::rgb(246, 246, 248),
            empty_bar: Rgba::rgb(222, 223, 228),
            mode_normal: Rgba::rgb(60, 120, 210),
            mode_insert: Rgba::rgb(220, 150, 30),
            mode_visual: Rgba::rgb(160, 90, 200),
            mode_command: Rgba::rgb(60, 160, 90),
            mode_picker: Rgba::rgb(40, 160, 175),
            mode_tracks: Rgba::rgb(210, 100, 60),
            track_colors: spectrum(accent, 8, 0.48),
        }
    }

    /// The colour of track `i` without a colour of its own.
    pub fn track_color(&self, i: usize) -> Rgba {
        match self.track_colors.len() {
            0 => self.accent,
            n => self.track_colors[i % n],
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

    /// The theme a config asks for, and any colours it couldn't read.
    pub fn from_config(c: &ThemeConfig) -> (Self, Vec<String>) {
        let mut errors = Vec::new();
        let mut t = match c.preset.as_deref().unwrap_or("dark") {
            "dark" => Self::dark(),
            "light" => Self::light(),
            other => {
                errors.push(format!(
                    "theme.preset: unknown preset '{other}' (dark, light)"
                ));
                Self::dark()
            }
        };
        let accent_before = t.accent;
        let fields: [(&str, &Option<String>, &mut Rgba); 19] = [
            ("paper", &c.paper, &mut t.paper),
            ("chrome", &c.chrome, &mut t.chrome),
            ("chrome_text", &c.chrome_text, &mut t.chrome_text),
            ("dim", &c.dim, &mut t.dim),
            ("accent", &c.accent, &mut t.accent),
            ("cursor", &c.cursor, &mut t.cursor),
            ("cursor_outline", &c.cursor_outline, &mut t.cursor_outline),
            ("playhead", &c.playhead, &mut t.playhead),
            ("selection", &c.selection, &mut t.selection),
            ("tab", &c.tab, &mut t.tab),
            ("tab_hover", &c.tab_hover, &mut t.tab_hover),
            ("panel", &c.panel, &mut t.panel),
            ("empty_bar", &c.empty_bar, &mut t.empty_bar),
            ("mode_normal", &c.mode_normal, &mut t.mode_normal),
            ("mode_insert", &c.mode_insert, &mut t.mode_insert),
            ("mode_visual", &c.mode_visual, &mut t.mode_visual),
            ("mode_command", &c.mode_command, &mut t.mode_command),
            ("mode_picker", &c.mode_picker, &mut t.mode_picker),
            ("mode_tracks", &c.mode_tracks, &mut t.mode_tracks),
        ];
        for (name, value, slot) in fields {
            if let Some(v) = value {
                match Rgba::parse(v) {
                    Some(color) => *slot = color,
                    None => errors.push(format!("theme.{name}: '{v}' is not a colour (#rrggbb)")),
                }
            }
        }
        match &c.track_colors {
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

/// `n` colours around the colour wheel starting at `from`'s hue, at lightness `l`.
pub fn spectrum(from: Rgba, n: usize, l: f32) -> Vec<Rgba> {
    let (h, s, _) = from.to_hsl();
    let s = s.clamp(0.55, 0.85);
    (0..n)
        .map(|i| Rgba::from_hsl(h + i as f32 * 360.0 / n as f32, s, l))
        .collect()
}

/// `[theme]` in config.toml: a preset and colours overriding it (`"#rrggbb"`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ThemeConfig {
    pub preset: Option<String>,
    pub paper: Option<String>,
    pub chrome: Option<String>,
    pub chrome_text: Option<String>,
    pub dim: Option<String>,
    pub accent: Option<String>,
    pub cursor: Option<String>,
    pub cursor_outline: Option<String>,
    pub playhead: Option<String>,
    pub selection: Option<String>,
    pub tab: Option<String>,
    pub tab_hover: Option<String>,
    pub panel: Option<String>,
    pub empty_bar: Option<String>,
    pub mode_normal: Option<String>,
    pub mode_insert: Option<String>,
    pub mode_visual: Option<String>,
    pub mode_command: Option<String>,
    pub mode_picker: Option<String>,
    pub mode_tracks: Option<String>,
    pub track_colors: Option<Vec<String>>,
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
    }
}
