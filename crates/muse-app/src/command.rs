//! Commands: the single vocabulary shared by keymaps, the `:` command line and frontends.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Down,
    Up,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    BeatNext,
    BeatPrev,
    StringDown,
    StringUp,
    NoteNext,
    NotePrev,
    BarNext,
    BarPrev,
    BarStart,
    BarEnd,
    /// Go to the first bar, or to bar `count` when a count is given.
    GotoFirst,
    /// Go to the last bar, or to bar `count` when a count is given.
    GotoLast,
    GotoBar(usize),
    TrackNext,
    TrackPrev,
    Track(usize),
    PageDown,
    PageUp,
    PlayToggle,
    Play,
    Pause,
    Stop,
    FollowToggle,
    Split,
    VSplit,
    Focus(Direction),
    ClosePane,
    TabNext,
    TabPrev,
    TabNew(Option<PathBuf>),
    TabClose,
    Edit(PathBuf),
    /// Write the buffer, to its own file or (exporting) to another path.
    Write(Option<PathBuf>),
    WriteQuit,
    Quit {
        force: bool,
    },
    QuitAll {
        force: bool,
    },
    Source,
    CommandMode,
    Normal,
    Help,
    Nop,
}

impl Command {
    /// Parses a command as written in keymaps (`bar-next`, `goto-bar 3`) or on the command line
    /// (`e file.gp5`, `q!`, `12`).
    pub fn parse(input: &str) -> Result<Command, String> {
        let input = input.trim().trim_start_matches(':').trim();
        if input.is_empty() {
            return Ok(Command::Nop);
        }
        if let Ok(n) = input.parse::<usize>() {
            return Ok(Command::GotoBar(n));
        }
        let (name, arg) = match input.split_once(char::is_whitespace) {
            Some((n, a)) => (n, Some(a.trim()).filter(|a| !a.is_empty())),
            None => (input, None),
        };
        let number = |what: &str| -> Result<usize, String> {
            arg.ok_or_else(|| format!("{name}: missing {what}"))?
                .parse::<usize>()
                .map_err(|_| format!("{name}: invalid {what}"))
        };
        let path = || -> Result<PathBuf, String> {
            arg.map(expand_tilde)
                .ok_or_else(|| format!("{name}: missing file name"))
        };
        use Command::*;
        Ok(match name {
            "beat-next" => BeatNext,
            "beat-prev" => BeatPrev,
            "string-down" => StringDown,
            "string-up" => StringUp,
            "note-next" => NoteNext,
            "note-prev" => NotePrev,
            "bar-next" => BarNext,
            "bar-prev" => BarPrev,
            "bar-start" => BarStart,
            "bar-end" => BarEnd,
            "goto-first" => GotoFirst,
            "goto-last" => GotoLast,
            "goto-bar" | "bar" => GotoBar(number("bar number")?),
            "track-next" => TrackNext,
            "track-prev" => TrackPrev,
            "track" => Track(number("track number")?),
            "page-down" => PageDown,
            "page-up" => PageUp,
            "play-toggle" => PlayToggle,
            "play" => Play,
            "pause" => Pause,
            "stop" => Stop,
            "follow-toggle" => FollowToggle,
            "split" | "sp" => Split,
            "vsplit" | "vs" => VSplit,
            "focus-left" => Focus(Direction::Left),
            "focus-down" => Focus(Direction::Down),
            "focus-up" => Focus(Direction::Up),
            "focus-right" => Focus(Direction::Right),
            "close-pane" | "close" | "clo" => ClosePane,
            "tab-next" | "tabnext" | "tabn" => TabNext,
            "tab-prev" | "tabprevious" | "tabp" => TabPrev,
            "tabnew" | "tabe" | "tabedit" => TabNew(arg.map(expand_tilde)),
            "tabclose" | "tabc" => TabClose,
            "edit" | "e" => Edit(path()?),
            "write" | "w" => Write(arg.map(expand_tilde)),
            "wq" | "x" | "write-quit" => WriteQuit,
            "quit" | "q" => Quit { force: false },
            "quit!" | "q!" => Quit { force: true },
            "quitall" | "qa" => QuitAll { force: false },
            "quitall!" | "qa!" => QuitAll { force: true },
            "source" | "so" => Source,
            "command-mode" => CommandMode,
            "normal" => Normal,
            "help" | "h" => Help,
            "nop" => Nop,
            _ => return Err(format!("unknown command: {name}")),
        })
    }

    /// Whether a numeric count repeats this command.
    pub fn repeatable(&self) -> bool {
        use Command::*;
        matches!(
            self,
            BeatNext
                | BeatPrev
                | StringDown
                | StringUp
                | NoteNext
                | NotePrev
                | BarNext
                | BarPrev
                | TrackNext
                | TrackPrev
                | PageDown
                | PageUp
                | TabNext
                | TabPrev
        )
    }
}

fn expand_tilde(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(rest),
        None => PathBuf::from(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_keymap_and_ex_forms() {
        assert_eq!(Command::parse("bar-next"), Ok(Command::BarNext));
        assert_eq!(Command::parse(":12"), Ok(Command::GotoBar(12)));
        assert_eq!(Command::parse("q!"), Ok(Command::Quit { force: true }));
        assert_eq!(
            Command::parse("e song.gp5"),
            Ok(Command::Edit("song.gp5".into()))
        );
        assert_eq!(Command::parse("tabnew"), Ok(Command::TabNew(None)));
        assert_eq!(
            Command::parse("w out.gp"),
            Ok(Command::Write(Some("out.gp".into())))
        );
        assert_eq!(Command::parse("w"), Ok(Command::Write(None)));
        assert!(Command::parse("e").is_err());
        assert!(Command::parse("frobnicate").is_err());
    }
}
