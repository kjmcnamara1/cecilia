//! Text tablature rendering for one pane.

use muse_app::{App, Cursor};
use muse_core::{Beat, DurationValue, Note, Score, Track, TrackKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

/// A clickable region mapped to a score position.
#[derive(Debug, Clone, Copy)]
pub struct Hit {
    pub area: Rect,
    pub pane: u32,
    pub cursor: Cursor,
}

/// Horizontal layout of one bar within a system.
struct BarLayout {
    bar: usize,
    x: u16,
    /// (x offset from bar start, width) per beat
    beats: Vec<(u16, u16)>,
    width: u16,
}

pub struct System {
    bars: Vec<BarLayout>,
}

/// Rows used by a system: header + strings + spacer.
pub fn system_height(track: &Track) -> u16 {
    track.strings.len().max(1) as u16 + 2
}

const LABEL_WIDTH: u16 = 3;

fn beat_width(beat: &Beat, scale: u16) -> u16 {
    let base: u16 = match beat.duration.value {
        DurationValue::Whole => 8,
        DurationValue::Half => 6,
        DurationValue::Quarter => 4,
        _ => 3,
    };
    let fret_w = beat
        .notes
        .iter()
        .map(|n| note_text(n).len() as u16)
        .max()
        .unwrap_or(1)
        + 1;
    (base * scale / 2 + beat.duration.dots as u16)
        .max(fret_w)
        .max(2)
}

/// Breaks a track into systems that fit `width` columns.
pub fn layout_systems(score: &Score, track: usize, width: u16, scale: u16) -> Vec<System> {
    let Some(t) = score.tracks.get(track) else {
        return vec![];
    };
    let avail = width.saturating_sub(LABEL_WIDTH + 1).max(8);
    let mut systems = vec![];
    let mut current = System { bars: vec![] };
    let mut x = 0;
    for (bi, bar) in t.bars.iter().enumerate() {
        let beats = bar.voices.first().map_or(&[][..], |v| &v.beats[..]);
        let mut offs = vec![];
        let mut bx = 1; // bar line
        for b in beats {
            let w = beat_width(b, scale);
            offs.push((bx, w));
            bx += w;
        }
        if beats.is_empty() {
            offs.push((1, 4 * scale / 2));
            bx += 4 * scale / 2;
        }
        let bw = bx.min(avail);
        if x + bw > avail && !current.bars.is_empty() {
            systems.push(std::mem::replace(&mut current, System { bars: vec![] }));
            x = 0;
        }
        current.bars.push(BarLayout {
            bar: bi,
            x,
            beats: offs,
            width: bw,
        });
        x += bw;
    }
    if !current.bars.is_empty() {
        systems.push(current);
    }
    systems
}

pub fn system_of_bar(systems: &[System], bar: usize) -> usize {
    systems
        .iter()
        .position(|s| s.bars.iter().any(|b| b.bar == bar))
        .unwrap_or(0)
}

fn note_text(n: &Note) -> String {
    let fret = if n.dead {
        "x".to_string()
    } else {
        n.fret.to_string()
    };
    let mut s = if n.tie || n.ghost {
        format!("({fret})")
    } else {
        fret
    };
    let fx = &n.effects;
    if fx.harmonic.is_some() {
        s = format!("<{s}>");
    }
    if fx.bend.is_some() {
        s.push('b');
    } else if fx.hammer_pull {
        s.push('h');
    } else if !fx.slides.is_empty() {
        s.push('/');
    } else if fx.vibrato {
        s.push('~');
    }
    s
}

fn pitch_name(midi: u8) -> &'static str {
    [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ][midi as usize % 12]
}

pub struct TabView<'a> {
    pub app: &'a App,
    pub pane: u32,
    pub focused: bool,
    /// (bar, tick) being played in this pane's buffer.
    pub play: Option<(usize, u32)>,
    pub hits: &'a mut Vec<Hit>,
    pub scroll: &'a mut usize,
}

impl Widget for TabView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let pane = &self.app.panes[&self.pane];
        let score = &self.app.buffers[pane.buffer].score;
        let Some(track) = score.tracks.get(pane.cursor.track) else {
            let msg = "No score loaded — open one with :e <file>";
            buf.set_string(
                area.x + 1,
                area.y,
                msg,
                Style::default().fg(Color::DarkGray),
            );
            return;
        };
        let scale = self.app.config.file.ui.tab_width.clamp(1, 8) as u16;
        let systems = layout_systems(score, pane.cursor.track, area.width, scale);
        let sh = system_height(track);
        let visible = (area.height / sh).max(1) as usize;
        let cursor_sys = system_of_bar(&systems, pane.cursor.bar);
        let mut scroll = (*self.scroll).min(systems.len().saturating_sub(1));
        if cursor_sys < scroll {
            scroll = cursor_sys;
        } else if cursor_sys >= scroll + visible {
            scroll = cursor_sys + 1 - visible;
        }
        *self.scroll = scroll;

        let dim = Style::default().fg(Color::DarkGray);
        let line_style = Style::default().fg(Color::Gray);
        let cursor_style = if self.focused {
            Style::default().fg(Color::Black).bg(Color::Yellow)
        } else {
            Style::default().fg(Color::Black).bg(Color::DarkGray)
        };
        let column_style = Style::default().bg(Color::Rgb(40, 40, 50));
        let play_style = Style::default().bg(Color::Rgb(20, 70, 40));

        for (row, system) in systems.iter().enumerate().skip(scroll).take(visible) {
            let y0 = area.y + ((row - scroll) as u16) * sh;
            if y0 + sh > area.y + area.height + 1 {
                break;
            }
            // String labels.
            for (s, &open) in track.strings.iter().enumerate() {
                let label = if track.kind == TrackKind::Drums {
                    ""
                } else {
                    pitch_name(open)
                };
                buf.set_string(area.x, y0 + 1 + s as u16, format!("{label:>2}"), dim);
            }
            let x0 = area.x + LABEL_WIDTH;
            for bl in &system.bars {
                let bx = x0 + bl.x;
                let mb = &score.master_bars[bl.bar];
                // Header: bar number, section marker, time signature changes.
                let mut header = format!("{}", bl.bar + 1);
                let prev_ts = bl
                    .bar
                    .checked_sub(1)
                    .map(|p| score.master_bars[p].time_signature);
                if prev_ts != Some(mb.time_signature) {
                    header.push_str(&format!(
                        " {}/{}",
                        mb.time_signature.numerator, mb.time_signature.denominator
                    ));
                }
                if let Some(m) = &mb.marker {
                    header.push_str(&format!(" [{}]", m.title));
                }
                let header: String = header.chars().take(bl.width as usize).collect();
                buf.set_string(bx, y0, header, Style::default().fg(Color::Cyan));
                // Staff lines and bar lines.
                for s in 0..track.strings.len() as u16 {
                    let y = y0 + 1 + s;
                    let bar_char = if mb.repeat_start { "‖" } else { "|" };
                    buf.set_string(bx, y, bar_char, line_style);
                    for dx in 1..bl.width {
                        buf.set_string(bx + dx, y, "-", dim);
                    }
                }
                let beats = track.bars[bl.bar]
                    .voices
                    .first()
                    .map_or(&[][..], |v| &v.beats[..]);
                let mut tick = 0;
                for (i, &(off, w)) in bl.beats.iter().enumerate() {
                    if off + w > bl.width + 1 {
                        break;
                    }
                    let beat = beats.get(i);
                    let bx_beat = bx + off;
                    let len = beat.map_or(0, |b| b.ticks());
                    let is_play = self.play.is_some_and(|(pb, pt)| {
                        pb == bl.bar && pt >= tick && (pt < tick + len || (len == 0 && pt == tick))
                    });
                    let is_cursor_beat = pane.cursor.bar == bl.bar && pane.cursor.beat == i;
                    for s in 0..track.strings.len() {
                        let y = y0 + 1 + s as u16;
                        let cell = Rect {
                            x: bx_beat,
                            y,
                            width: w,
                            height: 1,
                        };
                        self.hits.push(Hit {
                            area: cell,
                            pane: self.pane,
                            cursor: Cursor {
                                track: pane.cursor.track,
                                bar: bl.bar,
                                beat: i,
                                string: s,
                            },
                        });
                        if is_play {
                            buf.set_style(cell, play_style);
                        } else if is_cursor_beat {
                            buf.set_style(cell, column_style);
                        }
                        if let Some(n) =
                            beat.and_then(|b| b.notes.iter().find(|n| n.string as usize == s))
                        {
                            let style = if n.ghost || n.tie {
                                dim
                            } else {
                                Style::default().fg(Color::White)
                            };
                            buf.set_string(
                                bx_beat,
                                y,
                                note_text(n),
                                style.add_modifier(Modifier::BOLD),
                            );
                        }
                        if is_cursor_beat && pane.cursor.string == s {
                            let text_w = beat
                                .and_then(|b| b.notes.iter().find(|n| n.string as usize == s))
                                .map_or(1, |n| note_text(n).len() as u16);
                            buf.set_style(
                                Rect {
                                    x: bx_beat,
                                    y,
                                    width: text_w,
                                    height: 1,
                                },
                                cursor_style,
                            );
                        }
                    }
                    // Beat text above the staff, if room.
                    if let Some(text) = beat.and_then(|b| b.text.as_deref()) {
                        if bl.beats.len() > 1 || bl.width > 6 {
                            let t: String = text.chars().take(w as usize).collect();
                            buf.set_string(
                                bx_beat,
                                y0 + sh - 1,
                                t,
                                Style::default().fg(Color::Magenta),
                            );
                        }
                    }
                    tick += len;
                }
                // Closing repeat.
                if let Some(n) = mb.repeat_end {
                    let end_x = bx + bl.width;
                    for s in 0..track.strings.len() as u16 {
                        buf.set_string(end_x.saturating_sub(1), y0 + 1 + s, "‖", line_style);
                    }
                    buf.set_string(
                        end_x.saturating_sub(3),
                        y0,
                        format!("x{n}"),
                        Style::default().fg(Color::Cyan),
                    );
                }
            }
            // Final bar line of the system.
            if let Some(last) = system.bars.last() {
                let x = x0 + last.x + last.width;
                if x < area.x + area.width {
                    for s in 0..track.strings.len() as u16 {
                        buf.set_string(x, y0 + 1 + s, "|", line_style);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muse_core::*;

    #[test]
    fn systems_wrap_to_width() {
        let beat = Beat {
            notes: vec![Note::new(0, 12)],
            ..Beat::default()
        };
        let bar = Bar {
            voices: vec![Voice {
                beats: vec![beat; 4],
            }],
        };
        let track = Track {
            strings: vec![64; 6],
            bars: vec![bar; 10],
            ..Track::default()
        };
        let score = Score {
            master_bars: vec![MasterBar::default(); 10],
            tracks: vec![track],
            ..Score::default()
        };
        let systems = layout_systems(&score, 0, 60, 2);
        // Each bar: 1 + 4 * 4 = 17 columns; 56 available → 3 bars per system.
        assert_eq!(systems[0].bars.len(), 3);
        assert_eq!(systems.len(), 4);
        assert_eq!(system_of_bar(&systems, 9), 3);
    }

    #[test]
    fn note_markers() {
        let mut n = Note::new(0, 7);
        n.effects.hammer_pull = true;
        assert_eq!(note_text(&n), "7h");
        n.tie = true;
        assert_eq!(note_text(&n), "(7)h");
    }
}
