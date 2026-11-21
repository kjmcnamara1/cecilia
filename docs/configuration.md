# Configuration

cecilia reads a TOML file at startup and picks up changes while it runs, so you can edit it and
see the result straight away.

| system  | file                                                                 |
| ------- | -------------------------------------------------------------------- |
| Linux   | `~/.config/cecilia/config.toml` (or `$XDG_CONFIG_HOME/cecilia/`)     |
| macOS   | `~/Library/Application Support/cecilia/config.toml`                  |
| Windows | `%APPDATA%\cecilia\config\config.toml`                               |

`--config FILE` uses another file. The Flatpak and Snap keep their configuration inside their
sandbox: see [Install](INSTALL.md#linux).

## Three ways to set an option

Every setting is an *option* named by its dotted path, such as `ui.tab_width`:

- in the config file, under its table (`[ui]` then `tab_width = 3`);
- inside cecilia, for this session: `:set ui.tab_width 3` (`:get ui.tab_width` shows it,
  `:set ui.tab_width default` resets it);
- on the command line, for this session: `cecilia -o ui.tab_width=3 song.gp5`.

[Options](OPTIONS.md) lists every option with its type and default.

## Themes

cecilia has 15 built-in themes, among them `dark`, `light`, `nord`, `gruvbox`, the Solarized and
Catppuccin families, and the logo's own `cecilia`. Try one with `:set theme.preset nord`, and keep
it with `preset = "nord"` under `[theme]` in the config file. The same table can override single
colours or point at a theme file of your own.

## Key bindings

Keymaps are set per mode (`[keymap.normal]`, `[keymap.insert]`, …) and bind keys to commands.
[Key bindings](keymap.md) lists the defaults and how to override them, and
[Commands](COMMANDS.md) lists what a key can be bound to.

## Sound

`:soundfont-install` (or `cecilia soundfont install`) downloads the FluidR3_GM General MIDI
SoundFont (148 MB, MIT licensed) into cecilia's data folder and uses it from then on. A General MIDI
`.sf2` of your own, such as GeneralUser GS, can be set as `audio.soundfont` and always wins.
Without either, cecilia uses a SoundFont from the usual system folders, and otherwise its built-in
synth.

## The example configuration

This is `docs/config.example.toml` from the repository, with every section and its defaults.
Copy the parts you want to change.

```toml
--8<-- "docs/config.example.toml"
```
