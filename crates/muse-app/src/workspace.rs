//! Workspaces: the open files, tabs, pane splits and per-pane views saved to a TOML file, so a
//! session can be reopened as it was. The active workspace is saved again whenever it changes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::app::{App, Buffer, Cursor, Pane, Tab};
use crate::layout::{Axis, Node, PaneId};

/// The saved form of a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub version: u32,
    /// Open files, by absolute path.
    pub files: Vec<PathBuf>,
    pub active_tab: usize,
    pub tabs: Vec<TabState>,
    pub panes: Vec<PaneState>,
    pub zoom: f64,
    pub follow: bool,
    pub chant_view: bool,
    pub global_waveform: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabState {
    pub focus: PaneId,
    pub split: Split,
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

impl App {
    /// The session as a workspace.
    pub fn workspace(&self) -> Workspace {
        let mut files = Vec::new();
        let mut file_of = HashMap::new();
        for (i, b) in self.buffers.iter().enumerate() {
            if let Some(p) = &b.path {
                file_of.insert(i, files.len());
                files.push(p.clone());
            }
        }
        let mut ids: Vec<&PaneId> = self.panes.keys().collect();
        ids.sort();
        let panes = ids
            .into_iter()
            .map(|id| {
                let p = &self.panes[id];
                PaneState {
                    id: *id,
                    file: file_of.get(&p.buffer).copied(),
                    track: p.cursor.track,
                    bar: p.cursor.bar,
                    beat: p.cursor.beat,
                    string: p.cursor.string,
                    voice: p.cursor.voice,
                    layout: p.layout.clone(),
                    waveform: p.waveform,
                }
            })
            .collect();
        Workspace {
            version: 1,
            files,
            active_tab: self.active_tab,
            tabs: self
                .tabs
                .iter()
                .map(|t| TabState {
                    focus: t.focus,
                    split: split_of(&t.root),
                })
                .collect(),
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
                self.workspace_file = Some(path.clone());
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
        self.workspace_file = Some(path.clone());
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
        self.tabs.clear();
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
            self.panes.insert(
                p.id,
                Pane {
                    buffer,
                    cursor,
                    scroll: 0,
                    layout: p.layout.clone(),
                    waveform: p.waveform,
                },
            );
        }
        for t in &ws.tabs {
            if let Some(root) = node_of(&t.split, &self.panes) {
                let ids = root.panes();
                let focus = if ids.contains(&t.focus) {
                    t.focus
                } else {
                    ids[0]
                };
                self.tabs.push(Tab { root, focus });
            }
        }
        // Panes in no tab are dropped; with nothing left, one pane on the first buffer.
        let shown: Vec<PaneId> = self.tabs.iter().flat_map(|t| t.root.panes()).collect();
        self.panes.retain(|id, _| shown.contains(id));
        if self.tabs.is_empty() {
            let layout = self.score_layout.clone();
            self.panes.insert(
                1,
                Pane {
                    buffer: 0,
                    cursor: Cursor::default(),
                    scroll: 0,
                    layout,
                    waveform: true,
                },
            );
            self.tabs.push(Tab {
                root: Node::Leaf(1),
                focus: 1,
            });
        }
        self.next_pane = self.panes.keys().max().copied().unwrap_or(0) + 1;
        self.active_tab = ws.active_tab.min(self.tabs.len() - 1);
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
        // Two small scores to open.
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        let (a, b) = (dir.join("a.muse"), dir.join("b.muse"));
        app.run(Command::Write(Some(a.clone())));
        app.run(Command::Write(Some(b.clone())));
        let mut app = App::new(Config::default(), None);
        app.run(Command::Edit(a.clone()));
        app.run(Command::BarNext);
        app.run(Command::Layout(Some("page".into())));
        app.run(Command::VSplit);
        app.run(Command::Edit(b.clone()));
        app.run(Command::Waveform);
        app.run(Command::TabNew(Some(a.clone())));
        app.zoom = 1.3;
        let ws_file = dir.join("session.toml");
        app.run(Command::WorkspaceSave(Some(ws_file.display().to_string())));
        let saved = app.workspace();

        let mut other = App::new(Config::default(), None);
        other.run(Command::WorkspaceOpen(ws_file.display().to_string(), false));
        assert_eq!(other.workspace(), saved);
        assert_eq!(other.tabs.len(), 2);
        assert_eq!(other.workspace_file.as_deref(), Some(ws_file.as_path()));
        // Changes are saved automatically.
        other.run(Command::TabPrev);
        other.autosave_workspace();
        let reread: Workspace =
            toml::from_str(&std::fs::read_to_string(&ws_file).unwrap()).unwrap();
        assert_eq!(reread.active_tab, 0);
        // Unsaved changes stop an open unless forced.
        other.buffers[0].modified = true;
        other.run(Command::WorkspaceOpen(ws_file.display().to_string(), false));
        assert!(other.message.as_ref().unwrap().error);
        std::fs::remove_dir_all(dir).ok();
    }
}
