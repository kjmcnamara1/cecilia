//! Guitar Pro 5 (v5.00) writer. The layout mirrors [`crate::gp`]'s reader.

use cecilia_core::*;

use crate::Result;

/// Encodes a score as a GP5 file. Pitched tracks are fingered onto a fretboard, and only the
/// first two voices of each bar are kept (GP5's limit).
pub fn write_gp5(score: &Score) -> Result<Vec<u8>> {
    // Guitar Pro 5 has one staff per track: a grand staff's halves become two tracks.
    let mut score = score.split_grand();
    for t in &mut score.tracks {
        cecilia_convert::pitched_to_fretted(t);
        if t.kind == TrackKind::Chant {
            t.kind = TrackKind::Pitched;
            cecilia_convert::pitched_to_fretted(t);
        }
    }
    let mut w = Writer::default();
    w.write_score(&score);
    Ok(w.buf)
}

#[derive(Default)]
struct Writer {
    buf: Vec<u8>,
}

/// MIDI channel pairs (primary, effect) used when there are few enough tracks.
const CHANNEL_PAIRS: [(u8, u8); 7] = [(0, 1), (2, 3), (4, 5), (6, 7), (8, 10), (11, 12), (13, 14)];

/// Where a track's MIDI settings live in GP5's 4 ports × 16 channels table.
#[derive(Clone, Copy)]
struct ChannelSlot {
    port: u8,
    channel: u8,
    effect: u8,
}

impl ChannelSlot {
    fn index(&self) -> usize {
        self.port as usize * 16 + self.channel as usize
    }
}

type TempoTargets = std::collections::HashMap<(usize, usize, usize, usize), f64>;

impl Writer {
    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    fn i8(&mut self, v: i8) {
        self.buf.push(v as u8);
    }
    fn bool(&mut self, v: bool) {
        self.buf.push(v as u8);
    }
    fn i16(&mut self, v: i16) {
        self.buf.extend(v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.buf.extend(v.to_le_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.buf.extend(v.to_le_bytes());
    }
    fn zeros(&mut self, n: usize) {
        self.buf.extend(std::iter::repeat_n(0, n));
    }

    fn byte_size_string(&mut self, s: &str, size: usize) {
        let mut bytes = encode_text(s);
        bytes.truncate(size.min(255));
        self.u8(bytes.len() as u8);
        let len = bytes.len();
        self.buf.extend(bytes);
        self.zeros(size - len);
    }

    fn int_byte_size_string(&mut self, s: &str) {
        let mut bytes = encode_text(s);
        bytes.truncate(255);
        self.i32(bytes.len() as i32 + 1);
        self.u8(bytes.len() as u8);
        self.buf.extend(bytes);
    }

    fn int_size_string(&mut self, s: &str) {
        let bytes = encode_text(s);
        self.i32(bytes.len() as i32);
        self.buf.extend(bytes);
    }

    fn color(&mut self, c: Color) {
        self.buf.extend([c.r, c.g, c.b, 0]);
    }

    fn write_score(&mut self, score: &Score) {
        self.byte_size_string("FICHIER GUITAR PRO v5.00", 30);
        let m = &score.meta;
        for s in [
            &m.title,
            &m.subtitle,
            &m.artist,
            &m.album,
            &m.words,
            &m.music,
            &m.copyright,
            &m.tab_author,
            &m.instructions,
        ] {
            self.int_byte_size_string(s);
        }
        self.i32(m.notices.len() as i32);
        for n in &m.notices {
            self.int_byte_size_string(n);
        }
        self.write_lyrics(score);
        self.write_page_setup(&score.page);
        self.int_byte_size_string(score.extras.get("gp.tempo_name").map_or("", String::as_str));
        self.i32(score.tempo as i32);
        let key = score
            .master_bars
            .first()
            .map_or(0, |b| b.key_signature.accidentals);
        self.i8(key);
        self.i32(0); // octave

        let channels = assign_channels(score);
        let mut table = [(24i32, 13i8, 8i8); 64];
        for (t, slot) in score.tracks.iter().zip(&channels) {
            table[slot.index()] = (
                t.midi.program as i32,
                to_channel_value(t.midi.volume),
                to_channel_value(t.midi.pan),
            );
        }
        for (program, volume, pan) in table {
            self.i32(program);
            self.i8(volume);
            self.i8(pan);
            self.zeros(4 + 2); // chorus, reverb, phaser, tremolo, padding
        }

        self.write_directions(score);
        self.i32(0); // master reverb
        self.i32(score.master_bars.len() as i32);
        self.i32(score.tracks.len() as i32);
        for (i, mb) in score.master_bars.iter().enumerate() {
            self.write_master_bar(score, i, mb);
        }
        for (t, slot) in score.tracks.iter().zip(&channels) {
            self.write_track(t, *slot);
        }
        self.zeros(2);
        let targets = tempo_targets(score);
        for b in 0..score.master_bars.len() {
            for (ti, t) in score.tracks.iter().enumerate() {
                self.write_bar(t, t.bars.get(b), |v, i| {
                    targets.get(&(b, ti, v, i)).copied()
                });
            }
        }
    }

    fn write_lyrics(&mut self, score: &Score) {
        let mut lyrics = score.lyrics.clone().unwrap_or_default();
        // Keep the stored lines unless the syllables on the beats were changed since.
        if !lyrics.lines.is_empty() {
            let mut check = score.clone();
            if let Some(t) = lyrics.track.and_then(|t| check.tracks.get_mut(t)) {
                for b in t
                    .bars
                    .iter_mut()
                    .flat_map(|b| &mut b.voices)
                    .flat_map(|v| &mut v.beats)
                {
                    b.lyrics.clear();
                }
                cecilia_core::lyrics::attach(&mut check);
                // As gabc+ lays them out (`side---` is `side-`, a beat of dashes is a dash).
                let syllables = |s: &Score| {
                    s.tracks
                        .get(lyrics.track.unwrap_or(0))
                        .map(cecilia_core::lyrics::normalized)
                };
                if syllables(&check) != syllables(score) {
                    lyrics.lines.clear();
                }
            }
        }
        if lyrics.lines.is_empty() {
            // Build lyric lines from per-beat syllables (GP6+ store lyrics per beat).
            if let Some((ti, t)) = score.tracks.iter().enumerate().find(|(_, t)| {
                t.bars
                    .iter()
                    .flat_map(|b| &b.voices)
                    .flat_map(|v| &v.beats)
                    .any(|b| !b.lyrics.is_empty())
            }) {
                lyrics.track = Some(ti);
                lyrics.lines = cecilia_core::lyrics::lines_from_beats(t);
                lyrics.lines.truncate(5);
            }
        }
        self.i32(lyrics.track.map_or(0, |t| t as i32 + 1));
        for i in 0..5 {
            match lyrics.lines.get(i) {
                Some(l) => {
                    self.i32(l.start_bar as i32 + 1);
                    self.int_size_string(&l.text);
                }
                None => {
                    self.i32(1);
                    self.int_size_string("");
                }
            }
        }
    }

    fn write_page_setup(&mut self, page: &PageSetup) {
        for v in [
            page.width,
            page.height,
            page.margin_left,
            page.margin_right,
            page.margin_top,
            page.margin_bottom,
            page.scale,
        ] {
            self.i32(v as i32);
        }
        self.i16(page.header_flags as i16);
        for s in &page.templates {
            self.int_byte_size_string(s);
        }
    }

    fn write_directions(&mut self, score: &Score) {
        for (d, _) in crate::gpif::DIRECTION_NAMES {
            let bar = score
                .master_bars
                .iter()
                .position(|mb| mb.directions.contains(&d));
            self.i16(bar.map_or(-1, |b| b as i16 + 1));
        }
    }

    fn write_master_bar(&mut self, score: &Score, i: usize, mb: &MasterBar) {
        if i > 0 {
            self.u8(0);
        }
        let prev = i.checked_sub(1).map(|p| &score.master_bars[p]);
        let mut flags = 0u8;
        if prev.is_none_or(|p| p.time_signature.numerator != mb.time_signature.numerator) {
            flags |= 0x01;
        }
        if prev.is_none_or(|p| p.time_signature.denominator != mb.time_signature.denominator) {
            flags |= 0x02;
        }
        if mb.repeat_start {
            flags |= 0x04;
        }
        if mb.repeat_end.is_some() {
            flags |= 0x08;
        }
        if mb.alternate_endings != 0 {
            flags |= 0x10;
        }
        if mb.marker.is_some() {
            flags |= 0x20;
        }
        if prev.is_some_and(|p| p.key_signature != mb.key_signature)
            || (prev.is_none() && mb.key_signature.minor)
        {
            flags |= 0x40;
        }
        if mb.double_bar {
            flags |= 0x80;
        }
        self.u8(flags);
        if flags & 0x01 != 0 {
            self.i8(mb.time_signature.numerator as i8);
        }
        if flags & 0x02 != 0 {
            self.i8(mb.time_signature.denominator as i8);
        }
        if let Some(n) = mb.repeat_end {
            self.i8(n as i8);
        }
        if let Some(m) = &mb.marker {
            self.int_byte_size_string(&m.title);
            self.color(m.color);
        }
        if flags & 0x40 != 0 {
            self.i8(mb.key_signature.accidentals);
            self.i8(mb.key_signature.minor as i8);
        }
        if flags & 0x10 != 0 {
            self.u8(mb.alternate_endings);
        }
        if flags & 0x03 != 0 {
            let ts = mb.time_signature;
            let beams = if ts.denominator == 8 && ts.numerator % 3 == 0 {
                [3, 3, 3, 3]
            } else {
                [2, 2, 2, 2]
            };
            self.buf.extend(beams);
        }
        if flags & 0x10 == 0 {
            self.u8(0);
        }
        self.u8(match mb.triplet_feel {
            TripletFeel::Eighth => 1,
            TripletFeel::Sixteenth => 2,
            _ => 0,
        });
    }

    fn write_track(&mut self, t: &Track, slot: ChannelSlot) {
        self.u8(0);
        let mut flags = 0x08; // visible
        if t.is_percussion() {
            flags |= 0x01;
        }
        if t.extras.contains_key("gp.twelve_string") {
            flags |= 0x02;
        }
        if t.extras.contains_key("gp.banjo") {
            flags |= 0x04;
        }
        if t.solo {
            flags |= 0x10;
        }
        if t.mute {
            flags |= 0x20;
        }
        self.u8(flags);
        self.byte_size_string(&t.name, 40);
        let strings: Vec<u8> = if t.strings.is_empty() {
            vec![0; 6]
        } else {
            t.strings.iter().take(7).copied().collect()
        };
        self.i32(strings.len() as i32);
        for i in 0..7 {
            self.i32(strings.get(i).copied().unwrap_or(0) as i32);
        }
        self.i32(slot.port as i32 + 1);
        self.i32(slot.channel as i32 + 1);
        self.i32(slot.effect as i32 + 1);
        self.i32(t.fret_count.max(1) as i32);
        self.i32(t.capo as i32);
        self.color(t.color);
        self.i16(0x0003); // show tablature and notation
        self.u8(0); // auto accentuation
        self.u8(t.midi.bank);
        self.u8(0); // humanize
        self.zeros(4 * 3 + 12);
        // RSE instrument: instrument, unknown, sound bank, effect number (GP5.0: i16 + padding).
        self.i32(-1);
        self.i32(-1);
        self.i32(-1);
        self.i16(-1);
        self.u8(0);
    }

    /// Writes two voices; `tempo(voice, beat)` gives a tempo change to put on that beat.
    fn write_bar(
        &mut self,
        t: &Track,
        bar: Option<&Bar>,
        tempo: impl Fn(usize, usize) -> Option<f64>,
    ) {
        let empty = Voice::default();
        for v in 0..2 {
            let voice = bar.and_then(|b| b.voices.get(v)).unwrap_or(&empty);
            if voice.beats.is_empty() {
                self.i32(1);
                self.write_empty_beat(tempo(v, 0));
                continue;
            }
            self.i32(voice.beats.len() as i32);
            for (i, beat) in voice.beats.iter().enumerate() {
                self.write_beat(t, beat, tempo(v, i));
            }
        }
        self.u8(0); // line break
    }

    fn write_empty_beat(&mut self, tempo: Option<f64>) {
        self.u8(0x40 | if tempo.is_some() { 0x10 } else { 0 });
        self.u8(0); // empty
        self.i8(0); // quarter
        if let Some(bpm) = tempo {
            self.write_mix_table(bpm);
        }
        self.u8(0); // no strings
        self.i16(0);
    }

    fn write_beat(&mut self, t: &Track, beat: &Beat, tempo: Option<f64>) {
        let fx = &beat.effects;
        let has_fx = fx.vibrato
            || fx.fade_in
            || fx.slap != Slap::None
            || fx.stroke.is_some()
            || fx.pick_stroke != StrokeDirection::None
            || fx.rasgueado
            || fx.tremolo_bar.is_some();
        let mut flags = 0u8;
        if beat.duration.dots > 0 {
            flags |= 0x01;
        }
        if beat.chord.is_some() {
            flags |= 0x02;
        }
        if beat.text.is_some() {
            flags |= 0x04;
        }
        if has_fx {
            flags |= 0x08;
        }
        if tempo.is_some() {
            flags |= 0x10;
        }
        if beat.duration.tuplet.is_some() {
            flags |= 0x20;
        }
        if beat.empty || beat.notes.is_empty() {
            flags |= 0x40;
        }
        self.u8(flags);
        if flags & 0x40 != 0 {
            self.u8(if beat.empty { 0 } else { 2 });
        }
        self.i8(beat.duration.value as i8 - 2);
        if let Some(tp) = beat.duration.tuplet {
            self.i32(tp.enters as i32);
        }
        if let Some(c) = &beat.chord {
            self.write_chord(c);
        }
        if let Some(text) = &beat.text {
            self.int_byte_size_string(text);
        }
        if has_fx {
            self.write_beat_effects(fx);
        }
        if let Some(bpm) = tempo {
            self.write_mix_table(bpm);
        }
        let strings = t.strings.len().clamp(1, 7);
        let mut notes: Vec<&Note> = beat
            .notes
            .iter()
            .filter(|n| (n.string as usize) < strings)
            .collect();
        notes.sort_by_key(|n| n.string);
        notes.dedup_by_key(|n| n.string);
        let mask = notes.iter().fold(0u8, |m, n| m | (1 << (6 - n.string)));
        self.u8(mask);
        for n in notes {
            self.write_note(t, n);
        }
        self.i16(match fx.ottava {
            Ottava::None => 0,
            Ottava::Ottava => 0x10,
            Ottava::OttavaBassa => 0x20,
            Ottava::Quindicesima => 0x40,
            Ottava::QuindicesimaBassa => 0x100,
        });
    }

    fn write_chord(&mut self, c: &Chord) {
        self.bool(true); // new format
        self.zeros(16);
        self.byte_size_string(&c.name, 21);
        self.zeros(4);
        self.i32(c.first_fret as i32);
        for i in 0..7 {
            self.i32(c.frets.get(i).copied().flatten().map_or(-1, |f| f as i32));
        }
        let barres = &c.barres[..c.barres.len().min(5)];
        self.u8(barres.len() as u8);
        for field in 0..3 {
            for i in 0..5 {
                self.u8(barres
                    .get(i)
                    .map_or(0, |b| [b.fret, b.first_string, b.last_string][field]));
            }
        }
        self.zeros(7 + 1); // omissions, padding
        self.buf.extend([0xFFu8; 7]); // fingerings
        self.bool(true); // show diagram
    }

    fn write_beat_effects(&mut self, fx: &BeatEffects) {
        let mut f1 = 0u8;
        let mut f2 = 0u8;
        if fx.vibrato {
            f1 |= 0x02;
        }
        if fx.fade_in {
            f1 |= 0x10;
        }
        if fx.slap != Slap::None {
            f1 |= 0x20;
        }
        if fx.stroke.is_some() {
            f1 |= 0x40;
        }
        if fx.rasgueado {
            f2 |= 0x01;
        }
        if fx.pick_stroke != StrokeDirection::None {
            f2 |= 0x02;
        }
        if fx.tremolo_bar.is_some() {
            f2 |= 0x04;
        }
        self.u8(f1);
        self.u8(f2);
        if f1 & 0x20 != 0 {
            self.u8(match fx.slap {
                Slap::Tap => 1,
                Slap::Slap => 2,
                Slap::Pop => 3,
                Slap::None => 0,
            });
        }
        if let Some(b) = &fx.tremolo_bar {
            self.write_bend(b);
        }
        if let Some(s) = fx.stroke {
            let v = stroke_value(s.duration);
            // GP5 stores (up, down).
            match s.direction {
                StrokeDirection::Up => self.buf.extend([v, 0]),
                StrokeDirection::Down => self.buf.extend([0, v]),
                StrokeDirection::None => self.buf.extend([0, 0]),
            }
        }
        if f2 & 0x02 != 0 {
            self.i8(if fx.pick_stroke == StrokeDirection::Up {
                1
            } else {
                2
            });
        }
    }

    fn write_mix_table(&mut self, bpm: f64) {
        self.i8(-1); // instrument
        self.buf.extend([0xFFu8; 12]); // RSE instrument
        self.zeros(4);
        for _ in 0..6 {
            self.i8(-1); // volume, balance, chorus, reverb, phaser, tremolo: unchanged
        }
        self.int_byte_size_string("");
        self.i32(bpm.round() as i32);
        self.i8(0); // tempo transition
        self.i8(0); // apply-to-all flags
        self.i8(-1); // wah
    }

    fn write_note(&mut self, t: &Track, n: &Note) {
        let fx = &n.effects;
        let has_fx = fx.bend.is_some()
            || fx.hammer_pull
            || fx.let_ring
            || fx.grace.is_some()
            || fx.staccato
            || fx.palm_mute
            || fx.tremolo_picking.is_some()
            || !fx.slides.is_empty()
            || fx.harmonic.is_some()
            || fx.trill.is_some()
            || fx.vibrato;
        let fingers = fx.left_finger.is_some() || fx.right_finger.is_some();
        let mut flags = 0x20 | 0x10;
        if n.duration_percent < 1.0 {
            flags |= 0x01;
        }
        if n.accent == Accent::Heavy {
            flags |= 0x02;
        }
        if n.ghost {
            flags |= 0x04;
        }
        if has_fx {
            flags |= 0x08;
        }
        if n.accent == Accent::Normal {
            flags |= 0x40;
        }
        if fingers {
            flags |= 0x80;
        }
        self.u8(flags);
        self.u8(if n.tie {
            2
        } else if n.dead {
            3
        } else {
            1
        });
        self.i8(((n.velocity as i32 + 1) / 16).clamp(1, 8) as i8);
        let fret = if t.kind == TrackKind::Fretted {
            n.fret.min(99)
        } else {
            n.fret.min(127)
        };
        self.i8(fret as i8);
        if fingers {
            self.i8(fx.left_finger.map_or(-1, |f| f as i8));
            self.i8(fx.right_finger.map_or(-1, |f| f as i8));
        }
        if flags & 0x01 != 0 {
            self.f64(n.duration_percent as f64);
        }
        self.u8(0);
        if has_fx {
            self.write_note_effects(fx);
        }
    }

    fn write_note_effects(&mut self, fx: &NoteEffects) {
        let mut f1 = 0u8;
        let mut f2 = 0u8;
        if fx.bend.is_some() {
            f1 |= 0x01;
        }
        if fx.hammer_pull {
            f1 |= 0x02;
        }
        if fx.let_ring {
            f1 |= 0x08;
        }
        if fx.grace.is_some() {
            f1 |= 0x10;
        }
        if fx.staccato {
            f2 |= 0x01;
        }
        if fx.palm_mute {
            f2 |= 0x02;
        }
        if fx.tremolo_picking.is_some() {
            f2 |= 0x04;
        }
        if !fx.slides.is_empty() {
            f2 |= 0x08;
        }
        if fx.harmonic.is_some() {
            f2 |= 0x10;
        }
        if fx.trill.is_some() {
            f2 |= 0x20;
        }
        if fx.vibrato {
            f2 |= 0x40;
        }
        self.u8(f1);
        self.u8(f2);
        if let Some(b) = &fx.bend {
            self.write_bend(b);
        }
        if let Some(g) = &fx.grace {
            self.u8(g.fret);
            self.i8(((g.velocity as i32 + 1) / 16).clamp(1, 8) as i8);
            self.u8(match g.transition {
                GraceTransition::None => 0,
                GraceTransition::Slide => 1,
                GraceTransition::Bend => 2,
                GraceTransition::Hammer => 3,
            });
            self.u8(match g.duration {
                DurationValue::SixtyFourth | DurationValue::HundredTwentyEighth => 1,
                DurationValue::ThirtySecond => 2,
                _ => 3,
            });
            self.u8(u8::from(g.dead) | (u8::from(g.on_beat) << 1));
        }
        if let Some(d) = fx.tremolo_picking {
            self.i8(match d {
                DurationValue::Eighth => 1,
                DurationValue::Sixteenth => 2,
                _ => 3,
            });
        }
        if !fx.slides.is_empty() {
            let kinds = [
                SlideKind::Shift,
                SlideKind::Legato,
                SlideKind::OutDown,
                SlideKind::OutUp,
                SlideKind::IntoFromBelow,
                SlideKind::IntoFromAbove,
            ];
            let mask = kinds
                .iter()
                .enumerate()
                .filter(|(_, k)| fx.slides.contains(k))
                .fold(0u8, |m, (i, _)| m | (1 << i));
            self.u8(mask);
        }
        if let Some(h) = fx.harmonic {
            match h {
                Harmonic::Natural => self.i8(1),
                Harmonic::Artificial { semitones } => {
                    self.i8(2);
                    self.u8(0); // pitch class
                    self.i8(0); // accidental
                    self.u8(u8::from(semitones >= 24)); // 0 = 8va, 1 = 15ma
                }
                Harmonic::Tapped { fret } => {
                    self.i8(3);
                    self.u8(fret);
                }
                Harmonic::Pinch => self.i8(4),
                Harmonic::Semi => self.i8(5),
            }
        }
        if let Some(tr) = fx.trill {
            self.i8(tr.fret as i8);
            self.i8(match tr.speed {
                DurationValue::ThirtySecond => 2,
                DurationValue::SixtyFourth | DurationValue::HundredTwentyEighth => 3,
                _ => 1,
            });
        }
    }

    fn write_bend(&mut self, b: &Bend) {
        let peak = b
            .points
            .iter()
            .map(|p| p.value.unsigned_abs() as i32)
            .max()
            .unwrap_or(0);
        self.i8(1); // bend type
        self.i32(peak * 25);
        self.i32(b.points.len() as i32);
        for p in &b.points {
            self.i32(p.position.min(Bend::MAX_POSITION) as i32);
            self.i32(p.value as i32 * 25);
            self.bool(p.vibrato);
        }
    }
}

fn assign_channels(score: &Score) -> Vec<ChannelSlot> {
    let melodic = score.tracks.iter().filter(|t| !t.is_percussion()).count();
    let singles: Vec<u8> = (0..16).filter(|&c| c != 9).collect();
    let mut next = 0;
    score
        .tracks
        .iter()
        .map(|t| {
            if t.is_percussion() {
                return ChannelSlot {
                    port: 0,
                    channel: 9,
                    effect: 9,
                };
            }
            let k = next;
            next += 1;
            if melodic <= CHANNEL_PAIRS.len() {
                let (channel, effect) = CHANNEL_PAIRS[k];
                ChannelSlot {
                    port: 0,
                    channel,
                    effect,
                }
            } else {
                let channel = singles[k % singles.len()];
                ChannelSlot {
                    port: ((k / singles.len()) % 4) as u8,
                    channel,
                    effect: channel,
                }
            }
        })
        .collect()
}

/// Places each tempo change on a beat starting exactly at its tick (in any track or voice),
/// falling back to the first track's beat sounding at that tick.
fn tempo_targets(score: &Score) -> TempoTargets {
    let mut out = TempoTargets::new();
    for (b, mb) in score.master_bars.iter().enumerate() {
        for change in &mb.tempo_changes {
            let mut fallback = None;
            let mut found = None;
            'search: for (ti, t) in score.tracks.iter().enumerate() {
                let Some(bar) = t.bars.get(b) else { continue };
                for (v, voice) in bar.voices.iter().take(2).enumerate() {
                    let mut tick = 0;
                    for (i, beat) in voice.beats.iter().enumerate() {
                        let end = tick + beat.ticks();
                        if tick == change.tick && !out.contains_key(&(b, ti, v, i)) {
                            found = Some((b, ti, v, i));
                            break 'search;
                        }
                        if fallback.is_none() && change.tick < end.max(tick + 1) {
                            fallback = Some((b, ti, v, i));
                        }
                        tick = end;
                    }
                }
            }
            if let Some(key) = found.or(fallback).or(Some((b, 0, 0, 0))) {
                out.entry(key).or_insert(change.bpm);
            }
        }
    }
    out
}

/// MIDI 0..=127 to GP's 0..=16 channel scale (the reader multiplies by 8).
fn to_channel_value(v: u8) -> i8 {
    ((v as i32 + 4) / 8).clamp(0, 16) as i8
}

fn stroke_value(d: DurationValue) -> u8 {
    match d {
        DurationValue::HundredTwentyEighth => 1,
        DurationValue::SixtyFourth => 2,
        DurationValue::ThirtySecond => 3,
        DurationValue::Sixteenth => 4,
        DurationValue::Eighth => 5,
        _ => 6,
    }
}

/// Windows-1252 when every character fits (as Guitar Pro expects), else UTF-8.
fn encode_text(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        match cp1252_byte(c) {
            Some(b) => out.push(b),
            None => return s.as_bytes().to_vec(),
        }
    }
    out
}

fn cp1252_byte(c: char) -> Option<u8> {
    let code = c as u32;
    if code < 0x80 || (0xA0..=0xFF).contains(&code) {
        return Some(code as u8);
    }
    (0x80u8..=0x9F).find(|&b| crate::binary::decode_text(&[b]).starts_with(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_cp1252_or_utf8() {
        assert_eq!(encode_text("café"), b"caf\xe9");
        assert_eq!(encode_text("\u{201C}x"), b"\x93x");
        assert_eq!(encode_text("日本"), "日本".as_bytes());
    }

    #[test]
    fn minimal_score_round_trips() {
        let beat = Beat {
            notes: vec![Note::new(0, 3)],
            ..Beat::default()
        };
        let track = Track {
            name: "Gtr".into(),
            strings: vec![64, 59, 55, 50, 45, 40],
            fret_count: 24,
            bars: vec![Bar {
                voices: vec![Voice {
                    beats: vec![beat; 4],
                }],
                ..Default::default()
            }],
            ..Track::default()
        };
        let mut score = Score {
            tempo: 100,
            meta: Metadata {
                title: "Test".into(),
                ..Metadata::default()
            },
            master_bars: vec![MasterBar::default()],
            tracks: vec![track],
            ..Score::default()
        };
        score.page.width = 216;
        score.page.scale = 80;
        score.page.templates[0] = "<%TITLE%>".into();
        let bytes = write_gp5(&score).unwrap();
        let back = crate::gp::read(&bytes).unwrap();
        assert_eq!(back.page, score.page);
        assert_eq!(back.meta.title, "Test");
        assert_eq!(back.tempo, 100);
        assert_eq!(back.tracks[0].bars[0].voices[0].beats.len(), 4);
        assert_eq!(back.tracks[0].bars[0].voices[0].beats[0].notes[0].fret, 3);
    }
}
