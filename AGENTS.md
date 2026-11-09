# AGENTS.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this repo is

`cecilia`: a keyboard-first editor/player for guitar tab, standard notation and Gregorian chant
(Guitar Pro compatible, audio play-along). Rust 2024 workspace; `docs/SPEC.md` is the spec,
`docs/keymap.md` the key bindings, `docs/config.example.toml` the config reference. Keep the last
two in sync when adding commands, keys or settings.

## Commands

```sh
cargo build --release                      # binary: target/release/cecilia
cargo test --workspace                     # all tests
cargo test -p cecilia-app tracks::            # one crate / one module or test name
cargo clippy --workspace --all-targets     # keep clean
cargo fmt                                  # rustfmt.toml: max_width 100
cargo run --release -- --gui song.gp5      # window (default is the terminal UI)
# headless GUI screenshot, for checking engraving/layout changes:
target/release/cecilia -W <scratch-workspace> --screenshot out.png --keys "<keys>" song.gp5
```

Use `-W` with a scratch workspace name so runs don't touch the user's saved workspace.

Sample score files (GP, GABC, etc.) for testing and screenshots live in `test-corpus/`
(`gabc/`, `Sacred/`, `Tabs/`).

## Commits

Use Conventional Commits (`feat:`, `fix(engrave):`, `refactor:`, `docs:`, `test:` …), even though
older history uses plain sentences.

- Implement new features in feature branches before merging back into `main`. Ask the user when to
  merge.
- Use semantic versioning and tag commits for releases. The user will say when to go to the first
  major version, 1.0.0.
- Keep `CHANGELOG.md` up to date: finished work goes under `[Unreleased]`. Planned work lives in
  `TODO.md`, not the changelog.

## Plans

Write implementation plans as markdown files in `.agents/plans/` (create the directory if needed),
named `YYYY-MM-DD-short-topic.md`, so another agent can pick one up with no other context. Each plan
should contain:

- **Goal**: what is being built and why, plus anything explicitly out of scope.
- **Context**: the relevant crates, modules and files (with paths), existing patterns to follow, and
  decisions already made (don't leave the next agent to re-derive them).
- **Steps**: small, ordered, concrete tasks, each naming the files to touch and how to verify it
  (tests to add, commands to run).
- **Status**: a checklist of steps, ticked as they are done, with notes on what is in progress,
  what changed from the plan, and open questions or blockers.

Update the plan as work proceeds so it always reflects the current state. Plans are handoff
documents: when finished, say so at the top; planned-but-unscheduled work still goes in `TODO.md`.

## Architecture

Crates (dependencies flow downward):

- `cecilia-core` — the `Score` model (Track → Bar → Voice → Beat → Note; `MasterBar`, markers,
  lyrics, chant marks, `Track.staves: Vec<Staff>` = notation *views* of one track's notes).
- `cecilia-formats` — readers/writers (GP3–8, GPX/GP, MusicXML, MIDI, GABC, native `.ccc` / `.ccm`, legacy `.cc` read-only by default).
  Readers merge multi-staff parts into one grand-staff track; writers split internally, so
  round-trip tests compare `split_grand()` forms.
- `cecilia-convert` — modern ⇄ chant conversion. `cecilia-audio` — synth, player, beat detection, tuner.
- `cecilia-engrave` — score → layout (pages of drawing primitives) for standard/tab/tab+/slash/chant
  staves; one engraving context per staff spec; `BeatPos`/`Hit` give the frontends cursor and
  click positions per staff.
- `cecilia-app` — all editor logic, frontend-independent: `App` (views → pane tree → panes, cursor
  per pane, buffers, modes, keymaps, commands in `command.rs`, pickers, workspaces, undo tree,
  playback/play-along, progress bar, tracks pane). Anything behavioural belongs here with tests.
- `cecilia-gui` (vello/wgpu; `draw.rs` builds a scene from `App` state and records click hit lists)
  and `cecilia-tui` (ratatui) — thin renderers over `App`; changes to a pane kind or widget
  usually need both. `cecilia` is the CLI binary wiring them together.

Panes: `PaneKind` is `Score` or a tool (`Tracks`, `Audio`, `Piano`, `Fretboard`, `Tuner`).
Tool panes live in the same layout tree (`layout.rs`) as scores, have fixed natural sizes, and
act on a score pane: global tools (`ui.global_tools`) dock full width at the bottom and follow the
last focused score pane; local tools are owned by (`Pane.owner`) and sit under one score pane.
`App::focused()` is the score pane being worked on, `window_focus()` the actual focus.

Config (`config.rs`, TOML) holds keymaps per mode, theme and `[ui]` settings; commands are
`:name args` strings dispatched in `command.rs`/`editor.rs`, and keymaps bind keys to them.
