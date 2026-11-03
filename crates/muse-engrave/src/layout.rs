//! Lays out one track as standard notation and/or tablature.

use std::collections::HashMap;

use kurbo::{BezPath, Point, Rect, Shape as _};
use muse_core::*;

use crate::display::{Align, BeatRef, Color, FontKind, Hit, Item, MarkKind, NoteMark, Page};
use crate::fonts::fonts;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    /// Paginated for printing.
    Page { width: f64, height: f64 },
    /// Systems wrap to `width` on one tall canvas.
    Vertical { width: f64 },
    /// One endless system.
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    pub mode: Mode,
    /// Staff space in points.
    pub sp: f64,
    pub standard: bool,
    pub tab: bool,
    /// Title block above the first system.
    pub header: bool,
    pub margin: f64,
    /// Show chant tracks in square notation (otherwise as their modern transcription).
    pub chant: bool,
    /// Which note of a chord a chant staff shows.
    pub chords: muse_core::melody::ChordPolicy,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            mode: Mode::Page {
                width: 595.0,
                height: 842.0,
            },
            sp: 5.0,
            standard: true,
            tab: true,
            header: true,
            margin: 40.0,
            chant: true,
            chords: Default::default(),
        }
    }
}

/// Where a beat was drawn, for cursors.
#[derive(Debug, Clone, PartialEq)]
pub struct BeatPos {
    pub page: usize,
    pub x: f64,
    pub width: f64,
    pub top: f64,
    pub bottom: f64,
    /// Centre x of the note heads of the beat (a note's square cursor goes there).
    pub head: f64,
    /// Where the beat lies on each of the track's staves, for a cursor on any of them.
    pub staves: Vec<StaffPos>,
}

/// A beat's place on one staff of its track.
#[derive(Debug, Clone, PartialEq)]
pub enum StaffPos {
    /// Lines and spaces: positions are diatonic numbers on a standard or slash staff and GABC
    /// positions on chant, at `y0 - position * step`. A grand staff gives one per half, each
    /// for the positions in `range`.
    Lines {
        staff: usize,
        y0: f64,
        step: f64,
        range: (i32, i32),
    },
    /// A tablature: the y of each string (model string order).
    Strings { staff: usize, ys: Vec<f64> },
}

impl BeatPos {
    /// Y of each string of the given staff, if it is a tab.
    pub fn strings(&self, staff: usize) -> Option<&[f64]> {
        self.staves.iter().find_map(|p| match p {
            StaffPos::Strings { staff: s, ys } if *s == staff => Some(ys.as_slice()),
            _ => None,
        })
    }

    /// Y of a position on the given staff (the half of a grand staff that shows it).
    pub fn line_y(&self, staff: usize, position: i32) -> Option<(f64, f64)> {
        self.staves.iter().find_map(|p| match p {
            StaffPos::Lines {
                staff: s,
                y0,
                step,
                range,
            } if *s == staff && (range.0..=range.1).contains(&position) => {
                Some((y0 - position as f64 * step, *step))
            }
            _ => None,
        })
    }

    /// The staff position under a y on the given staff.
    pub fn position_at(&self, staff: usize, y: f64) -> Option<i32> {
        self.staves.iter().find_map(|p| match p {
            StaffPos::Lines {
                staff: s,
                y0,
                step,
                range,
            } if *s == staff => {
                let pos = ((y0 - y) / step).round() as i32;
                (range.0..=range.1).contains(&pos).then_some(pos)
            }
            _ => None,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub pages: Vec<Page>,
    pub beats: HashMap<BeatRef, BeatPos>,
    /// Page index and x range of every bar.
    pub bars: Vec<(usize, f64, f64)>,
}

// SMuFL code points.
const G_CLEF: char = '\u{E050}';
const G_CLEF_8VB: char = '\u{E052}';
const F_CLEF: char = '\u{E062}';
const F_CLEF_8VB: char = '\u{E064}';
const PERC_CLEF: char = '\u{E069}';
const TAB_CLEF: char = '\u{E06D}';
const HEAD_WHOLE: char = '\u{E0A2}';
const HEAD_HALF: char = '\u{E0A3}';
const HEAD_BLACK: char = '\u{E0A4}';
const HEAD_X: char = '\u{E0A9}';
const FLAT: char = '\u{E260}';
const NATURAL: char = '\u{E261}';
const SHARP: char = '\u{E262}';
const DOT: char = '\u{E1E7}';
const METRONOME_QUARTER: char = '\u{E1D5}';
const BRACE: char = '\u{E000}';
const OPEN: char = '\u{E5E7}';
const VIBRATO: char = '\u{EAB3}';
const TIME_PAREN_LEFT: char = '\u{E094}';
const TIME_PAREN_RIGHT: char = '\u{E095}';
const SLASH_BLACK: char = '\u{E101}';
const SLASH_HALF: char = '\u{E103}';
const SLASH_WHOLE: char = '\u{E102}';
const PAREN_LEFT: char = '\u{E0F5}';
const PAREN_RIGHT: char = '\u{E0F6}';

fn time_digit(d: u32) -> char {
    char::from_u32(0xE080 + d).unwrap_or('0')
}

fn tuplet_digit(d: u32) -> char {
    char::from_u32(0xE880 + d).unwrap_or('0')
}

fn rest_glyph(v: DurationValue) -> char {
    char::from_u32(0xE4E3 + v as u32).unwrap_or('\u{E4E5}')
}

/// Number of flags/beams of a note value (eighth = 1).
fn beam_count(v: DurationValue) -> usize {
    (v as usize).saturating_sub(2)
}

fn flag_glyph(v: DurationValue, up: bool) -> char {
    let base = 0xE240 + 2 * (beam_count(v).saturating_sub(1) as u32);
    char::from_u32(base + u32::from(!up)).unwrap_or('\u{E240}')
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Clef {
    Treble8vb,
    Treble,
    Bass8vb,
    Bass,
    Percussion,
}

impl Clef {
    fn from_staff(c: StaffClef) -> Clef {
        match c {
            StaffClef::Bass => Clef::Bass,
            StaffClef::Treble => Clef::Treble,
            StaffClef::Bass8vb => Clef::Bass8vb,
            StaffClef::Treble8vb => Clef::Treble8vb,
        }
    }

    fn for_track(t: &Track) -> Clef {
        match t.kind {
            TrackKind::Drums => Clef::Percussion,
            _ => match t.staff_clef() {
                StaffClef::Bass => Clef::Bass,
                StaffClef::Treble => Clef::Treble,
                StaffClef::Bass8vb => Clef::Bass8vb,
                StaffClef::Treble8vb => Clef::Treble8vb,
            },
        }
    }

    /// Diatonic number of the bottom staff line (octave * 7 + step).
    fn bottom(self) -> i32 {
        match self {
            Clef::Treble | Clef::Treble8vb | Clef::Percussion => 4 * 7 + 2,
            Clef::Bass | Clef::Bass8vb => 2 * 7 + 4,
        }
    }

    /// Semitones added to sounding pitch to get the written pitch.
    fn transpose(self) -> i32 {
        match self {
            Clef::Treble8vb | Clef::Bass8vb => 12,
            _ => 0,
        }
    }

    fn glyph(self) -> (char, i32) {
        // Glyph and the staff position its origin sits on.
        match self {
            Clef::Treble => (G_CLEF, 2),
            Clef::Treble8vb => (G_CLEF_8VB, 2),
            Clef::Bass => (F_CLEF, 6),
            Clef::Bass8vb => (F_CLEF_8VB, 6),
            Clef::Percussion => (PERC_CLEF, 4),
        }
    }

    fn key_offset(self) -> i32 {
        match self {
            Clef::Bass | Clef::Bass8vb => -2,
            _ => 0,
        }
    }
}

use crate::neume::{self, Kind, N, Typeset, digit};
use muse_core::chant::{ChantClef, ClefKind};
use muse_core::melody::ChantView;
use muse_core::staff::{key_alters, spell};

fn drum_position(key: u8) -> (i32, bool) {
    // (staff position on the percussion staff, x notehead)
    match key {
        35 | 36 => (1, false),
        37..=40 => (5, false),
        41 | 43 => (2, false),
        45 | 47 => (6, false),
        48 | 50 => (7, false),
        42 | 46 => (9, true),
        44 => (-1, true),
        49 | 57 | 52 | 55 => (10, true),
        51 | 53 | 59 => (8, true),
        _ => (8, true),
    }
}

/// A beat's place in a measure.
struct Column {
    tick: u32,
    width: f64,
}

struct MeasurePlan {
    bar: usize,
    columns: Vec<Column>,
    /// Width before the first column (clef/key/time changes and padding).
    lead: f64,
    natural: f64,
}

struct Ctx<'a> {
    score: &'a Score,
    track: &'a Track,
    track_index: usize,
    /// All staves of the part (this one included): columns line up across them.
    part: Vec<&'a Track>,
    /// The top staff of its part draws bar numbers, markers, tempo and endings.
    first_staff: bool,
    o: Options,
    clef: Clef,
    sp: f64,
    standard: bool,
    tab: bool,
    /// Rhythm stems below the tab.
    rhythm: bool,
    /// The standard staff shows rhythm slashes instead of pitches.
    slash: bool,
    /// A four-line chant staff: the melody it shows, or why it cannot.
    chant: Option<ChantCtx>,
    /// The last chant note drawn: its event, head x and y, and system (a neume goes on from it).
    chant_set: std::cell::RefCell<Option<ChantSet>>,
    /// Index of this staff among the track's staves.
    staff_index: usize,
    /// The half of a grand staff this is: split pitch, treble half.
    half: Option<(u8, bool)>,
    /// The bottom staff of the track: dynamics and hairpins go under it.
    last_staff: bool,
    /// Vibrato marks are drawn over this staff.
    vibrato: bool,
    /// A brace joins this staff to the next (a grand staff).
    brace_next: bool,
    /// Some staff of the track shows key signatures.
    keyed: bool,
    /// Beats left out of this staff (the other half's notes on a grand staff).
    hidden: HiddenBeats,
    /// Lines of lyrics under the staff, and above it.
    lyrics: usize,
    lyrics_above: usize,
    /// Room added above the staff for lyrics above it, in the system being laid out.
    above: std::cell::Cell<f64>,
    /// Baseline of the lowest line of lyrics above the staff, from the system's top.
    lyric_rel: std::cell::Cell<f64>,
    /// Top of the system being drawn.
    origin: std::cell::Cell<f64>,
    /// Measuring the system on a scratch page: lyrics above the staff are left out.
    measuring: std::cell::Cell<bool>,
    /// Horizontal extents of the syllables above the staff found while measuring.
    lyric_spans: std::cell::RefCell<Vec<(f64, f64)>>,
    /// Right edge of the last syllable of each verse drawn in the system, for attached lyrics
    /// (which come from columns of another track): the next one starts clear of it.
    lyric_end: std::cell::RefCell<Vec<f64>>,
    /// Lyrics attached to this staff: the track they come from (they are aligned to this
    /// staff's columns by their tick in the bar). Without it a track's own lyrics go under its
    /// top staff.
    lyric_source: Option<&'a Track>,
    nh_width: f64,
    /// Syllables drawn so far, for hyphens: (page, verse y, left, right, continues).
    syllables: std::cell::RefCell<Vec<Syllable>>,
    /// Whether the track has dynamics or hairpins to show below the staves.
    dynamics: bool,
    /// Beats under a hairpin, drawn once the layout is done.
    wedges: std::cell::RefCell<Vec<Wedge>>,
    /// Extra room above the tab in the system being drawn, to clear the standard staff.
    extra: std::cell::Cell<f64>,
    /// Lowest point reached by the standard notation drawn so far.
    low: std::cell::Cell<f64>,
    /// Stems drawn in the current voice: x, end and whether up, for tuplet brackets.
    stems: std::cell::RefCell<Vec<(f64, f64, bool)>>,
}

/// A beat under a hairpin: its kind, (bar, beat) in the first voice, page, system, the beat's
/// left and right x, and the hairpin's centre line.
type Wedge = (Hairpin, (usize, usize), usize, usize, f64, f64, f64);

/// Two consecutive notes on one string: positions (bar, voice, beat), frets, and whether
/// they are tied, slurred (hammer, pull or legato slide) and joined by a slide line.
#[derive(Clone)]
struct Link {
    string: u8,
    from: (usize, usize, usize),
    to: (usize, usize, usize),
    frets: (u8, u8),
    tie: bool,
    legato: bool,
    slide: bool,
}

/// A filled slur from p0 to p1 whose middle reaches the height `peak`, `thick` at its middle.
fn slur(page: &mut Page, p0: (f64, f64), p1: (f64, f64), peak: f64, thick: f64) {
    let (x0, x1) = (p0.0, p1.0);
    // The middle of a cubic with both controls at height cy lies at (y0 + 6 cy + y1) / 8.
    let control = |peak: f64| (8.0 * peak - p0.1 - p1.1) / 6.0;
    let inner = peak
        + if peak < (p0.1 + p1.1) / 2.0 {
            thick
        } else {
            -thick
        };
    let (q0, q1) = (x0 + (x1 - x0) * 0.25, x0 + (x1 - x0) * 0.75);
    let mut p = BezPath::new();
    p.move_to(p0);
    p.curve_to((q0, control(peak)), (q1, control(peak)), p1);
    p.curve_to((q1, control(inner)), (q0, control(inner)), p0);
    page.items.push(Item::Path {
        path: p,
        color: Color::BLACK,
        stroke: None,
    });
}

/// A beat on a tab+ staff waiting for its beam: start tick, stem x, value and dots.
type RhythmBeat = (usize, f64, DurationValue, u8);

/// A note in a tuplet: its left and right x, and the heights of its noteheads with its stem
/// direction (none for a rest).
type TupletNote = (f64, f64, Option<(Vec<f64>, bool)>);

/// A drawn lyric syllable: page, baseline y, left and right x, and whether a hyphen follows.
type Syllable = (usize, f64, f64, f64, bool);

/// Note positions on the standard staff, for ties.
#[derive(Clone, Copy)]
struct Placed {
    page: usize,
    system: usize,
    x: f64,
    y: f64,
    tab_y: Option<f64>,
    above: bool,
    /// Tops of the standard and tab staves of the note's system.
    std_top: f64,
    tab_top: f64,
    /// Width of the fret label on the tab staff.
    tab_w: f64,
}

/// What a chant staff of an ordinary track shows: the track's melody as chant, or the message
/// that says why there is none (a chord under the `error` policy).
#[derive(Clone)]
struct ChantCtx {
    view: Option<ChantView>,
    error: Option<String>,
}

/// The neume last set on a chant staff: which, where its heads are and where it ends.
struct ChantSet {
    neume: usize,
    centers: Vec<f64>,
    right: f64,
}

/// How one staff of a track is drawn.
struct StaffSpec {
    /// Index among the track's staves.
    index: usize,
    standard: bool,
    tab: bool,
    /// Rhythm stems below the tab.
    rhythm: bool,
    slash: bool,
    /// Square notation on a four-line staff.
    chant: bool,
    clef: Option<StaffClef>,
    /// The half of a grand staff: its split pitch and whether it is the treble half.
    half: Option<(u8, bool)>,
}

pub fn engrave(score: &Score, track_index: usize, options: &Options) -> Layout {
    let Some(track) = score.tracks.get(track_index) else {
        return Layout::default();
    };
    let staves = track.staves();
    if let Some(chant) = track.chant.as_deref().filter(|_| options.chant) {
        if staves == [Staff::Chant] {
            return crate::chant::engrave_chant(score, track_index, track, chant, options);
        }
    }
    // Drum tracks split into hands and feet.
    let shown = display_track(track);
    let fretted = track.kind == TrackKind::Fretted && !track.strings.is_empty();
    let mut specs: Vec<StaffSpec> = Vec::new();
    for (index, staff) in staves.iter().enumerate() {
        let spec = |standard, tab, rhythm, slash, clef, half| StaffSpec {
            index,
            standard,
            tab,
            rhythm,
            slash,
            chant: false,
            clef,
            half,
        };
        match *staff {
            Staff::Standard(muse_core::Clef::Grand { split }) if track.kind != TrackKind::Drums => {
                let clef = muse_core::Clef::Grand { split };
                specs.push(spec(
                    true,
                    false,
                    false,
                    false,
                    Some(track.written_clef(clef, false)),
                    Some((split, true)),
                ));
                specs.push(spec(
                    true,
                    false,
                    false,
                    false,
                    Some(track.written_clef(clef, true)),
                    Some((split, false)),
                ));
            }
            Staff::Tab | Staff::TabRhythm if fretted => specs.push(spec(
                false,
                true,
                *staff == Staff::TabRhythm,
                false,
                None,
                None,
            )),
            Staff::Slash => specs.push(spec(
                true,
                false,
                false,
                true,
                Some(track.written_clef(muse_core::Clef::Treble, false)),
                None,
            )),
            Staff::Standard(clef) => specs.push(spec(
                true,
                false,
                false,
                false,
                Some(track.written_clef(clef, false)),
                None,
            )),
            Staff::Chant | Staff::ChantFa => specs.push(StaffSpec {
                chant: true,
                ..spec(true, false, false, false, None, None)
            }),
            // Tab without strings shows standard notation.
            _ => specs.push(spec(true, false, false, false, None, None)),
        }
    }
    if !options.tab && specs.iter().any(|s| s.standard) {
        specs.retain(|s| !s.tab);
    }
    if !options.standard && specs.iter().any(|s| s.tab) {
        specs.retain(|s| !s.standard);
    }
    // The halves of grand staves, with only their notes.
    let halves: Vec<Option<(Track, HiddenBeats)>> = specs
        .iter()
        .map(|s| s.half.map(|(split, high)| grand_half(&shown, split, high)))
        .collect();
    let tracks: Vec<&Track> = halves
        .iter()
        .map(|h| h.as_ref().map_or(shown.as_ref(), |(t, _)| t))
        .collect();
    let chant_ctx = specs.iter().any(|s| s.chant).then(|| {
        match ChantView::build(score, track, options.chords) {
            Ok(view) => ChantCtx {
                view: Some(view),
                error: None,
            },
            Err(e) => ChantCtx {
                view: None,
                error: Some(e.to_string()),
            },
        }
    });
    let keyed = specs
        .iter()
        .any(|s| s.standard && !s.slash && !s.chant && track.kind != TrackKind::Drums);
    // Vibrato marks go over the tab if there is one, else over the top staff.
    let vibrato = specs.iter().position(|s| s.tab).unwrap_or(0);
    let n = specs.len();
    let ctxs: Vec<Ctx> = specs
        .iter()
        .enumerate()
        .map(|(k, spec)| {
            // Lyrics attached to a staff go above its first part or under its last.
            let role = if !track.has_attached_lyrics() {
                if k == 0 {
                    LyricsRole::Own
                } else {
                    LyricsRole::None
                }
            } else {
                let same = |j: usize| specs[j].index == spec.index;
                match track.staff_lyrics_of(spec.index) {
                    Some(l) if l.above && (0..k).all(|j| !same(j)) => LyricsRole::Attached {
                        source: l.track.and_then(|t| score.tracks.get(t)).unwrap_or(track),
                        above: true,
                    },
                    Some(l) if !l.above && (k + 1..n).all(|j| !same(j)) => LyricsRole::Attached {
                        source: l.track.and_then(|t| score.tracks.get(t)).unwrap_or(track),
                        above: false,
                    },
                    _ => LyricsRole::None,
                }
            };
            let mut c = make_ctx(
                score,
                tracks[k],
                track_index,
                options,
                tracks.clone(),
                spec,
                k == 0,
                chant_ctx.as_ref(),
                role,
            );
            c.last_staff = k + 1 == n;
            c.keyed = keyed;
            c.vibrato = k == vibrato;
            c.brace_next = spec.half == Some((spec.half.map_or(0, |h| h.0), true));
            if let Some((_, hidden)) = &halves[k] {
                c.hidden = hidden.clone();
            }
            c
        })
        .collect();
    run_part(&ctxs)
}

/// Beats a staff leaves out: (bar, voice, beat).
type HiddenBeats = std::collections::HashSet<(usize, usize, usize)>;

/// One half of a grand staff: the notes from `split` up (treble) or below it (bass). Beats
/// whose notes are all on the other half are hidden, and a rest shows on the half where its
/// voice has most notes in the bar.
fn grand_half(track: &Track, split: u8, high: bool) -> (Track, HiddenBeats) {
    let mut half = track.clone();
    let mut hidden = HiddenBeats::new();
    let side = |n: &Note| track.pitch_of(n) >= split;
    for (bi, bar) in half.bars.iter_mut().enumerate() {
        for (vi, voice) in bar.voices.iter_mut().enumerate() {
            let notes = voice.beats.iter().flat_map(|b| &b.notes);
            let (up, down) = notes.fold(
                (0, 0),
                |(u, d), n| if side(n) { (u + 1, d) } else { (u, d + 1) },
            );
            // Rests belong with the voice: the first voice above when it has no notes.
            let home = if up == down { vi == 0 } else { up > down };
            for (i, beat) in voice.beats.iter_mut().enumerate() {
                if beat.empty {
                    continue;
                }
                let had = !beat.notes.is_empty();
                beat.notes.retain(|n| side(n) == high);
                if (had && beat.notes.is_empty()) || (!had && home != high) {
                    hidden.insert((bi, vi, i));
                }
            }
        }
    }
    (half, hidden)
}

/// Where a staff shows lyrics.
#[derive(Clone, Copy)]
enum LyricsRole<'a> {
    None,
    /// The track's own, under its top staff (when no staff has lyrics attached).
    Own,
    /// Those of a track, attached to the staff.
    Attached {
        source: &'a Track,
        above: bool,
    },
}

#[allow(clippy::too_many_arguments)]
fn make_ctx<'a>(
    score: &'a Score,
    track: &'a Track,
    track_index: usize,
    options: &Options,
    part: Vec<&'a Track>,
    spec: &StaffSpec,
    first_staff: bool,
    chant: Option<&ChantCtx>,
    role: LyricsRole<'a>,
) -> Ctx<'a> {
    let sp = options.sp;
    let verses = |t: &Track| {
        t.bars
            .iter()
            .flat_map(|b| &b.voices)
            .flat_map(|v| &v.beats)
            .map(|b| b.lyrics.len())
            .max()
            .unwrap_or(0)
    };
    // Verses with something in them: attached lyrics make no room for empty ones.
    let used = |t: &Track| {
        t.bars
            .iter()
            .flat_map(|b| &b.voices)
            .flat_map(|v| &v.beats)
            .map(|b| {
                b.lyrics
                    .iter()
                    .rposition(|l| !l.is_empty())
                    .map_or(0, |i| i + 1)
            })
            .max()
            .unwrap_or(0)
    };
    let (lyrics, lyrics_above, lyric_source) = match role {
        LyricsRole::None => (0, 0, None),
        LyricsRole::Own => (verses(track), 0, None),
        LyricsRole::Attached {
            source,
            above: true,
        } => (0, used(source), Some(source)),
        LyricsRole::Attached {
            source,
            above: false,
        } => (used(source), 0, Some(source)),
    };
    let head = if spec.slash { SLASH_BLACK } else { HEAD_BLACK };
    let nh_width = fonts().bbox(FontKind::Music, head, 4.0 * sp).width();
    Ctx {
        score,
        track,
        track_index,
        part,
        first_staff,
        o: *options,
        clef: match spec.clef {
            Some(c) if track.kind != TrackKind::Drums => Clef::from_staff(c),
            _ => Clef::for_track(track),
        },
        sp,
        standard: spec.standard,
        tab: spec.tab,
        rhythm: spec.rhythm,
        slash: spec.slash,
        chant: chant.filter(|_| spec.chant).cloned(),
        chant_set: Default::default(),
        staff_index: spec.index,
        half: spec.half,
        last_staff: true,
        vibrato: true,
        brace_next: false,
        keyed: true,
        hidden: HiddenBeats::new(),
        lyrics,
        lyrics_above,
        above: Default::default(),
        lyric_rel: Default::default(),
        origin: Default::default(),
        measuring: Default::default(),
        lyric_spans: Default::default(),
        lyric_end: Default::default(),
        lyric_source,
        nh_width,
        syllables: Default::default(),
        dynamics: track
            .bars
            .iter()
            .filter_map(|b| b.voices.first())
            .flat_map(|v| &v.beats)
            .any(|b| {
                b.effects.hairpin.is_some()
                    || b.notes.iter().any(|n| n.velocity != DEFAULT_VELOCITY)
            }),
        wedges: Default::default(),
        extra: Default::default(),
        low: std::cell::Cell::new(f64::MIN),
        stems: Default::default(),
    }
}

/// MIDI keys played with the feet (bass drums, hi-hat pedal).
fn is_foot(key: u8) -> bool {
    matches!(key, 35 | 36 | 44)
}

/// A drum track as engraved: where hands and feet share the first voice (and the second is
/// unused), the feet move to a second voice so they get stems down. Other tracks are shown as
/// they are.
fn display_track(track: &Track) -> std::borrow::Cow<'_, Track> {
    use std::borrow::Cow;
    if track.kind != TrackKind::Drums {
        return Cow::Borrowed(track);
    }
    let mut t = track.clone();
    for bar in &mut t.bars {
        let second_used = bar.voices.iter().skip(1).any(|v| !v.is_empty());
        let Some(first) = bar.voices.first() else {
            continue;
        };
        let notes = first.beats.iter().flat_map(|b| &b.notes);
        let (feet, hands) = notes.fold((false, false), |(f, h), n| {
            (f || is_foot(n.fret), h || !is_foot(n.fret))
        });
        if second_used || !(feet && hands) {
            continue;
        }
        let mut upper = first.beats.clone();
        let mut lower = first.beats.clone();
        for (u, l) in upper.iter_mut().zip(&mut lower) {
            u.notes.retain(|n| !is_foot(n.fret));
            l.notes.retain(|n| is_foot(n.fret));
            l.text = None;
            l.lyrics.clear();
        }
        merge_rests(&mut lower);
        bar.voices.truncate(1);
        bar.voices[0].beats = upper;
        bar.voices.push(muse_core::Voice { beats: lower });
    }
    Cow::Owned(t)
}

/// Joins pairs of equal rests into the next longer rest where the result starts on a multiple
/// of its own length.
fn merge_rests(beats: &mut Vec<Beat>) {
    loop {
        let mut changed = false;
        let mut tick = 0;
        let mut i = 0;
        while i + 1 < beats.len() {
            let (a, b) = (&beats[i], &beats[i + 1]);
            let plain =
                |x: &Beat| x.is_rest() && x.duration.dots == 0 && x.duration.tuplet.is_none();
            if plain(a)
                && plain(b)
                && a.duration.value == b.duration.value
                && a.duration.value != DurationValue::Whole
            {
                let longer = a.duration.value.longer();
                if tick % longer.ticks() == 0 {
                    beats[i].duration.value = longer;
                    beats.remove(i + 1);
                    changed = true;
                    continue;
                }
            }
            tick += beats[i].ticks();
            i += 1;
        }
        if !changed {
            break;
        }
    }
}

/// Lays out a part: one system per line holding each of its staves.
fn run_part(ctxs: &[Ctx]) -> Layout {
    let main = &ctxs[0];
    let sp = main.sp;
    let plans: Vec<MeasurePlan> = (0..main.score.master_bars.len())
        .map(|b| main.plan_measure(b))
        .collect();
    let multi = ctxs.len() > 1;
    // Room for the brace of a multi-staff part.
    let margin = main.o.margin;
    let braced = ctxs.iter().any(|c| c.brace_next);
    let left = margin + if braced { 2.2 * sp } else { 0.0 };
    let header_width = ctxs.iter().map(|c| c.header_width()).fold(0.0, f64::max);
    let width = match main.o.mode {
        Mode::Page { width, .. } | Mode::Vertical { width } => width,
        Mode::Horizontal => {
            left + margin + header_width + plans.iter().map(|p| p.natural).sum::<f64>() + sp
        }
    };
    let avail = (width - left - margin - header_width).max(20.0 * sp);

    // Break into systems.
    let mut systems: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut used = 0.0;
    for (i, p) in plans.iter().enumerate() {
        if !current.is_empty() && used + p.natural > avail && main.o.mode != Mode::Horizontal {
            systems.push(std::mem::take(&mut current));
            used = 0.0;
        }
        current.push(i);
        used += p.natural;
    }
    if !current.is_empty() {
        systems.push(current);
    }

    let mut layout = Layout {
        pages: Vec::new(),
        beats: HashMap::new(),
        bars: vec![(0, 0.0, 0.0); plans.len()],
    };
    let page_height = match main.o.mode {
        Mode::Page { height, .. } => height,
        _ => 0.0,
    };
    let mut page = Page {
        width,
        height: page_height,
        ..Page::default()
    };
    let mut y = margin;
    if main.o.header {
        y = main.draw_header(&mut page, width, y);
    }
    let mut placed: Vec<HashMap<(usize, usize, usize, u8), Placed>> =
        vec![HashMap::new(); ctxs.len()];
    let mut system_ends: Vec<(usize, f64, f64)> = Vec::new();
    for (si, sys) in systems.iter().enumerate() {
        let natural: f64 = sys.iter().map(|&i| plans[i].natural).sum();
        let last = si + 1 == systems.len();
        let stretch = if main.o.mode == Mode::Horizontal || (last && natural < avail * 0.75) {
            1.0
        } else {
            avail / natural
        };
        for c in ctxs {
            c.fit_lyrics_above(si, sys, &plans, stretch, left, &layout);
            c.fit_tab(si, sys, &plans, stretch, left, &layout);
        }
        let heights: Vec<f64> = ctxs.iter().map(|c| c.metrics().2).collect();
        let system_height: f64 = heights.iter().sum();
        if let Mode::Page { height, .. } = main.o.mode {
            if y + system_height > height - margin && y > margin + 1.0 {
                main.draw_page_number(&mut page, layout.pages.len());
                layout.pages.push(std::mem::replace(
                    &mut page,
                    Page {
                        width,
                        height,
                        ..Page::default()
                    },
                ));
                y = margin;
            }
        }
        let page_index = layout.pages.len();
        let mut x_end = left;
        let mut top = y;
        let mut spans = Vec::new();
        for (k, c) in ctxs.iter().enumerate() {
            x_end = c.draw_system(
                &mut page,
                page_index,
                si,
                sys,
                &plans,
                top,
                left,
                stretch,
                &mut layout,
                &mut placed[k],
            );
            spans.push(c.staff_span(top));
            top += heights[k];
        }
        if multi {
            // A line joining the staves at the start of the system.
            let (a, b) = (spans[0].0, spans[spans.len() - 1].1);
            page.line((left, a), (left, b), 0.16 * sp);
        }
        // A grand staff: a brace, and barlines through both halves.
        for k in (0..ctxs.len().saturating_sub(1)).filter(|&k| ctxs[k].brace_next) {
            let (a, b) = (spans[k].0, spans[k + 1].1);
            for &bi in sys {
                let end = layout.bars[bi].2;
                page.line((end, a), (end, b), 0.16 * sp);
            }
            let unit = fonts().bbox(FontKind::Music, BRACE, 4.0 * sp);
            let size = 4.0 * sp * (b - a) / unit.height().max(1.0);
            let bb = fonts().bbox(FontKind::Music, BRACE, size);
            page.glyph(BRACE, (left - 0.4 * sp - bb.x1, b - bb.y1), size);
        }
        system_ends.push((page_index, left, x_end));
        y += system_height;
    }
    if let Mode::Page { height, .. } = main.o.mode {
        page.height = height;
        main.draw_page_number(&mut page, layout.pages.len());
    } else {
        page.height = y + margin;
    }
    layout.pages.push(page);
    for (k, c) in ctxs.iter().enumerate() {
        c.draw_ties(&mut layout, &placed[k], &system_ends);
        c.draw_hyphens(&mut layout);
        c.draw_wedges(&mut layout);
    }
    layout
}

/// Bottom of the zone, from the top of the system, where the section marker and tempo are drawn
/// (in staff spaces).
const MARKER_ZONE: f64 = 3.2;
/// Clear space between lyrics above a staff and what is under them (staff spaces).
const LYRIC_GAP: f64 = 0.9;

/// The highest point of the items that stand over any of the horizontal `spans`. Only what is
/// over a syllable counts: a tempo mark at the bar's start does not push lyrics up elsewhere.
fn highest_over(items: &[Item], spans: &[(f64, f64)], sp: f64, floor: f64) -> (f64, bool) {
    let reaches = |x0: f64, x1: f64| {
        spans
            .iter()
            .any(|&(a, b)| x0 < b + 0.3 * sp && x1 > a - 0.3 * sp)
    };
    let mut high = f64::MAX;
    let mut above_floor = false;
    for item in items {
        let (x0, x1, y) = match item {
            Item::Line {
                from, to, width, ..
            } => (
                from.x.min(to.x),
                from.x.max(to.x),
                from.y.min(to.y) - width / 2.0,
            ),
            Item::Rect { rect, .. } => (rect.x0, rect.x1, rect.y0),
            Item::Path { path, stroke, .. } => {
                let b = path.bounding_box();
                (b.x0, b.x1, b.y0 - stroke.map_or(0.0, |w| w / 2.0))
            }
            Item::Glyph {
                font, ch, at, size, ..
            } => {
                let b = fonts().bbox(*font, *ch, *size);
                (at.x + b.x0, at.x + b.x1, at.y + b.y0)
            }
            Item::Text {
                font,
                text,
                at,
                size,
                align,
                ..
            } => {
                let w = fonts().text_width(*font, text, *size);
                let x0 = match align {
                    Align::Left => at.x,
                    Align::Center => at.x - w / 2.0,
                    Align::Right => at.x - w,
                };
                (x0, x0 + w, at.y - 0.75 * size)
            }
        };
        if y < floor {
            // The section marker and tempo zone: lyrics stay below it, whatever its x.
            above_floor = true;
        } else if reaches(x0, x1) {
            high = high.min(y);
        }
    }
    (high, above_floor)
}

impl Ctx<'_> {
    /// Finds where lyrics above the staff go in a system: just over the highest thing drawn
    /// above it (notes, ledger lines, stems, beams, flags, clefs, marks, chord names), found by
    /// laying the system out once on a scratch page without them. The room above the staff grows
    /// when the lyrics do not fit in what is already there.
    #[allow(clippy::too_many_arguments)]
    fn fit_lyrics_above(
        &self,
        si: usize,
        sys: &[usize],
        plans: &[MeasurePlan],
        stretch: f64,
        left: f64,
        layout: &Layout,
    ) {
        self.above.set(0.0);
        if self.lyrics_above == 0 {
            return;
        }
        let sp = self.sp;
        let (syllables, wedges) = (self.syllables.borrow().len(), self.wedges.borrow().len());
        let mut scratch = Layout {
            pages: Vec::new(),
            beats: HashMap::new(),
            bars: layout.bars.clone(),
        };
        let mut page = Page::default();
        self.measuring.set(true);
        self.draw_system(
            &mut page,
            0,
            si,
            sys,
            plans,
            0.0,
            left,
            stretch,
            &mut scratch,
            &mut HashMap::new(),
        );
        self.measuring.set(false);
        self.syllables.borrow_mut().truncate(syllables);
        self.wedges.borrow_mut().truncate(wedges);
        // The section marker and tempo are drawn in a zone at the very top of the system, above
        // the room for lyrics; lyrics go between it and the notation, never above it.
        let zone = if self.first_staff {
            MARKER_ZONE * sp
        } else {
            f64::MIN
        };
        let (high, marked) = highest_over(&page.items, &self.lyric_spans.take(), sp, zone);
        let (top, ..) = self.metrics();
        let high = high.min(top);
        // A line of lyrics stands 1.5 spaces tall, and keeps clear of what is under it.
        let gap = LYRIC_GAP * sp;
        let lowest = high - gap;
        let height = 1.5 * sp + (self.lyrics_above - 1) as f64 * 1.8 * sp;
        let floor = if marked { zone + 0.5 * sp } else { 0.0 };
        let grow = (floor + height - lowest).max(0.0);
        self.above.set(grow);
        self.lyric_rel.set(lowest + grow);
    }

    /// Moves the tab of a system down far enough to clear the stems, beams and brackets of
    /// the standard staff above it, found by laying the system out once on a scratch page.
    #[allow(clippy::too_many_arguments)]
    fn fit_tab(
        &self,
        si: usize,
        sys: &[usize],
        plans: &[MeasurePlan],
        stretch: f64,
        left: f64,
        layout: &Layout,
    ) {
        self.extra.set(0.0);
        if !self.standard || self.last_staff {
            return;
        }
        let (syllables, wedges) = (self.syllables.borrow().len(), self.wedges.borrow().len());
        let mut scratch = Layout {
            pages: Vec::new(),
            beats: HashMap::new(),
            bars: layout.bars.clone(),
        };
        self.low.set(f64::MIN);
        self.draw_system(
            &mut Page::default(),
            0,
            si,
            sys,
            plans,
            0.0,
            left,
            stretch,
            &mut scratch,
            &mut HashMap::new(),
        );
        self.syllables.borrow_mut().truncate(syllables);
        self.wedges.borrow_mut().truncate(wedges);
        // The next staff of the track starts 3 spaces below this one's height.
        let (_, _, height) = self.metrics();
        self.extra
            .set((self.low.get() + 2.5 * self.sp - (height + 3.0 * self.sp)).max(0.0));
    }

    /// Width of a time signature's digits, and of one of the parentheses (with its gap) around
    /// it on the tallest staff.
    fn time_widths(&self, ts: TimeSignature) -> (f64, f64) {
        let sp = self.sp;
        let width = |n: u32| -> f64 {
            n.to_string()
                .chars()
                .filter_map(|c| c.to_digit(10))
                .map(|d| fonts().advance(FontKind::Music, time_digit(d), 4.0 * sp))
                .sum()
        };
        let w = width(ts.numerator as u32).max(width(ts.denominator as u32));
        let height = if self.tab {
            (self.track.strings.len().max(2) - 1) as f64 * 1.5 * sp
        } else {
            4.0 * sp
        }
        .max(4.0 * sp);
        let unit = fonts().bbox(FontKind::Music, TIME_PAREN_LEFT, 4.0 * sp);
        (w, unit.width() * height / unit.height().max(1.0) + 0.2 * sp)
    }

    /// An arpeggio (a wavy line) or brush stroke (a straight one) from top to bottom at x,
    /// with an arrowhead at the end the stroke goes towards.
    fn draw_stroke(&self, page: &mut Page, st: Stroke, x: f64, top: f64, bottom: f64) {
        let sp = self.sp;
        let up = st.direction == StrokeDirection::Up;
        let head = 0.8 * sp;
        // The line stops where the arrowhead starts.
        let (a, b) = if up {
            (top + head, bottom)
        } else {
            (top, bottom - head)
        };
        let mut path = BezPath::new();
        path.move_to((x, b));
        if st.arpeggio {
            let step = 0.4 * sp;
            let mut y = b;
            let mut side = 1.0;
            while y - 2.0 * step >= a {
                path.quad_to((x + side * 0.5 * sp, y - step), (x, y - 2.0 * step));
                y -= 2.0 * step;
                side = -side;
            }
            path.line_to((x, a));
        } else {
            path.line_to((x, a));
        }
        page.items.push(Item::Path {
            path,
            color: Color::BLACK,
            stroke: Some(0.12 * sp),
        });
        let (tip, base) = if up {
            (top, top + head)
        } else {
            (bottom, bottom - head)
        };
        let mut arrow = BezPath::new();
        arrow.move_to((x, tip));
        arrow.line_to((x - 0.4 * sp, base));
        arrow.line_to((x + 0.4 * sp, base));
        arrow.close_path();
        page.items.push(Item::Path {
            path: arrow,
            color: Color::BLACK,
            stroke: None,
        });
    }

    /// Gap between the lines of a chant staff: its four lines take the height of five modern ones.
    fn chant_gap(&self) -> f64 {
        self.sp * 4.0 / 3.0
    }

    /// A chant clef (do or fa) on its line of a four-line staff whose bottom line is at
    /// `bottom`.
    fn draw_chant_clef(&self, page: &mut Page, x: f64, clef: ChantClef, bottom: f64) {
        let mut ts = Typeset {
            page,
            bottom,
            g: self.chant_gap(),
        };
        ts.clef(x, clef.kind == ClefKind::Fa, clef.position());
    }

    /// A beat on a chant staff. The first note of a neume sets the whole neume (a pes, clivis,
    /// torculus, climacus … in Caeciliae glyphs) with its first head in the beat's column, the
    /// other notes packed against it; their beats draw nothing but the division after them.
    /// Returns the centre x of the head of this beat's note.
    #[allow(clippy::too_many_arguments)]
    fn draw_chant_beat(
        &self,
        page: &mut Page,
        view: &ChantView,
        (bi, vi, i): (usize, usize, usize),
        x: f64,
        bottom: f64,
        at: BeatRef,
    ) -> Option<f64> {
        let g = self.chant_gap();
        let (ei, is_head) = view.at(bi, i).filter(|_| vi == 0)?;
        let event = &view.events[ei];
        let mut head_x = None;
        let mut ts = Typeset { page, bottom, g };
        let u = ts.u();
        if is_head {
            let (neume, k, len) = view.neume_of(ei).unwrap_or((usize::MAX, 0, 1));
            let kind_of = |e: usize| {
                let n = &view.notes[e];
                if n.initio_debilis || n.mods.iter().any(|m| m == "~") {
                    Kind::Small
                } else if n.inclinatum {
                    Kind::Inclinatum
                } else if n.mods.iter().any(|m| m == "v") {
                    Kind::Virga
                } else {
                    Kind::Punctum
                }
            };
            let set = self.chant_set.borrow();
            let cached = set
                .as_ref()
                .filter(|s| s.neume == neume && k > 0)
                .map(|s| s.centers[k.min(s.centers.len() - 1)]);
            drop(set);
            let cx = match cached {
                Some(cx) => cx,
                None => {
                    let members: Vec<usize> =
                        view.neumes.get(neume).cloned().unwrap_or_else(|| vec![ei]);
                    let ns: Vec<N> = members
                        .iter()
                        .map(|&e| N {
                            pos: view.notes[e].pitch as i32,
                            kind: kind_of(e),
                        })
                        .collect();
                    let left = x + self.nh_width / 2.0 - neume::HEAD / 2.0 * u;
                    // Flats go before the neume.
                    if let Some(&e) = members.iter().find(|&&e| view.alters[e] < 0) {
                        ts.accidental(left - 0.1 * g, view.notes[e].pitch as i32, true);
                    }
                    let first_item = ts.page.items.len();
                    let (width, centers) = if ns.len() == 1 {
                        ts.single(left, ns[0], 1)
                    } else {
                        ts.neume(left, &ns)
                    };
                    // Every note of the neume lights up with the glyphs that draw it.
                    for &e in &members {
                        let ev = &view.events[e];
                        let r = BeatRef {
                            track: at.track,
                            bar: ev.bar,
                            voice: 0,
                            beat: ev.beat,
                        };
                        for item in first_item..ts.page.items.len() {
                            ts.page.marks.push(NoteMark {
                                item,
                                kind: MarkKind::Body,
                                beat: r,
                                string: ev.string,
                                staff: self.staff_index,
                            });
                        }
                    }
                    // Signs: the mora after a head, an episema over it, an ictus under it.
                    for (&e, &c) in members.iter().zip(&centers) {
                        let note = &view.notes[e];
                        let (pos, y) = (note.pitch as i32, ts.y(digit(note.pitch as i32)));
                        let has = |m: &str| note.mods.iter().any(|x| x == m);
                        if note.mora() > 0 {
                            ts.mora(c + 64.0 * u, pos);
                        }
                        if has("_") {
                            let ey = y - 0.5 * neume::HEAD_H * u - 0.2 * g;
                            ts.page
                                .line((c - 64.0 * u, ey), (c + 64.0 * u, ey), 0.07 * g);
                        }
                        if has("'") {
                            ts.page.line(
                                (c, y + 0.5 * neume::HEAD_H * u + 0.1 * g),
                                (c, y + 0.5 * neume::HEAD_H * u + 0.45 * g),
                                0.07 * g,
                            );
                        }
                    }
                    let right = left + width;
                    let first = centers[0];
                    self.chant_set.replace(Some(ChantSet {
                        neume,
                        centers,
                        right,
                    }));
                    first
                }
            };
            head_x = Some(cx);
            let _ = len;
        }
        if event.last_beat() == (bi, i) {
            if let Some(d) = &event.division {
                let after = self
                    .chant_set
                    .borrow()
                    .as_ref()
                    .map_or(0.0, |s| s.right + 0.7 * g);
                let dx = (x + self.nh_width + 0.5 * g).max(after);
                self.draw_chant_division(page, d, dx, bottom);
            }
        }
        head_x
    }

    /// A division after a note: a virgula, a short, long or full bar, or a double bar.
    fn draw_chant_division(&self, page: &mut Page, d: &str, x: f64, bottom: f64) {
        let mut ts = Typeset {
            page,
            bottom,
            g: self.chant_gap(),
        };
        ts.division(x, muse_core::chant::Division::of(d));
    }

    /// Staff positions this staff shows: all, or one side of a grand staff's split.
    fn position_range(&self) -> (i32, i32) {
        match self.half {
            Some((split, high)) => {
                let d = muse_core::staff::diatonic(split as i32 + self.clef.transpose(), 0);
                if high {
                    (d, i32::MAX)
                } else {
                    (i32::MIN, d - 1)
                }
            }
            None => (i32::MIN, i32::MAX),
        }
    }

    /// Notes that standard notation reaches down to y.
    fn reach(&self, y: f64) {
        self.low.set(self.low.get().max(y));
    }

    fn staff_y(&self, bottom: f64, p: i32) -> f64 {
        bottom - p as f64 * self.sp / 2.0
    }

    /// Vertical metrics of a system relative to its top: (standard top, tab top, height).
    fn metrics(&self) -> (f64, f64, f64) {
        let sp = self.sp;
        // Room above the top staff for markers, tempo and chord names; a further staff of the
        // track sits closer.
        let top = if self.first_staff { 10.0 } else { 3.0 } * sp + self.above.get();
        let lyric_space =
            self.lyrics as f64 * 1.8 * sp + if self.lyrics > 0 { 1.5 * sp } else { 0.0 };
        let staff = if self.tab {
            (self.track.strings.len().max(1) - 1) as f64 * 1.5 * sp
                + if self.rhythm { 6.0 * sp } else { 0.0 }
        } else {
            4.0 * sp
        };
        let below = if self.last_staff { 5.0 } else { 3.0 } * sp;
        let dynamics = if self.dynamics && self.last_staff {
            2.0 * sp
        } else {
            0.0
        };
        (
            top,
            top,
            top + staff + lyric_space + below + dynamics + self.extra.get(),
        )
    }

    fn header_width(&self) -> f64 {
        let sp = self.sp;
        3.5 * sp
            + self.key_width(
                self.score
                    .master_bars
                    .first()
                    .map_or(0, |m| m.key_signature.accidentals),
            )
    }

    /// Room for a key signature: on every staff of a track that shows one anywhere, so the
    /// staves' columns line up.
    fn key_width(&self, key: i8) -> f64 {
        if self.keyed {
            key.unsigned_abs() as f64 * 1.0 * self.sp
        } else {
            0.0
        }
    }

    fn plan_measure(&self, bi: usize) -> MeasurePlan {
        let sp = self.sp;
        let mb = &self.score.master_bars[bi];
        let bar_ticks = self.score.played_ticks(bi);
        let mut ticks: Vec<u32> = Vec::new();
        for bar in self.part.iter().filter_map(|t| t.bars.get(bi)) {
            for v in &bar.voices {
                let mut t = 0;
                for b in &v.beats {
                    if !b.empty {
                        ticks.push(t);
                    }
                    t += b.ticks();
                }
            }
        }
        ticks.sort_unstable();
        ticks.dedup();
        if ticks.is_empty() {
            ticks.push(0);
        }
        let mut columns = Vec::new();
        for (i, &t) in ticks.iter().enumerate() {
            let next = ticks.get(i + 1).copied().unwrap_or(bar_ticks.max(t + 240));
            let gap = next.saturating_sub(t).max(60) as f64;
            let mut width = sp * (1.5 + 1.15 * (gap / 120.0 + 1.0).log2());
            if self.has_accidental_or_wide_fret(bi, t) {
                width += 0.8 * sp;
            }
            columns.push(Column { tick: t, width });
        }
        if ticks.len() == 1
            && self.part.iter().all(|t| {
                t.bars.get(bi).is_none_or(|b| {
                    b.voices
                        .iter()
                        .all(|v| v.beats.iter().all(|x| x.notes.is_empty()))
                })
            })
        {
            columns[0].width = columns[0].width.max(6.0 * sp);
        }
        let prev = bi.checked_sub(1).map(|p| &self.score.master_bars[p]);
        let mut lead = 1.2 * sp;
        if prev.is_some_and(|p| p.key_signature.accidentals != mb.key_signature.accidentals) {
            lead += self.key_width(mb.key_signature.accidentals) + 0.5 * sp;
        }
        // Free-time bars still show their time signature (bracketed), as Guitar Pro stores one
        // for them.
        if prev.is_none_or(|p| p.time_signature != mb.time_signature) {
            lead += if mb.free_time {
                let (w, paren) = self.time_widths(mb.time_signature);
                w + 2.0 * paren + 1.4 * sp
            } else {
                2.5 * sp
            };
        }
        if mb.repeat_start {
            lead += 1.0 * sp;
        }
        let natural = lead
            + columns.iter().map(|c| c.width).sum::<f64>()
            + 0.6 * sp
            + if mb.repeat_end.is_some() {
                1.0 * sp
            } else {
                0.0
            };
        MeasurePlan {
            bar: bi,
            columns,
            lead,
            natural,
        }
    }

    fn has_accidental_or_wide_fret(&self, bi: usize, tick: u32) -> bool {
        self.part.iter().filter_map(|t| t.bars.get(bi)).any(|bar| {
            bar.voices.iter().any(|v| {
                let mut t = 0;
                v.beats.iter().any(|b| {
                    let here = t == tick;
                    t += b.ticks();
                    here && (b.effects.stroke.is_some()
                        || b.notes.iter().any(|n| n.fret >= 10 || n.tie || n.ghost))
                })
            })
        })
    }

    /// Centre line of dynamics and hairpins: below the bottom staff, clear of the rhythm stems
    /// under a lone tab staff and of lyrics.
    /// Baseline of the first verse of lyrics: under the staff (and a tab+ staff's stems).
    fn lyrics_top(&self, std_bottom: f64, tab_top: f64) -> f64 {
        let sp = self.sp;
        if self.tab {
            let tab_bottom = tab_top + (self.track.strings.len().max(1) - 1) as f64 * 1.5 * sp;
            tab_bottom + if self.rhythm { 8.0 } else { 3.0 } * sp
        } else {
            std_bottom + 5.0 * sp
        }
    }

    /// Baseline of verse `li` of lyrics: under the staff, or above it (and above the chord names
    /// of the top staff).
    fn lyric_y(&self, li: usize, std_bottom: f64, tab_top: f64) -> f64 {
        let sp = self.sp;
        if self.lyrics_above == 0 {
            return self.lyrics_top(std_bottom, tab_top) + li as f64 * 1.8 * sp;
        }
        self.origin.get() + self.lyric_rel.get() - (self.lyrics_above - 1 - li) as f64 * 1.8 * sp
    }

    /// Draws the verses of lyrics of a beat, centred on `cx`.
    fn draw_syllables(
        &self,
        page: &mut Page,
        verses: &[String],
        cx: f64,
        (std_bottom, tab_top): (f64, f64),
        page_index: usize,
    ) {
        let sp = self.sp;
        let measuring = self.measuring.get() && self.lyrics_above > 0;
        for (li, syl) in verses.iter().enumerate() {
            // A trailing hyphen is drawn between this syllable and the next.
            let continues = syl.ends_with('-');
            let text = syl.trim_end_matches('-');
            if !text.is_empty() {
                let y = self.lyric_y(li, std_bottom, tab_top);
                let w = fonts().text_width(FontKind::Text, text, 1.9 * sp);
                // Syllables taken from another track's rhythm can fall on the same column or
                // too close: each starts clear of the one before it in its verse, with room
                // for the hyphen that joins it to the next.
                let mut cx = cx;
                if self.lyric_source.is_some() {
                    let mut ends = self.lyric_end.borrow_mut();
                    if ends.len() <= li {
                        ends.resize(li + 1, f64::MIN);
                    }
                    cx = cx.max(ends[li] + 0.4 * sp + w / 2.0);
                    ends[li] = cx + w / 2.0 + if continues { 0.8 * sp } else { 0.0 };
                }
                if measuring {
                    self.lyric_spans
                        .borrow_mut()
                        .push((cx - w / 2.0, cx + w / 2.0));
                    continue;
                }
                page.text(
                    FontKind::Text,
                    text.to_string(),
                    (cx, y),
                    1.9 * sp,
                    Align::Center,
                );
                self.syllables.borrow_mut().push((
                    page_index,
                    y,
                    cx - w / 2.0,
                    cx + w / 2.0,
                    continues,
                ));
            }
        }
    }

    fn dynamics_y(&self, std_bottom: f64, tab_top: f64) -> f64 {
        let sp = self.sp;
        if self.tab {
            let tab_bottom = tab_top + (self.track.strings.len().max(1) - 1) as f64 * 1.5 * sp;
            tab_bottom + if self.rhythm { 7.5 } else { 3.0 } * sp
        } else {
            let lyrics =
                self.lyrics as f64 * 1.8 * sp + if self.lyrics > 0 { 1.5 * sp } else { 0.0 };
            std_bottom + lyrics + 3.0 * sp
        }
    }

    /// Velocity of the last beat with notes before (bar, beat) in the first voice.
    fn previous_velocity(&self, bar: usize, beat: usize) -> Option<u8> {
        let first_voice = |b: usize| self.track.bars.get(b).and_then(|b| b.voices.first());
        let earlier = first_voice(bar)
            .map(|v| &v.beats[..beat.min(v.beats.len())])
            .into_iter()
            .chain(
                (0..bar)
                    .rev()
                    .filter_map(|b| first_voice(b).map(|v| &v.beats[..])),
            );
        earlier
            .flat_map(|beats| beats.iter().rev())
            .find_map(|b| b.notes.first())
            .map(|n| n.velocity)
    }

    /// A dynamic marking (p, mf, ff...) centred on x, its letters sitting on the line y.
    fn draw_dynamic(&self, page: &mut Page, velocity: u8, x: f64, y: f64) {
        let size = 4.0 * self.sp;
        let glyphs: Vec<char> = dynamic_name(velocity)
            .chars()
            .map(|c| match c {
                'P' => '\u{E520}',
                'M' => '\u{E521}',
                _ => '\u{E522}',
            })
            .collect();
        let w: f64 = glyphs
            .iter()
            .map(|&g| fonts().advance(FontKind::Music, g, size))
            .sum();
        let mut cx = x - w / 2.0;
        for g in glyphs {
            page.glyph(g, (cx, y + 0.5 * self.sp), size);
            cx += fonts().advance(FontKind::Music, g, size);
        }
    }

    /// A vibrato wiggle from x0 towards x1 (whole waves only) along the line y.
    fn draw_vibrato(&self, page: &mut Page, x0: f64, x1: f64, y: f64) {
        let size = 3.0 * self.sp;
        let step = fonts().advance(FontKind::Music, VIBRATO, size);
        if step <= 0.0 {
            return;
        }
        let mut x = x0;
        loop {
            page.glyph(VIBRATO, (x, y), size);
            x += step;
            if x + step > x1 {
                break;
            }
        }
    }

    /// Hairpins over runs of consecutive beats of the same kind, split across systems.
    fn draw_wedges(&self, layout: &mut Layout) {
        let sp = self.sp;
        let wedges = self.wedges.borrow();
        let beats_in = |bar: usize| {
            self.track
                .bars
                .get(bar)
                .and_then(|b| b.voices.first())
                .map_or(0, |v| v.beats.len())
        };
        let follows = |a: &Wedge, b: &Wedge| {
            let ((ab, ai), (bb, bi)) = (a.1, b.1);
            a.0 == b.0
                && ((ab == bb && bi == ai + 1)
                    || (bb == ab + 1 && bi == 0 && ai + 1 == beats_in(ab)))
        };
        let mut start = 0;
        while start < wedges.len() {
            let mut end = start + 1;
            while end < wedges.len() && follows(&wedges[end - 1], &wedges[end]) {
                end += 1;
            }
            let run = &wedges[start..end];
            let n = run.len() as f64;
            let open = 0.7 * sp;
            // Half-opening at a fraction of the way through the run.
            let half = |f: f64| match run[0].0 {
                Hairpin::Crescendo => open * f,
                Hairpin::Decrescendo => open * (1.0 - f),
            };
            let mut a = 0;
            while a < run.len() {
                let mut b = a + 1;
                while b < run.len() && (run[b].2, run[b].3) == (run[a].2, run[a].3) {
                    b += 1;
                }
                let (f0, f1) = (a as f64 / n, b as f64 / n);
                let x0 = run[a].4;
                let x1 = run[b - 1].5 - if b == run.len() { 1.5 * sp } else { 0.0 };
                let y = run[a].6;
                if let Some(page) = layout.pages.get_mut(run[a].2) {
                    page.line((x0, y - half(f0)), (x1, y - half(f1)), 0.12 * sp);
                    page.line((x0, y + half(f0)), (x1, y + half(f1)), 0.12 * sp);
                }
                a = b;
            }
            start = end;
        }
    }

    /// Hyphens between the syllables of a word: centred in the gap to the next syllable on the
    /// same line, or just after the syllable at the end of a system.
    fn draw_hyphens(&self, layout: &mut Layout) {
        let sp = self.sp;
        let mut syl = self.syllables.borrow().clone();
        syl.sort_by(|a, b| (a.0, a.1, a.2).partial_cmp(&(b.0, b.1, b.2)).unwrap());
        let size = 1.9 * sp;
        let hyphen = fonts().text_width(FontKind::Text, "-", size);
        for (k, &(page, y, _, right, continues)) in syl.iter().enumerate() {
            if !continues {
                continue;
            }
            let next = syl
                .get(k + 1)
                .filter(|n| n.0 == page && (n.1 - y).abs() < 0.01);
            let x = match next {
                Some(n) if n.2 - right > hyphen * 1.5 => (right + n.2) / 2.0,
                Some(_) => continue,
                None => right + 0.4 * sp + hyphen / 2.0,
            };
            if let Some(p) = layout.pages.get_mut(page) {
                p.text(FontKind::Text, "-", (x, y), size, Align::Center);
            }
        }
    }

    /// Top and bottom y of this staff (standard staff, or tablature) in a system at `top`.
    fn staff_span(&self, top: f64) -> (f64, f64) {
        let (first, _, _) = self.metrics();
        let height = if self.tab {
            (self.track.strings.len().max(1) - 1) as f64 * 1.5 * self.sp
        } else {
            4.0 * self.sp
        };
        (top + first, top + first + height)
    }

    fn draw_header(&self, page: &mut Page, width: f64, y: f64) -> f64 {
        let m = &self.score.meta;
        let sp = self.sp;
        let mut y = y;
        if !m.title.is_empty() {
            y += 5.0 * sp;
            page.text(
                FontKind::Bold,
                m.title.clone(),
                (width / 2.0, y),
                5.0 * sp,
                Align::Center,
            );
        }
        if !m.subtitle.is_empty() {
            y += 3.2 * sp;
            page.text(
                FontKind::Text,
                m.subtitle.clone(),
                (width / 2.0, y),
                3.0 * sp,
                Align::Center,
            );
        }
        let credit = if !m.artist.is_empty() {
            m.artist.clone()
        } else {
            m.music.clone()
        };
        if !credit.is_empty() {
            y += 3.2 * sp;
            page.text(
                FontKind::Text,
                credit,
                (width - self.o.margin, y),
                2.6 * sp,
                Align::Right,
            );
        }
        if !self.track.name.trim().is_empty() {
            page.text(
                FontKind::Text,
                self.track.name.trim().to_string(),
                (self.o.margin, y),
                2.6 * sp,
                Align::Left,
            );
        }
        // The tuning (low string first) and capo of a fretted track, as Guitar Pro prints it.
        if let Some(tuning) = self.track.tuning_text() {
            y += 3.0 * sp;
            page.text(
                FontKind::Text,
                tuning,
                (self.o.margin, y),
                2.2 * sp,
                Align::Left,
            );
        }
        y + 4.0 * sp
    }

    fn draw_page_number(&self, page: &mut Page, index: usize) {
        if let Mode::Page { width, height } = self.o.mode {
            page.text(
                FontKind::Text,
                format!("{}", index + 1),
                (width / 2.0, height - self.o.margin / 2.0),
                2.4 * self.sp,
                Align::Center,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_system(
        &self,
        page: &mut Page,
        page_index: usize,
        system_index: usize,
        bars: &[usize],
        plans: &[MeasurePlan],
        top: f64,
        x0: f64,
        stretch: f64,
        layout: &mut Layout,
        placed: &mut HashMap<(usize, usize, usize, u8), Placed>,
    ) -> f64 {
        let sp = self.sp;
        self.chant_set.replace(None);
        self.origin.set(top);
        self.lyric_end.borrow_mut().clear();
        let (std_rel, tab_rel, _) = self.metrics();
        let std_top = top + std_rel;
        let std_bottom = std_top + 4.0 * sp;
        let tab_top = top + tab_rel;
        let strings = self.track.strings.len().max(1);
        let tab_y = |s: usize| tab_top + s as f64 * 1.5 * sp;
        let tab_bottom = tab_y(strings - 1);
        let header = self.header_width();
        let total: f64 = bars.iter().map(|&i| plans[i].natural * stretch).sum();
        let x1 = x0 + header + total;

        // Staff lines: four for chant, five otherwise, and just the middle one for slashes.
        let chant_error = self.chant.as_ref().and_then(|c| c.error.as_deref());
        if let Some(message) = chant_error {
            // No staff: the reason, where it would be.
            page.items.push(Item::Text {
                font: FontKind::Italic,
                text: message.to_string(),
                at: Point::new(x0 + 0.8 * sp, std_bottom - 1.5 * sp),
                size: 1.6 * sp,
                align: Align::Left,
                color: Color::RED,
            });
        } else if self.standard {
            let lines = if self.chant.is_some() { 4 } else { 5 };
            let gap = if self.chant.is_some() {
                self.chant_gap()
            } else {
                sp
            };
            for l in 0..lines {
                if self.slash && l != 2 {
                    continue;
                }
                let y = std_bottom - l as f64 * gap;
                page.line((x0, y), (x1, y), 0.13 * sp);
            }
        }
        if self.tab {
            for s in 0..strings {
                page.items.push(Item::Line {
                    from: Point::new(x0, tab_y(s)),
                    to: Point::new(x1, tab_y(s)),
                    width: 0.1 * sp,
                    color: Color::GREY,
                });
            }
        }
        // System start line and clefs.
        let first_top = if self.standard { std_top } else { tab_top };
        let last_bottom = if self.tab { tab_bottom } else { std_bottom };
        page.line((x0, first_top), (x0, last_bottom), 0.16 * sp);
        let first_bar = bars[0];
        if let Some(view) = self.chant.as_ref().and_then(|c| c.view.as_ref()) {
            self.draw_chant_clef(page, x0 + 0.8 * sp, view.clef, std_bottom);
        }
        // A slash staff has neither clef nor key: it shows rhythm only.
        if self.standard && !self.slash && self.chant.is_none() {
            let (g, p) = self.clef.glyph();
            page.glyph(g, (x0 + 0.8 * sp, self.staff_y(std_bottom, p)), 4.0 * sp);
            let key = self.score.master_bars[first_bar].key_signature.accidentals;
            self.draw_key(page, key, x0 + 3.8 * sp, std_bottom);
        }
        if self.tab {
            let mid = (tab_top + tab_bottom) / 2.0;
            // Scale the clef to 80% of the tab staff's height.
            let unit = fonts().bbox(FontKind::Music, TAB_CLEF, 4.0 * sp);
            let size = 4.0 * sp * 0.8 * (tab_bottom - tab_top) / unit.height().max(1.0);
            let b = fonts().bbox(FontKind::Music, TAB_CLEF, size);
            page.glyph(TAB_CLEF, (x0 + 0.8 * sp, mid - (b.y0 + b.y1) / 2.0), size);
        }
        // Bar number above the first bar.
        if self.first_staff && (system_index > 0 || first_bar > 0) {
            page.text(
                FontKind::Italic,
                format!("{}", first_bar + 1),
                (x0, first_top - 1.4 * sp),
                1.8 * sp,
                Align::Left,
            );
        }

        let mut x = x0 + header;
        for &bi in bars {
            let plan = &plans[bi];
            let w = plan.natural * stretch;
            layout.bars[bi] = (page_index, x, x + w);
            self.draw_measure(
                page,
                page_index,
                system_index,
                plan,
                x,
                w,
                stretch,
                std_bottom,
                tab_top,
                layout,
                placed,
            );
            x += w;
        }
        // A custos at the end of a chant staff shows where the next system begins; it sits on
        // the staff's open-ended lines, at the next note's height.
        if let (Some(view), Some(&last)) = (
            self.chant.as_ref().and_then(|c| c.view.as_ref()),
            bars.last(),
        ) {
            if let Some(next) = view.events.iter().position(|e| e.bar > last) {
                let g = self.chant_gap();
                let mut ts = Typeset {
                    page,
                    bottom: std_bottom,
                    g,
                };
                ts.custos(x1 - 0.3 * g, view.notes[next].pitch as i32);
            }
        }
        x1
    }

    fn draw_key(&self, page: &mut Page, key: i8, x: f64, bottom: f64) {
        let sharps = [8, 5, 9, 6, 3, 7, 4];
        let flats = [4, 7, 3, 6, 2, 5, 1];
        let off = self.clef.key_offset();
        for i in 0..key.unsigned_abs().min(7) as usize {
            let (g, p) = if key > 0 {
                (SHARP, sharps[i])
            } else {
                (FLAT, flats[i])
            };
            page.glyph(
                g,
                (x + i as f64 * self.sp, self.staff_y(bottom, p + off)),
                4.0 * self.sp,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_measure(
        &self,
        page: &mut Page,
        page_index: usize,
        system: usize,
        plan: &MeasurePlan,
        x: f64,
        width: f64,
        stretch: f64,
        std_bottom: f64,
        tab_top: f64,
        layout: &mut Layout,
        placed: &mut HashMap<(usize, usize, usize, u8), Placed>,
    ) {
        let sp = self.sp;
        let bi = plan.bar;
        let mb = &self.score.master_bars[bi];
        let prev = bi.checked_sub(1).map(|p| &self.score.master_bars[p]);
        let std_top = std_bottom - 4.0 * sp;
        let strings = self.track.strings.len().max(1);
        let tab_bottom = tab_top + (strings - 1) as f64 * 1.5 * sp;
        let staves: Vec<(f64, f64)> = [
            self.standard.then_some((std_top, std_bottom)),
            self.tab.then_some((tab_top, tab_bottom)),
        ]
        .into_iter()
        .flatten()
        .collect();
        let top = staves.first().map_or(std_top, |s| s.0);
        let bottom = staves.last().map_or(std_bottom, |s| s.1);

        // Barlines and repeats (a chant staff has only its divisions).
        let end = x + width;
        let staves_for_bars: &[(f64, f64)] = if self.chant.is_some() { &[] } else { &staves };
        for &(a, b) in staves_for_bars {
            if mb.repeat_end.is_some() {
                page.line((end - 0.8 * sp, a), (end - 0.8 * sp, b), 0.16 * sp);
                page.rect(Rect::new(end - 0.5 * sp, a, end, b), Color::BLACK);
                self.repeat_dots(page, end - 1.6 * sp, a, b);
            } else if mb.double_bar {
                page.line((end - 0.6 * sp, a), (end - 0.6 * sp, b), 0.16 * sp);
                page.line((end, a), (end, b), 0.16 * sp);
            } else {
                page.line((end, a), (end, b), 0.16 * sp);
            }
            if mb.repeat_start {
                page.rect(Rect::new(x, a, x + 0.5 * sp, b), Color::BLACK);
                page.line((x + 0.8 * sp, a), (x + 0.8 * sp, b), 0.16 * sp);
                self.repeat_dots(page, x + 1.4 * sp, a, b);
            }
        }
        if let Some(n) = mb.repeat_end.filter(|&n| n > 2 && self.first_staff) {
            page.text(
                FontKind::Bold,
                format!("x{n}"),
                (end, top - 0.6 * sp),
                1.8 * sp,
                Align::Right,
            );
        }
        if bi + 1 == self.score.master_bars.len() && mb.repeat_end.is_none() {
            for &(a, b) in staves_for_bars {
                page.rect(Rect::new(end - 0.5 * sp, a, end, b), Color::BLACK);
                page.line((end - 1.0 * sp, a), (end - 1.0 * sp, b), 0.16 * sp);
            }
        }
        // Alternate ending bracket.
        if mb.alternate_endings != 0 && self.first_staff {
            let y = top - 5.5 * sp;
            page.line((x, y), (end - 0.3 * sp, y), 0.12 * sp);
            let starts = prev.is_none_or(|p| p.alternate_endings != mb.alternate_endings);
            if starts {
                page.line((x, y), (x, y + 2.0 * sp), 0.12 * sp);
                let label: Vec<String> = (0..8)
                    .filter(|i| mb.alternate_endings & (1 << i) != 0)
                    .map(|i| format!("{}.", i + 1))
                    .collect();
                page.text(
                    FontKind::Text,
                    label.join(" "),
                    (x + 0.5 * sp, y + 1.7 * sp),
                    1.8 * sp,
                    Align::Left,
                );
            }
        }
        // Section marker, directions and tempo.
        if self.first_staff {
            // Anchored to the top of the system: room made for lyrics above the staff goes under it.
            let mut above = top - self.above.get() - 7.5 * sp;
            if let Some(m) = &mb.marker {
                let (letter, title) = (m.letter.trim(), m.title.trim());
                if !letter.is_empty() || !title.is_empty() {
                    // The section letter in a box, then the title on a grey ground.
                    let mut tx = x;
                    if !letter.is_empty() {
                        let lw = fonts().text_width(FontKind::Bold, letter, 2.2 * sp);
                        let side = (lw + 0.8 * sp).max(2.6 * sp);
                        let r = Rect::new(
                            x - 0.3 * sp,
                            above - 2.0 * sp,
                            x - 0.3 * sp + side,
                            above + 0.6 * sp,
                        );
                        let mut b = BezPath::new();
                        b.move_to((r.x0, r.y0));
                        b.line_to((r.x1, r.y0));
                        b.line_to((r.x1, r.y1));
                        b.line_to((r.x0, r.y1));
                        b.close_path();
                        page.items.push(Item::Path {
                            path: b,
                            color: Color::BLACK,
                            stroke: Some(0.15 * sp),
                        });
                        page.text(
                            FontKind::Bold,
                            letter.to_string(),
                            ((r.x0 + r.x1) / 2.0, above),
                            2.2 * sp,
                            Align::Center,
                        );
                        tx = r.x1 + 0.8 * sp;
                    }
                    if !title.is_empty() {
                        let w = fonts().text_width(FontKind::Bold, title, 2.2 * sp);
                        page.items.push(Item::Rect {
                            rect: Rect::new(
                                tx - 0.3 * sp,
                                above - 2.0 * sp,
                                tx + w + 0.3 * sp,
                                above + 0.5 * sp,
                            ),
                            color: Color::rgb(235, 235, 235),
                        });
                        page.text(
                            FontKind::Bold,
                            title.to_string(),
                            (tx, above),
                            2.2 * sp,
                            Align::Left,
                        );
                    }
                    above -= 3.0 * sp;
                }
            }
            let tempos: Vec<(u32, f64)> = if bi == 0 {
                vec![(0, self.score.tempo as f64)]
            } else {
                vec![]
            }
            .into_iter()
            .chain(mb.tempo_changes.iter().map(|c| (c.tick, c.bpm)))
            .collect();
            if let Some(&(_, bpm)) = tempos.first() {
                page.glyph(METRONOME_QUARTER, (x + 0.5 * sp, above), 3.2 * sp);
                page.text(
                    FontKind::Text,
                    format!("= {}", bpm.round()),
                    (x + 2.0 * sp, above),
                    2.2 * sp,
                    Align::Left,
                );
            }
            for d in &mb.directions {
                let label = format!("{d:?}");
                page.text(
                    FontKind::Italic,
                    label,
                    (end, top - 2.0 * sp),
                    1.8 * sp,
                    Align::Right,
                );
            }
        }
        // Key and time signature changes.
        let mut lead_x = x + 0.6 * sp;
        if self.standard
            && !self.slash
            && self.chant.is_none()
            && prev.is_some_and(|p| p.key_signature.accidentals != mb.key_signature.accidentals)
        {
            self.draw_key(page, mb.key_signature.accidentals, lead_x, std_bottom);
            lead_x += self.key_width(mb.key_signature.accidentals) + 0.5 * sp;
        }
        // Free-time bars still show their time signature, in parentheses (chant has none).
        if self.chant.is_none() && prev.is_none_or(|p| p.time_signature != mb.time_signature) {
            let ts = mb.time_signature;
            let (w, paren) = self.time_widths(ts);
            let centre = lead_x
                + if mb.free_time {
                    0.4 * sp + paren + w / 2.0
                } else {
                    1.0 * sp
                };
            for &(a, b) in &staves {
                let mid = (a + b) / 2.0;
                if mb.free_time {
                    // Parentheses as tall as the staff.
                    let unit = fonts().bbox(FontKind::Music, TIME_PAREN_LEFT, 4.0 * sp);
                    let size = 4.0 * sp * (b - a) / unit.height().max(1.0);
                    let l = fonts().bbox(FontKind::Music, TIME_PAREN_LEFT, size);
                    let r = fonts().bbox(FontKind::Music, TIME_PAREN_RIGHT, size);
                    page.glyph(
                        TIME_PAREN_LEFT,
                        (
                            centre - w / 2.0 - 0.2 * sp - l.x1,
                            mid - (l.y0 + l.y1) / 2.0,
                        ),
                        size,
                    );
                    page.glyph(
                        TIME_PAREN_RIGHT,
                        (
                            centre + w / 2.0 + 0.2 * sp - r.x0,
                            mid - (r.y0 + r.y1) / 2.0,
                        ),
                        size,
                    );
                }
                let draw = |page: &mut Page, n: u32, y: f64| {
                    let s = n.to_string();
                    let digits: Vec<u32> = s.chars().filter_map(|c| c.to_digit(10)).collect();
                    let w: f64 = digits
                        .iter()
                        .map(|d| fonts().advance(FontKind::Music, time_digit(*d), 4.0 * sp))
                        .sum();
                    let mut cx = centre - w / 2.0;
                    for d in digits {
                        page.glyph(time_digit(d), (cx, y), 4.0 * sp);
                        cx += fonts().advance(FontKind::Music, time_digit(d), 4.0 * sp);
                    }
                };
                // Standard-size digits close together on the middle (a tab staff's lines are
                // closer than a standard staff's, so they are not spread over its height).
                let half = ((b - a) / 4.0).min(sp);
                draw(page, ts.numerator as u32, mid - half);
                draw(page, ts.denominator as u32, mid + half);
            }
        }

        // Columns.
        let mut col_x: HashMap<u32, (f64, f64)> = HashMap::new();
        let mut cx = x + plan.lead * stretch;
        for c in &plan.columns {
            col_x.insert(c.tick, (cx, c.width * stretch));
            cx += c.width * stretch;
        }
        let Some(bar) = self.track.bars.get(bi) else {
            return;
        };
        let voices = bar
            .voices
            .iter()
            .filter(|v| !v.beats.iter().all(|b| b.empty))
            .count();
        let bar_blank = bar.voices.iter().all(|v| v.beats.iter().all(|b| b.empty));
        let key_alter = key_alters(mb.key_signature.accidentals);
        let mut alters: HashMap<i32, i32> = HashMap::new();
        let mut shown = false;
        // Lyrics attached to this staff, from the first voice of their track.
        if let Some(src) = self.lyric_source {
            let beats = src.bars.get(bi).and_then(|b| b.voices.first());
            let mut t = 0;
            for beat in beats.map_or(&[][..], |v| &v.beats[..]) {
                let start = t;
                t += beat.ticks();
                if beat.lyrics.iter().all(|l| l.is_empty()) {
                    continue;
                }
                // The staff's column at that tick, or the last one before it.
                let col = col_x.get(&start).copied().or_else(|| {
                    col_x
                        .iter()
                        .filter(|(k, _)| **k <= start)
                        .max_by_key(|(k, _)| **k)
                        .map(|(_, v)| *v)
                });
                if let Some((bx, _)) = col {
                    self.draw_syllables(
                        page,
                        &beat.lyrics,
                        bx + self.nh_width / 2.0,
                        (std_bottom, tab_top),
                        page_index,
                    );
                }
            }
        }
        for (vi, voice) in bar.voices.iter().enumerate() {
            let mut t = 0;
            let mut beamable: Vec<(usize, f64, Vec<f64>, DurationValue, u32)> = Vec::new();
            // Beats of a tab+ staff waiting for their beam: start tick, stem x, value, dots.
            let mut rhythm: Vec<RhythmBeat> = Vec::new();
            let mut rhythm_beat = 0;
            let up_voice = if voices > 1 { Some(vi % 2 == 0) } else { None };
            let mut tuplet_group: Vec<TupletNote> = Vec::new();
            // The beat (of the time signature) the open beam group lies in.
            let mut beam_beat = 0;
            let mut tuplet_ratio: Option<Tuplet> = None;
            let mut tuplet_fill = 0;
            // Complete tuplets, drawn once the voice's beams are in place.
            let mut tuplets: Vec<(Vec<TupletNote>, Option<Tuplet>)> = Vec::new();
            self.stems.borrow_mut().clear();
            for (i, beat) in voice.beats.iter().enumerate() {
                let start = t;
                t += beat.ticks();
                // The first beat of a bar with nothing in it gets a position (for the cursor and
                // clicks) but draws nothing.
                let placeholder = beat.empty && vi == 0 && i == 0 && bar_blank;
                if beat.empty && !placeholder {
                    continue;
                }
                // A beat shown on the other half of a grand staff.
                let hidden = self.hidden.contains(&(bi, vi, i));
                shown |= !hidden && !placeholder;
                if hidden {
                    if self.standard {
                        self.flush_beams(page, &mut beamable, std_bottom);
                    }
                    continue;
                }
                let Some(&(bx, bw)) = col_x.get(&start) else {
                    continue;
                };
                let beat_ref = BeatRef {
                    track: self.track_index,
                    bar: bi,
                    voice: vi,
                    beat: i,
                };
                let string_ys: Vec<f64> = if self.tab {
                    (0..strings)
                        .map(|s| tab_top + s as f64 * 1.5 * sp)
                        .collect()
                } else {
                    vec![]
                };
                // The beat's place on this staff, joined to its places on the track's others.
                let entry = layout.beats.entry(beat_ref).or_insert_with(|| BeatPos {
                    page: page_index,
                    x: bx - 0.4 * sp,
                    width: bw,
                    top: top - 2.0 * sp,
                    bottom: bottom + 2.0 * sp,
                    head: bx + self.nh_width / 2.0,
                    staves: Vec::new(),
                });
                entry.top = entry.top.min(top - 2.0 * sp);
                entry.bottom = entry.bottom.max(bottom + 2.0 * sp);
                if self.tab {
                    entry.staves.push(StaffPos::Strings {
                        staff: self.staff_index,
                        ys: string_ys.clone(),
                    });
                }
                if self.chant.as_ref().is_some_and(|c| c.view.is_some()) {
                    // GABC positions: d (3) on the bottom line.
                    entry.staves.push(StaffPos::Lines {
                        staff: self.staff_index,
                        y0: std_bottom + 1.5 * self.chant_gap(),
                        step: self.chant_gap() / 2.0,
                        range: (i32::MIN, i32::MAX),
                    });
                } else if self.standard && self.chant.is_none() && self.clef != Clef::Percussion {
                    entry.staves.push(StaffPos::Lines {
                        staff: self.staff_index,
                        y0: std_bottom + self.clef.bottom() as f64 * sp / 2.0,
                        step: sp / 2.0,
                        range: self.position_range(),
                    });
                }
                page.hits.push(Hit {
                    rect: Rect::new(
                        bx - 0.4 * sp,
                        top - 2.0 * sp,
                        bx + bw - 0.4 * sp,
                        bottom + 2.0 * sp,
                    ),
                    beat: beat_ref,
                    string: None,
                    staff: self.staff_index,
                });
                for (s, &sy) in string_ys.iter().enumerate() {
                    page.hits.push(Hit {
                        rect: Rect::new(
                            bx - 0.4 * sp,
                            sy - 0.75 * sp,
                            bx + bw - 0.4 * sp,
                            sy + 0.75 * sp,
                        ),
                        beat: beat_ref,
                        string: Some(s as u8),
                        staff: self.staff_index,
                    });
                }

                if placeholder {
                    continue;
                }

                // Standard notation.
                let mut chord: Option<(Vec<f64>, bool)> = None;
                if let Some(view) = self.chant.as_ref().and_then(|c| c.view.as_ref()) {
                    if let Some(hx) =
                        self.draw_chant_beat(page, view, (bi, vi, i), bx, std_bottom, beat_ref)
                    {
                        if let Some(entry) = layout.beats.get_mut(&beat_ref) {
                            entry.head = hx;
                        }
                    }
                } else if self.chant.is_some() {
                    // No chant staff to draw (see the message).
                } else if self.standard {
                    if beat.notes.is_empty() {
                        let off = match up_voice {
                            Some(true) => 2,
                            Some(false) => -2,
                            None => 0,
                        };
                        let p = 4 + off;
                        page.glyph(
                            rest_glyph(beat.duration.value),
                            (bx, self.staff_y(std_bottom, p)),
                            4.0 * sp,
                        );
                        self.draw_dots(page, beat, bx + 1.6 * sp, std_bottom, p | 1);
                        if beam_count(beat.duration.value) > 0 {
                            self.flush_beams(page, &mut beamable, std_bottom);
                        }
                    } else {
                        let (stem_x, ys, up) = self.draw_chord(
                            page,
                            beat,
                            bx,
                            std_bottom,
                            up_voice,
                            key_alter,
                            &mut alters,
                            (bi, vi, i),
                            page_index,
                            system,
                            placed,
                        );
                        chord = Some((ys.clone(), up));
                        let level = beam_count(beat.duration.value);
                        // Beams group the notes of each beat; a note across a beat stands alone.
                        let unit = self.beam_unit(mb);
                        let (first, last) = (start / unit, (t - 1) / unit);
                        if level > 0 && (first != beam_beat || first != last) {
                            self.flush_beams(page, &mut beamable, std_bottom);
                        }
                        if level > 0 {
                            beamable.push((
                                start as usize,
                                stem_x,
                                ys,
                                beat.duration.value,
                                u32::from(up),
                            ));
                            beam_beat = first;
                            if first != last {
                                self.flush_beams(page, &mut beamable, std_bottom);
                            }
                        } else {
                            self.flush_beams(page, &mut beamable, std_bottom);
                            if beat.duration.value != DurationValue::Whole {
                                self.draw_stem(page, stem_x, &ys, up, None);
                            }
                        }
                    }
                }
                // On a tab+ staff the stems below mark the tuplet.
                if self.rhythm && !beat.notes.is_empty() {
                    chord = Some((Vec::new(), false));
                }
                // Tuplet groups (chant has none).
                if let Some(tp) = beat.duration.tuplet.filter(|_| self.chant.is_none()) {
                    if tuplet_ratio != Some(tp) {
                        tuplets.push((std::mem::take(&mut tuplet_group), tuplet_ratio));
                        tuplet_ratio = Some(tp);
                        tuplet_fill = 0;
                    }
                    tuplet_group.push((bx, bx + self.nh_width, chord.clone()));
                    tuplet_fill += beat.duration.ticks();
                    if tuplet_fill + 2 >= tp.times as u32 * beat.duration.value.ticks() {
                        tuplets.push((std::mem::take(&mut tuplet_group), tuplet_ratio));
                        tuplet_ratio = None;
                    }
                } else if tuplet_ratio.is_some() {
                    tuplets.push((std::mem::take(&mut tuplet_group), tuplet_ratio));
                    tuplet_ratio = None;
                }
                // Dynamics where the first voice's loudness changes, and hairpins.
                if vi == 0 && self.last_staff {
                    let dyn_y = self.dynamics_y(std_bottom, tab_top);
                    if let Some(n) = beat.notes.first() {
                        let before = self.previous_velocity(bi, i);
                        if before.map_or(n.velocity != DEFAULT_VELOCITY, |v| {
                            dynamic_name(v) != dynamic_name(n.velocity)
                        }) {
                            self.draw_dynamic(page, n.velocity, bx + self.nh_width / 2.0, dyn_y);
                        }
                    }
                    if let Some(h) = beat.effects.hairpin {
                        self.wedges.borrow_mut().push((
                            h,
                            (bi, i),
                            page_index,
                            system,
                            bx,
                            bx + bw,
                            dyn_y,
                        ));
                    }
                }
                // Vibrato: a wavy line through the beat, over the tab (or the lone staff).
                if self.vibrato
                    && (beat.effects.vibrato || beat.notes.iter().any(|n| n.effects.vibrato))
                {
                    let y = if self.tab {
                        tab_top - 1.0 * sp
                    } else {
                        top - 1.5 * sp
                    };
                    let len = (bw - 0.5 * sp).clamp(self.nh_width, 3.0 * sp);
                    self.draw_vibrato(page, bx, bx + len, y);
                }
                // Text, chord names, lyrics: once, on the track's top staff.
                if self.first_staff {
                    if let Some(text) = &beat.text {
                        page.text(
                            FontKind::Italic,
                            text.clone(),
                            (bx, top - 3.2 * sp),
                            1.8 * sp,
                            Align::Left,
                        );
                    }
                    if beat.effects.rasgueado {
                        // Above the staff and clear of an up stem.
                        let rasg_y = chord.as_ref().map_or(top, |(ys, up)| {
                            let high = ys.iter().cloned().fold(top, f64::min);
                            high - if *up { 3.6 * sp } else { 1.0 * sp }
                        }) - 0.6 * sp;
                        page.text(
                            FontKind::Italic,
                            "rasg.",
                            (bx, rasg_y),
                            1.6 * sp,
                            Align::Left,
                        );
                    }
                    if let Some(c) = &beat.chord {
                        if !c.name.is_empty() {
                            page.text(
                                FontKind::Bold,
                                c.name.clone(),
                                (bx, top - 5.2 * sp),
                                2.0 * sp,
                                Align::Left,
                            );
                        }
                    }
                    if self.lyrics > 0 && self.lyric_source.is_none() {
                        self.draw_syllables(
                            page,
                            &beat.lyrics,
                            bx + self.nh_width / 2.0,
                            (std_bottom, tab_top),
                            page_index,
                        );
                    }
                }
                // Tablature.
                if self.tab {
                    self.draw_tab_beat(
                        page,
                        beat,
                        bx,
                        tab_top,
                        (bi, vi, i),
                        (page_index, system),
                        placed,
                    );
                }
                // Rests of a tab+ staff sit in the middle of the strings.
                if self.rhythm && beat.notes.is_empty() {
                    let tab_bottom = tab_top + (strings - 1) as f64 * 1.5 * sp;
                    let mid = (tab_top + tab_bottom) / 2.0;
                    let glyph = rest_glyph(beat.duration.value);
                    let r = fonts().bbox(FontKind::Music, glyph, 4.0 * sp);
                    let gx = bx + self.nh_width / 2.0 - (r.x0 + r.x1) / 2.0;
                    page.glyph(glyph, (gx, mid - (r.y0 + r.y1) / 2.0), 4.0 * sp);
                    for d in 0..beat.duration.dots {
                        page.glyph(
                            DOT,
                            (gx + r.x1 + (0.4 + d as f64 * 0.6) * sp, mid - 0.5 * sp),
                            4.0 * sp,
                        );
                    }
                }
                // Rhythm below a tab+ staff, beamed by beat like standard notation; rests and
                // whole notes break the beams.
                if self.rhythm {
                    let value = beat.duration.value;
                    let unit = self.beam_unit(mb);
                    let (first, last) = (start / unit, (t - 1) / unit);
                    let level = beam_count(value);
                    if beat.notes.is_empty() || level == 0 || first != rhythm_beat || first != last
                    {
                        self.flush_rhythm(page, &mut rhythm, tab_top);
                    }
                    if !beat.notes.is_empty() && value == DurationValue::Whole {
                        // A whole note: an open circle where a stem would be.
                        let tab_bottom = tab_top + (strings - 1) as f64 * 1.5 * sp;
                        let centre = (bx + self.nh_width / 2.0, tab_bottom + 2.9 * sp);
                        page.items.push(Item::Path {
                            path: kurbo::Circle::new(centre, 0.55 * sp).to_path(0.01),
                            color: Color::BLACK,
                            stroke: Some(0.14 * sp),
                        });
                    } else if !beat.notes.is_empty() {
                        let x = bx + self.nh_width / 2.0;
                        rhythm.push((start as usize, x, value, beat.duration.dots));
                        rhythm_beat = first;
                        if level == 0 || first != last {
                            self.flush_rhythm(page, &mut rhythm, tab_top);
                        }
                    }
                }
            }
            if self.standard {
                self.flush_beams(page, &mut beamable, std_bottom);
            }
            self.flush_rhythm(page, &mut rhythm, tab_top);
            tuplets.push((std::mem::take(&mut tuplet_group), tuplet_ratio));
            for (group, ratio) in tuplets {
                self.draw_tuplet(page, &group, ratio, std_top);
            }
        }
        // A grand staff's half with nothing in the bar shows a whole-bar rest.
        if self.half.is_some() && !shown {
            page.glyph(
                rest_glyph(DurationValue::Whole),
                (x + width / 2.0 - 0.7 * sp, self.staff_y(std_bottom, 6)),
                4.0 * sp,
            );
        }
    }

    fn repeat_dots(&self, page: &mut Page, x: f64, a: f64, b: f64) {
        let mid = (a + b) / 2.0;
        let r = 0.25 * self.sp;
        for y in [mid - 0.5 * self.sp, mid + 0.5 * self.sp] {
            page.items.push(Item::Path {
                path: kurbo::Shape::to_path(&kurbo::Circle::new((x, y), r), 0.1),
                color: Color::BLACK,
                stroke: None,
            });
        }
    }

    /// Beams break at each beat (quarter, or dotted quarter in compound meters).
    /// Length of the beats that beams group, in ticks.
    fn beam_unit(&self, mb: &MasterBar) -> u32 {
        let ts = mb.time_signature;
        if ts.denominator == 8 && ts.numerator % 3 == 0 {
            1440
        } else if ts.denominator == 2 {
            1920
        } else {
            960
        }
    }

    fn draw_dots(&self, page: &mut Page, beat: &Beat, x: f64, bottom: f64, p: i32) {
        for d in 0..beat.duration.dots {
            page.glyph(
                DOT,
                (x + d as f64 * 0.6 * self.sp, self.staff_y(bottom, p)),
                4.0 * self.sp,
            );
        }
    }

    /// Draws noteheads, ledger lines, accidentals and dots; returns (stem x, note ys, stem up).
    #[allow(clippy::too_many_arguments)]
    fn draw_chord(
        &self,
        page: &mut Page,
        beat: &Beat,
        x: f64,
        bottom: f64,
        up_voice: Option<bool>,
        key_alter: [i32; 7],
        alters: &mut HashMap<i32, i32>,
        pos: (usize, usize, usize),
        page_index: usize,
        system: usize,
        placed: &mut HashMap<(usize, usize, usize, u8), Placed>,
    ) -> (f64, Vec<f64>, bool) {
        let sp = self.sp;
        let key = self.score.master_bars[pos.0].key_signature.accidentals;
        // (staff position, accidental, x notehead, string, tie, ghost)
        let mut heads: Vec<(i32, Option<char>, bool, u8, bool, bool)> = Vec::new();
        // Open hi-hats are marked with a small circle.
        let mut open: Vec<i32> = Vec::new();
        for n in &beat.notes {
            // Slashes sit on the middle line, one per beat whatever its pitches.
            if self.slash {
                heads.push((4, None, false, n.string, n.tie, false));
                continue;
            }
            if self.clef == Clef::Percussion {
                let (p, xhead) = drum_position(n.fret);
                if n.fret == 46 {
                    open.push(p);
                }
                heads.push((p, None, xhead, n.string, n.tie, n.ghost));
                continue;
            }
            let written = self.track.pitch_of(n) as i32 + self.clef.transpose();
            let (step, alter, octave) = spell(written, key);
            let d = octave * 7 + step;
            let p = d - self.clef.bottom();
            let current = alters.get(&d).copied().unwrap_or(key_alter[step as usize]);
            let acc = if n.tie {
                None
            } else if current != alter {
                alters.insert(d, alter);
                Some(match alter {
                    1 => SHARP,
                    -1 => FLAT,
                    _ => NATURAL,
                })
            } else {
                None
            };
            heads.push((p, acc, n.dead, n.string, n.tie, n.ghost));
        }
        heads.sort_by_key(|h| h.0);
        let avg = heads.iter().map(|h| h.0).sum::<i32>() as f64 / heads.len().max(1) as f64;
        // Slashes always have their stems up.
        let up = self.slash || up_voice.unwrap_or(avg < 4.0);
        let head = match (self.slash, beat.duration.value) {
            (false, DurationValue::Whole) => HEAD_WHOLE,
            (false, DurationValue::Half) => HEAD_HALF,
            (false, _) => HEAD_BLACK,
            (true, DurationValue::Whole) => SLASH_WHOLE,
            (true, DurationValue::Half) => SLASH_HALF,
            (true, _) => SLASH_BLACK,
        };
        let mut ys = Vec::new();
        // Left edge of the noteheads and their accidentals, for an arpeggio sign.
        let mut left = x;
        let paren_w = |ghost: bool| {
            if ghost {
                fonts().bbox(FontKind::Music, PAREN_LEFT, 4.0 * sp).width() + 0.1 * sp
            } else {
                0.0
            }
        };
        let mut prev_p: Option<i32> = None;
        let mut shifted = false;
        let at = BeatRef {
            track: self.track_index,
            bar: pos.0,
            voice: pos.1,
            beat: pos.2,
        };
        // The slash a slash beat's notes share.
        let mut slash_item = None;
        for (k, &(p, acc, xhead, string, _, ghost)) in heads.iter().enumerate() {
            // Seconds sit on the other side of the stem.
            let second = prev_p.is_some_and(|q| p - q == 1) && !shifted;
            shifted = second;
            prev_p = Some(p);
            let hx = if second {
                if up {
                    x + self.nh_width
                } else {
                    x - self.nh_width
                }
            } else {
                x
            };
            let y = self.staff_y(bottom, p);
            ys.push(y);
            let g = if xhead { HEAD_X } else { head };
            // A slash beat's notes share one slash.
            if !(self.slash && k > 0) {
                page.glyph(g, (hx, y), 4.0 * sp);
                slash_item = page.items.len().checked_sub(1);
            }
            if let Some(item) = slash_item {
                page.marks.push(NoteMark {
                    item,
                    kind: MarkKind::Body,
                    beat: at,
                    string,
                    staff: self.staff_index,
                });
            }
            self.reach(y + 0.6 * sp);
            left = left.min(hx - paren_w(ghost));
            // Ghost notes are bracketed.
            let paren = if ghost {
                let left = fonts().bbox(FontKind::Music, PAREN_LEFT, 4.0 * sp);
                page.glyph(PAREN_LEFT, (hx - left.x1 - 0.1 * sp, y), 4.0 * sp);
                page.glyph(PAREN_RIGHT, (hx + self.nh_width + 0.1 * sp, y), 4.0 * sp);
                left.width() + 0.2 * sp
            } else {
                0.0
            };
            if open.contains(&p) {
                page.glyph(OPEN, (hx, self.staff_y(bottom, p.max(9) + 3)), 4.0 * sp);
            }
            // Ledger lines.
            let lx0 = hx - 0.4 * sp;
            let lx1 = hx + self.nh_width + 0.4 * sp;
            let mut q = -2;
            while q >= p {
                page.line(
                    (lx0, self.staff_y(bottom, q)),
                    (lx1, self.staff_y(bottom, q)),
                    0.16 * sp,
                );
                q -= 2;
            }
            let mut q = 10;
            while q <= p {
                page.line(
                    (lx0, self.staff_y(bottom, q)),
                    (lx1, self.staff_y(bottom, q)),
                    0.16 * sp,
                );
                q += 2;
            }
            if let Some(a) = acc {
                let stagger = (k % 2) as f64 * 0.9 * sp;
                page.glyph(a, (x - 1.2 * sp - stagger - paren, y), 4.0 * sp);
                left = left.min(x - 1.2 * sp - stagger - paren);
            }
            self.draw_dots(
                page,
                beat,
                x + self.nh_width + 0.5 * sp,
                bottom,
                if p % 2 == 0 { p + 1 } else { p },
            );
            placed.insert(
                (pos.0, pos.1, pos.2, string),
                Placed {
                    page: page_index,
                    system,
                    x: hx,
                    y,
                    tab_y: None,
                    // Ties and slurs go outward from the outer notes of a chord, otherwise
                    // away from the stem.
                    above: if heads.len() > 1 && k + 1 == heads.len() {
                        true
                    } else if heads.len() > 1 && k == 0 {
                        false
                    } else {
                        !up
                    },
                    std_top: bottom - 4.0 * sp,
                    tab_top: 0.0,
                    tab_w: 0.0,
                },
            );
        }
        if let Some(st) = beat.effects.stroke {
            let lo = ys.iter().cloned().fold(f64::MAX, f64::min);
            let hi = ys.iter().cloned().fold(f64::MIN, f64::max);
            self.draw_stroke(page, st, left - 0.7 * sp, lo - 0.6 * sp, hi + 0.6 * sp);
        }
        let stem_x = if up {
            x + self.nh_width - 0.06 * sp
        } else {
            x + 0.06 * sp
        };
        (stem_x, ys, up)
    }

    fn draw_stem(&self, page: &mut Page, x: f64, ys: &[f64], up: bool, to: Option<f64>) {
        let sp = self.sp;
        let (lo, hi) = (
            ys.iter().cloned().fold(f64::MAX, f64::min),
            ys.iter().cloned().fold(f64::MIN, f64::max),
        );
        let (from, end) = if up {
            (hi, to.unwrap_or(lo - 3.5 * sp))
        } else {
            (lo, to.unwrap_or(hi + 3.5 * sp))
        };
        page.line((x, from), (x, end), 0.15 * sp);
        self.reach(end.max(hi + 0.6 * sp));
        self.stems.borrow_mut().push((x, end, up));
    }

    /// Stems below a tab+ staff for collected beats, all ending on one line, joined by flat
    /// beams (more levels stacked upwards; a short beam for a lone note or a partial one), a
    /// half note's stem half as long; then clears them.
    fn flush_rhythm(&self, page: &mut Page, group: &mut Vec<RhythmBeat>, tab_top: f64) {
        let sp = self.sp;
        let tab_bottom = tab_top + (self.track.strings.len().max(1) - 1) as f64 * 1.5 * sp;
        let (top, bottom) = (tab_bottom + 0.8 * sp, tab_bottom + 5.0 * sp);
        let thick = 0.45 * sp;
        // The beams: (left, right, level), level 0 at the stems' ends.
        let mut beams: Vec<(f64, f64, usize)> = Vec::new();
        if group.len() > 1 {
            let levels = group.iter().map(|g| beam_count(g.2)).max().unwrap_or(0);
            for level in 0..levels {
                let mut i = 0;
                while i < group.len() {
                    if beam_count(group[i].2) <= level {
                        i += 1;
                        continue;
                    }
                    let mut j = i;
                    while j + 1 < group.len() && beam_count(group[j + 1].2) > level {
                        j += 1;
                    }
                    let (mut xa, mut xb) = (group[i].1, group[j].1);
                    if i == j {
                        // A partial beam points back when its note completes a group of its
                        // value's length (as after a dotted note), otherwise forward.
                        let pair = 2 * group[i].2.ticks() as usize;
                        let back = i > 0
                            && group
                                .get(i + 1)
                                .is_none_or(|next| next.0 % pair == 0 || group[i].0 % pair != 0);
                        if back {
                            xa -= 1.2 * sp;
                        } else {
                            xb += 1.2 * sp;
                        }
                    }
                    beams.push((xa - 0.06 * sp, xb + 0.06 * sp, level));
                    i = j + 1;
                }
            }
        }
        for &(xa, xb, level) in &beams {
            let y = bottom - level as f64 * 0.75 * sp;
            page.rect(Rect::new(xa, y - thick, xb, y), Color::BLACK);
        }
        for &(_, x, value, dots) in group.iter() {
            let from = if value == DurationValue::Half {
                (top + bottom) / 2.0
            } else {
                top
            };
            page.line((x, from), (x, bottom), 0.12 * sp);
            self.stems.borrow_mut().push((x, bottom, false));
            // A lone note's flag hangs from the stem's end, curving up.
            let lone = group.len() == 1 && beam_count(value) > 0;
            if lone {
                page.glyph(flag_glyph(value, false), (x - 0.06 * sp, bottom), 4.0 * sp);
            }
            if dots == 0 {
                continue;
            }
            // Dots as in standard notation: just above a beamed note's own top beam, else at
            // the foot of the stem (past the flag of a lone note).
            let own = beam_count(value);
            let (x0, y) = if lone {
                let flag = fonts().bbox(FontKind::Music, flag_glyph(value, false), 4.0 * sp);
                (x + flag.x1 + 0.25 * sp, bottom - 0.3 * sp)
            } else if own > 0 {
                let top_beam = bottom - (own - 1) as f64 * 0.75 * sp - thick;
                (x + 0.3 * sp, top_beam - 0.35 * sp)
            } else {
                (x + 0.3 * sp, bottom - 0.3 * sp)
            };
            for d in 0..dots {
                page.glyph(DOT, (x0 + d as f64 * 0.6 * sp, y), 4.0 * sp);
            }
        }
        group.clear();
    }

    /// Draws beams (or a flag for a lone note) for collected beats, then clears them.
    fn flush_beams(
        &self,
        page: &mut Page,
        group: &mut Vec<(usize, f64, Vec<f64>, DurationValue, u32)>,
        _bottom: f64,
    ) {
        let sp = self.sp;
        if group.len() == 1 {
            let (_, x, ys, v, up) = &group[0];
            let up = *up == 1;
            self.draw_stem(page, *x, ys, up, None);
            let lo = ys.iter().cloned().fold(f64::MAX, f64::min);
            let hi = ys.iter().cloned().fold(f64::MIN, f64::max);
            let end = if up { lo - 3.5 * sp } else { hi + 3.5 * sp };
            page.glyph(flag_glyph(*v, up), (*x - 0.06 * sp, end), 4.0 * sp);
            if !up {
                self.reach(end + 0.5 * sp);
            }
        } else if group.len() > 1 {
            // The group's stem direction follows the majority.
            let ups = group.iter().filter(|g| g.4 == 1).count();
            let up = ups * 2 >= group.len();
            // Stems outvoted by the group move to the other side of the notehead.
            let shift = self.nh_width - 0.12 * sp;
            for g in group.iter_mut() {
                match (g.4 == 1, up) {
                    (true, false) => g.1 -= shift,
                    (false, true) => g.1 += shift,
                    _ => {}
                }
            }
            let ends: Vec<f64> = group
                .iter()
                .map(|(_, _, ys, _, _)| {
                    if up {
                        ys.iter().cloned().fold(f64::MAX, f64::min) - 3.5 * sp
                    } else {
                        ys.iter().cloned().fold(f64::MIN, f64::max) + 3.5 * sp
                    }
                })
                .collect();
            let (x0, x1) = (group[0].1, group[group.len() - 1].1);
            let extreme = if up {
                ends.iter().cloned().fold(f64::MAX, f64::min)
            } else {
                ends.iter().cloned().fold(f64::MIN, f64::max)
            };
            let slope = ((ends[ends.len() - 1] - ends[0]) / (x1 - x0).max(1.0)).clamp(-0.15, 0.15);
            // Beam line through the extreme end, with the clamped slope.
            let anchor = ends
                .iter()
                .zip(group.iter())
                .map(|(e, g)| (*e, g.1))
                .find(|(e, _)| (*e - extreme).abs() < 1e-6)
                .unwrap();
            let beam_y = |x: f64| anchor.0 + slope * (x - anchor.1);
            for (_, x, ys, _, _) in group.iter() {
                self.draw_stem(page, *x, ys, up, Some(beam_y(*x)));
            }
            let thick = 0.5 * sp;
            let gap = 0.75 * sp;
            let max_level = group.iter().map(|g| beam_count(g.3)).max().unwrap_or(1);
            for level in 0..max_level {
                let off = level as f64 * gap * if up { 1.0 } else { -1.0 };
                let mut i = 0;
                while i < group.len() {
                    if beam_count(group[i].3) <= level {
                        i += 1;
                        continue;
                    }
                    let mut j = i;
                    while j + 1 < group.len() && beam_count(group[j + 1].3) > level {
                        j += 1;
                    }
                    let (xa, mut xb) = (group[i].1, group[j].1);
                    let mut xa = xa;
                    if i == j {
                        // A partial beam points back when its note completes a group of its
                        // value's length (as after a dotted note), otherwise forward.
                        let pair = 2 * group[i].3.ticks() as usize;
                        let back = i > 0
                            && group
                                .get(i + 1)
                                .is_none_or(|next| next.0 % pair == 0 || group[i].0 % pair != 0);
                        if back {
                            xa -= 1.2 * sp
                        } else {
                            xb = xa + 1.2 * sp
                        }
                    }
                    let (ya, yb) = (beam_y(xa) + off, beam_y(xb) + off);
                    let dir = if up { 1.0 } else { -1.0 };
                    let mut p = BezPath::new();
                    p.move_to((xa, ya));
                    p.line_to((xb, yb));
                    p.line_to((xb, yb + thick * dir));
                    p.line_to((xa, ya + thick * dir));
                    p.close_path();
                    page.items.push(Item::Path {
                        path: p,
                        color: Color::BLACK,
                        stroke: None,
                    });
                    i = j + 1;
                }
            }
        }
        group.clear();
    }

    /// The tuplet number centred in a bracket beyond the stems of the group, parallel to its
    /// beam (just the number above the staff for a group without stems).
    fn draw_tuplet(
        &self,
        page: &mut Page,
        group: &[TupletNote],
        ratio: Option<Tuplet>,
        std_top: f64,
    ) {
        let sp = self.sp;
        let (Some(tp), Some(first), Some(last)) = (ratio, group.first(), group.last()) else {
            return;
        };
        // A tab without rhythm has no tuplets to mark.
        if self.tab && !self.rhythm {
            return;
        }
        let size = 3.0 * sp;
        let digits: Vec<char> = tp
            .enters
            .to_string()
            .chars()
            .filter_map(|c| c.to_digit(10))
            .map(tuplet_digit)
            .collect();
        let w: f64 = digits
            .iter()
            .map(|&d| fonts().advance(FontKind::Music, d, size))
            .sum();
        let bb = digits
            .first()
            .map_or(Rect::ZERO, |&d| fonts().bbox(FontKind::Music, d, size));
        let number = |page: &mut Page, x: f64, y: f64| {
            let mut cx = x - w / 2.0;
            for &d in &digits {
                page.glyph(d, (cx, y - (bb.y0 + bb.y1) / 2.0), size);
                cx += fonts().advance(FontKind::Music, d, size);
            }
        };
        // The stem of each note of the group.
        let stems = self.stems.borrow();
        let ends: Vec<(f64, f64, bool)> = group
            .iter()
            .filter(|g| g.2.is_some())
            .filter_map(|g| {
                stems
                    .iter()
                    .rev()
                    .find(|s| s.0 > g.0 - 0.5 * sp && s.0 < g.1 + 0.5 * sp)
                    .copied()
            })
            .collect();
        let (x0, x1) = (first.0 - 0.2 * sp, last.1 + 0.2 * sp);
        let mid = (x0 + x1) / 2.0;
        if !(self.standard || self.rhythm) || ends.is_empty() {
            number(page, mid, std_top - 1.4 * sp);
            return;
        }
        let up = ends.iter().filter(|e| e.2).count() * 2 >= ends.len();
        // Stems the other way in a mixed group are left out.
        let ends: Vec<(f64, f64, bool)> = ends.into_iter().filter(|e| e.2 == up).collect();
        // The bracket's ends stand on the first and last stems.
        let (x0, x1) = (ends[0].0, ends[ends.len() - 1].0);
        let mid = (x0 + x1) / 2.0;
        let dir = if up { -1.0 } else { 1.0 };
        let (a, b) = (ends[0], ends[ends.len() - 1]);
        let slope = if b.0 - a.0 > 1.0 {
            (b.1 - a.1) / (b.0 - a.0)
        } else {
            0.0
        };
        let line = |x: f64| a.1 + slope * (x - a.0);
        // Clear every stem end, then keep the bracket off the staff.
        let excess = ends
            .iter()
            .map(|e| dir * (e.1 - line(e.0)))
            .fold(0.0, f64::max);
        let mut off = dir * (1.0 * sp + excess);
        let ends_y = [line(x0) + off, line(x1) + off];
        if up {
            let worst = ends_y[0].max(ends_y[1]);
            off -= (worst - (std_top - 0.8 * sp)).max(0.0);
        } else {
            let worst = ends_y[0].min(ends_y[1]);
            off += ((std_top + 4.8 * sp) - worst).max(0.0);
        }
        let y = |x: f64| line(x) + off;
        let hook = -dir * 0.7 * sp;
        let gap = w / 2.0 + 0.4 * sp;
        let t = 0.1 * sp;
        page.line((x0, y(x0) + hook), (x0, y(x0)), t);
        page.line((x0, y(x0)), (mid - gap, y(mid - gap)), t);
        page.line((mid + gap, y(mid + gap)), (x1, y(x1)), t);
        page.line((x1, y(x1)), (x1, y(x1) + hook), t);
        number(page, mid, y(mid));
        if !up {
            self.reach(y(x0).max(y(x1)) + 1.0 * sp);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_tab_beat(
        &self,
        page: &mut Page,
        beat: &Beat,
        x: f64,
        tab_top: f64,
        pos: (usize, usize, usize),
        (page_index, system): (usize, usize),
        placed: &mut HashMap<(usize, usize, usize, u8), Placed>,
    ) {
        let sp = self.sp;
        let size = 1.9 * sp;
        let cx = x + self.nh_width / 2.0;
        let mut above_marks: Vec<String> = Vec::new();
        for n in &beat.notes {
            let y = tab_top + n.string as f64 * 1.5 * sp;
            let fret = if n.dead {
                "x".to_string()
            } else {
                n.fret.to_string()
            };
            let mut label = match n.effects.harmonic {
                Some(Harmonic::Natural) => format!("<{fret}>"),
                Some(_) => format!("{fret}<{}>", n.fret as u32 + 12),
                // Tied and ghost notes are bracketed.
                None if n.tie || n.ghost => format!("({fret})"),
                None => fret,
            };
            if n.effects.grace.is_some() {
                label = format!("{}{label}", n.effects.grace.map(|g| g.fret).unwrap_or(0));
            }
            // A tie shows its last note, grey in brackets; the notes between are only crossed
            // by the tie arcs.
            let shown = !n.tie || !self.tie_continues(pos, n.string);
            let w = if shown {
                fonts().text_width(FontKind::Text, &label, size)
            } else {
                0.0
            };
            if shown {
                page.rect(
                    Rect::new(
                        cx - w / 2.0 - 0.15 * sp,
                        y - 0.7 * sp,
                        cx + w / 2.0 + 0.15 * sp,
                        y + 0.7 * sp,
                    ),
                    Color::WHITE,
                );
                page.items.push(Item::Text {
                    font: FontKind::Text,
                    text: label,
                    at: Point::new(cx, y + 0.65 * sp),
                    size,
                    align: Align::Center,
                    color: if n.tie { Color::GREY } else { Color::BLACK },
                });
                let at = BeatRef {
                    track: self.track_index,
                    bar: pos.0,
                    voice: pos.1,
                    beat: pos.2,
                };
                page.mark_last(MarkKind::Number, at, n.string, self.staff_index);
            }
            let entry = placed
                .entry((pos.0, pos.1, pos.2, n.string))
                .or_insert(Placed {
                    page: page_index,
                    system,
                    x,
                    y: 0.0,
                    tab_y: None,
                    above: true,
                    std_top: 0.0,
                    tab_top,
                    tab_w: w,
                });
            entry.tab_y = Some(y);
            entry.tab_top = tab_top;
            entry.tab_w = w;
            entry.x = x;
            if let Some(b) = &n.effects.bend {
                // Arrow from the note up above the staff, labelled with the bend amount.
                let peak = b.points.iter().map(|p| p.value).max().unwrap_or(0);
                let top = tab_top - 2.0 * sp;
                let mut path = BezPath::new();
                path.move_to((cx + w / 2.0 + 0.2 * sp, y));
                path.quad_to((cx + 1.8 * sp, y), (cx + 1.8 * sp, top));
                page.items.push(Item::Path {
                    path,
                    color: Color::BLACK,
                    stroke: Some(0.12 * sp),
                });
                let amount = match peak {
                    4 => "full".to_string(),
                    2 => "½".to_string(),
                    v => format!("{}", v as f64 / 4.0),
                };
                page.text(
                    FontKind::Text,
                    amount,
                    (cx + 1.8 * sp, top - 0.4 * sp),
                    1.5 * sp,
                    Align::Center,
                );
            }
            if n.effects.palm_mute {
                above_marks.push("P.M.".into());
            }
            if n.effects.let_ring {
                above_marks.push("let ring".into());
            }
            // Slides to the next note are drawn as lines with the ties; slides in and out here.
            if n.effects
                .slides
                .iter()
                .any(|k| !matches!(k, SlideKind::Shift | SlideKind::Legato))
            {
                above_marks.push("sl.".into());
            }
        }
        if let Some(st) = beat.effects.stroke {
            let shown: Vec<&Note> = beat.notes.iter().filter(|n| !n.tie).collect();
            let lo = shown.iter().map(|n| n.string).min();
            let hi = shown.iter().map(|n| n.string).max();
            if let (Some(lo), Some(hi)) = (lo, hi) {
                let y = |s: u8| tab_top + s as f64 * 1.5 * sp;
                let w = shown
                    .iter()
                    .map(|n| fonts().text_width(FontKind::Text, &n.fret.to_string(), size))
                    .fold(0.0, f64::max);
                self.draw_stroke(
                    page,
                    st,
                    cx - w / 2.0 - 0.7 * sp,
                    y(lo) - 0.6 * sp,
                    y(hi) + 0.6 * sp,
                );
            }
        }
        above_marks.dedup();
        // Sustained marks (let ring, palm mute) are shown where they start.
        let previous = pos
            .2
            .checked_sub(1)
            .and_then(|i| self.track.bars.get(pos.0)?.voices.get(pos.1)?.beats.get(i));
        let sustained =
            |b: &Beat, f: fn(&NoteEffects) -> bool| b.notes.iter().any(|n| f(&n.effects));
        if let Some(prev) = previous {
            if sustained(prev, |e| e.let_ring) {
                above_marks.retain(|m| m != "let ring");
            }
            if sustained(prev, |e| e.palm_mute) {
                above_marks.retain(|m| m != "P.M.");
            }
        }
        if !above_marks.is_empty() {
            page.text(
                FontKind::Italic,
                above_marks.join(" "),
                (cx, tab_top - 0.9 * sp),
                1.5 * sp,
                Align::Center,
            );
        }
    }

    /// Whether the tie through the note on `string` at (bar, voice, beat) goes on to the next
    /// beat of its voice.
    fn tie_continues(&self, (bar, voice, beat): (usize, usize, usize), string: u8) -> bool {
        let beats_of = |b: usize| {
            self.track
                .bars
                .get(b)
                .and_then(|b| b.voices.get(voice))
                .map(|v| v.beats.as_slice())
                .unwrap_or_default()
        };
        let next = beats_of(bar)
            .iter()
            .skip(beat + 1)
            .chain(beats_of(bar + 1))
            .find(|b| !b.empty);
        next.and_then(|b| b.note_on_string(string))
            .is_some_and(|n| n.tie)
    }

    /// Ties between consecutive notes on the same string; slurs over runs of hammer-ons,
    /// pull-offs and legato slides, with H or P over each hammer or pull; and slide lines.
    /// All are split across systems when needed.
    fn draw_ties(
        &self,
        layout: &mut Layout,
        placed: &HashMap<(usize, usize, usize, u8), Placed>,
        systems: &[(usize, f64, f64)],
    ) {
        let sp = self.sp;
        let voices = self
            .track
            .bars
            .iter()
            .map(|b| b.voices.len())
            .max()
            .unwrap_or(0);
        let arc = |page: &mut Page, x0: f64, x1: f64, y: f64, dy: f64| {
            let mut p = BezPath::new();
            let mid = (x0 + x1) / 2.0;
            p.move_to((x0, y + dy * 0.4));
            p.quad_to((mid, y + dy * 2.2), (x1, y + dy * 0.4));
            p.quad_to((mid, y + dy * 1.8), (x0, y + dy * 0.4));
            page.items.push(Item::Path {
                path: p,
                color: Color::BLACK,
                stroke: None,
            });
        };
        // Where a system's notes start, after its clef and key.
        let system_start = |s: usize| {
            systems
                .get(s)
                .map_or(0.0, |s| s.1 + self.header_width() - sp)
        };
        let system_end = |s: usize| systems.get(s).map_or(0.0, |s| s.2);
        for v in 0..voices {
            // Consecutive notes on each string, in order: the earlier note's position and fret,
            // the later's, and how they connect.
            let mut links: Vec<Link> = Vec::new();
            let mut last: HashMap<u8, ((usize, usize, usize), &Note)> = HashMap::new();
            for (bi, bar) in self.track.bars.iter().enumerate() {
                let Some(voice) = bar.voices.get(v) else {
                    continue;
                };
                for (i, beat) in voice.beats.iter().enumerate() {
                    for n in &beat.notes {
                        let here = (bi, v, i);
                        if let Some((from, prev)) = last.insert(n.string, (here, n)) {
                            let slides = &prev.effects.slides;
                            links.push(Link {
                                string: n.string,
                                from,
                                to: here,
                                frets: (prev.fret, n.fret),
                                tie: n.tie,
                                legato: !n.tie
                                    && (prev.effects.hammer_pull
                                        || slides.contains(&SlideKind::Legato)),
                                slide: !n.tie
                                    && (slides.contains(&SlideKind::Legato)
                                        || slides.contains(&SlideKind::Shift)),
                            });
                        }
                    }
                }
            }
            let get = |pos: (usize, usize, usize), string: u8| {
                placed.get(&(pos.0, pos.1, pos.2, string)).copied()
            };

            // Ties on the standard staff; on a tab, arcs under the string.
            for l in links.iter().filter(|l| l.tie) {
                let (Some(a), Some(b)) = (get(l.from, l.string), get(l.to, l.string)) else {
                    continue;
                };
                if let (true, Some(ya), Some(yb)) = (self.tab, a.tab_y, b.tab_y) {
                    let c = self.nh_width / 2.0;
                    let (x0, dy) = (a.x + c + a.tab_w / 2.0 + 0.1 * sp, 0.4 * sp);
                    if a.system == b.system {
                        arc(
                            &mut layout.pages[a.page],
                            x0,
                            b.x + c - b.tab_w / 2.0 - 0.1 * sp,
                            ya + 0.3 * sp,
                            dy,
                        );
                    } else {
                        arc(
                            &mut layout.pages[a.page],
                            x0,
                            system_end(a.system),
                            ya + 0.3 * sp,
                            dy,
                        );
                        arc(
                            &mut layout.pages[b.page],
                            system_start(b.system),
                            b.x + c,
                            yb + 0.3 * sp,
                            dy,
                        );
                    }
                    continue;
                }
                if a.y == 0.0 || !self.standard {
                    continue;
                }
                let dy = if a.above { -0.6 * sp } else { 0.6 * sp };
                if a.system == b.system {
                    arc(&mut layout.pages[a.page], a.x + self.nh_width, b.x, a.y, dy);
                } else {
                    arc(
                        &mut layout.pages[a.page],
                        a.x + self.nh_width,
                        system_end(a.system),
                        a.y,
                        dy,
                    );
                    arc(
                        &mut layout.pages[b.page],
                        system_start(b.system),
                        b.x,
                        b.y,
                        dy,
                    );
                }
            }

            // Slide lines between the notes, marked "sl.".
            for l in links.iter().filter(|l| l.slide) {
                let (Some(a), Some(b)) = (get(l.from, l.string), get(l.to, l.string)) else {
                    continue;
                };
                let rising = l.frets.1 > l.frets.0;
                let mut line = |page: usize, (x0, y0): (f64, f64), (x1, y1): (f64, f64)| {
                    let page = &mut layout.pages[page];
                    page.line((x0, y0), (x1, y1), 0.12 * sp);
                    page.text(
                        FontKind::Italic,
                        "sl.",
                        ((x0 + x1) / 2.0, y0.min(y1) - 0.2 * sp),
                        1.1 * sp,
                        Align::Center,
                    );
                };
                let tilt = if rising { -0.35 * sp } else { 0.35 * sp };
                if let (Some(ya), Some(yb)) = (a.tab_y, b.tab_y) {
                    let c = self.nh_width / 2.0;
                    let x0 = a.x + c + a.tab_w / 2.0 + 0.2 * sp;
                    let x1 = b.x + c - b.tab_w / 2.0 - 0.2 * sp;
                    if a.system == b.system {
                        line(a.page, (x0, ya - tilt), (x1, yb + tilt));
                    } else {
                        line(a.page, (x0, ya - tilt), (x0 + 1.5 * sp, ya + tilt));
                    }
                }
                if self.standard && a.y != 0.0 && b.y != 0.0 {
                    let x0 = a.x + self.nh_width + 0.3 * sp;
                    let x1 = b.x - 0.3 * sp;
                    if a.system == b.system && a.y != b.y {
                        line(a.page, (x0, a.y), (x1, b.y));
                    } else if a.system == b.system {
                        line(a.page, (x0, a.y - tilt), (x1, b.y + tilt));
                    } else {
                        line(a.page, (x0, a.y - tilt), (x0 + 1.5 * sp, a.y + tilt));
                    }
                }
            }

            // Runs of legato links, one slur each.
            let mut k = 0;
            while k < links.len() {
                if !links[k].legato {
                    k += 1;
                    continue;
                }
                let mut run = vec![k];
                let mut next = k + 1;
                while let Some(j) = links[next..].iter().position(|l| {
                    let tail = &links[run[run.len() - 1]];
                    l.string == tail.string && l.from == tail.to
                }) {
                    if !links[next + j].legato {
                        break;
                    }
                    run.push(next + j);
                    next += j + 1;
                }
                // Links taken into this run are skipped later.
                for &j in &run[1..] {
                    links[j].legato = false;
                }
                let run: Vec<Link> = run.iter().map(|&j| links[j].clone()).collect();
                self.draw_legato(layout, &run, placed, &system_start, &system_end);
                k += 1;
            }
        }
    }

    /// A slur over a run of hammer-ons, pull-offs and legato slides on one string, on the tab
    /// (over the frets) and the standard staff (on the noteheads' side), with H or P over
    /// each hammer or pull. A run across systems is slurred in pieces.
    fn draw_legato(
        &self,
        layout: &mut Layout,
        run: &[Link],
        placed: &HashMap<(usize, usize, usize, u8), Placed>,
        system_start: &dyn Fn(usize) -> f64,
        system_end: &dyn Fn(usize) -> f64,
    ) {
        let sp = self.sp;
        let string = run[0].string;
        let notes: Vec<Placed> = std::iter::once(run[0].from)
            .chain(run.iter().map(|l| l.to))
            .filter_map(|p| placed.get(&(p.0, p.1, p.2, string)).copied())
            .collect();
        if notes.len() != run.len() + 1 {
            return;
        }
        let c = self.nh_width / 2.0;
        // Pieces of the run on one system each, as ranges of notes.
        let mut pieces: Vec<(usize, usize)> = Vec::new();
        let mut s = 0;
        for e in 1..=notes.len() {
            if e == notes.len() || notes[e].system != notes[s].system {
                pieces.push((s, e - 1));
                s = e;
            }
        }
        let tab = notes.iter().all(|n| n.tab_y.is_some());
        let standard = self.standard && notes.iter().all(|n| n.y != 0.0);
        for (pi, &(a, b)) in pieces.iter().enumerate() {
            let first = &notes[a];
            let page = first.page;
            let x0 = if pi == 0 {
                first.x + c
            } else {
                system_start(first.system)
            };
            let x1 = if pi + 1 == pieces.len() {
                notes[b].x + c
            } else {
                system_end(first.system)
            };
            let height = (0.5 * sp + 0.04 * (x1 - x0)).min(1.6 * sp);
            // Letters over the hammers and pulls starting in this piece.
            let letters: Vec<(f64, &str)> = (a..=b)
                .filter(|&j| j < run.len() && !run[j].slide)
                .map(|j| {
                    let to = &notes[j + 1];
                    let x = if to.system == notes[j].system {
                        (notes[j].x + to.x) / 2.0 + c
                    } else {
                        (notes[j].x + c + x1) / 2.0
                    };
                    let (from, to) = run[j].frets;
                    (x, if to < from { "P" } else { "H" })
                })
                .collect();
            if tab {
                let ys: Vec<f64> = notes[a..=b]
                    .iter()
                    .map(|n| n.tab_y.unwrap_or(0.0) - 0.8 * sp)
                    .collect();
                let (y0, y1) = (ys[0], ys[ys.len() - 1]);
                let peak = ys.iter().cloned().fold(f64::MAX, f64::min) - height;
                slur(&mut layout.pages[page], (x0, y0), (x1, y1), peak, 0.12 * sp);
                for &(x, t) in &letters {
                    layout.pages[page].text(
                        FontKind::Text,
                        t,
                        (x, peak - 0.4 * sp),
                        1.5 * sp,
                        Align::Center,
                    );
                }
            }
            if standard {
                let above = first.above;
                let dir = if above { -1.0 } else { 1.0 };
                let ys: Vec<f64> = notes[a..=b].iter().map(|n| n.y + dir * 0.8 * sp).collect();
                let (y0, y1) = (ys[0], ys[ys.len() - 1]);
                let peak = if above {
                    ys.iter().cloned().fold(f64::MAX, f64::min) - height
                } else {
                    ys.iter().cloned().fold(f64::MIN, f64::max) + height
                };
                slur(&mut layout.pages[page], (x0, y0), (x1, y1), peak, 0.12 * sp);
                let std_top = first.std_top;
                let y = if above {
                    (peak - 0.5 * sp).min(std_top - 1.0 * sp)
                } else {
                    (peak + 1.6 * sp).max(std_top + 5.6 * sp)
                };
                for &(x, t) in &letters {
                    layout.pages[page].text(FontKind::Text, t, (x, y), 1.5 * sp, Align::Center);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_bar_gets_a_cursor_position() {
        let empty = || Beat {
            empty: true,
            ..Beat::default()
        };
        let blank = || Bar {
            voices: vec![Voice {
                beats: vec![empty()],
            }],
        };
        let track = Track {
            kind: TrackKind::Fretted,
            strings: vec![64, 59, 55, 50, 45, 40],
            bars: vec![blank()],
            staves: vec![Staff::Standard(muse_core::Clef::Treble), Staff::Tab],
            ..Track::default()
        };
        let score = Score {
            tempo: 100,
            master_bars: vec![MasterBar::default()],
            tracks: vec![track],
            ..Score::default()
        };
        let layout = engrave(&score, 0, &Options::default());
        let at = BeatRef {
            track: 0,
            bar: 0,
            voice: 0,
            beat: 0,
        };
        let pos = layout.beats.get(&at).expect("position for the empty bar");
        assert!(pos.width > 0.0);
        assert!(
            pos.staves
                .iter()
                .any(|s| matches!(s, StaffPos::Strings { ys, .. } if ys.len() == 6))
        );
        assert!(
            pos.staves
                .iter()
                .any(|s| matches!(s, StaffPos::Lines { .. }))
        );
        // It draws no rest.
        let mut rested = score.clone();
        rested.tracks[0].bars[0].voices[0].beats[0].empty = false;
        let drawn = |l: &Layout| l.pages[0].items.len();
        assert!(drawn(&engrave(&rested, 0, &Options::default())) > drawn(&layout));

        // With a real beat beside it, an empty beat still has no position.
        let mut mixed = score.clone();
        mixed.tracks[0].bars[0].voices[0].beats = vec![Beat::default(), empty()];
        let layout = engrave(&mixed, 0, &Options::default());
        assert!(layout.beats.contains_key(&at));
        assert!(!layout.beats.contains_key(&BeatRef { beat: 1, ..at }));
    }

    #[test]
    fn spelling_and_keys() {
        assert_eq!(spell(61, 0), (0, 1, 4)); // C#4
        assert_eq!(spell(61, -2), (1, -1, 4)); // Db4
        assert_eq!(key_alters(2), [1, 0, 0, 1, 0, 0, 0]); // F#, C#
        assert_eq!(key_alters(-1), [0, 0, 0, 0, 0, 0, -1]); // Bb
    }

    #[test]
    fn note_color_reaches_the_other_staves() {
        let chord = Beat {
            duration: Duration::new(DurationValue::Quarter),
            notes: vec![Note::new(1, 1), Note::new(2, 2)],
            ..Beat::default()
        };
        let track = Track {
            kind: TrackKind::Fretted,
            strings: vec![64, 59, 55, 50, 45, 40],
            bars: vec![Bar {
                voices: vec![Voice { beats: vec![chord] }],
            }],
            staves: vec![
                Staff::Standard(muse_core::Clef::Treble),
                Staff::Tab,
                Staff::Tab,
                Staff::Slash,
            ],
            ..Track::default()
        };
        let score = Score {
            tempo: 100,
            master_bars: vec![MasterBar::default()],
            tracks: vec![track],
            ..Score::default()
        };
        let page = &engrave(&score, 0, &Options::default()).pages[0];
        let beat = BeatRef {
            track: 0,
            bar: 0,
            voice: 0,
            beat: 0,
        };
        let red = Color::RED;
        // The cursor on the first tab: the treble head, the other tab's number and the slash.
        let lit = page.with_note_color(beat, 2, 1, red).unwrap();
        let changed: Vec<(usize, &Item)> = lit
            .items
            .iter()
            .enumerate()
            .filter(|(i, it)| **it != page.items[*i])
            .collect();
        assert_eq!(changed.len(), 3);
        assert!(changed.iter().any(|(_, i)| matches!(
            i,
            Item::Text { font: FontKind::Bold, text, color, .. } if text == "2" && *color == red
        )));
        assert!(
            changed
                .iter()
                .any(|(_, i)| matches!(i, Item::Glyph { color, .. } if *color == red))
        );
        // Only the chosen note: the other string's number stays plain.
        assert!(lit.items.iter().any(|i| matches!(
            i,
            Item::Text { font: FontKind::Text, text, .. } if text == "1"
        )));
        // A note the beat does not have lights nothing.
        assert!(page.with_note_color(beat, 5, 1, red).is_none());
    }

    #[test]
    fn staff_notations() {
        let chord = Beat {
            duration: Duration::new(DurationValue::Quarter),
            notes: vec![Note::new(1, 1), Note::new(2, 2)],
            ..Beat::default()
        };
        let mut score = Score {
            tempo: 100,
            master_bars: vec![MasterBar::default()],
            tracks: vec![Track {
                kind: TrackKind::Fretted,
                strings: vec![64, 59, 55, 50, 45, 40],
                bars: vec![Bar {
                    voices: vec![Voice {
                        beats: vec![chord; 4],
                    }],
                }],
                ..Track::default()
            }],
            ..Score::default()
        };
        let glyphs = |score: &Score| -> Vec<char> {
            engrave(score, 0, &Options::default()).pages[0]
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Glyph { ch, .. } => Some(*ch),
                    _ => None,
                })
                .collect()
        };
        let both = glyphs(&score);
        assert!(both.contains(&G_CLEF_8VB) && both.contains(&TAB_CLEF));
        // A cursor can find staff positions on the standard staff: E4 written (diatonic 30)
        // on the bottom line, a step half a space.
        let layout = engrave(&score, 0, &Options::default());
        let pos = layout.beats.values().next().unwrap();
        let (bottom_line, step) = pos.line_y(0, 30).unwrap();
        assert_eq!(step, Options::default().sp / 2.0);
        assert!(bottom_line < pos.strings(1).unwrap()[0]);
        assert_eq!(pos.position_at(0, bottom_line), Some(30));
        // Tab only: no clef, no stems.
        score.tracks[0].staves = vec![Staff::Tab];
        let tab = glyphs(&score);
        assert!(!tab.contains(&G_CLEF_8VB) && tab.contains(&TAB_CLEF));
        let layout = engrave(&score, 0, &Options::default());
        assert!(layout.beats.values().all(|p| p.line_y(0, 30).is_none()));
        let lines = |score: &Score| -> usize {
            engrave(score, 0, &Options::default()).pages[0]
                .items
                .iter()
                .filter(|i| matches!(i, Item::Line { .. }))
                .count()
        };
        let plain = lines(&score);
        // Tab+: the same with a stem per beat.
        score.tracks[0].staves = vec![Staff::TabRhythm];
        assert_eq!(lines(&score), plain + 4);
        // Slashes: one per beat, no clef and no tab.
        score.tracks[0].staves = vec![Staff::Slash];
        let slash = glyphs(&score);
        assert_eq!(slash.iter().filter(|&&c| c == SLASH_BLACK).count(), 4);
        assert!(!slash.contains(&G_CLEF_8VB) && !slash.contains(&TAB_CLEF));
        // Any number of staves, in order, all showing the same beats.
        score.tracks[0].staves = vec![
            Staff::Tab,
            Staff::Slash,
            Staff::Standard(muse_core::Clef::Bass),
        ];
        let layout = engrave(&score, 0, &Options::default());
        let pos = layout.beats.values().next().unwrap();
        let tab_y = pos.strings(0).unwrap()[0];
        let (slash_y, _) = pos.line_y(1, 34).unwrap();
        assert!(tab_y < slash_y);
        assert!(pos.line_y(2, 20).is_some_and(|(y, _)| y > slash_y));
    }

    #[test]
    fn staves_line_up_with_a_key_signature() {
        let mut score = Score {
            tempo: 100,
            master_bars: vec![MasterBar::default()],
            tracks: vec![Track {
                kind: TrackKind::Fretted,
                strings: vec![64, 59, 55, 50, 45, 40],
                bars: vec![Bar {
                    voices: vec![Voice {
                        beats: vec![Beat {
                            duration: Duration::new(DurationValue::Whole),
                            notes: vec![Note::new(0, 3)],
                            ..Beat::default()
                        }],
                    }],
                }],
                ..Track::default()
            }],
            ..Score::default()
        };
        score.master_bars[0].key_signature.accidentals = 4;
        let page = &engrave(&score, 0, &Options::default()).pages[0];
        let head = page
            .items
            .iter()
            .find_map(|i| match i {
                Item::Glyph { ch, at, .. } if *ch == HEAD_WHOLE => Some(at.x),
                _ => None,
            })
            .unwrap();
        let fret = page
            .items
            .iter()
            .find_map(|i| match i {
                Item::Text { text, at, .. } if text == "3" => Some(at.x),
                _ => None,
            })
            .unwrap();
        // The fret number is centred under the notehead.
        let nh = fonts()
            .bbox(FontKind::Music, HEAD_BLACK, 4.0 * Options::default().sp)
            .width();
        assert!((fret - (head + nh / 2.0)).abs() < 0.5, "{fret} vs {head}");
    }

    #[test]
    fn tab_rests_and_time_signature() {
        let sp = Options::default().sp;
        let rest = |dots| {
            let mut d = Duration::new(DurationValue::Quarter);
            d.dots = dots;
            Beat::rest(d)
        };
        let note = Beat {
            duration: Duration::new(DurationValue::Quarter),
            notes: vec![Note::new(0, 0)],
            ..Beat::default()
        };
        let make = |staff, ts: (u8, u8)| {
            let mut mb = MasterBar::default();
            mb.time_signature.numerator = ts.0 as _;
            mb.time_signature.denominator = ts.1 as _;
            Score {
                tempo: 100,
                master_bars: vec![mb],
                tracks: vec![Track {
                    kind: TrackKind::Fretted,
                    strings: vec![64, 59, 55, 50, 45, 40],
                    staves: vec![staff],
                    bars: vec![Bar {
                        voices: vec![Voice {
                            beats: vec![note.clone(), rest(1)],
                        }],
                    }],
                    ..Track::default()
                }],
                ..Score::default()
            }
        };
        let glyphs = |score: &Score| -> Vec<(char, Point, f64)> {
            let page = &engrave(score, 0, &Options::default()).pages[0];
            let mut g = Vec::new();
            for i in &page.items {
                if let Item::Glyph { ch, at, size, .. } = i {
                    g.push((*ch, *at, *size));
                }
            }
            g
        };
        let lines = |score: &Score| -> (f64, f64) {
            let page = &engrave(score, 0, &Options::default()).pages[0];
            let ys: Vec<f64> = page
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Line { from, to, .. } if from.y == to.y && to.x - from.x > 3.0 * sp => {
                        Some(from.y)
                    }
                    _ => None,
                })
                .collect();
            (
                ys.iter().cloned().fold(f64::MAX, f64::min),
                ys.iter().cloned().fold(f64::MIN, f64::max),
            )
        };
        for staff in [Staff::TabRhythm, Staff::Tab] {
            let score = make(staff, (6, 8));
            let (top, bottom) = lines(&score);
            let mid = (top + bottom) / 2.0;
            let g = glyphs(&score);
            // The time signature: standard-size digits just above and below the middle line.
            let six = g.iter().find(|x| x.0 == time_digit(6)).unwrap();
            let eight = g.iter().find(|x| x.0 == time_digit(8)).unwrap();
            assert_eq!((six.2, eight.2), (4.0 * sp, 4.0 * sp));
            assert!(
                (six.1.y - (mid - sp)).abs() < 0.01,
                "{staff:?} {} {top} {bottom}",
                six.1.y
            );
            assert!((eight.1.y - (mid + sp)).abs() < 0.01, "{staff:?}");
            // Only tab+ draws the rest, centred on the strings.
            let rests: Vec<_> = g
                .iter()
                .filter(|x| x.0 == rest_glyph(DurationValue::Quarter))
                .collect();
            if staff == Staff::TabRhythm {
                assert_eq!(rests.len(), 1);
                let b = fonts().bbox(FontKind::Music, rests[0].0, rests[0].2);
                let centre = rests[0].1.y + (b.y0 + b.y1) / 2.0;
                assert!((centre - mid).abs() < 0.01, "{centre} vs {mid}");
                assert!(g.iter().any(|x| x.0 == DOT));
            } else {
                assert!(rests.is_empty());
            }
        }
        // Four-four as well.
        let g = glyphs(&make(Staff::TabRhythm, (4, 4)));
        assert_eq!(g.iter().filter(|x| x.0 == time_digit(4)).count(), 2);
    }

    #[test]
    fn tab_rhythm_stems_and_beams() {
        let beat = |v: DurationValue| Beat {
            duration: Duration::new(v),
            notes: vec![Note::new(0, 0)],
            ..Beat::default()
        };
        let e = DurationValue::Eighth;
        let score = Score {
            tempo: 100,
            master_bars: vec![MasterBar::default()],
            tracks: vec![Track {
                kind: TrackKind::Fretted,
                strings: vec![64, 59, 55, 50, 45, 40],
                staves: vec![Staff::TabRhythm],
                bars: vec![Bar {
                    voices: vec![Voice {
                        beats: vec![
                            beat(e),
                            beat(e),
                            beat(DurationValue::Quarter),
                            beat(DurationValue::Half),
                        ],
                    }],
                }],
                ..Track::default()
            }],
            ..Score::default()
        };
        let page = &engrave(&score, 0, &Options::default()).pages[0];
        // Stems: vertical lines, all ending on one line; the half note's is half as long.
        let stems: Vec<(f64, f64)> = page
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Line { from, to, .. } if from.x == to.x && to.y > from.y => {
                    Some((from.y, to.y))
                }
                _ => None,
            })
            .collect();
        let stems = &stems[stems.len() - 4..];
        assert!(stems.iter().all(|s| s.1 == stems[0].1));
        let len = |s: (f64, f64)| s.1 - s.0;
        assert!((len(stems[3]) * 2.0 - len(stems[0])).abs() < 1.0);
        // One beam joins the eighths, at the stems' ends.
        let beams = page
            .items
            .iter()
            .filter(|i| matches!(i, Item::Rect { rect, .. } if rect.y1 == stems[0].1))
            .count();
        assert_eq!(beams, 1);

        // A whole note: no stem, an open circle.
        let mut whole = score.clone();
        whole.tracks[0].bars[0].voices[0].beats = vec![beat(DurationValue::Whole)];
        let page = &engrave(&whole, 0, &Options::default()).pages[0];
        let circles = page
            .items
            .iter()
            .filter(|i| {
                matches!(
                    i,
                    Item::Path {
                        stroke: Some(_),
                        ..
                    }
                )
            })
            .count();
        assert_eq!(circles, 1);
    }

    #[test]
    fn grand_staff_splits_notes() {
        let beat = |keys: &[u8]| Beat {
            duration: Duration::new(DurationValue::Quarter),
            notes: keys.iter().map(|&k| Note::new(0, k)).collect(),
            ..Beat::default()
        };
        let score = Score {
            tempo: 100,
            master_bars: vec![MasterBar::default()],
            tracks: vec![Track {
                kind: TrackKind::Pitched,
                staves: vec![Staff::Standard(muse_core::Clef::Grand { split: 60 })],
                bars: vec![Bar {
                    voices: vec![Voice {
                        beats: vec![
                            beat(&[72, 48]),
                            beat(&[64]),
                            beat(&[40]),
                            Beat::rest(Duration::new(DurationValue::Quarter)),
                        ],
                    }],
                }],
                ..Track::default()
            }],
            ..Score::default()
        };
        let layout = engrave(&score, 0, &Options::default());
        let heads = |layout: &Layout| -> Vec<f64> {
            let mut ys: Vec<f64> = layout.pages[0]
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Glyph { ch, at, .. } if *ch == HEAD_BLACK => Some(at.y),
                    _ => None,
                })
                .collect();
            ys.sort_by(f64::total_cmp);
            ys
        };
        // Four noteheads, one rest: C5 and E4 on the treble half, C3 and E2 on the bass half.
        let ys = heads(&layout);
        assert_eq!(ys.len(), 4);
        let glyphs: Vec<char> = layout.pages[0]
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Glyph { ch, .. } => Some(*ch),
                _ => None,
            })
            .collect();
        assert!(glyphs.contains(&BRACE) && glyphs.contains(&F_CLEF) && glyphs.contains(&G_CLEF));
        assert_eq!(
            glyphs
                .iter()
                .filter(|&&c| c == rest_glyph(DurationValue::Quarter))
                .count(),
            1
        );
        let pos = layout.beats.iter().find(|(b, _)| b.beat == 0).unwrap().1;
        // The middle-C split: B3 (diatonic 27) on the bass half, C4 (28) on the treble.
        assert!(pos.line_y(0, 27).unwrap().0 > pos.line_y(0, 28).unwrap().0 + 20.0);
    }

    #[test]
    fn grand_staff_and_drum_voices() {
        let beat = |keys: &[u8], v: DurationValue| Beat {
            duration: Duration::new(v),
            notes: keys.iter().map(|&k| Note::new(0, k)).collect(),
            ..Beat::default()
        };
        let bar = |beats: Vec<Beat>| Bar {
            voices: vec![Voice { beats }],
        };
        let q = DurationValue::Quarter;
        let right = Track {
            name: "Piano".into(),
            kind: TrackKind::Pitched,
            bars: vec![bar(vec![
                beat(&[72], q),
                beat(&[74], q),
                beat(&[76], q),
                beat(&[77], q),
            ])],
            ..Track::default()
        };
        let left = Track {
            name: "Piano (2)".into(),
            kind: TrackKind::Pitched,
            extra_staff: true,
            bars: vec![bar(vec![beat(&[47], DurationValue::Whole)])],
            ..Track::default()
        };
        let mut score = Score {
            tempo: 100,
            master_bars: vec![MasterBar::default()],
            tracks: vec![right, left],
            ..Score::default()
        };
        // A file's two staves become one track on a grand staff, the left hand a second voice.
        score.merge_staves();
        assert_eq!(score.tracks.len(), 1);
        let layout = engrave(&score, 0, &Options::default());
        assert_eq!(layout.beats.keys().filter(|b| b.voice == 0).count(), 4);
        assert_eq!(layout.beats.keys().filter(|b| b.voice == 1).count(), 1);
        let top = layout.beats.iter().find(|(b, _)| b.voice == 0).unwrap().1;
        let low = layout.beats.iter().find(|(b, _)| b.voice == 1).unwrap().1;
        assert!(low.line_y(0, 20).unwrap().0 > top.line_y(0, 35).unwrap().0);
        // Written out again, the hands are two staves of one part.
        let split = score.split_grand();
        assert_eq!(split.parts(), vec![0..2]);
        assert_eq!(split.tracks[1].bars[0].voices[0].beats[0].notes[0].fret, 47);
        assert_eq!(split.tracks[0].bars[0].voices.len(), 1);

        // Drums: hands and feet in one voice are shown as two.
        score.tracks = vec![Track {
            kind: TrackKind::Drums,
            bars: vec![bar(vec![
                beat(&[36, 42], DurationValue::Eighth),
                beat(&[42], DurationValue::Eighth),
                beat(&[38, 42], DurationValue::Eighth),
                beat(&[42], DurationValue::Eighth),
                beat(&[36, 42], DurationValue::Half),
            ])],
            ..Track::default()
        }];
        let shown = display_track(&score.tracks[0]);
        let voices = &shown.bars[0].voices;
        assert_eq!(voices.len(), 2);
        assert!(
            voices[0]
                .beats
                .iter()
                .all(|b| b.notes.iter().all(|n| !is_foot(n.fret)))
        );
        // Feet: kick, eighth rest, quarter rest, kick (two eighth rests merged on the beat).
        let feet: Vec<(bool, DurationValue)> = voices[1]
            .beats
            .iter()
            .map(|b| (b.is_rest(), b.duration.value))
            .collect();
        assert_eq!(
            feet,
            [
                (false, DurationValue::Eighth),
                (true, DurationValue::Eighth),
                (true, DurationValue::Quarter),
                (false, DurationValue::Half)
            ]
        );
    }

    #[test]
    fn lays_out_a_small_score() {
        let beat = |fret| Beat {
            duration: Duration::new(DurationValue::Eighth),
            notes: vec![Note::new(1, fret)],
            ..Beat::default()
        };
        let bar = Bar {
            voices: vec![Voice {
                beats: (0..8).map(|i| beat(i % 5)).collect(),
            }],
        };
        let track = Track {
            name: "Gtr".into(),
            strings: vec![64, 59, 55, 50, 45, 40],
            bars: vec![bar; 12],
            ..Track::default()
        };
        let score = Score {
            tempo: 100,
            master_bars: vec![MasterBar::default(); 12],
            tracks: vec![track],
            ..Score::default()
        };
        let layout = engrave(&score, 0, &Options::default());
        assert!(!layout.pages.is_empty());
        assert!(layout.pages.iter().map(|p| p.items.len()).sum::<usize>() > 500);
        assert_eq!(layout.beats.len(), 96);
        // Systems wrap within the page width.
        assert!(
            layout
                .bars
                .iter()
                .all(|&(_, a, b)| a >= 40.0 && b <= 595.0 - 39.0)
        );
    }

    #[test]
    fn chant_staff_of_an_ordinary_track() {
        use muse_core::melody::ChordPolicy;
        let beat = |frets: &[u8]| Beat {
            duration: Duration::new(DurationValue::Quarter),
            notes: frets.iter().map(|&f| Note::new(f % 3, f)).collect(),
            ..Beat::default()
        };
        let track = Track {
            strings: vec![64, 59, 55, 50, 45, 40],
            staves: vec![Staff::Standard(muse_core::Clef::Treble), Staff::Chant],
            bars: vec![Bar {
                voices: vec![Voice {
                    beats: vec![beat(&[2]), beat(&[5, 7]), beat(&[3]), beat(&[])],
                }],
            }],
            ..Track::default()
        };
        let score = Score {
            tempo: 100,
            master_bars: vec![MasterBar::default()],
            tracks: vec![track],
            ..Score::default()
        };
        let at = |beat| BeatRef {
            track: 0,
            bar: 0,
            voice: 0,
            beat,
        };
        // The skyline policy engraves a chant staff, whose positions know where the head is.
        let layout = engrave(&score, 0, &Options::default());
        let pos = &layout.beats[&at(1)];
        assert!(pos.line_y(1, 6).is_some());
        assert!(pos.head > pos.x && pos.head < pos.x + pos.width);
        // The staff line positions run only over the chant staff's own height.
        let (y3, _) = pos.line_y(1, 3).unwrap();
        let (y9, _) = pos.line_y(1, 9).unwrap();
        assert!(y9 < y3 && y3 - y9 < pos.bottom - pos.top);
        // A chord under the error policy: no staff, the message says where.
        let options = Options {
            chords: ChordPolicy::Error,
            ..Options::default()
        };
        let layout = engrave(&score, 0, &options);
        assert!(layout.beats[&at(1)].line_y(1, 6).is_none());
        let message = layout.pages[0].items.iter().find_map(|i| match i {
            Item::Text { text, .. } if text.contains("chord at bar 1 beat 2") => Some(text),
            _ => None,
        });
        assert!(message.is_some_and(|m| m.contains("monophonic")));
    }

    #[test]
    fn lyrics_attach_to_a_staff_from_any_track() {
        let beat = |syl: &str| Beat {
            duration: Duration::new(DurationValue::Quarter),
            notes: vec![Note::new(0, 0)],
            lyrics: vec![syl.into()],
            ..Beat::default()
        };
        let track = |syl: &str, staves, staff_lyrics| Track {
            kind: TrackKind::Fretted,
            strings: vec![64, 59, 55, 50, 45, 40],
            staves,
            staff_lyrics,
            bars: vec![Bar {
                voices: vec![Voice {
                    beats: vec![beat(syl), beat(syl)],
                }],
            }],
            ..Track::default()
        };
        let mut score = Score {
            tempo: 100,
            master_bars: vec![MasterBar::default()],
            tracks: vec![
                track(
                    "own",
                    vec![Staff::Standard(muse_core::Clef::Treble)],
                    vec![],
                ),
                track("other", vec![Staff::Tab], vec![]),
            ],
            ..Score::default()
        };
        let texts = |score: &Score, text: &str| -> Vec<f64> {
            engrave(score, 0, &Options::default()).pages[0]
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Text { text: t, at, .. } if t == text => Some(at.y),
                    _ => None,
                })
                .collect()
        };
        // Without attachments a track's own lyrics go under its top staff.
        let below = texts(&score, "own");
        assert_eq!(below.len(), 2);
        // Attached above, from the other track: its syllables replace the own ones, over the staff.
        score.tracks[0].set_staff_lyrics(
            0,
            Some(muse_core::StaffLyrics {
                track: Some(1),
                above: true,
            }),
        );
        assert!(texts(&score, "own").is_empty());
        let above = texts(&score, "other");
        assert_eq!(above.len(), 2);
        assert!(above[0] < below[0]);
    }

    /// Lyrics above a staff sit over the highest thing drawn above it, and hug the staff when
    /// there is nothing.
    #[test]
    fn lyrics_above_clear_what_rises_over_the_staff() {
        let sp = Options::default().sp;
        let score = |staff: Staff, string: u8, fret: u8, lyrics: usize| {
            let beat = |n: u8| Beat {
                duration: Duration::new(DurationValue::Eighth),
                notes: vec![Note::new(string, fret + n)],
                lyrics: (0..lyrics).map(|v| format!("la{v}")).collect(),
                ..Beat::default()
            };
            let mut track = Track {
                kind: TrackKind::Fretted,
                strings: vec![64, 59, 55, 50, 45, 40],
                staves: vec![staff],
                bars: vec![Bar {
                    voices: vec![Voice {
                        beats: vec![beat(0), beat(2), beat(4), beat(5)],
                    }],
                }],
                ..Track::default()
            };
            track.set_staff_lyrics(
                0,
                Some(muse_core::StaffLyrics {
                    track: None,
                    above: true,
                }),
            );
            Score {
                tempo: 100,
                master_bars: vec![MasterBar::default()],
                tracks: vec![track],
                ..Score::default()
            }
        };
        // (top of the staff, baseline of the lyrics, top of everything over the lyrics)
        let measure = |score: &Score| -> (f64, f64, f64) {
            let options = Options {
                header: false,
                ..Options::default()
            };
            let page = &engrave(score, 0, &options).pages[0];
            let mut staff_top = f64::MAX;
            let mut lyric = f64::MAX;
            let mut spans = Vec::new();
            let mut rest = Vec::new();
            for item in &page.items {
                match item {
                    Item::Text { text, at, size, .. } if text.starts_with("la") => {
                        lyric = lyric.min(at.y);
                        let w = fonts().text_width(FontKind::Text, text, *size);
                        spans.push((at.x - w / 2.0, at.x + w / 2.0));
                        continue;
                    }
                    Item::Line { from, to, .. } if from.y == to.y && to.x - from.x > 20.0 * sp => {
                        staff_top = staff_top.min(from.y);
                    }
                    _ => {}
                }
                // The tempo mark has a zone of its own at the top of the system.
                if matches!(
                    item,
                    Item::Glyph {
                        ch: METRONOME_QUARTER,
                        ..
                    }
                ) || matches!(item, Item::Text { text, .. } if text.starts_with("= "))
                {
                    continue;
                }
                rest.push(item.clone());
            }
            (
                staff_top,
                lyric,
                highest_over(&rest, &spans, sp, f64::MIN).0,
            )
        };
        let treble = Staff::Standard(muse_core::Clef::Treble);
        // High notes (ledger lines, stems down from them on a staff above) push the lyrics up.
        for staff in [
            treble,
            Staff::Standard(muse_core::Clef::Grand { split: 60 }),
        ] {
            let (top, lyric, high) = measure(&score(staff, 0, 12, 1));
            assert!(lyric < top, "lyrics are over the staff");
            assert!(
                lyric + 0.2 * sp <= high,
                "{lyric} clears the notation at {high}"
            );
            assert!(lyric > high - 1.0 * sp, "and sit right over it");
        }
        // Two verses stack, the lower one clear of the notation.
        let (top, lyric, high) = measure(&score(treble, 0, 12, 2));
        assert!(lyric < top && lyric + 0.2 * sp <= high);
        // Nothing tall: the lyrics hug whatever is highest over them (the tempo mark over the
        // first syllable, else the staff).
        let (top, lyric, high) = measure(&score(treble, 5, 0, 1));
        assert!(lyric < top && lyric + 0.2 * sp <= high && lyric > high - 1.0 * sp);
        // Rhythm stems and slashes: the same.
        for staff in [Staff::TabRhythm, Staff::Slash] {
            let (top, lyric, high) = measure(&score(staff, 0, 3, 1));
            assert!(lyric < top && lyric + 0.2 * sp <= high);
        }
    }

    /// Syllables from another track's rhythm that land on the same column (or too near it) are
    /// laid out one after another, never over each other.
    #[test]
    fn attached_lyrics_never_overlap() {
        let sp = Options::default().sp;
        let note = |value, lyric: &str| Beat {
            duration: Duration::new(value),
            notes: vec![Note::new(0, 3)],
            lyrics: if lyric.is_empty() {
                vec![]
            } else {
                vec![lyric.to_string()]
            },
            ..Beat::default()
        };
        let strings = vec![64, 59, 55, 50, 45, 40];
        // The staff has a single whole note; the source sings eight words in the same bar.
        let words = ["to", "play", "to-", "gether", "and", "to", "sing", "along"];
        let mut score = Score {
            tempo: 100,
            master_bars: vec![MasterBar::default()],
            tracks: vec![
                Track {
                    kind: TrackKind::Fretted,
                    strings: strings.clone(),
                    bars: vec![Bar {
                        voices: vec![Voice {
                            beats: vec![note(DurationValue::Whole, "")],
                        }],
                    }],
                    ..Track::default()
                },
                Track {
                    kind: TrackKind::Fretted,
                    strings,
                    bars: vec![Bar {
                        voices: vec![Voice {
                            beats: words
                                .iter()
                                .map(|w| note(DurationValue::Eighth, w))
                                .collect(),
                        }],
                    }],
                    ..Track::default()
                },
            ],
            ..Score::default()
        };
        for above in [true, false] {
            score.tracks[0].set_staff_lyrics(
                0,
                Some(muse_core::StaffLyrics {
                    track: Some(1),
                    above,
                }),
            );
            let page = &engrave(&score, 0, &Options::default()).pages[0];
            let mut boxes: Vec<(f64, f64)> = page
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Text {
                        font: FontKind::Text,
                        text,
                        at,
                        size,
                        ..
                    } if words.iter().any(|w| w.trim_end_matches('-') == text) => {
                        let w = fonts().text_width(FontKind::Text, text, *size);
                        Some((at.x - w / 2.0, at.x + w / 2.0))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(boxes.len(), words.len());
            boxes.sort_by(|a, b| a.0.total_cmp(&b.0));
            for pair in boxes.windows(2) {
                assert!(pair[0].1 + 0.3 * sp <= pair[1].0, "overlap {pair:?}");
            }
        }
    }

    /// Above a staff the order is: section marker, lyrics, then chord names and notation.
    #[test]
    fn lyrics_above_sit_under_the_section_marker() {
        let sp = Options::default().sp;
        let beat = |name: &str| Beat {
            duration: Duration::new(DurationValue::Quarter),
            notes: vec![Note::new(0, 3)],
            lyrics: vec!["la".into()],
            chord: Some(muse_core::Chord {
                name: name.into(),
                ..Default::default()
            }),
            ..Beat::default()
        };
        let mb = MasterBar {
            marker: Some(muse_core::Marker {
                title: "Pre-Chorus".into(),
                color: Default::default(),
                letter: "B".into(),
            }),
            ..MasterBar::default()
        };
        let mut track = Track {
            kind: TrackKind::Fretted,
            strings: vec![64, 59, 55, 50, 45, 40],
            staves: vec![Staff::Standard(muse_core::Clef::Treble)],
            bars: vec![Bar {
                voices: vec![Voice {
                    beats: vec![beat("Bm7"), beat("D5")],
                }],
            }],
            ..Track::default()
        };
        track.set_staff_lyrics(
            0,
            Some(muse_core::StaffLyrics {
                track: None,
                above: true,
            }),
        );
        let score = Score {
            tempo: 100,
            master_bars: vec![mb],
            tracks: vec![track],
            ..Score::default()
        };
        let options = Options {
            header: false,
            ..Options::default()
        };
        let page = &engrave(&score, 0, &options).pages[0];
        let (mut marker_bottom, mut lyric, mut chord_top) = (f64::MIN, f64::MAX, f64::MAX);
        for item in &page.items {
            match item {
                Item::Rect { rect, color } if *color == Color::rgb(235, 235, 235) => {
                    marker_bottom = marker_bottom.max(rect.y1)
                }
                Item::Text { text, at, size, .. } if text == "la" => lyric = lyric.min(at.y - size),
                Item::Text { text, at, size, .. } if text == "Bm7" => {
                    chord_top = chord_top.min(at.y - 0.75 * size)
                }
                _ => {}
            }
        }
        assert!(marker_bottom > f64::MIN && lyric < f64::MAX && chord_top < f64::MAX);
        assert!(
            marker_bottom + 0.3 * sp <= lyric,
            "marker {marker_bottom}, lyrics {lyric}"
        );
        assert!(lyric < chord_top, "lyrics over the chord names");
    }
}
