use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Duration, DurationValue, TICKS_PER_QUARTER};

/// Unsupported source-format data kept for lossless round-trips, keyed by `format.field`.
pub type Extras = BTreeMap<String, String>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub meta: Metadata,
    /// Initial tempo in quarter notes per minute.
    pub tempo: u32,
    pub master_bars: Vec<MasterBar>,
    pub tracks: Vec<Track>,
    pub lyrics: Option<Lyrics>,
    /// The first bar is a pickup (anacrusis) lasting only as long as its contents.
    pub anacrusis: bool,
    /// Audio track synchronised to the score.
    pub audio: Option<AudioSync>,
    /// Page size, margins and the header and footer templates (printing).
    #[serde(default)]
    pub page: PageSetup,
    pub extras: Extras,
}

/// Page layout of the printed score, as Guitar Pro keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageSetup {
    /// Page size in millimetres.
    pub width: u32,
    pub height: u32,
    /// Margins in millimetres.
    pub margin_left: u32,
    pub margin_right: u32,
    pub margin_top: u32,
    pub margin_bottom: u32,
    /// Size of the score on the page, in percent.
    pub scale: u32,
    /// Which of the header and footer templates are shown (Guitar Pro's flag bits).
    pub header_flags: u16,
    /// Title, subtitle, artist, album, words, music, words and music, copyright (two lines)
    /// and page number templates, with `%TITLE%`-style placeholders.
    pub templates: [String; 10],
}

impl Default for PageSetup {
    fn default() -> Self {
        Self {
            width: 210,
            height: 297,
            margin_left: 10,
            margin_right: 10,
            margin_top: 15,
            margin_bottom: 10,
            scale: 100,
            header_flags: 0x01FF,
            templates: [
                "%TITLE%",
                "%SUBTITLE%",
                "%ARTIST%",
                "%ALBUM%",
                "Words by %WORDS%",
                "Music by %MUSIC%",
                "Words & Music by %WORDSMUSIC%",
                "Copyright %COPYRIGHT%",
                "All Rights Reserved - International Copyright Secured",
                "Page %N%/%P%",
            ]
            .map(String::from),
        }
    }
}

/// A recorded audio track and the points that align it with the score.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AudioSync {
    /// Path of the audio file, relative to the score file or inside its container.
    pub file: Option<String>,
    /// Sample offset of the audio relative to the start of the score (44.1 kHz frames).
    pub frame_padding: i64,
    pub points: Vec<SyncPoint>,
    /// Audio bytes stored inside the score file (GP7/8 backing tracks, `.muse` archives);
    /// `file` then names the entry. Not part of the JSON model.
    #[serde(skip)]
    pub embedded: Option<std::sync::Arc<Vec<u8>>>,
}

/// Aligns the start of a bar occurrence (in playback order) with a position in the audio.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SyncPoint {
    pub bar: usize,
    /// Which repetition of the bar (0 = first time it is played).
    pub occurrence: u32,
    /// Audio position in 44.1 kHz frames.
    pub frame: i64,
    /// Tempo of the recording from this point, in quarter notes per minute.
    pub tempo: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub album: String,
    pub words: String,
    pub music: String,
    pub copyright: String,
    pub tab_author: String,
    pub instructions: String,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }
}

impl TimeSignature {
    pub fn bar_ticks(&self) -> u32 {
        let den = self.denominator.max(1) as u32;
        self.numerator as u32 * TICKS_PER_QUARTER * 4 / den
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeySignature {
    /// Number of sharps (positive) or flats (negative).
    pub accidentals: i8,
    pub minor: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TripletFeel {
    #[default]
    None,
    Eighth,
    Sixteenth,
    DottedEighth,
    DottedSixteenth,
    ScottishEighth,
    ScottishSixteenth,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    pub title: String,
    pub color: Color,
    /// Section letter (Guitar Pro's `A`, `B` …), shown boxed before the title.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub letter: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Coda,
    DoubleCoda,
    Segno,
    SegnoSegno,
    Fine,
    DaCapo,
    DaCapoAlCoda,
    DaCapoAlDoubleCoda,
    DaCapoAlFine,
    DaSegno,
    DaSegnoAlCoda,
    DaSegnoAlDoubleCoda,
    DaSegnoAlFine,
    DaSegnoSegno,
    DaSegnoSegnoAlCoda,
    DaSegnoSegnoAlDoubleCoda,
    DaSegnoSegnoAlFine,
    DaCoda,
    DaDoubleCoda,
}

/// Bar properties shared by all tracks.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MasterBar {
    pub time_signature: TimeSignature,
    pub key_signature: KeySignature,
    pub repeat_start: bool,
    /// Total number of plays when this bar closes a repeat (2 = play twice).
    pub repeat_end: Option<u8>,
    /// Bit mask of alternate endings this bar belongs to (bit 0 = 1st ending).
    pub alternate_endings: u8,
    pub marker: Option<Marker>,
    pub directions: Vec<Direction>,
    pub triplet_feel: TripletFeel,
    pub double_bar: bool,
    /// Tempo changes within this bar, ordered by tick.
    pub tempo_changes: Vec<TempoChange>,
    /// Free time: the bar lasts as long as its contents, regardless of time signature.
    pub free_time: bool,
}

/// A tempo change at a tick offset within a bar.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TempoChange {
    pub tick: u32,
    /// Quarter notes per minute.
    pub bpm: f64,
}

impl MasterBar {
    pub fn ticks(&self) -> u32 {
        self.time_signature.bar_ticks()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    #[default]
    Fretted,
    Drums,
    Pitched,
    Chant,
}

/// Clef of a standard staff, when the source file names one (otherwise chosen from the range).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StaffClef {
    Treble,
    Bass,
    /// Treble clef sounding an octave lower than written (guitar notation).
    Treble8vb,
    /// Bass clef sounding an octave lower than written (bass guitar notation).
    Bass8vb,
}

impl StaffClef {
    pub fn is_bass(self) -> bool {
        matches!(self, StaffClef::Bass | StaffClef::Bass8vb)
    }

    /// Whether the music sounds an octave below where it is written.
    pub fn octave_down(self) -> bool {
        matches!(self, StaffClef::Treble8vb | StaffClef::Bass8vb)
    }

    pub fn new(bass: bool, octave_down: bool) -> Self {
        match (bass, octave_down) {
            (false, false) => StaffClef::Treble,
            (true, false) => StaffClef::Bass,
            (false, true) => StaffClef::Treble8vb,
            (true, true) => StaffClef::Bass8vb,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiSettings {
    pub program: u8,
    pub channel: u8,
    pub effect_channel: u8,
    pub bank: u8,
    /// 0..=127
    pub volume: u8,
    /// 0..=127, 64 = centre
    pub pan: u8,
}

impl Default for MidiSettings {
    fn default() -> Self {
        Self {
            program: 25,
            channel: 0,
            effect_channel: 1,
            bank: 0,
            volume: 100,
            pan: 64,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub name: String,
    pub kind: TrackKind,
    /// Open-string MIDI pitches, index 0 = highest-sounding string (string 1 in tab).
    pub strings: Vec<u8>,
    pub fret_count: u8,
    pub capo: u8,
    pub midi: MidiSettings,
    pub color: Color,
    /// Colour chosen in muse for the tracks panel (otherwise the theme gives one). Kept apart
    /// from `color`, which files set to a default nobody chose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_color: Option<Color>,
    pub mute: bool,
    pub solo: bool,
    /// One bar per master bar. On a chant track these hold the transcription of `chant`.
    pub bars: Vec<Bar>,
    /// A further staff of the part begun by the previous track, as files store multi-staff
    /// instruments (a piano's left hand). Only readers and writers see such tracks: they are
    /// merged into one track after reading ([`Score::merge_staves`]) and split again for
    /// writing ([`Score::split_grand`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub extra_staff: bool,
    /// Clef of the standard staff when the source file sets one that differs from the default
    /// ([`Track::staff_clef`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clef: Option<StaffClef>,
    /// Chant notation (chant tracks only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chant: Option<Box<crate::chant::Chant>>,
    /// The staves showing the track's notes, top first; empty means the default
    /// ([`Track::default_staves`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub staves: Vec<Staff>,
    /// Lyrics attached to the staves, by staff position (shorter than `staves` means none for
    /// the rest). Without any attached lyrics a track shows its own below its top staff.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub staff_lyrics: Vec<Option<StaffLyrics>>,
    pub extras: Extras,
}

/// A staff showing a track's notes; a track has one or more, top first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Staff {
    /// Standard notation.
    Standard(Clef),
    /// Tablature without rhythm.
    Tab,
    /// Tablature with rhythm stems below it.
    TabRhythm,
    /// Rhythm slashes.
    Slash,
    /// Gregorian square notation, on a do (C) clef.
    Chant,
    /// Gregorian square notation, on a fa (F) clef.
    ChantFa,
}

/// The kind of notation a staff holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Notation {
    Standard,
    Tab,
    Slash,
    Chant,
}

impl Notation {
    /// Every notation, as typed after `:notation`.
    pub const NAMES: [&str; 5] = ["standard", "tab", "tab+", "slash", "chant"];
}

/// Lyrics shown with a staff: those of any track (the staff's own by default), aligned with the
/// beats of the staff, above or below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaffLyrics {
    /// Track the lyrics come from; `None` is the staff's own track.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<usize>,
    /// Above the staff rather than below it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub above: bool,
}

/// Clef of a standard staff. Fretted tracks read an octave above their sound (the small 8
/// under the clef).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Clef {
    Treble,
    Bass,
    /// The percussion clef of drum staves: lines carry no pitch.
    Neutral,
    /// Treble and bass staves joined by a brace: notes from `split` (a MIDI pitch) up on the
    /// treble staff, lower ones on the bass staff.
    Grand {
        split: u8,
    },
}

/// Default split of a grand staff: C3 (MIDI 48).
pub const GRAND_SPLIT: u8 = 48;

impl Clef {
    pub fn name(self) -> String {
        match self {
            Clef::Treble => "treble".into(),
            Clef::Bass => "bass".into(),
            Clef::Neutral => "neutral".into(),
            Clef::Grand { split } if split == GRAND_SPLIT => "grand".into(),
            Clef::Grand { split } => format!("grand {}", pitch_name(split)),
        }
    }
}

/// Scientific pitch name of a MIDI pitch (middle C = C4), with sharps.
pub fn pitch_name(midi: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!("{}{}", NAMES[midi as usize % 12], midi as i32 / 12 - 1)
}

/// MIDI pitch of a note name such as `C4`, `F#3`, `Bb2` (middle C = C4), or a number.
pub fn parse_pitch(s: &str) -> Option<u8> {
    if let Ok(n) = s.parse::<u8>() {
        return (n <= 127).then_some(n);
    }
    let mut chars = s.chars();
    let step = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest: String = chars.collect();
    let (alter, octave) = match rest.chars().next() {
        Some('#') => (1, &rest[1..]),
        Some('b') => (-1, &rest[1..]),
        _ => (0, rest.as_str()),
    };
    let octave: i32 = octave.parse().ok()?;
    u8::try_from((octave + 1) * 12 + step + alter).ok()
}

impl Staff {
    /// Every kind, as typed: `standard`, `tab`, `tab+`, `slash`, `chant`.
    pub const KINDS: [&str; 8] = [
        "treble", "bass", "grand", "neutral", "tab", "tab+", "slash", "chant",
    ];

    /// Parses `treble`, `bass`, `grand [split note]`, `tab`, `tab+`, `slash` or `chant` (standard
    /// notation is `standard [clef]`, which still works; `standard` alone takes `clef`).
    pub fn parse(s: &str, clef: Clef) -> Result<Staff, String> {
        let words: Vec<&str> = s.split_whitespace().collect();
        let grand = |rest: &[&str]| -> Result<Clef, String> {
            Ok(match rest {
                [] => Clef::Grand { split: GRAND_SPLIT },
                [note] => Clef::Grand {
                    split: parse_pitch(note)
                        .ok_or_else(|| format!("'{note}' is not a note (e.g. C4, F#3)"))?,
                },
                _ => return Err("grand takes a split note (e.g. C4)".into()),
            })
        };
        Ok(match words.as_slice() {
            ["standard" | "std"] => Staff::Standard(clef),
            ["standard" | "std", rest @ ..] => return Staff::parse(&rest.join(" "), clef),
            ["treble"] => Staff::Standard(Clef::Treble),
            ["bass"] => Staff::Standard(Clef::Bass),
            ["neutral"] => Staff::Standard(Clef::Neutral),
            ["grand", rest @ ..] => Staff::Standard(grand(rest)?),
            ["tab"] => Staff::Tab,
            ["tab+"] => Staff::TabRhythm,
            ["slash"] => Staff::Slash,
            ["chant"] | ["chant", "do"] => Staff::Chant,
            ["chant", "fa"] => Staff::ChantFa,
            _ => {
                return Err(format!(
                    "'{s}' is not a staff (treble, bass, grand [split], neutral, tab, tab+, slash, chant [do|fa])"
                ));
            }
        })
    }

    /// Parses several staves in order, e.g. `treble tab grand C4 slash`: a staff kind starts the
    /// next one, any other word belongs to the staff before it (the split note of `grand`).
    pub fn parse_list(s: &str, clef: Clef) -> Result<Vec<Staff>, String> {
        let starts = |w: &str| Staff::KINDS.contains(&w) || matches!(w, "standard" | "std");
        let mut specs: Vec<Vec<&str>> = Vec::new();
        for w in s.split_whitespace() {
            match specs.last_mut() {
                Some(last) if !starts(w) => last.push(w),
                _ => specs.push(vec![w]),
            }
        }
        specs
            .iter()
            .map(|words| Staff::parse(&words.join(" "), clef))
            .collect()
    }

    pub fn name(self) -> String {
        match self {
            Staff::Standard(c) => c.name(),
            Staff::Tab => "tab".into(),
            Staff::TabRhythm => "tab+".into(),
            Staff::Slash => "slash".into(),
            Staff::Chant => "chant".into(),
            Staff::ChantFa => "chant fa".into(),
        }
    }

    pub fn notation(self) -> Notation {
        match self {
            Staff::Standard(_) => Notation::Standard,
            Staff::Tab | Staff::TabRhythm => Notation::Tab,
            Staff::Slash => Notation::Slash,
            Staff::Chant | Staff::ChantFa => Notation::Chant,
        }
    }

    pub fn is_chant(self) -> bool {
        self.notation() == Notation::Chant
    }

    /// The family of a chant staff's clef (do or fa).
    pub fn chant_family(self) -> Option<crate::chant::ClefKind> {
        match self {
            Staff::Chant => Some(crate::chant::ClefKind::Do),
            Staff::ChantFa => Some(crate::chant::ClefKind::Fa),
            _ => None,
        }
    }

    pub fn is_tab(self) -> bool {
        matches!(self, Staff::Tab | Staff::TabRhythm)
    }
}

/// Chant signs kept with a note, for chant staves of ordinary tracks.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChantMarks {
    /// Punctum inclinatum (diamond).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inclinatum: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub initio_debilis: bool,
    /// The note begins a new neume.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub neume_start: bool,
    /// GABC modifiers (`v` `s` `o` `q` `w` `~` `.` `_` `'` …), in written order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<String>,
}

impl Track {
    /// "Tuning: E A D G B E" (low string first), with ", capo N", for fretted tracks.
    pub fn tuning_text(&self) -> Option<String> {
        const NAMES: [&str; 12] = [
            "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
        ];
        if self.kind != TrackKind::Fretted || self.strings.is_empty() {
            return None;
        }
        let notes: Vec<&str> = self
            .strings
            .iter()
            .rev()
            .map(|&p| NAMES[p as usize % 12])
            .collect();
        let mut s = format!("Tuning: {}", notes.join(" "));
        if self.capo > 0 {
            s.push_str(&format!(", capo {}", self.capo));
        }
        Some(s)
    }

    /// Sounding MIDI pitch of a note.
    pub fn pitch_of(&self, note: &Note) -> u8 {
        if self.kind != TrackKind::Fretted {
            return note.fret.min(127);
        }
        let open = self.strings.get(note.string as usize).copied().unwrap_or(0) as u32;
        (open + self.capo as u32 + note.fret as u32).min(127) as u8
    }

    /// Clef of the standard staff: the one from the source file, else for fretted instruments
    /// the octave-down treble clef, or bass clef when the highest string is low (bass
    /// guitars); for other parts treble or bass by how low their notes are.
    /// The staves shown when none were chosen: chant as chant; fretted tracks as standard
    /// notation over tab; others as standard notation in the clef that suits their range.
    pub fn default_staves(&self) -> Vec<Staff> {
        if self.chant.is_some() {
            return vec![Staff::Chant];
        }
        if self.kind == TrackKind::Drums {
            return vec![Staff::Standard(Clef::Neutral)];
        }
        let clef = if self.staff_clef().is_bass() {
            Clef::Bass
        } else {
            Clef::Treble
        };
        if self.kind == TrackKind::Fretted && !self.strings.is_empty() {
            vec![Staff::Standard(clef), Staff::Tab]
        } else {
            vec![Staff::Standard(clef)]
        }
    }

    /// The track's staves, top first (never empty).
    pub fn staves(&self) -> Vec<Staff> {
        if self.staves.is_empty() {
            self.default_staves()
        } else {
            self.staves.clone()
        }
    }

    /// The staves, made explicit so they can be changed.
    pub fn staves_mut(&mut self) -> &mut Vec<Staff> {
        if self.staves.is_empty() {
            self.staves = self.default_staves();
        }
        &mut self.staves
    }

    /// The lyrics attached to staff `i`.
    pub fn staff_lyrics_of(&self, i: usize) -> Option<StaffLyrics> {
        self.staff_lyrics.get(i).copied().flatten()
    }

    /// Attaches lyrics to staff `i` (or detaches them).
    pub fn set_staff_lyrics(&mut self, i: usize, lyrics: Option<StaffLyrics>) {
        if self.staff_lyrics.len() <= i {
            self.staff_lyrics.resize(i + 1, None);
        }
        self.staff_lyrics[i] = lyrics;
        while self.staff_lyrics.last() == Some(&None) {
            self.staff_lyrics.pop();
        }
    }

    /// Whether any staff of the track has lyrics attached.
    pub fn has_attached_lyrics(&self) -> bool {
        self.staff_lyrics.iter().any(Option::is_some)
    }

    /// Removes staff `i` and its lyrics attachment.
    pub fn remove_staff(&mut self, i: usize) {
        self.staves_mut().remove(i);
        if i < self.staff_lyrics.len() {
            self.staff_lyrics.remove(i);
        }
        while self.staff_lyrics.last() == Some(&None) {
            self.staff_lyrics.pop();
        }
    }

    /// Moves staff `from` to position `to`, taking its lyrics attachment along.
    pub fn move_staff(&mut self, from: usize, to: usize) {
        let lyrics = self.staff_lyrics_of(from);
        let n = self.staves_mut().len();
        let mut attached: Vec<Option<StaffLyrics>> =
            (0..n).map(|k| self.staff_lyrics_of(k)).collect();
        let staff = self.staves_mut().remove(from);
        attached.remove(from);
        self.staves_mut().insert(to, staff);
        attached.insert(to, lyrics);
        self.staff_lyrics = attached;
        while self.staff_lyrics.last() == Some(&None) {
            self.staff_lyrics.pop();
        }
    }

    /// The family of clef the track's chant staff uses, if it has one.
    pub fn chant_family(&self) -> Option<crate::chant::ClefKind> {
        self.staves().iter().find_map(|s| s.chant_family())
    }

    /// The clef a standard staff with `clef` shows (one half of a grand staff for `bass`).
    pub fn written_clef(&self, clef: Clef, bass: bool) -> StaffClef {
        let octave_down = self.staff_clef().octave_down();
        let bass = match clef {
            Clef::Treble | Clef::Neutral => false,
            Clef::Bass => true,
            Clef::Grand { .. } => bass,
        };
        StaffClef::new(bass, octave_down)
    }

    pub fn staff_clef(&self) -> StaffClef {
        if let Some(c) = self.clef {
            return c;
        }
        if self.kind == TrackKind::Fretted {
            // A 7-string guitar's low B is lower than a bass's top string is high.
            let top = self.strings.iter().copied().max().unwrap_or(64);
            return StaffClef::new(top < 52, true);
        }
        let pitches: Vec<u32> = self
            .bars
            .iter()
            .flat_map(|b| &b.voices)
            .flat_map(|v| &v.beats)
            .flat_map(|b| &b.notes)
            .map(|n| n.fret as u32)
            .collect();
        let avg = pitches.iter().sum::<u32>() / pitches.len().max(1) as u32;
        if !pitches.is_empty() && avg < 55 {
            StaffClef::Bass
        } else {
            StaffClef::Treble
        }
    }

    pub fn is_percussion(&self) -> bool {
        self.kind == TrackKind::Drums
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    pub voices: Vec<Voice>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Voice {
    pub beats: Vec<Beat>,
}

impl Voice {
    pub fn is_empty(&self) -> bool {
        self.beats.iter().all(|b| b.empty)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Beat {
    pub duration: Duration,
    /// An empty beat occupies no time (placeholder in an unused voice).
    pub empty: bool,
    pub notes: Vec<Note>,
    pub text: Option<String>,
    pub chord: Option<Chord>,
    pub effects: BeatEffects,
    /// Lyric syllables, one per lyric line (empty strings for lines without a syllable here).
    pub lyrics: Vec<String>,
    /// A chant division after this beat (`,` `;` `:` `::`), for chant staves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chant_division: Option<String>,
}

impl Beat {
    pub fn rest(duration: Duration) -> Self {
        Self {
            duration,
            ..Self::default()
        }
    }

    pub fn is_rest(&self) -> bool {
        !self.empty && self.notes.is_empty()
    }

    /// Ticks this beat advances the voice by.
    pub fn ticks(&self) -> u32 {
        if self.empty { 0 } else { self.duration.ticks() }
    }

    pub fn note_on_string(&self, string: u8) -> Option<&Note> {
        self.notes.iter().find(|n| n.string == string)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeDirection {
    #[default]
    None,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stroke {
    pub direction: StrokeDirection,
    /// Total time over which the strings are struck.
    pub duration: DurationValue,
    /// An arpeggio (a rolled chord, drawn wavy) rather than a brush stroke.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub arpeggio: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Slap {
    #[default]
    None,
    Tap,
    Slap,
    Pop,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ottava {
    #[default]
    None,
    Ottava,
    OttavaBassa,
    Quindicesima,
    QuindicesimaBassa,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BeatEffects {
    pub vibrato: bool,
    pub fade_in: bool,
    pub slap: Slap,
    pub stroke: Option<Stroke>,
    pub pick_stroke: StrokeDirection,
    pub rasgueado: bool,
    pub tremolo_bar: Option<Bend>,
    pub ottava: Ottava,
    /// Crescendo or decrescendo through this beat; consecutive beats form one hairpin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hairpin: Option<Hairpin>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hairpin {
    Crescendo,
    Decrescendo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Barre {
    pub fret: u8,
    pub first_string: u8,
    pub last_string: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chord {
    pub name: String,
    pub first_fret: u8,
    /// Fret per string (index 0 = string 1); `None` = not played.
    pub frets: Vec<Option<u8>>,
    pub barres: Vec<Barre>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// String index, 0 = highest string. For drums this is the drum lane.
    pub string: u8,
    /// Fret on fretted tracks; the MIDI key on drum and other non-fretted tracks.
    pub fret: u8,
    /// MIDI velocity 1..=127.
    pub velocity: u8,
    pub tie: bool,
    pub dead: bool,
    pub ghost: bool,
    pub accent: Accent,
    pub effects: NoteEffects,
    /// Playback length as a fraction of the written duration (1.0 = full).
    pub duration_percent: f32,
    /// Chant signs, when the track has a chant staff.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chant: Option<Box<ChantMarks>>,
}

impl Note {
    pub fn new(string: u8, fret: u8) -> Self {
        Self {
            string,
            fret,
            velocity: DEFAULT_VELOCITY,
            duration_percent: 1.0,
            ..Self::default()
        }
    }
}

pub const DEFAULT_VELOCITY: u8 = 95;

/// Dynamic marking for a velocity, in Guitar Pro's upper-case names ("PPP" to "FFF").
pub fn dynamic_name(velocity: u8) -> &'static str {
    match velocity {
        0..=22 => "PPP",
        23..=38 => "PP",
        39..=54 => "P",
        55..=70 => "MP",
        71..=86 => "MF",
        87..=102 => "F",
        103..=118 => "FF",
        _ => "FFF",
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Accent {
    #[default]
    None,
    Normal,
    Heavy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlideKind {
    Shift,
    Legato,
    OutDown,
    OutUp,
    IntoFromBelow,
    IntoFromAbove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Harmonic {
    Natural,
    /// Artificial harmonic sounding `semitones` above the fretted note.
    Artificial {
        semitones: u8,
    },
    Tapped {
        fret: u8,
    },
    Pinch,
    Semi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BendPoint {
    /// Position along the note, 0..=60.
    pub position: u8,
    /// Pitch offset in quarter tones (4 = whole tone).
    pub value: i16,
    pub vibrato: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bend {
    pub points: Vec<BendPoint>,
}

impl Bend {
    pub const MAX_POSITION: u8 = 60;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraceTransition {
    #[default]
    None,
    Slide,
    Bend,
    Hammer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grace {
    pub fret: u8,
    pub velocity: u8,
    pub duration: DurationValue,
    pub dead: bool,
    pub on_beat: bool,
    pub transition: GraceTransition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trill {
    pub fret: u8,
    pub speed: DurationValue,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NoteEffects {
    pub bend: Option<Bend>,
    pub slides: Vec<SlideKind>,
    pub hammer_pull: bool,
    pub let_ring: bool,
    pub palm_mute: bool,
    pub staccato: bool,
    pub vibrato: bool,
    pub harmonic: Option<Harmonic>,
    pub grace: Option<Grace>,
    pub tremolo_picking: Option<DurationValue>,
    pub trill: Option<Trill>,
    pub left_finger: Option<u8>,
    pub right_finger: Option<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricLine {
    /// Zero-based bar where the line starts.
    pub start_bar: u32,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lyrics {
    pub track: Option<usize>,
    pub lines: Vec<LyricLine>,
}

impl Score {
    /// Played length of a bar: the time signature length, or the length of the longest voice
    /// for free-time bars and a pickup first bar.
    pub fn played_ticks(&self, bar: usize) -> u32 {
        let mb = &self.master_bars[bar];
        if !(mb.free_time || (bar == 0 && self.anacrusis)) {
            return mb.ticks();
        }
        let content = self
            .tracks
            .iter()
            .filter_map(|t| t.bars.get(bar))
            .flat_map(|b| &b.voices)
            .map(|v| v.beats.iter().map(Beat::ticks).sum::<u32>())
            .max()
            .unwrap_or(0);
        if content == 0 { mb.ticks() } else { content }
    }

    /// Start tick of every master bar (written order, ignoring repeats).
    pub fn bar_starts(&self) -> Vec<u32> {
        let mut t = 0;
        self.master_bars
            .iter()
            .map(|mb| {
                let s = t;
                t += mb.ticks();
                s
            })
            .collect()
    }

    /// The tracks forming the part that `track` belongs to (its staves).
    pub fn part_tracks(&self, track: usize) -> std::ops::Range<usize> {
        let n = self.tracks.len();
        if track >= n {
            return track..track;
        }
        let mut start = track;
        while start > 0 && self.tracks[start].extra_staff {
            start -= 1;
        }
        let mut end = track + 1;
        while end < n && self.tracks[end].extra_staff {
            end += 1;
        }
        start..end
    }

    /// Folds each multi-staff instrument read from a file (a track followed by `extra_staff`
    /// tracks) into one track: the further staves' voices join its bars, and a pitched
    /// instrument is shown on a grand staff.
    pub fn merge_staves(&mut self) {
        let mut t = 0;
        while t < self.tracks.len() {
            let part = self.part_tracks(t);
            if part.len() > 1 {
                let extra: Vec<Track> = self.tracks.drain(part.start + 1..part.end).collect();
                let first = &mut self.tracks[part.start];
                for staff in extra {
                    for (bar, more) in first.bars.iter_mut().zip(staff.bars) {
                        // Placeholder voices (only empty beats) are left out.
                        bar.voices.extend(
                            more.voices
                                .into_iter()
                                .filter(|v| v.beats.iter().any(|b| !b.empty)),
                        );
                    }
                }
                if first.kind == TrackKind::Pitched {
                    first.staves = vec![Staff::Standard(Clef::Grand { split: GRAND_SPLIT })];
                    first.clef = None;
                }
                // Tracks after the merged ones move up.
                let removed = part.len() - 1;
                if let Some(l) = self.lyrics.as_mut() {
                    l.track = l.track.map(|i| match i {
                        i if i >= part.end => i - removed,
                        i if i > part.start => part.start,
                        i => i,
                    });
                }
            }
            t = part.start + 1;
        }
    }

    /// The score as files store it: a track on a grand staff becomes two tracks of one part
    /// (treble, then bass as an `extra_staff`), its notes split at the grand staff's split.
    /// A voice with no notes on one side is left out there.
    pub fn split_grand(&self) -> Score {
        let mut out = self.clone();
        let mut t = 0;
        while t < out.tracks.len() {
            let split = out.tracks[t].staves().iter().find_map(|s| match s {
                Staff::Standard(Clef::Grand { split }) => Some(*split),
                _ => None,
            });
            let Some(split) = split.filter(|_| out.tracks[t].kind != TrackKind::Drums) else {
                t += 1;
                continue;
            };
            let both = out.tracks[t].clone();
            let pitch = |n: &Note| both.pitch_of(n);
            let half = |high: bool| -> Track {
                let mut tr = both.clone();
                let keep = |n: &Note| (pitch(n) >= split) == high;
                // Voices with a note on this side, anywhere in the track.
                let voices = tr.bars.iter().map(|b| b.voices.len()).max().unwrap_or(0);
                let used: Vec<bool> = (0..voices)
                    .map(|v| {
                        tr.bars.iter().any(|b| {
                            b.voices
                                .get(v)
                                .is_some_and(|v| v.beats.iter().flat_map(|b| &b.notes).any(keep))
                        })
                    })
                    .collect();
                for bar in &mut tr.bars {
                    let mut kept: Vec<Voice> = Vec::new();
                    for (v, voice) in std::mem::take(&mut bar.voices).into_iter().enumerate() {
                        if used.get(v) == Some(&true) {
                            let mut voice = voice;
                            for beat in &mut voice.beats {
                                beat.notes.retain(keep);
                            }
                            kept.push(voice);
                        }
                    }
                    if kept.is_empty() {
                        kept.push(Voice {
                            beats: vec![Beat {
                                empty: true,
                                ..Beat::default()
                            }],
                        });
                    }
                    bar.voices = kept;
                }
                tr.clef = Some(StaffClef::new(!high, both.staff_clef().octave_down()));
                tr.staves = vec![Staff::Standard(if high {
                    Clef::Treble
                } else {
                    Clef::Bass
                })];
                tr
            };
            let (upper, mut lower) = (half(true), half(false));
            lower.name = format!("{} (2)", both.name.trim());
            lower.extra_staff = true;
            out.tracks[t] = upper;
            out.tracks.insert(t + 1, lower);
            if let Some(l) = out.lyrics.as_mut() {
                l.track = l.track.map(|i| if i > t { i + 1 } else { i });
            }
            t += 2;
        }
        out
    }

    /// The parts of the score as track ranges, in order.
    pub fn parts(&self) -> Vec<std::ops::Range<usize>> {
        let mut out = Vec::new();
        let mut t = 0;
        while t < self.tracks.len() {
            let r = self.part_tracks(t);
            t = r.end;
            out.push(r);
        }
        out
    }

    pub fn total_ticks(&self) -> u32 {
        self.master_bars.iter().map(MasterBar::ticks).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tuning_text_reads_low_to_high_with_capo() {
        let mut t = Track {
            strings: vec![64, 59, 55, 50, 45, 38],
            ..Track::default()
        };
        assert_eq!(t.tuning_text().unwrap(), "Tuning: D A D G B E");
        t.capo = 2;
        assert_eq!(t.tuning_text().unwrap(), "Tuning: D A D G B E, capo 2");
        t.kind = TrackKind::Drums;
        assert!(t.tuning_text().is_none());
    }

    #[test]
    fn pitch_with_capo() {
        let track = Track {
            strings: vec![64, 59, 55, 50, 45, 40],
            capo: 2,
            ..Track::default()
        };
        assert_eq!(track.pitch_of(&Note::new(5, 0)), 42);
        assert_eq!(track.pitch_of(&Note::new(0, 3)), 69);
    }

    #[test]
    fn bar_ticks() {
        let ts = TimeSignature {
            numerator: 6,
            denominator: 8,
        };
        assert_eq!(ts.bar_ticks(), 2880);
    }
}
