//! Gregorian chant in square notation, modelled on GABC: a sequence of syllables, each with its
//! text and the neumes sung on it.
//!
//! Notes are stored as staff positions under a clef (as chant is written) and resolved to MIDI
//! pitches for playback. A chant track also carries an ordinary transcription in its bars
//! ([`transcribe`]), one beat per chant note, so playback, the cursor, audio sync and the modern
//! formats work on chant like on any other track.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{
    Bar, Beat, Duration, DurationValue, MasterBar, Note, TICKS_PER_QUARTER, TimeSignature, Voice,
};

/// A chant piece.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chant {
    pub syllables: Vec<Syllable>,
    /// Number of staff lines (2–5, usually 4).
    pub staff_lines: u8,
    /// How note lengths are read for playback and modern transcription.
    pub interpretation: Interpretation,
    /// GABC header fields in file order (`name`, `mode`, `office-part`, ...).
    pub headers: Vec<(String, String)>,
    /// The next note id to hand out (see [`ChantNote::id`]); not part of the chant's content.
    #[serde(default)]
    pub next_id: u32,
    /// The GABC body as kept (its layout) with the map of it, when there is one. Not part of
    /// the chant's content: see [`crate::chant_source`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<crate::chant_source::ChantSource>,
}

impl Default for Chant {
    fn default() -> Self {
        Self {
            syllables: Vec::new(),
            staff_lines: 4,
            interpretation: Interpretation::default(),
            headers: Vec::new(),
            next_id: 0,
            source: None,
        }
    }
}

/// Chants are equal when they read the same; note ids and the id counter are bookkeeping.
impl PartialEq for Chant {
    fn eq(&self, o: &Self) -> bool {
        self.syllables == o.syllables
            && self.staff_lines == o.staff_lines
            && self.interpretation == o.interpretation
            && self.headers == o.headers
    }
}

/// Rhythmic reading of chant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interpretation {
    /// Every note has the same length; a mora dot doubles it.
    #[default]
    Equalist,
    /// Solesmes method: mora doubles, horizontal episema and the note before a quilisma lengthen.
    Solesmes,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Syllable {
    /// Stable identity (0 = none) of an empty group or a division of its own, the cells that
    /// are not notes: kept by edits so a sync point stays with it. Not written to GABC and
    /// ignored when comparing syllables.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: u32,
    /// Text as written in GABC, markup included (`<i>…</i>`, `<sp>V/</sp>`, `{` `}`).
    pub text: String,
    /// The syllable begins a word (a space precedes it in GABC).
    pub word_start: bool,
    pub items: Vec<NeumeItem>,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

impl PartialEq for Syllable {
    fn eq(&self, o: &Self) -> bool {
        self.text == o.text && self.word_start == o.word_start && self.items == o.items
    }
}

/// One element of a syllable's notation, in GABC order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NeumeItem {
    Note(ChantNote),
    /// Separator between neumes or glyphs as written: `/`, `//`, ` `, `!`, `/0`, `/[2]`, ...
    Space(String),
    Clef(ChantClef),
    /// Division (bar) as written: `,` `;` `:` `::` `` ` `` `^` `;3` `,0` `:?` ...
    Bar(String),
    /// Flat, natural or sharp at a staff position, as written after the pitch (`x`, `y`, `#`, `x?`).
    Alteration {
        pitch: u8,
        sign: String,
    },
    /// Custos: `z0` (pitch taken from the next note) or `f+` (explicit).
    Custos(Option<u8>),
    /// Line break as written: `z`, `Z`, `z+`, `z-`, `Z+`, `Z-`.
    LineBreak(String),
    /// Anything else, kept verbatim (`[…]` attributes, `{`, `}`, `=`).
    Raw(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClefKind {
    Do,
    Fa,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChantClef {
    pub kind: ClefKind,
    /// Staff line, 1 = bottom.
    pub line: u8,
    /// Flat clef (`cb3`): B is flat throughout.
    pub flat: bool,
}

impl Default for ChantClef {
    fn default() -> Self {
        Self {
            kind: ClefKind::Do,
            line: 4,
            flat: false,
        }
    }
}

impl ChantClef {
    /// Staff position of the clef line (0 = GABC `a`).
    pub fn position(&self) -> i32 {
        2 * self.line as i32 + 1
    }

    /// Staff position of do (C).
    fn do_position(&self) -> i32 {
        match self.kind {
            ClefKind::Do => self.position(),
            // Fa on the line: do is a fifth above (F G A B C).
            ClefKind::Fa => self.position() + 4,
        }
    }

    /// The clef as GABC text (`c4`, `cb3`, `f3`).
    pub fn gabc(&self) -> String {
        let k = if self.kind == ClefKind::Do { 'c' } else { 'f' };
        format!("{k}{}{}", if self.flat { "b" } else { "" }, self.line)
    }

    pub fn parse(s: &str) -> Option<Self> {
        let mut c = s.chars();
        let kind = match c.next()? {
            'c' => ClefKind::Do,
            'f' => ClefKind::Fa,
            _ => return None,
        };
        let mut rest = c.as_str();
        let flat = rest.starts_with('b');
        if flat {
            rest = &rest[1..];
        }
        let line: u8 = rest.parse().ok().filter(|l| (1..=5).contains(l))?;
        Some(Self { kind, line, flat })
    }

    /// MIDI pitch of a staff position, given the alteration in force there. Do is C5 under a
    /// C clef (so a fa clef line is F4), which keeps most chant within the treble staff.
    pub fn midi(&self, position: i32, alter: i32) -> u8 {
        const SCALE: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
        let d = position - self.do_position();
        let pitch = 72 + 12 * d.div_euclid(7) + SCALE[d.rem_euclid(7) as usize] + alter;
        pitch.clamp(0, 127) as u8
    }

    /// Scale degree (0 = do … 6 = ti) of a staff position.
    pub fn degree(&self, position: i32) -> i32 {
        (position - self.do_position()).rem_euclid(7)
    }

    /// The clef and octave shift (semitones) that keep pitches on the four-line staff, for
    /// showing ordinary notes as chant.
    /// `family` and `line` restrict the clef to a kind and a staff line when set.
    pub fn fitting(pitches: &[u8], family: Option<ClefKind>, line: Option<u8>) -> (ChantClef, i32) {
        let mut clefs = vec![
            (ClefKind::Do, 4),
            (ClefKind::Do, 3),
            (ClefKind::Fa, 3),
            (ClefKind::Do, 2),
            (ClefKind::Fa, 4),
            (ClefKind::Do, 1),
        ];
        if let Some(l) = line {
            clefs = vec![(ClefKind::Do, l), (ClefKind::Fa, l)];
        }
        let mut best: Option<(f64, ChantClef, i32)> = None;
        for (order, &(kind, line)) in clefs.iter().enumerate() {
            if family.is_some_and(|f| f != kind) {
                continue;
            }
            let clef = ChantClef {
                kind,
                line,
                flat: false,
            };
            for octave in [-24, -12, 0, 12, 24] {
                let cost: f64 = pitches
                    .iter()
                    .map(|&p| {
                        let (pos, _) = clef.place(p, octave);
                        if !(0..=12).contains(&pos) {
                            10.0
                        } else if !(2..=10).contains(&pos) {
                            1.0
                        } else {
                            0.0
                        }
                    })
                    .sum::<f64>()
                    + (octave as f64).abs() * 0.02
                    + order as f64 * 0.01;
                if best.as_ref().is_none_or(|b| cost < b.0) {
                    best = Some((cost, clef, octave));
                }
            }
        }
        best.map_or((ChantClef::default(), 0), |b| (b.1, b.2))
    }

    /// Where a pitch, shifted by `octave` semitones, is written: `(position, alteration)`,
    /// a flat or sharp marking a pitch between two positions.
    pub fn place(&self, pitch: u8, octave: i32) -> (i32, i32) {
        let target = (pitch as i32 + octave).clamp(0, 127) as u8;
        let (pos, alter) = self.position_of(target, false);
        if self.midi(pos, alter) == target {
            (pos, alter)
        } else {
            (pos, 1)
        }
    }

    /// The position nearest to a MIDI pitch: `(position, alteration)`, preferring a flat ti for
    /// B♭ and rounding other chromatic pitches up or down.
    pub fn position_of(&self, pitch: u8, round_up: bool) -> (i32, i32) {
        let target = pitch as i32;
        let mut p = self.do_position() + ((target - 72) as f64 / 12.0 * 7.0).floor() as i32 - 2;
        while self.midi(p + 1, 0) as i32 <= target {
            p += 1;
        }
        while self.midi(p, 0) as i32 > target {
            p -= 1;
        }
        let natural = self.midi(p, 0) as i32;
        if natural == target {
            return (p, 0);
        }
        // Between p and p + 1: B♭ is a flat on ti, anything else rounds.
        if self.degree(p + 1) == 6 {
            return (p + 1, -1);
        }
        if round_up { (p + 1, 0) } else { (p, 0) }
    }
}

/// A note as written: a staff position plus GABC modifiers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChantNote {
    /// Stable identity (0 = none yet): kept by every edit, so audio sync points stay with their
    /// note. Not written to GABC and ignored when comparing notes.
    #[serde(default)]
    pub id: u32,
    /// Staff position: 0 = GABC `a` (below the bottom line), 3 = `d` (bottom line), ...
    pub pitch: u8,
    /// Punctum inclinatum (diamond), written in upper case.
    pub inclinatum: bool,
    /// Initio debilis, written `-` before the pitch.
    pub initio_debilis: bool,
    /// Prefix `@` (fused glyph), kept for round-trips.
    pub fused: bool,
    /// Modifiers after the pitch, in written order: shapes (`v` `V` `vv` `s` `ss` `w` `W` `o`
    /// `O` `q`), liquescence (`~` `<` `>`), heads (`r` `R` `r0`), signs (`.` `..` `_` `_0` `'`
    /// `'1` `r1`…`r8`) and bracketed attributes.
    pub mods: Vec<String>,
}

/// The note head drawn for a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Punctum,
    Inclinatum,
    Virga,
    VirgaReversa,
    /// Two or three virgae on one pitch (`vv`, `vvv`).
    Bivirga(u8),
    /// Stropha; the count is 1–3 (`s`, `ss`, `sss`).
    Stropha(u8),
    Quilisma,
    /// Oriscus; `true` for the oriscus with a stem (`O`).
    Oriscus(bool),
    /// Square-headed first note of a pes quadratum (`q`, or `W` for a quilisma).
    Quadratum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liquescence {
    None,
    /// `~`: a smaller final note.
    Deminutus,
    /// `<`: augmented, curling up.
    AuctusAscendens,
    /// `>`: augmented, curling down.
    AuctusDescendens,
}

impl PartialEq for ChantNote {
    fn eq(&self, o: &Self) -> bool {
        self.pitch == o.pitch
            && self.inclinatum == o.inclinatum
            && self.initio_debilis == o.initio_debilis
            && self.fused == o.fused
            && self.mods == o.mods
    }
}

impl ChantNote {
    pub fn new(pitch: u8) -> Self {
        Self {
            pitch,
            ..Self::default()
        }
    }

    fn has(&self, f: impl Fn(&str) -> bool) -> bool {
        self.mods.iter().any(|m| f(m))
    }

    pub fn shape(&self) -> Shape {
        for m in &self.mods {
            let shape = match m.as_str() {
                "v" if self.inclinatum => None,
                "v" => Some(Shape::Virga),
                "V" => Some(Shape::VirgaReversa),
                "vv" | "VV" => Some(Shape::Bivirga(2)),
                "vvv" | "VVV" => Some(Shape::Bivirga(3)),
                "w" => Some(Shape::Quilisma),
                "W" | "q" => Some(Shape::Quadratum),
                m if m.starts_with("sss") => Some(Shape::Stropha(3)),
                m if m.starts_with("ss") => Some(Shape::Stropha(2)),
                "s" => Some(Shape::Stropha(1)),
                m if m.starts_with('o') => Some(Shape::Oriscus(false)),
                m if m.starts_with('O') => Some(Shape::Oriscus(true)),
                _ => None,
            };
            if let Some(s) = shape {
                return s;
            }
        }
        if self.inclinatum {
            Shape::Inclinatum
        } else {
            Shape::Punctum
        }
    }

    pub fn liquescence(&self) -> Liquescence {
        for m in &self.mods {
            match m.as_str() {
                "~" => return Liquescence::Deminutus,
                "<" => return Liquescence::AuctusAscendens,
                ">" => return Liquescence::AuctusDescendens,
                m if m.starts_with("ss") && m.ends_with('<') => {
                    return Liquescence::AuctusAscendens;
                }
                m if m.starts_with("ss") && m.ends_with('>') => {
                    return Liquescence::AuctusDescendens;
                }
                _ => {}
            }
        }
        Liquescence::None
    }

    /// Number of mora dots (0–2).
    pub fn mora(&self) -> u8 {
        self.mods
            .iter()
            .map(|m| match m.as_str() {
                ".." => 2,
                m if m.starts_with('.') => 1,
                _ => 0,
            })
            .max()
            .unwrap_or(0)
    }

    /// Horizontal episema (`_`).
    pub fn episema(&self) -> bool {
        self.has(|m| m.starts_with('_'))
    }

    /// Vertical episema / ictus (`'`).
    pub fn ictus(&self) -> bool {
        self.has(|m| m.starts_with('\''))
    }

    /// Hollow head (`r`, punctum cavum).
    pub fn cavum(&self) -> bool {
        self.has(|m| m == "r" || m == "r0")
    }

    /// Lines either side of the head (`R`, `r0`: punctum linea).
    pub fn linea(&self) -> bool {
        self.has(|m| m == "R" || m == "r0")
    }

    /// Number of notes sounded (a distropha is two repercussions of one written note).
    pub fn repeats(&self) -> u8 {
        match self.shape() {
            Shape::Stropha(n) | Shape::Bivirga(n) => n,
            _ => 1,
        }
    }

    /// The note as GABC.
    pub fn gabc(&self) -> String {
        let mut s = String::new();
        if self.initio_debilis {
            s.push('-');
        }
        if self.fused {
            s.push('@');
        }
        s.push(pitch_letter(self.pitch, self.inclinatum));
        for m in &self.mods {
            s.push_str(m);
        }
        s
    }
}

/// The GABC letter of a staff position (`a`..`n`, then `p`).
pub fn pitch_letter(position: u8, upper: bool) -> char {
    let c = match position {
        0..=13 => (b'a' + position) as char,
        _ => 'p',
    };
    if upper { c.to_ascii_uppercase() } else { c }
}

/// The staff position of a GABC pitch letter (either case).
pub fn letter_position(c: char) -> Option<u8> {
    match c.to_ascii_lowercase() {
        c @ 'a'..='n' => Some(c as u8 - b'a'),
        'p' => Some(14),
        _ => None,
    }
}

/// Kinds of division (bar line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Division {
    Virgula,
    Minima,
    Minor,
    Maior,
    Finalis,
}

impl Division {
    pub fn of(token: &str) -> Division {
        match token.trim_end_matches(['?', '0']) {
            "::" => Division::Finalis,
            ":" | ":'" => Division::Maior,
            t if t.starts_with(';') => Division::Minor,
            // Dominican bars `,1`…`,8` are minor divisions.
            t if t.starts_with(',') && t[1..].starts_with(|c: char| ('1'..='8').contains(&c)) => {
                Division::Minor
            }
            t if t.starts_with(',') || t.starts_with('^') => Division::Minima,
            _ => Division::Virgula,
        }
    }
}

impl NeumeItem {
    /// The item as GABC.
    pub fn gabc(&self) -> String {
        match self {
            NeumeItem::Note(n) => n.gabc(),
            NeumeItem::Space(s)
            | NeumeItem::Bar(s)
            | NeumeItem::LineBreak(s)
            | NeumeItem::Raw(s) => s.clone(),
            NeumeItem::Clef(c) => c.gabc(),
            NeumeItem::Alteration { pitch, sign } => {
                format!("{}{sign}", pitch_letter(*pitch, false))
            }
            NeumeItem::Custos(None) => "z0".into(),
            NeumeItem::Custos(Some(p)) => format!("{}+", pitch_letter(*p, false)),
        }
    }
}

impl Syllable {
    /// The notation inside the parentheses, as GABC.
    pub fn gabc_notes(&self) -> String {
        self.items.iter().map(NeumeItem::gabc).collect()
    }

    /// The syllable as GABC: `text(notes)`.
    pub fn gabc(&self) -> String {
        format!("{}({})", self.text, self.gabc_notes())
    }

    pub fn notes(&self) -> impl Iterator<Item = &ChantNote> {
        self.items.iter().filter_map(|i| match i {
            NeumeItem::Note(n) => Some(n),
            _ => None,
        })
    }

    /// The text without markup, for display.
    pub fn plain_text(&self) -> String {
        plain_text(&self.text)
    }

    pub fn italic(&self) -> bool {
        self.text.contains("<i>")
    }

    /// An empty group, `()`: a place in the chant where a note or a division can be entered.
    pub fn is_slot(&self) -> bool {
        self.items.is_empty()
    }

    /// A division written as its own symbol, `(:)`: no notes, only a division (and spacing).
    pub fn is_division_cell(&self) -> bool {
        self.items.iter().any(|i| matches!(i, NeumeItem::Bar(_)))
            && self
                .items
                .iter()
                .all(|i| matches!(i, NeumeItem::Bar(_) | NeumeItem::Space(_)))
    }

    /// A clef or a division of its own with no text: the GABC editor gives it a line, the
    /// division with a space before it.
    pub fn on_own_line(&self) -> bool {
        self.is_clef_line() || (self.text.trim().is_empty() && self.is_division_cell())
    }

    /// A clef with no text: the GABC editor gives it a line.
    pub fn is_clef_line(&self) -> bool {
        self.text.trim().is_empty() && matches!(self.items.as_slice(), [NeumeItem::Clef(_)])
    }

    /// The division that ends the syllable's group (`g::` in `(fg::)`), as written.
    pub fn trailing_division(&self) -> Option<&str> {
        let mut rest = self
            .items
            .iter()
            .rev()
            .skip_while(|i| matches!(i, NeumeItem::Space(_)));
        match rest.next() {
            Some(NeumeItem::Bar(b)) => Some(b),
            _ => None,
        }
    }

    pub fn division(&self) -> Option<Division> {
        self.items.iter().find_map(|i| match i {
            NeumeItem::Bar(b) => Some(Division::of(b)),
            _ => None,
        })
    }
}

/// The character a GABC `<sp>…</sp>` special stands for (`V/` is ℣), if it is one.
pub fn special(name: &str) -> Option<&'static str> {
    Some(match name {
        "V/" => "℣",
        "R/" => "℟",
        "A/" => "Ⱥ",
        "+" => "†",
        "*" => "✱",
        "ae" => "æ",
        "'ae" | "'æ" => "ǽ",
        "oe" => "œ",
        "'oe" | "'œ" => "œ́",
        _ => return None,
    })
}

/// GABC text without markup: tags removed, `<sp>` specials and `<v>` verbatim resolved.
pub fn plain_text(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find(['<', '{', '}', '$']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if let Some(t) = tail.strip_prefix('$') {
            // `$x` escapes one character.
            let mut c = t.chars();
            if let Some(ch) = c.next() {
                out.push(ch);
            }
            rest = c.as_str();
            continue;
        }
        if tail.starts_with('{') || tail.starts_with('}') {
            rest = &tail[1..];
            continue;
        }
        let Some(end) = tail.find('>') else {
            out.push_str(tail);
            return out;
        };
        let tag = &tail[1..end];
        rest = &tail[end + 1..];
        let close = |rest: &str, name: &str| rest.find(&format!("</{name}>"));
        match tag {
            "sp" => {
                if let Some(j) = close(rest, "sp") {
                    out.push_str(special(&rest[..j]).unwrap_or(&rest[..j]));
                    rest = &rest[j + 5..];
                }
            }
            // Verbatim TeX and text above the staff are not part of the underlay.
            "v" | "alt" => {
                if let Some(j) = close(rest, tag) {
                    rest = &rest[j + tag.len() + 3..];
                }
            }
            _ => {}
        }
    }
    out.push_str(rest);
    out
}

/// A stretch of a syllable's text in one style (see [`text_runs`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextRun {
    pub text: String,
    /// `<i>`.
    pub italic: bool,
    /// `<b>`.
    pub bold: bool,
    /// `<ul>`.
    pub underline: bool,
    /// `<sc>`: small capitals.
    pub small_caps: bool,
    /// `<c>`: colored (red).
    pub colored: bool,
    /// `<tt>`: monospaced.
    pub mono: bool,
    /// `<e>`: an elision, set small.
    pub elision: bool,
}

/// A syllable's text split into runs of one style, and the text of its `<alt>` tags (set above
/// the staff). `<sp>` specials are resolved; `<v>` verbatim TeX, `{` `}` and unknown tags are
/// dropped, `$x` is the character `x` (as [`plain_text`]).
pub fn text_runs(text: &str) -> (Vec<TextRun>, Vec<String>) {
    let mut runs: Vec<TextRun> = Vec::new();
    let mut alt: Vec<String> = Vec::new();
    // The tags open now, innermost last.
    let mut open: Vec<&str> = Vec::new();
    let style = |open: &[&str]| {
        let has = |t: &str| open.contains(&t);
        TextRun {
            text: String::new(),
            italic: has("i"),
            bold: has("b"),
            underline: has("ul"),
            small_caps: has("sc"),
            colored: has("c"),
            mono: has("tt"),
            elision: has("e"),
        }
    };
    let push = |runs: &mut Vec<TextRun>, open: &[&str], s: &str| {
        if s.is_empty() {
            return;
        }
        let st = style(open);
        match runs.last_mut() {
            Some(r)
                if (
                    r.italic,
                    r.bold,
                    r.underline,
                    r.small_caps,
                    r.colored,
                    r.mono,
                    r.elision,
                ) == (
                    st.italic,
                    st.bold,
                    st.underline,
                    st.small_caps,
                    st.colored,
                    st.mono,
                    st.elision,
                ) =>
            {
                r.text.push_str(s)
            }
            _ => runs.push(TextRun {
                text: s.to_string(),
                ..st
            }),
        }
    };
    let mut rest = text;
    while let Some(i) = rest.find(['<', '{', '}', '$']) {
        push(&mut runs, &open, &rest[..i]);
        let tail = &rest[i..];
        if let Some(t) = tail.strip_prefix('$') {
            let mut c = t.chars();
            if let Some(ch) = c.next() {
                push(&mut runs, &open, &ch.to_string());
            }
            rest = c.as_str();
            continue;
        }
        if tail.starts_with('{') || tail.starts_with('}') {
            rest = &tail[1..];
            continue;
        }
        let Some(end) = tail.find('>') else {
            push(&mut runs, &open, tail);
            rest = "";
            break;
        };
        let tag = &tail[1..end];
        rest = &tail[end + 1..];
        if let Some(name) = tag.strip_prefix('/') {
            if let Some(k) = open.iter().rposition(|t| *t == name) {
                open.truncate(k);
            }
            continue;
        }
        match tag {
            "sp" | "v" | "alt" => {
                let close = format!("</{tag}>");
                let Some(j) = rest.find(&close) else {
                    continue;
                };
                let inner = &rest[..j];
                rest = &rest[j + close.len()..];
                match tag {
                    "sp" => push(&mut runs, &open, special(inner).unwrap_or(inner)),
                    "alt" => alt.push(plain_text(inner)),
                    _ => {}
                }
            }
            "i" | "b" | "ul" | "sc" | "c" | "tt" | "e" => open.push(tag),
            _ => {}
        }
    }
    push(&mut runs, &open, rest);
    (runs, alt)
}

/// The text inside one `<name>…</name>` that wraps all of `text`: `(name, inner)`.
fn outer_wrapper(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix('<')?;
    let name = &rest[..rest.find('>')?];
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let close = format!("</{name}>");
    let inner = text[name.len() + 2..].strip_suffix(&close)?;
    (!inner.contains(&close)).then_some((name, inner))
}

/// A syllable's text with `<tag>…</tag>` around all of it, or without it when it is there
/// (also inside other tags that wrap all of it).
pub fn toggle_markup(text: &str, tag: &str) -> String {
    fn remove(text: &str, tag: &str) -> Option<String> {
        let (name, inner) = outer_wrapper(text)?;
        if name == tag {
            return Some(inner.to_string());
        }
        let done = remove(inner, tag)?;
        Some(format!("<{name}>{done}</{name}>"))
    }
    remove(text, tag).unwrap_or_else(|| format!("<{tag}>{text}</{tag}>"))
}

/// Where typed text goes in a syllable's text: before the closing tags it ends with, so text
/// typed into an empty `<i></i>` is in it.
pub fn typing_end(text: &str) -> usize {
    let mut end = text.len();
    while let Some(before) = text[..end].strip_suffix('>') {
        match before.rfind("</") {
            Some(i) if before[i + 2..].chars().all(|c| c.is_ascii_alphabetic()) => end = i,
            _ => break,
        }
    }
    end
}

impl Chant {
    /// The first clef, or C on the fourth line.
    pub fn initial_clef(&self) -> ChantClef {
        self.syllables
            .iter()
            .flat_map(|s| &s.items)
            .find_map(|i| match i {
                NeumeItem::Clef(c) => Some(*c),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Whether a syllable is a cell of its own (an empty group or a division of its own) that
    /// takes an id.
    fn is_cell(s: &Syllable) -> bool {
        s.is_slot() || s.is_division_cell()
    }

    /// Gives every note and every cell of its own without an id (or with one already used) a
    /// fresh one. The other ids stay.
    pub fn assign_ids(&mut self) {
        let mut seen = std::collections::HashSet::new();
        let mut next = self.next_id.max(1);
        for s in &mut self.syllables {
            if !Self::is_cell(s) {
                s.id = 0;
            }
            next = next.max(s.id.saturating_add(1));
            for n in &s.items {
                if let NeumeItem::Note(n) = n {
                    next = next.max(n.id.saturating_add(1));
                }
            }
        }
        let mut take = |id: &mut u32| {
            if *id == 0 || !seen.insert(*id) {
                *id = next;
                next += 1;
                seen.insert(*id);
            }
        };
        // The notes first, so a chant's notes are numbered 1, 2, 3, … as before.
        for item in self.syllables.iter_mut().flat_map(|s| &mut s.items) {
            if let NeumeItem::Note(n) = item {
                take(&mut n.id);
            }
        }
        for s in &mut self.syllables {
            if Self::is_cell(s) {
                take(&mut s.id);
            }
        }
        self.next_id = next;
    }

    /// The id of a cell: its note's, else its syllable's.
    pub fn cell_id(&self, cell: &Cell) -> Option<u32> {
        match cell.note {
            Some(n) => self.note_ids().get(n).copied(),
            None => self
                .syllables
                .get(cell.syllable)
                .map(|s| s.id)
                .filter(|&i| i != 0),
        }
    }

    /// For a chant made again from text (its notes all have fresh ids): gives the notes that
    /// match notes of `old` the ids of those, so a text edit keeps the audio sync of the notes
    /// it leaves alone. The others keep their fresh ids; the points of `old` notes that match
    /// nothing are lost with them.
    ///
    /// Matching, in order:
    /// 1. the same number of notes with the same heads and pitches: note for note, whatever
    ///    else changed (lyric text, spacing, a mora, a header);
    /// 2. otherwise the notes alike at both ends, then a longest common subsequence on
    ///    (syllable text, pitch position, shape), where notes of the same pitch and shape under
    ///    changed text still match (weaker);
    /// 3. between two matched notes, as many unmatched old notes as new ones (up to four) are
    ///    taken to be the same notes edited in place (a retyped pitch).
    pub fn match_ids(&mut self, old: &Chant) {
        fn flat(c: &Chant) -> Vec<(&str, &ChantNote)> {
            c.syllables
                .iter()
                .flat_map(|s| s.notes().map(move |n| (s.text.as_str(), n)))
                .collect()
        }
        let head = |n: &ChantNote| (n.pitch, n.shape());
        let new_ids: Vec<u32> = {
            let (a, b) = (flat(old), flat(self));
            let mut ids = vec![0u32; b.len()];
            if a.len() == b.len() && a.iter().zip(&b).all(|(x, y)| head(x.1) == head(y.1)) {
                for (id, x) in ids.iter_mut().zip(&a) {
                    *id = x.1.id;
                }
            } else {
                let strong = |x: &(&str, &ChantNote), y: &(&str, &ChantNote)| {
                    x.0 == y.0 && head(x.1) == head(y.1)
                };
                let lead = a.iter().zip(&b).take_while(|(x, y)| strong(x, y)).count();
                let trail = a[lead..]
                    .iter()
                    .rev()
                    .zip(b[lead..].iter().rev())
                    .take_while(|(x, y)| strong(x, y))
                    .count();
                // Matched pairs (old index, new index), in order.
                let mut pairs: Vec<(usize, usize)> = (0..lead).map(|i| (i, i)).collect();
                let (am, bm) = (&a[lead..a.len() - trail], &b[lead..b.len() - trail]);
                if am.len().saturating_mul(bm.len()) <= 16_000_000 {
                    let weight = |x: &(&str, &ChantNote), y: &(&str, &ChantNote)| -> u32 {
                        match (head(x.1) == head(y.1), x.0 == y.0) {
                            (true, true) => 3,
                            (true, false) => 1,
                            _ => 0,
                        }
                    };
                    // dp[i][j]: the best weight matching am[i..] with bm[j..].
                    let mut dp = vec![vec![0u32; bm.len() + 1]; am.len() + 1];
                    for i in (0..am.len()).rev() {
                        for j in (0..bm.len()).rev() {
                            let w = weight(&am[i], &bm[j]);
                            let take = if w > 0 { dp[i + 1][j + 1] + w } else { 0 };
                            dp[i][j] = take.max(dp[i + 1][j]).max(dp[i][j + 1]);
                        }
                    }
                    let (mut i, mut j) = (0, 0);
                    while i < am.len() && j < bm.len() {
                        let w = weight(&am[i], &bm[j]);
                        if w > 0 && dp[i][j] == dp[i + 1][j + 1] + w {
                            pairs.push((lead + i, lead + j));
                            i += 1;
                            j += 1;
                        } else if dp[i + 1][j] >= dp[i][j + 1] {
                            i += 1;
                        } else {
                            j += 1;
                        }
                    }
                }
                pairs.extend((0..trail).map(|k| (a.len() - trail + k, b.len() - trail + k)));
                // Notes edited in place: the same number of unmatched notes between two matches.
                let mut anchors = vec![(0usize, 0usize)];
                for &(i, j) in &pairs {
                    anchors.push((i, j));
                    anchors.push((i + 1, j + 1));
                }
                anchors.push((a.len(), b.len()));
                for w in anchors.chunks(2) {
                    let ((i0, j0), (i1, j1)) = (w[0], w[1]);
                    if i1 >= i0 && j1 >= j0 && i1 - i0 == j1 - j0 && i1 - i0 <= 4 {
                        // A gap (possibly none) between two matches or an end.
                        for k in 0..i1 - i0 {
                            pairs.push((i0 + k, j0 + k));
                        }
                    }
                }
                for (i, j) in pairs {
                    ids[j] = a[i].1.id;
                }
            }
            ids
        };
        let mut k = 0;
        for item in self.syllables.iter_mut().flat_map(|s| &mut s.items) {
            if let NeumeItem::Note(n) = item {
                n.id = new_ids[k];
                k += 1;
            }
        }
        // Cells of their own (empty groups, divisions): the same kinds before and after the
        // changed ones, or all in order when as many.
        let key = |s: &Syllable| {
            s.items
                .iter()
                .filter_map(|i| match i {
                    NeumeItem::Bar(b) => Some(b.as_str()),
                    _ => None,
                })
                .collect::<String>()
        };
        let olds: Vec<(String, u32)> = old
            .syllables
            .iter()
            .filter(|s| Self::is_cell(s))
            .map(|s| (key(s), s.id))
            .collect();
        let news: Vec<usize> = (0..self.syllables.len())
            .filter(|&i| Self::is_cell(&self.syllables[i]))
            .collect();
        let same = |i: usize, j: usize| olds[i].0 == key(&self.syllables[news[j]]);
        let lead = (0..olds.len().min(news.len()))
            .take_while(|&i| same(i, i))
            .count();
        let trail = (0..olds.len().min(news.len()) - lead)
            .take_while(|&k| same(olds.len() - 1 - k, news.len() - 1 - k))
            .count();
        for i in 0..lead {
            self.syllables[news[i]].id = olds[i].1;
        }
        for k in 0..trail {
            self.syllables[news[news.len() - 1 - k]].id = olds[olds.len() - 1 - k].1;
        }
        if olds.len() == news.len() {
            for (i, &j) in news.iter().enumerate() {
                self.syllables[j].id = olds[i].1;
            }
        }
        self.next_id = self.next_id.max(old.next_id);
        self.assign_ids();
    }

    /// The ids of the notes in reading order.
    pub fn note_ids(&self) -> Vec<u32> {
        self.syllables
            .iter()
            .flat_map(|s| s.notes())
            .map(|n| n.id)
            .collect()
    }

    /// The reading-order index of the note with this id.
    pub fn index_of_id(&self, id: u32) -> Option<usize> {
        self.syllables
            .iter()
            .flat_map(|s| s.notes())
            .position(|n| n.id == id && id != 0)
    }

    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn set_header(&mut self, key: &str, value: Option<String>) {
        match (self.headers.iter().position(|(k, _)| k == key), value) {
            (Some(i), Some(v)) => self.headers[i].1 = v,
            (Some(i), None) => {
                self.headers.remove(i);
            }
            (None, Some(v)) => self.headers.push((key.to_string(), v)),
            (None, None) => {}
        }
    }

    /// Every note with its syllable and item index, its clef and its MIDI pitch.
    pub fn resolved(&self) -> Vec<ResolvedNote> {
        let mut out = Vec::new();
        let mut clef = self.initial_clef();
        // Accidentals last until the end of the word or the next division.
        let mut alters: Vec<(u8, i32)> = Vec::new();
        for (si, s) in self.syllables.iter().enumerate() {
            if s.word_start {
                alters.clear();
            }
            for (ii, item) in s.items.iter().enumerate() {
                match item {
                    NeumeItem::Clef(c) => {
                        clef = *c;
                        alters.clear();
                    }
                    NeumeItem::Bar(_) => alters.clear(),
                    NeumeItem::Alteration { pitch, sign } => {
                        let a = match sign.chars().next() {
                            Some('x' | 'X') => -1,
                            Some('#') => 1,
                            _ => 0,
                        };
                        alters.retain(|(p, _)| p != pitch);
                        alters.push((*pitch, a));
                    }
                    NeumeItem::Note(n) => {
                        let alter = alters
                            .iter()
                            .find(|(p, _)| *p == n.pitch)
                            .map(|(_, a)| *a)
                            .unwrap_or(if clef.flat && clef.degree(n.pitch as i32) == 6 {
                                -1
                            } else {
                                0
                            });
                        out.push(ResolvedNote {
                            syllable: si,
                            item: ii,
                            clef,
                            midi: clef.midi(n.pitch as i32, alter),
                        });
                    }
                    _ => {}
                }
            }
        }
        out
    }
}

/// Where something is in a chant, as shown everywhere (status line, waveform ruler, `:goto`):
/// `bar:syllable.note`. A bar starts after every division (the first is bar 1); the syllables
/// of a bar are counted from 1 (only those that are not a division of their own), and the
/// notes of a syllable from 1. A division of its own is `bar:0`, the bar it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    pub bar: usize,
    /// 0 for a division.
    pub syllable: usize,
    /// 0 for a division; an empty group has no note either (it reads as `.1`).
    pub note: usize,
}

impl Place {
    /// `3:4.2`; a place that has no decimal part reads as `.1` (`3:4` is `3:4.1`), and a
    /// division is `3:0`.
    pub fn label(&self) -> String {
        if self.syllable == 0 {
            format!("{}:0", self.bar)
        } else {
            format!("{}:{}.{}", self.bar, self.syllable, self.note.max(1))
        }
    }

    /// Reads `bar:syllable` or `bar:syllable.note`: a missing note is 1, and `bar:0` is the
    /// division.
    pub fn parse(s: &str) -> Option<Place> {
        let (bar, rest) = s.trim().split_once(':')?;
        let bar: usize = bar.trim().parse().ok().filter(|&b| b > 0)?;
        let (syllable, note) = match rest.trim().split_once('.') {
            Some((a, b)) => (a.trim().parse().ok()?, b.trim().parse().ok()?),
            None => (rest.trim().parse().ok()?, 1),
        };
        if syllable == 0 {
            return Some(Place {
                bar,
                syllable: 0,
                note: 0,
            });
        }
        (note > 0).then_some(Place {
            bar,
            syllable,
            note,
        })
    }
}

/// The [`Place`] of everything in a chant, from [`Chant::places`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChantPlaces {
    /// Of every note, in reading order.
    pub notes: Vec<Place>,
    /// Of a note, an empty group or a division of its own, by (syllable, item).
    cells: HashMap<(usize, usize), Place>,
    /// For every syllable: the bar it is in when it starts, and whether a bar starts there
    /// (the number of a bar is shown where it starts).
    pub syllables: Vec<(usize, bool)>,
}

impl ChantPlaces {
    /// The place of a cell of the transcription.
    pub fn of_cell(&self, cell: &Cell) -> Option<Place> {
        self.cells.get(&(cell.syllable, cell.item)).copied()
    }
}

impl Chant {
    /// The places of everything in the chant.
    pub fn places(&self) -> ChantPlaces {
        let mut out = ChantPlaces::default();
        let (mut bar, mut syl) = (1, 0);
        // A bar starts at the next syllable that is not a division of its own: the first bar,
        // and each after a division.
        let mut pending = true;
        for (si, s) in self.syllables.iter().enumerate() {
            let own_division = s.is_division_cell();
            let start = bar;
            let mut starts = false;
            if s.is_slot() {
                syl += 1;
                out.cells.insert(
                    (si, 0),
                    Place {
                        bar,
                        syllable: syl,
                        note: 0,
                    },
                );
                out.syllables.push((bar, std::mem::take(&mut pending)));
                continue;
            }
            let mut counted = false;
            let mut note = 0;
            for (ii, item) in s.items.iter().enumerate() {
                match item {
                    NeumeItem::Bar(_) => {
                        bar += 1;
                        syl = 0;
                        counted = false;
                        note = 0;
                        if own_division {
                            out.cells.insert(
                                (si, ii),
                                Place {
                                    bar,
                                    syllable: 0,
                                    note: 0,
                                },
                            );
                            starts = true;
                            pending = false;
                        } else {
                            pending = true;
                        }
                    }
                    NeumeItem::Note(_) => {
                        if !counted {
                            syl += 1;
                            counted = true;
                            note = 0;
                            if std::mem::take(&mut pending) {
                                starts = true;
                            }
                        }
                        note += 1;
                        let place = Place {
                            bar,
                            syllable: syl,
                            note,
                        };
                        out.notes.push(place);
                        out.cells.insert((si, ii), place);
                    }
                    _ => {}
                }
            }
            out.syllables
                .push((if own_division { bar } else { start }, starts));
        }
        out
    }
}

/// A note located in the chant, with its sounding pitch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedNote {
    pub syllable: usize,
    pub item: usize,
    pub clef: ChantClef,
    pub midi: u8,
}

/// A chant written out as ordinary bars: one free-time bar per phrase (between divisions) and
/// one beat per sounded note.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Transcription {
    pub master_bars: Vec<MasterBar>,
    pub bars: Vec<Bar>,
    /// (bar, beat) of every chant note, in [`Chant::resolved`] order. Strophae and bivirgae
    /// sound several beats; this is the first.
    pub positions: Vec<(usize, usize)>,
    /// Beats each chant note sounds (1, or 2–3 for a stropha or bivirga), parallel to
    /// `positions`.
    pub lengths: Vec<u8>,
    /// Ticks each chant note sounds in all (its beats' ticks added up: mora, Solesmes
    /// lengthening and repeats counted), parallel to `positions`.
    pub ticks: Vec<u32>,
    /// Everything the cursor can stand on, in reading order: notes, empty groups and divisions
    /// written as their own symbol. The last two are beats that take no time.
    pub cells: Vec<Cell>,
}

/// What a [`Cell`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    Note,
    /// An empty group `()`.
    Slot,
    /// A division written as its own symbol `(:)`.
    Division,
}

/// A place of the chant the cursor can stand on, and the beats it has in the transcription.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub kind: CellKind,
    pub syllable: usize,
    /// The item of the syllable (a note's, a division's; 0 for an empty group).
    pub item: usize,
    /// The note's index in reading order ([`Chant::resolved`]) for a note cell.
    pub note: Option<usize>,
    pub bar: usize,
    pub beat: usize,
    /// Beats it sounds (a stropha sounds several).
    pub len: usize,
}

impl Transcription {
    /// The cell standing on a beat.
    pub fn cell_at(&self, bar: usize, beat: usize) -> Option<usize> {
        let k = self
            .cells
            .partition_point(|c| (c.bar, c.beat) <= (bar, beat));
        let k = k.checked_sub(1)?;
        let c = &self.cells[k];
        (c.bar == bar && beat < c.beat + c.len).then_some(k)
    }

    /// Like [`Self::cell_at`], but a beat with no cell maps to the last cell before it, or the
    /// first one.
    pub fn cell_near(&self, bar: usize, beat: usize) -> Option<usize> {
        self.cell_at(bar, beat).or_else(|| {
            let k = self
                .cells
                .partition_point(|c| (c.bar, c.beat) <= (bar, beat));
            match k {
                0 => (!self.cells.is_empty()).then_some(0),
                k => Some(k - 1),
            }
        })
    }

    /// The cell of a syllable's item (a note, or a division cell's division), else of the
    /// syllable's first cell.
    pub fn cell_of(&self, syllable: usize, item: usize) -> Option<usize> {
        self.cells
            .iter()
            .position(|c| c.syllable == syllable && c.item == item)
            .or_else(|| self.cells.iter().position(|c| c.syllable == syllable))
    }

    /// The chant note (index in reading order, see [`Chant::resolved`]) sounded by a beat of
    /// the transcription. Every beat of a stropha or bivirga maps to its note.
    pub fn note_at(&self, bar: usize, beat: usize) -> Option<usize> {
        let k = self.positions.partition_point(|&p| p <= (bar, beat));
        let k = k.checked_sub(1)?;
        let (b, first) = self.positions[k];
        (b == bar && beat < first + self.lengths[k] as usize).then_some(k)
    }

    /// Like [`Self::note_at`], but a beat that sounds no note (an empty bar, past the end)
    /// maps to the last note before it, or the first note.
    pub fn note_near(&self, bar: usize, beat: usize) -> Option<usize> {
        self.note_at(bar, beat).or_else(|| {
            let k = self.positions.partition_point(|&p| p <= (bar, beat));
            match k {
                0 => (!self.positions.is_empty()).then_some(0),
                k => Some(k - 1),
            }
        })
    }

    /// The `(bar, beat)` of the first beat sounded by a chant note.
    pub fn place_of(&self, note: usize) -> Option<(usize, usize)> {
        self.positions.get(note).copied()
    }
}

const EIGHTH: u32 = TICKS_PER_QUARTER / 2;

/// Writes a chant out in modern rhythm according to its interpretation.
pub fn transcribe(chant: &Chant) -> Transcription {
    let notes = chant.resolved();
    let mut out = Transcription::default();
    let mut beats: Vec<Beat> = Vec::new();
    let finish = |beats: &mut Vec<Beat>, out: &mut Transcription, division: Option<Division>| {
        if beats.is_empty() {
            if let (Some(Division::Finalis), Some(last)) = (division, out.master_bars.last_mut()) {
                last.double_bar = true;
            }
            return;
        }
        // Beats that take no time (empty groups, divisions of their own) make no bar by
        // themselves: they stand at the end of the bar before, or go on into the next one.
        if beats.iter().all(|b| b.empty) {
            if out.bars.is_empty() {
                if division.is_some() {
                    return;
                }
            } else {
                let pending = out.bars.len();
                let prev = pending - 1;
                let offset = out.bars[prev].voices[0].beats.len();
                for c in out.cells.iter_mut().filter(|c| c.bar == pending) {
                    c.bar = prev;
                    c.beat += offset;
                }
                out.bars[prev].voices[0].beats.append(beats);
                if let (Some(Division::Finalis), Some(last)) =
                    (division, out.master_bars.last_mut())
                {
                    last.double_bar = true;
                }
                return;
            }
        }
        let ticks: u32 = beats.iter().map(Beat::ticks).sum();
        let eighths = (ticks / EIGHTH).clamp(1, 255) as u8;
        out.master_bars.push(MasterBar {
            time_signature: TimeSignature {
                numerator: eighths,
                denominator: 8,
            },
            free_time: true,
            double_bar: division == Some(Division::Finalis),
            ..MasterBar::default()
        });
        out.bars.push(Bar {
            voices: vec![Voice {
                beats: std::mem::take(beats),
            }],
            ..Default::default()
        });
    };
    // An empty group or a division of its own is a beat that takes no time: the cursor can
    // stand on it, playback and the audio sync pass over it.
    let place_beat = || Beat {
        empty: true,
        ..Beat::default()
    };
    let mut k = 0;
    for (si, s) in chant.syllables.iter().enumerate() {
        let mut first_in_syllable = true;
        if s.is_slot() {
            out.cells.push(Cell {
                kind: CellKind::Slot,
                syllable: si,
                item: 0,
                note: None,
                bar: out.bars.len(),
                beat: beats.len(),
                len: 1,
            });
            beats.push(place_beat());
        }
        let own_division = s.is_division_cell();
        for (ii, item) in s.items.iter().enumerate() {
            match item {
                NeumeItem::Bar(b) => {
                    if own_division {
                        out.cells.push(Cell {
                            kind: CellKind::Division,
                            syllable: si,
                            item: ii,
                            note: None,
                            bar: out.bars.len(),
                            beat: beats.len(),
                            len: 1,
                        });
                        beats.push(place_beat());
                    }
                    let d = Division::of(b);
                    if d != Division::Virgula {
                        finish(&mut beats, &mut out, Some(d));
                    }
                }
                NeumeItem::Note(n) => {
                    let r = notes[k];
                    k += 1;
                    let next = notes
                        .get(k)
                        .map(|r| &chant.syllables[r.syllable].items[r.item]);
                    let before_quilisma =
                        matches!(next, Some(NeumeItem::Note(m)) if m.shape() == Shape::Quilisma);
                    let mut ticks = EIGHTH * (1 + n.mora().min(1) as u32);
                    if chant.interpretation == Interpretation::Solesmes
                        && n.mora() == 0
                        && (n.episema() || before_quilisma)
                    {
                        ticks = EIGHTH * 3 / 2;
                    }
                    out.cells.push(Cell {
                        kind: CellKind::Note,
                        syllable: si,
                        item: ii,
                        note: Some(k - 1),
                        bar: out.bars.len(),
                        beat: beats.len(),
                        len: n.repeats() as usize,
                    });
                    out.positions.push((out.bars.len(), beats.len()));
                    out.lengths.push(n.repeats());
                    out.ticks.push(ticks * n.repeats() as u32);
                    // A note of the same pitch right after another in a neume (`gg`) goes on
                    // with it: one note of both lengths, not two attacks.
                    let continues_note = ii > 0
                        && matches!(s.items[ii - 1], NeumeItem::Note(_))
                        && k >= 2
                        && notes[k - 2].midi == r.midi;
                    for rep in 0..n.repeats() {
                        let mut beat = Beat {
                            duration: duration_of(ticks),
                            notes: vec![Note::new(0, r.midi)],
                            ..Beat::default()
                        };
                        beat.notes[0].tie = continues_note && rep == 0;
                        if first_in_syllable {
                            let text = s.plain_text();
                            let text = text.trim();
                            if !text.is_empty() {
                                let continues =
                                    chant.syllables.get(si + 1).is_some_and(|n| !n.word_start);
                                beat.lyrics =
                                    vec![format!("{text}{}", if continues { "-" } else { "" })];
                            }
                            first_in_syllable = false;
                        }
                        beats.push(beat);
                    }
                }
                _ => {}
            }
        }
    }
    finish(&mut beats, &mut out, None);
    out
}

/// Rewrites a chant track's bars (and the score's master bars) from its chant notation. Other
/// tracks keep their bars, padded or cut to the new bar count.
pub fn retranscribe(score: &mut crate::Score, track: usize) {
    let Some(chant) = score
        .tracks
        .get_mut(track)
        .and_then(|t| t.chant.as_deref_mut())
    else {
        return;
    };
    chant.assign_ids();
    let t = transcribe(chant);
    let n = t.master_bars.len().max(1);
    score.master_bars = if t.master_bars.is_empty() {
        vec![MasterBar {
            free_time: true,
            ..MasterBar::default()
        }]
    } else {
        t.master_bars
    };
    score.anacrusis = false;
    for (i, tr) in score.tracks.iter_mut().enumerate() {
        if i == track {
            tr.bars = t.bars.clone();
        }
        tr.bars.resize_with(n, Bar::default);
    }
    score.refresh_chant_sync();
}

fn duration_of(ticks: u32) -> Duration {
    let value = if ticks >= TICKS_PER_QUARTER {
        DurationValue::Quarter
    } else {
        DurationValue::Eighth
    };
    let mut d = Duration::new(value);
    if ticks * 2 == value.ticks() * 3 {
        d.dots = 1;
    }
    d
}

/// The kinds of glyph (ligature) of Gregorio's `gabc-glyphs-determination.c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum G {
    Undetermined,
    Punctum,
    Podatus,
    PesQuadratumFirst,
    PesQuadratum,
    PesAscendensOriscus,
    Salicus,
    SalicusFlexus,
    Flexa,
    Torculus,
    TorculusResupinus,
    TorculusResupinusFlexus,
    TorculusLiquescens,
    Porrectus,
    PorrectusFlexus,
    Scandicus,
    Ancus,
    Virga,
    Bivirga,
    Trivirga,
    VirgaReversa,
    Stropha,
    Distropha,
    Tristropha,
    /// One punctum inclinatum.
    Inclinatum,
    /// Two to five puncta inclinata one way (`true`: ascending).
    Inclinata(u8, bool),
    /// Puncta inclinata that change direction.
    InclinataMixed,
}

impl G {
    fn is_inclinata(self) -> bool {
        matches!(self, G::Inclinatum | G::Inclinata(..) | G::InclinataMixed)
    }
}

/// What a note did to the glyph being determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    /// The note joined the glyph, which may go on.
    No,
    /// The note starts the next glyph.
    Previous,
    /// The note joined the glyph, which is complete.
    Current,
    /// The note stands alone, and the glyph before it is complete.
    Both,
}

/// How far apart notes of one glyph may be (Gregorio's `MAX_AMBITUS`).
const MAX_AMBITUS: i32 = 5;

/// Gregorio's `add_note_to_a_glyph`: whether the note `n` (at pitch `pitch`, after a note at
/// `last`) joins the glyph of type `cur` that began with `first`, and what the glyph is then.
fn add_note_to_glyph(
    mut cur: G,
    pitch: i32,
    last: Option<i32>,
    n: &ChantNote,
    first: Option<&ChantNote>,
) -> (G, End) {
    let normal_first =
        first.is_some_and(|f| f.shape() == Shape::Punctum && !f.initio_debilis && !f.inclinatum);
    let mut end = End::No;
    let next;
    let far = last.is_some_and(|l| (pitch - l).abs() > MAX_AMBITUS);
    if far {
        cur = G::Undetermined;
    }
    let lq = n.liquescence();
    let starts_new = |end: &mut End| {
        *end = End::Previous;
        G::Punctum
    };
    let last_p = last.unwrap_or(-1);
    let shape = if n.inclinatum {
        Shape::Inclinatum
    } else {
        n.shape()
    };
    if n.linea() {
        // A punctum linea stands alone.
        next = G::Punctum;
        end = End::Both;
    } else {
        match shape {
            Shape::Punctum => {
                if last == Some(pitch) {
                    next = starts_new(&mut end);
                } else {
                    match cur {
                        G::Punctum => {
                            next = if pitch > last_p { G::Podatus } else { G::Flexa };
                        }
                        G::Podatus => {
                            if pitch > last_p {
                                if normal_first {
                                    next = G::Scandicus;
                                    end = End::Current;
                                } else {
                                    next = starts_new(&mut end);
                                }
                            } else {
                                next = G::Torculus;
                            }
                        }
                        G::PesQuadratumFirst => {
                            if pitch > last_p {
                                next = G::PesQuadratum;
                                end = End::Current;
                            } else {
                                next = G::Flexa;
                            }
                        }
                        G::PesAscendensOriscus => {
                            if pitch > last_p {
                                next = G::Salicus;
                            } else {
                                next = starts_new(&mut end);
                            }
                        }
                        G::Salicus => {
                            if pitch < last_p {
                                next = G::SalicusFlexus;
                                end = End::Current;
                            } else {
                                next = starts_new(&mut end);
                            }
                        }
                        G::Flexa => {
                            if pitch > last_p {
                                if normal_first {
                                    next = G::Porrectus;
                                } else {
                                    next = starts_new(&mut end);
                                }
                            } else if lq == Liquescence::Deminutus {
                                end = End::Current;
                                next = G::Ancus;
                            } else {
                                next = starts_new(&mut end);
                            }
                        }
                        G::Torculus => {
                            if pitch > last_p {
                                next = G::TorculusResupinus;
                            } else if lq == Liquescence::Deminutus {
                                end = End::Current;
                                next = G::TorculusLiquescens;
                            } else {
                                next = starts_new(&mut end);
                            }
                        }
                        G::TorculusResupinus => {
                            if pitch > last_p {
                                next = starts_new(&mut end);
                            } else {
                                end = End::Current;
                                next = G::TorculusResupinusFlexus;
                            }
                        }
                        G::Porrectus => {
                            if pitch > last_p {
                                next = starts_new(&mut end);
                            } else {
                                end = End::Current;
                                next = G::PorrectusFlexus;
                            }
                        }
                        _ => next = starts_new(&mut end),
                    }
                }
            }
            Shape::Oriscus(false) | Shape::Quilisma => next = starts_new(&mut end),
            Shape::Virga => {
                if cur == G::Virga && last == Some(pitch) {
                    next = G::Bivirga;
                } else if cur == G::Bivirga && last == Some(pitch) {
                    next = G::Trivirga;
                } else {
                    end = End::Previous;
                    next = G::Virga;
                }
            }
            Shape::VirgaReversa => {
                end = End::Previous;
                next = G::VirgaReversa;
            }
            Shape::Bivirga(2) => {
                if cur == G::Virga && last == Some(pitch) {
                    end = End::Current;
                    next = G::Trivirga;
                } else {
                    end = End::Previous;
                    next = G::Bivirga;
                }
            }
            Shape::Bivirga(_) => {
                end = End::Both;
                next = G::Trivirga;
            }
            Shape::Quadratum => {
                end = End::Previous;
                next = G::PesQuadratumFirst;
            }
            Shape::Oriscus(true) => {
                if cur == G::Punctum && last_p < pitch {
                    next = G::PesAscendensOriscus;
                } else {
                    next = starts_new(&mut end);
                }
            }
            Shape::Inclinatum => {
                if cur.is_inclinata() && cur != G::Inclinatum || cur == G::Inclinatum {
                    if last == Some(pitch) {
                        end = End::Previous;
                        next = G::Inclinatum;
                    } else {
                        let up = last_p < pitch;
                        next = match cur {
                            G::Inclinatum => G::Inclinata(2, up),
                            G::Inclinata(k, dir) if dir == up && k < 5 => G::Inclinata(k + 1, dir),
                            _ => G::InclinataMixed,
                        };
                    }
                } else {
                    end = End::Previous;
                    next = G::Inclinatum;
                }
            }
            Shape::Stropha(1) => {
                if last != Some(pitch) {
                    end = End::Previous;
                    next = G::Stropha;
                } else {
                    match cur {
                        G::Stropha => next = G::Distropha,
                        G::Distropha => {
                            end = End::Current;
                            next = G::Tristropha;
                        }
                        _ => {
                            end = End::Previous;
                            next = G::Stropha;
                        }
                    }
                }
            }
            Shape::Stropha(2) => {
                if last == Some(pitch) && cur == G::Stropha {
                    end = End::Current;
                    next = G::Tristropha;
                } else {
                    end = End::Previous;
                    next = G::Distropha;
                }
            }
            Shape::Stropha(_) => {
                end = End::Both;
                next = G::Tristropha;
            }
        }
    }
    if cur == G::Undetermined {
        // The first glyph, or the one before has been closed already.
        end = match end {
            End::Previous => End::No,
            End::Both => End::Current,
            e => e,
        };
    }
    if far {
        end = if matches!(end, End::Current | End::Both) {
            End::Both
        } else {
            End::Previous
        };
    }
    (next, end)
}

/// Groups a run of notes (no separators) into glyphs (ligatures): the pes, clivis, torculus,
/// porrectus, climacus, scandicus and so on, as Gregorio determines them
/// (`gabc_det_glyphs_from_notes`), and as every renderer and converter shows them.
pub fn group_neumes<'a>(run: &[(usize, &'a ChantNote)]) -> Vec<Vec<(usize, &'a ChantNote)>> {
    let mut out: Vec<Vec<(usize, &ChantNote)>> = Vec::new();
    // The glyph being determined: its type and the run indices of its notes.
    let mut cur = G::Undetermined;
    let mut open: Vec<usize> = Vec::new();
    let mut last: Option<i32> = None;
    let close = |out: &mut Vec<Vec<(usize, &'a ChantNote)>>, open: &mut Vec<usize>| {
        if !open.is_empty() {
            out.push(open.drain(..).map(|i| run[i]).collect());
        }
    };
    for (i, &(_, n)) in run.iter().enumerate() {
        let pitch = n.pitch as i32;
        // A weak beginning (initio debilis) starts a glyph.
        if n.initio_debilis && cur != G::Undetermined {
            close(&mut out, &mut open);
            cur = G::Undetermined;
        }
        let first = open.first().map(|&k| run[k].1);
        let (next, end) = add_note_to_glyph(cur, pitch, last, n, first);
        let tail = n.liquescence() != Liquescence::None;
        match end {
            End::No => {
                cur = next;
                open.push(i);
                if tail {
                    close(&mut out, &mut open);
                    cur = G::Undetermined;
                }
            }
            End::Previous => {
                close(&mut out, &mut open);
                cur = next;
                open.push(i);
                if tail {
                    close(&mut out, &mut open);
                    cur = G::Undetermined;
                }
            }
            End::Current => {
                open.push(i);
                close(&mut out, &mut open);
                cur = G::Undetermined;
            }
            End::Both => {
                close(&mut out, &mut open);
                open.push(i);
                close(&mut out, &mut open);
                cur = G::Undetermined;
            }
        }
        last = Some(pitch);
    }
    close(&mut out, &mut open);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(p: u8, mods: &[&str]) -> NeumeItem {
        NeumeItem::Note(ChantNote {
            pitch: p,
            mods: mods.iter().map(|s| s.to_string()).collect(),
            ..ChantNote::default()
        })
    }

    /// The glyphs Gregorio finds in gabc notes (`/` separates runs), as the pitch letters of
    /// each.
    fn glyphs(gabc: &str) -> Vec<String> {
        let mut out = Vec::new();
        for part in gabc.split('/') {
            let mut notes: Vec<ChantNote> = Vec::new();
            for c in part.chars() {
                match c {
                    'a'..='m' if !matches!(c, 'v' | 'w' | 's') => notes.push(ChantNote {
                        pitch: c as u8 - b'a',
                        ..ChantNote::default()
                    }),
                    'v' | 'w' | 's' | 'V' | 'W' | 'O' | '\'' => {
                        notes.last_mut().unwrap().mods.push(c.to_string())
                    }
                    _ => {}
                }
            }
            let run: Vec<(usize, &ChantNote)> = notes.iter().enumerate().collect();
            out.extend(group_neumes(&run).iter().map(|g| {
                g.iter()
                    .map(|(_, n)| (b'a' + n.pitch) as char)
                    .collect::<String>()
            }));
        }
        out
    }

    #[test]
    fn glyphs_are_found_the_way_gregorio_finds_them() {
        // Gregorio's own output for these (gregorio -S): a quilisma starts a glyph that a
        // note above makes a pes quilisma; the notes at one pitch are glyphs of their own.
        assert_eq!(glyphs("efwg"), ["e", "fg"]);
        assert_eq!(glyphs("efwghVg"), ["e", "fg", "h", "g"]);
        // A porrectus flexus, then a flexus; or, after a break, a torculus.
        assert_eq!(glyphs("jjjghf'fd"), ["j", "j", "jghf", "fd"]);
        assert_eq!(glyphs("jjj/ghf'fd"), ["j", "j", "j", "ghf", "fd"]);
    }

    #[test]
    fn clefs_resolve_pitches() {
        let c4 = ChantClef::parse("c4").unwrap();
        assert_eq!(c4.midi(9, 0), 72); // j = do = C5
        assert_eq!(c4.midi(3, 0), 62); // d = re
        let f3 = ChantClef::parse("f3").unwrap();
        assert_eq!(f3.midi(7, 0), 65); // h = fa = F4
        let c3 = ChantClef::parse("cb3").unwrap();
        assert!(c3.flat);
        assert_eq!(c3.degree(6), 6); // g = ti
        assert_eq!(c4.position_of(70, false), (8, -1)); // B♭4 = flat on i
        assert_eq!(c4.position_of(66, false), (5, 0)); // F#4 rounds down to f
        assert_eq!(c4.position_of(66, true), (6, 0));
    }

    #[test]
    fn transcribes_phrases_flats_and_morae() {
        let chant = Chant {
            syllables: vec![
                Syllable {
                    text: String::new(),
                    word_start: true,
                    items: vec![NeumeItem::Clef(ChantClef::default())],
                    ..Default::default()
                },
                Syllable {
                    text: "Sal".into(),
                    word_start: true,
                    items: vec![
                        note(5, &[]),
                        NeumeItem::Alteration {
                            pitch: 8,
                            sign: "x".into(),
                        },
                        note(8, &[]),
                    ],
                    ..Default::default()
                },
                Syllable {
                    text: "ve".into(),
                    word_start: false,
                    items: vec![note(8, &["."])],
                    ..Default::default()
                },
                Syllable {
                    text: String::new(),
                    word_start: true,
                    items: vec![NeumeItem::Bar(";".into())],
                    ..Default::default()
                },
                Syllable {
                    text: "Re".into(),
                    word_start: true,
                    items: vec![note(8, &["ss"])],
                    ..Default::default()
                },
            ],
            ..Chant::default()
        };
        let t = transcribe(&chant);
        assert_eq!(t.master_bars.len(), 2);
        let beats = &t.bars[0].voices[0].beats;
        let pitches: Vec<u8> = beats
            .iter()
            .filter(|b| !b.empty)
            .map(|b| b.notes[0].fret)
            .collect();
        assert_eq!(pitches, [65, 70, 70]); // F4, B♭4, B♭4 (same word)
        assert_eq!(beats[2].duration.value, DurationValue::Quarter);
        assert!(beats[3].empty, "the division is a cell of its own");
        assert_eq!(beats[0].lyrics, ["Sal-"]);
        // The flat ends with the word; the distropha sounds twice.
        let b2 = &t.bars[1].voices[0].beats;
        assert_eq!(b2.len(), 2);
        assert_eq!(b2[0].notes[0].fret, 71);
        assert_eq!(t.cells.len(), 5);
        assert_eq!(t.positions, [(0, 0), (0, 1), (0, 2), (1, 0)]);
        assert_eq!(t.master_bars[0].time_signature.numerator, 4);
    }

    #[test]
    fn plain_text_strips_markup() {
        assert_eq!(plain_text("<i>Ps.</i>"), "Ps.");
        assert_eq!(plain_text("<sp>V/</sp>. Ex"), "℣. Ex");
        assert_eq!(plain_text("Dó{mi}nus<v>\\x</v>"), "Dóminus");
        assert_eq!(plain_text("c<sp>'ae</sp>"), "cǽ");
    }
}

#[cfg(test)]
mod text_run_tests {
    use super::*;

    #[test]
    fn markup_toggles_around_all_the_text_and_typing_goes_inside() {
        assert_eq!(toggle_markup("Ky", "i"), "<i>Ky</i>");
        assert_eq!(toggle_markup("<i>Ky</i>", "i"), "Ky");
        assert_eq!(
            toggle_markup("<i>a</i> b <i>c</i>", "i"),
            "<i><i>a</i> b <i>c</i></i>"
        );
        assert_eq!(toggle_markup("", "b"), "<b></b>");
        assert_eq!(&"<i><b></b></i>"[..typing_end("<i><b></b></i>")], "<i><b>");
        assert_eq!(typing_end("a<i>b</i>"), "a<i>b".len());
        assert_eq!(typing_end("a"), 1);
    }

    #[test]
    fn markup_makes_runs_and_alt_text() {
        let (runs, alt) = text_runs("a<i>b<b>c</b></i>d<alt>up</alt><sp>R/</sp>$<");
        let t: Vec<(&str, bool, bool)> = runs
            .iter()
            .map(|r| (r.text.as_str(), r.italic, r.bold))
            .collect();
        assert_eq!(
            t,
            [
                ("a", false, false),
                ("b", true, false),
                ("c", true, true),
                ("d℟<", false, false)
            ]
        );
        assert_eq!(alt, ["up"]);
        let (runs, _) = text_runs("<sc>x</sc><ul>y</ul><c>z</c><tt>m</tt><e>e</e>");
        assert!(runs[0].small_caps && runs[1].underline && runs[2].colored);
        assert!(runs[3].mono && runs[4].elision);
    }
}

#[cfg(test)]
mod repeated_note_tests {
    use super::*;
    use crate::performance::Performance;
    use crate::{Score, Track, TrackKind};

    fn sounded(notes: Vec<Vec<u8>>) -> Vec<(u64, u64)> {
        let syllables = notes
            .into_iter()
            .map(|g| Syllable {
                text: "a".into(),
                word_start: true,
                items: g
                    .into_iter()
                    .map(|p| NeumeItem::Note(ChantNote::new(p)))
                    .collect(),
                ..Default::default()
            })
            .collect();
        let mut score = Score::default();
        score.tracks.push(Track {
            kind: TrackKind::Chant,
            chant: Some(Box::new(Chant {
                syllables,
                ..Chant::default()
            })),
            ..Track::default()
        });
        retranscribe(&mut score, 0);
        Performance::build(&score, &[])
            .notes
            .iter()
            .map(|n| (n.start, n.end))
            .collect()
    }

    #[test]
    fn two_notes_of_one_pitch_in_a_neume_sound_as_one_of_both_lengths() {
        let one = sounded(vec![vec![5]]);
        assert_eq!(one.len(), 1);
        let len = one[0].1 - one[0].0;
        // (gg): one note twice as long.
        let two = sounded(vec![vec![5, 5]]);
        assert_eq!(two.len(), 1, "{two:?}");
        assert_eq!(two[0].1 - two[0].0, 2 * len);
        // A different pitch, or another syllable, is another note.
        assert_eq!(sounded(vec![vec![5, 6]]).len(), 2);
        assert_eq!(sounded(vec![vec![5], vec![5]]).len(), 2);
    }
}
