# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Built-in themes `cecilia-dark` (dark UI and page, light engraving), `cecilia-hybrid` (dark UI, light page, black engraving) and `cecilia-light` (light UI and page, black engraving), in the gold and cream of the logo on neutral charcoal: `[theme] preset = "cecilia-dark"`.
- The waveform stays still while the red playhead crosses it; at the right edge the playhead returns to the left and the waveform slides quickly forward to catch up (GUI).
- Audio sync of modern scores works by beat, as on chants: every beat has a stable id (`Beat::id`, kept through edits, new for pasted beats; saved in `.cc` files) and a sync point of its own. In Sync mode `<CR>` syncs the cursor beat at the playhead and moves to the next one; `:sync-here`, `:sync-nudge`, `:sync-undo`, `x`, `d` / `c` over a range, Visual mode and the mouse on the waveform (click to add, drag to move, plays while held) do what they do on a chant, the waveform's top ruler marks the beats of the time signature (4/4: major on 1, minor on 2 3 4; 6/8: 1 to 6), not every note, the red playhead at rest stands at the cursor beat's time and the MIDI notes sound where the ruler puts them. Bar points of a Guitar Pro score are kept (and become beat points) when the first beat is synced; `.gp` files still save bar points. Repeated bars are synced on their first play.
- The waveform's red playhead at rest stands at the cursor's cell on the ruler: on a division (`5:0`), at its own tick, not at
  the first note after it.
- A synced chant is timed note by note: the MIDI notes sound where the waveform's ruler puts them (before, they were
  spread evenly between the bars' sync points, so they drifted from the ticks).
- Sync points (taps, clicks, `:sync-here`) snap to the strongest onset of the audio within `ui.sync_snap` milliseconds
  (default 70; 0 turns it off).
- A chant's divisions take time in the audio sync (an eighth by default, between the notes either side of them): they have a
  tick on the waveform ruler of their own (`bar:0`), a click on the waveform can put a sync point on one (the nearest
  syllable *or* division), and `:sync-nudge` adds and moves theirs; Enter passes over them.
- A place in a chant is `bar:syllable.note` everywhere (status line, waveform ruler, `:3:4.2` goto): a bar starts after
  every division, which is itself `bar:0`; syllables count from 1 (not the divisions), a syllable's notes from `.1`;
  a missing note reads as `.1`. The waveform's ruler shows exactly these. Chant scores number their bars above the staff
  (`ui.bar_numbers`).
- `x` in Sync mode (`:sync-remove`) takes the sync point at the cursor (or of the selection) away.
- `S` in Normal mode enters Sync mode; Enter there passes over syllables without notes (rests, divisions).
- Chant glyphs are determined the way Gregorio does it (a port of `gabc-glyphs-determination.c`): a quilisma starts a
  glyph, so `efwg` is a punctum and a pes quilisma (drawn in one column), `h` + `V` is a virga reversa (stem on the
  left), `jghf` is a porrectus flexus, `ghf` a torculus; the glyph of a note too far from the last, of the same
  pitch, of a virga, a stropha, puncta inclinata and so on follow Gregorio's rules.
- A horizontal episema over (or under) a note in a space rides clear of the staff line.
- Sync mode: playback always starts at the cursor; `d` / `c` (with a motion, `dd`) take away sync points only, and in
  Visual mode `d`, `c` and the nudge keys act on every sync point of the selection. A mouse press on the waveform
  adds a sync point of a chant on the syllable (or note, by the sync unit) nearest the mouse (snapped to the amplitude) or grabs the one within 5 pixels,
  plays the audio while the button is held and drags the point.
- `:zoom-in`, `:zoom-out` and `:zoom-reset` zoom the focused pane: the waveform pane's timeline when it has the focus.
- Sync mode: `:sync` enters it (`SYNC` in the status line, `[keymap.sync]`), opening the waveform pane; `<Space>`
  plays and pauses, `<CR>` syncs the cell under the cursor and moves on by the sync unit, `<Esc>` leaves. The
  waveform shows sync points and rulers in this mode only.
- `:sync-detect` for a chant score: the pitch of the recording is tracked, cut into notes and the chant's
  notes are aligned with them (transposition and missing notes allowed).
- The waveform is drawn from the audio's samples at half-pixel columns with its RMS core; dragging along it
  scrolls it.
- The waveform pane has a fixed time ruler along the bottom (a tick every 0.2 s, a tall one each
  second labelled `minutes:seconds`) and, for a chant score, a bar/syllable ruler along the top that stretches
  with the sync points and shows before any is set (a tall tick at each bar labelled `bar:syllable`, a short one
  at each syllable labelled with the syllable); the colons stand on the ticks and the labels sit against the
  pane's edge. Every sync point is an unlabelled yellow line. `ui.waveform_time_ticks`, `waveform_beat_ticks`
  (`all`, `major`, `off`), `waveform_time_labels` and `waveform_beat_labels` set what is shown.
- A chant sync point can be on any cell: a note, a rest `()` or a division of its own (kept through edits by
  an id on the cell). `:sync-tap` steps by `:sync-unit` (a syllable's first cell, or every cell); `:sync-nudge`
  on a cell with no point adds one first. Sync points snap to the strongest onset of the audio within `ui.sync_snap` ms. A rest or division point does not change the timing of the notes.
- The page layout (GUI, terminal and exported images) follows the score's page setup: the width and height
  and the four margins of the Score tab (millimetres) set the page size and where the music starts, ends and
  breaks to a new page. (The default A4 page has the setup's 10 / 15 / 10 mm margins, not a flat 40 pt.)
- On a chant score `d`, `y` (and Visual `d` `y`), `x` and `<Del>` put what they take in the register: a syllable
  all of whose cells are taken goes whole, lyrics and all (it is gone from the chant); of a neume taken in
  part the notes are a group without lyrics. `p` and `P` put them in after / before the cursor syllable
  (`3p` three times), one undo step.
- `:lyrics-delete` (chant, `<S-Del>`): deletes the lyrics of the syllables of the Visual selection (the
  cursor syllable without one), keeping the notes. `:lyrics-shift N` moves the notes N syllables on
  (back when negative) among the syllables of the Visual selection, or from the cursor syllable to the
  end of the chant, the lyrics staying: `be-() ne-(i) díc(g')` shifted by -1 is `be-(i) ne-(g') díc()`.
- Noto Serif (SIL OFL, regular and bold) is bundled and is the default font of chant lyrics, as the
  versicle ℣, response ℟, dagger † and barred Ⱥ are always Noto Serif's. `ui.chant_text_font =
  "source-sans"` keeps the lyrics in the app's sans font.
- Chant text markup is engraved: `<b>` bold, `<ul>` underline, `<sc>` small capitals, `<c>` red, `<tt>`
  monospaced, `<e>` elision (small) and `<alt>` text above the staff, as well as `<i>`; `<sp>R/</sp>`
  `<sp>V/</sp>` give ℟ and ℣ (the chant font's own glyphs), `<sp>A/</sp>` the barred A, `<sp>'oe</sp>` `<sp>'ae</sp>` the accented ligatures, and
  a `+` in the text is the dagger †. The asterisk is the six-pointed star. Nothing in the text is colored
  unless `<c>` says so. A custos written in the notation (`g+`) is engraved where it stands.
- Chant Insert mode keys from the GABC shape table (`G` `V` `O` `<` `>` `r` `+` `x` `y` `#`; `+` turns the note into a custos, `:chant-custos`), and `z` / `Z`
  for a forced line break / one that does not stretch the line (`:chant-linebreak`), which the engraver
  now obeys. `:chant-alter sharp`.
- The chant palette has a barred A tool (`:chant-symbol Ⱥ`, `<sp>A/</sp>`), icons for ℣ ℟ Ⱥ drawn as the
  score draws them, a lyric mode tool and a tool for each text markup (italic, bold, underline, small
  capitals, red, monospaced, elision, text above the staff; `:chant-markup TAG`) under Text. A markup tool
  puts the tag around the cursor syllable's text, or takes it off; lyric mode types inside the tags.
- Bold and italic together are both engraved (bold, slanted).
- A chant sign (shape, liquescence, mora, episema, ictus, flat, natural, `:chant-mod`) used where the
  cursor has no note (an empty beat, a division, the end) puts a punctum on the cursor's staff line
  there first. In Insert mode an empty group follows the new note, as with `<CR>`.
- Chant GABC layout: the clef has a line of its own in the GABC editor, and a division entered on its
  own beat in the score gets a space before it and a line break after it, which go again when the
  division is taken off (the rest of the text stays as it was).
- The chant palette's icons (clefs, divisions, note shapes, liquescences, mora, episema, ictus) are the
  symbols the engraver draws, in its chant font, instead of their key letters.
- Lyrics on chant scores: `:lyrics TEXT` sets the cursor syllable's text, and `<A-i>` (`:lyric-mode`)
  enters a mode where the keys type it, shown in the score and the lyrics pane as you type. `<Tab>` and `<S-Tab>` move to the next and previous syllable. A space adds just a space to the GABC (on a syllable
  without text it is the space before the syllable), without rewriting the layout of the rest.
- Chant Insert mode keeps adding notes as vim's does: `<CR>` and `<Space>` put a punctum in the empty
  group under the cursor and add a new empty group after it; the cursor stays on the note. A division
  entered in an empty group gets an empty group after it too (and takes it with it when toggled off). Groups entered in
  the score are written without a space before them.
- Chant Replace mode (`R`, `:replace-mode`): Insert mode that adds no empty groups. `<CR>` moves the note
  under the cursor to the cursor's line (keeping its modifiers) or, on its own line, adds another note
  of that pitch in the same group (`(g)` → `(gg)`); its status label and edit cursor are the theme's red; `<Space>` does the same.
- `:chant-join` merges the syllables of the Visual selection (else the cursor syllable and the next)
  into one, `:chant-split` splits the syllable after the cursor note (each selected note). `/` and `!`
  toggle a neume separator after the cursor note (`:chant-sep`). `<A-J>` and `<A-S>` (Alt+Shift) run them in Normal, Insert and Visual mode.
- Leaving the lyrics text area of a chant score (normal mode, `<Esc>`, a focus key) puts the score
  cursor on the note the caret was in.
- Chant cells. The cursor of a chant score stands on notes, on empty beats `()` and on divisions
  written as their own symbol `(:)`. `i` `a` `I` `A` make an empty beat before / after the cursor
  cell and at the start / end of the bar (before its closing division); `<CR>` in Insert mode puts a
  note on the cursor's staff line into the empty beat and moves on to a new one (`<Esc>` takes the
  last empty beat away), `<Space>` moves on to the next cell. `,` `;` `:` put a division into the
  note's own parentheses, `(g::)`, make one of its own on an empty beat, `(:)`, or change the one
  the cursor is on; `:` goes round single, double, none. Visual mode highlights the selected cells
  as one band, as on modern scores, and `d` deletes them. In Insert mode `h` `l` only move along (the staff line stays); in Normal mode they put the cursor on the line of each note; `w` `e` `b` go between the notes on the cursor's staff position.
- `:lyrics-select` (`<leader>g` on a chant score) jumps from the cursor's note to the lyrics text
  area with the same GABC selected in Visual mode; the setting `ui.lyrics_select_cursor` (on by
  default) does the same whenever focus goes from the score pane to the lyrics pane.
- `:new [modern|chant]` (modern by default; `:new chant` opens an empty chant), and
  `:modern-to-chant` works on an empty score (an empty chant).
- The lyrics text area of a chant score keeps the GABC as typed: score edits splice into the kept
  text at the changed note (the rest of your layout stays), typing re-reads the chant after each
  key that parses (a parse error is marked at its position and the last good chant stays), notes
  keep their ids and audio sync across text edits, and the text caret and the score cursor follow
  each other.
- The GABC editor (the Lyrics text area) closes parentheses: `(` writes `()` with the caret
  inside (not before a letter), `)` before one steps over it, and Backspace between an empty pair
  removes both.

- The interface follows the kind of score. The status line has a `MODERN` / `CHANT` chip beside the
  mode (theme colors `score_modern`, new, and `icon_chant`; clicking it names the conversion
  command, `:score-kind`), a chant score's tab has the chant icon and its pane title ends
  `— chant`. The editing palette has its own sections for a chant score (clefs, divisions, heads,
  liquescences, mora / episema / ictus, flat / natural, pitch, text symbols), with folds and
  scroll remembered per kind. The properties pane shows a chant score's GABC header fields,
  interpretation, staff lines and page, and a chant track's name, clef and sound (no staves,
  tuning, capo or frets). New commands `:chant-shape`, `:chant-division`, `:chant-alter`,
  `:chant-liquescence`, `:chant-mora`, `:chant-episema`, `:chant-ictus`, `:chant-symbol`,
  `:chant-clef` and `:score-kind`, with default keys in `[keymap.normal-chant]` (`tp` `tv` `tq`
  `tm` `te` `tb` `td;` `tc3` …), a layer looked up only on chant scores.
- Every command has a scope (both, modern or chant). Completion, the key help, the keymaps picker
  and the which-key popup list only the commands of the score's kind; a command of the other kind
  still parses and answers `capo: not in a chant score`.

- Audio sync for chant scores. A chant score's sync is kept on its notes (stable note ids, kept
  by every edit; a point goes only with its deleted note, and the status line says so), so
  editing the chant never moves it. Time between synced notes is shared by the lengths the
  interpretation gives (mora, Solesmes lengthening); before the first and after the last point
  the nearest stretch's rate is used. The player still works on bars: bar sync points are derived
  from the note points after every change and are not saved. `:sync` / `:sync-tap` tap along with
  the singing: each tap syncs the next syllable (or note, `:sync-unit syllable|note`, also a row
  in the chant Score tab) at the playhead snapped to the nearest sung onset, the cursor follows
  and the status shows `tap: next <syllable>`; `:sync-undo` takes a tap back, `:sync-here` syncs
  the cursor note at the playhead, `:sync-nudge MS` moves the point under the cursor. Default
  keys `<leader>yh` `yu` `y[` `y]` `y{` `y}` `yy` `yn` in `[keymap.normal-chant]`. `.cc` files
  keep the ids and note points; `:w x.gabc` warns that audio and sync are not saved there.
  `:chant-to-modern` turns the bar points into the modern score's own and `:modern-to-chant`
  puts each bar point on the note that starts that bar. `:sync-detect` is not available for chant
  yet.

### Fixed

- The focus ring is red in Replace mode (it was the Insert color there), as the status line is.
- Visual mode: `o` moves the cursor to the other end of the selection (`:visual-other-end`).
- Visual selection in the score highlights the barlines between the selected beats too, instead of breaking at each barline.
- Sync mode: moving the cursor (Enter moves it on) no longer seeks the playback, which runs on by itself; with following
  on the cursor follows the playhead, with it off (`follow-toggle`) it stays where you put it; Enter syncs the note it is on.
- The playhead of a chant bar that wraps over two lines no longer sweeps to the end of the line and back
  at the end of its first part (it jumped at the bar after, e.g. 5:1, 9:1, 12:1 and 14:1 of the Salve Regina).
- Chant playback: two notes of one pitch together in a neume (`gg`) sound as one note of both lengths,
  not two attacks. (A stropha or bivirga still repeats its note.)
- Editing the score info `name`, `office-part` or `transcriber` of a chant score changes the
  title, subtitle and author shown in the score pane.
- On a chant score `w` / `e` / `b` step to the next note / the end of the note / the previous
  note again, and the cursor stands on the staff line of the note it moves to (`h` `l` too), so
  `j` / `k` then choose the pitch of a note to enter.
- In Insert mode on a chant score `<Space>` (a note in the same neume), `<CR>` (a note on a new
  syllable) and `/` (a new neume) enter the note on the staff line the cursor is on, not at the
  pitch of the note beside it.
- The asterisk in a chant's text is the red six-pointed one, as in lyrics.

### Removed

- `:chant-view` and the workspace's `chant_view`: a chant is always drawn in square notation.
  With the toggle off the score was drawn as an ordinary staff from the cached bars while the
  cursor and the edit keys still acted on the chant, so the cursor could sit off the staff.

### Changed

- Insert and Replace mode work on modern scores as on chants: `<CR>` and `<Space>` finish the beat the cursor is on and stay on it (`hjkl` move on) instead of moving to the next beat; Insert mode adds an empty beat after it (taken away again by `<Esc>` if nothing was entered), Replace mode (`R`, now on every score) adds none.
- The tracks pane is the only global tool pane along the whole width of the window; the others sit between the
  palette and the properties pane.
- One kind per score: a score is a modern score (staff tracks) or a chant score (exactly one chant
  track; `Score::is_chant()`). `:chant-to-modern` no longer changes the chant in place: it opens a
  new buffer `[modern] name` with the chant's bars as a pitched track, its syllables as lyrics
  text, title and audio sync kept; the chant buffer is untouched. `:modern-to-chant` says that
  only the cursor track was converted and other tracks were left out. `:track-add` refuses on a
  chant score (and a chant track on a modern one), pointing at the conversions. `cecilia convert
  in.gabc out.gp5` (and other modern formats) writes the same conversion. A `.cc` file from an
  older version with a chant track beside other tracks loads as a modern score, the chant track
  converted in place (there is no load message for it).

- Chant scores are edited through the chant, never through their bars. New edit operations on
  `Chant` (`cecilia-core/src/chant_edit.rs`) address notes by index in reading order: insert
  (same neume, new neume, new syllable), delete, move by staff positions, flat / natural /
  sharp, shape, mora, episema, ictus, liquescence, divisions, clef, syllable text, split and
  join. `Transcription::note_at` / `place_of` map the cursor to a note and back, so the cursor
  stays on the edited note. The editing keys of a chant track use them (`<C-a>` / `<C-x>` now move
  a note one staff position rather than one semitone; `accidental flat` adds the flat); keys that
  edit modern bars (durations, tuplets, rests, ties, effects, dynamics, bar insert / delete,
  paste …) report "not in chant", and an edit that changes the bars of a chant track without
  changing its chant is refused. `cecilia_convert::chant::reconcile` is gone.

- Modern lyrics syntax: lyrics anchors are `(bar:beat)` / `(bar:beat.fraction)` instead of
  `<bar:beat>`; `_` no longer prints italics (it only separates words and, alone, skips beats);
  styles use GABC markup tags `<i>` `<b>` `<ul>` `<sc>` `<c>` (italic, bold, underline, small
  capitals, red), which nest and span syllables, and `<sp>V/</sp>` specials show as their
  characters. GABC groups are no longer part of a modern track's lyrics (a chant track's lyrics
  stay its GABC). The engraver draws the styles; the terminal shows the text without tags.
- `.cc` format version 2: lyrics text saved by earlier versions is converted once when the file
  is read (`<bar:n>` anchors to `(bar:beat)` keeping the syllables on their notes, GABC groups
  removed, `_` italics to `<i>` tags). There is no message about it.
- Guitar Pro 3–5: lyric lines are translated into the new syntax on reading (comments to
  `[labels]`, empty notes to `_` words, line breaks kept) and the text is translated back on
  writing: one lyric line per verse row, line breaks kept, `_` words and dashes as empty notes,
  labels as comments, markup dropped. Rows past the fifth are left out and `:w` warns. A dash
  word's dash is lost in Guitar Pro 3–5.

### Added

- Positions are `bar:beat.fraction` everywhere (status line, marks, `:lyrics` and gabc+
  anchors): the eighth on the upbeat of beat 3 of bar 5 is `5:3.5`; a bar holding more than its
  count goes on counting (`2:6.0`). A beat is the time signature's unit. `:5:3.5` goes there;
  anchors `<2:6>` and `<2:6.0>` both read. Anchors used to count beats of the first voice, so
  `<bar:n>` in files saved before this means the n-th beat unit now.
- Lyrics text area: GABC is colored by part (clefs, divisions, notes, shape / attachment / space
  modifiers, markup tags; new theme colors `lyrics_clef` … `lyrics_markup`), the words keep the
  normal color and the parentheses are dimmed. `:` and Ctrl / Alt keys the keymap binds (`<C-s>`
  …) work in the text area's normal mode.
- The `*` of lyrics and chant is drawn red in the score.

- Lyrics text area: Visual mode (`v` `V`), operators with motions and text objects (`diw` `daw`
  `dap` `ci"` `yip` …), `W` `B` `E` `r` `~` `s` `S` `C` `X` `Y`; in insert mode `<C-Left>` /
  `<C-Right>` jump words, `<C-BS>` / `<C-Del>` delete by word; `H` / `L` still switch tabs and a
  focus key (`<C-h>` …) leaves the text area. Focusing the pane again restores the text area
  caret (or the GABC switch). With GABC on, ordinary tracks show the chant GABC inferred from
  the notes, live. `:lyrics` puts its line where it belongs in the text, ending in a newline.
  The font is the app's text font at 12 pt by default; `ui.lyrics_font` takes any installed
  family.
- Lyrics are one gabc+ text per track, the single source of the syllables on its notes: GABC
  (`text(notes)`) with dashes and skips that cover beats (`some -- one`, `some __ one`), `_`
  italics, `[labels]` shown before a syllable (verse numbers), `*` printed ✻, and `<bar:beat>`
  anchors. Syllables are sung in the order the music is played: repeats, alternate endings,
  D.C./D.S. al Fine/al Coda; the second pass of a repeat sings the next words on the second verse
  row. After every edit the text is laid out on the notes again. Guitar Pro lyrics (all
  versions) and lyrics read per note (GP6–8, MusicXML) become gabc+ text, every syllable staying
  on its beat. The score shows the labels, italics and ✻.
- The LYRICS tab of the properties pane edits that text under a GABC switch: off, the text shows
  without its GABC groups and clef (what is typed keeps the groups of the syllables still there);
  on, the raw gabc+ from its clef `(c4)`. A chant track's lyrics are its chant's GABC. The text
  area has no label, box or background, is drawn in the app's text font by default (`ui.lyrics_font`, any
  installed family, and `ui.lyrics_font_size`, 12 pt) and colored by new theme colors in every built-in theme:
  `lyrics_text`, `lyrics_text_dim`, `lyrics_gabc` (bold), `lyrics_label` (bold) and
  `lyrics_anchor`.

- `docs/LYRICS.md`: a reference for lyrics: gabc+ syntax with worked examples, how syllables
  are placed (sung order, verse rows, edits), drawing, the LYRICS tab and its keys, commands,
  settings, file formats and limits.

### Changed

- Selecting the lyrics text area (it is the only row of the LYRICS tab now) or clicking in it puts
  the normal-mode (block) caret there; `<CR>` there is `i`, `<Esc>` in normal mode leaves the text
  area and the properties pane.
- The LYRICS tab of a chant track always shows its GABC (it had a switch); the tab of any other
  track shows its text without its GABC groups.
- `ui.chant_chords` is used only by `:modern-to-chant` and `cecilia convert … .gabc`.
- `:clef` on a chant track takes only chant clefs (`c1`–`c4`, `f1`–`f4`, `cb3`); `:notation` and
  `:staff-add` refuse on a chant track, which has one square-notation staff.

### Removed

- The chant staff of ordinary tracks (`:notation chant`, `:staff-add chant`, `:clef f3` on a
  staff of an ordinary track, the chant entries of the staff and clef pickers, the chant staff in
  the terminal's tab systems and in the GUI) and everything that edited through it (`<CR>`
  punctums, `:chant-mod`, `:chant-bar`, `:chant-append`, `:chant-syllable` on an ordinary track).
  Square notation belongs to chant tracks: convert a melody with `:modern-to-chant`. Old `.cc`
  files that name a chant staff (and its `chant_line`) on an ordinary track load without it,
  keeping the track's other staves (or the default ones).
- The GABC switch of the LYRICS tab, and the GABC inferred from the notes it showed.
- `:lyrics TEXT` adds an anchor at the cursor (its beat in the first voice) and the words to the track's gabc+ text (`\n` a line
  break); words over notes that already have some go on the next verse row. The verse number
  argument is gone. `:lyrics-clear` clears the text; `:lyrics-clear N` removes a verse row.
- Syllables past the last note are kept in the track's gabc+ text (they were kept in the score's
  Guitar Pro lyric lines).

### Changed

- The default colors of Insert and Command mode are swapped in every theme preset: Insert is the green, Command the amber.

### Removed

- `:insert-here` (Normal `<CR>`): `R` (Replace mode) and `gi` enter Insert mode without adding anything.

## [0.32.0] - 2026-11-06

### Added

- `<CR>` (`:chant-syllable`) on a chant staff of an ordinary track enters a plain punctum at the cursor's position with the insert duration (when the beat has no note) and moves on to the next beat, as on a standard staff (it used to be refused). Each such punctum is its own neume until the track has lyrics; then the syllables condense the notes into pes, clivis, torculus etc.

### Added

- `:reflow` redraws the barlines of the cursor's voice to match the time signatures: beats that run over a bar move to the start of the next, bars that come up short are filled from the next, and a beat that straddles a barline is split and tied (durations and pitches never change). It works over every bar, or in Visual mode over just the selection, whose last bar keeps whatever is left over. Other voices are not touched; bars with repeat signs are refused.

### Fixed

- Lyrics follow the notes: after an edit the text is laid out on the notes again, one syllable to a note and never two on one. Deleting a note hands its syllable (and the rest of the text) on to the following notes, new notes take the next syllables, and syllables past the last note are kept in the score's lyric text but not shown. Setting lyrics with more syllables than notes keeps the rest instead of reporting an error. Bars whose notes don't match their text (changed some other way, chant tracks) are left alone.

### Changed

- Removed `:chant-note` and the `a`–`m` / `A`–`M` keys of chant insert mode: a pitch is entered by moving the cursor to its position and pressing `<CR>`.
- `<CR>` on a chant staff of an ordinary track moves a note the beat already has to the cursor's position (chant is monophonic), keeping its signs, and moves on to the next beat.
- `:` on a chant staff cycles the major division, the double division and none (it only switched between the first two).
- Normal mode: `w` `e` `b` on a chant staff of an ordinary track jump to the next / previous note at the cursor's position, as on a standard staff.
- Chant engraving: the ictus never crosses a staff line; the mora sits a little further from its note, high when the note is on a line and low when it is in a space; a division is spaced midway between its beat and the next; the stropha (`s`) is now drawn on a chant staff of an ordinary track.
- Insert mode on a chant staff: `h` `j` `k` `l` move the cursor as on a standard staff (they were the GABC pitches `h`–`l`; enter those by moving there and `<CR>`, or as `H`–`L`), and `<Space>` goes to the next beat (on a chant track it still adds a note to the neume).
- The other staves of a track light the note under the cursor on a chant staff only while the cursor is on that note's position (they lit the beat's note wherever the cursor was).

## [0.31.0] - 2026-11-05

### Added

- Voices (Guitar Pro's up to four independent rhythmic streams per track): `:voice-next` / `:voice-prev` (`]v` `[v`, `<A-v>` `<A-V>`) and `<A-F1>`…`<A-F4>` (`:voice N`) switch the voice being edited; a voice the bar doesn't have yet is created empty (no rest, 0/4 filled) and the cursor keeps its time position. The status line shows `V2` … when the bar has several voices or voice 2–4 is being edited. `cc` clears only the cursor's voice, deleting a voice's last beats leaves it empty again, and typing into an empty voice fills it.
- The editing palette starts with four voice buttons (1–4), centred and without a title, that switch the voice being edited (`:voice N`) and show which one is in force. Not offered on chant staves or over a selection.
- `<Del>` in insert mode deletes the whole beat (`:delete-beat`) instead of leaving a rest; `<BS>` still deletes the note. In normal mode `<CR>` is `:insert-here` (was `<leader>i`).
- Insert mode: `<Right>` moves to the next beat like `l` (it used to append one); the arrow keys now do exactly what `h` `j` `k` `l` do.
- The GUI dims the voices other than the one being edited in bars that have several.
- Fixed: voice 1 lost its cursor in a bar where it was still empty once another voice held music.
- Saving to `.gp5` warns when voices beyond the two it stores hold notes.
- The terminal tab view draws every voice on a shared time grid (columns where any voice has a beat), with the cursor in the voice being edited and the other voices dimmed.
- Tab fret numbers of different voices that clash on one string at one moment are drawn side by side.

## [0.30.0] - 2026-11-05

### Added

- `:save-as [file]` saves the buffer under a new name: with no file it opens the system save dialog (filters for the formats cecilia writes, `.cc` first); with a file it writes there. The buffer is renamed, so later `:w` writes to it; `:w file` is the same as `:save-as file`. The dialog suggests the buffer's file name, else the kebab-case score title. A plain `:w` on a buffer with no file now runs `:save-as` instead of failing.
- A file open in a buffer that is changed outside the app flags the buffer modified and asks whether to load the changes (`y` reloads, `n` keeps your version); a deleted file flags the buffer modified (`:w` recreates it).
- The chant clef can be moved to another staff line anywhere a chant staff is shown: `:clef c1`–`c4` / `f1`–`f4` now also work on a chant staff of an ordinary track (the notes keep their pitch and are re-placed at the new clef; stored as `Track.chant_line`, old files load as before), and the tracks pane's clef box steps through `c1`–`c4`, `f1`–`f4` (and shows the clef with its line).

## [0.29.0] - 2026-11-05

### Added

- Workspaces are managed explicitly: `:workspace-open [name]` saves the current workspace and switches to `name` (created, empty, if new; no name opens the picker), `:workspace-open!` does the same without saving it (and discards unsaved changes in files). `:workspace-save [name]` saves under a new name, leaving the old file as it was (a copy), and refuses to overwrite another workspace unless `:workspace-save!`. `:workspace-delete [name]` deletes a workspace file after asking (no name: the active one, leaving no workspace); `:workspace-delete!` doesn't ask.
- Settings `ui.workspace_autosave_on_quit` (default on: quitting saves the active workspace; `:quit!` never does) and `ui.workspace_restore_last` (default on: `cecilia` with no arguments loads the last workspace or creates `default`; off: the dashboard, with no workspace).

### Changed

- There can be no workspace (files named on the command line, or after `:workspace-delete`); quitting then saves nothing.
- A workspace is no longer saved whenever the session changes, but on `:workspace-save`, when switching and on quit.
- In the properties pane the staff reorder keys are now `<A-j>` / `<A-k>` instead of `<C-j>` / `<C-k>`.

### Removed

- `:workspace-close` (use `:workspace-open default`, or `:workspace-delete` to leave a workspace).

### Fixed

- The GUI's command palette and pickers cast a soft, blurred shadow instead of a hard-edged one.

## [0.28.0] - 2026-11-04

### Added

- Chant notation in the terminal UI: a four-line text staff with the clef, neumes as note glyphs (punctum, virga, inclinatum, quilisma, oriscus, stropha, cavum, liquescents; `tabline.icons = "none"` uses ASCII), ligatures joined by a stroke, ledger lines, flats, naturals and sharps, divisions (virgula, minima, minor, maior, finalis) crossing the staff lines, custodes at the line ends, and the lyrics under their neumes with hyphens and extenders through melismas. It follows the pane's layout like tab does: wrapped systems (spread to the full width, each with its clef), page boxes of A4 proportions, or one horizontal line scrolled by syllable; the cursor, selection, playhead and `zz`/`zt`/`zb` work on it, and clicking a note or a syllable's text puts the cursor there. A staff set to chant with `:notation chant` on an ordinary (e.g. Guitar Pro) track shows the same way: the track's melody as chant, as the GUI's chant staff builds it (clef and shift fitted to the scale, neumes, divisions at rests, syllables from the track's lyrics). A track whose staves are all chant uses the chant layouts above; a chant staff among other staves is drawn above or below the tab in each system, on the beat columns of the tab, with its own cursor, clicks, selection and playhead, and switching the notation back changes the rendering at once.

### Fixed

- The terminal UI's progress bar is dark like the tabline and status line, with light text, and its sections are told apart by gray vertical bars instead of shaded cells.

## [0.27.0] - 2026-11-04

### Added

- The terminal UI shows the rhythm of `tab+` staves as one row under the strings, a music note or
  rest glyph per beat (Nerd Font glyphs; `tabline.icons` `unicode`/`none` give symbols or letters).
  Eighths and shorter notes in one quarter-note group are joined by a beam row (`─`, `═` for
  sixteenths) below. Clicks on the rhythm rows pick the beat.

## [0.26.0] - 2026-11-04

### Added

- Dashboard: when no buffer is open (the last one closed, the only view closed, or a new, empty
  workspace) the window shows buttons Open Recent, Select Workspace, New Score and Open Score, in
  the window and the terminal, under the lily icon (a flower glyph in the terminal), the name and a
  tagline, with no tabline or placeholder tab. `j`/`k`/`Tab`/`Shift-Tab`/arrows move, `Enter` or a click runs one.
- Setting `ui.keep_files_in_new_view` (default on): off, `:view-new` starts the new view on the
  dashboard instead of with the current files. The dashboard is per view and saved in workspaces.
- Recently closed files are remembered (up to 20, kept between sessions) for the dashboard's Open
  Recent button and `:pick recent`.

- Terminal UI page layout: lines and pages break where the GUI's page layout puts them (taken from the engraver, so manual line breaks and bar widths count), each page is drawn in a box with a "Page N" title, and design mode works there: the same keys, `+` / `-` buttons and draggable bar lines at each line, `↵` marking manual breaks. The engraver's design handle geometry (`Handle`, `Grab`) moved from `cecilia-gui` to `cecilia-engrave::design`.

### Fixed

- The terminal UI honours the horizontal layout: one line of bars that scrolls sideways, following the cursor, instead of wrapped systems scrolling vertically.

### Changed

- `:pane-close` on the last pane of the last view no longer quits: it says it cannot (`:q` quits).
- `:view-close` on the only view closes its buffers and shows the dashboard instead of failing.
- The Linux desktop icon is now the ivory lily alone (`assets/lily.svg`) instead of the full emblem.

## [0.25.0] - 2026-11-04

### Changed

- **Breaking:** the project is renamed from muse to cecilia: the binary, crates, window/app ID,
  desktop entry and config/data directories (`~/.config/cecilia`, `~/.local/share/cecilia`) are
  now `cecilia`; move existing config and workspaces over by hand.
- **Breaking:** the native format uses the `.cc` extension instead of `.muse`.
- The Linux desktop icon is now `assets/cecilia-transparent.svg`.

## [0.24.0] - 2026-11-04

### Added

- `ui.focus_ring_width` (0 to 8, default 4) and `ui.focus_ring_color` (default: the mode's color) set the ring around the focused pane; the terminal UI takes the color, and a thick border above width 4.
- `:focus-ring-width`, `:focus-ring-color` and `:tabline-wrap` change these settings for the session (no argument shows the value; `:tabline-wrap` toggles); the width clamps like on config load.
- `tabline.wrap` (default on): off stops `:buffer-next` / `:buffer-prev` at the last / first tab instead of wrapping.

## [0.23.0] - 2026-11-04

### Added

- `ui.equalize_on_close` (`:equalize-on-close [on|off]` toggles it) (default on): closing a pane gives the split it was in equal weights again, so the remaining panes share the space equally; other splits keep their sizes. Off: the neighbour takes the closed pane's space as before.

## [0.22.0] - 2026-11-04

### Added

- `gg` / `G` in the focused properties pane select its first / last property (`:properties-first`, `:properties-last`). `zz` scrolls the pane so the selected property is in the middle (`:properties-center`).

### Changed

- Each staff row of the track properties pane has two boxes: the notation (standard, tab, tab+, slash, chant) and the clef (treble, bass, grand, neutral; do, fa for chant; empty and dimmed for tab and slash), each with its own picker and icon. `h` / `l` (`properties-box`) or a click select the box. The Add Staff row picks a notation.
- In the properties pane `h` / `l` (and the arrows) move focus between boxes, and `<A-h>` / `<A-l>` step the value (switch, choice, number) instead; staff reorder is now `<C-j>` / `<C-k>`.

## [0.21.1] - 2026-11-04

### Fixed

- The whole-note circle of a tab+ staff sits level with the feet of the stems.
- The cursor follows the note to its new staff position after `:pitch-up`/`:pitch-down`/`<C-a>`/`<C-x>` on standard notation.

## [0.21.0] - 2026-11-04

### Changed

- The TUI tab view draws bar lines as continuous vertical lines through every string row, with box-drawing joints where they cross the staff lines (`┬┼┴`), cut flush at the start and end of each system (`┌├└`, `┐┤┘`), and the staff lines as `─`. Double bars (`╥╫╨`), the final bar (thin + heavy), free-time bars (`┆`) and the lines at the start and end of each system use the same style. Repeat signs are a thin + heavy pair with two dots on the middle rows (`:│┃`, `┃│:`, and `:│┃│:` where a repeat end meets a repeat start). The bars with wider lines take 2 to 5 columns, accounted for in the line breaking, cursor and click positions. See SPEC.md.

## [0.20.0] - 2026-11-04

### Added

- `gg` / `G` in the focused palette select its first / last row (`:palette-first`, `:palette-last`), in every mode including Visual; the view scrolls to it.
- The palette's keyboard motions wrap around at its edges: `j` / `k` / `Tab` / `S-Tab` from the last line to the first and back, `l` / `h` from the end of a line of buttons to the next / previous line and around the ends of the grid. Counts wrap step by step. Only the visible (filtered, unfolded) tools count. `ui.palette_wrap = false` stops at the edges as before; the mouse is unchanged.
- `zz` / `zt` / `zb` scroll the view so the cursor sits in the middle / at the top / at the bottom of the score pane (the left / right in the horizontal layout), without moving it (`:scroll-center`, `:scroll-top`, `:scroll-bottom`). The view then stays put until the cursor leaves it.

### Changed

- The barline closing a free-time bar (and so the one between two free-time bars) is drawn dashed, on every staff, the grand staff joins and the TUI tab view (`┆`). The line that closes the previous normal bar stays solid, and repeat ends, double bars and the final barline stay solid.


## [0.19.0] - 2026-11-04

### Added

- Palette tools: double dot, fermata, double sharp / flat, crescendo / decrescendo, fade in / out and volume swell, artificial harmonic, left-hand tapping, grace notes before / on the beat, the six slides (shift, legato, in from above / below, out downwards / upwards), arpeggio down / up, rasgueado, pickstroke down / up, beams (automatic, force, break, break secondary) and stems (automatic, up, down), and the bar tools: clef, key and time signature, free time, double bar line, repeat one / two bars, repeat open / close, alternate endings, directions and section. Tools that need an argument open the command line with the command typed. Commands: `:double-dot`, `:free-time`, `:double-bar`, `:bar-repeat 1|2|off`, `:ending`, `:direction`, `:beams`, `:stems`, `:accidental double-sharp|double-flat`, more `:toggle` techniques; `:key` also takes names (`Bb`, `F#m`).
- The palette searches like the properties pane: `/` opens a field, the selection jumps to the first matching tool or section as you type (name, tooltip, command or section title), `<Esc>` goes back, `<CR>` keeps the jump, `n` / `N` repeat (`:palette-search`, `:palette-search-next`, `:palette-search-prev`); only the tools shown for the staff and mode are searched.
- Palette icons: the GUI draws the engraver's own music-font glyphs (notes and rests, accidentals, dynamics, fermata, repeat signs, clefs, bow signs …) and pictograms for beams and stems; the terminal uses the closest Unicode / Nerd Font music glyphs, following `tabline.icons` (`none`: short text labels).
- Visual mode has the palette's range tools: techniques (staccato, accents, effects, strokes, fermata …) toggle on every beat or note of the selection, crescendo / decrescendo make one hairpin across it, dynamics go on its first beat, legato / slides / ties chain each beat to the next, `+` / `-` and the note-value tools change every beat's duration (a triplet over three beats; the status line reports a bar that runs over), rest-out, semitone and octave shifts, accidentals, beams and stems. Bar selections add the bar tools (repeat open / close, section, key and time signatures, double bar, direction, free time, alternate endings) for their first, last or all bars. New palette items: longer, shorter, semitone up, semitone down. Every operation is one undo step and a button is tinted when the whole selection has it. In Visual mode `+` / `-` step the durations.
- `.` repeats the last palette tool at the cursor (a toggle such as staccato, ghost note or let ring toggles there; a duration bump or note value, an accidental or a dynamic applies again), over a selection of the same number of beats or bars if the tool was used over a Visual selection, and with the same argument for tools that ask for one (clef, key, time signature, direction, section, endings). Navigating or searching the palette does not replace it; editing keys still do.
- Dynamics over a Visual selection change the whole section (every note, not only the first beat), and `:dynamic-step +1|-1` (palette "Louder" / "Softer", `<A-+>` / `<A-->` in Visual mode) moves the section one dynamic up or down keeping the differences, clamped at ppp and fff. The engraver prints one mark per change, so a uniform section has one at its start and the beat after shows its own.
- New score data, kept in `.muse` files (older files still open): bar repeat signs, per-beat beam and stem overrides, fermatas, fade out and volume swell, left-hand taps, and the written double sharp / flat of a note. The engraving honours the beam and stem overrides and shows fermatas, pick strokes, fades, repeat signs and double accidentals; directions print as "D.C. al Coda" and the like. Guitar Pro 7 files keep fades, left-hand taps and repeat signs; MusicXML keeps stems, fermatas and double accidentals.

### Fixed

- Engraving: tremolo picking (one to three slashes through the stem, or above the fret on tab), tap / slap / pop (`T` `S` `P` above the staff), grace notes (small head, slashed before the beat; the fret in small digits on tab), slides into and out of a note (a short diagonal at the note instead of "sl."), fade in / fade out / volume swell (labelled swell wedges), left-hand tapping (a `+` above the note), and harmonics (a diamond head on standard staves, `<fret>` on tab with "A.H." for artificial ones, also "P.H." "T.H." "S.H.").
- With the palette focused in Visual mode, `h` `j` `k` `l` and the arrows move in the palette's grid instead of the score selection; the selection stays for the tools, and `<Esc>` back to the score keeps it.

### Changed

- A dynamic belongs to the beat, not to its notes: `Beat.dynamic` drives the velocity of all the beat's notes in playback, the palette, the range tools, `:dynamic-step` and the engraver (one mark per beat where it changes). Older `.muse` files and formats with a velocity per note are read by giving each beat its first note's velocity, and every note of a beat is written with the beat's dynamic.


## [0.18.0] - 2026-11-04

### Added

- `gv` (`:visual-reselect`) reselects the last Visual selection of the file (same anchor, cursor and beat / bar kind; clamped if the score shrank); in Visual mode it swaps with the current selection. `gi` (`:insert-last`) starts Insert mode where Insert mode was last left. The marks `` `< `` `` `> `` `` `^ `` jump to the selection's ends and that insert position.

### Changed

- Let ring and palm mute are drawn as one span over consecutive beats, as in Guitar Pro: the label (`let ring`, `P.M.`) at the first beat, a dashed line across the beats that have the effect and a hook down at the end of the last. A rest (or a beat without the effect) ends a span; spans run on across bars and, without the label, over line breaks. Both together stack as two lines. They are shown on tab staves, and on the standard staff when the track has no tab.
- `gi` is now vim's `gi` (`:insert-last`); the old `gi`, Insert mode on the cursor beat adding nothing (`:insert-here`), is on `<CR>`.

### Fixed

- Accents (`>`), heavy accents (marcato `^`) and staccato dots are engraved. They were stored and played back but never drawn. On standard and slash staves they stand on the side of the heads away from the stem (the dot nearest, the accent outside it); on tab and tab+ staves, when the track shows no standard staff, above the strings. Tied notes and rests show none, and lyrics above the staff clear them.
- Select with the mouse: press on a beat and drag across others to highlight them as a Visual selection (anchor at the pressed beat, the cursor end follows the pointer, bars and systems included; in the GUI the view scrolls when the pointer is held near or beyond the pane's edge). The selection stays after the release for `d` `y` `c`, the palette and the other keys. `Shift`+click extends the selection, `Ctrl`/`Alt` selects whole bars (as `V`), a double click selects the bar. A plain click is unchanged. Works in the terminal too. The logic lives in `muse-app` (`App::mouse_press`, `mouse_drag`, `mouse_release`).


## [0.17.0] - 2026-11-04

### Added

- Design mode for the page layout (`:design`, `<leader>d`): fix clashes by hand. Bar widths (`:bar-width`), bars per line (`:bars-per-line`, `:line-break`, `:line-join`) and beat positions (`:beat-nudge`) can be set with commands and the keys of the new `[keymap.design]`, or in the GUI by dragging bar lines and the dots under the cursor bar's beats (live re-flow) and clicking the "+" / "-" buttons at the end of each line to add or remove a bar on it. Every change is an undo step (one drag is one); `:design-reset` returns to the automatic layout. The adjustments are stored in `.muse` files and follow bars through inserting, deleting and pasting.

## [0.16.0] - 2026-11-04

### Fixed

- On a slash staff the cursor always sits on the staff's single line: clicking above or below the line selects the beat only, and the cursor's line is normalised whenever it lands on a slash staff (click, `<A-j>` / `<A-k>`, `j` / `k`, loading a score).

### Changed

- On a chant staff the status line shows the solfège syllable of the cursor's position (`do` `re` `mi` `fa` `sol` `la` `ti`, read from the clef in force there) instead of the GABC letter.

### Added

- The status line shows, in every mode, how full the cursor's bar is, as quarter-note beats of the voice under the cursor over the time signature's length (`3.0/4.0`, `4.5/4.0`; 6/8 is `3.0`, 7/8 `3.5`), in the error color unless the bar is complete. Free-time bars and a pickup bar show nothing.
- `ui.highlight_incomplete_bars = true` (and `:incomplete-bars on|off|toggle`) draws the staff lines of bars that are too short or too long in red, on every kind of staff in the GUI and on the dashed staff lines of the terminal tab view. The color is the new theme color `incomplete_bar`.

## [0.15.0] - 2026-11-03

### Added

- Vim-style editing workflow. Normal mode: `w` / `e` / `b` move along the cursor's string (tab) or line / space (standard) by notes and ties (`:note-row-next`, `:note-row-end`, `:note-row-prev`); `^` / `gh` and `$` / `gl` go to the first / last beat of the bar with a note (`:bar-first-note`, `:bar-last-note`); `I` / `A` insert a beat at the start / end of the bar and start typing (`:insert-bar-start`, `:append-bar-end`); `dl` `dh` `cl` `ch` `yl` `yh` act on the beat under / before the cursor, also on the last beat of the score.
- Visual mode: `i` / `a` insert a beat before / after the selection (a bar selection: at the start of its first bar / the end of its last) and start typing (`:visual-insert`, `:visual-append`).
- Insert mode: Enter finishes the beat (on a standard staff it puts a note at the cursor's line or space, on a slash staff a slash; a tab's fret is already in) and moves to the next beat, to the start of the next bar after the last complete beat, with the duration chosen in the palette (`:insert-enter`); `h` `l` `j` `k` move the cursor (not on chant, where they are pitches); `<A-CR>` adds a beat without typing into it.
- Normal-mode keymaps per notation, `[keymap.normal-tab]`, `[keymap.normal-standard]`, `[keymap.normal-slash]` and `[keymap.normal-chant]`, looked up before `[keymap.normal]` like the insert ones; on a standard staff `t#` `t&` `t=` make a note sharp, flat or natural.
- The editing palette lists only the tools that apply to the mode and the staff under the cursor: accidentals on standard staves, bends and palm mute on tabs, note values and ties on slash, note values only on chant and in Visual mode.

### Changed

- `i` and `a` now add a rest beat before / after the cursor beat (in the insert duration) and insert into it; `o` and `O` add a bar and start Insert mode. The old `i` (Insert mode on the cursor beat, adding nothing) is `gi` / `:insert-here`; `:bar-insert-after` / `:bar-insert-before` still add a bar without starting Insert mode.
- Normal `w` / `b` follow the cursor's row and ties; the old next / previous beat with a note is `W` / `B`. `$` is the last beat with a note (the last beat of the bar is `<End>`, the first `0` / `<Home>`).
- `x` deletes only the note under the cursor, also on a piano or other non-fretted track, where it used to clear the beat (on a slash staff it still clears the beat).
- `c` over beats (`cl`, `ch`, `cw`, visual `c` …) turns them into rests of the same lengths instead of deleting them and adding one beat; `cc` clears only the cursor track's bar.
- Insert mode: `<CR>` is Enter as above (a new beat after the cursor is `<A-CR>`); on a tab `h` and `l` (hammer-on / pull-off, let ring) are `H` and `L`, and on a standard staff hammer-on / pull-off is `H`.

## [0.14.0] - 2026-11-03

### Added

- `:clef` picks the clef of the cursor's staff by its notation: `treble`, `bass`, `grand [split]` or `neutral` (the percussion clef, now the clef of drum staves) for standard notation, `c1`–`c4` or `f1`–`f4` for chant. Tab and slash staves have no clef. On a chant track it still takes `c1`–`c4`, `f1`–`f4` and `cb3`.
- `:lyrics-attach above|below [TRACK]` (or `none`) attaches lyrics to the cursor's staff, above or below it, taken from any track (the staff's own by default) and aligned with the staff's beats by position in the bar. Lyrics above a staff sit just over the highest notation, stem, mark or chord name over them, and under a section marker. Lyrics below a staff (the track's own or attached) are placed under whatever hangs below it, including dynamics and hairpins, which now sit closest to the staff; syllables that fall on the same place or too close are set one after another, never overlapping.
- `ui.bar_numbers = "every"` (and `:bar-numbers every|system|toggle`) numbers every bar in the engraved score, not just the first bar of each line.

### Changed

- A grand staff now splits between its treble and bass halves at C3 (MIDI 48) by default instead of middle C; `:clef grand NOTE` still sets another split.
- `:staff` is now `:notation NOTATION` (`standard`, `tab`, `tab+`, `slash`, `chant`); `:staff` still works as an alias. `:notation standard` keeps the staff's clef. `<A-j>` / `<A-k>` still move between staves.

## [0.13.1] - 2026-11-03

### Fixed

- GUI: scrolling to keep the edit cursor in view now targets the cursor itself (its string or staff position) instead of the whole beat's height, so a zoomed view no longer snaps to the top staff; moving between staves or strings in the same beat also scrolls. The cursor is always scrolled fully into view, horizontally too, at any zoom (including a bar wider than the pane and the centred page layout).

## [0.13.0] - 2026-11-03

### Added

- `<F1>` shows the key bindings of the current mode in every mode (insert, visual, command line, picker and tool panes included), from the effective keymaps; `<F1>` or `<Esc>` hides it.

## [0.12.0] - 2026-11-03

### Added

- The GUI edit cursor has a rounded outline (theme color `cursor_border`, default contrasting with the page) so it shows in every mode.

## [0.11.1] - 2026-11-03

### Changed

- The `tab+` staff engraves rests in the middle of the strings, and both tab staves set their time signature in standard-size digits close together on the middle line.

## [0.11.0] - 2026-11-03

### Added

- `ui.chant_chords` (`top` default, `bottom`, `error`): which note of a chord the chant staff of
  an ordinary track shows, since chant is monophonic. With `error` no chant staff is drawn: it
  shows, and the status line says, where the first chord is ("Chant staff: chord at bar 12 beat 3
  — chant is monophonic"). `:modern-to-chant` follows the same setting.
- The chant staff of an ordinary track shows the neumes `:modern-to-chant` produces: notes are
  grouped into syllables (lyrics, or quarter notes) and neumes (pes, clivis, torculus, porrectus,
  climacus, scandicus …) by shared code in `muse-core`, joined by slanted strokes, with the same
  clef and transposition, mora on long notes and divisions after rests (a rest of a quarter or
  more is `:`, a shorter one `,`), double bars (`:`) and the end (`::`). A beat's own chant
  division wins over those. `neume_start` marks still force a new neume.
- A test checks that playback is the same whichever staves a track shows.

### Changed

- Square notation (GABC files and chant staves of ordinary tracks) is set with the bundled Caeciliae
  chant font (SIL OFL 1.1): its heads, podatus, clivis, porrectus, virga, inclinata, quilisma,
  oriscus, stropha, clefs, custos, divisions, flats and mora. Neumes are composed from its
  glyphs and connecting lines; a neume is set whole at its first note, so it stays joined
  across a barline or line break.
- The chant staff's four lines now span the height of a modern five-line staff (ordinary tracks
  and GABC); note heads are taller. A pes is a stacked column, a clivis has its stem, a
  torculus sits side by side, and falling runs of three or more notes are a climacus (a virga
  then diamonds, also in `:modern-to-chant` output). The custos sits on the open end of the
  staff lines.
- Square notation: note heads with flat sides and an arched top and bottom (the lower note of a
  pes nearly square), larger mora dots, a tall rounded C clef with a round notch, a custos and a
  red initial; the same in GABC files.
- On the chant staff of an ordinary track only the first note of a neume sits in the beat column
  shared with the other staves; the rest are packed right after it and joined, across barlines
  too. The staff has no barlines, only divisions, and a custos at the end of each line shows the
  next line's first note.
- The edit cursor on a chant staff is a single square on the note position of that staff, not a
  column across all staves; `<A-j>` / `<A-k>` onto a chant staff land on the beat's note.

## [0.10.1] - 2026-10-31

### Fixed

- `<C-d>` / `<C-u>` (and `<PageDown>` / `<PageUp>`) move the cursor half a screen: half the
  displayed bars in the horizontal layout; in the vertical layout half the displayed rows of
  bars (rounded up, at least one), and in the page layout half the rows on a page whatever the
  zoom, staying at the same place in the bar and on the same line.

## [0.10.0] - 2026-10-31

### Added

- `:staff-next` / `:staff-prev` (`<A-j>` / `<A-k>`) move the cursor between the staves of a track.
- `<A-j>` / `<A-k>` select the next / previous item in pickers and in command-line completion,
  like `<Tab>` / `<S-Tab>`.
- `:focus-next` / `:focus-prev` (`<C-w>w`, `<C-w>W`) cycle the focus through the panes of the view.
- `:focus-last` (`<C-w>p`) switches back to the previously focused pane.

### Changed

- `j` / `k` stay on the cursor's staff, with four ledger-line positions above and below a staff
  for extra notes, instead of running on into the next staff. Set `ui.jk_moves_between_staves =
  true` for the old behaviour.

## [0.9.0] - 2026-10-31

### Changed

- Swap files keep the undo tree, so recovering unsaved changes (automatic or `:recover`) no longer
  loses undo history. Version 1 swap files still load, without history.
- Closing a buffer or view, or quitting, with unsaved changes asks whether to save each changed
  file (`y`), discard it (`n`) or cancel, instead of refusing.
- A new score is not marked as changed until it is edited, so an empty one closes without a prompt.

### Fixed

- Directional pane focus (`focus-left` etc.) goes to the most recently focused of the panes
  touching the current one, instead of the one nearest its centre.

## [0.8.0] - 2026-10-31

### Added

- Shift and Super (Windows/Cmd) in key notation: `<C-S-q>` is distinct from `<C-q>`, `<S-F5>`,
  `<S-Up>`, `<D-q>` (aliases `Super-`, `Win-`, `Cmd-`). Shift on a plain character still folds
  into it (`Q` is `<S-q>`). The GUI reports both modifiers; the terminal UI enables the kitty
  keyboard protocol where supported.

## [0.7.0] - 2026-10-31

### Added
- Swap files: with `[swap] auto_recover` (default on) opening a file with a newer swap file restores it as unsaved changes. buffers with unsaved changes are autosaved to a recovery copy in the swap directory (`[swap]` `enabled`, `interval`, `dir`; default `$XDG_STATE_HOME/muse/swap`, else `~/.local/state/muse/swap`), removed on save, close and quit. Opening a file with a newer swap file offers `:recover` (`:recover!`, `:recover swapfile`) and `:swap-discard`.

## [0.6.1] - 2026-10-31

### Fixed

- The edit cursor now shows on empty bars, e.g. after inserting a bar with `o`.

## [0.6.0] - 2026-10-31

### Added

- Editing palette along the left of the window, above the bottom tool panes, which keep the full width, also in restored workspaces (`<leader>e` shows it, `<leader>E` focuses it, `ui.palette` (off by default), `:palette`, `[keymap.palette]`): a grid of icon buttons in collapsible sections (durations, dotted/triplet, rest, accidentals, ties, dynamics, articulations, effects) that apply to the score pane worked on, like Guitar Pro's editing panel. Buttons in force at the cursor are tinted and follow it; one gray highlight is shared by the mouse and the keyboard selection, and its button's name, with its insert-mode keys, shows at the bottom of the pane (and as a tooltip on hover; the terminal draws short labels). `h`/`j`/`k`/`l` move through the grid, `<CR>` applies, `za` folds. Buttons that can't take effect at the cursor (a note-level edit on a rest, an accidental on a tab) are dimmed, and applying one only says why. New `:dynamic ppp|pp|p|mp|mf|f|ff|fff` sets the note's velocity.
- `:staff-add` takes several staves, added in the order given (`:staff-add treble tab grand G3`).

## [0.5.0] - 2026-10-31

### Added
- With the cursor on a note on any staff, the same note on the track's other staves is drawn in the current mode's color: the notehead, slash or square note on treble, bass, grand, slash and chant staves, and the fret number (in bold) on other tab staves.

### Fixed

- Opening a workspace with a pane kind this build doesn't know drops that pane instead of showing it as a score pane.
- The progress bar no longer panics in a pane narrower than a pixel.

## [0.4.0] - 2026-10-30

### Added
- Toolbar buttons for the properties pane, waveform, piano, fretboard and tuner, in the group with the tracks button (they show whether their pane is on).
- Properties pane along the right of the window (`<leader>p` shows it, `<leader>P` focuses it, `ui.properties` (off by default), `:properties`) with three tabs: SCORE (metadata, page setup), TRACK (name, kind, color, tuning, frets, capo, staves, instrument, channel, bank, volume, pan, mute, solo) and LYRICS (a text area in Guitar Pro lyric syntax, laid onto the notes). Each property is a label over a full-width value box, in sections that fold (`za`, `zc`, `zo`). Editing a value is insert mode, applied to the score as you type as one undo step (`<Esc>` cancels; the lyrics text area is edited like vim, with its modes in the status line). Color opens a picker of named colors with swatches and hex codes (type `#rrggbb` or `rgb(r,g,b)` for a custom one); tuning opens a picker of common tunings for the track's instrument (type notes for a custom one). Edits are undoable. Staves are rows (a picker of staff kinds, an empty row to add one, drag or `<A-j>`/`<A-k>` to reorder), mute and solo are switches, `H`/`L` change tab, `c` clears and edits, `d` clears. `h`/`l` set switches and step through choices (no wrapping), icons show staves and instruments, only a double click edits, and a line shows where a dragged staff lands. Standard staves are named `treble`, `bass` and `grand` (`:staff grand G3`); `standard …` still parses. The wheel scrolls the pane and pickers; numbers step with `h`/`l`; arrows and `<Tab>` follow `hjkl`. `/` searches the pane (and the lyrics text) with `n`/`N`. Slash staves draw a single line with their stems always up. A Guitar Pro file's lyric line breaks show in the pane. The wheel scrolls the lyrics text, and a click there places the caret, a drag selects, a double click selects a word (or enters insert mode from normal mode), `<C-a>` selects all, and `<C-c>`/`<C-x>`/`<C-v>`, `y`, `yy`, `p` and `P` use the system clipboard.
- Scores keep their page setup, read and written by Guitar Pro 5 files.

### Fixed

- `j`/`k` stop at the last/first row of a track instead of jumping into the neighbouring track.


## [0.3.0] - 2026-10-30

### Added

- Scrollbars on score panes (`:scrollbar`, `ui.scrollbar`): pill-shaped thumbs in a light tint of
  the background, in their own gutter so they never overlap the notation. Click or drag to
  scroll, in the GUI and TUI.
- GUI: score panes scroll both ways when the score is bigger than the pane (a narrow vertical
  layout, a tall horizontal layout, a zoomed page), with a scrollbar for each direction. The
  vertical wheel scrolls down and the horizontal wheel (or Shift) across, in every layout.

### Fixed

- GUI: the vertical layout fits panes narrower than 300pt instead of cutting off the bars.

## [0.2.0] - 2026-10-30

### Added

- Waveform tool: `:waveform` toggles the pane and `:global-tool waveform` toggles its global status.
- Tracks pane refits its height when the buffer of a pane changes.

### Changed

- `close-pane` is now `pane-close`; `quit` (`:q`, `:qa`) exits muse instead of closing a pane; `write-quit` (`:wq`, `:x`) writes every changed score and quits.
- The audio tool is now called the waveform tool.
- Progress bar matches the Go Playalong look: paper background, solid black dividers, no bluish tint.
- The pane _maximized_ indicator in the titlebar is an icon instead of "Z".

### Fixed

- Progress bar's red position indicator is half as wide in the GUI.
