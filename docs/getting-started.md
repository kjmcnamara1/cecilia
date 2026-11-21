# Getting started

This page takes you from installing cecilia to opening, playing and editing your first score.

!!! warning "cecilia is still in development"
    Until version 1.0.0, key bindings, commands, options and the native file formats (`.ccc`,
    `.ccm`) can change between releases. Keep your scores in a standard format too (Guitar Pro,
    MusicXML or GABC) and read the [changelog](changelog.md) before upgrading.

## 1. Install

On Linux and macOS the quickest way is the install script:

```sh
curl -fsSL https://github.com/kjmcnamara1/cecilia/releases/latest/download/install.sh | sh
```

It picks the build for your system, checks it and installs it without root. The
[Download](download.md) page has the installers for every platform, and [Install](INSTALL.md)
covers Homebrew, Flatpak, Snap, the AUR, Nix, Windows and building from source.

## 2. Open a score

cecilia has two interfaces: a window and a terminal interface. They use the same key bindings.

```sh
cecilia --gui song.gp5      # open in a window
cecilia song.gp5            # open in the terminal
cecilia                     # reopen your last workspace
```

On macOS, opening `Cecilia.app` starts the window.

cecilia reads Guitar Pro 3 to 8 (`.gp3`, `.gp4`, `.gp5`, `.gpx`, `.gp`), MusicXML, MIDI and GABC
chant. To open more files from inside cecilia, type `:open song.gp5`, or press `Ctrl-o` for the
system file picker.

## 3. Get sound

For realistic instruments, download a SoundFont once:

```text
:soundfont-install
```

This fetches FluidR3_GM (148 MB) into cecilia's data folder. Until you do, cecilia uses a
SoundFont from the usual system folders if it finds one, and otherwise a basic built-in synth.

## 4. Move around and play

`Space` is the *leader* key: many commands start with it.

| keys                            | what it does                                                        |
| ------------------------------- | ------------------------------------------------------------------- |
| `h` `j` `k` `l`                 | move by beat and string (`j` / `k` between staff lines in notation) |
| `w` `b`, `{` `}`                | next / previous note, previous / next bar                           |
| `J` `K`                         | next / previous track                                               |
| `Space Space`, `Space s`        | play / pause, stop                                                  |
| `Space v`                       | cycle page / vertical / horizontal layout                           |
| `Space T`, `Space F`, `Space K` | tracks panel, fretboard, piano                                      |
| `Ctrl-=` `Ctrl--`               | zoom                                                                |
| `<C-w>s` `<C-w>v`               | split the pane                                                      |
| `F1` or `Space ?`               | every key binding, searchable                                       |

In the window you can also click to place the cursor.

## 5. Edit and save

| keys     | what it does                                    |
| -------- | ----------------------------------------------- |
| `i`      | Insert mode: type fret numbers, `Esc` to leave  |
| `u`, `U` | undo, redo (`g-` / `g+` walk the undo tree)     |
| `:`      | command line, e.g. `:w song.gp5` to save, `:q`  |

Saving to `.gp5`, `.gpx` or `.gp` writes Guitar Pro; `.musicxml`, `.mid` and `.gabc` work too.

## Next steps

- [Playing along](playing-along.md) with a recording: sync, loops, speed.
- [Chant](CHANT.md): editing Gregorian chant in square notation.
- [Key bindings](keymap.md) and [Commands](COMMANDS.md): everything cecilia can do.
- [Configuration](configuration.md): themes, options and your own key bindings.
