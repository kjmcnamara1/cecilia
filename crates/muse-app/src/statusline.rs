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
    /// The section-b colour (`status_b`) behind, the mode's colour as text (sections `b`, `y`).
    ModeText,
    /// The status line's own colour (sections `c` and `x`).
    Plain,
}

/// A small symbol before a segment's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusIcon {
    Workspace,
    /// A quarter note, before the tempo (`=85`).
    Tempo,
    /// A magnifying glass, before the zoom (`95%`).
    Zoom,
    /// A speedometer, before the playback speed (`95%`).
    Speed,
    /// What plays with an audio track: the synth (MIDI), the recording, or both.
    Synth,
    Audio,
    SynthAndAudio,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub text: String,
    pub icon: Option<StatusIcon>,
    pub kind: SegKind,
    pub bold: bool,
    /// What clicking it does.
    pub action: Option<crate::command::Command>,
}

impl Segment {
    fn new(text: impl Into<String>, kind: SegKind) -> Self {
        Self {
            text: text.into(),
            icon: None,
            kind,
            bold: kind == SegKind::Mode,
            action: None,
        }
    }

    fn on_click(self, action: crate::command::Command) -> Self {
        Self {
            action: Some(action),
            ..self
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
            // Click: the next layout.
            Segment::new(pane.layout.clone(), SegKind::ModeText)
                .on_click(crate::command::Command::Layout(None)),
        ];
        if let Some(ws) = self.workspace_name() {
            left.push(
                Segment {
                    icon: Some(StatusIcon::Workspace),
                    ..Segment::new(ws, SegKind::Plain)
                }
                .on_click(crate::command::Command::Pick(
                    crate::picker::PickerKind::Workspace,
                )),
            );
        }
        let mut file = buffer.name.clone();
        if buffer.modified {
            file.push_str(" ●");
        }
        left.push(Segment::new(file, SegKind::Plain));

        // Section x: everything else, plainly.
        let mut x: Vec<String> = Vec::new();
        if let Some((t, total)) = play {
            let icon = if self.playing { "▶" } else { "⏸" };
            x.push(format!("{icon} {}/{}", fmt_time(t), fmt_time(total)));
        }
        let flags = self.pa.flags();
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

        // The tempo as ♩=85 (tuning and capo are shown at the top of the score), then the rest.
        let mut right = Vec::new();
        if !score.tracks.is_empty() {
            right.push(Segment {
                icon: Some(StatusIcon::Tempo),
                ..Segment::new(format!("={}", score.tempo), SegKind::Plain)
            });
        }
        // The playback speed, when it isn't the written tempo.
        let speed = self.config.file.audio.speed;
        if (speed - 1.0).abs() > 1e-6 {
            right.push(
                Segment {
                    icon: Some(StatusIcon::Speed),
                    ..Segment::new(format!("{:.0}%", speed * 100.0), SegKind::Plain)
                }
                .on_click(crate::command::Command::Speed(100)),
            );
        }
        // With an audio track: what plays (synth, recording or both), as an icon.
        if score.audio.is_some() {
            right.push(
                Segment {
                    icon: Some(match self.pa.mode {
                        crate::playalong::MixMode::Midi => StatusIcon::Synth,
                        crate::playalong::MixMode::Audio => StatusIcon::Audio,
                        crate::playalong::MixMode::Both => StatusIcon::SynthAndAudio,
                    }),
                    ..Segment::new("", SegKind::Plain)
                }
                .on_click(crate::command::Command::MixMode(None)),
            );
        }
        right.push(Segment::new(x.join("  "), SegKind::Plain));
        // The zoom, while it isn't 100%.
        if zoom && (self.zoom - 1.0).abs() > 1e-6 {
            right.push(
                Segment {
                    icon: Some(StatusIcon::Zoom),
                    ..Segment::new(format!("{:.0}%", self.zoom * 100.0), SegKind::Plain)
                }
                .on_click(crate::command::Command::ZoomReset),
            );
        }
        if !score.tracks.is_empty() {
            right.push(Segment::new(
                progress(pane.cursor.bar, score.master_bars.len()),
                SegKind::ModeText,
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
        assert_eq!(texts(&s.right)[2..], ["Top", "1:1"]);
        assert_eq!(s.right[0].icon, Some(StatusIcon::Tempo));
        assert!(s.right[0].text.starts_with('='));
        for k in parse_keys("}}}i", &[]).unwrap() {
            app.handle_key(k);
        }
        let s = app.statusline(Some((42.0, 190.0)), true);
        assert_eq!(s.left[0].text, "INSERT");
        assert_eq!(s.right.last().unwrap().text, "4:1");
        assert!(s.right[1].text.contains("0:42/3:10"), "{}", s.right[1].text);
        // At 100% the zoom isn't shown.
        assert!(s.right.iter().all(|s| s.icon != Some(StatusIcon::Zoom)));
        // With an audio track, what plays shows as icons.
        let b = app.pane().buffer;
        app.buffers[b].score.audio = Some(Default::default());
        let mix = |app: &App| {
            app.statusline(None, false)
                .right
                .iter()
                .find_map(|s| s.icon.filter(|i| *i != StatusIcon::Tempo))
        };
        assert_eq!(mix(&app), Some(StatusIcon::SynthAndAudio));
        app.run(Command::MixMode(Some("audio".into())));
        assert_eq!(mix(&app), Some(StatusIcon::Audio));
        app.run(Command::MixMode(Some("midi".into())));
        assert_eq!(mix(&app), Some(StatusIcon::Synth));
        // A changed speed shows with its icon.
        app.config.file.audio.speed = 0.95;
        let s = app.statusline(None, false);
        let speed = s
            .right
            .iter()
            .find(|s| s.icon == Some(StatusIcon::Speed))
            .unwrap();
        assert_eq!(speed.text, "95%");
        // Clicks: speed back to 100%, the next layout, the next mix, the workspace picker.
        let click = |app: &mut App, icon: Option<StatusIcon>, text: Option<&str>| {
            let s = app.statusline(None, true);
            let seg = s
                .left
                .iter()
                .chain(&s.right)
                .find(|s| icon.is_some_and(|i| s.icon == Some(i)) || text == Some(s.text.as_str()))
                .unwrap()
                .clone();
            app.run(seg.action.unwrap());
        };
        click(&mut app, Some(StatusIcon::Speed), None);
        assert_eq!(app.config.file.audio.speed, 1.0);
        click(&mut app, None, Some("vertical"));
        assert_eq!(app.pane().layout, "page");
        click(&mut app, Some(StatusIcon::Synth), None);
        assert_ne!(mix(&app), Some(StatusIcon::Synth));
        app.zoom = 1.3;
        click(&mut app, Some(StatusIcon::Zoom), None);
        assert_eq!(app.zoom, 1.0);
        assert_eq!(progress(3, 8), "50%");
        assert_eq!(progress(7, 8), "Bot");
    }
}
