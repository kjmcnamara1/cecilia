//! Score edits. An [`EditCx`] records what an edit touches so it can be undone: the original
//! of every bar it modifies, or a whole-score snapshot for structural edits.

use std::collections::BTreeMap;

use muse_core::melody::{self, ChordPolicy};
use muse_core::*;

use crate::Cursor;
use crate::app::uses_strings;
use crate::history::{BarChange, Change};

pub struct EditCx<'a> {
    pub score: &'a mut Score,
    pub cursor: Cursor,
    before: BTreeMap<(usize, usize), Bar>,
    snapshot: Option<Score>,
}

impl<'a> EditCx<'a> {
    pub fn new(score: &'a mut Score, cursor: Cursor) -> Self {
        Self {
            score,
            cursor,
            before: BTreeMap::new(),
            snapshot: None,
        }
    }

    /// A bar to modify (its original is kept for undo).
    pub fn bar(&mut self, t: usize, b: usize) -> &mut Bar {
        if self.snapshot.is_none() {
            let original = &self.score.tracks[t].bars[b];
            self.before
                .entry((t, b))
                .or_insert_with(|| original.clone());
        }
        &mut self.score.tracks[t].bars[b]
    }

    /// The whole score, for structural edits (a full snapshot is kept for undo).
    pub fn whole(&mut self) -> &mut Score {
        if self.snapshot.is_none() {
            let mut original = self.score.clone();
            for (&(t, b), bar) in &self.before {
                original.tracks[t].bars[b] = bar.clone();
            }
            self.snapshot = Some(original);
        }
        self.score
    }

    /// Carries edits of a chant track's transcription back into its chant notation and
    /// rewrites the transcription from it.
    pub fn sync_chant(&mut self) {
        let t = self.cursor.track;
        let Some(chant) = self.score.tracks.get(t).and_then(|x| x.chant.as_deref()) else {
            return;
        };
        let expected = muse_core::chant::transcribe(chant);
        if expected.bars == self.score.tracks[t].bars
            && expected.master_bars == self.score.master_bars
        {
            return;
        }
        let bars = self.score.tracks[t].bars.clone();
        let score = self.whole();
        let mut chant = (**score.tracks[t].chant.as_ref().unwrap()).clone();
        muse_convert::chant::reconcile(&mut chant, &bars);
        score.tracks[t].chant = Some(Box::new(chant));
        muse_core::chant::retranscribe(score, t);
    }

    /// The recorded change, if anything changed.
    pub fn finish(self) -> Option<Change> {
        if let Some(before) = self.snapshot {
            return (before != *self.score).then(|| Change::Score {
                before: Box::new(before),
                after: Box::new(self.score.clone()),
            });
        }
        let changes: Vec<BarChange> = self
            .before
            .into_iter()
            .filter(|((t, b), before)| self.score.tracks[*t].bars[*b] != *before)
            .map(|((t, b), before)| BarChange {
                track: t,
                bar: b,
                before,
                after: self.score.tracks[t].bars[b].clone(),
            })
            .collect();
        (!changes.is_empty()).then_some(Change::Bars(changes))
    }

    pub fn track(&self) -> &Track {
        &self.score.tracks[self.cursor.track]
    }

    /// The cursor's voice, created (with an empty placeholder beat) if missing.
    pub fn voice(&mut self) -> &mut Vec<Beat> {
        let (t, b, v) = (self.cursor.track, self.cursor.bar, self.cursor.voice);
        let bar = self.bar(t, b);
        while bar.voices.len() <= v {
            bar.voices.push(Voice {
                beats: vec![Beat {
                    empty: true,
                    ..Beat::default()
                }],
            });
        }
        if bar.voices[v].beats.is_empty() {
            bar.voices[v].beats.push(Beat {
                empty: true,
                ..Beat::default()
            });
        }
        &mut bar.voices[v].beats
    }

    /// The beat under the cursor (clamped).
    pub fn beat(&mut self) -> &mut Beat {
        let i = self.cursor.beat;
        let beats = self.voice();
        let i = i.min(beats.len() - 1);
        &mut beats[i]
    }

    /// String (or note index) of the note the cursor is on: on a staff, the note at the
    /// cursor's position (the beat's note on a slash staff), none if there is no note there;
    /// on a tab, the cursor's string.
    fn string(&self) -> u8 {
        if self.on_lines() {
            self.note_at_staff(self.cursor.line).unwrap_or(u8::MAX)
        } else {
            self.cursor.string as u8
        }
    }

    /// The cursor's staff.
    fn staff(&self) -> Option<Staff> {
        let track = self.score.tracks.get(self.cursor.track)?;
        track
            .staves()
            .get(crate::app::staff_index(track, self.cursor.staff))
            .copied()
    }

    /// Whether the cursor is on a staff's lines (not a tab's strings).
    fn on_lines(&self) -> bool {
        let track = &self.score.tracks[self.cursor.track];
        self.staff().is_some_and(|s| !uses_strings(track, s))
    }

    /// Key signature of the cursor bar.
    fn key(&self) -> i8 {
        self.score
            .master_bars
            .get(self.cursor.bar)
            .map_or(0, |m| m.key_signature.accidentals)
    }

    /// Diatonic staff position of a note on the cursor track.
    fn staff_position(&self, n: &Note) -> i32 {
        let track = &self.score.tracks[self.cursor.track];
        let written = track.pitch_of(n) as i32 + track.staff_clef().transpose();
        staff::diatonic(written, self.key())
    }

    /// String of the cursor beat's note at a staff position.
    fn note_at_staff(&self, p: i32) -> Option<u8> {
        let track = self.score.tracks.get(self.cursor.track)?;
        let beat = track
            .bars
            .get(self.cursor.bar)?
            .voices
            .get(self.cursor.voice)?
            .beats
            .get(self.cursor.beat)?;
        // Slashes and chant show one note per beat.
        if matches!(self.staff(), Some(s) if s == Staff::Slash || s.is_chant()) {
            return beat.notes.first().map(|n| n.string);
        }
        beat.notes
            .iter()
            .find(|n| self.staff_position(n) == p)
            .map(|n| n.string)
    }
}

/// Gives the notes of a beat on a non-fretted track distinct strings, highest pitch first.
fn renumber_pitched(beat: &mut Beat) {
    beat.notes.sort_by_key(|n| std::cmp::Reverse(n.fret));
    for (i, n) in beat.notes.iter_mut().enumerate() {
        n.string = i as u8;
    }
}

/// Puts the note at a written staff position (diatonic) and alteration into the cursor beat:
/// replacing the note the cursor is on, or added to the chord. The cursor moves onto it.
fn put_staff_note(cx: &mut EditCx, d: i32, alter: i32, duration: Duration) -> Result<(), String> {
    let track = cx.track().clone();
    let written = staff::letter_midi(d, 0) + alter;
    let pitch = written - track.staff_clef().transpose();
    let pitch = u8::try_from(pitch)
        .ok()
        .filter(|p| *p <= 127)
        .ok_or("note out of range")?;
    let under = if cx.on_lines() {
        cx.note_at_staff(cx.cursor.line)
    } else {
        None
    };
    ensure_real(cx.beat(), duration);
    if track.kind == TrackKind::Fretted {
        // set_pitch replaces the note on the cursor string: that of the note under the
        // cursor, or a free string for a new one.
        let string = match under {
            Some(s) => s,
            None => {
                let used: Vec<u8> = cx.beat().notes.iter().map(|n| n.string).collect();
                (0..track.strings.len() as u8)
                    .find(|s| !used.contains(s))
                    .ok_or("no free string for another note")?
            }
        };
        set_pitch_on(cx, pitch, string)?;
    } else {
        let beat = cx.beat();
        match under.and_then(|s| beat.notes.iter_mut().find(|n| n.string == s)) {
            Some(n) => {
                n.fret = pitch;
                n.tie = false;
            }
            None => beat.notes.push(Note::new(u8::MAX, pitch)),
        }
        renumber_pitched(beat);
    }
    cx.cursor.line = d;
    Ok(())
}

/// The note of a letter (step 0 = C) nearest the cursor's staff position, in the key.
pub fn put_letter(cx: &mut EditCx, step: i32, duration: Duration) -> Result<(), String> {
    let track = cx.track();
    let from = if cx.on_lines() {
        cx.cursor.line
    } else {
        track.staff_clef().bottom_line() + 4
    };
    let d = staff::nearest(step, from);
    let alter = staff::key_alters(cx.key())[step as usize];
    put_staff_note(cx, d, alter, duration)
}

/// Makes the cursor note sharp (+1), flat (-1) or natural (0) on its staff position.
pub fn set_accidental(cx: &mut EditCx, alter: i32) -> Result<(), String> {
    if !cx.on_lines() {
        return Err("accidental: the cursor is not on a staff".into());
    }
    let d = cx.cursor.line;
    if cx.note_at_staff(d).is_none() {
        return Err("accidental: no note under the cursor".into());
    }
    put_staff_note(cx, d, alter, Duration::default())
}

/// The chant clef and shift (semitones) a chant staff of the cursor track shows its notes in:
/// the one the engraver and modern-to-chant choose for its melody.
pub fn chant_clef_of(
    score: &Score,
    track: &Track,
    policy: muse_core::melody::ChordPolicy,
) -> (chant::ChantClef, i32) {
    // Under `error` the staff is not shown with chords; its notes are edited as the top ones.
    let view = muse_core::melody::ChantView::build(score, track, policy)
        .or_else(|_| muse_core::melody::ChantView::build(score, track, Default::default()));
    view.map_or((chant::ChantClef::default(), 0), |v| (v.clef, v.shift))
}

/// On a chant staff of an ordinary track: the cursor beat's note at a GABC position (its
/// first note, added if the beat has none), as an inclinatum or not.
pub fn put_chant_note(
    cx: &mut EditCx,
    policy: ChordPolicy,
    position: u8,
    inclinatum: bool,
    duration: Duration,
) -> Result<(), String> {
    let track = cx.track().clone();
    let (clef, octave) = chant_clef_of(cx.score, &track, policy);
    let pitch = (clef.midi(position as i32, 0) as i32 - octave).clamp(0, 127) as u8;
    ensure_real(cx.beat(), duration);
    let picked = melody::pick(&track, cx.beat(), policy);
    let first = picked.map(|i| cx.beat().notes[i].string);
    if track.kind == TrackKind::Fretted {
        set_pitch_on(cx, pitch, first.unwrap_or(0))?;
    } else {
        let beat = cx.beat();
        match picked {
            Some(i) => beat.notes[i].fret = pitch,
            None => beat.notes.push(Note::new(0, pitch)),
        }
        renumber_pitched(beat);
    }
    // The note that now has the pitch carries the marks.
    let beat = cx.beat();
    if let Some(n) = beat.notes.iter_mut().find(|n| track.pitch_of(n) == pitch) {
        let marks = n.chant.get_or_insert_with(Default::default);
        marks.inclinatum = inclinatum;
    }
    cx.cursor.line = position as i32;
    Ok(())
}

/// On a chant staff of an ordinary track: toggles a chant sign of the cursor beat's note
/// (`-` is the initio debilis, others GABC modifiers).
pub fn toggle_chant_mark(cx: &mut EditCx, policy: ChordPolicy, mark: &str) -> Result<(), String> {
    let track = cx.track().clone();
    let beat = cx.beat();
    let i = melody::pick(&track, beat, policy).ok_or("no note under the cursor")?;
    let n = &mut beat.notes[i];
    let marks = n.chant.get_or_insert_with(Default::default);
    if mark == "-" {
        marks.initio_debilis = !marks.initio_debilis;
    } else if let Some(i) = marks.mods.iter().position(|m| m == mark) {
        marks.mods.remove(i);
    } else {
        marks.mods.push(mark.to_string());
    }
    if **marks == ChantMarks::default() {
        n.chant = None;
    }
    Ok(())
}

/// On a chant staff of an ordinary track: a division after the cursor beat (`:` on `:`
/// makes `::`; the same sign again removes it).
pub fn set_chant_division(cx: &mut EditCx, sign: &str) {
    let beat = cx.beat();
    beat.chant_division = match (beat.chant_division.as_deref(), sign) {
        (Some(":"), ":") => Some("::".into()),
        (Some(d), s) if d == s => None,
        _ => Some(sign.to_string()),
    };
}

/// On a chant staff of an ordinary track: a new beat after the cursor with the same pitch,
/// starting a new neume when `neume` is set.
pub fn append_chant_note(
    cx: &mut EditCx,
    policy: ChordPolicy,
    neume: bool,
    duration: Duration,
) -> Result<(), String> {
    let track = cx.track().clone();
    let (clef, octave) = chant_clef_of(cx.score, &track, policy);
    let position = melody::pick(&track, cx.beat(), policy).map_or(6, |i| {
        clef.place(track.pitch_of(&cx.beat().notes[i]), octave).0
    });
    insert_beat(cx, true, duration);
    put_chant_note(cx, policy, position.clamp(0, 13) as u8, false, duration)?;
    if neume {
        let beat = cx.beat();
        let picked = melody::pick(&track, beat, policy).unwrap_or(0);
        if let Some(n) = beat.notes.get_mut(picked) {
            n.chant.get_or_insert_with(Default::default).neume_start = true;
        }
    }
    Ok(())
}

/// A slash on a slash staff: the beat gets a note (kept if it has one).
pub fn put_slash(cx: &mut EditCx, duration: Duration) -> Result<(), String> {
    let track = cx.track().clone();
    let beat = cx.beat();
    ensure_real(beat, duration);
    if beat.notes.is_empty() {
        let middle = staff::letter_midi(track.staff_clef().bottom_line() + 4, 0)
            - track.staff_clef().transpose();
        beat.notes.push(match track.kind {
            TrackKind::Fretted => Note::new(0, 0),
            _ => Note::new(0, middle.clamp(0, 127) as u8),
        });
    }
    Ok(())
}

/// Adds a drum instrument to the cursor beat of a drum track, or removes it if present.
pub fn toggle_drum(cx: &mut EditCx, key: u8, duration: Duration) -> Result<(), String> {
    if cx.track().kind != TrackKind::Drums {
        return Err("not a drum track".into());
    }
    let beat = cx.beat();
    ensure_real(beat, duration);
    if let Some(i) = beat.notes.iter().position(|n| n.fret == key) {
        beat.notes.remove(i);
    } else {
        beat.notes.push(Note::new(muse_core::drums::lane(key), key));
        beat.notes.sort_by_key(|n| n.string);
    }
    cx.cursor.string = muse_core::drums::lane(key) as usize;
    Ok(())
}

/// Fills an empty bar placeholder so it can hold notes.
fn ensure_real(beat: &mut Beat, duration: Duration) {
    if beat.empty {
        beat.empty = false;
        beat.duration = duration;
    }
}

/// Sets the fret (or MIDI key on non-fretted tracks) of the note on the cursor's string.
pub fn set_fret(cx: &mut EditCx, fret: u8, duration: Duration) -> Result<(), String> {
    let track = cx.track();
    let max = if track.kind == TrackKind::Fretted {
        track.fret_count.max(24)
    } else {
        127
    };
    if fret > max {
        return Err(format!("fret {fret} is beyond the {max}-fret neck"));
    }
    let string = if track.kind == TrackKind::Fretted {
        cx.string()
    } else {
        0
    };
    let beat = cx.beat();
    ensure_real(beat, duration);
    match beat.notes.iter_mut().find(|n| n.string == string) {
        Some(n) => {
            n.fret = fret;
            n.tie = false;
        }
        None => {
            beat.notes.push(Note::new(string, fret));
            beat.notes.sort_by_key(|n| n.string);
        }
    }
    Ok(())
}

/// Puts `fret` on `string` in the cursor beat (a fretboard click), or takes it off if that note
/// is there already.
pub fn toggle_fret(
    cx: &mut EditCx,
    string: usize,
    fret: u8,
    duration: Duration,
) -> Result<(), String> {
    if cx.track().kind != TrackKind::Fretted {
        return Err("not a fretted track".into());
    }
    cx.cursor.string = string;
    let beat = cx.beat();
    if let Some(i) = beat
        .notes
        .iter()
        .position(|n| n.string as usize == string && n.fret == fret)
    {
        beat.notes.remove(i);
        return Ok(());
    }
    set_fret(cx, fret, duration)
}

/// Adds `pitch` to the cursor beat as a chord note (a piano click), or takes it off if it's
/// there. On a fretted track it goes on a free string that can play it, the cursor's first,
/// then the one needing the lowest fret; on drums it toggles that drum.
pub fn toggle_pitch(cx: &mut EditCx, pitch: u8, duration: Duration) -> Result<(), String> {
    let track = cx.track().clone();
    match track.kind {
        TrackKind::Drums => return toggle_drum(cx, pitch, duration),
        TrackKind::Fretted => {}
        _ => {
            let beat = cx.beat();
            ensure_real(beat, duration);
            if let Some(i) = beat.notes.iter().position(|n| n.fret == pitch) {
                beat.notes.remove(i);
            } else {
                let string = beat.notes.len() as u8;
                beat.notes.push(Note::new(string, pitch));
            }
            return Ok(());
        }
    }
    let cursor_string = cx.string() as usize;
    let beat = cx.beat();
    if let Some(i) = beat.notes.iter().position(|n| track.pitch_of(n) == pitch) {
        beat.notes.remove(i);
        return Ok(());
    }
    let max = track.fret_count.max(24) as i32;
    let fret_on = |s: usize| {
        let f = pitch as i32 - track.strings[s] as i32 - track.capo as i32;
        (0..=max).contains(&f).then_some(f as u8)
    };
    let used: Vec<usize> = beat.notes.iter().map(|n| n.string as usize).collect();
    let free = |s: &usize| !used.contains(s);
    let string = Some(cursor_string)
        .filter(|s| *s < track.strings.len() && free(s) && fret_on(*s).is_some())
        .or_else(|| {
            (0..track.strings.len())
                .filter(free)
                .filter_map(|s| fret_on(s).map(|f| (f, s)))
                .min()
                .map(|(_, s)| s)
        })
        .ok_or_else(|| format!("no free string can play {}", crate::edit::pitch_name(pitch)))?;
    let fret = fret_on(string).unwrap();
    cx.cursor.string = string;
    set_fret(cx, fret, duration)
}

/// A pitch's name, e.g. "C#4".
pub fn pitch_name(pitch: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!("{}{}", NAMES[pitch as usize % 12], pitch as i32 / 12 - 1)
}

pub fn delete_note(cx: &mut EditCx) {
    let string = cx.string();
    let fretted = cx.track().kind == TrackKind::Fretted;
    let beat = cx.beat();
    if fretted {
        beat.notes.retain(|n| n.string != string);
    } else {
        beat.notes.clear();
    }
}

/// Turns the cursor beat into a rest.
pub fn make_rest(cx: &mut EditCx, duration: Duration) {
    let beat = cx.beat();
    ensure_real(beat, duration);
    beat.notes.clear();
}

pub fn set_duration(cx: &mut EditCx, f: impl Fn(Duration) -> Duration) -> Duration {
    let beat = cx.beat();
    beat.duration = f(beat.duration);
    beat.duration
}

/// Inserts a rest beat before or after the cursor and moves onto it.
pub fn insert_beat(cx: &mut EditCx, after: bool, duration: Duration) {
    let i = cx.cursor.beat;
    let beats = cx.voice();
    // An empty placeholder becomes the new beat itself.
    if beats.len() == 1 && beats[0].empty {
        beats[0] = Beat::rest(duration);
        cx.cursor.beat = 0;
        return;
    }
    let at = if after {
        (i + 1).min(beats.len())
    } else {
        i.min(beats.len())
    };
    beats.insert(at, Beat::rest(duration));
    cx.cursor.beat = at;
}

/// Moves to the next beat, appending a rest (or moving to the next bar, creating one at the end
/// of the score) when the cursor is on the last beat. Used while typing in insert mode.
pub fn advance(cx: &mut EditCx, duration: Duration) {
    let (t, b) = (cx.cursor.track, cx.cursor.bar);
    let len = cx.voice().len();
    if cx.cursor.beat + 1 < len {
        cx.cursor.beat += 1;
        return;
    }
    let filled: u32 = cx.voice().iter().map(Beat::ticks).sum();
    if filled + duration.ticks() <= cx.score.played_ticks(b) {
        insert_beat(cx, true, duration);
        return;
    }
    if b + 1 >= cx.score.master_bars.len() {
        insert_bars(cx, b + 1, 1);
    }
    let _ = t;
    cx.cursor.bar = b + 1;
    cx.cursor.beat = 0;
}

pub fn toggle(cx: &mut EditCx, technique: &str, duration: Duration) -> Result<(), String> {
    let string = cx.string();
    let fretted = cx.track().kind == TrackKind::Fretted;
    let beat = cx.beat();
    ensure_real(beat, duration);
    match technique {
        "vibrato-wide" => {
            beat.effects.vibrato = !beat.effects.vibrato;
            return Ok(());
        }
        "fade-in" => {
            beat.effects.fade_in = !beat.effects.fade_in;
            return Ok(());
        }
        "tap" | "slap" | "pop" => {
            let s = match technique {
                "tap" => Slap::Tap,
                "slap" => Slap::Slap,
                _ => Slap::Pop,
            };
            beat.effects.slap = if beat.effects.slap == s {
                Slap::None
            } else {
                s
            };
            return Ok(());
        }
        "stroke-up" | "stroke-down" => {
            let d = if technique == "stroke-up" {
                StrokeDirection::Up
            } else {
                StrokeDirection::Down
            };
            beat.effects.stroke = match beat.effects.stroke {
                Some(s) if s.direction == d => None,
                _ => Some(Stroke {
                    direction: d,
                    duration: DurationValue::Sixteenth,
                    arpeggio: false,
                }),
            };
            return Ok(());
        }
        _ => {}
    }
    let notes: Vec<&mut Note> = if fretted {
        beat.notes
            .iter_mut()
            .filter(|n| n.string == string)
            .collect()
    } else {
        beat.notes.iter_mut().collect()
    };
    if notes.is_empty() {
        return Err("no note under the cursor".into());
    }
    for n in notes {
        let fx = &mut n.effects;
        match technique {
            "hammer" => fx.hammer_pull = !fx.hammer_pull,
            "bend" => {
                fx.bend = match fx.bend {
                    Some(_) => None,
                    None => Some(Bend {
                        points: vec![
                            BendPoint {
                                position: 0,
                                value: 0,
                                vibrato: false,
                            },
                            BendPoint {
                                position: 15,
                                value: 4,
                                vibrato: false,
                            },
                            BendPoint {
                                position: Bend::MAX_POSITION,
                                value: 4,
                                vibrato: false,
                            },
                        ],
                    }),
                }
            }
            "half-bend" => {
                fx.bend = match fx.bend {
                    Some(_) => None,
                    None => Some(Bend {
                        points: vec![
                            BendPoint {
                                position: 0,
                                value: 0,
                                vibrato: false,
                            },
                            BendPoint {
                                position: 15,
                                value: 2,
                                vibrato: false,
                            },
                            BendPoint {
                                position: Bend::MAX_POSITION,
                                value: 2,
                                vibrato: false,
                            },
                        ],
                    }),
                }
            }
            "slide" => {
                fx.slides = if fx.slides.is_empty() {
                    vec![SlideKind::Shift]
                } else {
                    vec![]
                }
            }
            "vibrato" => fx.vibrato = !fx.vibrato,
            "palm-mute" => fx.palm_mute = !fx.palm_mute,
            "let-ring" => fx.let_ring = !fx.let_ring,
            "staccato" => fx.staccato = !fx.staccato,
            "harmonic" => {
                fx.harmonic = if fx.harmonic.is_some() {
                    None
                } else {
                    Some(Harmonic::Natural)
                }
            }
            "pinch-harmonic" => {
                fx.harmonic = if fx.harmonic.is_some() {
                    None
                } else {
                    Some(Harmonic::Pinch)
                }
            }
            "tremolo" => {
                fx.tremolo_picking = if fx.tremolo_picking.is_some() {
                    None
                } else {
                    Some(DurationValue::Sixteenth)
                }
            }
            "dead" => n.dead = !n.dead,
            "ghost" => n.ghost = !n.ghost,
            "tie" => n.tie = !n.tie,
            "accent" => {
                n.accent = if n.accent == Accent::Normal {
                    Accent::None
                } else {
                    Accent::Normal
                }
            }
            "heavy-accent" => {
                n.accent = if n.accent == Accent::Heavy {
                    Accent::None
                } else {
                    Accent::Heavy
                }
            }
            other => return Err(format!("unknown technique '{other}'")),
        }
    }
    Ok(())
}

/// Sets the velocity of the cursor note (on a fretted track the one on its string) to a dynamic
/// marking (`ppp` … `fff`).
pub fn set_dynamic(cx: &mut EditCx, dynamic: &str) -> Result<(), String> {
    let velocity = crate::palette::dynamic_velocity(dynamic).ok_or("unknown dynamic")?;
    let string = cx.string();
    let fretted = cx.track().kind == TrackKind::Fretted;
    let notes: Vec<&mut Note> = cx
        .beat()
        .notes
        .iter_mut()
        .filter(|n| !fretted || n.string == string)
        .collect();
    if notes.is_empty() {
        return Err("no note under the cursor".into());
    }
    for n in notes {
        n.velocity = velocity;
    }
    Ok(())
}

/// Changes the pitch of the cursor note by semitones, keeping its string when possible and
/// re-fingering the chord otherwise.
pub fn shift_pitch(cx: &mut EditCx, semitones: i32) -> Result<(), String> {
    let track = cx.track().clone();
    let string = cx.string();
    if track.kind != TrackKind::Fretted {
        let beat = cx.beat();
        if beat.notes.is_empty() {
            return Err("no note under the cursor".into());
        }
        for n in &mut beat.notes {
            n.fret = (n.fret as i32 + semitones).clamp(0, 127) as u8;
        }
        return Ok(());
    }
    let beat = cx.beat();
    let Some(i) = beat.notes.iter().position(|n| n.string == string) else {
        return Err("no note under the cursor".into());
    };
    let fret = beat.notes[i].fret as i32 + semitones;
    if (0..=track.fret_count.max(24) as i32).contains(&fret) {
        beat.notes[i].fret = fret as u8;
        return Ok(());
    }
    let target = track.pitch_of(&beat.notes[i]) as i32 + semitones;
    set_pitch(cx, target.clamp(0, 127) as u8)
}

/// Sets the pitch of the cursor note (or adds a note) on a fretted track, choosing the string.
pub fn set_pitch(cx: &mut EditCx, pitch: u8) -> Result<(), String> {
    let string = cx.string();
    set_pitch_on(cx, pitch, string)
}

/// Sets the pitch of the note on a string (or adds it there), as [`set_pitch`].
fn set_pitch_on(cx: &mut EditCx, pitch: u8, string: u8) -> Result<(), String> {
    let track = cx.track().clone();
    if track.kind != TrackKind::Fretted {
        return set_fret(cx, pitch, Duration::default());
    }
    let beat = cx.beat();
    ensure_real(beat, Duration::default());
    let mut pitches: Vec<u8> = beat
        .notes
        .iter()
        .filter(|n| n.string != string)
        .map(|n| track.pitch_of(n))
        .collect();
    pitches.push(pitch);
    let mut pos = beat.notes.iter().map(|n| n.fret as i32).max().unwrap_or(0);
    let placed = muse_convert::fret_chord(&pitches, &track.strings, track.capo, &mut pos);
    let Some(Some((new_string, new_fret))) = placed.last().copied() else {
        return Err(format!("pitch {pitch} cannot be played on this tuning"));
    };
    // Re-place the other notes too, keeping their attributes.
    let mut others: Vec<Note> = beat
        .notes
        .iter()
        .filter(|n| n.string != string)
        .cloned()
        .collect();
    let cursor_note = beat
        .notes
        .iter()
        .find(|n| n.string == string)
        .cloned()
        .unwrap_or_else(|| Note::new(string, 0));
    let mut notes = Vec::new();
    for (n, p) in others.drain(..).zip(&placed) {
        if let Some((s, f)) = p {
            notes.push(Note {
                string: *s,
                fret: *f,
                ..n
            });
        }
    }
    notes.push(Note {
        string: new_string,
        fret: new_fret,
        ..cursor_note
    });
    notes.sort_by_key(|n| n.string);
    beat.notes = notes;
    cx.cursor.string = new_string as usize;
    Ok(())
}

/// A position in a voice: (bar, beat).
pub type Pos = (usize, usize);

/// Beats between two positions (inclusive) of the cursor's track and voice.
pub fn collect(score: &Score, track: usize, voice: usize, from: Pos, to: Pos) -> Vec<Beat> {
    let mut out = Vec::new();
    for b in from.0..=to.0.min(score.master_bars.len().saturating_sub(1)) {
        let Some(v) = score.tracks[track]
            .bars
            .get(b)
            .and_then(|bar| bar.voices.get(voice))
        else {
            continue;
        };
        for (i, beat) in v.beats.iter().enumerate() {
            let after_start = b > from.0 || i >= from.1;
            let before_end = b < to.0 || i <= to.1;
            if after_start && before_end && !beat.empty {
                out.push(beat.clone());
            }
        }
    }
    out
}

/// Deletes beats between two positions (inclusive); emptied bars keep a placeholder.
pub fn delete_range(cx: &mut EditCx, from: Pos, to: Pos) {
    let (t, v) = (cx.cursor.track, cx.cursor.voice);
    for b in from.0..=to.0.min(cx.score.master_bars.len().saturating_sub(1)) {
        if cx.score.tracks[t].bars[b].voices.get(v).is_none() {
            continue;
        }
        let bar = cx.bar(t, b);
        let beats = &mut bar.voices[v].beats;
        let mut i = 0;
        beats.retain(|_| {
            let keep = !((b > from.0 || i >= from.1) && (b < to.0 || i <= to.1));
            i += 1;
            keep
        });
        if beats.is_empty() {
            beats.push(Beat {
                empty: true,
                ..Beat::default()
            });
        }
    }
    cx.cursor.bar = from.0;
    cx.cursor.beat = from.1;
}

/// Inserts beats after (or before) the cursor beat.
pub fn put_beats(cx: &mut EditCx, beats: &[Beat], after: bool) {
    if beats.is_empty() {
        return;
    }
    let i = cx.cursor.beat;
    let voice = cx.voice();
    let placeholder = voice.len() == 1 && voice[0].empty;
    let at = if placeholder {
        voice.clear();
        0
    } else if after {
        (i + 1).min(voice.len())
    } else {
        i.min(voice.len())
    };
    for (k, b) in beats.iter().enumerate() {
        voice.insert(at + k, b.clone());
    }
    cx.cursor.beat = at;
}

/// Inserts `count` empty bars at index `at` in every track.
pub fn insert_bars(cx: &mut EditCx, at: usize, count: usize) {
    let score = cx.whole();
    let at = at.min(score.master_bars.len());
    let template = score
        .master_bars
        .get(at.saturating_sub(1))
        .cloned()
        .unwrap_or_default();
    for _ in 0..count {
        let mb = MasterBar {
            time_signature: template.time_signature,
            key_signature: template.key_signature,
            triplet_feel: template.triplet_feel,
            ..MasterBar::default()
        };
        score.master_bars.insert(at, mb);
        for t in &mut score.tracks {
            t.bars.insert(
                at,
                Bar {
                    voices: vec![Voice {
                        beats: vec![Beat {
                            empty: true,
                            ..Beat::default()
                        }],
                    }],
                },
            );
        }
    }
}

/// A copied range of whole bars: master bars plus each track's bars.
#[derive(Debug, Clone)]
pub struct BarClip {
    pub master: Vec<MasterBar>,
    pub tracks: Vec<Vec<Bar>>,
}

pub fn yank_bars(score: &Score, from: usize, to: usize) -> BarClip {
    let to = to.min(score.master_bars.len().saturating_sub(1));
    BarClip {
        master: score.master_bars[from..=to].to_vec(),
        tracks: score
            .tracks
            .iter()
            .map(|t| t.bars[from..=to].to_vec())
            .collect(),
    }
}

pub fn delete_bars(cx: &mut EditCx, from: usize, to: usize) -> Result<(), String> {
    let n = cx.score.master_bars.len();
    let to = to.min(n.saturating_sub(1));
    if to + 1 - from >= n {
        return Err("cannot delete every bar".into());
    }
    let score = cx.whole();
    score.master_bars.drain(from..=to);
    for t in &mut score.tracks {
        t.bars.drain(from..=to);
    }
    cx.cursor.bar = from.min(n - (to + 1 - from) - 1);
    cx.cursor.beat = 0;
    Ok(())
}

pub fn put_bars(cx: &mut EditCx, clip: &BarClip, at: usize) {
    let score = cx.whole();
    let at = at.min(score.master_bars.len());
    for (k, mb) in clip.master.iter().enumerate() {
        score.master_bars.insert(at + k, mb.clone());
    }
    for (ti, t) in score.tracks.iter_mut().enumerate() {
        for k in 0..clip.master.len() {
            let bar = clip
                .tracks
                .get(ti)
                .and_then(|b| b.get(k))
                .cloned()
                .unwrap_or_else(|| Bar {
                    voices: vec![Voice {
                        beats: vec![Beat {
                            empty: true,
                            ..Beat::default()
                        }],
                    }],
                });
            t.bars.insert(at + k, bar);
        }
    }
    cx.cursor.bar = at;
    cx.cursor.beat = 0;
}

/// Parses note names like "C#4", "Bb2" or a MIDI number.
pub fn parse_pitch(s: &str) -> Option<u8> {
    if let Ok(n) = s.parse::<u8>() {
        return (n <= 127).then_some(n);
    }
    let mut chars = s.chars();
    let step = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest: String = chars.collect();
    let (alter, octave) = match rest.chars().next() {
        Some('#') => (1, &rest[1..]),
        Some('b') => (-1, &rest[1..]),
        _ => (0, rest.as_str()),
    };
    let octave: i32 = octave.parse().ok()?;
    u8::try_from((octave + 1) * 12 + step + alter).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score() -> Score {
        let track = Track {
            strings: vec![64, 59, 55, 50, 45, 40],
            fret_count: 24,
            bars: vec![
                Bar {
                    voices: vec![Voice {
                        beats: vec![Beat {
                            empty: true,
                            ..Beat::default()
                        }]
                    }]
                };
                2
            ],
            ..Track::default()
        };
        Score {
            master_bars: vec![MasterBar::default(); 2],
            tracks: vec![track],
            ..Score::default()
        }
    }

    #[test]
    fn typing_frets_and_advancing() {
        let mut s = score();
        let mut cx = EditCx::new(&mut s, Cursor::default());
        let q = Duration::new(DurationValue::Quarter);
        set_fret(&mut cx, 3, q).unwrap();
        for f in [5, 7, 8] {
            advance(&mut cx, q);
            set_fret(&mut cx, f, q).unwrap();
        }
        // The bar is full: the next advance moves to bar 2.
        advance(&mut cx, q);
        assert_eq!((cx.cursor.bar, cx.cursor.beat), (1, 0));
        let change = cx.finish().unwrap();
        assert!(matches!(change, Change::Bars(ref c) if c.len() == 1));
        let frets: Vec<u8> = s.tracks[0].bars[0].voices[0]
            .beats
            .iter()
            .map(|b| b.notes[0].fret)
            .collect();
        assert_eq!(frets, vec![3, 5, 7, 8]);
    }

    #[test]
    fn delete_put_and_bars() {
        let mut s = score();
        let mut cx = EditCx::new(&mut s, Cursor::default());
        let q = Duration::new(DurationValue::Quarter);
        for f in [1, 2, 3] {
            set_fret(&mut cx, f, q).unwrap();
            advance(&mut cx, q);
        }
        let clip = collect(cx.score, 0, 0, (0, 0), (0, 1));
        delete_range(&mut cx, (0, 0), (0, 1));
        assert_eq!(cx.score.tracks[0].bars[0].voices[0].beats.len(), 2);
        put_beats(&mut cx, &clip, false);
        assert_eq!(cx.score.tracks[0].bars[0].voices[0].beats.len(), 4);
        insert_bars(&mut cx, 1, 2);
        assert_eq!(cx.score.master_bars.len(), 4);
        assert!(matches!(cx.finish(), Some(Change::Score { .. })));
    }

    #[test]
    fn pitch_changes_move_strings() {
        let mut s = score();
        let mut cx = EditCx::new(
            &mut s,
            Cursor {
                string: 5,
                ..Cursor::default()
            },
        );
        set_fret(&mut cx, 0, Duration::default()).unwrap(); // E2
        assert!(shift_pitch(&mut cx, -2).is_err()); // D2 is below the lowest string
        shift_pitch(&mut cx, 24).unwrap(); // E4: fret 24 on the low E
        assert_eq!(
            cx.score.tracks[0].bars[0].voices[0].beats[0].notes[0].fret,
            24
        );
        shift_pitch(&mut cx, 1).unwrap(); // F4: beyond fret 24, moves to another string
        let n = cx.score.tracks[0].bars[0].voices[0].beats[0].notes[0].clone();
        assert_eq!(cx.score.tracks[0].pitch_of(&n), 65);
        assert_eq!(parse_pitch("C#4"), Some(61));
        assert_eq!(parse_pitch("Bb2"), Some(46));
    }
}
