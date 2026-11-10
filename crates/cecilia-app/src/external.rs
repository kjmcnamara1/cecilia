//! Files changed or deleted outside the app while open in a buffer: the buffer is flagged
//! modified, and for a changed file a prompt asks whether to load the new contents.

use std::time::{Duration, Instant};

use crate::app::{App, clamp_cursor, file_stamp};
use crate::config::Mode;
use crate::keys::{Key, KeyCode};

/// How often the open files are looked at.
const INTERVAL: Duration = Duration::from_secs(1);

impl App {
    /// Looks at the open files now and then (called from the frontends' poll).
    pub(crate) fn poll_files(&mut self) {
        let now = Instant::now();
        if self
            .disk_checked
            .is_some_and(|t| now.duration_since(t) < INTERVAL)
        {
            return;
        }
        self.disk_checked = Some(now);
        self.check_external();
    }

    /// Compares each open file with what the buffer last read or wrote. A buffer whose file
    /// vanished is flagged modified (`:w` recreates it); one whose file changed is flagged too,
    /// and the user is asked whether to load the new contents.
    pub(crate) fn check_external(&mut self) {
        // Not while another question is open or a command line is being typed.
        if self.confirm_reload.is_some()
            || self.confirm_close.is_some()
            || self.confirm_delete_workspace.is_some()
            || self.mode == Mode::Command
        {
            return;
        }
        for index in 0..self.buffers.len() {
            let Some(path) = self.buffers[index].path.clone() else {
                continue;
            };
            let now = file_stamp(&path);
            if now == self.buffers[index].disk {
                continue;
            }
            // Answered (or noted) once: ask again only if the file changes again.
            self.buffers[index].disk = now;
            self.buffers[index].modified = true;
            let name = self.buffers[index].name.clone();
            if now.is_none() {
                self.error(format!(
                    "\"{name}\" was deleted on disk; kept as unsaved (:w to recreate it)"
                ));
                continue;
            }
            self.confirm_reload = Some(index);
            self.ask(format!(
                "\"{name}\" changed on disk: load the changes? (y/n; n keeps your version)"
            ));
            return;
        }
    }

    /// The answer to the reload prompt: `y` loads the file, anything else keeps the buffer
    /// (still flagged modified, so it is not lost silently).
    pub(crate) fn answer_reload(&mut self, index: usize, key: Key) {
        self.answered();
        if matches!(key.code, KeyCode::Char('y' | 'Y')) {
            self.reload_buffer(index);
        } else {
            self.info("kept your version; :w overwrites the file");
        }
    }

    /// Replaces a buffer's score with its file's current contents. History starts over.
    pub(crate) fn reload_buffer(&mut self, index: usize) {
        let Some(path) = self.buffers.get(index).and_then(|b| b.path.clone()) else {
            return;
        };
        let score = match cecilia_formats::read_file(&path) {
            Ok(score) => score,
            Err(e) => return self.error(format!("{}: {e}", path.display())),
        };
        let b = &mut self.buffers[index];
        b.score = score;
        b.history = Default::default();
        b.modified = false;
        b.disk = file_stamp(&path);
        b.version += 1;
        let name = b.name.clone();
        self.swap_release(index);
        self.effects
            .push(crate::app::Effect::ScoreChanged { buffer: index });
        let score = &self.buffers[index].score;
        for pane in self.panes.values_mut().filter(|p| p.buffer == index) {
            clamp_cursor(score, &mut pane.cursor);
        }
        self.info(format!("\"{name}\" reloaded"));
    }
}

#[cfg(test)]
mod tests {
    use crate::App;
    use crate::command::Command;
    use crate::config::Config;
    use crate::keys::Key;

    fn key(c: char) -> Key {
        Key::new(crate::keys::KeyCode::Char(c))
    }

    fn app_with_file(tag: &str) -> (App, std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("cecilia-ext-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("song.ccm");
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.run(Command::Write(Some(file.clone())));
        (app, dir, file)
    }

    #[test]
    fn a_changed_file_flags_the_buffer_and_asks_to_reload() {
        let (mut app, dir, file) = app_with_file("changed");
        let i = app.pane().buffer;
        app.check_external();
        assert!(app.confirm_reload.is_none() && !app.buffers[i].modified);
        // Another program rewrites the file (different title, so a different size).
        let mut other = app.buffers[i].score.clone();
        other.meta.title = "Changed elsewhere, with a longer title".into();
        cecilia_formats::write_file(&other, &file).unwrap();
        app.check_external();
        assert_eq!(app.confirm_reload, Some(i));
        assert!(app.buffers[i].modified);
        app.handle_key(key('y'));
        assert!(app.confirm_reload.is_none() && !app.buffers[i].modified);
        assert_eq!(app.buffers[i].score.meta.title, other.meta.title);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn declining_keeps_the_buffer_and_does_not_ask_again() {
        let (mut app, dir, file) = app_with_file("decline");
        let i = app.pane().buffer;
        let mut other = app.buffers[i].score.clone();
        other.meta.title = "Changed elsewhere, with a longer title".into();
        cecilia_formats::write_file(&other, &file).unwrap();
        app.check_external();
        app.handle_key(key('n'));
        assert!(app.buffers[i].modified);
        assert_eq!(app.buffers[i].score.meta.title, "Untitled");
        app.check_external();
        assert!(app.confirm_reload.is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_deleted_file_flags_the_buffer() {
        let (mut app, dir, file) = app_with_file("deleted");
        let i = app.pane().buffer;
        std::fs::remove_file(&file).unwrap();
        app.check_external();
        assert!(app.buffers[i].modified);
        assert!(app.confirm_reload.is_none());
        assert!(app.last_notification().is_some_and(|m| m.error));
        // :w recreates it, and the buffer is clean and quiet again.
        app.run(Command::Write(None));
        assert!(file.exists() && !app.buffers[i].modified);
        app.check_external();
        assert!(app.confirm_reload.is_none() && !app.buffers[i].modified);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn our_own_writes_are_not_outside_changes() {
        let (mut app, dir, _) = app_with_file("own");
        let i = app.pane().buffer;
        app.buffers[i].modified = true;
        app.run(Command::Write(None));
        app.check_external();
        assert!(app.confirm_reload.is_none() && !app.buffers[i].modified);
        std::fs::remove_dir_all(dir).ok();
    }
}
