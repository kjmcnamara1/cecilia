//! GABC, the chant notation of the Gregorio project: a header (`name: …;` lines) then `%%` and
//! the body, where each syllable is written `text(notes)`.
//!
//! Notes keep their modifiers as written, and anything not understood is kept verbatim, so a
//! file read and written again keeps its meaning (and usually its exact text).

use cecilia_core::chant::{
    Chant, ChantClef, ChantNote, NeumeItem, Syllable, letter_position, retranscribe,
};
use cecilia_core::{MidiSettings, Score, Track, TrackKind};

use crate::{Error, Result};

/// MIDI program for chant playback (Choir Aahs).
pub const CHANT_PROGRAM: u8 = 52;
/// Default chant tempo in quarter notes per minute (a note is an eighth).
pub const CHANT_TEMPO: u32 = 72;

/// Reads a GABC file into a score with one chant track.
pub fn read_gabc(bytes: &[u8]) -> Result<Score> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_start_matches('\u{feff}');
    let chant = parse(text)?;
    Ok(chant_score(chant))
}

/// A score holding one chant track, its bars transcribed from the chant.
pub fn chant_score(chant: Chant) -> Score {
    let mut score = Score {
        tempo: CHANT_TEMPO,
        ..Score::default()
    };
    score.meta.title = chant.header("name").unwrap_or_default().to_string();
    score.meta.subtitle = chant.header("office-part").unwrap_or_default().to_string();
    score.meta.tab_author = chant.header("transcriber").unwrap_or_default().to_string();
    score.tracks.push(Track {
        name: "Chant".into(),
        kind: TrackKind::Chant,
        midi: MidiSettings {
            program: CHANT_PROGRAM,
            ..MidiSettings::default()
        },
        chant: Some(Box::new(chant)),
        ..Track::default()
    });
    retranscribe(&mut score, 0);
    score
}

/// True when bytes look like GABC (a `%%` line after `key: value;` headers).
pub fn is_gabc(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    head.lines().any(|l| l.trim() == "%%")
        && head
            .lines()
            .take_while(|l| l.trim() != "%%")
            .any(|l| l.contains(':') && l.trim_end().ends_with(';'))
}

/// Parses GABC text.
pub fn parse(text: &str) -> Result<Chant> {
    let mut chant = Chant::default();
    let (header, body) = split_header(text);
    let mut key: Option<String> = None;
    let mut value = String::new();
    for line in header.lines() {
        if line.starts_with(['%', '#']) {
            continue;
        }
        if key.is_none() {
            let Some((k, v)) = line.split_once(':') else {
                continue;
            };
            key = Some(k.trim().to_string());
            value = v.trim_start().to_string();
        } else {
            value.push('\n');
            value.push_str(line);
        }
        // A value ends with `;` (or `;;`) at the end of a line; it may span lines.
        let trimmed = value.trim_end();
        if let Some(v) = trimmed.strip_suffix(';') {
            let v = v.strip_suffix(';').unwrap_or(v).to_string();
            if let Some(k) = key.take() {
                chant.headers.push((k, v));
            }
            value.clear();
        }
    }
    if let Some(n) = chant
        .header("staff-lines")
        .and_then(|v| v.parse::<u8>().ok())
    {
        chant.staff_lines = n.clamp(2, 5);
    }
    let nabc_lines = chant
        .header("nabc-lines")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1usize)
        .max(1);
    chant.syllables = parse_body(body, nabc_lines)?;
    Ok(chant)
}

fn split_header(text: &str) -> (&str, &str) {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.trim() == "%%" {
            return (&text[..offset], &text[offset + line.len()..]);
        }
        offset += line.len();
    }
    // No header: the whole text is the body.
    ("", text)
}

/// Splits the body into syllables. `%` starts a comment except inside `<sp>`, `<v>` and
/// `<alt>` in text and inside `[…]` in notes; `$` escapes the next character.
fn parse_body(body: &str, nabc_lines: usize) -> Result<Vec<Syllable>> {
    let c: Vec<char> = body.chars().collect();
    let starts = |i: usize, s: &str| {
        s.chars()
            .enumerate()
            .all(|(k, ch)| c.get(i + k) == Some(&ch))
    };
    let mut syllables = Vec::new();
    let mut text = String::new();
    let mut tag: Option<&str> = None;
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch == '$' && i + 1 < c.len() {
            text.push(ch);
            text.push(c[i + 1]);
            i += 2;
            continue;
        }
        if let Some(t) = tag {
            let close = format!("</{t}>");
            if starts(i, &close) {
                text.push_str(&close);
                i += close.len();
                tag = None;
            } else {
                text.push(ch);
                i += 1;
            }
            continue;
        }
        if ch == '%' {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if let Some(t) = ["sp", "v", "alt"]
            .into_iter()
            .find(|t| starts(i, &format!("<{t}>")))
        {
            text.push_str(&format!("<{t}>"));
            i += t.len() + 2;
            tag = Some(t);
            continue;
        }
        if ch != '(' {
            text.push(ch);
            i += 1;
            continue;
        }
        // Notes up to the closing parenthesis.
        let open = i;
        let mut notes = String::new();
        let mut depth = 0;
        i += 1;
        loop {
            let Some(&n) = c.get(i) else {
                return Err(Error::Parse {
                    offset: open,
                    location: None,
                    message: "unclosed '('".into(),
                });
            };
            match n {
                ')' if depth == 0 => break,
                '[' => depth += 1,
                ']' => depth = (depth - 1).max(0),
                '%' if depth == 0 => {
                    while i < c.len() && c[i] != '\n' {
                        i += 1;
                    }
                    continue;
                }
                _ => {}
            }
            notes.push(n);
            i += 1;
        }
        i += 1;
        let word_start = syllables.is_empty() || text.starts_with(char::is_whitespace);
        // Leading whitespace separates words; trailing whitespace is part of the text.
        let mut t = text.trim_start().to_string();
        if t.ends_with(['\n', '\r', '\t']) {
            t = format!("{} ", t.trim_end());
        }
        syllables.push(Syllable {
            text: t,
            word_start,
            items: parse_notes(&notes, nabc_lines),
        });
        text.clear();
    }
    Ok(syllables)
}

/// Parses the notation inside one pair of parentheses. Sections after `|` are adiastematic
/// (NABC) neumes, kept verbatim.
pub fn parse_notes(s: &str, nabc_lines: usize) -> Vec<NeumeItem> {
    let mut items = Vec::new();
    for (k, part) in s.split('|').enumerate() {
        if k % (nabc_lines + 1) != 0 {
            items.push(NeumeItem::Raw(format!("|{part}")));
        } else {
            if k > 0 {
                items.push(NeumeItem::Raw("|".into()));
            }
            items.extend(parse_gabc_notes(part));
        }
    }
    items
}

fn parse_gabc_notes(s: &str) -> Vec<NeumeItem> {
    let c: Vec<char> = s.chars().collect();
    let mut items: Vec<NeumeItem> = Vec::new();
    let mut i = 0;
    let at = |i: usize| c.get(i).copied().unwrap_or('\0');
    // Takes `[` … `]` (nested brackets allowed) starting at i.
    let bracket = |i: usize| -> (String, usize) {
        let mut depth = 0;
        let mut j = i;
        while j < c.len() {
            match c[j] {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        return (c[i..=j].iter().collect(), j + 1);
                    }
                }
                _ => {}
            }
            j += 1;
        }
        (c[i..].iter().collect(), c.len())
    };
    while i < c.len() {
        let ch = c[i];
        match ch {
            '\n' | '\r' | '\t' => i += 1,
            ' ' => {
                items.push(NeumeItem::Space(" ".into()));
                i += 1;
            }
            '/' | '!' => {
                let start = i;
                if ch == '!' {
                    i += 1;
                }
                while at(i) == '/' && i - start < 3 {
                    i += 1;
                }
                if c[start..i].ends_with(&['/']) {
                    match at(i) {
                        '0' | '!' => i += 1,
                        '[' => i = bracket(i).1,
                        _ => {}
                    }
                } else if ch == '!' && at(i) == ' ' {
                    i += 1;
                }
                items.push(NeumeItem::Space(c[start..i].iter().collect()));
            }
            '[' | ']' if at(i + 1) == ch => {
                items.push(NeumeItem::Raw(format!("{ch}{ch}")));
                i += 2;
            }
            ']' => {
                items.push(NeumeItem::Raw("]".into()));
                i += 1;
            }
            '<' if ["<nlba>", "</nlba>"]
                .iter()
                .any(|t| c[i..].iter().take(t.len()).copied().eq(t.chars())) =>
            {
                let len = if at(i + 1) == '/' { 7 } else { 6 };
                items.push(NeumeItem::Raw(c[i..i + len].iter().collect()));
                i += len;
            }
            '@' if at(i + 1) == '[' => {
                items.push(NeumeItem::Raw("@[".into()));
                i += 2;
            }
            '[' => {
                let (b, next) = bracket(i);
                match items.last_mut() {
                    Some(NeumeItem::Note(n)) => n.mods.push(b),
                    _ => items.push(NeumeItem::Raw(b)),
                }
                i = next;
            }
            '`' | '^' | ',' | ';' | ':' => {
                let start = i;
                i += 1;
                match ch {
                    ':' if matches!(at(i), ':' | '\'') => i += 1,
                    ',' | ';' if at(i).is_ascii_digit() => i += 1,
                    '`' | ',' | '^' if at(i) == '0' => i += 1,
                    _ => {}
                }
                while matches!(at(i), '?' | '0') && ch != ':' || (ch == ':' && at(i) == '?') {
                    i += 1;
                }
                items.push(NeumeItem::Bar(c[start..i].iter().collect()));
            }
            'z' | 'Z' => {
                if ch == 'z' && at(i + 1) == '0' {
                    items.push(NeumeItem::Custos(None));
                    i += 2;
                } else {
                    let end = if matches!(at(i + 1), '+' | '-') {
                        i + 2
                    } else {
                        i + 1
                    };
                    items.push(NeumeItem::LineBreak(c[i..end].iter().collect()));
                    i = end;
                }
            }
            'c' | 'f'
                if at(i + 1).is_ascii_digit()
                    || (at(i + 1) == 'b' && at(i + 2).is_ascii_digit()) =>
            {
                let len = if at(i + 1) == 'b' { 3 } else { 2 };
                let token: String = c[i..i + len].iter().collect();
                match ChantClef::parse(&token) {
                    Some(clef) => items.push(NeumeItem::Clef(clef)),
                    None => items.push(NeumeItem::Raw(token)),
                }
                i += len;
            }
            '@' if matches!(at(i + 1), 'c' | 'f')
                && (at(i + 2).is_ascii_digit() || at(i + 2) == 'b') =>
            {
                // Clef link (`c4@c3`): kept verbatim.
                let len = if at(i + 2) == 'b' { 4 } else { 3 };
                items.push(NeumeItem::Raw(
                    c[i..(i + len).min(c.len())].iter().collect(),
                ));
                i += len;
            }
            '-' | '@' if letter_position(at(i + 1)).is_some() => {
                let (note, next) = note_at(&c, i + 1);
                let mut note = note;
                note.initio_debilis = ch == '-';
                note.fused = ch == '@';
                items.push(NeumeItem::Note(note));
                i = next;
            }
            _ if letter_position(ch).is_some() => {
                let pitch = letter_position(ch).unwrap();
                let next = at(i + 1);
                if ch.is_ascii_lowercase() && matches!(next, 'x' | 'X' | '#' | 'y' | 'Y') {
                    let start = i + 1;
                    i += 2;
                    if next == '#' && at(i) == '#' {
                        i += 1;
                    }
                    if at(i) == '?' {
                        i += 1;
                    }
                    items.push(NeumeItem::Alteration {
                        pitch,
                        sign: c[start..i].iter().collect(),
                    });
                } else if ch.is_ascii_lowercase() && next == '+' {
                    items.push(NeumeItem::Custos(Some(pitch)));
                    i += 2;
                } else {
                    let (note, next) = note_at(&c, i);
                    items.push(NeumeItem::Note(note));
                    i = next;
                }
            }
            _ => {
                // `{` `}` `=` and anything unexpected.
                let token = ch.to_string();
                i += 1;
                match items.last_mut() {
                    Some(NeumeItem::Raw(r)) if !matches!(ch, '{' | '}') => r.push_str(&token),
                    _ => items.push(NeumeItem::Raw(token)),
                }
            }
        }
    }
    items
}

/// Reads a note (pitch letter then modifiers) starting at the pitch letter.
fn note_at(c: &[char], start: usize) -> (ChantNote, usize) {
    let at = |i: usize| c.get(i).copied().unwrap_or('\0');
    let letter = c[start];
    let mut note = ChantNote::new(letter_position(letter).unwrap_or(0));
    note.inclinatum = letter.is_ascii_uppercase();
    let mut i = start + 1;
    if note.inclinatum && matches!(at(i), '0' | '1' | '2') {
        note.mods.push(at(i).to_string());
        i += 1;
    }
    loop {
        let ch = at(i);
        let token_end = match ch {
            'v' | 'V' | 's' => {
                let mut j = i;
                while at(j) == ch && j - i < 3 {
                    j += 1;
                }
                if ch == 's' && j - i >= 2 && matches!(at(j), '<' | '>') {
                    j += 1;
                }
                j
            }
            '<' if matches!(at(i + 1), 'n' | '/') => break,
            'w' | 'W' | 'q' | '~' | '<' | '>' | 'R' => i + 1,
            'o' | 'O' => i + 1 + usize::from(matches!(at(i + 1), '0' | '1')),
            'r' => i + 1 + usize::from(at(i + 1).is_ascii_digit()),
            '.' => i + 1 + usize::from(matches!(at(i + 1), '.' | '0' | '1')),
            '_' => {
                let mut j = i + 1;
                while at(j).is_ascii_digit() {
                    j += 1;
                }
                j
            }
            '\'' => i + 1 + usize::from(matches!(at(i + 1), '0' | '1')),
            '[' => {
                let mut depth = 0;
                let mut j = i;
                while j < c.len() {
                    match c[j] {
                        '[' => depth += 1,
                        ']' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                (j + 1).min(c.len())
            }
            _ => break,
        };
        note.mods.push(c[i..token_end].iter().collect());
        i = token_end;
    }
    (note, i)
}

/// Writes the first chant track as GABC.
pub fn write_gabc(score: &Score) -> Result<Vec<u8>> {
    let chant = score
        .tracks
        .iter()
        .find_map(|t| t.chant.as_deref())
        .ok_or_else(|| Error::Unsupported("GABC needs a chant track".into()))?;
    Ok(write(chant, &score.meta.title).into_bytes())
}

/// GABC text of a chant; `title` replaces the `name` header when set.
pub fn write(chant: &Chant, title: &str) -> String {
    let mut out = String::new();
    let mut headers = chant.headers.clone();
    if !title.is_empty() {
        match headers.iter_mut().find(|(k, _)| k == "name") {
            Some(h) => h.1 = title.to_string(),
            None => headers.insert(0, ("name".into(), title.to_string())),
        }
    }
    for (k, v) in &headers {
        out.push_str(&format!("{k}: {v};\n"));
    }
    out.push_str("%%\n");
    let mut line = 0;
    for (i, s) in chant.syllables.iter().enumerate() {
        if s.word_start && i > 0 {
            if line > 72 {
                out.push('\n');
                line = 0;
            } else {
                out.push(' ');
                line += 1;
            }
        }
        let text = s.gabc();
        line += text.chars().count();
        out.push_str(&text);
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cecilia_core::chant::{Division, Liquescence, Shape};

    const SAMPLE: &str = "name: Pópulus Sion;\noffice-part: Introitus;\nmode: 7;\n%%\n\
        (c3) Pó(eh/hi)pu(h)lus(h) Si(hi)on,(hgh.) *(;) ec(hihi)ce(e.) Dó(e.f!gwhhi)mi(h)nus(h) \
        vé(hi)ni(ig//ih)et(h.) (,) ad(iv./hig) sal(fe~)ván(ghg)das(fg) gen(e_f_e_)tes(e.) :(:) \
        et(e) au(eh)dí(hhi)tam(i) fá(kjki)ci(i)et(i) Dó(ij)mi(ihi)nus(iv./hiHF.) (,) \
        <i>Ps.</i>(::) Gló(ixi)ri(h'_[oh:h])a(gsss)(::)\n";

    #[test]
    fn parses_headers_syllables_and_neumes() {
        let chant = parse(SAMPLE).unwrap();
        assert_eq!(chant.header("name"), Some("Pópulus Sion"));
        assert_eq!(chant.header("mode"), Some("7"));
        let s = &chant.syllables;
        assert_eq!(s[0].text, "");
        assert!(matches!(s[0].items[0], NeumeItem::Clef(c) if c.line == 3));
        assert_eq!(s[1].text, "Pó");
        assert!(s[1].word_start && !s[2].word_start);
        assert_eq!(s[1].gabc_notes(), "eh/hi");
        let dominus: Vec<_> = s[9].notes().collect();
        assert_eq!(dominus[0].mora(), 1);
        assert_eq!(dominus[2].shape(), Shape::Quilisma);
        let ad = s.iter().find(|x| x.text == "ad").unwrap();
        let notes: Vec<_> = ad.notes().collect();
        assert_eq!(notes[0].shape(), Shape::Virga);
        assert_eq!(notes[0].mora(), 1);
        let sal = s.iter().find(|x| x.text == "sal").unwrap();
        assert_eq!(
            sal.notes().last().unwrap().liquescence(),
            Liquescence::Deminutus
        );
        let nus = s.iter().rev().find(|x| x.text == "nus").unwrap();
        assert!(nus.notes().any(|n| n.shape() == Shape::Inclinatum));
        assert_eq!(
            s.iter().find(|x| x.text == "*").unwrap().division(),
            Some(Division::Minor)
        );
        let glo = s.iter().find(|x| x.text == "Gló").unwrap();
        assert!(matches!(&glo.items[0], NeumeItem::Alteration { pitch: 8, sign } if sign == "x"));
        let ri = s.iter().find(|x| x.text == "ri").unwrap();
        let n = ri.notes().next().unwrap();
        assert!(n.ictus() && n.episema());
        assert_eq!(n.mods, ["'", "_", "[oh:h]"]);
        let a = s.iter().rev().find(|x| x.text == "a").unwrap();
        assert_eq!(a.notes().next().unwrap().shape(), Shape::Stropha(3));
    }

    #[test]
    fn writes_what_it_reads() {
        let chant = parse(SAMPLE).unwrap();
        let text = write(&chant, "");
        let again = parse(&text).unwrap();
        assert_eq!(again, chant);
        let body = text.split("%%\n").nth(1).unwrap();
        assert!(body.starts_with("(c3) Pó(eh/hi)pu(h)lus(h) Si(hi)on,(hgh.) *(;)"));
    }

    #[test]
    fn score_from_gabc_plays_in_phrases() {
        let score = read_gabc(SAMPLE.as_bytes()).unwrap();
        assert_eq!(score.meta.title, "Pópulus Sion");
        let t = &score.tracks[0];
        assert_eq!(t.kind, TrackKind::Chant);
        assert_eq!(t.bars.len(), score.master_bars.len());
        assert!(score.master_bars.iter().all(|m| m.free_time));
        // c3: h is do (C5); e is sol (G4).
        let first = &t.bars[0].voices[0].beats[0];
        assert_eq!(first.notes[0].fret, 67);
        assert_eq!(first.lyrics, ["Pó-"]);
        // Every bar's time signature matches its contents.
        for (mb, bar) in score.master_bars.iter().zip(&t.bars) {
            let ticks: u32 = bar.voices[0].beats.iter().map(|b| b.ticks()).sum();
            assert_eq!(mb.ticks(), ticks);
        }
    }
}
