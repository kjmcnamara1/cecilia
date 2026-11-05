//! The dashboard: what fills the window when no score pane is open (all score panes closed with
//! `:pane-close`, or a workspace with nothing in it). A column of buttons that run existing
//! commands; the frontends draw it and report clicks.

use crate::app::{App, PaneKind};
use crate::command::Command;
use crate::config::Mode;
use crate::keys::{Key, KeyCode};
use crate::layout::PaneId;
use crate::picker::PickerKind;

/// A button of the dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardButton {
    SelectWorkspace,
    NewScore,
    OpenScore,
}

impl DashboardButton {
    /// Every button, top to bottom.
    pub const ALL: [DashboardButton; 3] = [
        DashboardButton::SelectWorkspace,
        DashboardButton::NewScore,
        DashboardButton::OpenScore,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DashboardButton::SelectWorkspace => "Select Workspace",
            DashboardButton::NewScore => "New Score",
            DashboardButton::OpenScore => "Open Score",
        }
    }

    /// The command the button runs.
    pub fn command(self) -> Command {
        match self {
            DashboardButton::SelectWorkspace => Command::Pick(PickerKind::Workspace),
            DashboardButton::NewScore => Command::New,
            DashboardButton::OpenScore => Command::OpenDialog,
        }
    }
}

impl App {
    /// Whether the dashboard replaces the panes.
    pub fn dashboard_visible(&self) -> bool {
        self.dashboard
    }

    /// The focused button (an index into [`DashboardButton::ALL`]).
    pub fn dashboard_focus(&self) -> usize {
        self.dashboard_focus
    }

    /// Shows the dashboard with the first button focused.
    pub(crate) fn show_dashboard(&mut self) {
        self.dashboard = true;
        self.dashboard_focus = 0;
    }

    /// Moves the button focus by `delta`, wrapping around.
    pub fn dashboard_move(&mut self, delta: isize) {
        let n = DashboardButton::ALL.len() as isize;
        self.dashboard_focus = (self.dashboard_focus as isize + delta).rem_euclid(n) as usize;
    }

    /// Runs button `index` (Enter on it, or a click). The dashboard goes once a score shows.
    pub fn dashboard_activate(&mut self, index: usize) {
        let Some(button) = DashboardButton::ALL.get(index) else {
            return;
        };
        self.dashboard_focus = index;
        self.run(button.command());
    }

    /// Takes a key while the dashboard shows. Returns false when it is not the dashboard's:
    /// the key then goes the usual way (`:` opens the command line; pickers and the command
    /// line take their own keys).
    pub(crate) fn dashboard_key(&mut self, key: Key) -> bool {
        if !self.dashboard
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
            KeyCode::Home | KeyCode::Char('g') => self.dashboard_focus = 0,
            KeyCode::End | KeyCode::Char('G') => {
                self.dashboard_focus = DashboardButton::ALL.len() - 1;
            }
            KeyCode::Enter | KeyCode::Char(' ') => self.dashboard_activate(self.dashboard_focus),
            // Other keys would edit the score hidden behind it.
            _ => {}
        }
        true
    }

    /// Closes the view's last score pane for the dashboard: the tool panes it owns go with it,
    /// the buffers stay loaded (listed by the buffer picker, asked about on quit).
    pub(crate) fn pane_to_dashboard(&mut self, id: PaneId) {
        let owned: Vec<PaneId> = self
            .panes
            .iter()
            .filter(|(_, p)| p.owner == Some(id) && p.kind != PaneKind::Score)
            .map(|(t, _)| *t)
            .collect();
        let equalize = self.config.file.ui.equalize_on_close;
        for t in owned {
            self.views[self.active_view].root.remove_with(t, equalize);
            self.panes.remove(&t);
        }
        self.show_dashboard();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{keys, test_app};

    #[test]
    fn closing_the_last_score_pane_shows_the_dashboard() {
        let mut app = test_app();
        assert!(!app.dashboard_visible());
        keys(&mut app, ":pane-close<CR>");
        assert!(app.dashboard_visible());
        assert!(!app.quit);
        assert_eq!(app.dashboard_focus(), 0);
        assert!(app.layout().is_empty());
    }

    #[test]
    fn closing_one_of_two_panes_keeps_the_score() {
        let mut app = test_app();
        keys(&mut app, "<C-w>v:pane-close<CR>");
        assert!(!app.dashboard_visible());
    }

    #[test]
    fn tool_panes_do_not_keep_the_dashboard_away() {
        let mut app = test_app();
        app.set_tool(PaneKind::Tracks, Some(true));
        app.set_focus(app.focused());
        keys(&mut app, ":pane-close<CR>");
        assert!(app.dashboard_visible());
        assert!(app.layout().is_empty());
        keys(&mut app, ":q<CR>");
        assert!(app.quit);
    }

    #[test]
    fn keys_move_the_focus_and_wrap() {
        let mut app = test_app();
        keys(&mut app, ":pane-close<CR>");
        keys(&mut app, "j");
        assert_eq!(app.dashboard_focus(), 1);
        keys(&mut app, "<Down>");
        assert_eq!(app.dashboard_focus(), 2);
        keys(&mut app, "<Tab>");
        assert_eq!(app.dashboard_focus(), 0);
        keys(&mut app, "k");
        assert_eq!(app.dashboard_focus(), 2);
        keys(&mut app, "<Up><S-Tab>");
        assert_eq!(app.dashboard_focus(), 0);
        // Other keys do not reach the hidden score.
        let before = app.pane().cursor;
        keys(&mut app, "l");
        assert_eq!(app.pane().cursor, before);
    }

    #[test]
    fn enter_runs_the_focused_button() {
        let mut app = test_app();
        keys(&mut app, ":pane-close<CR>j<CR>");
        assert!(!app.dashboard_visible());
        assert_eq!(app.buffer().name, "[New]");

        let mut app = test_app();
        keys(&mut app, ":pane-close<CR><CR>");
        assert!(app.picker.is_some());
        keys(&mut app, "<Esc>");
        assert!(app.picker.is_none());
        assert!(app.dashboard_visible());

        let mut app = test_app();
        keys(&mut app, ":pane-close<CR>jj<CR>");
        assert!(
            app.take_effects()
                .iter()
                .any(|e| matches!(e, crate::Effect::OpenDialog { .. }))
        );
    }

    #[test]
    fn clicking_a_button_activates_it() {
        let mut app = test_app();
        keys(&mut app, ":pane-close<CR>");
        app.dashboard_activate(1);
        assert!(!app.dashboard_visible());
        assert_eq!(app.buffer().name, "[New]");
    }

    #[test]
    fn the_command_line_still_works_on_the_dashboard() {
        let mut app = test_app();
        keys(&mut app, ":pane-close<CR>:qa<CR>");
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
        let dir = std::env::temp_dir().join(format!("cecilia-dash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.toml");
        std::fs::write(&path, toml::to_string_pretty(&ws).unwrap()).unwrap();
        app.open_workspace(path.to_str().unwrap(), true);
        assert!(app.dashboard_visible());
        // Activating a button leaves it.
        app.dashboard_activate(1);
        assert!(!app.dashboard_visible());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_dashboard_is_saved_with_the_workspace() {
        let mut app = test_app();
        keys(&mut app, ":pane-close<CR>");
        let ws = app.workspace();
        assert!(ws.dashboard);
        app.dashboard = false;
        app.restore(&ws);
        assert!(app.dashboard_visible());
    }
}
