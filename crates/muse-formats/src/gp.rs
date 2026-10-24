//! Guitar Pro 3, 4 and 5 binary reader.
//!
//! Field layout follows the format as documented by the PyGuitarPro and alphaTab projects.

use muse_core::*;

use crate::Result;
use crate::binary::{Reader, decode_text};

const MAX_BARS: usize = 10_000;
const MAX_TRACKS: usize = 128;
const MAX_BEATS: usize = 1_000;
const MAX_STRINGS: usize = 7;

/// Reads a GP3, GP4 or GP5 file.
pub fn read(bytes: &[u8]) -> Result<Score> {
    read_with_trailing(bytes).map(|(score, _)| score)
}

/// Reads a file and also returns how many bytes were left unread (0 for a well-formed file).
pub fn read_with_trailing(bytes: &[u8]) -> Result<(Score, usize)> {
    let mut reader = GpReader::new(bytes)?;
    let score = reader.read_score()?;
    Ok((score, bytes.len() - reader.r.pos().min(bytes.len())))
}

struct GpReader<'a> {
    r: Reader<'a>,
    /// Version as major * 100 + minor (e.g. 406, 510).
    version: u16,
    triplet_feel: TripletFeel,
    /// Last fret per track, voice and string, used to resolve tied notes.
    last_fret: Vec<[[u8; MAX_STRINGS]; 2]>,
    /// Tempo change read from the current beat's mix table.
    beat_tempo: Option<u32>,
    /// Tempo changes found in the current bar (any track), by tick.
    bar_tempos: Vec<TempoChange>,
}

struct Channel {
    program: i32,
    volume: u8,
    pan: u8,
}

impl<'a> GpReader<'a> {
    fn new(bytes: &'a [u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let version_string = byte_size_string(&mut r, 30)?;
        let version = parse_version(&version_string)
            .ok_or_else(|| r.error(format!("unsupported version '{version_string}'")))?;
        Ok(Self {
            r,
            version,
            triplet_feel: TripletFeel::None,
            last_fret: Vec::new(),
            beat_tempo: None,
            bar_tempos: Vec::new(),
        })
    }

    fn gp5(&self) -> bool {
        self.version >= 500
    }

    fn gp4(&self) -> bool {
        self.version >= 400
    }

    fn read_score(&mut self) -> Result<Score> {
        let mut score = Score::default();
        score
            .extras
            .insert("gp.version".into(), self.version.to_string());
        self.read_info(&mut score.meta)?;
        if !self.gp5() {
            self.triplet_feel = if self.r.bool()? {
                TripletFeel::Eighth
            } else {
                TripletFeel::None
            };
        }
        if self.gp4() {
            score.lyrics = Some(self.read_lyrics()?);
        }
        if self.version > 500 {
            // RSE master volume, unknown, 11-band equalizer.
            self.r.skip(4 + 4 + 11)?;
        }
        if self.gp5() {
            self.read_page_setup()?;
            let tempo_name = self.int_byte_size_string()?;
            if !tempo_name.is_empty() {
                score.extras.insert("gp.tempo_name".into(), tempo_name);
            }
        }
        score.tempo = self.r.i32()?.clamp(1, 1000) as u32;
        if self.version > 500 {
            self.r.bool()?; // hide tempo
        }
        let key = if self.gp5() {
            let k = self.r.i8()?;
            self.r.i32()?; // octave
            k
        } else {
            let k = self.r.i32()? as i8;
            if self.gp4() {
                self.r.i8()?; // octave
            }
            k
        };
        let channels = self.read_midi_channels()?;
        let directions = if self.gp5() {
            Some(self.read_directions()?)
        } else {
            None
        };
        if self.gp5() {
            self.r.i32()?; // master reverb
        }
        let bar_count = self.r.count(MAX_BARS, "bar")?;
        let track_count = self.r.count(MAX_TRACKS, "track")?;

        for i in 0..bar_count {
            self.r.location = Some(format!("bar header {}", i + 1));
            let previous = score.master_bars.last().cloned();
            let mb = self.read_master_bar(previous.as_ref(), &score.master_bars, key)?;
            score.master_bars.push(mb);
        }
        if let Some(directions) = directions {
            for (bar, direction) in directions {
                if let Some(mb) = score.master_bars.get_mut(bar) {
                    mb.directions.push(direction);
                }
            }
        }
        for i in 0..track_count {
            self.r.location = Some(format!("track {}", i + 1));
            let track = self.read_track(i, &channels)?;
            score.tracks.push(track);
        }
        if self.gp5() {
            self.r.skip(if self.version == 500 { 2 } else { 1 })?;
        }
        self.last_fret = vec![[[0; MAX_STRINGS]; 2]; track_count];
        for bar in 0..bar_count {
            for t in 0..track_count {
                self.r.location = Some(format!("track {}, bar {}", t + 1, bar + 1));
                let b = self.read_bar(&score.tracks[t], t)?;
                score.tracks[t].bars.push(b);
            }
            let mut changes = std::mem::take(&mut self.bar_tempos);
            changes.sort_by_key(|c| c.tick);
            changes.dedup_by_key(|c| c.tick);
            if bar == 0 && changes.first().is_some_and(|c| c.tick == 0) {
                score.tempo = changes.remove(0).bpm.round() as u32;
            }
            score.master_bars[bar].tempo_changes = changes;
        }
        // Guitar Pro sings the lyric lines one syllable per note; put them on the beats.
        muse_core::lyrics::attach(&mut score);
        Ok(score)
    }

    fn int_byte_size_string(&mut self) -> Result<String> {
        let size = self.r.i32()?;
        if !(0..=1 << 20).contains(&size) {
            return Err(self.r.error(format!("invalid string size {size}")));
        }
        let len = self.r.u8()? as usize;
        let field = (size as usize).saturating_sub(1);
        let raw = self.r.bytes(if field > 0 { field } else { len })?;
        Ok(decode_text(&raw[..len.min(raw.len())]))
    }

    fn int_size_string(&mut self) -> Result<String> {
        let len = self.r.count(1 << 20, "string length")?;
        Ok(decode_text(self.r.bytes(len)?))
    }

    fn read_info(&mut self, meta: &mut Metadata) -> Result<()> {
        meta.title = self.int_byte_size_string()?;
        meta.subtitle = self.int_byte_size_string()?;
        meta.artist = self.int_byte_size_string()?;
        meta.album = self.int_byte_size_string()?;
        meta.words = self.int_byte_size_string()?;
        meta.music = if self.gp5() {
            self.int_byte_size_string()?
        } else {
            meta.words.clone()
        };
        meta.copyright = self.int_byte_size_string()?;
        meta.tab_author = self.int_byte_size_string()?;
        meta.instructions = self.int_byte_size_string()?;
        let n = self.r.count(1000, "notice")?;
        for _ in 0..n {
            meta.notices.push(self.int_byte_size_string()?);
        }
        Ok(())
    }

    fn read_lyrics(&mut self) -> Result<Lyrics> {
        let track = self.r.i32()?;
        let mut lyrics = Lyrics {
            track: (track > 0).then(|| track as usize - 1),
            lines: vec![],
        };
        for _ in 0..5 {
            let start = self.r.i32()?;
            let text = self.int_size_string()?;
            if !text.is_empty() {
                lyrics.lines.push(LyricLine {
                    start_bar: (start - 1).max(0) as u32,
                    text,
                });
            }
        }
        Ok(lyrics)
    }

    fn read_page_setup(&mut self) -> Result<()> {
        // Page size (2), margins (4), score size proportion: i32 each; header/footer flags: i16.
        self.r.skip(4 * 7 + 2)?;
        for _ in 0..10 {
            self.int_byte_size_string()?;
        }
        Ok(())
    }

    fn read_midi_channels(&mut self) -> Result<Vec<Channel>> {
        let mut channels = Vec::with_capacity(64);
        for _ in 0..64 {
            let program = self.r.i32()?;
            let volume = self.r.i8()?;
            let pan = self.r.i8()?;
            self.r.skip(4 + 2)?; // chorus, reverb, phaser, tremolo, padding
            channels.push(Channel {
                program,
                volume: channel_value(volume),
                pan: channel_value(pan),
            });
        }
        Ok(channels)
    }

    fn read_directions(&mut self) -> Result<Vec<(usize, Direction)>> {
        use Direction::*;
        const ORDER: [Direction; 19] = [
            Coda,
            DoubleCoda,
            Segno,
            SegnoSegno,
            Fine,
            DaCapo,
            DaCapoAlCoda,
            DaCapoAlDoubleCoda,
            DaCapoAlFine,
            DaSegno,
            DaSegnoAlCoda,
            DaSegnoAlDoubleCoda,
            DaSegnoAlFine,
            DaSegnoSegno,
            DaSegnoSegnoAlCoda,
            DaSegnoSegnoAlDoubleCoda,
            DaSegnoSegnoAlFine,
            DaCoda,
            DaDoubleCoda,
        ];
        let mut out = Vec::new();
        for d in ORDER {
            let bar = self.r.i16()?;
            if bar > 0 {
                out.push((bar as usize - 1, d));
            }
        }
        Ok(out)
    }

    fn read_master_bar(
        &mut self,
        previous: Option<&MasterBar>,
        bars: &[MasterBar],
        song_key: i8,
    ) -> Result<MasterBar> {
        if self.gp5() && previous.is_some() {
            self.r.skip(1)?;
        }
        let flags = self.r.u8()?;
        let mut mb = MasterBar {
            time_signature: previous.map(|p| p.time_signature).unwrap_or_default(),
            key_signature: previous.map(|p| p.key_signature).unwrap_or(KeySignature {
                accidentals: song_key,
                minor: false,
            }),
            triplet_feel: self.triplet_feel,
            ..MasterBar::default()
        };
        if flags & 0x01 != 0 {
            mb.time_signature.numerator = self.r.i8()?.max(1) as u8;
        }
        if flags & 0x02 != 0 {
            mb.time_signature.denominator = self.r.i8()?.max(1) as u8;
        }
        mb.repeat_start = flags & 0x04 != 0;
        if flags & 0x08 != 0 {
            let n = self.r.i8()?.max(0) as u8;
            // GP3/4 store the number of repeats, GP5 the number of plays.
            mb.repeat_end = Some(if self.gp5() { n } else { n.saturating_add(1) });
        }
        if flags & 0x10 != 0 && !self.gp5() {
            let n = self.r.u8()?.min(8);
            let mut existing = 0u8;
            for b in bars.iter().rev() {
                if b.repeat_start {
                    break;
                }
                existing |= b.alternate_endings;
            }
            mb.alternate_endings = ((1u16 << n) - 1) as u8 ^ existing;
        }
        if flags & 0x20 != 0 {
            let title = self.int_byte_size_string()?;
            let color = self.read_color()?;
            mb.marker = Some(Marker { title, color });
        }
        if flags & 0x40 != 0 {
            let accidentals = self.r.i8()?;
            let minor = self.r.i8()? != 0;
            mb.key_signature = KeySignature { accidentals, minor };
        }
        mb.double_bar = flags & 0x80 != 0;
        if self.gp5() {
            if flags & 0x10 != 0 {
                mb.alternate_endings = self.r.u8()?;
            }
            if flags & 0x03 != 0 {
                self.r.skip(4)?; // beam grouping
            }
            if flags & 0x10 == 0 {
                self.r.skip(1)?;
            }
            mb.triplet_feel = match self.r.u8()? {
                1 => TripletFeel::Eighth,
                2 => TripletFeel::Sixteenth,
                _ => TripletFeel::None,
            };
        }
        Ok(mb)
    }

    fn read_color(&mut self) -> Result<Color> {
        let c = self.r.bytes(4)?;
        Ok(Color {
            r: c[0],
            g: c[1],
            b: c[2],
        })
    }

    fn read_track(&mut self, index: usize, channels: &[Channel]) -> Result<Track> {
        if self.gp5() && (index == 0 || self.version == 500) {
            self.r.skip(1)?;
        }
        let flags = self.r.u8()?;
        let mut track = Track {
            name: byte_size_string(&mut self.r, 40)?,
            ..Track::default()
        };
        let string_count = self.r.count(MAX_STRINGS, "string")?;
        for i in 0..MAX_STRINGS {
            let tuning = self.r.i32()?;
            if i < string_count {
                track.strings.push(tuning.clamp(0, 127) as u8);
            }
        }
        let port = self.r.i32()?.max(1) as u32;
        let channel_index = self.r.i32()? - 1;
        let effect_channel = self.r.i32()? - 1;
        track.fret_count = self.r.i32()?.clamp(0, 99) as u8;
        track.capo = self.r.i32()?.clamp(0, 99) as u8;
        track.color = self.read_color()?;

        let midi_channel = channel_index.clamp(0, 15) as u8;
        track.midi.channel = midi_channel;
        track.midi.effect_channel = effect_channel.clamp(0, 15) as u8;
        let ch_slot = (port as usize - 1) * 16 + channel_index.max(0) as usize;
        if let Some(ch) = channels.get(ch_slot) {
            track.midi.program = ch.program.clamp(0, 127) as u8;
            track.midi.volume = ch.volume;
            track.midi.pan = ch.pan;
        }
        track.kind = if flags & 0x01 != 0 || midi_channel == 9 {
            TrackKind::Drums
        } else {
            TrackKind::Fretted
        };
        if flags & 0x02 != 0 {
            track
                .extras
                .insert("gp.twelve_string".into(), "true".into());
        }
        if flags & 0x04 != 0 {
            track.extras.insert("gp.banjo".into(), "true".into());
        }
        if self.gp5() {
            track.mute = flags & 0x20 != 0;
            track.solo = flags & 0x10 != 0;
            self.r.i16()?; // display flags
            self.r.u8()?; // auto accentuation
            track.midi.bank = self.r.u8()?;
            self.r.u8()?; // humanize
            self.r.skip(4 * 3 + 12)?;
            // RSE instrument
            self.r.skip(4 * 3)?;
            if self.version == 500 {
                self.r.skip(2 + 1)?;
            } else {
                self.r.skip(4)?;
            }
            if self.version > 500 {
                self.r.skip(4)?; // equalizer
                self.int_byte_size_string()?;
                self.int_byte_size_string()?;
            }
        }
        Ok(track)
    }

    fn read_bar(&mut self, track: &Track, track_index: usize) -> Result<Bar> {
        let voice_count = if self.gp5() { 2 } else { 1 };
        let mut bar = Bar::default();
        for v in 0..voice_count {
            let n = self.r.count(MAX_BEATS, "beat")?;
            let mut voice = Voice::default();
            let mut tick = 0;
            for _ in 0..n {
                let beat = self.read_beat(track, track_index, v)?;
                if let Some(bpm) = self.beat_tempo.take() {
                    self.bar_tempos.push(TempoChange {
                        tick,
                        bpm: bpm as f64,
                    });
                }
                tick += beat.ticks();
                voice.beats.push(beat);
            }
            bar.voices.push(voice);
        }
        if self.gp5() && !self.r.at_end() {
            self.r.u8()?; // line break
        }
        Ok(bar)
    }

    fn read_beat(&mut self, track: &Track, track_index: usize, voice: usize) -> Result<Beat> {
        let flags = self.r.u8()?;
        let mut beat = Beat::default();
        // Status: 0 = empty, 1 = normal, 2 = rest. Notes on a "rest" beat still sound in Guitar
        // Pro (and other readers keep them), so only the empty status matters here.
        if flags & 0x40 != 0 && self.r.u8()? == 0 {
            beat.empty = true;
        }
        beat.duration = self.read_duration(flags)?;
        let mut shared = NoteEffects::default();
        if flags & 0x02 != 0 {
            beat.chord = Some(self.read_chord(track.strings.len())?);
        }
        if flags & 0x04 != 0 {
            beat.text = Some(self.int_byte_size_string()?);
        }
        if flags & 0x08 != 0 {
            beat.effects = self.read_beat_effects(&mut shared)?;
        }
        if flags & 0x10 != 0 {
            self.beat_tempo = self.read_mix_table()?;
        }
        let string_flags = self.r.u8()?;
        for s in 0..track.strings.len() {
            if string_flags & (1 << (6 - s)) != 0 {
                let note = self.read_note(s as u8, &shared, track_index, voice)?;
                beat.notes.push(note);
            }
        }
        if self.gp5() {
            let flags2 = self.r.i16()?;
            beat.effects.ottava = if flags2 & 0x10 != 0 {
                Ottava::Ottava
            } else if flags2 & 0x20 != 0 {
                Ottava::OttavaBassa
            } else if flags2 & 0x40 != 0 {
                Ottava::Quindicesima
            } else if flags2 & 0x100 != 0 {
                Ottava::QuindicesimaBassa
            } else {
                Ottava::None
            };
            if flags2 & 0x800 != 0 {
                self.r.u8()?; // break secondary beams
            }
        }
        Ok(beat)
    }

    fn read_duration(&mut self, flags: u8) -> Result<Duration> {
        let v = self.r.i8()?;
        let value = match v {
            -2 => DurationValue::Whole,
            -1 => DurationValue::Half,
            0 => DurationValue::Quarter,
            1 => DurationValue::Eighth,
            2 => DurationValue::Sixteenth,
            3 => DurationValue::ThirtySecond,
            4 => DurationValue::SixtyFourth,
            5 => DurationValue::HundredTwentyEighth,
            _ => return Err(self.r.error(format!("invalid duration {v}"))),
        };
        let mut d = Duration::new(value);
        if flags & 0x01 != 0 {
            d.dots = 1;
        }
        if flags & 0x20 != 0 {
            let enters = self.r.i32()?;
            let times = match enters {
                3 => 2,
                5..=7 => 4,
                9..=13 => 8,
                _ => 0,
            };
            if times > 0 {
                d.tuplet = Some(Tuplet {
                    enters: enters as u8,
                    times,
                });
            }
        }
        Ok(d)
    }

    fn read_chord(&mut self, string_count: usize) -> Result<Chord> {
        let mut chord = Chord::default();
        let new_format = self.r.bool()?;
        let to_fret = |f: i32| (f >= 0).then(|| f.min(99) as u8);
        if !new_format {
            chord.name = self.int_byte_size_string()?;
            chord.first_fret = self.r.i32()?.clamp(0, 99) as u8;
            if chord.first_fret != 0 {
                for i in 0..6 {
                    let f = self.r.i32()?;
                    if i < string_count {
                        chord.frets.push(to_fret(f));
                    }
                }
            }
            return Ok(chord);
        }
        if self.gp4() {
            self.r.skip(16)?;
            chord.name = byte_size_string(&mut self.r, 21)?;
            self.r.skip(4)?;
            chord.first_fret = self.r.i32()?.clamp(0, 99) as u8;
            for i in 0..7 {
                let f = self.r.i32()?;
                if i < string_count {
                    chord.frets.push(to_fret(f));
                }
            }
            let barre_count = self.r.u8()?.min(5) as usize;
            let b = self.r.bytes(15)?;
            for i in 0..barre_count {
                chord.barres.push(Barre {
                    fret: b[i],
                    first_string: b[5 + i],
                    last_string: b[10 + i],
                });
            }
            self.r.skip(7 + 1 + 7 + 1)?; // omissions, padding, fingerings, show
        } else {
            self.r.skip(25)?;
            chord.name = byte_size_string(&mut self.r, 22)?;
            self.r.skip(12)?;
            chord.first_fret = self.r.i32()?.clamp(0, 99) as u8;
            for i in 0..6 {
                let f = self.r.i32()?;
                if i < string_count {
                    chord.frets.push(to_fret(f));
                }
            }
            let barre_count = self.r.i32()?.clamp(0, 2) as usize;
            let mut vals = [0i32; 6];
            for v in &mut vals {
                *v = self.r.i32()?;
            }
            for i in 0..barre_count {
                chord.barres.push(Barre {
                    fret: vals[i].clamp(0, 99) as u8,
                    first_string: vals[2 + i].clamp(0, 7) as u8,
                    last_string: vals[4 + i].clamp(0, 7) as u8,
                });
            }
            self.r.skip(7 + 1)?;
        }
        Ok(chord)
    }

    fn read_beat_effects(&mut self, shared: &mut NoteEffects) -> Result<BeatEffects> {
        let mut fx = BeatEffects::default();
        let flags1 = self.r.u8()?;
        let flags2 = if self.gp4() { self.r.u8()? } else { 0 };
        fx.vibrato = flags1 & 0x02 != 0;
        fx.fade_in = flags1 & 0x10 != 0;
        if !self.gp4() {
            shared.vibrato = flags1 & 0x01 != 0;
            if flags1 & 0x04 != 0 {
                shared.harmonic = Some(Harmonic::Natural);
            }
            if flags1 & 0x08 != 0 {
                shared.harmonic = Some(Harmonic::Artificial { semitones: 12 });
            }
        }
        if flags1 & 0x20 != 0 {
            fx.slap = slap(self.r.u8()?);
            if !self.gp4() {
                let value = self.r.i32()?;
                if fx.slap == Slap::None {
                    let dip = (-(value / 25)).clamp(-400, 400) as i16;
                    fx.tremolo_bar = Some(Bend {
                        points: vec![
                            BendPoint {
                                position: 0,
                                value: 0,
                                vibrato: false,
                            },
                            BendPoint {
                                position: 30,
                                value: dip,
                                vibrato: false,
                            },
                            BendPoint {
                                position: 60,
                                value: 0,
                                vibrato: false,
                            },
                        ],
                    });
                }
            }
        }
        if flags2 & 0x04 != 0 {
            fx.tremolo_bar = self.read_bend()?;
        }
        if flags1 & 0x40 != 0 {
            let first = self.r.i8()?;
            let second = self.r.i8()?;
            // GP3/4 store (down, up); GP5 stores (up, down).
            let (down, up) = if self.gp5() {
                (second, first)
            } else {
                (first, second)
            };
            fx.stroke = if up > 0 {
                Some(Stroke {
                    direction: StrokeDirection::Up,
                    duration: stroke_value(up),
                })
            } else if down > 0 {
                Some(Stroke {
                    direction: StrokeDirection::Down,
                    duration: stroke_value(down),
                })
            } else {
                None
            };
        }
        fx.rasgueado = flags2 & 0x01 != 0;
        if flags2 & 0x02 != 0 {
            fx.pick_stroke = match self.r.i8()? {
                1 => StrokeDirection::Up,
                2 => StrokeDirection::Down,
                _ => StrokeDirection::None,
            };
        }
        Ok(fx)
    }

    /// Reads a mix table change, returning its tempo change if any.
    fn read_mix_table(&mut self) -> Result<Option<u32>> {
        self.r.i8()?; // instrument
        if self.gp5() {
            // RSE instrument (GP5.0 has a shorter effect field plus one padding byte).
            self.r.skip(12 + 4)?;
        }
        let mut values = [0i8; 6]; // volume, balance, chorus, reverb, phaser, tremolo
        for v in &mut values {
            *v = self.r.i8()?;
        }
        if self.gp5() {
            self.int_byte_size_string()?; // tempo name
        }
        let tempo = self.r.i32()?;
        for v in values {
            if v >= 0 {
                self.r.i8()?; // transition duration
            }
        }
        if tempo >= 0 {
            self.r.i8()?;
            if self.version > 500 {
                self.r.bool()?; // hide tempo
            }
        }
        if self.gp4() {
            self.r.i8()?; // apply-to-all-tracks flags
        }
        if self.gp5() {
            self.r.i8()?; // wah
            if self.version > 500 {
                self.int_byte_size_string()?;
                self.int_byte_size_string()?;
            }
        }
        Ok((tempo > 0).then_some(tempo as u32))
    }

    fn read_note(
        &mut self,
        string: u8,
        shared: &NoteEffects,
        track_index: usize,
        voice: usize,
    ) -> Result<Note> {
        let flags = self.r.u8()?;
        let mut note = Note::new(string, 0);
        note.effects = shared.clone();
        note.ghost = flags & 0x04 != 0;
        if flags & 0x40 != 0 {
            note.accent = Accent::Normal;
        }
        if self.gp5() && flags & 0x02 != 0 {
            note.accent = Accent::Heavy;
        }
        let mut kind = 1;
        if flags & 0x20 != 0 {
            kind = self.r.u8()?;
        }
        if !self.gp5() && flags & 0x01 != 0 {
            self.r.skip(2)?; // independent note duration and tuplet
        }
        if flags & 0x10 != 0 {
            let dynamic = self.r.i8()? as i32;
            note.velocity = (16 * dynamic - 1).clamp(1, 127) as u8;
        }
        let last = &mut self.last_fret[track_index][voice][string as usize];
        if flags & 0x20 != 0 {
            let fret = self.r.i8()?;
            note.fret = fret.clamp(0, 99) as u8;
        }
        match kind {
            2 => {
                note.tie = true;
                note.fret = *last;
            }
            3 => note.dead = true,
            _ => {}
        }
        *last = note.fret;
        if flags & 0x80 != 0 {
            note.effects.left_finger = finger(self.r.i8()?);
            note.effects.right_finger = finger(self.r.i8()?);
        }
        if self.gp5() {
            if flags & 0x01 != 0 {
                note.duration_percent = self.r.f64()?.clamp(0.0, 1.0) as f32;
            }
            self.r.u8()?; // accidental display flags
        }
        if flags & 0x08 != 0 {
            self.read_note_effects(&mut note)?;
        }
        Ok(note)
    }

    fn read_note_effects(&mut self, note: &mut Note) -> Result<()> {
        let flags1 = self.r.u8()?;
        let flags2 = if self.gp4() { self.r.u8()? } else { 0 };
        let fx = &mut note.effects;
        fx.hammer_pull = flags1 & 0x02 != 0;
        fx.let_ring = flags1 & 0x08 != 0;
        if flags1 & 0x01 != 0 {
            fx.bend = self.read_bend()?;
        }
        if flags1 & 0x10 != 0 {
            fx.grace = Some(self.read_grace()?);
        }
        if !self.gp4() {
            if flags1 & 0x04 != 0 {
                fx.slides.push(SlideKind::Shift);
            }
            return Ok(());
        }
        fx.staccato = flags2 & 0x01 != 0;
        fx.palm_mute = flags2 & 0x02 != 0;
        fx.vibrato |= flags2 & 0x40 != 0;
        if flags2 & 0x04 != 0 {
            fx.tremolo_picking = match self.r.i8()? {
                1 => Some(DurationValue::Eighth),
                2 => Some(DurationValue::Sixteenth),
                3 => Some(DurationValue::ThirtySecond),
                _ => None,
            };
        }
        if flags2 & 0x08 != 0 {
            if self.gp5() {
                let s = self.r.u8()?;
                let kinds = [
                    SlideKind::Shift,
                    SlideKind::Legato,
                    SlideKind::OutDown,
                    SlideKind::OutUp,
                    SlideKind::IntoFromBelow,
                    SlideKind::IntoFromAbove,
                ];
                fx.slides = kinds
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| s & (1 << i) != 0)
                    .map(|(_, k)| k)
                    .collect();
            } else {
                let kind = match self.r.i8()? {
                    -2 => Some(SlideKind::IntoFromAbove),
                    -1 => Some(SlideKind::IntoFromBelow),
                    1 => Some(SlideKind::Shift),
                    2 => Some(SlideKind::Legato),
                    3 => Some(SlideKind::OutDown),
                    4 => Some(SlideKind::OutUp),
                    _ => None,
                };
                fx.slides.extend(kind);
            }
        }
        if flags2 & 0x10 != 0 {
            let kind = self.r.i8()?;
            fx.harmonic = match kind {
                1 => Some(Harmonic::Natural),
                2 if self.gp5() => {
                    self.r.skip(2)?; // pitch class, accidental
                    // Octave: 0 = 8va, 1 = 15ma.
                    let semitones = if self.r.u8()? == 1 { 24 } else { 12 };
                    Some(Harmonic::Artificial { semitones })
                }
                3 => {
                    let fret = if self.gp5() {
                        self.r.u8()?
                    } else {
                        note.fret.saturating_add(12)
                    };
                    Some(Harmonic::Tapped { fret })
                }
                4 => Some(Harmonic::Pinch),
                5 => Some(Harmonic::Semi),
                15 => Some(Harmonic::Artificial { semitones: 19 }),
                17 => Some(Harmonic::Artificial { semitones: 24 }),
                22 => Some(Harmonic::Artificial { semitones: 12 }),
                _ => None,
            };
        }
        if flags2 & 0x20 != 0 {
            let fret = self.r.i8()?.clamp(0, 99) as u8;
            let speed = match self.r.i8()? {
                2 => DurationValue::ThirtySecond,
                3 => DurationValue::SixtyFourth,
                _ => DurationValue::Sixteenth,
            };
            note.effects.trill = Some(Trill { fret, speed });
        }
        Ok(())
    }

    fn read_bend(&mut self) -> Result<Option<Bend>> {
        self.r.i8()?; // bend type
        self.r.i32()?; // value
        let n = self.r.count(100, "bend point")?;
        let mut points = Vec::with_capacity(n);
        for _ in 0..n {
            let position = self.r.i32()?.clamp(0, Bend::MAX_POSITION as i32) as u8;
            let value = (self.r.i32()? / 25).clamp(-400, 400) as i16;
            let vibrato = self.r.bool()?;
            points.push(BendPoint {
                position,
                value,
                vibrato,
            });
        }
        Ok((!points.is_empty()).then_some(Bend { points }))
    }

    fn read_grace(&mut self) -> Result<Grace> {
        let fret = self.r.i8()?;
        let dynamic = self.r.u8()? as i32;
        let (transition, duration, flags) = if self.gp5() {
            let t = self.r.u8()?;
            let d = self.r.u8()?;
            (t as i8, d, self.r.u8()?)
        } else {
            let d = self.r.u8()?;
            (self.r.i8()?, d, 0)
        };
        Ok(Grace {
            fret: fret.max(0) as u8,
            velocity: (16 * dynamic - 1).clamp(1, 127) as u8,
            duration: match duration {
                1 => DurationValue::SixtyFourth,
                2 => DurationValue::ThirtySecond,
                _ => DurationValue::Sixteenth,
            },
            dead: if self.gp5() {
                flags & 0x01 != 0
            } else {
                fret == -1
            },
            on_beat: flags & 0x02 != 0,
            transition: match transition {
                1 => GraceTransition::Slide,
                2 => GraceTransition::Bend,
                3 => GraceTransition::Hammer,
                _ => GraceTransition::None,
            },
        })
    }
}

fn byte_size_string(r: &mut Reader, size: usize) -> Result<String> {
    let len = r.u8()? as usize;
    let raw = r.bytes(size)?;
    Ok(decode_text(&raw[..len.min(size)]))
}

fn parse_version(s: &str) -> Option<u16> {
    let rest = s.strip_prefix("FICHIER GUITAR PRO ")?;
    let digits = rest.trim_start_matches(['v', 'L']);
    let (major, minor) = digits.split_once('.')?;
    let major: u16 = major.parse().ok()?;
    let minor: u16 = minor.get(..2)?.parse().ok()?;
    (3..=5).contains(&major).then_some(major * 100 + minor)
}

fn channel_value(v: i8) -> u8 {
    (v as i32 * 8).clamp(0, 127) as u8
}

fn slap(v: u8) -> Slap {
    match v {
        1 => Slap::Tap,
        2 => Slap::Slap,
        3 => Slap::Pop,
        _ => Slap::None,
    }
}

fn stroke_value(v: i8) -> DurationValue {
    match v {
        1 => DurationValue::HundredTwentyEighth,
        3 => DurationValue::ThirtySecond,
        4 => DurationValue::Sixteenth,
        5 => DurationValue::Eighth,
        6 => DurationValue::Quarter,
        _ => DurationValue::SixtyFourth,
    }
}

fn finger(v: i8) -> Option<u8> {
    (v >= 0).then_some(v as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("FICHIER GUITAR PRO v3.00"), Some(300));
        assert_eq!(parse_version("FICHIER GUITAR PRO L4.06"), Some(406));
        assert_eq!(parse_version("FICHIER GUITAR PRO v5.10"), Some(510));
        assert_eq!(parse_version("FICHIER GUITARE PRO v1.04"), None);
    }

    #[test]
    fn rejects_truncated_input() {
        let mut data = vec![24u8];
        data.extend(b"FICHIER GUITAR PRO v5.00");
        data.resize(31, 0);
        assert!(read(&data).is_err());
    }
}
