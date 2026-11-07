//! Chant commands: editing the notes of a chant score through the operations on [`Chant`],
//! syllables as GABC, interpretation, clefs, conversion to and from modern notation.
//!
//! A chant score's chant is its source; the bars are a cache made by `retranscribe` after every
//! edit. Nothing here (or anywhere) edits those bars: keys that would are mapped to a chant
//! operation or refused with "not in chant".

use cecilia_core::chant::{self, Chant, Liquescence, Shape, retranscribe, transcribe};
use cecilia_core::chant_edit::{Alter, Join};

use crate::app::{App, Buffer};
use crate::command::Command;

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
        self.chant_op(move |chant, at| {
            let n = at.ok_or("no note under the cursor")?;
            let now = chant.alteration_of(n);
            chant.set_alteration(n, if now == Some(want) { None } else { Some(want) })?;
            Ok(Some(n))
        });
    }

    /// Toggles a GABC modifier of the cursor note (see [`toggle_modifier`]).
    fn chant_toggle(&mut self, modifier: &'static str) {
        self.chant_op(move |chant, at| {
            let n = at.ok_or("no note under the cursor")?;
            toggle_modifier(chant, n, modifier)?;
            Ok(Some(n))
        });
    }

    /// Index (in reading order) of the chant note under the cursor: the note sounded by the
    /// cursor beat, else the last one before it. None in a chant without notes.
    pub(crate) fn chant_cursor_note(&self) -> Option<usize> {
        let c = self.pane().cursor;
        let chant = self.buffer().score.tracks.get(c.track)?.chant.as_deref()?;
        transcribe(chant).note_near(c.bar, c.beat)
    }

    /// Edits the cursor track's chant as one undo step and writes its bars again. The closure
    /// gets the cursor note (None in a chant without notes) and returns the note the cursor
    /// should be on afterwards (None leaves the cursor where it is).
    fn chant_op(
        &mut self,
        f: impl FnOnce(&mut Chant, Option<usize>) -> Result<Option<usize>, String>,
    ) {
        if !self.require_chant() {
            return;
        }
        let at = self.chant_cursor_note();
        self.edit(|cx| {
            let t = cx.cursor.track;
            let score = cx.whole();
            let mut chant = (**score.tracks[t].chant.as_ref().ok_or("not a chant track")?).clone();
            let target = f(&mut chant, at)?;
            let placed = target.and_then(|k| {
                let place = transcribe(&chant).place_of(k)?;
                Some((place, chant.note(k)?.pitch))
            });
            score.tracks[t].chant = Some(Box::new(chant));
            retranscribe(score, t);
            if let Some(((bar, beat), pitch)) = placed {
                cx.cursor.bar = bar;
                cx.cursor.beat = beat;
                cx.cursor.line = pitch as i32;
            }
            Ok(())
        });
    }

    /// The cursor note and its pitch, for a new note to start at.
    fn pitch_of(chant: &Chant, at: Option<usize>) -> u8 {
        at.and_then(|n| chant.note(n)).map_or(6, |n| n.pitch)
    }

    /// Inserts a note next to the cursor note.
    fn chant_insert(&mut self, after: bool, join: Join) {
        self.chant_op(move |chant, at| {
            let pitch = Self::pitch_of(chant, at);
            chant
                .insert_note(at.unwrap_or(0), after, join, pitch)
                .map(Some)
        });
    }

    /// Deletes the notes sounding in the beats `from` to `to` (a Visual selection, `d` with a
    /// motion).
    pub(crate) fn chant_delete_range(&mut self, from: (usize, usize), to: (usize, usize)) {
        self.chant_op(move |chant, _| {
            let t = transcribe(chant);
            let doomed: Vec<usize> = (0..t.positions.len())
                .filter(|&k| {
                    let (bar, beat) = t.positions[k];
                    let last = (bar, beat + t.lengths[k] as usize - 1);
                    last >= from && (bar, beat) <= to
                })
                .collect();
            if doomed.is_empty() {
                return Err("no notes there".into());
            }
            for &k in doomed.iter().rev() {
                chant.delete_note(k)?;
            }
            let left = chant.note_count();
            Ok((left > 0).then(|| doomed[0].min(left - 1)))
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
                self.chant_op(move |chant, at| {
                    let n = at.ok_or("no note under the cursor")?;
                    toggle_modifier(chant, n, m)?;
                    Ok(Some(n))
                });
            }
            DurationLonger | DurationShorter => {
                let on = matches!(cmd, DurationLonger);
                self.chant_op(move |chant, at| {
                    let n = at.ok_or("no note under the cursor")?;
                    chant.set_episema(n, on)?;
                    Ok(Some(n))
                });
            }
            DeleteNote | DeleteBeat => self.chant_op(|chant, at| {
                let n = at.ok_or("no note under the cursor")?;
                chant.delete_note(n)?;
                let left = chant.note_count();
                Ok((left > 0).then(|| n.saturating_sub(1)))
            }),
            // A new beat is a new note.
            InsertNext | BeatNew => self.chant_insert(true, Join::Neume),
            InsertEnter => self.chant_insert(true, Join::NewSyllable),
            // `i` `a` `I` `A` open no beat in chant: they only start Insert mode, where
            // `<Space>` adds a note.
            InsertBeat(_) => {
                self.ed.group = Some(false);
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
            ChantMod(m) => {
                let m = m.clone();
                self.chant_op(move |chant, at| {
                    let n = at.ok_or("chant-mod: no note under the cursor")?;
                    toggle_modifier(chant, n, &m)?;
                    Ok(Some(n))
                });
            }
            ChantShape(name) => {
                let Some(shape) = shape_by_name(name) else {
                    return true;
                };
                self.chant_op(move |chant, at| {
                    let n = at.ok_or("no note under the cursor")?;
                    let now = chant.note(n).ok_or("no note under the cursor")?.shape();
                    chant.set_shape(n, if now == shape { Shape::Punctum } else { shape })?;
                    Ok(Some(n))
                });
            }
            ChantDivision(sign) => {
                let sign = sign.clone();
                self.chant_op(move |chant, at| {
                    let n = at.unwrap_or(0);
                    match chant.division_after(n) {
                        Some(d) if d == sign => chant.delete_division(n)?,
                        Some(_) => chant.set_division(n, &sign)?,
                        None => {
                            chant.insert_division(n, &sign)?;
                        }
                    }
                    Ok(at)
                });
            }
            ChantAlter(name) => {
                let want = if name == "flat" {
                    Alter::Flat
                } else {
                    Alter::Natural
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
            ScoreKind => {
                let msg = if self.cursor_on_chant() {
                    "chant score; :chant-to-modern opens it as a modern score in a new buffer"
                } else {
                    "modern score; :modern-to-chant opens the cursor track's melody as a chant score in a new buffer"
                };
                self.info(msg);
            }
            ChantAppend => self.chant_insert(true, Join::Neume),
            ChantBreak => self.chant_insert(true, Join::NewNeume),
            ChantSyllable => self.chant_insert(true, Join::NewSyllable),
            ChantBar(sign) => {
                let sign = sign.clone();
                self.chant_op(move |chant, at| {
                    let n = at.unwrap_or(0);
                    // A second `:` makes the double bar `::`, a third takes it away.
                    if sign == ":" {
                        match chant.division_after(n) {
                            Some(":") => {
                                chant.set_division(n, "::")?;
                                return Ok(at);
                            }
                            Some("::") => {
                                chant.delete_division(n)?;
                                return Ok(at);
                            }
                            _ => {}
                        }
                    }
                    chant.insert_division(n, &sign)?;
                    Ok(at)
                });
            }
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
                    chant.inherit_ids(&old);
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
                     the chant is unchanged; :w name.cc to save",
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
    fn chant_insert_mode() {
        let mut a = app();
        // The first note of "Al" becomes a virga.
        keys(&mut a, "iv");
        assert_eq!(gabc(&a), "(c4) Al(fvg) le(h) lu(g) ia.(f.) (::)");
        // A new note after it; then a new neume.
        keys(&mut a, "<Space>");
        assert_eq!(gabc(&a), "(c4) Al(fvfg) le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, "/");
        assert_eq!(gabc(&a), "(c4) Al(fvf/fg) le(h) lu(g) ia.(f.) (::)");
        // Deleting it leaves g starting the neume; a division follows the syllable.
        keys(&mut a, "<BS>");
        assert_eq!(gabc(&a), "(c4) Al(fvf/g) le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, ";");
        assert_eq!(gabc(&a), "(c4) Al(fvf/g) (;) le(h) lu(g) ia.(f.) (::)");
        // `:` cycles major, double, none.
        keys(&mut a, "::");
        assert_eq!(gabc(&a), "(c4) Al(fvf/g) (::) (;) le(h) lu(g) ia.(f.) (::)");
        keys(&mut a, ":");
        assert_eq!(gabc(&a), "(c4) Al(fvf/g) (;) le(h) lu(g) ia.(f.) (::)");
        assert_cache(&a);
        keys(&mut a, "<Esc>uuuuuu");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)");
        assert_cache(&a);
    }

    #[test]
    fn a_chant_track_has_no_staves_to_change() {
        let mut a = app();
        let before = a.buffer().score.tracks[0].clone();
        a.run(Command::Notation("treble".into()));
        assert!(a.message.as_ref().unwrap().error);
        a.run(Command::StaffAdd(Some("tab".into())));
        assert!(a.message.as_ref().unwrap().error);
        assert_eq!(a.buffer().score.tracks[0], before);
        // `chant` is not a notation of an ordinary track either.
        a.run(Command::ChantToModern);
        a.run(Command::Notation("chant".into()));
        assert!(a.message.as_ref().unwrap().error);
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
        assert!(a.message.as_ref().unwrap().error);
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
        assert!(a.message.as_ref().unwrap().error);
        assert_eq!(gabc(&a), "(c4) Al(fn) le(h) lu(g) ia.(f.) (::)");
        run(&mut a, "octave-down");
        run(&mut a, "octave-down");
        assert!(a.message.as_ref().unwrap().error);
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
        assert!(a.message.as_ref().unwrap().text.contains("not in chant"));
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
        assert_eq!(a.buffer().score.tracks[0].bars[0].voices[0].beats.len(), 6);
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
        assert!(text.starts_with("(c4) Al(fg)le(h)"), "{text}");
        // A division after the cursor note's syllable splits the bar and moves the notes
        // after it; the cursor stays on its note.
        go(&mut a, 0, 1);
        run(&mut a, "chant-bar ,");
        assert_eq!(
            gabc(&a),
            "(c4) Al(fg) (,) le(h) (;) lu(gg/g) (g) ia.(f.) (::)"
        );
        assert_eq!(cursor_note(&a), Some(1));
        assert_eq!(at(&a), (0, 1));
        assert_eq!(a.buffer().score.tracks[0].bars.len(), 3);
        assert_cache(&a);
        // Deleting a note: the cursor goes to the one before it, even across a bar.
        go(&mut a, 1, 0);
        keys(&mut a, "x");
        assert_eq!(
            gabc(&a),
            "(c4) Al(fg) (,) le() (;) lu(gg/g) (g) ia.(f.) (::)"
        );
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
        keys(&mut a, "lvld");
        assert_eq!(gabc(&a), "(c4) Al(f) le() lu(g) ia.(f.) (::)");
        assert_cache(&a);
        keys(&mut a, "u");
        assert_eq!(gabc(&a), "(c4) Al(fg) le(h) lu(g) ia.(f.) (::)");
        // Changing or pasting over bars is not something chant does.
        keys(&mut a, "vlc");
        assert!(a.message.as_ref().unwrap().error);
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
            a.message = None;
            let Ok(c) = Command::parse(cmd) else {
                panic!("{cmd} does not parse");
            };
            a.run(c);
            let m = a
                .message
                .clone()
                .unwrap_or_else(|| panic!("{cmd}: no message"));
            assert!(
                m.error && m.text.contains("not in chant"),
                "{cmd}: {}",
                m.text
            );
            assert!(same(&a, &before), "{cmd}");
        }
        // Digits do not type frets on a chant track either.
        keys(&mut a, "i1");
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
        let m = a.message.clone().unwrap();
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
                .map(|beat| beat.notes.iter().map(|n| (n.string, n.fret)).collect())
                .collect()
        };
        assert_eq!(pitches(new), pitches(old));
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
        assert!(a.buffer().score.is_chant(), "{:?}", a.message);
        let msg = a.message.as_ref().unwrap().text.clone();
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
        assert!(a.message.as_ref().unwrap().error);
        assert!(
            a.message
                .as_ref()
                .unwrap()
                .text
                .contains(":chant-to-modern")
        );
        assert_eq!(a.buffer().score, before);
        // Its only track stays too.
        a.run(Command::TrackDelete);
        assert!(a.message.as_ref().unwrap().error);
        assert_eq!(a.buffer().score, before);
        // A modern score takes no chant track.
        let mut m = App::new(Config::default(), None);
        m.run(Command::New);
        let before = m.buffer().score.clone();
        m.run(Command::TrackAdd("chant".into()));
        assert!(m.message.as_ref().unwrap().error);
        assert!(
            m.message
                .as_ref()
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
