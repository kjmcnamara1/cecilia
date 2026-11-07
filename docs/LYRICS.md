# Lyrics

How cecilia keeps, edits, lays out, draws, imports and exports lyrics. Key bindings are also
listed in [keymap.md](keymap.md); the design is summarised in [SPEC.md](SPEC.md) (4.3).

1. [Overview](#1-overview)
2. [Lyrics syntax](#2-lyrics-syntax)
3. [Which beats get which syllables](#3-which-beats-get-which-syllables)
4. [How lyrics are drawn](#4-how-lyrics-are-drawn)
5. [The LYRICS tab of the properties pane](#5-the-lyrics-tab-of-the-properties-pane)
6. [Commands](#6-commands)
7. [Configuration](#7-configuration)
8. [Limitations and plans](#8-limitations-and-plans)

## 1. Overview

### One text per track

Every modern track's lyrics are **one text**, written in the lyrics syntax of section 2 (words,
dashes, labels, anchors and GABC-style markup tags). A chant track's lyrics are its GABC instead
(below). That text is the single source of the track's lyrics. The syllables you see under the notes are *laid out from it*: after every
edit of the score (and whenever the text changes) cecilia lays the text onto the track's notes
again, one syllable to a note, in the order the music is played (section 3).

Each note (beat) then holds one syllable per *verse row*, the lines of lyrics stacked under the
staff. A syllable followed by a hyphen keeps a trailing `-` (`mor-`). These per-beat syllables
are what is drawn, played into chant notation and exported; they are rewritten from the text, so
changing them any other way does not last (pasting bars with lyrics, for instance: the text lays
its own syllables over them).

Only the first voice of a track takes syllables; lyrics on other voices are removed when the text
is laid out.

### Ordinary tracks and chant tracks

- **Ordinary tracks** (guitar, voice, piano …) keep the text in the track (`lyrics_text`). They
  have no chant staff: square notation belongs to chant tracks, and `:modern-to-chant` opens a
  track's melody as chant in a new buffer.
- **Chant tracks** (opened from a `.gabc` file, or made with `:modern-to-chant`) have their chant
  notation as their lyrics: the text area shows the chant's GABC body (its header, `name:` and so
  on, is kept apart). Editing it changes the chant itself, which is read again as GABC, and the
  notes that play and that the other staves show are transcribed from it again. Chant tracks
  don't take the `:lyrics` and `:lyrics-clear` commands.

### Files

| format | reading | writing |
| --- | --- | --- |
| `.cc` (cecilia) | the text as saved, laid out again on reading; files saved before the lyrics syntax (format version 1) get their text converted once, see below; files with no text get one written from their syllables | everything: the text exactly, and the syllables |
| Guitar Pro 3–5 (`.gp3`, `.gp4`, `.gp5`) | the file's lyric track and its up to five lyric lines (each from a bar, in Guitar Pro's lyric syntax) are placed as Guitar Pro places them, then **translated** into the syntax of section 2, keeping the line breaks: `[comments]` become `[labels]`, notes left without a syllable `_` words, anchors where Guitar Pro's placement (bar order) differs from the sung order | GP5: the text is **translated back** to Guitar Pro's syntax, one lyric line per verse row (see below). Lyrics of other tracks are dropped |
| Guitar Pro 6–8 (`.gpx`, `.gp`) | the syllables stored on the beats (one per lyric line) become text, one line of text per verse row | the syllables on the beats (one lyric line per verse row); a dash word and a skip word become empty beats |
| MusicXML (`.musicxml`, `.mxl`) | `<lyric number="n">` on each note (verse row n; `begin`/`middle` syllables get a trailing hyphen) become text | `<lyric number>` per note and verse row, `begin` for a syllable with a hyphen, else `single` |
| MIDI | no lyrics | no lyrics |
| GABC | a chant track; its lyrics are the chant | the chant of the first chant track (`:w song.gabc` of a score without one refuses). `cecilia convert song.gp5 song.gabc`, and `:modern-to-chant` then `:w`, turn a modern track into chant with its syllables (first verse row) |
| PDF, SVG, PNG | — | as drawn in the window (section 4) |

**Writing Guitar Pro 3–5.** Guitar Pro stores up to five lines of text, each sung from a bar, one
syllable per note in bar order. The track's text is translated into them:

- one line per verse row (an anchor going back starts the next row); a line starts at the bar of
  its first syllable; rows past the fifth are left out, and `:w` says so;
- line breaks in the text stay inside their line; words, hyphens and `+` are as written;
- a word of `_` of n underscores becomes n empty notes (n `_` words, which Guitar Pro reads as
  empty notes, also right after a hyphen); a word of n dashes keeps the hyphen on the syllable
  before it and then leaves n empty notes (**Guitar Pro has no dash that spans beats: the dash
  itself is lost**); an anchor going forward leaves that many empty notes;
- labels become `[comments]`; markup tags and `*` are dropped (the text inside tags stays).

**Old `.cc` files.** Text saved by earlier versions is converted the first time such a file is
read (the file then saves as format version 2): `<bar:n>` anchors become `(bar:beat)` (the number
was the beat's index in the first voice; the syllables stay on the same notes), GABC groups are
removed (a group between two letters becomes a hyphen, a leading clef is dropped), and `_`
italics become tags (`_si-lence` is `<i>si</i>-lence`, `a-_men` is `a-<i>men</i>`). Nothing tells
you it happened.

**What gets lost.** Labels, styles and ✻ exist only in the text: Guitar Pro, MusicXML and MIDI
have no place for them (Guitar Pro 3–5 keeps labels as comments), so they are kept by `.cc`
alone. A beat of a GP6–8 file holding just `-` (a melisma dash) becomes part of a dash word, and
is written back empty. Comments of Guitar Pro 3–5 lyric lines survive as labels but are lost
again when writing GP6–8.

## 2. Lyrics syntax

The text is read word by word. **Line breaks and runs of spaces only separate words**: a new line
does not start a new bar, verse or anything else (the line breaks are kept in the text, and
Guitar Pro 3–5 files keep them too). A word is sung **one syllable per beat**.

### Reference

| text | beats | meaning |
| --- | --- | --- |
| `someone` | 1 | one syllable |
| `some one` | 2 | two words |
| `some_one` | 2 | `_` between letters separates words, like a space |
| `some-one` | 2 | two syllables joined by a hyphen; the hyphen takes no beat |
| `some--one` | 2 | the same: dashes inside a word only join |
| `some- one`, `some -one` | 2 | the same: a dash at the end of a word joins it to the next, at the start of a word to the one before |
| `some - one` | 3 | a word of dashes covers that many beats under the dash: `some`, one beat under the dash, `one` |
| `some -- one` | 4 | `some`, two beats under the dash, `one` |
| `some _ one` | 3 | a word of underscores is not drawn and leaves that many beats empty |
| `some __ one` | 4 | two empty beats |
| `a+b` | 1 | `+` joins words into one syllable, printed `a b` (Guitar Pro's word joiner) |
| `*` | 0 | alone, a red ✻ printed after the syllable before it |
| `[1.]` | 0 | a label (non-syllable text) printed before the next syllable (verse numbers, "Refrain") |
| `(5:3.5)` | 0 | an anchor: the following syllables start at bar 5, beat 3.5 (`bar:beat.fraction`) |
| `<i>si</i>-lence` | 2 | GABC markup tags style the text they enclose (below) |

Details:

- **Hyphens.** A syllable joined to the next (in a word, at a word's edge, or before a dash word)
  is stored with a trailing `-` and drawn with a hyphen to the next syllable. A dash at the start
  of a word or a dash word joins the nearest syllable before it, passing over labels and `*`
  (but not over a skip or an anchor).
- **Underscores.** A word of only `_` is a skip. Anywhere else `_` separates words. (`_` no longer
  prints italics: use `<i>`.)
- **`*`** is a star only as a word on its own. It never takes a beat. It goes after the syllable
  just placed; at the start of the text, after a skip or a dash word, or after a syllable past the
  last note, it goes before the next syllable instead. `**` or `a*` are ordinary text.
- **Labels** `[…]` take no beat and go before the next syllable that is placed; several in a row
  are joined with spaces. A label ends the word it touches: `Ky[x]rie` is `Ky`, the label, `rie`.
- **Anchors** `(bar:beat)` or `(bar:beat.fraction)` are 1-based: bar numbers as on the score,
  beats as the status line's `bar:beat.fraction` (see [Positions](#positions)). The syllables after
  it start at the first note at or after that position, the **first time** the bar is sung; a
  beat number past the end of the bar goes on to the next bar sung. An anchor that goes **back**
  in the music (to a beat already passed) starts the next verse row for everything after it; one
  going forward leaves beats without text, as a skip would. An anchor to a bar the score doesn't
  have puts the following syllables past the last note. Like a label, an anchor ends the word it
  touches. A `(` that doesn't start a position (`(sigh)`, `(0:1)`) is ordinary text. GABC groups
  such as `Ky(f)ri(g)e` are **not** part of modern lyrics (they are chant notation: chant tracks
  keep their GABC).
- **Unclosed signs are text.** A `[` without a later `]` and an unclosed or unknown tag are
  ordinary characters of a syllable.

### Styles: markup tags

Text styles use GABC's markup tags, the same as in a chant score:

| tag | style |
| --- | --- |
| `<i>…</i>` | italic |
| `<b>…</b>` | bold |
| `<ul>…</ul>` | underlined (the line runs on under the hyphen to a next underlined syllable) |
| `<sc>…</sc>` | small capitals (lower-case letters as smaller capitals) |
| `<c>…</c>` | colored: red, as the rubrics of GABC |

- Tags take no beat, may span several syllables, words and lines, and nest
  (`<b><i>A</i>men</b>`). A tag applies to a syllable when it covers any of its letters; a hyphen
  ends a syllable, so `<i>si</i>-lence` and `a-<i>men</i>` style one syllable each.
- A tag that is unknown, never closed or has no opening is ordinary text, shown as typed. Other
  GABC tags (`<v>`, `<alt>`, `<e>`, `<nlba>`, `<pr>`, `<clear>`) are text in modern lyrics.
- `<sp>…</sp>` specials show as their characters: `<sp>V/</sp>` ℣, `<sp>R/</sp>` ℟,
  `<sp>A/</sp>` Ⱥ, `<sp>'ae</sp>` ǽ, `<sp>'oe</sp>` œ́, `<sp>ae</sp>` æ, `<sp>oe</sp>` œ,
  `<sp>+</sp>` ✠, `<sp>*</sp>` ✱.
- Italic and bold use the text font's faces (bold wins when both are set). The terminal shows the
  text without the tags and without the styles.

### Examples

The spec's examples, on a bar of four quarter notes:

| text | beat 1 | beat 2 | beat 3 | beat 4 |
| --- | --- | --- | --- | --- |
| `someone` | someone | | | |
| `some one` / `some_one` | some | one | | |
| `some-one` (`some--one`, `some- one`, `some -one`) | some- | one | | |
| `some - one` | some- | (dash) | one | |
| `some -- one` | some- | (dash) | (dash) | one |
| `some _ one` | some | | one | |
| `some __ one` | some | | | one |

"(dash)" beats hold no text; the score draws one hyphen centred between `some` and `one`.

A hymn with two verses, as typed in the text area:

```
(1:1) [1.] Let all mor-tal flesh keep _ si-lence,
And with fear and trem-bling _ stand;

[2.] Rank on rank the host of _ hea-ven
Spreads its van-guard on the _ way,
```

On five bars in 4/4 where bar 1 starts with a quarter rest (three quarters after it), bars 2–4
have four quarters and bar 5 two halves, the whole repeated once (bars 1–5 twice), the syllables
land like this (bar:beat; row 1 is the first pass, row 2 the second):

| beat | 1:1 | 1:2 | 1:3 | 1:4 | 2:1 | 2:2 | 2:3 | 2:4 | 3:1 | 3:2 | 3:3 | 3:4 | 4:1 | 4:2 | 4:3 | 4:4 | 5:1 | 5:2 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| row 1 | rest | 1. Let | all | mor- | tal | flesh | keep | | si- | lence, | And | with | fear | and | trem- | bling | | stand; |
| row 2 | rest | 2. Rank | on | rank | the | host | of | | hea- | ven | Spreads | its | van- | guard | on | the | | way, |

`(1:1)` is a rest, so the label `1.` and `Let` go on the first note (1:2), the label printed just
before `Let`, which stays centred on its note. Each `_` leaves a beat without text. The second
verse needs nothing to start it: it simply continues, and the second pass of the repeat is shown
on the second row. Without the repeat it would go on in bar 6 on the first row; to stack it under
the first verse instead, start it with an anchor that goes back, `(1:1) [2.] Rank on rank …`.

A few more, on the same bars (one pass shown):

| text | where it lands |
| --- | --- |
| `Ky-ri-e * e-le-i-son` | Ky- 1:2, ri- 1:3, e 1:4 with ✻ after it, e- 2:1, le- 2:2, i- 2:3, son 2:4 |
| `<i>A</i>-men` | *A*- 1:2 (italic), men 1:3 |
| `la (3:2) lo` | la 1:2, lo 3:2 (the beats between stay empty) |
| `la (3:2.5) lo` | lo on the note at the middle of beat 2 of bar 3 |
| `la lo (1:1) li` | la 1:2, lo 1:3, li 1:2 on the next row (the anchor went back) |

## 3. Which beats get which syllables

### Notes that take a syllable

A beat of the first voice takes a syllable when it has a note: tied notes (continuing the note
before) and dead notes too, as in Guitar Pro. Rests and empty beats take none and are passed
over, so `Let` in the hymn above goes on the first note after the rest. Without an anchor the
lyrics start at the first note of the track, a pickup bar included.

### Sung order

Syllables go to the notes in the order the music is played:

- **Repeats.** A repeated section is sung again, each closing repeat bar counting its own passes
  (as Guitar Pro does), so the next words go on the second pass.
- **Alternate endings** are sung on their passes (ending 1 on the first, ending 2 on the second …);
  past the last ending the repeat is over.
- **D.C. and D.S.** (D.C., D.C. al Fine, al Coda, al Double Coda; D.S. and D.S.S. likewise, from
  the segno or double segno): after the bar with the jump has had all its passes, the music goes
  back once. After the jump repeats are not taken again and of a group of alternate endings only
  the last is played. *al Fine* stops after the bar marked Fine; *al Coda* goes from the bar
  marked To Coda to the Coda bar. A jump without its target (D.S. without a segno) is not taken.
- **Bars never reached** (alternate endings without a repeat that would get there, say) are sung
  after everything else, in bar order, so lyrics put on them are not lost.

Playback follows repeats and endings but not D.C./D.S. jumps; lyrics do follow those.

### Verse rows

A syllable's row is the pass through its repeat (the first pass is row 1) or one more than the
times its bar was sung before, whichever is more; every anchor that went back in the music adds
one more row to the syllables after it. So the second time through a repeat (or after a D.C.)
sings on row 2, and lyric lines that start with an anchor to a bar already sung stack under the
ones before them. In bars sung once, rows past the first only come from anchors going back.

### After edits

The text is laid out again after every edit, so the lyrics follow the notes:

- deleting a note hands its syllable, and all that follow, on to the next notes;
- turning a note into a rest moves the text on past it;
- new notes take the next syllables;
- syllables past the last note stay in the text but are not shown ("lyrics: N syllables past the
  last note are kept, not shown"); they come back when notes are added;
- anchors keep their bar and beat number, so the syllables after an anchor stay where it points.

A track whose beats have syllables but no text yet (made by `:chant-to-modern`, or from older
files) gets its text written from its syllables before the next edit; the LYRICS tab shows that
text meanwhile. Each change to the text, and each edit with the lyrics following, is one undo
step.

## 4. How lyrics are drawn

### Window, PDF, SVG and PNG

- **Rows.** The track's own lyrics go under its top staff, one line per verse row, below anything
  hanging under the staff (dynamics, hairpins). With `:lyrics-attach` they go above or below
  another staff instead (section 6).
- **Syllables** are centred on their notes, in the text font (Source Sans 3).
- **Hyphens.** One hyphen is drawn between a syllable with a trailing `-` and the next syllable on
  its row, centred in the gap when it is wide enough (else none); at the end of a system it goes
  just after the syllable. Dash words (`some -- one`) give the same single hyphen across their
  beats; no long dashes or extender lines are drawn under melismas.
- **Labels** go just before their syllable, which stays centred on its note.
- **Italics:** syllables marked with `_` are slanted.
- **✻** is drawn (six strokes) just after its syllable. A `*` that goes before a syllable (at the
  start of the text) is printed `*` with the labels.
- Lyrics attached from another track are placed by their position in the bar and pushed apart so
  that none overlap.

### Terminal

- The tab view shows up to five verse rows under the tab, in the theme's `lyrics` color, each
  syllable as stored (`mor-` with its hyphen) at its beat's column; columns widen to fit them.
- Labels, italics and ✻ are **not** shown there, nor lyrics attached to other staves.
- A chant track shows its syllables under the neumes with hyphens and extenders.

## 5. The LYRICS tab of the properties pane

`<leader>p` shows the properties pane, `<leader>P` focuses it, `3` (or `L`/`H`, or
`:properties-tab lyrics`) shows the LYRICS tab. The tab is about the track under the score
cursor (its name at the top) and has one row, the **text area** with the track's lyrics. It has no
label, box or background. What it shows depends on the kind of track:

### An ordinary track: the lyrics text

The text area shows the text as written (section 2) and edits it directly; there is no GABC
switch. After each key the syllables are laid onto the notes again.

### A chant track: its GABC

The text is the chant's GABC and is edited; it is always shown as GABC (there is no switch). Each
key reads it again as GABC; while it doesn't read (an unclosed `(` half typed) the chant stays as
it was, and the text area keeps what you type either way. Leaving the text area and coming back
shows the chant as cecilia writes GABC (single spaces, line breaks every 72 characters or so).

### Colors and font

| piece | ordinary track | chant track |
| --- | --- | --- |
| words | `lyrics_text` | `lyrics_text` |
| `(…)` groups and the clef | — (`(5:3)` is an anchor) | shown, in the colors below |
| `[labels]` | `lyrics_label`, bold | `lyrics_label`, bold |
| `(bar:beat)` anchors | `lyrics_anchor` (red) | — (GABC groups) |
| GABC clefs / divisions / notes | — | `lyrics_clef` (purple) / `lyrics_division` (red) / `lyrics_note` (blue) |
| shape / attachment / space modifiers | — | `lyrics_shape` (yellow) / `lyrics_attach` (cyan) / `lyrics_space` (gray) |
| markup tags `<b>` `</i>` … (the text inside keeps its color) | `lyrics_markup` (green) | `lyrics_markup` (green) |
| parentheses of groups | — | `lyrics_text_dim` |

The caret and selection use the Normal-mode color (`mode_normal`). In the window the text area is
drawn in (`ui.lyrics_font`, `ui.lyrics_font_size`; section 7); the terminal draws it in its own
font, with bold where the table says. The text wraps at the pane's width, at spaces.

### Getting into the text area

The text area is edited the way vim edits text, with a normal mode (a block caret) and an insert
mode (a bar caret); the status line shows NORMAL or INSERT. Moving onto it (it is the first row you can select) gives it the block
caret at once:

| keys | |
| --- | --- |
| `G` | the last row: the text area |
| `<leader>P` | focus the pane; with the text area selected it gets the caret |
| a click in the text | the caret goes there, normal mode |

While the text area has the caret **it takes every key**, so press `<Esc>` before `:` commands
and `<leader>` keys.

### Normal mode

| keys | |
| --- | --- |
| `h` `l` `<Left>` `<Right>` | character left / right |
| `j` `k` `<Down>` `<Up>` | line down / up, by the lines as wrapped on screen |
| `w` `b` `e`, `W` `B` `E`, `<C-Left>` `<C-Right>` | next word, word back, end of word (`W` `B` `E`: words split at spaces only) |
| `0` `^` `$` `<Home>` `<End>` | start of the line, its first non-space, its end |
| `gg` / `G` | start of the text / of its last line |
| `d` `c` `y` + motion | delete / change / copy to where the motion goes: `dw` `cw` `de` `db` `d0` `d$` `dj` `dk` `dgg` `dG` … |
| `dd` `cc` `yy`, `D` `C` `S` `Y` `x` `X` `s` | the line, the rest of the line, a character |
| `d` `c` `y` + `i`/`a` + object | `iw` `aw` `iW` `aW` (word), `ip` `ap` (paragraph, with the blank lines after it for `ap`), `i(` `a(` (`b`), `i[` `i{` (`B`) `i<`, `i"` `i'` ``i` ``: `diw`, `caw`, `dap`, `yip`, `ci"` … |
| `r` + character, `~` | replace the character, flip its case |
| `i` `a` `I` `A` `o` `O`, `<CR>` | insert before / after the caret, at the line's first non-space / end, on a new line below / above; `<CR>` is `i` |
| `v` / `V` | Visual mode (characters / whole lines) |
| `u` / `<C-r>` | undo / redo (the score's undo: the text is read back) |
| `/`, `n` `N` | search the text; next / previous match |
| `p` `P` | paste after / before the caret (copied lines go below / above the line) |
| `<C-a>` `<C-c>` `<C-x>` `<C-v>` | select all, copy, cut, paste (system clipboard) |
| `H` `L` | previous / next tab of the properties pane |
| `:`, `<C-s>` … | `:` opens the command line and any Ctrl / Alt key your keymap binds works as elsewhere (`<C-s>` save …) |
| `<C-h>` `<C-j>` … | a focus key of your keymap (`focus-left` …) leaves the text area for that pane; coming back puts the caret where it was |
| `<Esc>` | forget a command begun; clear the selection; else leave the text area **and the pane** (back to the score) |

What `d` and `c` remove is also copied, so `p` puts it back. There are no counts. Leaving the pane
and focusing it again puts the caret back where it was in the text area.

### Visual mode

`v` selects characters, `V` whole lines; the status line shows VISUAL. Motions (all of the above,
`gg` `G`) move the end of the selection, `iw` `aw` `ip` `ap` `i(` `i"` … select an object, `o` goes
to the other end, `v` / `V` switch kind or leave.

| keys | |
| --- | --- |
| `d` `x` `<Del>` `<BS>`, `c` `s` | delete, change (delete and insert) |
| `D` `X`, `C` `S` `R` | the same on whole lines |
| `y` `<C-c>`, `Y` | copy, copy the lines |
| `p` `P`, `<C-v>` | replace the selection with the clipboard |
| `~` `u` `U` | flip case, lower case, upper case |
| `<Esc>` | leave Visual mode |

### Insert mode

| keys | |
| --- | --- |
| characters, `<CR>` | typed in (a line break) |
| `<BS>` `<Del>` | delete before / at the caret (the selection, if any) |
| `<C-BS>` / `<C-Del>` | delete to the start / end of the word |
| `<C-Left>` `<C-Right>` | jump a word left / right |
| `<Left>` `<Right>` `<Up>` `<Down>` `<Home>` `<End>` | move (up and down by wrapped lines) |
| `<C-u>` | clear the whole text |
| `<C-a>` `<C-c>` `<C-x>` `<C-v>` | select all, copy, cut, paste; typing replaces a selection |
| `<Esc>` | back to normal mode (the caret steps back onto the last character, as in vim) |

The score follows every key. An insert session (from `i` to `<Esc>`) is one undo step; each change
made in normal mode is one too.

### Mouse

A click places the caret (entering the text area in normal mode); a drag selects; a double click
selects the word in insert mode and enters insert mode at the click in normal mode. The wheel
scrolls the text.

### Search

`/` in normal mode opens a search field along the bottom of the pane. It jumps to the first match
from the caret as you type, ignoring case unless the query has a capital letter; `<CR>` keeps the
jump and remembers the query for `n` and `N` (which wrap around), `<Esc>` goes back to where the
search started, `<BS>` on an empty query closes it and `<C-u>` clears it.

### Clearing

`:properties-clear` with the text area selected removes all the track's lyrics (as
`:lyrics-clear` does). Inside the text area `d`, `x` and `c` are its own keys; `<C-a>` then `d`,
or `<C-u>` in insert mode, empties it as an edit.

## 6. Commands

| command | |
| --- | --- |
| `:lyrics TEXT` | adds lyrics from the cursor on: a line `(bar:beat) TEXT` ending in a line break, put where it belongs in the text: before the first syllable sung from the cursor on, the text after it keeping its place under an anchor of its own. When the words would run into the text after them it goes at the end instead. `\n` in `TEXT` is a line break. The anchor names the cursor's beat in the first voice (with the cursor in another voice, the first-voice beat starting at or after it). If the text already reaches past the cursor, the anchor goes back and the words go on the next verse row; otherwise they continue the row from the cursor. Reports how many syllables were placed, or how many are past the last note. Not on chant tracks |
| `:lyrics-clear` | removes all the track's lyrics (the text) |
| `:lyrics-clear N` | removes the syllables laid on verse row `N`, with their labels, the ✻ and the dashes and skips after them; later rows move up (in text order: with flowing verses, the next verse moves into the gap). Not on chant tracks |
| `:lyrics-attach above\|below [TRACK]` | shows lyrics above or below the cursor's staff: those of track `TRACK` (number; the staff's own track by default), aligned with the staff's beats by their position in the bar. Once any staff has lyrics attached, only those staves show lyrics |
| `:lyrics-attach none` | detaches them; without any attachment a track shows its own lyrics under its top staff |
| `:properties-tab lyrics` | the LYRICS tab of the properties pane (`3` in the pane) |
| `:properties-clear` | in the LYRICS tab: removes all the track's lyrics |
| `:modern-to-chant` | the cursor track as chant, in a new buffer: syllables from the first verse row (a syllable starts at each note that has one; a trailing `-` continues the word) |
| `:chant-to-modern` | a chant track becomes an ordinary track (its transcription); its syllables stay on the beats and become its lyrics text |

For example `:lyrics Let all mor-tal flesh` at bar 1 beat 1 of an empty track gives the text
`(1:1) Let all mor-tal flesh` and a line break; `:lyrics Hel-lo\nworld` writes two lines.

## 7. Configuration

Theme colors (`[theme]` in `config.toml`, or a theme file), set by every built-in theme to suit
its palette:

| key | used for |
| --- | --- |
| `lyrics_text` | words in the lyrics text area |
| `lyrics_text_dim` | the parentheses of GABC groups (chant tracks) |
| `lyrics_gabc` | GABC groups and the clef (bold) |
| `lyrics_label` | `[labels]` (bold) |
| `lyrics_anchor` | `(bar:beat)` anchors |
| `lyrics_clef` `lyrics_division` `lyrics_note` `lyrics_shape` `lyrics_attach` `lyrics_space` `lyrics_markup` | the parts of GABC and markup tags (section 5) |
| `lyrics` | lyrics in the terminal's score views (tab and chant) |

Engraved lyrics use the page's ink color.

`[ui]` settings for the window:

| key | default | |
| --- | --- | --- |
| `lyrics_font` | `"Source Sans 3"` | font family of the lyrics text area: the app's text font, or any installed family, looked up in the system's font folders (`$XDG_DATA_HOME/fonts`, `~/.local/share/fonts`, `~/.fonts`, `/usr/share/fonts`, `/usr/local/share/fonts`, the macOS and Windows font folders): `.ttf` and `.otf` files whose name starts with the family (letters and digits, any case) whose name table gives that family, upright, the weights nearest regular and bold. Characters it lacks, and a family that isn't found, come from the app's text font |
| `lyrics_font_size` | `12.0` | size in points (6 to 48), at 96 pixels per inch |

```toml
[ui]
lyrics_font = "Source Sans 3"
lyrics_font_size = 12.0

[theme]
preset = "nord"
lyrics_text = "#e5e9f0"
lyrics_text_dim = "#7b88a1"
lyrics_gabc = "#eceff4"
lyrics_label = "#81a1c1"
lyrics_anchor = "#bf616a"
```

## 8. Limitations and plans

- The terminal's score views don't show labels, styles, ✻ or attached lyrics.
- Guitar Pro 6–8 files can also keep lyric lines on the track (Guitar Pro 3–5 style); those are
  not placed on the notes: cecilia reads the syllables stored on the beats.
- Labels, styles and ✻ are kept only by `.cc` (section 1); Guitar Pro 3–5 keeps labels as comments, and a dash word's dash is lost there.
- Playback doesn't follow D.C./D.S. jumps (the lyrics do).

## Positions

Everywhere a place in the score is shown or typed it is `bar:beat.fraction`: the bar from 1, the
beat from 1 (the time signature's unit: a quarter in 4/4, an eighth in 6/8) and the part of the
beat after the point. The eighth on the upbeat of beat 3 of bar 5 is `5:3.5`. A bar that holds
more than its count goes on counting: the sixth quarter of a 4/4 bar is `2:6.0`. The status line,
marks, `:5:3.5` (go there), `:lyrics` and lyrics anchors use it; an anchor may leave out a zero
fraction (`(2:6)`).
