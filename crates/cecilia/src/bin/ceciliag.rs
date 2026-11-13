//! `ceciliag`: cecilia's window, always (`cecilia --gui`). On Windows it is a program of the GUI
//! subsystem, so the Start Menu shortcut opens no console window; errors show in a message box.

#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() -> std::process::ExitCode {
    match cecilia::main(true) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            cecilia::show_error(&e);
            std::process::ExitCode::FAILURE
        }
    }
}
