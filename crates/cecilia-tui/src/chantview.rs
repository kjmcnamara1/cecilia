//! Chant in the terminal: a text staff (one row per staff position, lines drawn with `─`),
//! neumes as block characters and the syllables underneath.

use crate::rc;
use cecilia_app::{App, Cursor};
use cecilia_core::chant::{
    Chant, ChantClef, ClefKind, Division, Liquescence, NeumeItem, Shape, transcribe,
};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;

use crate::tabview::Hit;

/// Highest and lowest staff positions shown (4-line staff: lines at 3, 5, 7, 9).
const TOP: i32 = 11;
const BOTTOM: i32 = 1;

/// Rows used by one line of chant: the staff positions, the text and a spacer.
pub fn system_height() -> u16 {
    (TOP - BOTTOM + 1) as u16 + 2
}

enum Mark {
    Note {
        ch: char,
        pos: i32,
        /// (bar, beat, beats sounded) in the transcription.
        beat: Option<(usize, usize, u8)>,
        mora: u8,
    },
    Flat(i32, char),
    Division(Division),
    Clef(ChantClef),
}

struct Syl {
    index: usize,
    /// (column, mark) relative to the syllable start.
    marks: Vec<(u16, Mark)>,
    width: u16,
    text: String,
    /// The next syllable continues the word.
    hyphen: bool,
}

fn note_char(n: &cecilia_core::chant::ChantNote, last_in_glyph: bool) -> char {
    if last_in_glyph && n.liquescence() == Liquescence::Deminutus {
        return '▪';
    }
    match n.shape() {
        Shape::Inclinatum => '◆',
        Shape::Quilisma => '◊',
        Shape::Oriscus(_) => '≈',
        Shape::Stropha(_) => '▾',
        Shape::Virga | Shape::VirgaReversa | Shape::Bivirga(_) => '▮',
        _ => '■',
    }
}

fn layout(chant: &Chant) -> Vec<Syl> {
    let t = transcribe(chant);
    let mut beats = std::collections::HashMap::new();
    for (k, r) in chant.resolved().iter().enumerate() {
        if let (Some(&(bar, beat)), NeumeItem::Note(n)) = (
            t.positions.get(k),
            &chant.syllables[r.syllable].items[r.item],
        ) {
            beats.insert((r.syllable, r.item), (bar, beat, n.repeats()));
        }
    }
    let mut out = Vec::new();
    for (si, s) in chant.syllables.iter().enumerate() {
        let mut marks = Vec::new();
        let mut col: u16 = 0;
        let mut gap = false;
        for (ii, item) in s.items.iter().enumerate() {
            match item {
                NeumeItem::Note(n) => {
                    if gap && col > 0 {
                        col += 1;
                    }
                    gap = false;
                    let last = !matches!(s.items.get(ii + 1), Some(NeumeItem::Note(_)));
                    for r in 0..n.repeats() as u16 {
                        marks.push((
                            col + r,
                            Mark::Note {
                                ch: note_char(n, last),
                                pos: n.pitch as i32,
                                beat: beats.get(&(si, ii)).copied().filter(|_| r == 0),
                                mora: n.mora(),
                            },
                        ));
                    }
                    col += n.repeats() as u16 + n.mora() as u16;
                }
                NeumeItem::Space(sp) if !sp.trim_start_matches('!').is_empty() => gap = true,
                NeumeItem::Alteration { pitch, sign } => {
                    let ch = match sign.chars().next() {
                        Some('x' | 'X') => '♭',
                        Some('#') => '♯',
                        _ => '♮',
                    };
                    marks.push((col, Mark::Flat(*pitch as i32, ch)));
                    col += 1;
                }
                NeumeItem::Bar(b) => {
                    if col > 0 {
                        col += 1;
                    }
                    marks.push((col, Mark::Division(Division::of(b))));
                    col += 1;
                }
                NeumeItem::Clef(c) if si > 0 || col > 0 => {
                    marks.push((col, Mark::Clef(*c)));
                    col += 2;
                }
                _ => {}
            }
        }
        let text = s.plain_text().trim().to_string();
        let hyphen = !text.is_empty()
            && chant
                .syllables
                .get(si + 1)
                .is_some_and(|n| !n.word_start && !n.plain_text().trim().is_empty());
        let text_w = text.chars().count() as u16 + u16::from(hyphen);
        if marks.is_empty() && text.is_empty() {
            continue;
        }
        out.push(Syl {
            index: si,
            width: col.max(text_w) + 1,
            marks,
            text,
            hyphen,
        });
    }
    out
}

pub struct ChantView<'a> {
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

impl Widget for ChantView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let th = &self.app.config.theme;
        let pane = &self.app.panes[&self.pane];
        let score = &self.app.buffers[pane.buffer].score;
        let Some(chant) = score
            .tracks
            .get(pane.cursor.track)
            .and_then(|t| t.chant.as_deref())
        else {
            return;
        };
        let track = &score.tracks[pane.cursor.track];
        let syls = layout(chant);
        // Break into lines after a 3-column clef margin.
        let avail = area.width.saturating_sub(4).max(10);
        let mut lines: Vec<Vec<(u16, &Syl)>> = vec![Vec::new()];
        let mut x = 0;
        for s in &syls {
            if x + s.width > avail && !lines.last().unwrap().is_empty() {
                lines.push(Vec::new());
                x = 0;
            }
            lines.last_mut().unwrap().push((x, s));
            x += s.width;
        }
        // The beat sounding now, as (bar, beat).
        let playing = self.play.and_then(|(bar, tick)| {
            let beats = &track.bars.get(bar)?.voices.first()?.beats;
            let mut t = 0;
            beats
                .iter()
                .position(|b| {
                    let hit = tick >= t && tick < t + b.ticks();
                    t += b.ticks();
                    hit
                })
                .map(|i| (bar, i))
        });

        // The line to keep in view: the playhead's while following playback, else the cursor's.
        let c = pane.cursor;
        let focus = match playing.filter(|_| self.app.follow) {
            Some(p) => p,
            None => (c.bar, c.beat),
        };
        let cursor_line = lines
            .iter()
            .rposition(|l| {
                l.iter().flat_map(|(_, s)| &s.marks).any(|(_, m)| {
                    matches!(m, Mark::Note { beat: Some((b, i, _)), .. } if (*b, *i) <= focus)
                })
            })
            .unwrap_or(0);
        let sh = system_height();
        let visible = (area.height / sh).max(1) as usize;
        let mut scroll = (*self.scroll).min(lines.len().saturating_sub(1));
        if let Some(to) = self.app.panes[&self.pane].scroll_request {
            scroll = crate::tabview::scroll_for(to, cursor_line, visible);
        } else if !self.follow {
            scroll = scroll.min(lines.len().saturating_sub(visible));
        } else if cursor_line < scroll {
            scroll = cursor_line;
        } else if cursor_line >= scroll + visible {
            scroll = cursor_line + 1 - visible;
        }
        *self.scroll = scroll;
        *self.extent = (lines.len(), visible);

        let dim = Style::default().fg(rc(th.dim));
        let ink = Style::default()
            .fg(rc(th.chrome_text))
            .add_modifier(Modifier::BOLD);
        let on = |bg| Style::default().fg(rc(th.text_on(bg))).bg(rc(bg));
        let cursor_style = if self.focused {
            on(th.mode_color(self.app.mode))
        } else {
            on(th.dim)
        };
        let play_style = Style::default().bg(rc(th.playhead));
        let mut clef = chant.initial_clef();
        for (li, line) in lines.iter().enumerate() {
            let visible_line = li >= scroll && li < scroll + visible;
            let y0 = area.y + (li.saturating_sub(scroll) as u16) * sh;
            if visible_line && y0 + sh <= area.y + area.height + 1 {
                let row = |p: i32| y0 + (TOP - p.clamp(BOTTOM, TOP)) as u16;
                for l in [3, 5, 7, 9] {
                    for dx in 0..area.width.min(4 + x.max(avail)) {
                        buf.set_string(area.x + dx, row(l), "─", dim);
                    }
                }
                let letter = if clef.kind == ClefKind::Do { "C" } else { "F" };
                buf.set_string(
                    area.x,
                    row(clef.position()),
                    letter,
                    Style::default().fg(rc(th.marker)),
                );
                if clef.flat {
                    buf.set_string(
                        area.x + 1,
                        row(clef.position() - 1),
                        "♭",
                        Style::default().fg(rc(th.marker)),
                    );
                }
                let x0 = area.x + 3;
                for (sx, s) in line {
                    let base = x0 + sx;
                    for (dx, m) in &s.marks {
                        let x = base + dx;
                        if x >= area.x + area.width {
                            continue;
                        }
                        match m {
                            Mark::Note {
                                ch,
                                pos,
                                beat,
                                mora,
                            } => {
                                let y = row(*pos);
                                if let Some((bar, b, repeats)) = beat {
                                    let column = Rect {
                                        x,
                                        y: y0,
                                        width: 1,
                                        height: (TOP - BOTTOM + 2) as u16,
                                    };
                                    let here = |bb: usize| (c.bar, c.beat) == (*bar, bb);
                                    let on_cursor = (0..*repeats as usize).any(|r| here(b + r))
                                        && !self.app.cursor_hidden(pane.buffer);
                                    let on_play = (0..*repeats as usize)
                                        .any(|r| playing == Some((*bar, b + r)));
                                    if on_play {
                                        buf.set_style(column, play_style);
                                    }
                                    self.hits.push(Hit {
                                        area: column,
                                        pane: self.pane,
                                        cursor: Cursor {
                                            track: c.track,
                                            bar: *bar,
                                            beat: *b,
                                            string: 0,
                                            voice: 0,
                                            staff: Cursor::HOME,
                                            line: 0,
                                        },
                                    });
                                    buf.set_string(
                                        x,
                                        y,
                                        ch.to_string(),
                                        if on_cursor { cursor_style } else { ink },
                                    );
                                } else {
                                    buf.set_string(x, y, ch.to_string(), ink);
                                }
                                if *mora > 0 {
                                    buf.set_string(x + 1, y, "·".repeat(*mora as usize), ink);
                                }
                            }
                            Mark::Flat(pos, ch) => {
                                buf.set_string(x, row(*pos), ch.to_string(), ink)
                            }
                            Mark::Division(d) => {
                                let (from, to) = match d {
                                    Division::Virgula | Division::Minima => (9, 8),
                                    Division::Minor => (7, 5),
                                    _ => (9, 3),
                                };
                                let glyph = if *d == Division::Finalis {
                                    "‖"
                                } else {
                                    "│"
                                };
                                for p in (to..=from).rev() {
                                    buf.set_string(
                                        x,
                                        row(p),
                                        glyph,
                                        Style::default().fg(rc(th.chrome_text.mix(th.dim, 0.5))),
                                    );
                                }
                            }
                            Mark::Clef(cl) => {
                                let letter = if cl.kind == ClefKind::Do { "C" } else { "F" };
                                buf.set_string(
                                    x,
                                    row(cl.position()),
                                    letter,
                                    Style::default().fg(rc(th.marker)),
                                );
                            }
                        }
                    }
                    let mut text = s.text.clone();
                    if s.hyphen {
                        text.push('-');
                    }
                    let room = (area.x + area.width).saturating_sub(base) as usize;
                    let text: String = text.chars().take(room).collect();
                    let style = if chant.syllables[s.index].italic() {
                        Style::default()
                            .fg(rc(th.chrome_text.mix(th.dim, 0.5)))
                            .add_modifier(Modifier::ITALIC)
                    } else {
                        Style::default().fg(rc(th.chrome_text.mix(th.dim, 0.5)))
                    };
                    buf.set_string(base, y0 + (TOP - BOTTOM + 1) as u16, text, style);
                }
            }
            for (_, s) in line {
                for (_, m) in &s.marks {
                    if let Mark::Clef(c) = m {
                        clef = *c;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syllables_get_columns_for_notes_and_text() {
        let chant = cecilia_formats_parse("(c4) Al(fg/h)le(h.) lu(Gs)ia.(f) (::)");
        let syls = layout(&chant);
        assert_eq!(syls[0].text, "Al");
        assert!(syls[0].hyphen);
        // f, g, a gap, h.
        let cols: Vec<u16> = syls[0].marks.iter().map(|(c, _)| *c).collect();
        assert_eq!(cols, [0, 1, 3]);
        assert!(matches!(syls[1].marks[0].1, Mark::Note { mora: 1, .. }));
        assert!(matches!(
            syls.last().unwrap().marks[0].1,
            Mark::Division(Division::Finalis)
        ));
    }

    fn cecilia_formats_parse(body: &str) -> Chant {
        let score_text = format!("name: t;\n%%\n{body}\n");
        cecilia_formats::gabc::parse(&score_text).unwrap()
    }
}
