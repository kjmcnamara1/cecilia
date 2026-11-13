//! The Windows installer's folder on the user's `PATH`, so `cecilia` (the terminal interface)
//! runs from any terminal. Added after install and update (copies installed before this existed
//! get it at their next update), removed before uninstall. Velopack installs to
//! `%LocalAppData%\cecilia\current`, a folder that keeps its path across updates.
//!
//! The list editing is plain string work, tested everywhere; the registry part is Windows only.

/// Whether two `PATH` entries name the same folder (Windows paths: case and a trailing `\` do
/// not matter).
fn same_dir(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        s.trim()
            .trim_matches('"')
            .trim_end_matches(['\\', '/'])
            .to_lowercase()
    };
    norm(a) == norm(b)
}

/// `path` (a `;`-separated `PATH` value) with `dir` appended, or None when it is already there.
#[cfg_attr(not(windows), allow(dead_code))]
pub(super) fn with_dir(path: &str, dir: &str) -> Option<String> {
    if path.split(';').any(|e| same_dir(e, dir)) {
        return None;
    }
    let path = path.trim_end_matches(';');
    Some(if path.is_empty() {
        dir.to_string()
    } else {
        format!("{path};{dir}")
    })
}

/// `path` without `dir`, or None when it is not there.
#[cfg_attr(not(windows), allow(dead_code))]
pub(super) fn without_dir(path: &str, dir: &str) -> Option<String> {
    let entries: Vec<&str> = path.split(';').collect();
    let kept: Vec<&str> = entries
        .iter()
        .copied()
        .filter(|e| !same_dir(e, dir))
        .collect();
    (kept.len() != entries.len()).then(|| kept.join(";"))
}

#[cfg(windows)]
pub(super) use registry::{add, remove};

#[cfg(windows)]
mod registry {
    use std::path::PathBuf;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_EXPAND_SZ, REG_SZ, RegCloseKey,
        RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// The folder holding this executable (`…\cecilia\current`).
    fn exe_dir() -> Option<String> {
        let exe: PathBuf = std::env::current_exe().ok()?;
        Some(exe.parent()?.to_str()?.to_string())
    }

    /// Adds the install folder to the user's `PATH`.
    pub fn add() {
        if let Some(dir) = exe_dir() {
            edit(|path| super::with_dir(path, &dir));
        }
    }

    /// Takes the install folder off the user's `PATH`.
    pub fn remove() {
        if let Some(dir) = exe_dir() {
            edit(|path| super::without_dir(path, &dir));
        }
    }

    /// Rewrites `HKCU\Environment\Path` with `change` (None: leave it) and tells running
    /// programs (Explorer, so new terminals) that the environment changed. The value is read
    /// unexpanded and written back with its type, so `%VAR%` entries stay as they were.
    fn edit(change: impl FnOnce(&str) -> Option<String>) {
        let mut key: HKEY = null_mut();
        let subkey = wide("Environment");
        // SAFETY: plain Win32 registry calls with buffers sized as the API reports.
        unsafe {
            if RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                KEY_READ | KEY_WRITE,
                &mut key,
            ) != ERROR_SUCCESS
            {
                return;
            }
            let name = wide("Path");
            let mut kind = REG_EXPAND_SZ;
            let mut bytes = 0u32;
            let found = RegQueryValueExW(
                key,
                name.as_ptr(),
                null(),
                &mut kind,
                null_mut(),
                &mut bytes,
            ) == ERROR_SUCCESS;
            let mut current = String::new();
            if found {
                let mut buf = vec![0u16; (bytes as usize).div_ceil(2)];
                if RegQueryValueExW(
                    key,
                    name.as_ptr(),
                    null(),
                    &mut kind,
                    buf.as_mut_ptr().cast(),
                    &mut bytes,
                ) != ERROR_SUCCESS
                {
                    RegCloseKey(key);
                    return;
                }
                buf.truncate((bytes as usize) / 2);
                while buf.last() == Some(&0) {
                    buf.pop();
                }
                current = String::from_utf16_lossy(&buf);
            }
            if kind != REG_SZ {
                kind = REG_EXPAND_SZ;
            }
            if let Some(new) = change(&current) {
                let data = wide(&new);
                let ok = RegSetValueExW(
                    key,
                    name.as_ptr(),
                    0,
                    kind,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                ) == ERROR_SUCCESS;
                if ok {
                    let env = wide("Environment");
                    SendMessageTimeoutW(
                        HWND_BROADCAST,
                        WM_SETTINGCHANGE,
                        0,
                        env.as_ptr() as isize,
                        SMTO_ABORTIFHUNG,
                        2000,
                        null_mut(),
                    );
                }
            }
            RegCloseKey(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = r"C:\Users\me\AppData\Local\cecilia\current";

    #[test]
    fn adds_once() {
        assert_eq!(with_dir("", DIR).as_deref(), Some(DIR));
        assert_eq!(
            with_dir(r"C:\a;%USERPROFILE%\b;", DIR),
            Some(format!(r"C:\a;%USERPROFILE%\b;{DIR}"))
        );
        assert_eq!(with_dir(&format!(r"C:\a;{DIR}"), DIR), None);
        let other_case = r"c:\users\ME\appdata\local\Cecilia\current\";
        assert_eq!(with_dir(&format!(r"{other_case};C:\a"), DIR), None);
    }

    #[test]
    fn removes_every_copy_and_keeps_the_rest() {
        let path = format!(r"C:\a;{DIR};%X%\b;{}\", DIR.to_uppercase());
        assert_eq!(without_dir(&path, DIR), Some(r"C:\a;%X%\b".to_string()));
        assert_eq!(without_dir(r"C:\a;C:\b", DIR), None);
        assert_eq!(without_dir(DIR, DIR), Some(String::new()));
    }
}
