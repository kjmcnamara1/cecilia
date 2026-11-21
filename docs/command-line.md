# Command line

`cecilia` opens the terminal interface by default and the window with `--gui`. It also converts,
engraves, inspects and plays files without opening either.

## Opening files

```sh
cecilia song.gp5               # open in the terminal interface
cecilia --gui song.gp5         # open in a window
cecilia                        # reopen your last workspace
cecilia -W practice a.gp5 b.gp # open files in the workspace "practice" (created if new)
```

| option                  | what it does                                                                 |
| ----------------------- | ---------------------------------------------------------------------------- |
| `--gui`                 | open in a window instead of the terminal                                     |
| `-W`, `--workspace NAME`| use a workspace (by name, or the path of its `.toml` file); created if new    |
| `--config FILE`         | read this config file instead of the default one                             |
| `-o`, `--option NAME=VALUE` | set an [option](OPTIONS.md) for this run, e.g. `-o ui.tab_width=4`; put `--` before file names that follow |
| `-V`, `--version`       | print the version                                                            |

A workspace remembers your views, pane splits and open files between sessions.

## `cecilia convert`

Converts between formats; the output file's extension picks the format.

```sh
cecilia convert in.gpx out.musicxml                         # Guitar Pro to MusicXML
cecilia convert song.gp5 song.pdf --track 2 --layout page   # engrave a track to PDF
cecilia convert song.gp5 song.svg                           # or SVG, or PNG
cecilia convert song.gp song.flac                           # MIDI and recording, mixed to audio
cecilia convert hymn.gp5 hymn.gabc                          # a melody and its lyrics as chant
cecilia convert chant.gabc chant.pdf                        # square notation
```

| option            | what it does                                                              |
| ----------------- | ------------------------------------------------------------------------- |
| `--track N`       | the track to engrave (`.pdf`, `.svg`, `.png`) or to turn into chant (`.gabc`); 1 by default |
| `--layout KIND`   | engraving layout: `page` (default), `vertical` or `horizontal`            |
| `--scale N`       | pixels per point for `.png` output (2 by default)                         |

Inputs: Guitar Pro 3 to 8 (`.gp3` `.gp4` `.gp5` `.gpx` `.gp`), MusicXML, MIDI, GABC and cecilia's
own `.ccc` / `.ccm`. Outputs: GP5, GPX, GP7/8, MusicXML, MIDI, GABC, `.ccc` / `.ccm`, PDF, SVG,
PNG, WAV and FLAC. Turning a modern score into chant keeps the melody and lyrics only.

## `cecilia info`

```sh
cecilia info song.gp5          # title, tempo, length, tracks and tunings
cecilia info song.gp5 --json   # the whole score model as JSON
```

## `cecilia play`

Plays a file in the terminal without the interface; `--speed 0.75` plays it at 75%.

## `cecilia soundfont install`

Downloads the FluidR3_GM SoundFont (148 MB) into cecilia's data folder, where playback finds it.
The `audio.soundfont` option still takes precedence.
