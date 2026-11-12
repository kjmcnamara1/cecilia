# Code review follow-ups Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

> **Status: IMPLEMENTED (2026-11-11) on branch `fix/code-review-followups`, one commit per task; awaiting review and the user's go-ahead to merge to `main`.** Deviations: task 2 clef list is `cb1`–`cb4`/`fb1`–`fb4` in completion too; task 6 added no test for the same-path swap name (`live_owner_needs_force` and `live_owner_is_not_auto_recovered` already pin it) and no orphan retention (orphans are announced, never deleted); task 7 used reader-level unit tests with crafted bytes (no corpus test, the corpus is not in CI); task 9 is two lines in `TODO.md` (the AltGr logic was already unit-tested); `cecilia-tui`'s bufferline test wrote a `.cc` file and now writes `.ccc`. Open question 2 stays open: the audio command channel is still `mpsc`. The `TODO.md` section is gone; new TODOs: wide vibrato / feedback harmonic / golpe.

## Goal

Close the open items under "Code review follow-ups (2026-11-10)" in `TODO.md`, one commit per task. Items that need real Windows/macOS hardware (task 9) cannot be closed from this machine: they get a written checklist, not code. Out of scope: the other `TODO.md` sections.

## Context

- Follow `AGENTS.md`: Conventional Commits, feature branch (ask the user before merging to `main`), changelog entries under `[Unreleased]` in `CHANGELOG.md`, `cargo fmt`, `cargo clippy --workspace --all-targets` clean. New commands need `docs/COMMANDS.md` entries; options need `docs/OPTIONS.md` rows (tests enforce both).
- Worktrees lack `test-corpus/`: symlink it from the main checkout first (a saved feedback memory); tests that read it otherwise fail.
- Baseline: run `cargo test --workspace` once before starting and note pre-existing failures.
- Tasks are independent; order is by risk (small and certain first). Tasks 1–3 touch different crates and can be parallel.

## Global Constraints

- `.cc` stays readable; only writing it changes (AGENTS.md: "legacy `.cc` read-only by default").
- The audio callback must not allocate or take locks on the paths this plan touches.
- Never add a speculative option: decisions below are fixed unless an open question says otherwise.

## Review Focus

- Undo to the saved state of a buffer whose file is gone must stay modified; a buffer with no path (new score) must still go clean.
- `:w x.cc` must fail with a clear error, not silently write a `.ccc`; opening `.cc` and `:w x.ccc` must keep working.
- A full reaper queue must never block or allocate on the audio thread.
- A swap file whose owner pid is alive (another instance, or a recycled pid) must never be offered for recovery without `!`.
- A crafted GP file with `i32::MIN`/`i32::MAX` in channel and lyric-start fields must return a score or an `Err`, never panic.

---

### Task 1: Modified flag when the file was deleted (`editor.rs`)

**Files:**
- Modify: `crates/cecilia-app/src/editor.rs:1953`
- Test: `crates/cecilia-app/src/external.rs` (tests module, next to `a_deleted_file_flags_the_buffer`, line ~166)

- [ ] **Step 1: Failing test** `undoing_to_the_saved_state_keeps_a_deleted_file_modified`: use `app_with_file`, make an edit, save (`:w`), edit again, delete the file, run `external` poll so the buffer is flagged (`disk == None`), undo to the saved state; assert `buffers[i].modified` is true. A second test `undoing_to_the_saved_state_of_a_new_buffer_is_clean` asserts a path-less buffer goes clean (guards the `path.is_some()` condition).
- [ ] **Step 2:** `cargo test -p cecilia-app external::` → the first test fails.
- [ ] **Step 3:** In the undo/redo handler set `buffer.modified = !buffer.history.at_saved() || (buffer.path.is_some() && buffer.disk.is_none());` (`disk` is the file stamp the external-change poll compares; `None` means missing, see `external.rs:52`).
- [ ] **Step 4:** Re-run: both pass. Add a `fix(app):` line to `CHANGELOG.md`; remove the TODO item; commit `fix(app): keep a buffer modified after undo when its file was deleted`.

### Task 2: Docs and completion drift (`chant-clef`, `write`, config example, TODO Tracks)

**Files:**
- Modify: `crates/cecilia-app/src/complete.rs:373,430,1492`, `crates/cecilia-app/src/command.rs:499-501,1045`, `crates/cecilia-app/src/chant.rs:1152`, `docs/COMMANDS.md:270`, `docs/keymap.md:946`, `docs/config.example.toml:~198-216`, `TODO.md`
- Test: `crates/cecilia-app/src/complete.rs` tests

Decision: `ChantClef::parse` (`cecilia-core/src/chant.rs:160`) accepts `c`/`f` plus optional `b` and lines 1–4, and GABC allows the same, so **the parser stays and the docs/messages are corrected** to "c1–c4, cb1–cb4, f1–f4, fb1–fb4"; completion lists the same set (`complete.rs:430` currently omits `cb2/cb4`, `fb*`).

- [ ] **Step 1: Failing tests** in `complete.rs`: `write_completion_offers_only_writable_extensions` (candidates for `:w x.` exclude `pdf svg png wav flac`) and `chant_clef_completion_matches_the_parser` (every candidate passes `ChantClef::parse`, and `cb1..cb4`, `fb1..fb4` are all offered).
- [ ] **Step 2:** Run `cargo test -p cecilia-app complete::` → both fail.
- [ ] **Step 3:** Remove `pdf svg png wav flac` from `WRITE_EXTENSIONS` (the comment becomes "Extensions `:w` writes (scores)"); extend the `chant-clef` value list; reword the three error/help strings and both docs rows.
- [ ] **Step 4:** `docs/config.example.toml`: add a commented `[keymap.visual]` section (after `[keymap.insert-standard]`, same style as its neighbours, naming the default `v`-mode keys from `config.rs:~1070`) and `[keymap.insert-tab]`, `[keymap.insert-slash]`, `[keymap.insert-chant]` sections. Check the section names against `config.rs:552-558,1277-1295`. Is there a test that parses the example file? If so it must still pass; if not, run `cecilia -W scratch --config docs/config.example.toml` style check via `cargo test -p cecilia-app config::`.
- [ ] **Step 5:** `TODO.md` "Tracks" list: fix "c should change the instrument" → `c` is track colour, `i` is instrument (verify in `docs/keymap.md` Tracks section before editing); tick items already implemented if the keymap shows they are.
- [ ] **Step 6:** `cargo test -p cecilia-app` (COMMANDS.md/OPTIONS.md drift tests included); remove the three TODO items; commit `docs: fix clef, write-completion and config example drift`.

### Task 3: `write_bytes` stops writing legacy `.cc`

**Files:**
- Modify: `crates/cecilia-formats/src/lib.rs:160-173`, `crates/cecilia-formats/src/native.rs:1` (doc), `docs/SPEC.md` (native file section), `docs/COMMANDS.md`/`README.md` if they mention writing `.cc`
- Modify tests: `crates/cecilia-formats/tests/roundtrip.rs:678,694,806`, `crates/cecilia-formats/src/gabc.rs:877`
- Audit (grep `"cc"`): `crates/cecilia-app/src/dialog.rs:10` (save dialog extension list), `crates/cecilia-app/src/complete.rs:370`, `crates/cecilia/src/main.rs:163`, `crates/cecilia-app/src/app.rs:442` (the last is icon/format naming and stays)

- [ ] **Step 1: Failing test** in `roundtrip.rs`: `legacy_cc_is_read_but_not_written`: `write_bytes(&score, "cc")` is `Err(Error::Unsupported(msg))` with `msg` containing `.ccc`; `read_bytes` of `.ccc` bytes with path `x.cc` still succeeds. Update the loop at `:678` and `:806` to write `ccc` and read back under the `cc` path (keep `CCM` upper-case case).
- [ ] **Step 2:** Run `cargo test -p cecilia-formats` → the new test fails.
- [ ] **Step 3:** In `write_bytes`, drop `"cc"` from the native arm and add `"cc" => Err(Error::Unsupported("cannot write legacy .cc files; use .ccc or .ccm".into()))`.
- [ ] **Step 4:** Remove `cc` from `complete.rs` write candidates and the save dialog's list if it is write-only there (`dialog.rs:10` is shared with open: check and split if so); `main.rs:163`: read the surrounding code to see whether `cecilia convert x.gp5 y.cc` must now be rejected up front (it will error via `write_bytes`; make sure the message reaches the user).
- [ ] **Step 5:** App behaviour test in `cecilia-app` (near the save tests in `app.rs`): `:w foo.cc` leaves the buffer modified, sets an error notification, creates no file. Run `cargo test --workspace`.
- [ ] **Step 6:** CHANGELOG (`changed`), remove the TODO item, commit `fix(formats): write_bytes no longer writes the legacy .cc format`.

### Task 4: MIDI writer and non-power-of-two time signatures

**Files:**
- Modify: `crates/cecilia-formats/src/lib.rs` (new helper beside `voices_lost`), `crates/cecilia-formats/src/midi.rs:90`, the callers of `voices_lost` in `cecilia-app` (grep `voices_lost`) to add the warning
- Test: `crates/cecilia-formats/tests/roundtrip.rs`

Decision: MIDI's `0x58` event stores the denominator as `2^n`, so it cannot be represented; keep writing the floored value (so playback stays usable) but **warn at save**, as `voices_lost`/`lyric_rows_lost` do for other lossy writers.

- [ ] **Step 1: Failing tests:** `time_signatures_lost(&score, "mid")` is 0 for 4/4 and 6/8, and 1 for a score with a 5/12 bar (count distinct master bars whose denominator is not a power of two; 0 for any extension but `mid`/`midi`). An app test: `:w x.mid` on such a score emits a warning containing "time signature".
- [ ] **Step 2:** Run → fail (function missing).
- [ ] **Step 3:** Add `pub fn time_signatures_lost(score: &Score, extension: &str) -> usize` in `lib.rs`; call it where `voices_lost` is surfaced in `cecilia-app` and append to the same warning text. Fix the comment at `midi.rs:90` to say the value is floored and callers warn.
- [ ] **Step 4:** `cargo test -p cecilia-formats -p cecilia-app`; CHANGELOG; remove the TODO item; commit `fix(midi): warn when a time signature cannot be stored`.

### Task 5: Lock-free reaper queue (`player.rs`)

**Files:**
- Modify: `Cargo.toml` (`rtrb = "0.3"` in `[workspace.dependencies]`), `crates/cecilia-audio/Cargo.toml`, `crates/cecilia-audio/src/player.rs:51,114,144,153,161,168-178`
- Test: `crates/cecilia-audio/src/player.rs` tests module

`std::sync::mpsc::Sender::send` can allocate a block on the audio thread. `rtrb` is a small wait-free SPSC ring buffer (one producer, the audio thread; one consumer, the reaper). Alternatives rejected: `crossbeam-channel` (larger, still allocates in unbounded mode), hand-rolled ring (reinventing).

- [ ] **Step 1: Failing/guard test** `replaced_clips_are_freed_off_the_audio_thread`: build a `Renderer` `.with_reaper()`, hold a `Weak` to an `Arc<AudioClip>`, replace the clip via the renderer's method at `:161`, poll up to 1 s until `Weak::upgrade` is `None`, and assert the drop happened on a thread named `cecilia-reaper` (wrap the clip's drop observation via a small `Drop` guard type if `AudioClip` cannot be observed directly, otherwise assert only that it is freed). Also `full_queue_never_blocks`: push more than capacity without the reaper draining (spawn failed → `trash: None` path) and assert it returns immediately.
- [ ] **Step 2:** Add the dependency; change `trash` to `Option<rtrb::Producer<Trash>>` (capacity 64). `with_reaper` spawns a thread that loops `pop()` with `std::thread::sleep(Duration::from_millis(50))` when empty, owning the `Consumer`; drop the `use std::sync::mpsc` import. Replace `t.send(..)` at `:153,:161` with `push`; on `PushError::Full(v)` call `std::mem::forget(v)` with a comment (a bounded, practically unreachable leak beats a glitch). The reaper thread exits when the producer drops (`Consumer::is_abandoned()`).
- [ ] **Step 3:** `cargo test -p cecilia-audio`; update `docs/SPEC.md` (the "not audited to be allocation- or lock-free" sentence, §7, says the command channel is still `mpsc`: reword to say only that one remains — see open questions). CHANGELOG; remove the TODO item; commit `fix(audio): free replaced clips through a lock-free queue`.

### Task 6: Swap files: orphaned `unnamed-<pid>-<idx>.swp`

**Files:**
- Modify: `crates/cecilia-app/src/swap.rs` (new `orphan_swaps`, notice in startup path, `recover`), `docs/COMMANDS.md` (`:recover`, `:swap-discard`), `docs/SPEC.md` (swap section), `CHANGELOG.md`
- Test: `crates/cecilia-app/src/swap.rs` tests (helpers `crashed_swap`, `app_in` exist)

Decisions: (a) **Same-path swap name is kept** (Vim semantics): `swap_check_open` (`swap.rs:326`) already refuses a live owner and offers `:recover!`; just pin it with a test. (b) Unnamed swaps of crashed sessions are **offered, not auto-restored**: a startup notice lists them, `:recover` with no argument on an unnamed buffer opens the newest orphan, `:swap-discard-all`-style cleanup is *not* added (YAGNI): orphans whose pid is dead and which are older than the retention already handled? Check `swap_tick`/`swap_clear`; if there is no retention, delete orphans older than 30 days at startup.

- [ ] **Step 1: Failing tests:** `orphaned_unnamed_swaps_are_listed_on_startup` (write a crashed `unnamed-<deadpid>-0.swp` via `crashed_swap`, start `app_in(dir)`, assert an info notice naming it and that the live-pid file `unnamed-<our pid>-0.swp` and a file of a live foreign pid (pid 1 when `/proc` exists) are not listed); `recover_without_argument_opens_the_newest_orphan_for_an_unnamed_buffer`; `second_instance_on_the_same_path_is_refused_without_bang` (a foreign live pid owns `swap_name(path)`; opening reports the refusal and `:recover!` takes it).
- [ ] **Step 2:** Run `cargo test -p cecilia-app swap::` → first two fail.
- [ ] **Step 3:** Implement `fn orphan_swaps(dir: &Path) -> Vec<(PathBuf, SwapInfo)>` (files matching `unnamed-*.swp` whose `read_swap` succeeds and `!pid_alive(info.pid)`, newest first by `modified_time`); push a notice at startup next to the existing ones (`self.swap.notices`), worded "N unsaved session(s) from a crash: :recover to open the newest". In `recover`, when `given` is `None` and the buffer has no path, use the first orphan as `given`.
- [ ] **Step 4:** `cargo test -p cecilia-app`; docs; CHANGELOG; remove the TODO item; commit `feat(app): offer to recover swap files of crashed unnamed buffers`.

### Task 7: Regression tests for GP grace notes and saturating arithmetic

**Files:**
- Test: `crates/cecilia-formats/src/gp.rs` (tests module) or `crates/cecilia-formats/tests/gp_edge.rs`; helpers in `tests/common`
- Modify only if a test exposes a bug: `gp.rs:172,214,324,397-398,919-945`

- [ ] **Step 1: Grace notes, GP5:** write a score with a grace note via `gp_write::write_gp5`, read it back, assert fret/duration/dead/on-beat survive (this exists if `roundtrip.rs` already covers it: grep `grace` first and extend rather than duplicate).
- [ ] **Step 2: Grace notes, GP3/4** (the field order is from corpus evidence): scan `test-corpus/` for a `.gp3`/`.gp4` file containing a grace note (throwaway test printing `fx.grace` for every note, run once, not committed); add `corpus.rs` test `gp4_grace_notes_keep_transition_then_duration` asserting that file's exact grace values (fret, duration, velocity). If the corpus has none, craft one: patch a GP4 header + minimal track stream, size permitting, and say so in the status.
- [ ] **Step 3: Saturating arithmetic:** with a helper that writes a minimal GP5 and patches 4-byte little-endian fields by searching for a sentinel written into them first (track `port`/`channel` = `i32::MIN` and `i32::MAX`, lyrics `start_bar` = `i32::MIN`, `repeat_end` = `i32::MAX`, string size field `i32::MAX` and `0`), assert `read_bytes` returns `Ok` or `Err` without panicking and, for `Ok`, that `start_bar == 0` and channel indices are clamped (`gp.rs:214,397`).
- [ ] **Step 4:** `cargo test -p cecilia-formats`; remove the TODO item; commit `test(formats): cover GP grace notes and saturating field arithmetic`.

### Task 8: SPEC.md verification (§4.1, §4.2, §8 lines 306–345)

**Files:**
- Modify: `docs/SPEC.md:91-190,306-345`

This is an audit, not code. Output is an accurate spec, with discrepancies recorded.

- [ ] **Step 1:** §4.1 "target: Guitar Pro 7/8 parity": list every technique in the table; for each find its field in `crates/cecilia-core/src/score.rs`, a reader (`gp.rs`, `gpif.rs`), a writer, an engraving site (`cecilia-engrave`), and a playback site (`cecilia-audio` sequence builder). Rewrite the heading to "Technique set" and mark each row as done/partial/planned with the gap named.
- [ ] **Step 2:** §4.2 "As built (M6)": check each claim against `cecilia-core/src/chant.rs` and `docs/CHANT.md`; rewrite as present-tense description; move stale milestone names out.
- [ ] **Step 3:** Lines ~306–345 (bufferline, notification history, mouse): trace bufferline drawing (`cecilia-gui/src/draw.rs`, `cecilia-tui/src/lib.rs`: file-type icon, middle truncation, hover close button, modified dot) and notification claims (`notify.rs`, options in `OPTIONS.md`) and click hit lists; fix the text, and file real gaps as `TODO.md` items rather than leaving "planned" prose.
- [ ] **Step 4:** Commit `docs(spec): re-check sections 4.1, 4.2 and 8 against the code`; remove the TODO item.

### Task 9: Real Windows/macOS verification (hand-off checklist)

Cannot be done here (no hardware; known VM limitations are already in `TODO.md`'s Windows and MacOS sections). Replace the TODO item with these checks under the respective platform sections, then remove it from the review list:

- [ ] Windows: AltGr combinations in the GUI (`cecilia-gui/src/keys.rs`: e.g. AltGr+Q/@ on a German layout must type the character, not trigger `<C-A-q>`), fonts from `%WINDIR%\Fonts`.
- [ ] macOS: SoundFont default search paths, microphone prompt text (`NSMicrophoneUsageDescription` in the bundle's `Info.plist`) when opening the tuner.
- [ ] Optionally add a pure unit test for the AltGr logic in `keys.rs` that does not need Windows (feed synthesised `ModifiersState` with ctrl+alt and a produced `text`); do it only if the function is already separable.

## Open questions

1. Task 3: is a user opt-in to write `.cc` wanted ("read-only *by default*" in AGENTS.md)? The plan assumes no and rejects it outright.
2. Task 5: `docs/SPEC.md` says the audio thread is fed through `mpsc` command channels too; those sends can allocate as well. Out of scope here (the review named only the reaper): keep the sentence, narrowed, and leave a `TODO.md` item unless the user wants the command channel converted in the same task.
3. Task 2: GABC allows clefs `c1–c4/f1–f4` with `b`; plan documents the superset. Say if the user would rather restrict the parser to `cb3`.

## Status

- [x] Task 1 · [x] Task 2 · [x] Task 3 · [x] Task 4 · [x] Task 5 · [x] Task 6 · [x] Task 7 · [x] Task 8 · [x] Task 9
