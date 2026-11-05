//! The system file picker, shown by the frontends for [`Effect::OpenDialog`] (with the `dialog`
//! feature; without it, `:open` needs a file name).

use std::path::PathBuf;

use crate::app::{App, Effect};

/// File types cecilia opens, for the picker's filter.
pub const EXTENSIONS: &[&str] = &[
    "cc", "gp", "gpx", "gp5", "gp4", "gp3", "musicxml", "xml", "mxl", "mid", "midi", "gabc",
];

/// Carries out a dialog effect, or hands any other effect back.
pub fn apply(app: &mut App, effect: Effect) -> Option<Effect> {
    match effect {
        Effect::OpenDialog { dir } => {
            let files = pick_files(dir);
            app.open_files(files);
            None
        }
        e => Some(e),
    }
}

#[cfg(feature = "dialog")]
fn pick_files(dir: Option<PathBuf>) -> Vec<PathBuf> {
    let mut dialog = rfd::FileDialog::new()
        .set_title("Open")
        .add_filter("Scores", EXTENSIONS)
        .add_filter("All files", &["*"]);
    if let Some(dir) = dir.or_else(|| std::env::current_dir().ok()) {
        dialog = dialog.set_directory(dir);
    }
    dialog.pick_files().unwrap_or_default()
}

#[cfg(not(feature = "dialog"))]
fn pick_files(_dir: Option<PathBuf>) -> Vec<PathBuf> {
    Vec::new()
}

impl App {
    /// Opens files as buffer tabs and shows the first.
    pub fn open_files(&mut self, files: Vec<PathBuf>) {
        let mut first = None;
        let mut failed = Vec::new();
        for f in files {
            match self.open(&f) {
                Ok(b) => {
                    // Showing a buffer gives it a tab.
                    self.show_buffer(b);
                    first.get_or_insert(b);
                }
                Err(e) => failed.push(e),
            }
        }
        if let Some(b) = first {
            self.show_buffer(b);
        }
        if !failed.is_empty() {
            self.error(failed.join("; "));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Config};

    #[test]
    fn open_without_a_file_asks_for_files() {
        let dir = std::env::temp_dir().join(format!("cecilia-dialog-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        let (a, b) = (dir.join("a.cc"), dir.join("b.cc"));
        app.run(Command::Write(Some(a.clone())));
        app.run(Command::Write(Some(b.clone())));
        assert_eq!(Command::parse("open"), Ok(Command::OpenDialog));
        // It starts in the folder of the file shown.
        app.run(Command::Edit(a.clone()));
        app.take_effects();
        app.run(Command::OpenDialog);
        assert_eq!(
            app.take_effects(),
            vec![Effect::OpenDialog {
                dir: Some(dir.clone())
            }]
        );
        // The files chosen open as tabs; the first is shown.
        let mut app = App::new(Config::default(), None);
        app.open_files(vec![a, b]);
        assert_eq!(app.buffer().name, "a.cc");
        assert_eq!(app.bufferline().len(), 2);
        std::fs::remove_dir_all(dir).ok();
    }
}
