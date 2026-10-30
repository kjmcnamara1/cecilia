//! The progress bar along the bottom of score panes: the song's sections as segments, the
//! play position (or the cursor's) and the time.

use muse_core::Score;

use crate::app::App;
use crate::layout::PaneId;

/// A section of the progress bar: where it starts and ends (fractions of the song), its first
/// bar, and its marker's letter and title (empty before the first marker).
#[derive(Debug, Clone, PartialEq)]
pub struct ProgressSection {
    pub start: f64,
    pub end: f64,
    pub bar: usize,
    pub letter: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub sections: Vec<ProgressSection>,
    /// The play position, or the cursor's when not playing, as a fraction of the song.
    pub at: f64,
    /// Seconds: the position and the whole song.
    pub now: f64,
    pub total: f64,
    /// Start of every bar and the end of the last, as fractions of the song.
    pub bars: Vec<f64>,
}

/// Seconds from the start of the score to each bar, and to the end of the last (in written
/// order, following tempo changes).
pub fn bar_times(score: &Score) -> Vec<f64> {
    let mut out = Vec::with_capacity(score.master_bars.len() + 1);
    let (mut t, mut bpm) = (0.0, score.tempo.max(1) as f64);
    out.push(0.0);
    for (b, mb) in score.master_bars.iter().enumerate() {
        let ticks = score.played_ticks(b) as f64;
        let mut changes: Vec<(f64, f64)> = mb
            .tempo_changes
            .iter()
            .map(|c| (c.tick as f64, c.bpm))
            .collect();
        changes.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut last = 0.0;
        for (tick, new) in changes {
            let tick = tick.min(ticks);
            t += (tick - last) / muse_core::TICKS_PER_QUARTER as f64 * 60.0 / bpm;
            last = tick;
            if new > 0.0 {
                bpm = new;
            }
        }
        t += (ticks - last) / muse_core::TICKS_PER_QUARTER as f64 * 60.0 / bpm;
        out.push(t);
    }
    out
}

/// `m:ss` (or `h:mm:ss`), zero-padded as `00:58`.
pub fn clock(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

impl App {
    /// The progress bar of a score pane. `play` is the frontend's play position (buffer, bar,
    /// tick, seconds played, length in seconds), used while that pane's file plays.
    pub fn progress(
        &self,
        pane: PaneId,
        play: Option<(usize, usize, u32, f64, f64)>,
    ) -> Option<Progress> {
        let p = self.panes.get(&pane)?;
        let score = &self.buffers.get(p.buffer)?.score;
        let n = score.master_bars.len();
        if n == 0 {
            return None;
        }
        let times = bar_times(score);
        let length = times[n].max(f64::EPSILON);
        let bars: Vec<f64> = times.iter().map(|t| t / length).collect();
        // Sections from each marker to the next (and from the start to the first).
        let mut sections: Vec<ProgressSection> = Vec::new();
        for (b, mb) in score.master_bars.iter().enumerate() {
            let marker = mb.marker.as_ref();
            if marker.is_none() && b > 0 {
                continue;
            }
            if let Some(last) = sections.last_mut() {
                last.end = bars[b];
            }
            sections.push(ProgressSection {
                start: bars[b],
                end: 1.0,
                bar: b,
                letter: marker.map_or(String::new(), |m| m.letter.trim().to_string()),
                title: marker.map_or(String::new(), |m| m.title.trim().to_string()),
            });
        }
        let at_bar = |bar: usize, tick: u32| {
            let bar = bar.min(n - 1);
            let ticks = score.played_ticks(bar).max(1) as f64;
            let frac = (tick as f64 / ticks).clamp(0.0, 1.0);
            bars[bar] + (bars[bar + 1] - bars[bar]) * frac
        };
        let playing = play.filter(|q| q.0 == p.buffer && (self.playing || self.paused));
        Some(match playing {
            Some((_, bar, tick, now, total)) => Progress {
                at: at_bar(bar, tick),
                now,
                total: if total > 0.0 { total } else { length },
                sections,
                bars,
            },
            None => {
                let tick = crate::app::cursor_tick(score, &p.cursor);
                let at = at_bar(p.cursor.bar, tick);
                Progress {
                    at,
                    now: at * length,
                    total: length,
                    sections,
                    bars,
                }
            }
        })
    }

    /// The bar at a fraction of a pane's song (a click on its progress bar).
    pub fn progress_bar_at(&self, pane: PaneId, frac: f64) -> Option<usize> {
        let p = self.progress(pane, None)?;
        let n = p.bars.len().saturating_sub(1);
        (0..n).rev().find(|&b| p.bars[b] <= frac).or(Some(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muse_core::*;

    #[test]
    fn sections_times_and_position() {
        let mut score = Score {
            tempo: 120,
            master_bars: vec![MasterBar::default(); 4],
            ..Score::default()
        };
        score.master_bars[1].marker = Some(Marker {
            title: "Verse".into(),
            color: Color::default(),
            letter: "A".into(),
        });
        score.master_bars[3]
            .tempo_changes
            .push(TempoChange { tick: 0, bpm: 60.0 });
        // Four 4/4 bars: 2 s each at 120, the last 4 s at 60.
        assert_eq!(bar_times(&score), vec![0.0, 2.0, 4.0, 6.0, 10.0]);
        let mut app = App::new(crate::Config::default(), None);
        let b = app.pane().buffer;
        app.buffers[b].score = score;
        let id = app.focused();
        app.pane_mut().cursor.bar = 2;
        let p = app.progress(id, None).unwrap();
        assert_eq!(p.sections.len(), 2);
        assert_eq!(
            (p.sections[1].letter.as_str(), p.sections[1].title.as_str()),
            ("A", "Verse")
        );
        assert!((p.sections[1].start - 0.2).abs() < 1e-9);
        assert!((p.at - 0.4).abs() < 1e-9 && (p.now - 4.0).abs() < 1e-9);
        assert_eq!(p.total, 10.0);
        assert_eq!(app.progress_bar_at(id, 0.45), Some(2));
        assert_eq!(clock(58.0), "00:58");
        assert_eq!(clock(288.0), "04:48");
    }
}
