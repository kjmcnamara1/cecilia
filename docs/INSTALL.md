# Installing cecilia

Pick the line that fits your system. Every release is on the
[releases page](https://github.com/kjmcnamara1/cecilia/releases); building from source is covered
in the [README](../README.md#installation).

| System  | Way                                    | Command                                                                  |
| ------- | -------------------------------------- | ------------------------------------------------------------------------ |
| any     | install script (Linux, macOS)          | `curl -fsSL https://github.com/kjmcnamara1/cecilia/releases/latest/download/install.sh \| sh` |
| macOS   | Homebrew cask (the app)                | `brew install --cask kjmcnamara1/cecilia/cecilia`                        |
| macOS   | Homebrew formula (command line only)   | `brew install kjmcnamara1/cecilia/cecilia`                               |
| macOS   | disk image                             | download `cecilia-vX.Y.Z-universal-apple-darwin.dmg`, drag to Applications |
| Linux   | Flatpak                                | `flatpak install --user cecilia-vX.Y.Z.flatpak` (from the release)       |
| Linux   | Snap                                   | `sudo snap install cecilia`                                              |
| Linux   | AppImage                               | `chmod +x cecilia-vX.Y.Z-x86_64.AppImage && ./cecilia-vX.Y.Z-x86_64.AppImage` |
| Arch    | AUR                                    | `yay -S cecilia` (builds), `cecilia-bin` (release binary), `cecilia-git` |
| Debian  | `.deb`                                 | `sudo apt install ./cecilia-editor_X.Y.Z_amd64.deb`                      |
| Fedora  | `.rpm`                                 | `sudo dnf install ./cecilia-editor-X.Y.Z-1.x86_64.rpm`                   |
| Windows | Scoop, Winget, Chocolatey, zip         | see [Windows](#windows)                                                  |
| any     | `cargo binstall` (release binary)      | `cargo binstall cecilia`                                                 |
| any     | `cargo install` (builds from source)   | `cargo install --locked cecilia`                                         |

Downloads can be checked against `SHA256SUMS` on the release page. The macOS and Windows builds
and the Flatpak, Snap and AppImage are new and have had little testing on real machines: please
report problems.

## The install script

```sh
curl -fsSL https://github.com/kjmcnamara1/cecilia/releases/latest/download/install.sh | sh
```

It picks the build for your system, checks it against `SHA256SUMS`, and installs without root:

- **Linux (x86_64)**: the binary, desktop entry, icon and file types under `~/.local`
  (`CECILIA_PREFIX` changes that; put `~/.local/bin` on your `PATH`). Remove it with
  `packaging/linux/uninstall.sh` from a source checkout or release archive.
- **macOS**: `Cecilia.app` in `~/Applications` (`CECILIA_APP_DIR`) and a `cecilia` command in
  `~/.local/bin` (`CECILIA_BIN_DIR`). Downloads made this way carry no quarantine flag, so there is
  no Gatekeeper prompt.
- `CECILIA_VERSION=0.46.0` installs that version instead of the latest.

Other systems get a message pointing at `cargo binstall cecilia` or `cargo install`.

## macOS

`Cecilia.app` opens the window; the `cecilia` command inside it (`Cecilia.app/Contents/MacOS/cecilia`,
linked onto your `PATH` by the install script and the Homebrew cask) starts the terminal interface
unless you pass `--gui`. Files open from the command line (`cecilia --gui song.gp5`) or from
inside the app; double-clicking a score in Finder does not open it in cecilia yet.

The app is **not signed or notarized** (that needs a paid Apple developer account), so macOS
Gatekeeper refuses a `.dmg` download at first:

- right-click `Cecilia.app`, choose **Open**, then **Open** again; or
- `xattr -dr com.apple.quarantine /Applications/Cecilia.app`.

The Homebrew cask and the install script do this for you. The tap `kjmcnamara1/cecilia` holds
both the cask (`--cask`, the app plus the command) and the formula (the command only, also for
Linux).

The first time the tuner opens, macOS asks for microphone access.

## Linux

- **Flatpak**: download `cecilia-vX.Y.Z.flatpak` from the release. The app ID is
  `io.github.kjmcnamara1.cecilia`. It can read your home folder; its configuration lives in
  `~/.var/app/io.github.kjmcnamara1.cecilia/config/cecilia`, and a SoundFont has to be somewhere
  under your home directory. Flathub is planned.
- **Snap**: `snap install cecilia`. `cecilia.gui` is the window (and the desktop entry),
  `cecilia` the terminal interface. A snap sees only your non-hidden home files and keeps its
  configuration in `~/snap/cecilia/current/.config/cecilia`.
- **AppImage**: runs as it is. It does not add a menu entry by itself; an AppImage manager does.
  Double-clicked from a file manager it starts the window; started from a terminal with no
  arguments it starts the terminal interface.
- **Arch, Debian/Ubuntu, Fedora**: the AUR packages, `.deb` and `.rpm` also install the desktop
  entry, icon and file types.

The window loads Vulkan or OpenGL at run time (`ui.renderer` falls back to the CPU renderer when
there is no GPU, see the README).

## Windows

The release zip contains `cecilia.exe` (unzip, run `cecilia.exe --gui`). Scoop, Winget and
Chocolatey manifests are in `packaging/` but are not published yet. Use Windows Terminal for the
terminal interface.

## For maintainers

The release workflow builds everything above from a `vX.Y.Z` tag; see "Releases" in `AGENTS.md`
for the secrets and the one-time steps (the Homebrew tap, the Snap Store name, Flathub).

A Flathub submission uses the same manifest as `packaging/flatpak/` with the `dir` source replaced
by

```yaml
- type: git
  url: https://github.com/kjmcnamara1/cecilia.git
  tag: vX.Y.Z
  commit: <the commit the tag points at>
```

plus a `cargo-sources.json` made with `flatpak-cargo-generator.py Cargo.lock`.
