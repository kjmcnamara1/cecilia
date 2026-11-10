//! Interface colors: a preset (`[theme] preset = "dark"` or `"light"`) with any color
//! overridden in `config.toml`, and the palette tracks are colored from.

use serde::{Deserialize, Serialize};

/// A color with alpha.
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

/// The interface colors.
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

/// The built-in themes.
pub const PRESETS: &[&str] = &[
    "dark",
    "light",
    "cecilia-dark",
    "cecilia-light",
    "cecilia-hybrid",
    "nord",
    "gruvbox",
    "solarized-dark",
    "solarized-light",
    "catppuccin-latte",
    "catppuccin-frappe",
    "catppuccin-macchiato",
    "catppuccin-mocha",
];

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

fn hex(s: &str) -> Rgba {
    Rgba::parse(s).expect("valid built-in color")
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
            capo: Rgba::rgb(225, 225, 230),
            dim: Rgba::rgb(150, 152, 160),
            accent,
            playhead: Rgba::rgb(225, 40, 40),
            selection: Rgba::rgb(150, 90, 220),
            tab: Rgba::rgb(52, 55, 63),
            tab_hover: Rgba::rgb(72, 75, 86),
            status_b: Rgba::rgb(60, 63, 73),
            toolbar: Rgba::rgb(46, 48, 55),
            button_hover: Rgba::rgb(72, 75, 86),
            panel: Rgba::rgb(30, 32, 37),
            empty_bar: Rgba::rgb(55, 58, 66),
            incomplete_bar: Rgba::rgb(214, 40, 40),
            popover: Rgba::rgb(36, 38, 44),
            error: Rgba::rgb(255, 110, 100),
            lyrics: Rgba::rgb(110, 200, 130),
            lyrics_text: Rgba::rgb(225, 225, 230),
            lyrics_text_dim: Rgba::rgb(150, 152, 160),
            lyrics_gabc: Rgba::rgb(240, 240, 244),
            lyrics_label: Rgba::rgb(100, 160, 240),
            lyrics_anchor: Rgba::rgb(240, 90, 90),
            lyrics_clef: Rgba::rgb(190, 130, 230),
            lyrics_division: Rgba::rgb(240, 90, 90),
            lyrics_note: Rgba::rgb(100, 160, 240),
            lyrics_shape: Rgba::rgb(230, 200, 90),
            lyrics_attach: Rgba::rgb(90, 200, 210),
            lyrics_space: Rgba::rgb(130, 134, 144),
            lyrics_markup: Rgba::rgb(110, 200, 130),
            marker: Rgba::rgb(90, 190, 210),
            waveform: Rgba::rgb(110, 170, 230),
            loop_region: Rgba::rgb(60, 190, 110).with_alpha(50),
            sync_point: accent,
            mode_normal: Rgba::rgb(90, 150, 230),
            mode_insert: Rgba::rgb(100, 190, 120),
            mode_visual: Rgba::rgb(190, 120, 220),
            mode_command: Rgba::rgb(240, 180, 60),
            mode_picker: Rgba::rgb(80, 190, 200),
            mode_tracks: Rgba::rgb(230, 120, 80),
            icon_guitar_pro: Rgba::rgb(240, 140, 50),
            icon_musicxml: Rgba::rgb(80, 150, 230),
            icon_midi: Rgba::rgb(90, 190, 110),
            icon_chant: Rgba::rgb(180, 110, 210),
            icon_cecilia: Rgba::rgb(60, 190, 190),
            score_modern: Rgba::rgb(214, 140, 80),
            cursor_border: None,
            voice_colors: default_voice_colors(),
            track_colors: spectrum(accent, 8, 0.62),
        }
    }

    pub fn light() -> Self {
        let accent = Rgba::rgb(214, 140, 0);
        Self {
            paper: Rgba::rgb(250, 250, 251),
            chrome: Rgba::rgb(236, 237, 241),
            chrome_text: Rgba::rgb(36, 38, 44),
            capo: Rgba::rgb(36, 38, 44),
            dim: Rgba::rgb(110, 114, 124),
            accent,
            playhead: Rgba::rgb(215, 30, 30),
            selection: Rgba::rgb(140, 80, 210),
            tab: Rgba::rgb(220, 221, 227),
            tab_hover: Rgba::rgb(205, 207, 214),
            status_b: Rgba::rgb(214, 216, 222),
            toolbar: Rgba::rgb(244, 244, 247),
            button_hover: Rgba::rgb(218, 219, 225),
            panel: Rgba::rgb(246, 246, 248),
            empty_bar: Rgba::rgb(222, 223, 228),
            incomplete_bar: Rgba::rgb(214, 40, 40),
            popover: Rgba::rgb(252, 252, 253),
            error: Rgba::rgb(200, 40, 30),
            lyrics: Rgba::rgb(40, 140, 80),
            lyrics_text: Rgba::rgb(36, 38, 44),
            lyrics_text_dim: Rgba::rgb(130, 134, 144),
            lyrics_gabc: Rgba::rgb(20, 20, 24),
            lyrics_label: Rgba::rgb(40, 100, 200),
            lyrics_anchor: Rgba::rgb(200, 40, 40),
            lyrics_clef: Rgba::rgb(140, 70, 190),
            lyrics_division: Rgba::rgb(200, 40, 40),
            lyrics_note: Rgba::rgb(40, 100, 200),
            lyrics_shape: Rgba::rgb(170, 130, 0),
            lyrics_attach: Rgba::rgb(0, 140, 150),
            lyrics_space: Rgba::rgb(120, 124, 134),
            lyrics_markup: Rgba::rgb(30, 140, 60),
            marker: Rgba::rgb(30, 130, 150),
            waveform: Rgba::rgb(40, 110, 200),
            loop_region: Rgba::rgb(40, 160, 90).with_alpha(50),
            sync_point: accent,
            mode_normal: Rgba::rgb(60, 120, 210),
            mode_insert: Rgba::rgb(60, 160, 90),
            mode_visual: Rgba::rgb(160, 90, 200),
            mode_command: Rgba::rgb(220, 150, 30),
            mode_picker: Rgba::rgb(40, 160, 175),
            mode_tracks: Rgba::rgb(210, 100, 60),
            score_modern: Rgba::rgb(190, 105, 45),
            cursor_border: None,
            voice_colors: default_voice_colors(),
            track_colors: spectrum(accent, 8, 0.48),
            ..Self::dark()
        }
    }

    /// Dark UI, dark score page and light engraving: neutral charcoal with the logo's gold and cream.
    pub fn cecilia_dark() -> Self {
        let accent = hex("#e8b73c");
        Self {
            paper: hex("#161514"),
            page: hex("#1f1e1c"),
            ink: hex("#f6efdc"),
            chrome: hex("#1a1918"),
            chrome_text: hex("#f6efdc"),
            capo: hex("#f6efdc"),
            dim: hex("#9a948a"),
            accent,
            playhead: hex("#e5606a"),
            selection: hex("#8d7ad6"),
            tab: hex("#262422"),
            tab_hover: hex("#33302c"),
            status_b: hex("#2c2a27"),
            toolbar: hex("#1c1b19"),
            button_hover: hex("#33302c"),
            panel: hex("#161514"),
            empty_bar: hex("#2a2826"),
            popover: hex("#211f1d"),
            error: hex("#ff7a82"),
            lyrics_text: hex("#f6efdc"),
            lyrics_text_dim: hex("#9a948a"),
            lyrics_gabc: hex("#fffaf0"),
            lyrics_anchor: hex("#ff7a82"),
            lyrics_division: hex("#ff7a82"),
            lyrics_shape: accent,
            lyrics: hex("#aab95a"),
            lyrics_markup: hex("#aab95a"),
            sync_point: accent,
            waveform: hex("#8fb4e8"),
            mode_command: accent,
            score_modern: hex("#d98a45"),
            track_colors: spectrum(accent, 8, 0.62),
            ..Self::dark()
        }
    }

    /// Dark UI around a light cream page with black engraving.
    pub fn cecilia_hybrid() -> Self {
        Self {
            paper: hex("#e4dcc4"),
            page: hex("#fbf7ea"),
            ink: hex("#000000"),
            cursor_border: None,
            voice_colors: default_voice_colors(),
            ..Self::cecilia_dark()
        }
    }

    /// Light UI, light cream page and black engraving.
    pub fn cecilia_light() -> Self {
        let accent = hex("#c8921c");
        Self {
            paper: hex("#f4efe0"),
            page: hex("#fffcf2"),
            ink: hex("#000000"),
            chrome: hex("#ece4cc"),
            chrome_text: hex("#0f2a52"),
            capo: hex("#0f2a52"),
            dim: hex("#6b7690"),
            accent,
            playhead: hex("#d6404c"),
            tab: hex("#ddd3b6"),
            tab_hover: hex("#cfc4a2"),
            status_b: hex("#d8ceb0"),
            toolbar: hex("#f3ecd6"),
            button_hover: hex("#d8ceb0"),
            panel: hex("#f6f0dd"),
            empty_bar: hex("#ddd3b6"),
            popover: hex("#fffcf2"),
            error: hex("#c4303a"),
            lyrics_text: hex("#0f2a52"),
            lyrics_text_dim: hex("#8590a8"),
            lyrics_gabc: hex("#06142a"),
            lyrics_anchor: hex("#c4303a"),
            lyrics_division: hex("#c4303a"),
            lyrics_shape: hex("#a87400"),
            sync_point: accent,
            waveform: hex("#2a5fa8"),
            mode_command: hex("#d9921a"),
            score_modern: hex("#b05f25"),
            track_colors: spectrum(accent, 8, 0.48),
            ..Self::light()
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
            capo: hex("#eceff4"),
            dim: hex("#9aa5b8"),
            accent,
            playhead: hex("#bf616a"),
            selection: hex("#b48ead"),
            tab: hex("#3b4252"),
            tab_hover: hex("#434c5e"),
            status_b: hex("#3b4252"),
            toolbar: hex("#323946"),
            button_hover: hex("#434c5e"),
            panel: hex("#272c36"),
            empty_bar: hex("#3b4252"),
            incomplete_bar: Rgba::rgb(214, 40, 40),
            popover: hex("#3b4252"),
            error: hex("#bf616a"),
            lyrics: hex("#a3be8c"),
            lyrics_text: hex("#e5e9f0"),
            lyrics_text_dim: hex("#7b88a1"),
            lyrics_gabc: hex("#eceff4"),
            lyrics_label: hex("#81a1c1"),
            lyrics_anchor: hex("#bf616a"),
            lyrics_clef: hex("#b48ead"),
            lyrics_division: hex("#bf616a"),
            lyrics_note: hex("#81a1c1"),
            lyrics_shape: hex("#ebcb8b"),
            lyrics_attach: hex("#88c0d0"),
            lyrics_space: hex("#7b88a1"),
            lyrics_markup: hex("#a3be8c"),
            marker: hex("#88c0d0"),
            waveform: hex("#81a1c1"),
            loop_region: hex("#a3be8c").with_alpha(50),
            sync_point: hex("#ebcb8b"),
            mode_normal: hex("#81a1c1"),
            mode_insert: hex("#a3be8c"),
            mode_visual: hex("#b48ead"),
            mode_command: hex("#ebcb8b"),
            mode_picker: hex("#88c0d0"),
            mode_tracks: hex("#d08770"),
            icon_guitar_pro: hex("#d08770"),
            icon_musicxml: hex("#81a1c1"),
            icon_midi: hex("#a3be8c"),
            icon_chant: hex("#b48ead"),
            icon_cecilia: hex("#8fbcbb"),
            score_modern: hex("#ebcb8b"),
            cursor_border: None,
            voice_colors: default_voice_colors(),
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
            capo: hex("#ebdbb2"),
            dim: hex("#a89984"),
            accent,
            playhead: hex("#fb4934"),
            selection: hex("#d3869b"),
            tab: hex("#3c3836"),
            tab_hover: hex("#504945"),
            status_b: hex("#3c3836"),
            toolbar: hex("#32302f"),
            button_hover: hex("#504945"),
            panel: hex("#1d2021"),
            empty_bar: hex("#3c3836"),
            incomplete_bar: Rgba::rgb(214, 40, 40),
            popover: hex("#3c3836"),
            error: hex("#fb4934"),
            lyrics: hex("#b8bb26"),
            lyrics_text: hex("#ebdbb2"),
            lyrics_text_dim: hex("#928374"),
            lyrics_gabc: hex("#fbf1c7"),
            lyrics_label: hex("#83a598"),
            lyrics_anchor: hex("#fb4934"),
            lyrics_clef: hex("#d3869b"),
            lyrics_division: hex("#fb4934"),
            lyrics_note: hex("#83a598"),
            lyrics_shape: hex("#fabd2f"),
            lyrics_attach: hex("#8ec07c"),
            lyrics_space: hex("#928374"),
            lyrics_markup: hex("#b8bb26"),
            marker: hex("#8ec07c"),
            waveform: hex("#83a598"),
            loop_region: hex("#b8bb26").with_alpha(50),
            sync_point: hex("#fabd2f"),
            mode_normal: hex("#83a598"),
            mode_insert: hex("#b8bb26"),
            mode_visual: hex("#d3869b"),
            mode_command: hex("#fabd2f"),
            mode_picker: hex("#8ec07c"),
            mode_tracks: hex("#fe8019"),
            icon_guitar_pro: hex("#fe8019"),
            icon_musicxml: hex("#83a598"),
            icon_midi: hex("#b8bb26"),
            icon_chant: hex("#d3869b"),
            icon_cecilia: hex("#8ec07c"),
            score_modern: hex("#fe8019"),
            cursor_border: None,
            voice_colors: default_voice_colors(),
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
            capo: hex("#93a1a1"),
            dim: hex("#657b83"),
            accent,
            playhead: hex("#dc322f"),
            selection: hex("#6c71c4"),
            tab: hex("#073642"),
            tab_hover: hex("#0b4553"),
            status_b: hex("#073642"),
            toolbar: hex("#03303b"),
            button_hover: hex("#0b4553"),
            panel: hex("#00212b"),
            empty_bar: hex("#073642"),
            incomplete_bar: Rgba::rgb(214, 40, 40),
            popover: hex("#073642"),
            error: hex("#dc322f"),
            lyrics: hex("#859900"),
            lyrics_text: hex("#93a1a1"),
            lyrics_text_dim: hex("#586e75"),
            lyrics_gabc: hex("#eee8d5"),
            lyrics_label: hex("#268bd2"),
            lyrics_anchor: hex("#dc322f"),
            lyrics_clef: hex("#6c71c4"),
            lyrics_division: hex("#dc322f"),
            lyrics_note: hex("#268bd2"),
            lyrics_shape: hex("#b58900"),
            lyrics_attach: hex("#2aa198"),
            lyrics_space: hex("#586e75"),
            lyrics_markup: hex("#859900"),
            marker: hex("#2aa198"),
            waveform: hex("#268bd2"),
            loop_region: hex("#859900").with_alpha(50),
            sync_point: hex("#b58900"),
            mode_normal: hex("#268bd2"),
            mode_insert: hex("#859900"),
            mode_visual: hex("#d33682"),
            mode_command: hex("#b58900"),
            mode_picker: hex("#2aa198"),
            mode_tracks: hex("#cb4b16"),
            icon_guitar_pro: hex("#cb4b16"),
            icon_musicxml: hex("#268bd2"),
            icon_midi: hex("#859900"),
            icon_chant: hex("#6c71c4"),
            icon_cecilia: hex("#2aa198"),
            score_modern: hex("#cb4b16"),
            cursor_border: None,
            voice_colors: default_voice_colors(),
            track_colors: spectrum(accent, 8, 0.55),
        }
    }

    pub fn solarized_light() -> Self {
        Self {
            chrome: hex("#eee8d5"),
            chrome_text: hex("#586e75"),
            capo: hex("#586e75"),
            dim: hex("#93a1a1"),
            tab: hex("#e4ddc8"),
            tab_hover: hex("#d9d2bd"),
            status_b: hex("#e4ddc8"),
            toolbar: hex("#f5efdc"),
            button_hover: hex("#e4ddc8"),
            panel: hex("#f5efdc"),
            empty_bar: hex("#e4ddc8"),
            incomplete_bar: Rgba::rgb(214, 40, 40),
            popover: hex("#fdf6e3"),
            lyrics_text: hex("#586e75"),
            lyrics_text_dim: hex("#93a1a1"),
            lyrics_gabc: hex("#073642"),
            track_colors: spectrum(hex("#b58900"), 8, 0.45),
            ..Self::solarized_dark()
        }
    }

    /// A built-in theme by name.
    /// A Catppuccin flavor (<https://catppuccin.com/palette>): the page in the flavor's base with
    /// its text as ink, so the dark flavors engrave light on dark.
    fn catppuccin(c: &Catppuccin) -> Self {
        let h = hex;
        Self {
            paper: h(c.crust),
            page: h(c.base),
            ink: h(c.text),
            chrome: h(c.mantle),
            chrome_text: h(c.text),
            capo: h(c.text),
            dim: h(c.overlay1),
            accent: h(c.mauve),
            playhead: h(c.red),
            selection: h(c.lavender),
            tab: h(c.surface0),
            tab_hover: h(c.surface1),
            status_b: h(c.surface0),
            toolbar: h(c.mantle),
            button_hover: h(c.surface1),
            panel: h(c.crust),
            empty_bar: h(c.surface0),
            incomplete_bar: h(c.red),
            popover: h(c.surface0),
            error: h(c.red),
            lyrics: h(c.green),
            lyrics_text: h(c.text),
            lyrics_text_dim: h(c.overlay0),
            lyrics_gabc: h(c.subtext1),
            lyrics_label: h(c.blue),
            lyrics_anchor: h(c.red),
            lyrics_clef: h(c.mauve),
            lyrics_division: h(c.maroon),
            lyrics_note: h(c.sapphire),
            lyrics_shape: h(c.yellow),
            lyrics_attach: h(c.teal),
            lyrics_space: h(c.overlay0),
            lyrics_markup: h(c.green),
            marker: h(c.teal),
            waveform: h(c.blue),
            loop_region: h(c.green).with_alpha(50),
            sync_point: h(c.yellow),
            mode_normal: h(c.blue),
            mode_insert: h(c.green),
            mode_visual: h(c.mauve),
            mode_command: h(c.peach),
            mode_picker: h(c.teal),
            mode_tracks: h(c.flamingo),
            icon_guitar_pro: h(c.peach),
            icon_musicxml: h(c.blue),
            icon_midi: h(c.green),
            icon_chant: h(c.mauve),
            icon_cecilia: h(c.teal),
            score_modern: h(c.peach),
            cursor_border: None,
            voice_colors: vec![h(c.blue), h(c.peach), h(c.green), h(c.mauve)],
            track_colors: [
                c.mauve, c.blue, c.green, c.peach, c.pink, c.teal, c.yellow, c.red,
            ]
            .into_iter()
            .map(h)
            .collect(),
        }
    }

    pub fn preset(name: &str) -> Option<Self> {
        Some(match name {
            "dark" => Self::dark(),
            "light" => Self::light(),
            "cecilia-dark" => Self::cecilia_dark(),
            "cecilia-light" => Self::cecilia_light(),
            "cecilia-hybrid" => Self::cecilia_hybrid(),
            "nord" => Self::nord(),
            "gruvbox" => Self::gruvbox(),
            "solarized-dark" => Self::solarized_dark(),
            "solarized-light" => Self::solarized_light(),
            "catppuccin-latte" => Self::catppuccin(&LATTE),
            "catppuccin-frappe" => Self::catppuccin(&FRAPPE),
            "catppuccin-macchiato" => Self::catppuccin(&MACCHIATO),
            "catppuccin-mocha" => Self::catppuccin(&MOCHA),
            _ => return None,
        })
    }

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
        let mut voice_colors = None;
        for layer in layers {
            for (name, value) in layer.colors.iter().filter(|(_, v)| !v.is_empty()) {
                if name == "cursor_border" {
                    match Rgba::parse(value) {
                        Some(color) => t.cursor_border = Some(color),
                        None => {
                            errors.push(format!("theme.{name}: '{value}' is not a color (#rrggbb)"))
                        }
                    }
                    continue;
                }
                let Some(slot) = t.color_mut(name) else {
                    errors.push(format!("theme.{name}: not a theme color"));
                    continue;
                };
                match Rgba::parse(value) {
                    Some(color) => *slot = color,
                    None => {
                        errors.push(format!("theme.{name}: '{value}' is not a color (#rrggbb)"))
                    }
                }
            }
            if !layer.track_colors.is_empty() {
                track_colors = Some(layer.track_colors.clone());
            }
            if !layer.voice_colors.is_empty() {
                voice_colors = Some(layer.voice_colors.clone());
            }
        }
        match track_colors {
            Some(list) => {
                let colors: Vec<Rgba> = list
                    .iter()
                    .filter_map(|v| {
                        let c = Rgba::parse(v);
                        if c.is_none() {
                            errors.push(format!("theme.track_colors: '{v}' is not a color"));
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
        if let Some(list) = voice_colors {
            let colors: Vec<Rgba> = list
                .iter()
                .filter_map(|v| {
                    let c = Rgba::parse(v);
                    if c.is_none() {
                        errors.push(format!("theme.voice_colors: '{v}' is not a color"));
                    }
                    c
                })
                .collect();
            if !colors.is_empty() {
                t.voice_colors = colors;
            }
        }
        (t, errors)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
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

/// The Catppuccin colors the presets use, for one flavor.
struct Catppuccin {
    flamingo: &'static str,
    pink: &'static str,
    mauve: &'static str,
    red: &'static str,
    maroon: &'static str,
    peach: &'static str,
    yellow: &'static str,
    green: &'static str,
    teal: &'static str,
    sapphire: &'static str,
    blue: &'static str,
    lavender: &'static str,
    text: &'static str,
    subtext1: &'static str,
    overlay0: &'static str,
    overlay1: &'static str,
    surface0: &'static str,
    surface1: &'static str,
    base: &'static str,
    mantle: &'static str,
    crust: &'static str,
}

const LATTE: Catppuccin = Catppuccin {
    flamingo: "#dd7878",
    pink: "#ea76cb",
    mauve: "#8839ef",
    red: "#d20f39",
    maroon: "#e64553",
    peach: "#fe640b",
    yellow: "#df8e1d",
    green: "#40a02b",
    teal: "#179299",
    sapphire: "#209fb5",
    blue: "#1e66f5",
    lavender: "#7287fd",
    text: "#4c4f69",
    subtext1: "#5c5f77",
    overlay0: "#9ca0b0",
    overlay1: "#8c8fa1",
    surface0: "#ccd0da",
    surface1: "#bcc0cc",
    base: "#eff1f5",
    mantle: "#e6e9ef",
    crust: "#dce0e8",
};

const FRAPPE: Catppuccin = Catppuccin {
    flamingo: "#eebebe",
    pink: "#f4b8e4",
    mauve: "#ca9ee6",
    red: "#e78284",
    maroon: "#ea999c",
    peach: "#ef9f76",
    yellow: "#e5c890",
    green: "#a6d189",
    teal: "#81c8be",
    sapphire: "#85c1dc",
    blue: "#8caaee",
    lavender: "#babbf1",
    text: "#c6d0f5",
    subtext1: "#b5bfe2",
    overlay0: "#737994",
    overlay1: "#838ba7",
    surface0: "#414559",
    surface1: "#51576d",
    base: "#303446",
    mantle: "#292c3c",
    crust: "#232634",
};

const MACCHIATO: Catppuccin = Catppuccin {
    flamingo: "#f0c6c6",
    pink: "#f5bde6",
    mauve: "#c6a0f6",
    red: "#ed8796",
    maroon: "#ee99a0",
    peach: "#f5a97f",
    yellow: "#eed49f",
    green: "#a6da95",
    teal: "#8bd5ca",
    sapphire: "#7dc4e4",
    blue: "#8aadf4",
    lavender: "#b7bdf8",
    text: "#cad3f5",
    subtext1: "#b8c0e0",
    overlay0: "#6e738d",
    overlay1: "#8087a2",
    surface0: "#363a4f",
    surface1: "#494d64",
    base: "#24273a",
    mantle: "#1e2030",
    crust: "#181926",
};

const MOCHA: Catppuccin = Catppuccin {
    flamingo: "#f2cdcd",
    pink: "#f5c2e7",
    mauve: "#cba6f7",
    red: "#f38ba8",
    maroon: "#eba0ac",
    peach: "#fab387",
    yellow: "#f9e2af",
    green: "#a6e3a1",
    teal: "#94e2d5",
    sapphire: "#74c7ec",
    blue: "#89b4fa",
    lavender: "#b4befe",
    text: "#cdd6f4",
    subtext1: "#bac2de",
    overlay0: "#6c7086",
    overlay1: "#7f849c",
    surface0: "#313244",
    surface1: "#45475a",
    base: "#1e1e2e",
    mantle: "#181825",
    crust: "#11111b",
};

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
        for p in PRESETS {
            let mut t = Theme::preset(p).unwrap();
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
        assert!(Theme::preset("nope").is_none());
    }

    #[test]
    fn cecilia_themes_resolve_and_split_ui_from_score() {
        let luma = |c: Rgba| 0.299 * c.r as f32 + 0.587 * c.g as f32 + 0.114 * c.b as f32;
        let by_name = |n: &str| {
            let c: ThemeConfig = toml::from_str(&format!("preset = \"{n}\"")).unwrap();
            let (t, errs) = Theme::from_config(&c);
            assert!(errs.is_empty(), "{n}: {errs:?}");
            t
        };
        let dark = by_name("cecilia-dark");
        assert!(luma(dark.page) < 80.0 && luma(dark.chrome) < 80.0 && luma(dark.ink) > 200.0);
        let hybrid = by_name("cecilia-hybrid");
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
        assert_eq!(t.paper, Theme::nord().paper);
        assert_eq!(t.accent, Rgba::rgb(255, 0, 0));
        assert_eq!(t.chrome, Rgba::rgb(10, 11, 12));
        let c: ThemeConfig = toml::from_str("file = \"/nonexistent/x.toml\"").unwrap();
        assert_eq!(Theme::from_config(&c).1.len(), 1);
        std::fs::remove_dir_all(dir).ok();
    }
}
