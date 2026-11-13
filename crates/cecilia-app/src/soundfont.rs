//! Downloading the General MIDI SoundFont FluidR3_GM (`:soundfont-install`, `cecilia soundfont
//! install`) into cecilia's own soundfonts folder, which [`find_system_soundfont`] searches
//! first. It is too big to bundle (148 MB), so it is fetched on demand, checked against its
//! size and SHA-256, and only then moved into place: a failed or partial download never leaves
//! a broken `.sf2` behind.
//!
//! [`find_system_soundfont`]: crate::config::find_system_soundfont

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::App;
use crate::notify::{Notice, NoticeIcon};

/// A SoundFont that can be downloaded, as a member of a `.tar.gz` archive.
#[derive(Debug, Clone, Copy)]
pub struct Asset {
    /// The name it is installed under.
    pub file: &'static str,
    /// The `.tar.gz` archive holding it.
    pub url: &'static str,
    /// Its path inside the archive.
    pub member: &'static str,
    /// Size in bytes.
    pub size: u64,
    /// SHA-256 of the file, in lowercase hex.
    pub sha256: &'static str,
}

/// FluidR3_GM 3.1 by Frank Wen (MIT), from Debian's source archive of `fluid-soundfont` (the
/// original site is gone; Arch packages it from the same archive).
pub const FLUIDR3_GM: Asset = Asset {
    file: "FluidR3_GM.sf2",
    url: "https://deb.debian.org/debian/pool/main/f/fluid-soundfont/fluid-soundfont_3.1.orig.tar.gz",
    member: "fluid-soundfont-3.1/FluidR3_GM.sf2",
    size: 148_398_306,
    sha256: "74594e8f4250680adf590507a306655a299935343583256f3b722c48a1bc1cb0",
};

/// What an install did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Downloaded, checked and moved into place.
    Installed(PathBuf),
    /// Already there with the right size: nothing was downloaded.
    AlreadyInstalled(PathBuf),
}

impl Outcome {
    pub fn path(&self) -> &Path {
        match self {
            Outcome::Installed(p) | Outcome::AlreadyInstalled(p) => p,
        }
    }
}

/// cecilia's own soundfonts folder (`<data dir>/soundfonts`), the first one searched.
pub fn dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "cecilia").map(|d| d.data_dir().join("soundfonts"))
}

/// The installed file, if it is in `dir` with the right size.
pub fn installed(asset: &Asset, dir: &Path) -> Option<PathBuf> {
    let path = dir.join(asset.file);
    std::fs::metadata(&path)
        .is_ok_and(|m| m.is_file() && m.len() == asset.size)
        .then_some(path)
}

/// Megabytes, for messages.
pub fn mb(bytes: u64) -> u64 {
    (bytes + 500_000) / 1_000_000
}

/// Downloads `asset` into `dir` (see [`install_with`]), over HTTPS.
pub fn install(
    asset: &Asset,
    dir: &Path,
    force: bool,
    progress: impl FnMut(u64, u64),
) -> Result<Outcome, String> {
    install_with(asset, dir, force, || fetch(asset), progress)
}

/// Opens the download and reads its archive up to `asset.member`; the reader returned yields
/// that file's bytes as they arrive.
fn fetch(asset: &Asset) -> Result<impl Read + use<>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        .timeout_per_read(Some(Duration::from_secs(60)))
        .user_agent(concat!("cecilia/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let response = agent
        .get(asset.url)
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    let gz = flate2::read::GzDecoder::new(response.into_body().into_reader());
    tar_member(gz, asset.member)
}

/// Skips through a tar stream to the regular file `name` and returns a reader of its bytes.
/// Only what a plain ustar archive needs: 512-byte headers, octal sizes, zero padding.
fn tar_member<R: Read>(mut tar: R, name: &str) -> Result<std::io::Take<R>, String> {
    let fail = |e: std::io::Error| format!("download failed: {e}");
    let missing = || format!("download failed: {name} is not in the archive");
    let mut header = [0u8; 512];
    loop {
        tar.read_exact(&mut header).map_err(|e| match e.kind() {
            std::io::ErrorKind::UnexpectedEof => missing(),
            _ => fail(e),
        })?;
        if header.iter().all(|&b| b == 0) {
            return Err(missing()); // end of archive
        }
        let field = |r: std::ops::Range<usize>| {
            let f = &header[r];
            String::from_utf8_lossy(&f[..f.iter().position(|&b| b == 0).unwrap_or(f.len())])
                .into_owned()
        };
        let size = u64::from_str_radix(field(124..136).trim(), 8)
            .map_err(|_| "download failed: not a tar archive".to_string())?;
        let prefix = field(345..500);
        let path = match prefix.is_empty() {
            true => field(0..100),
            false => format!("{prefix}/{}", field(0..100)),
        };
        let regular = matches!(header[156], b'0' | 0);
        if regular && path == name {
            return Ok(tar.take(size));
        }
        let padded = size.div_ceil(512) * 512;
        let skipped =
            std::io::copy(&mut (&mut tar).take(padded), &mut std::io::sink()).map_err(fail)?;
        if skipped != padded {
            return Err(missing());
        }
    }
}

/// Installs `asset` into `dir` from what `open` reads: unless `force`, a file already there
/// with the right size is kept and `open` is not called. The bytes go to a temporary file in
/// `dir`, which is checked (size, SHA-256) and then renamed over `asset.file`; on any failure
/// it is removed. `progress` gets the bytes written so far and the expected total.
pub fn install_with<R: Read>(
    asset: &Asset,
    dir: &Path,
    force: bool,
    open: impl FnOnce() -> Result<R, String>,
    mut progress: impl FnMut(u64, u64),
) -> Result<Outcome, String> {
    if !force && let Some(path) = installed(asset, dir) {
        return Ok(Outcome::AlreadyInstalled(path));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    remove_stale_parts(asset, dir);
    let temp = dir.join(format!(".{}.{}.part", asset.file, std::process::id()));
    let target = dir.join(asset.file);
    let result = open()
        .and_then(|reader| write_verified(asset, reader, &temp, &mut progress))
        .and_then(|()| {
            std::fs::rename(&temp, &target)
                .map_err(|e| format!("can't move it to {}: {e}", target.display()))
        });
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map(|()| Outcome::Installed(target))
}

/// Copies `reader` to `temp`, hashing as it goes, and fails unless exactly `asset.size` bytes
/// with the right SHA-256 arrived.
fn write_verified(
    asset: &Asset,
    mut reader: impl Read,
    temp: &Path,
    progress: &mut impl FnMut(u64, u64),
) -> Result<(), String> {
    let write_err = |e: std::io::Error| format!("can't write {}: {e}", temp.display());
    let mut file = std::fs::File::create(temp).map_err(write_err)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0; 64 * 1024];
    let mut done = 0u64;
    progress(0, asset.size);
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("download failed: {e}")),
        };
        done += n as u64;
        if done > asset.size {
            return Err(format!(
                "download failed: more than the expected {} bytes",
                asset.size
            ));
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(write_err)?;
        progress(done, asset.size);
    }
    if done != asset.size {
        return Err(format!(
            "download incomplete: {done} of {} bytes",
            asset.size
        ));
    }
    let digest = format!("{:x}", hasher.finalize());
    if digest != asset.sha256 {
        return Err(format!(
            "checksum mismatch: got {digest}, expected {}",
            asset.sha256
        ));
    }
    file.sync_all().map_err(write_err)
}

/// Removes temporary files of downloads that were cut off (cecilia quit or crashed) and not
/// touched for ten minutes; a download running in another cecilia keeps writing to its own.
fn remove_stale_parts(asset: &Asset, dir: &Path) {
    let prefix = format!(".{}.", asset.file);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        let stale = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > Duration::from_secs(600));
        if name.starts_with(&prefix) && name.ends_with(".part") && stale {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// What the download thread reports.
pub(crate) enum Msg {
    /// Bytes so far and the total, at most once per percent.
    Progress(u64, u64),
    Done(Result<Outcome, String>),
}

/// A download running in the background (`:soundfont-install`).
pub(crate) struct Job {
    rx: Receiver<Msg>,
    /// The notification card showing the progress, while it is on screen.
    card: Option<u64>,
}

/// Runs [`install`] of FluidR3_GM on a thread.
fn spawn(dir: PathBuf, force: bool) -> Receiver<Msg> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut last = None;
        let progress = tx.clone();
        let result = install(&FLUIDR3_GM, &dir, force, |done, total| {
            let pct = done * 100 / total.max(1);
            if last != Some(pct) {
                last = Some(pct);
                let _ = progress.send(Msg::Progress(done, total));
            }
        });
        let _ = tx.send(Msg::Done(result));
    });
    rx
}

impl App {
    /// `:soundfont-install[!]`: downloads FluidR3_GM in the background (`force`: again, even if
    /// it is there).
    pub(crate) fn soundfont_install(&mut self, force: bool) {
        if self.soundfont_job.is_some() {
            self.error("SoundFont: a download is already running");
            return;
        }
        let Some(dir) = self.soundfont_dir.clone() else {
            self.error("SoundFont: no data folder to install into");
            return;
        };
        if !force && let Some(path) = installed(&FLUIDR3_GM, &dir) {
            let note = self.soundfont_note(&path);
            self.info(format!(
                "SoundFont: FluidR3_GM is already installed ({}){note}; :soundfont-install! downloads it again",
                path.display()
            ));
            return;
        }
        self.soundfont_start(spawn(dir, force));
    }

    /// Tracks a download (`rx`), showing its progress on a card that stays until it is over.
    pub(crate) fn soundfont_start(&mut self, rx: Receiver<Msg>) {
        let card = self.raise(Notice {
            icon: Some(NoticeIcon::Audio),
            timeout: Some(0.0),
            ..Notice::new(
                format!(
                    "SoundFont: downloading FluidR3_GM ({} MB)…",
                    mb(FLUIDR3_GM.size)
                ),
                false,
            )
        });
        self.soundfont_job = Some(Job {
            rx,
            card: Some(card),
        });
    }

    /// A download is running.
    pub fn soundfont_downloading(&self) -> bool {
        self.soundfont_job.is_some()
    }

    /// Picks up the download's progress and its end (called with [`App::poll_config`]).
    pub(crate) fn poll_soundfont(&mut self) {
        let Some(job) = &mut self.soundfont_job else {
            return;
        };
        let (mut latest, mut done) = (None, None);
        loop {
            match job.rx.try_recv() {
                Ok(Msg::Progress(a, b)) => latest = Some((a, b)),
                Ok(Msg::Done(r)) => {
                    done = Some(r);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    done = Some(Err("the download stopped unexpectedly".into()));
                    break;
                }
            }
        }
        if let (Some((a, b)), Some(id)) = (latest, job.card) {
            let text = format!(
                "SoundFont: downloading FluidR3_GM {}% ({} of {} MB)",
                a * 100 / b.max(1),
                mb(a),
                mb(b)
            );
            if !self.notifications.update(id, text) {
                job.card = None; // dismissed: no more progress
            }
        }
        let Some(result) = done else { return };
        if let Some(id) = self.soundfont_job.take().and_then(|j| j.card) {
            self.notifications.dismiss(id);
        }
        match result {
            Ok(outcome) => {
                let path = outcome.path().to_path_buf();
                let note = self.soundfont_note(&path);
                let installed = matches!(outcome, Outcome::Installed(_));
                self.info(if installed {
                    format!("SoundFont: FluidR3_GM installed ({}){note}", path.display())
                } else {
                    format!(
                        "SoundFont: FluidR3_GM is already installed ({}){note}",
                        path.display()
                    )
                });
                // The next playback picks it up, unless the setting names another one.
                if installed && self.config.file.audio.soundfont.is_none() {
                    self.effects.push(crate::Effect::ConfigReloaded);
                }
            }
            Err(e) => self.error(format!("SoundFont: {e}")),
        }
    }

    /// Whether playback will use the SoundFont at `path`, and if not why: the setting
    /// `audio.soundfont`, or another one found first.
    fn soundfont_note(&self, path: &Path) -> String {
        if let Some(set) = &self.config.file.audio.soundfont {
            return format!(
                "; audio.soundfont ({}) still takes precedence",
                set.display()
            );
        }
        let same = |a: &Path| {
            a == path
                || std::fs::canonicalize(a)
                    .ok()
                    .is_some_and(|a| std::fs::canonicalize(path).is_ok_and(|b| a == b))
        };
        match crate::config::find_system_soundfont() {
            Some(found) if !same(&found) => format!(
                "; {} is found first (set audio.soundfont to choose)",
                found.display()
            ),
            _ => "; playback uses it from now on".into(),
        }
    }

    /// Playback starts with the built-in synth because no SoundFont is set or found: says once
    /// per session how to get one.
    pub(crate) fn builtin_synth_hint(&mut self) {
        if self.soundfont_hinted || self.soundfont_job.is_some() {
            return;
        }
        self.soundfont_hinted = true;
        self.info(format!(
            "no SoundFont: built-in synth (:soundfont-install downloads FluidR3_GM, {} MB)",
            mb(FLUIDR3_GM.size)
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Config};
    use std::io::Cursor;

    const DATA: &[u8] = b"not really a SoundFont";

    fn asset() -> Asset {
        Asset {
            file: "Test.sf2",
            url: "http://unused.invalid/",
            member: "dir/Test.sf2",
            size: DATA.len() as u64,
            // Not the hash of DATA (see `real_asset`).
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        }
    }

    fn real_asset() -> Asset {
        Asset {
            sha256: Box::leak(format!("{:x}", Sha256::digest(DATA)).into_boxed_str()),
            ..asset()
        }
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cecilia-sf-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn files(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .map(|r| {
                r.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    #[test]
    fn the_right_bytes_are_installed() {
        let (dir, a) = (temp("ok"), real_asset());
        let mut seen = Vec::new();
        let out = install_with(
            &a,
            &dir,
            false,
            || Ok(Cursor::new(DATA)),
            |d, t| seen.push((d, t)),
        );
        assert_eq!(out, Ok(Outcome::Installed(dir.join("Test.sf2"))));
        assert_eq!(std::fs::read(dir.join("Test.sf2")).unwrap(), DATA);
        assert_eq!(files(&dir), ["Test.sf2"], "no temporary file left");
        assert_eq!(seen.first(), Some(&(0, a.size)));
        assert_eq!(seen.last(), Some(&(a.size, a.size)));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_wrong_hash_leaves_no_file() {
        let dir = temp("hash");
        let out = install_with(&asset(), &dir, false, || Ok(Cursor::new(DATA)), |_, _| {});
        assert!(out.unwrap_err().contains("checksum mismatch"));
        assert_eq!(files(&dir), Vec::<String>::new());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn short_long_and_failed_downloads_leave_no_file() {
        let (dir, a) = (temp("short"), real_asset());
        let short = install_with(&a, &dir, false, || Ok(Cursor::new(&DATA[1..])), |_, _| {});
        assert!(short.unwrap_err().contains("incomplete"));
        let long = [DATA, b"!"].concat();
        let long = install_with(&a, &dir, false, || Ok(Cursor::new(long)), |_, _| {});
        assert!(long.unwrap_err().contains("more than"));
        let failed = install_with(
            &a,
            &dir,
            false,
            || Err::<Cursor<&[u8]>, _>("offline".into()),
            |_, _| {},
        );
        assert_eq!(failed, Err("offline".into()));
        assert_eq!(files(&dir), Vec::<String>::new());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_installed_file_is_kept_unless_forced() {
        let (dir, a) = (temp("skip"), real_asset());
        std::fs::create_dir_all(&dir).unwrap();
        // Right size, other bytes: only the size is checked for "installed".
        std::fs::write(dir.join("Test.sf2"), vec![b'x'; DATA.len()]).unwrap();
        let out = install_with(
            &a,
            &dir,
            false,
            || -> Result<Cursor<&[u8]>, String> { panic!("must not download") },
            |_, _| {},
        );
        assert_eq!(out, Ok(Outcome::AlreadyInstalled(dir.join("Test.sf2"))));
        let out = install_with(&a, &dir, true, || Ok(Cursor::new(DATA)), |_, _| {});
        assert_eq!(out, Ok(Outcome::Installed(dir.join("Test.sf2"))));
        assert_eq!(std::fs::read(dir.join("Test.sf2")).unwrap(), DATA);
        // A file of another size is not taken for the installed one.
        std::fs::write(dir.join("Test.sf2"), b"short").unwrap();
        assert_eq!(installed(&a, &dir), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A ustar archive of `(path, typeflag, bytes)` entries.
    fn tar(entries: &[(&str, u8, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (path, kind, data) in entries {
            let mut h = [0u8; 512];
            h[..path.len()].copy_from_slice(path.as_bytes());
            h[124..135].copy_from_slice(format!("{:011o}", data.len()).as_bytes());
            h[156] = *kind;
            out.extend_from_slice(&h);
            out.extend_from_slice(data);
            out.resize(out.len().div_ceil(512) * 512, 0);
        }
        out.extend_from_slice(&[0; 1024]);
        out
    }

    #[test]
    fn the_member_is_read_out_of_the_archive() {
        let archive = tar(&[
            ("dir/", b'5', b""),
            ("dir/Other.sf2", b'0', &[7; 700]),
            ("dir/Test.sf2", b'0', DATA),
            ("dir/README", b'0', b"readme"),
        ]);
        let (dir, a) = (temp("tar"), real_asset());
        let out = install_with(
            &a,
            &dir,
            false,
            || tar_member(Cursor::new(archive), a.member),
            |_, _| {},
        );
        assert_eq!(out, Ok(Outcome::Installed(dir.join("Test.sf2"))));
        assert_eq!(std::fs::read(dir.join("Test.sf2")).unwrap(), DATA);
        std::fs::remove_dir_all(&dir).unwrap();

        let without = tar(&[("dir/README", b'0', b"readme")]);
        let err = tar_member(Cursor::new(without), "dir/Test.sf2")
            .err()
            .unwrap();
        assert!(err.contains("not in the archive"), "{err}");
        let cut = &tar(&[("dir/Other.sf2", b'0', &[7; 700])])[..600];
        assert!(tar_member(Cursor::new(cut), "dir/Test.sf2").is_err());
        assert!(tar_member(Cursor::new(b"<html>not found</html>"), "x").is_err());
    }

    fn message(a: &App) -> String {
        a.last_notification()
            .map(|m| m.text.clone())
            .unwrap_or_default()
    }

    #[test]
    fn the_command_parses() {
        assert_eq!(
            Command::parse("soundfont-install"),
            Ok(Command::SoundfontInstall { force: false })
        );
        assert_eq!(
            Command::parse("soundfont-install!"),
            Ok(Command::SoundfontInstall { force: true })
        );
    }

    #[test]
    fn the_command_skips_an_installed_file() {
        let dir = temp("cmd");
        std::fs::create_dir_all(&dir).unwrap();
        // A sparse file of the right size stands for the installed SoundFont.
        let f = std::fs::File::create(dir.join(FLUIDR3_GM.file)).unwrap();
        f.set_len(FLUIDR3_GM.size).unwrap();
        let mut app = App::new(Config::default(), None);
        app.soundfont_dir = Some(dir.clone());
        app.run(Command::SoundfontInstall { force: false });
        assert!(!app.soundfont_downloading());
        assert!(
            message(&app).contains("already installed"),
            "{}",
            message(&app)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_second_download_is_refused_and_progress_updates_one_card() {
        let mut app = App::new(Config::default(), None);
        let (tx, rx) = channel();
        app.soundfont_start(rx);
        app.run(Command::SoundfontInstall { force: true });
        assert_eq!(message(&app), "SoundFont: a download is already running");

        tx.send(Msg::Progress(0, 200_000_000)).unwrap();
        tx.send(Msg::Progress(50_000_000, 200_000_000)).unwrap();
        app.poll_soundfont();
        let cards = app.notifications_shown().0;
        let card = cards
            .iter()
            .find(|c| c.text.contains("downloading"))
            .unwrap();
        assert_eq!(
            card.text,
            "SoundFont: downloading FluidR3_GM 25% (50 of 200 MB)"
        );
        assert_eq!(card.count, 1, "progress is not a repeat");

        let path = PathBuf::from("/x/FluidR3_GM.sf2");
        tx.send(Msg::Done(Ok(Outcome::Installed(path)))).unwrap();
        app.poll_soundfont();
        assert!(!app.soundfont_downloading());
        assert!(message(&app).starts_with("SoundFont: FluidR3_GM installed (/x/FluidR3_GM.sf2)"));
        assert!(
            !app.notifications_shown()
                .0
                .iter()
                .any(|c| c.text.contains("downloading"))
        );
        assert!(app.take_effects().contains(&crate::Effect::ConfigReloaded));
    }

    #[test]
    fn a_set_soundfont_keeps_precedence_and_failures_are_errors() {
        let mut app = App::new(Config::default(), None);
        app.config.file.audio.soundfont = Some("/sf/Mine.sf2".into());
        let (tx, rx) = channel();
        app.soundfont_start(rx);
        tx.send(Msg::Done(Ok(Outcome::Installed(
            "/x/FluidR3_GM.sf2".into(),
        ))))
        .unwrap();
        app.poll_soundfont();
        assert!(
            message(&app).ends_with("; audio.soundfont (/sf/Mine.sf2) still takes precedence"),
            "{}",
            message(&app)
        );
        assert!(!app.take_effects().contains(&crate::Effect::ConfigReloaded));

        let (tx, rx) = channel();
        app.soundfont_start(rx);
        tx.send(Msg::Done(Err("checksum mismatch".into()))).unwrap();
        app.poll_soundfont();
        let last = app.last_notification().unwrap();
        assert!(last.error);
        assert_eq!(last.text, "SoundFont: checksum mismatch");

        // A thread that died without a word is a failure too.
        let (tx, rx) = channel::<Msg>();
        app.soundfont_start(rx);
        drop(tx);
        app.poll_soundfont();
        assert!(!app.soundfont_downloading());
        assert!(message(&app).contains("stopped unexpectedly"));
    }

    #[test]
    fn the_builtin_synth_hint_shows_once() {
        let mut app = App::new(Config::default(), None);
        app.builtin_synth_hint();
        assert!(message(&app).contains(":soundfont-install downloads FluidR3_GM, 148 MB"));
        let n = app.notifications.history().len();
        app.builtin_synth_hint();
        assert_eq!(app.notifications.history().len(), n);
    }
}
