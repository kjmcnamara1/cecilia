//! General MIDI drum kit: lanes for tablature-style display and entry keys.

/// Display lane of a drum note, top to bottom: cymbals, hi-hat, high toms, snare, low toms,
/// bass drum.
pub fn lane(key: u8) -> u8 {
    match key {
        35 | 36 => 5,
        41 | 43 | 45 => 4,
        37..=40 => 3,
        47 | 48 | 50 => 2,
        42 | 44 | 46 => 1,
        _ => 0,
    }
}

/// Short labels of the lanes.
pub const LANES: [&str; 6] = ["CY", "HH", "T1", "SN", "FT", "BD"];

/// Whether the instrument is a cymbal or hi-hat (drawn with an x).
pub fn is_cymbal(key: u8) -> bool {
    matches!(key, 42 | 44 | 46 | 49 | 51 | 52 | 53 | 55 | 57 | 59)
}

/// The instrument a letter enters on a drum track, as a MIDI key.
pub fn key_for_letter(c: char) -> Option<u8> {
    Some(match c {
        'k' => 36, // bass drum
        's' => 38, // snare
        'h' => 42, // closed hi-hat
        'o' => 46, // open hi-hat
        'p' => 44, // pedal hi-hat
        'c' => 49, // crash
        'd' => 51, // ride (r stays rest)
        't' => 50, // high tom
        'm' => 47, // mid tom
        'f' => 43, // floor tom
        _ => return None,
    })
}

/// Name of a General MIDI drum key.
pub fn name(key: u8) -> &'static str {
    match key {
        35 | 36 => "bass drum",
        37 => "side stick",
        38 | 40 => "snare",
        39 => "clap",
        41 | 43 => "floor tom",
        45 | 47 => "mid tom",
        48 | 50 => "high tom",
        42 => "closed hi-hat",
        44 => "pedal hi-hat",
        46 => "open hi-hat",
        49 | 57 => "crash",
        51 | 59 => "ride",
        53 => "ride bell",
        52 => "china",
        55 => "splash",
        _ => "percussion",
    }
}
