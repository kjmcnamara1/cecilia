//! Icons the frontends draw beside values: a kind of staff, a family of instruments.

use cecilia_core::{Staff, Track, TrackKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Treble,
    Bass,
    Grand,
    Standard,
    Neutral,
    ChantDo,
    ChantFa,
    Tab,
    TabRhythm,
    Slash,
    Chant,
    Guitar,
    Piano,
    Organ,
    Drums,
    Violin,
    Voice,
    Brass,
    Wind,
    Synth,
    Note,
}

impl Icon {
    /// The icon of a staff's notation: standard, tab, tab+, slash or chant.
    pub fn of_notation(staff: Staff) -> Icon {
        match staff {
            Staff::Standard(_) => Icon::Standard,
            Staff::Tab => Icon::Tab,
            Staff::TabRhythm => Icon::TabRhythm,
            Staff::Slash => Icon::Slash,
            Staff::Chant | Staff::ChantFa => Icon::Chant,
        }
    }

    /// The icon of a staff's clef; none for tab and slash.
    pub fn of_clef(staff: Staff) -> Option<Icon> {
        use cecilia_core::Clef;
        Some(match staff {
            Staff::Standard(Clef::Treble) => Icon::Treble,
            Staff::Standard(Clef::Bass) => Icon::Bass,
            Staff::Standard(Clef::Neutral) => Icon::Neutral,
            Staff::Standard(Clef::Grand { .. }) => Icon::Grand,
            Staff::Chant => Icon::ChantDo,
            Staff::ChantFa => Icon::ChantFa,
            Staff::Tab | Staff::TabRhythm | Staff::Slash => return None,
        })
    }

    /// The family of a General MIDI program (0-based): generic, a guitar for every guitar.
    pub fn of_program(program: usize) -> Icon {
        match program {
            0..=7 => Icon::Piano,
            8..=15 => Icon::Drums,
            16..=23 => Icon::Organ,
            24..=39 | 104..=107 => Icon::Guitar,
            47 | 112..=119 => Icon::Drums,
            40..=51 | 110 => Icon::Violin,
            52..=54 => Icon::Voice,
            55 => Icon::Note,
            56..=63 => Icon::Brass,
            64..=79 | 109 | 111 => Icon::Wind,
            80..=103 | 120..=127 => Icon::Synth,
            _ => Icon::Note,
        }
    }

    pub fn of_track(track: &Track) -> Icon {
        if track.kind == TrackKind::Drums {
            Icon::Drums
        } else {
            Icon::of_program(track.midi.program as usize)
        }
    }

    /// A character standing for the icon in a terminal.
    pub fn glyph(self) -> &'static str {
        match self {
            Icon::Treble => "𝄞",
            Icon::Bass => "𝄢",
            Icon::Grand => "𝄞",
            Icon::Standard => "𝄚",
            Icon::Neutral => "𝄥",
            Icon::ChantDo => "𝄡",
            Icon::ChantFa => "F",
            Icon::Tab => "▦",
            Icon::TabRhythm => "▤",
            Icon::Slash => "/",
            Icon::Chant => "◆",
            Icon::Guitar => "🎸",
            Icon::Piano => "🎹",
            Icon::Organ => "🎹",
            Icon::Drums => "🥁",
            Icon::Violin => "🎻",
            Icon::Voice => "🎤",
            Icon::Brass => "🎺",
            Icon::Wind => "🎷",
            Icon::Synth => "〰",
            Icon::Note => "♪",
        }
    }
}
