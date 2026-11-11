//! Swap files: a recovery copy of every buffer with unsaved changes, written a few seconds after
//! the last change and removed once the buffer is saved or closed, so work survives a crash.
//!
//! A swap file is a small text header (magic, owning process id, the buffer's path) followed by
//! the score in the native `.ccc` / `.ccm` format, which stores the whole score model.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::app::{App, Buffer, Effect, clamp_cursor};

const MAGIC: &str = "CECILIASWAP2";
/// The first version: the score only, no undo history.
const MAGIC_V1: &str = "CECILIASWAP1";

/// Written after this many times the interval at the latest, however busy the editing.
const MAX_WAIT_FACTOR: u32 = 6;

#[derive(Default)]
pub(crate) struct SwapState {
    /// Buffer index → the swap file written for it and the buffer version it holds.
    written: HashMap<usize, (PathBuf, u64)>,
    /// Buffer index → the version last seen, when it changed and when it was first unsaved.
    dirty: HashMap<usize, (u64, Instant, Instant)>,
    /// Swap files found on open and not yet recovered or discarded: autosave leaves them alone.
    held: HashSet<PathBuf>,
    /// Messages about swap files found on open, shown at the next opportunity.
    notices: Vec<(String, bool)>,
    /// A write failed and was reported; stays quiet until one works.
    failed: bool,
}

/// What a swap file says about its writer.
pub(crate) struct SwapInfo {
    pub pid: u32,
    pub score: cecilia_core::Score,
    /// The undo tree, so recovering keeps it (absent in version 1 files).
    pub history: Option<crate::history::History>,
}

/// Default swap directory: `$XDG_STATE_HOME/cecilia/swap`, else `~/.local/state/cecilia/swap`.
pub fn default_swap_dir() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            directories::BaseDirs::new().map(|d| d.home_dir().join(".local").join("state"))
        })?;
    Some(state.join("cecilia").join("swap"))
}

/// Swap file name for a buffer path: the full path with `%` and `/` escaped (`%25`, `%2F`),
/// so different paths never share a name. Very long names keep their tail and gain a hash.
pub(crate) fn swap_name(path: &Path) -> String {
    let mut name = String::new();
    for c in path.to_string_lossy().chars() {
        match c {
            '%' => name.push_str("%25"),
            '/' | '\\' => name.push_str("%2F"),
            c => name.push(c),
        }
    }
    if name.len() > 200 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        name.hash(&mut h);
        let mut cut = name.len() - 200;
        while !name.is_char_boundary(cut) {
            cut += 1;
        }
        name = format!("{:016x}-{}", h.finish(), &name[cut..]);
    }
    format!("{name}.swp")
}

/// Whether another process with this id exists. Where that can't be told (not Unix) it is
/// assumed to, so a swap file is never taken from a cecilia that may still be running.
fn pid_alive(pid: u32) -> bool {
    if pid == std::process::id() {
        return false;
    }
    #[cfg(unix)]
    {
        // 0 and ids beyond pid_t would address process groups or nothing at all.
        let Ok(pid) = libc::pid_t::try_from(pid).ok().filter(|p| *p > 0).ok_or(()) else {
            return false;
        };
        // SAFETY: signal 0 only checks that the process exists and may be signalled.
        if unsafe { libc::kill(pid, 0) } == 0 {
            return true;
        }
        // EPERM: it exists but belongs to someone else.
        std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn modified_time(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Writes `bytes` to `path` through a temp file and a rename, so an existing file is never
/// left half written.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        std::fs::remove_file(&tmp).ok();
    })
}

pub(crate) fn read_swap(path: &Path) -> Result<SwapInfo, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut rest = &bytes[..];
    let mut line = || {
        let end = rest.iter().position(|&b| b == b'\n')?;
        let (l, r) = rest.split_at(end);
        rest = &r[1..];
        Some(String::from_utf8_lossy(l).into_owned())
    };
    let bad = || "not a cecilia swap file".to_string();
    let magic = line();
    let v2 = magic.as_deref() == Some(MAGIC);
    if !v2 && magic.as_deref() != Some(MAGIC_V1) {
        return Err(bad());
    }
    let pid = line().and_then(|l| l.parse().ok()).ok_or_else(bad)?;
    line().ok_or_else(bad)?; // the buffer's path (informational)
    if !v2 {
        let score = cecilia_formats::native::read_cecilia(rest).map_err(|e| e.to_string())?;
        return Ok(SwapInfo {
            pid,
            score,
            history: None,
        });
    }
    let len: usize = line().and_then(|l| l.parse().ok()).ok_or_else(bad)?;
    if len > rest.len() {
        return Err(bad());
    }
    let (score, history) = rest.split_at(len);
    let score = cecilia_formats::native::read_cecilia(score).map_err(|e| e.to_string())?;
    // A history that doesn't parse costs only the undo tree, not the recovered score.
    let history = serde_json::from_slice(history).ok();
    Ok(SwapInfo {
        pid,
        score,
        history,
    })
}

impl App {
    fn swap_dir(&self) -> Option<PathBuf> {
        match &self.config.file.swap.dir {
            Some(d) => Some(match d.strip_prefix("~") {
                Ok(rest) => directories::BaseDirs::new()
                    .map(|b| b.home_dir().join(rest))
                    .unwrap_or_else(|| d.clone()),
                Err(_) => d.clone(),
            }),
            None => default_swap_dir(),
        }
    }

    /// The swap file for a buffer: named by its path, or by process and slot when unnamed.
    fn swap_file(&self, index: usize) -> Option<PathBuf> {
        let name = match &self.buffers.get(index)?.path {
            Some(p) => swap_name(p),
            None => format!("unnamed-{}-{index}.swp", std::process::id()),
        };
        Some(self.swap_dir()?.join(name))
    }

    /// Writes a buffer's swap file now (atomically); never touches `modified` or the history.
    pub(crate) fn swap_write(&mut self, index: usize) {
        let (Some(file), Some(buffer)) = (self.swap_file(index), self.buffers.get(index)) else {
            return;
        };
        let result = (|| -> Result<(), String> {
            let dir = file.parent().ok_or("no swap directory")?;
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            let body =
                cecilia_formats::native::write_cecilia(&buffer.score).map_err(|e| e.to_string())?;
            let path = buffer.path.as_deref().map(|p| p.display().to_string());
            let history = serde_json::to_vec(&buffer.history).map_err(|e| e.to_string())?;
            let mut bytes = format!(
                "{MAGIC}\n{}\n{}\n{}\n",
                std::process::id(),
                path.unwrap_or_default().replace('\n', " "),
                body.len()
            )
            .into_bytes();
            bytes.extend(body);
            bytes.extend(history);
            write_atomic(&file, &bytes).map_err(|e| e.to_string())
        })();
        match result {
            Ok(()) => {
                self.swap.failed = false;
                let version = self.buffers[index].version;
                self.swap.written.insert(index, (file, version));
            }
            Err(e) => {
                if !self.swap.failed {
                    self.error(format!("swap file not written: {e}"));
                }
                self.swap.failed = true;
            }
        }
    }

    /// Called about once a second: writes swap files for buffers that changed and have been
    /// quiet for `swap.interval` seconds, and removes those of buffers that no longer need one.
    pub(crate) fn swap_tick(&mut self, now: Instant) {
        self.show_swap_notices();
        let cfg = self.config.file.swap.clone();
        if !cfg.enabled {
            return self.swap_clear();
        }
        // Swap files that went stale: saved, closed, undone to the saved state or renamed.
        let stale: Vec<usize> = self
            .swap
            .written
            .iter()
            .filter(|(i, (file, _))| {
                let i = **i;
                !self.buffers.get(i).is_some_and(|b| b.modified)
                    || self.swap_file(i).as_ref() != Some(file)
            })
            .map(|(&i, _)| i)
            .collect();
        for i in stale {
            if let Some((file, _)) = self.swap.written.remove(&i) {
                std::fs::remove_file(file).ok();
            }
        }
        let interval = Duration::from_secs_f64(cfg.interval.max(0.0));
        for i in 0..self.buffers.len() {
            let b = &self.buffers[i];
            if !b.modified {
                self.swap.dirty.remove(&i);
                continue;
            }
            let version = b.version;
            let held = self
                .swap_file(i)
                .is_some_and(|f| self.swap.held.contains(&f));
            let written = self.swap.written.get(&i).map(|(_, v)| *v);
            let entry = self.swap.dirty.entry(i).or_insert((version, now, now));
            if entry.0 != version {
                entry.0 = version;
                entry.1 = now;
            }
            let (_, changed, first) = *entry;
            let due = now.duration_since(changed) >= interval
                || now.duration_since(first) >= interval * MAX_WAIT_FACTOR;
            if due && written != Some(version) && !held {
                self.swap_write(i);
                if let Some(e) = self.swap.dirty.get_mut(&i) {
                    e.2 = now;
                }
            }
        }
    }

    /// Removes every swap file this session wrote (a clean quit).
    pub(crate) fn swap_clear(&mut self) {
        self.swap.dirty.clear();
        for (_, (file, _)) in self.swap.written.drain() {
            std::fs::remove_file(file).ok();
        }
    }

    /// A buffer was saved: its swap file (and any unresolved one for the same path) goes.
    pub(crate) fn swap_release(&mut self, index: usize) {
        if let Some(file) = self.swap_file(index) {
            if self.swap.held.remove(&file) || self.swap.written.contains_key(&index) {
                std::fs::remove_file(&file).ok();
            }
        }
        self.swap.written.remove(&index);
        self.swap.dirty.remove(&index);
    }

    /// Looks for a swap file left by an earlier session of the file being opened; if it is
    /// newer than the file, queues a message. With `swap.auto_recover` (and no running cecilia
    /// owning it) its score is returned for the caller to load as unsaved changes; otherwise
    /// the file is held (no autosave over it) until `:recover` or `:swap-discard`.
    pub(crate) fn swap_check_open(
        &mut self,
        path: &Path,
    ) -> Option<(cecilia_core::Score, Option<crate::history::History>)> {
        if !self.config.file.swap.enabled {
            return None;
        }
        let dir = self.swap_dir()?;
        let file = dir.join(swap_name(path));
        let (Some(swap_time), Ok(info)) = (modified_time(&file), read_swap(&file)) else {
            return None;
        };
        if info.pid == std::process::id() {
            return None;
        }
        if modified_time(path).is_some_and(|t| t >= swap_time) {
            return None; // older than the file: stale
        }
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into());
        let alive = pid_alive(info.pid);
        if self.config.file.swap.auto_recover && !alive {
            self.swap.notices.push((
                format!(
                    "\"{name}\": recovered unsaved changes from a swap file (:w saves them, :bd! then :open gets the saved file back)"
                ),
                false,
            ));
            return Some((info.score, info.history));
        }
        self.swap.held.insert(file);
        self.swap.notices.push((if alive {
            format!(
                "\"{name}\": swap file of a running cecilia (pid {}); :recover! to take it anyway, :swap-discard to delete it",
                info.pid
            )
        } else {
            format!(
                "\"{name}\": unsaved changes found in a swap file (crash?): :recover to restore, :swap-discard to delete (not autosaving until then)"
            )
        }, true));
        None
    }

    /// Shows queued swap notices now (after the opened file's own message).
    pub(crate) fn swap_show_notices(&mut self) {
        self.show_swap_notices();
    }

    fn show_swap_notices(&mut self) {
        let notices = std::mem::take(&mut self.swap.notices);
        if notices.is_empty() {
            return;
        }
        let error = notices.iter().any(|n| n.1);
        let text = notices
            .into_iter()
            .map(|n| n.0)
            .collect::<Vec<_>>()
            .join("; ");
        if error {
            self.error(text);
        } else {
            self.info(text);
        }
    }

    /// `:recover [swapfile]`: replace the buffer with its swap file's score (left unsaved), or
    /// open a given swap file (for an unnamed buffer) as a new buffer.
    pub(crate) fn recover(&mut self, given: Option<PathBuf>, force: bool) {
        let buffer = self.pane().buffer;
        let Some(file) = given.clone().or_else(|| self.swap_file(buffer)) else {
            return self.error("recover: no swap directory");
        };
        let info = match read_swap(&file) {
            Ok(i) => i,
            Err(e) if !file.exists() => {
                return self.error(format!("recover: no swap file {} ({e})", file.display()));
            }
            Err(e) => return self.error(format!("recover: {}: {e}", file.display())),
        };
        if pid_alive(info.pid) && !force {
            return self.error(format!(
                "recover: swap file belongs to a running cecilia (pid {}); :recover! to take it anyway",
                info.pid
            ));
        }
        self.swap.held.remove(&file);
        if given.is_some() {
            self.buffers.push(Buffer {
                path: None,
                name: "[Recovered]".into(),
                score: info.score,
                modified: true,
                history: info.history.unwrap_or_default(),
                version: 0,
                disk: None,
            });
            let b = self.buffers.len() - 1;
            self.show_buffer(b);
            self.swap_write(b);
            if self.swap.written.get(&b).is_some_and(|(f, _)| *f != file) {
                std::fs::remove_file(&file).ok();
            }
        } else {
            let b = &mut self.buffers[buffer];
            b.score = info.score;
            b.history = info.history.unwrap_or_default();
            b.modified = true;
            b.version += 1;
            self.effects.push(Effect::ScoreChanged { buffer });
            let score = &self.buffers[buffer].score;
            for pane in self.panes.values_mut().filter(|p| p.buffer == buffer) {
                clamp_cursor(score, &mut pane.cursor);
            }
        }
        self.info("recovered from swap file; :w to keep it");
    }

    /// `:swap-discard`: delete the buffer's swap file.
    pub(crate) fn swap_discard(&mut self) {
        let buffer = self.pane().buffer;
        let Some(file) = self.swap_file(buffer) else {
            return self.error("swap-discard: no swap directory");
        };
        self.swap.held.remove(&file);
        self.swap.written.remove(&buffer);
        self.swap.dirty.remove(&buffer);
        match std::fs::remove_file(&file) {
            Ok(()) => self.info("swap file deleted"),
            Err(_) => self.info("no swap file"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_alive_asks_the_os() {
        assert!(
            !pid_alive(std::process::id()),
            "this process is not 'another' one"
        );
        assert!(!pid_alive(0) && !pid_alive(u32::MAX));
        #[cfg(unix)]
        {
            assert!(pid_alive(1), "init exists (EPERM counts as alive)");
            let mut child = std::process::Command::new("true").spawn().unwrap();
            let pid = child.id();
            child.wait().unwrap();
            assert!(!pid_alive(pid));
        }
    }
    use crate::app::tests::test_app;
    use crate::command::Command;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cecilia-swap-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn app_in(dir: &Path) -> App {
        let mut app = test_app();
        app.config.file.swap.dir = Some(dir.join("swap"));
        app.config.file.swap.interval = 5.0;
        app
    }

    fn swaps(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir.join("swap"))
            .map(|r| r.flatten().map(|e| e.path()).collect())
            .unwrap_or_default()
    }

    fn make_file(dir: &Path) -> PathBuf {
        let file = dir.join("a.ccm");
        let mut app = test_app();
        app.run(Command::Write(Some(file.clone())));
        file
    }

    #[test]
    fn names_are_unique_per_path() {
        assert_eq!(
            swap_name(Path::new("/home/u/s.gp5")),
            "%2Fhome%2Fu%2Fs.gp5.swp"
        );
        assert_ne!(swap_name(Path::new("/a%2Fb")), swap_name(Path::new("/a/b")));
        let long = swap_name(Path::new(&format!("/{}", "x".repeat(400))));
        assert!(long.len() < 255);
    }

    #[test]
    fn default_dir_follows_xdg_state_home() {
        // Only the pure logic is checked: the variable is process-global.
        let dir = default_swap_dir().unwrap();
        assert!(dir.ends_with("cecilia/swap"));
    }

    #[test]
    fn writes_after_quiet_period_and_removes_on_save() {
        let dir = tmp("write");
        let file = make_file(&dir);
        let mut app = app_in(&dir);
        app.run(Command::Edit(file.clone()));
        let t0 = Instant::now();
        app.swap_tick(t0);
        assert!(swaps(&dir).is_empty());
        crate::app::tests::keys(&mut app, "i5<Esc>");
        assert!(app.buffer().modified);
        app.swap_tick(t0);
        assert!(swaps(&dir).is_empty());
        app.swap_tick(t0 + Duration::from_secs(6));
        let files = swaps(&dir);
        assert_eq!(files.len(), 1);
        // Autosave neither saves nor touches undo state.
        assert!(app.buffer().modified);
        let info = read_swap(&files[0]).unwrap();
        assert_eq!(info.score, app.buffer().score);
        assert!(!files[0].with_extension("swp.tmp").exists());
        app.run(Command::Write(None));
        assert!(swaps(&dir).is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn disabled_and_quit_remove_files() {
        let dir = tmp("quit");
        let mut app = app_in(&dir);
        app.run(Command::New);
        crate::app::tests::keys(&mut app, "i5<Esc>");
        let t0 = Instant::now() + Duration::from_secs(100);
        app.swap_tick(t0);
        app.swap_tick(t0 + Duration::from_secs(6));
        assert_eq!(swaps(&dir).len(), 1);
        app.run(Command::Quit { force: true });
        assert!(app.quit && swaps(&dir).is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn open_offers_recovery_then_recovers_or_discards() {
        let dir = tmp("recover");
        let file = make_file(&dir);
        // A crashed session: its swap file with a changed score and a dead pid.
        let mut crashed = app_in(&dir);
        crashed.run(Command::Edit(file.clone()));
        crate::app::tests::keys(&mut crashed, "i7<Esc>");
        let changed = crashed.buffer().score.clone();
        let t0 = Instant::now();
        crashed.swap_tick(t0);
        crashed.swap_tick(t0 + Duration::from_secs(6));
        let swap = swaps(&dir).remove(0);
        let mut bytes = std::fs::read(&swap).unwrap();
        let header = format!("{MAGIC}\n{}\n", std::process::id());
        let dead = format!("{MAGIC}\n{}\n", 4_000_000_000u32);
        bytes.splice(0..header.len(), dead.bytes());
        std::fs::write(&swap, bytes).unwrap();
        // Make the swap newer than the file.
        let later = std::time::SystemTime::now() + Duration::from_secs(60);
        std::fs::File::options()
            .write(true)
            .open(&swap)
            .unwrap()
            .set_modified(later)
            .unwrap();

        let mut app = app_in(&dir);
        app.config.file.swap.auto_recover = false;
        app.run(Command::Edit(file.clone()));
        app.swap_show_notices();
        let msg = app.last_notification().unwrap();
        assert!(msg.error && msg.text.contains(":recover"), "{}", msg.text);
        // Held: no autosave over it while unresolved.
        crate::app::tests::keys(&mut app, "i3<Esc>");
        app.swap_tick(t0);
        app.swap_tick(t0 + Duration::from_secs(60));
        assert!(read_swap(&swap).unwrap().score == changed);
        app.run(Command::Recover(None, false));
        assert!(app.buffer().modified && app.buffer().score == changed);
        app.run(Command::SwapDiscard);
        assert!(swaps(&dir).is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn live_owner_needs_force() {
        let dir = tmp("live");
        let file = make_file(&dir);
        let mut other = app_in(&dir);
        other.run(Command::Edit(file.clone()));
        crate::app::tests::keys(&mut other, "i7<Esc>");
        let t0 = Instant::now();
        other.swap_tick(t0);
        other.swap_tick(t0 + Duration::from_secs(6));
        let mut app = app_in(&dir);
        app.run(Command::Edit(file));
        assert!(
            !app.buffer().modified,
            "a live owner is never auto-recovered"
        );
        // Our own pid is never "another process", so no refusal here; a foreign live pid is
        // simulated by pid 1 (init), when /proc exists.
        if cfg!(target_os = "linux") {
            let swap = swaps(&dir).remove(0);
            let mut bytes = std::fs::read(&swap).unwrap();
            let header = format!("{MAGIC}\n{}\n", std::process::id());
            bytes.splice(0..header.len(), format!("{MAGIC}\n1\n").bytes());
            std::fs::write(&swap, bytes).unwrap();
            app.run(Command::Recover(None, false));
            assert!(app.last_notification().unwrap().error);
            assert!(!app.buffer().modified);
            app.run(Command::Recover(None, true));
            assert!(app.buffer().modified);
        }
        std::fs::remove_dir_all(dir).ok();
    }

    /// A crashed session's swap file for `file`, newer than it, owned by `pid`.
    fn crashed_swap(dir: &Path, file: &Path, pid: u32) -> cecilia_core::Score {
        let mut crashed = app_in(dir);
        crashed.run(Command::Edit(file.to_path_buf()));
        crate::app::tests::keys(&mut crashed, "i7<Esc>");
        let changed = crashed.buffer().score.clone();
        let t0 = Instant::now();
        crashed.swap_tick(t0);
        crashed.swap_tick(t0 + Duration::from_secs(6));
        let swap = swaps(dir).remove(0);
        let mut bytes = std::fs::read(&swap).unwrap();
        let header = format!("{MAGIC}\n{}\n", std::process::id());
        bytes.splice(0..header.len(), format!("{MAGIC}\n{pid}\n").bytes());
        std::fs::write(&swap, bytes).unwrap();
        let later = std::time::SystemTime::now() + Duration::from_secs(60);
        let f = std::fs::File::options().write(true).open(&swap).unwrap();
        f.set_modified(later).unwrap();
        changed
    }

    #[test]
    fn auto_recover_restores_by_default() {
        let dir = tmp("auto");
        let file = make_file(&dir);
        let changed = crashed_swap(&dir, &file, 4_000_000_000);
        let mut app = app_in(&dir);
        assert!(app.config.file.swap.auto_recover);
        app.run(Command::Edit(file.clone()));
        assert!(app.buffer().modified && app.buffer().score == changed);
        let msg = app.last_notification().unwrap();
        assert!(!msg.error && msg.text.contains("recovered"), "{}", msg.text);
        // The undo tree came back too: undo returns to the saved file, redo to the recovery.
        crate::app::tests::keys(&mut app, "u");
        assert!(!app.buffer().modified);
        assert_eq!(
            app.buffer().score,
            cecilia_formats::read_file(&file).unwrap()
        );
        crate::app::tests::keys(&mut app, "U");
        assert!(app.buffer().modified && app.buffer().score == changed);
        // The file on disk is untouched, and closing the buffer drops the swap file.
        assert_ne!(cecilia_formats::read_file(&file).unwrap(), changed);
        app.run(Command::BufferClose(None, true));
        assert!(swaps(&dir).is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn auto_recover_off_only_warns() {
        let dir = tmp("autooff");
        let file = make_file(&dir);
        let changed = crashed_swap(&dir, &file, 4_000_000_000);
        let mut app = app_in(&dir);
        app.config.file.swap.auto_recover = false;
        app.run(Command::Edit(file));
        assert!(!app.buffer().modified && app.buffer().score != changed);
        let msg = app.last_notification().unwrap();
        assert!(msg.error && msg.text.contains(":recover"), "{}", msg.text);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn live_owner_is_not_auto_recovered() {
        if !cfg!(target_os = "linux") {
            return;
        }
        let dir = tmp("autolive");
        let file = make_file(&dir);
        let changed = crashed_swap(&dir, &file, 1);
        let mut app = app_in(&dir);
        app.run(Command::Edit(file));
        assert!(!app.buffer().modified && app.buffer().score != changed);
        assert!(
            app.last_notification()
                .unwrap()
                .text
                .contains("running cecilia")
        );
        std::fs::remove_dir_all(dir).ok();
    }
}
