//! Manual overrides of the page layout ("design mode"): bar widths, beat offsets and line
//! breaks. Automatic layout is the default; an override exists only where the user set one.
//! Bars are master-bar indices, beats are `(track, bar, voice, beat)`.

use serde::{Deserialize, Serialize};

/// A bar's width weight of 1.0 (automatic), in permille.
pub const AUTO_WIDTH: u32 = 1000;
/// The narrowest and widest a bar can be weighted, in permille.
pub const MIN_WIDTH: u32 = 100;
pub const MAX_WIDTH: u32 = 5000;

/// A bar's share of its line, as a weight relative to its natural width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BarWidth {
    pub bar: usize,
    /// Permille of the automatic width ([`AUTO_WIDTH`] = automatic).
    pub width: u32,
}

/// A beat moved sideways from where the layout puts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeatOffset {
    pub track: usize,
    pub bar: usize,
    pub voice: usize,
    pub beat: usize,
    /// Tenths of a staff space, right positive.
    pub tenths: i32,
}

/// A line that starts at a bar by decree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineStart {
    pub bar: usize,
    /// Exactly this many bars on the line (squeezed if need be); `None`: as many as fit.
    pub count: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignOverrides {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bar_widths: Vec<BarWidth>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub beat_offsets: Vec<BeatOffset>,
    /// Sorted by bar.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line_starts: Vec<LineStart>,
}

impl DesignOverrides {
    pub fn is_empty(&self) -> bool {
        self.bar_widths.is_empty() && self.beat_offsets.is_empty() && self.line_starts.is_empty()
    }

    /// The bar's width weight in permille.
    pub fn width(&self, bar: usize) -> u32 {
        self.bar_widths
            .iter()
            .find(|w| w.bar == bar)
            .map_or(AUTO_WIDTH, |w| w.width)
    }

    /// Weights a bar (clamped); automatic removes the override.
    pub fn set_width(&mut self, bar: usize, width: u32) {
        let width = width.clamp(MIN_WIDTH, MAX_WIDTH);
        self.bar_widths.retain(|w| w.bar != bar);
        if width != AUTO_WIDTH {
            self.bar_widths.push(BarWidth { bar, width });
            self.bar_widths.sort_by_key(|w| w.bar);
        }
    }

    pub fn offset(&self, track: usize, bar: usize, voice: usize, beat: usize) -> i32 {
        self.beat_offsets
            .iter()
            .find(|o| (o.track, o.bar, o.voice, o.beat) == (track, bar, voice, beat))
            .map_or(0, |o| o.tenths)
    }

    /// Moves a beat to `tenths` of a staff space from its place; 0 removes the override.
    pub fn set_offset(&mut self, track: usize, bar: usize, voice: usize, beat: usize, tenths: i32) {
        self.beat_offsets
            .retain(|o| (o.track, o.bar, o.voice, o.beat) != (track, bar, voice, beat));
        if tenths != 0 {
            self.beat_offsets.push(BeatOffset {
                track,
                bar,
                voice,
                beat,
                tenths,
            });
            self.beat_offsets
                .sort_by_key(|o| (o.track, o.bar, o.voice, o.beat));
        }
    }

    pub fn line_start(&self, bar: usize) -> Option<LineStart> {
        self.line_starts.iter().find(|l| l.bar == bar).copied()
    }

    /// The first forced line start after `bar`.
    pub fn next_line_start(&self, bar: usize) -> Option<usize> {
        self.line_starts.iter().map(|l| l.bar).find(|&b| b > bar)
    }

    /// Makes a line start at `bar` (with the bars on it, if given); `None` removes it.
    pub fn set_line_start(&mut self, bar: usize, start: Option<Option<u32>>) {
        self.line_starts.retain(|l| l.bar != bar);
        if let Some(count) = start {
            self.line_starts.push(LineStart {
                bar,
                count: count.map(|c| c.max(1)),
            });
            self.line_starts.sort_by_key(|l| l.bar);
        }
    }

    /// Bars were inserted at `at`: later overrides move along.
    pub fn insert_bars(&mut self, at: usize, count: usize) {
        let shift = |b: &mut usize| {
            if *b >= at {
                *b += count;
            }
        };
        self.bar_widths.iter_mut().for_each(|w| shift(&mut w.bar));
        self.beat_offsets.iter_mut().for_each(|o| shift(&mut o.bar));
        self.line_starts.iter_mut().for_each(|l| shift(&mut l.bar));
    }

    /// Bars `from..=to` were deleted: their overrides go, later ones move up.
    pub fn delete_bars(&mut self, from: usize, to: usize) {
        let count = to + 1 - from;
        let keep = |b: usize| b < from || b > to;
        let shift = |b: &mut usize| {
            if *b > to {
                *b -= count;
            }
        };
        self.bar_widths.retain(|w| keep(w.bar));
        self.beat_offsets.retain(|o| keep(o.bar));
        self.line_starts.retain(|l| keep(l.bar));
        self.bar_widths.iter_mut().for_each(|w| shift(&mut w.bar));
        self.beat_offsets.iter_mut().for_each(|o| shift(&mut o.bar));
        self.line_starts.iter_mut().for_each(|l| shift(&mut l.bar));
    }

    /// Track `track` was deleted: its beat offsets go, later tracks move up.
    pub fn delete_track(&mut self, track: usize) {
        self.beat_offsets.retain(|o| o.track != track);
        for o in &mut self.beat_offsets {
            if o.track > track {
                o.track -= 1;
            }
        }
    }

    /// Drops overrides past the end of a score of `bars` bars.
    pub fn clamp_bars(&mut self, bars: usize) {
        self.bar_widths.retain(|w| w.bar < bars);
        self.beat_offsets.retain(|o| o.bar < bars);
        self.line_starts.retain(|l| l.bar < bars);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_and_clearing() {
        let mut o = DesignOverrides::default();
        assert!(o.is_empty());
        o.set_width(3, 1500);
        o.set_width(1, 50);
        assert_eq!(o.width(3), 1500);
        assert_eq!(o.width(1), MIN_WIDTH);
        assert_eq!(o.width(2), AUTO_WIDTH);
        o.set_width(3, AUTO_WIDTH);
        assert_eq!(o.bar_widths.len(), 1);
        o.set_offset(0, 2, 0, 1, -4);
        assert_eq!(o.offset(0, 2, 0, 1), -4);
        o.set_offset(0, 2, 0, 1, 0);
        assert_eq!(o.offset(0, 2, 0, 1), 0);
        o.set_line_start(4, Some(Some(0)));
        assert_eq!(o.line_start(4).unwrap().count, Some(1));
        o.set_line_start(4, Some(None));
        assert_eq!(o.line_start(4).unwrap().count, None);
        o.set_line_start(4, None);
        o.set_width(1, AUTO_WIDTH);
        assert!(o.is_empty());
    }

    #[test]
    fn bar_indices_shift() {
        let mut o = DesignOverrides::default();
        o.set_width(1, 1200);
        o.set_width(4, 800);
        o.set_offset(1, 4, 0, 0, 3);
        o.set_line_start(2, Some(Some(2)));
        o.insert_bars(2, 2);
        assert_eq!((o.width(1), o.width(6)), (1200, 800));
        assert_eq!(o.offset(1, 6, 0, 0), 3);
        assert_eq!(o.line_start(4).unwrap().count, Some(2));
        o.delete_bars(3, 4);
        assert_eq!(o.line_start(4), None);
        assert_eq!(o.width(4), 800);
        assert_eq!(o.offset(1, 4, 0, 0), 3);
        // Deleting the bar removes what was set on it.
        o.delete_bars(4, 4);
        assert!(o.bar_widths.iter().all(|w| w.bar != 4));
        assert!(o.beat_offsets.is_empty());
        o.set_offset(2, 0, 0, 0, 1);
        o.set_offset(1, 0, 0, 0, 1);
        o.delete_track(1);
        assert_eq!(o.offset(1, 0, 0, 0), 1);
        assert_eq!(o.beat_offsets.len(), 1);
    }

    #[test]
    fn json_round_trip_and_old_files() {
        let mut o = DesignOverrides::default();
        o.set_offset(0, 1, 0, 2, -3);
        o.set_width(2, 1300);
        o.set_line_start(3, Some(Some(2)));
        let json = serde_json::to_string(&o).unwrap();
        assert_eq!(serde_json::from_str::<DesignOverrides>(&json).unwrap(), o);
        let empty: DesignOverrides = serde_json::from_str("{}").unwrap();
        assert!(empty.is_empty());
    }
}
