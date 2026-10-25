//! Workspaces: a session's views — each with its pane splits and its bufferline of loaded
//! files — and each pane's file, cursor, recent files and view settings, saved to a TOML file so
//! the session can be reopened as it was. The active workspace is saved again whenever it
//! changes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::app::{App, Buffer, Cursor, Pane, View};
use crate::layout::{Axis, Node, PaneId};

/// The saved form of a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub version: u32,
    /// Open files, by absolute path.
    pub files: Vec<PathBuf>,
    #[serde(alias = "active_tab")]
    pub active_view: usize,
    /// (Version 1 called views "tabs".)
    #[serde(alias = "tabs")]
    pub views: Vec<ViewState>,
    pub panes: Vec<PaneState>,
    pub zoom: f64,
    pub follow: bool,
    pub chant_view: bool,
    pub global_waveform: bool,
    /// Marks: lowercase ones belong to their file, uppercase ones are global.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<MarkState>,
    /// Recorded macros by register, in key notation.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub macros: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarkState {
    pub name: String,
    pub file: usize,
    pub track: usize,
    pub bar: usize,
    pub beat: usize,
    pub string: usize,
    pub voice: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewState {
    pub focus: PaneId,
    pub split: Split,
    /// The bufferline, as indices into `files` (absent in version 1: the files shown).
    #[serde(default)]
    pub buffers: Vec<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// A pane split tree: a pane, or panes stacked / side by side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Split {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane: Option<PaneId>,
    /// "stacked" or "side-by-side" for a split.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Split>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaneState {
    pub id: PaneId,
    /// Index into `files`; none for a pane on an unsaved score.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<usize>,
    pub track: usize,
    pub bar: usize,
    pub beat: usize,
    pub string: usize,
    pub voice: usize,
    pub layout: String,
    pub waveform: bool,
    /// Files shown in the pane before, most recent first, with their cursors.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent: Vec<RecentState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentState {
    pub file: usize,
    pub track: usize,
    pub bar: usize,
    pub beat: usize,
    pub string: usize,
    pub voice: usize,
}

fn split_of(node: &Node) -> Split {
    match node {
        Node::Leaf(id) => Split {
            pane: Some(*id),
            axis: None,
            children: Vec::new(),
        },
        Node::Split { axis, children } => Split {
            pane: None,
            axis: Some(
                match axis {
                    Axis::Stacked => "stacked",
                    Axis::SideBySide => "side-by-side",
                }
                .into(),
            ),
            children: children.iter().map(split_of).collect(),
        },
    }
}

/// The tree back, keeping only panes that exist.
fn node_of(split: &Split, panes: &HashMap<PaneId, Pane>) -> Option<Node> {
    if let Some(id) = split.pane {
        return panes.contains_key(&id).then_some(Node::Leaf(id));
    }
    let axis = match split.axis.as_deref() {
        Some("stacked") => Axis::Stacked,
        _ => Axis::SideBySide,
    };
    let mut children: Vec<Node> = split
        .children
        .iter()
        .filter_map(|c| node_of(c, panes))
        .collect();
    match children.len() {
        0 => None,
        1 => children.pop(),
        _ => Some(Node::Split { axis, children }),
    }
}

/// Where named workspaces are kept.
pub fn workspace_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "muse").map(|d| d.data_dir().join("workspaces"))
}

/// The workspace used when none was chosen.
pub const DEFAULT_WORKSPACE: &str = "default";

/// The file recording the last active workspace.
fn last_workspace_marker() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "muse").map(|d| d.data_dir().join("last-workspace"))
}

/// The workspace active when muse last ran (as a path), if it still exists.
pub fn last_workspace() -> Option<String> {
    let text = std::fs::read_to_string(last_workspace_marker()?).ok()?;
    let path = text.trim();
    (!path.is_empty() && Path::new(path).exists()).then(|| path.to_string())
}

/// A workspace argument as a file: a path (with `/` or `.toml`) or a name in [`workspace_dir`].
pub fn workspace_path(arg: &str) -> Option<PathBuf> {
    let arg = arg.trim();
    if arg.is_empty() {
        return None;
    }
    if arg.contains('/') || arg.ends_with(".toml") {
        let p = match arg.strip_prefix("~/") {
            Some(rest) => PathBuf::from(std::env::var_os("HOME")?).join(rest),
            None => PathBuf::from(arg),
        };
        return Some(p);
    }
    Some(workspace_dir()?.join(format!("{arg}.toml")))
}

/// Names of the saved workspaces.
pub fn workspace_names() -> Vec<String> {
    let mut names: Vec<String> = workspace_dir()
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension()? != "toml" {
                return None;
            }
            Some(p.file_stem()?.to_string_lossy().to_string())
        })
        .collect();
    names.sort();
    names
}

fn cursor_of(r: &RecentState) -> Cursor {
    Cursor {
        track: r.track,
        bar: r.bar,
        beat: r.beat,
        string: r.string,
        voice: r.voice,
    }
}

impl App {
    /// The session as a workspace.
    pub fn workspace(&self) -> Workspace {
        // Files in the order they appear: views' bufferlines first, then anything else shown.
        let mut files = Vec::new();
        let mut file_of: HashMap<usize, usize> = HashMap::new();
        let mut add = |b: usize, files: &mut Vec<PathBuf>, app: &App| -> Option<usize> {
            let path = app.buffers.get(b)?.path.clone()?;
            Some(*file_of.entry(b).or_insert_with(|| {
                files.push(path);
                files.len() - 1
            }))
        };
        let mut views = Vec::new();
        for v in &self.views {
            let buffers = v
                .buffers
                .iter()
                .filter_map(|&b| add(b, &mut files, self))
                .collect();
            views.push(ViewState {
                focus: v.focus,
                split: split_of(&v.root),
                buffers,
                name: v.name.clone(),
            });
        }
        let mut ids: Vec<&PaneId> = self.panes.keys().collect();
        ids.sort();
        let panes = ids
            .into_iter()
            .map(|id| {
                let p = &self.panes[id];
                PaneState {
                    id: *id,
                    file: add(p.buffer, &mut files, self),
                    track: p.cursor.track,
                    bar: p.cursor.bar,
                    beat: p.cursor.beat,
                    string: p.cursor.string,
                    voice: p.cursor.voice,
                    layout: p.layout.clone(),
                    waveform: p.waveform,
                    recent: p
                        .recent
                        .iter()
                        .filter_map(|(b, c)| {
                            Some(RecentState {
                                file: add(*b, &mut files, self)?,
                                track: c.track,
                                bar: c.bar,
                                beat: c.beat,
                                string: c.string,
                                voice: c.voice,
                            })
                        })
                        .collect(),
                }
            })
            .collect();
        // Marks of saved files.
        let mut marks = Vec::new();
        let mut mark = |name: char,
                        b: usize,
                        c: &Cursor,
                        files: &mut Vec<PathBuf>,
                        marks: &mut Vec<MarkState>| {
            if let Some(file) = add(b, files, self) {
                marks.push(MarkState {
                    name: name.to_string(),
                    file,
                    track: c.track,
                    bar: c.bar,
                    beat: c.beat,
                    string: c.string,
                    voice: c.voice,
                });
            }
        };
        for (b, m) in &self.marks.local {
            for (name, c) in m {
                mark(*name, *b, c, &mut files, &mut marks);
            }
        }
        for (name, g) in &self.marks.global {
            mark(*name, g.buffer, &g.cursor, &mut files, &mut marks);
        }
        marks.sort_by(|a, b| a.name.cmp(&b.name).then(a.file.cmp(&b.file)));
        let macros = self
            .ed
            .macros
            .iter()
            .map(|(r, keys)| (r.to_string(), crate::keys::format_keys(keys)))
            .collect();
        Workspace {
            version: 2,
            marks,
            macros,
            files,
            active_view: self.active_view,
            views,
            panes,
            zoom: self.zoom,
            follow: self.follow,
            chant_view: self.chant_view,
            global_waveform: self.global_waveform,
        }
    }

    /// Saves the session as a workspace (by name or path; the active one without), and makes
    /// it the active workspace.
    pub fn save_workspace(&mut self, target: Option<&str>) {
        let path = match target {
            Some(t) => workspace_path(t),
            None => self.workspace_file.clone(),
        };
        let Some(path) = path else {
            return self.error("workspace-save: give a name (no workspace is active)");
        };
        match self.write_workspace(&path) {
            Ok(()) => {
                self.set_workspace_file(path.clone());
                self.info(format!("workspace saved: {}", workspace_label(&path)));
            }
            Err(e) => self.error(format!("workspace-save: {e}")),
        }
    }

    fn write_workspace(&mut self, path: &Path) -> Result<(), String> {
        let text = toml::to_string_pretty(&self.workspace()).map_err(|e| e.to_string())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
        self.workspace_saved = Some(text);
        Ok(())
    }

    /// Makes a workspace the active one: opens it if it was saved, otherwise starts it with the
    /// current session.
    pub fn use_workspace(&mut self, name: &str) {
        match workspace_path(name) {
            Some(path) if path.exists() => self.open_workspace(name, true),
            Some(_) => self.save_workspace(Some(name)),
            None => self.error(format!("workspace {name}: no data folder to keep it in")),
        }
    }

    /// Leaves the active workspace (saving it) for the default one: its saved session, or an
    /// empty one the first time.
    pub fn close_workspace(&mut self) {
        let default = workspace_path(DEFAULT_WORKSPACE);
        if self.workspace_file.is_none() || self.workspace_file == default {
            return self.error("this is the default workspace (open another with :workspace-open)");
        }
        if let Some(b) = self.buffers.iter().find(|b| b.modified) {
            return self.error(format!(
                "workspace-close: \"{}\" has unsaved changes (:w first)",
                b.name
            ));
        }
        match default {
            Some(p) if p.exists() => self.open_workspace(DEFAULT_WORKSPACE, false),
            _ => {
                self.autosave_workspace();
                let fresh = App::new(self.config.clone(), None).workspace();
                self.restore(&fresh);
                self.save_workspace(Some(DEFAULT_WORKSPACE));
            }
        }
    }

    fn set_workspace_file(&mut self, path: PathBuf) {
        if self.remember_workspace {
            if let Some(marker) = last_workspace_marker() {
                let abs = std::path::absolute(&path).unwrap_or_else(|_| path.clone());
                std::fs::write(marker, abs.display().to_string()).ok();
            }
        }
        self.workspace_file = Some(path);
    }

    /// Saves the active workspace if the session changed since it was last saved.
    pub fn autosave_workspace(&mut self) {
        let Some(path) = self.workspace_file.clone() else {
            return;
        };
        let Ok(text) = toml::to_string_pretty(&self.workspace()) else {
            return;
        };
        if self.workspace_saved.as_deref() != Some(text.as_str()) {
            if let Err(e) = self.write_workspace(&path) {
                self.error(format!("workspace not saved: {e}"));
            }
        }
    }

    /// Replaces the session with a saved workspace. Unsaved changes stop it unless `force`.
    pub fn open_workspace(&mut self, name: &str, force: bool) {
        let Some(path) = workspace_path(name) else {
            return self.error("workspace-open: give a name or path");
        };
        if !force {
            if let Some(b) = self.buffers.iter().find(|b| b.modified) {
                return self.error(format!(
                    "workspace-open: \"{}\" has unsaved changes (:w, or :workspace-open! to discard)",
                    b.name
                ));
            }
        }
        let ws: Workspace = match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| toml::from_str(&t).map_err(|e| e.to_string()))
        {
            Ok(ws) => ws,
            Err(e) => return self.error(format!("workspace-open: {}: {e}", path.display())),
        };
        // Save the workspace being left first.
        self.autosave_workspace();
        self.restore(&ws);
        self.set_workspace_file(path.clone());
        self.workspace_saved = toml::to_string_pretty(&self.workspace()).ok();
        let missing = ws.files.iter().filter(|f| !f.exists()).count();
        if missing > 0 {
            self.error(format!(
                "workspace {}: {missing} file(s) no longer exist",
                workspace_label(&path)
            ));
        } else {
            self.info(format!("workspace opened: {}", workspace_label(&path)));
        }
    }

    fn restore(&mut self, ws: &Workspace) {
        self.buffers = vec![Buffer::empty()];
        self.panes.clear();
        self.views.clear();
        // Open the files; the first replaces the empty buffer.
        let mut buffer_of = HashMap::new();
        for (i, f) in ws.files.iter().enumerate() {
            if let Ok(b) = self.open(f) {
                buffer_of.insert(i, b);
            }
        }
        let empty = |app: &mut App| -> usize {
            match app.buffers.iter().position(|b| b.path.is_none()) {
                Some(i) => i,
                None => {
                    app.buffers.push(Buffer::empty());
                    app.buffers.len() - 1
                }
            }
        };
        for p in &ws.panes {
            let buffer = match p.file.and_then(|f| buffer_of.get(&f)) {
                Some(&b) => b,
                None => empty(self),
            };
            let mut cursor = Cursor {
                track: p.track,
                bar: p.bar,
                beat: p.beat,
                string: p.string,
                voice: p.voice,
            };
            crate::app::clamp_cursor(&self.buffers[buffer].score, &mut cursor);
            let recent = p
                .recent
                .iter()
                .filter_map(|r| Some((*buffer_of.get(&r.file)?, cursor_of(r))))
                .collect();
            self.panes.insert(
                p.id,
                Pane {
                    buffer,
                    cursor,
                    scroll: 0,
                    layout: p.layout.clone(),
                    waveform: p.waveform,
                    recent,
                    jump_back: None,
                },
            );
        }
        for v in &ws.views {
            let Some(root) = node_of(&v.split, &self.panes) else {
                continue;
            };
            let ids = root.panes();
            let focus = if ids.contains(&v.focus) {
                v.focus
            } else {
                ids[0]
            };
            let mut buffers: Vec<usize> = v
                .buffers
                .iter()
                .filter_map(|f| buffer_of.get(f).copied())
                .collect();
            // Every buffer a pane shows is on the view's bufferline (version 1 had no list).
            for id in &ids {
                let b = self.panes[id].buffer;
                if !buffers.contains(&b) {
                    buffers.push(b);
                }
            }
            self.views.push(View {
                root,
                focus,
                buffers,
                name: v.name.clone(),
            });
        }
        // Panes in no view are dropped; with nothing left, one pane on the first buffer.
        let shown: Vec<PaneId> = self.views.iter().flat_map(|v| v.root.panes()).collect();
        self.panes.retain(|id, _| shown.contains(id));
        if self.views.is_empty() {
            let layout = self.score_layout.clone();
            self.panes.insert(
                1,
                Pane {
                    buffer: 0,
                    cursor: Cursor::default(),
                    scroll: 0,
                    layout,
                    waveform: true,
                    recent: Vec::new(),
                    jump_back: None,
                },
            );
            self.views.push(View {
                root: Node::Leaf(1),
                focus: 1,
                buffers: vec![0],
                name: None,
            });
        }
        self.marks = Default::default();
        for m in &ws.marks {
            let (Some(name), Some(&buffer)) = (m.name.chars().next(), buffer_of.get(&m.file))
            else {
                continue;
            };
            let cursor = Cursor {
                track: m.track,
                bar: m.bar,
                beat: m.beat,
                string: m.string,
                voice: m.voice,
            };
            if name.is_ascii_uppercase() {
                self.marks.global.insert(
                    name,
                    crate::marks::GlobalMark {
                        path: self.buffers[buffer].path.clone(),
                        buffer,
                        cursor,
                    },
                );
            } else {
                self.marks
                    .local
                    .entry(buffer)
                    .or_default()
                    .insert(name, cursor);
            }
        }
        self.ed.macros.clear();
        for (r, keys) in &ws.macros {
            let leader = self.config.leader.clone();
            if let (Some(r), Ok(keys)) = (r.chars().next(), crate::keys::parse_keys(keys, &leader))
            {
                self.ed.macros.insert(r, keys);
            }
        }
        self.next_pane = self.panes.keys().max().copied().unwrap_or(0) + 1;
        self.active_view = ws.active_view.min(self.views.len() - 1);
        self.zoom = ws.zoom;
        self.follow = ws.follow;
        self.chant_view = ws.chant_view;
        self.global_waveform = ws.global_waveform;
    }
}

impl App {
    /// Name of the active workspace, for the status line.
    pub fn workspace_name(&self) -> Option<String> {
        self.workspace_file.as_deref().map(workspace_label)
    }
}

fn workspace_label(path: &Path) -> String {
    match workspace_dir() {
        Some(d) if path.parent() == Some(d.as_path()) => path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        _ => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Config};

    #[test]
    fn a_session_round_trips_through_a_workspace() {
        let dir = std::env::temp_dir().join(format!("muse-ws-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Three small scores to open.
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        let (a, b, c) = (dir.join("a.muse"), dir.join("b.muse"), dir.join("c.muse"));
        for f in [&a, &b, &c] {
            app.run(Command::Write(Some(f.clone())));
        }
        let mut app = App::new(Config::default(), None);
        app.run(Command::Edit(a.clone()));
        app.run(Command::BarNext);
        app.run(Command::Layout(Some("page".into())));
        app.run(Command::VSplit);
        app.run(Command::Edit(b.clone()));
        app.run(Command::Waveform);
        app.run(Command::ViewNew(Some(c.clone())));
        app.run(Command::ViewRename(Some("practice".into())));
        app.zoom = 1.3;
        // Marks and macros are kept too.
        app.run(Command::Mark('q'));
        app.run(Command::Mark('Z'));
        for k in crate::keys::parse_keys("qa}<C-w>vq", &[crate::keys::Key::char(' ')]).unwrap() {
            app.handle_key(k);
        }
        let ws_file = dir.join("session.toml");
        app.run(Command::WorkspaceSave(Some(ws_file.display().to_string())));
        let saved = app.workspace();
        assert_eq!(saved.views[0].buffers, [0, 1]);
        assert_eq!(saved.views[1].buffers, [0, 1, 2]);

        let mut other = App::new(Config::default(), None);
        other.run(Command::WorkspaceOpen(ws_file.display().to_string(), false));
        assert_eq!(other.workspace(), saved);
        assert_eq!(other.views.len(), 2);
        assert_eq!(other.view_labels(), ["1", "practice"]);
        assert_eq!(
            other
                .marks
                .local
                .values()
                .flat_map(|m| m.keys())
                .collect::<Vec<_>>(),
            [&'q']
        );
        assert!(other.marks.global.contains_key(&'Z'));
        assert_eq!(saved.macros.get("a").map(String::as_str), Some("}<C-w>v"));
        assert_eq!(other.workspace_file.as_deref(), Some(ws_file.as_path()));
        // Changes are saved automatically.
        other.run(Command::ViewPrev);
        other.autosave_workspace();
        let reread: Workspace =
            toml::from_str(&std::fs::read_to_string(&ws_file).unwrap()).unwrap();
        assert_eq!(reread.active_view, 0);
        // Unsaved changes stop an open unless forced.
        other.buffers[0].modified = true;
        other.run(Command::WorkspaceOpen(ws_file.display().to_string(), false));
        assert!(other.message.as_ref().unwrap().error);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn using_a_workspace_starts_it_or_opens_it() {
        let dir = std::env::temp_dir().join(format!("muse-ws-use-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ws = dir.join("gig.toml").display().to_string();
        // Not saved yet: the current session becomes it.
        let mut app = App::new(Config::default(), None);
        app.run(Command::Split);
        app.use_workspace(&ws);
        assert_eq!(app.workspace_name().as_deref(), Some(ws.as_str()));
        assert!(std::path::Path::new(&ws).exists());
        // Saved: it is opened.
        let mut other = App::new(Config::default(), None);
        other.use_workspace(&ws);
        assert_eq!(other.views[0].root.panes().len(), 2);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn version_one_workspaces_still_open() {
        let text = r#"
version = 1
files = []
active_tab = 0
zoom = 1.0
follow = true
chant_view = true
global_waveform = false

[[tabs]]
focus = 1

[tabs.split]
pane = 1

[[panes]]
id = 1
track = 0
bar = 0
beat = 0
string = 0
voice = 0
layout = "vertical"
waveform = true
"#;
        let ws: Workspace = toml::from_str(text).unwrap();
        let mut app = App::new(Config::default(), None);
        app.restore(&ws);
        assert_eq!(app.views.len(), 1);
        assert_eq!(app.view().buffers.len(), 1);
    }
}
