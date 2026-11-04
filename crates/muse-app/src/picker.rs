//! Pickers: the command palette's box, listing buffers, views or workspaces to switch to. Typing
//! filters the list (with the completion settings: fuzzy, case), Up/Down move, Enter switches.

use crate::app::{App, Palette, PaletteItem};
use crate::command::Command;
use crate::config::Mode;
use crate::icons::Icon;
use crate::keys::{Key, KeyCode};
use crate::theme::Rgba;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    Buffer,
    View,
    Workspace,
    Mark,
    Macro,
    /// Every key binding of every mode, found by keys, command or description.
    Keymap,
    /// The sections (markers) of the file shown.
    Section,
    /// General MIDI instruments (and the drum kit) for a track.
    Instrument(usize),
    /// Colors (named, or typed as `#rrggbb` or `rgb(r,g,b)`) for a track.
    Color(usize),
    /// Common tunings of a track's instrument.
    Tuning(usize),
    /// The kinds of staff for a track's staff (its index; one past the last adds one).
    Staff(usize, usize),
}

impl PickerKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "buffer" | "buffers" => PickerKind::Buffer,
            "view" | "views" => PickerKind::View,
            "workspace" | "workspaces" => PickerKind::Workspace,
            "mark" | "marks" => PickerKind::Mark,
            "macro" | "macros" => PickerKind::Macro,
            "keymap" | "keymaps" | "keys" => PickerKind::Keymap,
            "section" | "sections" => PickerKind::Section,
            _ => return None,
        })
    }

    fn title(self) -> &'static str {
        match self {
            PickerKind::Buffer => "Buffers",
            PickerKind::View => "Views",
            PickerKind::Workspace => "Workspaces",
            PickerKind::Mark => "Marks",
            PickerKind::Macro => "Macros",
            PickerKind::Keymap => "Keymaps",
            PickerKind::Section => "Sections",
            PickerKind::Instrument(_) => "Instruments",
            PickerKind::Color(_) => "Colors",
            PickerKind::Tuning(_) => "Tunings",
            PickerKind::Staff(..) => "Staves",
        }
    }
}

/// An open picker: what it lists, the filter typed and the selected (filtered) entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Picker {
    pub kind: PickerKind,
    pub query: String,
    pub selected: usize,
    /// The mode to return to.
    pub(crate) back: Mode,
}

/// One entry: label (matched against the query), description and what choosing it runs.
struct Entry {
    label: String,
    description: String,
    action: Command,
    /// The description is searched too (keymaps).
    search_description: bool,
    /// A color shown next to the label.
    swatch: Option<Rgba>,
    /// An icon shown next to the label.
    icon: Option<Icon>,
    /// The choice now in force (the properties pane steps from it).
    current: bool,
}

/// Common colors: name and hex code.
const COLORS: &[(&str, &str)] = &[
    ("Red", "#e53935"),
    ("Crimson", "#c2185b"),
    ("Pink", "#ec407a"),
    ("Hot Pink", "#ff4081"),
    ("Magenta", "#d500f9"),
    ("Purple", "#8e24aa"),
    ("Violet", "#7e57c2"),
    ("Indigo", "#3949ab"),
    ("Blue", "#1e88e5"),
    ("Sky Blue", "#4fc3f7"),
    ("Cyan", "#00bcd4"),
    ("Teal", "#009688"),
    ("Turquoise", "#26c6da"),
    ("Mint", "#69f0ae"),
    ("Green", "#43a047"),
    ("Lime", "#c0ca33"),
    ("Olive", "#827717"),
    ("Yellow", "#fdd835"),
    ("Gold", "#ffb300"),
    ("Amber", "#ffa000"),
    ("Orange", "#fb8c00"),
    ("Coral", "#ff7043"),
    ("Salmon", "#ff8a80"),
    ("Brown", "#6d4c41"),
    ("Tan", "#bcaaa4"),
    ("Maroon", "#7f0000"),
    ("Navy", "#0d2a6b"),
    ("Slate", "#607d8b"),
    ("Silver", "#b0bec5"),
    ("Gray", "#9e9e9e"),
    ("Charcoal", "#37474f"),
    ("White", "#fafafa"),
    ("Black", "#212121"),
];

/// A color typed as `#rgb`, `#rrggbb`, or `rgb(r, g, b)`.
fn parse_color_query(q: &str) -> Option<Rgba> {
    let q = q.trim();
    if q.starts_with('#') {
        return Rgba::parse(q).filter(|c| c.a == 255);
    }
    let inner = q.to_ascii_lowercase();
    let inner = inner.strip_prefix("rgb(")?.strip_suffix(')')?;
    let n: Vec<u8> = inner
        .split(',')
        .map(|p| p.trim().parse().ok())
        .collect::<Option<_>>()?;
    match n.as_slice() {
        &[r, g, b] => Some(Rgba::rgb(r, g, b)),
        _ => None,
    }
}

/// A staff as the properties pane shows it: `treble`, `bass`, `grand`, `tab` …
pub(crate) fn staff_label(staff: muse_core::Staff) -> String {
    match staff {
        muse_core::Staff::Standard(c) => c.name(),
        other => other.name(),
    }
}

/// The name of a color of the picker's list.
pub(crate) fn color_name(c: Rgba) -> Option<&'static str> {
    COLORS
        .iter()
        .find(|(_, hex)| Rgba::parse(hex) == Some(c))
        .map(|(n, _)| *n)
}

/// The name of a tuning of the picker's lists (strings highest first).
pub(crate) fn tuning_name(strings: &[u8]) -> Option<&'static str> {
    tunings(tuning_family(strings), strings.len())
        .into_iter()
        .find(|(_, _, s)| s == strings)
        .map(|(n, _, _)| n)
}

/// The instrument family a track's tuning is picked from, by its strings.
#[derive(Clone, Copy, PartialEq)]
enum Family {
    Guitar,
    Bass,
    Ukulele,
    Other,
}

fn tuning_family(strings: &[u8]) -> Family {
    let top = strings.iter().copied().max().unwrap_or(0);
    match strings.len() {
        4 if top >= 60 => Family::Ukulele,
        n if top < 55 && (4..=6).contains(&n) => Family::Bass,
        6..=8 if top >= 55 => Family::Guitar,
        _ => Family::Other,
    }
}

/// Common tunings of a family: name, description and the open strings (highest first).
fn tunings(family: Family, strings: usize) -> Vec<(&'static str, &'static str, Vec<u8>)> {
    let t = |n, d, p: &[u8]| (n, d, p.to_vec());
    match (family, strings) {
        (Family::Guitar, 6) => vec![
            t(
                "Standard",
                "E A D G B E: the usual tuning",
                &[64, 59, 55, 50, 45, 40],
            ),
            t(
                "Half Step Down",
                "Eb tuning: Hendrix, Nirvana, easier bends",
                &[63, 58, 54, 49, 44, 39],
            ),
            t(
                "Whole Step Down",
                "D standard: darker, looser strings",
                &[62, 57, 53, 48, 43, 38],
            ),
            t(
                "Drop D",
                "Low string down a tone: power chords on one finger",
                &[64, 59, 55, 50, 45, 38],
            ),
            t(
                "Double Drop D",
                "Both E strings down to D: Neil Young",
                &[62, 59, 55, 50, 45, 38],
            ),
            t(
                "Drop C#",
                "Drop D, a half step lower",
                &[63, 58, 54, 49, 44, 37],
            ),
            t(
                "Drop C",
                "Drop D, a whole step lower: heavy rock and metal",
                &[62, 57, 53, 48, 43, 36],
            ),
            t(
                "C# Standard",
                "Every string down 3 half steps",
                &[61, 56, 52, 47, 42, 37],
            ),
            t(
                "C Standard",
                "Every string down 4 half steps",
                &[60, 55, 51, 46, 41, 36],
            ),
            t(
                "Baritone (B Standard)",
                "A fourth below standard",
                &[59, 54, 50, 45, 40, 35],
            ),
            t(
                "DADGAD",
                "Celtic and folk: suspended open chord",
                &[62, 57, 55, 50, 45, 38],
            ),
            t(
                "Open G",
                "D G D G B D: slide and blues, Keith Richards",
                &[62, 59, 55, 50, 43, 38],
            ),
            t(
                "Open D",
                "D A D F# A D: slide, folk, Joni Mitchell",
                &[62, 57, 54, 50, 45, 38],
            ),
            t(
                "Open E",
                "E B E G# B E: slide, Allman Brothers",
                &[64, 59, 56, 52, 47, 40],
            ),
            t(
                "Open A",
                "E A E A C# E: slide, Rolling Stones",
                &[64, 61, 57, 52, 45, 40],
            ),
            t(
                "Open C",
                "C G C G C E: strummed open chord, Led Zeppelin",
                &[64, 60, 55, 48, 43, 36],
            ),
            t(
                "All Fourths",
                "E A D G C F: even intervals, jazz",
                &[65, 60, 55, 50, 45, 40],
            ),
            t(
                "New Standard",
                "C G D A E G: Robert Fripp",
                &[67, 64, 57, 50, 43, 36],
            ),
        ],
        (Family::Guitar, 7) => vec![
            t(
                "Standard",
                "B E A D G B E: the usual 7-string",
                &[64, 59, 55, 50, 45, 40, 35],
            ),
            t("Half Step Down", "Bb to Eb", &[63, 58, 54, 49, 44, 39, 34]),
            t(
                "Drop A",
                "Low string down a tone: heavy metal",
                &[64, 59, 55, 50, 45, 40, 33],
            ),
        ],
        (Family::Guitar, 8) => vec![t(
            "Standard",
            "F# B E A D G B E: the usual 8-string",
            &[64, 59, 55, 50, 45, 40, 35, 30],
        )],
        (Family::Bass, 4) => vec![
            t("Standard", "E A D G: the usual tuning", &[43, 38, 33, 28]),
            t("Half Step Down", "Eb Ab Db Gb", &[42, 37, 32, 27]),
            t("D Standard", "Whole step down: D G C F", &[41, 36, 31, 26]),
            t("Drop D", "Low string down a tone", &[43, 38, 33, 26]),
            t("Drop C", "C G C F: heavy rock and metal", &[41, 36, 31, 24]),
        ],
        (Family::Bass, 5) => vec![
            t("Standard", "B E A D G: low B string", &[43, 38, 33, 28, 23]),
            t("Tenor", "E A D G C: high C string", &[48, 43, 38, 33, 28]),
            t(
                "Drop A",
                "A E A D G: low string down a tone",
                &[43, 38, 33, 28, 21],
            ),
        ],
        (Family::Bass, 6) => vec![t(
            "Standard",
            "B E A D G C: the usual 6-string",
            &[48, 43, 38, 33, 28, 23],
        )],
        (Family::Ukulele, _) => vec![
            t(
                "Standard (GCEA)",
                "Re-entrant: the high G string",
                &[69, 64, 60, 67],
            ),
            t("Low G", "Linear: the G an octave lower", &[69, 64, 60, 55]),
            t(
                "D Tuning (ADF#B)",
                "A whole step up from standard",
                &[71, 66, 62, 69],
            ),
            t(
                "Baritone (DGBE)",
                "Like the top four guitar strings",
                &[64, 59, 55, 50],
            ),
        ],
        _ => Vec::new(),
    }
}

/// Notes of a tuning from the lowest string up, as `:tuning` takes them.
pub(crate) fn tuning_notes(strings: &[u8]) -> String {
    strings
        .iter()
        .rev()
        .map(|&p| muse_core::pitch_name(p))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `~/…` for paths under the home folder.
fn tilde(path: &std::path::Path) -> String {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    match home.as_deref().and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

impl App {
    /// Opens a picker in the command palette's box.
    pub fn open_picker(&mut self, kind: PickerKind) {
        let back = Mode::Normal;
        self.picker = Some(Picker {
            kind,
            query: String::new(),
            selected: 0,
            back,
        });
        self.mode = Mode::Picker;
    }

    fn picker_entries(&self, kind: PickerKind) -> Vec<Entry> {
        match kind {
            PickerKind::Buffer => self
                .bufferline()
                .into_iter()
                .enumerate()
                .map(|(i, tab)| {
                    let b = &self.buffers[tab.buffer];
                    let mut description = b
                        .path
                        .as_deref()
                        .and_then(|p| p.parent())
                        .map(tilde)
                        .unwrap_or_else(|| "unsaved".into());
                    if tab.modified {
                        description = format!("● {description}");
                    }
                    if tab.active {
                        description = format!("{description}  (shown)");
                    }
                    Entry {
                        label: tab.name,
                        description,
                        action: Command::BufferGoto(i + 1),
                        search_description: false,
                        swatch: None,
                        icon: None,
                        current: false,
                    }
                })
                .collect(),
            PickerKind::View => self
                .view_labels()
                .into_iter()
                .zip(&self.views)
                .enumerate()
                .map(|(i, (label, v))| {
                    let names: Vec<&str> = v
                        .buffers
                        .iter()
                        .filter_map(|&b| self.buffers.get(b))
                        .map(|b| b.name.as_str())
                        .collect();
                    let panes = v.root.panes().len();
                    let mut description = format!(
                        "{} pane{} · {}",
                        panes,
                        if panes == 1 { "" } else { "s" },
                        names.join(", ")
                    );
                    if i == self.active_view {
                        description = format!("(current) {description}");
                    }
                    Entry {
                        label,
                        description,
                        action: Command::ViewGoto(i + 1),
                        search_description: false,
                        swatch: None,
                        icon: None,
                        current: false,
                    }
                })
                .collect(),
            PickerKind::Mark => self
                .mark_list()
                .into_iter()
                .map(|m| Entry {
                    label: m.name.to_string(),
                    description: format!(
                        "{} · bar {}, beat {}, string {}",
                        m.file,
                        m.cursor.bar + 1,
                        m.cursor.beat + 1,
                        m.cursor.string + 1
                    ),
                    action: Command::GotoMark(m.name),
                    search_description: false,
                    swatch: None,
                    icon: None,
                    current: false,
                })
                .collect(),
            PickerKind::Macro => {
                let mut regs: Vec<(&char, &Vec<crate::keys::Key>)> =
                    self.ed.macros.iter().collect();
                regs.sort();
                regs.into_iter()
                    .map(|(r, keys)| Entry {
                        label: format!("@{r}"),
                        description: crate::keys::format_keys(keys),
                        action: Command::RunMacro(*r),
                        search_description: false,
                        swatch: None,
                        icon: None,
                        current: false,
                    })
                    .collect()
            }
            PickerKind::Instrument(t) => {
                let n = t + 1;
                let current = self.buffer().score.tracks.get(t).map(|t| {
                    if t.kind == muse_core::TrackKind::Drums {
                        usize::MAX
                    } else {
                        t.midi.program as usize
                    }
                });
                let mark = |on: bool| if on { "current" } else { "" }.to_string();
                crate::tracks::GM_PROGRAMS
                    .iter()
                    .enumerate()
                    .map(|(p, name)| Entry {
                        label: format!("{:>3} {name}", p + 1),
                        description: mark(current == Some(p)),
                        icon: Some(Icon::of_program(p)),
                        action: Command::TrackInstrument(Some(format!("{} {n}", p + 1))),
                        search_description: false,
                        swatch: None,
                        current: current == Some(p),
                    })
                    .chain(std::iter::once(Entry {
                        label: "    Drum Kit".into(),
                        description: mark(current == Some(usize::MAX)),
                        icon: Some(Icon::Drums),
                        action: Command::TrackInstrument(Some(format!("drums {n}"))),
                        search_description: false,
                        swatch: None,
                        current: current == Some(usize::MAX),
                    }))
                    .collect()
            }
            PickerKind::Color(t) => {
                let now = self
                    .buffer()
                    .score
                    .tracks
                    .get(t)
                    .and_then(|t| t.display_color)
                    .map(|c| Rgba::rgb(c.r, c.g, c.b));
                let auto = Entry {
                    label: "Automatic".into(),
                    description: if now.is_none() {
                        "current"
                    } else {
                        "from the theme"
                    }
                    .into(),
                    action: Command::TrackColor(Some("off".into())),
                    search_description: false,
                    swatch: Some(self.track_color(self.pane().buffer, t)),
                    icon: None,
                    current: now.is_none(),
                };
                std::iter::once(auto)
                    .chain(COLORS.iter().filter_map(|&(name, hex)| {
                        let c = Rgba::parse(hex)?;
                        Some(Entry {
                            label: name.into(),
                            description: if now == Some(c) {
                                format!("{}  current", hex.to_uppercase())
                            } else {
                                hex.to_uppercase()
                            },
                            action: Command::TrackColor(Some(hex.into())),
                            search_description: true,
                            swatch: Some(c),
                            icon: None,
                            current: now == Some(c),
                        })
                    }))
                    .collect()
            }
            PickerKind::Tuning(t) => {
                let Some(track) = self.buffer().score.tracks.get(t) else {
                    return Vec::new();
                };
                let family = tuning_family(&track.strings);
                tunings(family, track.strings.len())
                    .into_iter()
                    .map(|(name, about, strings)| {
                        // The notes are listed apart: leave them out of the text.
                        let about = about
                            .split_once(": ")
                            .filter(|(notes, _)| notes.chars().all(|c| "ABCDEFG# ".contains(c)))
                            .map_or(about, |(_, rest)| rest);
                        let here = strings == track.strings;
                        Entry {
                            label: name.into(),
                            description: format!(
                                "{}  {about}{}",
                                tuning_notes(&strings),
                                if here { "  (current)" } else { "" }
                            ),
                            action: Command::Tuning(tuning_notes(&strings)),
                            search_description: true,
                            swatch: None,
                            icon: None,
                            current: here,
                        }
                    })
                    .collect()
            }
            PickerKind::Staff(t, i) => {
                let Some(track) = self.buffer().score.tracks.get(t) else {
                    return Vec::new();
                };
                let staves = track.staves();
                let here = staves.get(i).copied();
                let tab_ok = track.kind == muse_core::TrackKind::Fretted
                    || track.kind == muse_core::TrackKind::Pitched;
                let kinds = [
                    ("treble", "treble", "Standard notation, treble clef"),
                    ("bass", "bass", "Standard notation, bass clef"),
                    (
                        "grand",
                        "grand",
                        "Standard notation, treble and bass joined",
                    ),
                    (
                        "neutral",
                        "neutral",
                        "Standard notation, percussion clef (drum tracks)",
                    ),
                    ("tab", "tab", "Tablature"),
                    ("tab+", "tab+", "Tablature with rhythm"),
                    ("slash", "slash", "Rhythm slashes"),
                    ("chant", "chant", "Gregorian square notation, do clef"),
                    ("chant fa", "chant fa", "Gregorian square notation, fa clef"),
                ];
                let drums = track.kind == muse_core::TrackKind::Drums;
                let clef = self.config.default_clef();
                let mut out: Vec<Entry> = kinds
                    .into_iter()
                    .filter(|(k, ..)| {
                        (tab_ok || !k.starts_with("tab")) && (drums || *k != "neutral")
                    })
                    .map(|(kind, label, about)| {
                        let staff = muse_core::Staff::parse(kind, clef).ok();
                        // A grand staff counts whatever its split.
                        let same = match (staff, here) {
                            (
                                Some(muse_core::Staff::Standard(muse_core::Clef::Grand { .. })),
                                Some(muse_core::Staff::Standard(muse_core::Clef::Grand { .. })),
                            ) => true,
                            (a, b) => a.is_some() && a == b,
                        };
                        Entry {
                            label: label.into(),
                            description: if same {
                                format!("{about}  (current)")
                            } else {
                                about.into()
                            },
                            action: if here.is_some() {
                                Command::Notation(kind.into())
                            } else {
                                Command::StaffAdd(Some(kind.into()))
                            },
                            search_description: true,
                            swatch: None,
                            icon: staff.map(Icon::of_staff),
                            current: same,
                        }
                    })
                    .collect();
                if here.is_some() && staves.len() > 1 {
                    out.push(Entry {
                        label: "remove".into(),
                        description: "Remove this staff".into(),
                        action: Command::StaffRemove(None),
                        search_description: true,
                        swatch: None,
                        icon: None,
                        current: false,
                    });
                }
                out
            }
            PickerKind::Section => {
                let bars = &self.buffer().score.master_bars;
                let at = self.pane().cursor.bar;
                // The section the cursor is in.
                let current = (0..=at.min(bars.len().saturating_sub(1)))
                    .rev()
                    .find(|&b| bars[b].marker.is_some());
                bars.iter()
                    .enumerate()
                    .filter_map(|(b, m)| {
                        let title = m.marker.as_ref()?.title.trim().to_string();
                        // Its length: up to the next section (or the end).
                        let end = (b + 1..bars.len())
                            .find(|&n| bars[n].marker.is_some())
                            .unwrap_or(bars.len());
                        let mut description = format!(
                            "bar {}{}",
                            b + 1,
                            if end - b > 1 {
                                format!("–{}", end)
                            } else {
                                String::new()
                            }
                        );
                        if current == Some(b) {
                            description.push_str("  (here)");
                        }
                        Some(Entry {
                            label: if title.is_empty() {
                                format!("Section at bar {}", b + 1)
                            } else {
                                title
                            },
                            description,
                            action: Command::GotoBar(b + 1),
                            search_description: false,
                            swatch: None,
                            icon: None,
                            current: false,
                        })
                    })
                    .collect()
            }
            PickerKind::Keymap => {
                use crate::config::InsertKind;
                let c = &self.config;
                let maps = [
                    (c.keymap(Mode::Normal), "", true),
                    (c.normal_keymap(InsertKind::Tab), "(tab) ", true),
                    (c.normal_keymap(InsertKind::Standard), "(standard) ", true),
                    (c.normal_keymap(InsertKind::Slash), "(slash) ", true),
                    (c.normal_keymap(InsertKind::Chant), "(chant) ", true),
                    (c.keymap(Mode::Visual), "visual ", false),
                    (c.keymap(Mode::Insert), "insert ", false),
                    (c.insert_keymap(InsertKind::Tab), "insert (tab) ", false),
                    (
                        c.insert_keymap(InsertKind::Standard),
                        "insert (standard) ",
                        false,
                    ),
                    (c.insert_keymap(InsertKind::Slash), "insert (slash) ", false),
                    (c.insert_keymap(InsertKind::Chant), "insert (chant) ", false),
                    (c.keymap(Mode::Tracks), "tracks ", false),
                    (c.keymap(Mode::Properties), "properties ", false),
                    (c.keymap(Mode::Palette), "palette ", false),
                    (c.keymap(Mode::Command), "command ", false),
                ];
                let mut out = Vec::new();
                for (map, prefix, normal) in maps {
                    let mut keys: Vec<_> = map.bindings.iter().collect();
                    keys.sort_by_key(|(k, _)| crate::keys::format_keys(k));
                    for (k, cmds) in keys {
                        let names = map.names.get(k).cloned().unwrap_or_default();
                        let what: Vec<String> = names
                            .iter()
                            .map(|n| {
                                let head = n.split_whitespace().next().unwrap_or(n);
                                match crate::complete::describe(head) {
                                    Some(d) => format!("{n} — {d}"),
                                    None => n.clone(),
                                }
                            })
                            .collect();
                        // Choosing a Normal-mode binding runs it.
                        let action = match cmds.as_slice() {
                            [c] if normal => c.clone(),
                            _ => Command::Nop,
                        };
                        out.push(Entry {
                            label: format!("{prefix}{}", crate::keys::format_keys(k)),
                            description: what.join("; "),
                            action,
                            search_description: true,
                            swatch: None,
                            icon: None,
                            current: false,
                        });
                    }
                }
                out
            }
            PickerKind::Workspace => {
                let active = self.workspace_name();
                crate::workspace::workspace_names()
                    .into_iter()
                    .map(|name| {
                        let summary = crate::workspace::workspace_path(&name)
                            .and_then(|p| std::fs::read_to_string(p).ok())
                            .and_then(|t| toml::from_str::<crate::workspace::Workspace>(&t).ok())
                            .map(|w| {
                                format!(
                                    "{} view{} · {} file{}",
                                    w.views.len(),
                                    if w.views.len() == 1 { "" } else { "s" },
                                    w.files.len(),
                                    if w.files.len() == 1 { "" } else { "s" }
                                )
                            })
                            .unwrap_or_else(|| "unreadable".into());
                        let description = if active.as_deref() == Some(name.as_str()) {
                            format!("(active) {summary}")
                        } else {
                            summary
                        };
                        Entry {
                            action: Command::WorkspaceOpen(name.clone(), false),
                            label: name,
                            description,
                            search_description: false,
                            swatch: None,
                            icon: None,
                            current: false,
                        }
                    })
                    .collect()
            }
        }
    }

    /// Moves a choice (a color, tuning, instrument or staff kind) to the previous (`by` < 0) or
    /// next one, without wrapping around.
    pub fn step_choice(&mut self, kind: PickerKind, by: isize) {
        let entries: Vec<Entry> = self
            .picker_entries(kind)
            .into_iter()
            .filter(|e| e.label != "remove")
            .collect();
        let next = match entries.iter().position(|e| e.current) {
            Some(i) => i as isize + by.signum(),
            // Not one of the list (a custom value): forwards starts at the first.
            None if by > 0 => 0,
            None => return,
        };
        if let Some(e) = usize::try_from(next)
            .ok()
            .and_then(|n| entries.into_iter().nth(n))
        {
            self.run(e.action);
        }
    }

    /// Runs the first or the last choice of a picker's list.
    pub(crate) fn pick_choice(&mut self, kind: PickerKind, first: bool) {
        let mut entries: Vec<Entry> = self
            .picker_entries(kind)
            .into_iter()
            .filter(|e| e.label != "remove")
            .collect();
        let pick = if first {
            0
        } else {
            entries.len().saturating_sub(1)
        };
        if pick < entries.len() {
            self.run(entries.swap_remove(pick).action);
        }
    }

    /// The picker's entries filtered by its query, best first.
    fn picker_matches(&self, p: &Picker) -> Vec<(Entry, Vec<usize>)> {
        let mut entries = self.picker_entries(p.kind);
        // What is searched: the label, and for keymaps the description after it.
        let text = |e: &Entry| {
            if e.search_description {
                format!("{}  {}", e.label, e.description)
            } else {
                e.label.clone()
            }
        };
        let ranked = self.matcher().rank(entries.iter().map(text), &p.query);
        // Match ranked texts back to entries (texts can repeat: take each once). Highlights
        // fall on the label only.
        let custom = self.custom_entry(p);
        custom
            .map(|e| (e, Vec::new()))
            .into_iter()
            .chain(ranked.into_iter().filter_map(|(t, pos)| {
                let i = entries.iter().position(|e| text(e) == t)?;
                let e = entries.remove(i);
                let n = e.label.chars().count();
                let pos = pos.into_iter().filter(|&p| p < n).collect();
                Some((e, pos))
            }))
            .collect()
    }

    /// What the typed query itself stands for, listed first: a color as `#rrggbb` or
    /// `rgb(r,g,b)`, or a tuning as note names for each string.
    fn custom_entry(&self, p: &Picker) -> Option<Entry> {
        match p.kind {
            PickerKind::Color(_) => {
                let c = parse_color_query(&p.query)?;
                Some(Entry {
                    label: "Custom".into(),
                    description: c.hex(),
                    action: Command::TrackColor(Some(c.hex())),
                    search_description: false,
                    swatch: Some(c),
                    icon: None,
                    current: false,
                })
            }
            PickerKind::Tuning(t) => {
                let track = self.buffer().score.tracks.get(t)?;
                let notes: Vec<&str> = p.query.split_whitespace().collect();
                let named = notes
                    .iter()
                    .all(|n| n.starts_with(|c: char| c.is_ascii_alphabetic()));
                let pitches: Option<Vec<u8>> =
                    notes.iter().map(|n| muse_core::parse_pitch(n)).collect();
                let pitches = pitches.filter(|p| named && p.len() == track.strings.len())?;
                let strings: Vec<u8> = pitches.into_iter().rev().collect();
                Some(Entry {
                    label: "Custom".into(),
                    description: tuning_notes(&strings),
                    action: Command::Tuning(tuning_notes(&strings)),
                    search_description: false,
                    swatch: None,
                    icon: None,
                    current: false,
                })
            }
            _ => None,
        }
    }

    /// The picker as the palette shows it.
    pub(crate) fn picker_palette(&self) -> Option<Palette> {
        let p = self.picker.as_ref()?;
        let matches = self.picker_matches(p);
        let selected = (!matches.is_empty()).then(|| p.selected.min(matches.len() - 1));
        Some(Palette {
            prompt: format!("{} ›", p.kind.title().to_lowercase()),
            title: p.kind.title().into(),
            input: p.query.clone(),
            items: matches
                .into_iter()
                .map(|(e, matched)| PaletteItem {
                    text: e.label,
                    description: Some(e.description),
                    swatch: e.swatch,
                    icon: e.icon,
                    matched,
                })
                .collect(),
            selected,
        })
    }

    /// The wheel over an open picker: moves the selection by `by` entries.
    pub fn picker_scroll(&mut self, by: isize) {
        let Some(p) = self.picker.clone() else {
            return;
        };
        let n = self.picker_matches(&p).len();
        if let Some(q) = self.picker.as_mut() {
            q.selected = (q.selected as isize + by).clamp(0, n.saturating_sub(1) as isize) as usize;
        }
    }

    fn close_picker(&mut self) -> Option<Picker> {
        let p = self.picker.take()?;
        self.mode = p.back;
        Some(p)
    }

    pub(crate) fn handle_picker_key(&mut self, key: Key) {
        let Some(p) = self.picker.as_mut() else {
            self.mode = Mode::Normal;
            return;
        };
        match key.code {
            KeyCode::Esc => {
                self.close_picker();
            }
            KeyCode::Char('c') if key.ctrl => {
                self.close_picker();
            }
            KeyCode::Enter => {
                let p = p.clone();
                let matches = self.picker_matches(&p);
                self.close_picker();
                match matches.into_iter().nth(p.selected) {
                    Some((entry, _)) if entry.action == Command::Nop => {
                        self.info(format!("{}: {}", entry.label, entry.description))
                    }
                    Some((entry, _)) => self.run(entry.action),
                    None => self.error(format!(
                        "no {} matches '{}'",
                        p.kind.title().to_lowercase(),
                        p.query
                    )),
                }
            }
            KeyCode::Down | KeyCode::Tab => p.selected += 1,
            KeyCode::Char('j') if key.alt => p.selected += 1,
            KeyCode::Char('k') if key.alt => p.selected = p.selected.saturating_sub(1),
            KeyCode::Char('n') if key.ctrl => p.selected += 1,
            KeyCode::Up | KeyCode::BackTab => p.selected = p.selected.saturating_sub(1),
            KeyCode::Char('p') if key.ctrl => p.selected = p.selected.saturating_sub(1),
            KeyCode::Char('u') if key.ctrl => {
                p.query.clear();
                p.selected = 0;
            }
            KeyCode::Backspace => {
                p.query.pop();
                p.selected = 0;
            }
            KeyCode::Char(c) if !key.ctrl && !key.alt && !key.sup => {
                p.query.push(c);
                p.selected = 0;
            }
            _ => {}
        }
        // Keep the selection on an entry.
        if let Some(p) = self.picker.clone() {
            let n = self.picker_matches(&p).len();
            if let Some(q) = self.picker.as_mut() {
                q.selected = q.selected.min(n.saturating_sub(1));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use crate::keys::parse_keys;

    fn keys(app: &mut App, s: &str) {
        for k in parse_keys(s, &[Key::char(' ')]).unwrap() {
            app.handle_key(k);
        }
    }

    #[test]
    fn buffer_and_view_pickers_filter_and_switch() {
        let dir = std::env::temp_dir().join(format!("muse-picker-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        let names = ["Wonderwall.muse", "Hurt.muse", "Yesterday.muse"];
        for n in names {
            app.run(Command::Write(Some(dir.join(n))));
        }
        let mut app = App::new(Config::default(), None);
        for n in names {
            app.run(Command::Edit(dir.join(n)));
        }
        // Buffers: fuzzy filter, Enter switches.
        keys(&mut app, "<Space>b");
        assert_eq!(app.mode, Mode::Picker);
        let p = app.palette().unwrap();
        assert_eq!((p.title.as_str(), p.items.len()), ("Buffers", 3));
        keys(&mut app, "wndw");
        let p = app.palette().unwrap();
        assert_eq!(p.items[0].text, "Wonderwall.muse");
        keys(&mut app, "<CR>");
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.buffer().name, "Wonderwall.muse");
        // Up/Down move; Esc cancels without switching.
        keys(&mut app, "<Space>b<Down><Down><Esc>");
        assert_eq!(app.buffer().name, "Wonderwall.muse");
        keys(&mut app, "<Space>b<Down><CR>");
        assert_eq!(app.buffer().name, "Hurt.muse");
        // Views.
        app.run(Command::ViewNew(None));
        app.run(Command::ViewRename(Some("practice".into())));
        keys(&mut app, "<Space>V1<CR>");
        assert_eq!(app.active_view, 0);
        keys(&mut app, "<Space>Vpra<CR>");
        assert_eq!(app.active_view, 1);
        // No match: nothing happens but a message.
        keys(&mut app, "<Space>bzzz<CR>");
        assert!(app.message.as_ref().unwrap().error);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_keymap_picker_finds_bindings_by_keys_or_description() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        keys(&mut app, "<Space>?");
        assert_eq!(app.palette().unwrap().title, "Keymaps");
        // By description…
        keys(&mut app, "file picker");
        let p = app.palette().unwrap();
        assert_eq!(p.items[0].text, "<C-o>");
        // …or by keys; Enter runs a Normal-mode binding.
        keys(&mut app, "<C-u>}");
        let p = app.palette().unwrap();
        assert_eq!(p.items[0].text, "}");
        assert!(
            p.items[0]
                .description
                .as_deref()
                .unwrap()
                .contains("bar-next")
        );
        keys(&mut app, "<CR>");
        assert_eq!(app.pane().cursor.bar, 1);
        // Other modes' bindings are listed with the mode.
        keys(&mut app, "<Space>?tracks s");
        assert_eq!(app.palette().unwrap().items[0].text, "tracks s");
    }

    #[test]
    fn sections_by_motion_and_by_picker() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.run(Command::Section(Some("Intro".into())));
        keys(&mut app, "}}");
        app.run(Command::Section(Some("Verse".into())));
        keys(&mut app, "}}");
        app.run(Command::Section(Some("Chorus".into())));
        keys(&mut app, "gg");
        // ]s / [s, with a count; '' goes back.
        keys(&mut app, "]s");
        assert_eq!(app.pane().cursor.bar, 2);
        keys(&mut app, "]s");
        assert_eq!(app.pane().cursor.bar, 4);
        keys(&mut app, "]s");
        assert_eq!(app.pane().cursor.bar, 4);
        keys(&mut app, "2[s");
        assert_eq!(app.pane().cursor.bar, 0);
        keys(&mut app, "''");
        assert_eq!(app.pane().cursor.bar, 4);
        // The picker lists them, with their bars; choosing one jumps there.
        keys(&mut app, "<Space>S");
        let p = app.palette().unwrap();
        assert_eq!(p.title, "Sections");
        let names: Vec<&str> = p.items.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(names, ["Intro", "Verse", "Chorus"]);
        assert_eq!(p.items[1].description.as_deref(), Some("bar 3–4"));
        assert!(
            p.items[2]
                .description
                .as_deref()
                .unwrap()
                .ends_with("(here)")
        );
        keys(&mut app, "vers<CR>");
        assert_eq!(app.pane().cursor.bar, 2);
    }

    #[test]
    fn uppercase_registers_append_and_the_macro_picker_runs_one() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        keys(&mut app, "qa}q");
        keys(&mut app, "qA}q");
        assert_eq!(crate::keys::format_keys(&app.ed.macros[&'a']), "}}");
        keys(&mut app, "gg");
        app.run(Command::Pick(PickerKind::Macro));
        let p = app.palette().unwrap();
        assert_eq!(p.items[0].text, "@a");
        assert_eq!(p.items[0].description.as_deref(), Some("}}"));
        keys(&mut app, "<CR>");
        assert_eq!(app.pane().cursor.bar, 2);
    }
}
