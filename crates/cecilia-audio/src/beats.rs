//! Beat detection for the sync assist: tempo by autocorrelation of the onset envelope, then the
//! beat phase that best lines up with onsets.

/// Frames per second of the onset envelope ([`crate::AudioClip::onsets`]).
pub const ONSET_RATE: f64 = 100.0;

/// Estimated beat period and phase (seconds), given an expected beat length near which to
/// prefer a solution.
pub fn estimate_beats(onsets: &[f32], expected_period: f64) -> Option<(f64, f64)> {
    if onsets.len() < 400 {
        return None;
    }
    let mean = onsets.iter().sum::<f32>() / onsets.len() as f32;
    let x: Vec<f32> = onsets.iter().map(|v| v - mean).collect();
    // Lags for 40..240 beats per minute.
    let (lo, hi) = ((0.25 * ONSET_RATE) as usize, (1.5 * ONSET_RATE) as usize);
    let mut best = (f64::MIN, 0usize);
    for lag in lo..=hi.min(x.len() / 2) {
        let score: f32 =
            x.iter().zip(&x[lag..]).map(|(a, b)| a * b).sum::<f32>() / (x.len() - lag) as f32;
        // Favour periods near the notated tempo (and its doubles and halves).
        let period = lag as f64 / ONSET_RATE;
        let ratio = (period / expected_period).log2();
        let nearest = ratio.round();
        let weight = (-(ratio - nearest).powi(2) * 8.0).exp() * (-(nearest.abs()) * 0.7).exp();
        let s = score as f64 * (0.3 + weight);
        if s > best.0 {
            best = (s, lag);
        }
    }
    let mut period = best.1 as f64 / ONSET_RATE;
    // Snap to the multiple closest to the expected beat.
    while period * 1.5 < expected_period {
        period *= 2.0;
    }
    while period > expected_period * 1.5 {
        period /= 2.0;
    }
    // Refine period and phase together: the grid that collects the most onset energy over the
    // whole recording (the lag search above is only accurate to one onset frame).
    let mut best = (f32::MIN, period, 0.0);
    for i in -60..=60 {
        let p = period * (1.0 + i as f64 * 0.0005);
        let step = p * ONSET_RATE;
        for ph in 0..(step * 2.0) as usize {
            let phase = ph as f64 * 0.5;
            let s = grid_energy(onsets, phase, step);
            if s > best.0 {
                best = (s, p, phase / ONSET_RATE);
            }
        }
    }
    Some((best.1, best.2))
}

/// Onset energy collected by a beat grid (in onset frames), tolerating one frame of jitter.
fn grid_energy(onsets: &[f32], phase: f64, step: f64) -> f32 {
    let mut sum = 0.0;
    let mut t = phase;
    while (t as usize + 1) < onsets.len() {
        let i = t.round() as usize;
        sum += onsets[i.saturating_sub(1)..(i + 2).min(onsets.len())]
            .iter()
            .copied()
            .fold(0.0, f32::max);
        t += step;
    }
    sum
}

/// The time of the strongest onset within `window` seconds of `t`, or `t` itself.
pub fn snap(onsets: &[f32], t: f64, window: f64) -> f64 {
    let a = ((t - window) * ONSET_RATE).max(0.0) as usize;
    let b = (((t + window) * ONSET_RATE) as usize).min(onsets.len());
    match (a..b).max_by(|&i, &j| onsets[i].total_cmp(&onsets[j])) {
        Some(i) if onsets[i] > 0.0 => i as f64 / ONSET_RATE,
        _ => t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_steady_beat() {
        // Clicks every 0.5 s (120 bpm) starting at 0.23 s, 20 s long.
        let mut onsets = vec![0.0f32; 2000];
        let mut t = 0.23;
        while t < 20.0 {
            onsets[(t * ONSET_RATE) as usize] = 1.0;
            t += 0.5;
        }
        let (period, phase) = estimate_beats(&onsets, 0.55).unwrap();
        assert!((period - 0.5).abs() < 0.011, "{period}");
        assert!((phase - 0.23).abs() < 0.02, "{phase}");
        assert!((snap(&onsets, 1.25, 0.06) - 1.23).abs() < 0.011);
    }
}
