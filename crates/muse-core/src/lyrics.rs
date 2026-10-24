//! Lyrics as syllables on beats.
//!
//! Every beat holds one syllable per verse (`Beat::lyrics`); a syllable that the next one
//! continues ends with `-`. Guitar Pro 3–5 store lyrics instead as up to five text lines, each
//! starting at a bar and sung one syllable per note; [`attach`] and [`lines_from_beats`]
//! convert between the two.

use crate::{Beat, LyricLine, Score, Track};

/// Splits Guitar Pro lyric text into syllables, one per note: syllables end at whitespace or
/// after a hyphen (`he-si-tate` → `he-`, `si-`, `tate`); every further space leaves a note
/// without a syllable (an empty string), except right after a hyphen; line breaks only separate;
/// `[…]` at the start of a syllable is a comment; `+` joins words on one note; trailing `_` are
/// dropped.
pub fn syllables(text: &str) -> Vec<String> {
    #[derive(PartialEq)]
    enum State {
        Between,
        Start,
        Word,
        Comment,
        Dashes,
    }
    let c: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let push = |out: &mut Vec<String>, s: &[char]| {
        let chunk: String = s.iter().map(|&c| if c == '+' { ' ' } else { c }).collect();
        out.push(chunk.trim_end_matches('_').to_string());
    };
    let mut state = State::Start;
    let mut skip_space = false;
    let mut start = 0;
    let mut p = 0;
    while p < c.len() {
        let ch = c[p];
        match state {
            State::Between => match ch {
                '\n' | '\r' | '\t' => {}
                ' ' if skip_space => {}
                _ => {
                    skip_space = false;
                    state = State::Start;
                    continue;
                }
            },
            State::Start => {
                if ch == '[' {
                    state = State::Comment;
                } else {
                    start = p;
                    state = State::Word;
                    continue;
                }
            }
            State::Comment => {
                if ch == ']' {
                    state = State::Start;
                }
            }
            State::Word => match ch {
                '-' => state = State::Dashes,
                '\r' | '\n' | ' ' => {
                    push(&mut out, &c[start..p]);
                    state = State::Between;
                }
                _ => {}
            },
            State::Dashes => {
                if ch != '-' {
                    push(&mut out, &c[start..p]);
                    skip_space = true;
                    state = State::Between;
                    continue;
                }
            }
        }
        p += 1;
    }
    if matches!(state, State::Word | State::Dashes) && p != start {
        push(&mut out, &c[start..p]);
    }
    out
}

/// Whether a beat takes a syllable: anything but rests and empty beats (tied notes too, as in
/// Guitar Pro).
pub fn sings(beat: &Beat) -> bool {
    !beat.empty && !beat.notes.is_empty()
}

/// The singing beats of a track's first voice from `start_bar`, as (bar, beat) positions.
fn singing_beats(track: &Track, start_bar: usize) -> Vec<(usize, usize)> {
    track
        .bars
        .iter()
        .enumerate()
        .skip(start_bar)
        .flat_map(|(bi, bar)| {
            bar.voices
                .first()
                .into_iter()
                .flat_map(|v| v.beats.iter().enumerate())
                .filter(|(_, b)| sings(b))
                .map(move |(i, _)| (bi, i))
        })
        .collect()
}

/// Sets syllables on a verse of a track from the beat at (`bar`, `beat`) onwards (singing beats
/// only, first voice). Returns how many syllables were placed.
pub fn place(
    track: &mut Track,
    verse: usize,
    bar: usize,
    beat: usize,
    syllables: &[String],
) -> usize {
    let targets: Vec<(usize, usize)> = singing_beats(track, bar)
        .into_iter()
        .filter(|&(b, i)| b > bar || i >= beat)
        .collect();
    let mut n = 0;
    for (&(b, i), syl) in targets.iter().zip(syllables) {
        let beat = &mut track.bars[b].voices[0].beats[i];
        if beat.lyrics.len() <= verse {
            beat.lyrics.resize(verse + 1, String::new());
        }
        beat.lyrics[verse] = syl.clone();
        while beat.lyrics.last().is_some_and(String::is_empty) {
            beat.lyrics.pop();
        }
        n += 1;
    }
    n
}

/// Puts a Guitar Pro 3–5 score's lyric lines on the beats of its lyric track, when that track
/// has no syllables yet.
pub fn attach(score: &mut Score) {
    let Some(lyrics) = score.lyrics.clone() else {
        return;
    };
    let Some(track) = lyrics.track.and_then(|t| score.tracks.get_mut(t)) else {
        return;
    };
    if has_lyrics(track) {
        return;
    }
    for (verse, line) in lyrics.lines.iter().enumerate() {
        place(
            track,
            verse,
            line.start_bar as usize,
            0,
            &syllables(&line.text),
        );
    }
}

pub fn has_lyrics(track: &Track) -> bool {
    track
        .bars
        .iter()
        .flat_map(|b| &b.voices)
        .flat_map(|v| &v.beats)
        .any(|b| b.lyrics.iter().any(|l| !l.is_empty()))
}

/// Lyric lines (one per verse) rebuilt from a track's syllables, in Guitar Pro's text form.
pub fn lines_from_beats(track: &Track) -> Vec<LyricLine> {
    let verses = track
        .bars
        .iter()
        .flat_map(|b| b.voices.first())
        .flat_map(|v| &v.beats)
        .map(|b| b.lyrics.len())
        .max()
        .unwrap_or(0);
    let mut out = Vec::new();
    for verse in 0..verses {
        let Some(start) = track.bars.iter().position(|bar| {
            bar.voices.first().is_some_and(|v| {
                v.beats
                    .iter()
                    .any(|b| b.lyrics.get(verse).is_some_and(|s| !s.is_empty()))
            })
        }) else {
            continue;
        };
        // One chunk per singing beat from the start bar; notes without a syllable are empty.
        let mut chunks: Vec<String> = singing_beats(track, start)
            .into_iter()
            .map(|(b, i)| {
                let beat = &track.bars[b].voices[0].beats[i];
                beat.lyrics
                    .get(verse)
                    .cloned()
                    .unwrap_or_default()
                    .replace(' ', "+")
            })
            .collect();
        while chunks.last().is_some_and(String::is_empty) {
            chunks.pop();
        }
        let mut text = String::new();
        for (k, chunk) in chunks.iter().enumerate() {
            if k > 0 && !chunks[k - 1].ends_with('-') {
                text.push(' ');
            }
            text.push_str(chunk);
        }
        out.push(LyricLine {
            start_bar: start as u32,
            text,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    fn track(n: usize) -> Track {
        let beat = |p: u8, tie: bool| Beat {
            duration: Duration::new(DurationValue::Quarter),
            notes: if p == 0 {
                vec![]
            } else {
                vec![Note {
                    tie,
                    ..Note::new(0, p)
                }]
            },
            ..Beat::default()
        };
        Track {
            kind: TrackKind::Pitched,
            bars: (0..n)
                .map(|_| Bar {
                    voices: vec![Voice {
                        beats: vec![
                            beat(60, false),
                            beat(0, false),
                            beat(62, false),
                            beat(62, true),
                        ],
                    }],
                })
                .collect(),
            ..Track::default()
        }
    }

    #[test]
    fn syllables_split_at_spaces_and_hyphens() {
        assert_eq!(
            syllables("I won't he-si-tate\r\nno-  more a+few_"),
            ["I", "won't", "he-", "si-", "tate", "no-", "more", "a few"]
        );
        // Extra spaces leave notes without a syllable; brackets are comments.
        assert_eq!(syllables("la  la [Chorus]Oh"), ["la", "", "la", "Oh"]);
    }

    #[test]
    fn lines_attach_to_singing_beats_and_come_back() {
        let mut score = Score {
            master_bars: vec![MasterBar::default(); 3],
            tracks: vec![track(3)],
            lyrics: Some(Lyrics {
                track: Some(0),
                lines: vec![
                    LyricLine {
                        start_bar: 0,
                        text: "Hel-lo there my".into(),
                    },
                    LyricLine {
                        start_bar: 1,
                        text: "Sec-ond".into(),
                    },
                ],
            }),
            ..Score::default()
        };
        attach(&mut score);
        let t = &score.tracks[0];
        let syl =
            |b: usize, i: usize, v: usize| t.bars[b].voices[0].beats[i].lyrics.get(v).cloned();
        // Rests are skipped; tied notes take a syllable, as in Guitar Pro.
        assert_eq!(syl(0, 0, 0).as_deref(), Some("Hel-"));
        assert_eq!(syl(0, 1, 0), None);
        assert_eq!(syl(0, 2, 0).as_deref(), Some("lo"));
        assert_eq!(syl(0, 3, 0).as_deref(), Some("there"));
        assert_eq!(syl(1, 0, 0).as_deref(), Some("my"));
        assert_eq!(syl(1, 0, 1).as_deref(), Some("Sec-"));
        let lines = lines_from_beats(t);
        assert_eq!(lines[0].text, "Hel-lo there my");
        assert_eq!((lines[1].start_bar, lines[1].text.as_str()), (1, "Sec-ond"));
    }
}
