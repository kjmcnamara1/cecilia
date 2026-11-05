//! Bar lines of the text tab view, drawn as continuous vertical lines.
//!
//! A bar line is one or more columns, each spanning every string row of the staff. Plain
//! Unicode box drawing only (no Nerd Font needed); a column's glyph depends on the row so the
//! line reads as one stroke crossing the staff lines:
//!
//! | column | single row | top (both / left / right arm) | middle | bottom |
//! |--------|------------|-------------------------------|--------|--------|
//! | thin   | `│`        | `┬` `┐` `┌`                   | `┼` `┤` `├` | `┴` `┘` `└` |
//! | heavy  | `┃`        | `┰` `┒` `┎`                   | `╂` `┨` `┠` | `┸` `┚` `┖` |
//! | double | `║`        | `╥` `╖` `╓`                   | `╫` `╢` `╟` | `╨` `╜` `╙` |
//! | dashed | `┆` on every row                                              ||
//! | dots   | `•` on the two dot rows, `─` elsewhere                        ||
//!
//! An arm is a staff line meeting the column from that side. The strings stop flush at the
//! edges of a system: the first column of a line at the start of a system has no left arm, the
//! last column of the line closing a system no right arm. Between two bar columns there is no
//! string, so those sides have no arm either (a plain `│` / `┃` / `║` where neither side has
//! one); next to a dots column there is one (`arms`).
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

use cecilia_core::Score;

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

/// Whether staff lines meet column `i` of `cols` from the left and from the right. Between two
/// bar columns there is no string; next to a dots column and beyond the line there is, except
/// at the edge of a system (`left_edge` / `right_edge`), where the strings stop flush.
pub fn arms(cols: &[Col], i: usize, left_edge: bool, right_edge: bool) -> (bool, bool) {
    let left = if i == 0 {
        !left_edge
    } else {
        cols[i - 1] == Dots
    };
    let right = if i + 1 == cols.len() {
        !right_edge
    } else {
        cols[i + 1] == Dots
    };
    (left, right)
}

/// The glyph of column `col` on string row `row` of `n`, with staff lines arriving from the
/// `left` / `right` (see [`arms`]).
pub fn glyph(col: Col, row: usize, n: usize, left: bool, right: bool) -> char {
    // [top, middle, bottom] by arms: both, left only, right only, none.
    let pick = |g: [[char; 4]; 3], single: char| {
        if n <= 1 {
            return single;
        }
        let r = if row == 0 {
            0
        } else if row + 1 == n {
            2
        } else {
            1
        };
        g[r][match (left, right) {
            (true, true) => 0,
            (true, false) => 1,
            (false, true) => 2,
            (false, false) => 3,
        }]
    };
    match col {
        Thin => pick(
            [
                ['┬', '┐', '┌', '│'],
                ['┼', '┤', '├', '│'],
                ['┴', '┘', '└', '│'],
            ],
            '│',
        ),
        Heavy => pick(
            [
                ['┰', '┒', '┎', '┃'],
                ['╂', '┨', '┠', '┃'],
                ['┸', '┚', '┖', '┃'],
            ],
            '┃',
        ),
        Double => pick(
            [
                ['╥', '╖', '╓', '║'],
                ['╫', '╢', '╟', '║'],
                ['╨', '╜', '╙', '║'],
            ],
            '║',
        ),
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
        let col = |c, l, r| (0..6).map(|i| glyph(c, i, 6, l, r)).collect::<String>();
        assert_eq!(col(Thin, true, true), "┬┼┼┼┼┴");
        assert_eq!(col(Thin, false, true), "┌├├├├└");
        assert_eq!(col(Thin, true, false), "┐┤┤┤┤┘");
        assert_eq!(col(Thin, false, false), "││││││");
        assert_eq!(col(Heavy, true, true), "┰╂╂╂╂┸");
        assert_eq!(col(Heavy, false, true), "┎┠┠┠┠┖");
        assert_eq!(col(Heavy, true, false), "┒┨┨┨┨┚");
        assert_eq!(col(Double, true, true), "╥╫╫╫╫╨");
        assert_eq!(col(Double, false, true), "╓╟╟╟╟╙");
        assert_eq!(col(Double, true, false), "╖╢╢╢╢╜");
        assert_eq!(col(Dashed, true, true), "┆┆┆┆┆┆");
        assert_eq!(col(Dots, true, true), "─•──•─");
        assert_eq!(glyph(Thin, 0, 1, true, true), '│');
    }

    #[test]
    fn strings_stop_flush_at_system_edges() {
        // Left edge: only a right arm; right edge: only a left arm.
        assert_eq!(arms(&[Thin], 0, true, false), (false, true));
        assert_eq!(arms(&[Thin], 0, false, true), (true, false));
        assert_eq!(arms(&[Thin], 0, false, false), (true, true));
        // Final line at the right edge: no string between thin and heavy, none past heavy.
        assert_eq!(arms(&[Thin, Heavy], 0, false, true), (true, false));
        assert_eq!(arms(&[Thin, Heavy], 1, false, true), (false, false));
        // Repeat end at the right edge: strings reach the thin line from the dots.
        let end = [Dots, Thin, Heavy];
        assert_eq!(arms(&end, 1, false, true), (true, false));
        assert_eq!(arms(&end, 2, false, true), (false, false));
        // Repeat start at the left edge: the strings continue after the dots.
        let start = [Heavy, Thin, Dots];
        assert_eq!(arms(&start, 0, true, false), (false, false));
        assert_eq!(arms(&start, 1, true, false), (false, true));
        assert_eq!(arms(&start, 2, true, false), (false, true));
        // Mid-system repeat start: heavy line has strings on its left.
        assert_eq!(arms(&start, 0, false, false), (true, false));
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
