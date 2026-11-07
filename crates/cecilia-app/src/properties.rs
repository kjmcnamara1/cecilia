//! The properties pane: the score's, the cursor track's and the lyrics' properties as a list
//! of rows along the right of the window, in three tabs. `<leader>p` shows it, `<leader>P`
//! focuses it (Properties mode: `j`/`k` choose a property, `h`/`l` or `1`–`3` a tab, `Enter`
//! edits, `za` folds a section). The rows are built here; the frontends only draw them.

use std::cell::Cell;
use std::collections::HashSet;

use crate::app::{App, PaneKind};
use crate::command::{Command, Strip};
use crate::config::Mode;
use crate::keys::{Key, KeyCode};
use crate::picker::PickerKind;
use crate::textobj;
use cecilia_core::TrackKind;

/// Width of the pane, in columns.
pub const WIDTH: u16 = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PropTab {
    #[default]
    Score,
    Tracks,
    Lyrics,
}

impl PropTab {
    pub const ALL: [PropTab; 3] = [PropTab::Score, PropTab::Tracks, PropTab::Lyrics];

    pub fn name(self) -> &'static str {
        match self {
            PropTab::Score => "score",
            PropTab::Tracks => "track",
            PropTab::Lyrics => "lyrics",
        }
    }

    /// The tab's title as drawn.
    pub fn title(self) -> String {
        self.name().to_uppercase()
    }

    pub fn parse(s: &str) -> Option<PropTab> {
        let s = s.trim();
        // "tracks" is the tab's old name (saved workspaces).
        let s = if s == "tracks" { "track" } else { s };
        PropTab::ALL.into_iter().find(|t| t.name() == s)
    }

    fn step(self, by: isize) -> PropTab {
        let i = PropTab::ALL.iter().position(|&t| t == self).unwrap_or(0) as isize;
        PropTab::ALL[(i + by).rem_euclid(3) as usize]
    }
}

/// A foldable group of properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    ScoreInfo,
    /// A chant score's reading: interpretation and staff lines.
    ScoreChant,
    ScorePage,
    TrackInfo,
    TrackNotation,
    TrackSound,
}

impl Section {
    pub fn name(self) -> &'static str {
        match self {
            Section::ScoreInfo => "Info",
            Section::ScoreChant => "Chant",
            Section::ScorePage => "Page",
            Section::TrackInfo => "Info",
            Section::TrackNotation => "Notation",
            Section::TrackSound => "Sound",
        }
    }
}

/// A property the pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Title,
    Subtitle,
    Artist,
    Album,
    Words,
    Music,
    Copyright,
    TabAuthor,
    Instructions,
    PageWidth,
    PageHeight,
    MarginLeft,
    MarginRight,
    MarginTop,
    MarginBottom,
    PageScale,
    /// A chant score's GABC header at this index of [`chant_header_keys`].
    ChantHeader(usize),
    ChantInterpretation,
    ChantStaffLines,
    /// The initial clef of a chant score's track.
    ChantClef,
    TrackName,
    TrackKind,
    TrackColor,
    Tuning,
    FretCount,
    Capo,
    /// The track's staff at this index; the one past the last adds a staff.
    StaffAt(usize),
    Program,
    Channel,
    Bank,
    Volume,
    Pan,
    Mute,
    Solo,
    /// All the track's lyrics as one lyrics text.
    Lyrics,
}

impl Field {
    /// A switch: on or off, flipped by a click or `<CR>`.
    pub fn is_switch(self) -> bool {
        matches!(self, Field::Mute | Field::Solo)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RowKind {
    Header {
        section: Section,
        folded: bool,
    },
    Field {
        field: Field,
        editable: bool,
    },
    /// Text without a property (an empty list, the track the lyrics belong to).
    Note,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropRow {
    pub kind: RowKind,
    pub label: String,
    pub value: String,
    /// Read-only text beside the value box (a color's hex code, a tuning's pitches).
    pub aside: String,
    /// A color shown in the value box.
    pub swatch: Option<crate::theme::Rgba>,
    /// An icon before the value.
    pub icon: Option<crate::icons::Icon>,
    /// A staff's clef: the text of its second box, and the icon.
    pub value2: String,
    pub icon2: Option<crate::icons::Icon>,
    /// Dim text shown when the value is empty.
    pub placeholder: &'static str,
}

/// Row heights, in lines (the frontends scale them): a section title with room above, a
/// property (label, value box, room below), a note, and the lyrics' text area at least.
const HEADER_H: usize = 2;
const FIELD_H: usize = 3;
const NOTE_H: usize = 2;
pub const TEXT_AREA_H: usize = 8;

impl PropRow {
    pub fn height(&self) -> usize {
        match self.kind {
            RowKind::Header { .. } => HEADER_H,
            RowKind::Note => NOTE_H,
            RowKind::Field {
                field: Field::Lyrics,
                ..
            } => TEXT_AREA_H,
            // A staff after the first has no label of its own.
            RowKind::Field { .. } if self.label.is_empty() => FIELD_H - 1,
            RowKind::Field { .. } => FIELD_H,
        }
    }
}

/// A property being typed. A single line is typed in insert mode; the lyrics text area is
/// edited like vim (`modal`): insert and normal mode.
#[derive(Debug, Clone, PartialEq)]
pub struct PropEdit {
    pub field: Field,
    pub text: String,
    /// The caret, as a character index.
    pub caret: usize,
    /// Edited with vim's modes (the lyrics).
    pub modal: bool,
    /// In insert mode (else normal mode of a modal edit).
    pub insert: bool,
    /// The keys typed so far of a normal- or visual-mode command (`d`, `di`, `g` …).
    pending: String,
    /// Visual mode: where the selection began, and whether it takes whole lines (`V`). The
    /// selection runs from there to the caret, both included.
    pub visual: Option<(usize, bool)>,
    /// Where a selection (made with the mouse) began; it runs to the caret.
    pub anchor: Option<usize>,
    /// A chant track's lyrics as typed (GABC), which the text area shows while it is edited:
    /// the chant read from it is written back in its own layout, and half-typed GABC (an
    /// unclosed group) can't be read yet.
    full: Option<String>,
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl PropEdit {
    fn new(field: Field, text: String) -> Self {
        let caret = text.chars().count();
        Self {
            field,
            text,
            caret,
            modal: field == Field::Lyrics,
            insert: true,
            pending: String::new(),
            visual: None,
            anchor: None,
            full: None,
        }
    }

    /// The selected characters, from the first to one past the last.
    pub fn selection(&self) -> Option<(usize, usize)> {
        if let Some((v, lines)) = self.visual {
            let c = self.chars();
            let (lo, hi) = (v.min(self.caret), v.max(self.caret));
            let (a, b) = if lines {
                textobj::line_range(&c, lo, hi)
            } else {
                (lo, (hi + 1).min(c.len()))
            };
            return (a < b).then_some((a, b));
        }
        let a = self.anchor.filter(|&a| a != self.caret)?;
        Some((a.min(self.caret), a.max(self.caret).min(self.len())))
    }

    fn select_all(&mut self) {
        self.visual = None;
        self.anchor = Some(0);
        self.caret = self.len();
    }

    /// Selects the word at `at`: its letters, or the run of spaces or the sign it is on.
    fn select_word(&mut self, at: usize) {
        let c = self.chars();
        if c.is_empty() {
            return;
        }
        let at = at.min(c.len() - 1);
        let class = |ch: char| match ch {
            _ if is_word(ch) => 0,
            ' ' | '\t' => 1,
            '\n' => 2,
            _ => 3,
        };
        let k = class(c[at]);
        let (mut a, mut b) = (at, at + 1);
        // Signs are taken one at a time.
        if k != 3 {
            while a > 0 && class(c[a - 1]) == k {
                a -= 1;
            }
            while b < c.len() && class(c[b]) == k {
                b += 1;
            }
        }
        self.anchor = Some(a);
        self.caret = b;
    }

    fn selected_text(&self) -> Option<String> {
        let (a, b) = self.selection()?;
        Some(self.chars()[a..b].iter().collect())
    }

    /// Inserts a text at the caret, line breaks as `\n`.
    fn insert_str(&mut self, text: &str) {
        for c in text.replace("\r\n", "\n").replace('\r', "\n").chars() {
            self.insert_char(c);
        }
    }

    fn chars(&self) -> Vec<char> {
        self.text.chars().collect()
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn byte(&self, caret: usize) -> usize {
        self.text
            .char_indices()
            .nth(caret)
            .map_or(self.text.len(), |(i, _)| i)
    }

    fn insert_char(&mut self, c: char) {
        let at = self.byte(self.caret);
        self.text.insert(at, c);
        self.caret += 1;
    }

    fn backspace(&mut self) {
        if self.caret > 0 {
            self.caret -= 1;
            let at = self.byte(self.caret);
            self.text.remove(at);
        }
    }

    fn delete(&mut self) {
        let at = self.byte(self.caret);
        if at < self.text.len() {
            self.text.remove(at);
        }
    }

    /// Removes the characters `from..to`.
    fn remove(&mut self, from: usize, to: usize) {
        let (a, b) = (self.byte(from), self.byte(to));
        self.text.replace_range(a..b, "");
    }

    /// The caret's (line, column), lines being split at newlines.
    pub fn line_col(&self) -> (usize, usize) {
        let before: Vec<char> = self.text.chars().take(self.caret).collect();
        let line = before.iter().filter(|&&c| c == '\n').count();
        let col = before.iter().rev().take_while(|&&c| c != '\n').count();
        (line, col)
    }

    fn line_start(&self) -> usize {
        self.caret - self.line_col().1
    }

    fn line_end(&self) -> usize {
        let rest = self.text.chars().skip(self.caret);
        self.caret + rest.take_while(|&c| c != '\n').count()
    }

    /// Moves the caret `by` visual lines (as the text area wraps them), if the lines drawn are
    /// those of this text; else by lines of the text.
    fn move_visual(&mut self, by: isize, lines: &[(usize, usize)]) {
        if lines.last().map(|l| l.1) != Some(self.len()) {
            return self.move_lines(by);
        }
        let (row, col) = caret_in(lines, self.caret);
        let to = (row as isize + by).clamp(0, lines.len() as isize - 1) as usize;
        let (start, end) = lines[to];
        self.caret = start + col.min(end - start);
    }

    /// Moves the caret `by` lines, keeping the column where the line is long enough.
    fn move_lines(&mut self, by: isize) {
        let (line, col) = self.line_col();
        let lens: Vec<usize> = self.text.split('\n').map(|l| l.chars().count()).collect();
        let to = (line as isize + by).clamp(0, lens.len() as isize - 1) as usize;
        let start: usize = lens[..to].iter().map(|l| l + 1).sum();
        self.caret = start + col.min(lens[to]);
    }

    /// In normal mode the caret rests on a character, not past the end of its line.
    fn rest(&mut self) {
        if !self.insert && self.caret > self.line_start() && self.caret >= self.line_end() {
            self.caret -= 1;
        }
    }

    /// `w`, `e` and `b` (`W`, `E`, `B` with `big`).
    fn word_forward(&mut self, big: bool) {
        self.caret = textobj::word_forward(&self.chars(), self.caret, big);
    }

    fn word_back(&mut self, big: bool) {
        self.caret = textobj::word_back(&self.chars(), self.caret, big);
    }

    /// The first character of the caret's line that isn't a blank.
    fn first_non_blank(&self) -> usize {
        let start = self.line_start();
        let rest = self.text.chars().skip(start);
        start + rest.take_while(|c| *c == ' ' || *c == '\t').count()
    }

    /// Where a motion takes the caret (`lines`: the visual lines, for `j` and `k`).
    fn target(&self, m: Motion, lines: &[(usize, usize)]) -> usize {
        let c = self.chars();
        match m {
            Motion::Left => self
                .caret
                .saturating_sub(usize::from(self.caret > self.line_start())),
            Motion::Right => (self.caret + 1).min(self.line_end()),
            Motion::Up | Motion::Down => {
                let mut e = self.clone();
                e.move_visual(if m == Motion::Up { -1 } else { 1 }, lines);
                e.caret
            }
            Motion::Word(big) => textobj::word_forward(&c, self.caret, big),
            Motion::End(big) => textobj::word_end(&c, self.caret, big),
            Motion::Back(big) => textobj::word_back(&c, self.caret, big),
            Motion::LineStart => self.line_start(),
            Motion::FirstNonBlank => self.first_non_blank(),
            Motion::LineEnd => self.line_end(),
            Motion::First => 0,
            Motion::Last => textobj::line_bounds(&c, c.len()).0,
        }
    }

    /// The characters an operator (`d`, `c`, `y`) takes with a motion: from, to, and whether
    /// whole lines.
    fn motion_range(&self, op: char, m: Motion) -> Option<(usize, usize, bool)> {
        let c = self.chars();
        let n = c.len();
        let (ls, le) = (self.line_start(), self.line_end());
        let at = self.caret;
        let r = match m {
            Motion::Left => (at > ls).then(|| (at - 1, at, false))?,
            Motion::Right => (at < le).then(|| (at, at + 1, false))?,
            // `cw` on a word changes to its end, as `ce`.
            Motion::Word(big) if op == 'c' && at < n && !c[at].is_whitespace() => {
                let same = |i: usize| {
                    i < n && !c[i].is_whitespace() && (big || is_word(c[i]) == is_word(c[at]))
                };
                let mut z = at + 1;
                while same(z) {
                    z += 1;
                }
                (at, z, false)
            }
            // `dw` on the last word of a line stops at its end.
            Motion::Word(big) => {
                let t = textobj::word_forward(&c, at, big);
                (at, if t > le && at < le { le } else { t }, false)
            }
            Motion::End(big) => (at, (textobj::word_end(&c, at, big) + 1).min(n), false),
            Motion::Back(big) => (textobj::word_back(&c, at, big), at, false),
            Motion::LineStart => (ls, at, false),
            Motion::FirstNonBlank => {
                let f = self.first_non_blank();
                (f.min(at), f.max(at), false)
            }
            Motion::LineEnd => (at, le, false),
            Motion::Up => {
                let prev = textobj::line_bounds(&c, ls.checked_sub(1)?).0;
                let (a, b) = textobj::line_range(&c, prev, at);
                (a, b, true)
            }
            Motion::Down => {
                if le >= n {
                    return None;
                }
                let (a, b) = textobj::line_range(&c, at, le + 1);
                (a, b, true)
            }
            Motion::First => {
                let (a, b) = textobj::line_range(&c, 0, at);
                (a, b, true)
            }
            Motion::Last => {
                let (a, b) = textobj::line_range(&c, at, n);
                (a, b, true)
            }
        };
        (r.0 < r.1 || r.2).then_some(r)
    }

    /// A text object at the caret (`iw`, `ap`, `i(` …): from, to, and whether whole lines.
    fn object_range(&self, around: bool, o: char) -> Option<(usize, usize, bool)> {
        let c = self.chars();
        let at = self.caret;
        let (a, b) = match o {
            'w' | 'W' => textobj::word_object(&c, at, around, o == 'W')?,
            'p' => {
                let (a, b) = textobj::paragraph(&c, at, around)?;
                return Some((a, b, true));
            }
            _ => match textobj::object_chars(o)? {
                (q, z) if q == z => textobj::quote_object(&c, at, q, around)?,
                (open, close) => textobj::pair_object(&c, at, open, close, around)?,
            },
        };
        // (Empty brackets are an empty object: `ci(` types into them.)
        Some((a, b, false))
    }
}

/// A motion of the lyrics text area's normal and visual modes.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Motion {
    Left,
    Right,
    Up,
    Down,
    /// `w`, `e`, `b`; `W`, `E`, `B` with `true`.
    Word(bool),
    End(bool),
    Back(bool),
    LineStart,
    FirstNonBlank,
    LineEnd,
    /// `gg` and `G`.
    First,
    Last,
}

impl Motion {
    fn of(c: char) -> Option<Motion> {
        Some(match c {
            'h' => Motion::Left,
            'l' => Motion::Right,
            'k' => Motion::Up,
            'j' => Motion::Down,
            'w' | 'W' => Motion::Word(c == 'W'),
            'e' | 'E' => Motion::End(c == 'E'),
            'b' | 'B' => Motion::Back(c == 'B'),
            '0' => Motion::LineStart,
            '^' => Motion::FirstNonBlank,
            '$' => Motion::LineEnd,
            'G' => Motion::Last,
            _ => return None,
        })
    }
}

/// What an operator acts on: its line (`dd`), a motion's way, or a text object.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Target {
    Line,
    Move(Motion),
    /// Around (`a`) or inside (`i`), and the object's character.
    Object(bool, char),
}

/// The keys of a normal- or visual-mode command as typed so far.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Typed {
    /// More keys to come.
    More,
    /// Not a command.
    Bad,
    Move(Motion),
    /// An operator (`d`, `c`, `y`) and what it acts on.
    Op(char, Target),
    /// A text object, selected (visual mode).
    Object(bool, char),
    /// `r` and the character to put.
    Replace(char),
    /// A command of one key.
    Cmd(char),
}

fn parse_typed(keys: &[char], visual: bool) -> Typed {
    match keys {
        [] | ['g'] => Typed::More,
        ['g', 'g'] => Typed::Move(Motion::First),
        ['g', _] => Typed::Bad,
        ['r'] if !visual => Typed::More,
        ['r', c] if !visual => Typed::Replace(*c),
        ['i' | 'a'] if visual => Typed::More,
        [ia @ ('i' | 'a'), o] if visual => Typed::Object(*ia == 'a', *o),
        [op @ ('d' | 'c' | 'y'), rest @ ..] if !visual => match rest {
            [] | ['g'] | ['i'] | ['a'] => Typed::More,
            [x] if x == op => Typed::Op(*op, Target::Line),
            ['g', 'g'] => Typed::Op(*op, Target::Move(Motion::First)),
            [ia @ ('i' | 'a'), o] => Typed::Op(*op, Target::Object(*ia == 'a', *o)),
            [m] => Motion::of(*m).map_or(Typed::Bad, |m| Typed::Op(*op, Target::Move(m))),
            _ => Typed::Bad,
        },
        [c] => Motion::of(*c).map_or(Typed::Cmd(*c), Typed::Move),
        _ => Typed::Bad,
    }
}

/// A search being typed in the pane (`/`): the field at its bottom.
#[derive(Debug, Clone, PartialEq)]
pub struct PropSearch {
    pub query: String,
    /// Where the search began: Escape goes back there.
    origin_row: usize,
    origin_caret: usize,
    /// Searching the lyrics text rather than the rows.
    in_text: bool,
}

#[derive(Debug, Default)]
pub struct PropsState {
    /// The box of a staff row selected: 0 the notation, 1 the clef.
    pub staff_box: u8,
    pub search: Option<PropSearch>,
    /// The `/` register: what `n` and `N` look for.
    pub last_search: String,
    pub tab: PropTab,
    /// Selected row of the tab's [`App::property_rows`].
    pub selected: usize,
    pub folded: HashSet<Section>,
    pub edit: Option<PropEdit>,
    /// The lines the lyrics text is wrapped to as last drawn (character ranges), for moving
    /// by visual line.
    pub visual: std::cell::RefCell<Vec<(usize, usize)>>,
    /// Where the lyrics text area's caret was when it was left (the track, the caret): it is
    /// there again when the text area is entered.
    text_caret: Option<(usize, usize)>,
    /// The first line of the lyrics text area shown, and the caret when it last followed it.
    text_scroll: Cell<usize>,
    text_seen: Cell<usize>,
    /// The selected row when the window last followed it: while that is the selection the
    /// wheel scrolls freely.
    seen: Cell<usize>,
    /// First row shown; kept by [`App::property_window`].
    scroll: Cell<usize>,
    /// Centre the selected row at the next draw (`zz`), when the room is known.
    center: Cell<bool>,
}

/// Small capitals for terminals: each word's first letter stays, the rest turn to small caps.
pub fn small_caps(text: &str) -> String {
    const SMALL: [char; 26] = [
        'ᴀ', 'ʙ', 'ᴄ', 'ᴅ', 'ᴇ', 'ꜰ', 'ɢ', 'ʜ', 'ɪ', 'ᴊ', 'ᴋ', 'ʟ', 'ᴍ', 'ɴ', 'ᴏ', 'ᴘ', 'ǫ', 'ʀ',
        'ꜱ', 'ᴛ', 'ᴜ', 'ᴠ', 'ᴡ', 'x', 'ʏ', 'ᴢ',
    ];
    let mut start = true;
    text.chars()
        .map(|c| {
            let out = if !start && c.is_ascii_lowercase() {
                SMALL[(c as u8 - b'a') as usize]
            } else {
                c
            };
            start = !c.is_alphanumeric();
            out
        })
        .collect()
}

/// The lines `text` wraps to as character ranges (end exclusive): at newlines, and at the last
/// space (else anywhere) once a line no longer `fits`.
pub fn wrap_ranges(text: &str, fits: impl Fn(&str) -> bool) -> Vec<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let (mut start, mut i, mut last_space) = (0, 0, None);
    while i <= chars.len() {
        if i == chars.len() || chars[i] == '\n' {
            out.push((start, i));
            start = i + 1;
            last_space = None;
            i += 1;
            continue;
        }
        if chars[i] == ' ' {
            last_space = Some(i);
        }
        let line: String = chars[start..=i].iter().collect();
        if i > start && !fits(&line) {
            match last_space.filter(|&s| s > start) {
                Some(s) => {
                    out.push((start, s));
                    start = s + 1;
                    i = start;
                }
                None => {
                    out.push((start, i));
                    start = i;
                }
            }
            last_space = None;
            continue;
        }
        i += 1;
    }
    out
}

/// The (line, column) of a caret in [`wrap_ranges`]' lines.
pub fn caret_in(lines: &[(usize, usize)], caret: usize) -> (usize, usize) {
    let row = lines.iter().rposition(|&(s, _)| s <= caret).unwrap_or(0);
    (row, caret - lines.get(row).map_or(0, |l| l.0))
}

/// The GABC header fields a chant score's Score tab always lists, in this order; headers the
/// chant holds besides these follow in file order.
const STANDARD_HEADERS: [&str; 8] = [
    "name",
    "office-part",
    "mode",
    "annotation",
    "book",
    "transcriber",
    "commentary",
    "user-notes",
];

/// The header keys the Score tab of a chant score shows: the standard ones, then any other the
/// chant has. [`Field::ChantHeader`] indexes this list.
pub fn chant_header_keys(chant: &cecilia_core::chant::Chant) -> Vec<String> {
    let mut keys: Vec<String> = STANDARD_HEADERS.iter().map(|k| k.to_string()).collect();
    for (k, _) in &chant.headers {
        if !keys.iter().any(|x| x == k) {
            keys.push(k.clone());
        }
    }
    keys
}

fn onoff(b: bool) -> String {
    if b { "on" } else { "off" }.into()
}

/// A track's lyrics as one text: its own (the source of the syllables on its beats), or a
/// chant track's chant as GABC (without the header).
pub fn full_lyrics(score: &cecilia_core::Score, track: usize) -> String {
    let Some(t) = score.tracks.get(track) else {
        return String::new();
    };
    match t.chant.as_deref() {
        Some(chant) => {
            let gabc = cecilia_formats::gabc::write(chant, "");
            let body = gabc.split_once("%%\n").map_or(gabc.as_str(), |(_, b)| b);
            body.trim_end().to_string()
        }
        // Syllables only on the beats (a chant track just made a staff track) get their text
        // written from them, as the next edit does.
        None if t.lyrics_text.trim().is_empty() && cecilia_core::lyrics::has_lyrics(t) => {
            cecilia_core::gabcplus::text_from_beats(score, track)
        }
        None => t.lyrics_text.clone(),
    }
}

fn kind_name(k: TrackKind) -> &'static str {
    match k {
        TrackKind::Fretted => "fretted",
        TrackKind::Drums => "drums",
        TrackKind::Pitched => "pitched",
        TrackKind::Chant => "chant",
    }
}

impl App {
    /// The rows of the shown tab, for the focused pane's file and cursor track, with folded
    /// sections' rows left out.
    pub fn property_rows(&self) -> Vec<PropRow> {
        let score = &self.buffer().score;
        let cursor = self.pane().cursor;
        let mut rows = Vec::new();
        let folded = &self.props.folded;
        let section = |rows: &mut Vec<PropRow>, s: Section| {
            let f = folded.contains(&s);
            rows.push(PropRow {
                kind: RowKind::Header {
                    section: s,
                    folded: f,
                },
                label: s.name().into(),
                value: String::new(),
                aside: String::new(),
                swatch: None,
                icon: None,
                value2: String::new(),
                icon2: None,
                placeholder: "",
            });
            !f
        };
        let field = |rows: &mut Vec<PropRow>, f: Field, label: &str, value: String, editable| {
            rows.push(PropRow {
                kind: RowKind::Field { field: f, editable },
                label: label.into(),
                value,
                aside: String::new(),
                swatch: None,
                icon: None,
                value2: String::new(),
                icon2: None,
                placeholder: "",
            })
        };
        let page_fields = |rows: &mut Vec<PropRow>| {
            let p = &score.page;
            for (f, l, v) in [
                (Field::PageWidth, "Width (mm)", p.width),
                (Field::PageHeight, "Height (mm)", p.height),
                (Field::MarginLeft, "Left margin", p.margin_left),
                (Field::MarginRight, "Right margin", p.margin_right),
                (Field::MarginTop, "Top margin", p.margin_top),
                (Field::MarginBottom, "Bottom margin", p.margin_bottom),
                (Field::PageScale, "Scale (%)", p.scale),
            ] {
                field(rows, f, l, v.to_string(), true);
            }
        };
        match self.props.tab {
            PropTab::Score if score.is_chant() => {
                let chant = score
                    .tracks
                    .first()
                    .and_then(|t| t.chant.as_deref())
                    .expect("a chant score has its chant");
                if section(&mut rows, Section::ScoreInfo) {
                    for (i, key) in chant_header_keys(chant).iter().enumerate() {
                        let value = chant
                            .headers
                            .iter()
                            .find(|(k, _)| k == key)
                            .map_or(String::new(), |(_, v)| v.clone());
                        field(&mut rows, Field::ChantHeader(i), key, value, true);
                    }
                }
                if section(&mut rows, Section::ScoreChant) {
                    field(
                        &mut rows,
                        Field::ChantInterpretation,
                        "Interpretation",
                        format!("{:?}", chant.interpretation).to_lowercase(),
                        true,
                    );
                    field(
                        &mut rows,
                        Field::ChantStaffLines,
                        "Staff lines",
                        chant.staff_lines.to_string(),
                        true,
                    );
                }
                if section(&mut rows, Section::ScorePage) {
                    page_fields(&mut rows);
                }
            }
            PropTab::Score => {
                let m = &score.meta;
                if section(&mut rows, Section::ScoreInfo) {
                    for (f, l, v) in [
                        (Field::Title, "Title", &m.title),
                        (Field::Subtitle, "Subtitle", &m.subtitle),
                        (Field::Artist, "Artist", &m.artist),
                        (Field::Album, "Album", &m.album),
                        (Field::Words, "Words", &m.words),
                        (Field::Music, "Music", &m.music),
                        (Field::Copyright, "Copyright", &m.copyright),
                        (Field::TabAuthor, "Tab author", &m.tab_author),
                        (Field::Instructions, "Instructions", &m.instructions),
                    ] {
                        field(&mut rows, f, l, v.clone(), true);
                    }
                }
                if section(&mut rows, Section::ScorePage) {
                    page_fields(&mut rows);
                }
            }
            PropTab::Tracks => {
                let Some(t) = score.tracks.get(cursor.track) else {
                    return vec![note("no track")];
                };
                let n = score.tracks.len();
                let chant_track = score.is_chant() && t.chant.is_some();
                rows.push(note(&format!("Track {} of {n}", cursor.track + 1)));
                if section(&mut rows, Section::TrackInfo) {
                    field(&mut rows, Field::TrackName, "Name", t.name.clone(), true);
                    if !chant_track {
                        field(
                            &mut rows,
                            Field::TrackKind,
                            "Kind",
                            kind_name(t.kind).into(),
                            false,
                        );
                    }
                    let shown = self.track_color(self.pane().buffer, cursor.track);
                    let custom = t.display_color.is_some();
                    let name = match (custom, crate::picker::color_name(shown)) {
                        (false, _) => "Automatic",
                        (true, Some(n)) => n,
                        (true, None) => "Custom",
                    };
                    field(&mut rows, Field::TrackColor, "Color", name.into(), true);
                    let last = rows.last_mut().unwrap();
                    last.aside = shown.hex();
                    last.swatch = Some(shown);
                }
                if chant_track {
                    // A chant has one clef to set; no staves, tuning, capo or frets.
                    if section(&mut rows, Section::TrackNotation) {
                        field(
                            &mut rows,
                            Field::ChantClef,
                            "Clef",
                            t.chant
                                .as_deref()
                                .map_or(String::new(), |c| c.initial_clef().gabc()),
                            true,
                        );
                        if let Some(c) = t.chant.as_deref() {
                            rows.last_mut().unwrap().icon =
                                Some(crate::icons::Icon::of_chant_clef(c.initial_clef().kind));
                        }
                    }
                } else if section(&mut rows, Section::TrackNotation) {
                    let fretted = t.kind == TrackKind::Fretted;
                    if t.kind != TrackKind::Drums && !t.strings.is_empty() {
                        let name = crate::picker::tuning_name(&t.strings).unwrap_or("Custom");
                        field(&mut rows, Field::Tuning, "Tuning", name.into(), true);
                        rows.last_mut().unwrap().aside = crate::picker::tuning_notes(&t.strings);
                    }
                    if fretted {
                        field(
                            &mut rows,
                            Field::FretCount,
                            "Frets",
                            t.fret_count.to_string(),
                            true,
                        );
                        field(&mut rows, Field::Capo, "Capo", t.capo.to_string(), true);
                    }
                    {
                        let staves = t.staves();
                        // One key, "Staves", over a field per staff and an empty one to add
                        // another.
                        for (i, s) in staves.iter().enumerate() {
                            let label = if i == 0 { "Staves" } else { "" };
                            field(
                                &mut rows,
                                Field::StaffAt(i),
                                label,
                                crate::picker::notation_label(*s).into(),
                                true,
                            );
                            let row = rows.last_mut().unwrap();
                            row.icon = Some(crate::icons::Icon::of_notation(*s));
                            row.value2 = crate::picker::clef_label(*s);
                            row.icon2 = crate::icons::Icon::of_clef(*s);
                        }
                        field(
                            &mut rows,
                            Field::StaffAt(staves.len()),
                            "",
                            String::new(),
                            true,
                        );
                        rows.last_mut().unwrap().placeholder = "Add Staff";
                    }
                }
                if section(&mut rows, Section::TrackSound) {
                    field(
                        &mut rows,
                        Field::Program,
                        "Instrument",
                        crate::tracks::instrument_name(t),
                        true,
                    );
                    rows.last_mut().unwrap().icon = Some(crate::icons::Icon::of_track(t));
                    if !chant_track {
                        field(
                            &mut rows,
                            Field::Channel,
                            "Channel",
                            (t.midi.channel + 1).to_string(),
                            true,
                        );
                        field(
                            &mut rows,
                            Field::Bank,
                            "Bank",
                            t.midi.bank.to_string(),
                            true,
                        );
                    }
                    field(
                        &mut rows,
                        Field::Volume,
                        "Volume",
                        t.midi.volume.to_string(),
                        true,
                    );
                    field(&mut rows, Field::Pan, "Pan", t.midi.pan.to_string(), true);
                    field(&mut rows, Field::Mute, "Mute", onoff(t.mute), true);
                    field(&mut rows, Field::Solo, "Solo", onoff(t.solo), true);
                }
            }
            PropTab::Lyrics => {
                let Some(t) = score.tracks.get(cursor.track) else {
                    return vec![note("no track")];
                };
                let name = if t.name.trim().is_empty() {
                    format!("Track {}", cursor.track + 1)
                } else {
                    t.name.trim().to_string()
                };
                rows.push(note(&name));
                // The text area has no label.
                field(
                    &mut rows,
                    Field::Lyrics,
                    "",
                    self.shown_lyrics(cursor.track),
                    true,
                );
            }
        }
        rows
    }

    /// The chant of the cursor track, if the focused buffer is a chant score.
    fn chant_of_cursor(&self) -> Option<&cecilia_core::chant::Chant> {
        let score = &self.buffer().score;
        if !score.is_chant() {
            return None;
        }
        score.tracks.get(self.pane().cursor.track)?.chant.as_deref()
    }

    /// The lyrics text area's text for a track: a chant track's GABC, else the lyrics text.
    pub fn shown_lyrics(&self, track: usize) -> String {
        full_lyrics(&self.buffer().score, track)
    }

    /// Whether the lyrics text area (of the cursor track) holds GABC (a chant track) rather than
    /// the modern lyrics syntax: it decides how the text is colored.
    pub fn lyrics_is_gabc(&self) -> bool {
        self.buffer().score.is_chant()
    }

    /// The selected row, clamped to the rows there are.
    pub fn property_selected(&self) -> usize {
        self.props
            .selected
            .min(self.property_rows().len().saturating_sub(1))
    }

    /// The first row to draw, given `visible` lines of room: the selected row is kept in view
    /// when it has just changed; else the wheel's scrolling stands.
    pub fn property_window(&self, visible: usize) -> usize {
        let rows = self.property_rows();
        let sel = self.property_selected();
        let height = |r: &PropRow| r.height();
        let mut first = self.props.scroll.get().min(rows.len().saturating_sub(1));
        if self.props.center.take() && !rows.is_empty() {
            self.props.seen.set(sel);
            let budget = visible.saturating_sub(height(&rows[sel])) / 2;
            first = sel;
            while first > 0 && rows[first - 1..sel].iter().map(height).sum::<usize>() <= budget {
                first -= 1;
            }
        } else if sel != self.props.seen.get() {
            self.props.seen.set(sel);
            first = first.min(sel);
            // Scroll down until the selected row fits.
            while first < sel && rows[first..=sel].iter().map(height).sum::<usize>() > visible {
                first += 1;
            }
        }
        // Don't leave room empty below the last row.
        while first > 0 && rows[first - 1..].iter().map(height).sum::<usize>() <= visible {
            first -= 1;
        }
        self.props.scroll.set(first);
        first
    }

    /// The first line of the lyrics text area to draw, of `total` lines with `shown` in view:
    /// the wheel's scrolling stands, until the caret (row `row` of the lines) has moved.
    pub fn text_window(&self, caret: Option<(usize, usize)>, total: usize, shown: usize) -> usize {
        let mut skip = self
            .props
            .text_scroll
            .get()
            .min(total.saturating_sub(shown));
        if let Some((caret, row)) = caret {
            if caret != self.props.text_seen.get() {
                self.props.text_seen.set(caret);
                if row < skip {
                    skip = row;
                } else if row >= skip + shown {
                    skip = row + 1 - shown;
                }
            }
        }
        self.props.text_scroll.set(skip);
        skip
    }

    /// Puts the caret of the text being edited at character `at` (a click).
    pub fn set_text_caret(&mut self, at: usize) {
        if let Some(e) = self.props.edit.as_mut() {
            e.caret = at.min(e.len());
            e.anchor = None;
            e.visual = None;
            e.rest();
        }
    }

    /// A double click in the text area being edited: in normal mode it enters insert mode at
    /// the click, in insert mode it selects the word there.
    pub fn text_double_click(&mut self, at: usize) {
        let Some(e) = self.props.edit.as_mut().filter(|e| e.modal) else {
            return;
        };
        e.visual = None;
        if e.insert {
            e.select_word(at);
        } else {
            e.caret = at.min(e.len());
            e.anchor = None;
            e.insert = true;
            self.ed.group = Some(false);
        }
    }

    /// Ctrl-A, Ctrl-C, Ctrl-X and Ctrl-V in the text area, and `y` (with a selection), `p` and
    /// `P` in its normal mode. True when the key was used up.
    fn clipboard_key(&mut self, key: Key) -> bool {
        let Some(edit) = self.props.edit.as_mut().filter(|e| e.modal) else {
            return false;
        };
        let insert = edit.insert;
        let selected = edit.selected_text();
        let ready = edit.pending.is_empty();
        let ctrl = |c: char| key.ctrl && key.code == KeyCode::Char(c);
        let plain =
            |c: char| ready && !key.ctrl && !key.alt && !key.sup && key.code == KeyCode::Char(c);
        if ctrl('a') {
            edit.select_all();
        } else if ctrl('c') || (!insert && plain('y') && selected.is_some()) {
            if let Some(t) = &selected {
                crate::clipboard::set(t);
            }
            if !ctrl('c') {
                edit.anchor = None;
            }
        } else if ctrl('x') {
            let Some(t) = selected else {
                return true;
            };
            crate::clipboard::set(&t);
            self.remove_selection();
        } else if ctrl('v') || (!insert && (plain('p') || plain('P'))) {
            let after = !insert && !plain('P');
            self.paste(after);
        } else {
            return false;
        }
        true
    }

    fn remove_selection(&mut self) {
        if let Some(e) = self.props.edit.as_mut() {
            if let Some((a, b)) = e.selection() {
                e.remove(a, b);
                e.caret = a;
                e.anchor = None;
                e.visual = None;
                e.rest();
                if !e.insert {
                    self.ed.group = None;
                }
                self.apply_live();
            }
        }
    }

    /// Puts the clipboard's text in: at the caret in insert mode, over a selection (a visual
    /// one too), else after (`after`) or before the caret, as a step of its own. Lines (text
    /// ending in a line break) go below or above the caret's line in normal mode.
    fn paste(&mut self, after: bool) {
        let text = crate::clipboard::get();
        let Some(e) = self.props.edit.as_mut() else {
            return;
        };
        if text.is_empty() {
            return;
        }
        if !e.insert {
            self.ed.group = None;
        }
        if !e.insert && e.selection().is_none() && text.ends_with('\n') {
            let (start, end, n) = (e.line_start(), e.line_end(), e.len());
            let (at, text, skip) = if !after {
                (start, text, 0)
            } else if end < n {
                (end + 1, text, 0)
            } else {
                (n, format!("\n{}", &text[..text.len() - 1]), 1)
            };
            e.caret = at;
            e.insert_str(&text);
            e.caret = at + skip;
            e.anchor = None;
            return self.apply_live();
        }
        let replaced = e.selection();
        if let Some((a, b)) = replaced {
            e.remove(a, b);
            e.caret = a;
        }
        e.anchor = None;
        e.visual = None;
        // Normal mode: after the caret's character, unless the line is empty.
        if !e.insert && replaced.is_none() && after && e.caret < e.line_end() {
            e.caret += 1;
        }
        let start = e.caret;
        e.insert_str(&text);
        if !e.insert {
            e.caret = e.caret.saturating_sub(1).max(start);
        }
        self.apply_live();
    }

    /// Whether the lyrics text area is being edited and `row` is it.
    pub fn text_editing(&self, row: usize) -> bool {
        self.props.edit.as_ref().is_some_and(|e| e.modal) && row == self.props.selected
    }

    /// A mouse press in the text area: the caret goes there and a selection may begin.
    pub fn text_click(&mut self, at: usize) {
        if let Some(e) = self.props.edit.as_mut() {
            e.caret = at.min(e.len());
            e.anchor = Some(e.caret);
            e.visual = None;
            e.pending.clear();
            e.rest();
        }
    }

    /// The mouse moves with the button down: the selection grows to `at`.
    pub fn text_drag(&mut self, at: usize) {
        if let Some(e) = self.props.edit.as_mut() {
            e.caret = at.min(e.len());
        }
    }

    /// The button comes up: a press without a drag selects nothing.
    pub fn text_release(&mut self) {
        if let Some(e) = self.props.edit.as_mut() {
            if e.anchor == Some(e.caret) {
                e.anchor = None;
            }
        }
    }

    /// Keys while text is selected: typing replaces it, Backspace and Delete remove it, and in
    /// normal mode `d`, `x` and `c` do. True when the key was used up.
    fn selection_key(&mut self, key: Key) -> bool {
        let Some(edit) = self.props.edit.as_mut() else {
            return false;
        };
        let Some((a, b)) = edit.selection() else {
            edit.anchor = None;
            return false;
        };
        let insert = edit.insert;
        let plain = !key.ctrl && !key.alt && !key.sup && edit.pending.is_empty();
        let remove = match key.code {
            KeyCode::Backspace | KeyCode::Delete if insert => Some(false),
            KeyCode::Char('d' | 'x') if plain && !insert => Some(false),
            KeyCode::Char('c') if plain && !insert => Some(true),
            KeyCode::Char(_) | KeyCode::Enter if plain && insert => {
                // Typing over it: the key goes on after it is removed.
                edit.remove(a, b);
                edit.caret = a;
                edit.anchor = None;
                self.apply_live();
                return false;
            }
            _ => None,
        };
        let esc = key.code == KeyCode::Esc && !insert;
        edit.anchor = None;
        match remove {
            Some(then_insert) => {
                edit.remove(a, b);
                edit.caret = a;
                if then_insert {
                    edit.insert = true;
                    self.ed.group = Some(false);
                } else if !insert {
                    // Each removal in normal mode is a step of its own.
                    self.ed.group = None;
                }
                edit.rest();
                self.apply_live();
                true
            }
            None => esc,
        }
    }

    /// The wheel: scrolls the rows by `by` (positive: down) without moving the selection; in the
    /// lyrics tab the text.
    pub fn scroll_properties(&self, by: isize) {
        if self.props.tab == PropTab::Lyrics {
            let now = self.props.text_scroll.get() as isize;
            self.props.text_scroll.set((now + by).max(0) as usize);
            return;
        }
        let n = self.property_rows().len() as isize;
        let first = (self.props.scroll.get() as isize + by).clamp(0, (n - 1).max(0));
        self.props.scroll.set(first as usize);
        self.props.seen.set(self.property_selected());
    }

    pub fn set_property_tab(&mut self, tab: PropTab) {
        if self.props.tab != tab {
            self.props.tab = tab;
            self.props.selected = self
                .property_rows()
                .iter()
                .position(|r| r.kind != RowKind::Note)
                .unwrap_or(0);
            self.props.scroll.set(0);
            self.end_edit(false);
        }
    }

    /// Selects a row (a click); a section header folds or unfolds.
    pub fn select_property(&mut self, row: usize) {
        self.end_edit(false);
        let rows = self.property_rows();
        if row >= rows.len() {
            return;
        }
        if rows[row].kind == RowKind::Note {
            return;
        }
        self.props.selected = row;
        if matches!(rows[row].kind, RowKind::Header { .. }) {
            self.fold_property(None);
        }
    }

    /// Folds (`Some(true)`), unfolds (`Some(false)`) or toggles the selected row's section.
    fn fold_property(&mut self, fold: Option<bool>) {
        let rows = self.property_rows();
        let sel = self.property_selected();
        let section = rows[..=sel.min(rows.len().saturating_sub(1))]
            .iter()
            .rev()
            .find_map(|r| match r.kind {
                RowKind::Header { section, .. } => Some(section),
                _ => None,
            });
        if let Some(s) = section {
            let folded = self.props.folded.contains(&s);
            match fold.unwrap_or(!folded) {
                true => self.props.folded.insert(s),
                false => self.props.folded.remove(&s),
            };
            // The selection stays on the header.
            let rows = self.property_rows();
            self.props.selected = rows
                .iter()
                .position(|r| matches!(r.kind, RowKind::Header { section, .. } if section == s))
                .unwrap_or(0);
        }
    }

    fn selected_field(&self) -> Option<(Field, bool)> {
        let rows = self.property_rows();
        match rows.get(self.property_selected())?.kind {
            RowKind::Field { field, editable } => Some((field, editable)),
            _ => None,
        }
    }

    /// Starts editing the selected property: text is typed in place; toggles flip; the
    /// instrument opens its picker.
    pub fn edit_property(&mut self) {
        let rows = self.property_rows();
        let Some(row) = rows.get(self.property_selected()) else {
            return;
        };
        let (field, editable) = match row.kind {
            RowKind::Header { .. } => return self.fold_property(None),
            RowKind::Note => return,
            RowKind::Field { field, editable } => (field, editable),
        };
        let t = self.pane().cursor.track;
        match field {
            _ if !editable => self.info(format!("{}: can't be edited here", row.label)),
            Field::Mute => self.run(Command::Mute(Some(Strip::Track(t)))),
            Field::Solo => self.run(Command::Solo(Some(Strip::Track(t)))),
            Field::Program => self.run(Command::TrackInstrument(None)),
            Field::TrackColor => self.open_picker(PickerKind::Color(t)),
            Field::StaffAt(i) => {
                // The staff chosen is the cursor's, which the picker's commands work on.
                self.pane_mut().cursor.staff = i;
                if self.staff_box_at(i) == 1 {
                    self.open_picker(PickerKind::Clef(t, i));
                } else {
                    self.open_picker(PickerKind::Staff(t, i));
                }
            }
            Field::Tuning => self.open_picker(PickerKind::Tuning(t)),
            Field::ChantClef => self.open_picker(PickerKind::Clef(t, 0)),
            _ => {
                self.props.edit = Some(PropEdit::new(field, row.value.clone()));
                // The edit is one undo step, made as it is typed.
                self.ed.group = Some(false);
            }
        }
    }

    /// Ends the edit. Cancelling takes back what it changed.
    pub(crate) fn end_edit(&mut self, cancel: bool) {
        let Some(e) = &self.props.edit else {
            return;
        };
        if e.field == Field::Lyrics {
            self.props.text_caret = Some((self.pane().cursor.track, e.caret));
        }
        let edited = self.ed.group == Some(true);
        self.ed.group = None;
        self.props.edit = None;
        if cancel && edited {
            self.run(Command::Undo);
        }
    }

    /// Applies the typed text as it is (fields that can't take half-typed text wait for Enter).
    fn apply_live(&mut self) {
        let Some(PropEdit { field, text, .. }) = self.props.edit.clone() else {
            return;
        };
        match field {
            Field::Capo => {}
            // Spaces typed at the end count in the lyrics.
            Field::Lyrics => {
                self.commit_property(field, &text, true);
            }
            _ => {
                self.commit_property(field, text.trim(), true);
            }
        }
    }

    /// Keys while a property is typed. A single line is in insert mode: `<CR>` keeps the text,
    /// `<Esc>` and `<C-c>` cancel. The lyrics are edited like vim.
    pub(crate) fn props_edit_key(&mut self, key: Key) {
        if self.text_global_key(key) {
            return;
        }
        if self.props.edit.as_ref().is_some_and(|e| e.visual.is_some()) {
            return self.text_visual_key(key);
        }
        if self.clipboard_key(key) || self.selection_key(key) {
            return;
        }
        let lines = self.props.visual.borrow().clone();
        let Some(edit) = self.props.edit.as_mut() else {
            return;
        };
        if edit.modal && !edit.insert {
            return self.text_normal_key(key);
        }
        let multiline = edit.modal;
        let before = (edit.text.clone(), edit.field);
        match key.code {
            KeyCode::Esc if multiline => {
                // Back to normal mode: the insert session's changes are one undo step.
                edit.insert = false;
                edit.caret = edit
                    .caret
                    .saturating_sub(usize::from(edit.caret > edit.line_start()));
                self.ed.group = None;
                return;
            }
            KeyCode::Esc => return self.end_edit(true),
            KeyCode::Char('c') if key.ctrl => return self.end_edit(true),
            KeyCode::Enter if !multiline => {
                let PropEdit { field, text, .. } = edit.clone();
                let ok = self.commit_property(field, text.trim(), false);
                return self.end_edit(!ok);
            }
            KeyCode::Enter => edit.insert_char('\n'),
            // Ctrl: by words.
            KeyCode::Backspace if key.ctrl => {
                let from = textobj::word_start_before(&edit.chars(), edit.caret);
                edit.remove(from, edit.caret);
                edit.caret = from;
            }
            KeyCode::Delete if key.ctrl => {
                let to = textobj::word_end_after(&edit.chars(), edit.caret);
                edit.remove(edit.caret, to);
            }
            KeyCode::Left if key.ctrl => edit.word_back(false),
            KeyCode::Right if key.ctrl => edit.word_forward(false),
            KeyCode::Backspace => edit.backspace(),
            KeyCode::Delete => edit.delete(),
            KeyCode::Left => edit.caret = edit.caret.saturating_sub(1),
            KeyCode::Right => edit.caret = (edit.caret + 1).min(edit.len()),
            KeyCode::Up if multiline => edit.move_visual(-1, &lines),
            KeyCode::Down if multiline => edit.move_visual(1, &lines),
            KeyCode::Home => edit.caret = edit.line_start(),
            KeyCode::End => edit.caret = edit.line_end(),
            KeyCode::Char('u') if key.ctrl => *edit = PropEdit::new(edit.field, String::new()),
            _ => {
                if let Some(c) = key.as_char() {
                    edit.insert_char(c);
                }
            }
        }
        if edit.text != before.0 {
            self.apply_live();
        }
    }

    /// Keys the text area has no use for that the keymap binds go on working: `:` in normal and
    /// Visual mode (command line), and Ctrl and Alt keys (`<C-s>` save, `<C-h>` focus …).
    /// A command that moves the focus leaves the text area first. True when it did something.
    fn text_global_key(&mut self, key: Key) -> bool {
        let Some(e) = self.props.edit.as_ref().filter(|e| e.modal) else {
            return false;
        };
        let colon = !e.insert
            && e.pending.is_empty()
            && !key.ctrl
            && !key.alt
            && !key.sup
            && key.code == KeyCode::Char(':');
        let own = key.ctrl
            && !key.alt
            && !key.shift
            && matches!(key.code, KeyCode::Char(c) if "acxvurk".contains(c));
        let chord =
            (key.ctrl || key.alt || key.sup) && matches!(key.code, KeyCode::Char(_)) && !own;
        if !colon && !chord {
            return false;
        }
        let bound = |mode: Mode| match self.config.keymap(mode).lookup(&[key]) {
            crate::config::Lookup::Exact(cmds) => Some(cmds.to_vec()),
            _ => None,
        };
        let Some(cmds) = bound(Mode::Normal).or_else(|| bound(Mode::Properties)) else {
            return false;
        };
        if cmds.is_empty() {
            return false;
        }
        let focus = |c: &Command| {
            matches!(
                c,
                Command::Focus(_) | Command::FocusCycle(_) | Command::FocusLast
            )
        };
        if cmds.iter().any(focus) {
            self.end_edit(false);
        }
        for c in cmds {
            self.run(c);
        }
        true
    }

    /// Keys in the lyrics text area's normal mode: a vim subset, with operators (`d`, `c`, `y`)
    /// over motions and text objects. Each change is its own undo step, and `<Esc>` leaves the
    /// text area.
    fn text_normal_key(&mut self, key: Key) {
        let lines = self.props.visual.borrow().clone();
        let Some(edit) = self.props.edit.as_mut() else {
            return;
        };
        let plain = !key.ctrl && !key.alt && !key.sup;
        let typed = match key.code {
            // Escape forgets a command begun, else leaves the text area and the pane.
            KeyCode::Esc if !edit.pending.is_empty() => {
                edit.pending.clear();
                return;
            }
            KeyCode::Esc => {
                self.end_edit(false);
                if self.focused_kind() == PaneKind::Properties {
                    self.set_focus(self.focused());
                }
                return;
            }
            KeyCode::Char('r') if key.ctrl => return self.text_history(Command::Redo),
            // The pane's tab keys (`H` `L`) work as in the rest of
            // the pane; the text area is entered again afterwards where it is still selected.
            KeyCode::Char('H' | 'L') if plain && edit.pending.is_empty() => {
                self.end_edit(false);
                self.properties_key(key);
                self.enter_text_area();
                return;
            }
            KeyCode::Char(c) if plain => {
                let mut keys: Vec<char> = edit.pending.chars().collect();
                keys.push(c);
                let t = parse_typed(&keys, false);
                edit.pending = if t == Typed::More {
                    keys.into_iter().collect()
                } else {
                    String::new()
                };
                t
            }
            KeyCode::Enter => Typed::Cmd('i'),
            KeyCode::Left if key.ctrl => Typed::Move(Motion::Back(false)),
            KeyCode::Right if key.ctrl => Typed::Move(Motion::Word(false)),
            KeyCode::Left => Typed::Move(Motion::Left),
            KeyCode::Right => Typed::Move(Motion::Right),
            KeyCode::Up => Typed::Move(Motion::Up),
            KeyCode::Down => Typed::Move(Motion::Down),
            KeyCode::Home => Typed::Move(Motion::LineStart),
            KeyCode::End => Typed::Move(Motion::LineEnd),
            KeyCode::Delete => Typed::Cmd('x'),
            _ => Typed::Bad,
        };
        edit.pending.retain(|_| typed == Typed::More);
        let before = edit.text.clone();
        let mut insert = false;
        match typed {
            Typed::More | Typed::Bad | Typed::Object(..) => return,
            Typed::Move(m) => edit.caret = edit.target(m, &lines),
            Typed::Op(op, target) => {
                let range = match target {
                    Target::Line => {
                        let c = edit.chars();
                        let (a, b) = textobj::line_range(&c, edit.caret, edit.caret);
                        Some((a, b, true))
                    }
                    Target::Move(m) => edit.motion_range(op, m),
                    Target::Object(around, o) => edit.object_range(around, o),
                };
                if let Some(r) = range {
                    self.text_operate(op, r);
                }
                return;
            }
            Typed::Replace(c) => {
                if edit.caret < edit.line_end() {
                    edit.delete();
                    edit.insert_char(c);
                    edit.caret -= 1;
                }
            }
            Typed::Cmd(c) => {
                // Changes made by an operator over a motion.
                let op = match c {
                    'x' => Some(('d', Motion::Right)),
                    'X' => Some(('d', Motion::Left)),
                    'D' => Some(('d', Motion::LineEnd)),
                    'C' => Some(('c', Motion::LineEnd)),
                    's' => Some(('c', Motion::Right)),
                    _ => None,
                };
                if let Some((op, m)) = op {
                    match edit.motion_range(op, m) {
                        Some(r) => self.text_operate(op, r),
                        // `s` on an empty line just inserts.
                        None if op == 'c' => {
                            edit.insert = true;
                            self.ed.group = Some(false);
                        }
                        None => {}
                    }
                    return;
                }
                if matches!(c, 'S' | 'Y') {
                    let ch = edit.chars();
                    let (a, b) = textobj::line_range(&ch, edit.caret, edit.caret);
                    return self.text_operate(if c == 'S' { 'c' } else { 'y' }, (a, b, true));
                }
                match c {
                    'u' => return self.text_history(Command::Undo),
                    '/' => return self.start_search(),
                    'n' | 'N' => return self.search_step(if c == 'n' { 1 } else { -1 }, None),
                    'p' | 'P' => return self.paste(c == 'p'),
                    'v' | 'V' => {
                        edit.anchor = None;
                        edit.visual = Some((edit.caret, c == 'V'));
                        return;
                    }
                    '~' => {
                        if let Some(&ch) = edit.chars().get(edit.caret).filter(|c| **c != '\n') {
                            let flipped: String = if ch.is_uppercase() {
                                ch.to_lowercase().collect()
                            } else {
                                ch.to_uppercase().collect()
                            };
                            edit.remove(edit.caret, edit.caret + 1);
                            edit.insert_str(&flipped);
                            edit.caret = edit.caret.min(edit.line_end());
                        }
                    }
                    'i' => insert = true,
                    'a' => {
                        edit.caret = (edit.caret + 1).min(edit.line_end());
                        insert = true;
                    }
                    'I' => {
                        edit.caret = edit.first_non_blank();
                        insert = true;
                    }
                    'A' => {
                        edit.caret = edit.line_end();
                        insert = true;
                    }
                    'o' => {
                        edit.caret = edit.line_end();
                        edit.insert_char('\n');
                        insert = true;
                    }
                    'O' => {
                        edit.caret = edit.line_start();
                        edit.insert_char('\n');
                        edit.caret -= 1;
                        insert = true;
                    }
                    _ => return,
                }
            }
        }
        edit.caret = edit.caret.min(edit.len());
        let changed = edit.text != before;
        if insert {
            edit.insert = true;
            // The insert session is one undo step (and so is the `o`/`O` that began it).
            self.ed.group = Some(false);
        } else {
            edit.rest();
        }
        if changed {
            // A change in normal mode is a step of its own.
            self.apply_live();
        }
    }

    /// Keys in the lyrics text area's visual mode (`v`, or `V` for whole lines): motions and
    /// text objects (`iw`, `ap`, `i(` …) grow the selection, `o` goes to its other end, and
    /// `d` `x` `c` `s` `y` `p` `~` `u` `U` act on it (`D` `X` `C` `S` `Y` on its lines).
    fn text_visual_key(&mut self, key: Key) {
        let lines = self.props.visual.borrow().clone();
        let Some(edit) = self.props.edit.as_mut() else {
            return;
        };
        let Some((start, linewise)) = edit.visual else {
            return;
        };
        let plain = !key.ctrl && !key.alt && !key.sup;
        let typed = match key.code {
            KeyCode::Esc if !edit.pending.is_empty() => {
                edit.pending.clear();
                return;
            }
            KeyCode::Esc => Typed::Cmd('\u{1b}'),
            KeyCode::Char(c) if plain => {
                let mut keys: Vec<char> = edit.pending.chars().collect();
                keys.push(c);
                let t = parse_typed(&keys, true);
                edit.pending = if t == Typed::More {
                    keys.into_iter().collect()
                } else {
                    String::new()
                };
                t
            }
            KeyCode::Char('c') if key.ctrl => Typed::Cmd('\u{3}'),
            KeyCode::Char('x') if key.ctrl => Typed::Cmd('\u{18}'),
            KeyCode::Char('v') if key.ctrl => Typed::Cmd('p'),
            KeyCode::Char('a') if key.ctrl => {
                edit.select_all();
                return;
            }
            KeyCode::Left if key.ctrl => Typed::Move(Motion::Back(false)),
            KeyCode::Right if key.ctrl => Typed::Move(Motion::Word(false)),
            KeyCode::Left => Typed::Move(Motion::Left),
            KeyCode::Right => Typed::Move(Motion::Right),
            KeyCode::Up => Typed::Move(Motion::Up),
            KeyCode::Down => Typed::Move(Motion::Down),
            KeyCode::Home => Typed::Move(Motion::LineStart),
            KeyCode::End => Typed::Move(Motion::LineEnd),
            KeyCode::Delete | KeyCode::Backspace => Typed::Cmd('d'),
            _ => Typed::Bad,
        };
        let sel = edit.selection();
        // The selection's lines, for the commands on whole lines.
        let line_sel = || {
            let c = edit.chars();
            let (lo, hi) = (start.min(edit.caret), start.max(edit.caret));
            let (a, b) = textobj::line_range(&c, lo, hi);
            (a, b, true)
        };
        let range = sel.map(|(a, b)| (a, b, linewise));
        match typed {
            Typed::More | Typed::Bad | Typed::Op(..) | Typed::Replace(_) => {}
            Typed::Move(m) => {
                edit.caret = edit.target(m, &lines);
                edit.rest();
            }
            Typed::Object(around, o) => {
                if let Some((a, b, lines)) = edit.object_range(around, o).filter(|r| r.0 < r.1) {
                    edit.visual = Some((a, linewise || lines));
                    edit.caret = b - 1;
                }
            }
            Typed::Cmd(c) => match c {
                '\u{1b}' => edit.visual = None,
                'v' | 'V' => {
                    let lines = c == 'V';
                    edit.visual = (lines != linewise).then_some((start, lines));
                }
                'o' => {
                    edit.visual = Some((edit.caret, linewise));
                    edit.caret = start;
                }
                'd' | 'x' | 'c' | 's' | 'y' | '\u{3}' | '\u{18}' => {
                    edit.visual = None;
                    let op = match c {
                        'x' | '\u{18}' => 'd',
                        '\u{3}' => 'y',
                        's' => 'c',
                        _ => c,
                    };
                    if let Some(r) = range {
                        self.text_operate(op, r);
                    }
                }
                'D' | 'X' | 'C' | 'S' | 'R' | 'Y' => {
                    let r = line_sel();
                    edit.visual = None;
                    let op = match c {
                        'D' | 'X' => 'd',
                        'Y' => 'y',
                        _ => 'c',
                    };
                    self.text_operate(op, r);
                }
                'p' | 'P' => self.paste(true),
                '~' | 'u' | 'U' => {
                    if let Some((a, b)) = sel {
                        let text: String = edit.chars()[a..b]
                            .iter()
                            .map(|&ch| match c {
                                'u' => ch.to_lowercase().collect::<String>(),
                                'U' => ch.to_uppercase().collect(),
                                _ if ch.is_uppercase() => ch.to_lowercase().collect(),
                                _ => ch.to_uppercase().collect(),
                            })
                            .collect();
                        edit.remove(a, b);
                        edit.caret = a;
                        edit.insert_str(&text);
                        edit.caret = a;
                        edit.visual = None;
                        edit.rest();
                        self.ed.group = None;
                        self.apply_live();
                    }
                }
                _ => {}
            },
        }
    }

    /// An operator over a range (from, to, whole lines): `d` deletes, `c` deletes and inserts,
    /// `y` copies; what `d` and `c` take is copied too, as in vim.
    fn text_operate(&mut self, op: char, (mut a, mut b, lines): (usize, usize, bool)) {
        let Some(e) = self.props.edit.as_mut() else {
            return;
        };
        let c = e.chars();
        let (a0, b0) = (a.min(c.len()), b.min(c.len()));
        (a, b) = (a0, b0);
        let mut taken: String = c[a..b].iter().collect();
        if lines && !taken.ends_with('\n') {
            taken.push('\n');
        }
        if !taken.is_empty() {
            crate::clipboard::set(&taken);
        }
        e.anchor = None;
        e.visual = None;
        match op {
            'y' => {
                if !lines || textobj::line_bounds(&c, e.caret).0 != a {
                    e.caret = a;
                }
                e.rest();
            }
            'd' => {
                // The last lines take the line break before them.
                if lines && b == c.len() && a > 0 && c[a - 1] == '\n' && !c[a..b].ends_with(&['\n'])
                {
                    a -= 1;
                }
                e.remove(a, b);
                e.caret = a.min(e.len());
                if lines {
                    e.caret = e.first_non_blank();
                }
                e.rest();
                self.ed.group = None;
                self.apply_live();
            }
            _ => {
                // Changing lines keeps one, empty, to type into.
                if lines && b > a && c[b - 1] == '\n' {
                    b -= 1;
                }
                e.remove(a, b);
                e.caret = a;
                e.insert = true;
                self.ed.group = Some(false);
                self.apply_live();
            }
        }
    }

    /// Undo or redo from the text area: the text is read back from the score.
    fn text_history(&mut self, cmd: Command) {
        self.ed.group = None;
        self.run(cmd);
        let t = self.pane().cursor.track;
        let text = Some(self.shown_lyrics(t));
        if let (Some(text), Some(edit)) = (text, self.props.edit.as_mut()) {
            edit.text = text;
            edit.full = None;
            edit.visual = None;
            edit.caret = edit.caret.min(edit.len());
            edit.rest();
        }
    }

    /// Applies typed text to a property; false when it isn't a valid value. While typing
    /// (`live`) half-typed values are passed over quietly.
    pub(crate) fn commit_property(&mut self, field: Field, text: &str, live: bool) -> bool {
        let t = self.pane().cursor.track;
        let valid = std::cell::Cell::new(true);
        // A whole number within bounds, or an error naming the property.
        let number = |app: &mut App, what: &str, min: u32, max: u32| -> Option<u32> {
            match text.parse::<u32>() {
                Ok(n) if (min..=max).contains(&n) => Some(n),
                _ => {
                    valid.set(false);
                    if !live {
                        app.error(format!("{what}: expected a number from {min} to {max}"));
                    }
                    None
                }
            }
        };
        match field {
            Field::Title
            | Field::Subtitle
            | Field::Artist
            | Field::Album
            | Field::Words
            | Field::Music
            | Field::Copyright
            | Field::TabAuthor
            | Field::Instructions => {
                let text = text.to_string();
                self.edit(|cx| {
                    let m = &mut cx.whole().meta;
                    *match field {
                        Field::Title => &mut m.title,
                        Field::Subtitle => &mut m.subtitle,
                        Field::Artist => &mut m.artist,
                        Field::Album => &mut m.album,
                        Field::Words => &mut m.words,
                        Field::Music => &mut m.music,
                        Field::Copyright => &mut m.copyright,
                        Field::TabAuthor => &mut m.tab_author,
                        _ => &mut m.instructions,
                    } = text;
                    Ok(())
                });
            }
            Field::PageWidth
            | Field::PageHeight
            | Field::MarginLeft
            | Field::MarginRight
            | Field::MarginTop
            | Field::MarginBottom
            | Field::PageScale => {
                let (what, min, max) = match field {
                    Field::PageWidth | Field::PageHeight => ("page size", 50, 2000),
                    Field::PageScale => ("scale", 10, 500),
                    _ => ("margin", 0, 200),
                };
                if let Some(n) = number(self, what, min, max) {
                    self.edit(|cx| {
                        let p = &mut cx.whole().page;
                        match field {
                            Field::PageWidth => p.width = n,
                            Field::PageHeight => p.height = n,
                            Field::MarginLeft => p.margin_left = n,
                            Field::MarginRight => p.margin_right = n,
                            Field::MarginTop => p.margin_top = n,
                            Field::MarginBottom => p.margin_bottom = n,
                            _ => p.scale = n,
                        }
                        Ok(())
                    });
                }
            }
            Field::ChantHeader(i) => {
                let key = self
                    .chant_of_cursor()
                    .and_then(|c| chant_header_keys(c).get(i).cloned());
                if let Some(key) = key {
                    let value = text.to_string();
                    self.edit(|cx| {
                        let chant = cx
                            .whole()
                            .tracks
                            .get_mut(t)
                            .and_then(|k| k.chant.as_deref_mut())
                            .ok_or("not in a modern score")?;
                        match chant.headers.iter().position(|(k, _)| *k == key) {
                            Some(at) if value.is_empty() => {
                                chant.headers.remove(at);
                            }
                            Some(at) => chant.headers[at].1 = value,
                            None if value.is_empty() => {}
                            None => chant.headers.push((key, value)),
                        }
                        Ok(())
                    });
                }
            }
            Field::ChantInterpretation => {
                let word = text.trim().to_lowercase();
                if ["equalist", "solesmes"].contains(&word.as_str()) {
                    self.run(Command::Interpretation(Some(word)));
                } else {
                    valid.set(false);
                    if !live {
                        self.error("interpretation: expected equalist or solesmes");
                    }
                }
            }
            Field::ChantStaffLines => {
                if let Some(n) = number(self, "staff lines", 2, 5) {
                    self.edit(|cx| {
                        let chant = cx
                            .whole()
                            .tracks
                            .get_mut(t)
                            .and_then(|k| k.chant.as_deref_mut())
                            .ok_or("not in a modern score")?;
                        chant.staff_lines = n as u8;
                        Ok(())
                    });
                }
            }
            Field::ChantClef => {
                if live {
                    valid.set(false);
                } else {
                    self.run(Command::ChantClef(text.trim().to_string()));
                }
            }
            Field::TrackName if text.is_empty() => valid.set(false),
            Field::TrackName => self.run(Command::TrackRename(format!("{text} {}", t + 1))),
            Field::Capo => {
                if let Some(n) = number(self, "capo", 0, 36) {
                    self.run(Command::Capo(n as u8));
                }
            }
            Field::FretCount => {
                if let Some(n) = number(self, "frets", 1, 36) {
                    self.edit(|cx| {
                        let track = cx.whole().tracks.get_mut(t).ok_or("no such track")?;
                        track.fret_count = n as u8;
                        Ok(())
                    });
                }
            }
            Field::Channel => {
                if let Some(n) = number(self, "channel", 1, 16) {
                    self.edit(|cx| {
                        let track = cx.whole().tracks.get_mut(t).ok_or("no such track")?;
                        track.midi.channel = n as u8 - 1;
                        Ok(())
                    });
                }
            }
            Field::Bank | Field::Pan => {
                if let Some(n) = number(self, "value", 0, 127) {
                    self.edit(|cx| {
                        let track = cx.whole().tracks.get_mut(t).ok_or("no such track")?;
                        if field == Field::Bank {
                            track.midi.bank = n as u8;
                        } else {
                            track.midi.pan = n as u8;
                        }
                        Ok(())
                    });
                }
            }
            Field::Volume => {
                if let Some(n) = number(self, "volume", 0, 127) {
                    self.set_track_volume(t, n as u8);
                }
            }
            Field::Lyrics => self.set_lyrics(t, text, live),
            Field::TrackKind
            | Field::StaffAt(_)
            | Field::TrackColor
            | Field::Tuning
            | Field::Mute
            | Field::Solo
            | Field::Program => {}
        }
        valid.get()
    }

    /// Replaces the lyrics of a track with the text area's `shown` text. The syllables are laid
    /// onto the notes again; a chant track's chant is read from it. One undo step per edit
    /// session.
    fn set_lyrics(&mut self, track: usize, shown: &str, live: bool) {
        let typed = self
            .props
            .edit
            .as_ref()
            .filter(|e| e.field == Field::Lyrics)
            .and_then(|e| e.full.clone());
        let old = typed.unwrap_or_else(|| full_lyrics(&self.buffer().score, track));
        let chant = self
            .buffer()
            .score
            .tracks
            .get(track)
            .and_then(|t| t.chant.as_deref())
            .cloned();
        let full = shown.to_string();
        if let Some(chant) = chant {
            // The chant's header stays; the body is read again.
            let mut gabc = String::new();
            for (k, v) in &chant.headers {
                gabc.push_str(&format!("{k}: {v};\n"));
            }
            gabc.push_str("%%\n");
            gabc.push_str(&full);
            match cecilia_formats::gabc::parse(&gabc) {
                Ok(mut new) => {
                    new.interpretation = chant.interpretation;
                    if new != chant {
                        self.edit(|cx| {
                            let score = cx.whole();
                            score.tracks[track].chant = Some(Box::new(new));
                            cecilia_core::chant::retranscribe(score, track);
                            Ok(())
                        });
                    }
                }
                Err(e) if !live => self.error(format!("lyrics: {e}")),
                Err(_) => {}
            }
            // The text area keeps the text as typed.
            if let Some(e) = self
                .props
                .edit
                .as_mut()
                .filter(|e| e.field == Field::Lyrics)
            {
                e.full = Some(full.clone());
                if e.text != full {
                    e.text = full;
                    e.caret = e.caret.min(e.len());
                    e.anchor = None;
                }
            }
            return;
        } else if old != full {
            let placed = self.edit(|cx| {
                let score = cx.whole();
                let t = score.tracks.get_mut(track).ok_or("no such track")?;
                t.lyrics_text = full;
                let lay = cecilia_core::gabcplus::apply(score, track);
                crate::editor::refresh_lyric_lines(score, track);
                Ok(lay.unplaced)
            });
            if let Some(n) = placed.filter(|&n| n > 0) {
                self.info(format!(
                    "lyrics: {n} syllables past the last note are kept, not shown"
                ));
            }
        }
        // The text area shows the text as it now reads.
        let now = self.shown_lyrics(track);
        if let Some(e) = self
            .props
            .edit
            .as_mut()
            .filter(|e| e.field == Field::Lyrics)
        {
            if e.text != now {
                e.text = now;
                e.caret = e.caret.min(e.len());
                e.anchor = None;
            }
        }
    }

    /// With the lyrics text area selected, its text is edited at once, in normal mode (the
    /// block caret shows), the caret where it was when the text area was last left.
    pub(crate) fn enter_text_area(&mut self) {
        if self.props.edit.is_some()
            || !matches!(self.selected_field(), Some((Field::Lyrics, true)))
        {
            return;
        }
        let t = self.pane().cursor.track;
        let mut e = PropEdit::new(Field::Lyrics, self.shown_lyrics(t));
        e.insert = false;
        // The caret is where it was left on this track.
        e.caret = match self.props.text_caret {
            Some((track, caret)) if track == t => caret.min(e.len()),
            _ => 0,
        };
        e.rest();
        self.props.edit = Some(e);
        self.ed.group = None;
    }

    /// `<A-h>` / `<A-l>`: a switch goes off / on; a choice moves to the previous / next one; a number
    /// goes down / up by one.
    fn step_property(&mut self, by: isize) {
        let t = self.pane().cursor.track;
        let Some((field, _)) = self.selected_field() else {
            return;
        };
        let on = by > 0;
        let track = self.buffer().score.tracks.get(t);
        match field {
            Field::Mute | Field::Solo => {
                let now = track.is_some_and(|k| if field == Field::Mute { k.mute } else { k.solo });
                if now != on {
                    self.edit_property();
                }
            }
            Field::TrackColor => self.step_choice(PickerKind::Color(t), by),
            Field::Tuning => self.step_choice(PickerKind::Tuning(t), by),
            Field::ChantClef => self.step_choice(PickerKind::Clef(t, 0), by),
            Field::ChantInterpretation => {
                self.commit_property(field, if by > 0 { "solesmes" } else { "equalist" }, true);
            }
            Field::Program => self.step_choice(PickerKind::Instrument(t), by),
            Field::StaffAt(i) => {
                let n = track.map_or(0, |k| k.staves().len());
                if i < n {
                    self.pane_mut().cursor.staff = i;
                    if self.staff_box_at(i) == 1 {
                        self.step_choice(PickerKind::Clef(t, i), by);
                    } else {
                        self.step_choice(PickerKind::Staff(t, i), by);
                    }
                } else {
                    // The empty field adds a staff: `l` the first kind, `h` the last.
                    self.pick_choice(PickerKind::Staff(t, n), by > 0);
                }
            }
            Field::PageWidth
            | Field::PageHeight
            | Field::MarginLeft
            | Field::MarginRight
            | Field::MarginTop
            | Field::MarginBottom
            | Field::PageScale
            | Field::FretCount
            | Field::ChantStaffLines
            | Field::Capo
            | Field::Channel
            | Field::Bank
            | Field::Volume
            | Field::Pan => {
                let now = self
                    .property_rows()
                    .get(self.property_selected())
                    .and_then(|r| r.value.parse::<i64>().ok());
                if let Some(n) = now {
                    // Out of range steps are refused quietly.
                    self.commit_property(field, &(n + by.signum() as i64).to_string(), true);
                }
            }
            _ => {}
        }
    }

    /// Whether `hay` has `query`: in any case, unless the query has a capital.
    pub(crate) fn has_text(hay: &str, query: &str) -> bool {
        if query.chars().any(char::is_uppercase) {
            hay.contains(query)
        } else {
            hay.to_lowercase().contains(query)
        }
    }

    /// Opens the search field: in the lyrics text area it looks through the text.
    pub(crate) fn start_search(&mut self) {
        let edit = self.props.edit.as_ref().filter(|e| e.modal);
        self.props.search = Some(PropSearch {
            query: String::new(),
            origin_row: self.props.selected,
            origin_caret: edit.map_or(0, |e| e.caret),
            in_text: edit.is_some(),
        });
    }

    /// Jumps to the match of `query` that is `dir` (1, -1) from `from`, wrapping around: a row
    /// of the tab by its key or value, or a place in the lyrics text. `from` is the row or
    /// character the search starts at, itself included when `dir` is 0.
    fn search_jump(&mut self, query: &str, from: usize, dir: isize, in_text: bool) -> bool {
        if query.is_empty() {
            return false;
        }
        if in_text {
            let Some(edit) = self.props.edit.as_mut() else {
                return false;
            };
            let chars: Vec<char> = edit.text.chars().collect();
            let n = chars.len();
            let q: Vec<char> = query.chars().collect();
            let fold = !query.chars().any(char::is_uppercase);
            let same = |a: char, b: char| {
                if fold {
                    a.to_lowercase().eq(b.to_lowercase())
                } else {
                    a == b
                }
            };
            let at = |i: usize| {
                i + q.len() <= n && q.iter().enumerate().all(|(k, &c)| same(chars[i + k], c))
            };
            let m = n.max(1);
            for step in 0..m {
                let i = if dir < 0 {
                    (from + m - step) % m
                } else {
                    (from + step) % m
                };
                if at(i) {
                    edit.caret = i;
                    edit.rest();
                    return true;
                }
            }
            return false;
        }
        let rows = self.property_rows();
        let n = rows.len();
        for step in 0..n {
            let i = match dir {
                -1 => (from + n - step) % n,
                _ => (from + step) % n,
            };
            let row = &rows[i];
            if row.kind == RowKind::Note {
                continue;
            }
            let hay = format!("{}  {}  {}", row.label, row.value, row.aside);
            if Self::has_text(&hay, query) {
                self.props.selected = i;
                return true;
            }
        }
        false
    }

    /// Keys while the search field is open: they type the query, which jumps as it is typed;
    /// Enter keeps the jump and sets the `/` register, Escape goes back.
    pub(crate) fn props_search_key(&mut self, key: Key) {
        let Some(mut search) = self.props.search.take() else {
            return;
        };
        let mut done = false;
        match key.code {
            KeyCode::Esc => {
                self.props.selected = search.origin_row;
                if let Some(e) = self.props.edit.as_mut().filter(|_| search.in_text) {
                    e.caret = search.origin_caret;
                }
                done = true;
            }
            KeyCode::Enter => {
                if !search.query.is_empty() {
                    self.props.last_search = search.query.clone();
                }
                done = true;
            }
            KeyCode::Backspace if search.query.is_empty() => {
                self.props.selected = search.origin_row;
                done = true;
            }
            KeyCode::Backspace => {
                search.query.pop();
            }
            KeyCode::Char('u') if key.ctrl => search.query.clear(),
            _ => {
                if let Some(c) = key.as_char() {
                    search.query.push(c);
                }
            }
        }
        if done {
            return;
        }
        // Each change looks again from where the search began.
        let from = if search.in_text {
            search.origin_caret
        } else {
            search.origin_row
        };
        if !self.search_jump(&search.query, from, 1, search.in_text) {
            self.props.selected = search.origin_row;
            if let Some(e) = self.props.edit.as_mut().filter(|_| search.in_text) {
                e.caret = search.origin_caret;
            }
        }
        self.props.search = Some(search);
    }

    /// `n` / `N`: the next / previous match of the `/` register.
    pub(crate) fn search_step(&mut self, dir: isize, _: Option<()>) {
        let query = self.props.last_search.clone();
        if query.is_empty() {
            return self.info("no previous search");
        }
        let in_text = self.props.edit.as_ref().is_some_and(|e| e.modal);
        let from = match (in_text, dir) {
            (true, d) => {
                let caret = self.props.edit.as_ref().map_or(0, |e| e.caret);
                let n = self.props.edit.as_ref().map_or(0, |e| e.len());
                if d > 0 {
                    (caret + 1) % n.max(1)
                } else {
                    (caret + n.max(1) - 1) % n.max(1)
                }
            }
            (false, d) => {
                let n = self.property_rows().len();
                let sel = self.property_selected();
                if d > 0 {
                    (sel + 1) % n.max(1)
                } else {
                    (sel + n.max(1) - 1) % n.max(1)
                }
            }
        };
        if !self.search_jump(&query, from, dir, in_text) {
            self.error(format!("pattern not found: {query}"));
        }
    }

    /// Moves a staff of the cursor's track to another place; the selection follows it.
    pub fn move_property_staff(&mut self, from: usize, to: usize) {
        let rows = self.property_rows();
        let row_of = |f: Field| {
            rows.iter()
                .position(|r| matches!(r.kind, RowKind::Field { field, .. } if field == f))
        };
        let (Some(a), Some(b)) = (row_of(Field::StaffAt(from)), row_of(Field::StaffAt(to))) else {
            return;
        };
        self.pane_mut().cursor.staff = from;
        self.run(Command::StaffMove(format!("{}", to + 1)));
        self.props.selected = b;
        let _ = a;
    }

    /// The box of staff `i` of the cursor's track the selection is in: 1 (the clef) only while
    /// the staff has a clef, else 0 (the notation).
    fn staff_box_at(&self, i: usize) -> u8 {
        let has_clef = self
            .buffer()
            .score
            .tracks
            .get(self.pane().cursor.track)
            .and_then(|t| t.staves().get(i).copied())
            .is_some_and(|s| crate::icons::Icon::of_clef(s).is_some());
        if has_clef { self.props.staff_box } else { 0 }
    }

    /// Whether box `b` (0 notation, 1 clef) of a staff row is the selection.
    pub fn property_box_selected(&self, row: usize, b: u8) -> bool {
        row == self.property_selected()
            && matches!(self.selected_field(), Some((Field::StaffAt(i), _)) if self.staff_box_at(i) == b)
    }

    /// Selects a box of a staff row (a click on it).
    pub fn set_staff_box(&mut self, b: u8) {
        self.props.staff_box = b.min(1);
    }

    /// Whether a row is a staff that can be dragged.
    pub fn property_is_staff(&self, row: usize) -> bool {
        let n = self
            .buffer()
            .score
            .tracks
            .get(self.pane().cursor.track)
            .map_or(0, |t| t.staves().len());
        matches!(
            self.property_rows().get(row).map(|r| &r.kind),
            Some(RowKind::Field { field: Field::StaffAt(i), .. }) if *i < n
        )
    }

    /// A click on a row: it selects it, and a double click edits it. A switch flips at once.
    pub fn click_property(&mut self, row: usize, double: bool) {
        // In the lyrics text area a double click is `i`, as `<CR>` is.
        if let Some(e) = self.props.edit.as_mut().filter(|e| e.modal) {
            if row == self.props.selected {
                if double && !e.insert {
                    e.insert = true;
                    self.ed.group = Some(false);
                }
                return;
            }
        }
        let toggle = matches!(
            self.property_rows().get(row).map(|r| &r.kind),
            Some(RowKind::Field { field, .. }) if field.is_switch()
        );
        let area = matches!(
            self.property_rows().get(row).map(|r| &r.kind),
            Some(RowKind::Field {
                field: Field::Lyrics,
                ..
            })
        );
        self.select_property(row);
        // One click in the text area gives it the caret, in normal mode.
        if area && !double {
            return self.enter_text_area();
        }
        // (A double click on a switch is two clicks: it would flip it back.)
        if toggle != double {
            self.edit_property();
        }
    }

    /// Whether the row is selected, or is one of the staff rows while one is (they are all the
    /// value of the key "Staves").
    pub fn property_row_selected(&self, row: usize) -> bool {
        let rows = self.property_rows();
        let staff = |r: usize| {
            matches!(
                rows.get(r).map(|r| &r.kind),
                Some(RowKind::Field {
                    field: Field::StaffAt(_),
                    ..
                })
            )
        };
        let sel = self.property_selected();
        row == sel || (staff(row) && staff(sel))
    }

    /// A drag from one row to another: a staff takes the other's place.
    pub fn drag_property(&mut self, from: usize, to: usize) {
        let rows = self.property_rows();
        let at = |r: usize| match rows.get(r).map(|r| &r.kind) {
            Some(RowKind::Field {
                field: Field::StaffAt(i),
                ..
            }) => Some(*i),
            _ => None,
        };
        if let (Some(a), Some(b)) = (at(from), at(to)) {
            if a != b && self.property_is_staff(from) && self.property_is_staff(to) {
                self.move_property_staff(a, b);
            }
        }
    }

    /// Clears the selected verse, or the track's own color.
    fn clear_property(&mut self) {
        match self.selected_field().map(|(f, _)| f) {
            Some(Field::Lyrics) => self.run(Command::LyricsClear(None)),
            Some(Field::TrackColor) => self.run(Command::TrackColor(Some("off".into()))),
            Some(Field::StaffAt(i)) => {
                self.pane_mut().cursor.staff = i;
                self.run(Command::StaffRemove(None));
            }
            // A text property is emptied, as one undo step.
            Some(
                f @ (Field::Title
                | Field::Subtitle
                | Field::Artist
                | Field::Album
                | Field::Words
                | Field::Music
                | Field::Copyright
                | Field::TabAuthor
                | Field::Instructions
                | Field::ChantHeader(_)),
            ) => {
                self.commit_property(f, "", false);
            }
            _ => self.info("nothing to clear here"),
        }
    }

    /// Keys while the properties pane has the focus: its own keymap (`[keymap.properties]`).
    pub(crate) fn properties_key(&mut self, key: Key) {
        self.pending.push(key);
        let cmds = match self.config.keymap(Mode::Properties).lookup(&self.pending) {
            crate::config::Lookup::Exact(cmds) => cmds.to_vec(),
            crate::config::Lookup::Prefix => return,
            // Not a properties key: the keys go on as in any pane (`<C-w>` moves, `:` …).
            crate::config::Lookup::None => {
                for k in std::mem::take(&mut self.pending) {
                    self.normal_key(k);
                }
                return;
            }
        };
        self.pending.clear();
        let count = self.count.take();
        for cmd in cmds {
            self.run_counted(cmd, count);
        }
    }

    pub(crate) fn run_properties(&mut self, cmd: &Command) -> bool {
        use Command::*;
        match cmd {
            Properties(on) => self.set_tool(PaneKind::Properties, *on),
            PropertiesFocus => {
                if self.focused_kind() == PaneKind::Properties {
                    self.end_edit(false);
                    self.set_focus(self.focused());
                } else {
                    self.set_tool(PaneKind::Properties, Some(true));
                    if let Some(id) = self.tool_pane(PaneKind::Properties) {
                        self.set_focus(id);
                        self.pending.clear();
                        self.enter_text_area();
                    }
                }
            }
            PropertiesDown | PropertiesUp => {
                // Notes (no property) are skipped.
                let rows = self.property_rows();
                let mut i = self.property_selected();
                let down = matches!(cmd, PropertiesDown);
                loop {
                    let next = if down { i + 1 } else { i.wrapping_sub(1) };
                    match rows.get(next) {
                        None => break,
                        Some(r) if r.kind == RowKind::Note => i = next,
                        Some(_) => {
                            i = next;
                            break;
                        }
                    }
                }
                self.props.selected = if rows.get(i).is_some_and(|r| r.kind == RowKind::Note) {
                    self.property_selected()
                } else {
                    i
                };
                self.enter_text_area();
            }
            PropertiesFirst | PropertiesLast => {
                // The first / last row that is a property (notes are skipped).
                let rows = self.property_rows();
                let mut it = rows
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.kind != RowKind::Note);
                let pick = if matches!(cmd, PropertiesFirst) {
                    it.next()
                } else {
                    it.next_back()
                };
                if let Some((i, _)) = pick {
                    self.props.selected = i;
                }
                self.enter_text_area();
            }
            PropertiesCenter => self.props.center.set(true),
            PropertiesTab(Some(tab)) => self.set_property_tab(*tab),
            PropertiesTab(None) => self.set_property_tab(self.props.tab.step(1)),
            PropertiesTabPrev => self.set_property_tab(self.props.tab.step(-1)),
            PropertiesEdit => self.edit_property(),
            PropertiesFold(fold) => self.fold_property(*fold),
            PropertiesClear => self.clear_property(),
            PropertiesStep(by) => self.step_property(*by),
            PropertiesSearch(0) => self.start_search(),
            PropertiesSearch(dir) => self.search_step(*dir, None),
            PropertiesChange => {
                self.edit_property();
                // Text is cleared (as the score follows); pickers and toggles just open.
                if let Some(e) = self.props.edit.as_mut() {
                    e.text.clear();
                    e.caret = 0;
                    self.apply_live();
                }
            }
            PropertiesBox(by) => self.props.staff_box = if *by > 0 { 1 } else { 0 },
            PropertiesStaffMove(by) => {
                if let Some((Field::StaffAt(i), _)) = self.selected_field() {
                    let n = self
                        .buffer()
                        .score
                        .tracks
                        .get(self.pane().cursor.track)
                        .map_or(0, |t| t.staves().len());
                    let to = i as isize + by;
                    if i < n && (0..n as isize).contains(&to) {
                        self.move_property_staff(i, to as usize);
                    }
                }
            }
            _ => return false,
        }
        true
    }
}

fn note(text: &str) -> PropRow {
    PropRow {
        kind: RowKind::Note,
        label: text.into(),
        value: String::new(),
        aside: String::new(),
        swatch: None,
        icon: None,
        value2: String::new(),
        icon2: None,
        placeholder: "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{keys, test_app};
    use crate::icons::Icon;
    use crate::layout::{Axis, Node};
    use cecilia_core::{Notation, Staff};

    fn with_props() -> App {
        let mut app = test_app();
        app.run(Command::Properties(Some(true)));
        app
    }

    fn field_row(app: &App, f: Field) -> usize {
        app.property_rows()
            .iter()
            .position(|r| matches!(r.kind, RowKind::Field { field, .. } if field == f))
            .unwrap_or_else(|| panic!("no row for {f:?}"))
    }

    /// Sets the cursor track's lyrics lyrics.
    fn lyrics_text(app: &mut App, text: &str) {
        let b = app.pane().buffer;
        app.buffers[b].score.tracks[0].lyrics_text = text.into();
        cecilia_core::gabcplus::apply(&mut app.buffers[b].score, 0);
    }

    fn type_into(app: &mut App, f: Field, text: &str) {
        app.props.selected = field_row(app, f);
        app.run(Command::PropertiesEdit);
        assert!(app.props.edit.is_some());
        *app.props.edit.as_mut().unwrap() = PropEdit::new(f, String::new());
        for c in text.chars() {
            app.props_edit_key(Key::char(c));
        }
        app.props_edit_key(Key::new(KeyCode::Enter));
    }

    /// The tree: the properties pane is the last child of a side-by-side root.
    fn docked_right(app: &App) -> Option<(Node, PaneKindId)> {
        let id = app.tool_pane(PaneKind::Properties)?;
        match &app.view().root {
            Node::Split {
                axis: Axis::SideBySide,
                children,
                ..
            } if children.last() == Some(&Node::Leaf(id)) => Some((children[0].clone(), id)),
            _ => None,
        }
    }
    type PaneKindId = crate::layout::PaneId;

    #[test]
    fn docks_along_the_right_in_either_order_with_bottom_tools() {
        for props_first in [true, false] {
            let mut app = test_app();
            if props_first {
                app.run(Command::Properties(Some(true)));
                app.run(Command::Tracks(Some(true)));
            } else {
                app.run(Command::Tracks(Some(true)));
                app.run(Command::Properties(Some(true)));
            }
            let tracks = app.tool_pane(PaneKind::Tracks).unwrap();
            let (body, _) = docked_right(&app).expect("properties is the right-hand child");
            // The tracks pane lies under the scores, left of the properties pane.
            assert!(
                body.panes().contains(&tracks),
                "props_first={props_first}: {body:?}"
            );
            let rects = app.layout();
            let (props, tr) = (
                rects
                    .iter()
                    .find(|(i, _)| app.panes[i].kind == PaneKind::Properties),
                rects.iter().find(|(i, _)| *i == tracks),
            );
            let (props, tr) = (props.unwrap().1, tr.unwrap().1);
            assert_eq!(
                app.panes
                    .values()
                    .find(|p| p.kind == PaneKind::Properties)
                    .unwrap()
                    .size
                    .1,
                Some(props.w)
            );
            assert!(tr.x + tr.w <= props.x, "tracks sit left of properties");
        }
        let mut app = with_props();
        app.run(Command::Properties(Some(false)));
        assert_eq!(app.view().root.panes().len(), 1, "the tree collapses again");
    }

    #[test]
    fn the_pane_takes_the_focus_and_gives_it_back() {
        let mut app = with_props();
        let score = app.focused();
        keys(&mut app, "<leader>P");
        assert_eq!(app.focused_kind(), PaneKind::Properties);
        assert_eq!(app.focused(), score, "tools work on the last score pane");
        keys(&mut app, "<Esc>");
        assert_eq!(app.focused_kind(), PaneKind::Score);
        keys(&mut app, "<leader>p");
        assert!(!app.has_tool(PaneKind::Properties));
        keys(&mut app, "<leader>p");
        assert!(app.has_tool(PaneKind::Properties));
    }

    #[test]
    fn tabs_and_sections_fold() {
        let mut app = with_props();
        keys(&mut app, "<leader>P");
        assert_eq!(app.props.tab, PropTab::Score);
        keys(&mut app, "2");
        assert_eq!(app.props.tab, PropTab::Tracks);
        keys(&mut app, "L");
        assert_eq!(app.props.tab, PropTab::Lyrics);
        keys(&mut app, "L");
        assert_eq!(app.props.tab, PropTab::Score, "tabs wrap");
        keys(&mut app, "H");
        assert_eq!(app.props.tab, PropTab::Lyrics);
        keys(&mut app, "1");
        let before = app.property_rows().len();
        keys(&mut app, "za");
        let after = app.property_rows().len();
        assert!(after < before, "folding the info section hides its rows");
        keys(&mut app, "za");
        assert_eq!(app.property_rows().len(), before);
        // zc closes a section and stays closed; zo opens it.
        keys(&mut app, "zczc");
        assert_eq!(app.property_rows().len(), after);
        keys(&mut app, "zozo");
        assert_eq!(app.property_rows().len(), before);
    }

    #[test]
    fn titles_and_heights() {
        assert_eq!(PropTab::Tracks.name(), "track");
        assert_eq!(PropTab::Tracks.title(), "TRACK");
        assert_eq!(PropTab::parse("tracks"), Some(PropTab::Tracks));
        assert_eq!(small_caps("Page setup"), "Pᴀɢᴇ sᴇᴛᴜᴘ");
        let mut app = with_props();
        let rows = app.property_rows();
        assert!(
            !rows
                .iter()
                .any(|r| r.label == "Tempo" || r.label == "Pickup bar"),
            "the score tab has no tempo section"
        );
        // Scrolling keeps the selected row in view.
        let last = rows.len() - 1;
        app.props.selected = last;
        let first = app.property_window(10);
        let used: usize = rows[first..=last].iter().map(PropRow::height).sum();
        assert!(first > 0 && used <= 10, "{first} {used}");
    }

    #[test]
    fn text_wraps_and_the_caret_is_found() {
        let lines = wrap_ranges("ab cd\nef", |l| l.chars().count() <= 4);
        assert_eq!(lines, [(0, 2), (3, 5), (6, 8)]);
        assert_eq!(caret_in(&lines, 4), (1, 1));
        assert_eq!(caret_in(&lines, 8), (2, 2));
        assert_eq!(wrap_ranges("", |_| true), [(0, 0)]);
    }

    #[test]
    fn score_properties_edit_and_undo() {
        let mut app = with_props();
        type_into(&mut app, Field::Title, "My Song");
        assert_eq!(app.buffer().score.meta.title, "My Song");
        type_into(&mut app, Field::MarginLeft, "25");
        assert_eq!(app.buffer().score.page.margin_left, 25);
        type_into(&mut app, Field::PageScale, "x");
        assert_eq!(
            app.buffer().score.page.scale,
            100,
            "bad numbers are refused"
        );
        app.run(Command::Undo);
        assert_eq!(app.buffer().score.page.margin_left, 10);
    }

    #[test]
    fn track_properties_follow_the_cursor_track() {
        let mut app = with_props();
        app.set_property_tab(PropTab::Tracks);
        assert!(
            !matches!(
                app.property_rows()[app.property_selected()].kind,
                RowKind::Note
            ),
            "the selection skips the track note"
        );
        type_into(&mut app, Field::TrackName, "Lead");
        assert_eq!(app.buffer().score.tracks[0].name, "Lead");
        type_into(&mut app, Field::Capo, "2");
        assert_eq!(app.buffer().score.tracks[0].capo, 2);
        type_into(&mut app, Field::Volume, "80");
        assert_eq!(app.buffer().score.tracks[0].midi.volume, 80);
        type_into(&mut app, Field::Channel, "5");
        assert_eq!(app.buffer().score.tracks[0].midi.channel, 4);
        // Color: a picker of named colors, with hex or rgb() typed for a custom one.
        let color = |app: &App| {
            app.buffer().score.tracks[0]
                .display_color
                .map(|c| (c.r, c.g, c.b))
        };
        app.props.selected = field_row(&app, Field::TrackColor);
        app.run(Command::PropertiesEdit);
        assert_eq!(app.mode, Mode::Picker);
        let p = app.palette().unwrap();
        assert_eq!(p.title, "Colors");
        assert!(p.items.iter().all(|i| i.swatch.is_some()));
        keys(&mut app, "gold<CR>");
        assert_eq!(color(&app), Some((0xff, 0xb3, 0x00)));
        app.props.selected = field_row(&app, Field::TrackColor);
        app.run(Command::PropertiesEdit);
        keys(&mut app, "#102030");
        assert_eq!(app.palette().unwrap().items[0].text, "Custom");
        keys(&mut app, "<CR>");
        assert_eq!(color(&app), Some((0x10, 0x20, 0x30)));
        app.run(Command::PropertiesEdit);
        keys(&mut app, "rgb(1, 2, 3)<CR>");
        assert_eq!(color(&app), Some((1, 2, 3)));
        assert_eq!(app.mode, Mode::Normal);
        // Tuning: a picker of the instrument's tunings.
        app.props.selected = field_row(&app, Field::Tuning);
        app.run(Command::PropertiesEdit);
        assert_eq!(app.palette().unwrap().title, "Tunings");
        keys(&mut app, "drop d<CR>");
        assert_eq!(app.buffer().score.tracks[0].strings[5], 38);
        app.props.selected = field_row(&app, Field::Tuning);
        app.run(Command::PropertiesEdit);
        keys(&mut app, "E2 A2 D3 G3 B3 E4");
        assert_eq!(app.palette().unwrap().items[0].text, "Custom");
        keys(&mut app, "<CR>");
        assert_eq!(
            app.buffer().score.tracks[0].strings,
            [64, 59, 55, 50, 45, 40]
        );
        app.props.selected = field_row(&app, Field::Mute);
        app.run(Command::PropertiesEdit);
        assert!(app.buffer().score.tracks[0].mute);
        // The second track, through the cursor.
        app.run(Command::TrackNext);
        let rows = app.property_rows();
        assert_eq!(rows[0].label, "Track 2 of 2");
        type_into(&mut app, Field::TrackName, "Rhythm");
        assert_eq!(app.buffer().score.tracks[1].name, "Rhythm");
        assert_eq!(app.buffer().score.tracks[0].name, "Lead");
        // Read-only rows refuse.
        app.props.selected = field_row(&app, Field::TrackKind);
        app.run(Command::PropertiesEdit);
        assert!(app.props.edit.is_none());
    }

    #[test]
    fn lyrics_are_a_vim_text_area_with_live_undo_steps() {
        let mut app = with_props();
        lyrics_text(&mut app, "Hel-lo world");
        app.set_property_tab(PropTab::Lyrics);
        let rows = app.property_rows();
        // The text area has no label.
        let area = &rows[field_row(&app, Field::Lyrics)];
        assert_eq!(
            (area.label.as_str(), area.value.as_str()),
            ("", "Hel-lo world")
        );
        assert!(!rows.iter().any(|r| r.label == "Lyrics"), "{rows:?}");
        let verse = |app: &App, v| {
            cecilia_core::lyrics::verse_line(&app.buffer().score.tracks[0], v).map(|l| l.text)
        };
        // The text area, in normal mode (the block caret).
        keys(&mut app, "<leader>P");
        assert!(
            app.props
                .edit
                .as_ref()
                .is_some_and(|e| e.modal && !e.insert)
        );
        // Enter is i; the score follows every key; Esc is normal mode.
        keys(&mut app, "<CR>");
        assert_eq!(app.display_mode(), Mode::Insert);
        keys(&mut app, "<C-u>Good-bye");
        assert_eq!(verse(&app, 0).as_deref(), Some("Good-bye"), "live");
        assert_eq!(app.buffer().score.tracks[0].lyrics_text, "Good-bye");
        // Line breaks only separate words.
        keys(&mut app, "<CR><CR>la la");
        assert_eq!(verse(&app, 0).as_deref(), Some("Good-bye la la"));
        assert_eq!(verse(&app, 1), None);
        keys(&mut app, "<Esc>");
        assert_eq!(app.display_mode(), Mode::Normal);
        assert!(app.props.edit.is_some(), "still in the text area");
        // The whole insert session is one undo step.
        keys(&mut app, "u");
        assert_eq!(verse(&app, 0).as_deref(), Some("Hel-lo world"));
        assert_eq!(verse(&app, 1), None);
        assert_eq!(app.props.edit.as_ref().unwrap().text, "Hel-lo world");
        // Normal mode: motions and changes, each its own step; i and Enter insert.
        keys(&mut app, "0e");
        assert_eq!(
            app.props.edit.as_ref().unwrap().caret,
            2,
            "e: the end of the word"
        );
        keys(&mut app, "e");
        assert_eq!(
            app.props.edit.as_ref().unwrap().caret,
            3,
            "and of the next: the sign"
        );
        keys(&mut app, "0wlx");
        assert_eq!(verse(&app, 0).as_deref(), Some("Hel-o world"));
        keys(&mut app, "ddu");
        assert_eq!(verse(&app, 0).as_deref(), Some("Hel-o world"));
        keys(&mut app, "ix<Esc>");
        assert_eq!(app.props.edit.as_ref().unwrap().text, "xHel-o world");
        // Escape in normal mode leaves the text area and the pane.
        keys(&mut app, "<Esc>");
        assert!(app.props.edit.is_none(), "Escape in normal mode leaves");
        assert_eq!(app.focused_kind(), PaneKind::Score);
        assert_eq!(app.display_mode(), Mode::Normal);
        // `d` in the pane clears the lyrics.
        app.run(Command::PropertiesClear);
        assert_eq!(verse(&app, 0), None);
        assert_eq!(app.buffer().score.tracks[0].lyrics_text, "");
    }

    #[test]
    fn a_chant_tracks_lyrics_are_its_gabc() {
        let mut app = with_props();
        let chant =
            cecilia_formats::gabc::parse("name: t;\n%%\n(c4) Al(fg)le(h) lu(g)ia.(f.) (::)\n")
                .unwrap();
        app.buffers[0].score = cecilia_formats::gabc::chant_score(chant);
        app.set_property_tab(PropTab::Lyrics);
        let shown = |app: &App| {
            app.property_rows()[field_row(app, Field::Lyrics)]
                .value
                .clone()
        };
        // A chant track's text is always its GABC; there is no switch.
        assert!(
            !app.property_rows().iter().any(|r| r.label == "GABC"),
            "no GABC switch"
        );
        assert_eq!(shown(&app), "(c4) Al(fg)le(h) lu(g)ia.(f.) (::)");
        // Raw edits are read as GABC: the chant (and what plays) follows.
        keys(&mut app, "<leader>P0llllllllll");
        assert_eq!(app.props.edit.as_ref().unwrap().caret, 10);
        keys(&mut app, "i h<Esc>");
        let chant = app.buffer().score.tracks[0].chant.clone().unwrap();
        assert_eq!(chant.syllables[1].gabc_notes(), "fg h");
        assert_eq!(chant.header("name"), Some("t"), "the header stays");
        // Text typed at the start of the GABC is read too.
        keys(&mut app, "0iH<Esc>");
        let chant = app.buffer().score.tracks[0].chant.clone().unwrap();
        assert_ne!(chant.syllables[1].gabc_notes(), "");
        // A syllable typed key by key: the text keeps what is typed (a space at the end, a
        // group not closed yet) and the chant follows once it reads as GABC.
        keys(&mut app, "A Ky(");
        assert!(app.props.edit.as_ref().unwrap().text.ends_with("(::) Ky("));
        keys(&mut app, "f)<Esc>");
        let chant = app.buffer().score.tracks[0].chant.clone().unwrap();
        let ky = chant.syllables.last().unwrap();
        assert_eq!((ky.text.as_str(), ky.gabc_notes().as_str()), ("Ky", "f"));
        assert!(ky.word_start);
        keys(&mut app, "u");
        let chant = app.buffer().score.tracks[0].chant.clone().unwrap();
        assert_ne!(chant.syllables.last().unwrap().text, "Ky", "one undo step");
        assert!(!app.props.edit.as_ref().unwrap().text.contains("Ky("));
    }

    #[test]
    fn a_chant_track_made_a_staff_track_keeps_showing_its_lyrics() {
        let mut app = with_props();
        let chant =
            cecilia_formats::gabc::parse("name: t;\n%%\n(c4) Al(fg)le(h) lu(g)ia.(f.)\n").unwrap();
        app.buffers[0].score = cecilia_formats::gabc::chant_score(chant);
        app.run(Command::ChantToModern);
        assert!(app.buffer().score.tracks[0].chant.is_none());
        app.set_property_tab(PropTab::Lyrics);
        let shown = app.property_rows()[field_row(&app, Field::Lyrics)]
            .value
            .clone();
        // "Al" is sung on two notes: a dash covers the second.
        assert_eq!(shown, "Al- _ le lu-ia.");
    }

    #[test]
    fn a_staff_tracks_text_is_the_modern_syntax_without_a_gabc_switch() {
        let mut app = with_props();
        lyrics_text(&mut app, "(1:1) <i>Ky</i>-ri-e e-le-i-son");
        app.set_property_tab(PropTab::Lyrics);
        let shown = |app: &App| {
            app.property_rows()[field_row(app, Field::Lyrics)]
                .value
                .clone()
        };
        assert_eq!(shown(&app), "(1:1) <i>Ky</i>-ri-e e-le-i-son");
        assert!(!app.lyrics_is_gabc());
        assert!(
            !app.property_rows().iter().any(|r| r.label == "GABC"),
            "no GABC switch"
        );
        let s = |app: &App, b: usize, i: usize| {
            app.buffer().score.tracks[0].bars[b].voices[0].beats[i]
                .lyrics
                .first()
                .cloned()
                .unwrap_or_default()
        };
        assert_eq!(s(&app, 0, 0), "Ky-");
        keys(&mut app, "<leader>P$i,<Esc>");
        assert_eq!(
            app.buffer().score.tracks[0].lyrics_text,
            "(1:1) <i>Ky</i>-ri-e e-le-i-so,n"
        );
        keys(&mut app, "u");
        assert_eq!(shown(&app), "(1:1) <i>Ky</i>-ri-e e-le-i-son");
    }

    #[test]
    fn editing_is_live_insert_mode_and_escape_cancels() {
        let mut app = with_props();
        keys(&mut app, "<leader>P");
        app.props.selected = field_row(&app, Field::Title);
        assert_eq!(app.display_mode(), Mode::Normal);
        keys(&mut app, "ijq");
        assert_eq!(app.display_mode(), Mode::Insert, "every key is typed");
        assert_eq!(
            app.buffer().score.meta.title,
            "jq",
            "the score updates live"
        );
        // The caret moves; typing goes in at it.
        keys(&mut app, "<Left>x<Home>y");
        assert_eq!(app.buffer().score.meta.title, "yjxq");
        // Escape cancels: the title is as it was, and nothing is left to undo.
        keys(&mut app, "<Esc>");
        assert!(app.props.edit.is_none());
        assert_eq!(app.display_mode(), Mode::Normal);
        assert_eq!(app.buffer().score.meta.title, "");
        assert_eq!(app.focused_kind(), PaneKind::Properties);
        // Enter keeps it, as one undo step; Ctrl-C cancels like Escape.
        keys(&mut app, "<CR>AB<CR>");
        assert_eq!(app.buffer().score.meta.title, "AB");
        keys(&mut app, "iZ<C-c>");
        assert_eq!(app.buffer().score.meta.title, "AB");
        app.run(Command::Undo);
        assert_eq!(app.buffer().score.meta.title, "");
        // A half-typed number is passed over; a bad one is refused on Enter.
        type_into(&mut app, Field::PageScale, "x");
        assert_eq!(app.buffer().score.page.scale, 100);
        assert!(app.props.edit.is_none());
    }

    #[test]
    fn staves_are_rows_with_a_picker_and_reorder() {
        let mut app = with_props();
        app.set_property_tab(PropTab::Tracks);
        let names = |app: &App| {
            app.buffer().score.tracks[0]
                .staves()
                .iter()
                .map(|s| s.name())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&app), ["treble", "tab"]);
        // An empty row at the bottom adds a staff through the picker.
        app.props.selected = field_row(&app, Field::StaffAt(2));
        let add = &app.property_rows()[app.property_selected()];
        assert_eq!((add.label.as_str(), add.placeholder), ("", "Add Staff"));
        let first = &app.property_rows()[field_row(&app, Field::StaffAt(0))];
        assert_eq!(first.label, "Staves");
        assert!(first.icon.is_some());
        app.run(Command::PropertiesEdit);
        assert_eq!(app.palette().unwrap().title, "Notation");
        keys(&mut app, "slash<CR>");
        assert_eq!(names(&app), ["treble", "tab", "slash"]);
        // A filled row picks another kind.
        app.props.selected = field_row(&app, Field::StaffAt(1));
        app.run(Command::PropertiesEdit);
        keys(&mut app, "tab+<CR>");
        assert_eq!(names(&app)[1], "tab+");
        // Alt-j / alt-k swap with the staff below / above; the selection follows.
        keys(&mut app, "<leader>P");
        app.props.selected = field_row(&app, Field::StaffAt(0));
        keys(&mut app, "<A-j>");
        assert_eq!(names(&app), ["tab+", "treble", "slash"]);
        assert_eq!(app.props.selected, field_row(&app, Field::StaffAt(1)));
        keys(&mut app, "<A-k>");
        assert_eq!(names(&app), ["treble", "tab+", "slash"]);
        // Dragging a row onto another moves it.
        let (a, b) = (
            field_row(&app, Field::StaffAt(0)),
            field_row(&app, Field::StaffAt(2)),
        );
        assert!(
            app.property_is_staff(a) && !app.property_is_staff(field_row(&app, Field::StaffAt(3)))
        );
        app.drag_property(a, b);
        assert_eq!(names(&app), ["tab+", "slash", "treble"]);
    }

    #[test]
    fn staff_rows_have_a_notation_box_and_a_clef_box() {
        let mut app = with_props();
        app.set_property_tab(PropTab::Tracks);
        let row = |app: &App, i| app.property_rows()[field_row(app, Field::StaffAt(i))].clone();
        let (a, b) = (row(&app, 0), row(&app, 1));
        assert_eq!(
            (a.value.as_str(), a.value2.as_str()),
            ("standard", "treble")
        );
        assert_eq!(
            (a.icon, a.icon2),
            (Some(Icon::Standard), Some(Icon::Treble))
        );
        assert_eq!((b.value.as_str(), b.value2.as_str()), ("tab", ""));
        assert_eq!((b.icon, b.icon2), (Some(Icon::Tab), None));
        let add = row(&app, 2);
        assert!(add.value.is_empty() && add.value2.is_empty());
        keys(&mut app, "<leader>P");
        // The notation box is selected at first; the clef picker lists the clefs.
        app.props.selected = field_row(&app, Field::StaffAt(0));
        assert!(app.property_box_selected(app.props.selected, 0));
        keys(&mut app, "l");
        assert!(app.property_box_selected(app.props.selected, 1));
        app.run(Command::PropertiesEdit);
        assert_eq!(app.palette().unwrap().title, "Clefs");
        keys(&mut app, "bass<CR>");
        assert_eq!(app.buffer().score.tracks[0].staves()[0].name(), "bass");
        // Stepping in the clef box changes the clef only.
        keys(&mut app, "<A-l>");
        assert_eq!(app.buffer().score.tracks[0].staves()[0].name(), "grand");
        keys(&mut app, "h<A-h>");
        assert_eq!(app.buffer().score.tracks[0].staves()[0].name(), "grand");
        // The notation box steps through the notations, keeping what it can.
        keys(&mut app, "h");
        keys(&mut app, "<A-l>");
        assert_eq!(app.buffer().score.tracks[0].staves()[0], Staff::Tab);
        // A tab has no clef: the clef box can't be selected or opened.
        keys(&mut app, "l");
        assert!(app.property_box_selected(app.props.selected, 0));
        app.run(Command::PropertiesEdit);
        assert_eq!(app.palette().unwrap().title, "Notation");
        keys(&mut app, "<Esc>");
        // Back to standard gives the default clef.
        app.run(Command::Notation("standard".into()));
        assert_eq!(
            app.buffer().score.tracks[0].staves()[0].notation(),
            Notation::Standard
        );
        // The add row picks a notation first.
        app.props.selected = field_row(&app, Field::StaffAt(2));
        keys(&mut app, "l");
        assert!(app.property_box_selected(app.props.selected, 0));
        app.run(Command::PropertiesEdit);
        assert_eq!(app.palette().unwrap().title, "Notation");
        keys(&mut app, "slash<CR>");
        assert_eq!(app.buffer().score.tracks[0].staves()[2], Staff::Slash);
    }

    #[test]
    fn keys_for_tabs_clearing_and_toggles() {
        let mut app = with_props();
        keys(&mut app, "<leader>P");
        keys(&mut app, "L");
        assert_eq!(app.props.tab, PropTab::Tracks);
        keys(&mut app, "<C-Tab>");
        assert_eq!(app.props.tab, PropTab::Lyrics);
        keys(&mut app, "<C-S-Tab>H");
        assert_eq!(app.props.tab, PropTab::Score);
        // `d` empties a text property; `c` empties it and starts typing.
        type_into(&mut app, Field::Title, "Song");
        app.props.selected = field_row(&app, Field::Title);
        keys(&mut app, "d");
        assert_eq!(app.buffer().score.meta.title, "");
        type_into(&mut app, Field::Title, "Song");
        app.props.selected = field_row(&app, Field::Title);
        keys(&mut app, "cNew");
        assert_eq!(app.display_mode(), Mode::Insert);
        assert_eq!(app.buffer().score.meta.title, "New");
        keys(&mut app, "<CR>");
        // A click on a switch flips it at once.
        app.set_property_tab(PropTab::Tracks);
        let mute = field_row(&app, Field::Mute);
        app.click_property(mute, false);
        assert!(app.buffer().score.tracks[0].mute);
        // h / l set a switch off / on, and keep it so.
        keys(&mut app, "<A-h><A-h>");
        assert!(!app.buffer().score.tracks[0].mute);
        keys(&mut app, "<A-l><A-l>");
        assert!(app.buffer().score.tracks[0].mute);
        // Only a double click edits; one click selects.
        let name = field_row(&app, Field::TrackName);
        app.click_property(name, false);
        assert!(app.props.edit.is_none() && app.property_selected() == name);
        app.click_property(name, true);
        assert!(app.props.edit.is_some());
        keys(&mut app, "<Esc>");
    }

    #[test]
    fn choices_step_without_wrapping_and_the_staff_group_highlights() {
        let mut app = with_props();
        app.set_property_tab(PropTab::Tracks);
        keys(&mut app, "<leader>P");
        // h / l on a choice: the previous / next one, stopping at the ends.
        app.props.selected = field_row(&app, Field::Tuning);
        let strings = |app: &App| app.buffer().score.tracks[0].strings.clone();
        assert_eq!(strings(&app), [64, 59, 55, 50, 45, 40]);
        keys(&mut app, "<A-h>");
        assert_eq!(
            strings(&app),
            [64, 59, 55, 50, 45, 40],
            "no wrapping before the first"
        );
        keys(&mut app, "<A-l>");
        assert_eq!(strings(&app), [63, 58, 54, 49, 44, 39], "the next tuning");
        app.props.selected = field_row(&app, Field::Program);
        let before = app.buffer().score.tracks[0].midi.program;
        keys(&mut app, "<A-l>");
        assert_eq!(app.buffer().score.tracks[0].midi.program, before + 1);
        // Every staff row is lit with the selected one.
        app.props.selected = field_row(&app, Field::StaffAt(1));
        assert!(app.property_row_selected(field_row(&app, Field::StaffAt(0))));
        assert!(app.property_row_selected(field_row(&app, Field::StaffAt(2))));
        assert!(!app.property_row_selected(field_row(&app, Field::Capo)));
        // The staff kind steps too.
        keys(&mut app, "<A-l>");
        assert_eq!(app.buffer().score.tracks[0].staves()[1].name(), "tab+");
        assert_eq!(Icon::of_program(25), Icon::Guitar);
    }

    #[test]
    fn gg_and_capital_g_select_the_first_and_last_property() {
        let mut app = with_props();
        app.set_property_tab(PropTab::Tracks);
        keys(&mut app, "<leader>P");
        let rows = app.property_rows();
        let last = rows.iter().rposition(|r| r.kind != RowKind::Note).unwrap();
        let first = rows.iter().position(|r| r.kind != RowKind::Note).unwrap();
        keys(&mut app, "G");
        assert_eq!(app.property_selected(), last);
        keys(&mut app, "gg");
        assert_eq!(app.property_selected(), first);
    }

    #[test]
    fn zz_centres_the_selected_property() {
        let mut app = with_props();
        app.set_property_tab(PropTab::Tracks);
        keys(&mut app, "<leader>P");
        let rows = app.property_rows();
        let sel = rows.len() / 2;
        app.props.selected = sel;
        let visible = 12;
        app.property_window(visible);
        keys(&mut app, "zz");
        let first = app.property_window(visible);
        let h = |r: &PropRow| r.height();
        let above: usize = rows[first..sel].iter().map(h).sum();
        let after = visible.saturating_sub(above + h(&rows[sel]));
        // As near the middle as whole rows allow.
        assert!(above.abs_diff(after) <= 4, "{above} above, {after} below");
        assert!(first > 0);
        // Drawing again keeps it.
        assert_eq!(app.property_window(visible), first);
    }

    #[test]
    fn j_and_k_move_by_visual_lines() {
        let mut app = with_props();
        lyrics_text(&mut app, "aa bb cc dd");
        app.set_property_tab(PropTab::Lyrics);
        keys(&mut app, "<leader>P");
        // As drawn: two visual lines of one logical line.
        *app.props.visual.borrow_mut() = vec![(0, 5), (6, 11)];
        app.props.edit.as_mut().unwrap().caret = 1;
        keys(&mut app, "j");
        assert_eq!(app.props.edit.as_ref().unwrap().caret, 7);
        keys(&mut app, "k");
        assert_eq!(app.props.edit.as_ref().unwrap().caret, 1);
    }

    #[test]
    fn numbers_step_arrows_follow_hjkl_and_the_wheel_scrolls() {
        let mut app = with_props();
        app.set_property_tab(PropTab::Tracks);
        keys(&mut app, "<leader>P");
        app.props.selected = field_row(&app, Field::Capo);
        keys(&mut app, "<A-l>");
        keys(&mut app, "<A-l>");
        assert_eq!(app.buffer().score.tracks[0].capo, 2);
        keys(&mut app, "<A-h>");
        assert_eq!(app.buffer().score.tracks[0].capo, 1);
        keys(&mut app, "<A-l>");
        assert_eq!(app.buffer().score.tracks[0].capo, 2);
        keys(&mut app, "<A-h>");
        assert_eq!(app.buffer().score.tracks[0].capo, 1);
        // Tab is j, shift-tab is k, the arrows likewise.
        let at = app.property_selected();
        keys(&mut app, "<Tab>");
        assert_eq!(app.property_selected(), at + 1);
        keys(&mut app, "<S-Tab>");
        assert_eq!(app.property_selected(), at);
        keys(&mut app, "<Down><Up>");
        assert_eq!(app.property_selected(), at);
        // The wheel scrolls the rows and leaves the selection.
        app.props.selected = 1;
        assert_eq!(app.property_window(8), 0);
        app.scroll_properties(3);
        assert_eq!(app.property_window(8), 3);
        assert_eq!(app.property_selected(), 1);
        // Pickers: the wheel moves the selection.
        app.run(Command::Pick(crate::picker::PickerKind::Keymap));
        app.picker_scroll(2);
        assert_eq!(app.picker.as_ref().unwrap().selected, 2);
        app.picker_scroll(-5);
        assert_eq!(app.picker.as_ref().unwrap().selected, 0);
        keys(&mut app, "<Esc>");
        // The staff names: treble, bass, grand.
        use cecilia_core::{Clef, Staff};
        assert_eq!(
            Staff::parse("bass", Clef::Treble),
            Ok(Staff::Standard(Clef::Bass))
        );
        assert_eq!(Staff::Standard(Clef::Treble).name(), "treble");
        assert!(matches!(
            Staff::parse("grand G3", Clef::Treble),
            Ok(Staff::Standard(Clef::Grand { split: 55 }))
        ));
    }

    #[test]
    fn search_jumps_as_typed_and_n_repeats() {
        let mut app = with_props();
        keys(&mut app, "<leader>P");
        let row = |app: &App| app.property_rows()[app.property_selected()].label.clone();
        // `/` opens the field and jumps at once; Escape goes back and sets nothing.
        keys(&mut app, "/arti");
        assert!(app.props.search.is_some());
        assert_eq!(row(&app), "Artist");
        keys(&mut app, "<Esc>");
        assert!(app.props.search.is_none() && app.props.last_search.is_empty());
        assert_ne!(row(&app), "Artist");
        // Enter keeps the jump and sets the register; n and N go on from there.
        keys(&mut app, "/t<CR>");
        let first = row(&app);
        assert_eq!(app.props.last_search, "t");
        keys(&mut app, "n");
        let second = row(&app);
        assert_ne!(first, second);
        keys(&mut app, "N");
        assert_eq!(row(&app), first);
        // In the lyrics text area it looks through the text.
        lyrics_text(&mut app, "one two three two");
        app.set_property_tab(PropTab::Lyrics);
        keys(&mut app, "j0/two<CR>");
        assert_eq!(app.props.edit.as_ref().unwrap().caret, 4);
        keys(&mut app, "n");
        assert_eq!(app.props.edit.as_ref().unwrap().caret, 14);
        keys(&mut app, "n");
        assert_eq!(app.props.edit.as_ref().unwrap().caret, 4, "it wraps");
        keys(&mut app, "N");
        assert_eq!(app.props.edit.as_ref().unwrap().caret, 14);
        keys(&mut app, "0/e<Esc>");
        assert_eq!(
            app.props.edit.as_ref().unwrap().caret,
            0,
            "Escape goes back"
        );
    }

    #[test]
    fn lyric_line_breaks_come_from_the_file_and_the_add_staff_field_steps() {
        let mut app = with_props();
        lyrics_text(&mut app, "Hel-lo world");
        // The score keeps the text as it was given, with its line breaks.
        app.set_property_tab(PropTab::Lyrics);
        keys(&mut app, "<leader>P");
        keys(&mut app, "<CR><C-u>Hel-lo<CR>world<Esc>");
        let rows = app.property_rows();
        let lyrics = &rows[field_row(&app, Field::Lyrics)];
        assert_eq!(lyrics.value, "Hel-lo\nworld");
        // A double click in the text area is `i`.
        assert!(!app.props.edit.as_ref().unwrap().insert);
        let row = app.props.selected;
        app.click_property(row, true);
        assert!(app.props.edit.as_ref().unwrap().insert);
        keys(&mut app, "<Esc><Esc>");
        assert_eq!(
            app.focused_kind(),
            PaneKind::Score,
            "the second Escape leaves the pane"
        );
        // h on the empty staff field adds the last kind, l the first.
        keys(&mut app, "<leader>P");
        app.set_property_tab(PropTab::Tracks);
        app.props.selected = field_row(&app, Field::StaffAt(2));
        keys(&mut app, "<A-h>");
        let staves = app.buffer().score.tracks[0].staves();
        assert_eq!(staves[2], Staff::Slash);
        app.props.selected = field_row(&app, Field::StaffAt(3));
        keys(&mut app, "<A-l>");
        let staves = app.buffer().score.tracks[0].staves();
        assert_eq!(staves[3].name(), "treble");
    }

    #[test]
    fn the_lyrics_text_scrolls_with_the_wheel_and_takes_the_caret_from_a_click() {
        let mut app = with_props();
        lyrics_text(&mut app, "one two three");
        app.set_property_tab(PropTab::Lyrics);
        // 20 lines, 5 shown: the wheel scrolls, and stays while the caret doesn't move.
        assert_eq!(app.text_window(None, 20, 5), 0);
        app.scroll_properties(4);
        assert_eq!(app.text_window(None, 20, 5), 4);
        app.scroll_properties(100);
        assert_eq!(app.text_window(None, 20, 5), 15, "kept within the text");
        app.scroll_properties(-100);
        assert_eq!(app.text_window(None, 20, 5), 0);
        // A caret that moves to a line out of view brings it into view.
        assert_eq!(app.text_window(Some((7, 9)), 20, 5), 5);
        app.scroll_properties(-3);
        assert_eq!(
            app.text_window(Some((7, 9)), 20, 5),
            2,
            "not while it stays"
        );
        // A double click edits and the caret goes where it is told.
        keys(&mut app, "<leader>P");
        let row = field_row(&app, Field::Lyrics);
        app.click_property(row, true);
        app.set_text_caret(4);
        assert_eq!(app.props.edit.as_ref().unwrap().caret, 4);
    }

    #[test]
    fn the_mouse_places_the_caret_and_selects_in_the_lyrics() {
        let mut app = with_props();
        lyrics_text(&mut app, "one two three");
        app.set_property_tab(PropTab::Lyrics);
        keys(&mut app, "<leader>P");
        let row = app.props.selected;
        assert!(app.text_editing(row));
        let edit = |app: &App| app.props.edit.clone().unwrap();
        // A click places the caret; with no drag nothing is selected.
        app.text_click(4);
        assert_eq!(edit(&app).caret, 4);
        app.text_release();
        assert_eq!(edit(&app).selection(), None);
        // A drag selects from the press to where it ends (backwards too).
        app.text_click(4);
        app.text_drag(7);
        assert_eq!(edit(&app).selection(), Some((4, 7)));
        app.text_release();
        assert_eq!(edit(&app).selection(), Some((4, 7)));
        app.text_drag(1);
        assert_eq!(edit(&app).selection(), Some((1, 4)));
        // Normal mode: d removes it, as one step of its own.
        app.text_drag(7);
        keys(&mut app, "d");
        assert_eq!(edit(&app).text, "one  three");
        assert_eq!(edit(&app).selection(), None);
        // Insert mode: typing replaces the selection, Backspace removes it.
        keys(&mut app, "i");
        app.text_click(0);
        app.text_drag(3);
        keys(&mut app, "X");
        assert_eq!(edit(&app).text, "X  three");
        app.text_click(1);
        app.text_drag(3);
        keys(&mut app, "<BS>");
        assert_eq!(edit(&app).text, "Xthree");
        // A click clears a selection; Escape in normal mode clears it before leaving.
        keys(&mut app, "<Esc>");
        app.text_click(0);
        app.text_drag(2);
        keys(&mut app, "<Esc>");
        assert!(app.props.edit.is_some() && edit(&app).selection().is_none());
    }

    #[test]
    fn words_select_all_and_the_clipboard() {
        let mut app = with_props();
        lyrics_text(&mut app, "one two three");
        app.set_property_tab(PropTab::Lyrics);
        keys(&mut app, "<leader>P<CR>");
        let edit = |app: &App| app.props.edit.clone().unwrap();
        // In insert mode a double click selects the word; in normal mode it enters insert mode.
        app.text_double_click(5);
        assert_eq!(edit(&app).selection(), Some((4, 7)));
        assert_eq!(edit(&app).selected_text().as_deref(), Some("two"));
        app.text_double_click(3);
        assert_eq!(edit(&app).selection(), Some((3, 4)), "the space between");
        keys(&mut app, "<Esc><Esc>");
        assert!(app.props.edit.is_none());
        keys(&mut app, "<leader>P");
        assert!(
            !edit(&app).insert,
            "focusing the pane on the text area gives it the caret"
        );
        assert!(!edit(&app).insert);
        app.text_double_click(5);
        assert!(edit(&app).insert && edit(&app).selection().is_none());
        assert_eq!(edit(&app).caret, 5);
        // Ctrl-A selects everything; Ctrl-C copies; typing over a selection replaces it.
        keys(&mut app, "<C-a>");
        assert_eq!(edit(&app).selection(), Some((0, 13)));
        keys(&mut app, "<C-c>");
        assert_eq!(crate::clipboard::get(), "one two three");
        keys(&mut app, "Z");
        assert_eq!(edit(&app).text, "Z");
        // Ctrl-V pastes at the caret, Ctrl-X cuts.
        keys(&mut app, "<C-v>");
        assert_eq!(edit(&app).text, "Zone two three");
        app.text_click(0);
        app.text_drag(4);
        keys(&mut app, "<C-x>");
        assert_eq!(edit(&app).text, " two three");
        assert_eq!(crate::clipboard::get(), "Zone");
        // Normal mode: y with a selection, yy for the line, p and P paste.
        keys(&mut app, "<Esc>");
        app.text_click(1);
        app.text_drag(4);
        keys(&mut app, "y");
        assert_eq!(crate::clipboard::get(), "two");
        keys(&mut app, "$p");
        assert_eq!(edit(&app).text, " two threetwo");
        keys(&mut app, "0P");
        assert_eq!(edit(&app).text, "two two threetwo");
        keys(&mut app, "yy");
        assert_eq!(crate::clipboard::get(), "two two threetwo\n");
    }

    fn edit_of(app: &App) -> PropEdit {
        app.props.edit.clone().unwrap()
    }

    #[test]
    fn operators_text_objects_and_visual_mode() {
        let mut app = with_props();
        lyrics_text(&mut app, "one two three\nfour five\n\nsix seven");
        app.set_property_tab(PropTab::Lyrics);
        keys(&mut app, "<leader>P");
        let text = |app: &App| edit_of(app).text;
        let words = |app: &App| edit_of(app).text.replace('\n', "|");
        keys(&mut app, "wdiw");
        assert_eq!(words(&app), "one  three|four five||six seven");
        keys(&mut app, "u0wdaw");
        assert_eq!(words(&app), "one three|four five||six seven");
        keys(&mut app, "ggcwONE<Esc>");
        assert_eq!(words(&app), "ONE three|four five||six seven");
        keys(&mut app, "wdw");
        assert_eq!(words(&app), "ONE |four five||six seven");
        // Paragraphs: dap takes the blank lines after it; dip leaves them.
        keys(&mut app, "ggdap");
        assert_eq!(text(&app), "six seven");
        keys(&mut app, "u");
        keys(&mut app, "ggdip");
        assert_eq!(words(&app), "|six seven");
        keys(&mut app, "u");
        // dd, yy and p are lines; D and x.
        keys(&mut app, "ddp");
        assert_eq!(words(&app), "four five|ONE ||six seven");
        keys(&mut app, "ggyyGp");
        assert_eq!(words(&app), "four five|ONE ||six seven|four five");
        // Visual mode: v with a motion and an object, V for lines.
        keys(&mut app, "ggvee");
        assert_eq!(app.display_mode(), Mode::Visual);
        assert_eq!(edit_of(&app).selected_text().as_deref(), Some("four five"));
        keys(&mut app, "d");
        assert_eq!(words(&app), "|ONE ||six seven|four five");
        assert_eq!(app.display_mode(), Mode::Normal);
        keys(&mut app, "jviwy");
        assert_eq!(crate::clipboard::get(), "ONE");
        keys(&mut app, "Vjd");
        assert_eq!(words(&app), "|six seven|four five");
        keys(&mut app, "vawU");
        assert_eq!(words(&app), "|SIX seven|four five");
        keys(&mut app, "v<Esc>");
        assert!(edit_of(&app).visual.is_none());
    }

    #[test]
    fn ctrl_arrows_and_ctrl_delete_work_by_words_in_insert_mode() {
        let mut app = with_props();
        lyrics_text(&mut app, "one two three");
        app.set_property_tab(PropTab::Lyrics);
        keys(&mut app, "<leader>P<CR><End>");
        keys(&mut app, "<C-Left>");
        assert_eq!(edit_of(&app).caret, 8);
        keys(&mut app, "<C-Left><C-Right>");
        assert_eq!(edit_of(&app).caret, 8);
        keys(&mut app, "<C-Left><C-Left>");
        assert_eq!(edit_of(&app).caret, 0);
        keys(&mut app, "<C-Del>");
        assert_eq!(edit_of(&app).text, " two three");
        keys(&mut app, "<C-Right><C-BS>");
        assert_eq!(
            edit_of(&app).text,
            "two three",
            "the blank before the word goes"
        );
        keys(&mut app, "<C-Right><C-BS>");
        assert_eq!(edit_of(&app).text, "three");
    }

    #[test]
    fn the_text_area_gives_up_the_focus_and_gets_it_back_as_left() {
        let mut app = with_props();
        lyrics_text(&mut app, "one two three");
        app.set_property_tab(PropTab::Lyrics);
        let score = app.focused();
        keys(&mut app, "<leader>P$");
        let caret = edit_of(&app).caret;
        // A focus key bound in the normal keymap leaves the pane from the text area.
        let km = crate::keys::parse_keys("<c-h>", &[]).unwrap();
        app.config
            .keymaps
            .get_mut(&Mode::Normal)
            .unwrap()
            .bindings
            .insert(km, vec![Command::parse("focus-left").unwrap()]);
        keys(&mut app, "<C-h>");
        assert!(app.props.edit.is_none());
        let _ = score;
        keys(&mut app, "<leader>P");
        assert_eq!(app.focused_kind(), PaneKind::Properties);
        assert_eq!(edit_of(&app).caret, caret, "the caret is where it was");
    }

    #[test]
    fn h_and_l_change_tabs_from_the_text_area() {
        let mut app = with_props();
        app.set_property_tab(PropTab::Lyrics);
        keys(&mut app, "<leader>P");
        assert!(app.props.edit.is_some());
        keys(&mut app, "H");
        assert_eq!(app.props.tab, PropTab::Tracks);
        keys(&mut app, "L");
        assert_eq!(app.props.tab, PropTab::Lyrics);
    }
}
