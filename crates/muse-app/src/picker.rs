//! Pickers: the command palette's box, listing buffers, views or workspaces to switch to. Typing
//! filters the list (with the completion settings: fuzzy, case), Up/Down move, Enter switches.

use crate::app::{App, Palette, PaletteItem};
use crate::command::Command;
use crate::config::Mode;
use crate::keys::{Key, KeyCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    Buffer,
    View,
    Workspace,
    Mark,
    Macro,
    /// Every key binding of every mode, found by keys, command or description.
    Keymap,
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
                    })
                    .collect()
            }
            PickerKind::Keymap => {
                let modes = [
                    (Mode::Normal, ""),
                    (Mode::Visual, "visual "),
                    (Mode::Insert, "insert "),
                    (Mode::Tracks, "tracks "),
                    (Mode::Command, "command "),
                ];
                let mut out = Vec::new();
                for (mode, prefix) in modes {
                    let map = self.config.keymap(mode);
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
                        let action = match (mode, cmds.as_slice()) {
                            (Mode::Normal, [c]) => c.clone(),
                            _ => Command::Nop,
                        };
                        out.push(Entry {
                            label: format!("{prefix}{}", crate::keys::format_keys(k)),
                            description: what.join("; "),
                            action,
                            search_description: true,
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
                        }
                    })
                    .collect()
            }
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
        ranked
            .into_iter()
            .filter_map(|(t, pos)| {
                let i = entries.iter().position(|e| text(e) == t)?;
                let e = entries.remove(i);
                let n = e.label.chars().count();
                let pos = pos.into_iter().filter(|&p| p < n).collect();
                Some((e, pos))
            })
            .collect()
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
                    matched,
                })
                .collect(),
            selected,
        })
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
            KeyCode::Char(c) if !key.ctrl && !key.alt => {
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
