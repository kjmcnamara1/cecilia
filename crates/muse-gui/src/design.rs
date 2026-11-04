//! Design mode in the page layout: the handles drawn over the score (bar lines, line ends and
//! beats of the cursor's bar), finding the one under the mouse and turning a drag into new
//! design overrides. The geometry is in page points, so it is all testable without a window.

use muse_core::design::DesignOverrides;
use muse_engrave::display::BeatRef;
use muse_engrave::{Layout, SystemInfo};

/// Something a drag can grab.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Handle {
    /// The bar line after bar `k` of system `system` (not the line's last).
    BarLine { system: usize, k: usize },
    /// The tab past the end of a system: drag it to change the bars on the line.
    LineEnd { system: usize },
    /// A beat of the cursor's bar.
    Beat(BeatRef),
}

/// Where a line-end tab sits right of the system's end, and its size.
pub const END_TAB: (f64, f64, f64) = (3.0, 9.0, 18.0);
/// Radius of a beat's handle and its distance under the staves.
pub const BEAT_R: f64 = 3.0;
pub const BEAT_GAP: f64 = 5.0;

/// The line-end tab of a system: left, right, top, bottom.
pub fn end_tab(sys: &SystemInfo) -> (f64, f64, f64, f64) {
    let mid = (sys.top + sys.bottom) / 2.0;
    (
        sys.x1 + END_TAB.0,
        sys.x1 + END_TAB.1,
        mid - END_TAB.2 / 2.0,
        mid + END_TAB.2 / 2.0,
    )
}

/// Where a beat's handle is drawn: x and y.
pub fn beat_handle(layout: &Layout, beat: &BeatRef) -> Option<(usize, f64, f64)> {
    let p = layout.beats.get(beat)?;
    Some((p.page, p.head, p.bottom + BEAT_GAP))
}

/// The handle at a point of a page (`tol` points of slack), the most specific first: a beat,
/// then the line end, then a bar line. `bar` is the cursor's `(track, bar)`.
pub fn handle_at(
    layout: &Layout,
    cursor: (usize, usize),
    page: usize,
    (x, y): (f64, f64),
    tol: f64,
) -> Option<Handle> {
    let mut beats: Vec<&BeatRef> = layout
        .beats
        .keys()
        .filter(|b| (b.track, b.bar) == cursor)
        .collect();
    beats.sort_by_key(|b| (b.voice, b.beat));
    for b in beats {
        if let Some((p, bx, by)) = beat_handle(layout, b) {
            if p == page && (x - bx).abs() <= BEAT_R + tol && (y - by).abs() <= BEAT_R + tol {
                return Some(Handle::Beat(*b));
            }
        }
    }
    for (i, sys) in layout
        .systems
        .iter()
        .enumerate()
        .filter(|(_, s)| s.page == page)
    {
        let (l, r, t, b) = end_tab(sys);
        if x >= l - tol && x <= r + tol && y >= t - tol && y <= b + tol {
            return Some(Handle::LineEnd { system: i });
        }
    }
    for (i, sys) in layout
        .systems
        .iter()
        .enumerate()
        .filter(|(_, s)| s.page == page)
    {
        if y < sys.top - tol || y > sys.bottom + tol {
            continue;
        }
        for k in 0..sys.bars.saturating_sub(1) {
            let line = layout.bars[sys.first_bar + k].2;
            if (x - line).abs() <= tol {
                return Some(Handle::BarLine { system: i, k });
            }
        }
    }
    None
}

/// What a drag started on: the handle and the layout it began in.
#[derive(Debug, Clone, PartialEq)]
pub struct Grab {
    pub handle: Handle,
    /// The press, in page points.
    pub start: f64,
    /// The overrides at the press.
    pub base: DesignOverrides,
    pub system: Option<SystemInfo>,
    /// The system's bars' widths and natural widths at the press.
    pub widths: Vec<f64>,
    pub natural: Vec<f64>,
    /// Natural widths of the bars after the system.
    pub following: Vec<f64>,
    /// Bars in the score.
    pub bars: usize,
}

impl Grab {
    pub fn new(
        layout: &Layout,
        handle: Handle,
        start: f64,
        base: DesignOverrides,
        bars: usize,
    ) -> Self {
        let system = match handle {
            Handle::BarLine { system, .. } | Handle::LineEnd { system } => {
                layout.systems.get(system).cloned()
            }
            Handle::Beat(_) => None,
        };
        let range = system.as_ref().map(|s| s.range()).unwrap_or(0..0);
        let widths = range
            .clone()
            .map(|b| layout.bars[b].2 - layout.bars[b].1)
            .collect();
        let natural = range.clone().map(|b| layout.natural[b]).collect();
        let following = layout.natural.iter().skip(range.end).copied().collect();
        Self {
            handle,
            start,
            base,
            system,
            widths,
            natural,
            following,
            bars,
        }
    }

    /// The overrides with the drag carried `dx` points from where it began (`unit` is a
    /// tenth of a staff space in points).
    pub fn apply(&self, dx: f64, unit: f64) -> DesignOverrides {
        let mut design = self.base.clone();
        match (self.handle, &self.system) {
            (Handle::BarLine { k, .. }, Some(sys)) => {
                if let Some(w) = sys.drag_barline(&self.widths, &self.natural, k, dx) {
                    for i in [k, k + 1] {
                        design.set_width(sys.first_bar + i, w[i]);
                    }
                }
            }
            (Handle::LineEnd { .. }, Some(sys)) => {
                let n = sys.drag_end(&self.widths, &self.following, dx);
                if n != 0 {
                    let count = (sys.bars as i32 + n).clamp(1, self.bars as i32) as u32;
                    design.set_line_start(sys.first_bar, Some(Some(count)));
                }
            }
            (Handle::Beat(b), _) => {
                let tenths =
                    self.base.offset(b.track, b.bar, b.voice, b.beat) + (dx / unit).round() as i32;
                design.set_offset(b.track, b.bar, b.voice, b.beat, tenths.clamp(-200, 200));
            }
            _ => {}
        }
        design
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muse_core::*;
    use muse_engrave::{Options, engrave};

    fn score(bars: usize) -> Score {
        let beat = |fret| Beat {
            duration: Duration::new(DurationValue::Eighth),
            notes: vec![Note::new(1, fret)],
            ..Beat::default()
        };
        let bar = Bar {
            voices: vec![Voice {
                beats: (0..8).map(|i| beat(i % 5)).collect(),
            }],
        };
        Score {
            tempo: 100,
            master_bars: vec![MasterBar::default(); bars],
            tracks: vec![Track {
                kind: TrackKind::Fretted,
                strings: vec![64, 59, 55, 50, 45, 40],
                bars: vec![bar; bars],
                ..Track::default()
            }],
            ..Score::default()
        }
    }

    #[test]
    fn handles_are_found_where_they_are_drawn() {
        let score = score(8);
        let layout = engrave(&score, 0, &Options::default());
        let sys = &layout.systems[0];
        assert!(sys.bars >= 2);
        let mid = (sys.top + sys.bottom) / 2.0;
        let line = layout.bars[sys.first_bar].2;
        assert_eq!(
            handle_at(&layout, (0, 5), 0, (line + 1.0, mid), 3.0),
            Some(Handle::BarLine { system: 0, k: 0 })
        );
        let (l, r, ..) = end_tab(sys);
        assert_eq!(
            handle_at(&layout, (0, 5), 0, ((l + r) / 2.0, mid), 1.0),
            Some(Handle::LineEnd { system: 0 })
        );
        // The cursor bar's beats have handles under the staves; other bars' do not.
        let beat = BeatRef {
            track: 0,
            bar: 0,
            voice: 0,
            beat: 2,
        };
        let (page, bx, by) = beat_handle(&layout, &beat).unwrap();
        assert_eq!(
            handle_at(&layout, (0, 0), page, (bx, by), 1.0),
            Some(Handle::Beat(beat))
        );
        assert_eq!(handle_at(&layout, (0, 1), page, (bx, by), 1.0), None);
        // Away from every handle.
        assert_eq!(handle_at(&layout, (0, 5), 0, (line - 30.0, mid), 3.0), None);
    }

    #[test]
    fn a_drag_becomes_overrides() {
        let mut score = score(8);
        let layout = engrave(&score, 0, &Options::default());
        let bars = score.master_bars.len();
        let unit = Options::default().sp / 10.0;
        let sys = &layout.systems[0];
        // A bar line: absolute from the press, so a back-and-forth lands where it began.
        let grab = Grab::new(
            &layout,
            Handle::BarLine { system: 0, k: 0 },
            100.0,
            score.design.clone(),
            bars,
        );
        let wide = grab.apply(25.0, unit);
        assert!(wide.width(0) > 1000 && wide.width(1) < 1000);
        assert!(grab.apply(0.0, unit).is_empty());
        score.design = wide;
        let after = engrave(&score, 0, &Options::default());
        assert!(after.bars[0].2 - after.bars[0].1 > layout.bars[0].2 - layout.bars[0].1 + 24.0);
        // The line end: past half a bar takes it in.
        let grab = Grab::new(
            &layout,
            Handle::LineEnd { system: 0 },
            0.0,
            DesignOverrides::default(),
            bars,
        );
        assert!(grab.apply(10.0, unit).is_empty());
        let more = grab.apply(layout.natural[sys.bars], unit);
        assert_eq!(more.line_start(0).unwrap().count, Some(sys.bars as u32 + 1));
        let fewer = grab.apply(-1e6, unit);
        assert_eq!(fewer.line_start(0).unwrap().count, Some(1));
        // A beat: tenths of a staff space, on top of the offset it had.
        let beat = BeatRef {
            track: 0,
            bar: 0,
            voice: 0,
            beat: 1,
        };
        let mut base = DesignOverrides::default();
        base.set_offset(0, 0, 0, 1, 4);
        let grab = Grab::new(&layout, Handle::Beat(beat), 0.0, base, bars);
        assert_eq!(grab.apply(2.0 * unit * 3.0, unit).offset(0, 0, 0, 1), 10);
        assert_eq!(grab.apply(-1e6, unit).offset(0, 0, 0, 1), -200);
    }
}
