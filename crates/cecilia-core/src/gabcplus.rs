//! gabc+: a track's lyrics as one text, the single source of the syllables on its beats.
//!
//! The text is GABC (Gregorio's chant notation, `text(notes)`) with a few additions for
//! modern scores. Newlines and runs of spaces only separate words. In it:
//!
//! - a word is sung one syllable per beat; `-` inside a word splits it into syllables joined
//!   by a hyphen (`mor-tal`, `some--one`), and so does a `(…)` group followed by more text
//!   (`Pó(g)pu(h)lus(i)`, as in GABC); a word starting with `-` joins the one before it
//!   (`some -one`);
//! - `_` between letters separates words (`some_one` is `some one`); `_` at the start of a
//!   syllable (`_si`, `a-_men`) prints it in italics;
//! - a word of dashes (`--`) covers that many beats under one dash; a word of underscores
//!   (`__`) leaves that many beats without text;
//! - `[text]` is shown before the next syllable without taking a beat (verse numbers);
//! - `*` alone is printed as ✻ after the previous syllable without taking a beat;
//! - `<bar:beat>` (1-based, as in the status line) starts the following syllables at the first
//!   singing beat from there; anchors that go back in the music start a new line of lyrics
//!   (the next verse row);
//! - `(…)` groups are GABC notes, used only for chant notation; a group without text (`(c4)`,
//!   `(,)`) takes no beat.
//!
//! Syllables are sung in the order the music is played: repeats, alternate endings and
//! directions (D.C., D.S., al Fine, al Coda) are followed, from the first singing beat (one
//! with a note, tied notes included, as in Guitar Pro) unless an anchor says otherwise. A beat
//! sung on the second pass of a repeat shows its syllable on the second verse row, and so on.

use crate::{Direction, Score, Track};

/// A piece of gabc+ text, by character range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    /// Lyric text.
    Text,
    /// `(…)`: GABC notes.
    Gabc,
    /// `[…]`: a label shown before a syllable.
    Label,
    /// `<bar:beat>`.
    Anchor,
    /// Spaces and line breaks.
    Space,
}

/// The pieces of a text, as (first char, one past the last, kind), in order and covering it.
/// An unclosed `(` or `[` is text.
pub fn spans(text: &str) -> Vec<(usize, usize, SpanKind)> {
    let c: Vec<char> = text.chars().collect();
    let mut out: Vec<(usize, usize, SpanKind)> = Vec::new();
    let push = |out: &mut Vec<(usize, usize, SpanKind)>, a: usize, b: usize, k: SpanKind| match out
        .last_mut()
    {
        Some(last)
            if last.2 == k && last.1 == a && matches!(k, SpanKind::Text | SpanKind::Space) =>
        {
            last.1 = b
        }
        _ => out.push((a, b, k)),
    };
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        let close = |end: char| c[i + 1..].iter().position(|&x| x == end).map(|p| i + 1 + p);
        if ch.is_whitespace() {
            push(&mut out, i, i + 1, SpanKind::Space);
            i += 1;
        } else if let Some(end) = (ch == '(').then(|| close(')')).flatten() {
            push(&mut out, i, end + 1, SpanKind::Gabc);
            i = end + 1;
        } else if let Some(end) = (ch == '[').then(|| close(']')).flatten() {
            push(&mut out, i, end + 1, SpanKind::Label);
            i = end + 1;
        } else if let Some(end) = (ch == '<').then(|| anchor_end(&c, i)).flatten() {
            push(&mut out, i, end, SpanKind::Anchor);
            i = end;
        } else {
            push(&mut out, i, i + 1, SpanKind::Text);
            i += 1;
        }
    }
    out
}

/// What a character of gabc+ text is, for coloring it (see [`highlight`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hl {
    /// Lyric text.
    Text,
    Space,
    /// The parentheses around GABC notes.
    Paren,
    /// A clef (`c3`, `cb2`, `f4`).
    Clef,
    /// A division (`,` `;` `:` `::` `` ` `` and their digits).
    Division,
    /// A note (`a`-`m`, `A`-`M`).
    Note,
    /// A shape modifier (`v` `V` `s` `w` `o` `O` `<` `>` `~` `+` `x` `y` `#` `r` …).
    Shape,
    /// An attachment modifier (`.` `'` `_` `R` `r0`-`r5`).
    Attach,
    /// A space modifier (`!` `/` `@`).
    Gap,
    /// Anything else in a GABC group.
    Other,
    /// A markup tag in the lyrics (`<b>`, `</i>`).
    Markup,
    Label,
    Anchor,
}

/// The kind of each character of a text, for coloring: lyrics, the parts of GABC groups, markup
/// tags, labels and anchors.
pub fn highlight(text: &str) -> Vec<Hl> {
    let c: Vec<char> = text.chars().collect();
    let mut out = vec![Hl::Text; c.len()];
    for (a, z, kind) in spans(text) {
        match kind {
            SpanKind::Text => {
                // Markup tags: `<b>`, `</b>`, `<sp>`.
                let mut i = a;
                while i < z {
                    if c[i] == '<' {
                        let close = (i + 1..z.min(i + 8)).find(|&k| c[k] == '>');
                        if let Some(k) = close {
                            let name = &c[i + 1..k];
                            let name = name.strip_prefix(&['/']).unwrap_or(name);
                            if !name.is_empty() && name.iter().all(|x| x.is_ascii_lowercase()) {
                                for slot in &mut out[i..=k] {
                                    *slot = Hl::Markup;
                                }
                                i = k + 1;
                                continue;
                            }
                        }
                    }
                    i += 1;
                }
            }
            SpanKind::Space => out[a..z].fill(Hl::Space),
            SpanKind::Label => out[a..z].fill(Hl::Label),
            SpanKind::Anchor => out[a..z].fill(Hl::Anchor),
            SpanKind::Gabc => {
                out[a] = Hl::Paren;
                out[z - 1] = Hl::Paren;
                let g = &c[a + 1..z - 1];
                let at = |k: usize| g.get(k).copied();
                let mut k = 0;
                while k < g.len() {
                    let (ch, i) = (g[k], a + 1 + k);
                    let digits =
                        |k: usize| g[k..].iter().take_while(|d| d.is_ascii_digit()).count();
                    let (kind, len) = match ch {
                        // A clef: c or f, maybe b, a line.
                        'c' | 'f'
                            if (k == 0 || matches!(g[k - 1], '/' | '!' | '@' | ' '))
                                && match at(k + 1) {
                                    Some('b') => {
                                        at(k + 2).is_some_and(|d| ('1'..='4').contains(&d))
                                    }
                                    d => d.is_some_and(|d| ('1'..='4').contains(&d)),
                                } =>
                        {
                            (Hl::Clef, if at(k + 1) == Some('b') { 3 } else { 2 })
                        }
                        'a'..='m' | 'A'..='M' => (Hl::Note, 1),
                        ',' | ';' | ':' | '`' => (Hl::Division, 1 + digits(k + 1)),
                        '.' | '\'' | '_' => (Hl::Attach, 1 + digits(k + 1)),
                        'R' => (Hl::Attach, 1),
                        'r' if digits(k + 1) > 0 => (Hl::Attach, 1 + digits(k + 1)),
                        'v' | 'V' | 's' | 'w' | 'W' | 'o' | 'O' | 'q' | 'Q' | '<' | '>' | '~'
                        | '+' | 'x' | 'y' | '#' | 'r' => (Hl::Shape, 1),
                        '!' | '/' | '@' => (Hl::Gap, 1),
                        c if c.is_whitespace() => (Hl::Space, 1),
                        _ => (Hl::Other, 1),
                    };
                    out[i..i + len].fill(kind);
                    k += len;
                }
            }
        }
    }
    out
}

/// The end of a `<bar:beat>` anchor starting at `i`, if one does.
fn anchor_end(c: &[char], i: usize) -> Option<usize> {
    let close = c[i..].iter().position(|&x| x == '>')? + i;
    let inner: String = c[i + 1..close].iter().collect();
    parse_anchor(&inner).map(|_| close + 1)
}

/// `bar:beat` or `bar:beat.fraction` as the zero-based bar and the beat number (from 1).
fn parse_anchor(s: &str) -> Option<(usize, f64)> {
    crate::position::parse_position(s)
}

/// What a token of gabc+ text does.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// A sung syllable: one beat. `hyphen`: joined to the next by a hyphen.
    Syllable {
        text: String,
        italic: bool,
        hyphen: bool,
        gabc: String,
    },
    /// A word of dashes: that many beats under the dash to the next syllable.
    Dash(usize),
    /// A word of underscores: that many beats without text.
    Skip(usize),
    /// `[text]`, shown before the next syllable.
    Label(String),
    /// `*`, shown as ✻ after the previous syllable.
    Star,
    /// `<bar:beat>`, zero-based.
    /// `<bar:beat.fraction>`: the bar from 0, the beat number from 1 (fraction included).
    Anchor { bar: usize, beat: f64 },
    /// GABC notes without text: a clef, a division.
    Notes(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    /// Characters of the text it was read from.
    pub span: (usize, usize),
}

/// One part of a word: a character of text or a `(…)` group (its inside).
enum Piece {
    Char(usize, char),
    Group(usize, usize, String),
}

/// Reads gabc+ text into tokens.
pub fn tokens(text: &str) -> Vec<Token> {
    let c: Vec<char> = text.chars().collect();
    let mut out: Vec<Token> = Vec::new();
    let mut word: Vec<Piece> = Vec::new();
    for (a, b, kind) in spans(text) {
        match kind {
            SpanKind::Text => word.extend((a..b).map(|i| Piece::Char(i, c[i]))),
            SpanKind::Gabc => word.push(Piece::Group(a, b, c[a + 1..b - 1].iter().collect())),
            SpanKind::Space => finish_word(&mut out, &mut word),
            SpanKind::Label => {
                finish_word(&mut out, &mut word);
                out.push(Token {
                    kind: TokenKind::Label(
                        c[a + 1..b - 1].iter().collect::<String>().trim().into(),
                    ),
                    span: (a, b),
                });
            }
            SpanKind::Anchor => {
                finish_word(&mut out, &mut word);
                let inner: String = c[a + 1..b - 1].iter().collect();
                if let Some((bar, beat)) = parse_anchor(&inner) {
                    out.push(Token {
                        kind: TokenKind::Anchor { bar, beat },
                        span: (a, b),
                    });
                }
            }
        }
    }
    finish_word(&mut out, &mut word);
    // A dash word joins the syllable before it to the next one.
    for k in 1..out.len() {
        if matches!(out[k].kind, TokenKind::Dash(_)) {
            if let Some(TokenKind::Syllable { hyphen, .. }) =
                out[..k].iter_mut().rev().map(|t| &mut t.kind).find(|t| {
                    !matches!(
                        t,
                        TokenKind::Label(_) | TokenKind::Star | TokenKind::Notes(_)
                    )
                })
            {
                *hyphen = true;
            }
        }
    }
    out
}

fn finish_word(out: &mut Vec<Token>, word: &mut Vec<Piece>) {
    if word.is_empty() {
        return;
    }
    let pieces = std::mem::take(word);
    let start = match pieces[0] {
        Piece::Char(i, _) => i,
        Piece::Group(a, _, _) => a,
    };
    let end = match pieces.last().unwrap() {
        Piece::Char(i, _) => i + 1,
        Piece::Group(_, b, _) => *b,
    };
    let chars: String = pieces
        .iter()
        .filter_map(|p| match p {
            Piece::Char(_, c) => Some(*c),
            _ => None,
        })
        .collect();
    let groups: String = pieces
        .iter()
        .filter_map(|p| match p {
            Piece::Group(_, _, g) => Some(format!("({g})")),
            _ => None,
        })
        .collect();
    let simple = |kind: TokenKind| Token {
        kind,
        span: (start, end),
    };
    if chars.is_empty() {
        out.push(simple(TokenKind::Notes(groups)));
        return;
    }
    if chars.chars().all(|c| c == '-') {
        out.push(simple(TokenKind::Dash(chars.chars().count())));
        return;
    }
    if chars.chars().all(|c| c == '_') {
        out.push(simple(TokenKind::Skip(chars.chars().count())));
        return;
    }
    if chars == "*" {
        out.push(simple(TokenKind::Star));
        if !groups.is_empty() {
            out.push(simple(TokenKind::Notes(groups)));
        }
        return;
    }
    // Syllables of a word.
    struct Cur {
        text: String,
        gabc: String,
        italic: bool,
        start: Option<usize>,
        end: usize,
    }
    let fresh = || Cur {
        text: String::new(),
        gabc: String::new(),
        italic: false,
        start: None,
        end: 0,
    };
    let mut cur = fresh();
    // What the previous character did: nothing yet, a dash, an italic mark, a separator.
    #[derive(PartialEq, Clone, Copy)]
    enum Prev {
        Start,
        Dash,
        Italic,
        Separator,
        Other,
    }
    let mut prev = Prev::Start;
    let done = |out: &mut Vec<Token>, cur: &mut Cur, hyphen: bool| {
        let c = std::mem::replace(cur, fresh());
        let Some(s) = c.start else {
            return false;
        };
        if c.text.is_empty() {
            out.push(Token {
                kind: TokenKind::Notes(c.gabc),
                span: (s, c.end),
            });
            return false;
        }
        out.push(Token {
            kind: TokenKind::Syllable {
                text: c.text,
                italic: c.italic,
                hyphen,
                gabc: c.gabc,
            },
            span: (s, c.end),
        });
        true
    };
    for p in &pieces {
        match p {
            Piece::Group(a, b, g) => {
                cur.start.get_or_insert(*a);
                cur.gabc.push_str(g);
                cur.end = *b;
                prev = Prev::Other;
            }
            Piece::Char(i, '-') => {
                if cur.start.is_some() {
                    done(out, &mut cur, true);
                } else if prev == Prev::Start {
                    // `-one`: the syllable before goes on with a hyphen.
                    if let Some(TokenKind::Syllable { hyphen, .. }) =
                        out.iter_mut().rev().map(|t| &mut t.kind).find(|t| {
                            !matches!(
                                t,
                                TokenKind::Label(_) | TokenKind::Star | TokenKind::Notes(_)
                            )
                        })
                    {
                        *hyphen = true;
                    }
                }
                let _ = i;
                prev = Prev::Dash;
            }
            Piece::Char(_, '_') => {
                if cur.start.is_none() && matches!(prev, Prev::Start | Prev::Dash | Prev::Italic) {
                    cur.italic = true;
                    prev = Prev::Italic;
                } else {
                    done(out, &mut cur, false);
                    prev = Prev::Separator;
                }
            }
            Piece::Char(i, ch) => {
                // Text after a group starts the next syllable of the word.
                if !cur.gabc.is_empty() {
                    done(out, &mut cur, true);
                }
                if cur.start.is_none() && prev == Prev::Italic {
                    cur.italic = true;
                }
                cur.start.get_or_insert(*i);
                cur.text.push(*ch);
                cur.end = i + 1;
                prev = Prev::Other;
            }
        }
    }
    done(out, &mut cur, false);
}

/// The bars in the order they are sung: repeats (each closing bar counting its own passes),
/// alternate endings by pass, then a D.C. or D.S. jump (once; repeats are not taken again
/// after it and only the last alternate ending is played), al Fine stopping after the Fine bar
/// and al Coda jumping from the To Coda bar to the Coda. With each bar, its verse row: the
/// pass through its repeat (0 the first) or how often it was sung before, whichever is more.
/// Bars the music never reaches come last, in order.
pub fn sung_order(score: &Score) -> Vec<(usize, usize)> {
    const LIMIT: usize = 100_000;
    let bars = &score.master_bars;
    let n = bars.len();
    let has = |i: usize, d: Direction| bars[i].directions.contains(&d);
    let find = |d: Direction| (0..n).find(|&i| has(i, d));
    // Whether bar i (in alternate endings) holds the last ending of its group.
    let last_ending = |i: usize| {
        let (mut a, mut b) = (i, i);
        while a > 0 && bars[a - 1].alternate_endings != 0 {
            a -= 1;
        }
        while b + 1 < n && bars[b + 1].alternate_endings != 0 {
            b += 1;
        }
        let all = (a..=b).fold(0u8, |m, k| m | bars[k].alternate_endings);
        let top = 1u8 << (7 - all.leading_zeros().min(7));
        bars[i].alternate_endings & top != 0
    };
    let mut out = Vec::new();
    let mut sung = vec![0usize; n];
    let mut counts = vec![0u8; n];
    let (mut i, mut repeat_start, mut pass) = (0usize, 0usize, 1u8);
    let mut jumped_to = None;
    let mut jumped = false;
    let mut until: Option<Direction> = None;
    while i < n && out.len() < LIMIT {
        let mb = &bars[i];
        if !jumped {
            if mb.repeat_start && jumped_to != Some(i) {
                repeat_start = i;
                pass = 1;
            }
            jumped_to = None;
            // Past the last alternate ending the repeat is over.
            if mb.alternate_endings == 0 && i > 0 && bars[i - 1].alternate_endings != 0 {
                pass = 1;
            }
            if mb.alternate_endings != 0
                && pass <= 8
                && mb.alternate_endings & (1 << (pass - 1)) == 0
            {
                i += 1;
                continue;
            }
        } else if mb.alternate_endings != 0 && !last_ending(i) {
            i += 1;
            continue;
        }
        let p = if jumped { 0 } else { pass as usize - 1 };
        out.push((i, sung[i].max(p)));
        sung[i] += 1;
        if jumped {
            match until {
                Some(Direction::Fine) if has(i, Direction::Fine) => break,
                Some(c @ (Direction::Coda | Direction::DoubleCoda)) => {
                    let to = if c == Direction::Coda {
                        Direction::DaCoda
                    } else {
                        Direction::DaDoubleCoda
                    };
                    if let Some(target) = find(c).filter(|_| has(i, to)) {
                        until = None;
                        i = target;
                        continue;
                    }
                }
                _ => {}
            }
            i += 1;
            continue;
        }
        if let Some(total) = mb.repeat_end {
            counts[i] += 1;
            if counts[i] < total {
                pass = pass.saturating_add(1);
                i = repeat_start;
                jumped_to = Some(i);
                continue;
            }
            counts[i] = 0;
            pass = 1;
        }
        use Direction::*;
        let jump = mb.directions.iter().find_map(|d| {
            let (to, al) = match d {
                DaCapo => (Some(0), None),
                DaCapoAlCoda => (Some(0), Some(Coda)),
                DaCapoAlDoubleCoda => (Some(0), Some(DoubleCoda)),
                DaCapoAlFine => (Some(0), Some(Fine)),
                DaSegno => (find(Segno), None),
                DaSegnoAlCoda => (find(Segno), Some(Coda)),
                DaSegnoAlDoubleCoda => (find(Segno), Some(DoubleCoda)),
                DaSegnoAlFine => (find(Segno), Some(Fine)),
                DaSegnoSegno => (find(SegnoSegno), None),
                DaSegnoSegnoAlCoda => (find(SegnoSegno), Some(Coda)),
                DaSegnoSegnoAlDoubleCoda => (find(SegnoSegno), Some(DoubleCoda)),
                DaSegnoSegnoAlFine => (find(SegnoSegno), Some(Fine)),
                _ => return None,
            };
            to.map(|t| (t, al))
        });
        if let Some((to, al)) = jump {
            jumped = true;
            until = al;
            i = to;
            continue;
        }
        i += 1;
    }
    // Bars never reached (endings without their repeat) come last, so they can hold lyrics.
    for (i, n) in sung.iter().enumerate() {
        if *n == 0 && out.len() < LIMIT {
            out.push((i, 0));
        }
    }
    out
}

/// A beat that can take a syllable, in the order it is sung.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Occ {
    bar: usize,
    beat: usize,
    /// Verse row on the first line of lyrics.
    row: usize,
    /// Which bar of the sung order it is in.
    j: usize,
    /// Ticks from the start of its bar, and its beat number (from 1, with fraction).
    tick: u32,
    pos: f64,
}

/// The singing beats of a track's first voice in sung order, and for each bar of the sung
/// order (bar, index of its first singing beat in the list).
fn occurrences(score: &Score, track: &Track) -> (Vec<Occ>, Vec<(usize, usize)>) {
    let mut occ = Vec::new();
    let mut bars = Vec::new();
    for (j, (bar, row)) in sung_order(score).into_iter().enumerate() {
        bars.push((bar, occ.len()));
        let Some(voice) = track.bars.get(bar).and_then(|b| b.voices.first()) else {
            continue;
        };
        for (beat, b) in voice.beats.iter().enumerate() {
            if crate::lyrics::sings(b) {
                let tick = crate::position::offset_ticks(track, bar, 0, beat);
                let pos = 1.0 + tick as f64 / crate::position::beat_ticks(score, bar) as f64;
                occ.push(Occ {
                    bar,
                    beat,
                    row,
                    j,
                    tick,
                    pos,
                });
            }
        }
    }
    (occ, bars)
}

/// Where an anchor at `tick` ticks into `bar` starts: the first singing beat from there, the
/// first time the bar is sung (`len` when nothing is sung from there; `None` when the bar never
/// is).
fn resolve(occ: &[Occ], bars: &[(usize, usize)], bar: usize, tick: u32) -> Option<usize> {
    let (j, &(_, first)) = bars.iter().enumerate().find(|(_, b)| b.0 == bar)?;
    Some(
        (first..occ.len())
            .find(|&i| occ[i].j > j || occ[i].tick >= tick)
            .unwrap_or(occ.len()),
    )
}

/// A syllable laid on a beat.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub bar: usize,
    pub beat: usize,
    /// Verse row.
    pub verse: usize,
    /// The syllable as beats hold it: a trailing `-` when a hyphen joins it to the next.
    pub text: String,
    pub italic: bool,
    /// Labels shown before it.
    pub before: String,
    /// Shown after it (✻).
    pub after: String,
    /// Index of its token.
    pub token: usize,
}

/// gabc+ text laid out on a track.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    pub placed: Vec<Placed>,
    /// Syllables past the last singing beat: kept in the text, not shown.
    pub unplaced: usize,
    pub tokens: Vec<Token>,
}

/// The six-pointed asterisk a standalone `*` is printed as.
pub const STAR: &str = "✻";

/// Lays gabc+ `text` out on track `track` of `score`.
pub fn layout(score: &Score, track: usize, text: &str) -> Layout {
    match score.tracks.get(track) {
        Some(t) => layout_on(score, t, text),
        None => Layout {
            tokens: tokens(text),
            ..Layout::default()
        },
    }
}

/// How the syllables of a track's own gabc+ text are shown: by (bar, beat, verse), whether in
/// italics, and the labels before and ✻ after them. Empty without text (or for a chant track).
pub fn decorations(
    score: &Score,
    t: &Track,
) -> std::collections::HashMap<(usize, usize, usize), (bool, String, String)> {
    if t.lyrics_text.is_empty() || t.chant.is_some() {
        return Default::default();
    }
    layout_on(score, t, &t.lyrics_text)
        .placed
        .into_iter()
        .filter(|p| p.italic || !p.before.is_empty() || !p.after.is_empty())
        .map(|p| ((p.bar, p.beat, p.verse), (p.italic, p.before, p.after)))
        .collect()
}

/// [`layout`] on a track.
pub fn layout_on(score: &Score, t: &Track, text: &str) -> Layout {
    let tokens = tokens(text);
    let (occ, bars) = occurrences(score, t);
    let mut placed: Vec<Placed> = Vec::new();
    let mut unplaced = 0;
    let (mut k, mut layer) = (0usize, 0usize);
    let mut before = String::new();
    // Whether the last syllable token was placed (✻ goes after it).
    let mut last_placed = false;
    for (ti, tok) in tokens.iter().enumerate() {
        match &tok.kind {
            TokenKind::Anchor { bar, beat } => {
                let tick = ((*beat - 1.0) * crate::position::beat_ticks(score, *bar) as f64)
                    .round()
                    .max(0.0) as u32;
                let to = resolve(&occ, &bars, *bar, tick).unwrap_or(occ.len());
                if to < k {
                    layer += 1;
                }
                k = to;
            }
            TokenKind::Label(s) => {
                if !before.is_empty() {
                    before.push(' ');
                }
                before.push_str(s);
            }
            TokenKind::Star => match placed.last_mut().filter(|_| last_placed) {
                Some(p) => {
                    if !p.after.is_empty() {
                        p.after.push(' ');
                    }
                    p.after.push_str(STAR);
                }
                None => {
                    if !before.is_empty() {
                        before.push(' ');
                    }
                    before.push_str(STAR);
                }
            },
            TokenKind::Syllable {
                text,
                italic,
                hyphen,
                ..
            } => {
                last_placed = false;
                let Some(o) = occ.get(k) else {
                    unplaced += 1;
                    continue;
                };
                let mut shown = text.replace('+', " ");
                if *hyphen {
                    shown.push('-');
                }
                placed.push(Placed {
                    bar: o.bar,
                    beat: o.beat,
                    verse: o.row + layer,
                    text: shown,
                    italic: *italic,
                    before: std::mem::take(&mut before),
                    after: String::new(),
                    token: ti,
                });
                last_placed = true;
                k += 1;
            }
            TokenKind::Dash(n) | TokenKind::Skip(n) => {
                last_placed = false;
                k = (k + n).min(occ.len());
            }
            TokenKind::Notes(_) => {}
        }
    }
    Layout {
        placed,
        unplaced,
        tokens,
    }
}

/// The syllables a track's beats get from gabc+ `text`: by bar, beat (first voice) and verse.
pub fn beat_lyrics(score: &Score, track: usize, text: &str) -> (Vec<Vec<Vec<String>>>, Layout) {
    let lay = layout(score, track, text);
    let mut out: Vec<Vec<Vec<String>>> = score
        .tracks
        .get(track)
        .map(|t| {
            t.bars
                .iter()
                .map(|b| vec![Vec::new(); b.voices.first().map_or(0, |v| v.beats.len())])
                .collect()
        })
        .unwrap_or_default();
    for p in &lay.placed {
        let rows = &mut out[p.bar][p.beat];
        if rows.len() <= p.verse {
            rows.resize(p.verse + 1, String::new());
        }
        rows[p.verse] = p.text.clone();
    }
    (out, lay)
}

/// Lays a track's own gabc+ text onto its beats (first voice; other voices lose theirs).
/// Returns the layout.
pub fn apply(score: &mut Score, track: usize) -> Layout {
    let Some(text) = score.tracks.get(track).map(|t| t.lyrics_text.clone()) else {
        return Layout::default();
    };
    let (rows, lay) = beat_lyrics(score, track, &text);
    let t = &mut score.tracks[track];
    for (bar, beats) in t.bars.iter_mut().zip(rows) {
        for (v, voice) in bar.voices.iter_mut().enumerate() {
            for (i, beat) in voice.beats.iter_mut().enumerate() {
                beat.lyrics = if v == 0 {
                    beats.get(i).cloned().unwrap_or_default()
                } else {
                    Vec::new()
                };
            }
        }
    }
    lay
}

/// Lyrics added to a text by [`add_lyrics`].
#[derive(Debug, Clone, PartialEq)]
pub struct Added {
    /// The whole new text.
    pub text: String,
    /// The characters of it that were added (the anchor, the words and the line break).
    pub range: (usize, usize),
}

/// Adds `words` to track `track`'s gabc+ `text`, starting at `bar` (from 0) and beat number `beat`
/// (from 1, with fraction), as a line of its own of the form `<bar:beat> words`, ending in a line break.
///
/// The line goes where it belongs in the music: before the first syllable sung from there on,
/// at a word boundary, with an anchor after it that keeps that text where it was. When that
/// would move any syllable (the words run into the text after them, or there is no clean place
/// between its words), the line goes at the end instead, and its anchor going back in the music
/// puts the words on the next verse row.
pub fn add_lyrics(
    score: &Score,
    track: usize,
    text: &str,
    bar: usize,
    beat: f64,
    words: &str,
) -> Added {
    let line = format!(
        "<{}> {}\n",
        crate::position::format_position(bar, beat, true),
        words.trim_end()
    );
    let width = line.chars().count();
    let appended = || {
        let mut t = text.trim_end().to_string();
        if !t.is_empty() {
            t.push('\n');
        }
        let start = t.chars().count();
        t.push_str(&line);
        Added {
            text: t,
            range: (start, start + width),
        }
    };
    let Some(tr) = score.tracks.get(track) else {
        return appended();
    };
    let old = layout_on(score, tr, text);
    let Some(next) = old
        .placed
        .iter()
        .filter(|p| {
            p.bar > bar
                || (p.bar == bar
                    && crate::position::beat_position(score, tr, p.bar, 0, p.beat) >= beat - 1e-6)
        })
        .min_by_key(|p| p.token)
    else {
        return appended();
    };
    // The anchors and labels in front of that syllable belong to it.
    let mut k = next.token;
    let mut anchored = false;
    while k > 0
        && matches!(
            old.tokens[k - 1].kind,
            TokenKind::Anchor { .. } | TokenKind::Label(_)
        )
    {
        k -= 1;
        anchored |= matches!(old.tokens[k].kind, TokenKind::Anchor { .. });
    }
    let chars: Vec<char> = text.chars().collect();
    let at = old.tokens[k].span.0;
    if at > 0 && !chars[at - 1].is_whitespace() {
        return appended();
    }
    let mut head: String = chars[..at].iter().collect();
    head.truncate(head.trim_end_matches([' ', '\t']).len());
    if !head.is_empty() && !head.ends_with('\n') {
        head.push('\n');
    }
    // The text after keeps its place with an anchor, unless it has one.
    let keep = if anchored {
        String::new()
    } else {
        let pos = crate::position::beat_position(score, tr, next.bar, 0, next.beat);
        format!(
            "<{}> ",
            crate::position::format_position(next.bar, pos, true)
        )
    };
    let tail: String = chars[at..].iter().collect();
    let start = head.chars().count();
    let end = start + width + keep.chars().count();
    let cand = format!("{head}{line}{keep}{tail}");
    // Nothing that was there may have moved.
    let new = layout_on(score, tr, &cand);
    let inside = |t: &Token| t.span.0 >= start && t.span.1 <= end;
    let key = |p: &Placed| {
        (
            p.bar,
            p.beat,
            p.verse,
            p.text.clone(),
            p.italic,
            p.before.clone(),
            p.after.clone(),
        )
    };
    let was: Vec<_> = old.placed.iter().map(key).collect();
    let now: Vec<_> = new
        .placed
        .iter()
        .filter(|p| !inside(&new.tokens[p.token]))
        .map(key)
        .collect();
    let added = new
        .tokens
        .iter()
        .filter(|t| inside(t) && matches!(t.kind, TokenKind::Syllable { .. }))
        .count();
    let shown = new
        .placed
        .iter()
        .filter(|p| inside(&new.tokens[p.token]))
        .count();
    if was == now && new.unplaced == old.unplaced + (added - shown) {
        Added {
            text: cand,
            range: (start, end),
        }
    } else {
        appended()
    }
}

/// A syllable to write into gabc+ text: where it is sung (`None`: past the last note), its
/// verse row and its text (a trailing `-`: joined to the next by a hyphen).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Item {
    pub at: Option<(usize, usize)>,
    pub row: usize,
    pub text: String,
    /// A line break before it.
    pub newline: bool,
    /// A label shown before it.
    pub label: String,
}

/// A syllable as gabc+ text: spaces as `+`, signs that mean something else left out.
fn escape(text: &str) -> String {
    let hyphen = text.ends_with('-');
    let core: String = text
        .trim_end_matches('-')
        .trim()
        .trim_start_matches(['-', '_'])
        .chars()
        .map(|c| match c {
            ' ' => '+',
            '(' => '{',
            ')' => '}',
            '[' | ']' => '\'',
            '_' => '+',
            c => c,
        })
        .collect();
    if core.is_empty() || core == "*" || core.chars().all(|c| c == '-') {
        return String::new();
    }
    let mut core = core.replace('<', "‹");
    if hyphen {
        core.push('-');
    }
    core
}

/// Writes syllables as gabc+ text that lays them out where they are, one line of text per
/// `lines` entry: anchors where the music jumps, `_` (or dashes after a hyphen) for a few
/// beats left out. A syllable that can't be put on its own verse row keeps its beat.
pub fn emit(score: &Score, track: usize, lines: &[Vec<Item>]) -> String {
    const GAP: usize = 8;
    let Some(t) = score.tracks.get(track) else {
        return String::new();
    };
    let (occ, bars) = occurrences(score, t);
    let mut out = String::new();
    let (mut k, mut layer) = (0usize, 0usize);
    // The last syllable ended with a hyphen (nothing written after it yet: see `len`).
    let mut glue = false;
    let word = |out: &mut String, w: &str| {
        if !out.is_empty() && !out.ends_with(['\n', ' ']) {
            out.push(' ');
        }
        out.push_str(w);
    };
    let anchor = |o: &Occ| format!("<{}>", crate::position::format_position(o.bar, o.pos, true));
    // Labels of syllables without text go on the next one.
    let mut label = String::new();
    for line in lines {
        let mut first = true;
        for item in line {
            let text = escape(&item.text);
            if !item.label.is_empty() {
                if !label.is_empty() {
                    label.push(' ');
                }
                label.push_str(&item.label);
            }
            if text.is_empty() {
                continue;
            }
            let len = out.len();
            if !out.is_empty() && (first || item.newline) {
                out.push('\n');
            }
            // Where it goes, and how to get there.
            let at = item.at.and_then(|(b, i)| {
                let cands: Vec<usize> = (0..occ.len())
                    .filter(|&n| occ[n].bar == b && occ[n].beat == i)
                    .collect();
                let forward = cands
                    .iter()
                    .find(|&&n| n >= k && occ[n].row + layer == item.row);
                let back = cands.iter().find(|&&n| occ[n].row + layer + 1 == item.row);
                match (forward, back) {
                    (Some(&n), _) => Some((n, false)),
                    (None, Some(&n)) => Some((n, true)),
                    _ => cands
                        .iter()
                        .find(|&&n| n >= k)
                        .map(|&n| (n, false))
                        .or(cands.first().map(|&n| (n, true))),
                }
            });
            let filler = |out: &mut String, gap: usize, hyphen: bool| {
                if gap > 0 {
                    let c = if hyphen { "-" } else { "_" };
                    word(out, &c.repeat(gap));
                }
            };
            if let Some((n, back)) = at {
                let from = resolve(&occ, &bars, occ[n].bar, occ[n].tick).unwrap_or(n);
                if back || n < k {
                    if from >= k && k > 0 {
                        // Back to the start to begin a new row, then forward.
                        word(&mut out, "<1:1>");
                        layer += 1;
                        k = 0;
                    }
                    if from < k {
                        layer += 1;
                    }
                    if from != k || out.is_empty() {
                        word(&mut out, &anchor(&occ[from]));
                    }
                    k = from;
                }
                let gap = n.saturating_sub(k);
                if gap > 0 {
                    // A few beats are skipped; more (or at the start of a line) take an
                    // anchor as near the syllable as one reaches.
                    let near = (first || gap > GAP)
                        .then(|| {
                            (k + 1..=n)
                                .rev()
                                .find(|&m| resolve(&occ, &bars, occ[m].bar, occ[m].tick) == Some(m))
                        })
                        .flatten();
                    match near {
                        Some(m) => {
                            word(&mut out, &anchor(&occ[m]));
                            filler(&mut out, n - m, false);
                        }
                        None => {
                            let dash = glue && out.len() == len;
                            filler(&mut out, gap, dash)
                        }
                    }
                }
                k = n + 1;
            } else {
                k += 1;
            }
            // Glued to a syllable that ends in a hyphen right before it.
            if glue && out.len() == len && label.is_empty() {
                out.push_str(&text);
            } else {
                if !label.is_empty() {
                    word(&mut out, &format!("[{}]", std::mem::take(&mut label)));
                }
                word(&mut out, &text);
            }
            glue = text.ends_with('-');
            first = false;
        }
    }
    out
}

/// gabc+ text for the syllables a track's beats hold (first voice), one line per verse row.
pub fn text_from_beats(score: &Score, track: usize) -> String {
    let Some(t) = score.tracks.get(track) else {
        return String::new();
    };
    let rows = crate::lyrics::verse_count(t);
    let lines: Vec<Vec<Item>> = (0..rows)
        .map(|r| {
            t.bars
                .iter()
                .enumerate()
                .flat_map(|(b, bar)| {
                    bar.voices
                        .first()
                        .into_iter()
                        .flat_map(|v| v.beats.iter().enumerate())
                        .filter_map(move |(i, beat)| {
                            let s = beat.lyrics.get(r).filter(|s| !s.trim().is_empty())?;
                            Some(Item {
                                at: Some((b, i)),
                                row: r,
                                text: s.clone(),
                                ..Item::default()
                            })
                        })
                })
                .collect()
        })
        .collect();
    emit(score, track, &lines)
}

/// gabc+ text for a Guitar Pro 3–5 score's lyric lines, laid out as Guitar Pro does (from the
/// line's bar, one syllable per singing beat in bar order), keeping their line breaks and
/// `[comments]` (as labels).
pub fn text_from_lines(score: &Score) -> Option<(usize, String)> {
    let lyrics = score.lyrics.as_ref()?;
    let track = lyrics.track?;
    let t = score.tracks.get(track)?;
    let lines: Vec<Vec<Item>> = lyrics
        .lines
        .iter()
        .enumerate()
        .map(|(row, line)| {
            let chunks = crate::lyrics::chunks(&line.text);
            let beats = crate::lyrics::singing_beats(t, line.start_bar as usize);
            chunks
                .into_iter()
                .enumerate()
                .map(|(n, c)| Item {
                    at: beats.get(n).copied(),
                    row,
                    text: c.text,
                    newline: c.newline,
                    label: c.comment,
                })
                .collect()
        })
        .collect();
    Some((track, emit(score, track, &lines)))
}

/// Gives every track whose beats hold syllables but has no gabc+ text one written from them
/// (chant tracks keep their chant).
pub fn ensure_text(score: &mut Score) {
    for t in 0..score.tracks.len() {
        let track = &score.tracks[t];
        if track.lyrics_text.trim().is_empty()
            && track.chant.is_none()
            && crate::lyrics::has_lyrics(track)
        {
            let text = text_from_beats(score, t);
            score.tracks[t].lyrics_text = text;
        }
    }
}

/// gabc+ text shown without its GABC: what the lyrics text area shows with GABC off. Every
/// shown character keeps the range of the text it stands for; a group between two letters
/// shows as a hyphen (its range empty, after the group).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plain {
    pub text: String,
    /// For each shown character, its range in the full text.
    pub ranges: Vec<(usize, usize)>,
    /// The hidden groups: their range and the shown characters of the syllable they follow.
    hidden: Vec<((usize, usize), (usize, usize))>,
    /// Length of the full text in characters.
    len: usize,
}

/// The text without its GABC groups (and without the clef group it starts with and the
/// space after it).
pub fn plain(full: &str) -> Plain {
    let c: Vec<char> = full.chars().collect();
    let mut p = Plain {
        len: c.len(),
        ..Plain::default()
    };
    let sp = spans(full);
    let mut skip_space = false;
    for (n, &(a, b, kind)) in sp.iter().enumerate() {
        match kind {
            SpanKind::Gabc => {
                // The syllable it follows: shown letters back to a space or a dash.
                let end = p.text.chars().count();
                let shown: Vec<char> = p.text.chars().collect();
                let mut start = end;
                while start > 0
                    && !shown[start - 1].is_whitespace()
                    && shown[start - 1] != '-'
                    && p.ranges[start - 1].0 != p.ranges[start - 1].1
                {
                    start -= 1;
                }
                p.hidden.push(((a, b), (start, end)));
                if n == 0 || (n == 1 && sp[0].2 == SpanKind::Space) {
                    skip_space = true;
                    continue;
                }
                // Between two letters it shows as a hyphen.
                let before = shown
                    .last()
                    .is_some_and(|ch| !ch.is_whitespace() && *ch != '-');
                let after = sp
                    .get(n + 1)
                    .is_some_and(|s| s.2 == SpanKind::Text && c[s.0] != '-' && c[s.0] != '_');
                if before && after {
                    p.text.push('-');
                    p.ranges.push((b, b));
                }
            }
            _ => {
                for (i, &ch) in c.iter().enumerate().take(b).skip(a) {
                    if skip_space && kind == SpanKind::Space {
                        skip_space = false;
                        continue;
                    }
                    skip_space = false;
                    p.text.push(ch);
                    p.ranges.push((i, i + 1));
                }
            }
        }
    }
    p
}

impl Plain {
    /// The character of the full text a caret in the shown text stands at.
    pub fn to_full(&self, caret: usize) -> usize {
        match self.ranges.get(caret) {
            Some(r) => r.0,
            None => self.len,
        }
    }

    /// The caret in the shown text for a caret in the full text.
    pub fn from_full(&self, at: usize) -> usize {
        self.ranges
            .iter()
            .position(|r| r.0 >= at)
            .unwrap_or(self.ranges.len())
    }

    /// The full text after the shown text was changed to `shown`: the change goes in at its
    /// place; GABC groups stay unless every letter of their syllable was removed.
    pub fn edit(&self, full: &str, shown: &str) -> String {
        let old: Vec<char> = self.text.chars().collect();
        let new: Vec<char> = shown.chars().collect();
        let fc: Vec<char> = full.chars().collect();
        let p = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let max_s = old.len().min(new.len()) - p;
        let s = old
            .iter()
            .rev()
            .zip(new.iter().rev())
            .take(max_s)
            .take_while(|(a, b)| a == b)
            .count();
        let last = old.len() - s;
        let start = if p == 0 {
            self.ranges.first().map_or(self.len, |r| r.0)
        } else {
            self.ranges[p - 1].1
        };
        let end = if last == old.len() {
            self.len
        } else {
            self.ranges[last].0
        }
        .max(start);
        let mut out: String = fc[..start].iter().collect();
        out.extend(&new[p..new.len() - s]);
        for &((a, b), (o0, o1)) in &self.hidden {
            if a >= start && b <= end {
                let gone = o1 > o0 && o0 >= p && o1 <= last;
                if !gone {
                    out.extend(&fc[a..b]);
                }
            }
        }
        out.extend(&fc[end..]);
        out
    }
}

/// gabc+ text with a clef group at its start: `(c4)` unless it has one.
pub fn with_clef(text: &str, clef: &str) -> String {
    let t = text.trim_start();
    if t.starts_with('(') && spans(t).first().is_some_and(|s| s.2 == SpanKind::Gabc) {
        return text.to_string();
    }
    if t.is_empty() {
        format!("({clef}) ")
    } else {
        format!("({clef}) {text}")
    }
}

/// Removes the syllables (and the dashes and gaps) laid on verse row `verse`, with the labels
/// and ✻ that go with them.
pub fn remove_verse(score: &Score, track: usize, text: &str, verse: usize) -> String {
    let lay = layout(score, track, text);
    let mut gone = vec![false; lay.tokens.len()];
    for p in lay.placed.iter().filter(|p| p.verse == verse) {
        gone[p.token] = true;
        // Labels before it, ✻ and dashes after it.
        let mut k = p.token;
        while k > 0 && matches!(lay.tokens[k - 1].kind, TokenKind::Label(_)) {
            k -= 1;
            gone[k] = true;
        }
        let mut k = p.token + 1;
        while k < lay.tokens.len()
            && matches!(
                lay.tokens[k].kind,
                TokenKind::Star | TokenKind::Dash(_) | TokenKind::Skip(_)
            )
        {
            gone[k] = true;
            k += 1;
        }
    }
    let c: Vec<char> = text.chars().collect();
    let mut keep = vec![true; c.len()];
    for (t, g) in lay.tokens.iter().zip(&gone) {
        if *g {
            for x in &mut keep[t.span.0..t.span.1] {
                *x = false;
            }
        }
    }
    let kept: String = c
        .iter()
        .zip(&keep)
        .filter(|(_, k)| **k)
        .map(|(c, _)| *c)
        .collect();
    // Spaces left doubled are one again; lines are kept.
    kept.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    fn syl(t: &Token) -> Option<(&str, bool, bool)> {
        match &t.kind {
            TokenKind::Syllable {
                text,
                italic,
                hyphen,
                ..
            } => Some((text.as_str(), *italic, *hyphen)),
            _ => None,
        }
    }

    /// Beats each word takes, from the spec's examples.
    fn beats(text: &str) -> usize {
        tokens(text)
            .iter()
            .map(|t| match t.kind {
                TokenKind::Syllable { .. } => 1,
                TokenKind::Dash(n) | TokenKind::Skip(n) => n,
                _ => 0,
            })
            .sum()
    }

    /// Whether a hyphen joins the first syllable to the next.
    fn dashed(text: &str) -> bool {
        tokens(text).iter().find_map(syl).unwrap().2
    }

    #[test]
    fn words_dashes_and_skips_take_beats() {
        assert_eq!(beats("someone"), 1);
        for t in ["some one", "some_one"] {
            assert_eq!(beats(t), 2, "{t}");
            assert!(!dashed(t), "{t}");
        }
        for t in ["some-one", "some--one", "some- one", "some -one"] {
            assert_eq!(beats(t), 2, "{t}");
            assert!(dashed(t), "{t}");
        }
        assert_eq!(beats("some - one"), 3);
        assert!(dashed("some - one"));
        assert_eq!(beats("some -- one"), 4);
        assert_eq!(beats("some _ one"), 3);
        assert!(!dashed("some _ one"));
        assert_eq!(beats("some __ one"), 4);
        // Newlines and runs of spaces only separate.
        assert_eq!(beats("some  \n\n  one"), 2);
    }

    #[test]
    fn italics_labels_stars_anchors_and_gabc() {
        let t = tokens("<2:3> [1.] _si-lence * Ky(f)ri(gh)e(h) a-_men (,) (c4)");
        assert_eq!(t[0].kind, TokenKind::Anchor { bar: 1, beat: 3.0 });
        assert_eq!(t[1].kind, TokenKind::Label("1.".into()));
        assert_eq!(syl(&t[2]), Some(("si", true, true)));
        assert_eq!(syl(&t[3]), Some(("lence", false, false)));
        assert_eq!(t[4].kind, TokenKind::Star);
        assert_eq!(syl(&t[5]), Some(("Ky", false, true)));
        assert!(matches!(&t[5].kind, TokenKind::Syllable { gabc, .. } if gabc == "f"));
        assert_eq!(syl(&t[6]), Some(("ri", false, true)));
        assert_eq!(syl(&t[7]), Some(("e", false, false)));
        assert_eq!(syl(&t[8]), Some(("a", false, true)));
        assert_eq!(syl(&t[9]), Some(("men", true, false)));
        assert_eq!(t[10].kind, TokenKind::Notes("(,)".into()));
        assert_eq!(t[11].kind, TokenKind::Notes("(c4)".into()));
        // Spans cover the text by kind.
        let s = spans("(c4) a[1.]<1:2>(");
        let kinds: Vec<SpanKind> = s.iter().map(|s| s.2).collect();
        use SpanKind::*;
        assert_eq!(kinds, [Gabc, Space, Text, Label, Anchor, Text]);
        // The spec's examples with gabc.
        assert_eq!(beats("some(g)---one(e)"), 2);
        assert!(dashed("some(g)---one(e)"));
        assert_eq!(beats("some(gh) -- one(e)"), 4);
    }

    fn quarter(p: u8) -> Beat {
        Beat {
            duration: Duration::new(DurationValue::Quarter),
            notes: if p == 0 {
                vec![]
            } else {
                vec![Note::new(0, p)]
            },
            ..Beat::default()
        }
    }

    /// A score of `n` bars of four beats (the first a rest when `rest`).
    fn score(n: usize, rest: bool) -> Score {
        let bar = |b: usize| Bar {
            voices: vec![Voice {
                beats: (0..4)
                    .map(|i| quarter(if rest && b == 0 && i == 0 { 0 } else { 60 }))
                    .collect(),
            }],
            ..Default::default()
        };
        Score {
            master_bars: vec![MasterBar::default(); n],
            tracks: vec![Track {
                kind: TrackKind::Pitched,
                bars: (0..n).map(bar).collect(),
                ..Track::default()
            }],
            ..Score::default()
        }
    }

    fn at(s: &Score, text: &str) -> Vec<(usize, usize, usize, String)> {
        layout(s, 0, text)
            .placed
            .into_iter()
            .map(|p| (p.bar, p.beat, p.verse, p.text))
            .collect()
    }

    #[test]
    fn syllables_start_at_the_first_singing_beat_and_follow_anchors() {
        let s = score(3, true);
        let lay = layout(&s, 0, "<1:1> [1.] Let all mor-tal flesh keep _ si-lence,");
        let p = &lay.placed;
        // 1:1 is a rest: the label and "Let" go on the first note.
        assert_eq!((p[0].bar, p[0].beat, p[0].before.as_str()), (0, 1, "1."));
        assert_eq!(p[0].text, "Let");
        assert_eq!(p[2].text, "mor-");
        // `_` leaves a beat: "si-" is two beats after "keep".
        let keep = p.iter().position(|x| x.text == "keep").unwrap();
        assert_eq!((p[keep].bar, p[keep].beat), (1, 2));
        assert_eq!((p[keep + 1].bar, p[keep + 1].beat), (2, 0));
        // An anchor further on; one going back starts the next verse row.
        assert_eq!(at(&s, "a <2:2> b")[1], (1, 1, 0, "b".into()));
        let back = at(&s, "a b c <1:1> d");
        assert_eq!(back[3], (0, 1, 1, "d".into()));
        // Past the last note: kept, not shown.
        let many = vec!["la"; 20].join(" ");
        assert_eq!(layout(&s, 0, &many).unplaced, 9);
        // ✻ goes after the syllable before it.
        let star = layout(&s, 0, "Ky-ri-e * e-le-i-son");
        assert_eq!(star.placed[2].after, STAR);
        assert_eq!(star.placed[3].text, "e-");
    }

    #[test]
    fn syllables_follow_repeats_endings_and_directions() {
        // Bars 1–2 repeated with a first ending in bar 3 and a second in bar 4.
        let mut s = score(5, false);
        s.master_bars[0].repeat_start = true;
        s.master_bars[2].alternate_endings = 1;
        s.master_bars[2].repeat_end = Some(2);
        s.master_bars[3].alternate_endings = 2;
        let order: Vec<usize> = sung_order(&s).iter().map(|o| o.0).collect();
        assert_eq!(order, [0, 1, 2, 0, 1, 3, 4]);
        let rows: Vec<usize> = sung_order(&s).iter().map(|o| o.1).collect();
        assert_eq!(rows, [0, 0, 0, 1, 1, 1, 0]);
        let words: Vec<String> = (0..28).map(|i| format!("w{i}")).collect();
        let p = at(&s, &words.join(" "));
        // The second pass sings the next words on the second row.
        assert_eq!(p[12], (0, 0, 1, "w12".into()));
        assert_eq!(p[20], (3, 0, 1, "w20".into()));
        assert_eq!(p[24], (4, 0, 0, "w24".into()));
        // D.C. al Fine: back to the start after bar 3, ending at the Fine in bar 2.
        let mut s = score(3, false);
        s.master_bars[1].directions = vec![Direction::Fine];
        s.master_bars[2].directions = vec![Direction::DaCapoAlFine];
        let order: Vec<usize> = sung_order(&s).iter().map(|o| o.0).collect();
        assert_eq!(order, [0, 1, 2, 0, 1]);
        // D.S. al Coda: from the segno to To Coda, then the coda.
        let mut s = score(5, false);
        s.master_bars[1].directions = vec![Direction::Segno];
        s.master_bars[2].directions = vec![Direction::DaCoda];
        s.master_bars[3].directions = vec![Direction::DaSegnoAlCoda];
        s.master_bars[4].directions = vec![Direction::Coda];
        let order: Vec<usize> = sung_order(&s).iter().map(|o| o.0).collect();
        assert_eq!(order, [0, 1, 2, 3, 1, 2, 4]);
    }

    #[test]
    fn beats_text_round_trips() {
        let mut s = score(4, true);
        s.master_bars[1].repeat_start = true;
        s.master_bars[2].repeat_end = Some(2);
        let text = "<1:1> [1.] Hel-lo there my _ friend\n\nsec-ond verse goes here and on";
        s.tracks[0].lyrics_text = text.into();
        apply(&mut s, 0);
        let before = s.clone();
        let again = text_from_beats(&s, 0);
        s.tracks[0].lyrics_text = again.clone();
        apply(&mut s, 0);
        let lyr = |s: &Score| -> Vec<Vec<String>> {
            s.tracks[0]
                .bars
                .iter()
                .flat_map(|b| b.voices[0].beats.iter().map(|b| b.lyrics.clone()))
                .collect()
        };
        assert_eq!(lyr(&s), lyr(&before), "{again}");
        // Lines from beats that need another row on bars sung once.
        let mut s = score(3, false);
        s.tracks[0].bars[0].voices[0].beats[0].lyrics = vec!["a".into(), "b".into()];
        s.tracks[0].bars[2].voices[0].beats[3].lyrics = vec!["".into(), "c".into()];
        let before = lyr(&s);
        s.tracks[0].lyrics_text = text_from_beats(&s, 0);
        apply(&mut s, 0);
        assert_eq!(lyr(&s), before, "{}", s.tracks[0].lyrics_text);
    }

    #[test]
    fn plain_text_hides_gabc_and_takes_edits_back() {
        let full = "(c4) Pó(g)pu(h)lus(i) some(g) one(e) two";
        let p = plain(full);
        assert_eq!(p.text, "Pó-pu-lus some one two");
        // Typing at the end of a word goes before its group.
        let typed = p.text.replace("some", "somes");
        assert_eq!(
            p.edit(full, &typed),
            "(c4) Pó(g)pu(h)lus(i) somes(g) one(e) two"
        );
        // Changing part of a syllable keeps its notes; removing it removes them.
        assert_eq!(
            p.edit(full, &p.text.replace("one", "on")),
            "(c4) Pó(g)pu(h)lus(i) some(g) on(e) two"
        );
        assert_eq!(
            p.edit(full, &p.text.replace(" one", "")),
            "(c4) Pó(g)pu(h)lus(i) some(g) two"
        );
        // At the very start the text goes after the clef.
        assert_eq!(
            p.edit(full, &format!("X{}", p.text)),
            "(c4) XPó(g)pu(h)lus(i) some(g) one(e) two"
        );
        assert_eq!(plain("(c4) ").text, "");
        assert_eq!(plain("(c4) ").edit("(c4) ", "la"), "(c4) la");
        // Carets map both ways.
        assert_eq!(p.to_full(0), 5);
        assert_eq!(p.from_full(5), 0);
        assert_eq!(with_clef("la", "c4"), "(c4) la");
        assert_eq!(with_clef("(f3) la", "c4"), "(f3) la");
    }

    #[test]
    fn a_verse_row_can_be_removed() {
        let s = score(2, false);
        let text = "[1.] a b c\n<1:1> [2.] d e";
        assert_eq!(remove_verse(&s, 0, text, 1), "[1.] a b c\n<1:1>");
        assert_eq!(remove_verse(&s, 0, text, 0), "<1:1> [2.] d e");
    }

    #[test]
    fn highlighting_tells_the_parts_of_gabc_apart() {
        use Hl::*;
        let kinds = |t: &str| highlight(t);
        assert_eq!(kinds("(c3)"), [Paren, Clef, Clef, Paren]);
        assert_eq!(kinds("(cb2)"), [Paren, Clef, Clef, Clef, Paren]);
        assert_eq!(
            kinds("a(gv/h.:)"),
            [Text, Paren, Note, Shape, Gap, Note, Attach, Division, Paren]
        );
        assert_eq!(kinds("(;3)"), [Paren, Division, Division, Paren]);
        assert_eq!(
            kinds("(gr0r2)"),
            [Paren, Note, Attach, Attach, Attach, Attach, Paren]
        );
        assert_eq!(kinds("(fg)"), [Paren, Note, Note, Paren]);
        assert_eq!(
            kinds("<b>x</b>"),
            [Markup, Markup, Markup, Text, Markup, Markup, Markup, Markup]
        );
        assert_eq!(kinds("<2:3.5>")[0], Anchor);
        assert_eq!(kinds("[1.]")[0], Label);
    }
}
