//! Lyrics as one text per track, the single source of the syllables on its beats.
//!
//! A modern track's text is written in a small syntax (a chant track's lyrics are its GABC
//! instead: see [`gabc_spans`] and [`highlight_gabc`]). Newlines and runs of spaces only
//! separate words. In it:
//!
//! - a word is sung one syllable per beat; `-` inside a word splits it into syllables joined
//!   by a hyphen (`mor-tal`, `some--one`); a word starting with `-` joins the one before it
//!   (`some -one`);
//! - `_` between letters separates words (`some_one` is `some one`);
//! - a word of dashes (`--`) covers that many beats under one dash; a word of underscores
//!   (`__`) leaves that many beats without text;
//! - `+` joins words on one beat (`a+b` is shown `a b`);
//! - `[text]` is shown before the next syllable without taking a beat (verse numbers);
//! - `*` alone is printed as ✻ after the previous syllable without taking a beat;
//! - `(bar:beat)` or `(bar:beat.fraction)` (1-based, as in the status line) starts the
//!   following syllables at the first singing beat from there; anchors that go back in the
//!   music start a new line of lyrics (the next verse row);
//! - GABC's markup tags `<i>` `<b>` `<ul>` `<sc>` `<c>` style the text they enclose (italic,
//!   bold, underlined, small capitals, red); they take no beat, may span words and nest, and a
//!   tag that is unknown or never closed is ordinary text; `<sp>V/</sp>` and the like are
//!   shown as the character they stand for (℣).
//!
//! Syllables are sung in the order the music is played: repeats, alternate endings and
//! directions (D.C., D.S., al Fine, al Coda) are followed, from the first singing beat (one
//! with a note, tied notes included, as in Guitar Pro) unless an anchor says otherwise. A beat
//! sung on the second pass of a repeat shows its syllable on the second verse row, and so on.

use crate::{Direction, Score, Track};

/// How a syllable's text is drawn, from the markup tags around it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Style {
    /// `<i>`.
    pub italic: bool,
    /// `<b>`.
    pub bold: bool,
    /// `<ul>`.
    pub underline: bool,
    /// `<sc>`: lower-case letters as smaller capitals.
    pub small_caps: bool,
    /// `<c>`: red, as GABC's rubrics.
    pub red: bool,
}

impl Style {
    pub fn is_plain(&self) -> bool {
        *self == Style::default()
    }

    fn or(&mut self, o: Style) {
        self.italic |= o.italic;
        self.bold |= o.bold;
        self.underline |= o.underline;
        self.small_caps |= o.small_caps;
        self.red |= o.red;
    }
}

/// A piece of lyrics text, by character range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    /// Lyric text.
    Text,
    /// `(…)`: GABC notes (chant text only).
    Gabc,
    /// `[…]`: a label shown before a syllable.
    Label,
    /// `(bar:beat)`.
    Anchor,
    /// A markup tag (`<i>`, `</i>`).
    Markup,
    /// Spaces and line breaks.
    Space,
}

/// The markup tags of lyrics (`sp` is the special-character one).
const TAGS: [&str; 6] = ["i", "b", "ul", "sc", "c", "sp"];

/// The tag starting at `i` (`<i>`, `</ul>`): its name, whether it closes, and one past its end.
fn tag_at(c: &[char], i: usize) -> Option<(&'static str, bool, usize)> {
    if c.get(i) != Some(&'<') {
        return None;
    }
    let close = c.get(i + 1) == Some(&'/');
    let from = i + 1 + close as usize;
    let end = (from..c.len().min(from + 3)).find(|&k| c[k] == '>')?;
    let name: String = c[from..end].iter().collect();
    TAGS.iter()
        .find(|t| **t == name)
        .map(|t| (*t, close, end + 1))
}

/// What a segment of modern lyrics text is.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Seg {
    Space,
    Label,
    Anchor { bar: usize, beat: f64 },
    Tag { name: &'static str, close: bool },
    Char,
}

/// The segments of a text: spaces, labels, anchors, tags that have their partner, and the
/// characters of everything else, covering the text in order.
fn scan(c: &[char]) -> Vec<(usize, usize, Seg)> {
    let mut out: Vec<(usize, usize, Seg)> = Vec::new();
    let mut tags: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        let find = |end: char| c[i + 1..].iter().position(|&x| x == end).map(|p| i + 1 + p);
        if ch.is_whitespace() {
            out.push((i, i + 1, Seg::Space));
            i += 1;
        } else if let Some(end) = (ch == '[').then(|| find(']')).flatten() {
            out.push((i, end + 1, Seg::Label));
            i = end + 1;
        } else if let Some((end, (bar, beat))) =
            (ch == '(').then(|| find(')')).flatten().and_then(|end| {
                let inner: String = c[i + 1..end].iter().collect();
                crate::position::parse_position(&inner).map(|p| (end, p))
            })
        {
            out.push((i, end + 1, Seg::Anchor { bar, beat }));
            i = end + 1;
        } else if let Some((name, close, end)) = tag_at(c, i) {
            tags.push(out.len());
            out.push((i, end, Seg::Tag { name, close }));
            i = end;
        } else {
            out.push((i, i + 1, Seg::Char));
            i += 1;
        }
    }
    // A tag counts when it is paired: a closing one with the latest open one of its name.
    let mut open: Vec<(&str, usize)> = Vec::new();
    let mut paired = vec![false; out.len()];
    for &k in &tags {
        let Seg::Tag { name, close } = out[k].2 else {
            continue;
        };
        if !close {
            open.push((name, k));
        } else if let Some(p) = open.iter().rposition(|o| o.0 == name) {
            paired[k] = true;
            paired[open[p].1] = true;
            open.remove(p);
        }
    }
    let mut result = Vec::with_capacity(out.len());
    for (k, (a, b, seg)) in out.into_iter().enumerate() {
        if matches!(seg, Seg::Tag { .. }) && !paired[k] {
            result.extend((a..b).map(|x| (x, x + 1, Seg::Char)));
        } else {
            result.push((a, b, seg));
        }
    }
    result
}

/// The pieces of a modern track's text, as (first char, one past the last, kind), in order and
/// covering it. An unclosed `(` or `[`, a `(` that starts no position, and an unknown or
/// unpaired tag are text.
pub fn spans(text: &str) -> Vec<(usize, usize, SpanKind)> {
    let c: Vec<char> = text.chars().collect();
    let mut out: Vec<(usize, usize, SpanKind)> = Vec::new();
    for (a, b, seg) in scan(&c) {
        let kind = match seg {
            Seg::Space => SpanKind::Space,
            Seg::Label => SpanKind::Label,
            Seg::Anchor { .. } => SpanKind::Anchor,
            Seg::Tag { .. } => SpanKind::Markup,
            Seg::Char => SpanKind::Text,
        };
        match out.last_mut() {
            Some(last)
                if last.2 == kind
                    && last.1 == a
                    && matches!(kind, SpanKind::Text | SpanKind::Space) =>
            {
                last.1 = b
            }
            _ => out.push((a, b, kind)),
        }
    }
    out
}

/// The pieces of a chant track's text (GABC): `(…)` groups, `[…]` labels, spaces and the rest.
/// An unclosed `(` or `[` is text.
pub fn gabc_spans(text: &str) -> Vec<(usize, usize, SpanKind)> {
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
        } else {
            push(&mut out, i, i + 1, SpanKind::Text);
            i += 1;
        }
    }
    out
}

/// What a character of lyrics text is, for coloring it (see [`highlight`]).
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

/// The kind of each character of a modern track's text, for coloring: words, markup tags,
/// labels and anchors.
pub fn highlight(text: &str) -> Vec<Hl> {
    let mut out = vec![Hl::Text; text.chars().count()];
    for (a, z, kind) in spans(text) {
        let hl = match kind {
            SpanKind::Text | SpanKind::Gabc => continue,
            SpanKind::Space => Hl::Space,
            SpanKind::Label => Hl::Label,
            SpanKind::Anchor => Hl::Anchor,
            SpanKind::Markup => Hl::Markup,
        };
        out[a..z].fill(hl);
    }
    out
}

/// The kind of each character of a chant track's text (GABC), for coloring: lyrics, the parts
/// of GABC groups, markup tags and labels.
pub fn highlight_gabc(text: &str) -> Vec<Hl> {
    let c: Vec<char> = text.chars().collect();
    let mut out = vec![Hl::Text; c.len()];
    for (a, z, kind) in gabc_spans(text) {
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
            SpanKind::Anchor | SpanKind::Markup => {}
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

/// What a token of lyrics text does.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// A sung syllable: one beat. `hyphen`: joined to the next by a hyphen. `text` has no
    /// markup (specials resolved); `style` is what the tags around it say.
    Syllable {
        text: String,
        style: Style,
        hyphen: bool,
    },
    /// A word of dashes: that many beats under the dash to the next syllable.
    Dash(usize),
    /// A word of underscores: that many beats without text.
    Skip(usize),
    /// `[text]`, shown before the next syllable.
    Label(String),
    /// `*`, shown as ✻ after the previous syllable.
    Star,
    /// `(bar:beat.fraction)`: the bar from 0, the beat number from 1 (fraction included).
    Anchor { bar: usize, beat: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    /// Characters of the text it was read from.
    pub span: (usize, usize),
}

/// One character of a word, with the characters of the text it came from and its style.
struct Piece {
    from: usize,
    to: usize,
    ch: char,
    style: Style,
}

/// The character (or characters) a `<sp>…</sp>` special stands for.
fn special(name: &str) -> String {
    crate::chant::special(name).unwrap_or(name).to_string()
}

/// Reads a modern track's text into tokens.
pub fn tokens(text: &str) -> Vec<Token> {
    let c: Vec<char> = text.chars().collect();
    let segs = scan(&c);
    let mut out: Vec<Token> = Vec::new();
    let mut word: Vec<Piece> = Vec::new();
    // How many tags of each kind are open: i, b, ul, sc, c.
    let mut depth = [0i32; 5];
    let style = |depth: &[i32; 5]| Style {
        italic: depth[0] > 0,
        bold: depth[1] > 0,
        underline: depth[2] > 0,
        small_caps: depth[3] > 0,
        red: depth[4] > 0,
    };
    let mut k = 0;
    while k < segs.len() {
        let (a, b, seg) = segs[k];
        k += 1;
        match seg {
            Seg::Char => word.push(Piece {
                from: a,
                to: b,
                ch: c[a],
                style: style(&depth),
            }),
            Seg::Space => finish_word(&mut out, &mut word),
            Seg::Label => {
                finish_word(&mut out, &mut word);
                out.push(Token {
                    kind: TokenKind::Label(
                        c[a + 1..b - 1].iter().collect::<String>().trim().into(),
                    ),
                    span: (a, b),
                });
            }
            Seg::Anchor { bar, beat } => {
                finish_word(&mut out, &mut word);
                out.push(Token {
                    kind: TokenKind::Anchor { bar, beat },
                    span: (a, b),
                });
            }
            Seg::Tag {
                name: "sp",
                close: false,
            } => {
                // The characters up to `</sp>` are one special.
                let end = segs[k..]
                    .iter()
                    .position(|s| {
                        matches!(
                            s.2,
                            Seg::Tag {
                                name: "sp",
                                close: true
                            }
                        )
                    })
                    .map(|p| k + p);
                if let Some(end) = end {
                    let inner: String = c[b..segs[end].0].iter().collect();
                    let st = style(&depth);
                    word.extend(special(&inner).chars().map(|ch| Piece {
                        from: a,
                        to: segs[end].1,
                        ch,
                        style: st,
                    }));
                    k = end + 1;
                }
            }
            Seg::Tag { name, close } => {
                let i = match name {
                    "i" => 0,
                    "b" => 1,
                    "ul" => 2,
                    "sc" => 3,
                    "c" => 4,
                    _ => continue,
                };
                depth[i] = (depth[i] + if close { -1 } else { 1 }).max(0);
            }
        }
    }
    finish_word(&mut out, &mut word);
    // A dash word joins the syllable before it to the next one.
    for k in 1..out.len() {
        if matches!(out[k].kind, TokenKind::Dash(_))
            && let Some(TokenKind::Syllable { hyphen, .. }) = out[..k]
                .iter_mut()
                .rev()
                .map(|t| &mut t.kind)
                .find(|t| !matches!(t, TokenKind::Label(_) | TokenKind::Star))
        {
            *hyphen = true;
        }
    }
    out
}

fn finish_word(out: &mut Vec<Token>, word: &mut Vec<Piece>) {
    if word.is_empty() {
        return;
    }
    let pieces = std::mem::take(word);
    let (start, end) = (pieces[0].from, pieces.last().unwrap().to);
    let chars: String = pieces.iter().map(|p| p.ch).collect();
    let simple = |kind: TokenKind| Token {
        kind,
        span: (start, end),
    };
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
        return;
    }
    // Syllables of a word.
    struct Cur {
        text: String,
        style: Style,
        start: Option<usize>,
        end: usize,
    }
    let fresh = || Cur {
        text: String::new(),
        style: Style::default(),
        start: None,
        end: 0,
    };
    let mut cur = fresh();
    // What the previous character did: nothing yet, a dash, a separator.
    #[derive(PartialEq, Clone, Copy)]
    enum Prev {
        Start,
        Dash,
        Separator,
        Other,
    }
    let mut prev = Prev::Start;
    let done = |out: &mut Vec<Token>, cur: &mut Cur, hyphen: bool| {
        let c = std::mem::replace(cur, fresh());
        let Some(s) = c.start else {
            return;
        };
        out.push(Token {
            kind: TokenKind::Syllable {
                text: c.text,
                style: c.style,
                hyphen,
            },
            span: (s, c.end),
        });
    };
    for p in &pieces {
        match p.ch {
            '-' => {
                if cur.start.is_some() {
                    done(out, &mut cur, true);
                } else if prev == Prev::Start {
                    // `-one`: the syllable before goes on with a hyphen.
                    if let Some(TokenKind::Syllable { hyphen, .. }) = out
                        .iter_mut()
                        .rev()
                        .map(|t| &mut t.kind)
                        .find(|t| !matches!(t, TokenKind::Label(_) | TokenKind::Star))
                    {
                        *hyphen = true;
                    }
                }
                prev = Prev::Dash;
            }
            '_' => {
                done(out, &mut cur, false);
                prev = Prev::Separator;
            }
            ch => {
                cur.start.get_or_insert(p.from);
                cur.text.push(ch);
                cur.style.or(p.style);
                cur.end = p.to;
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
    /// How its text is drawn (from the markup tags around it).
    pub style: Style,
    /// Labels shown before it.
    pub before: String,
    /// Shown after it (✻).
    pub after: String,
    /// Index of its token.
    pub token: usize,
}

/// lyrics text laid out on a track.
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

/// How a syllable is shown besides its text: its style and the labels before and ✻ after it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Deco {
    pub style: Style,
    pub before: String,
    pub after: String,
}

/// How the syllables of a track's own text are shown: by (bar, beat, verse), their style and
/// the labels before and ✻ after them. Empty without text (or for a chant track).
pub fn decorations(
    score: &Score,
    t: &Track,
) -> std::collections::HashMap<(usize, usize, usize), Deco> {
    if t.lyrics_text.is_empty() || t.chant.is_some() {
        return Default::default();
    }
    layout_on(score, t, &t.lyrics_text)
        .placed
        .into_iter()
        .filter(|p| !p.style.is_plain() || !p.before.is_empty() || !p.after.is_empty())
        .map(|p| {
            (
                (p.bar, p.beat, p.verse),
                Deco {
                    style: p.style,
                    before: p.before,
                    after: p.after,
                },
            )
        })
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
                style,
                hyphen,
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
                    style: *style,
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

/// Lays a track's own lyrics text onto its beats (first voice; other voices lose theirs).
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

/// Adds `words` to track `track`'s `text`, starting at `bar` (from 0) and beat number `beat`
/// (from 1, with fraction), as a line of its own of the form `(bar:beat) words`, ending in a line break.
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
        "({}) {}\n",
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
            "({}) ",
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
            p.style,
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

/// A syllable to write into lyrics text: where it is sung (`None`: past the last note), its
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

/// A syllable as lyrics text: spaces as `+`, signs that mean something else left out.
fn escape(text: &str) -> String {
    let hyphen = text.ends_with('-');
    let core: String = text
        .trim_end_matches('-')
        .trim()
        .trim_start_matches(['-', '_'])
        .chars()
        .map(|c| match c {
            ' ' => '+',
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

/// Writes syllables as lyrics text that lays them out where they are, one line of text per
/// `lines` entry: anchors where the music jumps, `_` for a few beats left out. A syllable that can't be put on its own verse row keeps its beat.
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
    let anchor = |o: &Occ| format!("({})", crate::position::format_position(o.bar, o.pos, true));
    // Labels of syllables without text go on the next one.
    let mut label = String::new();
    // A line break on a note without a syllable goes on to the next syllable.
    let mut newline = false;
    for line in lines {
        let mut first = true;
        for item in line {
            let text = escape(&item.text);
            newline |= item.newline;
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
            let broken = std::mem::take(&mut newline);
            if !out.is_empty() && (first || broken) {
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
            let filler = |out: &mut String, gap: usize| {
                if gap > 0 {
                    word(out, &"_".repeat(gap));
                }
            };
            if let Some((n, back)) = at {
                let from = resolve(&occ, &bars, occ[n].bar, occ[n].tick).unwrap_or(n);
                if back || n < k {
                    if from >= k && k > 0 {
                        // Back to the start to begin a new row, then forward.
                        word(&mut out, "(1:1)");
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
                            filler(&mut out, n - m);
                        }
                        None => filler(&mut out, gap),
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

/// lyrics text for the syllables a track's beats hold (first voice), one line per verse row.
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

/// lyrics text for a Guitar Pro 3–5 score's lyric lines, laid out as Guitar Pro does (from the
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

/// Guitar Pro 3–5 lyric lines made from a track's text, and how many verse rows did not fit.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GpLines {
    pub lines: Vec<crate::LyricLine>,
    /// Rows beyond the five lines Guitar Pro stores (left out).
    pub dropped: usize,
}

/// The track Guitar Pro 3–5 lyrics are written from and its lines: the score's lyric track if it
/// has text, else the first track with text (chant tracks have none of their own).
pub fn gp_lyrics(score: &Score) -> Option<(usize, GpLines)> {
    let has = |i: usize| {
        score
            .tracks
            .get(i)
            .is_some_and(|t| t.chant.is_none() && !t.lyrics_text.trim().is_empty())
    };
    let track = score
        .lyrics
        .as_ref()
        .and_then(|l| l.track)
        .filter(|&i| has(i))
        .or_else(|| (0..score.tracks.len()).find(|&i| has(i)))?;
    Some((track, to_gp_lines(score, track)))
}

/// A track's text as Guitar Pro 3–5 lyric lines, translating it back to GP's syntax: one line
/// per verse row (an anchor going back starts the next), starting at the bar of its first
/// syllable, at most five. Line breaks stay where they are; words and hyphens are as written
/// (`+` too); notes without a syllable (`_` words, dashes, jumps forward) are `_` words, which
/// read back as empty notes; labels are `[comments]`; markup and ✻ are left out.
pub fn to_gp_lines(score: &Score, track: usize) -> GpLines {
    let Some(t) = score.tracks.get(track) else {
        return GpLines::default();
    };
    let lay = layout_on(score, t, &t.lyrics_text);
    let chars: Vec<char> = t.lyrics_text.chars().collect();
    struct Chunk {
        text: String,
        comment: String,
        newline: bool,
    }
    struct Row {
        start: usize,
        /// The notes Guitar Pro sings from the start bar, in bar order.
        slots: Vec<(usize, usize)>,
        next: usize,
        chunks: Vec<Chunk>,
    }
    let mut rows: std::collections::BTreeMap<usize, Row> = Default::default();
    let by_token: std::collections::HashMap<usize, &Placed> =
        lay.placed.iter().map(|p| (p.token, p)).collect();
    let mut labels: Vec<String> = Vec::new();
    let mut newline = false;
    let mut prev_end = 0;
    for (ti, tok) in lay.tokens.iter().enumerate() {
        if prev_end < tok.span.0 && chars[prev_end..tok.span.0].contains(&'\n') {
            newline = true;
        }
        prev_end = tok.span.1;
        match &tok.kind {
            TokenKind::Label(s) => labels.push(s.clone()),
            TokenKind::Syllable { .. } => {
                let Some(p) = by_token.get(&ti) else {
                    continue;
                };
                let row = rows.entry(p.verse).or_insert_with(|| Row {
                    start: p.bar,
                    slots: crate::lyrics::singing_beats(t, p.bar),
                    next: 0,
                    chunks: Vec::new(),
                });
                let idx = row
                    .slots
                    .iter()
                    .position(|&s| s == (p.bar, p.beat))
                    .filter(|&i| i >= row.next)
                    .unwrap_or(row.next);
                let mut broken = std::mem::take(&mut newline);
                for _ in row.next..idx {
                    row.chunks.push(Chunk {
                        text: String::new(),
                        comment: String::new(),
                        newline: std::mem::take(&mut broken),
                    });
                }
                row.chunks.push(Chunk {
                    text: p.text.replace(' ', "+"),
                    comment: std::mem::take(&mut labels).join(" "),
                    newline: broken,
                });
                row.next = idx + 1;
            }
            _ => {}
        }
    }
    let total = rows.len();
    let lines = rows
        .into_values()
        .take(5)
        .map(|row| {
            let mut text = String::new();
            for (n, c) in row.chunks.iter().enumerate() {
                if n > 0 {
                    // Syllables follow a hyphen directly; everything else is spaced.
                    if c.newline {
                        text.push('\n');
                    } else if !(row.chunks[n - 1].text.ends_with('-') && !c.text.is_empty()) {
                        text.push(' ');
                    }
                }
                if !c.comment.is_empty() {
                    text.push_str(&format!("[{}]", c.comment));
                }
                text.push_str(if c.text.is_empty() { "_" } else { &c.text });
            }
            crate::LyricLine {
                start_bar: row.start as u32,
                text,
            }
        })
        .collect();
    GpLines {
        lines,
        dropped: total.saturating_sub(5),
    }
}

/// Gives every track whose beats hold syllables but has no lyrics text one written from them
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

/// Text of a version 1 `.cc` file's track made into the current syntax, so the syllables stay on
/// the same notes: GABC groups go (a group between two letters is a hyphen, a clef at the start
/// is dropped), `<bar:beat>` anchors become `(bar:beat)` (a whole number was the index of the
/// beat in the bar's first voice; it becomes the beat's position), and `_` italics become
/// `<i>` tags (`_si-lence` is `<i>si</i>-lence`).
pub fn from_v1(score: &Score, track: usize, old: &str) -> String {
    let c: Vec<char> = old.chars().collect();
    // Without the GABC groups.
    let mut text = String::new();
    let sp = gabc_spans(old);
    let mut skip_space = false;
    for (n, &(a, b, kind)) in sp.iter().enumerate() {
        match kind {
            SpanKind::Gabc => {
                if n == 0 || (n == 1 && sp[0].2 == SpanKind::Space) {
                    skip_space = true;
                    continue;
                }
                let before = text
                    .chars()
                    .last()
                    .is_some_and(|ch| !ch.is_whitespace() && ch != '-');
                let after = sp
                    .get(n + 1)
                    .is_some_and(|s| s.2 == SpanKind::Text && c[s.0] != '-' && c[s.0] != '_');
                if before && after {
                    text.push('-');
                }
            }
            _ => {
                for &ch in &c[a..b] {
                    if skip_space && kind == SpanKind::Space {
                        skip_space = false;
                        continue;
                    }
                    skip_space = false;
                    text.push(ch);
                }
            }
        }
    }
    // Anchors and italics.
    let c: Vec<char> = text.chars().collect();
    let t = score.tracks.get(track);
    let anchor = |i: usize| -> Option<(usize, String)> {
        let end = c[i..].iter().position(|&x| x == '>')? + i;
        let inner: String = c[i + 1..end].iter().collect();
        let (bar, pos) = crate::position::parse_position(&inner)?;
        let whole = !inner.contains('.');
        let beats = t.and_then(|t| t.bars.get(bar)?.voices.first().map(|v| v.beats.len()));
        let pos = match (t, beats) {
            (Some(t), Some(len)) if whole && (pos as usize) <= len => {
                crate::position::beat_position(score, t, bar, 0, pos as usize - 1)
            }
            _ => pos,
        };
        Some((
            end + 1,
            format!("({})", crate::position::format_position(bar, pos, true)),
        ))
    };
    let mut out = String::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if let Some((end, a)) = (ch == '<').then(|| anchor(i)).flatten() {
            out.push_str(&a);
            i = end;
        } else if let Some(end) = (ch == '[')
            .then(|| c[i + 1..].iter().position(|&x| x == ']').map(|p| i + 1 + p))
            .flatten()
        {
            out.extend(&c[i..=end]);
            i = end + 1;
        } else if ch.is_whitespace() {
            out.push(ch);
            i += 1;
        } else {
            // A word, up to a space, a label or an anchor.
            let mut j = i;
            while j < c.len()
                && !c[j].is_whitespace()
                && !(c[j] == '[' && c[j + 1..].contains(&']'))
                && !(c[j] == '<' && anchor(j).is_some())
            {
                j += 1;
            }
            let word = &c[i..j];
            i = j;
            if word.iter().all(|&x| x == '_' || x == '-') || word == ['*'] {
                out.extend(word);
                continue;
            }
            #[derive(PartialEq, Clone, Copy)]
            enum Prev {
                Start,
                Dash,
                Italic,
                Separator,
                Other,
            }
            let (mut prev, mut pending, mut open, mut has) = (Prev::Start, false, false, false);
            for &x in word {
                match x {
                    '-' => {
                        if open {
                            out.push_str("</i>");
                            open = false;
                        }
                        out.push('-');
                        has = false;
                        prev = Prev::Dash;
                    }
                    '_' if !has && matches!(prev, Prev::Start | Prev::Dash | Prev::Italic) => {
                        pending = true;
                        prev = Prev::Italic;
                    }
                    '_' => {
                        if open {
                            out.push_str("</i>");
                            open = false;
                        }
                        out.push('_');
                        has = false;
                        prev = Prev::Separator;
                    }
                    x => {
                        if pending {
                            out.push_str("<i>");
                            open = true;
                            pending = false;
                        }
                        out.push(x);
                        has = true;
                        prev = Prev::Other;
                    }
                }
            }
            if open {
                out.push_str("</i>");
            }
        }
    }
    out
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
    // Tags left around nothing go; spaces left doubled are one again; lines are kept.
    let mut kept = kept;
    for tag in TAGS {
        for gap in ["", " ", "\n"] {
            kept = kept.replace(&format!("<{tag}>{gap}</{tag}>"), "");
        }
    }
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

    fn syl(t: &Token) -> Option<(&str, Style, bool)> {
        match &t.kind {
            TokenKind::Syllable {
                text,
                style,
                hyphen,
            } => Some((text.as_str(), *style, *hyphen)),
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
        assert!(dashed("some -- one"));
        assert_eq!(beats("some _ one"), 3);
        assert!(!dashed("some _ one"));
        assert_eq!(beats("some __ one"), 4);
        // Newlines and runs of spaces only separate.
        assert_eq!(beats("some  \n\n  one"), 2);
        // `+` joins words on one beat (shown with a space).
        assert_eq!(beats("some+one"), 1);
    }

    #[test]
    fn labels_stars_and_anchors_are_tokens() {
        let t = tokens("(2:3) [1.] si-lence * a-men (5:3.5)(1:2)");
        assert_eq!(t[0].kind, TokenKind::Anchor { bar: 1, beat: 3.0 });
        assert_eq!(t[1].kind, TokenKind::Label("1.".into()));
        assert_eq!(syl(&t[2]), Some(("si", Style::default(), true)));
        assert_eq!(syl(&t[3]), Some(("lence", Style::default(), false)));
        assert_eq!(t[4].kind, TokenKind::Star);
        assert_eq!(syl(&t[5]).map(|s| (s.0, s.2)), Some(("a", true)));
        assert_eq!(syl(&t[6]).map(|s| (s.0, s.2)), Some(("men", false)));
        assert_eq!(t[7].kind, TokenKind::Anchor { bar: 4, beat: 3.5 });
        assert_eq!(t[8].kind, TokenKind::Anchor { bar: 0, beat: 2.0 });
        // `_` no longer makes italics: it separates.
        let t = tokens("_si-lence");
        assert_eq!(syl(&t[0]), Some(("si", Style::default(), true)));
        // A `(` that starts no position is text, and so are `<…>` that are not tags; GABC groups
        // are not part of modern text.
        for text in ["(sigh)", "(0:1)", "(1:", "<2:3>", "<v>x</v>", "a(g)b"] {
            assert!(
                tokens(text)
                    .iter()
                    .all(|t| matches!(t.kind, TokenKind::Syllable { .. })),
                "{text}"
            );
        }
        assert_eq!(syl(&tokens("(sigh)")[0]).unwrap().0, "(sigh)");
        assert_eq!(syl(&tokens("<2:3>")[0]).unwrap().0, "<2:3>");
        assert_eq!(syl(&tokens("a(g)b")[0]).unwrap().0, "a(g)b");
        // Spans cover the text by kind.
        let kinds: Vec<SpanKind> = spans("<i>a</i>[1.](1:2)(").iter().map(|s| s.2).collect();
        use SpanKind::*;
        assert_eq!(kinds, [Markup, Text, Markup, Label, Anchor, Text]);
    }

    #[test]
    fn markup_tags_style_the_text_they_enclose() {
        let styles = |text: &str| -> Vec<(String, Style)> {
            tokens(text)
                .iter()
                .filter_map(syl)
                .map(|s| (s.0.to_string(), s.1))
                .collect()
        };
        let italic = Style {
            italic: true,
            ..Style::default()
        };
        let both = Style {
            bold: true,
            italic: true,
            ..Style::default()
        };
        let bold = Style {
            bold: true,
            ..Style::default()
        };
        // The tag ends at the dash.
        let s = styles("<i>si</i>-lence a-<i>men</i>");
        assert_eq!(s[0], ("si".into(), italic));
        assert_eq!(s[1], ("lence".into(), Style::default()));
        assert_eq!(s[3], ("men".into(), italic));
        // Tags span words and syllables and nest.
        let s = styles("<b>Glo-ry <i>to</i> you</b> x");
        assert_eq!(s[0].1, bold);
        assert_eq!(s[2].1, both);
        assert_eq!(s[3].1, bold);
        assert_eq!(s[4].1, Style::default());
        // Spans over lines.
        let s = styles("<ul>a\n\nb</ul> c");
        assert!(s[0].1.underline && s[1].1.underline && !s[2].1.underline);
        // Small caps and rubrics.
        let s = styles("<sc>Lord</sc> <c>R</c>");
        assert!(s[0].1.small_caps && !s[0].1.red);
        assert!(s[1].1.red);
        // An unknown or unclosed tag, and a closing tag alone, are ordinary text.
        for (text, shown) in [
            ("<x>a</x>", "<x>a</x>"),
            ("<i>a", "<i>a"),
            ("a</i>", "a</i>"),
            ("<b>a</i>", "<b>a</i>"),
        ] {
            let s = styles(text);
            assert_eq!(s[0], (shown.into(), Style::default()), "{text}");
        }
        // Tags take no beat.
        assert_eq!(beats("<i>some</i> <b></b> <c>one</c>"), 2);
        // A label's tags are not styled; highlighting shows only paired tags.
        use Hl::*;
        assert_eq!(
            highlight("<b>x</b>"),
            [Markup, Markup, Markup, Text, Markup, Markup, Markup, Markup]
        );
        assert_eq!(highlight("<i>x"), [Text, Text, Text, Text]);
    }

    #[test]
    fn specials_are_shown_as_their_characters() {
        let text = |t: &str| -> Vec<String> {
            tokens(t)
                .iter()
                .filter_map(syl)
                .map(|s| s.0.to_string())
                .collect()
        };
        assert_eq!(text("<sp>V/</sp> <sp>R/</sp>"), ["℣", "℟"]);
        assert_eq!(text("c<sp>'ae</sp>-li"), ["cǽ", "li"]);
        assert_eq!(text("<sp>A/</sp>men"), ["Ⱥmen"]);
        assert_eq!(text("<sp>'oe</sp>"), ["œ́"]);
        // The syllable's range covers the whole tag; the tags are marked.
        let t = tokens("a<sp>V/</sp>");
        assert_eq!(t[0].span, (0, 12));
        assert_eq!(highlight("<sp>V/</sp>")[0], Hl::Markup);
        assert_eq!(highlight("<sp>V/</sp>")[5], Hl::Text);
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
        let lay = layout(&s, 0, "(1:1) [1.] Let all mor-tal flesh keep _ si-lence,");
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
        assert_eq!(at(&s, "a (2:2) b")[1], (1, 1, 0, "b".into()));
        let back = at(&s, "a b c (1:1) d");
        assert_eq!(back[3], (0, 1, 1, "d".into()));
        // Past the last note: kept, not shown.
        let many = vec!["la"; 20].join(" ");
        assert_eq!(layout(&s, 0, &many).unplaced, 9);
        // ✻ goes after the syllable before it.
        let star = layout(&s, 0, "Ky-ri-e * e-le-i-son");
        assert_eq!(star.placed[2].after, STAR);
        assert_eq!(star.placed[3].text, "e-");
        // A dash word covers beats: "b" is three beats after "a" (bar 1 starts with a rest).
        assert_eq!(at(&s, "a -- b")[1], (1, 0, 0, "b".into()));
        assert_eq!(at(&s, "a -- b")[0].3, "a-");
        // `+` joins words on a beat.
        assert_eq!(at(&s, "a+b c")[0].3, "a b");
        // Styles reach the placed syllables.
        let lay = layout(&s, 0, "<i>a</i> b");
        assert!(lay.placed[0].style.italic && lay.placed[1].style.is_plain());
        assert_eq!(
            decorations(&s, &{
                let mut t = s.tracks[0].clone();
                t.lyrics_text = "<b>a</b> b".into();
                t
            })
            .get(&(0, 1, 0))
            .map(|d| d.style.bold),
            Some(true)
        );
    }

    #[test]
    fn anchors_take_fractions_of_a_beat() {
        // Eighth notes: the second of a beat is at 1:2.5.
        let mut s = score(2, false);
        let eighth = |p: u8| Beat {
            duration: Duration::new(DurationValue::Eighth),
            ..quarter(p)
        };
        s.tracks[0].bars[0].voices[0].beats = (0..8).map(|_| eighth(60)).collect();
        let p = at(&s, "(1:2.5) a b (1:2) c");
        assert_eq!((p[0].0, p[0].1), (0, 3));
        assert_eq!((p[1].0, p[1].1), (0, 4));
        assert_eq!((p[2].1, p[2].2), (2, 1));
        // Past the bar's end: the next bar.
        assert_eq!(at(&s, "(2:3) a")[0].0, 1);
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
        let text = "(1:1) [1.] Hel-lo there my _ friend\n\nsec-ond verse goes here and on";
        s.tracks[0].lyrics_text = text.into();
        apply(&mut s, 0);
        let before = s.clone();
        let again = text_from_beats(&s, 0);
        assert!(!again.contains('<') && !again.contains("(c"), "{again}");
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
    fn a_verse_row_can_be_removed() {
        let s = score(2, false);
        let text = "[1.] a b c\n(1:1) [2.] d e";
        assert_eq!(remove_verse(&s, 0, text, 1), "[1.] a b c\n(1:1)");
        assert_eq!(remove_verse(&s, 0, text, 0), "(1:1) [2.] d e");
        // Tags emptied by it go too.
        assert_eq!(remove_verse(&s, 0, "<i>a</i> b\n(1:1) c", 0), "(1:1) c");
    }

    #[test]
    fn version_1_text_is_converted() {
        let s = score(3, false);
        assert_eq!(
            from_v1(
                &s,
                0,
                "(c4) <2:3> Pó(g)pu(h)lus(i) _si-lence a-_men [1.] some_one __ x <1:2.5>*"
            ),
            "(2:3) Pó-pu-lus <i>si</i>-lence a-<i>men</i> [1.] some_one __ x (1:2.5)*"
        );
        // The number was a beat index: with eighth notes beat 3 is at position 2.
        let mut s = score(2, false);
        let eighth = Beat {
            duration: Duration::new(DurationValue::Eighth),
            ..quarter(60)
        };
        s.tracks[0].bars[0].voices[0].beats = vec![eighth; 8];
        assert_eq!(
            from_v1(&s, 0, "<1:3> a <1:9> b <2:4> c"),
            "(1:2) a (1:9) b (2:4) c"
        );
        // The converted text lays out on the same notes as the old one did.
        assert_eq!(at(&s, "(1:2) a")[0], (0, 2, 0, "a".into()));
    }

    #[test]
    fn lyrics_are_added_with_a_position_anchor() {
        let s = score(3, false);
        let added = add_lyrics(&s, 0, "", 1, 2.0, "la la");
        assert_eq!(added.text, "(2:2) la la\n");
        let added = add_lyrics(&s, 0, "(1:1) a b c d e f", 0, 3.0, "X");
        assert!(added.text.contains("(1:3) X"), "{}", added.text);
    }

    #[test]
    fn highlighting_tells_the_parts_of_modern_text_and_of_gabc_apart() {
        use Hl::*;
        assert_eq!(highlight("a [1.](2:3)")[1], Space);
        assert_eq!(highlight("(2:3.5)")[0], Anchor);
        assert_eq!(highlight("(2:3.5)")[6], Anchor);
        assert_eq!(highlight("[1.]")[0], Label);
        // No GABC colours in modern text.
        assert_eq!(highlight("(c3)"), [Text, Text, Text, Text]);
        let kinds = |t: &str| highlight_gabc(t);
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
        // In a chant's text `(1:2)` is a GABC group and `<2:3>` is not an anchor.
        assert_eq!(kinds("(1:2)")[0], Paren);
        assert_eq!(kinds("<2:3>")[0], Text);
        assert_eq!(kinds("[1.]")[0], Label);
        assert_eq!(
            gabc_spans("(c4) a[1.](")
                .iter()
                .map(|s| s.2)
                .collect::<Vec<_>>(),
            [
                SpanKind::Gabc,
                SpanKind::Space,
                SpanKind::Text,
                SpanKind::Label,
                SpanKind::Text
            ]
        );
    }

    #[test]
    fn gp_lyrics_are_translated_back_to_guitar_pro_text() {
        let s = score(4, false);
        let lines = |text: &str| -> Vec<(u32, String)> {
            let mut s = s.clone();
            s.tracks[0].lyrics_text = text.into();
            to_gp_lines(&s, 0)
                .lines
                .into_iter()
                .map(|l| (l.start_bar, l.text))
                .collect()
        };
        // Words and hyphens as written; labels become comments; markup and ✻ go.
        assert_eq!(
            lines("[1.] Let all mor-tal <i>flesh</i> * keep"),
            [(0, "[1.]Let all mor-tal flesh keep".to_string())]
        );
        // Line breaks are kept inside the line.
        assert_eq!(lines("a b\nc d"), [(0, "a b\nc d".to_string())]);
        // `_` words and forward anchors are empty notes; a dash word keeps the hyphen.
        assert_eq!(lines("a __ b"), [(0, "a _ _ b".to_string())]);
        assert_eq!(lines("a _ b"), [(0, "a _ b".to_string())]);
        assert_eq!(lines("a - b"), [(0, "a- _ b".to_string())]);
        assert_eq!(lines("a -- b"), [(0, "a- _ _ b".to_string())]);
        assert_eq!(lines("a (1:4) b"), [(0, "a _ _ b".to_string())]);
        // `+` stays; an anchor going back starts the next line, at its first syllable's bar.
        assert_eq!(
            lines("a+b c (2:1) d e\n(1:1) f g"),
            [(0, "a+b c _ _ d e".to_string()), (0, "f g".to_string())]
        );
        // Only five lines fit.
        let many = (1..=7)
            .map(|i| format!("(1:1) w{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut s2 = s.clone();
        s2.tracks[0].lyrics_text = many;
        let g = to_gp_lines(&s2, 0);
        assert_eq!((g.lines.len(), g.dropped), (5, 2));
        // The line starts at the bar of its first syllable.
        assert_eq!(lines("(3:1) a b")[0].0, 2);
        // What reads back is the same: empty notes, comments, line breaks.
        let back = crate::lyrics::chunks("[1.]Let a- _ b\nc");
        let shown: Vec<(&str, bool, &str)> = back
            .iter()
            .map(|c| (c.text.as_str(), c.newline, c.comment.as_str()))
            .collect();
        assert_eq!(
            shown,
            [
                ("Let", false, "1."),
                ("a-", false, ""),
                ("", false, ""),
                ("b", false, ""),
                ("c", true, "")
            ]
        );
    }

    #[test]
    fn gp_lines_read_into_the_new_syntax_keeping_line_breaks() {
        let mut s = score(4, false);
        s.lyrics = Some(Lyrics {
            track: Some(0),
            lines: vec![LyricLine {
                start_bar: 0,
                text: "[Verse]I won't he-si-tate  no  more\nno more".into(),
            }],
        });
        let (t, text) = text_from_lines(&s).unwrap();
        assert_eq!(t, 0);
        assert_eq!(text, "[Verse] I won't he-si-tate _ no _ more\nno more");
        // And it lays out on the same notes Guitar Pro puts them on.
        let lay = layout(&s, 0, &text);
        assert_eq!(lay.placed.len(), 9);
        assert_eq!((lay.placed[2].bar, lay.placed[2].beat), (0, 2));
    }
}
