//! The system clipboard, through the platform's own tools (`wl-copy`, `xclip`, `xsel`,
//! `pbcopy`, PowerShell), with a copy kept here for when none of them works.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// The last text copied, for when the system clipboard can't be reached.
#[cfg(not(test))]
static LOCAL: Mutex<String> = Mutex::new(String::new());

// (Tests run in parallel: each has its own.)
#[cfg(test)]
thread_local! {
    static LOCAL: Mutex<String> = const { Mutex::new(String::new()) };
}

#[cfg(not(test))]
fn local<R>(f: impl FnOnce(&Mutex<String>) -> R) -> R {
    f(&LOCAL)
}

#[cfg(test)]
fn local<R>(f: impl FnOnce(&Mutex<String>) -> R) -> R {
    LOCAL.with(f)
}

/// The tools that put text on the clipboard (read from stdin), most likely first.
fn setters() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut v = Vec::new();
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        v.push(("wl-copy", vec![]));
    }
    v.push(("xclip", vec!["-selection", "clipboard"]));
    v.push(("xsel", vec!["--clipboard", "--input"]));
    v.push(("pbcopy", vec![]));
    v.push(("clip.exe", vec![]));
    v
}

/// The tools that print the clipboard.
fn getters() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut v = Vec::new();
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        v.push(("wl-paste", vec!["--no-newline"]));
    }
    v.push(("xclip", vec!["-selection", "clipboard", "-o"]));
    v.push(("xsel", vec!["--clipboard", "--output"]));
    v.push(("pbpaste", vec![]));
    v.push((
        "powershell.exe",
        vec!["-NoProfile", "-Command", "Get-Clipboard -Raw"],
    ));
    v
}

/// Copies `text` to the clipboard.
pub fn set(text: &str) {
    local(|l| *l.lock().unwrap() = text.to_string());
    if cfg!(test) {
        return;
    }
    for (tool, args) in setters() {
        let Ok(mut child) = Command::new(tool)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            continue;
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        if child.wait().is_ok_and(|s| s.success()) {
            return;
        }
    }
}

/// The text on the clipboard (what was last copied here, if it can't be read).
pub fn get() -> String {
    if !cfg!(test) {
        for (tool, args) in getters() {
            let out = Command::new(tool)
                .args(&args)
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output();
            if let Ok(out) = out
                && out.status.success()
            {
                return String::from_utf8_lossy(&out.stdout).into_owned();
            }
        }
    }
    local(|l| l.lock().unwrap().clone())
}
