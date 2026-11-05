//! Chant commands: GABC editing of syllables, interpretation, clefs, conversion to and from
//! modern notation.

use cecilia_core::TrackKind;
use cecilia_core::chant::{
    self, Chant, NeumeItem, Syllable, letter_position, retranscribe, transcribe,
};

use crate::app::{App, Buffer};
use crate::command::Command;
use crate::config::InsertKind;

/// A chant note by place: (syllable, item).
type NotePlace = (usize, usize);

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

    /// The chant note under the cursor: (syllable, item) of the last note sounding at or
    /// before the cursor beat.
    fn chant_cursor_note(&self) -> Option<NotePlace> {
        let c = self.pane().cursor;
        let chant = self.buffer().score.tracks.get(c.track)?.chant.as_deref()?;
        let t = transcribe(chant);
        let k = t
            .positions
            .iter()
            .rposition(|&(bar, beat)| (bar, beat) <= (c.bar, c.beat))?;
        chant.resolved().get(k).map(|r| (r.syllable, r.item))
    }

    /// Edits the cursor track's chant, written out again afterwards; the cursor goes to the
    /// note the edit returns.
    fn chant_edit(
        &mut self,
        f: impl FnOnce(&mut Chant, Option<NotePlace>) -> Result<Option<NotePlace>, String>,
    ) {
        if !self.require_chant() {
            return;
        }
        let at = self.chant_cursor_note();
        self.edit(|cx| {
            let t = cx.cursor.track;
            let score = cx.whole();
            let mut chant = (**score.tracks[t].chant.as_ref().unwrap()).clone();
            let target = f(&mut chant, at)?;
            let written = transcribe(&chant);
            let resolved = chant.resolved();
            score.tracks[t].chant = Some(Box::new(chant));
            retranscribe(score, t);
            if let Some(k) =
                target.and_then(|p| resolved.iter().position(|r| (r.syllable, r.item) == p))
            {
                let (bar, beat) = written.positions[k];
                cx.cursor.bar = bar;
                cx.cursor.beat = beat;
                let chant = cx.score.tracks[t].chant.as_deref().unwrap();
                let (si, ii) = (resolved[k].syllable, resolved[k].item);
                if let Some(NeumeItem::Note(n)) = chant.syllables[si].items.get(ii) {
                    cx.cursor.line = n.pitch as i32;
                }
            }
            Ok(())
        });
    }

    pub(crate) fn run_chant(&mut self, cmd: &Command) -> bool {
        use Command::*;
        // A chant staff of an ordinary track edits its notes, keeping chant signs with them.
        let c = self.pane().cursor;
        let ordinary = self
            .buffer()
            .score
            .tracks
            .get(c.track)
            .is_some_and(|t| t.chant.is_none());
        let duration = self.insert_duration();
        let policy = self.chord_policy();
        match cmd {
            ChantNote(ch) if ordinary => {
                let pos = letter_position(*ch).unwrap_or(0);
                let upper = ch.is_ascii_uppercase();
                self.edit(|cx| crate::edit::put_chant_note(cx, policy, pos, upper, duration));
                return true;
            }
            ChantMod(m) if ordinary => {
                self.edit(|cx| crate::edit::toggle_chant_mark(cx, policy, m));
                return true;
            }
            ChantBar(sign) if ordinary => {
                self.edit(|cx| {
                    crate::edit::set_chant_division(cx, sign);
                    Ok(())
                });
                return true;
            }
            ChantAppend | ChantBreak if ordinary => {
                let neume = matches!(cmd, ChantBreak);
                self.edit(|cx| crate::edit::append_chant_note(cx, policy, neume, duration));
                return true;
            }
            ChantSyllable if ordinary => {
                self.error(
                    "chant-syllable: on this track, syllables come from its lyrics (:lyrics)",
                );
                return true;
            }
            _ => {}
        }
        match cmd {
            ChantNote(c) => {
                let pitch = letter_position(*c).unwrap_or(0);
                let inclinatum = c.is_ascii_uppercase();
                self.chant_edit(|chant, at| {
                    let Some((si, ii)) = at else {
                        // No note yet: the first one, in the first syllable.
                        if chant.syllables.is_empty() {
                            chant.syllables.push(Syllable::default());
                        }
                        let items = &mut chant.syllables[0].items;
                        items.push(NeumeItem::Note(chant::ChantNote {
                            pitch,
                            inclinatum,
                            ..chant::ChantNote::default()
                        }));
                        return Ok(Some((0, items.len() - 1)));
                    };
                    if let Some(NeumeItem::Note(n)) = chant.syllables[si].items.get_mut(ii) {
                        n.pitch = pitch;
                        n.inclinatum = inclinatum;
                    }
                    Ok(Some((si, ii)))
                });
            }
            ChantMod(m) => {
                let m = m.clone();
                self.chant_edit(|chant, at| {
                    let (si, ii) = at.ok_or("chant-mod: no note under the cursor")?;
                    if let Some(NeumeItem::Note(n)) = chant.syllables[si].items.get_mut(ii) {
                        if m == "-" {
                            n.initio_debilis = !n.initio_debilis;
                        } else if let Some(i) = n.mods.iter().position(|x| *x == m) {
                            n.mods.remove(i);
                        } else {
                            n.mods.push(m);
                        }
                    }
                    Ok(Some((si, ii)))
                });
            }
            ChantAppend | ChantBreak => {
                let brk = matches!(cmd, ChantBreak);
                self.chant_edit(|chant, at| {
                    let (si, ii) = at.ok_or("no note to add after")?;
                    let pitch = match chant.syllables[si].items.get(ii) {
                        Some(NeumeItem::Note(n)) => n.pitch,
                        _ => 6,
                    };
                    let items = &mut chant.syllables[si].items;
                    let mut at = ii + 1;
                    if brk {
                        items.insert(at, NeumeItem::Space("/".into()));
                        at += 1;
                    }
                    items.insert(
                        at,
                        NeumeItem::Note(chant::ChantNote {
                            pitch,
                            ..chant::ChantNote::default()
                        }),
                    );
                    Ok(Some((si, at)))
                });
            }
            ChantSyllable => {
                self.chant_edit(|chant, at| {
                    let (si, pitch) = match at {
                        Some((si, ii)) => match chant.syllables[si].items.get(ii) {
                            Some(NeumeItem::Note(n)) => (si, n.pitch),
                            _ => (si, 6),
                        },
                        None => (chant.syllables.len().saturating_sub(1), 6),
                    };
                    let at = (si + 1).min(chant.syllables.len());
                    chant.syllables.insert(
                        at,
                        Syllable {
                            items: vec![NeumeItem::Note(chant::ChantNote {
                                pitch,
                                ..chant::ChantNote::default()
                            })],
                            ..Syllable::default()
                        },
                    );
                    Ok(Some((at, 0)))
                });
            }
            ChantBar(sign) => {
                let sign = sign.clone();
                self.chant_edit(|chant, at| {
                    let si = at.map_or(chant.syllables.len().saturating_sub(1), |p| p.0);
                    // A second `:` makes the double bar `::`.
                    if let Some(next) = chant.syllables.get_mut(si + 1) {
                        if sign == ":" && next.items == [NeumeItem::Bar(":".into())] {
                            next.items[0] = NeumeItem::Bar("::".into());
                            return Ok(at);
                        }
                    }
                    let at_bar = (si + 1).min(chant.syllables.len());
                    chant.syllables.insert(
                        at_bar,
                        Syllable {
                            word_start: true,
                            items: vec![NeumeItem::Bar(sign)],
                            ..Syllable::default()
                        },
                    );
                    Ok(at)
                });
            }
            DeleteNote if !ordinary && self.on_staff_kind(InsertKind::Chant) => {
                self.chant_edit(|chant, at| {
                    let (si, ii) = at.ok_or("no note under the cursor")?;
                    let syl = &mut chant.syllables[si];
                    syl.items.remove(ii);
                    // A neume break left at the end goes too.
                    if ii > 0 && ii == syl.items.len() {
                        if let Some(NeumeItem::Space(_)) = syl.items.last() {
                            syl.items.pop();
                        }
                    }
                    let notes_left = syl.notes().next().is_some();
                    if !notes_left && syl.text.trim().is_empty() {
                        chant.syllables.remove(si);
                        return Ok(None);
                    }
                    let prev = (0..ii.min(syl.items.len()))
                        .rev()
                        .find(|&j| matches!(syl.items[j], NeumeItem::Note(_)));
                    Ok(prev.map(|j| (si, j)))
                });
            }
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
                let policy = self.chord_policy();
                let score = &self.buffer().score;
                match cecilia_convert::chant::modern_to_chant(score, c.track, policy) {
                    Ok(r) => {
                        let mut new = cecilia_formats::gabc::chant_score(r.chant);
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
        let chant =
            cecilia_formats::gabc::parse("name: t;\n%%\n(c4) Al(fg)le(h) lu(g)ia.(f.) (::)\n")
                .unwrap();
        app.buffers[0].score = cecilia_formats::gabc::chant_score(chant);
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
    fn chant_insert_mode() {
        let mut a = app();
        // The first note of "Al" becomes h, then a virga.
        keys(&mut a, "ih");
        assert_eq!(gabc(&a), "(c4) Al(hg) le(h) lu(g) ia.(f.) (::)");
        assert_eq!(a.pane().cursor.line, 7);
        keys(&mut a, "v");
        assert_eq!(gabc(&a), "(c4) Al(hvg) le(h) lu(g) ia.(f.) (::)");
        // A new note after it, set to j; then a new neume.
        keys(&mut a, "<Space>j");
        assert_eq!(gabc(&a), "(c4) Al(hvjg) le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, "/k");
        assert_eq!(gabc(&a), "(c4) Al(hvj/kg) le(h) lu(g) ia.(f.) (::)");
        // Deleting it leaves g starting the neume; a division follows the syllable.
        keys(&mut a, "<BS>");
        assert_eq!(gabc(&a), "(c4) Al(hvj/g) le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, ";");
        assert_eq!(gabc(&a), "(c4) Al(hvj/g) (;) le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, "<Esc>uuuuuu");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)");
    }

    #[test]
    fn chant_staves_are_views() {
        let mut a = app();
        let chant = a.buffer().score.tracks[0].chant.clone();
        // Shown as standard notation and tab, the chant itself stays.
        a.run(Command::Notation("treble".into()));
        a.run(Command::StaffAdd(Some("chant".into())));
        let t = &a.buffer().score.tracks[0];
        assert_eq!(t.chant, chant);
        assert_eq!(
            t.staves(),
            vec![
                cecilia_core::Staff::Standard(cecilia_core::Clef::Treble),
                cecilia_core::Staff::Chant
            ]
        );
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
            cecilia_core::chant::Interpretation::Solesmes
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

    #[test]
    fn modern_chant_staff_takes_a_clef_line() {
        use cecilia_core::chant::ClefKind;
        let mut app = app();
        app.run(Command::ChantToModern);
        app.run(Command::StaffAdd(Some("chant".into())));
        app.run(Command::StaffNext);
        let view = |app: &App| {
            let b = app.buffer();
            cecilia_core::melody::ChantView::build(&b.score, &b.score.tracks[0], Default::default())
                .unwrap()
        };
        app.run(Command::Clef("c3".into()));
        assert_eq!(app.buffer().score.tracks[0].chant_line, Some(3));
        assert_eq!(
            (view(&app).clef.kind, view(&app).clef.line),
            (ClefKind::Do, 3)
        );
        let pitches = |app: &App| view(app).events.iter().map(|e| e.pitch).collect::<Vec<_>>();
        let before = pitches(&app);
        app.run(Command::Clef("f2".into()));
        assert_eq!(
            (view(&app).clef.kind, view(&app).clef.line),
            (ClefKind::Fa, 2)
        );
        assert_eq!(pitches(&app), before, "same sounding pitches");
        app.run(Command::Clef("do".into()));
        assert_eq!(
            (view(&app).clef.kind, view(&app).clef.line),
            (ClefKind::Fa, 2),
            "do is no longer a clef"
        );
    }
}
