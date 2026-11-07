//! Edit operations on a [`Chant`]. The chant is the source of truth of a chant score; its bars are
//! a cache ([`crate::chant::retranscribe`]), so every edit of a chant goes through here and the
//! caller transcribes again afterwards.
//!
//! Notes are addressed by their index in reading order: the order of [`Chant::resolved`], which is
//! also the order [`crate::chant::transcribe`] makes beats in ([`Transcription::note_at`] and
//! [`Transcription::place_of`] map a cursor `(bar, beat)` to a note and back).
//!
//! Each operation returns `Result<_, String>`; on `Err` the chant is left unchanged.

use crate::chant::{
    Chant, ChantClef, ChantNote, Liquescence, NeumeItem, Shape, Syllable, transcribe,
};

/// Highest staff position GABC can write (`p`; `n` is 13).
const TOP: i32 = 14;

/// How a new note joins the notation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    /// In the same neume (no separator).
    Neume,
    /// In the same syllable, starting a new neume (a `/`).
    NewNeume,
    /// On a new, empty syllable.
    NewSyllable,
}

/// A flat, natural or sharp sign before a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alter {
    Flat,
    Natural,
    Sharp,
}

impl Alter {
    fn sign(self) -> &'static str {
        match self {
            Alter::Flat => "x",
            Alter::Natural => "y",
            Alter::Sharp => "#",
        }
    }

    fn of(sign: &str) -> Option<Alter> {
        match sign.chars().next()? {
            'x' | 'X' => Some(Alter::Flat),
            'y' | 'Y' => Some(Alter::Natural),
            '#' => Some(Alter::Sharp),
            _ => None,
        }
    }
}

impl Chant {
    /// Number of notes (the length of [`Chant::resolved`]).
    pub fn note_count(&self) -> usize {
        self.syllables.iter().map(|s| s.notes().count()).sum()
    }

    /// `(syllable, item)` of note `n`.
    pub fn note_place(&self, n: usize) -> Option<(usize, usize)> {
        let mut k = 0;
        for (si, s) in self.syllables.iter().enumerate() {
            for (ii, item) in s.items.iter().enumerate() {
                if matches!(item, NeumeItem::Note(_)) {
                    if k == n {
                        return Some((si, ii));
                    }
                    k += 1;
                }
            }
        }
        None
    }

    /// Index of the note at `(syllable, item)`.
    pub fn note_index(&self, syllable: usize, item: usize) -> Option<usize> {
        if !matches!(
            self.syllables.get(syllable)?.items.get(item)?,
            NeumeItem::Note(_)
        ) {
            return None;
        }
        let before: usize = self.syllables[..syllable]
            .iter()
            .map(|s| s.notes().count())
            .sum();
        let within = self.syllables[syllable].items[..item]
            .iter()
            .filter(|i| matches!(i, NeumeItem::Note(_)))
            .count();
        Some(before + within)
    }

    /// Note `n`.
    pub fn note(&self, n: usize) -> Option<&ChantNote> {
        let (si, ii) = self.note_place(n)?;
        match &self.syllables[si].items[ii] {
            NeumeItem::Note(note) => Some(note),
            _ => None,
        }
    }

    fn note_mut(&mut self, n: usize) -> Result<&mut ChantNote, String> {
        let (si, ii) = self.note_place(n).ok_or("no such note")?;
        match &mut self.syllables[si].items[ii] {
            NeumeItem::Note(note) => Ok(note),
            _ => Err("no such note".into()),
        }
    }

    /// The syllable holding note `n`.
    pub fn syllable_of(&self, n: usize) -> Option<usize> {
        self.note_place(n).map(|p| p.0)
    }

    /// The `(bar, beat)` of note `n` in the chant's transcription.
    pub fn place_of(&self, n: usize) -> Option<(usize, usize)> {
        transcribe(self).place_of(n)
    }

    /// The note sounded at `(bar, beat)` of the chant's transcription.
    pub fn note_at(&self, bar: usize, beat: usize) -> Option<usize> {
        transcribe(self).note_at(bar, beat)
    }

    /// Inserts a note at staff position `pitch` after (or before) note `n`. Returns the index
    /// of the new note. In a chant without notes `n` is 0 and the note starts the chant.
    pub fn insert_note(
        &mut self,
        n: usize,
        after: bool,
        join: Join,
        pitch: u8,
    ) -> Result<usize, String> {
        if pitch as i32 > TOP {
            return Err("pitch above the staff".into());
        }
        // The new note takes a fresh id; the others keep theirs.
        self.assign_ids();
        let id = self.next_id;
        self.next_id += 1;
        let note = NeumeItem::Note(ChantNote {
            id,
            ..ChantNote::new(pitch)
        });
        let Some((si, ii)) = self.note_place(n) else {
            if self.note_count() > 0 {
                return Err("no such note".into());
            }
            // Nothing to attach to: the first note goes on the last syllable (a new one if the
            // chant has none).
            if self.syllables.is_empty() {
                self.syllables.push(Syllable {
                    word_start: true,
                    ..Syllable::default()
                });
            }
            let si = self.syllables.len() - 1;
            self.syllables[si].items.push(note);
            return Ok(0);
        };
        let placed;
        match join {
            Join::NewSyllable => {
                // The new syllable goes after (or before) the whole syllable of note `n`.
                let at = if after { si + 1 } else { si };
                self.syllables.insert(
                    at,
                    Syllable {
                        items: vec![note],
                        ..Syllable::default()
                    },
                );
                placed = (at, 0);
            }
            Join::Neume | Join::NewNeume => {
                let items = &mut self.syllables[si].items;
                let separator = || NeumeItem::Space("/".into());
                if after {
                    let mut at = ii + 1;
                    if join == Join::NewNeume {
                        items.insert(at, separator());
                        at += 1;
                    }
                    items.insert(at, note);
                    placed = (si, at);
                } else {
                    // The note's own flat or natural stays with it.
                    let mut at = ii;
                    while at > 0 && matches!(items[at - 1], NeumeItem::Alteration { .. }) {
                        at -= 1;
                    }
                    items.insert(at, note);
                    if join == Join::NewNeume {
                        items.insert(at + 1, separator());
                    }
                    placed = (si, at);
                }
            }
        }
        self.note_index(placed.0, placed.1)
            .ok_or_else(|| "internal error: inserted note not found".into())
    }

    /// Deletes note `n`. A syllable left with nothing (no text, no other signs) goes too, and
    /// so does a neume separator left dangling at its end.
    pub fn delete_note(&mut self, n: usize) -> Result<(), String> {
        let (si, ii) = self.note_place(n).ok_or("no such note")?;
        let syl = &mut self.syllables[si];
        syl.items.remove(ii);
        // A separator now at the start or the end of the notation, or doubled, goes.
        let is_space =
            |syl: &Syllable, i: usize| matches!(syl.items.get(i), Some(NeumeItem::Space(_)));
        if ii < syl.items.len() && is_space(syl, ii) && (ii == 0 || is_space(syl, ii - 1)) {
            syl.items.remove(ii);
        } else if ii > 0 && ii == syl.items.len() && is_space(syl, ii - 1) {
            syl.items.pop();
        }
        let only_blank = syl
            .items
            .iter()
            .all(|i| matches!(i, NeumeItem::Space(_) | NeumeItem::Alteration { .. }));
        if only_blank && syl.text.trim().is_empty() {
            self.syllables.remove(si);
        }
        Ok(())
    }

    /// Moves note `n` by `delta` staff positions (a line or a space each, seven for an octave).
    /// A flat or natural written right before it moves with it.
    pub fn move_note(&mut self, n: usize, delta: i32) -> Result<(), String> {
        let (si, ii) = self.note_place(n).ok_or("no such note")?;
        let old = self.note(n).map_or(0, |x| x.pitch);
        let new = old as i32 + delta;
        if !(0..=TOP).contains(&new) {
            return Err(if delta > 0 {
                "already at the top of the staff".into()
            } else {
                "already at the bottom of the staff".into()
            });
        }
        let new = new as u8;
        if ii > 0 {
            if let NeumeItem::Alteration { pitch, .. } = &mut self.syllables[si].items[ii - 1] {
                if *pitch == old {
                    *pitch = new;
                }
            }
        }
        self.note_mut(n)?.pitch = new;
        Ok(())
    }

    /// Puts note `n` at staff position `pitch`.
    pub fn set_pitch(&mut self, n: usize, pitch: u8) -> Result<(), String> {
        let old = self.note(n).ok_or("no such note")?.pitch;
        self.move_note(n, pitch as i32 - old as i32)
    }

    /// The flat, natural or sharp written right before note `n`.
    pub fn alteration_of(&self, n: usize) -> Option<Alter> {
        let (si, ii) = self.note_place(n)?;
        let pitch = self.note(n)?.pitch;
        match self.syllables[si].items.get(ii.checked_sub(1)?)? {
            NeumeItem::Alteration { pitch: p, sign } if *p == pitch => Alter::of(sign),
            _ => None,
        }
    }

    /// Writes a flat, natural or sharp before note `n` (replacing the one there), or takes the
    /// one there away with `None`.
    pub fn set_alteration(&mut self, n: usize, alter: Option<Alter>) -> Result<(), String> {
        let (si, ii) = self.note_place(n).ok_or("no such note")?;
        let pitch = self.note(n).map_or(0, |x| x.pitch);
        let items = &mut self.syllables[si].items;
        let existing = ii.checked_sub(1).filter(
            |&j| matches!(&items[j], NeumeItem::Alteration { pitch: p, .. } if *p == pitch),
        );
        match (existing, alter) {
            (Some(j), Some(a)) => {
                items[j] = NeumeItem::Alteration {
                    pitch,
                    sign: a.sign().into(),
                }
            }
            (Some(j), None) => {
                items.remove(j);
            }
            (None, Some(a)) => items.insert(
                ii,
                NeumeItem::Alteration {
                    pitch,
                    sign: a.sign().into(),
                },
            ),
            (None, None) => {}
        }
        Ok(())
    }

    /// Gives note `n` a head: punctum, inclinatum, virga, quilisma, oriscus, stropha, bivirga …
    /// Liquescence, mora and the other signs stay. Shapes GABC has more variants of than this
    /// knows (`o0` …) are refused.
    pub fn set_shape(&mut self, n: usize, shape: Shape) -> Result<(), String> {
        let note = self.note_mut(n)?;
        let mut mods = note.mods.clone();
        let mut kept = Vec::new();
        for m in mods.drain(..) {
            match strip_shape(&m) {
                Some(rest) if rest.is_empty() => {}
                Some(rest) => kept.push(rest),
                None => {
                    let probe = ChantNote {
                        mods: vec![m.clone()],
                        ..ChantNote::default()
                    };
                    if probe.shape() != Shape::Punctum {
                        return Err(
                            "this note's shape is written in a way only the GABC text can change"
                                .into(),
                        );
                    }
                    kept.push(m);
                }
            }
        }
        let token = match shape {
            Shape::Punctum | Shape::Inclinatum => None,
            Shape::Virga => Some("v"),
            Shape::VirgaReversa => Some("V"),
            Shape::Bivirga(2) => Some("vv"),
            Shape::Bivirga(_) => Some("vvv"),
            Shape::Stropha(1) => Some("s"),
            Shape::Stropha(2) => Some("ss"),
            Shape::Stropha(_) => Some("sss"),
            Shape::Quilisma => Some("w"),
            Shape::Oriscus(false) => Some("o"),
            Shape::Oriscus(true) => Some("O"),
            Shape::Quadratum => Some("q"),
        };
        let mut candidate = note.clone();
        candidate.inclinatum = shape == Shape::Inclinatum;
        candidate.mods = token.map(String::from).into_iter().chain(kept).collect();
        if candidate.shape() != shape {
            return Err(
                "this note's shape is written in a way only the GABC text can change".into(),
            );
        }
        *note = candidate;
        Ok(())
    }

    /// Sets the mora dots of note `n` (0–2).
    pub fn set_mora(&mut self, n: usize, dots: u8) -> Result<(), String> {
        if dots > 2 {
            return Err("at most two mora dots".into());
        }
        let note = self.note_mut(n)?;
        note.mods.retain(|m| !m.starts_with('.'));
        if dots > 0 {
            note.mods.push(".".repeat(dots as usize));
        }
        Ok(())
    }

    /// Horizontal episema on or off.
    pub fn set_episema(&mut self, n: usize, on: bool) -> Result<(), String> {
        let note = self.note_mut(n)?;
        note.mods.retain(|m| !m.starts_with('_'));
        if on {
            note.mods.push("_".into());
        }
        Ok(())
    }

    /// Vertical episema (ictus) on or off.
    pub fn set_ictus(&mut self, n: usize, on: bool) -> Result<(), String> {
        let note = self.note_mut(n)?;
        note.mods.retain(|m| !m.starts_with('\''));
        if on {
            note.mods.push("'".into());
        }
        Ok(())
    }

    /// Initio debilis (a small first note) on or off.
    pub fn set_initio_debilis(&mut self, n: usize, on: bool) -> Result<(), String> {
        self.note_mut(n)?.initio_debilis = on;
        Ok(())
    }

    /// Liquescence of note `n`.
    pub fn set_liquescence(&mut self, n: usize, liquescence: Liquescence) -> Result<(), String> {
        let note = self.note_mut(n)?;
        let mut mods = Vec::new();
        for m in note.mods.drain(..) {
            match m.as_str() {
                "~" | "<" | ">" => {}
                // A liquescent stropha is written `ss<`.
                m if m.starts_with("ss") && m.len() > 2 && m.ends_with(['<', '>']) => {
                    mods.push(m[..m.len() - 1].to_string())
                }
                _ => mods.push(m),
            }
        }
        match liquescence {
            Liquescence::None => {}
            Liquescence::Deminutus => mods.push("~".into()),
            Liquescence::AuctusAscendens => mods.push("<".into()),
            Liquescence::AuctusDescendens => mods.push(">".into()),
        }
        note.mods = mods;
        Ok(())
    }

    /// Toggles any other GABC modifier of note `n` as written (`r`, `R`, `[ll:1]` …).
    pub fn toggle_mod(&mut self, n: usize, modifier: &str) -> Result<(), String> {
        if modifier.is_empty() || modifier.contains(['(', ')', ' ']) {
            return Err(format!("'{modifier}' is not a GABC modifier"));
        }
        let note = self.note_mut(n)?;
        match note.mods.iter().position(|m| m == modifier) {
            Some(i) => {
                note.mods.remove(i);
            }
            None => note.mods.push(modifier.to_string()),
        }
        Ok(())
    }

    /// Whether `sign` is a division as GABC writes it.
    pub fn is_division(sign: &str) -> bool {
        sign.starts_with([',', ';', ':', '`', '^'])
            && sign.chars().all(|c| ",;:`^'?012345678".contains(c))
    }

    /// The division written on its own right after the syllable of note `n`.
    pub fn division_after(&self, n: usize) -> Option<&str> {
        let si = self.syllable_of(n)?;
        match self.syllables.get(si + 1)?.items.as_slice() {
            [NeumeItem::Bar(b)] => Some(b),
            _ => None,
        }
    }

    /// Writes a division (`,` `;` `:` `::` `` ` ``, finalis, virgula …) after the syllable of
    /// note `n`. Returns the index of the syllable made.
    pub fn insert_division(&mut self, n: usize, sign: &str) -> Result<usize, String> {
        if !Self::is_division(sign) {
            return Err(format!("'{sign}' is not a division (, ; : :: `)"));
        }
        let si = match self.syllable_of(n) {
            Some(si) => si,
            None if self.note_count() == 0 => self.syllables.len().saturating_sub(1),
            None => return Err("no such note".into()),
        };
        let at = (si + 1).min(self.syllables.len());
        self.syllables.insert(
            at,
            Syllable {
                word_start: true,
                items: vec![NeumeItem::Bar(sign.into())],
                ..Syllable::default()
            },
        );
        Ok(at)
    }

    /// Replaces the division right after the syllable of note `n` by another kind.
    pub fn set_division(&mut self, n: usize, sign: &str) -> Result<(), String> {
        if !Self::is_division(sign) {
            return Err(format!("'{sign}' is not a division (, ; : :: `)"));
        }
        if self.division_after(n).is_none() {
            return Err("no division here".into());
        }
        let si = self.syllable_of(n).ok_or("no such note")? + 1;
        self.syllables[si].items = vec![NeumeItem::Bar(sign.into())];
        Ok(())
    }

    /// Deletes the division right after the syllable of note `n`.
    pub fn delete_division(&mut self, n: usize) -> Result<(), String> {
        if self.division_after(n).is_none() {
            return Err("no division here".into());
        }
        let si = self.syllable_of(n).ok_or("no such note")? + 1;
        self.syllables.remove(si);
        Ok(())
    }

    /// Sets the clef the chant starts with (the first clef, or one before everything).
    pub fn set_initial_clef(&mut self, clef: ChantClef) -> Result<(), String> {
        let first = self
            .syllables
            .iter_mut()
            .flat_map(|s| s.items.iter_mut())
            .find(|i| matches!(i, NeumeItem::Clef(_)));
        match first {
            Some(c) => *c = NeumeItem::Clef(clef),
            None => match self.syllables.first_mut() {
                Some(s) => s.items.insert(0, NeumeItem::Clef(clef)),
                None => {
                    self.syllables.push(Syllable {
                        word_start: true,
                        items: vec![NeumeItem::Clef(clef)],
                        ..Syllable::default()
                    });
                }
            },
        }
        Ok(())
    }

    /// Changes the clef from note `n` on (replacing a clef right before it).
    pub fn set_clef_before(&mut self, n: usize, clef: ChantClef) -> Result<(), String> {
        let (si, ii) = self.note_place(n).ok_or("no such note")?;
        let items = &mut self.syllables[si].items;
        let mut at = ii;
        while at > 0 && matches!(items[at - 1], NeumeItem::Alteration { .. }) {
            at -= 1;
        }
        if at > 0 && matches!(items[at - 1], NeumeItem::Clef(_)) {
            items[at - 1] = NeumeItem::Clef(clef);
        } else {
            items.insert(at, NeumeItem::Clef(clef));
        }
        Ok(())
    }

    /// Sets the text of the syllable holding note `n`, as written in GABC (markup allowed; an
    /// unescaped parenthesis or a line break is not).
    pub fn set_syllable_text(&mut self, n: usize, text: &str) -> Result<(), String> {
        let si = self.syllable_of(n).ok_or("no such note")?;
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            match c {
                '$' => {
                    chars.next();
                }
                '(' | ')' => return Err("parentheses in syllable text need a $ before them".into()),
                '\n' | '\r' => return Err("syllable text cannot hold a line break".into()),
                _ => {}
            }
        }
        self.syllables[si].text = text.to_string();
        Ok(())
    }

    /// Splits the syllable of note `n` before that note: the note and those after it move to a
    /// new syllable (without text) following it. Returns the new syllable's index.
    pub fn split_syllable(&mut self, n: usize) -> Result<usize, String> {
        let (si, ii) = self.note_place(n).ok_or("no such note")?;
        let items = &mut self.syllables[si].items;
        let mut at = ii;
        while at > 0 && matches!(items[at - 1], NeumeItem::Alteration { .. }) {
            at -= 1;
        }
        if !items[..at].iter().any(|i| matches!(i, NeumeItem::Note(_))) {
            return Err("the note starts its syllable; there is nothing to split".into());
        }
        let tail = items.split_off(at);
        // A neume separator left at the end of the first part goes.
        if matches!(items.last(), Some(NeumeItem::Space(_))) {
            items.pop();
        }
        self.syllables.insert(
            si + 1,
            Syllable {
                text: String::new(),
                word_start: false,
                items: tail,
            },
        );
        Ok(si + 1)
    }

    /// Joins the syllable of note `n` with the one after it: texts and notation are
    /// concatenated (the notation keeps the second part as its own neume). Refuses when the
    /// next syllable is a division or has no notation.
    pub fn join_syllable(&mut self, n: usize) -> Result<(), String> {
        let si = self.syllable_of(n).ok_or("no such note")?;
        let Some(next) = self.syllables.get(si + 1) else {
            return Err("no syllable to join with".into());
        };
        if next.notes().next().is_none() {
            return Err("the next syllable has no notes to join".into());
        }
        let next = self.syllables.remove(si + 1);
        let syl = &mut self.syllables[si];
        syl.text.push_str(&next.text);
        if matches!(syl.items.last(), Some(NeumeItem::Note(_))) {
            syl.items.push(NeumeItem::Space("/".into()));
        }
        syl.items.extend(next.items);
        Ok(())
    }
}

/// What is left of a modifier once its shape is taken away, or `None` if it holds no shape this
/// module can change.
fn strip_shape(m: &str) -> Option<String> {
    const SHAPES: [&str; 12] = [
        "v", "V", "vv", "VV", "vvv", "VVV", "w", "W", "q", "s", "ss", "sss",
    ];
    if SHAPES.contains(&m) {
        return Some(String::new());
    }
    // A stropha with liquescence (`ss<`) or an oriscus with it (`o<`).
    for prefix in ["sss", "ss", "o", "O"] {
        if let Some(rest) = m.strip_prefix(prefix) {
            if matches!(rest, "<" | ">" | "~") {
                return Some(rest.to_string());
            }
            if rest.is_empty() {
                return Some(String::new());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chant::{Interpretation, retranscribe};

    fn syl(text: &str, word_start: bool, items: Vec<NeumeItem>) -> Syllable {
        Syllable {
            text: text.into(),
            word_start,
            items,
        }
    }

    fn note(p: u8) -> NeumeItem {
        NeumeItem::Note(ChantNote::new(p))
    }

    /// `(c4) Al(fg)le(h) lu(g)ia.(f.) (::)` built by hand.
    fn small() -> Chant {
        let mut dotted = ChantNote::new(5);
        dotted.mods.push(".".into());
        Chant {
            syllables: vec![
                syl("", true, vec![NeumeItem::Clef(ChantClef::default())]),
                syl("Al", true, vec![note(5), note(6)]),
                syl("le", false, vec![note(7)]),
                syl("lu", true, vec![note(6)]),
                syl("ia.", false, vec![NeumeItem::Note(dotted)]),
                syl("", true, vec![NeumeItem::Bar("::".into())]),
            ],
            ..Chant::default()
        }
    }

    fn gabc(c: &Chant) -> String {
        c.syllables
            .iter()
            .map(Syllable::gabc)
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn notes_are_addressed_in_reading_order() {
        let c = small();
        assert_eq!(c.note_count(), 5);
        assert_eq!(c.note_place(2), Some((2, 0)));
        assert_eq!(c.note_index(2, 0), Some(2));
        assert_eq!(c.note_index(5, 0), None);
        let t = transcribe(&c);
        assert_eq!(t.positions.len(), 5);
        for n in 0..5 {
            let (bar, beat) = t.place_of(n).unwrap();
            assert_eq!(t.note_at(bar, beat), Some(n));
        }
        // The division ends the only bar.
        assert_eq!(t.note_at(1, 0), None);
        assert_eq!(t.note_near(1, 0), Some(4));
    }

    #[test]
    fn a_stropha_maps_every_beat_to_its_note() {
        let mut c = small();
        c.set_shape(1, Shape::Stropha(2)).unwrap();
        let t = transcribe(&c);
        assert_eq!(t.lengths, [1, 2, 1, 1, 1]);
        let (bar, beat) = t.place_of(1).unwrap();
        assert_eq!(t.note_at(bar, beat), Some(1));
        assert_eq!(t.note_at(bar, beat + 1), Some(1));
        assert_eq!(t.note_at(bar, beat + 2), Some(2));
        assert_eq!(t.place_of(2), Some((bar, beat + 2)));
    }

    #[test]
    fn insert_and_delete() {
        let mut c = small();
        assert_eq!(c.insert_note(0, true, Join::Neume, 7), Ok(1));
        assert_eq!(gabc(&c), "(c4) Al(fhg) le(h) lu(g) ia.(f.) (::)");
        assert_eq!(c.insert_note(0, false, Join::NewNeume, 3), Ok(0));
        assert_eq!(gabc(&c), "(c4) Al(d/fhg) le(h) lu(g) ia.(f.) (::)");
        // The new syllable follows the whole syllable, so its note comes after the g.
        assert_eq!(c.insert_note(2, true, Join::NewSyllable, 4), Ok(4));
        assert_eq!(gabc(&c), "(c4) Al(d/fhg) (e) le(h) lu(g) ia.(f.) (::)");
        c.delete_note(4).unwrap();
        c.delete_note(0).unwrap();
        c.delete_note(0).unwrap();
        assert_eq!(gabc(&c), "(c4) Al(hg) le(h) lu(g) ia.(f.) (::)");
        // A syllable with text stays when its notes go; an empty one does not.
        c.delete_note(2).unwrap();
        assert_eq!(gabc(&c), "(c4) Al(hg) le() lu(g) ia.(f.) (::)");
        assert_eq!(
            c.insert_note(5, true, Join::Neume, 1),
            Err("no such note".into())
        );
    }

    #[test]
    fn a_note_inserted_before_keeps_the_flat_of_its_neighbour() {
        let mut c = small();
        c.set_alteration(1, Some(Alter::Flat)).unwrap();
        assert_eq!(gabc(&c), "(c4) Al(fgxg) le(h) lu(g) ia.(f.) (::)");
        assert_eq!(c.insert_note(1, false, Join::Neume, 3), Ok(1));
        assert_eq!(gabc(&c), "(c4) Al(fdgxg) le(h) lu(g) ia.(f.) (::)");
        assert_eq!(c.alteration_of(2), Some(Alter::Flat));
        assert_eq!(c.alteration_of(1), None);
    }

    #[test]
    fn the_first_note_of_an_empty_chant() {
        let mut c = Chant::default();
        assert_eq!(c.insert_note(0, true, Join::Neume, 6), Ok(0));
        assert_eq!(c.note_count(), 1);
        assert_eq!(c.insert_note(0, true, Join::NewSyllable, 7), Ok(1));
        assert_eq!(c.note_count(), 2);
    }

    #[test]
    fn moving_notes() {
        let mut c = small();
        c.move_note(1, 1).unwrap();
        assert_eq!(gabc(&c), "(c4) Al(fh) le(h) lu(g) ia.(f.) (::)");
        c.move_note(1, 7).unwrap();
        assert_eq!(gabc(&c), "(c4) Al(fp) le(h) lu(g) ia.(f.) (::)");
        assert!(c.move_note(1, 1).is_err());
        c.set_pitch(1, 0).unwrap();
        assert_eq!(gabc(&c), "(c4) Al(fa) le(h) lu(g) ia.(f.) (::)");
        assert!(c.move_note(1, -1).is_err());
        assert_eq!(gabc(&c), "(c4) Al(fa) le(h) lu(g) ia.(f.) (::)");
    }

    #[test]
    fn a_flat_follows_its_note() {
        let mut c = small();
        c.set_pitch(2, 8).unwrap();
        c.set_alteration(2, Some(Alter::Flat)).unwrap();
        assert_eq!(gabc(&c), "(c4) Al(fg) le(ixi) lu(g) ia.(f.) (::)");
        assert_eq!(c.alteration_of(2), Some(Alter::Flat));
        // B♭ sounds a semitone below B.
        let natural = {
            c.set_alteration(2, None).unwrap();
            c.resolved()[2].midi
        };
        c.set_alteration(2, Some(Alter::Flat)).unwrap();
        assert_eq!(c.resolved()[2].midi, natural - 1);
        c.move_note(2, 1).unwrap();
        assert_eq!(c.alteration_of(2), Some(Alter::Flat));
        c.set_alteration(2, Some(Alter::Natural)).unwrap();
        assert_eq!(c.alteration_of(2), Some(Alter::Natural));
        c.set_alteration(2, None).unwrap();
        assert_eq!(c.alteration_of(2), None);
    }

    #[test]
    fn shapes() {
        let mut c = small();
        c.set_shape(1, Shape::Virga).unwrap();
        assert_eq!(c.note(1).unwrap().shape(), Shape::Virga);
        c.set_shape(1, Shape::Quilisma).unwrap();
        assert_eq!(gabc(&c), "(c4) Al(fgw) le(h) lu(g) ia.(f.) (::)");
        c.set_shape(1, Shape::Oriscus(true)).unwrap();
        assert_eq!(c.note(1).unwrap().gabc(), "gO");
        c.set_shape(1, Shape::Inclinatum).unwrap();
        assert_eq!(c.note(1).unwrap().gabc(), "G");
        c.set_shape(1, Shape::Punctum).unwrap();
        assert_eq!(c.note(1).unwrap().gabc(), "g");
        // The mora and the liquescence stay.
        c.set_shape(4, Shape::Stropha(2)).unwrap();
        c.set_liquescence(4, Liquescence::AuctusAscendens).unwrap();
        assert_eq!(c.note(4).unwrap().gabc(), "fss.<");
        assert_eq!(
            c.note(4).unwrap().liquescence(),
            Liquescence::AuctusAscendens
        );
        c.set_shape(4, Shape::Virga).unwrap();
        assert_eq!(c.note(4).unwrap().gabc(), "fv.<");
        // A shape written in a way this does not know is refused and left alone.
        c.toggle_mod(0, "o0").unwrap();
        assert!(c.set_shape(0, Shape::Virga).is_err());
        assert_eq!(c.note(0).unwrap().gabc(), "fo0");
    }

    #[test]
    fn signs() {
        let mut c = small();
        c.set_mora(0, 1).unwrap();
        assert_eq!(c.note(0).unwrap().mora(), 1);
        c.set_mora(0, 2).unwrap();
        assert_eq!(c.note(0).unwrap().gabc(), "f..");
        c.set_mora(0, 0).unwrap();
        c.set_episema(0, true).unwrap();
        c.set_ictus(0, true).unwrap();
        assert_eq!(c.note(0).unwrap().gabc(), "f_'");
        assert!(c.note(0).unwrap().episema() && c.note(0).unwrap().ictus());
        c.set_episema(0, false).unwrap();
        c.set_ictus(0, false).unwrap();
        c.set_liquescence(0, Liquescence::Deminutus).unwrap();
        assert_eq!(c.note(0).unwrap().liquescence(), Liquescence::Deminutus);
        c.set_liquescence(0, Liquescence::None).unwrap();
        c.set_initio_debilis(0, true).unwrap();
        assert_eq!(c.note(0).unwrap().gabc(), "-f");
        assert!(c.set_mora(0, 3).is_err());
        assert!(c.toggle_mod(0, "a b").is_err());
    }

    #[test]
    fn divisions() {
        let mut c = small();
        assert_eq!(c.division_after(1), None);
        assert_eq!(c.insert_division(1, ";"), Ok(2));
        assert_eq!(gabc(&c), "(c4) Al(fg) (;) le(h) lu(g) ia.(f.) (::)");
        assert_eq!(c.division_after(0), Some(";"));
        c.set_division(0, "::").unwrap();
        assert_eq!(c.division_after(1), Some("::"));
        c.delete_division(1).unwrap();
        assert_eq!(gabc(&c), "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)");
        assert!(c.delete_division(1).is_err());
        assert!(c.insert_division(1, "x").is_err());
        assert_eq!(c.division_after(4), Some("::"));
        // The finalis ends the last bar.
        let t = transcribe(&c);
        assert!(t.master_bars.last().unwrap().double_bar);
    }

    #[test]
    fn clefs() {
        let mut c = small();
        c.set_initial_clef(ChantClef::parse("f3").unwrap()).unwrap();
        assert!(gabc(&c).starts_with("(f3)"));
        let mut empty = Chant::default();
        empty.set_initial_clef(ChantClef::default()).unwrap();
        assert_eq!(empty.syllables[0].gabc_notes(), "c4");
        c.set_clef_before(2, ChantClef::parse("c3").unwrap())
            .unwrap();
        assert_eq!(gabc(&c), "(f3) Al(fg) le(c3h) lu(g) ia.(f.) (::)");
        c.set_clef_before(2, ChantClef::parse("c2").unwrap())
            .unwrap();
        assert_eq!(gabc(&c), "(f3) Al(fg) le(c2h) lu(g) ia.(f.) (::)");
        // The clef moves later notes' pitches, not their positions.
        assert_ne!(c.resolved()[2].midi, small().resolved()[2].midi);
    }

    #[test]
    fn syllable_text_split_and_join() {
        let mut c = small();
        c.set_syllable_text(0, "Ál").unwrap();
        assert!(c.set_syllable_text(0, "A(l").is_err());
        assert!(c.set_syllable_text(0, "A\nl").is_err());
        c.set_syllable_text(0, "A$(l").unwrap();
        assert_eq!(c.syllables[1].text, "A$(l");
        c.set_syllable_text(0, "Al").unwrap();
        // Split "Al(fg)" before g: "Al(f) (g)", the new syllable without text.
        assert!(c.split_syllable(0).is_err());
        assert_eq!(c.split_syllable(1), Ok(2));
        assert_eq!(gabc(&c), "(c4) Al(f) (g) le(h) lu(g) ia.(f.) (::)");
        c.set_syllable_text(1, "le").unwrap();
        assert_eq!(gabc(&c), "(c4) Al(f) le(g) le(h) lu(g) ia.(f.) (::)");
        // Joining brings the texts and the notes together; the notes stay a separate neume.
        c.join_syllable(0).unwrap();
        assert_eq!(gabc(&c), "(c4) Alle(f/g) le(h) lu(g) ia.(f.) (::)");
        // The last syllable has nothing to join, the division is not a syllable to join with.
        assert!(c.join_syllable(4).is_err());
    }

    #[test]
    fn edits_leave_a_chant_that_transcribes() {
        let mut score = crate::Score::default();
        let mut c = small();
        c.interpretation = Interpretation::Equalist;
        score.tracks.push(crate::Track {
            chant: Some(Box::new(c)),
            ..crate::Track::default()
        });
        retranscribe(&mut score, 0);
        let beats = |s: &crate::Score| s.tracks[0].bars[0].voices[0].beats.len();
        assert_eq!(beats(&score), 5);
        let mut c = (**score.tracks[0].chant.as_ref().unwrap()).clone();
        c.insert_note(4, true, Join::Neume, 3).unwrap();
        score.tracks[0].chant = Some(Box::new(c));
        retranscribe(&mut score, 0);
        assert_eq!(beats(&score), 6);
    }

    #[test]
    fn note_ids_stay_with_their_notes_through_every_edit() {
        let mut c = small();
        c.assign_ids();
        assert_eq!(c.note_ids(), [1, 2, 3, 4, 5]);
        // New notes take fresh ids, the others keep theirs.
        c.insert_note(1, true, Join::Neume, 8).unwrap();
        assert_eq!(c.note_ids(), [1, 2, 6, 3, 4, 5]);
        c.insert_note(0, false, Join::NewSyllable, 4).unwrap();
        assert_eq!(c.note_ids(), [7, 1, 2, 6, 3, 4, 5]);
        c.delete_note(1).unwrap();
        assert_eq!(c.note_ids(), [7, 2, 6, 3, 4, 5]);
        // A deleted note's id is not handed out again.
        c.insert_note(0, true, Join::Neume, 4).unwrap();
        assert_eq!(c.note_ids(), [7, 8, 2, 6, 3, 4, 5]);
        let before = c.note_ids();
        c.move_note(2, 2).unwrap();
        c.set_shape(2, Shape::Virga).unwrap();
        c.set_mora(3, 1).unwrap();
        c.set_alteration(4, Some(Alter::Flat)).unwrap();
        c.insert_division(3, ";").unwrap();
        c.split_syllable(3).unwrap();
        assert_eq!(c.note_ids(), before);
        c.join_syllable(3).unwrap_or(());
        c.set_syllable_text(0, "x").unwrap();
        assert_eq!(c.note_ids(), before);
    }

    #[test]
    fn ids_do_not_take_part_in_comparisons() {
        let mut a = small();
        let b = small();
        a.assign_ids();
        assert_eq!(a, b);
        assert_eq!(gabc(&a), gabc(&b));
    }

    #[test]
    fn a_chant_made_again_from_text_keeps_the_ids_of_the_notes_it_matches() {
        let mut old = small();
        old.assign_ids();
        // The same melody with a note added in the middle and one dropped at the end.
        let mut new = small();
        new.insert_note(1, true, Join::Neume, 9).unwrap();
        new.delete_note(5).unwrap();
        for item in new.syllables.iter_mut().flat_map(|s| &mut s.items) {
            if let NeumeItem::Note(n) = item {
                n.id = 0;
            }
        }
        new.match_ids(&old);
        let ids = new.note_ids();
        // The new note has a fresh id, the others their old ones.
        assert_eq!(&ids[..2], [1, 2]);
        assert!(ids[2] > 5);
        assert_eq!(&ids[3..], [3, 4]);
        // Identical text: every id stays.
        let mut same = small();
        same.match_ids(&old);
        assert_eq!(same.note_ids(), old.note_ids());
    }
}
