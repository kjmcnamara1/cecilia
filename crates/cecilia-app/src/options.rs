//! The option registry: every setting of `config.toml` (except keymaps) by its dotted TOML path,
//! with a type, a one-line doc and typed get/set shared by `:set`, `:get` and `-o`.

use std::sync::LazyLock;

use serde::Deserialize;
use toml::{Table, Value};

use crate::config::ConfigFile;
use crate::theme::{COLOR_NAMES, PRESETS, Rgba};

/// What an option holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Bool,
    Int {
        min: i64,
        max: i64,
    },
    Float {
        min: f64,
        max: f64,
    },
    Str,
    Choice(&'static [&'static str]),
    /// "" (unset) or `#rgb` / `#rrggbb` / `#rrggbbaa`.
    Color,
    List(&'static ItemKind),
}

/// What the items of a list option are.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ItemKind {
    Str,
    Choice(&'static [&'static str]),
    Color,
}

#[derive(Debug, Clone, Copy)]
pub struct OptionSpec {
    pub name: &'static str,
    pub kind: Kind,
    pub doc: &'static str,
}

const LAYOUTS: &[&str] = &["page", "vertical", "horizontal"];
const TICKS: &[&str] = &["all", "major", "off"];
const TOOLS: &[&str] = &["tracks", "waveform", "fretboard", "piano", "tuner"];

const fn o(name: &'static str, kind: Kind, doc: &'static str) -> OptionSpec {
    OptionSpec { name, kind, doc }
}

const fn int(min: i64, max: i64) -> Kind {
    Kind::Int { min, max }
}

const fn float(min: f64, max: f64) -> Kind {
    Kind::Float { min, max }
}

/// All options, in the order of `config.toml`. The theme colors come from
/// [`COLOR_NAMES`], so the list is built on first use.
pub static OPTIONS: LazyLock<Vec<OptionSpec>> = LazyLock::new(|| {
    use Kind::*;
    let mut v = vec![
        o(
            "general.leader",
            Str,
            "The key `<leader>` stands for in keymaps.",
        ),
        o(
            "ui.tab_width",
            int(1, 8),
            "TUI: characters per sixteenth note in the tab view.",
        ),
        o(
            "ui.follow_playback",
            Bool,
            "Scroll to keep the playhead in view while playing.",
        ),
        o(
            "ui.jk_moves_between_staves",
            Bool,
            "j/k go on from the last row of a staff into the next staff.",
        ),
        o(
            "ui.lyrics_select_cursor",
            Bool,
            "Going to a chant score's lyrics area puts the caret on the cursor's note.",
        ),
        o(
            "ui.completion_ignore_case",
            Bool,
            "Command-line Tab completion ignores letter case.",
        ),
        o(
            "ui.completion_fuzzy",
            Bool,
            "Command-line completion matches the typed letters in order anywhere.",
        ),
        o(
            "ui.picker_position",
            int(0, 100),
            "How far down the window (percent) the command palette and pickers sit; lists grow upward.",
        ),
        o(
            "ui.progress_bar",
            Bool,
            "A progress bar along the bottom of every score pane.",
        ),
        o("ui.scrollbar", Bool, "A scrollbar on each score pane."),
        o(
            "ui.tracks_panel",
            Bool,
            "Start with the tracks panel (tracks with mixer and bar overview) open.",
        ),
        o(
            "ui.properties",
            Bool,
            "Start with the properties pane (score, tracks and lyrics) open.",
        ),
        o("ui.palette", Bool, "Start with the editing palette open."),
        o(
            "ui.palette_wrap",
            Bool,
            "The palette's keyboard motions wrap around at its edges.",
        ),
        o(
            "ui.global_tools",
            List(&ItemKind::Choice(TOOLS)),
            "Tool panes that dock at the full width of the view and follow the last score pane.",
        ),
        o(
            "ui.tracks_peek",
            float(0.0, 60.0),
            "Seconds the tracks show after a track change while no tracks pane is shown (0: off).",
        ),
        o(
            "ui.auto_resize",
            Bool,
            "Tracks panes follow the height their score needs, until resized by hand.",
        ),
        o(
            "ui.equalize_on_close",
            Bool,
            "Closing a pane gives the remaining panes of its split equal weights.",
        ),
        o(
            "ui.keep_files_in_new_view",
            Bool,
            "A new view starts with the current files (off: the dashboard).",
        ),
        o(
            "ui.workspace_autosave_on_quit",
            Bool,
            "Save the active workspace when quitting; :quit! never does.",
        ),
        o(
            "ui.workspace_restore_last",
            Bool,
            "Starting without arguments loads the last workspace.",
        ),
        o(
            "ui.toolbar",
            Bool,
            "Start with the toolbar of common commands at the top of the window.",
        ),
        o("ui.fretboard", Bool, "Start with the fretboard panel open."),
        o("ui.piano", Bool, "Start with the piano panel open."),
        o(
            "ui.hide_cursor_while_following",
            Bool,
            "While playing with follow on, hide the edit cursor.",
        ),
        o(
            "ui.layout",
            Choice(LAYOUTS),
            "Default GUI score layout for new panes.",
        ),
        o(
            "ui.zoom",
            float(0.1, 10.0),
            "Default GUI zoom factor at startup.",
        ),
        o(
            "ui.chant_chords",
            Choice(&["top", "bottom", "error"]),
            "Which note of a chord :modern-to-chant keeps.",
        ),
        o(
            "ui.bar_numbers",
            Choice(&["system", "every"]),
            "Bar numbers on the score: the first bar of each line, or every bar.",
        ),
        o(
            "ui.highlight_incomplete_bars",
            Bool,
            "Draw the staff lines of bars that don't hold their full count in red.",
        ),
        o(
            "ui.focus_ring_width",
            float(0.0, 8.0),
            "Thickness in pixels of the ring around the focused pane in the GUI (0: none).",
        ),
        o(
            "ui.focus_ring_color",
            Color,
            "Color of the focus ring; empty: the current mode's color.",
        ),
        o(
            "ui.lyrics_font",
            Str,
            "GUI: the font family of the lyrics text area in the properties pane.",
        ),
        o(
            "ui.lyrics_font_size",
            float(4.0, 96.0),
            "GUI: the lyrics text area's font size in points.",
        ),
        o(
            "ui.chant_text_font",
            Choice(&["noto-serif", "source-sans"]),
            "The font of the lyrics of a chant score.",
        ),
        o(
            "ui.waveform_time_ticks",
            Choice(TICKS),
            "GUI: which ticks of the waveform's time ruler are drawn.",
        ),
        o(
            "ui.waveform_beat_ticks",
            Choice(TICKS),
            "GUI: which ticks of the waveform's score ruler are drawn.",
        ),
        o(
            "ui.waveform_time_labels",
            Bool,
            "GUI: label the major ticks of the time ruler.",
        ),
        o(
            "ui.waveform_beat_labels",
            Bool,
            "GUI: label the ticks of the score ruler.",
        ),
        o(
            "ui.sync_snap",
            float(0.0, 200.0),
            "Sync mode: milliseconds a sync point may move to the strongest nearby onset (0: none).",
        ),
        o(
            "tabline.new_view",
            Choice(&["copy", "current", "empty"]),
            "What a new view starts with: the current buffers, the current buffer or nothing.",
        ),
        o(
            "tabline.icons",
            Choice(&["nerd", "unicode", "none"]),
            "File-type icons in the terminal UI.",
        ),
        o(
            "tabline.hide_single_view",
            Bool,
            "Hide the views on the right of the tabline while there is only one.",
        ),
        o(
            "tabline.wrap",
            Bool,
            "Buffer next/prev wrap around at the ends of the tabline.",
        ),
        o(
            "tabline.truncation.max_width",
            int(1, 200),
            "Longest file name (characters) a tab shows before truncating.",
        ),
        o(
            "tabline.truncation.prefix_width",
            int(0, 200),
            "Characters kept from the start of a truncated tab name.",
        ),
        o(
            "tabline.truncation.suffix_width",
            int(0, 200),
            "Characters kept from the end of a truncated tab name.",
        ),
        o(
            "audio.soundfont",
            Str,
            "SoundFont (.sf2) for MIDI playback; empty: the built-in synth.",
        ),
        o(
            "audio.speed",
            float(0.1, 4.0),
            "Playback speed multiplier (1.0 = written tempo).",
        ),
        o(
            "audio.tuner_a4",
            float(380.0, 500.0),
            "The tuner's reference pitch for A4, in Hz.",
        ),
        o(
            "swap.enabled",
            Bool,
            "Write swap files (recovery copies of unsaved buffers).",
        ),
        o(
            "swap.interval",
            float(0.1, 3600.0),
            "Seconds after the last change before the swap file is written.",
        ),
        o(
            "swap.auto_recover",
            Bool,
            "Opening a file with a newer swap file restores it as unsaved changes.",
        ),
        o(
            "swap.dir",
            Str,
            "Where swap files go; empty: the XDG state directory.",
        ),
        o("theme.preset", Choice(PRESETS), "The color preset."),
        o(
            "theme.file",
            Str,
            "A theme file with the same keys, applied under the colors set here.",
        ),
        o(
            "theme.track_colors",
            List(&ItemKind::Color),
            "The colors of tracks 1, 2, ...; empty: the built-in palette.",
        ),
        o(
            "theme.voice_colors",
            List(&ItemKind::Color),
            "The colors of voices 1, 2, ...; empty: the built-in palette.",
        ),
    ];
    v.extend(COLOR_NAMES.iter().map(|n| {
        let name: &'static str = Box::leak(format!("theme.{n}").into_boxed_str());
        o(name, Color, "A theme color; empty: the preset's color.")
    }));
    v.extend([
        o(
            "toolbar.items",
            List(&ItemKind::Str),
            "The toolbar's buttons: command lines in order, \"|\" between groups.",
        ),
        o(
            "statusline.separators",
            Choice(&["arrow", "round", "none"]),
            "TUI: how the status line separates its sections.",
        ),
        o(
            "score.clef",
            Choice(&["treble", "bass", "grand"]),
            "Clef of a standard staff added without one.",
        ),
    ]);
    v
});

/// The option called `name`, or an error listing near matches.
pub fn spec(name: &str) -> Result<&'static OptionSpec, String> {
    if name == "keymap" || name.starts_with("keymap.") {
        return Err(format!(
            "{name}: keymaps are not options (edit [keymap.*] in config.toml)"
        ));
    }
    if let Some(s) = OPTIONS.iter().find(|s| s.name == name) {
        return Ok(s);
    }
    let mut near: Vec<(usize, &str)> = OPTIONS
        .iter()
        .map(|s| (distance(name, s.name), s.name))
        .filter(|(d, s)| *d <= 3 || s.contains(name) || name.contains(s))
        .collect();
    near.sort();
    let near: Vec<&str> = near.into_iter().take(4).map(|(_, s)| s).collect();
    Err(if near.is_empty() {
        format!("unknown option '{name}'")
    } else {
        format!(
            "unknown option '{name}' (did you mean {}?)",
            near.join(", ")
        )
    })
}

fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut prev = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let cur = row[j];
            row[j] = (prev + usize::from(a[i - 1] != b[j - 1]))
                .min(row[j] + 1)
                .min(row[j - 1] + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

fn to_table(file: &ConfigFile) -> Result<Table, String> {
    Table::try_from(file).map_err(|e| e.to_string())
}

fn lookup<'a>(t: &'a Table, path: &str) -> Option<&'a Value> {
    let mut parts = path.split('.');
    let mut v = t.get(parts.next()?)?;
    for p in parts {
        v = v.as_table()?.get(p)?;
    }
    Some(v)
}

fn store(t: &mut Table, path: &str, value: Option<Value>) {
    let (parents, leaf) = path.rsplit_once('.').unwrap_or(("", path));
    let mut cur = t;
    for p in parents.split('.').filter(|p| !p.is_empty()) {
        let entry = cur
            .entry(p.to_string())
            .or_insert_with(|| Value::Table(Table::new()));
        let Some(next) = entry.as_table_mut() else {
            return;
        };
        cur = next;
    }
    match value {
        Some(v) => {
            cur.insert(leaf.to_string(), v);
        }
        None => {
            cur.remove(leaf);
        }
    }
}

fn show(v: Option<&Value>) -> String {
    match v {
        None => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => a
            .iter()
            .map(|i| show(Some(i)))
            .collect::<Vec<_>>()
            .join(","),
        Some(Value::Float(f)) => f.to_string(),
        Some(Value::Integer(i)) => i.to_string(),
        Some(Value::Boolean(b)) => b.to_string(),
        Some(other) => other.to_string(),
    }
}

/// The current value of `name`, as `set` accepts it (lists comma-joined, unset: empty).
pub fn get(file: &ConfigFile, name: &str) -> Result<String, String> {
    let s = spec(name)?;
    Ok(show(lookup(&to_table(file)?, s.name)))
}

fn describe(kind: &Kind) -> String {
    match kind {
        Kind::Bool => "a boolean (true, false, toggle)".into(),
        Kind::Int { min, max } => format!("an integer ({min} to {max})"),
        Kind::Float { min, max } => format!("a number ({min} to {max})"),
        Kind::Str => "a string".into(),
        Kind::Choice(c) => format!("one of ({})", c.join(", ")),
        Kind::Color => "a color (#rgb, #rrggbb, #rrggbbaa, or empty)".into(),
        Kind::List(item) => format!("a comma-separated list of {}", describe_item(item)),
    }
}

fn describe_item(item: &ItemKind) -> String {
    match item {
        ItemKind::Str => "strings; +item adds, -item removes".into(),
        ItemKind::Choice(c) => format!("({}); +item adds, -item removes", c.join(", ")),
        ItemKind::Color => "colors (#rrggbb); +item adds, -item removes".into(),
    }
}

fn bad(name: &str, kind: &Kind, raw: &str) -> String {
    format!("{name}: expected {}, got '{raw}'", describe(kind))
}

fn item_ok(item: &ItemKind, v: &str) -> bool {
    match item {
        ItemKind::Str => !v.is_empty(),
        ItemKind::Choice(c) => c.contains(&v),
        ItemKind::Color => Rgba::parse(v).is_some(),
    }
}

fn color_ok(v: &str) -> bool {
    v.is_empty() || Rgba::parse(v).is_some()
}

/// The value `raw` stands for as an option of `s` whose current value is `cur`.
fn parse(s: &OptionSpec, cur: Option<&Value>, raw: &str) -> Result<Value, String> {
    let fail = || bad(s.name, &s.kind, raw);
    let relative = raw.starts_with(['+', '-']);
    match &s.kind {
        Kind::Bool => match raw {
            "true" => Ok(Value::Boolean(true)),
            "false" => Ok(Value::Boolean(false)),
            "toggle" => Ok(Value::Boolean(
                !cur.and_then(Value::as_bool).unwrap_or(false),
            )),
            _ => Err(fail()),
        },
        Kind::Int { min, max } => {
            let n: i64 = raw.trim_start_matches('+').parse().map_err(|_| fail())?;
            let n = if relative {
                cur.and_then(Value::as_integer)
                    .unwrap_or(0)
                    .saturating_add(n)
            } else {
                n
            };
            if (*min..=*max).contains(&n) {
                Ok(Value::Integer(n))
            } else {
                Err(format!("{} (that makes {n})", fail()))
            }
        }
        Kind::Float { min, max } => {
            let n: f64 = raw.trim_start_matches('+').parse().map_err(|_| fail())?;
            let n = if relative {
                cur.and_then(Value::as_float).unwrap_or(0.0) + n
            } else {
                n
            };
            if n.is_finite() && (*min..=*max).contains(&n) {
                Ok(Value::Float(n))
            } else {
                Err(format!("{} (that makes {n})", fail()))
            }
        }
        Kind::Str => Ok(Value::String(raw.into())),
        Kind::Choice(c) => {
            if c.contains(&raw) {
                Ok(Value::String(raw.into()))
            } else {
                Err(fail())
            }
        }
        Kind::Color => {
            if color_ok(raw) {
                Ok(Value::String(raw.into()))
            } else {
                Err(fail())
            }
        }
        Kind::List(item) => {
            let mut list: Vec<String> = if relative {
                cur.and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            for part in raw.split(',').filter(|p| !p.is_empty()) {
                let (add, v) = match part.as_bytes()[0] {
                    b'-' if relative => (false, &part[1..]),
                    b'+' if relative => (true, &part[1..]),
                    _ => (true, part),
                };
                if !item_ok(item, v) {
                    return Err(format!(
                        "{}: '{v}' is not valid; expected {}",
                        s.name,
                        describe_item(item)
                    ));
                }
                if add {
                    if !list.iter().any(|i| i == v) {
                        list.push(v.into());
                    }
                } else {
                    list.retain(|i| i != v);
                }
            }
            Ok(Value::Array(list.into_iter().map(Value::String).collect()))
        }
    }
}

fn is_empty(v: &Value) -> bool {
    match v {
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        _ => false,
    }
}

fn rebuild(file: &mut ConfigFile, table: Table) -> Result<(), String> {
    *file = ConfigFile::deserialize(table).map_err(|e| e.to_string())?;
    Ok(())
}

/// Sets `name` to `raw` (`None` or `default`: back to the default) and returns the new value as
/// [`get`] formats it. On an error nothing changes.
pub fn set(file: &mut ConfigFile, name: &str, raw: Option<&str>) -> Result<String, String> {
    let s = spec(name)?;
    let mut table = to_table(file)?;
    let defaults = to_table(&ConfigFile::default())?;
    let default = lookup(&defaults, s.name).cloned();
    let value = match raw {
        None | Some("default") => default.clone(),
        Some(raw) => Some(parse(s, lookup(&table, s.name), raw)?),
    };
    // Options without a default value (theme colors, optional paths) are unset by removing them.
    let value = value.filter(|v| default.is_some() || !is_empty(v));
    store(&mut table, s.name, value);
    let mut next = file.clone();
    rebuild(&mut next, table)?;
    *file = next;
    get(file, name)
}

/// Checks every option's current value against its spec: numbers are clamped, other invalid
/// values reset to the default. One warning per fix.
pub fn validate(file: &mut ConfigFile) -> Vec<String> {
    let (Ok(mut table), Ok(defaults)) = (to_table(file), to_table(&ConfigFile::default())) else {
        return vec!["options: could not check the settings (left unchanged)".into()];
    };
    let mut warnings = Vec::new();
    for s in OPTIONS.iter() {
        let Some(cur) = lookup(&table, s.name).cloned() else {
            continue;
        };
        let default = lookup(&defaults, s.name).cloned();
        let fixed = match (&s.kind, &cur) {
            (Kind::Int { min, max }, Value::Integer(n)) if !(*min..=*max).contains(n) => {
                warnings.push(format!(
                    "{}: {n} is outside {min} to {max} (clamped)",
                    s.name
                ));
                Some(Value::Integer((*n).clamp(*min, *max)))
            }
            (Kind::Float { min, max }, Value::Float(n)) if !(*min..=*max).contains(n) => {
                if n.is_nan() {
                    warnings.push(format!("{}: not a number (using the default)", s.name));
                    default
                } else {
                    warnings.push(format!(
                        "{}: {n} is outside {min} to {max} (clamped)",
                        s.name
                    ));
                    Some(Value::Float(n.clamp(*min, *max)))
                }
            }
            (Kind::Choice(c), Value::String(v)) if !c.contains(&v.as_str()) => {
                warnings.push(format!(
                    "{}: '{v}' is not one of {} (using the default)",
                    s.name,
                    c.join(", ")
                ));
                default
            }
            (Kind::Color, Value::String(v)) if !color_ok(v) => {
                warnings.push(format!(
                    "{}: '{v}' is not a color (#rrggbb; using the default)",
                    s.name
                ));
                default
            }
            (Kind::List(item), Value::Array(a)) => {
                let keep: Vec<Value> = a
                    .iter()
                    .filter(|v| match v.as_str() {
                        Some(v) if item_ok(item, v) => true,
                        _ => {
                            warnings.push(format!(
                                "{}: '{}' is not valid; expected {} (dropped)",
                                s.name,
                                show(Some(v)),
                                describe_item(item)
                            ));
                            false
                        }
                    })
                    .cloned()
                    .collect();
                (keep.len() != a.len()).then_some(Value::Array(keep))
            }
            _ => None,
        };
        if let Some(v) = fixed {
            store(&mut table, s.name, Some(v));
        }
    }
    if let Err(e) = rebuild(file, table) {
        warnings.push(format!("options: could not apply the fixes: {e}"));
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f() -> ConfigFile {
        ConfigFile::default()
    }

    #[test]
    fn bool_toggle_and_explicit() {
        let mut c = f();
        assert_eq!(
            set(&mut c, "tabline.wrap", Some("toggle")).unwrap(),
            "false"
        );
        assert!(!c.tabline.wrap);
        assert_eq!(set(&mut c, "tabline.wrap", Some("toggle")).unwrap(), "true");
        assert_eq!(set(&mut c, "tabline.wrap", Some("false")).unwrap(), "false");
        assert!(
            set(&mut c, "tabline.wrap", Some("maybe"))
                .unwrap_err()
                .contains("boolean")
        );
        assert!(!c.tabline.wrap);
    }

    #[test]
    fn int_absolute_relative_and_range_error() {
        let mut c = f();
        assert_eq!(set(&mut c, "ui.tab_width", Some("4")).unwrap(), "4");
        assert_eq!(set(&mut c, "ui.tab_width", Some("+1")).unwrap(), "5");
        assert_eq!(set(&mut c, "ui.tab_width", Some("-1")).unwrap(), "4");
        assert!(
            set(&mut c, "ui.tab_width", Some("+100"))
                .unwrap_err()
                .contains("1 to 8")
        );
        assert_eq!(c.ui.tab_width, 4);
        for bad in ["abc", "+1.5"] {
            assert!(
                set(&mut c, "ui.tab_width", Some(bad))
                    .unwrap_err()
                    .contains("integer")
            );
        }
        assert_eq!(c.ui.tab_width, 4);
    }

    #[test]
    fn float_relative() {
        let mut c = f();
        assert_eq!(
            set(&mut c, "audio.tuner_a4", Some("+1.5")).unwrap(),
            "441.5"
        );
        assert_eq!(set(&mut c, "audio.tuner_a4", Some("-1.5")).unwrap(), "440");
        assert_eq!(set(&mut c, "audio.tuner_a4", Some("432")).unwrap(), "432");
        assert!(set(&mut c, "audio.tuner_a4", Some("x")).is_err());
    }

    #[test]
    fn choice_error_lists_values() {
        let mut c = f();
        let e = set(&mut c, "ui.bar_numbers", Some("foo")).unwrap_err();
        assert!(e.contains("system, every"), "{e}");
        assert_eq!(
            set(&mut c, "ui.bar_numbers", Some("every")).unwrap(),
            "every"
        );
        assert_eq!(
            set(&mut c, "ui.waveform_time_ticks", Some("major")).unwrap(),
            "major"
        );
    }

    #[test]
    fn list_add_remove_dedup() {
        let mut c = f();
        set(&mut c, "ui.global_tools", Some("tracks")).unwrap();
        assert_eq!(get(&c, "ui.global_tools").unwrap(), "tracks");
        set(&mut c, "ui.global_tools", Some("+tuner")).unwrap();
        let v = set(&mut c, "ui.global_tools", Some("+tuner")).unwrap();
        assert_eq!(v, "tracks,tuner");
        assert_eq!(
            set(&mut c, "ui.global_tools", Some("-tracks")).unwrap(),
            "tuner"
        );
        let e = set(&mut c, "ui.global_tools", Some("+bogus")).unwrap_err();
        assert!(e.contains("tracks, waveform"), "{e}");
        assert_eq!(get(&c, "ui.global_tools").unwrap(), "tuner");
        assert_eq!(
            set(&mut c, "ui.global_tools", Some("piano,tuner")).unwrap(),
            "piano,tuner"
        );
        assert_eq!(set(&mut c, "ui.global_tools", Some("")).unwrap(), "");
    }

    #[test]
    fn default_resets() {
        let mut c = f();
        set(&mut c, "ui.tab_width", Some("7")).unwrap();
        assert_eq!(set(&mut c, "ui.tab_width", None).unwrap(), "2");
        set(&mut c, "ui.tab_width", Some("7")).unwrap();
        assert_eq!(set(&mut c, "ui.tab_width", Some("default")).unwrap(), "2");
        set(&mut c, "ui.focus_ring_color", Some("#102030")).unwrap();
        assert_eq!(get(&c, "ui.focus_ring_color").unwrap(), "#102030");
        assert_eq!(set(&mut c, "ui.focus_ring_color", None).unwrap(), "");
        set(&mut c, "ui.focus_ring_color", Some("#102030")).unwrap();
        assert_eq!(
            set(&mut c, "ui.focus_ring_color", Some("default")).unwrap(),
            ""
        );
        assert!(set(&mut c, "ui.focus_ring_color", Some("red")).is_err());
        set(&mut c, "theme.accent", Some("#ff0000")).unwrap();
        assert_eq!(c.theme.colors["accent"], "#ff0000");
        set(&mut c, "theme.accent", Some("default")).unwrap();
        assert!(c.theme.colors.is_empty());
    }

    #[test]
    fn unknown_option_suggests() {
        let e = get(&f(), "tabline.wrapp").unwrap_err();
        assert!(e.contains("tabline.wrap"), "{e}");
        assert!(set(&mut f(), "nonsense", Some("1")).is_err());
    }

    #[test]
    fn keymap_is_not_an_option() {
        let e = spec("keymap.normal").unwrap_err();
        assert!(e.contains("keymaps are not options"), "{e}");
        assert!(get(&f(), "keymap").is_err());
    }

    fn leaves(prefix: &str, t: &Table, out: &mut Vec<String>) {
        for (k, v) in t {
            let path = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            match v {
                Value::Table(t) => leaves(&path, t, out),
                _ => out.push(path),
            }
        }
    }

    #[test]
    fn every_serialized_leaf_has_a_spec_and_back() {
        let mut paths = Vec::new();
        leaves("", &to_table(&f()).unwrap(), &mut paths);
        for p in paths.iter().filter(|p| !p.starts_with("keymap")) {
            assert!(OPTIONS.iter().any(|s| s.name == p), "no spec for {p}");
        }
        for s in OPTIONS.iter() {
            assert!(get(&f(), s.name).is_ok(), "{}", s.name);
            assert!(!s.doc.is_empty(), "{}", s.name);
        }
        // Resetting each option to its default leaves the config unchanged.
        let mut c = f();
        for s in OPTIONS.iter() {
            set(&mut c, s.name, None).unwrap();
        }
        assert_eq!(to_table(&c).unwrap(), to_table(&f()).unwrap());
    }

    #[test]
    fn snake_case_names() {
        for s in OPTIONS.iter() {
            let ok = s.name.split('.').all(|p| {
                p.starts_with(|c: char| c.is_ascii_lowercase())
                    && p.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            });
            assert!(ok, "{}", s.name);
        }
    }

    #[test]
    fn validate_clamps_and_defaults() {
        let mut c = f();
        c.ui.tab_width = 99;
        c.ui.bar_numbers = "x".into();
        c.ui.focus_ring_color = "red".into();
        let w = validate(&mut c);
        assert_eq!(w.len(), 3, "{w:?}");
        assert_eq!(c.ui.tab_width, 8);
        assert_eq!(c.ui.bar_numbers, "system");
        assert_eq!(c.ui.focus_ring_color, "");
        assert!(validate(&mut c).is_empty());
    }
}
