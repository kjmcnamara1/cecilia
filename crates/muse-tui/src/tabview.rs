//! Text tablature rendering for one pane.

use crate::rc;
use muse_app::{App, Cursor};
use muse_core::{Beat, DurationValue, Note, Score, Track, TrackKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
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

/// Rows used by a system: header + strings + one row per lyric verse + spacer.
pub fn system_height(track: &Track) -> u16 {
    track.strings.len().max(1) as u16 + 2 + verses(track)
}

/// Number of lyric verses on a track's first voice.
fn verses(track: &Track) -> u16 {
    track
        .bars
        .iter()
        .flat_map(|b| b.voices.first())
        .flat_map(|v| &v.beats)
        .map(|b| {
            b.lyrics
                .iter()
                .rposition(|l| !l.is_empty())
                .map_or(0, |i| i + 1)
        })
        .max()
        .unwrap_or(0)
        .min(5) as u16
}

const LABEL_WIDTH: u16 = 3;

fn beat_width(beat: &Beat, track: &Track, scale: u16) -> u16 {
    let base: u16 = match beat.duration.value {
        DurationValue::Whole => 8,
        DurationValue::Half => 6,
        DurationValue::Quarter => 4,
        _ => 3,
    };
    let fret_w = beat
        .notes
        .iter()
        .map(|n| note_text(n, track).len() as u16)
        .max()
        .unwrap_or(1)
        + 1;
    // Wide enough for the beat's syllables too.
    let lyric_w = beat
        .lyrics
        .iter()
        .map(|l| l.chars().count() as u16 + 1)
        .max()
        .unwrap_or(0);
    (base * scale / 2 + beat.duration.dots as u16)
        .max(fret_w)
        .max(lyric_w)
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
            let w = beat_width(b, t, scale);
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

/// The rows of bars a pane of `area` shows from system `scroll`, for half-page movement.
pub fn view_info(
    score: &Score,
    track: usize,
    area: Rect,
    scale: u16,
    scroll: usize,
) -> muse_app::ViewInfo {
    let Some(t) = score.tracks.get(track) else {
        return Default::default();
    };
    let systems = layout_systems(score, track, area.width, scale);
    let visible = (area.height / system_height(t)).max(1) as usize;
    let rows: Vec<std::ops::Range<usize>> = systems
        .iter()
        .map(|s| s.bars.first().map_or(0, |b| b.bar)..s.bars.last().map_or(0, |b| b.bar + 1))
        .collect();
    let shown = rows.iter().skip(scroll).take(visible);
    muse_app::ViewInfo {
        horizontal: false,
        visible_rows: shown.clone().count(),
        visible_bars: shown.map(|r| r.len()).sum(),
        rows,
        page_rows: 0,
    }
}

pub fn system_of_bar(systems: &[System], bar: usize) -> usize {
    systems
        .iter()
        .position(|s| s.bars.iter().any(|b| b.bar == bar))
        .unwrap_or(0)
}

/// Tab line a note is shown on: its string, or on drum tracks its instrument's lane.
fn row_of(n: &Note, track: &Track) -> usize {
    if track.kind == TrackKind::Drums {
        muse_core::drums::lane(n.fret) as usize
    } else {
        n.string as usize
    }
}

fn note_text(n: &Note, track: &Track) -> String {
    if track.kind == TrackKind::Drums {
        let c = if muse_core::drums::is_cymbal(n.fret) {
            "x"
        } else {
            "o"
        };
        let c = if n.accent != muse_core::Accent::None {
            c.to_uppercase()
        } else {
            c.to_string()
        };
        return if n.ghost { format!("({c})") } else { c };
    }
    let fret = if n.dead {
        "x".to_string()
    } else if matches!(track.kind, TrackKind::Pitched | TrackKind::Chant) {
        format!("{}{}", pitch_name(n.fret), n.fret as i32 / 12 - 1)
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
    /// Set to (lines in all, lines shown) for the scrollbar.
    pub extent: &'a mut (usize, usize),
    /// Keep the cursor (or playhead) in view; off after the scrollbar moved the view by hand.
    pub follow: bool,
}

impl Widget for TabView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let th = &self.app.config.theme;
        let pane = &self.app.panes[&self.pane];
        let score = &self.app.buffers[pane.buffer].score;
        let Some(track) = score.tracks.get(pane.cursor.track) else {
            let msg = "No score loaded — open one with :open <file>";
            buf.set_string(area.x + 1, area.y, msg, Style::default().fg(rc(th.dim)));
            return;
        };
        let scale = self.app.config.file.ui.tab_width.clamp(1, 8) as u16;
        let systems = layout_systems(score, pane.cursor.track, area.width, scale);
        let sh = system_height(track);
        let rows = track.strings.len().max(1);
        let visible = (area.height / sh).max(1) as usize;
        // Follow the playhead while playing (if following), otherwise the cursor.
        let focus_bar = match self.play.filter(|_| self.app.follow) {
            Some((bar, _)) => bar,
            None => pane.cursor.bar,
        };
        let cursor_sys = system_of_bar(&systems, focus_bar);
        let mut scroll = (*self.scroll).min(systems.len().saturating_sub(1));
        if !self.follow {
            scroll = scroll.min(systems.len().saturating_sub(visible));
        } else if cursor_sys < scroll {
            scroll = cursor_sys;
        } else if cursor_sys >= scroll + visible {
            scroll = cursor_sys + 1 - visible;
        }
        *self.scroll = scroll;
        *self.extent = (systems.len(), visible);

        let dim = Style::default().fg(rc(th.dim));
        let line_style = Style::default().fg(rc(th.chrome_text.mix(th.dim, 0.6)));
        let on = |bg| Style::default().fg(rc(th.text_on(bg))).bg(rc(bg));
        let cursor_style = if self.focused {
            on(th.mode_color(self.app.mode))
        } else {
            on(th.dim)
        };
        let playhead_style = on(th.playhead);

        for (row, system) in systems.iter().enumerate().skip(scroll).take(visible) {
            let y0 = area.y + ((row - scroll) as u16) * sh;
            if y0 + sh > area.y + area.height + 1 {
                break;
            }
            // String labels.
            for (s, &open) in track.strings.iter().enumerate() {
                let label = if track.kind == TrackKind::Drums {
                    muse_core::drums::LANES.get(s).copied().unwrap_or("")
                } else {
                    pitch_name(open)
                };
                buf.set_string(area.x, y0 + 1 + s as u16, format!("{label:>2}"), dim);
            }
            if track.strings.is_empty() {
                buf.set_string(area.x, y0 + 1, " ♪", dim);
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
                buf.set_string(bx, y0, header, Style::default().fg(rc(th.marker)));
                // Staff lines and bar lines; a bar without its full count has red staff lines
                // when `ui.highlight_incomplete_bars` is on.
                let staff_style = if self.app.highlight_incomplete_bars
                    && score.bar_incomplete(pane.cursor.track, bl.bar)
                {
                    Style::default().fg(rc(th.incomplete_bar))
                } else {
                    dim
                };
                for s in 0..rows as u16 {
                    let y = y0 + 1 + s;
                    let bar_char = if mb.repeat_start { "‖" } else { "|" };
                    buf.set_string(bx, y, bar_char, line_style);
                    for dx in 1..bl.width {
                        buf.set_string(bx + dx, y, "-", staff_style);
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
                    // Column of the playhead inside this beat, in proportion to time.
                    let playhead = self.play.and_then(|(pb, pt)| {
                        (pb == bl.bar
                            && pt >= tick
                            && (pt < tick + len || (len == 0 && pt == tick)))
                            .then(|| {
                                let frac = (pt - tick) as f64 / len.max(1) as f64;
                                bx_beat + (frac * w as f64) as u16
                            })
                    });
                    let is_cursor_beat = pane.cursor.bar == bl.bar && pane.cursor.beat == i;
                    let selected = self.focused
                        && self.app.selection().is_some_and(|(a, c, bars)| {
                            let (lo, hi) = if (a.bar, a.beat) <= (c.bar, c.beat) {
                                (a, c)
                            } else {
                                (c, a)
                            };
                            if bars {
                                (lo.bar..=hi.bar).contains(&bl.bar)
                            } else {
                                (lo.bar, lo.beat) <= (bl.bar, i) && (bl.bar, i) <= (hi.bar, hi.beat)
                            }
                        });
                    for s in 0..rows {
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
                                voice: 0,
                                staff: Cursor::HOME,
                                line: 0,
                            },
                        });
                        if selected {
                            buf.set_style(
                                cell,
                                Style::default().bg(rc(th.selection.mix(th.chrome, 0.5))),
                            );
                        }
                        if let Some(n) =
                            beat.and_then(|b| b.notes.iter().find(|n| row_of(n, track) == s))
                        {
                            let style = if n.ghost || n.tie {
                                dim
                            } else {
                                Style::default().fg(rc(th.chrome_text))
                            };
                            buf.set_string(
                                bx_beat,
                                y,
                                note_text(n, track),
                                style.add_modifier(Modifier::BOLD),
                            );
                        }
                        if is_cursor_beat
                            && pane.cursor.string == s
                            && !self.app.cursor_hidden(pane.buffer)
                        {
                            let text_w = beat
                                .and_then(|b| b.notes.iter().find(|n| row_of(n, track) == s))
                                .map_or(1, |n| note_text(n, track).len() as u16);
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
                    // Lyrics under the staff, one row per verse.
                    for (li, syl) in beat
                        .map_or(&[][..], |b| &b.lyrics[..])
                        .iter()
                        .enumerate()
                        .take(5)
                    {
                        if !syl.is_empty() {
                            let room = (area.x + area.width).saturating_sub(bx_beat) as usize;
                            let t: String = syl.chars().take(room).collect();
                            buf.set_string(
                                bx_beat,
                                y0 + 1 + rows as u16 + li as u16,
                                t,
                                Style::default().fg(rc(th.lyrics)),
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
                                Style::default().fg(rc(th.marker)),
                            );
                        }
                    }
                    if let Some(px) = playhead.filter(|&x| x < area.x + area.width) {
                        for s in 0..rows as u16 {
                            let cell = Rect {
                                x: px,
                                y: y0 + 1 + s,
                                width: 1,
                                height: 1,
                            };
                            buf.set_style(cell, playhead_style);
                        }
                    }
                    tick += len;
                }
                // Closing repeat.
                if let Some(n) = mb.repeat_end {
                    let end_x = bx + bl.width;
                    for s in 0..rows as u16 {
                        buf.set_string(end_x.saturating_sub(1), y0 + 1 + s, "‖", line_style);
                    }
                    buf.set_string(
                        end_x.saturating_sub(3),
                        y0,
                        format!("x{n}"),
                        Style::default().fg(rc(th.marker)),
                    );
                }
            }
            // Final bar line of the system.
            if let Some(last) = system.bars.last() {
                let x = x0 + last.x + last.width;
                if x < area.x + area.width {
                    for s in 0..rows as u16 {
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
        assert_eq!(note_text(&n, &Track::default()), "7h");
        n.tie = true;
        assert_eq!(note_text(&n, &Track::default()), "(7)h");
    }
}
