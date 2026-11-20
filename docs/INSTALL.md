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
| Nix     | flake (builds from source)             | `nix profile add github:kjmcnamara1/cecilia` (NixOS: see [Nix](#nix-and-nixos)) |
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
  (`CECILIA_PREFIX` changes that). Remove it with
  `packaging/linux/uninstall.sh` from a source checkout or release archive.
- **macOS**: `Cecilia.app` in `~/Applications` (`CECILIA_APP_DIR`) and a `cecilia` command in
  `~/.local/bin` (`CECILIA_BIN_DIR`). Downloads made this way carry no quarantine flag, so there is
  no Gatekeeper prompt.
- If the `bin` folder is not on your `PATH` yet, it is added for new terminals with one line in
  your shell's startup file (`~/.zshrc`, `~/.bashrc` on Linux or `~/.bash_profile` on macOS,
  `~/.config/fish/conf.d/cecilia.fish`, otherwise `~/.profile`), marked
  `# added by the cecilia installer`. `CECILIA_NO_MODIFY_PATH=1` leaves the file alone.
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
  under your home directory; `:soundfont-install` downloads one into
  `~/.var/app/io.github.kjmcnamara1.cecilia/data/cecilia/soundfonts`. Flathub is planned.
- **Snap**: `snap install cecilia`. `cecilia.gui` is the window (and the desktop entry),
  `cecilia` the terminal interface. A snap sees only your non-hidden home files and keeps its
  configuration in `~/snap/cecilia/current/.config/cecilia` (and a SoundFont from
  `:soundfont-install` in `~/snap/cecilia/current/.local/share/cecilia/soundfonts`).
- **AppImage**: runs as it is. It does not add a menu entry by itself; an AppImage manager does.
  Double-clicked from a file manager it starts the window; started from a terminal with no
  arguments it starts the terminal interface.
- **Arch, Debian/Ubuntu, Fedora**: the AUR packages, `.deb` and `.rpm` also install the desktop
  entry, icon and file types.

The window loads Vulkan or OpenGL at run time (`ui.renderer` falls back to the CPU renderer when
there is no GPU, see the README).

## Nix and NixOS

The repository is a Nix flake. It builds cecilia from source (there is no binary cache, so the
first install compiles for a few minutes) and installs `cecilia`, `ceciliag`, the desktop entry,
icon and file types. Flakes must be enabled: add `experimental-features = nix-command flakes` to
`~/.config/nix/nix.conf` (or `/etc/nix/nix.conf`), or pass
`--extra-experimental-features 'nix-command flakes'` to each command.

Try it without installing:

```sh
nix run github:kjmcnamara1/cecilia -- --gui song.gp5
```

### With Nix on any Linux or macOS

Install into your user profile:

```sh
nix profile add github:kjmcnamara1/cecilia            # latest main
nix profile add github:kjmcnamara1/cecilia/vX.Y.Z     # a release tag (from the first release with the flake)
nix profile upgrade cecilia                           # update later
nix profile remove cecilia
```

Older Nix versions only have `nix profile install`, which newer ones still accept.

Outside NixOS the system's own GPU drivers cannot be loaded by Nix-built programs, so cecilia falls
back to nixpkgs' Mesa: this covers AMD, Intel and software rendering. With NVIDIA's proprietary
driver, run it through [nixGL](https://github.com/nix-community/nixGL)
(`nixGL cecilia --gui`). For the desktop entry to show in your menu, `~/.nix-profile/share` must
be in `XDG_DATA_DIRS` (the Nix installer sets this up for login shells).

### On NixOS

Add the flake as an input of your system flake and the package to your system:

```nix
# flake.nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    cecilia.url = "github:kjmcnamara1/cecilia";   # or ".../cecilia/vX.Y.Z" for a release
    cecilia.inputs.nixpkgs.follows = "nixpkgs";  # build against your nixpkgs
  };

  outputs = { nixpkgs, cecilia, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ./configuration.nix
        ({ pkgs, ... }: {
          environment.systemPackages = [
            cecilia.packages.${pkgs.stdenv.hostPlatform.system}.default
          ];
        })
      ];
    };
  };
}
```

then `sudo nixos-rebuild switch`. `nix flake update cecilia` followed by a rebuild updates it. The
window uses the system's graphics drivers, so `hardware.graphics.enable = true;` must be set (desktop
environments turn it on). Sound goes through ALSA, which PipeWire and PulseAudio both provide.

With Home Manager, put the same package in `home.packages` instead.

### Working on cecilia

`nix develop` in a checkout gives a shell with the Rust toolchain, clippy, rustfmt, `just` and the
libraries cecilia needs, so `cargo build`, `cargo test` and `just ci` work as described in the
README.

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
