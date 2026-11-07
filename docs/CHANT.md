# Chant ⇄ modern notation

How cecilia turns a modern melody (staff or tab) into chant (`:modern-to-chant`), and a chant back
into modern notes. Source: `cecilia-core/src/chant.rs` (the chant model, clefs, transcription),
`cecilia-core/src/melody.rs` (`ChantView`, the modern → chant view), `cecilia-convert/src/chant.rs`
(`modern_to_chant`), `cecilia-core/src/chant_edit.rs` (the edit operations),
`cecilia-app/src/chant.rs` (editing keys and commands), and
`cecilia-engrave` (drawing).

## 1. Chant tracks only

Square notation belongs to **chant tracks** (`Track.chant` set): from a `.gabc` file or made with
`:modern-to-chant`. Their source of truth is the chant itself (syllables, neumes, clefs in GABC
form); the track's bars are a *transcription* of it (§4), rewritten after each edit.

### Chant and modern scores

The interface shows and follows the kind: a `MODERN` / `CHANT` chip in the status line, the chant
icon on a chant score's tab, `— chant` in the pane title, a palette of clefs, divisions, heads and
signs, header / interpretation / staff-line rows in the properties pane, a `[keymap.normal-chant]`
layer, and only the commands of that kind in completion and help (see `docs/keymap.md`, "Chant").
A chant is always drawn in square notation; `:chant-view` was removed.

A score is one of two kinds, and a score is never both:

| | modern score | chant score |
|---|---|---|
| tracks | any number of staff tracks | exactly one, with `chant` set |
| source of truth | the bars | the `Chant` (bars are a cache) |
| files | all formats | `.gabc` (and `.cc`) |

`Score::is_chant()` is the test. `:track-add` refuses on a chant score and refuses a chant track
on a modern one; a chant file cannot hold other tracks. The conversions open a new buffer and leave
the one you started from alone:

- `:chant-to-modern` → `[modern] name`: the cached bars as a pitched track, the syllables as
  lyrics text, title and audio sync kept. Also what `cecilia convert in.gabc out.gp5` writes.
- `:modern-to-chant` → `[chant] name`: the cursor track only (other tracks are left out), lossy as
  §3 describes.

To add an organ to a chant: `:chant-to-modern`, clean up the bars, `:track-add organ`. A `.cc` file
from an older version with a chant track and other tracks loads as a modern score (the chant track
converted in place).

An ordinary track has no chant staff: `:notation chant` and `:staff-add chant` do not exist, and a
`.cc` file that still names one drops it on load. To see a melody in square notation, convert it
(`:modern-to-chant`, or `cecilia convert in.gp5 out.gabc`); the conversion uses `ChantView`, which
builds the chant from the modern melody as described in §3. `Note.chant` (`ChantMarks`) and
`Beat.chant_division` are still read by that conversion.

## 2. The chant pitch model

Notes are staff positions, as chant is written. Position 0 is GABC `a` (below the bottom line),
3 is `d` (bottom line), 5 `f`, 7 `h`, 9 `j` (the four lines), up to 12 `m`. A staff shows positions
0–13. Uppercase letters are the same positions drawn as a punctum inclinatum.

A clef (`ChantClef`: kind do/fa, line 1–4, optional flat) fixes what a position means:

- the clef sits on position `2·line + 1`;
- **do** is at the clef position (do clef) or four positions above it (fa clef: F G A B **C**);
- the scale is always the white keys (do re mi fa sol la ti = 0 2 4 5 7 9 11);
- **do is C5 (MIDI 72)**, so `c4` puts do on `j` and a fa clef line is F4. This keeps most chant
  inside the treble staff.

```
midi(position, alter) = 72 + 12·floor(d/7) + [0,2,4,5,7,9,11][d mod 7] + alter,   d = position − do_position
```

The only alteration chant has is B♭ (and its natural). With a flat clef (`cb3`) ti is flat
throughout.

## 3. Modern → chant (`ChantView::build`)

The chant is built in these steps. Each is deterministic; nothing is stored except what is
marked as "kept" below.

### 3.1 The melody (`melody::melody`)

Chant is monophonic, so the staff reads **only the first voice** and reduces it to a line of
*events* (one per sung note):

- **Chords.** One note is picked per beat by the chord policy (`ui.chant_chords`): `top` (the
  highest note, default), `bottom`, or `error`. Under `error` any chord makes the conversion refuse
  (message "chord at bar N beat M — chant is monophonic") and `:modern-to-chant` fails.
- **Ties.** A beat whose notes are all tied extends the previous event (the event remembers the
  beats it continues, so the cursor and highlights map to every beat).
- **Rests** close a phrase: a division after the previous note, `:` if the rest lasts at least a
  quarter, otherwise `,`. A division already on the note (other than `,`) is kept.
- **A beat's own chant division** (`Beat.chant_division`: `,` `;` `:` `::`) overrides the above.
- **A double bar** in the score gives a `:` after the last note before it.
- **The end** of the melody always gets a final division `::`.
- Each event carries the beat's first lyric syllable, the pitch (MIDI, with the track's
  transposition), a rhythmic length in ticks, the quarter-note group it falls in, and the chant
  signs kept on the note (`ChantMarks`).

### 3.2 Syllables (`melody::syllable_starts`)

A note **starts a syllable** if
- the track has lyrics: its beat has a syllable, or it is the first note;
- the track has no lyrics: it is the first note of a new quarter-note group.

Notes up to the next syllable start (and up to the next division) form one run.

### 3.3 Fitting the melody to the chant staff (`melody::fit`, `ChantClef::fitting`)

Ordinary music is not in the white-key scale and does not sit on four lines, so the melody is
moved by a **shift** (semitones) and given a **clef**:

1. **Scale shift.** For each semitone shift from −6 to +5, score the notes: a pitch class in the
   white-key scale costs 0, B♭ (pitch class 10) 0.3, anything else 1.0, plus 0.01 per semitone of
   shift. The cheapest wins (ties favour the smaller shift).
2. **Clef and octave.** Candidate clefs, in preference order: C on line 4, C3, F3, C2, F4, C1.
   Each is tried at octave offsets −24, −12, 0, +12, +24. A note costs 0 on positions 2–10, 1 on
   0–1 and 11–12, and 10 outside 0–12; plus 0.02 per semitone of octave and 0.01 per place down
   the clef list. The cheapest wins.

The total `shift + octave` is `ChantView.shift`; it is shown in the `:modern-to-chant` message
("transposed ±N semitones"). Playback and the notes themselves never use it.

### 3.4 Placing each note (`ChantClef::place`)

`place(pitch, shift)` finds the staff position of `pitch + shift`:

- an exact scale note gives `(position, 0)`;
- B♭ is a **flat on ti** (the position above, alteration −1);
- any other pitch is **rounded down** to the position below (the conversion counts these as
  "rounded").

The result is clamped to positions 0–13.

### 3.5 Signs

- Kept signs (`ChantMarks`): inclinatum, initio debilis, and the GABC modifiers in `mods`
  (`v s o q w ~ . _ '`, `ss` `sss`, …) are copied from the note to the chant note.
- **Mora, automatic.** A note at least 1.75× as long as the *median* note of the melody gets a
  mora (`.`), unless it already has one. Shorter notes get none; double moras are only explicit.
- **Flats.** In the staff view a flat is drawn before the neume of any note placed as a flat. In
  `:modern-to-chant` the sign is written once per position and word: `x` before the first flat
  at a position, a natural `y` when the natural returns at a position that was flat, and the memory
  is cleared at each new word and each division — GABC's accidental scope.

### 3.6 Neumes (`ChantView::of`, `chant::group_neumes`)

Each run (syllable, up to a division) is cut into neumes:

1. **Forced cuts.** A note marked `neume_start` (`/`, `:chant-break`) starts a new neume. A note
   entered with Enter (`punctum` flag) also starts one **while the track has no lyrics**; once
   any lyric exists, the syllables decide and the flag is ignored.
2. **Climacus.** Three or more falling plain notes (no inclinatum or other sign except moras) in a
   row become one neume drawn as a virga followed by puncta inclinata.
3. **Contour grouping** of everything else (`group_neumes`). A neume grows note by note while:
   - it began with a plain punctum or an initio debilis, or with a quilisma or quadratum (which
     take just one rising note);
   - the new note is a plain punctum (no virga, stropha, oriscus, quilisma, cavum, linea,
     inclinatum) at a **different pitch** than the last — a repeated pitch always starts a new
     neume;
   - the up/down pattern so far is one of: up (pes), down (clivis), up-down (torculus),
     down-up (porrectus), up-down-up, down-up-down — **at most four notes**;
   - three rising notes are cut after the first (a scandicus: a punctum then a pes);
   - the previous note is not liquescent.
   Anything else starts a new neume.

### 3.7 Divisions and the output

Divisions come from §3.1. `Division::of` classifies the written token: `::` finalis, `:` maior,
`;` and the Dominican `,1`–`,8` minor, `,` and `^` minima, anything else virgula (no bar
split). The staff view draws them after the last beat of an event, midway between that note and
the next beat's column.

`:modern-to-chant` builds a `Chant` from the view — a clef item, then syllables with their text
(escaped for GABC), `word_start` from the lyric's trailing `-`, flats, the notes, and the
division as its own bar item — opens it as a new buffer `[chant] name`, and reports shift, rounded
notes and reduced chords. Drum tracks and tracks without notes are refused.

### 3.8 What is lost

Always lossy: rhythm (only the mora survives), rests (they become divisions), chords (one note),
the exact pitches (shifted into the scale, chromatic notes rounded), other voices, dynamics and
tab information. The notes of the original track are never touched by the staff view.

## 4. Chant → modern (a chant track's transcription, `chant::transcribe`)

A chant track keeps an ordinary rendering of the chant in its bars so playback, the cursor, audio
sync and the modern file formats work on it.

- **Resolving pitches.** `Chant::resolved` walks the syllables: the current clef (a clef item
  changes it and clears alterations), alterations (`x` flat, `#` sharp, `y` natural) at a position
  last until the end of the **word** (`word_start`) or the next **division**; a flat clef makes ti
  flat unless altered. Each note's MIDI is `clef.midi(position, alteration)`.
- **Rhythm** (`Interpretation`):
  - *equalist* (default): every note is an eighth; a mora makes it a quarter;
  - *solesmes*: as equalist, and a note with an episema or right before a quilisma, without a mora,
    is a dotted eighth.
- **Repeats.** A stropha, distropha, tristropha or bivirga/trivirga sounds `s`, `ss`, `sss`,
  `vv`, `vvv` beats of the same pitch.
- **Bars.** One *free-time* bar per phrase: a bar ends at each division except a virgula; its time
  signature is the number of eighths over 8; a finalis `::` marks the bar a double bar.
- **Lyrics.** The first beat of each syllable carries its plain text (markup removed), with a
  trailing `-` when the next syllable continues the word.
- `chant::retranscribe` rewrites the track's bars and the score's master bars after every chant
  edit; other tracks are padded or cut to the new bar count.
- `:chant-to-modern` drops the chant and keeps this transcription as an ordinary pitched track.

### Edits (`chant_edit.rs`)

The bars of a chant track are a cache and are never edited: there is no way back from the
transcription to the chant. Edits are operations on the `Chant`, each returning
`Result<_, String>` and leaving the chant unchanged on `Err`; the app runs one inside a single
`edit(...)` (one undo step on the whole score) and calls `retranscribe` afterwards.

- **Note index.** Notes are addressed by their index in *reading order*: syllable by syllable, item
  by item, the order of `Chant::resolved` and of the beats `transcribe` makes
  (`Transcription.positions[n]` is the first beat of note `n`, `lengths[n]` how many beats it
  sounds: 1, or 2–3 for a stropha or bivirga). `Transcription::note_at(bar, beat)` and
  `place_of(note)` convert between a cursor and a note; `note_near` takes the last note before a
  beat that sounds none. `Chant::note_place`, `note_index` and `syllable_of` give the
  `(syllable, item)` place. A test on the files of `test-corpus/gabc` pins this order.
- **Notes.** `insert_note(n, after, Join::{Neume, NewNeume, NewSyllable}, pitch)` (a flat before
  note `n` stays with it), `delete_note(n)` (a syllable left with no text and no signs goes;
  one with text stays), `move_note(n, ±positions)` (a flat or natural right before it moves with
  it; the staff's top and bottom stop it), `set_pitch`.
- **Signs.** `set_alteration(n, Some(Alter::{Flat, Natural, Sharp}) | None)`, `set_shape(n,
  Shape)` (shapes GABC writes in unusual ways, such as `o0`, are refused), `set_mora`,
  `set_episema`, `set_ictus`, `set_initio_debilis`, `set_liquescence`, `toggle_mod` for any other
  GABC modifier.
- **Structure.** `insert_division(n, sign)`, `set_division`, `delete_division` (a division is
  a syllable of its own after the note's syllable), `set_initial_clef`, `set_clef_before(n, clef)`,
  `set_syllable_text(n, text)`, `split_syllable(n)` (before note `n`), `join_syllable(n)` (with
  the next one, its notes kept as a separate neume).
- **Keys.** On a chant track the editing keys of a modern track map to these: `<C-a>` / `<C-x>`
  move a note up / down a position, `octave-up` / `octave-down` seven; `accidental flat|natural|
  sharp` toggles the sign; `.` is the mora (`..` two); `+` / `-` set / clear the horizontal
  episema; `x`, `<BS>`, `<Del>` and Visual `d` delete notes; `<Space>` adds a note. A pitch moves
  by staff position, not by semitone: a flat is added or removed with `accidental flat`. Durations,
  tuplets, rests, ties, guitar effects, dynamics, beams, stems, note letters, bar insert / open /
  delete, put and Change report "not in chant".
- **Guard.** `App::edit` compares a chant track before and after every edit: an edit that
  changed the chant has its bars written again, one that changed the bars alone is undone and
  refused with "not in chant".

## 5. Drawing (`cecilia-engrave`)

- Heads, from the note's marks: `~` or initio debilis a small head, inclinatum a diamond, `v` a
  virga, `s…` a stropha (`ss`/`sss` draw two or three heads on the pitch), otherwise a punctum.
  The Caeciliae font has a glyph per head per height; its ready-made podatus, clivis and porrectus
  glyphs are used where they exist, other neumes are heads joined by connecting lines.
- **Mora** dots: a little right of the head; on a line they sit high, in a space low, so they never
  lie on a line. **Ictus** is a short tick in the space under the head, never crossing a staff
  line (below the next line if the space is too narrow). The horizontal episema sits over the head.
- **Divisions**: glyphs for virgula, minima, minor, maior and finalis on a four-line staff; on
  other line counts they are drawn as lines.
- The staff gap is 4/3 of a modern one, so four lines take the height of five.

## 6. Settings and commands

| | |
| --- | --- |
| `ui.chant_chords` | `top` (default), `bottom` or `error`: which note of a chord `:modern-to-chant` and `cecilia convert … .gabc` keep |
| `:clef c1`–`c4`, `f1`–`f4` | the clef of a chant track (the tracks pane's clef box does the same) |
| `:modern-to-chant` | the track's melody as a new chant buffer |
| `:chant-to-modern` | the chant score opened as a modern score in a new buffer |
| `:chant-shape`, `:chant-division`, `:chant-clef`, `:chant-mora`, `:chant-episema`, `:chant-ictus`, `:chant-alter`, `:chant-liquescence`, `:chant-symbol` | thin commands over the edit operations (the palette and `[keymap.normal-chant]` use them) |
| `:score-kind` | names the kind of score and its conversion (the status line's `MODERN` / `CHANT` chip) |
| `:interpretation equalist\|solesmes` | rhythm used for transcription and playback |
| `:chant-mod`, `:chant-bar`, `:chant-break`, `:chant-append`, `:chant-syllable` | editing a chant track (see "Edits") |
