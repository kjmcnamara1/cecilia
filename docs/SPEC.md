# cecilia — Specification

Status: draft v0.1 (2026-10-23). Decisions recorded from the initial requirements session.

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
- Bundled assets must be permissively licensed (fonts: SIL OFL; SoundFont: MIT or equivalent).
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
| `cecilia-audio` | Audio output, decoding, MIDI synthesis, time-stretch/pitch-shift, mixer. |
| `cecilia-sync` | Audio↔score sync map and beat-detection assist. |
| `cecilia-app` | Headless app state: buffers, panes/tabs/workspaces, modes, commands, keymaps, undo, config, i18n. |
| `cecilia-tui` | ratatui frontend. |
| `cecilia-gui` | winit + wgpu + vello + parley frontend (desktop, later mobile). |
| `cecilia` | Binary: CLI entry point, launches GUI/TUI/headless commands. |

Dependency direction: `core ← formats/convert/engrave/sync ← audio ← app ← tui/gui ← cecilia`.
`cecilia-app` never depends on a frontend; both frontends drive the same command registry, keymaps and config.

### 3.1 Key third-party choices

| need | crate |
|---|---|
| audio output | `cpal` (ALSA/PipeWire/JACK, CoreAudio, WASAPI, AAudio) |
| audio decode | `symphonia` (mp3, flac, ogg/vorbis, wav, aac/m4a) |
| MIDI synth | `rustysynth` (SoundFont 2) |
| time-stretch / pitch-shift | `signalsmith-stretch` (MIT) |
| TUI | `ratatui` + `crossterm` |
| GUI | `winit`, `wgpu`, `vello`, `parley` |
| config | `toml`, `serde`, `directories` |
| CLI | `clap` |
| i18n | `fluent` |
| zip containers | `zip` |
| XML (GP7/GPX/MusicXML) | `quick-xml` |

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
│  ├─ staves: which views are shown (tab, standard, slash, chant)
│  ├─ lyrics_text: the track's lyrics as gabc+ text (see 4.3), source of the beats' syllables
│  └─ bars[] (one per master bar; repeat sign `simile`) → voices[] (≥4) → beats[]
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
Anything not yet editable must still round-trip through `extras`.

The editing palette (`cecilia-app`, `palette.rs`) exposes these as tools: durations, accidentals
(down to double sharp / flat), dynamics and hairpins, articulations, effects, the six slides,
strokes (brush, arpeggio, rasgueado, pick strokes), beam and stem overrides, and the bar tools
(clef, key and time signature, free time, double bar, bar repeat signs, repeats, alternate
endings, directions, sections). Each runs an ordinary command and is one undo step.

Free-time bars close with a dashed barline (also between two free-time bars); special lines (repeat
end, double bar, final barline) stay solid.

### 4.2 Chant model

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
transcription is what plays, exports to modern formats and carries the cursor; edits made to it
(pitch changes, deletions, insertions) are reconciled back into the chant, adding flats or
naturals where needed.

**Chant staff on ordinary tracks.** Any track can show a chant staff (`:notation chant`, on a C or F clef: `:clef c1`–`c4` / `f1`–`f4`). It is a
view: the track's notes are never changed and playback (which plays every note of the track,
chords included) does not depend on which staves are shown. The view is built in `cecilia-core`
(`melody::ChantView`) and is exactly what `modern-to-chant` converts, so both agree:

- Only the first voice is read. Chant is monophonic, so a chord shows one note, chosen by
  `ui.chant_chords`: `top` (default, the highest note: a "skyline"), `bottom`, or `error`. Under
  `error`, or whenever a melody cannot be chosen, no chant staff is drawn: a message
  ("Chant staff: chord at bar 12 beat 3 — chant is monophonic") takes its place and appears in the
  status line, and `:modern-to-chant` refuses.
- Syllables start at each lyric (at each quarter note when there are none); notes in a syllable
  form neumes by contour (`cecilia_core::chant::group_neumes`, shared with the GABC engraver), and a
  note marked `neume_start` (`/`, `:chant-break`) always starts a new one. A note entered with `<CR>` (`:chant-syllable`: a punctum at the cursor position and insert duration, in a new beat after the cursor, the bar filling like other staves) is marked `punctum`: its own neume while the track has no lyrics, and ignored for grouping once any lyric exists, so syllables condense their notes by contour. Tied beats extend the note before them.
- The melody is moved by the same shift to the white-key scale (B♭ allowed) and clef as
  `modern-to-chant`; a note longer than the typical one gets a mora.
- Breath and phrase marks become divisions: a rest shorter than a quarter note is a divisio
  minima (`,`), a quarter or longer a divisio maior (`:`), a double bar a divisio maior (`:`), and
  the end of the track a finalis (`::`). A division set on a beat (`:chant-bar`; `,` `;` `:` `::`)
  overrides the rest or bar rule. The model has no separate breath or fermata marks.

### 4.3 Lyrics (gabc+)

A track's lyrics are one text, `Track.lyrics_text`, in *gabc+*: GABC (`text(notes)`) plus
dashes, skips, italics, `[labels]`, `*` and `<bar:beat>` anchors (the syntax table is in
[keymap.md](keymap.md#gabc)). It is the single source of the syllables on the beats
(`Beat.lyrics`, one string per verse row, a trailing `-` for a hyphen), which
`cecilia_core::gabcplus` lays out from it after every edit, so the lyrics follow the notes.
A chant track's lyrics are its chant notation instead (GABC body, header kept).

- **Order.** Syllables go one to a singing beat (first voice, any beat with a note, tied notes
  included, as Guitar Pro does) in sung order: repeats (each closing bar counting its passes),
  alternate endings by pass, then one D.C./D.S. jump (repeats are not taken again after it and
  only the last ending is played), al Fine stopping after the Fine bar, al Coda jumping from
  To Coda to the Coda. Bars the music never reaches (endings without their repeat) are sung
  last, so no lyrics are lost. Lyrics start at the first singing beat unless an anchor says
  otherwise.
- **Verse rows.** A syllable's row is the pass through its repeat (or how often its bar was
  sung before, whichever is more), plus one for every anchor that went back in the music. So
  verse 2 of a repeated section shows under verse 1, and lyric lines anchored at the same bar
  stack.
- **Anchors** `<bar:beat>` (1-based; the beat is the index in the first voice, as in the status
  line) go to the first singing beat at or after that place, the first time the bar is sung.
- **Interpretations.** A word of only `_` is a skip; `_` at the start of a syllable (start of a
  word or after `-`) makes it italic; `_` between letters separates words. A word of only `-`
  covers that many beats under one dash; dashes inside or at the edge of a word join two
  syllables without taking a beat. `*` never takes a beat and is drawn after the syllable
  before it (before the next one at the start). `[label]` goes before the next syllable. Text
  right after a `(…)` group is the next syllable of the same word (GABC). `+` joins words on
  one syllable (Guitar Pro). A `(…)` group without text takes no beat. An unclosed `(`, `[` or
  `<` is text.
- **GABC groups** are kept in the text and shown with GABC on; they are not yet used to draw
  the chant staff of an ordinary track (it still shows the track's notes).
- **Import/export.** Readers turn lyrics into gabc+: Guitar Pro 3–5 lyric lines laid out as
  Guitar Pro does (`[comments]` become labels, line breaks kept), per-beat lyrics (GP6–8,
  MusicXML, older `.cc`) one line per verse row; anchors and `_`/`-` gaps are written where
  needed so every syllable stays on its beat (checked on the corpus; a verse row can change
  where a repeat makes the original one impossible to say). A beat holding only `-` becomes
  part of a dash. Writers export the beats' syllables as before; labels and italics have no
  place in Guitar Pro or MusicXML.

## 5. File formats (`cecilia-formats`)

| format | read | write | status / notes |
|---|---|---|---|
| GP3 / GP4 / GP5 (binary) | ✔ | GP5 | done (M1/M2); all versions incl. 4.06 and 5.10. Writing produces GP5 v5.00 |
| GPX (GP6) | ✔ | ✔ | done (M2); BCFZ/BCFS container with GPIF XML. Writer uses literal-only BCFZ blocks (larger files) |
| GP (GP7/8) | ✔ | ✔ | done (M2); zip with `Content/score.gpif`. Backing track, frame padding and sync points are read and written (M5); an attached external recording is embedded on save |
| MusicXML (partwise, .musicxml/.mxl) | ✔ | ✔ | done (M2); tuning and string/fret kept for tab; D.S./coda directions and chord diagrams not exported yet |
| Standard MIDI File | ✔ | ✔ | done (M2); export expands repeats; import quantises to 32nd/triplet grid |
| GABC (Gregorio) | ✔ | ✔ | done (M6); checked note-for-note against Gregorio 6.2 on its 480-file test suite and on GregoBase chants; written files re-read identically. NABC (adiastematic) sections are kept verbatim |
| ASCII tab | — | ✔ | shared with TUI rendering |
| `.cc` (native) | ✔ | ✔ | done (M2); see 5.1 |

Format detection by magic bytes first, extension second.
Encoding for GP3–5 strings: Windows-1252, or UTF-8 when the bytes are valid UTF-8.

Multi-staff parts (a piano grand staff in GPIF or MusicXML) are one model track per staff, the
further staves flagged `extra_staff`; they are engraved together (brace, joined barlines) and
written back as one multi-staff instrument to GP7, GPX and MusicXML (GP5, which has no staves,
gets one track per staff). Lyrics: GP3–5 lyric lines are put on the beats with Guitar Pro's
rules (checked against alphaTab: identical on 8 of 10 corpus files; the other two differ only at
grace notes, which alphaTab treats as separate beats) and rebuilt from the beats on export when
they were edited. All lyrics read then become the tracks' gabc+ text (4.3).

**Validation.** Every reader and writer is checked against a local corpus (`test-corpus/`,
163 Guitar Pro files of all versions):
- GP3/4/5 → GP5, and every file → GP7, GPX and `.cc`, round-trip through cecilia's own readers
  without loss (beyond each format's documented limits);
- PyGuitarPro and alphaTab read the exported GP5/GP7/GPX files with the same note counts as the
  originals, and cecilia's readers agree with alphaTab's note counts on every original;
- MIDI export/import keeps 99.99% of sounding notes; `mido` reads every exported file;
- MusicXML export validates against the MusicXML 4.0 XSD and MuseScore 4 opens 157 of 163
  files (the rest contain incomplete tuplet groups, which MuseScore rejects).

### 5.1 Native `.cc` format

Zip archive:
- `manifest.toml`: format version, app version, created/modified.
- `score.json`: versioned serde serialization of `Score` (migrations by version), including
  the audio sync points.
- `undo.bin`: persisted undo tree (optional).
- `audio/<file>`: embedded audio (optional; otherwise a relative/absolute path in `sync.toml`).

`:w file.ext` exports to any writable format (by extension); `:w file.cc` saves everything.
A plain `:w` writes back to the buffer's own file only when the buffer has been modified, so an
untouched original (which may contain data cecilia does not model) is never rewritten.

## 6. Conversion (`cecilia-convert`)

- **Linked staff/tab**: editing pitch in standard view picks a string/fret; editing tab updates pitch.
- **Auto-fingering** (staff→tab): dynamic programming over candidate (string, fret) positions per
  beat, minimising cost (fret span within a chord, position shifts, string changes, high frets).
  Weights, max span and open-string preference come from config (`[fingering]`).
- **Retune / re-instrument**: change tuning, string count or instrument (guitar→bass, drop D, capo);
  notes are kept by pitch and re-fingered; out-of-range notes are octave-shifted and flagged.
- **Chant ↔ modern**: neumes → notes using a rhythm interpretation (`equalist` default, `solesmes`
  for episema/mora lengthening); modern → chant groups notes per syllable using neume heuristics,
  always flagged as lossy.

## 7. Playback (`cecilia-audio`, `cecilia-sync`)

- MIDI playback through `rustysynth` with a bundled permissive GM SoundFont; user `.sf2/.sf3` in config.
- Effects rendered: bends/whammy (pitch bend), slides, vibrato, palm mute, dead notes, harmonics,
  let ring, tremolo picking, grace notes, strokes, swing/triplet feel, repeats and directions.
- Audio track: load mp3/flac/ogg/wav/m4a per score; play alone or mixed with MIDI.
- **Sync map**: list of (score position ↔ audio time) points. Positions between points are linearly
  interpolated in ticks, which handles tempo drift. Users add points by tapping during playback or by
  placing them on a waveform pane. A beat-detection assist proposes points to accept or adjust.
- Practice: speed 25–200% without pitch change, pitch shift ±12 semitones (with cents), A–B loop
  (from a Visual-mode selection), count-in, metronome, speed trainer (increase speed each loop).
- Mixer: per-track volume/pan/mute/solo, audio-track volume, metronome volume, master volume.
- Audio thread is real-time safe: no allocation or locks; communication via lock-free queues/atomics.
- Export: WAV/FLAC rendering of MIDI (and optionally the mixed audio).

## 8. User interface

### 8.1 Layout model (neovim-like)

- **Buffer**: one open score.
- **Pane** (window): a view of a buffer. Pane kinds: `tab`, `staff`, `staff+tab`, `chant`,
  `waveform` (sync editor), `mixer`, `tracks`, `files`, `help`. Panes viewing the same buffer can be
  cursor-linked or independent (`:set linkcursor`).
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

### 8.3 Mouse

Everything reachable by keyboard is also reachable by mouse: click to place the cursor, drag to
select, drag pane borders, click tabs, scroll/zoom, context menus that invoke the same commands.

### 8.4 Frontends

- **GUI** (`cecilia-gui`): full functionality. Score views support three layouts: page, continuous
  horizontal, continuous vertical. The page layout has a design mode (`:design`): bar widths,
  bars per line and beat positions can be adjusted by hand (keys, commands, dragging handles and
  clicking line-end "+" / "-" buttons with the mouse) and are stored in the score (`Score.design`, native `.cc` format only). Notation uses SMuFL fonts (Bravura, SIL OFL), including the
  plainchant ranges; square notation is set with the Caeciliae chant font (Oblates of St. Joseph,
  SIL OFL 1.1, bundled unmodified; see `assets/fonts/OFL-Caeciliae.txt`).
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
  custodes and the syllables underneath, in the horizontal, vertical and page layouts (also for
  ordinary tracks with a chant staff, `:notation chant`: all-chant tracks like that, a chant staff
  among other staves drawn in the tab's systems); read,
  play and basic editing of tablature (frets, durations, ties, rests,
  basic techniques) rendered as text tab. Same keymaps, config and commands as the GUI; commands not
  available in the TUI report an error instead of failing silently.
- **Mobile** (later): the same GUI crate with a touch interaction layer (tap to place cursor, gestures,
  on-screen fret/duration palette), support for hardware keyboards, and platform file pickers.

### 8.5 Output

PDF, SVG, PNG (from the engraving display list), ASCII tab, WAV/FLAC.

## 9. Configuration

File: `$XDG_CONFIG_HOME/cecilia/config.toml` on Linux (via `directories` on other platforms;
override with `--config`). Reloaded automatically when it changes, and on `:source`.

```toml
[general]
leader = "<Space>"
encoding = "cp1252"          # legacy GP string encoding

[ui]
theme = "dark"
default_layout = "continuous-vertical"
tab_width = 3                # TUI: characters per sixteenth

[audio]
device = "default"
buffer_size = 512
soundfont = "~/sf/GeneralUser.sf2"
count_in = true

[fingering]
max_span = 4
prefer_open_strings = true

[keymap.normal]
"<C-s>" = "write"
"<leader>p" = "play-toggle"
"gp" = ["goto-bar 1", "play"] # sequences of commands

[keymap.insert]
"<C-b>" = "toggle-technique bend"
```

Keymap values are command names (with arguments) or lists of commands. Unknown commands or keys are
reported at load time without aborting (the previous valid config stays active).

## 10. CLI

```
cecilia [FILE...]                 open in the TUI
cecilia --gui [FILE...]           open in the GUI (cecilia.desktop launches this)
cecilia play FILE [--audio A]     headless playback
cecilia convert IN OUT            convert by output extension (gp5, gp, gpx, musicxml, mid, gabc, txt,
                               pdf, svg, png, wav, flac, cecilia); --track, --tuning options
cecilia info FILE [--json]        tracks, tunings, tempo, bar count, metadata; --json dumps the model
```

## 11. Internationalisation

English only in v1, but every user-facing string goes through Fluent (`.ftl` resources).

## 12. Out of scope for v1

Lua/plugin API, external MIDI output, MIDI input (step entry / recording), adiastematic neumes
(St. Gall/Laon, nabc), translations.

## 13. Milestones

1. **M1 MVP** (done): workspace, `cecilia-core` (fretted subset), GP3/4/5 reader, `cecilia-app` (modes, keymap,
   config, pane tree), TUI tab reader with vim navigation, MIDI playback with moving cursor, `cecilia info`.
2. **M2 Formats** (done): GP7/8 + GPX read, GP5/GP7 write, MIDI + MusicXML I/O, `.cc`, `cecilia convert`,
   round-trip tests.
3. **M3 GUI viewer** (done): `cecilia-engrave` (tab + standard staff, 3 layouts), `cecilia-gui` panes/tabs/
   workspaces, PDF/SVG/PNG export.
4. **M4 Editing** (done; undo history is not yet saved in `.cc`): Insert/Visual/operators, undo tree, macros, full technique set, linked staff↔tab,
   auto-fingering, retune; TUI basic editing.
5. **M5 Playalong** (done; the mixer is commands plus the status line rather than a dedicated pane,
   and per-track pan/master volume are not exposed yet): audio loading, waveform/sync pane, manual
   sync + beat assist, stretch/pitch, A–B loop, count-in, mixer, audio export.
6. **M6 Chant** (done): chant model, GABC I/O, square-notation engraving, chant↔modern transcription.
7. **M7 Instruments** (done): drums/percussion (hands/feet voices, drum tab lanes, letter
   entry), piano grand staff (multi-staff parts), lyrics (verses, GP3–5 attachment, `:lyrics`,
   centred hyphens, terminal display; since then gabc+ text, see 4.3).
8. **M8 Platforms**: Windows/macOS packaging, then Android, then iOS.

## 14. Quality

- `cargo test --workspace`; parser tests against a local corpus in `test-corpus/` (gitignored,
  may contain copyrighted files) plus small committed fixtures authored for the project.
- Round-trip property: `read(write(score)) == score` for every writable format, within that
  format's capabilities.
- `cargo clippy -- -D warnings` and `cargo fmt --check` in CI.
- Readers never panic on malformed input; they return errors with a location (track/bar/beat).
