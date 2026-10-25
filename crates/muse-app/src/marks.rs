//! Marks, as in Vim: `m{a-z}` marks a position in the current file, `m{A-Z}` a position in any
//! file (jumping there opens it); `'{mark}` jumps to the mark's bar, `` `{mark} `` to its exact
//! beat and string; `''` / ``` `` ``` jump back to where the cursor was before the last jump.

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
        self.info(format!(
            "mark {name}: bar {}, beat {}",
            pane.cursor.bar + 1,
            pane.cursor.beat + 1
        ));
    }

    /// Jumps to mark `name`: its bar (`exact` false) or its exact position. `'` (and `` ` ``) is
    /// the position before the last jump.
    pub fn jump_to_mark(&mut self, name: char, exact: bool) {
        let target = match name {
            '\'' | '`' => self.pane().jump_back,
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
        self.jump(buffer, cursor, exact);
    }

    /// Moves the focused pane to a position (showing its buffer), remembering where it was for
    /// `''`.
    pub(crate) fn jump(&mut self, buffer: usize, mut cursor: Cursor, exact: bool) {
        let here = (self.pane().buffer, self.pane().cursor);
        if buffer != here.0 {
            self.show_buffer(buffer);
        }
        if !exact {
            cursor.beat = 0;
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
    fn local_and_global_marks_and_jumping_back() {
        let dir = std::env::temp_dir().join(format!("muse-marks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        let (a, b) = (dir.join("a.muse"), dir.join("b.muse"));
        app.run(Command::Write(Some(a.clone())));
        app.run(Command::Write(Some(b.clone())));
        let mut app = App::new(Config::default(), None);
        app.run(Command::Edit(a.clone()));
        // A local mark in a: bar 3, string 2 (beat 0 is the only beat of these bars).
        keys(&mut app, "}}jma");
        keys(&mut app, "}}}}mB");
        keys(&mut app, "gg");
        keys(&mut app, "'a");
        assert_eq!((app.pane().cursor.bar, app.pane().cursor.string), (2, 1));
        // '' goes back to before the jump.
        keys(&mut app, "''");
        assert_eq!(app.pane().cursor.bar, 0);
        // Local marks belong to their file; global marks bring their file back.
        app.run(Command::Edit(b.clone()));
        keys(&mut app, "'a");
        assert!(app.message.as_ref().unwrap().error);
        keys(&mut app, "`B");
        assert_eq!(app.buffer().name, "a.muse");
        assert_eq!(app.pane().cursor.bar, 6);
        // Even after the file was closed.
        app.run(Command::Edit(b.clone()));
        app.run(Command::BufferClose(Some(1), false));
        keys(&mut app, "'B");
        assert_eq!(
            (app.buffer().name.as_str(), app.pane().cursor.bar),
            ("a.muse", 6)
        );
        // Listing and deleting.
        let names: Vec<char> = app.mark_list().iter().map(|m| m.name).collect();
        assert_eq!(names, ['B']);
        app.run(Command::DeleteMarks(Some("B".into())));
        assert!(app.mark_list().is_empty());
        std::fs::remove_dir_all(dir).ok();
    }
}
