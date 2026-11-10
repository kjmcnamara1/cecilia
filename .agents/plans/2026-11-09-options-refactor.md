# Options Refactor Implementation Plan

> **Status: planned, not started.** Tick steps as they are done; note deviations here.
> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Work on a feature branch (`feat/options`); ask the user before merging to `main`.

**Goal:** Every setting is an *option* with a dotted lower_snake_case name and a default, settable from `config.toml`, `-o NAME=VALUE`, and `:set NAME [VALUE]`, readable with `:get NAME`; the per-option commands go away.

**Architecture:** A new `crates/cecilia-app/src/options.rs` holds a registry (`OPTIONS: &[OptionSpec]`) giving each option's name, kind (bool / int / float / string / choice / color / list) and one-line doc. get/set operate on `ConfigFile` by round-tripping it through a `toml::Table` (serialize → edit the dotted path → deserialize), so TOML, `-o` and `:set` share one parse/validate path, and defaults come from `ConfigFile::default()`. The registry replaces the ad-hoc checks in `Config::from_file`. Live options are read straight from `app.config.file` (the mirrored `App` fields go).

**Tech Stack:** Rust 2024, serde + toml (already used), clap (CLI).

**Spec:** the "Options" section of `TODO.md` (repeated in the user's request; reproduced under Global Constraints).

## Global Constraints

- All settings are "options"; names always lower_snake_case, dot-separated (`tabline.truncation.max_width`). Same names as the TOML tables/keys today.
- Every option has a default. `:set OPTION` with no value, or the value `default`, resets it.
- Remove all option-setting-specific commands.
- `config.toml` sets options directly. CLI: `-o NAME=VALUE ...` (several after one `-o`, e.g. `cecilia -o OPTION1=value1 OPTION2=value2.1,value2.2 OPTION3=true OPTION4=default`); everything after `--` is a file name (`cecilia -o tabline.wrap=true -W foo -- song.cc tab.gp`).
- `:set OPTION [value]`; invalid value fails with a helpful message giving the allowed values; `:get OPTION` prints the current value.
- Types: **boolean** `true`/`false`, plus `toggle`. **integer/float**: `N` absolute, `+N`/`-N` relative. **string**. **array**: comma-separated, no spaces (`a,b,c`); `+value` adds, `-value` removes.
- Conventions (AGENTS.md): Conventional Commits; keep `docs/keymap.md` and `docs/config.example.toml` in sync; entry under `[Unreleased]` in `CHANGELOG.md`; `cargo clippy --workspace --all-targets` clean; `cargo fmt`.

## Decisions already made

1. **Out of scope as options:** `[keymap.*]` bindings (they stay TOML-only; `-o keymap.x=..` errors "keymaps are not options"). Score data and playback state (`:tempo`, `:volume`, `:speed*`, `:metronome`, `:count-in`, `:mute`, `:solo`, `:clef`, `:chant-clef`, `track-*`) are not config options and keep their commands.
2. **Rule for which commands go:** a command goes if it only sets a value that is read live from config. Commands that open/close/arrange panes or change per-pane view state that workspaces remember **stay** (`:tracks`, `:properties`, `:palette`, `:fretboard`, `:piano`, `:tuner`, `:audio`/`:waveform`, `:toolbar`, `:layout`, `:zoom-*`); their `ui.*` options (`tracks_panel`, `properties`, `palette`, `fretboard`, `piano`, `toolbar`, `layout`, `zoom`) are the *start/new-pane defaults*, documented as such. **Confirm with the user** if they want these folded in too.
3. **Commands removed:** `equalize-on-close`, `focus-ring-width`, `focus-ring-color`, `tabline-wrap`, `completion-fuzzy`, `completion-ignore-case`, `bar-numbers`, `incomplete-bars`, `progress-bar`, `scrollbar`, `follow-toggle`, `global-tool`. Replacements: `:set <option> toggle` etc. (`global-tool X` → `:set ui.global_tools +X` / `-X`). Pre-1.0, so no deprecation shims; note in CHANGELOG.
4. **Optional config fields become plain values with an "empty = unset" default** (TOML has no null): `ui.focus_ring_color: String` (default `""` = mode color); `theme.preset: String` (default `"dark"`); `theme.file: String` (`""`); `theme.track_colors`, `theme.voice_colors`: `Vec<String>` (empty = built-in); `theme.<color>`: string, `""` = preset's color (registry lists them from `theme::COLOR_NAMES`).
5. **Invalid values:** from `:set`/`-o` → error, nothing changes (CLI: exit non-zero before the UI starts). From `config.toml` → warning, option falls back to its default (numbers: clamped to range, as today).
6. **Overrides survive config reload:** `Config.overrides: BTreeMap<String, String>` holds `-o` values and session `:set`s as *resolved absolute* strings (so `+5` isn't replayed). `Config::load` applies them over the file; the file watcher's reload therefore keeps them. `:set X default` stores X's default.
7. Array add of a present value and remove of an absent one are no-ops (items still validated against choices). Strings take the rest of the line verbatim (`:set ui.lyrics_font Source Sans 3`); `-o` splits on the first `=`. `:set NAME` with an empty array: use `-o NAME=` (empty after `=`); in `:set`, remove items one by one.

## Review Focus

1. `:set ui.tab_width +100` → error naming the range 1–8; value unchanged. (Task 1)
2. `:set ui.global_tools +tuner` twice → one `tuner`; `+bogus` → error listing the tools. (Task 1)
3. `:set ui.tab_width +1.5` / `:set ui.tab_width abc` → error says "integer". (Task 1)
4. After `-o tabline.wrap=false` and a config-file edit that triggers reload, `tabline.wrap` is still false; a session `:set` also survives. (Task 3)
5. `cecilia -o ui.lyrics_font=Noto\ Sans -- -odd.gp`: `-odd.gp` opens as a file; `cecilia -o song.gp` fails with "expected NAME=VALUE (file names go after --)". (Task 4)

## Files

- Create `crates/cecilia-app/src/options.rs` — registry, `get`/`set`/`validate`, errors.
- Modify `crates/cecilia-app/src/config.rs` — `Serialize` derives, Option→plain fields, drop ad-hoc checks, `Config.overrides`, `Config::load(path, overrides)`.
- Modify `crates/cecilia-app/src/theme.rs` — `ThemeConfig` non-Option fields.
- Modify `crates/cecilia-app/src/command.rs`, `app.rs` — `Set`/`Get` commands; delete the removed commands and mirrored fields.
- Modify `crates/cecilia-app/src/complete.rs`, `toolbar.rs` — completion; toolbar defaults/labels using removed commands.
- Modify `crates/cecilia/src/main.rs` — `-o`.
- Modify `crates/cecilia-gui/src/draw.rs`, `crates/cecilia-tui/src/lib.rs` — read options from config instead of mirrored fields.
- Docs: `docs/keymap.md`, `docs/config.example.toml`, `CHANGELOG.md`, `TODO.md`.

---

### Task 1: Option registry, get and set

**Files:** Create `options.rs` (+ `mod options;` / `pub mod options;` in `lib.rs`); modify `config.rs` (derives), `theme.rs` (decision 4); tests in `options.rs`.

**Interfaces:**
- Produces:
  ```rust
  pub enum Kind { Bool, Int { min: i64, max: i64 }, Float { min: f64, max: f64 },
                  Str, Choice(&'static [&'static str]), Color /* "" or #rgb/#rrggbb/#rrggbbaa */,
                  List(&'static ItemKind) }
  pub enum ItemKind { Str, Choice(&'static [&'static str]), Color }
  pub struct OptionSpec { pub name: &'static str, pub kind: Kind, pub doc: &'static str }
  pub static OPTIONS: &[OptionSpec];
  pub fn spec(name: &str) -> Result<&'static OptionSpec, String>;   // Err: unknown option + near matches
  pub fn get(file: &ConfigFile, name: &str) -> Result<String, String>;          // lists comma-joined
  pub fn set(file: &mut ConfigFile, name: &str, raw: Option<&str>) -> Result<String, String>;
      // raw None or "default" resets; returns the new value formatted as `get` would
  pub fn validate(file: &mut ConfigFile) -> Vec<String>; // used by Config::from_file (Task 2)
  ```
- `ConfigFile` and every nested config struct derive `Serialize` (`Ticks` keeps `rename_all = "lowercase"`).

- [ ] **Step 1: Failing tests** in `options.rs`: `bool_toggle_and_explicit`, `int_absolute_relative_and_range_error` (`ui.tab_width`: `"4"`, `"+1"`, `"-1"`, `"+100"` → `Err` containing `"1 to 8"`, `"abc"`/`"+1.5"` → `Err` containing `"integer"`), `float_relative` (`audio.tuner_a4`, `+1.5`), `choice_error_lists_values` (`ui.bar_numbers=foo` → `Err` containing `"system, every"`), `list_add_remove_dedup` (`ui.global_tools` `+tuner` twice, `-tracks`, `+bogus` error), `default_resets` (`None` and `"default"` for a changed option and for `ui.focus_ring_color` → `""`), `unknown_option_suggests` (`tabline.wrapp` → message contains `tabline.wrap`), `keymap_is_not_an_option`, `every_serialized_leaf_has_a_spec_and_back` (serialize `ConfigFile::default()`, every leaf path outside `keymap` is in `OPTIONS`, every spec resolves via `get`), `snake_case_names` (each name matches `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$`).
- [ ] **Step 2:** `cargo test -p cecilia-app options::` → FAIL (module missing).
- [ ] **Step 3:** Implement. Approach: `toml::Table::try_from(&*file)`; walk dotted path (a missing leaf reads as the kind's empty value — `theme.<color>`); parse `raw` by kind (bool: `true|false|toggle`; Int/Float: leading `+`/`-` ⇒ relative to current, else absolute, range-checked, **error not clamp**; List: `a,b` replaces, `+a`/`-a` add/remove, items checked against `ItemKind`); `""`/`default` removes the key for theme options and otherwise copies the value from `toml::Table::try_from(ConfigFile::default())`; write back and `ConfigFile::deserialize(table)`. Error text format: `NAME: expected <what> (<allowed values or range>), got 'X'`. Register every field currently in `ConfigFile` except `keymap`, with docs lifted from the field doc comments in `config.rs`. `validate` checks each option's current value against its spec (clamping numbers, defaulting others) and returns one warning per fix.
- [ ] **Step 4:** `cargo test -p cecilia-app options::` → PASS; `cargo clippy -p cecilia-app --all-targets`.
- [ ] **Step 5:** Commit `feat(options): option registry with typed get/set`.

### Task 2: Config uses the registry

**Files:** `config.rs` (`from_file`, `load`, `parse`, tests), `app.rs`/`main.rs` call sites of `Config::load`.

**Interfaces:**
- Consumes: `options::validate`, `options::set`.
- Produces: `Config.overrides: BTreeMap<String, String>`; `Config::load(path: &Path, overrides: &BTreeMap<String, String>) -> Result<(Self, Vec<String>), String>`; `Config::parse(text, overrides)` likewise. Override errors are returned as `Err` (fatal), file-value errors as warnings.

- [ ] **Step 1: Failing tests** in `config.rs`: `invalid_option_in_file_warns_and_defaults` (`bar_numbers = "x"` → warning `ui.bar_numbers: …` and value `"system"`; `tab_width = 99` → clamped to 8 with warning), `overrides_beat_the_file` (file `tabline.wrap=true`, override `tabline.wrap=false` → false), `bad_override_is_an_error`. Existing `the_example_config_loads_cleanly` must stay green.
- [ ] **Step 2:** run; FAIL.
- [ ] **Step 3:** In `Config::from_file`, delete the per-option checks (`bar_numbers`, `chant_text_font`, `sync_snap`, `chant_chords`, focus ring width/color, `score.clef`, `statusline.separators`; keep keymap, `toolbar.items`, `global_waveform`, `keymap.play` handling) and call `options::validate(&mut file)`. Add `Config::default_chant_chords`-style callers unchanged. Update every `Config::load`/`parse` caller (`app.rs::reload_config` passes `&self.config.overrides` and carries them into the new config).
- [ ] **Step 4:** `cargo test -p cecilia-app config::` → PASS.
- [ ] **Step 5:** Commit `refactor(config): validate options through the registry; overrides layer`.

### Task 3: `:set` and `:get`

**Files:** `command.rs`, `app.rs` (run arm), tests in `app.rs`.

**Interfaces:** `Command::Set(String, Option<String>)`, `Command::Get(String)`. Parse: `set NAME [VALUE…]` (value = rest of line, trimmed), `get NAME`; missing NAME → `set: expected OPTION [VALUE]` / `get: expected OPTION`.

- [ ] **Step 1: Failing tests** (in `app.rs` tests, next to `equalize_on_close_command_toggles_the_setting`): `set_toggles_and_shows_value` (`set tabline.wrap toggle` flips; `get tabline.wrap` info message `tabline.wrap = false`), `set_error_keeps_value_and_reports` (error message contains the allowed values), `set_rebuilds_keymaps` (`set general.leader ,` makes `,p` resolve like `<leader>p`), `set_survives_reload` (Review Focus 4: write a temp config, `set tabline.wrap false`, `reload_config()`, still false), `set_default_resets`.
- [ ] **Step 2:** run; FAIL.
- [ ] **Step 3:** `App::run` for `Set`: clone `config.file`, `options::set`, on `Ok(v)` rebuild `self.config = Config::from_file(file).0` keeping `overrides` plus `(name, v-as-stored)`, push `Effect::ConfigReloaded` (same as reload), info `NAME = v`; on `Err(e)` `self.error(e)`. `Get` → info `NAME = value`.
- [ ] **Step 4:** `cargo test -p cecilia-app` → PASS.
- [ ] **Step 5:** Commit `feat(options): :set and :get commands`.

### Task 4: `-o` / `--option` on the CLI

**Files:** `crates/cecilia/src/main.rs`.

- [ ] **Step 1: Failing tests** (`#[cfg(test)]` in `main.rs`, `Cli::try_parse_from`): `options_then_double_dash_then_files` (`["cecilia","-o","tabline.wrap=true","ui.tab_width=4","-W","foo","--","song.cc","tab.gp"]` → 2 options, workspace `foo`, files `[song.cc, tab.gp]`), `option_without_equals_is_an_error` (`["cecilia","-o","song.gp"]` → error mentioning `NAME=VALUE` and `--`), `dash_file_after_double_dash` (`-odd.gp` after `--` is a file).
- [ ] **Step 2:** run `cargo test -p cecilia`; FAIL.
- [ ] **Step 3:** Add `#[arg(short = 'o', long = "option", value_name = "NAME=VALUE", num_args = 1.., global = true, value_parser = parse_assignment)] options: Vec<(String, String)>` (split on first `=`; error text `expected NAME=VALUE (file names go after --)`). In `main`, apply each with `options::set` on the loaded file *before* building `Config` (fatal error → message and exit code 1 before any UI), and store resolved values in `Config.overrides` via `Config::load(path, &overrides)` (resolve by running `options::set` on a scratch `ConfigFile::default()` copy of the parsed file, or have `Config::load` accept raw `Vec<(String,String)>` — pick the latter if simpler and keep the `BTreeMap` for resolved values).
- [ ] **Step 4:** tests PASS; manual: `target/release/cecilia -W scratch -o ui.tab_width=nope -- x.gp` exits non-zero printing `ui.tab_width: expected integer (1 to 8), got 'nope'`.
- [ ] **Step 5:** Commit `feat(cli): -o/--option sets options`.

### Task 5: Remove option commands; read options live

**Files:** `command.rs`, `app.rs`, `toolbar.rs`, `config.rs` (default keymaps), `draw.rs`, `tui/src/lib.rs`, `chantsync.rs`/`theme.rs`/`complete.rs` where they mention the removed commands.

- [ ] **Step 1:** Update/replace the tests that exercise removed commands (`equalize_on_close_command_toggles_the_setting`, and any for focus-ring, tabline-wrap, progress-bar, scrollbar, bar-numbers, incomplete-bars, completion-*, follow-toggle, global-tool) to use `set …`; add `removed_option_commands_are_unknown` (each of the 12 names → `Command::parse` is `Err`). Run: FAIL where the old command is still accepted.
- [ ] **Step 2:** Delete the 12 commands (parse arms, `Command` variants, `App::run` arms, `SettingArg` and `number_arg` if now unused). Rebind: grep `DEFAULT_*` in `config.rs` and `toolbar.rs::DEFAULT_ITEMS` for them (e.g. `<leader>f` → `set ui.follow_playback toggle`); the toolbar's active-state/label lookup keyed on command names must key on the `set …` line too.
- [ ] **Step 3:** Delete the mirrored `App` fields `follow`, `completion_ignore_case`, `completion_fuzzy`, `every_bar_number`, `highlight_incomplete_bars`, `progress_bar`, `scrollbar` (and their copy lines in `App::new`/`reload_config`) and read `config.file.ui.*` at each use (`rg "app\.(progress_bar|scrollbar|every_bar_number|follow|highlight_incomplete_bars)"` in gui/tui must come back empty). `toolbar` stays a field (workspace state, decision 2). `ui.global_tools` edits already go through config; make sure a change re-docks (`Effect::ConfigReloaded`).
- [ ] **Step 4:** `cargo test --workspace`; `cargo clippy --workspace --all-targets`; GUI smoke: `cecilia -W scratch --screenshot out.png --keys ":set ui.progress_bar false<CR>" test-corpus/Tabs/<any>.gp5` shows no progress bar.
- [ ] **Step 5:** Commit `refactor!: replace per-option commands with :set`.

### Task 6: Completion for `:set` / `:get`

**Files:** `complete.rs` (+ tests there).

- [ ] **Step 1: Failing tests:** `set_completes_option_names` (`set tabl` → `tabline.…` names, using the existing fuzzy/ignore-case settings), `set_completes_choices` (`set ui.bar_numbers ` → `system`, `every`; bool → `true`, `false`, `toggle`, `default`), `get_completes_names`.
- [ ] **Step 2:** run; FAIL.
- [ ] **Step 3:** Feed candidates from `options::OPTIONS` (names) and the spec's `Kind` (values). Run tests; PASS.
- [ ] **Step 4:** Commit `feat(options): complete option names and values`.

### Task 7: Docs

**Files:** `docs/keymap.md` (document `:set`, `:get`; delete rows for removed commands; fix rebound keys), `docs/config.example.toml` (note `-o`/`:set`; focus_ring_color now `""`; remove mentions of removed commands in comments), `CHANGELOG.md` (`[Unreleased]`: added `-o`, `:set`, `:get`; breaking: removed commands list), `TODO.md` (tick the Options refactor; `docs/OPTIONS.md` stays its own item — the registry's `doc` strings are the source for it).

- [ ] **Step 1:** Edit docs; `cargo test -p cecilia-app config::the_example_config_loads_cleanly`.
- [ ] **Step 2:** Mark this plan finished at the top, commit `docs: options`, then ask the user when to merge to `main`.

## Self-review

- Spec coverage: lower_snake_case + dotted + defaults (T1 tests); remove commands (T5); config.toml (T2); `-o`/`--` (T4); `:set` incl. default/errors/arrays (T1, T3); `:get` (T3); boolean toggle, relative numbers, array `+`/`-` (T1).
- Interfaces: `options::set(&mut ConfigFile, &str, Option<&str>) -> Result<String, String>` is used identically in Tasks 2–4; `Config::load` signature changes in Task 2 are applied to the Task 4 call site.
- Open point to confirm with the user: decision 2 (which commands stay).
