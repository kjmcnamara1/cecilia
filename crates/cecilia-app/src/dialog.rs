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
        Effect::SaveDialog { dir, name } => {
            match pick_save_path(dir, &name) {
                Ok(Some(path)) => app.save_as(path),
                Ok(None) => {}
                Err(e) => app.error(e),
            }
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

/// File types cecilia writes, for the save dialog: the native format first.
pub const WRITE_FILTERS: &[(&str, &[&str])] = &[
    ("cecilia", &["cc"]),
    ("Guitar Pro", &["gp", "gpx", "gp5"]),
    ("MusicXML", &["musicxml", "xml", "mxl"]),
    ("MIDI", &["mid", "midi"]),
    ("Gregorian chant (GABC)", &["gabc"]),
];

/// The file chosen in the save dialog (None: cancelled). Without the `dialog` feature there is
/// no dialog: an error says so.
#[cfg(feature = "dialog")]
fn pick_save_path(dir: Option<PathBuf>, name: &str) -> Result<Option<PathBuf>, String> {
    let mut dialog = rfd::FileDialog::new()
        .set_title("Save as")
        .set_file_name(name);
    for (label, exts) in WRITE_FILTERS {
        dialog = dialog.add_filter(*label, exts);
    }
    if let Some(dir) = dir.or_else(|| std::env::current_dir().ok()) {
        dialog = dialog.set_directory(dir);
    }
    Ok(dialog.save_file())
}

#[cfg(not(feature = "dialog"))]
fn pick_save_path(_dir: Option<PathBuf>, _name: &str) -> Result<Option<PathBuf>, String> {
    Err("save-as: no file dialog in this build; use :write FILE".to_string())
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

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cecilia-saveas-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parses_save_as() {
        assert_eq!(Command::parse("save-as"), Ok(Command::SaveAs(None)));
        assert_eq!(
            Command::parse("save-as out.gp"),
            Ok(Command::SaveAs(Some("out.gp".into())))
        );
    }

    #[test]
    fn save_as_and_plain_write_of_an_unnamed_buffer_ask_for_a_file() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.take_effects();
        for cmd in ["save-as", "w", "write"] {
            app.run(Command::parse(cmd).unwrap());
            assert_eq!(
                app.take_effects(),
                vec![Effect::SaveDialog {
                    dir: None,
                    name: "untitled.cc".into()
                }],
                "{cmd}"
            );
        }
    }

    #[test]
    fn save_as_starts_beside_the_file_and_suggests_its_name() {
        let dir = temp_dir("start");
        let a = dir.join("a.cc");
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.run(Command::Write(Some(a.clone())));
        app.take_effects();
        app.run(Command::SaveAs(None));
        assert_eq!(
            app.take_effects(),
            vec![Effect::SaveDialog {
                dir: Some(dir.canonicalize().unwrap()),
                name: "a.cc".into()
            }]
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn plain_write_of_a_named_buffer_still_writes() {
        let dir = temp_dir("named");
        let a = dir.join("a.cc");
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.run(Command::Write(Some(a.clone())));
        app.take_effects();
        app.buffers.iter_mut().for_each(|b| b.modified = true);
        std::fs::remove_file(&a).unwrap();
        app.run(Command::Write(None));
        assert!(app.take_effects().is_empty());
        assert!(a.exists());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_chosen_path_names_the_buffer_and_later_writes_go_there() {
        let dir = temp_dir("pick");
        let out = dir.join("song.cc");
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.take_effects();
        app.run(Command::Write(None));
        let effect = app.take_effects().pop().unwrap();
        // Frontends hand the effect to `apply`; the dialog itself is replaced by `save_as`.
        assert!(matches!(effect, Effect::SaveDialog { .. }));
        app.save_as(out.clone());
        assert!(out.exists());
        assert_eq!(app.buffer().name, "song.cc");
        assert_eq!(app.buffer().path, Some(out.canonicalize().unwrap()));
        assert!(!app.buffer().modified);
        // A plain :w now writes there, with no dialog.
        app.buffers.iter_mut().for_each(|b| b.modified = true);
        std::fs::remove_file(&out).unwrap();
        app.run(Command::Write(None));
        assert!(app.take_effects().is_empty());
        assert!(out.exists());
        // `:w other` is `:save-as other`.
        let other = dir.join("other.mid");
        app.run(Command::Write(Some(other.clone())));
        assert!(other.exists());
        assert_eq!(app.buffer().name, "other.mid");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn writing_an_unnamed_buffer_to_a_path_names_it() {
        let dir = temp_dir("write-new");
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.run(Command::Write(Some(dir.join("first.cc"))));
        assert_eq!(app.buffer().name, "first.cc");
        assert!(!app.buffer().modified);
        // `:w other` is `:save-as other`: the buffer is renamed again.
        app.run(Command::Write(Some(dir.join("second.cc"))));
        assert_eq!(app.buffer().name, "second.cc");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_dialog_suggests_the_kebab_case_title() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        let i = app.pane().buffer;
        app.buffers[i].score.meta.title = "Ave Maria (2nd setting)".into();
        app.take_effects();
        app.run(Command::SaveAs(None));
        let Some(Effect::SaveDialog { name, .. }) = app.take_effects().pop() else {
            panic!("no dialog")
        };
        assert_eq!(name, "ave-maria-2nd-setting.cc");
    }

    #[test]
    fn save_as_with_a_path_writes_and_renames() {
        let dir = temp_dir("path");
        let (a, b) = (dir.join("a.cc"), dir.join("b"));
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.run(Command::SaveAs(Some(a.clone())));
        assert!(app.take_effects().is_empty());
        assert_eq!(app.buffer().name, "a.cc");
        // No extension: a native file.
        app.run(Command::SaveAs(Some(b)));
        assert_eq!(app.buffer().name, "b.cc");
        assert!(dir.join("b.cc").exists());
        // A format that can't be written changes nothing.
        app.run(Command::SaveAs(Some(dir.join("c.xyz"))));
        assert_eq!(app.buffer().name, "b.cc");
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(not(feature = "dialog"))]
    #[test]
    fn without_the_dialog_feature_save_as_says_so() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.run(Command::SaveAs(None));
        for e in app.take_effects() {
            assert!(apply(&mut app, e).is_none());
        }
        assert!(
            app.message
                .as_ref()
                .is_some_and(|m| m.error && m.text.contains("no file dialog in this build"))
        );
    }
}
