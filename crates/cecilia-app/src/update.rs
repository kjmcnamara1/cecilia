//! Updates: looking for a newer release (`ui.auto_update` at startup, at most once a day, and
//! `:update`) and, for a copy installed by the Windows installer (Velopack), downloading it in
//! the background and installing it when cecilia exits. Every other install (Scoop, winget,
//! Chocolatey, cargo, a Linux package, an unpacked archive) is only told how to update: package
//! managers own their files.
//!
//! The network work runs on a thread that reports to [`App`] through a channel, picked up by
//! [`App::poll_config`]. Tests never start it: they feed [`Event`]s to [`App::update_event`].

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use crate::app::App;
use crate::config::{AutoUpdate, Mode};
use crate::keys::{Key, KeyCode};

#[cfg(windows)]
mod velopack;

/// The releases page, for installs nothing else updates.
pub const RELEASES: &str = "https://github.com/kjmcnamara1/cecilia/releases/latest";
/// The latest release (not a pre-release or draft) as JSON.
const LATEST_API: &str = "https://api.github.com/repos/kjmcnamara1/cecilia/releases/latest";
/// Seconds between startup checks.
const DAY: u64 = 86_400;
/// The file in the state directory holding when the last startup check succeeded (Unix time).
const STAMP_FILE: &str = "update-check";

/// The version running.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Velopack's startup hooks, first thing in `main` (they run during install, update and
/// uninstall, and exit). Nothing outside Windows.
pub fn startup() {
    #[cfg(windows)]
    velopack::startup();
}

/// Hands a release downloaded this session to the installer's updater, which installs it once
/// cecilia has exited; called last in `main`. Nothing when there is none (or outside Windows).
pub fn apply_on_exit() {
    #[cfg(windows)]
    velopack::apply_on_exit();
}

/// How this copy of cecilia was installed, which decides who may update it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallKind {
    /// The Windows installer (Velopack): updates itself.
    Velopack,
    Scoop,
    Winget,
    Chocolatey,
    /// `cargo install`.
    Cargo,
    /// A system package (`/usr/bin`: .deb, .rpm, the AUR).
    System,
    /// Anything else: an unpacked archive, `install.sh`, a build from source.
    Manual,
}

impl InstallKind {
    /// The kind of install an executable at `exe` belongs to; `velopack`: Velopack found its
    /// install around this executable. Windows and Unix separators are both understood, and
    /// case is ignored, so the rules are testable on any system.
    pub fn detect(exe: &Path, velopack: bool) -> InstallKind {
        if velopack {
            return InstallKind::Velopack;
        }
        let text = exe.to_string_lossy().to_lowercase();
        let parts: Vec<&str> = text.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
        let has = |a: &str, b: &[&str]| parts.windows(2).any(|w| w[0] == a && b.contains(&w[1]));
        if has("scoop", &["apps", "shims"]) {
            InstallKind::Scoop
        } else if has("winget", &["packages", "links"]) {
            InstallKind::Winget
        } else if has("chocolatey", &["lib", "bin"]) {
            InstallKind::Chocolatey
        } else if has(".cargo", &["bin"]) {
            InstallKind::Cargo
        } else if text.starts_with("/usr/bin/") || text.starts_with("/bin/") {
            InstallKind::System
        } else {
            InstallKind::Manual
        }
    }

    /// Whether cecilia may install a release itself.
    pub fn self_updates(self) -> bool {
        self == InstallKind::Velopack
    }

    /// How to get a newer release onto this install.
    pub fn how_to_update(self) -> String {
        match self {
            InstallKind::Velopack => ":update installs it".into(),
            InstallKind::Scoop => "run `scoop update cecilia`".into(),
            InstallKind::Winget => "run `winget upgrade kjmcnamara1.cecilia`".into(),
            InstallKind::Chocolatey => "run `choco upgrade cecilia`".into(),
            InstallKind::Cargo => "run `cargo install --locked cecilia`".into(),
            InstallKind::System => "update it with your package manager".into(),
            InstallKind::Manual => format!("download it from {RELEASES}"),
        }
    }
}

/// What to do about a newer release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Download it now and install it when cecilia exits.
    Install,
    /// Ask whether to install it.
    Ask,
    /// Say it exists and how to update.
    Notify,
}

/// What to do about a newer release found by the startup check (`manual`: by `:update`). Only
/// an install that updates itself installs or asks; `:update` installs there whatever the
/// setting.
pub fn action(setting: AutoUpdate, kind: InstallKind, manual: bool) -> Action {
    if !kind.self_updates() {
        return Action::Notify;
    }
    if manual {
        return Action::Install;
    }
    match setting {
        AutoUpdate::On => Action::Install,
        AutoUpdate::Ask => Action::Ask,
        AutoUpdate::Notify | AutoUpdate::Off => Action::Notify,
    }
}

/// Whether the startup check runs: never with `off`, else once a day (`last`: when the last one
/// succeeded; a time in the future, from a changed clock, does not hold it back).
pub fn check_due(setting: AutoUpdate, last: Option<u64>, now: u64) -> bool {
    setting != AutoUpdate::Off && last.is_none_or(|t| t > now || now - t >= DAY)
}

/// Whether release `tag` (`v1.2.3` or `1.2.3`) is newer than version `current`; tags that are
/// not versions never are.
pub fn is_newer(tag: &str, current: &str) -> bool {
    let parse = |v: &str| semver::Version::parse(v.trim().trim_start_matches('v')).ok();
    matches!((parse(tag), parse(current)), (Some(t), Some(c)) if t > c)
}

/// A newer release.
#[derive(Debug)]
pub struct Release {
    /// Without the `v`.
    pub version: String,
    /// What Velopack downloads (installs made by the Windows installer only).
    #[cfg(windows)]
    package: Option<Box<::velopack::UpdateInfo>>,
}

impl Release {
    /// A release known only by its version.
    pub fn new(version: impl Into<String>) -> Release {
        Release {
            version: version.into(),
            #[cfg(windows)]
            package: None,
        }
    }
}

/// What the update thread reports.
#[derive(Debug)]
pub enum Event {
    /// A check finished: the newer release, if any (`manual`: started by `:update`).
    Checked {
        manual: bool,
        kind: InstallKind,
        result: Result<Option<Release>, String>,
    },
    /// The download of `version` finished.
    Downloaded {
        version: String,
        result: Result<(), String>,
    },
}

/// The update work of an [`App`].
#[derive(Debug, Default)]
pub struct UpdateState {
    /// The thread checking or downloading, while it runs.
    rx: Option<Receiver<Event>>,
    /// A release downloaded this session, installed when cecilia exits.
    ready: Option<String>,
    /// The release the open prompt asks about.
    pub(crate) offer: Option<Release>,
    /// Where the time of the last successful startup check is kept (set only by
    /// [`App::check_for_updates_on_start`], so tests never write it).
    stamp: Option<PathBuf>,
}

impl UpdateState {
    /// Whether a check or download is running.
    pub fn busy(&self) -> bool {
        self.rx.is_some()
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The install this executable belongs to.
fn this_install() -> InstallKind {
    #[cfg(windows)]
    let velopack = velopack::installed();
    #[cfg(not(windows))]
    let velopack = false;
    match std::env::current_exe() {
        Ok(exe) => InstallKind::detect(&exe.canonicalize().unwrap_or(exe), velopack),
        Err(_) => InstallKind::Manual,
    }
}

/// The tag of the latest GitHub release.
fn latest_tag() -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Latest {
        tag_name: String,
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();
    let mut response = agent
        .get(LATEST_API)
        .header("User-Agent", concat!("cecilia/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?;
    let latest: Latest = response.body_mut().read_json().map_err(|e| e.to_string())?;
    Ok(latest.tag_name)
}

/// Looks for a release newer than this one: on Velopack's feed for its installs, on GitHub for
/// the rest. Runs on the update thread.
fn check(kind: InstallKind) -> Result<Option<Release>, String> {
    #[cfg(windows)]
    if kind == InstallKind::Velopack {
        return Ok(velopack::check()?.map(|(version, info)| Release {
            version,
            package: Some(info),
        }));
    }
    let _ = kind;
    let tag = latest_tag()?;
    Ok(is_newer(&tag, CURRENT).then(|| Release::new(tag.trim_start_matches('v'))))
}

/// Downloads a release for the installer to install on exit. Runs on the update thread.
fn download(release: Release) -> Result<(), String> {
    #[cfg(windows)]
    if let Some(info) = release.package {
        return velopack::download(info);
    }
    let _ = release;
    Err("only the Windows installer's install updates itself".into())
}

impl App {
    /// The startup check (the frontends call this once): as `ui.auto_update` says, at most
    /// once a day; with `off` nothing connects to the network.
    pub fn check_for_updates_on_start(&mut self) {
        let setting = self.config.file.ui.auto_update;
        let stamp = crate::notify::state_file(STAMP_FILE);
        let last = stamp
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| s.trim().parse().ok());
        if !check_due(setting, last, unix_now()) || self.update.busy() {
            return;
        }
        self.update.stamp = stamp;
        self.start_update_check(false);
    }

    /// `:update`: checks now whatever `ui.auto_update` says, and installs a newer release when
    /// this install updates itself (else says how to update).
    pub(crate) fn update_command(&mut self) {
        if let Some(v) = &self.update.ready {
            return self.info(format!(
                "update: cecilia v{v} is ready; it is installed when cecilia exits"
            ));
        }
        if self.update.busy() {
            return self.info("update: already checking");
        }
        self.info("update: looking for a newer release…");
        self.start_update_check(true);
    }

    fn start_update_check(&mut self, manual: bool) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let kind = this_install();
            let result = check(kind);
            let _ = tx.send(Event::Checked {
                manual,
                kind,
                result,
            });
        });
        self.update.rx = Some(rx);
    }

    fn start_download(&mut self, release: Release) {
        let (tx, rx) = channel();
        let version = release.version.clone();
        self.info(format!("update: downloading cecilia v{version}…"));
        std::thread::spawn(move || {
            let result = download(release);
            let _ = tx.send(Event::Downloaded { version, result });
        });
        self.update.rx = Some(rx);
    }

    /// Picks up what the update thread reports (from the frontends' poll). Waits while another
    /// question is open or a command line is being typed, so a prompt never takes their keys.
    pub(crate) fn poll_update(&mut self) {
        let Some(rx) = &self.update.rx else { return };
        if self.question_open() || self.mode == Mode::Command {
            return;
        }
        match rx.try_recv() {
            Ok(event) => {
                self.update.rx = None;
                self.update_event(event);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.update.rx = None,
        }
    }

    /// Whether a prompt is waiting for its answer.
    pub(crate) fn question_open(&self) -> bool {
        self.confirm_close.is_some()
            || self.confirm_reload.is_some()
            || self.confirm_delete_workspace.is_some()
            || self.update.offer.is_some()
    }

    /// Acts on a report of the update thread.
    pub fn update_event(&mut self, event: Event) {
        match event {
            Event::Checked {
                manual,
                kind,
                result,
            } => {
                if !manual
                    && result.is_ok()
                    && let Some(stamp) = self.update.stamp.take()
                {
                    if let Some(dir) = stamp.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = std::fs::write(stamp, unix_now().to_string());
                }
                match result {
                    // A failed startup check stays quiet (offline is normal); a failed :update
                    // says why.
                    Err(e) if manual => self.error(format!("update: {e}")),
                    Err(_) => {}
                    Ok(None) if manual => {
                        self.info(format!("update: cecilia v{CURRENT} is up to date"))
                    }
                    Ok(None) => {}
                    Ok(Some(release)) => {
                        let setting = self.config.file.ui.auto_update;
                        match action(setting, kind, manual) {
                            Action::Install => self.start_download(release),
                            Action::Ask => self.ask_update(release),
                            Action::Notify => self.info(format!(
                                "update: cecilia v{} is available (this is v{CURRENT}); {}",
                                release.version,
                                kind.how_to_update()
                            )),
                        }
                    }
                }
            }
            Event::Downloaded { version, result } => match result {
                Ok(()) => {
                    self.info(format!(
                        "update: cecilia v{version} is ready; it is installed when cecilia exits"
                    ));
                    self.update.ready = Some(version);
                }
                Err(e) => self.error(format!("update: downloading v{version} failed: {e}")),
            },
        }
    }

    fn ask_update(&mut self, release: Release) {
        self.ask(
            format!(
                "cecilia v{} is available (this is v{CURRENT}): install it when cecilia exits? (y/n)",
                release.version
            ),
            &[("Install", KeyCode::Char('y')), ("Not now", KeyCode::Esc)],
        );
        self.update.offer = Some(release);
    }

    /// The answer to the update prompt: `y` downloads the release, anything else leaves it
    /// (the next day's check asks again).
    pub(crate) fn answer_update(&mut self, release: Release, key: Key) {
        self.answered();
        if matches!(key.code, KeyCode::Char('y' | 'Y')) {
            self.start_download(release);
        } else {
            self.info("update: not now (:update installs it)");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use crate::command::Command;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn the_install_kind_comes_from_where_the_executable_is() {
        use InstallKind::*;
        for (exe, kind) in [
            (r"C:\Users\me\scoop\apps\cecilia\current\cecilia.exe", Scoop),
            (r"D:\Scoop\shims\cecilia.exe", Scoop),
            (
                r"C:\Users\me\AppData\Local\Microsoft\WinGet\Packages\kjmcnamara1.cecilia_Microsoft.Winget.Source_8wekyb3d8bbwe\cecilia-v0.46.0-x86_64-pc-windows-msvc\cecilia.exe",
                Winget,
            ),
            (
                r"C:\Users\me\AppData\Local\Microsoft\WinGet\Links\cecilia.exe",
                Winget,
            ),
            (
                r"C:\ProgramData\chocolatey\lib\cecilia\tools\cecilia-v0.46.0-x86_64-pc-windows-msvc\ceciliag.exe",
                Chocolatey,
            ),
            (r"\\?\C:\Users\me\.cargo\bin\cecilia.exe", Cargo),
            ("/home/me/.cargo/bin/cecilia", Cargo),
            ("/usr/bin/cecilia", System),
            ("/usr/local/bin/cecilia", Manual),
            ("/home/me/.local/bin/cecilia", Manual),
            ("/home/me/src/cecilia/target/release/cecilia", Manual),
            (r"C:\Users\me\Downloads\cecilia-v0.46.0\cecilia.exe", Manual),
        ] {
            assert_eq!(InstallKind::detect(&p(exe), false), kind, "{exe}");
        }
        // Velopack's own check wins over the path.
        assert_eq!(
            InstallKind::detect(
                &p(r"C:\Users\me\AppData\Local\cecilia\current\ceciliag.exe"),
                true
            ),
            Velopack
        );
        assert_eq!(
            InstallKind::detect(
                &p(r"C:\Users\me\AppData\Local\cecilia\current\ceciliag.exe"),
                false
            ),
            Manual
        );
    }

    #[test]
    fn only_the_windows_installer_installs_or_asks() {
        use Action::*;
        use AutoUpdate as S;
        use InstallKind::*;
        let kinds = [Velopack, Scoop, Winget, Chocolatey, Cargo, System, Manual];
        for kind in kinds {
            for setting in [S::On, S::Ask, S::Notify, S::Off] {
                for manual in [false, true] {
                    let want = match (kind, manual, setting) {
                        (Velopack, true, _) | (Velopack, false, S::On) => Install,
                        (Velopack, false, S::Ask) => Ask,
                        _ => Notify,
                    };
                    assert_eq!(
                        action(setting, kind, manual),
                        want,
                        "{kind:?} {setting:?} manual={manual}"
                    );
                }
            }
        }
    }

    #[test]
    fn each_install_says_how_to_update_it() {
        assert!(
            InstallKind::Scoop
                .how_to_update()
                .contains("scoop update cecilia")
        );
        assert!(
            InstallKind::Winget
                .how_to_update()
                .contains("winget upgrade kjmcnamara1.cecilia")
        );
        assert!(
            InstallKind::Chocolatey
                .how_to_update()
                .contains("choco upgrade cecilia")
        );
        assert!(InstallKind::Cargo.how_to_update().contains("cargo install"));
        assert!(
            InstallKind::System
                .how_to_update()
                .contains("package manager")
        );
        assert!(InstallKind::Manual.how_to_update().contains(RELEASES));
        assert!(InstallKind::Velopack.how_to_update().contains(":update"));
    }

    #[test]
    fn the_startup_check_runs_once_a_day_and_never_when_off() {
        let now = 1_800_000_000;
        for s in [AutoUpdate::On, AutoUpdate::Ask, AutoUpdate::Notify] {
            assert!(check_due(s, None, now));
            assert!(!check_due(s, Some(now - 3600), now));
            assert!(check_due(s, Some(now - DAY), now));
            // A clock set back does not stop checks for good.
            assert!(check_due(s, Some(now + 3600), now));
        }
        assert!(!check_due(AutoUpdate::Off, None, now));
        assert!(!check_due(AutoUpdate::Off, Some(0), now));
    }

    #[test]
    fn versions_compare_as_semver() {
        assert!(is_newer("v0.47.0", "0.46.0"));
        assert!(is_newer("0.46.1", "0.46.0"));
        assert!(is_newer("v0.100.0", "0.99.9"));
        assert!(is_newer("v0.47.0", "0.47.0-rc.1"));
        assert!(!is_newer("v0.46.0", "0.46.0"));
        assert!(!is_newer("v0.45.9", "0.46.0"));
        assert!(!is_newer("v0.47.0-rc.1", "0.47.0"));
        assert!(!is_newer("nightly", "0.46.0"));
    }

    #[test]
    fn auto_update_is_an_option_defaulting_to_notify() {
        let app = App::new(Config::default(), None);
        assert_eq!(app.config.file.ui.auto_update, AutoUpdate::Notify);
        let (c, errors) = Config::parse("[ui]\nauto_update = \"off\"\n", &[]).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(c.file.ui.auto_update, AutoUpdate::Off);
        let e = Config::parse("[ui]\nauto_update = \"sometimes\"\n", &[])
            .err()
            .unwrap();
        assert!(e.contains("`on`, `ask`, `notify`, `off`"), "{e}");
        let mut app = App::new(Config::default(), None);
        app.run(Command::parse("set ui.auto_update ask").unwrap());
        assert_eq!(app.config.file.ui.auto_update, AutoUpdate::Ask);
    }

    fn app_with(setting: AutoUpdate) -> App {
        let mut config = Config::default();
        config.file.ui.auto_update = setting;
        let mut app = App::new(config, None);
        app.clear_notifications();
        app
    }

    fn found(manual: bool, kind: InstallKind) -> Event {
        Event::Checked {
            manual,
            kind,
            result: Ok(Some(Release::new("9.0.0"))),
        }
    }

    fn last_text(app: &App) -> String {
        app.last_notification()
            .map(|n| n.text.clone())
            .unwrap_or_default()
    }

    #[test]
    fn a_package_manager_install_is_told_the_command_even_with_on() {
        for setting in [AutoUpdate::On, AutoUpdate::Ask, AutoUpdate::Notify] {
            let mut app = app_with(setting);
            app.update_event(found(false, InstallKind::Scoop));
            let text = last_text(&app);
            assert!(
                text.contains("v9.0.0 is available") && text.contains("scoop update cecilia"),
                "{text}"
            );
            assert!(!app.update.busy() && app.update.offer.is_none());
        }
    }

    #[test]
    fn ask_asks_and_y_downloads() {
        let mut app = app_with(AutoUpdate::Ask);
        app.update_event(found(false, InstallKind::Velopack));
        assert_eq!(
            app.update.offer.as_ref().map(|r| r.version.as_str()),
            Some("9.0.0")
        );
        // The prompt takes the next key.
        app.handle_key(Key::new(KeyCode::Char('y')));
        assert!(app.update.offer.is_none());
        assert!(app.update.busy(), "a download started");
        assert!(last_text(&app).contains("downloading cecilia v9.0.0"));
        // (The download itself is Velopack's; here its report is fed in.)
        app.update.rx = None;
        app.update_event(Event::Downloaded {
            version: "9.0.0".into(),
            result: Ok(()),
        });
        assert!(last_text(&app).contains("v9.0.0 is ready"));
        // :update now says it is waiting for the exit instead of checking again.
        app.run(Command::parse("update").unwrap());
        assert!(!app.update.busy());
        assert!(last_text(&app).contains("installed when cecilia exits"));
    }

    #[test]
    fn declining_the_prompt_leaves_the_release() {
        let mut app = app_with(AutoUpdate::Ask);
        app.update_event(found(false, InstallKind::Velopack));
        app.handle_key(Key::new(KeyCode::Esc));
        assert!(app.update.offer.is_none() && !app.update.busy());
        assert!(last_text(&app).contains("not now"));
    }

    #[test]
    fn on_downloads_without_asking() {
        let mut app = app_with(AutoUpdate::On);
        app.update_event(found(false, InstallKind::Velopack));
        assert!(app.update.offer.is_none());
        assert!(app.update.busy());
        assert!(last_text(&app).contains("downloading"));
    }

    #[test]
    fn notify_on_the_windows_installer_points_at_update() {
        let mut app = app_with(AutoUpdate::Notify);
        app.update_event(found(false, InstallKind::Velopack));
        assert!(!app.update.busy());
        assert!(last_text(&app).contains(":update installs it"));
        // :update installs whatever the setting.
        app.update_event(found(true, InstallKind::Velopack));
        assert!(app.update.busy());
    }

    #[test]
    fn a_failed_startup_check_is_quiet_but_update_says_why() {
        let mut app = app_with(AutoUpdate::Notify);
        app.update_event(Event::Checked {
            manual: false,
            kind: InstallKind::Manual,
            result: Err("offline".into()),
        });
        assert!(app.last_notification().is_none());
        app.update_event(Event::Checked {
            manual: true,
            kind: InstallKind::Manual,
            result: Err("offline".into()),
        });
        assert!(
            app.last_notification()
                .is_some_and(|n| n.error && n.text.contains("offline"))
        );
        // Nothing newer: only :update says so.
        let mut app = app_with(AutoUpdate::Notify);
        app.update_event(Event::Checked {
            manual: false,
            kind: InstallKind::Manual,
            result: Ok(None),
        });
        assert!(app.last_notification().is_none());
        app.update_event(Event::Checked {
            manual: true,
            kind: InstallKind::Manual,
            result: Ok(None),
        });
        assert!(last_text(&app).contains("up to date"));
    }

    #[test]
    fn a_failed_download_is_an_error() {
        let mut app = app_with(AutoUpdate::On);
        app.update_event(Event::Downloaded {
            version: "9.0.0".into(),
            result: Err("disk full".into()),
        });
        assert!(
            app.last_notification()
                .is_some_and(|n| n.error && n.text.contains("disk full"))
        );
        assert!(app.update.ready.is_none());
    }

    #[test]
    fn the_report_waits_while_another_question_is_open() {
        let mut app = app_with(AutoUpdate::Ask);
        let (tx, rx) = channel();
        app.update.rx = Some(rx);
        tx.send(found(false, InstallKind::Velopack)).unwrap();
        app.confirm_reload = Some(0);
        app.poll_update();
        assert!(app.update.offer.is_none() && app.update.busy());
        app.confirm_reload = None;
        app.poll_update();
        assert!(app.update.offer.is_some() && !app.update.busy());
    }

    #[test]
    fn update_is_a_command() {
        assert_eq!(Command::parse("update"), Ok(Command::Update));
    }
}
