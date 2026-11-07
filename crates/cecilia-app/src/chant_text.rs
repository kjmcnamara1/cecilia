//! The live GABC text of a chant score: the lyrics text area of its Lyrics tab edits the chant's
//! GABC body, and the text and the score stay in step.
//!
//! * The text the user types is the source while editing: after each key that parses, the chant
//!   is replaced (notes keep their ids by [`Chant::match_ids`]); when it does not parse, the last
//!   good chant stays and the error is shown with its position.
//! * The chant keeps its GABC body as a layout ([`cecilia_core::chant_source`]); score edits
//!   splice into it at the changed note, so the rest of the user's text stays as it was.
//! * The caret of the text area and the score cursor follow each other through the source map:
//!   the caret in a note's characters puts the cursor on that note, and a cursor on a note
//!   puts the caret at the start of its characters.

use std::ops::Range;

use cecilia_core::chant::{Chant, retranscribe, transcribe};
use cecilia_core::chant_source::ChantSource;

use crate::app::App;
use crate::properties::Field;

/// What the frontends mark in the lyrics text area of a chant score.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextMarks {
    /// The characters of the cursor's note (a selection-like background).
    pub note: Option<Range<usize>>,
    /// The character a parse error is at (in the error color).
    pub error: Option<usize>,
}

impl App {
    /// The chant of the cursor track and its kept text, when the buffer is a chant score and
    /// the text's map fits the chant.
    fn chant_with_source(&self) -> Option<(&Chant, &cecilia_core::chant_source::ChantSource)> {
        let score = &self.buffer().score;
        if !score.is_chant() {
            return None;
        }
        let chant = score
            .tracks
            .get(self.pane().cursor.track)?
            .chant
            .as_deref()?;
        let src = chant.source.as_ref().filter(|s| s.map.fits(chant))?;
        Some((chant, src))
    }

    /// The marks of the lyrics text area: the cursor note's characters, and the position of the
    /// parse error of the text being typed.
    pub fn lyrics_marks(&self) -> TextMarks {
        let typing = self
            .props
            .edit
            .as_ref()
            .filter(|e| e.field == Field::Lyrics);
        let error = typing.and_then(|e| e.error);
        let note = self.chant_with_source().and_then(|(_, src)| {
            // The map is the text's only while the text shown is the kept one.
            if typing.is_some_and(|e| e.text != src.text) {
                return None;
            }
            src.map.note_range(self.chant_cursor_note()?)
        });
        TextMarks { note, error }
    }

    /// Puts the score cursor on note `n` of the cursor track's chant.
    fn move_cursor_to_note(&mut self, n: usize) {
        let place = {
            let Some((chant, _)) = self.chant_with_source() else {
                return;
            };
            let Some((bar, beat)) = transcribe(chant).place_of(n) else {
                return;
            };
            (bar, beat, chant.note(n).map_or(0, |x| x.pitch))
        };
        let c = &mut self.pane_mut().cursor;
        c.bar = place.0;
        c.beat = place.1;
        c.line = place.2 as i32;
    }

    /// Keeps the text area's caret and the score cursor of a chant score in step. While the text
    /// is edited (and reads as the chant) the caret rules: the cursor goes to the note under or
    /// nearest before it. Otherwise the cursor rules: the caret to return to is the start of
    /// the cursor note's characters (left alone while it is within them).
    pub(crate) fn sync_chant_text(&mut self) {
        let track = self.pane().cursor.track;
        let typing = self
            .props
            .edit
            .as_ref()
            .filter(|e| e.field == Field::Lyrics)
            .map(|e| (e.caret, e.text.clone(), e.error));
        let Some((chant, src)) = self.chant_with_source() else {
            return;
        };
        match typing {
            Some((caret, text, None)) if text == src.text => {
                let Some(n) = src.map.note_at(caret) else {
                    return;
                };
                if self.chant_cursor_note() != Some(n) {
                    self.move_cursor_to_note(n);
                }
            }
            Some(_) => {}
            None => {
                let Some(n) = self.chant_cursor_note() else {
                    return;
                };
                let Some(range) = src.map.note_range(n) else {
                    return;
                };
                let kept = match self.props.text_caret() {
                    Some((t, caret)) if t == track => src.map.note_at(caret) == Some(n),
                    _ => false,
                };
                let _ = chant;
                if !kept {
                    self.props.set_text_caret_memory(track, range.start);
                }
            }
        }
    }

    /// After an edit of the score: a lyrics text area that is open shows the chant's kept text
    /// again (a score edit spliced into it), with the caret at the cursor note.
    pub(crate) fn follow_score_edit_in_text(&mut self) {
        let Some((_, src)) = self.chant_with_source() else {
            return;
        };
        let text = src.text.clone();
        let start = self
            .chant_cursor_note()
            .and_then(|n| src.map.note_range(n))
            .map(|r| r.start);
        let Some(e) = self
            .props
            .edit
            .as_mut()
            .filter(|e| e.field == Field::Lyrics && e.text != text)
        else {
            return;
        };
        e.text = text.clone();
        e.set_full(text);
        e.error = None;
        e.visual = None;
        e.anchor = None;
        e.caret = start.unwrap_or(0).min(e.len());
    }

    /// The text area's `full` text, typed GABC, is the chant's body: reads it, and when it
    /// parses replaces the chant (one undo step per edit session) keeping the ids of the notes
    /// that match; when it does not, the last good chant stays and the error is shown with its
    /// position (highlighted in the text area).
    pub(crate) fn set_chant_text(&mut self, track: usize, old: Chant, full: String) {
        use cecilia_formats::Error;
        match cecilia_formats::gabc::reparse_body(&old, &full) {
            Ok((mut new, map)) => {
                new.match_ids(&old);
                new.source = Some(ChantSource {
                    text: full.clone(),
                    map,
                });
                if let Some(e) = self.lyrics_edit_mut() {
                    e.error = None;
                    e.set_full(full.clone());
                }
                if self
                    .message
                    .as_ref()
                    .is_some_and(|m| m.error && m.text.starts_with("lyrics: "))
                {
                    self.message = None;
                }
                if new != old {
                    self.edit(|cx| {
                        let score = cx.whole();
                        score.tracks[track].chant = Some(Box::new(new));
                        retranscribe(score, track);
                        Ok(())
                    });
                } else if old.source.as_ref().map(|s| s.text.as_str()) != Some(full.as_str()) {
                    // Only the layout changed (spacing, a comment): no new undo step, but the
                    // file is changed.
                    let b = self.pane().buffer;
                    let buffer = &mut self.buffers[b];
                    if let Some(c) = buffer
                        .score
                        .tracks
                        .get_mut(track)
                        .and_then(|t| t.chant.as_deref_mut())
                    {
                        c.source = new.source;
                    }
                    buffer.modified = true;
                    buffer.version += 1;
                }
            }
            Err(Error::Parse {
                offset, message, ..
            }) => {
                let chars: Vec<char> = full.chars().collect();
                let before = &chars[..offset.min(chars.len())];
                let line = before.iter().filter(|&&c| c == '\n').count() + 1;
                let column = before.iter().rev().take_while(|&&c| c != '\n').count() + 1;
                if let Some(e) = self.lyrics_edit_mut() {
                    e.error = Some(offset);
                    e.set_full(full.clone());
                }
                self.error(format!(
                    "lyrics: {message} at character {offset} (line {line}, column {column})"
                ));
            }
            Err(e) => self.error(format!("lyrics: {e}")),
        }
        self.sync_chant_text();
    }

    fn lyrics_edit_mut(&mut self) -> Option<&mut crate::properties::PropEdit> {
        self.props
            .edit
            .as_mut()
            .filter(|e| e.field == Field::Lyrics)
    }
}
