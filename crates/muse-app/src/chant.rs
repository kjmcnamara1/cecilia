//! Chant commands: GABC editing of syllables, interpretation, clefs, conversion to and from
//! modern notation.

use muse_core::TrackKind;
use muse_core::chant::{self, NeumeItem, retranscribe, transcribe};

use crate::app::{App, Buffer};
use crate::command::Command;

impl App {
    /// Index of the syllable holding the cursor note on a chant track.
    fn cursor_syllable(&self) -> Option<usize> {
        let c = self.pane().cursor;
        let chant = self.buffer().score.tracks.get(c.track)?.chant.as_deref()?;
        let t = transcribe(chant);
        let resolved = chant.resolved();
        let k = t
            .positions
            .iter()
            .rposition(|&(bar, beat)| (bar, beat) <= (c.bar, c.beat))
            .unwrap_or(0);
        resolved.get(k).map(|r| r.syllable).or_else(|| {
            chant
                .syllables
                .iter()
                .position(|s| s.notes().next().is_some())
        })
    }

    fn require_chant(&mut self) -> bool {
        let c = self.pane().cursor;
        let ok = self
            .buffer()
            .score
            .tracks
            .get(c.track)
            .is_some_and(|t| t.chant.is_some());
        if !ok {
            self.error("not a chant track (open a .gabc file or use :modern-to-chant)");
        }
        ok
    }

    pub(crate) fn run_chant(&mut self, cmd: &Command) -> bool {
        use Command::*;
        match cmd {
            Gabc(arg) => {
                if !self.require_chant() {
                    return true;
                }
                let Some(si) = self.cursor_syllable() else {
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
                    match muse_formats::gabc::parse(text) {
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
                let notes = muse_formats::gabc::parse_notes(text, 1);
                self.edit(|cx| {
                    let t = cx.cursor.track;
                    let score = cx.whole();
                    let chant = score.tracks[t].chant.as_mut().unwrap();
                    match replacement {
                        Some(mut syllables) => {
                            syllables[0].word_start = chant.syllables[si].word_start;
                            chant.syllables.splice(si..=si, syllables);
                        }
                        None => chant.syllables[si].items = notes,
                    }
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
                    Some("equalist") => muse_core::chant::Interpretation::Equalist,
                    Some("solesmes") => muse_core::chant::Interpretation::Solesmes,
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
                self.edit(|cx| {
                    let t = cx.cursor.track;
                    let score = cx.whole();
                    let chant = score.tracks[t].chant.as_mut().unwrap();
                    let first = chant
                        .syllables
                        .iter_mut()
                        .flat_map(|s| s.items.iter_mut())
                        .find(|i| matches!(i, NeumeItem::Clef(_)));
                    match first {
                        Some(c) => *c = NeumeItem::Clef(clef),
                        None => match chant.syllables.first_mut() {
                            Some(s) => s.items.insert(0, NeumeItem::Clef(clef)),
                            None => return Err("empty chant".into()),
                        },
                    }
                    retranscribe(score, t);
                    Ok(())
                });
            }
            ChantToModern => {
                if !self.require_chant() {
                    return true;
                }
                self.edit(|cx| {
                    let t = cx.cursor.track;
                    let track = &mut cx.whole().tracks[t];
                    track.chant = None;
                    track.kind = TrackKind::Pitched;
                    Ok(())
                });
                self.info("chant track is now a staff track (its transcription)");
            }
            ModernToChant => {
                let c = self.pane().cursor;
                let score = &self.buffer().score;
                match muse_convert::chant::modern_to_chant(score, c.track) {
                    Ok(r) => {
                        let mut new = muse_formats::gabc::chant_score(r.chant);
                        new.meta.title = score.meta.title.clone();
                        let name = format!("[chant] {}", self.buffer().name);
                        self.buffers.push(Buffer {
                            path: None,
                            name,
                            score: new,
                            modified: true,
                            history: Default::default(),
                            version: 0,
                        });
                        let b = self.buffers.len() - 1;
                        self.show_buffer(b);
                        self.info(format!(
                            "chant (lossy): transposed {:+} semitones, {} notes rounded, {} chords reduced; :w name.gabc to save",
                            r.transpose, r.rounded, r.chords
                        ));
                    }
                    Err(e) => self.error(format!("modern-to-chant: {e}")),
                }
            }
            ChantView => {
                self.chant_view = !self.chant_view;
                self.info(if self.chant_view {
                    "chant: square notation"
                } else {
                    "chant: modern transcription"
                });
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

    fn app() -> App {
        let mut app = App::new(Config::default(), None);
        let chant = muse_formats::gabc::parse("name: t;\n%%\n(c4) Al(fg)le(h) lu(g)ia.(f.) (::)\n")
            .unwrap();
        app.buffers[0].score = muse_formats::gabc::chant_score(chant);
        app
    }

    fn keys(app: &mut App, s: &str) {
        for k in crate::keys::parse_keys(s, &[Key::char(' ')]).unwrap() {
            app.handle_key(k);
        }
    }

    fn gabc(app: &App) -> String {
        let c = app.buffer().score.tracks[0].chant.as_ref().unwrap();
        c.syllables
            .iter()
            .map(|s| s.gabc())
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn editing_keys_change_the_chant() {
        let mut app = app();
        // Raise the second note (g = G4): A♭ is not in the scale and rounds up to A (h),
        // then A + 1 is B♭, a flat on i.
        keys(&mut app, "l<C-a>");
        assert_eq!(gabc(&app), "(c4) Al(fh) le(h) lu(g) ia.(f.) (::)");
        keys(&mut app, "<C-a>");
        assert_eq!(gabc(&app), "(c4) Al(fixi) le(h) lu(g) ia.(f.) (::)");
        // Delete the first note, then undo all three edits.
        keys(&mut app, "0x");
        assert_eq!(gabc(&app), "(c4) Al(ixi) le(h) lu(g) ia.(f.) (::)");
        keys(&mut app, "uuu");
        assert_eq!(gabc(&app), "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)");
    }

    #[test]
    fn gabc_and_interpretation_commands() {
        let mut app = app();
        app.run(Command::Gabc(None));
        assert_eq!(app.message.as_ref().unwrap().text, "Al(fg)");
        app.run(Command::Gabc(Some("ghg".into())));
        assert_eq!(gabc(&app), "(c4) Al(ghg) le(h) lu(g) ia.(f.) (::)");
        app.run(Command::Gabc(Some("Glo(f)ri(gh)".into())));
        assert_eq!(gabc(&app), "(c4) Glo(f) ri(gh) le(h) lu(g) ia.(f.) (::)");
        let beats = |app: &App| app.buffer().score.tracks[0].bars[0].voices[0].beats.len();
        assert_eq!(beats(&app), 6);
        app.run(Command::Interpretation(Some("solesmes".into())));
        assert_eq!(
            app.buffer().score.tracks[0]
                .chant
                .as_ref()
                .unwrap()
                .interpretation,
            muse_core::chant::Interpretation::Solesmes
        );
        app.run(Command::ChantClef("c3".into()));
        assert!(gabc(&app).starts_with("(c3)"));
        app.run(Command::ChantToModern);
        let t = &app.buffer().score.tracks[0];
        assert!(t.chant.is_none() && t.kind == TrackKind::Pitched);
        app.run(Command::ModernToChant);
        assert_eq!(app.buffers.len(), 2);
        assert!(app.buffer().score.tracks[0].chant.is_some());
    }
}
