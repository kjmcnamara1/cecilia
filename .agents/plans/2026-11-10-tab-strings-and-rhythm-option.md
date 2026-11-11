# 4-, 5- and 7-string tab, and tab rhythm as an option

Status: **done, uncommitted** on branch `t3/549e5f1e` (all steps below; `cargo test --workspace` and
`cargo clippy --workspace --all-targets` clean). Changes from the plan:

- `TabRhythm` (`Auto/On/Off`, `parse`, `shows(&Track)`) lives in `cecilia-core` (`score.rs`) and is
  shared by the engraver and the TUI; `Clef::parse` was added and `:clef` uses it.
- Step 9: no new preset aliases (`guitar7` etc.) or `:track-add` kinds: the existing presets
  (`bass`, `bass4`, `bass5`, `7-string`) and picker lists already cover 4/5/7 strings. The picker
  now lists the whole family (`family_tunings`) and typed notes may change the count (1-10).
- Step 8: `pitched_to_fretted` left as is (always a 6-string guitar). Stepping a tuning with
  `<A-l>` / `<A-h>` now crosses into the next string count at the end of a count's list.
- Step 5/6: the read-only-row assertion in the properties test was replaced by a check that the
  Kind row is gone (no read-only rows are left).
- `view_info` in `cecilia-tui` got a `#[allow(clippy::too_many_arguments)]` for the new `mode`.

## Goal

1. Make 4-, 5- and 7-string fretted tracks (bass, 5-string bass, 7-string guitar) first-class in
   tab: engraving, editing, tuning, file formats, tests.
2. Make the staff kinds `:staff-add` accepts exactly `standard`, `tab`, `slash`.
3. Hide the track *Kind* row in the Tracks pane's info section, for every track.
4. Turn `tab+` (tab with a rhythm row) from a staff kind into a **global** option with three
   values: `auto` (default), `on`, `off`, changed with `:set`.

Out of scope: more than 8 strings, per-track or per-staff rhythm setting, new tab notation
features (bends etc.), a `:strings N` command or string-count field (see decisions).

## Decisions (made with the user)

- The **string count is not part of the staff.** It is `Track.strings.len()` (the tuning), shared by
  every tab staff of the track. No `tab5` / clef-like staff variants.
- Staff kinds are `standard`, `tab`, `slash` only. `Staff::TabRhythm` goes away.
- Tab rhythm is a **global** option, `ui.tab_rhythm = auto | on | off`, set with `:set` (and in
  `config.toml`). `auto` (default) shows the rhythm row of a track's tab only when that track has
  no `standard` or `slash` staff.
- The track kind (`fretted`, `pitched`, `drums`, `chant`) stays in the model but is no longer
  shown in the Tracks pane. Fretted vs pitched differ only in whether the track has strings, and
  the tuning row already shows that.
- **The string count is decided by choosing a tuning** and nowhere else: no `:strings` command,
  no string-count field. The tuning pickers and `:tuning` offer 4-, 5- and 7-string tunings.

## Decisions (agent's calls — confirm or change)

- The option is read from the config like `ui.bar_numbers` / `every_bar_number`: a field in
  `engrave::Options` filled by the frontends (`cecilia-gui/src/draw.rs` ~l.2278,
  `cecilia-tui/src/lib.rs` ~l.146) from `app.config.file.ui`. Nothing is stored in the score.
- `auto` looks at the track's own `staves()`, not at the view filters `options.tab` /
  `options.standard` in the engraver (the engraver therefore needs the track's staves, which it
  already has).
- **Behaviour change:** a track whose only staff is `tab` now shows the rhythm row by default
  (before, plain `tab` never did). That is what `auto` as default means; screenshots and layout
  tests that assume a bare tab has no rhythm row need updating.
- The clef moves out of the staff name: `:staff-add standard [clef]` keeps an optional clef
  argument (`standard bass`, `standard grand C4`) since `:clef` can already change it afterwards.
  `treble`, `bass`, `grand`, `neutral` are no longer staff kinds on their own (**breaking**, goes in
  the changelog; check `docs/config.example.toml` and keymap docs for bindings that use them).
  `:notation` follows the same rule (`standard`, `tab`, `slash`; `tab+` removed).
- No dedicated command and no Tracks-pane field: `:set ui.tab_rhythm on` is the interface.

## Context

What exists (verified):

- `crates/cecilia-core/src/score.rs`: `enum Staff { Standard(Clef), Tab, TabRhythm, Slash }`
  (~l.462), `Staff::KINDS`, `parse`, `parse_list`, `name`, `notation`, `is_tab`;
  `Track.staves` / `staves()` / `staves_mut()` / `default_staves()`; `TrackKind { Fretted, Drums,
  Pitched, Chant }`: *fretted* has strings and a tuning (guitar, bass), *pitched* has pitches but no
  strings (piano, voice; its tab is made on demand by `pitched_to_fretted`), *drums* have lanes; `Track.strings: Vec<u8>`
  (index 0 = highest string), `tuning_text()`.
- `crates/cecilia-engrave/src/layout.rs` ~l.655: builds a spec per staff; `Staff::Tab | TabRhythm`
  passes `*staff == Staff::TabRhythm` as the rhythm flag. Tab height is already
  `(track.strings.len() - 1) * 1.5 * sp` (several places: ~l.1480, 1569, 1831, 2011, 2113, 2250,
  2300, 3681).
- `crates/cecilia-tui/src/rhythm.rs:10` uses `track.staves().contains(&Staff::TabRhythm)`.
- `crates/cecilia-app`: `editor.rs` (`add_staff` ~l.2222, `retune` ~l.1962, `fret_for_tab`
  ~l.2429, many staff tests), `app.rs` (`uses_strings` ~l.3375), `picker.rs` (staff-kind picker
  l.170, `tuning_family`, `tunings(family, strings)`), `icons.rs` (`Icon::TabRhythm`),
  `properties.rs` (Tracks pane fields), `options.rs` (the `:set` option list, `Kind::Choice`),
  `config.rs` (`UiConfig`), `complete.rs` (completions l.252, 387, 471, 1405),
  `command.rs` (`StaffAdd`, `Notation`, `Tuning`).
- `crates/cecilia-convert/src/lib.rs`: `retune` (re-fingers, reports shifted/dropped notes),
  `tuning_preset` (already has `7-string`, `bass`/`bass4`, `bass5`), `pitched_to_fretted`.
- `crates/cecilia-formats/src/native.rs` (2 uses of `Staff`): `.ccc` / `.cc` read the serialised
  `Staff`, so old files with `TabRhythm` must keep loading.
- Docs to keep in sync: `docs/keymap.md` (l.787-791 staff / tuning commands, Tracks pane
  l.453), `docs/config.example.toml`, `docs/SPEC.md`, `CHANGELOG.md` (`[Unreleased]`).

Existing patterns: commands are `:name args` strings parsed in `command.rs` and run in
`editor.rs`, undoable through `self.edit(|cx| …)`; Tracks-pane properties live in
`properties.rs` and step with `<A-h>` / `<A-l>`; behaviour belongs in `cecilia-app` with tests,
GUI and TUI are thin.

## Steps

Work on a feature branch (use the `feature-branch` skill); commits are Conventional Commits.

### Part A — staff kinds and the tab-rhythm option

1. **Core model.** In `score.rs`: remove `Staff::TabRhythm`; shrink `KINDS` to
   `["standard", "tab", "slash"]`; rewrite `parse` / `parse_list` so `standard` takes an optional
   clef (`treble|bass|neutral|grand [note]`) and `tab` / `slash` take nothing. Add
   `Track::has_notation_staff(&self) -> bool` (any `Standard` or `Slash` in `staves()`) for the
   `auto` rule. Tests: parse table, `has_notation_staff` over staff mixes.
2. **Old files.** `native.rs`: files whose staves contain `TabRhythm` still load, as `Tab` (the
   global option replaces what the variant said; no per-file flag). Do it with a legacy-shaped
   deserialisation step rather than keeping the variant. Test with a fixture in the old form
   (look in `test-corpus/` for `.ccc` / `.cc` with `tab+`; make a small one if none). Check the
   other readers/writers for `Staff` use (`cargo build` finds them).
3. **Option.** `config.rs`: `ui.tab_rhythm: String` (default `"auto"`); `options.rs`:
   `o("ui.tab_rhythm", Choice(&["auto", "on", "off"]), "Rhythm row under tab: always, never, or
   only when the track has no standard or slash staff.")`. Test that `:set ui.tab_rhythm on`
   works and rejects other values (follow the `ui.bar_numbers` tests).
4. **Engraver.** Add `tab_rhythm: TabRhythm { Auto, On, Off }` to `engrave::Options` (layout.rs
   ~l.23; update the constructors: `cecilia-gui/src/draw.rs`, `cecilia-tui/src/lib.rs`,
   `cecilia-engrave/examples/engrave.rs`, tests). `layout.rs` ~l.655: the rhythm flag for
   `Staff::Tab` is `On`, or `Auto && !track.has_notation_staff()`. Update the layout tests that
   use `TabRhythm` (~l.4743, 4873, 5337, 5881, 6061, 6454, 6557) to set the option, and add one
   test per value. `cecilia-tui/src/rhythm.rs:10` and its test take the same decision (share one
   helper, e.g. `Options::tab_rhythm_for(&Track)`, so engraver and TUI cannot disagree).
5. **App commands and pickers.** `add_staff` (`editor.rs` ~l.2222) and `:notation` accept only
   the new kinds; the error lists `standard, tab, slash`. `picker.rs` l.170 and `icons.rs` lose
   `tab+` / `Icon::TabRhythm` (and its drawing in `draw.rs` ~l.4015). `complete.rs`: staff-add /
   notation candidates, option completion. Update the editor tests that use `tab+`.
6. **Hide the Kind row.** `properties.rs` ~l.927: drop the `Field::TrackKind` row and the
   `chant_track` guard around it, then remove `Field::TrackKind`, its arm in the read-only match
   (~l.2188) and `kind_name` (l.790) if nothing else uses them. The test at ~l.3046 uses that row
   to check that read-only rows refuse editing: point it at another read-only row (e.g. the
   tuning row) or drop that assertion if none is left. Check the GUI and TUI draw nothing for
   the field by name (`cargo build` finds it).
7. **Docs.** `keymap.md` (staff commands l.790-791, Tracks pane l.453; remove any mention of the
   kind row), `config.example.toml`
   (`tab_rhythm`), `SPEC.md`. `CHANGELOG.md` `[Unreleased]`: Added (`ui.tab_rhythm`), Changed
   **Breaking** (staff kinds, `tab+` removed, bare tab shows rhythm by default).

Verify Part A: `cargo test --workspace`, `cargo clippy --workspace --all-targets`, `cargo fmt`,
and screenshots of a tab-only track, a standard+tab track, and `:set ui.tab_rhythm on` / `off`
(`target/release/cecilia -W scratch --screenshot out.png --keys "…" test-corpus/Tabs/…`).

### Part B — 4, 5 and 7 strings

The model already carries any string count; the work is to prove it and fill the gaps, with the tuning as the only way to choose the count. Do the audit
first so the list of fixes is evidence-based.

8. **Audit with tests** (new tests in the crate that owns each behaviour, using 4-string bass,
   5-string bass and 7-string guitar tracks):
   - engrave: tab line count, "TAB" clef label fits on 4 strings, rhythm row and barline heights,
     `BeatPos` / `Hit` rows for every string;
   - app: `j` / `k` and `:string-up/-down` stop at the first/last string, entering frets on the
     7th string, `uses_strings`, `fret_for_tab` / `pitched_to_fretted` (it picks `tuning_for_range`,
     which picks `tuning_for_range`: check what counts it can produce; a pitched track's tab is
     only a convenience, so leave it a 6-string guitar unless the audit shows a real problem),
     `retune` between counts (6 → 4 drops notes that do not fit, reports it; 4 → 7 keeps all);
   - audio: playback pitch of each string through `Track::pitch_of`.
   Fix whatever the audit finds, in the layer that owns it.
9. **Tunings (where the string count is chosen).** In `picker.rs` check which `(family, strings)`
   lists exist (6-string guitar is rich; look for 4/5/6-string bass, 7-string guitar, 8 if
   cheap) and add common ones (4-string: standard E A D G, drop D, D standard; 5-string: low B,
   high C; 7-string: standard B-low, drop A). Align `tuning_preset` names (`bass4`, `bass5`,
   `guitar7`; keep `7-string`) and the `:track-add` kinds (`bass5`, `guitar7` if wanted) and
   completions. The tuning picker of a track must offer the tunings of *other* string counts too
   (e.g. a 6-string guitar can pick a 7-string tuning), since that is the only way to change the
   count: make `tunings(family, strings)` list the whole family, grouped or labelled by count, and
   keep the current count's tunings first. Typing notes (`:tuning B1 E2 …`) stays the free-form
   way (1–10 strings today).
10. **File formats.** Round-trip tests with 4-, 5- and 7-string tracks, in each format's
   existing test module, comparing `split_grand()` forms like the others:
   GP3–5 (string count + tuning, max 7), GP6–8 / GPX, MusicXML (`staff-lines` and
   `staff-tuning`), native `.ccc`. Fix readers/writers that assume 6 strings (grep for hard-coded
   `6`, `[64, 59, 55, 50, 45, 40]` defaults in `cecilia-formats`).
11. **Docs.** `keymap.md` tuning rows and Tracks-pane text; `CHANGELOG.md` `[Unreleased]`.

Verify Part B: the tests above, plus screenshots of a 4-string bass, 5-string bass and 7-string
guitar tab from `test-corpus/` (find or create sample files) at default and `:set ui.tab_rhythm on`.

## Open questions

- Confirm the agent's calls above (optional clef argument on `standard`, strict removal of
  `treble` / `bass` / `grand` as staff kinds, `ui.tab_rhythm` as the option name).
- Retuning to fewer strings drops notes that do not fit (already reported by `retune`): is the
  existing message enough, or should it ask first?

## Progress checklist

- [x] 1 Core model
- [x] 2 Old `.ccc` / `.cc` files
- [x] 3 `ui.tab_rhythm` option
- [x] 4 Engraver and TUI rhythm
- [x] 5 Staff commands, pickers, icons
- [x] 6 Hide the Kind row
- [x] 7 Docs and changelog (Part A)
- [x] 8 Audit tests and fixes
- [x] 9 Tunings and presets
- [x] 10 File-format round trips
- [x] 11 Docs and changelog (Part B)
