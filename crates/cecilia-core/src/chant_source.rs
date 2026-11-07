//! The GABC text a chant was read from (or last written as), and where in it every note and
//! syllable is.
//!
//! The chant model is the source of truth of a chant score; its GABC body is kept next to it as
//! a *layout*: the user's own spacing, line breaks and comments survive score edits, because a
//! change to the chant is applied to the text as a splice at the changed place
//! ([`plan_splice`]) rather than by writing the whole text again.
//!
//! All offsets are **character** indexes (not bytes), like the lyrics text area's caret.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::chant::{Chant, NeumeItem, Syllable};

/// Where the notes and syllables of a GABC text are, as character ranges of that text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceMap {
    /// The characters of every note in reading order (the order of `Chant::resolved`): the
    /// pitch letter, a leading `-` or `@`, its modifiers and attached `[…]` attributes.
    pub notes: Vec<Range<usize>>,
    /// One entry per syllable of the chant, in order.
    pub syllables: Vec<SyllableSource>,
}

/// Where one syllable is written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyllableSource {
    /// From the first character of its text (of its `(` when it has none) to after its `)`.
    pub range: Range<usize>,
    /// The syllable's text, up to the `(`.
    pub text: Range<usize>,
    /// What is between the parentheses.
    pub notation: Range<usize>,
    /// The characters of every item of the syllable (notes, separators, divisions …), parallel
    /// to `Syllable::items`.
    pub items: Vec<Range<usize>>,
    /// Index (in reading order) of the syllable's first note: its notes are
    /// `first_note..first_note + count`.
    pub first_note: usize,
}

impl SourceMap {
    /// The characters of note `n`.
    pub fn note_range(&self, n: usize) -> Option<Range<usize>> {
        self.notes.get(n).cloned()
    }

    /// The note under the character `at`, or else the nearest one before it; before the first
    /// note, the first. None in a chant without notes.
    pub fn note_at(&self, at: usize) -> Option<usize> {
        if self.notes.is_empty() {
            return None;
        }
        let k = self.notes.partition_point(|r| r.start <= at);
        Some(k.saturating_sub(1))
    }

    /// The note whose characters include `at`.
    pub fn note_under(&self, at: usize) -> Option<usize> {
        let k = self.note_at(at)?;
        self.notes[k].contains(&at).then_some(k)
    }

    /// The syllable whose characters (text and notation) include `at`, or else the nearest
    /// one before it.
    pub fn syllable_at(&self, at: usize) -> Option<usize> {
        if self.syllables.is_empty() {
            return None;
        }
        let k = self.syllables.partition_point(|s| s.range.start <= at);
        Some(k.saturating_sub(1))
    }

    /// The map with every offset moved back by `by` characters (the map of a text with its
    /// first `by` characters cut off, such as a body without its header).
    pub fn shifted_back(&self, by: usize) -> SourceMap {
        let back = |r: &Range<usize>| r.start.saturating_sub(by)..r.end.saturating_sub(by);
        SourceMap {
            notes: self.notes.iter().map(back).collect(),
            syllables: self
                .syllables
                .iter()
                .map(|s| SyllableSource {
                    range: back(&s.range),
                    text: back(&s.text),
                    notation: back(&s.notation),
                    items: s.items.iter().map(back).collect(),
                    first_note: s.first_note,
                })
                .collect(),
        }
    }

    /// The map with every offset moved on by `by` characters.
    pub fn shifted_on(&self, by: usize) -> SourceMap {
        let on = |r: &Range<usize>| r.start + by..r.end + by;
        SourceMap {
            notes: self.notes.iter().map(on).collect(),
            syllables: self
                .syllables
                .iter()
                .map(|s| SyllableSource {
                    range: on(&s.range),
                    text: on(&s.text),
                    notation: on(&s.notation),
                    items: s.items.iter().map(on).collect(),
                    first_note: s.first_note,
                })
                .collect(),
        }
    }

    /// Whether the map describes `chant`: the same syllables, items and notes.
    pub fn fits(&self, chant: &Chant) -> bool {
        self.syllables.len() == chant.syllables.len()
            && self.notes.len() == chant.note_count()
            && self
                .syllables
                .iter()
                .zip(&chant.syllables)
                .all(|(m, s)| m.items.len() == s.items.len())
    }
}

/// A chant's GABC body as kept: the text (everything after the `%%` line, as the user wrote it)
/// and the map of it. Not part of the chant's content: chants compare equal without it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChantSource {
    pub text: String,
    /// Made again from `text` when a file is read; empty until then.
    #[serde(skip)]
    pub map: SourceMap,
}

/// The byte offset of character `at` of `text` (the end when past it).
pub fn byte_at(text: &str, at: usize) -> usize {
    text.char_indices().nth(at).map_or(text.len(), |(i, _)| i)
}

/// How much of the text a [`Splice`] touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpliceKind {
    /// The chant did not change.
    Nothing,
    /// Items of one syllable's notation (a note's pitch or signs, a note inserted or deleted
    /// within a group, a clef, an alteration).
    Items,
    /// The text of one syllable.
    SyllableText,
    /// Whole syllables (a new syllable or division, a split or join, a syllable deleted).
    Syllables,
}

/// A replacement of a range of characters of the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Splice {
    pub range: Range<usize>,
    pub text: String,
    pub kind: SpliceKind,
}

impl Splice {
    /// The text with the splice made.
    pub fn apply(&self, text: &str) -> String {
        let (a, b) = (
            byte_at(text, self.range.start),
            byte_at(text, self.range.end),
        );
        let mut out = String::with_capacity(text.len() + self.text.len());
        out.push_str(&text[..a]);
        out.push_str(&self.text);
        out.push_str(&text[b..]);
        out
    }
}

/// How many leading and trailing elements two slices share (the leading ones first, the
/// trailing ones without overlapping them).
fn shared<T: PartialEq>(a: &[T], b: &[T]) -> (usize, usize) {
    let lead = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let trail = a[lead..]
        .iter()
        .rev()
        .zip(b[lead..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    (lead, trail)
}

/// The splice that turns the text `old` was written as (`map` describing it) into text for
/// `new`, changing only the place where the two chants differ: the items of one syllable, one
/// syllable's text, or a run of syllables. None when the map does not fit `old`.
///
/// The result is a proposal: the caller reads the new text again and falls back to writing the
/// whole chant when that does not give `new` (spacing the splice could not get right).
pub fn plan_splice(old: &Chant, map: &SourceMap, new: &Chant) -> Option<Splice> {
    if !map.fits(old) {
        return None;
    }
    let (o, n) = (&old.syllables, &new.syllables);
    let (lead, trail) = shared(o, n);
    let (ob, nb) = (o.len() - trail, n.len() - trail);
    if lead == o.len() && lead == n.len() {
        return Some(Splice {
            range: 0..0,
            text: String::new(),
            kind: SpliceKind::Nothing,
        });
    }
    if ob - lead == 1 && nb - lead == 1 {
        let (os, ns) = (&o[lead], &n[lead]);
        let src = &map.syllables[lead];
        if os.text == ns.text && os.word_start == ns.word_start {
            return Some(item_splice(os, ns, src));
        }
        if os.items == ns.items && os.word_start == ns.word_start {
            return Some(Splice {
                range: src.text.clone(),
                text: ns.text.clone(),
                kind: SpliceKind::SyllableText,
            });
        }
    }
    Some(syllable_splice(map, &n[lead..nb], lead, ob, n.get(nb)))
}

/// The splice for two versions of one syllable with the same text.
fn item_splice(
    old: &Syllable,
    new: &Syllable,
    src: &crate::chant_source::SyllableSource,
) -> Splice {
    let (lead, trail) = shared(&old.items, &new.items);
    let (ob, nb) = (old.items.len() - trail, new.items.len() - trail);
    let text: String = new.items[lead..nb].iter().map(NeumeItem::gabc).collect();
    let range = if ob > lead {
        src.items[lead].start..src.items[ob - 1].end
    } else {
        // Nothing of the old items goes: the new ones are put in between.
        let at = if lead < old.items.len() {
            src.items[lead].start
        } else if lead > 0 {
            src.items[lead - 1].end
        } else {
            src.notation.start
        };
        at..at
    };
    Splice {
        range,
        text,
        kind: SpliceKind::Items,
    }
}

/// The splice replacing syllables `from..to` of the old chant with `new` (the syllables that
/// stand there now); `after` is the syllable that follows them in the new chant.
fn syllable_splice(
    map: &SourceMap,
    new: &[Syllable],
    from: usize,
    to: usize,
    after: Option<&Syllable>,
) -> Splice {
    let gap = |s: &Syllable| if s.word_start { " " } else { "" };
    let syl = &map.syllables;
    let (range, text) = match (to > from, new.is_empty()) {
        // Syllables go.
        (true, true) => {
            let start = if from > 0 {
                syl[from - 1].range.end
            } else {
                syl[from].range.start
            };
            (start..syl[to - 1].range.end, String::new())
        }
        // Replaced: the first new one takes the old one's place (and the space before it).
        (true, false) => {
            let mut text = new[0].gabc();
            for s in &new[1..] {
                text.push_str(gap(s));
                text.push_str(&s.gabc());
            }
            (syl[from].range.start..syl[to - 1].range.end, text)
        }
        // New ones come in after the syllable before, or before the first.
        (false, _) => {
            if from > 0 {
                let mut text = String::new();
                for s in new {
                    text.push_str(gap(s));
                    text.push_str(&s.gabc());
                }
                let at = syl[from - 1].range.end;
                (at..at, text)
            } else {
                let mut text = String::new();
                for (k, s) in new.iter().enumerate() {
                    if k > 0 {
                        text.push_str(gap(s));
                    }
                    text.push_str(&s.gabc());
                }
                if after.is_some_and(|s| s.word_start) {
                    text.push(' ');
                }
                let at = syl.first().map_or(0, |s| s.range.start);
                (at..at, text)
            }
        }
    };
    Splice {
        range,
        text,
        kind: SpliceKind::Syllables,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_lookup_by_character() {
        let map = SourceMap {
            notes: vec![4..6, 9..10, 12..13],
            syllables: Vec::new(),
        };
        assert_eq!(map.note_at(0), Some(0));
        assert_eq!(map.note_at(5), Some(0));
        assert_eq!(map.note_at(8), Some(0));
        assert_eq!(map.note_at(9), Some(1));
        assert_eq!(map.note_at(99), Some(2));
        assert_eq!(map.note_under(8), None);
        assert_eq!(map.note_under(12), Some(2));
        assert_eq!(SourceMap::default().note_at(3), None);
    }

    #[test]
    fn splice_applies_by_characters() {
        let s = Splice {
            range: 1..2,
            text: "XY".into(),
            kind: SpliceKind::Items,
        };
        assert_eq!(s.apply("aéc"), "aXYc");
        assert_eq!(s.apply("a"), "aXY");
    }
}
