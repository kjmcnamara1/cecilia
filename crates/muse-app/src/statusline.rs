//! The status line's sections, as in lualine.nvim: on the left the mode (`a`), the pane layout
//! (`b`), the workspace and the file (`c`); on the right the rest of the state (`x`), the
//! position as a percentage (`y`) and `bar:beat` (`z`). Frontends draw them with the
//! separators chosen in `[statusline] separators`.

use crate::app::App;
use crate::config::Mode;

/// How a segment is coloured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegKind {
    /// The mode's colour behind, text that reads on it (sections `a` and `z`).
    Mode,
    /// The section-b colour (`status_b`) behind, the mode's colour as text.
    ModeText,
    /// The section-b colour behind, normal text (section `y`).
    B,
    /// The status line's own colour (sections `c` and `x`).
    Plain,
}

/// A small symbol before a segment's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusIcon {
    Workspace,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub text: String,
    pub icon: Option<StatusIcon>,
    pub kind: SegKind,
    pub bold: bool,
}

impl Segment {
    fn new(text: impl Into<String>, kind: SegKind) -> Self {
        Self {
            text: text.into(),
            icon: None,
            kind,
            bold: kind == SegKind::Mode,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StatusLine {
    pub left: Vec<Segment>,
    /// Left to right, as drawn.
    pub right: Vec<Segment>,
}

/// The mode's label.
pub fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "NORMAL",
        Mode::Insert => "INSERT",
        Mode::Visual => "VISUAL",
        Mode::Command => "COMMAND",
        Mode::Picker => "PICK",
        Mode::Tracks => "TRACKS",
    }
}

/// Where the cursor's bar is in the score, as lualine's progress: `Top`, `Bot` or `NN%`.
pub fn progress(bar: usize, bars: usize) -> String {
    if bar == 0 {
        "Top".into()
    } else if bar + 1 >= bars {
        "Bot".into()
    } else {
        format!("{}%", (bar + 1) * 100 / bars.max(1))
    }
}

const NOTES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

fn fmt_time(t: f64) -> String {
    let t = t.max(0.0) as u64;
    format!("{}:{:02}", t / 60, t % 60)
}

impl App {
    /// The status line. `play` is the playing time and length (from the frontend's player),
    /// `zoom` whether to show the zoom level (GUI).
    pub fn statusline(&self, play: Option<(f64, f64)>, zoom: bool) -> StatusLine {
        let pane = self.pane();
        let buffer = self.buffer();
        let score = &buffer.score;
        let mut left = vec![
            Segment::new(mode_label(self.mode), SegKind::Mode),
            Segment::new(pane.layout.clone(), SegKind::ModeText),
        ];
        if let Some(ws) = self.workspace_name() {
            left.push(Segment {
                icon: Some(StatusIcon::Workspace),
                ..Segment::new(ws, SegKind::Plain)
            });
        }
        let mut file = buffer.name.clone();
        if buffer.modified {
            file.push_str(" ●");
        }
        left.push(Segment::new(file, SegKind::Plain));

        // Section x: everything else, plainly.
        let mut x: Vec<String> = Vec::new();
        if let Some(t) = score.tracks.get(pane.cursor.track) {
            if t.kind != muse_core::TrackKind::Drums && !t.strings.is_empty() {
                let tuning: Vec<&str> = t
                    .strings
                    .iter()
                    .rev()
                    .map(|&p| NOTES[p as usize % 12])
                    .collect();
                let mut s = tuning.join(" ");
                if t.capo > 0 {
                    s.push_str(&format!(" capo {}", t.capo));
                }
                x.push(s);
            }
            x.push(format!("{} bpm", score.tempo));
        }
        if let Some((t, total)) = play {
            let icon = if self.playing { "▶" } else { "⏸" };
            x.push(format!("{icon} {}/{}", fmt_time(t), fmt_time(total)));
        }
        let speed = self.config.file.audio.speed;
        if (speed - 1.0).abs() > 1e-6 {
            x.push(format!("speed {:.0}%", speed * 100.0));
        }
        let flags = self.pa.flags(score.audio.is_some());
        if !flags.is_empty() {
            x.push(flags.trim_start().to_string());
        }
        if let Some(r) = self.recording_macro() {
            x.push(format!("rec @{r}"));
        }
        if self.mode == Mode::Insert {
            let d = self.insert_duration();
            x.push(format!(
                "note 1/{}{}",
                d.value.denominator(),
                ".".repeat(d.dots as usize)
            ));
        }
        if zoom {
            x.push(format!("zoom {:.0}%", self.zoom * 100.0));
        }
        let mut right = vec![Segment::new(x.join("  "), SegKind::Plain)];
        if !score.tracks.is_empty() {
            right.push(Segment::new(
                progress(pane.cursor.bar, score.master_bars.len()),
                SegKind::B,
            ));
            right.push(Segment::new(
                format!("{}:{}", pane.cursor.bar + 1, pane.cursor.beat + 1),
                SegKind::Mode,
            ));
        }
        StatusLine { left, right }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::parse_keys;
    use crate::{Command, Config};

    #[test]
    fn sections_follow_the_mode_and_cursor() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        let s = app.statusline(None, false);
        let texts = |v: &[Segment]| v.iter().map(|s| s.text.clone()).collect::<Vec<_>>();
        assert_eq!(texts(&s.left)[..2], ["NORMAL", "vertical"]);
        assert_eq!(s.left[0].kind, SegKind::Mode);
        assert_eq!(s.left[1].kind, SegKind::ModeText);
        assert_eq!(texts(&s.right)[1..], ["Top", "1:1"]);
        assert!(s.right[0].text.contains(" bpm"));
        for k in parse_keys("}}}i", &[]).unwrap() {
            app.handle_key(k);
        }
        let s = app.statusline(Some((42.0, 190.0)), true);
        assert_eq!(s.left[0].text, "INSERT");
        assert_eq!(s.right.last().unwrap().text, "4:1");
        assert!(s.right[0].text.contains("0:42/3:10"), "{}", s.right[0].text);
        assert_eq!(progress(3, 8), "50%");
        assert_eq!(progress(7, 8), "Bot");
    }
}
