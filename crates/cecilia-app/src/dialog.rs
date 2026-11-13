//! The system file picker, shown by the frontends for [`Effect::OpenDialog`] (with the `dialog`
//! feature; without it, `:open` needs a file name).

use std::path::PathBuf;

use crate::app::{App, Effect};

/// File types cecilia opens, for the picker's filter.
pub const EXTENSIONS: &[&str] = &[
    "ccc", "ccm", "cc", "gp", "gpx", "gp5", "gp4", "gp3", "musicxml", "xml", "mxl", "mid", "midi",
    "gabc",
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
    ("cecilia chant", &["ccc"]),
    ("cecilia modern", &["ccm"]),
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

/// Shows an error in a system message box, for a program with no console (`ceciliaw` on
/// Windows); without the `dialog` feature it goes to stderr.
pub fn error_box(title: &str, text: &str) {
    #[cfg(feature = "dialog")]
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(title)
        .set_description(text)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
    #[cfg(not(feature = "dialog"))]
    eprintln!("{title}: {text}");
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
        let dir = crate::config::canonicalize(&dir).unwrap();
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        let (a, b) = (dir.join("a.ccm"), dir.join("b.ccm"));
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
        assert_eq!(app.buffer().name, "a.ccm");
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
                    name: "untitled.ccm".into()
                }],
                "{cmd}"
            );
        }
    }

    #[test]
    fn save_as_starts_beside_the_file_and_suggests_its_name() {
        let dir = temp_dir("start");
        let a = dir.join("a.ccm");
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.run(Command::Write(Some(a.clone())));
        app.take_effects();
        app.run(Command::SaveAs(None));
        assert_eq!(
            app.take_effects(),
            vec![Effect::SaveDialog {
                dir: Some(crate::config::canonicalize(&dir).unwrap()),
                name: "a.ccm".into()
            }]
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn plain_write_of_a_named_buffer_still_writes() {
        let dir = temp_dir("named");
        let a = dir.join("a.ccm");
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
        let out = dir.join("song.ccm");
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.take_effects();
        app.run(Command::Write(None));
        let effect = app.take_effects().pop().unwrap();
        // Frontends hand the effect to `apply`; the dialog itself is replaced by `save_as`.
        assert!(matches!(effect, Effect::SaveDialog { .. }));
        app.save_as(out.clone());
        assert!(out.exists());
        assert_eq!(app.buffer().name, "song.ccm");
        assert_eq!(
            app.buffer().path,
            Some(crate::config::canonicalize(&out).unwrap())
        );
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
        app.run(Command::Write(Some(dir.join("first.ccm"))));
        assert_eq!(app.buffer().name, "first.ccm");
        assert!(!app.buffer().modified);
        // `:w other` is `:save-as other`: the buffer is renamed again.
        app.run(Command::Write(Some(dir.join("second.ccm"))));
        assert_eq!(app.buffer().name, "second.ccm");
        std::fs::remove_dir_all(dir).ok();
    }

    fn chant_app() -> App {
        let mut app = App::new(Config::default(), None);
        let chant =
            cecilia_formats::gabc::parse("name: t;\n%%\n(c4) Al(fg)le(h) lu(g)ia.(f.) (::)\n")
                .unwrap();
        app.buffers[0].score = cecilia_formats::gabc::chant_score(chant);
        app
    }

    #[test]
    fn the_native_extension_follows_the_score_kind() {
        let dir = temp_dir("kind");
        let mut modern = App::new(Config::default(), None);
        let mut chant = chant_app();
        for (app, ext) in [(&mut modern, "ccm"), (&mut chant, "ccc")] {
            app.take_effects();
            app.run(Command::SaveAs(None));
            let Some(Effect::SaveDialog { name, .. }) = app.take_effects().pop() else {
                panic!("no dialog")
            };
            assert_eq!(
                name,
                if ext == "ccc" {
                    "t.ccc".into()
                } else {
                    format!("untitled.{ext}")
                }
            );
            // No extension typed: the kind's own.
            app.save_as(dir.join(format!("noext-{ext}")));
            assert_eq!(app.buffer().name, format!("noext-{ext}.{ext}"));
            assert!(dir.join(format!("noext-{ext}.{ext}")).exists());
            // An extension typed is honored, and the file reads back.
            let other = if ext == "ccc" { "ccm" } else { "ccc" };
            app.save_as(dir.join(format!("typed.{other}")));
            assert_eq!(app.buffer().name, format!("typed.{other}"));
            let mut reader = App::new(Config::default(), None);
            reader.open_files(vec![dir.join(format!("typed.{other}"))]);
            assert_eq!(reader.buffer().score.is_chant(), ext == "ccc");
        }
        // Legacy `.cc` files still open.
        std::fs::copy(dir.join("noext-ccm.ccm"), dir.join("old.cc")).unwrap();
        let mut reader = App::new(Config::default(), None);
        reader.open_files(vec![dir.join("old.cc")]);
        assert_eq!(reader.buffer().name, "old.cc");
        assert!(!reader.buffer().score.is_chant());
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
        assert_eq!(name, "ave-maria-2nd-setting.ccm");
    }

    #[test]
    fn save_as_with_a_path_writes_and_renames() {
        let dir = temp_dir("path");
        let (a, b) = (dir.join("a.ccm"), dir.join("b"));
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.run(Command::SaveAs(Some(a.clone())));
        assert!(app.take_effects().is_empty());
        assert_eq!(app.buffer().name, "a.ccm");
        // No extension: a native file.
        app.run(Command::SaveAs(Some(b)));
        assert_eq!(app.buffer().name, "b.ccm");
        assert!(dir.join("b.ccm").exists());
        // A format that can't be written changes nothing.
        app.run(Command::SaveAs(Some(dir.join("c.xyz"))));
        assert_eq!(app.buffer().name, "b.ccm");
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
            app.last_notification()
                .is_some_and(|m| m.error && m.text.contains("no file dialog in this build"))
        );
    }
}
