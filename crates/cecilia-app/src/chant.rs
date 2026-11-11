//! Chant commands: editing the notes of a chant score through the operations on [`Chant`],
//! syllables as GABC, interpretation, clefs, conversion to and from modern notation.
//!
//! A chant score's chant is its source; the bars are a cache made by `retranscribe` after every
//! edit. Nothing here (or anywhere) edits those bars: keys that would are mapped to a chant
//! operation or refused with "not in chant".

use cecilia_core::chant::{
    self, Cell, CellKind, Chant, Liquescence, NeumeItem, Shape, Syllable, retranscribe, transcribe,
};
use cecilia_core::chant_edit::{Alter, Join};

use crate::app::{App, Buffer};
use crate::command::{BeatAt, Command};

/// The shape a GABC modifier letter stands for.
fn shape_of_token(m: &str) -> Option<Shape> {
    Some(match m {
        "v" => Shape::Virga,
        "V" => Shape::VirgaReversa,
        "vv" => Shape::Bivirga(2),
        "vvv" => Shape::Bivirga(3),
        "s" => Shape::Stropha(1),
        "ss" => Shape::Stropha(2),
        "sss" => Shape::Stropha(3),
        "w" => Shape::Quilisma,
        "o" => Shape::Oriscus(false),
        "O" => Shape::Oriscus(true),
        "q" => Shape::Quadratum,
        _ => return None,
    })
}

/// The head a name stands for (`chant-shape NAME`).
pub(crate) fn shape_by_name(name: &str) -> Option<Shape> {
    Some(match name {
        "punctum" => Shape::Punctum,
        "inclinatum" => Shape::Inclinatum,
        "virga" => Shape::Virga,
        "virga-reversa" => Shape::VirgaReversa,
        "bivirga" => Shape::Bivirga(2),
        "trivirga" => Shape::Bivirga(3),
        "stropha" => Shape::Stropha(1),
        "bistropha" => Shape::Stropha(2),
        "tristropha" => Shape::Stropha(3),
        "quilisma" => Shape::Quilisma,
        "oriscus" => Shape::Oriscus(false),
        "oriscus-stem" => Shape::Oriscus(true),
        "quadratum" => Shape::Quadratum,
        _ => return None,
    })
}

/// Toggles a modifier of note `n` the way a key does: the note takes it, or loses it when it
/// already has it.
fn toggle_modifier(chant: &mut Chant, n: usize, m: &str) -> Result<(), String> {
    let note = chant.note(n).ok_or("no note under the cursor")?.clone();
    if let Some(shape) = shape_of_token(m) {
        let now = if note.shape() == shape {
            Shape::Punctum
        } else {
            shape
        };
        return chant.set_shape(n, now);
    }
    let liquescence = |l: Liquescence| {
        if note.liquescence() == l {
            Liquescence::None
        } else {
            l
        }
    };
    match m {
        "-" => chant.set_initio_debilis(n, !note.initio_debilis),
        "~" => chant.set_liquescence(n, liquescence(Liquescence::Deminutus)),
        "<" => chant.set_liquescence(n, liquescence(Liquescence::AuctusAscendens)),
        ">" => chant.set_liquescence(n, liquescence(Liquescence::AuctusDescendens)),
        "." => chant.set_mora(n, u8::from(note.mora() == 0)),
        ".." => chant.set_mora(n, if note.mora() == 2 { 0 } else { 2 }),
        "_" => chant.set_episema(n, !note.episema()),
        "'" => chant.set_ictus(n, !note.ictus()),
        other => chant.toggle_mod(n, other),
    }
}

/// Where the cursor goes after an edit of a chant.
enum Target {
    /// The note with this index in reading order.
    Note(usize),
    /// The first cell of this syllable.
    Syllable(usize),
    /// The cell with this ordinal.
    Cell(usize),
}

/// The syllables the cells in the beats `from` to `to` belong to, in order.
struct Covered {
    syllable: usize,
    /// The notes (reading order) of the cells there.
    notes: Vec<usize>,
    /// Every cell of the syllable is there.
    whole: bool,
}

fn covered(chant: &Chant, from: (usize, usize), to: (usize, usize)) -> Vec<Covered> {
    let t = transcribe(chant);
    let mut out: Vec<Covered> = Vec::new();
    for c in t
        .cells
        .iter()
        .filter(|c| (c.bar, c.beat + c.len - 1) >= from && (c.bar, c.beat) <= to)
    {
        if out.last().is_none_or(|o| o.syllable != c.syllable) {
            out.push(Covered {
                syllable: c.syllable,
                notes: Vec::new(),
                whole: false,
            });
        }
        out.last_mut().unwrap().notes.extend(c.note);
    }
    for cv in &mut out {
        let all = t.cells.iter().filter(|c| c.syllable == cv.syllable).count();
        let here = t
            .cells
            .iter()
            .filter(|c| {
                c.syllable == cv.syllable
                    && (c.bar, c.beat + c.len - 1) >= from
                    && (c.bar, c.beat) <= to
            })
            .count();
        cv.whole = here == all;
    }
    out
}

impl App {
    /// Whether the buffer shown is a chant score.
    pub(crate) fn cursor_on_chant(&self) -> bool {
        self.buffer().score.is_chant()
    }

    /// Opens `score` as a new unsaved buffer and shows it.
    fn open_new_buffer(&mut self, name: String, score: cecilia_core::Score) {
        self.buffers.push(Buffer {
            path: None,
            name,
            score,
            modified: true,
            history: Default::default(),
            version: 0,
            disk: None,
        });
        let b = self.buffers.len() - 1;
        self.show_buffer(b);
    }

    fn require_chant(&mut self) -> bool {
        let ok = self.cursor_on_chant();
        if !ok {
            self.error("chant command: not in a modern score (use :modern-to-chant to open its melody as chant)");
        }
        ok
    }

    /// Sets (or, when it is already there, takes off) the flat or natural before the cursor note.
    fn chant_alter(&mut self, want: Alter) {
        self.chant_note_op(move |chant, n| {
            let now = chant.alteration_of(n);
            chant.set_alteration(n, if now == Some(want) { None } else { Some(want) })
        });
    }

    /// Toggles a GABC modifier of the cursor note (see [`toggle_modifier`]).
    fn chant_toggle(&mut self, modifier: &'static str) {
        self.chant_note_op(move |chant, n| toggle_modifier(chant, n, modifier));
    }

    /// Like [`Self::chant_op`] for what changes a note's signs: where the cursor has no note
    /// (an empty beat, a division, the end) a punctum on the cursor's staff line goes in first,
    /// and the change is made to it. The closure gets the note.
    fn chant_note_op(&mut self, f: impl FnOnce(&mut Chant, usize) -> Result<(), String>) {
        let line = self.pane().cursor.line.clamp(0, 14) as u8;
        // Insert mode keeps going: a note made here gets an empty group after it.
        let insert = self.mode == crate::config::Mode::Insert && !self.ed.replace;
        self.chant_cell_op(move |chant, at, cell| {
            let made = at.is_none();
            let n = match (at, cell) {
                (Some(n), _) => n,
                (None, Some(c)) if c.kind == CellKind::Slot => chant.fill_slot(c.syllable, line)?,
                (None, Some(c)) => {
                    let si = chant.slot_after(c.syllable);
                    chant.fill_slot(si, line)?
                }
                (None, None) => {
                    let si = chant.insert_slot(chant.syllables.len());
                    chant.fill_slot(si, line)?
                }
            };
            f(chant, n)?;
            if insert
                && made
                && let Some(si) = chant.syllable_of(n)
            {
                chant.insert_slot(si + 1);
            }
            Ok(Some(Target::Note(n)))
        });
    }

    /// Index (in reading order) of the chant note under the cursor: the note sounded by the
    /// cursor beat, else the last one before it. None in a chant without notes.
    pub(crate) fn chant_cursor_note(&self) -> Option<usize> {
        let c = self.pane().cursor;
        let chant = self.buffer().score.tracks.get(c.track)?.chant.as_deref()?;
        transcribe(chant).note_near(c.bar, c.beat)
    }

    /// The cell the cursor stands on (a note, an empty group, a division of its own).
    pub(crate) fn chant_cursor_cell(&self) -> Option<Cell> {
        let c = self.pane().cursor;
        let chant = self.buffer().score.tracks.get(c.track)?.chant.as_deref()?;
        let t = transcribe(chant);
        t.cell_at(c.bar, c.beat).map(|k| t.cells[k])
    }

    /// Edits the cursor track's chant as one undo step and writes its bars again. The closure
    /// gets the note under the cursor (None off a note, or in a chant without notes) and returns
    /// the note the cursor should be on afterwards (None leaves the cursor where it is).
    fn chant_op(
        &mut self,
        f: impl FnOnce(&mut Chant, Option<usize>) -> Result<Option<usize>, String>,
    ) {
        self.chant_cell_op(move |chant, at, _| Ok(f(chant, at)?.map(Target::Note)));
    }

    /// Like [`Self::chant_op`], with the cell under the cursor too and any place to land on.
    fn chant_cell_op(
        &mut self,
        f: impl FnOnce(&mut Chant, Option<usize>, Option<Cell>) -> Result<Option<Target>, String>,
    ) {
        if !self.require_chant() {
            return;
        }
        let cell = self.chant_cursor_cell();
        let at = cell.and_then(|c| c.note);
        self.edit(|cx| {
            let t = cx.cursor.track;
            let score = cx.whole();
            let mut chant = (**score.tracks[t].chant.as_ref().ok_or("not a chant track")?).clone();
            let target = f(&mut chant, at, cell)?;
            let tr = transcribe(&chant);
            let landed = target.and_then(|target| {
                let k = match target {
                    Target::Note(n) => tr.cells.iter().position(|c| c.note == Some(n)),
                    Target::Syllable(si) => tr.cells.iter().position(|c| c.syllable == si),
                    Target::Cell(k) => (!tr.cells.is_empty()).then(|| k.min(tr.cells.len() - 1)),
                }?;
                let c = tr.cells[k];
                let pitch = c.note.and_then(|n| chant.note(n)).map(|n| n.pitch);
                Some((c.bar, c.beat, pitch))
            });
            score.tracks[t].chant = Some(Box::new(chant));
            retranscribe(score, t);
            if let Some((bar, beat, pitch)) = landed {
                cx.cursor.bar = bar;
                cx.cursor.beat = beat;
                if let Some(p) = pitch {
                    cx.cursor.line = p as i32;
                }
            }
            Ok(())
        });
    }

    /// A new empty group where `at` says (as `i` `a` `I` `A` make a beat), and the cursor on it.
    fn chant_new_slot(&mut self, at: BeatAt) {
        let bar = self.pane().cursor.bar;
        self.chant_cell_op(move |chant, _, cell| {
            let t = transcribe(chant);
            let in_bar: Vec<&Cell> = t.cells.iter().filter(|c| c.bar == bar).collect();
            let end = chant.syllables.len();
            let index = match at {
                BeatAt::Before => cell.map_or(end, |c| c.syllable),
                BeatAt::After => cell.map_or(end, |c| c.syllable + 1),
                BeatAt::BarStart => in_bar.first().map_or(end, |c| c.syllable),
                // Before the division that closes the bar, if one does.
                BeatAt::BarEnd => match in_bar.last() {
                    Some(c) if c.kind == CellKind::Division => c.syllable,
                    Some(c) => c.syllable + 1,
                    None => end,
                },
            };
            Ok(Some(Target::Syllable(chant.insert_slot(index))))
        });
    }

    /// `:lyrics TEXT` on a chant: `text` is the whole text of the cursor syllable.
    pub(crate) fn chant_lyrics(&mut self, text: String) {
        self.chant_syllable_text(text, None);
    }

    /// Sets the text of the cursor syllable, and whether a space precedes it (it starts a word).
    fn chant_syllable_text(&mut self, text: String, word_start: Option<bool>) {
        self.chant_cell_op(move |chant, _, cell| {
            let si = cell
                .ok_or("lyrics: the cursor is not on a syllable")?
                .syllable;
            chant.set_text_of_syllable(si, &text)?;
            if let Some(w) = word_start {
                chant.syllables[si].word_start = w;
            }
            Ok(None)
        });
    }

    /// The text of the syllable the cursor is on, and whether it starts a word.
    fn cursor_syllable_text(&self) -> Option<(String, bool)> {
        let si = self.chant_cursor_cell()?.syllable;
        let c = self.pane().cursor;
        let chant = self.buffer().score.tracks.get(c.track)?.chant.as_deref()?;
        chant
            .syllables
            .get(si)
            .map(|s| (s.text.clone(), s.word_start))
    }

    /// `:lyric-mode` (`<A-i>`): Insert mode where the keys type the cursor syllable's text.
    fn lyric_mode(&mut self) {
        if !self.require_chant() {
            return;
        }
        // The palette's tool (or the key) again leaves it.
        if self.ed.lyric && self.mode == crate::config::Mode::Insert {
            self.ed.lyric = false;
            return self.run(Command::Normal);
        }
        if self.chant_cursor_cell().is_none() {
            return self.error("lyric-mode: the cursor is not on a syllable");
        }
        self.enter_insert();
        self.ed.lyric = true;
    }

    /// In lyric mode, the cursor on the first note of the next (`by` = 1) or previous (`by` = -1)
    /// syllable that has notes. Stays where it is at the first and last.
    fn lyric_move(&mut self, by: isize) {
        let c = self.pane().cursor;
        let Some(cell) = self.chant_cursor_cell() else {
            return;
        };
        let Some(chant) = self
            .buffer()
            .score
            .tracks
            .get(c.track)
            .and_then(|t| t.chant.as_deref())
        else {
            return;
        };
        let has_notes = |i: usize| chant.syllables[i].notes().next().is_some();
        let target = if by > 0 {
            (cell.syllable + 1..chant.syllables.len()).find(|&i| has_notes(i))
        } else {
            (0..cell.syllable).rev().find(|&i| has_notes(i))
        };
        let Some(si) = target else {
            return;
        };
        let t = transcribe(chant);
        let Some(k) = t.cells.iter().position(|x| x.syllable == si) else {
            return;
        };
        let to = t.cells[k];
        let pitch = to.note.and_then(|n| chant.note(n)).map(|n| n.pitch);
        let cur = &mut self.pane_mut().cursor;
        cur.bar = to.bar;
        cur.beat = to.beat;
        if let Some(p) = pitch {
            cur.line = p as i32;
        }
    }

    /// A key in lyric mode: typed characters go to the end of the syllable's text, `<BS>` takes
    /// one off, `<Tab>` and `<S-Tab>` go to the next and previous syllable, `<Esc>` and `<CR>`
    /// leave. The score and the lyrics text area follow each key.
    pub(crate) fn lyric_key(&mut self, key: crate::keys::Key) {
        use crate::keys::KeyCode;
        let plain = !key.ctrl && !key.alt && !key.sup;
        let Some((mut text, mut word_start)) = self.cursor_syllable_text() else {
            self.ed.lyric = false;
            return self.run(Command::Normal);
        };
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.ed.lyric = false;
                return self.run(Command::Normal);
            }
            KeyCode::Tab if plain => return self.lyric_move(1),
            KeyCode::BackTab => return self.lyric_move(-1),
            // On a syllable without text a space is the one before it, which starts a word.
            KeyCode::Backspace if text.is_empty() => word_start = false,
            // Text goes before the closing tags it ends with (`<i>ab|</i>`).
            KeyCode::Backspace => {
                let end = cecilia_core::chant::typing_end(&text);
                match text[..end].char_indices().next_back() {
                    // An empty tag pair goes whole.
                    Some((i, '>')) if text[..i].rfind('<').is_some() => {
                        let open = text[..i].rfind('<').unwrap();
                        let name = text[open + 1..i].to_string();
                        text.replace_range(open..=i, "");
                        let close = format!("</{name}>");
                        if let Some(c) = text[open..].find(&close) {
                            text.replace_range(open + c..open + c + close.len(), "");
                        }
                    }
                    Some((i, _)) => {
                        text.remove(i);
                    }
                    None => {}
                }
            }
            KeyCode::Char(' ') if plain && text.is_empty() => word_start = true,
            KeyCode::Char(c) if plain => text.insert(cecilia_core::chant::typing_end(&text), c),
            _ => return,
        }
        self.chant_syllable_text(text, Some(word_start));
    }

    /// `<CR>` and `<Space>` in Insert and Replace mode: a punctum on the staff line of the
    /// cursor goes into the empty group the cursor is on, or the note there is changed (see
    /// [`Self::chant_put`]). The cursor stays where it is, on the note.
    fn chant_enter(&mut self) {
        self.chant_put(!self.ed.replace);
    }

    /// A punctum on the cursor's staff line goes into the empty group the cursor is on.
    /// Elsewhere (a division, the end of the chant) it goes into a new group after the cursor
    /// cell. On a note, the note moves to the line, keeping its modifiers, or, on its own line,
    /// another of the same pitch is added after it in its neume. Insert mode (`insert`) also
    /// adds an empty group after a group it put a note in, for the next note; Replace mode adds
    /// none. The cursor goes to the note.
    fn chant_put(&mut self, insert: bool) {
        let line = self.pane().cursor.line.clamp(0, 14) as u8;
        self.chant_cell_op(move |chant, _, cell| {
            let (n, new_group) = match cell {
                Some(c) if c.kind == CellKind::Slot => (chant.fill_slot(c.syllable, line)?, true),
                Some(Cell {
                    note: Some(n),
                    kind: CellKind::Note,
                    ..
                }) => match chant.note(n).map(|x| x.pitch) {
                    // On its own line: the same note again, in the same neume.
                    Some(p) if p == line => (chant.insert_note(n, true, Join::Neume, p)?, false),
                    _ => {
                        chant.set_pitch(n, line)?;
                        (n, false)
                    }
                },
                Some(c) => {
                    let si = chant.slot_after(c.syllable);
                    (chant.fill_slot(si, line)?, true)
                }
                None => {
                    let si = chant.insert_slot(chant.syllables.len());
                    (chant.fill_slot(si, line)?, true)
                }
            };
            if insert && new_group {
                let si = chant.syllable_of(n).ok_or("no such note")?;
                chant.insert_slot(si + 1);
            }
            Ok(Some(Target::Note(n)))
        });
    }

    /// Leaving Insert mode takes away the empty group the cursor is left on, or the one right
    /// after the cursor's note (nothing was entered there).
    fn chant_drop_slot(&mut self) {
        let c = self.pane().cursor;
        let Some(chant) = self
            .buffer()
            .score
            .tracks
            .get(c.track)
            .and_then(|t| t.chant.as_deref())
        else {
            return;
        };
        let t = transcribe(chant);
        let Some(here) = t.cell_near(c.bar, c.beat) else {
            return;
        };
        let empty = |k: usize| {
            t.cells
                .get(k)
                .filter(|x| x.kind == CellKind::Slot)
                .filter(|x| {
                    chant
                        .syllables
                        .get(x.syllable)
                        .is_some_and(|s| s.text.trim().is_empty())
                })
        };
        // The slot the cursor is on goes and the cursor with it, to the cell before; else a
        // slot right after the cursor's cell goes and the cursor stays.
        let (doomed, land) = match (empty(here), empty(here + 1)) {
            (Some(x), _) => (x.syllable, here.saturating_sub(1)),
            (None, Some(x)) => (x.syllable, here),
            _ => return,
        };
        self.chant_cell_op(move |chant, _, _| {
            chant.remove_syllable(doomed)?;
            Ok(Some(Target::Cell(land)))
        });
    }

    /// A division key (`,` `;` `:` or `:chant-division`): on a note it goes into the note's own
    /// parentheses (`(g::)`), on an empty group it makes a division of its own (`(:)`), on a
    /// division it changes that one. The same key again takes it off; with `cycle`, `:` goes
    /// between the single and the double.
    fn chant_division(&mut self, sign: String, cycle: bool) {
        // Insert mode keeps going: a division entered in an empty group gets an empty group
        // after it, as a note does.
        let insert = self.mode == crate::config::Mode::Insert && !self.ed.replace;
        self.chant_cell_op(move |chant, _, cell| {
            let was_empty = cell.is_none_or(|c| c.kind == CellKind::Slot);
            let si = match cell {
                Some(c) => c.syllable,
                None => chant.insert_slot(chant.syllables.len()),
            };
            let now = chant.group_division(si).map(String::from);
            // `:` goes round single, double, none.
            let new = if cycle && sign == ":" {
                match now.as_deref() {
                    Some(":") => Some("::".to_string()),
                    Some("::") => None,
                    _ => Some(":".to_string()),
                }
            } else if now.as_deref() == Some(sign.as_str()) {
                None
            } else {
                Some(sign)
            };
            chant.set_group_division(si, new.as_deref())?;
            // Taking a division of its own off in Insert mode takes the empty group that
            // followed it too.
            if insert
                && new.is_none()
                && cell.is_some_and(|c| c.kind == CellKind::Division)
                && chant
                    .syllables
                    .get(si + 1)
                    .is_some_and(|s| s.is_slot() && s.text.is_empty())
            {
                chant.remove_syllable(si + 1)?;
            }
            if insert && was_empty && new.is_some() {
                // The division ends its line in the text, so the group after it starts one.
                let next = chant.insert_slot(si + 1);
                chant.syllables[next].word_start = true;
            }
            Ok(Some(match cell.and_then(|c| c.note) {
                Some(n) => Target::Note(n),
                None => Target::Syllable(si),
            }))
        });
    }

    /// Puts the cursor on the staff line of the note it is on.
    pub(crate) fn chant_line_follows_note(&mut self) {
        if !self.cursor_on_chant() {
            return;
        }
        let pitch = self
            .chant_cursor_note()
            .and_then(|n| {
                self.buffer().score.tracks[self.pane().cursor.track]
                    .chant
                    .as_deref()?
                    .note(n)
            })
            .map(|n| n.pitch);
        if let Some(p) = pitch {
            self.pane_mut().cursor.line = p as i32;
        }
    }

    /// Inserts a note next to the cursor note (or the last one before the cursor): on the staff
    /// line the cursor is on (`on_row`) or at the pitch of that note.
    fn chant_insert(&mut self, after: bool, join: Join, on_row: bool) {
        let line = self.pane().cursor.line.clamp(0, 14) as u8;
        let near = self.chant_cursor_note();
        self.chant_op(move |chant, _| {
            let pitch = if on_row {
                line
            } else {
                near.and_then(|n| chant.note(n)).map_or(6, |n| n.pitch)
            };
            chant
                .insert_note(near.unwrap_or(0), after, join, pitch)
                .map(Some)
        });
    }

    /// The Visual selection as `(from, to)` beats, leaving Visual mode.
    fn take_selection(&mut self) -> Option<((usize, usize), (usize, usize))> {
        let (a, b, _) = self.selection()?;
        self.run(Command::Normal);
        let (a, b) = ((a.bar, a.beat), (b.bar, b.beat));
        Some((a.min(b), a.max(b)))
    }

    /// `:lyrics-delete`: the text of the syllables of the Visual selection (else the cursor
    /// syllable) goes, the notes stay.
    fn lyrics_delete(&mut self) {
        if !self.require_chant() {
            return;
        }
        let span = self.take_selection();
        self.chant_cell_op(move |chant, _, cell| {
            let mut doomed: Vec<usize> = match span {
                Some((from, to)) => transcribe(chant)
                    .cells
                    .iter()
                    .filter(|c| (c.bar, c.beat + c.len - 1) >= from && (c.bar, c.beat) <= to)
                    .map(|c| c.syllable)
                    .collect(),
                None => vec![cell.ok_or("not on a syllable")?.syllable],
            };
            doomed.dedup();
            doomed.retain(|&si| !chant.syllables[si].text.is_empty());
            if doomed.is_empty() {
                return Err("no lyrics there".into());
            }
            for si in doomed {
                chant.set_text_of_syllable(si, "")?;
            }
            Ok(None)
        });
    }

    /// `:lyrics-shift N`: the notes of the syllables of the Visual selection (else from the
    /// cursor syllable to the end of the chant) move `by` syllables on, the lyrics staying.
    fn lyrics_shift(&mut self, by: isize) {
        if !self.require_chant() {
            return;
        }
        let span = self.take_selection();
        self.chant_cell_op(move |chant, _, cell| {
            let (from, to) = match span {
                Some((from, to)) => {
                    let t = transcribe(chant);
                    let mut s = t
                        .cells
                        .iter()
                        .filter(|c| (c.bar, c.beat + c.len - 1) >= from && (c.bar, c.beat) <= to)
                        .map(|c| c.syllable);
                    let first = s.next().ok_or("nothing selected")?;
                    (first, s.next_back().unwrap_or(first))
                }
                None => (
                    cell.ok_or("not on a syllable")?.syllable,
                    chant.syllables.len(),
                ),
            };
            chant.shift_notes(from, to, by)?;
            Ok(None)
        });
    }

    /// `:chant-join`: the syllables of the Visual selection (else the cursor syllable and the
    /// next one with notes) become one: the notes go to the first, the text stays.
    fn chant_join(&mut self) {
        if !self.require_chant() {
            return;
        }
        let span = self.take_selection();
        self.chant_cell_op(move |chant, _, cell| {
            let (first, last) = match span {
                Some((from, to)) => {
                    let t = transcribe(chant);
                    let mut s = t
                        .cells
                        .iter()
                        .filter(|c| (c.bar, c.beat + c.len - 1) >= from && (c.bar, c.beat) <= to)
                        .map(|c| c.syllable);
                    let first = s.next().ok_or("nothing selected")?;
                    (first, s.next_back().unwrap_or(first))
                }
                None => {
                    let first = cell.ok_or("not on a syllable")?.syllable;
                    let next = (first + 1..chant.syllables.len())
                        .find(|&i| chant.syllables[i].notes().next().is_some())
                        .ok_or("no syllable after this one")?;
                    (first, next)
                }
            };
            chant.join_syllables(first, last)?;
            Ok(Some(Target::Syllable(first)))
        });
    }

    /// `:chant-split`: the syllable splits after the cursor note (after each selected note of
    /// the Visual selection): what follows goes to a new syllable without text.
    fn chant_split(&mut self) {
        if !self.require_chant() {
            return;
        }
        let span = self.take_selection();
        self.chant_cell_op(move |chant, at, _| {
            let (first, last) = match span {
                Some((from, to)) => {
                    let t = transcribe(chant);
                    let notes = t
                        .cells
                        .iter()
                        .filter(|c| (c.bar, c.beat + c.len - 1) >= from && (c.bar, c.beat) <= to)
                        .filter_map(|c| c.note);
                    let (lo, hi) =
                        notes.fold((usize::MAX, 0), |(lo, hi), n| (lo.min(n), hi.max(n)));
                    if lo > hi {
                        return Err("no notes selected".into());
                    }
                    (lo, hi)
                }
                None => {
                    let n = at.ok_or("not on a note")?;
                    (n, n)
                }
            };
            // Last first: splitting leaves the order of the notes as it was.
            let mut split = false;
            let mut error = String::new();
            for n in (first..=last).rev() {
                match chant.split_syllable_after(n) {
                    Ok(()) => split = true,
                    Err(e) => error = e,
                }
            }
            if !split {
                return Err(error);
            }
            Ok(Some(Target::Note(first)))
        });
    }

    /// `z` and `Z`: the forced line break after the cursor syllable goes in or out.
    fn chant_line_break(&mut self, sign: String) {
        self.chant_cell_op(move |chant, _, cell| {
            let si = cell.ok_or("not on a syllable")?.syllable;
            chant.toggle_line_break(si, &sign)?;
            Ok(None)
        });
    }

    /// `/` and `!`: the separator after the cursor note goes in or out.
    fn chant_separator(&mut self, sign: String) {
        self.chant_op(move |chant, at| {
            let n = at.ok_or("not on a note")?;
            chant.toggle_separator(n, &sign)?;
            Ok(Some(n))
        });
    }

    /// The cells in the beats `from` to `to` as groups to put back: a syllable all of whose
    /// cells are there goes whole, lyrics and all; of one only partly there (some notes of a
    /// neume) the notes are a group without lyrics.
    pub(crate) fn chant_clip(&self, from: (usize, usize), to: (usize, usize)) -> Vec<Syllable> {
        let c = self.pane().cursor;
        let Some(chant) = self
            .buffer()
            .score
            .tracks
            .get(c.track)
            .and_then(|t| t.chant.as_deref())
        else {
            return Vec::new();
        };
        covered(chant, from, to)
            .into_iter()
            .filter_map(|cv| {
                if cv.whole {
                    return chant.syllables.get(cv.syllable).cloned();
                }
                let items: Vec<NeumeItem> = cv
                    .notes
                    .iter()
                    .filter_map(|&n| chant.note(n).cloned().map(NeumeItem::Note))
                    .collect();
                (!items.is_empty()).then(|| Syllable {
                    items,
                    ..Syllable::default()
                })
            })
            .collect()
    }

    /// `p` and `P` on a chant: the groups of the register go in after (before) the cursor
    /// cell's syllable, `count` times over, and the cursor goes to the first.
    pub(crate) fn chant_paste(&mut self, groups: Vec<Syllable>, before: bool, count: usize) {
        self.chant_cell_op(move |chant, _, cell| {
            let at = match cell {
                Some(c) => c.syllable + usize::from(!before),
                None => chant.syllables.len(),
            };
            for (k, g) in std::iter::repeat_n(groups, count).flatten().enumerate() {
                chant.syllables.insert(at + k, g);
            }
            chant.assign_ids();
            Ok(Some(Target::Syllable(at)))
        });
    }

    /// Deletes what stands in the beats `from` to `to` (a Visual selection, `d` with a motion):
    /// a syllable all of whose cells are there goes whole, lyrics and all; of one only partly
    /// there the notes go and the lyrics stay with the others.
    pub(crate) fn chant_delete_range(&mut self, from: (usize, usize), to: (usize, usize)) {
        self.chant_delete_cells(from, to, false);
    }

    /// [`Self::chant_delete_range`]; `back` puts the cursor on the cell before what went (`x`),
    /// else on the one that took its place.
    fn chant_delete_cells(&mut self, from: (usize, usize), to: (usize, usize), back: bool) {
        self.chant_cell_op(move |chant, _, _| {
            let t = transcribe(chant);
            let first = t
                .cells
                .iter()
                .position(|c| (c.bar, c.beat + c.len - 1) >= from && (c.bar, c.beat) <= to)
                .ok_or("nothing there")?;
            // Last first, so the places of the others stay.
            for cv in covered(chant, from, to).into_iter().rev() {
                if cv.whole {
                    chant.remove_syllable(cv.syllable)?;
                } else {
                    for &n in cv.notes.iter().rev() {
                        chant.delete_note(n)?;
                    }
                }
            }
            Ok(Some(Target::Cell(if back {
                first.saturating_sub(1)
            } else {
                first
            })))
        });
    }

    /// Keys and commands that edit a modern score's bars, on a chant score: those that mean
    /// something in chant do it with a chant operation, the others say "not in chant". True if
    /// the command was one of them.
    fn chant_bar_command(&mut self, cmd: &Command) -> bool {
        use Command::*;
        let refuse = |app: &mut App, what: &str| {
            app.error(format!("{what}: not in chant"));
        };
        match cmd {
            PitchUp | PitchDown | OctaveUp | OctaveDown => {
                let delta = match cmd {
                    PitchUp => 1,
                    PitchDown => -1,
                    OctaveUp => 7,
                    _ => -7,
                };
                self.chant_op(move |chant, at| {
                    let n = at.ok_or("no note under the cursor")?;
                    chant.move_note(n, delta)?;
                    Ok(Some(n))
                });
            }
            Accidental(a) => {
                let want = match a {
                    -1 => Alter::Flat,
                    0 => Alter::Natural,
                    1 => Alter::Sharp,
                    _ => {
                        refuse(self, "double accidental");
                        return true;
                    }
                };
                self.chant_alter(want);
            }
            // A dot is the mora; a longer or shorter note is a horizontal episema.
            Dot | DoubleDot => {
                let m = if matches!(cmd, Dot) { "." } else { ".." };
                self.chant_note_op(move |chant, n| toggle_modifier(chant, n, m));
            }
            DurationLonger | DurationShorter => {
                let on = matches!(cmd, DurationLonger);
                self.chant_note_op(move |chant, n| chant.set_episema(n, on));
            }
            DeleteNote | DeleteBeat => {
                // What is deleted goes in the register, as with `d`.
                let at = self.pane().cursor;
                let (from, to) = ((at.bar, at.beat), (at.bar, at.beat));
                let clip = self.chant_clip(from, to);
                if !clip.is_empty() {
                    self.store(crate::editor::Clip::Chant(clip));
                }
                self.chant_delete_cells(from, to, true);
            }
            // A beat of a chant is an empty group `()`, where a note or a division goes.
            InsertNext => self.chant_enter(),
            BeatNew => self.chant_new_slot(BeatAt::After),
            InsertEnter => self.chant_enter(),
            InsertBeat(at) => {
                self.ed.group = Some(false);
                self.chant_line_follows_note();
                self.chant_new_slot(*at);
                self.enter_insert();
            }
            Duration(_) => refuse(self, "duration"),
            Tuplet(_) => refuse(self, "tuplet"),
            Rest => refuse(self, "rest"),
            Toggle(what) => refuse(self, &format!("toggle {what}")),
            Dynamic(_) | DynamicStep(_) => refuse(self, "dynamics"),
            Beams(_) => refuse(self, "beams"),
            Stems(_) => refuse(self, "stems"),
            NoteLetter(_) | Note(_) => refuse(self, "note entry by pitch name"),
            Slash => refuse(self, "slash"),
            Reflow => refuse(self, "reflow"),
            // Only what `d` or `y` took from a chant goes in a chant.
            Put { .. }
                if matches!(
                    self.ed.registers.get(&self.ed.register.unwrap_or('"')),
                    Some(crate::editor::Clip::Chant(_))
                ) =>
            {
                return false;
            }
            Put { .. } => refuse(self, "paste"),
            BarInsert { .. } | BarOpen { .. } | BarDelete => refuse(self, "bar edit"),
            _ => return false,
        }
        true
    }

    pub(crate) fn run_chant(&mut self, cmd: &Command) -> bool {
        use Command::*;
        if self.cursor_on_chant() && self.chant_bar_command(cmd) {
            return true;
        }
        match cmd {
            // Leaving Insert mode drops an empty group nothing was entered in.
            Normal
                if self.mode == crate::config::Mode::Insert
                    && self.cursor_on_chant()
                    && !self.ed.replace =>
            {
                self.chant_drop_slot();
                return false;
            }
            ChantMod(m) => {
                let m = m.clone();
                self.chant_note_op(move |chant, n| toggle_modifier(chant, n, &m));
            }
            ChantShape(name) => {
                let Some(shape) = shape_by_name(name) else {
                    return true;
                };
                self.chant_note_op(move |chant, n| {
                    let now = chant.note(n).ok_or("no note under the cursor")?.shape();
                    chant.set_shape(n, if now == shape { Shape::Punctum } else { shape })
                });
            }
            ChantDivision(sign) => self.chant_division(sign.clone(), false),
            ChantAlter(name) => {
                let want = match name.as_str() {
                    "flat" => Alter::Flat,
                    "sharp" => Alter::Sharp,
                    _ => Alter::Natural,
                };
                self.chant_alter(want);
            }
            ChantLiquescence(name) => self.chant_toggle(match name.as_str() {
                "ascendens" => "<",
                "descendens" => ">",
                _ => "~",
            }),
            ChantMora => self.chant_toggle("."),
            ChantEpisema => self.chant_toggle("_"),
            ChantIctus => self.chant_toggle("'"),
            ChantSymbol(symbol) => {
                // The liturgical versicle and response signs are GABC specials.
                let text = match symbol.as_str() {
                    "℣" => "<sp>V/</sp>",
                    "Ⱥ" => "<sp>A/</sp>",
                    "℟" => "<sp>R/</sp>",
                    other => other,
                }
                .to_string();
                self.chant_op(move |chant, at| {
                    let n = at.ok_or("no note under the cursor")?;
                    let si = chant.syllable_of(n).ok_or("no syllable under the cursor")?;
                    let now = chant.syllables[si].text.clone();
                    let new = match now.strip_suffix(text.as_str()) {
                        Some(rest) => rest.to_string(),
                        None => format!("{now}{text}"),
                    };
                    chant.set_syllable_text(n, &new)?;
                    Ok(Some(n))
                });
            }
            LyricsSelect => self.lyrics_select(),
            LyricMode => self.lyric_mode(),
            ScoreKind => {
                let msg = if self.cursor_on_chant() {
                    "chant score; :chant-to-modern opens it as a modern score in a new buffer"
                } else {
                    "modern score; :modern-to-chant opens the cursor track's melody as a chant score in a new buffer"
                };
                self.info(msg);
            }
            ChantAppend => self.chant_insert(true, Join::Neume, false),
            ChantBreak => self.chant_insert(true, Join::NewNeume, true),
            ChantSyllable => self.chant_insert(true, Join::NewSyllable, true),
            ChantMarkup(tag) => {
                let tag = tag.clone();
                self.chant_cell_op(move |chant, _, cell| {
                    let si = cell.ok_or("not on a syllable")?.syllable;
                    let text = chant
                        .syllables
                        .get(si)
                        .ok_or("no such syllable")?
                        .text
                        .clone();
                    chant.set_text_of_syllable(
                        si,
                        &cecilia_core::chant::toggle_markup(&text, &tag),
                    )?;
                    Ok(None)
                });
            }
            LyricsDelete => self.lyrics_delete(),
            LyricsShift(by) => self.lyrics_shift(*by),
            ChantCustos => self.chant_note_op(|chant, n| chant.set_custos(n)),
            ChantLineBreak(sign) => self.chant_line_break(sign.clone()),
            ChantJoin => self.chant_join(),
            ChantSplit => self.chant_split(),
            ChantSeparator(sign) => self.chant_separator(sign.clone()),
            ChantBar(sign) => self.chant_division(sign.clone(), true),
            Gabc(arg) => {
                if !self.require_chant() {
                    return true;
                }
                let Some(si) = self
                    .chant_cursor_note()
                    .and_then(|n| {
                        let c = self.pane().cursor;
                        self.buffer().score.tracks[c.track]
                            .chant
                            .as_deref()?
                            .syllable_of(n)
                    })
                    .or_else(|| {
                        let c = self.pane().cursor;
                        self.buffer().score.tracks[c.track]
                            .chant
                            .as_deref()?
                            .syllables
                            .iter()
                            .position(|s| s.notes().next().is_some())
                    })
                else {
                    self.error("gabc: no syllable under the cursor");
                    return true;
                };
                let Some(text) = arg.as_deref().map(str::trim).filter(|a| !a.is_empty()) else {
                    let s = self.buffer().score.tracks[self.pane().cursor.track]
                        .chant
                        .as_ref()
                        .unwrap()
                        .syllables[si]
                        .gabc();
                    self.info(s);
                    return true;
                };
                let replacement = if text.contains('(') {
                    match cecilia_formats::gabc::parse(text) {
                        Ok(c) if !c.syllables.is_empty() => Some(c.syllables),
                        Ok(_) => None,
                        Err(e) => {
                            self.error(format!("gabc: {e}"));
                            return true;
                        }
                    }
                } else {
                    None
                };
                let notes = cecilia_formats::gabc::parse_notes(text, 1);
                self.edit(|cx| {
                    let t = cx.cursor.track;
                    let score = cx.whole();
                    let chant = score.tracks[t].chant.as_mut().unwrap();
                    let old = (**chant).clone();
                    match replacement {
                        Some(mut syllables) => {
                            syllables[0].word_start = chant.syllables[si].word_start;
                            chant.syllables.splice(si..=si, syllables);
                        }
                        None => chant.syllables[si].items = notes,
                    }
                    // Notes the replacement leaves as they were keep their ids (and sync).
                    chant.match_ids(&old);
                    retranscribe(score, t);
                    Ok(())
                });
            }
            Interpretation(arg) => {
                if !self.require_chant() {
                    return true;
                }
                let value = match arg.as_deref().map(str::trim) {
                    None | Some("") => {
                        let t = self.pane().cursor.track;
                        let i = self.buffer().score.tracks[t]
                            .chant
                            .as_ref()
                            .unwrap()
                            .interpretation;
                        self.info(format!(
                            "interpretation: {}",
                            format!("{i:?}").to_lowercase()
                        ));
                        return true;
                    }
                    Some("equalist") => cecilia_core::chant::Interpretation::Equalist,
                    Some("solesmes") => cecilia_core::chant::Interpretation::Solesmes,
                    Some(other) => {
                        self.error(format!(
                            "interpretation: unknown '{other}' (equalist, solesmes)"
                        ));
                        return true;
                    }
                };
                self.edit(|cx| {
                    let t = cx.cursor.track;
                    let score = cx.whole();
                    score.tracks[t].chant.as_mut().unwrap().interpretation = value;
                    retranscribe(score, t);
                    Ok(())
                });
            }
            ChantClef(spec) => {
                if !self.require_chant() {
                    return true;
                }
                let Some(clef) = chant::ChantClef::parse(spec.trim()) else {
                    self.error(format!(
                        "clef: '{spec}' is not a chant clef (c1–c4, f1–f4, cb3)"
                    ));
                    return true;
                };
                self.chant_op(move |chant, at| {
                    chant.set_initial_clef(clef)?;
                    Ok(at)
                });
            }
            ChantToModern => {
                if !self.require_chant() {
                    return true;
                }
                let Some(new) = self.buffer().score.chant_to_modern() else {
                    return true;
                };
                let name = format!("[modern] {}", self.buffer().name);
                self.open_new_buffer(name, new);
                self.info(
                    "modern score (the chant's notes as a staff track, its syllables as lyrics); \
                     the chant is unchanged; :w name.ccm to save",
                );
            }
            ModernToChant => {
                let c = self.pane().cursor;
                let policy = self.chord_policy();
                let score = &self.buffer().score;
                match cecilia_convert::chant::modern_to_chant(score, c.track, policy) {
                    Ok(r) => {
                        let mut new = cecilia_formats::gabc::chant_score(r.chant.clone());
                        new.meta.title = score.meta.title.clone();
                        // The audio stays in place: each bar point is a point on the chant
                        // note that starts the bar.
                        if let Some(a) = &score.audio {
                            let ids = new.tracks[0].chant.as_deref().unwrap().note_ids();
                            let mut audio = a.clone();
                            audio.chant_points = r
                                .note_points(&a.points)
                                .into_iter()
                                .filter_map(|(k, frame)| {
                                    Some(cecilia_core::ChantSyncPoint {
                                        note: *ids.get(k)?,
                                        frame,
                                    })
                                })
                                .collect();
                            audio.points.clear();
                            new.audio = Some(audio);
                            new.refresh_chant_sync();
                        }
                        let name = format!("[chant] {}", self.buffer().name);
                        let left_out = self.buffer().score.tracks.len() - 1;
                        self.open_new_buffer(name, new);
                        let others = if left_out > 0 {
                            format!(
                                "; only the cursor track was converted, {left_out} other track{} left out",
                                if left_out == 1 { "" } else { "s" }
                            )
                        } else {
                            String::new()
                        };
                        if r.chant.note_count() == 0 {
                            self.info(format!(
                                "empty chant: i inserts a beat, Enter puts a note in it{others}; :w name.gabc to save"
                            ));
                            return true;
                        }
                        self.info(format!(
                            "chant (lossy): transposed {:+} semitones, {} notes rounded, {} chords reduced{others}; :w name.gabc to save",
                            r.transpose, r.rounded, r.chords
                        ));
                    }
                    Err(e) => self.error(format!("modern-to-chant: {e}")),
                }
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use crate::keys::Key;
    use cecilia_core::TrackKind;

    const TEXT: &str = "name: t;\n%%\n(c4) Al(fg)le(h) lu(g)ia.(f.) (::)\n";

    fn app_of(text: &str) -> App {
        let mut app = App::new(Config::default(), None);
        let chant = cecilia_formats::gabc::parse(text).unwrap();
        app.buffers[0].score = cecilia_formats::gabc::chant_score(chant);
        app
    }

    fn app() -> App {
        app_of(TEXT)
    }

    fn keys(app: &mut App, s: &str) {
        for k in crate::keys::parse_keys(s, &[Key::char(' ')]).unwrap() {
            app.handle_key(k);
        }
    }

    fn chant_of(app: &App) -> &Chant {
        app.buffer().score.tracks[0].chant.as_deref().unwrap()
    }

    /// The chant as GABC, syllable by syllable.
    fn gabc(app: &App) -> String {
        chant_of(app)
            .syllables
            .iter()
            .map(|s| s.gabc())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The GABC file the chant would be written as, without its header.
    fn written(app: &App) -> String {
        let text = cecilia_formats::gabc::write(chant_of(app), "");
        text.split_once("%%\n").unwrap().1.trim().to_string()
    }

    /// The bars are the chant's transcription, always.
    fn assert_cache(app: &App) {
        let score = &app.buffer().score;
        let t = transcribe(chant_of(app));
        assert_eq!(score.tracks[0].bars, t.bars);
        assert_eq!(score.master_bars, t.master_bars);
    }

    /// Whether the buffer's score is `score` (compared as chant and bars, to keep failures short).
    fn same(app: &App, score: &cecilia_core::Score) -> bool {
        let now = &app.buffer().score;
        now.master_bars == score.master_bars
            && now.tracks.len() == score.tracks.len()
            && now.tracks[0].chant == score.tracks[0].chant
            && now.tracks[0].bars == score.tracks[0].bars
    }

    fn pitches(app: &App) -> Vec<u8> {
        app.buffer().score.tracks[0]
            .bars
            .iter()
            .flat_map(|b| &b.voices[0].beats)
            .filter(|b| !b.empty)
            .map(|b| b.notes[0].fret)
            .collect()
    }

    fn cursor_note(app: &App) -> Option<usize> {
        app.chant_cursor_note()
    }

    fn at(app: &App) -> (usize, usize) {
        let c = app.pane().cursor;
        (c.bar, c.beat)
    }

    fn go(app: &mut App, bar: usize, beat: usize) {
        let id = app.focused();
        let p = app.panes.get_mut(&id).unwrap();
        p.cursor.bar = bar;
        p.cursor.beat = beat;
    }

    fn run(app: &mut App, line: &str) {
        app.run(Command::parse(line).unwrap());
    }

    fn undo_steps(app: &App) -> usize {
        let mut history = app.buffer().history.clone();
        let mut score = app.buffer().score.clone();
        let mut n = 0;
        while history.undo(&mut score).is_some() {
            n += 1;
        }
        n
    }

    #[test]
    fn w_e_b_walk_the_notes_on_the_cursors_staff_position() {
        let mut a = app();
        let at = |a: &App| {
            let c = a.pane().cursor;
            (c.beat, c.line)
        };
        // Notes: f g h g f (beats 0..4); positions f=5 g=6 h=7. In Normal mode the cursor
        // stands on the staff line of the note it moves to, and w e b stay on that line.
        keys(&mut a, "l");
        assert_eq!(at(&a), (1, 6));
        keys(&mut a, "w");
        assert_eq!(at(&a), (3, 6), "the next g");
        keys(&mut a, "b");
        assert_eq!(at(&a), (1, 6), "the previous g");
        keys(&mut a, "e");
        assert_eq!(at(&a), (3, 6));
        // j and k choose another position; w goes to the next note there.
        keys(&mut a, "0k");
        assert_eq!(at(&a), (0, 6));
        keys(&mut a, "w");
        assert_eq!(at(&a), (1, 6));
        keys(&mut a, "ll");
        assert_eq!(at(&a), (3, 6));
    }

    #[test]
    fn insert_mode_moves_along_and_leaves_the_staff_line_alone() {
        let mut a = app();
        // Normal mode: h and l put the cursor on the line of each note.
        keys(&mut a, "l");
        assert_eq!(a.pane().cursor.line, 6);
        // Insert mode: they just move; the line is where j k put it. So does an empty beat.
        keys(&mut a, "i");
        assert_eq!(a.chant_cursor_cell().unwrap().kind, CellKind::Slot);
        keys(&mut a, "kkh");
        assert_eq!(a.pane().cursor.line, 8);
        keys(&mut a, "ll");
        assert_eq!(a.pane().cursor.line, 8);
        keys(&mut a, "<Esc>");
    }

    const ORIGINAL: &str = "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)";

    #[test]
    fn i_a_capital_i_and_capital_a_make_an_empty_beat() {
        let mut a = app();
        // Before the cursor beat; the empty beat is a cell of its own and Esc takes it back.
        keys(&mut a, "i");
        assert_eq!(gabc(&a), "(c4) () Al(fg) le(h) lu(g) ia.(f.) (::)");
        assert_eq!(a.mode, crate::config::Mode::Insert);
        assert_eq!(a.chant_cursor_cell().unwrap().kind, CellKind::Slot);
        assert_cache(&a);
        keys(&mut a, "<Esc>");
        assert_eq!(gabc(&a), ORIGINAL);
        assert_eq!(cursor_note(&a), Some(0));
        // After the cursor note's group.
        keys(&mut a, "a");
        assert_eq!(gabc(&a), "(c4) Al(fg) () le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, "<Esc>");
        // At the start and the end of the bar (before the division that closes it).
        keys(&mut a, "I");
        assert_eq!(gabc(&a), "(c4) () Al(fg) le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, "<Esc>");
        keys(&mut a, "A");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) lu(g) ia.(f.) () (::)");
        keys(&mut a, "<Esc>");
        assert_eq!(gabc(&a), ORIGINAL);
        assert_cache(&a);
    }

    #[test]
    fn insert_mode_puts_a_note_in_the_empty_group_and_adds_the_next_one_after_it() {
        let mut a = app();
        // `<CR>` and `<Space>` put a punctum on the cursor's row in the empty group, and a new
        // empty group follows it; the cursor stays on the note.
        keys(&mut a, "ikk<CR>");
        assert_eq!(gabc(&a), "(c4) (h) () Al(fg) le(h) lu(g) ia.(f.) (::)");
        assert_eq!(a.chant_cursor_cell().unwrap().kind, CellKind::Note);
        assert_eq!(cursor_note(&a), Some(0));
        // On the note, `<Space>` is the same as `<CR>`: another row moves it, its own row
        // repeats it.
        keys(&mut a, "jj<Space>");
        assert_eq!(gabc(&a), "(c4) (f) () Al(fg) le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, "<CR>");
        assert_eq!(gabc(&a), "(c4) (ff) () Al(fg) le(h) lu(g) ia.(f.) (::)");
        // On to the empty group (the cursor moves only when asked to) for the next note.
        keys(&mut a, "l<CR>");
        assert_eq!(a.chant_cursor_cell().unwrap().kind, CellKind::Note);
        assert_eq!(gabc(&a), "(c4) (ff) (f) () Al(fg) le(h) lu(g) ia.(f.) (::)");
        // No space is written before the groups entered in the score.
        assert_eq!(written(&a), "(c4)(ff)(f)() Al(fg)le(h) lu(g)ia.(f.) (::)");
        // Esc drops the empty group after the note; the session is one undo step.
        keys(&mut a, "<Esc>");
        assert_eq!(gabc(&a), "(c4) (ff) (f) Al(fg) le(h) lu(g) ia.(f.) (::)");
        assert_cache(&a);
        keys(&mut a, "u");
        assert_eq!(gabc(&a), ORIGINAL);
    }

    #[test]
    fn replace_mode_changes_the_notes_there_and_adds_no_groups() {
        let mut a = app();
        keys(&mut a, "R");
        assert_eq!(a.mode, crate::config::Mode::Insert);
        assert_eq!(a.statusline(None, false).left[0].text, "REPLACE");
        // Its color is the theme's red; Insert mode has its own.
        assert_eq!(a.mode_color(), a.config.theme.error);
        keys(&mut a, "<Esc>i");
        assert_eq!(a.mode_color(), a.config.theme.mode_insert);
        keys(&mut a, "<Esc>R");
        // On a note another row moves it, with its modifiers.
        run(&mut a, "chant-mod .");
        keys(&mut a, "kk<CR>");
        assert_eq!(gabc(&a), "(c4) Al(h.g) le(h) lu(g) ia.(f.) (::)");
        // On its own row: the same note again in the same group; the cursor goes to it.
        keys(&mut a, "<CR>");
        assert_eq!(gabc(&a), "(c4) Al(h.hg) le(h) lu(g) ia.(f.) (::)");
        assert_eq!(cursor_note(&a), Some(1));
        // `<Space>` is `<CR>`: the cursor stays and no group is added.
        keys(&mut a, "<Space>");
        assert_eq!(gabc(&a), "(c4) Al(h.hhg) le(h) lu(g) ia.(f.) (::)");
        assert_eq!(cursor_note(&a), Some(2));
        // Leaving takes nothing away and is one undo step.
        keys(&mut a, "<Esc>");
        assert_eq!(a.mode, crate::config::Mode::Normal);
        assert!(!gabc(&a).contains("()"));
        keys(&mut a, "u");
        assert_eq!(gabc(&a), ORIGINAL);
        assert_cache(&a);
    }

    #[test]
    fn division_keys_go_into_the_note_or_stand_alone_on_an_empty_beat() {
        let mut a = app();
        // On a note: in its parentheses; `:` goes round single, double, none.
        keys(&mut a, "R;");
        assert_eq!(gabc(&a), "(c4) Al(fg;) le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, ":");
        assert_eq!(gabc(&a), "(c4) Al(fg:) le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, ":");
        assert_eq!(gabc(&a), "(c4) Al(fg::) le(h) lu(g) ia.(f.) (::)");
        // A third `:` takes it off, a fourth starts again.
        keys(&mut a, ":");
        assert_eq!(gabc(&a), ORIGINAL);
        keys(&mut a, ":");
        assert_eq!(gabc(&a), "(c4) Al(fg:) le(h) lu(g) ia.(f.) (::)");
        // The same key again takes a `,` or `;` off.
        keys(&mut a, ";");
        assert_eq!(gabc(&a), "(c4) Al(fg;) le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, ";");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)");
        assert_cache(&a);
        keys(&mut a, "<Esc>");
        // On an empty beat: a division of its own, which the cursor can stand on.
        keys(&mut a, "a:");
        // (Insert mode adds the empty group after it, to go on in.)
        assert_eq!(gabc(&a), "(c4) Al(fg) (:) () le(h) lu(g) ia.(f.) (::)");
        assert_eq!(a.chant_cursor_cell().unwrap().kind, CellKind::Division);
        keys(&mut a, ":");
        assert_eq!(gabc(&a), "(c4) Al(fg) (::) () le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, ",");
        assert_eq!(gabc(&a), "(c4) Al(fg) (,) () le(h) lu(g) ia.(f.) (::)");
        assert_cache(&a);
        // Deleting a division of its own, with the cursor on it.
        keys(&mut a, "<Esc>x");
        assert_eq!(gabc(&a), ORIGINAL);
    }

    #[test]
    fn new_takes_modern_or_chant_and_an_empty_score_converts_to_chant() {
        assert_eq!(Command::parse("new"), Ok(Command::New));
        assert_eq!(Command::parse("new modern"), Ok(Command::New));
        assert_eq!(Command::parse("new chant"), Ok(Command::NewChant));
        assert!(Command::parse("new gregorian").is_err());
        let mut a = App::new(Config::default(), None);
        a.run(Command::NewChant);
        assert!(a.buffer().score.is_chant());
        assert_eq!(chant_of(&a).note_count(), 0);
        // Notes go in as in any chant: i for a beat, Enter for a note.
        keys(&mut a, "i<CR><Esc>a<CR><Esc>");
        assert_eq!(chant_of(&a).note_count(), 2, "{}", gabc(&a));
        assert_cache(&a);
        // An empty modern score converts as well.
        let mut a = App::new(Config::default(), None);
        a.run(Command::New);
        a.run(Command::ModernToChant);
        assert!(a.buffer().score.is_chant(), "{:?}", a.last_notification());
        assert_eq!(chant_of(&a).note_count(), 0);
        keys(&mut a, "i<CR><Esc>");
        assert_eq!(chant_of(&a).note_count(), 1);
    }

    #[test]
    fn the_cursor_visits_divisions_of_their_own() {
        let mut a = app_of("name: t;\n%%\n(c4) Al(fg) (;) le(h) (::)\n");
        assert_eq!(a.buffer().score.tracks[0].bars.len(), 2);
        keys(&mut a, "ll");
        assert_eq!(a.chant_cursor_cell().unwrap().kind, CellKind::Division);
        keys(&mut a, "l");
        assert_eq!(cursor_note(&a), Some(2));
    }

    #[test]
    fn a_chant_track_has_no_staves_to_change() {
        let mut a = app();
        let before = a.buffer().score.tracks[0].clone();
        a.run(Command::Notation("treble".into()));
        assert!(a.last_notification().unwrap().error);
        a.run(Command::StaffAdd(Some("tab".into())));
        assert!(a.last_notification().unwrap().error);
        assert_eq!(a.buffer().score.tracks[0], before);
        // `chant` is not a notation of an ordinary track either.
        a.run(Command::ChantToModern);
        a.run(Command::Notation("chant".into()));
        assert!(a.last_notification().unwrap().error);
        assert!(
            a.buffer().score.tracks[0]
                .staves()
                .iter()
                .all(|s| !s.name().contains("chant"))
        );
    }

    #[test]
    fn a_chant_tracks_clef_is_a_chant_clef() {
        let mut a = app();
        a.run(Command::Clef("f3".into()));
        assert!(gabc(&a).starts_with("(f3)"));
        a.run(Command::Clef("treble".into()));
        assert!(a.last_notification().unwrap().error);
        assert!(gabc(&a).starts_with("(f3)"));
        assert_cache(&a);
        // The same notes under another clef sound at other pitches.
        assert_ne!(pitches(&a), pitches(&app()));
    }

    #[test]
    fn pitch_keys_move_notes_on_the_staff() {
        let mut a = app();
        assert_eq!(pitches(&a), [65, 67, 69, 67, 65]);
        // Raise the second note a staff position at a time: g → h → i (B, natural).
        keys(&mut a, "l<C-a>");
        assert_eq!(gabc(&a), "(c4) Al(fh) le(h) lu(g) ia.(f.) (::)");
        assert_eq!(pitches(&a), [65, 69, 69, 67, 65]);
        keys(&mut a, "<C-a>");
        assert_eq!(gabc(&a), "(c4) Al(fi) le(h) lu(g) ia.(f.) (::)");
        assert_eq!(pitches(&a), [65, 71, 69, 67, 65]);
        assert_cache(&a);
        // The cursor stays on the note it moved.
        assert_eq!(cursor_note(&a), Some(1));
        assert_eq!(at(&a), (0, 1));
        assert_eq!(a.pane().cursor.line, 8);
        // Delete the first note, then undo all three edits.
        keys(&mut a, "0x");
        assert_eq!(gabc(&a), "(c4) Al(i) le(h) lu(g) ia.(f.) (::)");
        assert_eq!(at(&a), (0, 0));
        assert_cache(&a);
        keys(&mut a, "uuu");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)");
        assert_cache(&a);
        // An octave is seven positions; the top and the bottom of the staff stop it.
        run(&mut a, "octave-up");
        assert_eq!(gabc(&a), "(c4) Al(fn) le(h) lu(g) ia.(f.) (::)");
        run(&mut a, "octave-up");
        assert!(a.last_notification().unwrap().error);
        assert_eq!(gabc(&a), "(c4) Al(fn) le(h) lu(g) ia.(f.) (::)");
        run(&mut a, "octave-down");
        run(&mut a, "octave-down");
        assert!(a.last_notification().unwrap().error);
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)");
    }

    #[test]
    fn accidentals_and_signs() {
        let mut a = app();
        go(&mut a, 0, 2);
        // A flat on the note, a second press takes it away; B♭ sounds a semitone lower.
        run(&mut a, "accidental flat");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(hxh) lu(g) ia.(f.) (::)");
        assert_eq!(at(&a), (0, 2));
        run(&mut a, "accidental flat");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)");
        run(&mut a, "accidental natural");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(hyh) lu(g) ia.(f.) (::)");
        run(&mut a, "accidental natural");
        run(&mut a, "accidental double-sharp");
        assert!(a.last_notification().unwrap().text.contains("not in chant"));
        // The flat is on B: raise the note onto it and it sounds flat.
        run(&mut a, "pitch-up");
        run(&mut a, "accidental flat");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(ixi) lu(g) ia.(f.) (::)");
        assert_eq!(pitches(&a)[2], 70);
        assert_cache(&a);
        run(&mut a, "pitch-down");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(hxh) lu(g) ia.(f.) (::)");
        // The mora and the episema keys.
        go(&mut a, 0, 2);
        run(&mut a, "accidental flat");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)");
        go(&mut a, 0, 0);
        run(&mut a, "dot");
        assert_eq!(gabc(&a), "(c4) Al(f.g) le(h) lu(g) ia.(f.) (::)");
        run(&mut a, "dot");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)");
        run(&mut a, "duration-longer");
        assert_eq!(gabc(&a), "(c4) Al(f_g) le(h) lu(g) ia.(f.) (::)");
        run(&mut a, "duration-shorter");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)");
        assert_cache(&a);
    }

    #[test]
    fn syllables_split_and_join_and_separators_toggle() {
        let mut a = app();
        run(&mut a, "chant-sep /");
        assert_eq!(gabc(&a), "(c4) Al(f/g) le(h) lu(g) ia.(f.) (::)");
        run(&mut a, "chant-sep !");
        assert_eq!(gabc(&a), "(c4) Al(f!g) le(h) lu(g) ia.(f.) (::)");
        run(&mut a, "chant-sep !");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)");
        // The syllable splits after the cursor note (f).
        run(&mut a, "chant-split");
        assert_eq!(gabc(&a), "(c4) Al(f) (g) le(h) lu(g) ia.(f.) (::)");
        // Joined again with the next syllable: the notes go to the first, the text stays.
        run(&mut a, "chant-join");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)");
        run(&mut a, "chant-join");
        assert_eq!(gabc(&a), "(c4) Al(fgh) le() lu(g) ia.(f.) (::)");
        // A Visual selection joins every syllable it touches.
        keys(&mut a, "vllll");
        run(&mut a, "chant-join");
        assert_eq!(a.mode, crate::config::Mode::Normal);
        assert_eq!(gabc(&a), "(c4) Al(fghg) le() lu() ia.(f.) (::)");
        assert_cache(&a);
    }

    #[test]
    fn alt_shift_j_and_s_join_and_split_in_normal_insert_and_visual_mode() {
        let mut a = app();
        keys(&mut a, "<A-S>");
        assert_eq!(gabc(&a), "(c4) Al(f) (g) le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, "<A-J>");
        assert_eq!(gabc(&a), ORIGINAL);
        // Insert mode.
        let mut a = app();
        keys(&mut a, "R<A-S>");
        assert_eq!(a.mode, crate::config::Mode::Insert);
        assert_eq!(gabc(&a), "(c4) Al(f) (g) le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, "<A-J>");
        assert_eq!(gabc(&a), ORIGINAL);
        keys(&mut a, "<Esc>");
        // Visual mode: the syllables the selection touches join; the mode is left.
        let mut a = app();
        keys(&mut a, "vlll<A-J>");
        assert_eq!(a.mode, crate::config::Mode::Normal);
        assert_eq!(gabc(&a), "(c4) Al(fghg) le() lu() ia.(f.) (::)");
        // After each selected note.
        let mut a = app();
        keys(&mut a, "vl<A-S>");
        assert_eq!(a.mode, crate::config::Mode::Normal);
        assert_eq!(gabc(&a), "(c4) Al(f) (g) le(h) lu(g) ia.(f.) (::)");
        assert_cache(&a);
    }

    #[test]
    fn lyric_mode_types_the_cursor_syllables_text() {
        let mut a = app();
        keys(&mut a, "<A-i>");
        assert_eq!(a.mode, crate::config::Mode::Insert);
        assert_eq!(a.statusline(None, false).left[0].text, "LYRIC");
        // The text goes on the cursor syllable as it is typed: score and kept GABC follow.
        keys(&mut a, "<BS><BS>Ky");
        assert_eq!(gabc(&a), "(c4) Ky(fg) le(h) lu(g) ia.(f.) (::)");
        assert!(
            chant_of(&a)
                .source
                .as_ref()
                .unwrap()
                .text
                .contains("Ky(fg)")
        );
        keys(&mut a, "ri<Esc>");
        assert_eq!(gabc(&a), "(c4) Kyri(fg) le(h) lu(g) ia.(f.) (::)");
        assert_eq!(a.mode, crate::config::Mode::Normal);
        assert_cache(&a);
        // Notes are not entered while typing, and the mode is one undo step.
        keys(&mut a, "u");
        assert_eq!(gabc(&a), ORIGINAL);
        // A parenthesis is refused.
        keys(&mut a, "<A-i>(<Esc>");
        assert_eq!(gabc(&a), ORIGINAL);
        // `:lyrics` puts all its text on the cursor syllable.
        run(&mut a, "lyrics Kyrie e-le-i-son");
        assert!(
            gabc(&a).starts_with("(c4) Kyrie e-le-i-son(fg)"),
            "{}",
            gabc(&a)
        );
        assert_cache(&a);
    }

    #[test]
    fn a_space_in_lyric_mode_is_just_a_space() {
        let Ok(text) = std::fs::read_to_string("../../test-corpus/gabc/10027.gabc") else {
            eprintln!("skipped: test-corpus is not in git");
            return;
        };
        let mut a = app_of(&text);
        let kept = |a: &App| chant_of(a).source.as_ref().unwrap().text.clone();
        let before = kept(&a);
        // After a word: a space in its text, nothing else in the text moves.
        keys(&mut a, "<A-i>x <Esc>");
        assert_eq!(kept(&a).replacen("Six (", "Si(", 1), before);
        assert_eq!(kept(&a).matches('\n').count(), before.matches('\n').count());
        keys(&mut a, "u");
        // On a syllable without text: the space before it (it starts a word), then its text.
        keys(&mut a, "i<CR><Esc><A-i> ");
        assert!(kept(&a).starts_with("(c4) (g)\nSi"), "{}", kept(&a));
        assert!(chant_of(&a).syllables[1].word_start);
        keys(&mut a, "ab");
        assert!(kept(&a).starts_with("(c4) ab(g)\nSi"), "{}", kept(&a));
        keys(&mut a, "<BS><BS><BS>");
        assert!(kept(&a).starts_with("(c4)(g)\nSi"), "{}", kept(&a));
        assert_eq!(kept(&a).matches('\n').count(), before.matches('\n').count());
    }

    #[test]
    fn tab_and_shift_tab_move_between_syllables_in_lyric_mode() {
        let mut a = app();
        keys(
            &mut a,
            "<A-i><BS><BS>A<Tab>B<Tab>C<Tab><BS>D<S-Tab><S-Tab>E",
        );
        assert_eq!(gabc(&a), "(c4) A(fg) leBE(h) luC(g) iaD(f.) (::)");
        // At the ends the cursor stays.
        keys(&mut a, "<S-Tab><S-Tab>");
        keys(&mut a, "F<Esc>");
        assert!(gabc(&a).starts_with("(c4) AF(fg)"), "{}", gabc(&a));
        assert_eq!(a.mode, crate::config::Mode::Normal);
        assert_eq!(undo_steps(&a), 1);
    }

    #[test]
    fn a_division_of_its_own_gets_a_space_before_and_a_line_break_after_in_the_gabc() {
        let mut a = app();
        let kept = |a: &App| chant_of(a).source.as_ref().unwrap().text.clone();
        // The clef has a line of its own.
        assert_eq!(kept(&a), "(c4)\nAl(fg)le(h) lu(g)ia.(f.) (::)");
        let original = kept(&a);
        // A division on an empty beat: the space before it, the line break after it.
        keys(&mut a, "lla;");
        // (Insert mode adds the empty group after it, to go on in.)
        assert_eq!(kept(&a), "(c4)\nAl(fg)le(h) (;)\n() lu(g)ia.(f.) (::)");
        assert!(chant_of(&a).source.is_some());
        // Taken off again, the white space goes with it.
        keys(&mut a, ";");
        assert_eq!(kept(&a), "(c4)\nAl(fg)le(h)() lu(g)ia.(f.) (::)");
        keys(&mut a, "<Esc>");
        assert_eq!(kept(&a), original);
        // Deleting a division of its own takes the line break too.
        keys(&mut a, "a:<Esc>");
        assert_eq!(kept(&a), "(c4)\nAl(fg)le(h) (:)\nlu(g)ia.(f.) (::)");
        keys(&mut a, "x");
        assert_eq!(kept(&a), original);
        assert_cache(&a);
    }

    #[test]
    fn a_sign_on_a_cell_without_a_note_puts_a_note_there_first() {
        let mut a = app();
        // On an empty beat: a punctum on the cursor's line takes the sign, and (in Insert mode)
        // an empty beat follows it.
        keys(&mut a, "a");
        run(&mut a, "chant-shape virga");
        assert_eq!(gabc(&a), "(c4) Al(fg) (fv) () le(h) lu(g) ia.(f.) (::)");
        assert_eq!(a.chant_cursor_cell().unwrap().kind, CellKind::Note);
        // Again toggles it off, on the note that is there now.
        run(&mut a, "chant-shape virga");
        assert_eq!(gabc(&a), "(c4) Al(fg) (f) () le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, "<Esc>");
        assert_eq!(gabc(&a), "(c4) Al(fg) (f) le(h) lu(g) ia.(f.) (::)");
        // On a division of its own: the empty beat after it takes the note, and another
        // follows.
        keys(&mut a, "a;");
        run(&mut a, "chant-mora");
        assert!(gabc(&a).contains("(;) (f.) () le"), "{}", gabc(&a));
        // In Replace mode no empty beat is added.
        keys(&mut a, "<Esc>");
        keys(&mut a, "R");
        run(&mut a, "chant-ictus");
        assert!(!gabc(&a).contains("()"), "{}", gabc(&a));
        assert_cache(&a);
    }

    #[test]
    fn insert_mode_keys_toggle_shapes_signs_and_line_breaks() {
        let mut a = app();
        for (keys_, expected) in [
            ("RV", "(c4) Al(fVg) le(h) lu(g) ia.(f.) (::)"),
            ("V", "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)"),
            ("O", "(c4) Al(fOg) le(h) lu(g) ia.(f.) (::)"),
            ("O", "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)"),
            ("<lt>", "(c4) Al(f<g) le(h) lu(g) ia.(f.) (::)"),
            ("<lt>>", "(c4) Al(f>g) le(h) lu(g) ia.(f.) (::)"),
            (">", "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)"),
            ("r", "(c4) Al(frg) le(h) lu(g) ia.(f.) (::)"),
            ("r", "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)"),
            ("G", "(c4) Al(Fg) le(h) lu(g) ia.(f.) (::)"),
            ("G", "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)"),
            ("x", "(c4) Al(fxfg) le(h) lu(g) ia.(f.) (::)"),
            ("y", "(c4) Al(fyfg) le(h) lu(g) ia.(f.) (::)"),
            ("#", "(c4) Al(f#fg) le(h) lu(g) ia.(f.) (::)"),
            ("#", "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)"),
        ] {
            keys(&mut a, keys_);
            assert_eq!(gabc(&a), expected, "{keys_}");
        }
        // Forced line breaks: `z` and `Z` toggle, each replacing the other.
        keys(&mut a, "z");
        assert_eq!(gabc(&a), "(c4) Al(fgz) le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, "Z");
        assert_eq!(gabc(&a), "(c4) Al(fgZ) le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, "Z");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)");
        // `+` changes the note into a custos, which is not a note any more.
        let notes = chant_of(&a).note_count();
        keys(&mut a, "+");
        assert_eq!(gabc(&a), "(c4) Al(f+g) le(h) lu(g) ia.(f.) (::)");
        assert_eq!(chant_of(&a).note_count(), notes - 1);
        assert_cache(&a);
    }

    #[test]
    fn markup_tools_wrap_the_syllables_text_and_lyric_mode_types_inside() {
        let mut a = app();
        run(&mut a, "chant-markup i");
        assert_eq!(gabc(&a), "(c4) <i>Al</i>(fg) le(h) lu(g) ia.(f.) (::)");
        run(&mut a, "chant-markup b");
        assert_eq!(
            gabc(&a),
            "(c4) <b><i>Al</i></b>(fg) le(h) lu(g) ia.(f.) (::)"
        );
        run(&mut a, "chant-markup i");
        run(&mut a, "chant-markup b");
        assert_eq!(gabc(&a), ORIGINAL);
        // Typing goes inside the tags, and <BS> on an empty pair takes the pair away.
        keys(&mut a, "<A-i><BS><BS>");
        run(&mut a, "chant-markup alt");
        keys(&mut a, "<A-i>x");
        assert_eq!(gabc(&a), "(c4) <alt>x</alt>(fg) le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, "<BS><BS><Esc>");
        assert_eq!(gabc(&a), "(c4) (fg) le(h) lu(g) ia.(f.) (::)");
        assert_cache(&a);
    }

    #[test]
    fn the_lyric_mode_tool_again_leaves_lyric_mode() {
        let mut a = app();
        let tool = |a: &mut App| {
            let rows = a.palette_rows();
            let at = rows.iter().position(|r| r.command == "lyric-mode").unwrap();
            assert_eq!(rows[at].unavailable, None);
            a.palette.selected = at;
            a.activate_palette();
        };
        let active = |a: &App| {
            a.palette_rows()
                .iter()
                .any(|r| r.command == "lyric-mode" && r.active)
        };
        assert!(!active(&a));
        tool(&mut a);
        assert!(a.ed.lyric && a.mode == crate::config::Mode::Insert);
        assert!(active(&a));
        keys(&mut a, "x");
        tool(&mut a);
        assert!(!a.ed.lyric);
        assert_eq!(a.mode, crate::config::Mode::Normal);
        assert!(!active(&a));
        assert!(gabc(&a).contains("Alx("));
    }

    #[test]
    fn the_barred_a_tool_puts_the_special_in_the_syllables_text() {
        let mut a = app();
        run(&mut a, "chant-symbol Ⱥ");
        assert_eq!(gabc(&a), "(c4) Al<sp>A/</sp>(fg) le(h) lu(g) ia.(f.) (::)");
        run(&mut a, "chant-symbol Ⱥ");
        assert_eq!(gabc(&a), ORIGINAL);
    }

    #[test]
    fn lyrics_delete_clears_the_text_of_the_selected_syllables_and_keeps_the_notes() {
        let mut a = app();
        // The syllables of the selection (Al, le): f g h are the first three notes.
        keys(&mut a, "vll");
        run(&mut a, "lyrics-delete");
        assert_eq!(a.mode, crate::config::Mode::Normal);
        assert_eq!(gabc(&a), "(c4) (fg) (h) lu(g) ia.(f.) (::)");
        assert_eq!(chant_of(&a).note_count(), 5);
        assert_cache(&a);
        keys(&mut a, "u");
        assert_eq!(gabc(&a), ORIGINAL);
        // Without a selection: the cursor syllable's.
        keys(&mut a, "ggll");
        run(&mut a, "lyrics-delete");
        assert_eq!(gabc(&a), "(c4) Al(fg) (h) lu(g) ia.(f.) (::)");
        // Nothing to delete says so.
        run(&mut a, "lyrics-delete");
        assert!(a.last_notification().is_some_and(|m| m.error));
    }

    #[test]
    fn lyrics_shift_moves_the_notes_among_the_syllables_and_shift_delete_deletes() {
        let mut a = app_of("name: t;\n%%\n(c4) Et(g) Je-(hi)sum,(jj) be-()ne-(i)díc(g')tum(h)\n");
        let before = gabc(&a);
        assert_eq!(
            Command::parse("lyrics-shift -2"),
            Ok(Command::LyricsShift(-2))
        );
        assert!(Command::parse("lyrics-shift x").is_err());
        // The empty group of "be-" goes to the end: the notes after it come a syllable earlier,
        // the lyrics stay.
        let at = a.buffer().score.tracks[0]
            .chant
            .as_deref()
            .unwrap()
            .syllables
            .iter()
            .position(|s| s.text == "be-")
            .unwrap();
        // Put the cursor on "be-"'s empty beat.
        let cell = transcribe(chant_of(&a))
            .cells
            .iter()
            .copied()
            .find(|c| c.syllable == at)
            .unwrap();
        a.pane_mut().cursor.bar = cell.bar;
        a.pane_mut().cursor.beat = cell.beat;
        run(&mut a, "lyrics-shift -1");
        assert_eq!(
            gabc(&a),
            "(c4) Et(g) Je-(hi) sum,(jj) be-(i) ne-(g') díc(h) tum()"
        );
        assert_cache(&a);
        keys(&mut a, "u");
        assert_eq!(gabc(&a), before);
        // A selection limits the range: the end of the selection, not the song.
        let cell = transcribe(chant_of(&a))
            .cells
            .iter()
            .copied()
            .find(|c| c.syllable == at)
            .unwrap();
        a.pane_mut().cursor.bar = cell.bar;
        a.pane_mut().cursor.beat = cell.beat;
        keys(&mut a, "vll");
        run(&mut a, "lyrics-shift -1");
        assert_eq!(a.mode, crate::config::Mode::Normal);
        assert!(
            gabc(&a).contains("be-(i) ne-(g') díc() tum(h)"),
            "{}",
            gabc(&a)
        );
        // Shift-Delete is :lyrics-delete, in Visual mode too.
        keys(&mut a, "gg");
        keys(&mut a, "vll<S-Del>");
        assert!(gabc(&a).contains("(g) (hi) "), "{}", gabc(&a));
    }

    #[test]
    fn what_d_takes_from_a_chant_is_put_back_with_p_and_capital_p() {
        let mut a = app();
        // Delete the notes f g (Visual `d`): all of the syllable Al goes, lyrics and all.
        keys(&mut a, "vld");
        assert_eq!(gabc(&a), "(c4) le(h) lu(g) ia.(f.) (::)");
        // `p` puts it back after the cursor cell's syllable.
        keys(&mut a, "p");
        assert_eq!(gabc(&a), "(c4) le(h) Al(fg) lu(g) ia.(f.) (::)");
        assert_eq!(a.chant_cursor_cell().unwrap().kind, CellKind::Note);
        assert_cache(&a);
        // `P` puts it before, and a count repeats it.
        keys(&mut a, "gg");
        keys(&mut a, "2P");
        assert!(
            gabc(&a).starts_with("(c4) Al(fg) Al(fg) le(h)"),
            "{}",
            gabc(&a)
        );
        keys(&mut a, "u");
        keys(&mut a, "u");
        // `y` takes without deleting.
        keys(&mut a, "gg");
        keys(&mut a, "vly");
        assert_eq!(gabc(&a), "(c4) le(h) lu(g) ia.(f.) (::)");
        let n = chant_of(&a).syllables.len();
        keys(&mut a, "p");
        // (le and lu, both whole, go in again after the cursor syllable)
        assert_eq!(chant_of(&a).syllables.len(), n + 2, "{}", gabc(&a));
        // A neume deleted in part goes as a group without lyrics.
        let mut b = app();
        keys(&mut b, "vd");
        keys(&mut b, "p");
        assert_eq!(gabc(&b), "(c4) Al(g) (f) le(h) lu(g) ia.(f.) (::)");
        // Note ids stay unique.
        let c = chant_of(&a);
        let ids: Vec<u32> = c
            .syllables
            .iter()
            .flat_map(|s| s.notes())
            .map(|n| n.id)
            .collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len());
    }

    #[test]
    fn x_and_del_put_the_deleted_cell_in_the_register_for_p() {
        let mut a = app();
        // `x` deletes the note under the cursor (f) and `p` puts it back after the cursor cell.
        keys(&mut a, "x");
        assert_eq!(gabc(&a), "(c4) Al(g) le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, "p");
        assert_eq!(gabc(&a), "(c4) Al(g) (f) le(h) lu(g) ia.(f.) (::)");
        // The next `x` takes the pasted group (f), the cursor goes to the cell before (the g of
        // Al), and `P` puts the group back before its syllable.
        keys(&mut a, "x");
        assert_eq!(gabc(&a), "(c4) Al(g) le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, "P");
        assert_eq!(gabc(&a), "(c4) (f) Al(g) le(h) lu(g) ia.(f.) (::)");
        assert_cache(&a);
    }

    #[test]
    fn shape_and_sign_toggles() {
        let mut a = app();
        for (cmd, expected) in [
            ("chant-mod v", "(c4) Al(fvg) le(h) lu(g) ia.(f.) (::)"),
            ("chant-mod v", "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)"),
            ("chant-mod w", "(c4) Al(fwg) le(h) lu(g) ia.(f.) (::)"),
            ("chant-mod o", "(c4) Al(fog) le(h) lu(g) ia.(f.) (::)"),
            ("chant-mod ss", "(c4) Al(fssg) le(h) lu(g) ia.(f.) (::)"),
            ("chant-mod ss", "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)"),
            ("chant-mod ~", "(c4) Al(f~g) le(h) lu(g) ia.(f.) (::)"),
            ("chant-mod ~", "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)"),
            ("chant-mod '", "(c4) Al(f'g) le(h) lu(g) ia.(f.) (::)"),
            ("chant-mod _", "(c4) Al(f'_g) le(h) lu(g) ia.(f.) (::)"),
            ("chant-mod -", "(c4) Al(-f'_g) le(h) lu(g) ia.(f.) (::)"),
        ] {
            run(&mut a, cmd);
            assert_eq!(gabc(&a), expected, "{cmd}");
            assert_cache(&a);
            assert_eq!(at(&a), (0, 0), "{cmd}");
        }
        // A stropha sounds twice; the cursor beat after it is still its note.
        run(&mut a, "chant-mod ss");
        let sounding = |a: &App| {
            a.buffer().score.tracks[0].bars[0].voices[0]
                .beats
                .iter()
                .filter(|b| !b.empty)
                .count()
        };
        assert_eq!(sounding(&a), 6);
        go(&mut a, 0, 1);
        assert_eq!(cursor_note(&a), Some(0));
        go(&mut a, 0, 2);
        assert_eq!(cursor_note(&a), Some(1));
    }

    #[test]
    fn notes_syllables_and_divisions() {
        let mut a = app_of("name: t;\n%%\n(c4) Al(fg)le(h) (;) lu(g)ia.(f.) (::)\n");
        assert_eq!(a.buffer().score.tracks[0].bars.len(), 2);
        // A new note in the neume of the cursor note; the cursor goes to it.
        go(&mut a, 1, 0);
        run(&mut a, "chant-append");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) (;) lu(gg) ia.(f.) (::)");
        assert_eq!(at(&a), (1, 1));
        assert_eq!(cursor_note(&a), Some(4));
        // A new neume, then a new syllable after the syllable.
        run(&mut a, "chant-break");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) (;) lu(gg/g) ia.(f.) (::)");
        run(&mut a, "chant-syllable");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) (;) lu(gg/g) (g) ia.(f.) (::)");
        assert_eq!(at(&a), (1, 3));
        assert_cache(&a);
        // The GABC written for the file has the same notes.
        let text = written(&a);
        assert!(text.contains("(;) lu(gg/g)(g)ia.(f.)"), "{text}");
        assert!(text.starts_with("(c4)\nAl(fg)le(h)"), "{text}");
        // A division on a note goes into its parentheses; it splits the bar and moves the notes
        // after it; the cursor stays on its note.
        go(&mut a, 0, 1);
        run(&mut a, "chant-bar ,");
        assert_eq!(gabc(&a), "(c4) Al(fg,) le(h) (;) lu(gg/g) (g) ia.(f.) (::)");
        assert_eq!(cursor_note(&a), Some(1));
        assert_eq!(at(&a), (0, 1));
        assert_eq!(a.buffer().score.tracks[0].bars.len(), 3);
        assert_cache(&a);
        // Deleting a syllable's only note takes the syllable, lyrics and all; the cursor goes to
        // the cell before it, even across a bar.
        go(&mut a, 1, 0);
        keys(&mut a, "x");
        assert_eq!(gabc(&a), "(c4) Al(fg,) (;) lu(gg/g) (g) ia.(f.) (::)");
        assert_eq!(cursor_note(&a), Some(1));
        assert_cache(&a);
        // Deleting a syllable's last note removes a syllable with no text.
        keys(&mut a, "0");
        run(&mut a, "chant-syllable");
        let n = chant_of(&a).syllables.len();
        keys(&mut a, "x");
        assert_eq!(chant_of(&a).syllables.len(), n - 1);
        assert_cache(&a);
    }

    #[test]
    fn a_visual_selection_deletes_notes() {
        let mut a = app();
        // The g of Al (only part of its syllable) and all of le: the g goes alone, le goes whole
        // with its lyrics.
        keys(&mut a, "lvld");
        assert_eq!(gabc(&a), "(c4) Al(f) lu(g) ia.(f.) (::)");
        assert_cache(&a);
        keys(&mut a, "u");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)");
        // Changing or pasting over bars is not something chant does.
        keys(&mut a, "vlc");
        assert!(a.last_notification().unwrap().error);
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)");
    }

    #[test]
    fn keys_that_edit_bars_say_not_in_chant() {
        let mut a = app();
        let before = a.buffer().score.clone();
        let steps = undo_steps(&a);
        for cmd in [
            "duration 8",
            "tuplet 3",
            "rest",
            "toggle tie",
            "toggle bend",
            "note-letter c",
            "dynamic f",
            "beams auto",
            "bar-insert-after",
            "bar-delete",
            "put",
            "reflow",
        ] {
            a.clear_notifications();
            let Ok(c) = Command::parse(cmd) else {
                panic!("{cmd} does not parse");
            };
            a.run(c);
            let m = a
                .last_notification()
                .cloned()
                .unwrap_or_else(|| panic!("{cmd}: no message"));
            assert!(
                m.error && m.text.contains("not in chant"),
                "{cmd}: {}",
                m.text
            );
            assert!(same(&a, &before), "{cmd}");
        }
        // Digits do not type frets on a chant track either.
        keys(&mut a, "R1<Esc>");
        assert!(same(&a, &before));
        assert_eq!(undo_steps(&a), steps);
        assert!(!a.buffer().modified);
    }

    #[test]
    fn the_bars_of_a_chant_track_are_never_edited_directly() {
        let mut a = app();
        let before = a.buffer().score.clone();
        // An edit that touches the bars without going through the chant is undone.
        let r = a.edit(|cx| {
            cx.bar(0, 0).voices[0].beats.remove(0);
            Ok(())
        });
        assert!(r.is_none());
        let m = a.last_notification().cloned().unwrap();
        assert!(m.error && m.text.contains("not in chant"), "{}", m.text);
        assert!(same(&a, &before));
        assert_eq!(undo_steps(&a), 0);
        assert!(!a.buffer().modified);
        // So is one that adds a bar, or replaces them.
        a.edit(|cx| {
            cx.whole().master_bars.push(Default::default());
            cx.whole().tracks[0].bars.push(Default::default());
            Ok(())
        });
        assert!(same(&a, &before));
        // An edit that changes the chant but leaves the bars old gets them made again.
        a.edit(|cx| {
            let t = cx.whole().tracks[0].chant.as_mut().unwrap();
            t.delete_note(0).unwrap();
            Ok(())
        });
        assert_eq!(chant_of(&a).note_count(), 4);
        assert_cache(&a);
        assert_eq!(undo_steps(&a), 1);
    }

    #[test]
    fn every_operation_is_one_undo_step_and_comes_back() {
        for cmd in [
            "pitch-up",
            "accidental flat",
            "dot",
            "chant-mod v",
            "chant-append",
            "chant-break",
            "chant-syllable",
            "chant-split",
            "chant-sep /",
            "chant-bar ;",
            "delete-note",
            "clef c3",
        ] {
            let mut a = app();
            let before = a.buffer().score.clone();
            run(&mut a, cmd);
            assert_eq!(undo_steps(&a), 1, "{cmd}");
            assert!(!same(&a, &before), "{cmd}");
            assert_cache(&a);
            let after = a.buffer().score.clone();
            keys(&mut a, "u");
            assert!(same(&a, &before), "{cmd}");
            assert_eq!(
                chant_of(&a),
                before.tracks[0].chant.as_deref().unwrap(),
                "{cmd}"
            );
            keys(&mut a, "U");
            assert!(same(&a, &after), "{cmd}");
        }
    }

    #[test]
    fn the_first_note_of_an_empty_chant() {
        let mut a = app_of("name: t;\n%%\n(c4)\n");
        assert_eq!(cursor_note(&a), None);
        run(&mut a, "chant-append");
        assert_eq!(gabc(&a), "(c4g)");
        assert_eq!(chant_of(&a).note_count(), 1);
        assert_eq!(cursor_note(&a), Some(0));
        assert_cache(&a);
    }

    #[test]
    fn gabc_and_interpretation_commands() {
        let mut app = app();
        app.run(Command::Gabc(None));
        assert_eq!(app.last_notification().unwrap().text, "Al(fg)");
        app.run(Command::Gabc(Some("ghg".into())));
        assert_eq!(gabc(&app), "(c4) Al(ghg) le(h) lu(g) ia.(f.) (::)");
        app.run(Command::Gabc(Some("Glo(f)ri(gh)".into())));
        assert_eq!(gabc(&app), "(c4) Glo(f) ri(gh) le(h) lu(g) ia.(f.) (::)");
        let beats = |app: &App| {
            app.buffer().score.tracks[0].bars[0].voices[0]
                .beats
                .iter()
                .filter(|b| !b.empty)
                .count()
        };
        assert_eq!(beats(&app), 6);
        app.run(Command::Interpretation(Some("solesmes".into())));
        assert_eq!(
            app.buffer().score.tracks[0]
                .chant
                .as_ref()
                .unwrap()
                .interpretation,
            cecilia_core::chant::Interpretation::Solesmes
        );
        app.run(Command::ChantClef("c3".into()));
        assert!(gabc(&app).starts_with("(c3)"));
        assert_cache(&app);
        let chant_score = app.buffer().score.clone();
        app.run(Command::ChantToModern);
        assert_eq!(app.buffers.len(), 2);
        assert_eq!(
            app.buffers[0].score, chant_score,
            "the chant buffer is untouched"
        );
        let t = &app.buffer().score.tracks[0];
        assert!(t.chant.is_none() && t.kind == TrackKind::Pitched);
        app.run(Command::ModernToChant);
        assert_eq!(app.buffers.len(), 3);
        assert!(app.buffer().score.tracks[0].chant.is_some());
    }

    #[test]
    fn chant_to_modern_opens_a_modern_score_and_leaves_the_chant() {
        let mut a = app();
        let chant = a.buffers[0].score.clone();
        let modified = a.buffers[0].modified;
        a.run(Command::ChantToModern);
        assert_eq!(a.buffers.len(), 2);
        assert_eq!(a.buffers[0].score, chant);
        assert_eq!(a.buffers[0].modified, modified);
        let b = &a.buffers[1];
        assert_eq!(b.name, format!("[modern] {}", a.buffers[0].name));
        assert!(b.modified && b.path.is_none());
        let m = &b.score;
        assert!(!m.is_chant() && chant.is_chant());
        let (old, new) = (&chant.tracks[0], &m.tracks[0]);
        assert!(new.chant.is_none() && new.kind == TrackKind::Pitched);
        assert_eq!((&new.name, &new.midi), (&old.name, &old.midi));
        assert_eq!(m.master_bars, chant.master_bars);
        assert_eq!(m.meta, chant.meta);
        let pitches = |t: &cecilia_core::Track| -> Vec<Vec<(u8, u8)>> {
            t.bars
                .iter()
                .flat_map(|b| &b.voices[0].beats)
                .filter(|beat| !beat.empty)
                .map(|beat| beat.notes.iter().map(|n| (n.string, n.fret)).collect())
                .collect()
        };
        // The chant's division beat (it takes no time) is not a beat of the modern score.
        assert_eq!(pitches(new), pitches(old));
        assert!(
            new.bars
                .iter()
                .all(|b| b.voices[0].beats.iter().all(|x| !x.empty))
        );
        // The syllables are the lyrics text of the new score.
        assert_eq!(new.lyrics_text, "Al- _ le lu-ia.");
        // The chant buffer's text still edits like a chant.
        a.show_buffer(0);
        assert!(a.buffer().score.is_chant());
    }

    #[test]
    fn modern_to_chant_says_other_tracks_were_left_out() {
        let mut a = App::new(Config::default(), None);
        a.run(Command::New);
        keys(&mut a, "i1<Space>2<Esc>");
        a.run(Command::TrackAdd("piano".into()));
        a.run(Command::Track(1));
        a.run(Command::ModernToChant);
        assert!(a.buffer().score.is_chant(), "{:?}", a.last_notification());
        let msg = a.last_notification().unwrap().text.clone();
        assert!(
            msg.contains("only the cursor track") && msg.contains("1 other track"),
            "{msg}"
        );
    }

    #[test]
    fn a_score_holds_one_kind_of_track() {
        // A chant score takes no further track.
        let mut a = app();
        let before = a.buffer().score.clone();
        a.run(Command::TrackAdd("piano".into()));
        assert!(a.last_notification().unwrap().error);
        assert!(
            a.last_notification()
                .unwrap()
                .text
                .contains(":chant-to-modern")
        );
        assert_eq!(a.buffer().score, before);
        // Its only track stays too.
        a.run(Command::TrackDelete);
        assert!(a.last_notification().unwrap().error);
        assert_eq!(a.buffer().score, before);
        // A modern score takes no chant track.
        let mut m = App::new(Config::default(), None);
        m.run(Command::New);
        let before = m.buffer().score.clone();
        m.run(Command::TrackAdd("chant".into()));
        assert!(m.last_notification().unwrap().error);
        assert!(
            m.last_notification()
                .unwrap()
                .text
                .contains(":modern-to-chant")
        );
        assert_eq!(m.buffer().score, before);
        // Ordinary tracks still can be added.
        m.run(Command::TrackAdd("piano".into()));
        assert_eq!(m.buffer().score.tracks.len(), before.tracks.len() + 1);
    }

    #[test]
    fn a_modern_track_still_edits_its_bars() {
        let mut a = App::new(Config::default(), None);
        a.run(Command::New);
        let before = a.buffer().score.clone();
        a.run(Command::Rest);
        keys(&mut a, "i1<Esc>");
        assert_ne!(a.buffer().score, before);
        assert!(a.buffer().score.tracks[0].chant.is_none());
    }
}
