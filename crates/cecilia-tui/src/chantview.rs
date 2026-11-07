//! Chant in the terminal: square notation on a four-line text staff.
//!
//! One row per staff position (lines at 3, 5, 7, 9 drawn with `─`, the spaces and the ledger
//! region between), a clef at the left, neumes as one glyph per note (ligatures joined by a
//! stroke), divisions, accidentals and custodes, and the syllables under their neumes with
//! hyphens and extenders. Lines wrap to the pane like the vertical and page layouts of the tab
//! view (the page layout boxes groups of lines into pages); the horizontal layout is one line
//! scrolled by syllable.
//!
//! The layout is pure and unit-tested: [`layout`] places each syllable's marks in columns,
//! [`wrap`] and [`line_from`] break them into lines, [`scroll_syllables`] scrolls the horizontal
//! line.

use crate::rc;
use crate::tabview::{self, Hit, System};
use cecilia_app::{App, Cursor, ScrollTo};
use cecilia_core::Track;
use cecilia_core::chant::{
    Chant, ChantClef, ChantNote, ClefKind, Division, Liquescence, NeumeItem, Shape, group_neumes,
    transcribe,
};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;
use std::collections::{HashMap, HashSet};

/// Columns of the clef margin at the left of every line.
pub const CLEF_W: u16 = 3;

/// The staff positions a chant is drawn over, bottom to top. The staff lines are 3, 5, 7 and 9;
/// there is a space of margin each side, more where the notes go beyond it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub low: i32,
    pub top: i32,
}

impl Range {
    pub fn rows(self) -> u16 {
        (self.top - self.low + 1) as u16
    }

    /// The row (from the top) of a staff position.
    pub fn row(self, position: i32) -> u16 {
        (self.top - position.clamp(self.low, self.top)) as u16
    }
}

/// The positions to draw for `chant`: at least 2 to 10, and every note it has.
pub fn staff_range(chant: &Chant) -> Range {
    let (mut low, mut top) = (2, 10);
    for n in chant.syllables.iter().flat_map(|s| s.notes()) {
        low = low.min(n.pitch as i32);
        top = top.max(n.pitch as i32);
    }
    Range { low, top }
}

/// Rows used by one line of chant: the staff positions, the text and a spacer.
pub fn system_height(chant: &Chant) -> u16 {
    staff_range(chant).rows() + 2
}

/// Whether the chant view draws `track`: a chant track in square notation.
pub fn shows(_app: &App, track: &Track) -> bool {
    track.chant.is_some()
}

/// A staff line: the odd positions 3 to 9.
fn is_line(position: i32) -> bool {
    (3..=9).contains(&position) && position % 2 == 1
}

/// The head drawn for a note. `last_in_glyph` is the last note of a ligature (where a
/// liquescent's small or curled head goes); `ascii` is for terminals without the symbols.
pub fn note_glyph(n: &ChantNote, last_in_glyph: bool, ascii: bool) -> char {
    let pick = |u: char, a: char| if ascii { a } else { u };
    if last_in_glyph {
        match n.liquescence() {
            Liquescence::Deminutus => return pick('▪', '.'),
            Liquescence::AuctusAscendens => return pick('◥', '/'),
            Liquescence::AuctusDescendens => return pick('◢', '\\'),
            Liquescence::None => {}
        }
    }
    match n.shape() {
        Shape::Inclinatum => pick('◆', '*'),
        Shape::Quilisma => pick('◊', '~'),
        Shape::Oriscus(_) => pick('≈', '='),
        Shape::Stropha(_) => pick('▾', 'u'),
        Shape::Virga | Shape::Bivirga(_) => pick('▮', '|'),
        Shape::VirgaReversa => pick('▯', 'i'),
        _ if n.linea() => pick('▣', '#'),
        _ if n.cavum() => pick('□', 'O'),
        _ if n.initio_debilis => pick('▫', ','),
        _ => pick('■', '#'),
    }
}

/// The cells of a division over the staff: (position, character) from the bottom up. The
/// strokes cross the staff lines with a junction.
pub fn division_cells(d: Division, ascii: bool) -> Vec<(i32, char)> {
    let (lo, hi) = match d {
        Division::Virgula => (9, 10),
        Division::Minima => (8, 10),
        Division::Minor => (5, 7),
        Division::Maior | Division::Finalis => (3, 9),
    };
    (lo..=hi)
        .map(|p| {
            let c = match (d, is_line(p), ascii) {
                (Division::Finalis, _, true) => ':',
                (_, _, true) => '|',
                (Division::Virgula, true, _) => '┴',
                (Division::Finalis, true, _) => '╫',
                (Division::Finalis, false, _) => '║',
                (_, true, _) => '┼',
                (_, false, _) => '│',
            };
            (p, c)
        })
        .collect()
}

/// The ledger lines a note at `position` needs, as the odd positions to draw beyond the staff.
pub fn ledger_lines(position: i32) -> Vec<i32> {
    if position < 3 {
        (position..=1).filter(|p| p % 2 != 0).collect()
    } else if position > 9 {
        (11..=position).filter(|p| p % 2 != 0).collect()
    } else {
        vec![]
    }
}

/// Which notes of a syllable end a ligature (item indexes), and the pitch of the note each other
/// note of a ligature joins to.
pub fn ligatures(items: &[NeumeItem]) -> (HashSet<usize>, HashMap<usize, i32>) {
    let (mut last, mut joined) = (HashSet::new(), HashMap::new());
    let mut run: Vec<(usize, &ChantNote)> = vec![];
    let mut flush = |run: &mut Vec<(usize, &ChantNote)>| {
        for g in group_neumes(run) {
            for w in g.windows(2) {
                joined.insert(w[0].0, w[1].1.pitch as i32);
            }
            if let Some(l) = g.last() {
                last.insert(l.0);
            }
        }
        run.clear();
    };
    for (i, item) in items.iter().enumerate() {
        match item {
            NeumeItem::Note(n) => run.push((i, n)),
            _ => flush(&mut run),
        }
    }
    flush(&mut run);
    (last, joined)
}

/// One thing drawn in a syllable's notation, at a column from the syllable's start.
#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    Note {
        ch: char,
        pos: i32,
        /// (bar, beat, beats sounded) in the transcription.
        beat: Option<(usize, usize, u8)>,
        /// Further beats of a modern track's note: those tied on to it.
        also: Vec<(usize, usize)>,
        mora: u8,
        episema: bool,
        /// The position of the next note of the same ligature, joined by a stroke.
        joined: Option<i32>,
    },
    /// Flat, natural or sharp at a staff position.
    Alter(i32, char),
    /// A division, and the transcription beat it stands on when it is a cell of its own.
    Division(Division, Option<(usize, usize)>),
    /// An empty group, at its beat.
    Slot((usize, usize)),
    Clef(ChantClef),
    Custos(i32),
}

/// A syllable placed on its own: its marks, text and width in columns.
#[derive(Debug, Clone)]
pub struct Syl {
    /// (column, mark) relative to the syllable start.
    pub marks: Vec<(u16, Mark)>,
    pub width: u16,
    pub text: String,
    /// The next syllable continues the word.
    pub hyphen: bool,
    pub italic: bool,
    /// The clef in force before the syllable.
    pub clef_in: ChantClef,
    /// (bar, beat) and staff position of its first note.
    pub first_beat: Option<(usize, usize)>,
    pub first_pitch: Option<i32>,
}

impl Syl {
    /// Nothing but divisions and clefs (it stays on the line it follows).
    fn only_division(&self) -> bool {
        self.first_pitch.is_none()
            && self.text.is_empty()
            && self
                .marks
                .iter()
                .any(|(_, m)| matches!(m, Mark::Division(..)))
    }

    /// The clef a line starting with this syllable opens with.
    fn line_clef(&self) -> ChantClef {
        match self.marks.first() {
            Some((0, Mark::Clef(c))) => *c,
            _ => self.clef_in,
        }
    }
}

pub(crate) fn alter_char(sign: &str, ascii: bool) -> char {
    match (sign.chars().next(), ascii) {
        (Some('x' | 'X'), false) => '♭',
        (Some('x' | 'X'), true) => 'b',
        (Some('#'), false) => '♯',
        (Some('#'), true) => '+',
        (_, false) => '♮',
        (_, true) => 'n',
    }
}

/// Places every syllable's marks in columns: notes one column each (a repeated note several), a
/// gap column between neumes, mora dots after their note.
pub fn layout(chant: &Chant, ascii: bool) -> Vec<Syl> {
    let t = transcribe(chant);
    let mut beats = HashMap::new();
    let mut cells = HashMap::new();
    for c in &t.cells {
        match (c.note, &chant.syllables[c.syllable].items.get(c.item)) {
            (Some(_), Some(NeumeItem::Note(n))) => {
                beats.insert((c.syllable, c.item), (c.bar, c.beat, n.repeats()));
            }
            _ => {
                cells.insert((c.syllable, c.item), (c.bar, c.beat));
            }
        }
    }
    let mut out: Vec<Syl> = Vec::new();
    let mut clef = chant.initial_clef();
    for (si, s) in chant.syllables.iter().enumerate() {
        let clef_in = clef;
        let (last, joined) = ligatures(&s.items);
        let mut marks = Vec::new();
        let mut col: u16 = 0;
        let mut gap = false;
        if s.is_slot() {
            if let Some(&at) = cells.get(&(si, 0)) {
                marks.push((0, Mark::Slot(at)));
                col = 1;
            }
        }
        for (ii, item) in s.items.iter().enumerate() {
            match item {
                NeumeItem::Note(n) => {
                    if gap && col > 0 {
                        col += 1;
                    }
                    gap = false;
                    for r in 0..n.repeats() as u16 {
                        marks.push((
                            col + r,
                            Mark::Note {
                                ch: note_glyph(n, last.contains(&ii), ascii),
                                pos: n.pitch as i32,
                                beat: beats.get(&(si, ii)).copied().filter(|_| r == 0),
                                also: vec![],
                                mora: n.mora(),
                                episema: n.episema(),
                                joined: joined
                                    .get(&ii)
                                    .copied()
                                    .filter(|_| r + 1 == n.repeats() as u16),
                            },
                        ));
                    }
                    col += n.repeats() as u16 + n.mora() as u16;
                }
                NeumeItem::Space(sp) if !sp.trim_start_matches('!').is_empty() => gap = true,
                NeumeItem::Alteration { pitch, sign } => {
                    if gap && col > 0 {
                        col += 1;
                    }
                    gap = false;
                    marks.push((col, Mark::Alter(*pitch as i32, alter_char(sign, ascii))));
                    col += 1;
                }
                NeumeItem::Bar(b) => {
                    if col > 0 {
                        col += 1;
                    }
                    gap = false;
                    marks.push((
                        col,
                        Mark::Division(Division::of(b), cells.get(&(si, ii)).copied()),
                    ));
                    col += 1;
                }
                NeumeItem::Clef(c) => {
                    // The clef opening the chant is the line's.
                    if !(si == 0 && col == 0) {
                        marks.push((col, Mark::Clef(*c)));
                        col += 2;
                    }
                    clef = *c;
                }
                NeumeItem::Custos(p) => {
                    // An implicit custos takes the next note's position (resolved below).
                    marks.push((col, Mark::Custos(p.map_or(-1, |p| p as i32))));
                    col += 1;
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
        finish(&mut out, marks, col, text, hyphen, s.italic(), clef_in);
    }
    resolve_custodes(&mut out);
    out
}

/// Adds a syllable of `col` columns of notation (nothing, if it has neither marks nor text).
fn finish(
    out: &mut Vec<Syl>,
    marks: Vec<(u16, Mark)>,
    col: u16,
    text: String,
    hyphen: bool,
    italic: bool,
    clef_in: ChantClef,
) {
    if marks.is_empty() && text.is_empty() {
        return;
    }
    let text_w = text.chars().count() as u16 + u16::from(hyphen);
    let first_note = marks.iter().find_map(|(_, m)| match m {
        Mark::Note { pos, beat, .. } => Some((*pos, beat.map(|(b, i, _)| (b, i)))),
        _ => None,
    });
    // Without notes, the cell it is (an empty group, a division of its own).
    let cell_beat = marks.iter().find_map(|(_, m)| match m {
        Mark::Slot(at) | Mark::Division(_, Some(at)) => Some(*at),
        _ => None,
    });
    out.push(Syl {
        width: col.max(text_w) + 1,
        marks,
        text,
        hyphen,
        italic,
        clef_in,
        first_beat: first_note.and_then(|n| n.1).or(cell_beat),
        first_pitch: first_note.map(|n| n.0),
    });
}

/// Custodes without a pitch show the next note.
fn resolve_custodes(out: &mut [Syl]) {
    let mut next = None;
    for s in out.iter_mut().rev() {
        for (_, m) in s.marks.iter_mut().rev() {
            if let Mark::Custos(p) = m
                && *p < 0
            {
                *p = next.unwrap_or(0);
            }
        }
        if s.first_pitch.is_some() {
            next = s.first_pitch;
        }
    }
}

/// A line of syllables: each with its column from the line's start.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Line {
    /// (column, index into the laid-out syllables).
    pub items: Vec<(u16, usize)>,
    /// Columns used.
    pub end: u16,
    /// The clef the line opens with.
    pub clef: ChantClef,
    /// A custos closing the line: the position of the next line's first note.
    pub custos: Option<i32>,
}

fn open_line(syls: &[Syl], items: Vec<(u16, usize)>, end: u16, custos: Option<i32>) -> Line {
    let clef = items.first().map(|&(_, i)| syls[i].line_clef());
    Line {
        items,
        end,
        clef: clef.unwrap_or_default(),
        custos,
    }
}

/// Breaks the syllables into lines of `avail` columns (a division stays on the line it
/// follows, and a column at the right is kept for the custos), and spreads all but the last
/// line to the full width.
pub fn wrap(syls: &[Syl], avail: u16) -> Vec<Line> {
    let mut lines: Vec<Line> = vec![];
    let mut cur: Vec<(u16, usize)> = vec![];
    let mut x = 0;
    for (i, s) in syls.iter().enumerate() {
        if !cur.is_empty() && x + s.width + 1 > avail && !s.only_division() {
            let custos = syls[i..].iter().find_map(|s| s.first_pitch);
            let mut line = open_line(syls, std::mem::take(&mut cur), x, custos);
            justify(&mut line, avail.saturating_sub(1));
            lines.push(line);
            x = 0;
        }
        cur.push((x, i));
        x += s.width;
    }
    if !cur.is_empty() {
        lines.push(open_line(syls, cur, x, None));
    }
    lines
}

/// Spreads the syllables of a line evenly so that it ends at column `to`.
pub fn justify(line: &mut Line, to: u16) {
    let n = line.items.len().saturating_sub(1);
    let extra = to.saturating_sub(line.end) as usize;
    if n == 0 || extra == 0 {
        return;
    }
    for (k, (x, _)) in line.items.iter_mut().enumerate() {
        *x += (extra * k / n) as u16;
    }
    line.end = to;
}

/// The horizontal layout's line: the syllables from `from` that fit `avail` columns (at least
/// one).
pub fn line_from(syls: &[Syl], from: usize, avail: u16) -> Line {
    let mut items = vec![];
    let mut x = 0;
    for (i, s) in syls.iter().enumerate().skip(from) {
        if !items.is_empty() && x + s.width > avail {
            break;
        }
        items.push((x, i));
        x += s.width;
    }
    open_line(syls, items, x, None)
}

/// The first syllable to show in the horizontal layout so that `focus` is whole on screen:
/// `scroll` if it is already, else the nearest change (`to` places it at the top or middle).
pub fn scroll_syllables(
    syls: &[Syl],
    avail: u16,
    scroll: usize,
    focus: usize,
    to: Option<ScrollTo>,
) -> usize {
    if syls.is_empty() {
        return 0;
    }
    let focus = focus.min(syls.len() - 1);
    // Columns from syllable `a` to the end of `b`.
    let span = |a: usize, b: usize| syls[a..=b].iter().map(|s| s.width as usize).sum::<usize>();
    let fits = |from: usize| span(from, focus) <= avail as usize;
    match to {
        Some(ScrollTo::Top) => focus,
        Some(ScrollTo::Center) => {
            let half = (avail as usize).saturating_sub(syls[focus].width as usize) / 2;
            (0..=focus)
                .find(|&s| s == focus || span(s, focus) - syls[focus].width as usize <= half)
                .unwrap_or(focus)
        }
        Some(ScrollTo::Bottom) => (0..=focus)
            .find(|&s| s == focus || fits(s))
            .unwrap_or(focus),
        None if focus < scroll => focus,
        None => (scroll.min(focus)..=focus)
            .find(|&s| s == focus || fits(s))
            .unwrap_or(focus),
    }
}

/// The syllable the cursor (a transcription bar and beat) is in: the last one whose first note
/// is at or before it.
pub fn syllable_at(syls: &[Syl], cursor: (usize, usize)) -> usize {
    syls.iter()
        .rposition(|s| s.first_beat.is_some_and(|b| b <= cursor))
        .unwrap_or(0)
}

/// How many lines a page of the page layout holds: the page has the proportions of an A4 sheet
/// and a terminal cell is twice as high as wide.
pub fn lines_per_page(width: u16, system_height: u16) -> usize {
    let rows = (width as f64 * 842.0 / 595.0 / 2.0) as u16;
    (rows / system_height.max(1)).max(1) as usize
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

/// How a frame of chant is painted: the area, staff range and styles shared by its lines.
struct Painter<'a> {
    app: &'a App,
    pane: u32,
    focused: bool,
    range: Range,
    ascii: bool,
    /// The area of a line's staff, from the clef.
    area: Rect,
    playing: Option<(usize, usize)>,
    cursor: Cursor,
    selection: Option<(Cursor, Cursor, bool)>,
}

impl Painter<'_> {
    fn selected(&self, bar: usize, beat: usize) -> bool {
        self.focused
            && self.selection.is_some_and(|(a, c, bars)| {
                let (lo, hi) = if (a.bar, a.beat) <= (c.bar, c.beat) {
                    (a, c)
                } else {
                    (c, a)
                };
                if bars {
                    (lo.bar..=hi.bar).contains(&bar)
                } else {
                    (lo.bar, lo.beat) <= (bar, beat) && (bar, beat) <= (hi.bar, hi.beat)
                }
            })
    }

    /// Draws one line of the chant with its top row at `y0`.
    fn line(&self, buf: &mut Buffer, hits: &mut Vec<Hit>, syls: &[Syl], l: &Line, y0: u16) {
        let th = &self.app.config.theme;
        let area = self.area;
        let pane = &self.app.panes[&self.pane];
        let range = self.range;
        let row = |p: i32| y0 + range.row(p);
        let right = area.x + area.width;
        let x0 = area.x + CLEF_W;
        let dim = Style::default().fg(rc(th.dim));
        let marker = Style::default().fg(rc(th.marker));
        let ink = Style::default()
            .fg(rc(th.chrome_text))
            .add_modifier(Modifier::BOLD);
        let faint = Style::default().fg(rc(th.chrome_text.mix(th.dim, 0.5)));
        let on = |bg| Style::default().fg(rc(th.text_on(bg))).bg(rc(bg));
        let cursor_style = if self.focused {
            on(th.mode_color(self.app.mode))
        } else {
            on(th.dim)
        };
        let select_style = Style::default().bg(rc(th.selection.mix(th.chrome, 0.5)));
        let play_style = Style::default().bg(rc(th.playhead));
        let put = |buf: &mut Buffer, x: u16, y: u16, s: &str, style: Style| {
            if x < right {
                buf.set_string(x, y, s, style);
            }
        };
        let dash = if self.ascii { "-" } else { "─" };

        // Staff, clef and the line's flat.
        for p in (3..=9).step_by(2) {
            for dx in 0..area.width {
                buf.set_string(area.x + dx, row(p), dash, dim);
            }
        }
        let clef_letter = |c: &ChantClef| if c.kind == ClefKind::Do { "C" } else { "F" };
        buf.set_string(area.x, row(l.clef.position()), clef_letter(&l.clef), marker);
        if l.clef.flat
            && let Some(b) = (0..=13).find(|&p| l.clef.degree(p) == 6)
        {
            buf.set_string(
                area.x + 1,
                row(b),
                alter_char("x", self.ascii).to_string(),
                marker,
            );
        }

        let text_y = y0 + range.rows();
        // Where a word's extender (`_`) runs from, while the syllables under it have no text.
        let mut extender: Option<u16> = None;
        for &(sx, si) in &l.items {
            let s = &syls[si];
            let base = x0 + sx;
            let syl_area = Rect {
                x: base,
                y: y0,
                width: s.width.min(right.saturating_sub(base)),
                height: range.rows(),
            };
            // Ledger lines and ligature strokes under the heads.
            for (dx, m) in &s.marks {
                let x = base + dx;
                match m {
                    Mark::Note { pos, joined, .. } => {
                        for q in ledger_lines(*pos) {
                            for lx in x.saturating_sub(1)..=x + 1 {
                                put(buf, lx, row(q), dash, dim);
                            }
                        }
                        if let Some(next) = joined {
                            let (a, b) = (*pos.min(next), *pos.max(next));
                            let stroke = if self.ascii { "|" } else { "┃" };
                            for q in a + 1..b {
                                put(buf, x, row(q), stroke, ink);
                            }
                        }
                    }
                    Mark::Division(d, _) => {
                        for (p, c) in division_cells(*d, self.ascii) {
                            put(buf, x, row(p), &c.to_string(), faint);
                        }
                    }
                    _ => {}
                }
            }
            // Heads, accidentals, clefs and custodes over them, and the click targets.
            let mut first_hit = None;
            for (dx, m) in &s.marks {
                let x = base + dx;
                match m {
                    Mark::Note {
                        ch,
                        pos,
                        beat,
                        also,
                        mora,
                        episema,
                        ..
                    } => {
                        let y = row(*pos);
                        let mut style = ink;
                        if *episema {
                            style = style.add_modifier(Modifier::UNDERLINED);
                        }
                        if let Some((bar, b, repeats)) = *beat {
                            let column = Rect {
                                x,
                                y: y0,
                                width: 1,
                                height: range.rows(),
                            };
                            // Every beat the note sounds on: repeated notes, tied beats.
                            let beats: Vec<(usize, usize)> = (0..repeats as usize)
                                .map(|r| (bar, b + r))
                                .chain(also.iter().copied())
                                .collect();
                            let c = self.cursor;
                            let on_cursor = beats.contains(&(c.bar, c.beat))
                                && !self.app.cursor_hidden(pane.buffer);
                            let on_play = self.playing.is_some_and(|p| beats.contains(&p));
                            if x < right {
                                if on_play {
                                    buf.set_style(column, play_style);
                                }
                                let hit = Hit {
                                    area: column,
                                    pane: self.pane,
                                    cursor: Cursor {
                                        track: c.track,
                                        bar,
                                        beat: b,
                                        string: 0,
                                        voice: 0,
                                        staff: Cursor::HOME,
                                        line: *pos,
                                    },
                                };
                                first_hit.get_or_insert(hit);
                                hits.push(hit);
                            }
                            if beats.iter().any(|&(a, bb)| self.selected(a, bb)) {
                                buf.set_style(
                                    Rect {
                                        y,
                                        height: 1,
                                        ..column
                                    },
                                    select_style,
                                );
                            }
                            if on_cursor {
                                style = cursor_style;
                            }
                        }
                        put(buf, x, y, &ch.to_string(), style);
                        if *mora > 0 {
                            put(buf, x + 1, y, &"·".repeat(*mora as usize), ink);
                        }
                    }
                    Mark::Alter(p, c) => put(buf, x, row(*p), &c.to_string(), ink),
                    Mark::Clef(cl) => put(buf, x, row(cl.position()), clef_letter(cl), marker),
                    Mark::Custos(p) => {
                        put(buf, x, row(*p), if self.ascii { "v" } else { "∨" }, faint)
                    }
                    Mark::Division(_, cell) => {
                        // A division of its own is a place for the cursor.
                        if let Some(&(bar, beat)) = cell.as_ref() {
                            let column = Rect {
                                x,
                                y: y0,
                                width: 1,
                                height: range.rows(),
                            };
                            let c = self.cursor;
                            if (c.bar, c.beat) == (bar, beat)
                                && !self.app.cursor_hidden(pane.buffer)
                            {
                                buf.set_style(column, cursor_style);
                            }
                            if self.selected(bar, beat) {
                                buf.set_style(column, select_style);
                            }
                        }
                    }
                    Mark::Slot((bar, beat)) => {
                        let c = self.cursor;
                        let y = row(c.line.clamp(0, 13));
                        let on_cursor = (c.bar, c.beat) == (*bar, *beat)
                            && !self.app.cursor_hidden(pane.buffer);
                        let style = if on_cursor { cursor_style } else { faint };
                        put(buf, x, y, if self.ascii { "_" } else { "·" }, style);
                        let column = Rect {
                            x,
                            y: y0,
                            width: 1,
                            height: range.rows(),
                        };
                        if self.selected(*bar, *beat) {
                            buf.set_style(column, select_style);
                        }
                    }
                }
            }
            // A click anywhere on the syllable picks its first note.
            if let Some(mut hit) = first_hit {
                hit.area = syl_area;
                hits.push(hit);
            }

            // The text underneath: its syllable, a hyphen when the word goes on, an extender
            // through the notes of a melisma.
            let style = if s.italic {
                Style::default()
                    .fg(rc(th.lyrics))
                    .add_modifier(Modifier::ITALIC)
            } else {
                Style::default().fg(rc(th.lyrics))
            };
            let mut text = s.text.clone();
            if s.hyphen {
                text.push('-');
            }
            if !text.is_empty() {
                let room = right.saturating_sub(base) as usize;
                let shown: String = text.chars().take(room).collect();
                buf.set_string(base, text_y, shown, style);
                if let Some(hit) = first_hit {
                    hits.push(Hit {
                        area: Rect {
                            x: base,
                            y: text_y,
                            width: (text.chars().count() as u16).min(right.saturating_sub(base)),
                            height: 1,
                        },
                        ..hit
                    });
                }
                extender = (!s.hyphen).then(|| base + text.chars().count() as u16);
            } else if s.first_pitch.is_some()
                && let Some(from) = extender
            {
                let to = base + s.width - 1;
                for x in from.max(base.saturating_sub(1))..to {
                    put(buf, x, text_y, "_", style);
                }
                extender = Some(to);
            }
        }
        if let Some(p) = l.custos {
            put(
                buf,
                x0 + l.end,
                row(p),
                if self.ascii { "v" } else { "∨" },
                faint,
            );
        }
    }
}

impl Widget for ChantView<'_> {
    fn render(self, outer: Rect, buf: &mut Buffer) {
        let th = &self.app.config.theme;
        let pane = &self.app.panes[&self.pane];
        let score = &self.app.buffers[pane.buffer].score;
        let Some(track) = score.tracks.get(pane.cursor.track) else {
            return;
        };
        let ascii = self.app.config.file.tabline.icons == "none";
        let Some(chant) = track.chant.as_deref() else {
            return;
        };
        let (syls, range) = (layout(chant, ascii), staff_range(chant));
        let sh = range.rows() + 2;
        let horizontal = pane.layout == "horizontal";
        let paged = pane.layout == "page";
        // The page layout draws each page in a box: the score sits two columns inside it.
        let area = if paged {
            Rect {
                x: outer.x + 2,
                width: outer.width.saturating_sub(4),
                ..outer
            }
        } else {
            outer
        };
        let avail = area.width.saturating_sub(CLEF_W + 1).max(8);

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
        // The syllable to keep in view: the playhead's while following playback, else the
        // cursor's.
        let c = pane.cursor;
        let focus = match playing.filter(|_| self.app.follow) {
            Some(p) => p,
            None => (c.bar, c.beat),
        };
        let focus_syl = syllable_at(&syls, focus);

        let request = pane.scroll_request;
        let (lines, scroll, first, visible) = if horizontal {
            // One line; the scroll is the first syllable shown.
            let mut scroll = (*self.scroll).min(syls.len().saturating_sub(1));
            if self.follow || request.is_some() {
                scroll = scroll_syllables(&syls, avail, scroll, focus_syl, request);
            }
            *self.extent = (0, 0);
            (vec![line_from(&syls, scroll, avail)], scroll, 0, 1)
        } else {
            let lines = wrap(&syls, avail);
            let cursor_line = lines
                .iter()
                .rposition(|l| l.items.first().is_some_and(|&(_, i)| i <= focus_syl))
                .unwrap_or(0);
            let mut scroll = (*self.scroll).min(lines.len().saturating_sub(1));
            let visible;
            if paged {
                let per = lines_per_page(outer.width, sh);
                let systems = page_systems(lines.len(), per);
                if let Some(to) = request {
                    scroll = tabview::scroll_for(
                        to,
                        cursor_line,
                        tabview::page_fit(&systems, sh, cursor_line, area.height),
                    );
                } else if self.follow {
                    scroll = tabview::page_scroll(&systems, sh, scroll, area.height, cursor_line);
                }
                visible = tabview::page_fit(&systems, sh, scroll, area.height);
            } else {
                visible = (area.height / sh).max(1) as usize;
                if let Some(to) = request {
                    scroll = tabview::scroll_for(to, cursor_line, visible);
                } else if !self.follow {
                    scroll = scroll.min(lines.len().saturating_sub(visible));
                } else if cursor_line < scroll {
                    scroll = cursor_line;
                } else if cursor_line >= scroll + visible {
                    scroll = cursor_line + 1 - visible;
                }
            }
            *self.extent = (lines.len(), visible);
            (lines, scroll, scroll, visible)
        };
        *self.scroll = scroll;

        // Where each shown line starts (relative to the pane): stacked, with the page layout's
        // borders between pages.
        let systems = page_systems(lines.len(), lines_per_page(outer.width, sh));
        let mut placed = vec![];
        let mut dy = 0;
        for i in first..lines.len() {
            let (before, after) = if paged {
                tabview::page_pad(&systems, i)
            } else {
                (0, 0)
            };
            if i > first
                && (placed.len() >= visible && !paged || dy + before + sh > area.height + 1)
            {
                break;
            }
            placed.push((i, dy + before, before > 0, after > 0));
            dy += before + sh + after;
        }

        let painter = Painter {
            app: self.app,
            pane: self.pane,
            focused: self.focused,
            range,
            ascii,
            area,
            playing,
            cursor: c,
            selection: self.app.selection(),
        };
        let page_style = Style::default().fg(rc(th.chrome_text.mix(th.dim, 0.5)));
        for (i, dy, top, bottom) in placed {
            let y0 = area.y + dy;
            if y0 + sh > area.y + area.height + 1 {
                break;
            }
            if paged {
                let (l, r) = (outer.x, (outer.x + outer.width).saturating_sub(1));
                if top {
                    let label = format!(" Page {} ", systems[i].page() + 1);
                    tabview::page_edge(buf, outer, y0 - 1, ('┌', '┐'), &label, page_style);
                }
                for y in y0..(y0 + sh).min(outer.y + outer.height) {
                    buf.set_string(l, y, "│", page_style);
                    buf.set_string(r, y, "│", page_style);
                }
                if bottom && y0 + sh < outer.y + outer.height {
                    tabview::page_edge(buf, outer, y0 + sh, ('└', '┘'), "", page_style);
                }
            }
            painter.line(buf, self.hits, &syls, &lines[i], y0);
        }
    }
}

/// The lines as page-layout systems, `per` to a page.
fn page_systems(lines: usize, per: usize) -> Vec<System> {
    (0..lines)
        .map(|i| System::on_page(i / per.max(1)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(body: &str) -> Chant {
        cecilia_formats::gabc::parse(&format!("name: t;\n%%\n{body}\n")).unwrap()
    }

    fn note(gabc_note: &str) -> ChantNote {
        let c = parse(&format!("(c4) a({gabc_note})"));
        c.syllables
            .iter()
            .flat_map(|s| s.notes().cloned().collect::<Vec<_>>())
            .next()
            .unwrap()
    }

    #[test]
    fn syllables_get_columns_for_notes_and_text() {
        let chant = parse("(c4) Al(fg/h)le(h.) lu(Gs)ia.(f) (::)");
        let syls = layout(&chant, false);
        assert_eq!(syls[0].text, "Al");
        assert!(syls[0].hyphen);
        // f, g, a gap, h.
        let cols: Vec<u16> = syls[0].marks.iter().map(|(c, _)| *c).collect();
        assert_eq!(cols, [0, 1, 3]);
        assert!(matches!(syls[1].marks[0].1, Mark::Note { mora: 1, .. }));
        assert!(matches!(
            syls.last().unwrap().marks[0].1,
            Mark::Division(Division::Finalis, _)
        ));
    }

    #[test]
    fn glyphs_follow_the_neume_shape() {
        assert_eq!(note_glyph(&note("f"), false, false), '■');
        assert_eq!(note_glyph(&note("F"), false, false), '◆');
        assert_eq!(note_glyph(&note("fv"), false, false), '▮');
        assert_eq!(note_glyph(&note("fw"), false, false), '◊');
        assert_eq!(note_glyph(&note("fo"), false, false), '≈');
        assert_eq!(note_glyph(&note("fs"), false, false), '▾');
        assert_eq!(note_glyph(&note("fr"), false, false), '□');
        // Liquescents change only the last note of a ligature.
        assert_eq!(note_glyph(&note("f~"), true, false), '▪');
        assert_eq!(note_glyph(&note("f~"), false, false), '■');
        assert_eq!(note_glyph(&note("f<"), true, false), '◥');
        assert_eq!(note_glyph(&note("f>"), true, false), '◢');
        assert_eq!(note_glyph(&note("f"), false, true), '#');
        assert_eq!(note_glyph(&note("F"), false, true), '*');
    }

    #[test]
    fn ligatures_join_the_notes_of_a_neume() {
        // A pes (f g), a clivis (h g) and a lone punctum after a break.
        let chant = parse("(c4) a(fg/hg/f)");
        let (last, joined) = ligatures(&chant.syllables[1].items);
        assert_eq!(joined.len(), 2);
        assert_eq!(last.len(), 3);
        let syls = layout(&chant, false);
        let joins: Vec<Option<i32>> = syls[0]
            .marks
            .iter()
            .filter_map(|(_, m)| match m {
                Mark::Note { joined, .. } => Some(*joined),
                _ => None,
            })
            .collect();
        assert_eq!(joins, [Some(6), None, Some(6), None, None]);
    }

    #[test]
    fn divisions_cross_the_staff_lines() {
        let maior = division_cells(Division::Maior, false);
        assert_eq!(maior.len(), 7);
        assert_eq!(maior[0], (3, '┼'));
        assert_eq!(maior[1], (4, '│'));
        assert_eq!(division_cells(Division::Finalis, false)[0], (3, '╫'));
        assert_eq!(
            division_cells(Division::Virgula, false),
            [(9, '┴'), (10, '│')]
        );
        assert!(
            division_cells(Division::Minor, true)
                .iter()
                .all(|c| c.1 == '|')
        );
    }

    #[test]
    fn ledger_lines_are_drawn_beyond_the_staff() {
        assert_eq!(ledger_lines(5), Vec::<i32>::new());
        assert_eq!(ledger_lines(2), Vec::<i32>::new());
        assert_eq!(ledger_lines(1), [1]);
        assert_eq!(ledger_lines(0), [1]);
        assert_eq!(ledger_lines(10), Vec::<i32>::new());
        assert_eq!(ledger_lines(11), [11]);
        assert_eq!(ledger_lines(13), [11, 13]);
    }

    #[test]
    fn staff_range_grows_with_the_notes() {
        assert_eq!(staff_range(&parse("(c4) a(f)")), Range { low: 2, top: 10 });
        let r = staff_range(&parse("(c4) a(a) b(m)"));
        assert_eq!(r, Range { low: 0, top: 12 });
        assert_eq!(r.rows(), 13);
        assert_eq!(r.row(12), 0);
        assert_eq!(r.row(0), 12);
        assert_eq!(system_height(&parse("(c4) a(f)")), 11);
    }

    #[test]
    fn lines_wrap_at_the_width_and_keep_divisions_with_their_line() {
        let chant = parse("(c4) Al(f)le(g) lu(h) ia(i) (::) sec(j)u(k)lo(l) (;)");
        let syls = layout(&chant, false);
        let lines = wrap(&syls, 16);
        assert!(lines.len() > 1, "{lines:?}");
        // Every syllable once, in order.
        let all: Vec<usize> = lines
            .iter()
            .flat_map(|l| l.items.iter().map(|i| i.1))
            .collect();
        assert_eq!(all, (0..syls.len()).collect::<Vec<_>>());
        for l in &lines {
            let (x, i) = *l.items.last().unwrap();
            assert!(x + syls[i].width <= 16 || l.items.len() == 1, "{l:?}");
        }
        // No line begins with a division only.
        for l in &lines {
            assert!(!syls[l.items[0].1].only_division(), "{l:?}");
        }
        // All but the last line close with the next line's first note as a custos, and are
        // spread to the full width.
        let first_pitch = syls[lines[1].items[0].1].first_pitch;
        assert_eq!(lines[0].custos, first_pitch);
        assert_eq!(lines[0].end, 15);
        assert_eq!(lines.last().unwrap().custos, None);
    }

    #[test]
    fn a_line_opens_with_the_clef_in_force() {
        let chant = parse("(c4) Al(f)le(g) lu(h) (c3) ia(i) sec(j) lo(k)");
        let syls = layout(&chant, false);
        let lines = wrap(&syls, 12);
        assert!(lines.len() >= 2);
        assert_eq!(lines[0].clef, ChantClef::parse("c4").unwrap());
        assert_eq!(lines.last().unwrap().clef, ChantClef::parse("c3").unwrap());
    }

    #[test]
    fn horizontal_line_scrolls_by_syllable() {
        let chant = parse("(c4) a(f) b(g) c(h) d(i) e(j) f(k) g(l)");
        let syls = layout(&chant, false);
        assert!(syls.len() >= 7);
        let w = syls[0].width;
        let avail = 3 * w;
        assert_eq!(line_from(&syls, 2, avail).items.len(), 3);
        // Already in view: stays. Past the right edge: slides just far enough.
        assert_eq!(scroll_syllables(&syls, avail, 0, 2, None), 0);
        assert_eq!(scroll_syllables(&syls, avail, 0, 4, None), 2);
        // Back before the first shown.
        assert_eq!(scroll_syllables(&syls, avail, 4, 1, None), 1);
        assert_eq!(scroll_syllables(&syls, avail, 0, 4, Some(ScrollTo::Top)), 4);
        assert_eq!(
            scroll_syllables(&syls, avail, 0, 4, Some(ScrollTo::Bottom)),
            2
        );
        assert_eq!(
            scroll_syllables(&syls, avail, 0, 4, Some(ScrollTo::Center)),
            3
        );
        assert_eq!(scroll_syllables(&[], avail, 0, 4, None), 0);
    }

    #[test]
    fn cursor_syllable_follows_the_beat() {
        let chant = parse("(c4) a(fg) b(h) (::) c(i)");
        let syls = layout(&chant, false);
        let first = syls[1].first_beat.unwrap();
        assert_eq!(syllable_at(&syls, (0, 0)), 0);
        assert_eq!(syllable_at(&syls, first), 1);
        assert_eq!(syllable_at(&syls, (first.0 + 5, 0)), syls.len() - 1);
    }

    #[test]
    fn pages_hold_lines_by_the_a4_proportion() {
        assert_eq!(lines_per_page(80, 11), 5);
        assert_eq!(lines_per_page(10, 11), 1);
        let s = page_systems(7, 3);
        assert_eq!(
            s.iter().map(|s| s.page()).collect::<Vec<_>>(),
            [0, 0, 0, 1, 1, 1, 2]
        );
    }

    #[test]
    fn custos_without_pitch_shows_the_next_note() {
        let chant = parse("(c4) a(f z0) b(h)");
        let syls = layout(&chant, false);
        let custos = syls[0].marks.iter().find_map(|(_, m)| match m {
            Mark::Custos(p) => Some(*p),
            _ => None,
        });
        assert_eq!(custos, Some(7));
    }
}
