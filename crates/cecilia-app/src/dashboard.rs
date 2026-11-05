//! The dashboard: what fills the window when no score pane is open (all score panes closed with
//! `:pane-close`, or a workspace with nothing in it). A column of buttons that run existing
//! commands; the frontends draw it and report clicks.

use crate::app::App;
use crate::command::Command;
use crate::config::Mode;
use crate::keys::{Key, KeyCode};
use crate::picker::PickerKind;
use std::path::PathBuf;

/// A button of the dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardButton {
    OpenRecent,
    SelectWorkspace,
    NewScore,
    OpenScore,
}

impl DashboardButton {
    /// Every button, top to bottom.
    pub const ALL: [DashboardButton; 4] = [
        DashboardButton::OpenRecent,
        DashboardButton::SelectWorkspace,
        DashboardButton::NewScore,
        DashboardButton::OpenScore,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DashboardButton::OpenRecent => "Open Recent",
            DashboardButton::SelectWorkspace => "Select Workspace",
            DashboardButton::NewScore => "New Score",
            DashboardButton::OpenScore => "Open Score",
        }
    }

    /// The command the button runs.
    pub fn command(self) -> Command {
        match self {
            DashboardButton::OpenRecent => Command::Pick(PickerKind::Recent),
            DashboardButton::SelectWorkspace => Command::Pick(PickerKind::Workspace),
            DashboardButton::NewScore => Command::New,
            DashboardButton::OpenScore => Command::OpenDialog,
        }
    }
}

impl App {
    /// Whether the dashboard replaces the panes.
    pub fn dashboard_visible(&self) -> bool {
        self.view().dashboard
    }

    /// Whether the tabline is drawn: not over the dashboard of a lone view, which has no tabs
    /// (with several views it still shows them, to switch between).
    pub fn tabline_visible(&self) -> bool {
        !(self.dashboard_visible() && self.views.len() == 1)
    }

    /// Whether button `index` can be used: Open Recent needs a recent file.
    pub fn dashboard_enabled(&self, index: usize) -> bool {
        match DashboardButton::ALL.get(index) {
            Some(DashboardButton::OpenRecent) => !self.recent_files().is_empty(),
            Some(_) => true,
            None => false,
        }
    }

    /// The focused button (an index into [`DashboardButton::ALL`]): the first enabled one when
    /// the one last focused cannot be used (any more).
    pub fn dashboard_focus(&self) -> usize {
        if self.dashboard_enabled(self.dashboard_focus) {
            return self.dashboard_focus;
        }
        (0..DashboardButton::ALL.len())
            .find(|&i| self.dashboard_enabled(i))
            .unwrap_or(0)
    }

    /// Shows the dashboard with the first enabled button focused.
    pub(crate) fn show_dashboard(&mut self) {
        let view = &mut self.views[self.active_view];
        view.dashboard = true;
        // No score is open: the placeholder behind the dashboard is not a tab.
        view.buffers.clear();
        self.dashboard_focus = 0;
    }

    /// Moves the button focus by `delta` enabled buttons, wrapping around.
    pub fn dashboard_move(&mut self, delta: isize) {
        let n = DashboardButton::ALL.len() as isize;
        let mut at = self.dashboard_focus() as isize;
        for _ in 0..n {
            at = (at + delta.signum()).rem_euclid(n);
            if self.dashboard_enabled(at as usize) {
                self.dashboard_focus = at as usize;
                return;
            }
        }
    }

    /// Runs button `index` (Enter on it, or a click); a disabled one does nothing. The
    /// dashboard goes once a score shows.
    pub fn dashboard_activate(&mut self, index: usize) {
        let Some(button) = DashboardButton::ALL.get(index) else {
            return;
        };
        if !self.dashboard_enabled(index) {
            return;
        }
        self.dashboard_focus = index;
        self.run(button.command());
    }

    /// The recently closed files that still exist, the most recent first.
    pub fn recent_files(&self) -> Vec<PathBuf> {
        self.recent.iter().filter(|p| p.exists()).cloned().collect()
    }

    /// Notes a closed file as the most recent one, and keeps the list for the next start.
    pub(crate) fn remember_closed(&mut self, path: Option<PathBuf>) {
        let Some(path) = path else {
            return;
        };
        self.recent.retain(|p| *p != path);
        self.recent.insert(0, path);
        self.recent.truncate(MAX_RECENT);
        if self.remember_workspace {
            if let Some(file) = recent_file() {
                let text: Vec<String> = self
                    .recent
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect();
                if let Some(dir) = file.parent() {
                    std::fs::create_dir_all(dir).ok();
                }
                std::fs::write(file, text.join("\n")).ok();
            }
        }
    }

    /// Reads the recent files kept by earlier sessions.
    pub fn load_recent(&mut self) {
        let Some(text) = recent_file().and_then(|f| std::fs::read_to_string(f).ok()) else {
            return;
        };
        self.recent = text
            .lines()
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .take(MAX_RECENT)
            .collect();
    }

    /// Takes a key while the dashboard shows. Returns false when it is not the dashboard's:
    /// the key then goes the usual way (`:` opens the command line, `g` starts `gt`; pickers
    /// and the command line take their own keys).
    pub(crate) fn dashboard_key(&mut self, key: Key) -> bool {
        if !self.dashboard_visible()
            || self.mode != Mode::Normal
            || self.picker.is_some()
            || self.show_help
            || !self.pending.is_empty()
        {
            return false;
        }
        let plain = !key.ctrl && !key.alt && !key.sup;
        match key.code {
            KeyCode::Char(':') if plain => return false,
            KeyCode::Down | KeyCode::Tab | KeyCode::Char('j') => self.dashboard_move(1),
            KeyCode::Up | KeyCode::BackTab | KeyCode::Char('k') => self.dashboard_move(-1),
            KeyCode::Home => {
                self.dashboard_focus = 0;
                self.dashboard_focus = self.dashboard_focus();
            }
            KeyCode::End => self.dashboard_focus = DashboardButton::ALL.len() - 1,
            // `g` starts `gt` / `gT`, to move to another view; the keys after it go the usual way.
            KeyCode::Char('g') if plain => return false,
            KeyCode::Enter | KeyCode::Char(' ') => self.dashboard_activate(self.dashboard_focus()),
            // Other keys would edit the score hidden behind it.
            _ => {}
        }
        true
    }
}

/// How many closed files are remembered.
const MAX_RECENT: usize = 20;

/// Where the recent files are kept, one path per line.
fn recent_file() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "cecilia").map(|d| d.data_dir().join("recent-files"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{keys, test_app};

    /// An app showing a saved score, with the dashboard behind it.
    fn app_with_file(name: &str) -> (App, PathBuf) {
        let dir = std::env::temp_dir().join(format!("cecilia-dash-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("song.cc");
        let mut app = test_app();
        app.run(Command::New);
        app.run(Command::Write(Some(file.clone())));
        app.run(Command::Edit(file.clone()));
        assert!(!app.buffer().modified);
        assert!(app.buffer().path.is_some());
        (app, file)
    }

    /// Closes every buffer: the dashboard shows.
    fn close_all(app: &mut App) {
        for _ in 0..5 {
            keys(app, ":bd!<CR>");
        }
    }

    #[test]
    fn closing_the_last_pane_does_nothing() {
        let mut app = test_app();
        keys(&mut app, ":pane-close<CR>");
        assert!(!app.dashboard_visible());
        assert!(!app.quit);
        assert_eq!(app.view().root.panes().len(), 1);
        assert!(app.message.as_ref().is_some_and(|m| m.error));
    }

    #[test]
    fn closing_one_of_two_panes_keeps_the_score() {
        let mut app = test_app();
        keys(&mut app, "<C-w>v:pane-close<CR>");
        assert!(!app.dashboard_visible());
        assert_eq!(app.view().root.panes().len(), 1);
    }

    #[test]
    fn closing_the_last_buffer_shows_the_dashboard() {
        let (mut app, _) = app_with_file("last");
        keys(&mut app, ":bd<CR>");
        assert!(!app.dashboard_visible(), "buffers are left");
        close_all(&mut app);
        assert!(app.dashboard_visible());
        assert!(!app.quit);
        assert!(app.layout().is_empty());
        keys(&mut app, ":q<CR>");
        assert!(app.quit);
    }

    #[test]
    fn closing_one_of_two_buffers_keeps_the_score() {
        let (mut app, _) = app_with_file("two");
        app.run(Command::New);
        keys(&mut app, ":bd!<CR>");
        assert!(!app.dashboard_visible());
    }

    #[test]
    fn closing_the_only_view_shows_the_dashboard() {
        let (mut app, file) = app_with_file("view");
        keys(&mut app, ":view-close<CR>");
        assert!(app.dashboard_visible());
        assert_eq!(app.views.len(), 1);
        assert_eq!(app.recent_files(), vec![file]);
        assert_eq!(app.view().root.panes().len(), 1);
        // Unsaved changes still stop it.
        let (mut app, _) = app_with_file("view-unsaved");
        let b = app.pane().buffer;
        app.buffers[b].modified = true;
        keys(&mut app, ":view-close<CR>");
        assert!(!app.dashboard_visible() && app.confirm_close.is_some());
    }

    #[test]
    fn closing_one_of_two_views_does_not() {
        let mut app = test_app();
        keys(&mut app, ":view-new<CR>:view-close<CR>");
        assert!(!app.dashboard_visible());
        assert_eq!(app.views.len(), 1);
    }

    #[test]
    fn closed_files_are_remembered_most_recent_first_without_repeats() {
        let (mut app, a) = app_with_file("recent-a");
        let (_, b) = app_with_file("recent-b");
        app.remember_closed(Some(b.clone()));
        app.remember_closed(None);
        app.remember_closed(Some(a.clone()));
        app.remember_closed(Some(b.clone()));
        assert_eq!(app.recent_files(), vec![b.clone(), a.clone()]);
        // Files that are gone are skipped.
        std::fs::remove_file(&a).unwrap();
        assert_eq!(app.recent_files(), vec![b]);
        for i in 0..40 {
            app.remember_closed(Some(PathBuf::from(format!("/nowhere/{i}.gp5"))));
        }
        assert!(app.recent.len() <= MAX_RECENT);
    }

    #[test]
    fn closing_a_buffer_remembers_its_file() {
        let (mut app, file) = app_with_file("close");
        assert!(app.recent_files().is_empty());
        close_all(&mut app);
        assert_eq!(app.recent_files(), vec![file]);
    }

    #[test]
    fn open_recent_is_first_and_disabled_without_files() {
        let mut app = test_app();
        assert_eq!(DashboardButton::ALL[0], DashboardButton::OpenRecent);
        app.show_dashboard();
        assert!(!app.dashboard_enabled(0));
        assert_eq!(
            app.dashboard_focus(),
            1,
            "focus starts on the first enabled button"
        );
        // Moving skips it, wrapping from the last button.
        keys(&mut app, "k");
        assert_eq!(app.dashboard_focus(), 3);
        keys(&mut app, "j");
        assert_eq!(app.dashboard_focus(), 1);
        keys(&mut app, "k");
        assert_eq!(app.dashboard_focus(), 3);
        keys(&mut app, "<Home>j");
        assert_eq!(app.dashboard_focus(), 2);
        // A click on it does nothing.
        app.dashboard_activate(0);
        assert!(app.picker.is_none());
    }

    #[test]
    fn open_recent_lists_the_files_and_opens_one() {
        let (mut app, file) = app_with_file("pick");
        close_all(&mut app);
        assert!(app.dashboard_visible());
        assert!(app.dashboard_enabled(0));
        assert_eq!(app.dashboard_focus(), 0);
        keys(&mut app, "<CR>");
        assert!(app.picker.is_some());
        assert!(app.dashboard_visible());
        keys(&mut app, "<CR>");
        assert!(!app.dashboard_visible());
        assert_eq!(
            app.buffer().path.as_deref(),
            file.canonicalize().ok().as_deref()
        );
    }

    #[test]
    fn keys_move_the_focus_and_wrap() {
        let (mut app, _) = app_with_file("keys");
        close_all(&mut app);
        assert_eq!(app.dashboard_focus(), 0);
        keys(&mut app, "j");
        assert_eq!(app.dashboard_focus(), 1);
        keys(&mut app, "<Down>");
        assert_eq!(app.dashboard_focus(), 2);
        keys(&mut app, "<Tab>");
        assert_eq!(app.dashboard_focus(), 3);
        keys(&mut app, "<Tab>");
        assert_eq!(app.dashboard_focus(), 0);
        keys(&mut app, "k");
        assert_eq!(app.dashboard_focus(), 3);
        keys(&mut app, "<Up><S-Tab>");
        assert_eq!(app.dashboard_focus(), 1);
        keys(&mut app, "<End>");
        assert_eq!(app.dashboard_focus(), 3);
        // Other keys do not reach the hidden score.
        let before = app.pane().cursor;
        keys(&mut app, "l");
        assert_eq!(app.pane().cursor, before);
    }

    #[test]
    fn enter_runs_the_focused_button() {
        let mut app = test_app();
        app.show_dashboard();
        keys(&mut app, "<CR>");
        assert!(app.picker.is_some(), "Select Workspace");
        keys(&mut app, "<Esc>");
        assert!(app.picker.is_none());
        assert!(app.dashboard_visible());
        keys(&mut app, "j<CR>");
        assert!(!app.dashboard_visible());
        assert_eq!(app.buffer().name, "[New]");

        let mut app = test_app();
        app.show_dashboard();
        keys(&mut app, "jj<CR>");
        assert!(
            app.take_effects()
                .iter()
                .any(|e| matches!(e, crate::Effect::OpenDialog { .. }))
        );
    }

    #[test]
    fn clicking_a_button_activates_it() {
        let mut app = test_app();
        app.show_dashboard();
        app.dashboard_activate(2);
        assert!(!app.dashboard_visible());
        assert_eq!(app.buffer().name, "[New]");
    }

    #[test]
    fn the_command_line_still_works_on_the_dashboard() {
        let mut app = test_app();
        app.show_dashboard();
        keys(&mut app, ":qa<CR>");
        assert!(app.quit);
    }

    #[test]
    fn a_new_workspace_starts_on_the_dashboard() {
        let mut app = test_app();
        let ws = app.workspace();
        app.restore(&ws);
        assert!(
            !app.dashboard_visible(),
            "restoring a session keeps its panes"
        );
        // Opening a saved workspace with no files in it.
        let dir = std::env::temp_dir().join(format!("cecilia-dash-ws-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.toml");
        std::fs::write(&path, toml::to_string_pretty(&ws).unwrap()).unwrap();
        app.open_workspace(path.to_str().unwrap(), true);
        assert!(app.dashboard_visible());
        // Activating a button leaves it.
        app.dashboard_activate(2);
        assert!(!app.dashboard_visible());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_dashboard_has_no_tab_and_no_tabline() {
        let (mut app, _) = app_with_file("tabs");
        assert!(app.tabline_visible());
        assert!(!app.bufferline().is_empty());
        close_all(&mut app);
        assert!(app.dashboard_visible());
        assert!(app.bufferline().is_empty(), "no [No file] tab");
        assert!(!app.tabline_visible());
        // Nor in the pickers, or by cycling.
        keys(&mut app, ":pick buffer<CR>");
        assert!(app.picker_palette().is_some_and(|p| p.items.is_empty()));
        keys(&mut app, "<Esc>");
        app.run(Command::BufferNext);
        assert!(app.dashboard_visible());
        // Showing a score brings the tabline back.
        app.dashboard_activate(2);
        assert!(app.tabline_visible());
        assert_eq!(app.bufferline().len(), 1);
    }

    #[test]
    fn the_tabline_stays_to_switch_between_several_views() {
        let mut app = test_app();
        app.config.file.ui.keep_files_in_new_view = false;
        keys(&mut app, ":view-new<CR>");
        assert!(app.dashboard_visible());
        assert!(app.tabline_visible());
        assert!(app.bufferline().is_empty());
    }

    #[test]
    fn a_new_view_keeps_the_files_by_default() {
        let mut app = test_app();
        assert!(app.config.file.ui.keep_files_in_new_view);
        keys(&mut app, ":view-new<CR>");
        assert_eq!(app.views.len(), 2);
        assert!(!app.dashboard_visible());
        assert_eq!(app.bufferline().len(), 1);
        assert_eq!(app.buffer().name, "test");
    }

    #[test]
    fn a_new_view_can_start_on_the_dashboard() {
        let mut app = test_app();
        app.config.file.ui.keep_files_in_new_view = false;
        keys(&mut app, ":view-new<CR>");
        assert_eq!(app.views.len(), 2);
        assert_eq!(app.active_view, 1);
        assert!(app.dashboard_visible());
        assert!(app.layout().is_empty());
        assert!(app.bufferline().is_empty());
        // The first view is untouched.
        keys(&mut app, "gT");
        assert_eq!(app.active_view, 0);
        assert!(!app.dashboard_visible());
        assert_eq!(app.bufferline().len(), 1);
        keys(&mut app, "gt");
        assert!(app.dashboard_visible());
        // A button there shows a score in this view only.
        app.dashboard_activate(2);
        assert!(!app.dashboard_visible());
        assert_eq!(app.buffer().name, "[New]");
        assert_eq!(app.bufferline().len(), 1);
        keys(&mut app, "gT");
        assert_eq!(app.buffer().name, "test");
    }

    #[test]
    fn a_dashboard_view_can_be_closed_and_the_quit_still_works() {
        let mut app = test_app();
        app.config.file.ui.keep_files_in_new_view = false;
        keys(&mut app, ":view-new<CR>");
        keys(&mut app, ":view-close<CR>");
        assert_eq!(app.views.len(), 1);
        assert!(!app.dashboard_visible(), "back on the first view's score");
        assert_eq!(app.buffer().name, "test");
        // A view made from a dashboard view is a dashboard view too (nothing to copy).
        app.config.file.ui.keep_files_in_new_view = true;
        app.show_dashboard();
        keys(&mut app, ":view-new<CR>");
        assert!(app.dashboard_visible());
        keys(&mut app, ":q<CR>");
        assert!(app.quit);
    }

    #[test]
    fn views_save_their_dashboard_individually() {
        let mut app = test_app();
        app.config.file.ui.keep_files_in_new_view = false;
        keys(&mut app, ":view-new<CR>");
        keys(&mut app, ":view-new<CR>");
        app.dashboard_activate(2);
        // Views: 0 score, 1 dashboard, 2 score.
        assert_eq!(
            app.views.iter().map(|v| v.dashboard).collect::<Vec<_>>(),
            [false, true, false]
        );
        let ws = app.workspace();
        assert_eq!(
            ws.views.iter().map(|v| v.dashboard).collect::<Vec<_>>(),
            [false, true, false]
        );
        let text = toml::to_string_pretty(&ws).unwrap();
        let ws: crate::workspace::Workspace = toml::from_str(&text).unwrap();
        let mut other = test_app();
        other.restore(&ws);
        assert_eq!(
            other.views.iter().map(|v| v.dashboard).collect::<Vec<_>>(),
            [false, true, false]
        );
        assert!(other.views[1].buffers.is_empty());
        assert!(!other.views[0].buffers.is_empty());
        other.active_view = 1;
        assert!(other.dashboard_visible() && other.bufferline().is_empty());
        other.active_view = 0;
        assert!(!other.dashboard_visible());
    }
}
