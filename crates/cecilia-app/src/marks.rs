//! Marks, as in Vim: `` `{a-z} `` marks a position in the current file, `` `{A-Z} `` a position in any
//! file (jumping there opens it); `'{mark}` jumps to the mark's exact
//! beat and string; `''` jumps back to where the cursor was before the last jump.

use crate::app::position_text;
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::app::{App, Cursor};

/// A global mark: the file (or, for an unsaved score, the buffer) and the position in it.
#[derive(Debug, Clone, PartialEq)]
pub struct GlobalMark {
    pub path: Option<PathBuf>,
    pub buffer: usize,
    pub cursor: Cursor,
}

/// All marks of a session.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Marks {
    /// Lowercase marks per buffer.
    pub local: BTreeMap<usize, BTreeMap<char, Cursor>>,
    /// Uppercase marks.
    pub global: BTreeMap<char, GlobalMark>,
}

/// A mark as listed (in the marks picker).
#[derive(Debug, Clone, PartialEq)]
pub struct MarkInfo {
    pub name: char,
    pub file: String,
    pub cursor: Cursor,
}

/// Labels for marks, each with the screen position `key` it is drawn at: marks sharing a
/// position become one label, their names run together in the order given (`ac'`).
pub fn merge_labels<K: PartialEq>(marks: impl IntoIterator<Item = (char, K)>) -> Vec<(String, K)> {
    let mut out: Vec<(String, K)> = Vec::new();
    for (name, at) in marks {
        match out.iter_mut().find(|(_, k)| *k == at) {
            Some((label, _)) => label.push(name),
            None => out.push((name.to_string(), at)),
        }
    }
    out
}

impl App {
    /// Sets mark `name` at the focused pane's cursor.
    pub fn set_mark(&mut self, name: char) {
        let pane = self.pane().clone();
        match name {
            'a'..='z' => {
                self.marks
                    .local
                    .entry(pane.buffer)
                    .or_default()
                    .insert(name, pane.cursor);
            }
            'A'..='Z' => {
                let path = self.buffers[pane.buffer].path.clone();
                self.marks.global.insert(
                    name,
                    GlobalMark {
                        path,
                        buffer: pane.buffer,
                        cursor: pane.cursor,
                    },
                );
            }
            _ => {
                return self.error(format!(
                    "mark: '{name}' is not a letter (a-z local, A-Z global)"
                ));
            }
        }
        let at = position_text(&self.buffers[pane.buffer].score, &pane.cursor);
        self.info(format!("mark {name}: {at}"));
    }

    /// Jumps to mark `name`: its exact position. `'` (and `` ` ``) is
    /// the position before the last jump.
    pub fn jump_to_mark(&mut self, name: char) {
        let target = match name {
            '\'' | '`' => self.pane().jump_back,
            '<' | '>' | '^' => self.special_mark(name),
            'a'..='z' => {
                let b = self.pane().buffer;
                self.marks
                    .local
                    .get(&b)
                    .and_then(|m| m.get(&name))
                    .map(|c| (b, *c))
            }
            'A'..='Z' => match self.marks.global.get(&name).cloned() {
                Some(m) => {
                    // The file may have been closed since: open it again.
                    let loaded = self.buffers.get(m.buffer).is_some_and(|b| b.path == m.path);
                    let buffer = match (&m.path, loaded) {
                        (_, true) => Some(m.buffer),
                        (Some(p), false) => match self.open(p) {
                            Ok(b) => {
                                if let Some(g) = self.marks.global.get_mut(&name) {
                                    g.buffer = b;
                                }
                                Some(b)
                            }
                            Err(e) => return self.error(format!("mark {name}: {e}")),
                        },
                        (None, false) => None,
                    };
                    buffer.map(|b| (b, m.cursor))
                }
                None => None,
            },
            _ => return self.error(format!("mark: '{name}' is not a mark")),
        };
        let Some((buffer, cursor)) = target else {
            return self.error(format!("mark {name} is not set"));
        };
        self.jump(buffer, cursor);
    }

    /// Moves the focused pane to a position (showing its buffer), remembering where it was for
    /// `''`.
    pub(crate) fn jump(&mut self, buffer: usize, mut cursor: Cursor) {
        let here = (self.pane().buffer, self.pane().cursor);
        if buffer != here.0 {
            self.show_buffer(buffer);
        }
        crate::app::clamp_cursor(&self.buffers[buffer].score, &mut cursor);
        let pane = self.pane_mut();
        pane.cursor = cursor;
        pane.jump_back = Some(here);
    }

    /// Deletes marks by name (`None`: all lowercase marks of the current file, as `:delmarks!`).
    pub fn delete_marks(&mut self, names: Option<&str>) {
        let b = self.pane().buffer;
        match names {
            None => {
                self.marks.local.remove(&b);
            }
            Some(names) => {
                for c in names.chars().filter(|c| !c.is_whitespace()) {
                    if c.is_ascii_uppercase() {
                        self.marks.global.remove(&c);
                    } else if let Some(m) = self.marks.local.get_mut(&b) {
                        m.remove(&c);
                    }
                }
            }
        }
    }

    /// The marks reachable from the focused pane: its file's lowercase marks, then the global
    /// ones.
    pub fn mark_list(&self) -> Vec<MarkInfo> {
        let b = self.pane().buffer;
        let mut out: Vec<MarkInfo> = self
            .marks
            .local
            .get(&b)
            .into_iter()
            .flatten()
            .map(|(&name, &cursor)| MarkInfo {
                name,
                file: self.buffers[b].name.clone(),
                cursor,
            })
            .collect();
        out.extend(self.marks.global.iter().map(|(&name, m)| {
            MarkInfo {
                name,
                file: m
                    .path
                    .as_deref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .or_else(|| self.buffers.get(m.buffer).map(|b| b.name.clone()))
                    .unwrap_or_default(),
                cursor: m.cursor,
            }
        }));
        out
    }

    /// While `'` waits for a mark letter (and `ui.mark_labels` is not off): the marks (and `'`,
    /// the way back) that sit in buffer `buffer`, so the score can show where each jump would
    /// land. Several can land on one drawn position: see [`merge_labels`].
    pub fn jump_targets(&self, buffer: usize) -> Vec<(char, Cursor)> {
        if !matches!(self.ed.awaiting, Some(crate::editor::Awaiting::Jump))
            || self.config.file.ui.mark_labels == crate::config::MarkLabels::Off
        {
            return Vec::new();
        }
        let path = self.buffers.get(buffer).map(|b| &b.path);
        let named = self
            .marks
            .local
            .get(&buffer)
            .into_iter()
            .flatten()
            .map(|(&n, &c)| (n, c));
        let global = self
            .marks
            .global
            .iter()
            .filter(|(_, m)| m.buffer == buffer && Some(&m.path) == path)
            .map(|(&n, m)| (n, m.cursor));
        let back = self
            .pane()
            .jump_back
            .filter(|j| j.0 == buffer)
            .map(|j| ('\'', j.1));
        named.chain(global).chain(back).collect()
    }

    /// The which-key popup: its title and rows (keys, what they do). While `'`, or `` ` ``
    /// waits for a mark letter, the saved marks and where they are; otherwise the keys that can
    /// follow the ones typed so far. `None` when there is nothing to show.
    pub fn which_key(&self) -> Option<(String, Vec<(String, String)>)> {
        use crate::editor::Awaiting;
        let place =
            |file: &str, c: &Cursor| format!("{file} · {}", position_text(&self.buffer().score, c));
        match self.ed.awaiting {
            Some(Awaiting::Jump) => {
                let mut rows = Vec::new();
                if let Some((b, c)) = self.pane().jump_back {
                    let file = self.buffers.get(b).map_or("", |b| b.name.as_str());
                    rows.push(("'".to_string(), format!("back · {}", place(file, &c))));
                }
                rows.extend(
                    self.mark_list()
                        .iter()
                        .map(|m| (m.name.to_string(), place(&m.file, &m.cursor))),
                );
                if rows.is_empty() {
                    rows.push(("".into(), "no marks yet (`{a-z} sets one)".into()));
                }
                Some(("'".to_string(), rows))
            }
            Some(Awaiting::Mark) => {
                let mut rows: Vec<(String, String)> = self
                    .mark_list()
                    .iter()
                    .map(|m| {
                        (
                            m.name.to_string(),
                            format!("(set) {}", place(&m.file, &m.cursor)),
                        )
                    })
                    .collect();
                rows.insert(
                    0,
                    (
                        "a-z / A-Z".into(),
                        "mark here (this file / any file)".into(),
                    ),
                );
                Some(("`".to_string(), rows))
            }
            Some(Awaiting::Register) => Some((
                "\"".to_string(),
                vec![(
                    "a-z".into(),
                    "register for the next yank, delete or paste".into(),
                )],
            )),
            Some(Awaiting::RecordMacro) => Some((
                "q".to_string(),
                vec![("a-z".into(), "record a macro into this register".into())],
            )),
            Some(Awaiting::PlayMacro) => Some((
                "@".to_string(),
                vec![("a-z".into(), "play the macro in this register".into())],
            )),
            None if self.pending.is_empty() => {
                // A count or an operator typed, waiting for what it applies to.
                let typed = crate::keys::format_keys(&self.ed.cmd_keys);
                if typed.is_empty() {
                    return None;
                }
                let rows = match self.ed.operator {
                    Some((op, _)) => self.operator_rows(op),
                    None => vec![("".into(), "a motion or command".into())],
                };
                Some((typed, rows))
            }
            None => {
                let typed = if self.ed.cmd_keys.is_empty() {
                    crate::keys::format_keys(&self.pending)
                } else {
                    crate::keys::format_keys(&self.ed.cmd_keys)
                };
                let map = self.config.keymap(match self.mode {
                    crate::config::Mode::Tracks => crate::config::Mode::Tracks,
                    crate::config::Mode::Properties => crate::config::Mode::Properties,
                    crate::config::Mode::Palette => crate::config::Mode::Palette,
                    _ => crate::config::Mode::Normal,
                });
                let chant = self.buffer().score.is_chant();
                // The keys of the kind of score being edited (its own layer first), and only
                // the commands that kind has.
                let layer = (!matches!(
                    self.mode,
                    crate::config::Mode::Tracks
                        | crate::config::Mode::Properties
                        | crate::config::Mode::Palette
                ))
                .then(|| self.config.normal_keymap(self.insert_kind()));
                let mut seen = std::collections::HashSet::new();
                let mut rows: Vec<(String, String)> = Vec::new();
                for m in layer.into_iter().chain([map]) {
                    for (keys, _) in m.continuations(&self.pending) {
                        let mut full = self.pending.clone();
                        full.extend(keys.iter().copied());
                        if !seen.insert(full.clone()) {
                            continue;
                        }
                        let names: Vec<String> = m
                            .names
                            .get(&full)
                            .cloned()
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|l| crate::editor::command_line_in_scope(l, chant))
                            .collect();
                        let had = m.names.get(&full).is_some_and(|n| !n.is_empty());
                        if had && names.is_empty() {
                            continue;
                        }
                        rows.push((crate::keys::format_keys(&keys), names.join(", ")));
                    }
                }
                rows.sort();
                (!rows.is_empty()).then_some((typed, rows))
            }
        }
    }

    /// What can follow an operator: the motions of the Normal keymap, and the operator's own
    /// key again (the whole bar).
    fn operator_rows(&self, op: crate::command::Operator) -> Vec<(String, String)> {
        let chant = self.buffer().score.is_chant();
        let layer = self.config.normal_keymap(self.insert_kind());
        let mut rows: Vec<(String, String)> = Vec::new();
        for m in [layer, self.config.keymap(crate::config::Mode::Normal)] {
            for (keys, cmds) in &m.bindings {
                let [cmd] = cmds.as_slice() else { continue };
                if !(crate::editor::is_motion(cmd) || *cmd == crate::Command::Operator(op)) {
                    continue;
                }
                let names: Vec<String> = m
                    .names
                    .get(keys)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|l| crate::editor::command_line_in_scope(l, chant))
                    .collect();
                let row = (crate::keys::format_keys(keys), names.join(", "));
                if !rows.iter().any(|r| r.0 == row.0) {
                    rows.push(row);
                }
            }
        }
        rows.sort();
        rows
    }

    /// Forgets the marks of an unloaded buffer.
    pub(crate) fn forget_marks(&mut self, buffer: usize) {
        self.marks.local.remove(&buffer);
    }
}

#[cfg(test)]
mod tests {
    use crate::keys::{Key, parse_keys};
    use crate::{App, Command, Config};

    fn keys(app: &mut App, s: &str) {
        for k in parse_keys(s, &[Key::char(' ')]).unwrap() {
            app.handle_key(k);
        }
    }

    #[test]
    fn which_key_shows_counts_operators_and_register_prompts() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        assert!(app.which_key().is_none());
        keys(&mut app, "3");
        assert_eq!(app.which_key().unwrap().0, "3");
        keys(&mut app, "d");
        let (title, rows) = app.which_key().unwrap();
        assert_eq!(title, "3d");
        assert!(rows.iter().any(|r| r.0 == "d"), "{rows:?}");
        assert!(rows.iter().any(|r| r.1.contains("bar-next")), "{rows:?}");
        keys(&mut app, "<Esc>");
        assert!(app.which_key().is_none());
        keys(&mut app, "c");
        assert_eq!(app.which_key().unwrap().0, "c");
        keys(&mut app, "<Esc>q");
        assert_eq!(app.which_key().unwrap().0, "q");
        keys(&mut app, "a");
        assert!(app.which_key().is_none());
        keys(&mut app, "q\"");
        assert_eq!(app.which_key().unwrap().0, "\"");
        keys(&mut app, "a");
        // The register chosen waits for what to do with it, until Esc.
        assert_eq!(app.which_key().unwrap().0, "\"a");
        keys(&mut app, "<Esc>");
        assert!(app.which_key().is_none());
        // The finished operator leaves nothing behind.
        keys(&mut app, "dd");
        assert!(app.which_key().is_none());
    }

    #[test]
    fn local_and_global_marks_and_jumping_back() {
        let dir = std::env::temp_dir().join(format!("cecilia-marks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        let (a, b) = (dir.join("a.ccm"), dir.join("b.ccm"));
        app.run(Command::Write(Some(a.clone())));
        app.run(Command::Write(Some(b.clone())));
        let mut app = App::new(Config::default(), None);
        app.run(Command::Edit(a.clone()));
        // A local mark in a: bar 3, string 2 (beat 0 is the only beat of these bars).
        keys(&mut app, "}}j`a");
        keys(&mut app, "}}}}`B");
        keys(&mut app, "gg");
        keys(&mut app, "'a");
        assert_eq!((app.pane().cursor.bar, app.pane().cursor.string), (2, 1));
        // '' goes back to before the jump.
        keys(&mut app, "''");
        assert_eq!(app.pane().cursor.bar, 0);
        // Local marks belong to their file; global marks bring their file back.
        app.run(Command::Edit(b.clone()));
        keys(&mut app, "'a");
        assert!(app.last_notification().unwrap().error);
        keys(&mut app, "'B");
        assert_eq!(app.buffer().name, "a.ccm");
        assert_eq!(app.pane().cursor.bar, 6);
        // Even after the file was closed.
        app.run(Command::Edit(b.clone()));
        app.run(Command::BufferClose(Some(1), false));
        keys(&mut app, "'B");
        assert_eq!(
            (app.buffer().name.as_str(), app.pane().cursor.bar),
            ("a.ccm", 6)
        );
        // ' and ` open the which-key popup with the marks (and the way back).
        keys(&mut app, "'");
        let (title, rows) = app.which_key().unwrap();
        assert_eq!(title, "'");
        assert!(
            rows.iter()
                .any(|(k, d)| k == "B" && d.contains("a.ccm · 7:1")),
            "{rows:?}"
        );
        assert!(
            rows.iter().any(|(k, d)| k == "'" && d.starts_with("back")),
            "{rows:?}"
        );
        keys(&mut app, "<Esc>");
        assert!(app.which_key().is_none());
        keys(&mut app, "`");
        assert_eq!(app.which_key().unwrap().0, "`");
        keys(&mut app, "<Esc>");
        // Listing and deleting.
        let names: Vec<char> = app.mark_list().iter().map(|m| m.name).collect();
        assert_eq!(names, ['B']);
        app.run(Command::DeleteMarks(Some("B".into())));
        assert!(app.mark_list().is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn marks_at_one_position_share_a_label_and_it_can_be_turned_off() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        keys(&mut app, "}}`a}}`b`c");
        let b = app.pane().buffer;
        assert!(app.jump_targets(b).is_empty(), "only while ' waits");
        keys(&mut app, "gg}'");
        let labels: Vec<String> =
            super::merge_labels(app.jump_targets(b).into_iter().map(|(n, c)| (n, c.bar)))
                .into_iter()
                .map(|t| t.0)
                .collect();
        // `gg` left from where b and c are, so the way back (') is there too.
        assert_eq!(labels, ["a", "bc'"]);
        keys(&mut app, "<Esc>");
        app.run(Command::parse("set ui.mark_labels off").unwrap());
        keys(&mut app, "'");
        assert!(app.jump_targets(b).is_empty());
    }

    #[test]
    fn labels_on_one_position_run_together_in_order() {
        let labels = super::merge_labels([('a', 1), ('b', 2), ('c', 1), ('\'', 1)]);
        assert_eq!(labels, [("ac'".to_string(), 1), ("b".to_string(), 2)]);
    }
}
