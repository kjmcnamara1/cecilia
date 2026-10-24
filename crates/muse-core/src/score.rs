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
    pub extras: Extras,
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
    pub mute: bool,
    pub solo: bool,
    /// One bar per master bar. On a chant track these hold the transcription of `chant`.
    pub bars: Vec<Bar>,
    /// Chant notation (chant tracks only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chant: Option<Box<crate::chant::Chant>>,
    pub extras: Extras,
}

impl Track {
    /// Sounding MIDI pitch of a note.
    pub fn pitch_of(&self, note: &Note) -> u8 {
        if self.kind != TrackKind::Fretted {
            return note.fret.min(127);
        }
        let open = self.strings.get(note.string as usize).copied().unwrap_or(0) as u32;
        (open + self.capo as u32 + note.fret as u32).min(127) as u8
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

    pub fn total_ticks(&self) -> u32 {
        self.master_bars.iter().map(MasterBar::ticks).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
