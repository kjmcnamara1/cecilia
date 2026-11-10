# Notifications Implementation Plan

> **Status: IMPLEMENTED (2026-11-10), uncommitted on branch `t3/b7e1f828`; awaiting review, commit and the user's go-ahead to merge to `main`.** Deviations: options are a top-level `[notifications]` table (`notifications.anchor`, …), not `ui.notifications.*`; questions (save/reload/delete-workspace) are sticky `prompt` cards; TUI cards show `×N` only (no peeking cards); no fade-out; Enter in the history picker just closes it; history is only persisted by the binary (`keep_notification_history`), never by `App::new`. Four tests that read `test-corpus/` fail in worktrees without that directory (pre-existing).
> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development (recommended) or superpowers:executing-plans.

## Goal

Remove the bottom bar (the "command line" row below the status line) and replace what it showed:

1. **Messages** (`App::info` / `App::error`) become in-app **notifications**: toast cards drawn over the score panes (or the window), stacked, with repeated/similar ones merged into a card stack, auto-dismissed after a per-severity timeout.
2. **Notification history**: persistent across sessions, browsable in a picker.
3. **Pending keys and count** (`3`, `d`, `c`, `"a`, `q`…) are shown by the which-key popup (bottom right) instead of the right end of the bottom bar.

Out of scope: notification actions/buttons, sounds, OS-level desktop notifications, per-notification custom icons, changing the command palette / pickers (they draw their own input, not in the bottom bar), the mouse-hover toolbar tooltip rework beyond relocating it (see step 7).

## Context

What the bottom bar does today (verified in the code):

- TUI: `draw_cmdline` (`crates/cecilia-tui/src/lib.rs:2833`), layout row `cmdline` in `draw` (`:734`, a `Constraint::Length(1)` under the status row). GUI: `draw_command_line` (`crates/cecilia-gui/src/draw.rs:3678`), `const CMDLINE: f64 = 26.0` (`:69`), used in `bottom = height - STATUS - CMDLINE` (`:1030`) and `draw_status` (`:3726`).
- It draws `app.message` (`Message { text, error }`, `app.rs:448`; set by `App::info`/`App::error`, `app.rs:1544`/`:1551`, ~258 call sites; cleared by `self.message = None` at the start of a command in `editor.rs:517`, plus `external.rs:68`, `workspace.rs:557`, `chant_text.rs:248`) in `Normal | Insert | Visual | Tracks | Properties | Palette | Sync` modes, and the right-aligned pending keys + count (`app.count`, `app.pending`).
- The TUI also draws the hovered toolbar button's tooltip over that row (`lib.rs:~980`).
- Tests assert on `app.message` in ~40 places (`app.message.as_ref().unwrap().error`, `.text.contains(..)`) across `chant.rs`, `workspace.rs`, `swap.rs`, `marks.rs`, `editor.rs`, `edit_tests.rs`, `chantsync.rs`, `external.rs`, `dashboard.rs`, `dialog.rs`, `palette.rs`, `palette_tools_tests.rs`, `chant_text.rs`.

Existing patterns to follow:

- **Which-key popup**: `App::which_key()` (`crates/cecilia-app/src/marks.rs:182`) returns `Option<(title, rows)>`; drawn bottom-right by `draw_which_key` in TUI (`lib.rs:2861`) and GUI (`draw.rs:5907`) over the `main` rect. Today it shows only while `pending` is non-empty *and* has continuations, or while `Awaiting::{Mark, Jump}`. **Not captured today:** the count alone (`app.count`), operators `d`/`y`/`c` (`ed.operator: Option<(Operator, Option<usize>)>`, set at `editor.rs:546`/`:801`; `Operator` in `command.rs:43`), and `Awaiting::{Register, RecordMacro, PlayMacro}` (`editor.rs:25`).
- **Timed UI state**: `tracks_peek_until: Option<Instant>` + `tracks_peeking()` (`app.rs:~1400`) is the precedent for Instant-based expiry that frontends just read while drawing. Frontends already redraw on a timer (GUI: `ControlFlow::WaitUntil` 100/500 ms in `crates/cecilia-gui/src/lib.rs:~405`; TUI loop polls with events, `poll_config` each second).
- **Options**: registry in `crates/cecilia-app/src/options.rs` (`OptionSpec`, `Kind::{Bool,Int,Float,Choice,..}`, `OPTIONS` in config.toml order); settings structs in `config.rs` (`ConfigFile`, `ui`); get/set shared by `:set`/`:get`/`-o`. New options go in the registry + `docs/config.example.toml`; read live from `app.config.file`.
- **Pickers**: `PickerKind` + `Picker` in `crates/cecilia-app/src/picker.rs` (Buffer, Recent, Mark, Keymap…); add a `Notification` kind. Commands in `command.rs`, dispatched in `editor.rs`; keymaps in `config.rs` defaults + `docs/keymap.md`.
- **Persistence**: user data dirs via `directories::ProjectDirs::from("", "", "cecilia")` (`workspace.rs:224`); swap files use `$XDG_STATE_HOME/cecilia/` (`config.rs:341`). History belongs in the *state* dir (`state_dir()`, falling back to `data_dir()`), e.g. `notifications.jsonl`.
- **Pane rectangles**: `App::pane_rects()` (`app.rs:1344`) gives `(PaneId, Rect)` in viewport units (cells in TUI, px in GUI); `App::viewport` is the whole window area (`set_viewport`, `app.rs:1034`). Score panes = `PaneKind::Score`.

### Decisions already made

1. **Model lives in `cecilia-app`** (new `crates/cecilia-app/src/notify.rs`); frontends only draw `app.notifications_visible()`. Behaviour and tests belong in the app crate.
2. **Severity**: `Info` and `Error` only (what `info()`/`error()` express today). Don't add Warn/Success unless a call site needs it.
3. **API**: `App::info()` / `App::error()` keep their signatures and now push a notification (no 258-call-site churn). `app.message` is removed; tests migrate to a helper `app.last_notification()` (returns the newest `&Notification`, with `.text`, `.error`) — mechanical replacement.
4. **Clearing**: `self.message = None` at command start becomes nothing (toasts time out instead). Sites that used it as "clear the stale error" (`external.rs:68`, `workspace.rs:557`, `chant_text.rs:248`, `workspace.rs:501` read) get a decision each: delete, or `dismiss_all()` where a state change really invalidates earlier messages. Default: delete.
5. **Placement option** `ui.notifications.anchor` (Choice of 8: `top_left`, `top`, `top_right`, `right`, `bottom_right`, `bottom`, `bottom_left`, `left`) × `ui.notifications.area` (Choice: `scores`, `window`). Default `top_right` / `scores`. Two options rather than a 16-value combined one. *`scores`* = the bounding rect of the score panes (union of `PaneKind::Score` rects, so tool panes and docks are excluded); *`window`* = whole viewport. (Spec says "pane scores": read as "the score panes' area".)
6. **Timeouts**: `ui.notifications.info_timeout` and `ui.notifications.error_timeout`, floats in seconds (0 to 3600), `0` = sticky until dismissed. Defaults: info 4.0, error 8.0.
7. **Stacking**: newest card nearest the anchor edge; older cards stack away from it (top anchors grow down, bottom anchors grow up, left/right anchors stack downward from the vertical centre... decide in step 4 and document). Max visible cards `ui.notifications.max_visible` (int 1–20, default 5); the rest collapse into a "+N more" line (still in history).
8. **Combining**: a new notification with identical `(severity, text)`, or the same *template* ("similar"), merges into the existing card: count increments (`×3`), timer resets, text updates to the newest. "Similar" = equal after normalising: ASCII digits runs → `#`, so "bar 3 deleted"/"bar 4 deleted" merge. Merge only against the **visible** cards, newest card first; a stack shows the newest text, a badge `×N`, and a peek edge of cards behind it (cap drawn depth at 3). History records every notification individually (not merged), with its timestamp.
9. **Timeout bar**: every card with a timeout draws a very thin bar along its bottom edge that drains (it shrinks toward the left edge) as the timeout runs out; its length is `remaining / total` of that card's timeout. Sticky cards (timeout 0) draw no bar. A merge resets the timer, so the bar refills. In a stack, the front card's bar is drawn. Error cards use the error colour, info cards the accent colour. No option for it (fixed behaviour).
10. **Dismissal**: click on a card dismisses it (GUI and TUI mouse); `:notify-dismiss` (all visible). No key stealing in Normal mode by default — bind `<leader>nd` → `:notify-dismiss` and `<leader>nh` → `:notifications` (history picker) in the default keymap.
11. **History**: ring of the last `ui.notifications.history_size` entries (int 0–10000, default 500; 0 disables persistence), appended to `notifications.jsonl` in the state dir as they arrive (one JSON object per line: `ts` unix secs, `error`, `text`), truncated on load when over the limit. Pre-existing files from other cecilia processes are not locked/merged: each process appends; on load take the last N lines. Write errors are swallowed silently (never recurse into `error()`).
12. **History picker**: `:notifications` opens `PickerKind::Notification`, newest first, rows `HH:MM  text` (errors coloured with the error colour as the Keymap picker colours its rows, if it does; otherwise prefix `✗`). Enter copies the text to the unnamed register... **open question for the user: what should Enter do?** Default if unanswered: nothing but close; `:notifications!`-style clearing is out of scope.
13. **Modes**: toasts draw in every mode (previously hidden in `Command`/`Picker` because the bar was needed for input; now there's no conflict — but while a picker/palette overlay is open, draw toasts beneath it).
14. **Which-key takes over pending state**: extend `App::which_key()` (not a new popup) so it also returns a popup when there is a count and/or operator and/or awaiting register/macro, with a title such as `3d` and rows of what can follow (motions/text objects for operators; "register name" / "macro register" hint rows for `"`, `q`, `@`). When there is nothing to list, a single-row popup still shows the pending text so `3` or `d` alone is visible. Keep the existing mark behaviour and tests.
15. **Tooltip relocation**: the TUI toolbar tooltip (drawn on the removed row) moves to the row just under the toolbar's hovered button… simplest: draw it as a one-line popover directly below the hovered button (GUI already does `draw_tooltip` with a rect — reuse that approach; TUI gets the same). Decide the exact position in step 7.

## Steps

Each step ends green: `cargo test --workspace`, `cargo clippy --workspace --all-targets`, `cargo fmt`.

### 1. Notification model (`cecilia-app`)

Files: new `crates/cecilia-app/src/notify.rs`, `lib.rs` (mod), `app.rs`.

- `struct Notification { id: u64, text: String, error: bool, count: u32, born: Instant, expires: Option<Instant> }`; `Notifications { visible: Vec<Notification>, next_id: u64, history: Vec<HistoryEntry> }` on `App`.
- `push(text, error)`: merge rule (decision 8), else append; set `expires` from the timeout option (read from `config.file.ui.notifications`); append to history.
- `fn expire(&mut self, now: Instant)` and `App::notifications_visible(&self) -> impl Iterator` which filters by `Instant::now()` (like `tracks_peeking`) so frontends need no tick; also prune in `poll_config`.
- `Notification::remaining_fraction(now) -> Option<f32>` (`None` when sticky; clamped 0..=1; needs `born`/`expires` or a stored `timeout: Option<Duration>` and `shown_at: Instant`, reset on merge).
- `dismiss(id)`, `dismiss_all()`.
- Replace `App::info`/`App::error` bodies to call `push`. Remove `Message` and `app.message`.
- Tests (in `notify.rs`): merging identical, merging digit-variants, not merging different severities, expiry per severity (inject `now`), `remaining_fraction` falls from 1 to 0, is `None` when sticky and refills on merge, sticky when timeout 0, `max_visible` overflow count, dismiss.

### 2. Options

Files: `config.rs` (new `UiNotifications` struct nested in `ui`, `#[serde(default)]`), `options.rs` (registry entries: `ui.notifications.anchor`, `.area`, `.info_timeout`, `.error_timeout`, `.max_visible`, `.history_size`), `docs/config.example.toml`.

- Choices as `const` arrays like `LAYOUTS`/`TICKS` in `options.rs`.
- Tests: `:set ui.notifications.anchor bottom_left` round-trips; invalid value errors listing allowed values (follow existing options tests); defaults as in decisions 5–6, 11.
- Check how a nested table (`tabline.truncation.max_width` already is one) is declared and copy it.

### 3. Migrate `app.message` readers

Files: all listed under Context. Mechanical: `app.message.as_ref().unwrap().error` → `app.last_notification().unwrap().error`, etc. Handle decision 4 for the clear-sites. Verify: `cargo test --workspace` before any rendering work, so behaviour is still proven.

### 4. Placement and stacking geometry (`cecilia-app`, frontend-independent)

Files: `notify.rs` (+ tests).

- `App::notification_layout(&self, card_size: impl Fn(&Notification) -> (u16/f64 w, h)) -> Vec<(NotificationId, Rect)>` is awkward across cell/px units: instead expose `fn notification_area(&self) -> Rect` (score-panes union via `pane_rects()` filtered to `PaneKind::Score`, or `viewport`; fallback to `viewport` if there is no score pane) and `fn anchor(&self) -> (Horizontal, Vertical)`; the frontends place cards with a small shared pure function `place_stack(area, anchor, sizes, gap) -> Vec<Rect>` in `notify.rs`, generic over `f64` or tested for both. Prefer one `f64` implementation both frontends call (TUI casts cells to f64 and back).
- Tests: each of 8 anchors places the first card at the right edge/corner; stacking direction; cards stay inside `area`; overflow cards are dropped from the end.

### 5. TUI rendering

File: `crates/cecilia-tui/src/lib.rs`.

- Remove the `cmdline` layout row and `draw_cmdline`; the status row becomes the last row. Where the command palette/picker used `cmdline`, check the call at `:978–990` and any other use (the layout array destructuring at `:734`) and fix.
- `draw_notifications(f, app, ui)` after panes/status, before the which-key and picker overlays: bordered `Block` cards (`Clear` first), error cards with the theme error colour, `×N` badge in the title, the timeout bar as the card's bottom border row: draw the first `round(fraction × inner width)` cells of the bottom border with `▔`/`─` in the card colour and the rest in the dim border colour (a 1-row "thin" bar is the best a terminal allows), width = clamp(text width + 4, 20, area.width/2), wrap text to the width (cap 3 lines, ellipsis).
- Register each card rect in `ui.hits` (see how other hit lists are cleared at `draw` start) so a click dismisses it.
- Verify: unit-test `place_stack`; run the TUI manually (`cargo run --release -- song.gp5`, trigger an error with `:nonsense`), check all anchors via `:set`.

### 6. GUI rendering

File: `crates/cecilia-gui/src/draw.rs` (+ `lib.rs` click handling).

- Remove `CMDLINE` and `draw_command_line`; fix `bottom` (`:1030`) and `y0` in `draw_status` (`:3726`); search for other users of `height - STATUS - CMDLINE`-style arithmetic and the headless `--screenshot` size assumptions.
- `draw_notifications(scene, app, views, ..)`: rounded cards (reuse the chip/popover helpers used for the which-key popup and tooltip), theme `error` colour accent bar for errors, `×N` badge, stacked-card peek edges, a 2 px timeout bar inside the card's bottom edge (clipped to the rounded corners; width = `fraction × card width`, anchored left), fade-out over the last 250 ms (optional; skip if it complicates redraw scheduling).
- Click hit list in `views` → dismiss, like other clickable chrome.
- Make sure the event loop keeps redrawing while cards are visible so expiry shows: if `app.notifications_visible().next().is_some()`, use the 100 ms `WaitUntil` branch (`gui/src/lib.rs:~405`); the TUI loop's poll timeout likewise.
- Verify with the screenshot tool: `target/release/cecilia -W scratch --screenshot out.png --keys ":nonsense<CR>" song.gp5` for top-right/scores (default), then `-o ui.notifications.anchor=bottom -o ui.notifications.area=window`; look at the PNGs.

### 7. Toolbar tooltip

Files: both frontends. The TUI drew `ui.tooltip` on the removed row; GUI has `draw_tooltip(scene, rect, tip, width)`. Draw the TUI one the same way (a one-line popover just below the hovered button, clamped to the screen). Verify manually by hovering.

### 8. Which-key shows pending count/operator/awaiting

Files: `crates/cecilia-app/src/marks.rs` (`which_key`), possibly moved to a new `whichkey.rs` if it grows; tests in the same file (existing `completion_help_and_which_key_follow_the_kind` in `edit_tests.rs:1381` and the marks tests must still pass).

- Build the title from `count` + `ed.operator` (+ count after operator) + `format_keys(pending)`, e.g. `3`, `d`, `2d3`, `"`, `q`.
- Rows: with an operator, the motions/text objects from the Normal keymap continuation (reuse the `continuations` code path with the operator's keys as the prefix, or if operators aren't keymap prefixes, a short fixed list: `w` word… *check how `d` is bound — `Command::Operator` in the default keymap — and let `continuations` do the work if it is a normal prefix*); with only a count, a hint row `…` + the keys that can follow is too broad: show just the title with one row `count` / `type a command`. With `Awaiting::Register|RecordMacro|PlayMacro`, one row hinting `a-z` register name.
- Remove the count/pending drawing from both frontends (done with `draw_cmdline` removal in steps 5–6; this step makes sure nothing is lost). Add tests: `3` → popup titled `3`; `d` → popup titled `d` with motion rows; `"` → register hint; popup gone after the command executes or `<Esc>`.

### 9. Notification history picker and commands

Files: `picker.rs` (`PickerKind::Notification`, `parse` aliases `notification`/`notifications`), `command.rs` (`Command::Notifications`, `Command::NotifyDismiss`), `editor.rs` (dispatch + `:notifications`, `:notify-dismiss`), `config.rs` default keymap (`<leader>nh`, `<leader>nd`), `docs/keymap.md`, command completion list if commands are enumerated in `complete.rs`/`palette.rs` (grep for an existing command name such as `buffers` to find every registry).

- Persistence (decision 11): `load_history()` in `App::new` (skip in tests: gate on the same `swap`/workspace-dir override the tests use, or inject the path via `Config` so tests use a temp dir — follow how `swap_dir` is configured in `config.rs:341`); append in `push`.
- Tests: push → file has a line; reload → entries present; truncate to `history_size`; `history_size = 0` writes nothing; picker lists newest first and filters by typing.

### 10. Docs, spec, changelog, cleanup

- `docs/SPEC.md`: wherever it says "message"/command line (grep `message`, `command line`, `status line`) describe notifications; `docs/keymap.md`: new keys + which-key now shows counts/operators; `docs/config.example.toml`: the six options; `CHANGELOG.md` `[Unreleased]`: Added (notifications, history picker, options, `:notifications`, `:notify-dismiss`), Changed (bottom bar removed; messages are notifications; pending keys in which-key).
- Remove dead code (`CMDLINE`, `draw_cmdline`, `Message`), re-grep for `message` use in both frontends.
- Final gate: `cargo test --workspace`, `cargo clippy --workspace --all-targets`, `cargo fmt --check`, screenshot of default and one non-default placement.

## Status

- [x] 1. Notification model + tests
- [x] 2. Options + tests
- [x] 3. Migrate `app.message` readers/tests
- [x] 4. Placement/stacking geometry + tests
- [x] 5. TUI rendering (incl. timeout bar)
- [x] 6. GUI rendering (incl. timeout bar)
- [x] 7. Toolbar tooltip relocation
- [x] 8. Which-key for count/operator/awaiting + tests
- [x] 9. History persistence, picker, commands, keys
- [x] 10. Docs, changelog, cleanup, final gate

Open questions:

- What should Enter do on a history entry (decision 12)? Default: just close the picker.
- "Pane scores": plan reads this as the union of the score panes' area. Say so if you meant each individual score pane (one notification stack per pane) or only the focused one.
- Default keys `<leader>nd` / `<leader>nh` are a proposal; confirm or give others.
