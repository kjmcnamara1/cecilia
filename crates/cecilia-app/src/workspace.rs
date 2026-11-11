//! Workspaces: a session's views — each with its pane splits and its bufferline of loaded
//! files — and each pane's file, cursor, recent files and view settings, saved to a TOML file so
//! the session can be reopened as it was. A workspace is saved by `:workspace-save`, when
//! switching to another and (by default) when the app quits.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::app::{App, Buffer, Cursor, Pane, PaneKind, View};
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
    pub global_waveform: bool,
    #[serde(default)]
    pub tracks_panel: bool,
    #[serde(default = "default_true")]
    pub toolbar: bool,
    #[serde(default)]
    pub fretboard: bool,
    #[serde(default)]
    pub piano: bool,
    /// Tracks the tracks panel shows at once.
    #[serde(default = "default_tracks_rows")]
    pub tracks_rows: usize,
    /// Height of waveform strips and bar (GUI, logical pixels).
    #[serde(default = "default_waveform_height")]
    pub waveform_height: u16,
    /// Tab of the properties pane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties_tab: Option<String>,
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
    /// The score pane tool panes work on (the focused pane when absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<PaneId>,
    pub split: Split,
    /// The bufferline, as indices into `files` (absent in version 1: the files shown).
    #[serde(default)]
    pub buffers: Vec<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The view shows the dashboard (it has no files).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dashboard: bool,
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
    /// The children's shares of the split (left out when equal).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weights: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaneState {
    pub id: PaneId,
    /// What the pane shows: "score" (left out), "tracks", "audio", "piano", "fretboard" or
    /// "tuner".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// A tool pane's own height and width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u16>,
    /// A local tool pane's score pane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<PaneId>,
    /// The size was set by hand.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub manual: bool,
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
    /// The pane's zoom (older workspaces had one for all).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom: Option<f64>,
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
            weights: Vec::new(),
        },
        Node::Split {
            axis,
            children,
            weights,
        } => Split {
            weights: if weights.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-4) {
                Vec::new()
            } else {
                let sum: f32 = weights.iter().sum();
                // Rounded, to keep the file tidy.
                weights
                    .iter()
                    .map(|w| (w / sum * 1000.0).round() / 1000.0)
                    .collect()
            },
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
    let (mut children, weights): (Vec<Node>, Vec<f32>) = split
        .children
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let w = split.weights.get(i).copied().filter(|w| *w > 0.0);
            Some((
                node_of(c, panes)?,
                w.unwrap_or(1.0 / split.children.len() as f32),
            ))
        })
        .unzip();
    match children.len() {
        0 => None,
        1 => children.pop(),
        _ => Some(Node::Split {
            axis,
            children,
            weights,
        }),
    }
}

fn default_true() -> bool {
    true
}

fn default_tracks_rows() -> usize {
    crate::app::DEFAULT_TRACKS_ROWS
}

fn default_waveform_height() -> u16 {
    crate::app::DEFAULT_WAVEFORM_HEIGHT
}

/// Where named workspaces are kept.
pub fn workspace_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "cecilia").map(|d| d.data_dir().join("workspaces"))
}

/// The workspace used when none was chosen.
pub const DEFAULT_WORKSPACE: &str = "default";

/// The file recording the last active workspace.
fn last_workspace_marker() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "cecilia").map(|d| d.data_dir().join("last-workspace"))
}

/// The workspace active when cecilia last ran (as a path), if it still exists.
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
            Some(rest) => crate::config::home_dir()?.join(rest),
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
        staff: Cursor::HOME,
        line: 0,
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
                score: Some(v.score),
                split: split_of(&v.root),
                buffers,
                name: v.name.clone(),
                dashboard: v.dashboard,
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
                    kind: (p.kind != PaneKind::Score).then(|| p.kind.name().to_string()),
                    height: p.size.0,
                    width: p.size.1,
                    owner: p.owner,
                    manual: p.manual_size,
                    file: add(p.buffer, &mut files, self),
                    track: p.cursor.track,
                    bar: p.cursor.bar,
                    beat: p.cursor.beat,
                    string: p.cursor.string,
                    voice: p.cursor.voice,
                    layout: p.layout.clone(),
                    waveform: p.waveform,
                    zoom: Some(p.zoom),
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
        // Files that could not be opened stay in the workspace (nothing refers to them).
        for (path, _) in &self.unopened_files {
            if !files.contains(path) {
                files.push(path.clone());
            }
        }
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
            zoom: self.pane().zoom,
            // Tool panes are saved as panes; these older switches stay off.
            global_waveform: false,
            tracks_panel: false,
            toolbar: self.toolbar,
            fretboard: false,
            piano: false,
            tracks_rows: default_tracks_rows(),
            waveform_height: self.waveform_height,
            properties_tab: Some(self.props.tab.name().to_string()),
        }
    }

    /// Saves the session as a workspace and makes it the active one. With a name (or path) it is
    /// saved under that name, leaving the file of the workspace it came from as it is; without,
    /// to the active workspace's file. An existing, different workspace of that name is only
    /// overwritten with `force`.
    pub fn save_workspace(&mut self, target: Option<&str>, force: bool) {
        let path = match target {
            Some(t) => {
                if !is_path_like(t) && !valid_workspace_name(t.trim()) {
                    return self.error(format!("workspace-save: \"{t}\" is not a valid name"));
                }
                workspace_path(t)
            }
            None => self.workspace_file.clone(),
        };
        let Some(path) = path else {
            return self.error("workspace-save: give a name (no workspace is active)");
        };
        if !force && self.workspace_file.as_ref() != Some(&path) && path.exists() {
            return self.error(format!(
                "workspace-save: {} already exists (:workspace-save! overwrites it)",
                workspace_label(&path)
            ));
        }
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
        crate::swap::write_atomic(path, text.as_bytes())
            .map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Saves the active workspace before the app quits (`ui.workspace_autosave_on_quit`); does
    /// nothing without an active workspace.
    pub(crate) fn autosave_workspace_on_quit(&mut self) {
        if self.config.file.ui.workspace_autosave_on_quit && self.workspace_file.is_some() {
            self.save_workspace(None, false);
        }
    }

    fn set_workspace_file(&mut self, path: PathBuf) {
        if self.remember_workspace
            && let Some(marker) = last_workspace_marker()
        {
            let abs = std::path::absolute(&path).unwrap_or_else(|_| path.clone());
            std::fs::write(marker, abs.display().to_string()).ok();
        }
        self.workspace_file = Some(path);
    }

    /// Switches to a workspace, creating it (empty) if it doesn't exist. The workspace being
    /// left is saved first, unless `discard` (`:workspace-open!`), which also drops unsaved
    /// changes in files; without it those stop the switch.
    pub fn open_workspace(&mut self, name: &str, discard: bool) {
        if !is_path_like(name) && !valid_workspace_name(name.trim()) {
            return self.error(format!("workspace-open: \"{name}\" is not a valid name"));
        }
        let Some(path) = workspace_path(name) else {
            return self.error("workspace-open: give a name or path");
        };
        if !discard && let Some(b) = self.buffers.iter().find(|b| b.modified) {
            return self.error(format!(
                "workspace-open: \"{}\" has unsaved changes (:w, or :workspace-open! to discard)",
                b.name
            ));
        }
        let ws: Option<Workspace> = if path.exists() {
            match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| toml::from_str(&t).map_err(|e| e.to_string()))
            {
                Ok(ws) => Some(ws),
                Err(e) => return self.error(format!("workspace-open: {}: {e}", path.display())),
            }
        } else {
            None
        };
        if !discard && self.workspace_file.is_some() {
            let errors = self.notifications.errors();
            self.save_workspace(None, false);
            if self.notifications.errors() != errors {
                return;
            }
        }
        let created = ws.is_none();
        let ws = ws.unwrap_or_else(|| App::new(self.config.clone(), None).workspace());
        self.restore(&ws);
        // A workspace with nothing open starts on the dashboard.
        if ws.files.is_empty() {
            self.show_dashboard();
        }
        self.set_workspace_file(path.clone());
        self.used(crate::frecency::Domain::Workspaces, &workspace_label(&path));
        if created {
            if let Err(e) = self.write_workspace(&path) {
                return self.error(format!("workspace-open: {e}"));
            }
            return self.info(format!("workspace created: {}", workspace_label(&path)));
        }
        let missing = ws.files.iter().filter(|f| !f.exists()).count();
        if !self.unopened_files.is_empty() {
            let list: Vec<String> = self.unopened_files.iter().map(|(_, e)| e.clone()).collect();
            self.error(format!(
                "workspace {}: {} file(s) could not be opened (kept in the workspace): {}",
                workspace_label(&path),
                list.len(),
                list.join("; ")
            ));
        } else if missing > 0 {
            self.error(format!(
                "workspace {}: {missing} file(s) no longer exist",
                workspace_label(&path)
            ));
        } else {
            self.info(format!("workspace opened: {}", workspace_label(&path)));
        }
    }

    /// Deletes a workspace file: the named one, or the active one (leaving the session as it is
    /// but with no workspace). Asks first unless `force`; without a name and an active
    /// workspace it does nothing.
    pub fn delete_workspace(&mut self, name: Option<&str>, force: bool) {
        let path = match name {
            Some(n) => match workspace_path(n) {
                Some(p) if p.exists() || self.workspace_file.as_ref() == Some(&p) => p,
                Some(_) => return self.error(format!("workspace-delete: no workspace \"{n}\"")),
                None => return self.error("workspace-delete: give a name or path"),
            },
            None => match self.workspace_file.clone() {
                Some(p) => p,
                None => return,
            },
        };
        // Only workspace files go: a path argument could name any file.
        let in_dir = workspace_dir().is_some_and(|d| path.parent() == Some(d.as_path()));
        if self.workspace_file.as_ref() != Some(&path)
            && !in_dir
            && path.exists()
            && !is_workspace_file(&path)
        {
            return self.error(format!(
                "workspace-delete: {} is not a workspace file",
                path.display()
            ));
        }
        if force {
            return self.remove_workspace(&path);
        }
        self.ask(
            format!("delete workspace {}? (y/n)", workspace_label(&path)),
            &[
                ("Delete", crate::keys::KeyCode::Char('y')),
                ("Cancel", crate::keys::KeyCode::Esc),
            ],
        );
        self.confirm_delete_workspace = Some(path);
    }

    /// The answer to the delete prompt: `y` deletes, any other key cancels.
    pub(crate) fn answer_delete_workspace(&mut self, path: PathBuf, key: crate::keys::Key) {
        self.answered();
        if matches!(key.code, crate::keys::KeyCode::Char('y' | 'Y')) {
            self.remove_workspace(&path);
        } else {
            self.info("cancelled");
        }
    }

    fn remove_workspace(&mut self, path: &Path) {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return self.error(format!("workspace-delete: {}: {e}", path.display())),
        }
        if self.workspace_file.as_deref() == Some(path) {
            self.workspace_file = None;
        }
        self.info(format!("workspace deleted: {}", workspace_label(path)));
    }

    pub(crate) fn restore(&mut self, ws: &Workspace) {
        // Every index is about to mean something else.
        for b in 0..self.buffers.len().max(1) {
            self.buffer_replaced(b);
        }
        self.buffers = vec![Buffer::empty()];
        self.panes.clear();
        self.views.clear();
        // Open the files; the first replaces the empty buffer.
        let mut buffer_of = HashMap::new();
        self.unopened_files.clear();
        for (i, f) in ws.files.iter().enumerate() {
            match self.open(f) {
                Ok(b) => {
                    buffer_of.insert(i, b);
                }
                // A file that is gone is dropped (and reported); one that exists but won't open
                // stays in the workspace.
                Err(e) if f.exists() => self.unopened_files.push((f.clone(), e)),
                Err(_) => {}
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
            // A pane kind this build doesn't know (a workspace saved by a newer one) is dropped,
            // not shown as a score pane.
            if p.kind
                .as_deref()
                .is_some_and(|k| PaneKind::parse(k).is_none())
            {
                continue;
            }
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
                staff: Cursor::HOME,
                line: 0,
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
                    kind: p
                        .kind
                        .as_deref()
                        .and_then(PaneKind::parse)
                        .unwrap_or_default(),
                    size: (p.height, p.width),
                    manual_size: p.manual,
                    owner: p.owner,
                    buffer,
                    cursor,
                    scroll: 0,
                    layout: p.layout.clone(),
                    waveform: p.waveform,
                    zoom: p.zoom.unwrap_or(ws.zoom),
                    recent,
                    jump_back: None,
                    scroll_request: None,
                    wave_zoom: None,
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
            let is_score = |id: &PaneId| self.panes[id].kind == PaneKind::Score;
            // A view needs a score pane for its tools to work on.
            let Some(first_score) = ids.iter().copied().find(is_score) else {
                continue;
            };
            let score = v
                .score
                .filter(|id| ids.contains(id) && is_score(id))
                .or(Some(focus).filter(is_score))
                .unwrap_or(first_score);
            let mut buffers: Vec<usize> = v
                .buffers
                .iter()
                .filter_map(|f| buffer_of.get(f).copied())
                .collect();
            // Every buffer a pane shows is on the view's bufferline (version 1 had no list);
            // a dashboard view has none.
            for id in ids.iter().filter(|_| !v.dashboard) {
                let b = self.panes[id].buffer;
                if !buffers.contains(&b) {
                    buffers.push(b);
                }
            }
            self.views.push(View {
                maximized: None,
                dashboard: v.dashboard,
                root,
                focus,
                score,
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
                    kind: PaneKind::Score,
                    size: (None, None),
                    manual_size: false,
                    owner: None,
                    buffer: 0,
                    cursor: Cursor::default(),
                    scroll: 0,
                    layout,
                    waveform: true,
                    zoom: ws.zoom,
                    recent: Vec::new(),
                    jump_back: None,
                    scroll_request: None,
                    wave_zoom: None,
                },
            );
            self.views.push(View {
                maximized: None,
                dashboard: false,
                root: Node::Leaf(1),
                focus: 1,
                score: 1,
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
                staff: Cursor::HOME,
                line: 0,
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
        self.toolbar = ws.toolbar;
        if let Some(tab) = ws
            .properties_tab
            .as_deref()
            .and_then(crate::properties::PropTab::parse)
        {
            self.set_property_tab(tab);
        }
        self.size_tools();
        // Workspaces from before tool panes switched the panels on: they become panes.
        for (kind, on) in [
            (PaneKind::Tracks, ws.tracks_panel),
            (PaneKind::Fretboard, ws.fretboard),
            (PaneKind::Piano, ws.piano),
            (PaneKind::Audio, ws.global_waveform),
        ] {
            if on {
                self.set_tool(kind, Some(true));
            }
        }
        self.normalize_layouts();
        self.dashboard_focus = 0;
        self.waveform_height = ws.waveform_height.clamp(
            crate::app::MIN_WAVEFORM_HEIGHT,
            crate::app::MAX_WAVEFORM_HEIGHT,
        );
    }
}

impl App {
    /// Name of the active workspace, for the status line.
    pub fn workspace_name(&self) -> Option<String> {
        self.workspace_file.as_deref().map(workspace_label)
    }
}

/// A plain workspace name: no path separators, not hidden, no extension.
fn valid_workspace_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\'])
        && !name.ends_with(".toml")
}

/// Whether a file is a workspace: a `.toml` that parses as one.
fn is_workspace_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "toml")
        && std::fs::read_to_string(path).is_ok_and(|t| toml::from_str::<Workspace>(&t).is_ok())
}

/// Whether a workspace argument is a path rather than a name.
fn is_path_like(arg: &str) -> bool {
    arg.contains('/') || arg.ends_with(".toml")
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
        let dir = std::env::temp_dir().join(format!("cecilia-ws-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Three small scores to open.
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        let (a, b, c) = (dir.join("a.ccm"), dir.join("b.ccm"), dir.join("c.ccm"));
        for f in [&a, &b, &c] {
            app.run(Command::Write(Some(f.clone())));
        }
        let mut app = App::new(Config::default(), None);
        app.run(Command::Edit(a.clone()));
        app.run(Command::BarNext);
        app.run(Command::Layout(Some("page".into())));
        // The new pane has the focus: it grows.
        app.run(Command::VSplit);
        app.run(Command::Edit(b.clone()));
        app.run(Command::Waveform);
        app.run(Command::ViewNew(Some(c.clone())));
        app.run(Command::ViewRename(Some("practice".into())));
        app.pane_mut().zoom = 1.3;
        // Marks and macros are kept too.
        app.run(Command::Mark('q'));
        app.run(Command::Mark('Z'));
        for k in crate::keys::parse_keys("qa}<C-w>vq", &[crate::keys::Key::char(' ')]).unwrap() {
            app.handle_key(k);
        }
        let ws_file = dir.join("session.toml");
        app.run(Command::WorkspaceSave(
            Some(ws_file.display().to_string()),
            false,
        ));
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
        // Saving without a name writes the active workspace again.
        other.run(Command::ViewPrev);
        other.run(Command::WorkspaceSave(None, false));
        let reread: Workspace =
            toml::from_str(&std::fs::read_to_string(&ws_file).unwrap()).unwrap();
        assert_eq!(reread.active_view, 0);
        // Unsaved changes stop an open unless forced.
        other.buffers[0].modified = true;
        other.run(Command::WorkspaceOpen(ws_file.display().to_string(), false));
        assert!(other.last_notification().unwrap().error);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn panes_of_an_unknown_kind_are_dropped() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::Fretboard(Some(true)));
        let mut ws = app.workspace();
        let before = ws.panes.len();
        assert_eq!(before, 2);
        // A tool pane from some newer build.
        let tool = ws.panes.iter_mut().find(|p| p.kind.is_some()).unwrap();
        tool.kind = Some("from-the-future".into());
        let mut other = App::new(Config::default(), None);
        other.restore(&ws);
        assert_eq!(other.panes.len(), before - 1);
        assert!(other.panes.values().all(|p| p.kind == PaneKind::Score));
        assert_eq!(other.views[0].root.panes().len(), 1);
    }

    #[test]
    fn pane_and_panel_sizes_are_kept() {
        let mut app = App::new(Config::default(), None);
        app.set_tool(PaneKind::Properties, Some(false));
        app.viewport = crate::layout::Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 50,
        };
        // The new pane has the focus: it grows.
        app.run(Command::VSplit);
        app.run(Command::Resize(
            crate::layout::Axis::SideBySide,
            crate::command::Amount::By(20),
        ));
        app.run(Command::WaveformResize(crate::command::Amount::By(36)));
        let widths = |app: &App| app.layout().iter().map(|(_, r)| r.w).collect::<Vec<_>>();
        assert_eq!(widths(&app), [30, 70]);
        let text = toml::to_string_pretty(&app.workspace()).unwrap();
        let mut other = App::new(Config::default(), None);
        other.viewport = app.viewport;
        other.restore(&toml::from_str(&text).unwrap());
        assert_eq!(widths(&other), [30, 70]);
        assert_eq!(other.waveform_height, 100);
        // <C-w>= evens them out again.
        for k in crate::keys::parse_keys("<C-w>=", &[]).unwrap() {
            other.handle_key(k);
        }
        assert_eq!(widths(&other), [50, 50]);
    }

    #[test]
    fn tool_panes_are_saved_and_old_switches_become_panes() {
        use crate::app::PaneKind;
        let mut app = App::new(Config::default(), None);
        app.run(Command::Tracks(Some(true)));
        app.run(Command::Fretboard(Some(true)));
        let text = toml::to_string_pretty(&app.workspace()).unwrap();
        let mut other = App::new(Config::default(), None);
        other.restore(&toml::from_str(&text).unwrap());
        assert!(other.has_tool(PaneKind::Tracks) && other.has_tool(PaneKind::Fretboard));
        assert_eq!(other.focused_kind(), PaneKind::Score);
        // A workspace from before tool panes, with the tracks panel switched on.
        let mut old: Workspace = toml::from_str(&text).unwrap();
        old.panes.retain(|p| p.kind.is_none());
        old.views[0].split = Split {
            pane: Some(old.panes[0].id),
            axis: None,
            children: Vec::new(),
            weights: Vec::new(),
        };
        old.tracks_panel = true;
        let mut third = App::new(Config::default(), None);
        third.restore(&old);
        assert!(third.has_tool(PaneKind::Tracks) && !third.has_tool(PaneKind::Fretboard));
    }

    #[test]
    fn the_properties_pane_and_its_tab_are_saved() {
        let mut app = App::new(Config::default(), None);
        assert!(!app.has_tool(PaneKind::Properties), "off by default");
        app.set_tool(PaneKind::Properties, Some(true));
        app.set_property_tab(crate::properties::PropTab::Lyrics);
        let text = toml::to_string_pretty(&app.workspace()).unwrap();
        let mut other = App::new(Config::default(), None);
        other.restore(&toml::from_str(&text).unwrap());
        assert!(other.has_tool(PaneKind::Properties));
        assert_eq!(other.props.tab, crate::properties::PropTab::Lyrics);
        // Hidden in the saved session: hidden again.
        app.set_tool(PaneKind::Properties, Some(false));
        let text = toml::to_string_pretty(&app.workspace()).unwrap();
        other.restore(&toml::from_str(&text).unwrap());
        assert!(!other.has_tool(PaneKind::Properties));
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cecilia-ws-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// How many panes the first view of a saved workspace has.
    fn panes_saved(file: &str) -> usize {
        let mut app = App::new(Config::default(), None);
        app.restore(&toml::from_str(&std::fs::read_to_string(file).unwrap()).unwrap());
        app.views[0].root.panes().len()
    }

    fn path_arg(dir: &Path, name: &str) -> String {
        dir.join(format!("{name}.toml")).display().to_string()
    }

    #[test]
    fn opening_creates_a_missing_workspace_and_saves_the_one_left() {
        let dir = scratch("open");
        let (a, b) = (path_arg(&dir, "a"), path_arg(&dir, "b"));
        let mut app = App::new(Config::default(), None);
        // From no workspace: a new, empty one, written at once.
        app.run(Command::WorkspaceOpen(a.clone(), false));
        assert_eq!(app.workspace_name().as_deref(), Some(a.as_str()));
        assert!(Path::new(&a).exists());
        assert!(app.dashboard_visible());
        // Change it, then switch: it is saved first, and `b` starts empty.
        app.run(Command::Split);
        assert_eq!(app.views[0].root.panes().len(), 2);
        app.run(Command::WorkspaceOpen(b.clone(), false));
        assert_eq!(app.views[0].root.panes().len(), 1);
        assert_eq!(panes_saved(&a), 2);
        // Back again: the saved one comes back.
        app.run(Command::WorkspaceOpen(a.clone(), false));
        assert_eq!(app.views[0].root.panes().len(), 2);
        // `!` does not save what it leaves.
        app.run(Command::Split);
        app.run(Command::WorkspaceOpen(b.clone(), true));
        app.run(Command::WorkspaceOpen(a.clone(), false));
        assert_eq!(app.views[0].root.panes().len(), 2);
        // A bad name is refused, and nothing changes.
        app.run(Command::WorkspaceOpen(".x".into(), false));
        assert!(app.last_notification().unwrap().error);
        assert_eq!(app.workspace_name().as_deref(), Some(a.as_str()));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn unsaved_files_stop_a_switch_unless_discarded() {
        let dir = scratch("guard");
        let (a, b) = (path_arg(&dir, "a"), path_arg(&dir, "b"));
        let mut app = App::new(Config::default(), None);
        app.run(Command::WorkspaceOpen(a.clone(), false));
        app.buffers[0].modified = true;
        app.run(Command::WorkspaceOpen(b.clone(), false));
        assert!(app.last_notification().unwrap().error);
        assert_eq!(app.workspace_name().as_deref(), Some(a.as_str()));
        app.run(Command::WorkspaceOpen(b.clone(), true));
        assert_eq!(app.workspace_name().as_deref(), Some(b.as_str()));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn saving_under_a_name_duplicates_and_refuses_to_overwrite() {
        let dir = scratch("save");
        let (a, b) = (path_arg(&dir, "a"), path_arg(&dir, "b"));
        let mut app = App::new(Config::default(), None);
        // No workspace and no name: nothing to save to.
        app.run(Command::WorkspaceSave(None, false));
        assert!(app.last_notification().unwrap().error && app.workspace_file.is_none());
        app.run(Command::WorkspaceSave(Some(a.clone()), false));
        app.run(Command::Split);
        // A new name duplicates: `a` stays as it was, `b` is active.
        app.run(Command::WorkspaceSave(Some(b.clone()), false));
        assert_eq!(app.workspace_name().as_deref(), Some(b.as_str()));
        assert_eq!(panes_saved(&a), 1);
        // Another name for the same session may not take over an existing workspace...
        app.run(Command::WorkspaceSave(Some(a.clone()), false));
        assert!(app.last_notification().unwrap().error);
        assert_eq!(app.workspace_name().as_deref(), Some(b.as_str()));
        // ...unless forced. Saving to the active one needs no `!`.
        app.run(Command::WorkspaceSave(Some(b.clone()), false));
        assert!(!app.last_notification().unwrap().error);
        app.run(Command::WorkspaceSave(Some(a.clone()), true));
        assert_eq!(app.workspace_name().as_deref(), Some(a.as_str()));
        assert_eq!(panes_saved(&a), 2);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn deleting_asks_then_leaves_no_workspace() {
        use crate::keys::{Key, KeyCode};
        let dir = scratch("delete");
        let (a, b) = (path_arg(&dir, "a"), path_arg(&dir, "b"));
        let mut app = App::new(Config::default(), None);
        // Nothing active and no name: nothing happens.
        app.run(Command::WorkspaceDelete(None, false));
        assert!(app.confirm_delete_workspace.is_none());
        app.run(Command::WorkspaceOpen(a.clone(), false));
        app.run(Command::WorkspaceOpen(b.clone(), false));
        // A named one that is not the active one: asked, and `n` keeps it.
        app.run(Command::WorkspaceDelete(Some(a.clone()), false));
        assert!(app.confirm_delete_workspace.is_some());
        app.handle_key(Key::new(KeyCode::Char('n')));
        assert!(Path::new(&a).exists() && app.confirm_delete_workspace.is_none());
        app.run(Command::WorkspaceDelete(Some(a.clone()), false));
        app.handle_key(Key::new(KeyCode::Char('y')));
        assert!(!Path::new(&a).exists());
        assert_eq!(app.workspace_name().as_deref(), Some(b.as_str()));
        // A name that does not exist is an error.
        app.run(Command::WorkspaceDelete(Some(a.clone()), true));
        assert!(app.last_notification().unwrap().error);
        // The active one, without asking: the session stays, with no workspace.
        app.run(Command::WorkspaceDelete(None, true));
        assert!(!Path::new(&b).exists() && app.workspace_file.is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn quitting_saves_the_workspace_unless_forced_or_switched_off() {
        let dir = scratch("quit");
        let a = path_arg(&dir, "a");
        let quit = |force: bool, on: bool| {
            let mut app = App::new(Config::default(), None);
            app.config.file.ui.workspace_autosave_on_quit = on;
            app.run(Command::WorkspaceOpen(a.clone(), false));
            std::fs::remove_file(&a).unwrap();
            app.run(Command::Quit { force });
            assert!(app.quit);
            Path::new(&a).exists()
        };
        assert!(quit(false, true));
        std::fs::remove_file(&a).unwrap();
        assert!(!quit(true, true));
        assert!(!quit(false, false));
        // No workspace: nothing is written.
        let mut app = App::new(Config::default(), None);
        app.run(Command::Quit { force: false });
        assert!(app.quit && app.workspace_file.is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_workspace_commands_parse() {
        use crate::picker::PickerKind;
        let p = |s| Command::parse(s);
        assert_eq!(
            p("workspace-open"),
            Ok(Command::Pick(PickerKind::Workspace))
        );
        assert_eq!(
            p("workspace-open!"),
            Ok(Command::Pick(PickerKind::WorkspaceDiscard))
        );
        assert_eq!(
            p("workspace-open x"),
            Ok(Command::WorkspaceOpen("x".into(), false))
        );
        assert_eq!(
            p("workspace-open! x"),
            Ok(Command::WorkspaceOpen("x".into(), true))
        );
        assert_eq!(p("workspace-save"), Ok(Command::WorkspaceSave(None, false)));
        assert_eq!(
            p("workspace-save! x"),
            Ok(Command::WorkspaceSave(Some("x".into()), true))
        );
        assert_eq!(
            p("workspace-delete"),
            Ok(Command::WorkspaceDelete(None, false))
        );
        assert_eq!(
            p("workspace-delete! x"),
            Ok(Command::WorkspaceDelete(Some("x".into()), true))
        );
        for gone in ["workspace-close", "workspace-new", "workspace-rename x"] {
            assert!(p(gone).is_err(), "{gone}");
        }
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

    #[test]
    fn a_workspace_switch_drops_what_was_cached_for_the_old_buffers() {
        let dir = scratch("stale");
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        let (a, b) = (dir.join("a.ccm"), dir.join("b.ccm"));
        app.run(Command::Write(Some(a.clone())));
        app.run(Command::New);
        app.run(Command::Write(Some(b.clone())));
        let ws_a = Workspace {
            files: vec![a.clone()],
            ..workspace_of(&a)
        };
        let ws_b = workspace_of(&b);
        let mut app = App::new(Config::default(), None);
        app.restore(&ws_a);
        let mut playback = crate::playback::Playback::default();
        let old = playback.sequence(&app, 0);
        app.playing = true;
        app.playing_buffer = Some(0);
        app.restore(&ws_b);
        assert!(
            !app.playing && app.playing_buffer.is_none(),
            "playback stops"
        );
        for e in app.take_effects() {
            if matches!(e, crate::Effect::BufferReplaced { .. }) {
                playback.apply(&mut app, e);
            }
        }
        let new = playback.sequence(&app, 0);
        assert!(
            !std::sync::Arc::ptr_eq(&old, &new),
            "the old sequence was reused"
        );
        std::fs::remove_dir_all(dir).ok();
    }

    /// A workspace holding just `file`.
    fn workspace_of(file: &Path) -> Workspace {
        let mut app = App::new(Config::default(), None);
        app.open(file).unwrap();
        app.workspace()
    }

    #[test]
    fn workspace_delete_refuses_files_that_are_not_workspaces() {
        let dir = scratch("delete");
        let song = dir.join("song.ccm");
        std::fs::write(&song, "not a workspace").unwrap();
        let other = dir.join("notes.toml");
        std::fs::write(&other, "x = 1\n").unwrap();
        let mut app = App::new(Config::default(), None);
        for f in [&song, &other] {
            app.delete_workspace(Some(&f.display().to_string()), true);
            assert!(f.exists(), "{} was deleted", f.display());
            assert!(app.last_notification().unwrap().error);
        }
        // A real workspace file goes.
        let ws = dir.join("mine.toml");
        std::fs::write(
            &ws,
            toml::to_string_pretty(&App::new(Config::default(), None).workspace()).unwrap(),
        )
        .unwrap();
        app.delete_workspace(Some(&ws.display().to_string()), true);
        assert!(!ws.exists());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn files_that_exist_but_cannot_open_stay_in_the_workspace() {
        let dir = scratch("unopened");
        let good = dir.join("good.ccm");
        let bad = dir.join("bad.ccm");
        let gone = dir.join("gone.ccm");
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.run(Command::Write(Some(good.clone())));
        let mut ws = workspace_of(&good);
        ws.files = vec![good.clone(), bad.clone(), gone.clone()];
        std::fs::write(&bad, "garbage, not a score").unwrap();
        let mut app = App::new(Config::default(), None);
        app.restore(&ws);
        assert_eq!(app.unopened_files.len(), 1);
        let saved = app.workspace().files;
        assert!(saved.contains(&good) && saved.contains(&bad), "{saved:?}");
        assert!(!saved.contains(&gone), "a file that is gone is dropped");
        // Opening reports it.
        let path = dir.join("ws.toml");
        std::fs::write(&path, toml::to_string_pretty(&ws).unwrap()).unwrap();
        let mut app = App::new(Config::default(), None);
        app.open_workspace(&path.display().to_string(), true);
        let n = app.last_notification().unwrap();
        assert!(
            n.error && n.text.contains("could not be opened"),
            "{}",
            n.text
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_workspace_is_written_atomically() {
        let dir = scratch("atomic");
        let path = dir.join("w.toml");
        let mut app = App::new(Config::default(), None);
        app.write_workspace(&path).unwrap();
        assert!(path.exists());
        assert!(!dir.join("w.toml.tmp").exists(), "temp file left behind");
        std::fs::remove_dir_all(dir).ok();
    }
}
