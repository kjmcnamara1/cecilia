# Commands

Every action in cecilia is a command: the same vocabulary is used by keymaps
(`[keymap.<mode>]` in `config.toml`, see [keymap.md](keymap.md)), the `:` command line, the
toolbar and `:source`d files. Commands are parsed in `crates/cecilia-app/src/command.rs`; the
scope of each is in `COMMAND_SCOPES` (`crates/cecilia-app/src/complete.rs`).

Conventions:

- `:name ARG` on the command line, or `"name ARG"` as a keymap action. A leading `:` is optional.
- `[ARG]` is optional, `a|b` a choice, `N` a number. `on|off` arguments toggle when omitted.
- _Aliases_ are listed after the name, separated by commas.
- A trailing `!` variant forces the action (discards changes, overwrites, skips confirmation).
- **Scope**: unmarked commands work on every score. _(modern)_ commands are refused on a chant
  score and _(chant)_ commands on a modern one ("not in a chant score"); the command line only
  offers the ones that apply.
- A bare number `:12` goes to bar 12, and `:5:3.5` to bar 5, beat 3.5.
- Many commands take a count in Normal mode (`3` before the key) and act on the Visual selection
  when there is one.

## Files, buffers and quitting

| Command | Description |
| --- | --- |
| `new [modern\|chant]`, `enew` | New empty score (modern by default). |
| `open [FILE]` | Open a file; without a name, the system file dialog. |
| `open-dialog` | System file dialog. |
| `write [FILE]`, `w`, `write! FILE`, `w! FILE` | Save the buffer, or save it as FILE (the buffer then takes that file; format by extension). An existing file other than the buffer's own is only overwritten with `!`. |
| `save-as [FILE]`, `saveas`, `save-as! FILE`, `saveas! FILE` | Save to a new file and make it the buffer's file; without a name, the dialog. An existing file is only overwritten with `!`. |
| `wall`, `wa` | Write every changed file. |
| `write-quit`, `wq`, `x`, `wqall`, `wqa`, `xall`, `xa` | Write every changed file, then quit. |
| `quit`, `q`, `quitall`, `qa` | Quit; refused while files have unsaved changes. |
| `quit!`, `q!`, `quitall!`, `qa!` | Quit, discarding changes. |
| `recover [SWAP]`, `recover!` | Restore the buffer from its swap file (`!`: even if another cecilia owns it). |
| `swap-discard` | Delete the buffer's swap file. |
| `buffer N`, `b N` | Show the buffer of bufferline tab N. |
| `buffer-next`, `bnext`, `bn` / `buffer-prev`, `bprevious`, `bprev`, `bp` | Next / previous buffer. |
| `buffer-alternate`, `balternate`, `balt` | Back to the buffer the pane showed before. |
| `buffer-close [N]`, `bdelete`, `bd` | Close tab N (default: the focused buffer). |
| `buffer-close! [N]`, `bdelete!`, `bd!` | Same, discarding changes. |
| `buffer-move N\|+N\|-N`, `tab-move` | Move the tab to position N or by N. |
| `buffer-move-next` / `buffer-move-prev` | Move the tab one place right / left. |
| `source`, `so` | Re-read the config. |

## Views, panes and layout

| Command | Description |
| --- | --- |
| `view N` | Go to view N. |
| `view-next` / `view-prev` | Next / previous view. |
| `view-new [FILE]` | New view, optionally opening FILE. |
| `view-close`, `view-close!` | Close the view (`!`: discard changes of buffers only it had). |
| `view-rename [NAME]` | Rename the view. |
| `split`, `sp` / `vsplit`, `vs` | Split the pane stacked / side by side. |
| `pane-close`, `close`, `clo` | Close the focused pane. |
| `focus-left`, `focus-down`, `focus-up`, `focus-right` | Focus the neighbouring pane. |
| `focus-next` / `focus-prev` | Focus the next / previous pane in layout order. |
| `focus-last` | Focus the previously focused pane. |
| `pane-move-left`, `pane-move-down`, `pane-move-up`, `pane-move-right` | Move the pane to that far side of the view. |
| `pane-exchange` | Swap the pane with the next one of its split. |
| `pane-rotate` / `pane-rotate-back` | Rotate the panes of the split. |
| `resize N\|+N\|-N`, `res` | Height of the pane in percent of its split. |
| `vertical-resize N\|+N\|-N`, `vresize` | Width of the pane. |
| `resize-equal` | Make every pane of the view the same size. |
| `maximize`, `max` | Show the focused pane alone, or the view's panes again. |
| `tracks-resize N\|+N\|-N` | Rows of the tracks panel. |
| `waveform-resize N\|+N\|-N` | Height of the waveform strips and bar, in pixels. |
| `layout [page\|vertical\|horizontal]` | Score layout of the pane; no argument cycles. |
| `scroll-center`, `scroll-top`, `scroll-bottom` | Scroll so the cursor is in the middle / top / bottom. |
| `zoom-in`, `zoom-out`, `zoom-reset` | Zoom. |
| `page-down`, `page-up` | Move a page. |

### Tool panes and toolbar

`on|off` toggles when omitted.

| Command | Description |
| --- | --- |
| `tracks [on\|off]` | Tracks panel (mixer). |
| `properties [on\|off]` | Properties pane. |
| `palette [on\|off]` | Editing palette. |
| `fretboard [on\|off]`, `piano [on\|off]` | Fretboard / piano panel. |
| `tuner [on\|off]` | Tuner (listens to the default audio input while shown). |
| `toolbar [on\|off]` | Toolbar. |
| `waveform [on\|off\|toggle]` | Audio (waveform) pane. |
| `waveform-strip` | Waveform strip of the focused pane. |

Keymap-only commands that drive the tool panes while they have the focus:

- Tracks panel: `tracks-focus`, `tracks-down`, `tracks-up`, `tracks-open`.
- Properties pane: `properties-focus`, `properties-down`, `properties-up`, `properties-first`,
  `properties-center`, `properties-last`, `properties-tab [score|track|lyrics]`,
  `properties-tab-prev`, `properties-edit`, `properties-change`, `properties-clear`,
  `properties-fold [open|close]`, `properties-step +1|-1`, `properties-box +1|-1`,
  `properties-staff-move +1|-1`, `properties-search`, `properties-search-next`,
  `properties-search-prev`.
- Palette: `palette-focus`, `palette-down`, `palette-up`, `palette-first`, `palette-last`,
  `palette-left`, `palette-right`, `palette-activate`, `palette-fold [open|close]`,
  `palette-search`, `palette-search-next`, `palette-search-prev`.

## Options, help and notifications

| Command | Description |
| --- | --- |
| `set OPTION [VALUE]` | Change an option for this session; no value or `default` resets it. |
| `get OPTION` | Show an option's value. Options are listed in [OPTIONS.md](OPTIONS.md). |
| `help`, `h` | Help. |
| `pick KIND` | Open a picker: `buffer`, `view`, `workspace`, `recent`, `mark`, `macro`, `keymap`, `section`, `notification` or `history`. |
| `pick-buffer`, `pick-view`, `pick-workspace`, `pick-recent`, `pick-keymap`, `pick-section` | Shortcuts for `pick KIND`. |
| `keymaps`, `maps` | Keymap picker. |
| `sections` | Section picker. |
| `notify [-s SUBJECT] [-t SECONDS] [-u info\|error] [-i ICON] MESSAGE` | Raise a notification (quotes group words; `-t 0` stays). Long flags `--subject`, `--timeout`, `--urgency`, `--icon`; urgency also takes `normal`, `low`, `critical`. |
| `notify-dismiss` | Dismiss the notification cards on screen. |
| `notifications` | Notification history picker. |
| `command-mode` | Open the command line. |
| `normal` | Back to Normal mode. |
| `nop` | Do nothing (to unbind a key). |

## Workspaces

| Command | Description |
| --- | --- |
| `workspace-save [NAME]`, `workspace-save!` | Save the session as a workspace (the active one without a name; `!` overwrites). |
| `workspace-open [NAME]`, `workspace-open!` | Save the current workspace and switch to NAME (created if new); without a name, a picker. `!` discards unsaved changes instead of saving. |
| `workspace-delete [NAME]`, `workspace-delete!` | Delete a workspace (the active one without a name); `!` skips the confirmation. Only workspace files go: a path to any other file is refused. |

## Moving the cursor

| Command | Description |
| --- | --- |
| `beat-next` / `beat-prev` | Next / previous beat. |
| `string-down` / `string-up` _(modern)_ | String or staff line below / above. |
| `staff-next` / `staff-prev` _(modern)_ | Next / previous staff of the track, keeping to the beat. |
| `note-next` / `note-prev` | Next / previous note. |
| `note-row-next`, `note-row-end`, `note-row-prev` | Along the cursor's row: next note not tied from the last (`w`), end of the tie (`e`), start of the tie (`b`). |
| `bar-next` / `bar-prev` | Next / previous bar. |
| `bar-start` / `bar-end` | First / last beat of the bar. |
| `bar-first-note` / `bar-last-note` | First / last beat of the bar with a note. |
| `goto-bar N`, `bar N` | Go to bar N (a bare `:N` works too). |
| `goto-first`, `goto-last` | First / last bar (or bar `count`). |
| `track-next` / `track-prev` _(modern)_ | Next / previous track. |
| `track N` _(modern)_ | Go to track N. |
| `voice N` / `voice-next` / `voice-prev` _(modern)_ | Edit voice N, or the next / previous (one more is created past the last). |
| `section-next` / `section-prev` | Next / previous bar that starts a section. |
| `set-mark`, `jump-mark` | Await a letter: set a mark here / jump to a mark's exact position. |
| `mark L`, `k L` | Set mark L (`a-z` local, `A-Z` global) at the cursor. |
| `goto-mark L` | Jump to mark L. |
| `delmarks LETTERS`, `delm` / `delmarks!`, `delm!` | Delete the listed marks / every lowercase mark of the file. |
| `marks` | Marks picker. |

## Modes and editing

| Command | Description |
| --- | --- |
| `insert` / `append` | Insert a new rest beat before / after the cursor beat, then Insert mode. |
| `insert-bar-start` / `append-bar-end` | Same, at the start / end of the bar. |
| `insert-last` | Insert mode where it was last left. |
| `insert-enter` | Insert mode: put a note at the cursor and go to the next beat. |
| `insert-next` | Insert mode: next beat (appending at the end of the bar). |
| `beat-new` | New beat after the cursor. |
| `visual`, `visual-bar` | Visual mode by beats / by bars. |
| `visual-reselect` / `visual-other-end` | Reselect the last selection / jump to its other end. |
| `visual-insert` / `visual-append` | Visual mode: insert a beat before / after the selection. |
| `replace-mode` | Insert mode where entering notes only changes the notes there. |
| `lyric-mode` _(chant)_ | Insert mode typing the cursor syllable's text (`<Esc>` leaves). |
| `delete`, `yank`, `change` | Operators, applied to a motion or the selection. |
| `put` / `put-before` | Paste after / before the cursor. |
| `delete-note`, `delete-beat` | Delete the cursor note / the whole beat. |
| `undo`, `u` / `redo`, `red` | Undo / redo. |
| `undo-older`, `earlier` / `undo-newer`, `later` | Walk the undo tree. |
| `repeat` | Repeat the last change. |
| `record-macro`, `play-macro`, `register` | Macro recording, playing and register selection. |
| `run-macro R` | Play the macro in register R. |
| `macros`, `registers`, `reg` | Macro picker. |

## Notes and beats

Commands here are for modern scores unless marked otherwise; `pitch-up`, `pitch-down`, `octave-up` and `octave-down` work on every score.

| Command | Description |
| --- | --- |
| `note PITCH` | Set the cursor note's pitch (`C#4` or a MIDI number). |
| `note-letter L` | On a standard staff: the note `a`–`g` nearest the cursor. |
| `accidental sharp\|flat\|natural\|double-sharp\|double-flat` | Accidental of the cursor note (also `#`, `b`, `=`, `##`, `bb`). |
| `pitch-up` / `pitch-down`, `octave-up` / `octave-down` | Shift the pitch by a semitone / octave. |
| `rest` | Make the beat a rest. |
| `slash` | A slash at the cursor beat (slash staff). |
| `duration N` | Note value: 1, 2, 4 … 128. |
| `duration-longer` / `duration-shorter` | Next longer / shorter value. |
| `dot`, `double-dot` | Dot / double dot (toggle). |
| `tuplet [N]` | Tuplet of N (default 3). |
| `toggle TECHNIQUE` | Toggle a technique (hammer, slide, bend, …) on the note. |
| `dynamic ppp\|pp\|p\|mp\|mf\|f\|ff\|fff` | Dynamic of the cursor note. |
| `dynamic-step +1\|-1` | Louder / softer by a step (selection too). |
| `beams auto\|force\|break\|break-secondary` | Beaming override. |
| `stems auto\|up\|down` | Stem direction. |
| `refinger` | Recompute string / fret assignment. |
| `transpose SEMITONES` | Transpose the track. |
| `tuning TUNING` | Set the tuning (`drop-d` or `D A D G B E`). |
| `capo FRET` | Set the capo. |
| `reflow` | Redraw barlines to match the time signatures (whole score or selection). |

## Bars and structure

Commands here are for modern scores, except `tempo`, which works on every score.

| Command | Description |
| --- | --- |
| `bar-insert-before` / `bar-insert-after` | New bar before / after the cursor bar. |
| `bar-open-before` / `bar-open-after` | Same, then Insert mode. |
| `bar-delete` | Delete the bar. |
| `time N/D` | Time signature from this bar. |
| `key KEY` | Key signature: sharps `1`..`7`, flats `-1`..`-7`, or a name (`G`, `Bb`, `Am`). |
| `tempo BPM` | Tempo. |
| `free-time` | Toggle free time. |
| `double-bar` | Toggle a double barline. |
| `bar-repeat 1\|2\|off` | Repeat the previous one or two bars with a sign. |
| `repeat-start`, `repeat-end [N]` | Repeat start / end (N: number of plays). |
| `ending N…\|none` | Alternate endings the bar belongs to. |
| `direction NAME` | Toggle a direction (D.C., D.S., Fine, Coda, …). |
| `section [NAME]` | Name a section starting at this bar. |
| `text [TEXT]` | Text above the beat. |

## Tracks and staves

Commands here are for modern scores, except `track-instrument`, `track-rename`, `track-rename-start`, `track-volume*`, `track-color`, `lyrics` and `clef`, which work on every score.

| Command | Description |
| --- | --- |
| `track-add [KIND]` | Add a track: `guitar` (default), `bass`, `drums`, `piano` or `voice`. |
| `track-delete` | Delete the track. |
| `track-rename NAME [N]` | Rename a track (the cursor's without a number). |
| `track-rename-start` | Rename the selected track in place. |
| `track-instrument [PROGRAM\|drums] [N]` | General MIDI program 1–128 for track N (the cursor's without a number); without an argument, a picker. |
| `track-volume PERCENT\|+N\|-N`, `track-volume-up`, `track-volume-down` | Track volume. |
| `track-color [#rrggbb\|off]` | Track colour; none cycles the theme palette. |
| `notation standard [clef]\|tab\|slash`, `staff` | Show the cursor's staff in another notation. |
| `staff-add [NOTATION]` | Add a staff to the track. |
| `staff-remove [N]` | Remove staff N (1 = top; the cursor's by default). |
| `staff-move N\|+N\|-N` | Move the staff. |
| `clef NAME` | Clef: `treble`, `bass`, `grand [split]`, `neutral` (standard), or `c1`–`c4`, `cb3`, `f1`–`f4` (chant). |
| `lyrics-attach above\|below [TRACK]\|none` | Attach lyrics to the staff. |
| `lyrics TEXT` | Add lyrics from the cursor on (`\n` = line break). |
| `lyrics-clear [VERSE]` | Remove a verse (all without a number). |
| `modern-to-chant` | Open the track's melody as chant in a new buffer. |

## Chant

Commands marked _(chant)_ only work on a chant score. See [CHANT.md](CHANT.md).

| Command | Description |
| --- | --- |
| `gabc [TEXT]` _(chant)_ | Show the cursor syllable as GABC, or replace it (`Glo(fg)` or just notes). |
| `interpretation [equalist\|solesmes]` _(chant)_ | Rhythm interpretation; none shows it. |
| `chant-append` / `chant-break` | New note after the cursor, in the same neume / starting a new one. |
| `chant-syllable` | New syllable after the cursor syllable. |
| `chant-join` / `chant-split` | Join selected syllables / split the syllable after the note. |
| `chant-mod MOD` | Toggle a GABC modifier (`v s o q w ~ . _ ' -`). |
| `chant-bar , ; : ::` | Division after the cursor syllable. |
| `chant-division DIV` | Same, accepting any division (`;3`, …); again removes it. |
| `chant-sep /\|!` | Toggle the separator after the note. |
| `chant-linebreak z\|Z` | Toggle a forced line break (`Z`: does not stretch the line). |
| `chant-custos` | Make the note a custos. |
| `chant-shape NAME` | Note head: `punctum`, `inclinatum`, `virga`, `virga-reversa`, `bivirga`, `trivirga`, `stropha`, `bistropha`, `tristropha`, `quilisma`, `oriscus`, `oriscus-stem`, `quadratum`. |
| `chant-alter flat\|natural\|sharp` | Alteration before the note. |
| `chant-liquescence deminutus\|ascendens\|descendens` | Liquescence. |
| `chant-mora`, `chant-episema`, `chant-ictus` | Mora, horizontal episema, ictus. |
| `chant-markup i\|b\|ul\|sc\|c\|tt\|e\|alt` | Toggle `<tag>` markup around the syllable text. |
| `chant-symbol SYMBOL` | Add `*`, `+`, `†`, `℣`, `℟`, `Ⱥ` to the syllable text. |
| `chant-clef c1-c4\|cb1-cb4\|f1-f4\|fb1-fb4` | Initial clef of the track (`b`: flat clef). |
| `chant-to-modern` | Turn the chant track into an ordinary staff track. |
| `lyrics-delete` | Delete the text of the syllable(s); the notes stay. |
| `lyrics-shift N` | Move the notes N syllables on (back when negative), the text staying. |
| `lyrics-select` | Select the same GABC in the lyrics text area. |
| `score-kind` | Say whether the score is modern or chant and which command converts it. |

## Playback and play-along

| Command | Description |
| --- | --- |
| `play`, `pause`, `stop`, `play-toggle` | Transport. |
| `speed PERCENT` / `speed-up` / `speed-down` | Playback speed (the steps are ±5). |
| `loop [A [B]\|off]` | Loop bars A–B; no argument loops the selection or bar (toggle). |
| `loop-toggle` | Loop the selection or current bar. |
| `trainer [START TARGET [STEP]\|off]` | Speed trainer, in percent (step 5 by default). |
| `metronome` | Toggle the metronome. |
| `count-in`, `countin` | Toggle the count-in. |
| `volume midi\|audio\|metronome\|master PERCENT` | Mixer level, 0–150 (`click` is the same as `metronome`). |
| `mute [N\|audio\|master]`, `solo [N\|audio\|master]` | Mute / solo a track, the audio track or the synth master (default: the cursor's track). |
| `audio [FILE\|off]` | Attach an audio file (the best match next to the score by default). It must be in the score's folder or below; the score needs a file name. |
| `mix [both\|audio\|midi]` | Mixer source; none cycles. |
| `pitch SEMITONES` | Pitch-shift the audio (clamped to ±12). |
| `sync` | Start / stop tapping sync points from the cursor bar. |
| `sync-tap`, `sync-clear`, `sync-detect` | Tap a point, clear all points, detect beats from the audio. |
| `sync-unit syllable\|note` _(chant)_ | What one tap syncs. |
| `sync-undo` | Take back the last tap (chant) or beat sync (modern). |
| `sync-remove` | Remove the sync point of the cursor (or selection). |
| `sync-here` | Sync the cursor note (chant) or beat (modern) at the playhead. |
| `sync-nudge MS` | Move the sync point under the cursor by MS milliseconds. |

## Page design

Design mode adjusts the page layout by hand (page layout only).

| Command | Description |
| --- | --- |
| `design [on\|off\|toggle]` | Design mode. |
| `bar-width N\|+N\|-N\|auto` | Width of the bar, in percent. |
| `bars-per-line N\|+N\|-N\|auto` | Bars on the line. |
| `beat-nudge N\|+N\|-N\|reset` | Move the beat, in tenths of a space. |
| `line-break` / `line-join` | Break the line after the bar / join it to the next. |
| `design-reset` | Clear the design overrides. |
