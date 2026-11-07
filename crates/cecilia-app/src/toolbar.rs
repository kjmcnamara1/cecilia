//! The toolbar: buttons for common commands along the top of the window, above the tabline (`ui.toolbar`,
//! `:toolbar`, `<leader>B`). Which buttons, in which order, is `[toolbar] items` in
//! config.toml: command lines as typed after `:`, with `"|"` between groups.

use crate::app::App;
use crate::command::Command;
use crate::config::Mode;

/// One toolbar entry.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolItem {
    Separator,
    Button(ToolButton),
}

/// A button: the command it runs and how it shows.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolButton {
    /// The command line, as configured (e.g. `"write"`, `"layout page"`).
    pub command: String,
    /// The command's name (its first word), which picks the icon.
    pub name: String,
    /// What it does, and its key (e.g. "Write the file (:w)  <C-s>").
    pub tooltip: String,
    /// A toggle that is on (follow, loop, metronome…), or play while playing.
    pub active: bool,
    /// Whether clicking does anything now (e.g. undo needs something to undo).
    pub enabled: bool,
}

/// The default buttons.
pub const DEFAULT_ITEMS: &[&str] = &[
    "new",
    "open",
    "write",
    "|",
    "undo",
    "redo",
    "|",
    "play-toggle",
    "stop",
    "follow-toggle",
    "loop-toggle",
    "metronome",
    "count-in",
    "|",
    "tracks",
    "properties",
    "waveform",
    "piano",
    "fretboard",
    "tuner",
    "split",
    "vsplit",
    "|",
    "command-mode",
    "pick-keymap",
];

impl App {
    /// The toolbar's entries, with their current state.
    pub fn toolbar_items(&self) -> Vec<ToolItem> {
        let buffer = self.buffer();
        let normal = self.config.keymap(Mode::Normal);
        self.config
            .file
            .toolbar
            .items
            .iter()
            .filter_map(|line| {
                let line = line.trim();
                if line == "|" {
                    return Some(ToolItem::Separator);
                }
                Command::parse(line).ok()?;
                let name = line.split_whitespace().next().unwrap_or(line).to_string();
                // The first key bound to exactly this command line, if any.
                let mut keys: Vec<String> = normal
                    .names
                    .iter()
                    .filter(|(_, names)| names.len() == 1 && names[0].trim() == line)
                    .map(|(k, _)| crate::keys::format_keys(k))
                    .collect();
                keys.sort_by_key(|k| (k.len(), k.clone()));
                let what = crate::complete::describe(&name).unwrap_or(line);
                let tooltip = match keys.first() {
                    Some(k) => format!("{what}  {k}"),
                    None => what.to_string(),
                };
                let active = match name.as_str() {
                    "play-toggle" | "play" => self.playing,
                    "follow-toggle" => self.follow,
                    "loop-toggle" => self.pa.loop_bars.is_some(),
                    "metronome" => self.pa.metronome,
                    "count-in" => self.pa.count_in,
                    "tracks" => self.has_tool(crate::app::PaneKind::Tracks),
                    "waveform" => self.has_tool(crate::app::PaneKind::Audio),
                    "properties" => self.has_tool(crate::app::PaneKind::Properties),
                    "piano" => self.has_tool(crate::app::PaneKind::Piano),
                    "fretboard" => self.has_tool(crate::app::PaneKind::Fretboard),
                    "tuner" => self.has_tool(crate::app::PaneKind::Tuner),
                    "progress-bar" => self.progress_bar,
                    "maximize" => self.maximized().is_some(),
                    _ => false,
                };
                let enabled = match name.as_str() {
                    "undo" | "u" => buffer.history.can_undo(),
                    "redo" => buffer.history.can_redo(),
                    "write" | "w" => buffer.modified || line != "write" && line != "w",
                    "stop" => self.playing || self.paused,
                    _ => true,
                };
                Some(ToolItem::Button(ToolButton {
                    command: line.to_string(),
                    name,
                    tooltip,
                    active,
                    enabled,
                }))
            })
            .collect()
    }

    /// Runs toolbar entry `i` (a button; separators do nothing).
    pub fn run_toolbar(&mut self, i: usize) {
        let Some(ToolItem::Button(b)) = self.toolbar_items().into_iter().nth(i) else {
            return;
        };
        if !b.enabled {
            return;
        }
        match self.parse_for_score(&b.command) {
            Ok(cmd) => self.run(cmd),
            Err(e) => self.error(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;

    #[test]
    fn toolbar_buttons_show_state_and_run_their_commands() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        let items = app.toolbar_items();
        let find = |items: &[ToolItem], name: &str| {
            items
                .iter()
                .position(|i| matches!(i, ToolItem::Button(b) if b.name == name))
        };
        let button = |items: &[ToolItem], name: &str| match &items[find(items, name).unwrap()] {
            ToolItem::Button(b) => b.clone(),
            _ => unreachable!(),
        };
        assert!(matches!(items[3], ToolItem::Separator));
        // Nothing to undo yet; follow is on; tooltips name the key.
        assert!(!button(&items, "undo").enabled);
        assert!(button(&items, "follow-toggle").active);
        assert!(
            button(&items, "play-toggle")
                .tooltip
                .contains("<Space><Space>")
        );
        // Clicking runs the command.
        let follow = find(&items, "follow-toggle").unwrap();
        app.run_toolbar(follow);
        assert!(!app.follow && !button(&app.toolbar_items(), "follow-toggle").active);
        let tracks = find(&items, "tracks").unwrap();
        app.run_toolbar(tracks);
        assert!(app.has_tool(crate::app::PaneKind::Tracks));
        // The pane buttons sit with the tracks button and toggle their panes.
        for (name, kind) in [
            ("properties", crate::app::PaneKind::Properties),
            ("waveform", crate::app::PaneKind::Audio),
            ("piano", crate::app::PaneKind::Piano),
            ("fretboard", crate::app::PaneKind::Fretboard),
            ("tuner", crate::app::PaneKind::Tuner),
        ] {
            let i = find(&items, name).unwrap();
            assert!(
                i > tracks && i < tracks + 8,
                "{name} is grouped with tracks"
            );
            let before = app.has_tool(kind);
            app.run_toolbar(i);
            assert_ne!(app.has_tool(kind), before, "{name} toggles");
            assert_eq!(button(&app.toolbar_items(), name).active, !before);
        }
        // :toolbar toggles it; bad entries in the config are reported and left out.
        app.run(Command::Toolbar(None));
        assert!(!app.toolbar);
        let (config, errors) =
            Config::parse("[toolbar]\nitems = [\"write\", \"|\", \"nonsense\"]\n").unwrap();
        assert_eq!(errors.len(), 1, "{errors:?}");
        let app = App::new(config, None);
        assert_eq!(app.toolbar_items().len(), 2);
    }
}
