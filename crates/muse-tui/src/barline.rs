//! Bar lines of the text tab view, drawn as continuous vertical lines.
//!
//! A bar line is one or more columns, each spanning every string row of the staff. Plain
//! Unicode box drawing only (no Nerd Font needed); a column's glyph depends on the row so the
//! line reads as one stroke crossing the staff lines:
//!
//! | column | single row | top | middle | bottom | used for                       |
//! |--------|------------|-----|--------|--------|--------------------------------|
//! | thin   | `│`        | `┬` | `┼`    | `┴`    | normal bar line                |
//! | heavy  | `┃`        | `┰` | `╂`    | `┸`    | thick half of final / repeats  |
//! | double | `║`        | `╥` | `╫`    | `╨`    | double bar line                |
//! | dashed | `┆`        | `┆` | `┆`    | `┆`    | line closing a free-time bar   |
//! | dots   | `•` on the two dot rows, `─` elsewhere                           | repeat sign |
//!
//! Sequences of columns:
//!
//! | line         | columns          | width |
//! |--------------|------------------|-------|
//! | normal       | thin             | 1     |
//! | double       | double           | 1     |
//! | free time    | dashed           | 1     |
//! | final        | thin heavy       | 2     |
//! | repeat end   | dots thin heavy  | 3     |
//! | repeat start | heavy thin dots  | 3     |
//! | end + start  | dots thin heavy thin dots | 5 |
//!
//! Dots sit on rows 1 and `n-2` of an `n`-string staff (`dot_rows`).

use muse_core::Score;

/// One column of a bar line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Col {
    Thin,
    Heavy,
    Double,
    Dashed,
    Dots,
}

use Col::*;

/// How the bar line closing a bar looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Close {
    Normal,
    Dashed,
    Double,
    Final,
    RepeatEnd,
}

/// The line closing `bar`. Repeat end, final and double lines outrank the dashed free-time line.
pub fn closing(score: &Score, bar: usize) -> Close {
    let Some(mb) = score.master_bars.get(bar) else {
        return Close::Normal;
    };
    if mb.repeat_end.is_some() {
        Close::RepeatEnd
    } else if bar + 1 >= score.master_bars.len() {
        Close::Final
    } else if mb.double_bar {
        Close::Double
    } else if score.closing_barline_dashed(bar) {
        Close::Dashed
    } else {
        Close::Normal
    }
}

/// Columns of the line after the last bar of a system.
pub fn tail(score: &Score, bar: usize) -> &'static [Col] {
    match closing(score, bar) {
        Close::Normal => &[Thin],
        Close::Dashed => &[Dashed],
        Close::Double => &[Double],
        Close::Final => &[Thin, Heavy],
        Close::RepeatEnd => &[Dots, Thin, Heavy],
    }
}

/// Columns of the line at the start of `bar`: the line closing the bar before it joined with
/// the repeat sign opening this one. At the start of a system (`line_start`) the line that
/// closed the previous system's last bar is not repeated, except a dashed or double one.
pub fn boundary(score: &Score, bar: usize, line_start: bool) -> &'static [Col] {
    let prev = bar.checked_sub(1).map(|p| closing(score, p));
    let start = score.master_bars.get(bar).is_some_and(|m| m.repeat_start);
    match (prev, start) {
        (Some(Close::RepeatEnd), true) if !line_start => &[Dots, Thin, Heavy, Thin, Dots],
        (_, true) => &[Heavy, Thin, Dots],
        (Some(Close::RepeatEnd), false) if !line_start => &[Dots, Thin, Heavy],
        (Some(Close::Dashed), false) => &[Dashed],
        (Some(Close::Double), false) => &[Double],
        _ => &[Thin],
    }
}

/// Rows (of `n`) the repeat dots sit on: the two middle gaps of the staff.
pub fn dot_rows(n: usize) -> (usize, usize) {
    match n {
        0..=3 => (0, n.saturating_sub(1)),
        4 | 5 => (1, n - 2),
        _ => (n / 4, n - 1 - n / 4),
    }
}

/// The glyph of column `col` on string row `row` of `n`.
pub fn glyph(col: Col, row: usize, n: usize) -> char {
    let pos = if n <= 1 {
        1
    } else if row == 0 {
        0
    } else if row + 1 == n {
        2
    } else {
        3
    };
    // [top, single row, bottom, middle]
    let set = |g: [char; 4]| g[pos];
    match col {
        Thin => set(['┬', '│', '┴', '┼']),
        Heavy => set(['┰', '┃', '┸', '╂']),
        Double => set(['╥', '║', '╨', '╫']),
        Dashed => '┆',
        Dots => {
            let (a, b) = dot_rows(n);
            if row == a || row == b { '•' } else { '─' }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_join_into_one_stroke() {
        let col = |c| (0..6).map(|r| glyph(c, r, 6)).collect::<String>();
        assert_eq!(col(Thin), "┬┼┼┼┼┴");
        assert_eq!(col(Heavy), "┰╂╂╂╂┸");
        assert_eq!(col(Double), "╥╫╫╫╫╨");
        assert_eq!(col(Dashed), "┆┆┆┆┆┆");
        assert_eq!(col(Dots), "─•──•─");
        assert_eq!(glyph(Thin, 0, 1), '│');
    }

    #[test]
    fn dots_sit_on_the_middle_gaps() {
        assert_eq!(dot_rows(4), (1, 2));
        assert_eq!(dot_rows(5), (1, 3));
        assert_eq!(dot_rows(6), (1, 4));
        assert_eq!(dot_rows(8), (2, 5));
        assert_eq!(dot_rows(1), (0, 0));
    }
}
