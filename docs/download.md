---
title: Download
hide:
  - toc
---

# Download cecilia

<p class="cc-release-line">
  Latest release: <strong>cecilia <span data-release="version">(see GitHub)</span></strong>,
  published <span data-release="date">recently</span>.
  <a data-release="notes" href="https://github.com/kjmcnamara1/cecilia/releases/latest">Release notes</a>
</p>

<div class="cc-quick">
  <p>The quickest way on Linux and macOS picks the right build, checks it and installs it without root:</p>
</div>

```sh
curl -fsSL https://github.com/kjmcnamara1/cecilia/releases/latest/download/install.sh | sh
```

cecilia is free and open source. The builds are not signed, so macOS and Windows warn the first
time you open them: [Install](INSTALL.md) explains what to do.

<div class="cc-systems" markdown>

<section class="cc-system cc-system--linux" markdown>

## Linux

<table class="cc-files">
  <tbody>
    <tr data-asset-row><td><a data-asset="appimage" href="https://github.com/kjmcnamara1/cecilia/releases/latest">AppImage</a></td><td>Runs on any distribution: make it executable and run it.</td><td data-asset-size="appimage"></td></tr>
    <tr data-asset-row><td><a data-asset="deb" href="https://github.com/kjmcnamara1/cecilia/releases/latest">.deb</a></td><td>Debian, Ubuntu, Mint: <code>sudo apt install ./cecilia-editor_*.deb</code></td><td data-asset-size="deb"></td></tr>
    <tr data-asset-row><td><a data-asset="rpm" href="https://github.com/kjmcnamara1/cecilia/releases/latest">.rpm</a></td><td>Fedora, openSUSE: <code>sudo dnf install ./cecilia-editor-*.rpm</code></td><td data-asset-size="rpm"></td></tr>
    <tr data-asset-row><td><a data-asset="flatpak" href="https://github.com/kjmcnamara1/cecilia/releases/latest">Flatpak</a></td><td><code>flatpak install --user cecilia-*.flatpak</code></td><td data-asset-size="flatpak"></td></tr>
    <tr data-asset-row><td><a data-asset="linux-tar" href="https://github.com/kjmcnamara1/cecilia/releases/latest">.tar.gz</a></td><td>The plain binary, x86_64.</td><td data-asset-size="linux-tar"></td></tr>
  </tbody>
</table>

| Package manager | Command |
| --------------- | ------- |
| Arch (AUR)      | `yay -S cecilia-bin` (or `cecilia` to build it) |
| Nix             | `nix profile add github:kjmcnamara1/cecilia` |

</section>

<section class="cc-system cc-system--macos" markdown>

## macOS

<table class="cc-files">
  <tbody>
    <tr data-asset-row><td><a data-asset="dmg" href="https://github.com/kjmcnamara1/cecilia/releases/latest">Disk image (.dmg)</a></td><td>For Apple silicon and Intel Macs: drag cecilia to Applications.</td><td data-asset-size="dmg"></td></tr>
    <tr data-asset-row><td><a data-asset="mac-arm-tar" href="https://github.com/kjmcnamara1/cecilia/releases/latest">.tar.gz, Apple silicon</a></td><td>The command-line binary only.</td><td data-asset-size="mac-arm-tar"></td></tr>
    <tr data-asset-row><td><a data-asset="mac-x64-tar" href="https://github.com/kjmcnamara1/cecilia/releases/latest">.tar.gz, Intel</a></td><td>The command-line binary only.</td><td data-asset-size="mac-x64-tar"></td></tr>
  </tbody>
</table>

The install script above installs `Cecilia.app` into `~/Applications` and a `cecilia` command, with
no Gatekeeper prompt.

</section>

<section class="cc-system cc-system--windows" markdown>

## Windows

<table class="cc-files">
  <tbody>
    <tr data-asset-row><td><a data-asset="win-setup" href="https://github.com/kjmcnamara1/cecilia/releases/latest">Installer (.exe)</a></td><td>Installs cecilia for your user and keeps it up to date.</td><td data-asset-size="win-setup"></td></tr>
    <tr data-asset-row><td><a data-asset="win-zip" href="https://github.com/kjmcnamara1/cecilia/releases/latest">.zip</a></td><td>Unzip and run <code>cecilia.exe --gui</code>.</td><td data-asset-size="win-zip"></td></tr>
  </tbody>
</table>

Use Windows Terminal for the terminal interface.

</section>

</div>

## Any system with Rust

```sh
cargo binstall cecilia           # a release binary
cargo install --locked cecilia   # build from source
```

## Check a download

Every file is listed with its checksum in
<a data-asset="sums" href="https://github.com/kjmcnamara1/cecilia/releases/latest">SHA256SUMS</a>.
On Linux, `sha256sum --check --ignore-missing SHA256SUMS` in the download folder checks the files
you have. Older versions are on the
[releases page](https://github.com/kjmcnamara1/cecilia/releases).
