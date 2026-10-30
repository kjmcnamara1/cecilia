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
use muse_core::TrackKind;

/// Width of the pane, in columns.
pub const WIDTH: u16 = 44;

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
            PropTab::Tracks => "tracks",
            PropTab::Lyrics => "lyrics",
        }
    }

    pub fn parse(s: &str) -> Option<PropTab> {
        PropTab::ALL.into_iter().find(|t| t.name() == s.trim())
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
    ScoreTempo,
    ScorePage,
    TrackInfo,
    TrackNotation,
    TrackSound,
    Verses,
}

impl Section {
    pub fn name(self) -> &'static str {
        match self {
            Section::ScoreInfo => "Info",
            Section::ScoreTempo => "Tempo",
            Section::ScorePage => "Page",
            Section::TrackInfo => "Info",
            Section::TrackNotation => "Notation",
            Section::TrackSound => "Sound",
            Section::Verses => "Verses",
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
    Tempo,
    Pickup,
    PageWidth,
    PageHeight,
    MarginLeft,
    MarginRight,
    MarginTop,
    MarginBottom,
    PageScale,
    TrackName,
    TrackKind,
    TrackColor,
    Tuning,
    FretCount,
    Capo,
    /// The staves of the track (read-only).
    Staves,
    /// The cursor's staff.
    Staff,
    Program,
    Channel,
    Bank,
    Volume,
    Pan,
    Mute,
    Solo,
    Verse(usize),
    NewVerse,
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
}

/// A property being typed.
#[derive(Debug, Clone, PartialEq)]
pub struct PropEdit {
    pub field: Field,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct PropsState {
    pub tab: PropTab,
    /// Selected row of the tab's [`App::property_rows`].
    pub selected: usize,
    pub folded: HashSet<Section>,
    pub edit: Option<PropEdit>,
    /// First row shown; kept by [`App::property_window`].
    scroll: Cell<usize>,
}

fn onoff(b: bool) -> String {
    if b { "on" } else { "off" }.into()
}

fn kind_name(k: TrackKind) -> &'static str {
    match k {
        TrackKind::Fretted => "fretted",
        TrackKind::Drums => "drums",
        TrackKind::Pitched => "pitched",
        TrackKind::Chant => "chant",
    }
}

/// A tuning as note names from the lowest string up (what `:tuning` takes).
fn tuning_text(strings: &[u8]) -> String {
    strings
        .iter()
        .rev()
        .map(|&p| muse_core::pitch_name(p))
        .collect::<Vec<_>>()
        .join(" ")
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
            });
            !f
        };
        let field = |rows: &mut Vec<PropRow>, f: Field, label: &str, value: String, editable| {
            rows.push(PropRow {
                kind: RowKind::Field { field: f, editable },
                label: label.into(),
                value,
            })
        };
        match self.props.tab {
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
                if section(&mut rows, Section::ScoreTempo) {
                    field(
                        &mut rows,
                        Field::Tempo,
                        "Tempo",
                        score.tempo.to_string(),
                        true,
                    );
                    field(
                        &mut rows,
                        Field::Pickup,
                        "Pickup bar",
                        onoff(score.anacrusis),
                        true,
                    );
                }
                if section(&mut rows, Section::ScorePage) {
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
                        field(&mut rows, f, l, v.to_string(), true);
                    }
                }
            }
            PropTab::Tracks => {
                let Some(t) = score.tracks.get(cursor.track) else {
                    return vec![note("no track")];
                };
                let n = score.tracks.len();
                rows.push(note(&format!("Track {} of {n}", cursor.track + 1)));
                if section(&mut rows, Section::TrackInfo) {
                    field(&mut rows, Field::TrackName, "Name", t.name.clone(), true);
                    field(
                        &mut rows,
                        Field::TrackKind,
                        "Kind",
                        kind_name(t.kind).into(),
                        false,
                    );
                    let color = match t.display_color {
                        Some(c) => format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b),
                        None => "auto".into(),
                    };
                    field(&mut rows, Field::TrackColor, "Colour", color, true);
                }
                if section(&mut rows, Section::TrackNotation) {
                    let fretted = t.kind == TrackKind::Fretted;
                    if t.kind != TrackKind::Drums && !t.strings.is_empty() {
                        field(
                            &mut rows,
                            Field::Tuning,
                            "Tuning",
                            tuning_text(&t.strings),
                            true,
                        );
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
                    let staves = t.staves();
                    let names: Vec<String> = staves.iter().map(|s| s.name()).collect();
                    field(&mut rows, Field::Staves, "Staves", names.join(", "), false);
                    let here = cursor.staff.min(staves.len().saturating_sub(1));
                    if let Some(s) = staves.get(here) {
                        field(
                            &mut rows,
                            Field::Staff,
                            &format!("Staff {}", here + 1),
                            s.name(),
                            true,
                        );
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
                rows.push(note(&format!("Lyrics of {name}")));
                if section(&mut rows, Section::Verses) {
                    for v in 0..muse_core::lyrics::verse_count(t) {
                        let text = muse_core::lyrics::verse_line(t, v).map(|l| l.text);
                        field(
                            &mut rows,
                            Field::Verse(v),
                            &format!("Verse {}", v + 1),
                            text.unwrap_or_default(),
                            true,
                        );
                    }
                    field(&mut rows, Field::NewVerse, "+ Verse", String::new(), true);
                }
            }
        }
        rows
    }

    /// The selected row, clamped to the rows there are.
    pub fn property_selected(&self) -> usize {
        self.props
            .selected
            .min(self.property_rows().len().saturating_sub(1))
    }

    /// Which rows of `visible` fit are shown: the first row index, kept so the selected row
    /// stays in view.
    pub fn property_window(&self, visible: usize) -> usize {
        let sel = self.property_selected();
        let mut first = self.props.scroll.get();
        if sel < first {
            first = sel;
        } else if visible > 0 && sel >= first + visible {
            first = sel + 1 - visible;
        }
        let total = self.property_rows().len();
        first = first.min(total.saturating_sub(visible));
        self.props.scroll.set(first);
        first
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
            self.props.edit = None;
        }
    }

    /// Selects a row (a click); a section header folds or unfolds.
    pub fn select_property(&mut self, row: usize) {
        self.props.edit = None;
        let rows = self.property_rows();
        if row >= rows.len() {
            return;
        }
        if rows[row].kind == RowKind::Note {
            return;
        }
        self.props.selected = row;
        if matches!(rows[row].kind, RowKind::Header { .. }) {
            self.fold_property();
        }
    }

    fn fold_property(&mut self) {
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
            if !self.props.folded.remove(&s) {
                self.props.folded.insert(s);
            }
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
            RowKind::Header { .. } => return self.fold_property(),
            RowKind::Note => return,
            RowKind::Field { field, editable } => (field, editable),
        };
        let t = self.pane().cursor.track;
        match field {
            _ if !editable => self.info(format!("{}: can't be edited here", row.label)),
            Field::Pickup => self.edit_pickup(),
            Field::Mute => self.run(Command::Mute(Some(Strip::Track(t)))),
            Field::Solo => self.run(Command::Solo(Some(Strip::Track(t)))),
            Field::Program => self.run(Command::TrackInstrument(None)),
            Field::TrackColor if row.value == "auto" => {
                self.props.edit = Some(PropEdit {
                    field,
                    text: String::new(),
                })
            }
            _ => {
                self.props.edit = Some(PropEdit {
                    field,
                    text: row.value.clone(),
                })
            }
        }
    }

    fn edit_pickup(&mut self) {
        self.edit(|cx| {
            let s = cx.whole();
            s.anacrusis = !s.anacrusis;
            Ok(())
        });
    }

    /// Keys while a property is typed: they edit its text; Enter keeps it, Escape drops it.
    pub(crate) fn props_edit_key(&mut self, key: Key) {
        let Some(edit) = self.props.edit.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.props.edit = None,
            KeyCode::Enter => {
                let PropEdit { field, text } = self.props.edit.take().unwrap();
                self.commit_property(field, text.trim());
            }
            KeyCode::Backspace => {
                edit.text.pop();
            }
            _ if key.ctrl && key.as_char() == Some('u') => edit.text.clear(),
            _ => {
                if let Some(c) = key.as_char().filter(|_| !key.ctrl) {
                    edit.text.push(c);
                }
            }
        }
    }

    /// Applies typed text to a property.
    fn commit_property(&mut self, field: Field, text: &str) {
        let t = self.pane().cursor.track;
        // A whole number within bounds, or an error naming the property.
        let number = |app: &mut App, what: &str, min: u32, max: u32| -> Option<u32> {
            match text.parse::<u32>() {
                Ok(n) if (min..=max).contains(&n) => Some(n),
                _ => {
                    app.error(format!("{what}: expected a number from {min} to {max}"));
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
            Field::Tempo => {
                if let Some(n) = number(self, "tempo", 10, 999) {
                    self.edit(|cx| {
                        cx.whole().tempo = n;
                        Ok(())
                    });
                }
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
            Field::TrackName => self.run(Command::TrackRename(format!("{text} {}", t + 1))),
            Field::TrackColor => {
                let arg = if text.is_empty() { "off" } else { text };
                self.run(Command::TrackColor(Some(arg.to_string())));
            }
            Field::Tuning => self.run(Command::Tuning(text.to_string())),
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
            Field::Staff => self.run(Command::Staff(text.to_string())),
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
            Field::Verse(v) => self.set_verse(t, v, text),
            Field::NewVerse => {
                let verse = self
                    .buffer()
                    .score
                    .tracks
                    .get(t)
                    .map_or(0, muse_core::lyrics::verse_count);
                self.set_verse(t, verse, text);
            }
            Field::TrackKind
            | Field::Staves
            | Field::Pickup
            | Field::Mute
            | Field::Solo
            | Field::Program => {}
        }
    }

    /// Replaces a verse of a track with `text` (Guitar Pro lyric syntax), placed from the bar
    /// the verse started at, or the cursor's bar for a new one: one undo step.
    fn set_verse(&mut self, track: usize, verse: usize, text: &str) {
        let syllables = muse_core::lyrics::syllables(text);
        let cursor_bar = self.pane().cursor.bar;
        let placed = self.edit(|cx| {
            let score = cx.whole();
            let t = score.tracks.get_mut(track).ok_or("no such track")?;
            let start = muse_core::lyrics::verse_line(t, verse)
                .map_or(cursor_bar, |l| l.start_bar as usize);
            muse_core::lyrics::clear(t, Some(verse));
            let n = muse_core::lyrics::place(t, verse, start, 0, &syllables);
            crate::editor::refresh_lyric_lines(score, track);
            Ok(n)
        });
        if let Some(n) = placed.filter(|&n| n < syllables.len()) {
            self.error(format!(
                "lyrics: {} syllables did not fit",
                syllables.len() - n
            ));
        }
    }

    /// Clears the selected verse, or the track's own colour.
    fn clear_property(&mut self) {
        match self.selected_field().map(|(f, _)| f) {
            Some(Field::Verse(v)) => self.run(Command::LyricsClear(Some(v))),
            Some(Field::TrackColor) => self.run(Command::TrackColor(Some("off".into()))),
            Some(Field::Staff) => self.run(Command::StaffRemove(None)),
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
                    self.props.edit = None;
                    self.set_focus(self.focused());
                } else {
                    self.set_tool(PaneKind::Properties, Some(true));
                    if let Some(id) = self.tool_pane(PaneKind::Properties) {
                        self.set_focus(id);
                        self.pending.clear();
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
            }
            PropertiesTab(Some(tab)) => self.set_property_tab(*tab),
            PropertiesTab(None) => self.set_property_tab(self.props.tab.step(1)),
            PropertiesTabPrev => self.set_property_tab(self.props.tab.step(-1)),
            PropertiesEdit => self.edit_property(),
            PropertiesFold => self.fold_property(),
            PropertiesClear => self.clear_property(),
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{keys, test_app};
    use crate::layout::{Axis, Node};

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

    fn type_into(app: &mut App, f: Field, text: &str) {
        app.props.selected = field_row(app, f);
        app.run(Command::PropertiesEdit);
        assert!(app.props.edit.is_some());
        app.props.edit.as_mut().unwrap().text.clear();
        for c in text.chars() {
            app.props_edit_key(Key::char(c));
        }
        app.props_edit_key(Key {
            code: KeyCode::Enter,
            ctrl: false,
            alt: false,
        });
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
        keys(&mut app, "l");
        assert_eq!(app.props.tab, PropTab::Lyrics);
        keys(&mut app, "l");
        assert_eq!(app.props.tab, PropTab::Score, "tabs wrap");
        keys(&mut app, "h");
        assert_eq!(app.props.tab, PropTab::Lyrics);
        keys(&mut app, "1");
        let before = app.property_rows().len();
        keys(&mut app, "za");
        let after = app.property_rows().len();
        assert!(after < before, "folding the info section hides its rows");
        keys(&mut app, "za");
        assert_eq!(app.property_rows().len(), before);
    }

    #[test]
    fn score_properties_edit_and_undo() {
        let mut app = with_props();
        type_into(&mut app, Field::Title, "My Song");
        assert_eq!(app.buffer().score.meta.title, "My Song");
        type_into(&mut app, Field::Tempo, "90");
        assert_eq!(app.buffer().score.tempo, 90);
        // The tempo field is the score's, wherever the cursor is.
        app.pane_mut().cursor.bar = 3;
        type_into(&mut app, Field::Tempo, "100");
        assert_eq!(app.buffer().score.tempo, 100);
        assert!(app.buffer().score.master_bars[3].tempo_changes.is_empty());
        type_into(&mut app, Field::MarginLeft, "25");
        assert_eq!(app.buffer().score.page.margin_left, 25);
        type_into(&mut app, Field::PageScale, "x");
        assert_eq!(
            app.buffer().score.page.scale,
            100,
            "bad numbers are refused"
        );
        app.props.selected = field_row(&app, Field::Pickup);
        app.run(Command::PropertiesEdit);
        assert!(app.buffer().score.anacrusis);
        app.run(Command::Undo);
        assert!(!app.buffer().score.anacrusis);
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
        type_into(&mut app, Field::TrackColor, "#102030");
        assert_eq!(
            app.buffer().score.tracks[0]
                .display_color
                .map(|c| (c.r, c.g, c.b)),
            Some((0x10, 0x20, 0x30))
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
    fn lyrics_verses_edit_as_text_in_one_undo_step() {
        let mut app = with_props();
        app.run(Command::Lyrics(0, "Hel-lo world".into()));
        app.set_property_tab(PropTab::Lyrics);
        let rows = app.property_rows();
        assert!(
            rows.iter()
                .any(|r| r.label == "Verse 1" && r.value == "Hel-lo world"),
            "{rows:?}"
        );
        type_into(&mut app, Field::Verse(0), "Good-bye moon");
        let line = muse_core::lyrics::verse_line(&app.buffer().score.tracks[0], 0).unwrap();
        assert_eq!(line.text, "Good-bye moon");
        type_into(&mut app, Field::NewVerse, "la la");
        assert!(muse_core::lyrics::verse_line(&app.buffer().score.tracks[0], 1).is_some());
        app.run(Command::Undo);
        assert!(muse_core::lyrics::verse_line(&app.buffer().score.tracks[0], 1).is_none());
        app.run(Command::Undo);
        let line = muse_core::lyrics::verse_line(&app.buffer().score.tracks[0], 0).unwrap();
        assert_eq!(line.text, "Hel-lo world");
        // `x` clears the selected verse.
        app.props.selected = field_row(&app, Field::Verse(0));
        app.run(Command::PropertiesClear);
        assert!(muse_core::lyrics::verse_line(&app.buffer().score.tracks[0], 0).is_none());
    }

    #[test]
    fn typing_takes_every_key_and_escape_drops_it() {
        let mut app = with_props();
        keys(&mut app, "<leader>P");
        app.props.selected = field_row(&app, Field::Title);
        keys(&mut app, "ijq<Esc>");
        assert!(app.props.edit.is_none(), "Escape ends the edit");
        assert_eq!(app.buffer().score.meta.title, "");
        assert_eq!(
            app.focused_kind(),
            PaneKind::Properties,
            "and keeps the focus in the pane"
        );
        keys(&mut app, "iAB<CR>");
        assert_eq!(app.buffer().score.meta.title, "AB");
    }
}
