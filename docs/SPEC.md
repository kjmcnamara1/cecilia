# cecilia — Specification

Status: living document. Written from the initial requirements session (2026-10-23) and kept in step
with the code (version 0.44.0, pre-1.0). Sections and items marked **Planned** are not implemented.

## 1. Purpose

**cecilia** is an open-source, keyboard-first application for reading, editing, composing and playing:

- guitar (and other fretted) tablature,
- standard notation (5-line staff, incl. piano grand staff and voice with lyrics),
- drums / percussion notation,
- Gregorian chant in square notation,

with conversion between them, Guitar Pro file compatibility, MIDI playback, and
GoPlayAlong-style synchronisation of a recorded audio file to the score.

Comparable to: Guitar Pro (editing, file format), GoPlayAlong (audio sync / practice),
neovim (modal keyboard control, buffers/windows/tabs, TOML-configurable keymaps).

## 2. Licensing & platforms

- License: dual **MIT / Apache-2.0**. No GPL dependencies (keeps App Store distribution possible).
- Bundled assets must be permissively licensed (fonts: SIL OFL). Planned: a bundled SoundFont (MIT or
  equivalent); none is bundled today.
- Platform order: **Linux → Windows/macOS → Android → iOS**. Core crates must be platform-neutral
  from day one (no direct filesystem/audio/windowing calls outside frontend/platform crates).

## 3. Architecture

Cargo workspace; frontends are thin shells over a headless application core.

| crate | responsibility |
|---|---|
| `cecilia-core` | Score data model (pure data, `serde`). No I/O. |
| `cecilia-formats` | Import/export of all file formats. |
| `cecilia-convert` | Staff↔tab linkage, auto-fingering, retuning, chant↔modern transcription. |
| `cecilia-engrave` | Layout engine → backend-agnostic display list + hit-test map. |
| `cecilia-audio` | Audio output, decoding, MIDI synthesis, time-stretch/pitch-shift, beat detection, tuner. |
| `cecilia-app` | Headless app state: buffers, panes/views/workspaces, modes, commands, keymaps, undo, config; playback and play-along control. The audio↔score sync map (`AudioSync`) is in `cecilia-core`. |
| `cecilia-tui` | ratatui frontend. |
| `cecilia-gui` | winit + vello (wgpu) frontend (desktop; mobile is planned). |
| `cecilia` | Binary: CLI entry point, launches GUI/TUI/headless commands. |

Dependency direction: `core ← formats/convert/engrave/audio ← app ← tui/gui ← cecilia`.
`cecilia-app` never depends on a frontend; both frontends drive the same command registry, keymaps and config.

### 3.1 Key third-party choices

| need | crate |
|---|---|
| audio output | `cpal` (ALSA/PipeWire/JACK, CoreAudio, WASAPI, AAudio) |
| audio decode | `symphonia` (mp3, flac, ogg/vorbis, wav, aac/m4a) |
| MIDI synth | `rustysynth` (SoundFont 2) |
| time-stretch / pitch-shift | `signalsmith-stretch` (MIT) |
| TUI | `ratatui` |
| GUI | `winit`, `vello` (which renders through `wgpu`) |
| config | `toml`, `serde`, `directories` |
| CLI | `clap` |
| zip containers | `zip` |
| XML (GP7/GPX/MusicXML) | `roxmltree` (reading; the writers build the XML as text) |

## 4. Data model (`cecilia-core`)

```
Score
├─ metadata: title, subtitle, artist, album, words, music, copyright, tab author, instructions, notices
├─ master_bars[]: time signature, key signature, tempo changes, repeat start/end(count),
│                 alternate endings, section marker, directions (segno/coda/fine/D.C./D.S. …), triplet feel
├─ tracks[]
│  ├─ name, instrument kind (Fretted | Drums | Pitched | Chant), MIDI program/channel/bank,
│  │  volume/pan, color, mute/solo
│  ├─ fretted: strings (tuning, MIDI pitch per string), fret count, capo
│  ├─ staves: which views are shown (tab, standard, slash)
│  ├─ lyrics_text: the track's lyrics as text (see 4.3), source of the beats' syllables
│  └─ bars[] (one per master bar; repeat sign `simile`) → voices[] (up to 4) → beats[]
│        beat: duration (value, dots, tuplet), rest/empty, text, chord diagram ref,
│              lyrics, stroke/brush, pick stroke, tremolo/whammy bar, slap/pop/tap, fade in/out,
│              volume swell, fermata, hairpin, octave sign, beam and stem overrides,
│              dynamic (one per beat: a MIDI velocity all its notes play at)
│        notes[]: string+fret (fretted) or pitch (staff/chant), velocity, tie, dead, ghost, accent,
│                 written alteration (double sharp / flat), effects (see 4.1)
│  └─ chant (chant tracks; see 4.2)
├─ lyrics, chord library, page setup
└─ extras: opaque key/value store preserving unsupported source-format fields for round-trip
```

Time is measured in **ticks** (960 per quarter note). Every note has a canonical MIDI pitch; tab
(string/fret) is a *view* of pitch for fretted tracks, so staff and tab are always linked.

### 4.1 Technique set (target: Guitar Pro 7/8 parity)

Bends (all GP bend types, arbitrary curve points), pre-bend/release, slides (shift, legato, in/out
above/below), hammer-on/pull-off, left/right-hand tapping, vibrato (note & beat, wide), harmonics
(natural, artificial, pinch, tapped, semi, feedback), palm mute, let ring, dead/ghost notes,
accents (normal/heavy), staccato, grace notes (before/on beat, dead, transition), tremolo picking,
trill, whammy/tremolo bar (dive, dip, release, return, custom), slap/pop, rasgueado, golpe, brush/
arpeggio strokes, pick strokes, fingering (left/right hand), dynamics, fade in/out, volume swell,
let-ring/palm-mute ranges, chord diagrams (barres, fingerings), free time, multiple voices,
tuplets (incl. nested), lyrics, repeats, alternate endings, directions.
The goal is that anything not yet editable still round-trips through `extras`.

The editing palette (`cecilia-app`, `palette.rs`) exposes these as tools: durations, accidentals
(down to double sharp / flat), dynamics and hairpins, articulations, effects, the six slides,
strokes (brush, arpeggio, rasgueado, pick strokes), beam and stem overrides, and the bar tools
(clef, key and time signature, free time, double bar, bar repeat signs, repeats, alternate
endings, directions, sections). Each runs an ordinary command and is one undo step.

Free-time bars close with a dashed barline (also between two free-time bars); special lines (repeat
end, double bar, final barline) stay solid.

### 4.2 Chant model

The interface follows the kind of score (`Score::is_chant()`): the status line has a `MODERN` or
`CHANT` chip, the bufferline and the pane title name the kind, the tool palette, the properties
pane rows, the keys (`[keymap.normal-chant]`) and the commands offered by completion, help and
which-key differ (every command has a scope: both, modern or chant), and a command of the other
kind answers with the kind (`capo: not in a chant score`). A chant is always drawn in square
notation (the toggle between it and the transcription is gone: in the transcription the cursor
and the edit keys act on the chant, so the cursor sat off the drawn staff).

The full logic of chant ⇄ modern conversion (pitch mapping, clef fitting, syllables, neumes, signs, transcription) is in [CHANT.md](CHANT.md).

Chant track = sequence of syllables with text underlay; each syllable carries neume groups.
Elements: clefs (C/F on any line, flat clefs), punctum, virga, punctum inclinatum (diamond),
podatus/pes, clivis, torculus, porrectus, scandicus, climacus, quilisma, oriscus, liquescents,
salicus, strophae, horizontal/vertical episema, mora (dot), custos, bar lines (quarter, half,
full, double), flats/naturals. Pitch stored as staff position + clef, resolved to MIDI pitch for
playback/conversion.

As built (M6): a chant track holds `chant` (syllables with their GABC text and items: notes with
their modifiers kept in written order, spaces, clefs, divisions, alterations, custodes, anything
else verbatim) and, in its ordinary bars, a transcription of it: one free-time bar per phrase
between divisions, one eighth-note beat per sounded note (a mora doubles it; the `solesmes`
reading also lengthens episemata and the note before a quilisma). Do under a C clef is C5. The
transcription is what plays, exports to modern formats and carries the cursor, but it is only a
**cache**: the chant is the source of truth, and the bars of a chant track are never edited. Every
edit is an operation on the `Chant` (`cecilia-core/src/chant_edit.rs`) that addresses notes by
their index in reading order (the order of `Chant::resolved`, which is also the order of the
beats); `chant::retranscribe` then rewrites the bars, as one undo step. `Transcription::note_at`
and `place_of` map the cursor's `(bar, beat)` to a note and back (every beat of a stropha or
bivirga maps to its note), so the cursor stays on the edited note. Operations: insert a note
before or after another (in the same neume, a new neume, or a new syllable), delete a note,
move it by staff positions, flat / natural / sharp, shape (punctum, inclinatum, virga, stropha,
bivirga, quilisma, oriscus, quadratum), mora, horizontal episema, ictus, initio debilis,
liquescence, insert / replace / delete a division, change the clef (the first one or from a
note on), set a syllable's text, split and join syllables. Keys and commands that edit modern
bars (durations, tuplets, rests, ties, effects, bar insert / delete, paste, note letters …)
report "not in chant" on a chant track, and the central edit path refuses any edit that changes
the bars of a chant track without changing its chant.

**Chant scores.** A score is either a *modern score* (staff tracks only: standard, tab,
slash) or a *chant score*: exactly one track, with `chant` set (`Score::is_chant()`, the one test
the app uses). A `.gabc` file reads as a chant score; a chant file cannot hold other tracks, and
`:track-add` refuses on a chant score (and refuses a chant track on a modern one), pointing at the
conversions. Moving between the kinds is explicit and never changes the buffer you start from:

- `:chant-to-modern` opens a **new buffer** `[modern] name` holding a modern score: the chant's
  bars as a pitched track (name, instrument and volume kept), master bars and title kept, the
  syllables as lyrics text on the beats, audio kept in place (the derived bar sync points become the
  modern score's own). `cecilia convert in.gabc
  out.gp5` (any modern format) does the same.
- `:modern-to-chant` opens a **new buffer** `[chant] name` from the cursor track only; other
  tracks are left out (the message says so).
- A legacy `.cc` file (from before the `.ccc` / `.ccm` split) that holds a chant track beside other
  tracks loads as a modern score, the chant track converted in place as `:chant-to-modern` does (there is no message).

To accompany a chant: `:chant-to-modern`, clean up the bars, `:track-add organ`. There are no chant
staves on ordinary tracks (`:notation chant` and `:staff-add chant` are gone; a legacy `.cc` file that
still names a chant staff on an ordinary track loses it on load). To see a hymn in square
notation, convert it with `:modern-to-chant` or `cecilia convert in.gp5 out.gabc`. The conversion
is built in
`cecilia-core` (`melody::ChantView`):

- Only the first voice is read. Chant is monophonic, so a chord shows one note, chosen by
  `ui.chant_chords`: `top` (default, the highest note: a "skyline"), `bottom`, or `error`. Under
  `error`, or whenever a melody cannot be chosen, `:modern-to-chant` refuses with a message
  ("chord at bar 12 beat 3 — chant is monophonic"). `ui.chant_chords` is used by the conversion
  only.
- Syllables start at each lyric (at each quarter note when there are none); notes in a syllable
  form neumes by contour (`cecilia_core::chant::group_neumes`, shared with the GABC engraver), and a
  note marked `neume_start` (`/`, `:chant-break`) always starts a new one. Tied beats extend the note before them.
- The melody is moved by the same shift to the white-key scale (B♭ allowed) and clef as
  `modern-to-chant`; a note longer than the typical one gets a mora.
- Breath and phrase marks become divisions: a rest shorter than a quarter note is a divisio
  minima (`,`), a quarter or longer a divisio maior (`:`), a double bar a divisio maior (`:`), and
  the end of the track a finalis (`::`). A division set on a beat (`:chant-bar`; `,` `;` `:` `::`)
  overrides the rest or bar rule. The model has no separate breath or fermata marks.

### 4.3 Lyrics

A track's lyrics are one text, `Track.lyrics_text`, in the lyrics syntax: words, `-` joins,
dash and skip words (`-`, `_`), `[labels]`, `*` (✻), `(bar:beat.fraction)` anchors and GABC markup
tags (`<i>` `<b>` `<ul>` `<sc>` `<c>`) for styles; GABC groups are not part of it. It is the single source of
the syllables on the beats (`Beat.lyrics`, one string per verse row, a trailing `-` for a
hyphen), which `cecilia_core::gabcplus` lays out from it after every edit, so the lyrics follow
the notes. A chant track's lyrics are its chant notation instead (GABC body, header kept).
Syllables go one to a note of the first voice in sung order (repeats, alternate endings,
D.C./D.S. al Fine/al Coda, unreached bars last); a syllable's verse row is its pass through the
repeat plus one per anchor that went back. Readers turn every format's lyrics into this text (Guitar Pro 3–5 lines are translated, and written back the same way);
writers export the beats' syllables. The full reference (syntax, placement, drawing, the LYRICS
tab, commands, settings, limits) is [LYRICS.md](LYRICS.md).

## 5. File formats (`cecilia-formats`)

| format | read | write | status / notes |
|---|---|---|---|
| GP3 / GP4 / GP5 (binary) | ✔ | GP5 | done (M1/M2); all versions incl. 4.06 and 5.10. Writing produces GP5 v5.00 |
| GPX (GP6) | ✔ | ✔ | done (M2); BCFZ/BCFS container with GPIF XML. Writer uses literal-only BCFZ blocks (larger files) |
| GP (GP7/8) | ✔ | ✔ | done (M2); zip with `Content/score.gpif`. Backing track, frame padding and sync points are read and written (M5); an attached external recording is embedded on save |
| MusicXML (partwise, .musicxml/.mxl) | ✔ | ✔ | done (M2); tuning and string/fret kept for tab; D.S./coda directions and chord diagrams not exported yet |
| Standard MIDI File | ✔ | ✔ | done (M2); export expands repeats; import quantises to 32nd/triplet grid |
| GABC (Gregorio) | ✔ | ✔ | done (M6); glyph rules follow Gregorio's (`gabc-glyphs-determination.c`); reading and writing are covered by the format tests. NABC (adiastematic) sections are kept verbatim |
| ASCII tab | — | — | **Planned**: there is no `.txt` writer (the TUI draws text tab on screen only) |
| `.ccc` / `.ccm` (native; legacy `.cc` read only) | ✔ | ✔ | done (M2); see 5.1 |

Format detection by magic bytes first, extension second.
Encoding for GP3–5 strings: Windows-1252, or UTF-8 when the bytes are valid UTF-8.

Multi-staff parts (a piano grand staff in GPIF or MusicXML) are one model track per staff, the
further staves flagged `extra_staff`; they are engraved together (brace, joined barlines) and
written back as one multi-staff instrument to GP7, GPX and MusicXML (GP5, which has no staves,
gets one track per staff). Lyrics: GP3–5 lyric lines are put on the beats with Guitar Pro's
rules and rebuilt from the beats on export when
they were edited. All lyrics read then become the tracks' lyrics text (4.3, [LYRICS.md](LYRICS.md#files)).

**Validation.** Reader and writer tests (`cargo test --workspace`) include a local corpus
(`test-corpus/`, gitignored: 163 Guitar Pro files and 47 GABC files on the author's machine) when it
is present, and committed fixtures. Round trips through cecilia's own readers (GP3/4/5 to GP5, files
to GP7, GPX and `.ccc` / `.ccm`) are tested within each format's limits. Agreement with external
tools (alphaTab, PyGuitarPro, MuseScore, a MusicXML XSD, `mido`) has been reported by the author but is not
reproduced by anything in this repository.

### 5.1 Native `.ccc` / `.ccm` format

One serialization, two extensions: `.ccc` (cecilia chant) is what a chant score saves as by default,
`.ccm` (cecilia modern) any other score. Both read and write the same way, and the extension typed
is honored. Legacy `.cc` files (the earlier single extension) are still read, never written.

Zip archive (`crates/cecilia-formats/src/native.rs`, `FORMAT_VERSION` 2):
- `manifest.toml`: `format = "cecilia"`, `format_version` (a newer file than the app supports is
  refused) and `app_version`.
- `score.json`: versioned serde serialization of `Score` (migrations by version), including
  the audio sync points.
- `audio/<file>`: the embedded audio, when the score has some embedded (otherwise the score keeps
  a path to the audio file in `score.json`).

**Planned**, not written today: a persisted undo tree (`undo.bin`; undo history is lost on close)
and any separate `sync.toml` (sync points live in `score.json`). Not everything in a score is
guaranteed to survive a save as `.ccc` / `.ccm` yet (see `TODO.md`).

`:w file.ext` is save-as (`:save-as`): it writes the buffer to `file.ext` in the format of its
extension (any writable score format, see section 5) and the buffer then takes the new file
(`app.rs`, `save_as`). Engravings (PDF, SVG, PNG) and audio (WAV, FLAC) are written by
`cecilia convert` only. A plain `:w` writes back to the buffer's own file only when the buffer has been
modified, so an untouched original (which may contain data cecilia does not model) is never
rewritten.

## 6. Conversion (`cecilia-convert`)

- **Linked staff/tab**: editing pitch in standard view picks a string/fret; editing tab updates pitch.
- **Auto-fingering** (staff→tab): dynamic programming over candidate (string, fret) positions per
  beat, minimising cost (fret span within a chord, position shifts, string changes, high frets).
  The weights, maximum span and open-string preference are fixed in the code; a `[fingering]`
  config section is **Planned**.
- **Retune / re-instrument**: change tuning, string count or instrument (guitar→bass, drop D, capo);
  notes are kept by pitch and re-fingered; out-of-range notes are octave-shifted and flagged.
- **Chant ↔ modern**: neumes → notes using a rhythm interpretation (`equalist` default, `solesmes`
  for episema/mora lengthening); modern → chant groups notes per syllable using neume heuristics,
  always flagged as lossy.

## 7. Playback (`cecilia-audio`, `cecilia-app`)

- MIDI playback through `rustysynth` with a SoundFont (`.sf2`) set as `audio.soundfont`; without one a
  basic built-in plucked-string synth plays. **Planned**: a bundled General MIDI SoundFont (none is
  bundled today).
- Effects rendered: bends/whammy (pitch bend), slides, vibrato, palm mute, dead notes, harmonics,
  let ring, tremolo picking, grace notes, strokes, swing/triplet feel, repeats and directions.
- Audio track: load mp3/flac/ogg/wav/m4a per score; play alone or mixed with MIDI.
- **Sync map**: list of (score position ↔ audio time) points. Positions between points are linearly
  interpolated in ticks, which handles tempo drift. Users add points by tapping during playback or by
  placing them on a waveform pane. A beat-detection assist proposes points to accept or adjust.
- **Audio sync of a chant score** is kept on notes, not bars: a chant's bars are a cache that edits
  renumber and unmeasured chant has no useful bars. `ChantNote.id` is a stable id (assigned when a
  chant is read or made, kept by every edit, never written to GABC) and `AudioSync.chant_points`
  are `(note id, frame)` pairs; a point is dropped only when its note is deleted. Between two
  points the time is shared in proportion to the note lengths of `chant::transcribe` (mora,
  Solesmes lengthening); outside them the nearest stretch's rate, and the score tempo with no
  point. After every change `Score::refresh_chant_sync` derives ordinary bar `SyncPoint`s (one at
  each bar's first note) so the player, seeking, waveform, progress bar and loops are unchanged;
  those are not saved for chant scores (`.ccc` writes the note points). Saving as `.gabc` warns that
  audio and sync are not stored. Tapping (`:sync`, `:sync-tap`), `:sync-undo`, `:sync-here` and
  `:sync-nudge` are the ways to set points; the tapped time snaps to the nearest audio onset
  within 70 ms. Automatic sync for chant (pitch tracking and alignment) is not done yet.
- Practice: speed without pitch change (`<leader>-` / `<leader>=` step ±5% within 25–200%; `:speed N`
  and `audio.speed` accept 10–400%), pitch shift ±12 semitones (`:pitch`, fractions allowed), A–B loop
  (from a Visual-mode selection), count-in, metronome, speed trainer (increase speed each loop).
- Mixer: per-track volume/mute/solo, audio-track volume and metronome volume, as commands and the
  tracks pane (per-track pan and a master volume are not exposed yet). The audio thread is fed
  through `std::sync::mpsc` channels; it is not audited to be allocation- or lock-free.
- Export: WAV/FLAC rendering of MIDI (and optionally the mixed audio).

## 8. User interface

### 8.1 Layout model (neovim-like)

- **Buffer**: one open score.
- **Pane** (window): a view of a buffer. Pane kinds (`PaneKind`): a score pane, and the tool
  panes `tracks`, `waveform` (audio), `piano`, `fretboard`, `tuner`, `properties` and `palette`. A
  score pane shows its track's staves (standard, tab, slash or chant); each pane has its own
  cursor. **Planned**: linking the cursors of panes on the same buffer.
- **View**: an arrangement of panes (split tree) with its own set of loaded buffers, shown as
  bufferline tabs (bufferline.nvim style: file-type icon, middle-truncated name, hover close
  button, modified dot) on the left of the tabline; the views are listed on the right. A file
  open in several views is one shared buffer. Each pane remembers the buffers it showed, so
  closing a tab falls back to the pane's previous buffer.
- **Workspace**: saved session (views with their panes and bufferlines, cursor positions) as
  TOML. Done: `:workspace-open[!]` (saves the current one first, creates a missing one),
  `:workspace-save[!]` (saving under a new name duplicates), `:workspace-delete[!]`, `--workspace`;
  there may be no workspace; one is saved on quit (`ui.workspace_autosave_on_quit`, never by
  `:quit!`) and a plain `cecilia` reopens the last (`ui.workspace_restore_last`) or creates `default`.

### 8.2 Modes

| mode | purpose |
|---|---|
| Normal | navigation, operators, pane management, playback control |
| Insert | entering/replacing notes (frets, pitches, durations, techniques) |
| Visual | beat/bar/track range selection (charwise = beats, linewise = bars, block = beats × tracks) |
| Command | `:` ex-style commands with completion and history |
| Sync | tap sync points while audio plays |

Vim grammar: `[count] operator motion`, `.` repeat, registers, macros (`q`/`@`), marks, branching
undo tree (`u`, `U`, `g-`/`g+`, `:undotree`), leader key with a which-key popup that lists
available continuations. The default keymap is in [`keymap.md`](keymap.md).
The which-key popup also shows what is pending: a count, an operator and the motions that can
follow it, or a register / macro prompt.

Messages and errors are notifications: cards with an icon (colored by severity), a bold subject and the message below, stacked against a corner or side of the score panes
(or the window; `notifications.anchor`, `notifications.area`), newest nearest the edge. Repeats
and look-alikes (equal but for numbers) merge into one card with a `×N` badge; each card goes
after `notifications.info_timeout` / `error_timeout` seconds (0: stays), a thin bar along its
bottom showing the time left; a click dismisses it. Questions (save, reload) are cards that stay
until answered. Every notification is kept in a history (`notifications.history_size`, stored in
the state directory) browsed with `:notifications` (repeats stacked as on screen, message left, when it appeared right as "5m ago" or a date and time: `notifications.history_time`; Enter opens the notification in a read-only text popup in the middle of the window: `hjkl`, `v`/`V` and `y` as in the lyrics text area but never editing, the mouse selects, `<Esc>` or the "x" in the corner closes it). `:notify` raises one.

### 8.3 Mouse

Goal: everything reachable by keyboard is also reachable by mouse: click to place the cursor, drag to
select, drag pane borders, click tabs, scroll/zoom, context menus that invoke the same commands.

### 8.4 Frontends

- **GUI** (`cecilia-gui`): the complete interface. Score views support three layouts: page, continuous
  horizontal, continuous vertical. The page layout has a design mode (`:design`): bar widths,
  bars per line and beat positions can be adjusted by hand (keys, commands, dragging handles and
  clicking line-end "+" / "-" buttons with the mouse) and are stored in the score (`Score.design`, native `.ccc` / `.ccm` format only). Notation uses SMuFL fonts (Bravura, SIL OFL), including the
  plainchant ranges; square notation is set with the Caeciliae chant font (Oblates of St. Joseph,
  SIL OFL 1.1, bundled unmodified; see `crates/cecilia-engrave/fonts/OFL-Caeciliae.txt`).
- **TUI tab bar lines** (`cecilia-tui`, `barline.rs`): every bar line is one or more columns that
  run through all string rows, with joints where the line crosses a staff line (`─`); plain
  Unicode box drawing, no Nerd Font. Columns (top / middle / bottom row): thin `┬┼┴`, heavy
  `┰╂┸`, double `╥╫╨` (`│ ┃ ║` on a one-string staff), dashed `┆`, repeat dots `•` on rows 1 and
  n-2 (n = strings). The strings stop flush at the edges of a system: the outermost column
  there has the half joints `┌├└` / `┎┠┖` / `╓╟╙` at the left edge and `┐┤┘` / `┒┨┚` / `╖╢╜` at
  the right edge, and columns of a line with no string between them are plain verticals.

  | line | glyph columns | width |
  |------|---------------|-------|
  | normal / system start | thin | 1 |
  | double bar | double | 1 |
  | closes a free-time bar | dashed | 1 |
  | final bar | thin heavy | 2 |
  | repeat end | dots thin heavy | 3 |
  | repeat start | heavy thin dots | 3 |
  | repeat end + start | dots thin heavy thin dots | 5 |

  Special lines outrank the dashed one. A wider line takes its columns from the start of the
  bar (or after the last bar of a system), and the beats are placed after it.
- **TUI** (`cecilia-tui`): chant tracks show as a four-line text staff with clefs, neume glyphs and ligatures, divisions,
  custodes and the syllables underneath, in the horizontal, vertical and page layouts; read,
  play and basic editing of tablature (frets, durations, ties, rests,
  basic techniques) rendered as text tab. Same keymaps, config and commands as the GUI; commands not
  available in the TUI report an error instead of failing silently.
- **Mobile** (later): the same GUI crate with a touch interaction layer (tap to place cursor, gestures,
  on-screen fret/duration palette), support for hardware keyboards, and platform file pickers.

### 8.5 Output

PDF, SVG, PNG (from the engraving display list) and WAV/FLAC, written by `cecilia convert`. **Planned**:
ASCII tab (`.txt`) output, and PDF/SVG/PNG/WAV/FLAC export from inside the editor.

## 9. Configuration

File: `$XDG_CONFIG_HOME/cecilia/config.toml` on Linux (via `directories` on other platforms;
override with `--config`). Reloaded automatically when it changes, and on `:source`. Every option
is a dotted name (`ui.tab_width`) that can also be set with `-o NAME=VALUE` or `:set`. The full list,
with types and defaults, is in [OPTIONS.md](OPTIONS.md), and `config.example.toml` is a commented
sample.

```toml
[general]
leader = "<Space>"

[ui]
layout = "vertical"          # page, vertical or horizontal
tab_width = 2                # TUI: characters per sixteenth

[theme]
preset = "dark"

[audio]
soundfont = "~/sf/GeneralUser.sf2"
speed = 1.0

[keymap.normal]
"<C-s>" = "write"
"gp" = ["goto-bar 1", "play"] # sequences of commands

[keymap.insert]
"<C-b>" = "toggle bend"
```

Keymap values are command names (with arguments) or lists of commands. Unknown commands or keys are
reported at load time without aborting (the previous valid config stays active).

## 10. CLI

```
cecilia [FILE...]                 open in the TUI
cecilia --gui [FILE...]           open in the GUI (cecilia.desktop launches this)
cecilia play FILE [--speed X]     headless playback
cecilia convert IN OUT            convert by output extension (gp5, gp, gpx, musicxml, mxl, mid, gabc,
                                  ccc, ccm, pdf, svg, png, wav, flac);
                                  --track N, --layout page|vertical|horizontal, --scale S
cecilia info FILE [--json]        tracks, tunings, tempo, bar count, metadata; --json dumps the model
```

Global options: `-W/--workspace NAME`, `--config FILE`, `-o/--option NAME=VALUE ...`.

## 11. Internationalisation

English only. **Planned**: translations, with user-facing strings moved to Fluent (`.ftl`)
resources. Strings are plain Rust strings today.

## 12. Out of scope for v1

Lua/plugin API, external MIDI output, MIDI input (step entry / recording), adiastematic neumes
(St. Gall/Laon, nabc), translations.

## 13. Milestones

1. **M1 MVP** (done): workspace, `cecilia-core` (fretted subset), GP3/4/5 reader, `cecilia-app` (modes, keymap,
   config, pane tree), TUI tab reader with vim navigation, MIDI playback with moving cursor, `cecilia info`.
2. **M2 Formats** (done): GP7/8 + GPX read, GP5/GP7 write, MIDI + MusicXML I/O, `.cc` (now `.ccc` / `.ccm`), `cecilia convert`,
   round-trip tests.
3. **M3 GUI viewer** (done): `cecilia-engrave` (tab + standard staff, 3 layouts), `cecilia-gui` panes/tabs/
   workspaces, PDF/SVG/PNG export.
4. **M4 Editing** (done; undo history is not yet saved in `.ccc` / `.ccm`): Insert/Visual/operators, undo tree, macros, the technique set (4.1), linked staff↔tab,
   auto-fingering, retune; TUI basic editing.
5. **M5 Playalong** (done; the mixer is commands plus the status line rather than a dedicated pane,
   and per-track pan/master volume are not exposed yet): audio loading, waveform/sync pane, manual
   sync + beat assist, stretch/pitch, A–B loop, count-in, mixer, audio export.
6. **M6 Chant** (done): chant model, GABC I/O, square-notation engraving, chant↔modern transcription.
7. **M7 Instruments** (done): drums/percussion (hands/feet voices, drum tab lanes, letter
   entry), piano grand staff (multi-staff parts), lyrics (verses, GP3–5 attachment, `:lyrics`,
   centred hyphens, terminal display; since then lyrics text, see 4.3).
8. **M8 Platforms**: Windows/macOS packaging, then Android, then iOS.

## 14. Quality

- `cargo test --workspace`; parser tests against a local corpus in `test-corpus/` (gitignored,
  may contain copyrighted files) plus small committed fixtures authored for the project.
- Round-trip goal: `read(write(score)) == score` for every writable format, within that
  format's capabilities.
- `cargo clippy --workspace --all-targets` and `cargo fmt` are kept clean by hand. **Planned**: CI
  (GitHub Actions; see `TODO.md`).
- Goal: readers never panic on malformed input; they return errors with a location (track/bar/beat).
