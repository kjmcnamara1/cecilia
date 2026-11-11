//! Colors for terminals without 24-bit support.
//!
//! The theme is RGB. Apple's Terminal.app only does the 256-color palette and misdraws RGB, so
//! there each drawn frame's RGB colors are replaced by the nearest palette entry.

use ratatui::buffer::Buffer;
use ratatui::style::Color;

/// Whether the terminal needs 256-color output. `CECILIA_COLORS=256` or `truecolor` decides;
/// otherwise only Terminal.app (`TERM_PROGRAM=Apple_Terminal`) is known not to do 24-bit color
/// (`COLORTERM` is unreliable: Windows Terminal does not set it, and ssh often drops it).
pub fn needs_256() -> bool {
    match std::env::var("CECILIA_COLORS").as_deref() {
        Ok("256") => true,
        Ok("truecolor") => false,
        _ => std::env::var("TERM_PROGRAM").as_deref() == Ok("Apple_Terminal"),
    }
}

/// The nearest entry of xterm's 256-color palette (6x6x6 cube or grayscale ramp).
pub fn to_256(r: u8, g: u8, b: u8) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let near = |v: u8| {
        (0..6)
            .min_by_key(|&i| (LEVELS[i] as i32 - v as i32).abs())
            .unwrap()
    };
    let dist = |p: (u8, u8, u8)| {
        let d = |a: u8, b: u8| (a as i32 - b as i32).pow(2);
        d(p.0, r) + d(p.1, g) + d(p.2, b)
    };
    let (ri, gi, bi) = (near(r), near(g), near(b));
    let cube = (LEVELS[ri], LEVELS[gi], LEVELS[bi]);
    let avg = (r as u16 + g as u16 + b as u16) / 3;
    let step = (avg.saturating_sub(8) as u8 / 10).min(23);
    let gray = 8 + 10 * step;
    if dist((gray, gray, gray)) < dist(cube) {
        232 + step
    } else {
        (16 + 36 * ri + 6 * gi + bi) as u8
    }
}

/// Replaces every RGB color in a drawn frame by its 256-color equivalent.
pub fn downgrade(buf: &mut Buffer) {
    let map = |c: Color| match c {
        Color::Rgb(r, g, b) => Color::Indexed(to_256(r, g, b)),
        c => c,
    };
    for cell in buf.content.iter_mut() {
        cell.fg = map(cell.fg);
        cell.bg = map(cell.bg);
        cell.underline_color = map(cell.underline_color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_corners() {
        assert_eq!(to_256(0, 0, 0), 16);
        assert_eq!(to_256(255, 255, 255), 231);
        assert_eq!(to_256(255, 0, 0), 196);
        assert_eq!(to_256(0, 255, 0), 46);
        assert_eq!(to_256(0, 0, 255), 21);
    }

    #[test]
    fn grays_use_the_ramp() {
        assert_eq!(to_256(128, 128, 128), 244);
        assert_eq!(to_256(30, 30, 30), 234);
    }

    #[test]
    fn frame_loses_all_rgb() {
        let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, 2, 1));
        buf[(0, 0)]
            .set_fg(Color::Rgb(255, 0, 0))
            .set_bg(Color::Rgb(0, 0, 255));
        buf[(1, 0)].set_fg(Color::Yellow);
        downgrade(&mut buf);
        assert_eq!(buf[(0, 0)].fg, Color::Indexed(196));
        assert_eq!(buf[(0, 0)].bg, Color::Indexed(21));
        assert_eq!(buf[(1, 0)].fg, Color::Yellow);
        assert!(buf.content.iter().all(|c| !matches!(c.fg, Color::Rgb(..))));
    }
}
