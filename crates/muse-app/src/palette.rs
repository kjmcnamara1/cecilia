//! The editing palette: a vertical list of note durations, rests, accidentals, dynamics,
//! articulations and effects along the left of the window, in collapsible sections. `<leader>e`
//! shows it, `<leader>E` focuses it (Palette mode: `j`/`k` choose an item, `Enter` applies it to
//! the score pane worked on, `za` folds a section). Every item runs an ordinary command; the
//! rows are built here and the frontends only draw them.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use crate::app::{App, Cursor, PaneKind};
use crate::command::Command;
use crate::config::InsertKind;
use crate::edit::{self, EditCx};
use muse_core::TrackKind;

/// Width of the pane, in columns.
pub const WIDTH: u16 = 26;

/// A section: its title and its items (full name for the tooltip, command line, short label for
/// the terminal). The GUI draws an icon per command.
/// An item: its name, command line and short label.
type Item = (&'static str, &'static str, &'static str);

/// An item whose command ends in a space takes an argument: choosing it opens the command line
/// with that text typed (and completion for the argument).
pub const SECTIONS: &[(&str, &[Item])] = &[
    (
        "Duration",
        &[
            ("Whole note", "duration 1", "1"),
            ("Half note", "duration 2", "1/2"),
            ("Quarter note", "duration 4", "1/4"),
            ("Eighth note", "duration 8", "1/8"),
            ("16th note", "duration 16", "16"),
            ("32nd note", "duration 32", "32"),
            ("64th note", "duration 64", "64"),
            ("Dotted", "dot", "."),
            ("Double dotted", "double-dot", ".."),
            ("Longer", "duration-longer", "+"),
            ("Shorter", "duration-shorter", "-"),
            ("Triplet", "tuplet 3", "3"),
            ("Rest", "rest", "rst"),
        ],
    ),
    (
        "Pitch",
        &[
            ("Sharp", "accidental sharp", "♯"),
            ("Flat", "accidental flat", "♭"),
            ("Natural", "accidental natural", "♮"),
            ("Double sharp", "accidental double-sharp", "♯♯"),
            ("Double flat", "accidental double-flat", "♭♭"),
            ("Semitone up", "pitch-up", "+1"),
            ("Semitone down", "pitch-down", "-1"),
            ("Octave up", "octave-up", "8↑"),
            ("Octave down", "octave-down", "8↓"),
            ("Tie", "toggle tie", "⌒"),
            ("Dead note", "toggle dead", "x"),
            ("Ghost note", "toggle ghost", "()"),
        ],
    ),
    (
        "Dynamics",
        &[
            ("Pianississimo (ppp)", "dynamic ppp", "ppp"),
            ("Pianissimo (pp)", "dynamic pp", "pp"),
            ("Piano (p)", "dynamic p", "p"),
            ("Mezzo piano (mp)", "dynamic mp", "mp"),
            ("Mezzo forte (mf)", "dynamic mf", "mf"),
            ("Forte (f)", "dynamic f", "f"),
            ("Fortissimo (ff)", "dynamic ff", "ff"),
            ("Fortississimo (fff)", "dynamic fff", "fff"),
            ("Louder", "dynamic-step +1", "d+"),
            ("Softer", "dynamic-step -1", "d-"),
            ("Crescendo", "toggle crescendo", "<"),
            ("Decrescendo", "toggle decrescendo", ">"),
            ("Fade in", "toggle fade-in", "fi"),
            ("Fade out", "toggle fade-out", "fo"),
            ("Volume swell", "toggle volume-swell", "<>"),
        ],
    ),
    (
        "Articulation",
        &[
            ("Accent", "toggle accent", ">"),
            ("Heavy accent", "toggle heavy-accent", "^"),
            ("Staccato", "toggle staccato", "·"),
            ("Fermata", "toggle fermata", "frm"),
            ("Let ring", "toggle let-ring", "LR"),
            ("Palm mute", "toggle palm-mute", "PM"),
        ],
    ),
    (
        "Effects",
        &[
            ("Legato (hammer-on / pull-off)", "toggle hammer", "leg"),
            ("Bend", "toggle bend", "↗"),
            ("Half bend", "toggle half-bend", "½"),
            ("Vibrato", "toggle vibrato", "~"),
            ("Wide vibrato", "toggle vibrato-wide", "≈"),
            ("Harmonic", "toggle harmonic", "◇"),
            ("Artificial harmonic", "toggle artificial-harmonic", "AH"),
            ("Tremolo picking", "toggle tremolo", "≡"),
            ("Tap", "toggle tap", "tap"),
            ("Left hand tapping", "toggle left-hand-tap", "LHT"),
            ("Slap", "toggle slap", "slp"),
            ("Pop", "toggle pop", "pop"),
            ("Grace note before beat", "toggle grace-before", "grb"),
            ("Grace note on beat", "toggle grace-on", "gro"),
        ],
    ),
    (
        "Slides",
        &[
            ("Shift slide", "toggle slide", "/"),
            ("Legato slide", "toggle slide-legato", "~/"),
            ("Slide in from above", "toggle slide-in-above", "in↘"),
            ("Slide in from below", "toggle slide-in-below", "in↗"),
            ("Slide out downwards", "toggle slide-out-down", "ou↘"),
            ("Slide out upwards", "toggle slide-out-up", "ou↗"),
        ],
    ),
    (
        "Strokes",
        &[
            ("Down stroke", "toggle stroke-down", "↓"),
            ("Up stroke", "toggle stroke-up", "↑"),
            ("Arpeggio down", "toggle arpeggio-down", "a↓"),
            ("Arpeggio up", "toggle arpeggio-up", "a↑"),
            ("Rasgueado", "toggle rasgueado", "ras"),
            ("Pickstroke down", "toggle pick-down", "pk↓"),
            ("Pickstroke up", "toggle pick-up", "pk↑"),
        ],
    ),
    (
        "Beams & stems",
        &[
            ("Automatic beams", "beams auto", "bA"),
            ("Force beams", "beams force", "bF"),
            ("Break beams", "beams break", "bB"),
            ("Break secondary beams", "beams break-secondary", "b2"),
            ("Automatic stems", "stems auto", "sA"),
            ("Stems up", "stems up", "s↑"),
            ("Stems down", "stems down", "s↓"),
        ],
    ),
    (
        "Bar",
        &[
            ("Clef", "clef ", "clf"),
            ("Key signature", "key ", "key"),
            ("Time signature", "time ", "4/4"),
            ("Free time", "free-time", "ft"),
            ("Double bar line", "double-bar", "||"),
            ("Section", "section ", "sec"),
        ],
    ),
    (
        "Repeats",
        &[
            ("Repeat one bar", "bar-repeat 1", "%"),
            ("Repeat two bars", "bar-repeat 2", "%%"),
            ("Repeat open", "repeat-start", "|:"),
            ("Repeat close", "repeat-end", ":|"),
            ("Alternate endings", "ending ", "1."),
            ("Directions", "direction ", "D.C"),
        ],
    ),
];

/// The terminal's icon of a palette command: the closest Unicode / Nerd Font glyph to the
/// symbol the engraver draws for it, or `None` where there is none and the item's short label
/// stands in. `nerd` picks the Nerd Font glyphs (Material Design music notes) over the plain
/// Unicode ones. This one table is the place to adjust the terminal's icons.
pub fn palette_glyph(command: &str, nerd: bool) -> Option<&'static str> {
    let (name, arg) = command
        .trim_end()
        .split_once(' ')
        .unwrap_or((command.trim_end(), ""));
    // (Nerd Font, Unicode); the Nerd Font column is empty where Unicode does as well.
    let (n, u): (&str, &str) = match (name, arg) {
        ("duration", "1") => ("\u{f038d}", "\u{1D15D}"),
        ("duration", "2") => ("\u{f0389}", "\u{1D15E}"),
        ("duration", "4") => ("\u{f038b}", "\u{2669}"),
        ("duration", "8") => ("\u{f0388}", "\u{266A}"),
        ("duration", "16") => ("\u{f038c}", "\u{266C}"),
        ("duration", "32") => ("", "\u{1D162}"),
        ("duration", "64") => ("", "\u{1D163}"),
        ("dot", _) => ("", "\u{2669}."),
        ("double-dot", _) => ("", "\u{2669}.."),
        ("tuplet", _) => ("", "\u{00B3}"),
        ("rest", _) => ("", "\u{1D13D}"),
        ("accidental", "sharp") => ("", "\u{266F}"),
        ("accidental", "flat") => ("", "\u{266D}"),
        ("accidental", "natural") => ("", "\u{266E}"),
        ("accidental", "double-sharp") => ("", "\u{1D12A}"),
        ("accidental", "double-flat") => ("", "\u{1D12B}"),
        ("toggle", "tie") => ("", "\u{2040}"),
        ("toggle", "fermata") => ("", "\u{1D110}"),
        ("toggle", "pick-down") => ("", "\u{2293}"),
        ("toggle", "pick-up") => ("", "\u{2228}"),
        ("clef", _) => ("", "\u{1D11E}"),
        ("key", _) => ("", "\u{266D}\u{266F}"),
        ("time", _) => ("", "\u{1D134}"),
        ("double-bar", _) => ("", "\u{1D101}"),
        ("repeat-start", _) => ("", "\u{1D106}"),
        ("repeat-end", _) => ("", "\u{1D107}"),
        ("direction", _) => ("", "\u{1D10B}"),
        _ => return None,
    };
    if nerd && !n.is_empty() {
        Some(n)
    } else {
        Some(u)
    }
}

/// The palette's selection and fold state.
#[derive(Debug, Default)]
pub struct PaletteState {
    /// Selected row, an index into [`App::palette_rows`].
    pub selected: usize,
    /// Sections folded shut (indices into [`SECTIONS`]).
    pub folded: HashSet<usize>,
    /// First row drawn, and the selection the window last followed.
    pub scroll: Cell<usize>,
    pub seen: Cell<usize>,
    /// Buttons per row of the grid, as the frontend last laid it out.
    pub cols: Cell<usize>,
    /// The column up/down moves keep to (the one last chosen with left/right or a click).
    pub want_col: Cell<usize>,
    /// A search being typed (`/`): the field at the bottom of the pane.
    pub search: Option<PalSearch>,
    /// The `/` register: what `n` and `N` look for.
    pub last_search: String,
    /// Which commands can't take effect at the cursor, and why; kept for the cursor and score
    /// version it was worked out for.
    avail: RefCell<Option<Availability>>,
}

/// A search being typed in the palette, as in the properties pane.
#[derive(Debug, Clone, PartialEq)]
pub struct PalSearch {
    pub query: String,
    /// The selected row when it began: Escape goes back there.
    origin: usize,
}

/// The commands that can't take effect at a cursor (with the reason), for a buffer and version.
#[derive(Debug)]
struct Availability {
    key: (usize, u64, Cursor, muse_core::Duration),
    reasons: HashMap<&'static str, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteKind {
    Header { section: usize, folded: bool },
    Item,
}

/// A line of the grid.
#[derive(Debug, Clone, PartialEq)]
pub enum PaletteLine {
    /// A section title (the row's index).
    Header(usize),
    /// A line of buttons (rows' indices).
    Cells(Vec<usize>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaletteRow {
    pub kind: PaletteKind,
    /// The full name (the title of a header; an item's tooltip).
    pub label: &'static str,
    /// A short label for frontends without icons.
    pub short: &'static str,
    /// The command line an item runs (empty for a header).
    pub command: &'static str,
    /// The item's setting is in force at the cursor (the duration, a technique …).
    pub active: bool,
    /// Why the item can't take effect at the cursor (it is dimmed, applying it does nothing).
    pub unavailable: Option<String>,
    /// The insert-mode keys bound to the item's command (at most three).
    pub keys: Vec<String>,
}

impl PaletteRow {
    /// The name as the tooltip and the label show it: with its insert-mode keys and, if it
    /// can't be applied now, "(unavailable)", e.g. "Palm mute (m) (unavailable)".
    pub fn name(&self) -> String {
        let mut name = self.label.to_string();
        if self.command.ends_with(' ') {
            name.push('…');
        }
        if !self.keys.is_empty() {
            name = format!("{name} ({})", self.keys.join(", "));
        }
        if self.unavailable.is_some() {
            name.push_str(" (unavailable)");
        }
        name
    }
}

/// Dynamic markings as MIDI velocities (Guitar Pro's).
pub fn dynamic_velocity(name: &str) -> Option<u8> {
    Some(match name {
        "ppp" => 15,
        "pp" => 31,
        "p" => 47,
        "mp" => 63,
        "mf" => 79,
        "f" => 95,
        "ff" => 111,
        "fff" => 127,
        _ => return None,
    })
}

/// Tries the edit a palette command makes on a copy of the score (the command's own checks
/// decide: no note under the cursor, not on a staff …).
fn probe_edit(
    mut score: muse_core::Score,
    cursor: Cursor,
    cmd: &Command,
    duration: muse_core::Duration,
) -> Result<(), String> {
    use Command::*;
    let mut cx = EditCx::new(&mut score, cursor);
    match cmd {
        Toggle(t) => edit::toggle(&mut cx, t, duration),
        Accidental(a) => edit::set_accidental(&mut cx, *a),
        PitchUp => edit::shift_pitch(&mut cx, 1),
        PitchDown => edit::shift_pitch(&mut cx, -1),
        OctaveUp => edit::shift_pitch(&mut cx, 12),
        OctaveDown => edit::shift_pitch(&mut cx, -12),
        Dynamic(d) => edit::set_dynamic(&mut cx, d),
        _ => Ok(()),
    }
}

/// Whether a palette item's command is for a notation (the staff under the cursor) in a mode.
///
/// Bar tools (key and time signatures, repeats, endings, directions, sections) apply on every
/// staff but a chant's; the clef only on standard and chant staves. Tab: everything but
/// accidentals, clefs and stems. Standard staff: accidentals and the marks it can show (no
/// bends, palm mutes, let ring, tap / slap / pop, fades or left-hand taps). Slash: note
/// values, rests, ties, accents, hairpins, fermatas and beams. Chant: note values and the clef.
/// Visual mode (a selection of beats, applying to every beat of it) offers the tools of the
/// staff's notation that make sense on a range; with a selection of bars it also offers the
/// bar tools, for the first, the last or all of the selected bars (not the clef).
fn applies(command: &str, kind: InsertKind, visual: Option<bool>) -> bool {
    let (name, arg) = command
        .trim_end()
        .split_once(' ')
        .unwrap_or((command.trim_end(), ""));
    let value = matches!(
        name,
        "duration" | "dot" | "double-dot" | "tuplet" | "duration-longer" | "duration-shorter"
    );
    if kind == InsertKind::Chant {
        return value || (name == "clef" && visual.is_none());
    }
    let bar_tool = matches!(
        name,
        "key"
            | "time"
            | "free-time"
            | "double-bar"
            | "section"
            | "bar-repeat"
            | "repeat-start"
            | "repeat-end"
            | "ending"
            | "direction"
    );
    match visual {
        // A beat selection: no bar tools, nor the clef.
        Some(false) if bar_tool || name == "clef" => return false,
        // A bar selection: the bar tools that mean something for several bars.
        Some(true) if name == "clef" || name == "bar-repeat" => return false,
        _ => {}
    }
    if bar_tool {
        return true;
    }
    match kind {
        InsertKind::Tab => !matches!(name, "accidental" | "clef" | "stems"),
        InsertKind::Standard => match name {
            "toggle" => !matches!(
                arg,
                "bend"
                    | "half-bend"
                    | "let-ring"
                    | "palm-mute"
                    | "tap"
                    | "slap"
                    | "pop"
                    | "fade-in"
                    | "fade-out"
                    | "volume-swell"
                    | "left-hand-tap"
            ),
            _ => true,
        },
        _ => {
            value
                || name == "rest"
                || name == "beams"
                || matches!(
                    arg,
                    "tie"
                        | "accent"
                        | "heavy-accent"
                        | "staccato"
                        | "crescendo"
                        | "decrescendo"
                        | "fermata"
                )
                || matches!(name, "dynamic" | "dynamic-step")
        }
    }
}

impl App {
    /// The rows: each section's header, then its items unless folded. Only the tools that
    /// apply to the mode and the staff under the cursor are listed; a section with none is
    /// left out.
    pub fn palette_rows(&self) -> Vec<PaletteRow> {
        let kind = self.insert_kind();
        // In Visual mode: whether the selection is of bars.
        let visual = self
            .selection()
            .map(|(_, _, bars)| bars)
            .or((self.mode == crate::config::Mode::Visual).then_some(false));
        // The selected beats, from and to.
        let range = self.selection().map(|(a, b, bars)| {
            let (lo, hi) = (
                (a.bar, a.beat).min((b.bar, b.beat)),
                (a.bar, a.beat).max((b.bar, b.beat)),
            );
            if bars {
                ((lo.0, 0), (hi.0, usize::MAX))
            } else {
                (lo, hi)
            }
        });
        let note = self.palette_note();
        let beat = self.palette_beat();
        // The beat under the cursor; the duration new notes get where there is none.
        let duration = beat
            .as_ref()
            .filter(|b| !b.empty)
            .map_or_else(|| self.insert_duration(), |b| b.duration);
        let rest = beat.as_ref().is_some_and(|b| !b.empty && b.is_rest());
        let fx = beat.as_ref().map(|b| b.effects.clone());
        let cursor = self.pane().cursor;
        let master = self.buffer().score.master_bars.get(cursor.bar).cloned();
        let simile = self
            .buffer()
            .score
            .tracks
            .get(cursor.track)
            .and_then(|t| t.bars.get(cursor.bar))
            .and_then(|b| b.simile);
        let mut rows = Vec::new();
        for (s, (title, items)) in SECTIONS.iter().enumerate() {
            if !items.iter().any(|(_, c, _)| applies(c, kind, visual)) {
                continue;
            }
            let folded = self.palette.folded.contains(&s);
            rows.push(PaletteRow {
                kind: PaletteKind::Header { section: s, folded },
                label: title,
                short: "",
                command: "",
                active: false,
                unavailable: None,
                keys: Vec::new(),
            });
            if folded {
                continue;
            }
            for (label, command, short) in items.iter() {
                if !applies(command, kind, visual) {
                    continue;
                }
                let (name, arg) = command
                    .trim_end()
                    .split_once(' ')
                    .unwrap_or((command.trim_end(), ""));
                let alter = |a: i8| note.as_ref().is_some_and(|n| n.alter == Some(a));
                let slide = |k: muse_core::SlideKind| {
                    note.as_ref().is_some_and(|n| n.effects.slides.contains(&k))
                };
                let stroke = |d: muse_core::StrokeDirection, arpeggio: bool| {
                    fx.as_ref().is_some_and(|e| {
                        e.stroke
                            .is_some_and(|s| s.direction == d && s.arpeggio == arpeggio)
                    })
                };
                // A technique is in force over a selection when every beat (or note) has it.
                let over_range = range.filter(|_| name == "toggle").map(|(from, to)| {
                    edit::range_has(
                        &self.buffer().score,
                        cursor.track,
                        cursor.voice,
                        from,
                        to,
                        arg,
                    )
                });
                let active = match (name, arg) {
                    _ if over_range.is_some() => over_range == Some(true),
                    ("dynamic", d) if range.is_some() => {
                        let (from, to) = range.unwrap();
                        edit::range_has_dynamic(
                            &self.buffer().score,
                            cursor.track,
                            cursor.voice,
                            from,
                            to,
                            d,
                        )
                    }
                    ("duration", n) => n.parse() == Ok(duration.value.denominator()),
                    ("rest", _) => rest,
                    ("accidental", "double-sharp") => alter(2),
                    ("accidental", "double-flat") => alter(-2),
                    ("double-dot", _) => duration.dots == 2,
                    ("free-time", _) => master.as_ref().is_some_and(|m| m.free_time),
                    ("double-bar", _) => master.as_ref().is_some_and(|m| m.double_bar),
                    ("repeat-start", _) => master.as_ref().is_some_and(|m| m.repeat_start),
                    ("repeat-end", _) => master.as_ref().is_some_and(|m| m.repeat_end.is_some()),
                    ("ending", _) => master.as_ref().is_some_and(|m| m.alternate_endings != 0),
                    ("direction", _) => master.as_ref().is_some_and(|m| !m.directions.is_empty()),
                    ("section", _) => master.as_ref().is_some_and(|m| m.marker.is_some()),
                    ("bar-repeat", "1") => simile == Some(muse_core::Simile::OneBar),
                    ("bar-repeat", _) => matches!(
                        simile,
                        Some(muse_core::Simile::TwoBarsFirst | muse_core::Simile::TwoBarsSecond)
                    ),
                    ("beams", m) => beat.as_ref().is_some_and(|b| {
                        b.beam
                            == match m {
                                "force" => muse_core::BeamMode::Force,
                                "break" => muse_core::BeamMode::Break,
                                "break-secondary" => muse_core::BeamMode::BreakSecondary,
                                _ => muse_core::BeamMode::Auto,
                            }
                    }),
                    ("stems", d) => beat.as_ref().is_some_and(|b| {
                        b.stem
                            == match d {
                                "up" => muse_core::StemDir::Up,
                                "down" => muse_core::StemDir::Down,
                                _ => muse_core::StemDir::Auto,
                            }
                    }),
                    ("toggle", "fade-out") => fx.as_ref().is_some_and(|e| e.fade_out),
                    ("toggle", "volume-swell") => fx.as_ref().is_some_and(|e| e.volume_swell),
                    ("toggle", "fermata") => fx.as_ref().is_some_and(|e| e.fermata),
                    ("toggle", "rasgueado") => fx.as_ref().is_some_and(|e| e.rasgueado),
                    ("toggle", "crescendo") => fx
                        .as_ref()
                        .is_some_and(|e| e.hairpin == Some(muse_core::Hairpin::Crescendo)),
                    ("toggle", "decrescendo") => fx
                        .as_ref()
                        .is_some_and(|e| e.hairpin == Some(muse_core::Hairpin::Decrescendo)),
                    ("toggle", "pick-down") => fx
                        .as_ref()
                        .is_some_and(|e| e.pick_stroke == muse_core::StrokeDirection::Down),
                    ("toggle", "pick-up") => fx
                        .as_ref()
                        .is_some_and(|e| e.pick_stroke == muse_core::StrokeDirection::Up),
                    ("toggle", "arpeggio-down") => stroke(muse_core::StrokeDirection::Down, true),
                    ("toggle", "arpeggio-up") => stroke(muse_core::StrokeDirection::Up, true),
                    ("toggle", "slide") => slide(muse_core::SlideKind::Shift),
                    ("toggle", "slide-legato") => slide(muse_core::SlideKind::Legato),
                    ("toggle", "slide-in-above") => slide(muse_core::SlideKind::IntoFromAbove),
                    ("toggle", "slide-in-below") => slide(muse_core::SlideKind::IntoFromBelow),
                    ("toggle", "slide-out-down") => slide(muse_core::SlideKind::OutDown),
                    ("toggle", "slide-out-up") => slide(muse_core::SlideKind::OutUp),
                    ("toggle", "left-hand-tap") => {
                        note.as_ref().is_some_and(|n| n.effects.left_hand_tap)
                    }
                    ("toggle", "artificial-harmonic") => note.as_ref().is_some_and(|n| {
                        matches!(
                            n.effects.harmonic,
                            Some(muse_core::Harmonic::Artificial { .. })
                        )
                    }),
                    ("toggle", "grace-before") => note
                        .as_ref()
                        .is_some_and(|n| n.effects.grace.is_some_and(|g| !g.on_beat)),
                    ("toggle", "grace-on") => note
                        .as_ref()
                        .is_some_and(|n| n.effects.grace.is_some_and(|g| g.on_beat)),
                    ("toggle", "vibrato-wide") => fx.as_ref().is_some_and(|e| e.vibrato),
                    ("toggle", "fade-in") => fx.as_ref().is_some_and(|e| e.fade_in),
                    ("toggle", "tap") => {
                        fx.as_ref().is_some_and(|e| e.slap == muse_core::Slap::Tap)
                    }
                    ("toggle", "slap") => {
                        fx.as_ref().is_some_and(|e| e.slap == muse_core::Slap::Slap)
                    }
                    ("toggle", "pop") => {
                        fx.as_ref().is_some_and(|e| e.slap == muse_core::Slap::Pop)
                    }
                    ("toggle", "stroke-down" | "stroke-up") => fx.as_ref().is_some_and(|e| {
                        e.stroke.is_some_and(|s| {
                            (s.direction == muse_core::StrokeDirection::Down)
                                == (arg == "stroke-down")
                        })
                    }),
                    ("dot", _) => duration.dots == 1,
                    ("tuplet", _) => duration.tuplet.is_some(),
                    ("dynamic", d) => note.as_ref().is_some_and(|n| {
                        dynamic_velocity(d).is_some_and(|v| {
                            muse_core::dynamic_name(n.velocity) == muse_core::dynamic_name(v)
                        })
                    }),
                    ("toggle", t) => note.as_ref().is_some_and(|n| match t {
                        "tie" => n.tie,
                        "dead" => n.dead,
                        "ghost" => n.ghost,
                        "accent" => n.accent == muse_core::Accent::Normal,
                        "heavy-accent" => n.accent == muse_core::Accent::Heavy,
                        "staccato" => n.effects.staccato,
                        "let-ring" => n.effects.let_ring,
                        "palm-mute" => n.effects.palm_mute,
                        "hammer" => n.effects.hammer_pull,
                        "bend" | "half-bend" => n.effects.bend.is_some(),
                        "vibrato" => n.effects.vibrato,
                        "harmonic" | "pinch-harmonic" => matches!(
                            n.effects.harmonic,
                            Some(h) if !matches!(h, muse_core::Harmonic::Artificial { .. })
                        ),
                        "tremolo" => n.effects.tremolo_picking.is_some(),
                        _ => false,
                    }),
                    _ => false,
                };
                rows.push(PaletteRow {
                    kind: PaletteKind::Item,
                    label,
                    short,
                    command,
                    active,
                    // Over a selection tools skip the beats they can't act on.
                    unavailable: if range.is_some() {
                        None
                    } else {
                        self.palette_unavailable(command)
                    },
                    keys: self.palette_keys(command),
                });
            }
        }
        rows
    }

    /// Why a command can't take effect at the cursor, if it can't: its edit is tried on a copy
    /// of the cursor's bar, so the reason is the command's own. Worked out once per cursor
    /// position and score version.
    fn palette_unavailable(&self, command: &'static str) -> Option<String> {
        let c = self.pane().cursor;
        let buffer = self.buffer();
        let duration = self.insert_duration();
        let key = (self.pane().buffer, buffer.version, c, duration);
        let mut cache = self.palette.avail.borrow_mut();
        if cache.as_ref().is_none_or(|a| a.key != key) {
            let mut reasons = HashMap::new();
            // A score with only the cursor's track and master bar filled in.
            let probe = buffer.score.tracks.get(c.track).map(|track| {
                let mut t = track.clone();
                let bar = t.bars.get(c.bar).cloned().unwrap_or_default();
                t.bars = vec![muse_core::Bar::default(); c.bar];
                t.bars.push(bar);
                let mut probe = muse_core::Score {
                    tracks: vec![muse_core::Track::default(); c.track],
                    master_bars: vec![muse_core::MasterBar::default(); c.bar],
                    ..Default::default()
                };
                probe.tracks.push(t);
                probe.master_bars.push(
                    buffer
                        .score
                        .master_bars
                        .get(c.bar)
                        .cloned()
                        .unwrap_or_default(),
                );
                probe
            });
            for (_, items) in SECTIONS {
                for (_, command, _) in items.iter() {
                    // An item that asks for an argument is always there to choose.
                    if command.ends_with(' ') {
                        continue;
                    }
                    let result = match (&probe, Command::parse(command)) {
                        (None, _) => Err("no score".to_string()),
                        (Some(p), Ok(cmd)) => probe_edit(p.clone(), c, &cmd, duration),
                        (_, Err(e)) => Err(e),
                    };
                    if let Err(e) = result {
                        reasons.insert(*command, e);
                    }
                }
            }
            *cache = Some(Availability { key, reasons });
        }
        cache.as_ref().and_then(|a| a.reasons.get(command).cloned())
    }

    /// The insert-mode keys bound to a command (as the live keymaps have them, user overrides
    /// included): those of the notation under the cursor, then the shared ones.
    fn palette_keys(&self, command: &str) -> Vec<String> {
        let Ok(cmd) = Command::parse(command) else {
            return Vec::new();
        };
        let kind = self.config.insert_keymap(self.insert_kind());
        let shared = self.config.keymap(crate::config::Mode::Insert);
        let mut keys: Vec<String> = Vec::new();
        for map in [kind, shared] {
            for (k, cmds) in &map.bindings {
                // A key the notation's own map binds to something else shadows the shared one.
                let shadowed = std::ptr::eq(map, shared) && kind.bindings.contains_key(k);
                if !shadowed && cmds.as_slice() == [cmd.clone()] {
                    let k = crate::keys::format_keys(k);
                    if !keys.contains(&k) {
                        keys.push(k);
                    }
                }
            }
        }
        keys.sort_by_key(|k| (k.len(), k.clone()));
        keys.truncate(3);
        keys
    }

    /// The beat under the cursor.
    fn palette_beat(&self) -> Option<muse_core::Beat> {
        let c = self.pane().cursor;
        let track = self.buffer().score.tracks.get(c.track)?;
        let beats = &track.bars.get(c.bar)?.voices.get(c.voice)?.beats;
        beats
            .get(c.beat.min(beats.len().saturating_sub(1)))
            .cloned()
    }

    /// The note under the cursor (on a fretted track the one on its string).
    fn palette_note(&self) -> Option<muse_core::Note> {
        let c = self.pane().cursor;
        let track = self.buffer().score.tracks.get(c.track)?;
        let beats = &track.bars.get(c.bar)?.voices.get(c.voice)?.beats;
        let beat = beats.get(c.beat.min(beats.len().saturating_sub(1)))?;
        let on_string = (track.kind == TrackKind::Fretted)
            .then(|| beat.notes.iter().find(|n| n.string as usize == c.string))
            .flatten();
        on_string.or_else(|| beat.notes.first()).cloned()
    }

    /// The selected row, clamped to the rows there are.
    pub fn palette_selected(&self) -> usize {
        self.palette
            .selected
            .min(self.palette_rows().len().saturating_sub(1))
    }

    /// The grid: a line per section title, and the section's buttons wrapped to the pane's
    /// width (`cols` per line). Each entry is an index into [`App::palette_rows`].
    pub fn palette_lines(&self) -> Vec<PaletteLine> {
        let cols = self.palette.cols.get().max(1);
        let mut lines = Vec::new();
        let mut cells: Vec<usize> = Vec::new();
        for (i, row) in self.palette_rows().iter().enumerate() {
            match row.kind {
                PaletteKind::Header { .. } => {
                    if !cells.is_empty() {
                        lines.push(PaletteLine::Cells(std::mem::take(&mut cells)));
                    }
                    lines.push(PaletteLine::Header(i));
                }
                PaletteKind::Item => {
                    cells.push(i);
                    if cells.len() == cols {
                        lines.push(PaletteLine::Cells(std::mem::take(&mut cells)));
                    }
                }
            }
        }
        if !cells.is_empty() {
            lines.push(PaletteLine::Cells(cells));
        }
        lines
    }

    /// Tells the palette how many buttons fit in a line (the frontends, each frame).
    pub fn set_palette_cols(&self, cols: usize) {
        self.palette.cols.set(cols.max(1));
    }

    /// The line and column of the selected row in [`App::palette_lines`].
    fn palette_at(&self, lines: &[PaletteLine]) -> (usize, usize) {
        let sel = self.palette_selected();
        lines
            .iter()
            .enumerate()
            .find_map(|(l, line)| match line {
                PaletteLine::Header(i) if *i == sel => Some((l, 0)),
                PaletteLine::Cells(c) => c.iter().position(|&i| i == sel).map(|k| (l, k)),
                _ => None,
            })
            .unwrap_or((0, 0))
    }

    /// Moves the selection a line down (`dy` 1) or up, keeping to the column last chosen.
    fn palette_vertical(&mut self, dy: isize) {
        let lines = self.palette_lines();
        if lines.is_empty() {
            return;
        }
        let (l, _) = self.palette_at(&lines);
        let to = (l as isize + dy).clamp(0, lines.len() as isize - 1) as usize;
        self.palette.selected = match &lines[to] {
            PaletteLine::Header(i) => *i,
            PaletteLine::Cells(c) => c[self.palette.want_col.get().min(c.len() - 1)],
        };
    }

    /// Moves the selection a button left or right within its line.
    fn palette_horizontal(&mut self, dx: isize) {
        let lines = self.palette_lines();
        let (l, k) = self.palette_at(&lines);
        if let PaletteLine::Cells(c) = &lines[l] {
            let k = (k as isize + dx).clamp(0, c.len() as isize - 1) as usize;
            self.palette.selected = c[k];
            self.palette.want_col.set(k);
        }
    }

    /// The first line to draw, given `visible` lines of room: the selected line is kept in view
    /// when it has just changed; else the wheel's scrolling stands.
    pub fn palette_window(&self, visible: usize) -> usize {
        let lines = self.palette_lines();
        let n = lines.len();
        let (sel, _) = self.palette_at(&lines);
        let visible = visible.max(1);
        let mut first = self.palette.scroll.get().min(n.saturating_sub(1));
        if self.palette.selected != self.palette.seen.get() {
            self.palette.seen.set(self.palette.selected);
            if sel < first {
                first = sel;
            } else if sel >= first + visible {
                first = sel + 1 - visible;
            }
        }
        first = first.min(n.saturating_sub(visible));
        self.palette.scroll.set(first);
        first
    }

    /// The wheel: scrolls the lines by `by` (positive: down) without moving the selection.
    pub fn scroll_palette(&self, by: isize) {
        let n = self.palette_lines().len() as isize;
        let first = (self.palette.scroll.get() as isize + by).clamp(0, (n - 1).max(0));
        self.palette.scroll.set(first as usize);
        self.palette.seen.set(self.palette.selected);
    }

    /// Folds (`Some(true)`), unfolds (`Some(false)`) or toggles the selected row's section.
    fn fold_palette(&mut self, fold: Option<bool>) {
        let rows = self.palette_rows();
        let sel = self.palette_selected();
        let Some(section) = rows[..=sel.min(rows.len().saturating_sub(1))]
            .iter()
            .rev()
            .find_map(|r| match r.kind {
                PaletteKind::Header { section, .. } => Some(section),
                _ => None,
            })
        else {
            return;
        };
        let folded = self.palette.folded.contains(&section);
        match fold.unwrap_or(!folded) {
            true => self.palette.folded.insert(section),
            false => self.palette.folded.remove(&section),
        };
        // The selection stays on the header.
        self.palette.selected = self
            .palette_rows()
            .iter()
            .position(|r| matches!(r.kind, PaletteKind::Header { section: s, .. } if s == section))
            .unwrap_or(0);
    }

    /// Applies the selected item to the score pane (a header folds or unfolds).
    pub fn activate_palette(&mut self) {
        let rows = self.palette_rows();
        let Some(row) = rows.get(self.palette_selected()) else {
            return;
        };
        if matches!(row.kind, PaletteKind::Header { .. }) {
            return self.fold_palette(None);
        }
        if let Some(why) = &row.unavailable {
            return self.info(format!("{}: {why}", row.label));
        }
        // A tool that needs an argument opens the command line with its command typed.
        if row.command.ends_with(' ') {
            self.pending.clear();
            self.open_command_line(row.command);
            self.cmd_from_palette = true;
            return;
        }
        match Command::parse(row.command) {
            Ok(cmd) => self.run_tool(cmd),
            Err(e) => self.error(e),
        }
    }

    /// Opens the search field (`/`).
    pub(crate) fn palette_start_search(&mut self) {
        self.palette.search = Some(PalSearch {
            query: String::new(),
            origin: self.palette_selected(),
        });
    }

    /// Jumps to the match of `query` that is `dir` (1, -1) from row `from`, wrapping around:
    /// a section by its title, a tool by its name, tooltip, command, label or section. Only
    /// the rows shown now are searched (the tools of the notation and mode, unfolded).
    fn palette_jump(&mut self, query: &str, from: usize, dir: isize) -> bool {
        if query.is_empty() {
            return false;
        }
        let rows = self.palette_rows();
        let n = rows.len();
        let mut section = "";
        let hays: Vec<String> = rows
            .iter()
            .map(|r| match r.kind {
                PaletteKind::Header { .. } => {
                    section = r.label;
                    r.label.to_string()
                }
                PaletteKind::Item => format!(
                    "{}  {}  {}  {}  {}",
                    r.name(),
                    r.command,
                    r.short,
                    r.label,
                    section
                ),
            })
            .collect();
        for step in 0..n {
            let i = match dir {
                -1 => (from + n - step) % n,
                _ => (from + step) % n,
            };
            if Self::has_text(&hays[i], query) {
                self.palette.selected = i;
                let lines = self.palette_lines();
                self.palette.want_col.set(self.palette_at(&lines).1);
                return true;
            }
        }
        false
    }

    /// Keys while the search field is open: they type the query, which jumps as it is typed;
    /// Enter keeps the jump and sets the `/` register, Escape goes back.
    pub(crate) fn palette_search_key(&mut self, key: crate::keys::Key) {
        use crate::keys::KeyCode;
        let Some(mut search) = self.palette.search.take() else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                self.palette.selected = search.origin;
                return;
            }
            KeyCode::Enter => {
                if !search.query.is_empty() {
                    self.palette.last_search = search.query.clone();
                }
                return;
            }
            KeyCode::Backspace if search.query.is_empty() => {
                self.palette.selected = search.origin;
                return;
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
        // Each change looks again from where the search began.
        if !self.palette_jump(&search.query, search.origin, 1) {
            self.palette.selected = search.origin;
        }
        self.palette.search = Some(search);
    }

    /// `n` / `N`: the next / previous match of the `/` register.
    fn palette_search_step(&mut self, dir: isize) {
        let query = self.palette.last_search.clone();
        if query.is_empty() {
            return self.info("no previous search");
        }
        let n = self.palette_rows().len().max(1);
        let sel = self.palette_selected();
        let from = if dir > 0 {
            (sel + 1) % n
        } else {
            (sel + n - 1) % n
        };
        if !self.palette_jump(&query, from, dir) {
            self.error(format!("pattern not found: {query}"));
        }
    }

    /// The mouse moved over a row: the keyboard selection moves there, so there is one highlight.
    pub fn hover_palette(&mut self, row: usize) {
        if row < self.palette_rows().len() && row != self.palette.selected {
            self.palette.selected = row;
            let lines = self.palette_lines();
            self.palette.want_col.set(self.palette_at(&lines).1);
        }
    }

    /// Selects a row (a click) and applies it; the focus goes back to the score pane, as in a
    /// toolbar, so the next key edits.
    pub fn click_palette(&mut self, row: usize) {
        if row >= self.palette_rows().len() {
            return;
        }
        self.palette.selected = row;
        let lines = self.palette_lines();
        self.palette.want_col.set(self.palette_at(&lines).1);
        self.activate_palette();
        if self.focused_kind() == PaneKind::Palette {
            self.set_focus(self.focused());
        }
    }

    pub(crate) fn palette_key(&mut self, key: crate::keys::Key) {
        self.pending.push(key);
        let cmds = match self
            .config
            .keymap(crate::config::Mode::Palette)
            .lookup(&self.pending)
        {
            crate::config::Lookup::Exact(cmds) => cmds.to_vec(),
            crate::config::Lookup::Prefix => return,
            // Not a palette key: the keys go on as in any pane (`<C-w>` moves, `:` …).
            crate::config::Lookup::None => {
                let visual = self.mode == crate::config::Mode::Visual;
                for k in std::mem::take(&mut self.pending) {
                    if visual {
                        self.visual_key(k);
                    } else {
                        self.normal_key(k);
                    }
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

    pub(crate) fn run_palette(&mut self, cmd: &Command) -> bool {
        use Command::*;
        match cmd {
            Palette(on) => self.set_tool(PaneKind::Palette, *on),
            PaletteFocus => {
                if self.focused_kind() == PaneKind::Palette {
                    self.set_focus(self.focused());
                } else {
                    self.set_tool(PaneKind::Palette, Some(true));
                    if let Some(id) = self.tool_pane(PaneKind::Palette) {
                        self.set_focus(id);
                        self.pending.clear();
                    }
                }
            }
            PaletteDown => self.palette_vertical(1),
            PaletteUp => self.palette_vertical(-1),
            PaletteLeft => self.palette_horizontal(-1),
            PaletteRight => self.palette_horizontal(1),
            PaletteActivate => self.activate_palette(),
            PaletteSearch(0) => self.palette_start_search(),
            PaletteSearch(dir) => self.palette_search_step(*dir),
            PaletteFold(fold) => self.fold_palette(*fold),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{keys as press, test_app as app};
    use crate::config::Config;
    use crate::layout::{Axis, Node};

    fn select(app: &mut App, command: &str) {
        let at = app
            .palette_rows()
            .iter()
            .position(|r| r.command == command)
            .unwrap();
        app.palette.selected = at;
    }

    #[test]
    fn every_item_has_a_terminal_icon_or_a_short_label() {
        for (_, items) in SECTIONS {
            for (label, command, short) in items.iter() {
                assert!(
                    palette_glyph(command, true).is_some() || !short.is_empty(),
                    "{label}"
                );
                assert!(
                    palette_glyph(command, false).is_some() || !short.is_empty(),
                    "{label}"
                );
                assert!(short.chars().count() <= 3, "{label}: label too wide");
            }
        }
        assert_eq!(palette_glyph("duration 4", true), Some("\u{f038b}"));
        assert_eq!(palette_glyph("duration 4", false), Some("\u{2669}"));
        assert_eq!(palette_glyph("toggle bend", true), None);
    }

    #[test]
    fn every_item_is_a_command() {
        for (_, items) in SECTIONS {
            for (label, command, _) in items.iter() {
                match command.strip_suffix(' ') {
                    // A prompt: a command the command line completes.
                    Some(name) => assert!(
                        crate::complete::COMMANDS.contains(&name),
                        "{label}: {command}"
                    ),
                    None => assert!(Command::parse(command).is_ok(), "{label}: {command}"),
                }
            }
        }
    }

    #[test]
    fn it_opens_on_the_left_and_focus_enters_and_leaves() {
        let mut app = app();
        app.run(Command::Palette(Some(true)));
        let id = app.tool_pane(PaneKind::Palette).unwrap();
        assert_eq!(app.layout()[0].0, id, "the first pane is the palette");
        press(&mut app, "<leader>E");
        assert_eq!(app.focused_kind(), PaneKind::Palette);
        press(&mut app, "<Esc>");
        assert_eq!(app.focused_kind(), PaneKind::Score);
        app.run(Command::Palette(Some(false)));
        assert!(!app.has_tool(PaneKind::Palette));
    }

    #[test]
    fn it_coexists_with_the_properties_pane_and_bottom_tools() {
        let mut app = app();
        app.set_tool(PaneKind::Properties, Some(true));
        app.set_tool(PaneKind::Palette, Some(true));
        app.set_tool(PaneKind::Fretboard, Some(true));
        let kinds: Vec<PaneKind> = app
            .layout()
            .iter()
            .map(|(id, _)| app.panes[id].kind)
            .collect();
        assert_eq!(kinds[0], PaneKind::Palette);
        assert!(kinds.contains(&PaneKind::Properties) && kinds.contains(&PaneKind::Fretboard));
        app.set_tool(PaneKind::Palette, Some(false));
        assert!(app.has_tool(PaneKind::Properties) && app.has_tool(PaneKind::Fretboard));
    }

    #[test]
    fn enter_applies_an_item_to_the_score_pane() {
        let mut app = app();
        app.run(Command::PaletteFocus);
        select(&mut app, "duration 8");
        press(&mut app, "<CR>");
        assert_eq!(app.insert_duration().value.denominator(), 8);
        assert_eq!(app.focused_kind(), PaneKind::Palette, "the focus stays");
        let row = app
            .palette_rows()
            .into_iter()
            .find(|r| r.command == "duration 8")
            .unwrap();
        assert!(row.active);
    }

    #[test]
    fn j_and_k_move_and_za_folds() {
        let mut app = app();
        app.run(Command::PaletteFocus);
        let all = app.palette_rows().len();
        press(&mut app, "j");
        assert_eq!(app.palette_selected(), 1);
        press(&mut app, "k");
        press(&mut app, "za");
        assert!(app.palette_rows().len() < all);
        press(&mut app, "<CR>");
        assert_eq!(
            app.palette_rows().len(),
            all,
            "enter on a header unfolds it"
        );
    }

    #[test]
    fn dynamics_set_the_note_velocity() {
        let mut app = app();
        app.run(Command::parse("note C4").unwrap());
        app.run(Command::PaletteFocus);
        select(&mut app, "dynamic pp");
        press(&mut app, "<CR>");
        let note = app.palette_note().unwrap();
        assert_eq!(note.velocity, 31);
        let row = app
            .palette_rows()
            .into_iter()
            .find(|r| r.command == "dynamic pp")
            .unwrap();
        assert!(row.active);
    }

    #[test]
    fn a_click_applies_and_returns_to_the_score() {
        let mut app = app();
        app.run(Command::PaletteFocus);
        let at = app
            .palette_rows()
            .iter()
            .position(|r| r.command == "duration 2")
            .unwrap();
        app.click_palette(at);
        assert_eq!(app.insert_duration().value.denominator(), 2);
        assert_eq!(app.focused_kind(), PaneKind::Score);
    }

    fn active(app: &App) -> Vec<&'static str> {
        app.palette_rows()
            .into_iter()
            .filter(|r| r.active)
            .map(|r| r.command)
            .collect()
    }

    #[test]
    fn the_active_set_follows_the_cursor() {
        let mut app = app();
        app.run(Command::parse("duration 8").unwrap());
        app.run(Command::parse("dynamic ppp").unwrap());
        for _ in 0..3 {
            app.run(Command::BeatNext);
        }
        app.run(Command::parse("duration 16").unwrap());
        app.run(Command::parse("dynamic ff").unwrap());
        app.run(Command::parse("toggle palm-mute").unwrap());
        let here = active(&app);
        assert!(here.contains(&"duration 16") && here.contains(&"dynamic ff"));
        assert!(here.contains(&"toggle palm-mute"));
        assert!(!here.contains(&"duration 8"));
        for _ in 0..3 {
            app.run(Command::BeatPrev);
        }
        let there = active(&app);
        assert!(there.contains(&"duration 8") && there.contains(&"dynamic ppp"));
        assert!(!there.contains(&"duration 16") && !there.contains(&"toggle palm-mute"));
        // A rest in between: the rest button is in force.
        app.run(Command::BeatNext);
        assert!(active(&app).contains(&"rest"));
    }

    #[test]
    fn h_and_l_move_along_a_line_and_j_k_keep_the_column() {
        let mut app = app();
        app.run(Command::PaletteFocus);
        app.set_palette_cols(4);
        let lines = app.palette_lines();
        let PaletteLine::Cells(first) = &lines[1] else {
            panic!("a line of buttons after the first title");
        };
        assert_eq!(first.len(), 4);
        press(&mut app, "j");
        assert_eq!(app.palette_selected(), first[0]);
        press(&mut app, "ll");
        assert_eq!(app.palette_selected(), first[2]);
        press(&mut app, "j");
        let PaletteLine::Cells(second) = &app.palette_lines()[2] else {
            panic!()
        };
        assert_eq!(app.palette_selected(), second[2]);
        press(&mut app, "k");
        assert_eq!(app.palette_selected(), first[2]);
        press(&mut app, "hhhh");
        assert_eq!(app.palette_selected(), first[0], "no wrapping");
        // Down past the last button of a short line goes to its last one.
        app.palette.want_col.set(3);
        for _ in 0..2 {
            press(&mut app, "j");
        }
        let PaletteLine::Cells(third) = &app.palette_lines()[3] else {
            panic!()
        };
        assert_eq!(app.palette_selected(), *third.last().unwrap());
    }

    #[test]
    fn hovering_moves_the_selection() {
        let mut app = app();
        app.set_palette_cols(4);
        let at = app
            .palette_rows()
            .iter()
            .position(|r| r.command == "duration 4")
            .unwrap();
        app.hover_palette(at);
        assert_eq!(app.palette_selected(), at);
        assert_eq!(app.focused_kind(), PaneKind::Score, "the focus stays");
        // Keyboard moves go on from there.
        app.run(Command::PaletteFocus);
        press(&mut app, "l");
        assert_eq!(app.palette_selected(), at + 1);
    }

    fn disabled(app: &App) -> Vec<&'static str> {
        app.palette_rows()
            .into_iter()
            .filter(|r| r.kind == PaletteKind::Item && r.unavailable.is_some())
            .map(|r| r.command)
            .collect()
    }

    #[test]
    fn note_items_are_unavailable_off_a_note() {
        let mut app = app();
        // On a note everything listed applies; accidentals are not listed on a tab.
        let accidentals = ["accidental sharp", "accidental flat", "accidental natural"];
        assert!(disabled(&app).is_empty());
        assert!(
            !app.palette_rows()
                .iter()
                .any(|r| accidentals.contains(&r.command))
        );
        // On a rest the note-level items are out; durations, rest and dynamics-free items stay.
        app.run(Command::BeatNext);
        let off = disabled(&app);
        for c in [
            "toggle tie",
            "toggle palm-mute",
            "toggle hammer",
            "octave-up",
            "dynamic mf",
        ] {
            assert!(off.contains(&c), "{c} in {off:?}");
        }
        for c in [
            "duration 8",
            "dot",
            "tuplet 3",
            "rest",
            "toggle tap",
            "toggle fade-in",
        ] {
            assert!(!off.contains(&c), "{c} in {off:?}");
        }
        // Back on a note they come back.
        app.run(Command::BeatPrev);
        assert!(disabled(&app).is_empty());
    }

    #[test]
    fn applying_an_unavailable_item_does_nothing() {
        let mut app = app();
        app.run(Command::BeatNext);
        let before = app.buffer().score.clone();
        let version = app.buffer().version;
        app.run(Command::PaletteFocus);
        select(&mut app, "toggle palm-mute");
        press(&mut app, "<CR>");
        assert_eq!(app.buffer().score, before);
        assert_eq!(app.buffer().version, version);
        let name = app
            .palette_rows()
            .into_iter()
            .find(|r| r.command == "toggle palm-mute")
            .unwrap()
            .name();
        assert!(name.ends_with("(unavailable)"));
        assert!(
            app.message
                .as_ref()
                .is_some_and(|m| m.text.contains("palm-mute") || m.text.contains("Palm mute"))
        );
        // A click does nothing either.
        let at = app
            .palette_rows()
            .iter()
            .position(|r| r.command == "toggle palm-mute")
            .unwrap();
        app.click_palette(at);
        assert_eq!(app.buffer().score, before);
    }

    #[test]
    fn bottom_tools_run_under_the_palette_at_the_full_width() {
        let rect = |app: &App, id| app.layout().into_iter().find(|(p, _)| *p == id).unwrap().1;
        // Whichever is opened first, and with the properties pane too.
        for order in [[0, 1, 2], [1, 0, 2], [2, 1, 0], [1, 2, 0]] {
            let mut app = app();
            for k in order {
                match k {
                    0 => app.set_tool(PaneKind::Palette, Some(true)),
                    1 => app.set_tool(PaneKind::Fretboard, Some(true)),
                    _ => app.set_tool(PaneKind::Properties, Some(true)),
                }
            }
            let pane = |app: &App, k| rect(app, app.tool_pane(k).unwrap());
            let (palette, tools, props) = (
                pane(&app, PaneKind::Palette),
                pane(&app, PaneKind::Fretboard),
                pane(&app, PaneKind::Properties),
            );
            assert!(
                palette.y + palette.h <= tools.y,
                "{order:?}: {palette:?} {tools:?}"
            );
            assert_eq!((palette.x, tools.x), (0, 0), "{order:?}");
            // The properties pane stays outermost along the right, at the full height.
            assert_eq!(tools.x + tools.w, props.x, "{order:?}");
            assert_eq!(props.y, 0);
            assert!(props.h > tools.h);
            let score = rect(&app, app.focused());
            assert!(score.x >= palette.x + palette.w, "{order:?}");
            // Closing the palette leaves the others.
            app.set_tool(PaneKind::Palette, Some(false));
            assert!(app.has_tool(PaneKind::Fretboard) && app.has_tool(PaneKind::Properties));
            assert_eq!(pane(&app, PaneKind::Fretboard).x, 0);
        }
    }

    /// The old shape: the palette outermost with the bottom tool nested in the score column.
    fn old_shape(app: &mut App) {
        let palette = app.tool_pane(PaneKind::Palette).unwrap();
        let tool = app.tool_pane(PaneKind::Fretboard).unwrap();
        let score = app.focused();
        let v = app.active_view;
        app.views[v].root = Node::Split {
            axis: Axis::SideBySide,
            children: vec![
                Node::Leaf(palette),
                Node::Split {
                    axis: Axis::Stacked,
                    children: vec![Node::Leaf(score), Node::Leaf(tool)],
                    weights: vec![1.0, 1.0],
                },
            ],
            weights: vec![1.0, 1.0],
        };
    }

    #[test]
    fn a_saved_old_shape_is_renested_on_restore() {
        let mut app = app();
        app.set_tool(PaneKind::Palette, Some(true));
        app.set_tool(PaneKind::Fretboard, Some(true));
        let sizes = |app: &App| {
            let size = |k| app.panes[&app.tool_pane(k).unwrap()].size;
            (size(PaneKind::Palette), size(PaneKind::Fretboard))
        };
        let before = sizes(&app);
        old_shape(&mut app);
        let text = toml::to_string_pretty(&app.workspace()).unwrap();
        let mut other = crate::app::tests::test_app();
        other.restore(&toml::from_str(&text).unwrap());
        let rect = |app: &App, id| app.layout().into_iter().find(|(p, _)| *p == id).unwrap().1;
        let palette = rect(&other, other.tool_pane(PaneKind::Palette).unwrap());
        let tool = rect(&other, other.tool_pane(PaneKind::Fretboard).unwrap());
        assert_eq!((tool.x, tool.w), (0, other.viewport.w), "{tool:?}");
        assert!(palette.y + palette.h <= tool.y, "{palette:?} {tool:?}");
        assert_eq!(palette.x, 0);
        assert_eq!(sizes(&other), before, "the sizes are kept");
    }

    #[test]
    fn normalising_a_correct_tree_changes_nothing() {
        let mut app = app();
        app.set_tool(PaneKind::Palette, Some(true));
        app.set_tool(PaneKind::Fretboard, Some(true));
        app.set_tool(PaneKind::Properties, Some(true));
        // A manual resize shows in the weights, which stay.
        app.views[app.active_view].root.equalize();
        let before = app.view().root.clone();
        app.normalize_layouts();
        assert_eq!(app.view().root, before);
        // The old shape is mended to the same nesting.
        let mut other = crate::app::tests::test_app();
        other.set_tool(PaneKind::Palette, Some(true));
        other.set_tool(PaneKind::Fretboard, Some(true));
        let good = other.view().root.shape();
        old_shape(&mut other);
        other.normalize_layouts();
        assert_eq!(other.view().root.shape(), good);
    }

    #[test]
    fn names_carry_the_insert_mode_keys() {
        let mut app = app();
        let name = |app: &App, command: &str| {
            app.palette_rows()
                .into_iter()
                .find(|r| r.command == command)
                .unwrap()
                .name()
        };
        // The test track is a tab: its keys, then the shared ones.
        assert_eq!(name(&app, "toggle palm-mute"), "Palm mute (m)");
        assert_eq!(name(&app, "duration 4"), "Quarter note (<A-3>)");
        assert_eq!(name(&app, "dot"), "Dotted (.)");
        // Arguments tell buttons apart; one with no key shows none.
        assert_eq!(name(&app, "dynamic mf"), "Mezzo forte (mf)");
        assert_eq!(name(&app, "toggle fade-in"), "Fade in");
        // Unavailable ones say so after the keys.
        app.run(Command::BeatNext);
        assert_eq!(
            name(&app, "toggle palm-mute"),
            "Palm mute (m) (unavailable)"
        );
        // A user override changes the text.
        let (config, errors) = Config::parse(
            "[keymap.insert-tab]\n\"p\" = \"toggle palm-mute\"\n\"M\" = \"toggle palm-mute\"\n",
        )
        .unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        app.config = config;
        assert_eq!(
            name(&app, "toggle palm-mute"),
            "Palm mute (M, m, p) (unavailable)"
        );
    }
}
