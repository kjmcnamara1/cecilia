//! Vim's motions and text objects over a text as characters, for the lyrics text area. Positions
//! are character indices; ranges run from the first character to one past the last.

/// A character's class for word motions: blank, line break, word (letters, digits, `_`) or
/// sign. With `big` (vim's WORD) words and signs are one class.
fn class(c: char, big: bool) -> u8 {
    match c {
        ' ' | '\t' => 0,
        '\n' => 1,
        _ if big || c.is_alphanumeric() || c == '_' => 2,
        _ => 3,
    }
}

/// The start of the line `i` is on, and its end (its line break, or the end of the text).
pub fn line_bounds(c: &[char], i: usize) -> (usize, usize) {
    let i = i.min(c.len());
    let start = c[..i].iter().rposition(|&x| x == '\n').map_or(0, |p| p + 1);
    let end = c[i..]
        .iter()
        .position(|&x| x == '\n')
        .map_or(c.len(), |p| i + p);
    (start, end)
}

/// `w` / `W`: the start of the next word (an empty line counts as one).
pub fn word_forward(c: &[char], i: usize, big: bool) -> usize {
    let n = c.len();
    if i >= n {
        return n;
    }
    let k = class(c[i], big);
    let mut j = i;
    if k >= 2 {
        while j < n && class(c[j], big) == k {
            j += 1;
        }
    } else if k == 1 {
        j += 1;
        // An empty line is a word of its own.
        if j < n && c[j] == '\n' {
            return j;
        }
    }
    while j < n && class(c[j], big) == 0 {
        j += 1;
    }
    // A line break followed by text: the text; one followed by another: the empty line.
    if j < n && c[j] == '\n' {
        let mut m = j + 1;
        while m < n && class(c[m], big) == 0 {
            m += 1;
        }
        return if m < n && c[m] == '\n' { j + 1 } else { m };
    }
    j
}

/// `e` / `E`: the last character of this word, or of the next one when already on its end.
pub fn word_end(c: &[char], i: usize, big: bool) -> usize {
    let n = c.len();
    if n == 0 {
        return 0;
    }
    let mut j = i + 1;
    while j < n && class(c[j], big) <= 1 {
        j += 1;
    }
    if j >= n {
        return n - 1;
    }
    let k = class(c[j], big);
    while j + 1 < n && class(c[j + 1], big) == k {
        j += 1;
    }
    j
}

/// `b` / `B`: the start of this word, or of the previous one when already on its start.
pub fn word_back(c: &[char], i: usize, big: bool) -> usize {
    let mut j = i.min(c.len());
    while j > 0 && class(c[j - 1], big) <= 1 {
        // An empty line stops the way back.
        if c[j - 1] == '\n' && (j == 1 || c[j - 2] == '\n') {
            return j - 1;
        }
        j -= 1;
    }
    if j == 0 {
        return 0;
    }
    let k = class(c[j - 1], big);
    while j > 0 && class(c[j - 1], big) == k {
        j -= 1;
    }
    j
}

/// Where Ctrl-Delete deletes to: past the blanks after `i`, then past the word (or signs) there.
pub fn word_end_after(c: &[char], i: usize) -> usize {
    let n = c.len();
    let mut j = i.min(n);
    if j < n && c[j] == '\n' {
        return j + 1;
    }
    while j < n && class(c[j], false) == 0 {
        j += 1;
    }
    if j < n && class(c[j], false) >= 2 {
        let k = class(c[j], false);
        while j < n && class(c[j], false) == k {
            j += 1;
        }
    }
    j
}

/// Where Ctrl-Backspace deletes back to: before the blanks, then the word (or signs) before `i`.
pub fn word_start_before(c: &[char], i: usize) -> usize {
    let mut j = i.min(c.len());
    if j > 0 && c[j - 1] == '\n' {
        return j - 1;
    }
    while j > 0 && class(c[j - 1], false) == 0 {
        j -= 1;
    }
    if j > 0 && class(c[j - 1], false) >= 2 {
        let k = class(c[j - 1], false);
        while j > 0 && class(c[j - 1], false) == k {
            j -= 1;
        }
    }
    j
}

/// `iw` / `aw` (`iW` / `aW` with `big`): the run of characters of one class at `i` (a word,
/// signs or blanks), within the line; `around` takes the blanks after it too (else before it).
pub fn word_object(c: &[char], i: usize, around: bool, big: bool) -> Option<(usize, usize)> {
    if c.is_empty() {
        return None;
    }
    let i = i.min(c.len() - 1);
    let k = class(c[i], big);
    if k == 1 {
        return None;
    }
    let run = |mut a: usize, mut b: usize, k: u8| {
        while a > 0 && class(c[a - 1], big) == k {
            a -= 1;
        }
        while b < c.len() && class(c[b], big) == k {
            b += 1;
        }
        (a, b)
    };
    let (a, b) = run(i, i + 1, k);
    if !around {
        return Some((a, b));
    }
    if k == 0 {
        // On blanks: the blanks and the word after them.
        let (_, z) = match c.get(b) {
            Some(&x) if class(x, big) >= 2 => run(b, b + 1, class(x, big)),
            _ => (a, b),
        };
        return Some((a, z));
    }
    let mut z = b;
    while z < c.len() && class(c[z], big) == 0 {
        z += 1;
    }
    if z > b {
        return Some((a, z));
    }
    let mut s = a;
    while s > 0 && class(c[s - 1], big) == 0 {
        s -= 1;
    }
    Some((s, b))
}

/// The lines of the text: (start, end) of each, the end its line break or the text's end.
fn lines(c: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut s = 0;
    for (i, &x) in c.iter().enumerate() {
        if x == '\n' {
            out.push((s, i));
            s = i + 1;
        }
    }
    out.push((s, c.len()));
    out
}

/// A range of whole lines, `first..=last`, with the line break after the last (if any).
pub fn line_range(c: &[char], first: usize, last: usize) -> (usize, usize) {
    let (a, _) = line_bounds(c, first);
    let (_, z) = line_bounds(c, last);
    (a, (z + 1).min(c.len()))
}

/// `ip` / `ap`: the paragraph at `i` (its lines with text, or the blank lines it is on);
/// `around` takes the blank lines after it too (else before it). A range of whole lines.
pub fn paragraph(c: &[char], i: usize, around: bool) -> Option<(usize, usize)> {
    let ls = lines(c);
    let blank = |l: (usize, usize)| c[l.0..l.1].iter().all(|x| x.is_whitespace());
    let at = ls.iter().position(|&(s, e)| i >= s && i <= e)?;
    let kind = blank(ls[at]);
    let (mut a, mut b) = (at, at);
    while a > 0 && blank(ls[a - 1]) == kind {
        a -= 1;
    }
    while b + 1 < ls.len() && blank(ls[b + 1]) == kind {
        b += 1;
    }
    if around {
        // Text: and the blank lines after it, or else those before it. Blank lines: and the
        // paragraph after them.
        let mut z = b;
        while z + 1 < ls.len() && blank(ls[z + 1]) != kind {
            z += 1;
        }
        if z > b {
            b = z;
        } else {
            while a > 0 && blank(ls[a - 1]) != kind {
                a -= 1;
            }
        }
    }
    Some((ls[a].0, (ls[b].1 + 1).min(c.len())))
}

/// `i(` / `a(` and the other pairs: the text inside the brackets around `i` (or the brackets
/// `i` is on), `around` with the brackets.
pub fn pair_object(
    c: &[char],
    i: usize,
    open: char,
    close: char,
    around: bool,
) -> Option<(usize, usize)> {
    if c.is_empty() {
        return None;
    }
    let i = i.min(c.len() - 1);
    // The opening bracket: the one `i` is on, else the nearest unclosed one before it.
    let o = if c[i] == open {
        i
    } else {
        let mut depth = 0;
        let mut j = if c[i] == close { i } else { i + 1 };
        loop {
            if j == 0 {
                return None;
            }
            j -= 1;
            if c[j] == close && j != i {
                depth += 1;
            } else if c[j] == open {
                if depth == 0 {
                    break j;
                }
                depth -= 1;
            }
        }
    };
    let mut depth = 0;
    let mut k = o + 1;
    let cl = loop {
        if k >= c.len() {
            return None;
        }
        if c[k] == open {
            depth += 1;
        } else if c[k] == close {
            if depth == 0 {
                break k;
            }
            depth -= 1;
        }
        k += 1;
    };
    Some(if around { (o, cl + 1) } else { (o + 1, cl) })
}

/// `i"` / `a"` and the other quotes: the quoted text on the line around `i` (or the first quote
/// after it), `around` with the quotes.
pub fn quote_object(c: &[char], i: usize, q: char, around: bool) -> Option<(usize, usize)> {
    let (s, e) = line_bounds(c, i);
    let at: Vec<usize> = (s..e).filter(|&k| c[k] == q).collect();
    let pair = at
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| (p[0], p[1]))
        .find(|&(a, b)| i <= b && (i >= a || a > i))?;
    Some(if around {
        (pair.0, pair.1 + 1)
    } else {
        (pair.0 + 1, pair.1)
    })
}

/// The brackets a text object character names (`b` is `(`, `B` is `{`), or the quote.
pub fn object_chars(ch: char) -> Option<(char, char)> {
    Some(match ch {
        '(' | ')' | 'b' => ('(', ')'),
        '[' | ']' => ('[', ']'),
        '{' | '}' | 'B' => ('{', '}'),
        '<' | '>' => ('<', '>'),
        '"' | '\'' | '`' => (ch, ch),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cs(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    fn sub(c: &[char], r: Option<(usize, usize)>) -> String {
        r.map_or("-".into(), |(a, b)| c[a..b].iter().collect())
    }

    #[test]
    fn word_motions_follow_vims_classes() {
        let c = cs("Ky-ri e  (f)\nlast");
        assert_eq!(word_forward(&c, 0, false), 2); // to the sign
        assert_eq!(word_forward(&c, 0, true), 6); // WORD: past "Ky-ri"
        assert_eq!(word_forward(&c, 6, false), 9);
        assert_eq!(word_forward(&c, 11, false), 13); // over the line break
        assert_eq!(word_end(&c, 0, false), 1);
        assert_eq!(word_end(&c, 0, true), 4);
        assert_eq!(word_back(&c, 13, false), 11);
        assert_eq!(word_back(&c, 6, false), 3);
        assert_eq!(word_back(&c, 6, true), 0);
        // An empty line is a stop both ways.
        let c = cs("a\n\nb");
        assert_eq!(word_forward(&c, 0, false), 2);
        assert_eq!(word_forward(&c, 2, false), 3);
        assert_eq!(word_back(&c, 3, false), 2);
        assert_eq!(word_back(&c, 2, false), 0);
    }

    #[test]
    fn ctrl_word_deletes_stop_at_words() {
        let c = cs("one two  three");
        assert_eq!(word_start_before(&c, 7), 4);
        assert_eq!(word_start_before(&c, 9), 4);
        assert_eq!(word_end_after(&c, 3), 7);
        assert_eq!(word_end_after(&c, 7), 14);
        assert_eq!(word_end_after(&c, 5), 7);
    }

    #[test]
    fn word_objects() {
        let c = cs("Glo-ri a  in");
        assert_eq!(sub(&c, word_object(&c, 1, false, false)), "Glo");
        assert_eq!(sub(&c, word_object(&c, 1, true, true)), "Glo-ri ");
        assert_eq!(sub(&c, word_object(&c, 7, true, false)), "a  ");
        // The last word takes the blanks before it.
        assert_eq!(sub(&c, word_object(&c, 11, true, false)), "  in");
        assert_eq!(sub(&c, word_object(&c, 8, false, false)), "  ");
    }

    #[test]
    fn paragraphs() {
        let c = cs("a\nb\n\nc\nd\n\n\ne");
        assert_eq!(sub(&c, paragraph(&c, 0, false)), "a\nb\n");
        assert_eq!(sub(&c, paragraph(&c, 0, true)), "a\nb\n\n");
        assert_eq!(sub(&c, paragraph(&c, 6, true)), "c\nd\n\n\n");
        assert_eq!(sub(&c, paragraph(&c, 4, false)), "\n");
        // The last paragraph takes the blank lines before it.
        assert_eq!(sub(&c, paragraph(&c, 12, true)), "\n\ne");
        assert_eq!(sub(&c, paragraph(&c, 12, false)), "e");
    }

    #[test]
    fn brackets_and_quotes() {
        let c = cs("Ky(f) ri(g(h)i) \"say\" it");
        assert_eq!(sub(&c, pair_object(&c, 3, '(', ')', false)), "f");
        assert_eq!(sub(&c, pair_object(&c, 2, '(', ')', true)), "(f)");
        assert_eq!(sub(&c, pair_object(&c, 13, '(', ')', false)), "g(h)i");
        assert_eq!(sub(&c, pair_object(&c, 11, '(', ')', false)), "h");
        assert_eq!(sub(&c, pair_object(&c, 0, '(', ')', false)), "-");
        assert_eq!(sub(&c, quote_object(&c, 18, '"', false)), "say");
        assert_eq!(sub(&c, quote_object(&c, 0, '"', true)), "\"say\"");
    }
}
