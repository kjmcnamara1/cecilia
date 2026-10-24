use serde::{Deserialize, Serialize};

use crate::TICKS_PER_QUARTER;

/// Base note value, from whole note to 128th.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DurationValue {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    SixtyFourth,
    HundredTwentyEighth,
}

impl DurationValue {
    pub const ALL: [DurationValue; 8] = [
        Self::Whole,
        Self::Half,
        Self::Quarter,
        Self::Eighth,
        Self::Sixteenth,
        Self::ThirtySecond,
        Self::SixtyFourth,
        Self::HundredTwentyEighth,
    ];

    /// The denominator of the note value (1 = whole, 4 = quarter, ...).
    pub fn denominator(self) -> u32 {
        1 << (self as u32)
    }

    /// Converts a denominator (1, 2, 4, ... 128) back to a value.
    pub fn from_denominator(d: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.denominator() == d)
    }

    pub fn ticks(self) -> u32 {
        TICKS_PER_QUARTER * 4 / self.denominator()
    }

    /// Next longer value (saturating at whole).
    pub fn longer(self) -> Self {
        Self::ALL[(self as usize).saturating_sub(1)]
    }

    /// Next shorter value (saturating at 128th).
    pub fn shorter(self) -> Self {
        Self::ALL[(self as usize + 1).min(Self::ALL.len() - 1)]
    }
}

/// `enters` notes in the time of `times` (a triplet is 3:2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tuplet {
    pub enters: u8,
    pub times: u8,
}

impl Tuplet {
    pub const TRIPLET: Tuplet = Tuplet {
        enters: 3,
        times: 2,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Duration {
    pub value: DurationValue,
    pub dots: u8,
    pub tuplet: Option<Tuplet>,
}

impl Default for Duration {
    fn default() -> Self {
        Self::new(DurationValue::Quarter)
    }
}

impl Duration {
    pub fn new(value: DurationValue) -> Self {
        Self {
            value,
            dots: 0,
            tuplet: None,
        }
    }

    pub fn ticks(&self) -> u32 {
        let base = self.value.ticks();
        let mut total = base;
        let mut add = base;
        for _ in 0..self.dots {
            add /= 2;
            total += add;
        }
        match self.tuplet {
            Some(t) if t.enters > 0 => total * t.times as u32 / t.enters as u32,
            _ => total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_lengths() {
        assert_eq!(Duration::new(DurationValue::Quarter).ticks(), 960);
        assert_eq!(Duration::new(DurationValue::Whole).ticks(), 3840);
        let dotted = Duration {
            dots: 1,
            ..Duration::new(DurationValue::Quarter)
        };
        assert_eq!(dotted.ticks(), 1440);
        let triplet = Duration {
            tuplet: Some(Tuplet::TRIPLET),
            ..Duration::new(DurationValue::Eighth)
        };
        assert_eq!(triplet.ticks(), 320);
    }

    #[test]
    fn denominators_round_trip() {
        for v in DurationValue::ALL {
            assert_eq!(DurationValue::from_denominator(v.denominator()), Some(v));
        }
        assert_eq!(DurationValue::Whole.longer(), DurationValue::Whole);
        assert_eq!(DurationValue::Quarter.shorter(), DurationValue::Eighth);
    }
}
