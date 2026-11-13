//! The Windows installer's updates, through Velopack (Windows only). Kept to Velopack and the
//! standard library so the rest of the update logic is testable everywhere.

use std::sync::Mutex;

use velopack::sources::HttpSource;
use velopack::{UpdateCheck, UpdateInfo, UpdateManager, VelopackApp};

/// Where Velopack finds the release feed (`releases.win.json`) and packages: the assets of the
/// latest GitHub release (`latest` skips pre-releases), uploaded by the release workflow.
const FEED: &str = "https://github.com/kjmcnamara1/cecilia/releases/latest/download";

/// The release downloaded this session, installed when cecilia exits.
static READY: Mutex<Option<Box<UpdateInfo>>> = Mutex::new(None);

/// Velopack's startup hooks: during install, update and uninstall the installer runs the
/// executable with hook arguments, which this handles and exits on. A release downloaded but
/// not installed (cecilia was killed before it could hand it over) is applied here too.
/// Install and update put the install folder on the user's `PATH`; uninstall takes it off.
pub fn startup() {
    VelopackApp::build()
        .on_after_install_fast_callback(|_| super::user_path::add())
        .on_after_update_fast_callback(|_| super::user_path::add())
        .on_before_uninstall_fast_callback(|_| super::user_path::remove())
        .run();
}

/// The update manager, or None when this copy was not installed by the Windows installer.
fn manager() -> Option<UpdateManager> {
    UpdateManager::new(HttpSource::new(FEED), None, None).ok()
}

/// Whether this copy was installed by the Windows installer (and so can update itself).
pub fn installed() -> bool {
    manager().is_some()
}

/// A newer release on the feed: its version, and what to download.
pub fn check() -> Result<Option<(String, Box<UpdateInfo>)>, String> {
    let manager = manager().ok_or("not installed by the Windows installer")?;
    match manager.check_for_updates().map_err(|e| e.to_string())? {
        UpdateCheck::UpdateAvailable(info) => {
            Ok(Some((info.TargetFullRelease.Version.clone(), info)))
        }
        UpdateCheck::NoUpdateAvailable | UpdateCheck::RemoteIsEmpty => Ok(None),
    }
}

/// Downloads a release (Velopack checks it against the feed's hashes) and keeps it to install
/// when cecilia exits.
pub fn download(info: Box<UpdateInfo>) -> Result<(), String> {
    let manager = manager().ok_or("not installed by the Windows installer")?;
    let target: &UpdateInfo = &info;
    manager
        .download_updates(target, None)
        .map_err(|e| e.to_string())?;
    *READY.lock().unwrap_or_else(|e| e.into_inner()) = Some(info);
    Ok(())
}

/// Hands the release downloaded this session to Velopack's updater, which waits for cecilia to
/// exit, installs it silently and does not restart it.
pub fn apply_on_exit() {
    let Some(info) = READY.lock().unwrap_or_else(|e| e.into_inner()).take() else {
        return;
    };
    let Some(manager) = manager() else { return };
    let target: &UpdateInfo = &info;
    if let Err(e) = manager.wait_exit_then_apply_updates(target, true, false, Vec::<String>::new())
    {
        // Not lost: the next start applies a downloaded release (see `startup`).
        eprintln!("cecilia: the update is installed at the next start ({e})");
    }
}
