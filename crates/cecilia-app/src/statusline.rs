//! The status line's sections, as in lualine.nvim: on the left the mode (`a`), the pane layout
//! (`b`), the workspace and the file (`c`); on the right the rest of the state (`x`), the
//! position as a percentage (`y`) and `bar:beat` (`z`). Frontends draw them with the
//! separators chosen in `[statusline] separators`.

use crate::app::App;
use crate::config::{InsertKind, Mode};
use cecilia_core::{TrackKind, chant, staff};

const SOLFEGE: [&str; 7] = ["do", "re", "mi", "fa", "sol", "la", "ti"];

/// The clef in force at a bar and beat of a chant track: that of the last note at or before it.
fn clef_at(chant: &chant::Chant, bar: usize, beat: usize) -> chant::ChantClef {
    let t = chant::transcribe(chant);
    let notes = chant.resolved();
    t.positions
        .iter()
        .zip(&notes)
        .take_while(|(p, _)| **p <= (bar, beat))
        .last()
        .map_or_else(|| chant.initial_clef(), |(_, n)| n.clef)
}

/// How a segment is colored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegKind {
    /// The mode's color behind, text that reads on it (sections `a` and `z`).
    Mode,
    /// The section-b color (`status_b`) behind, the mode's color as text (sections `b`, `y`).
    ModeText,
    /// The status line's own color (sections `c` and `x`).
    Plain,
    /// Plain, but in the error color: a bar that is too short or too long.
    Warn,
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
        Mode::Properties => "PROPS",
        Mode::Palette => "PALETTE",
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
    /// Where the cursor is across the staff: a string, a staff position (as the note a
    /// letter would enter there), a slash staff or the solfège syllable of a chant
    /// position under the clef in force there.
    fn cursor_place(&self) -> Option<String> {
        let c = self.pane().cursor;
        let score = &self.buffer().score;
        let track = score.tracks.get(c.track)?;
        Some(match self.insert_kind() {
            InsertKind::Tab if track.kind == TrackKind::Drums => return None,
            InsertKind::Tab => format!("string {}", c.string + 1),
            InsertKind::Slash => "slash".into(),
            InsertKind::Chant => {
                let clef = match track.chant.as_deref() {
                    Some(ch) => clef_at(ch, c.bar, c.beat),
                    None => crate::edit::chant_clef_of(score, track, self.chord_policy()).0,
                };
                SOLFEGE[clef.degree(c.line.clamp(0, 13)) as usize].into()
            }
            InsertKind::Standard => {
                let d = c.line;
                let key = score
                    .master_bars
                    .get(c.bar)
                    .map_or(0, |m| m.key_signature.accidentals);
                let step = d.rem_euclid(7) as usize;
                let sign = match staff::key_alters(key)[step] {
                    1 => "#",
                    -1 => "b",
                    _ => "",
                };
                format!(
                    "{}{sign}{}",
                    "CDEFGAB".as_bytes()[step] as char,
                    d.div_euclid(7)
                )
            }
        })
    }

    /// The mode as shown: typing into a property of the properties pane is insert mode (the
    /// lyrics text area has normal mode too).
    pub fn display_mode(&self) -> Mode {
        match &self.props.edit {
            // The lyrics text area has vim's normal mode too.
            Some(e) if e.modal && !e.insert => Mode::Normal,
            Some(_) => Mode::Insert,
            None => self.mode,
        }
    }

    /// The status line. `play` is the playing time and length (from the frontend's player),
    /// `zoom` whether to show the zoom level (GUI).
    pub fn statusline(&self, play: Option<(f64, f64)>, zoom: bool) -> StatusLine {
        let pane = self.pane();
        let buffer = self.buffer();
        let score = &buffer.score;
        let mut left = vec![
            Segment::new(mode_label(self.display_mode()), SegKind::Mode),
            // Click: the next layout.
            Segment::new(pane.layout.clone(), SegKind::ModeText)
                .on_click(crate::command::Command::Layout(None)),
        ];
        if self.design_active() {
            left.push(Segment::new("design", SegKind::ModeText).on_click(
                crate::command::Command::Design(crate::design::DesignCmd::Mode(Some(false))),
            ));
        }
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
        if let Some(place) = self.cursor_place() {
            x.push(place);
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
        // The cursor's bar, in quarter-note beats of the voice under the cursor against what
        // the time signature asks for (`3.0/4.0`); error-colored unless the bar is complete.
        // Nothing in free time and in a pickup bar.
        {
            let c = pane.cursor;
            if let Some(count) = score.bar_count(c.track, c.bar, c.voice) {
                let kind = if count.is_complete() {
                    SegKind::Plain
                } else {
                    SegKind::Warn
                };
                right.push(Segment::new(count.label(), kind));
            }
        }
        // The zoom, while it isn't 100%.
        if zoom && (self.zoom() - 1.0).abs() > 1e-6 {
            right.push(
                Segment {
                    icon: Some(StatusIcon::Zoom),
                    ..Segment::new(format!("{:.0}%", self.zoom() * 100.0), SegKind::Plain)
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
        assert_eq!(texts(&s.right)[2..], ["0.0/4.0", "Top", "1:1"]);
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
        app.pane_mut().zoom = 1.3;
        click(&mut app, Some(StatusIcon::Zoom), None);
        assert_eq!(app.zoom(), 1.0);
        assert_eq!(progress(3, 8), "50%");
        assert_eq!(progress(7, 8), "Bot");
    }

    #[test]
    fn the_status_line_shows_the_bar_count() {
        use cecilia_core::{Beat, Duration, DurationValue};
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        let count = |app: &App| {
            let s = app.statusline(None, false);
            s.right
                .iter()
                .find(|s| s.text.contains('/') && s.text.contains('.'))
                .map(|s| (s.text.clone(), s.kind))
        };
        // In normal mode too.
        assert_eq!(count(&app), Some(("0.0/4.0".into(), SegKind::Warn)));
        app.run(Command::parse("insert-here").unwrap());
        assert_eq!(app.mode, Mode::Insert);
        assert_eq!(count(&app), Some(("0.0/4.0".into(), SegKind::Warn)));
        let b = app.pane().buffer;
        let quarter = |n: usize| {
            let beat = Beat {
                duration: Duration::new(DurationValue::Quarter),
                ..Beat::default()
            };
            vec![beat; n]
        };
        let fill = |app: &mut App, n: usize| {
            app.buffers[b].score.tracks[0].bars[0].voices[0].beats = quarter(n);
        };
        fill(&mut app, 3);
        assert_eq!(count(&app), Some(("3.0/4.0".into(), SegKind::Warn)));
        fill(&mut app, 4);
        assert_eq!(count(&app), Some(("4.0/4.0".into(), SegKind::Plain)));
        fill(&mut app, 5);
        assert_eq!(count(&app), Some(("5.0/4.0".into(), SegKind::Warn)));
        // The voice under the cursor counts.
        app.buffers[b].score.tracks[0].bars[0]
            .voices
            .push(Default::default());
        app.buffers[b].score.tracks[0].bars[0].voices[1].beats = quarter(4);
        app.pane_mut().cursor.voice = 1;
        assert_eq!(count(&app), Some(("4.0/4.0".into(), SegKind::Plain)));
        // 6/8 is three quarter beats; free time shows nothing.
        app.pane_mut().cursor.voice = 0;
        app.buffers[b].score.master_bars[0].time_signature.numerator = 6;
        app.buffers[b].score.master_bars[0]
            .time_signature
            .denominator = 8;
        assert_eq!(count(&app), Some(("5.0/3.0".into(), SegKind::Warn)));
        app.buffers[b].score.master_bars[0].free_time = true;
        assert_eq!(count(&app), None);
    }

    #[test]
    fn zoom_is_per_pane() {
        let mut app = App::new(Config::default(), None);
        let first = app.focused();
        app.run(Command::Split);
        let second = app.focused();
        assert_ne!(first, second);
        app.zoom_pane(first, 2.0);
        assert_eq!(app.panes[&first].zoom, 2.0);
        assert_eq!(app.panes[&second].zoom, 1.0);
    }

    fn place(app: &App) -> String {
        app.cursor_place().unwrap()
    }

    fn chant_app(gabc: &str) -> App {
        let mut app = App::new(Config::default(), None);
        let chant = cecilia_formats::gabc::parse(gabc).unwrap();
        app.buffers[0].score = cecilia_formats::gabc::chant_score(chant);
        app.mode = Mode::Insert;
        app
    }

    #[test]
    fn chant_place_is_the_solfege_syllable() {
        // C clef on line 4: do is the line, position 9 (j).
        let mut app = chant_app("name: t;\n%%\n(c4) Al(fg)le(h) lu(g)ia.(f.) (::)\n");
        let names = |app: &mut App, lines: &[i32]| -> Vec<String> {
            lines
                .iter()
                .map(|&l| {
                    app.pane_mut().cursor.line = l;
                    place(app)
                })
                .collect()
        };
        assert_eq!(
            names(&mut app, &[9, 10, 11, 12, 13, 7, 8, 0]),
            ["do", "re", "mi", "fa", "sol", "la", "ti", "la"]
        );
        // Fa clef on line 3: fa is position 7, so do is position 11.
        let mut app = chant_app("name: t;\n%%\n(f3) Al(fg)le(h) lu(g)ia.(f.) (::)\n");
        assert_eq!(names(&mut app, &[11, 7, 13, 3]), ["do", "fa", "mi", "ti"]);
    }

    #[test]
    fn chant_place_follows_clef_changes() {
        let mut app = chant_app("name: t;\n%%\n(c4) Al(f) le(g) (::) lu(c3) ia.(g) (::)\n");
        app.pane_mut().cursor.line = 9;
        assert_eq!(place(&app), "do");
        // On the note after the change to c3 (do on position 7) position 9 is mi.
        let last = app.buffer().score.tracks[0].bars.len() - 1;
        {
            let c = &mut app.pane_mut().cursor;
            c.bar = last;
            c.beat = 0;
        }
        assert_eq!(place(&app), "mi");
    }

    #[test]
    fn chant_staff_of_an_ordinary_track_uses_the_fitted_clef() {
        let mut app = App::new(Config::default(), None);
        app.run(crate::Command::New);
        app.run(crate::Command::StaffAdd(Some("chant".into())));
        app.run(crate::Command::StaffNext);
        app.mode = Mode::Insert;
        assert_eq!(app.insert_kind(), InsertKind::Chant);
        let (clef, _) = crate::edit::chant_clef_of(
            &app.buffer().score,
            &app.buffer().score.tracks[0],
            app.chord_policy(),
        );
        for line in [2, 5, 9, 13] {
            app.pane_mut().cursor.line = line;
            assert_eq!(place(&app), SOLFEGE[clef.degree(line) as usize]);
        }
    }
}
