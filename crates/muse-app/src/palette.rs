//! The editing palette: a vertical list of note durations, rests, accidentals, dynamics,
//! articulations and effects along the left of the window, in collapsible sections. `<leader>e`
//! shows it, `<leader>E` focuses it (Palette mode: `j`/`k` choose an item, `Enter` applies it to
//! the score pane worked on, `za` folds a section). Every item runs an ordinary command; the
//! rows are built here and the frontends only draw them.

use std::cell::Cell;
use std::collections::HashSet;

use crate::app::{App, PaneKind};
use crate::command::Command;
use muse_core::TrackKind;

/// Width of the pane, in columns.
pub const WIDTH: u16 = 26;

/// A section: its title and its items (label, command line).
pub const SECTIONS: &[(&str, &[(&str, &str)])] = &[
    (
        "Duration",
        &[
            ("Whole", "duration 1"),
            ("Half", "duration 2"),
            ("Quarter", "duration 4"),
            ("Eighth", "duration 8"),
            ("16th", "duration 16"),
            ("32nd", "duration 32"),
            ("64th", "duration 64"),
            ("Dotted", "dot"),
            ("Triplet", "tuplet 3"),
            ("Rest", "rest"),
        ],
    ),
    (
        "Pitch",
        &[
            ("Sharp", "accidental sharp"),
            ("Flat", "accidental flat"),
            ("Natural", "accidental natural"),
            ("Octave up", "octave-up"),
            ("Octave down", "octave-down"),
            ("Tie", "toggle tie"),
            ("Dead note", "toggle dead"),
            ("Ghost note", "toggle ghost"),
        ],
    ),
    (
        "Dynamics",
        &[
            ("ppp", "dynamic ppp"),
            ("pp", "dynamic pp"),
            ("p", "dynamic p"),
            ("mp", "dynamic mp"),
            ("mf", "dynamic mf"),
            ("f", "dynamic f"),
            ("ff", "dynamic ff"),
            ("fff", "dynamic fff"),
        ],
    ),
    (
        "Articulation",
        &[
            ("Accent", "toggle accent"),
            ("Heavy accent", "toggle heavy-accent"),
            ("Staccato", "toggle staccato"),
            ("Let ring", "toggle let-ring"),
            ("Palm mute", "toggle palm-mute"),
        ],
    ),
    (
        "Effects",
        &[
            ("Hammer / pull", "toggle hammer"),
            ("Slide", "toggle slide"),
            ("Bend", "toggle bend"),
            ("Half bend", "toggle half-bend"),
            ("Vibrato", "toggle vibrato"),
            ("Wide vibrato", "toggle vibrato-wide"),
            ("Harmonic", "toggle harmonic"),
            ("Tremolo picking", "toggle tremolo"),
            ("Tap", "toggle tap"),
            ("Slap", "toggle slap"),
            ("Pop", "toggle pop"),
            ("Fade in", "toggle fade-in"),
            ("Stroke down", "toggle stroke-down"),
            ("Stroke up", "toggle stroke-up"),
        ],
    ),
];

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteKind {
    Header { section: usize, folded: bool },
    Item,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaletteRow {
    pub kind: PaletteKind,
    pub label: &'static str,
    /// The command line an item runs (empty for a header).
    pub command: &'static str,
    /// The item's setting is in force at the cursor (the duration, a technique …).
    pub active: bool,
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

impl App {
    /// The rows: each section's header, then its items unless folded.
    pub fn palette_rows(&self) -> Vec<PaletteRow> {
        let note = self.palette_note();
        let duration = self.insert_duration();
        let mut rows = Vec::new();
        for (s, (title, items)) in SECTIONS.iter().enumerate() {
            let folded = self.palette.folded.contains(&s);
            rows.push(PaletteRow {
                kind: PaletteKind::Header { section: s, folded },
                label: title,
                command: "",
                active: false,
            });
            if folded {
                continue;
            }
            for (label, command) in items.iter() {
                let (name, arg) = command.split_once(' ').unwrap_or((command, ""));
                let active = match (name, arg) {
                    ("duration", n) => n.parse() == Ok(duration.value.denominator()),
                    ("dot", _) => duration.dots > 0,
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
                        "slide" => !n.effects.slides.is_empty(),
                        "bend" | "half-bend" => n.effects.bend.is_some(),
                        "vibrato" => n.effects.vibrato,
                        "harmonic" | "pinch-harmonic" => n.effects.harmonic.is_some(),
                        "tremolo" => n.effects.tremolo_picking.is_some(),
                        _ => false,
                    }),
                    _ => false,
                };
                rows.push(PaletteRow {
                    kind: PaletteKind::Item,
                    label,
                    command,
                    active,
                });
            }
        }
        rows
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

    /// The first row to draw, given `visible` rows of room: the selected row is kept in view
    /// when it has just changed; else the wheel's scrolling stands.
    pub fn palette_window(&self, visible: usize) -> usize {
        let n = self.palette_rows().len();
        let sel = self.palette_selected();
        let visible = visible.max(1);
        let mut first = self.palette.scroll.get().min(n.saturating_sub(1));
        if sel != self.palette.seen.get() {
            self.palette.seen.set(sel);
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

    /// The wheel: scrolls the rows by `by` (positive: down) without moving the selection.
    pub fn scroll_palette(&self, by: isize) {
        let n = self.palette_rows().len() as isize;
        let first = (self.palette.scroll.get() as isize + by).clamp(0, (n - 1).max(0));
        self.palette.scroll.set(first as usize);
        self.palette.seen.set(self.palette_selected());
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
        match Command::parse(row.command) {
            Ok(cmd) => self.run(cmd),
            Err(e) => self.error(e),
        }
    }

    /// Selects a row (a click) and applies it; the focus goes back to the score pane, as in a
    /// toolbar, so the next key edits.
    pub fn click_palette(&mut self, row: usize) {
        if row >= self.palette_rows().len() {
            return;
        }
        self.palette.selected = row;
        self.activate_palette();
        if self.focused_kind() == PaneKind::Palette {
            self.set_focus(self.focused());
        }
    }

    pub(crate) fn palette_key(&mut self, key: crate::keys::Key) {
        self.pending.push(key);
        let cmds = match self.config.keymap(crate::config::Mode::Palette).lookup(&self.pending) {
            crate::config::Lookup::Exact(cmds) => cmds.to_vec(),
            crate::config::Lookup::Prefix => return,
            // Not a palette key: the keys go on as in any pane (`<C-w>` moves, `:` …).
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
            PaletteDown => {
                let last = self.palette_rows().len().saturating_sub(1);
                self.palette.selected = (self.palette_selected() + 1).min(last);
            }
            PaletteUp => self.palette.selected = self.palette_selected().saturating_sub(1),
            PaletteActivate => self.activate_palette(),
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

    fn select(app: &mut App, command: &str) {
        let at = app
            .palette_rows()
            .iter()
            .position(|r| r.command == command)
            .unwrap();
        app.palette.selected = at;
    }

    #[test]
    fn every_item_is_a_command() {
        for (_, items) in SECTIONS {
            for (label, command) in items.iter() {
                assert!(Command::parse(command).is_ok(), "{label}: {command}");
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
        let kinds: Vec<PaneKind> = app.layout().iter().map(|(id, _)| app.panes[id].kind).collect();
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
        assert_eq!(app.palette_rows().len(), all, "enter on a header unfolds it");
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
}
