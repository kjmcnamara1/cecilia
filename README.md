# muse

A keyboard-first, open-source editor and player for guitar tablature, standard notation and
Gregorian chant, with Guitar Pro compatibility and audio play-along. Written in Rust.

**Status:** milestones 1–2 (terminal reader, MIDI playback, file formats). See [docs/SPEC.md](docs/SPEC.md) for the
full specification and roadmap, and [docs/keymap.md](docs/keymap.md) for key bindings.

## What works now

- Reading Guitar Pro 3–8 (`.gp3`, `.gp4`, `.gp5`, `.gpx`, `.gp`), MusicXML (`.musicxml`, `.mxl`),
  MIDI and muse's own `.muse`; writing `.gp5`, `.gpx`, `.gp`, `.musicxml`, `.mxl`, `.mid`, `.muse`
- Terminal UI: tablature view, vim-style navigation with counts, `:` commands, split panes
  (`<C-w>s` / `<C-w>v`), tabs (`:tabnew`, `gt`), which-key hints, mouse clicks and scrolling
- MIDI playback with a moving cursor, repeats and tempo changes, through a SoundFont or a built-in
  plucked-string synth
- TOML configuration with custom keybinds, reloaded automatically when the file changes

## Usage

```sh
cargo run --release -- song.gp5            # open in the terminal UI
cargo run --release -- info song.gp5       # tracks, tunings, tempo, length (--json for the model)
cargo run --release -- play song.gp5       # headless playback
cargo run --release -- convert in.gpx out.musicxml   # convert by output extension
```

In the UI: `h j k l` move by beat/string, `w b` jump between notes, `{ }` bars, `J K` tracks,
`<Space><Space>` play/pause, `<Space>s` stop, `:q` close pane, `<F1>` all bindings.

## Configuration

`~/.config/muse/config.toml` — see [docs/config.example.toml](docs/config.example.toml).
For realistic instrument sounds set `audio.soundfont` to a General MIDI `.sf2` file
(e.g. FluidR3_GM or GeneralUser GS); otherwise the built-in synth is used.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Format tests also parse every Guitar Pro file under `test-corpus/` (gitignored; point it at
your own collection, e.g. `ln -s ~/Music/Tabs test-corpus`).

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at your option.
