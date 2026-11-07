//! Gregorian chant in square notation, modelled on GABC: a sequence of syllables, each with its
//! text and the neumes sung on it.
//!
//! Notes are stored as staff positions under a clef (as chant is written) and resolved to MIDI
//! pitches for playback. A chant track also carries an ordinary transcription in its bars
//! ([`transcribe`]), one beat per chant note, so playback, the cursor, audio sync and the modern
//! formats work on chant like on any other track.

use serde::{Deserialize, Serialize};

use crate::{
    Bar, Beat, Duration, DurationValue, MasterBar, Note, TICKS_PER_QUARTER, TimeSignature, Voice,
};

/// A chant piece.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chant {
    pub syllables: Vec<Syllable>,
    /// Number of staff lines (2–5, usually 4).
    pub staff_lines: u8,
    /// How note lengths are read for playback and modern transcription.
    pub interpretation: Interpretation,
    /// GABC header fields in file order (`name`, `mode`, `office-part`, ...).
    pub headers: Vec<(String, String)>,
}

impl Default for Chant {
    fn default() -> Self {
        Self {
            syllables: Vec::new(),
            staff_lines: 4,
            interpretation: Interpretation::default(),
            headers: Vec::new(),
        }
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Syllable {
    /// Text as written in GABC, markup included (`<i>…</i>`, `<sp>V/</sp>`, `{` `}`).
    pub text: String,
    /// The syllable begins a word (a space precedes it in GABC).
    pub word_start: bool,
    pub items: Vec<NeumeItem>,
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
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChantNote {
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
        "+" => "✠",
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
}

impl Transcription {
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
    let mut k = 0;
    for (si, s) in chant.syllables.iter().enumerate() {
        let mut first_in_syllable = true;
        for item in &s.items {
            match item {
                NeumeItem::Bar(b) => {
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
                    out.positions.push((out.bars.len(), beats.len()));
                    out.lengths.push(n.repeats());
                    for _ in 0..n.repeats() {
                        let mut beat = Beat {
                            duration: duration_of(ticks),
                            notes: vec![Note::new(0, r.midi)],
                            ..Beat::default()
                        };
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
    let Some(chant) = score.tracks.get(track).and_then(|t| t.chant.as_deref()) else {
        return;
    };
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

/// Groups a run of notes (no separators) into neumes (ligatures): the pes, clivis, torculus,
/// porrectus, climacus, scandicus and so on, as every renderer and converter shows them.
pub fn group_neumes<'a>(run: &[(usize, &'a ChantNote)]) -> Vec<Vec<(usize, &'a ChantNote)>> {
    let ligates = |n: &ChantNote| matches!(n.shape(), Shape::Punctum) && !n.cavum() && !n.linea();
    let mut out: Vec<Vec<(usize, &ChantNote)>> = Vec::new();
    let mut cur: Vec<(usize, &ChantNote)> = Vec::new();
    for (i, &(k, n)) in run.iter().enumerate() {
        let extend = match cur.last() {
            None => false,
            Some(&(_, prev)) => {
                let starts_ok = match cur[0].1.shape() {
                    Shape::Punctum => ligates(cur[0].1) || cur[0].1.initio_debilis,
                    // A quilisma or square note joins the next, higher note.
                    Shape::Quilisma | Shape::Quadratum => cur.len() == 1 && n.pitch > prev.pitch,
                    _ => false,
                };
                let dirs: Vec<bool> = cur
                    .windows(2)
                    .map(|p| p[1].1.pitch > p[0].1.pitch)
                    .chain([n.pitch > prev.pitch])
                    .collect();
                let pattern_ok = matches!(
                    dirs.as_slice(),
                    [_] | [true, false]
                        | [false, true]
                        | [true, false, true]
                        | [false, true, false]
                );
                // Scandicus: three rising notes are a punctum and a pes.
                let scandicus = cur.len() == 1
                    && n.pitch > prev.pitch
                    && run
                        .get(i + 1)
                        .is_some_and(|&(_, nx)| nx.pitch > n.pitch && ligates(nx));
                starts_ok
                    && ligates(n)
                    && n.pitch != prev.pitch
                    && pattern_ok
                    && !scandicus
                    && (prev.liquescence() == Liquescence::None)
            }
        };
        if !extend && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        cur.push((k, n));
    }
    if !cur.is_empty() {
        out.push(cur);
    }
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
                },
                Syllable {
                    text: "ve".into(),
                    word_start: false,
                    items: vec![note(8, &["."])],
                },
                Syllable {
                    text: String::new(),
                    word_start: true,
                    items: vec![NeumeItem::Bar(";".into())],
                },
                Syllable {
                    text: "Re".into(),
                    word_start: true,
                    items: vec![note(8, &["ss"])],
                },
            ],
            ..Chant::default()
        };
        let t = transcribe(&chant);
        assert_eq!(t.master_bars.len(), 2);
        let beats = &t.bars[0].voices[0].beats;
        let pitches: Vec<u8> = beats.iter().map(|b| b.notes[0].fret).collect();
        assert_eq!(pitches, [65, 70, 70]); // F4, B♭4, B♭4 (same word)
        assert_eq!(beats[2].duration.value, DurationValue::Quarter);
        assert_eq!(beats[0].lyrics, ["Sal-"]);
        // The flat ends with the word; the distropha sounds twice.
        let b2 = &t.bars[1].voices[0].beats;
        assert_eq!(b2.len(), 2);
        assert_eq!(b2[0].notes[0].fret, 71);
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
