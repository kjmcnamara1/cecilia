//! Embeds the icon (and version info) in the Windows executables, so Explorer, the taskbar and
//! shortcuts show it and the window can load it (`cecilia-gui` asks for icon resource 1). The
//! icon lives inside this crate because the crate is published: the build reads nothing from
//! outside it.

fn main() {
    println!("cargo:rerun-if-changed=assets/cecilia.ico");
    // The target, not the host: cfg!(windows) here would describe the machine building.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/cecilia.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=no icon embedded in the executable: {e}");
        }
    }
}
