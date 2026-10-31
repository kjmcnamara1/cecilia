//! Key representation and vim-style key notation (`<C-w>`, `<Space>`, `gg`, `<leader>p`).

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KeyCode {
    Char(char),
    Esc,
    Enter,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

/// A key press. Build it with [`Key::with_mods`] (or the parser) so `shift` follows the folding
/// rule: Shift on a printable character without ctrl/alt/super is folded into the character
/// (`Q`, never `shift+q`), and Shift+Tab is `BackTab`. Shift stays a separate flag with
/// ctrl/alt/super (`<C-S-q>` is not `<C-q>`) and on named keys (`<S-F5>`, `<S-Up>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key {
    pub code: KeyCode,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The Super/Windows/Cmd key.
    pub sup: bool,
}

impl Key {
    pub const fn new(code: KeyCode) -> Self {
        Self {
            code,
            ctrl: false,
            alt: false,
            shift: false,
            sup: false,
        }
    }

    pub const fn char(c: char) -> Self {
        Self::new(KeyCode::Char(c))
    }

    pub const fn ctrl(c: char) -> Self {
        Self {
            ctrl: true,
            ..Self::char(c)
        }
    }

    /// A key with modifiers, normalized (see the type docs). All frontends build keys here.
    pub fn with_mods(code: KeyCode, ctrl: bool, alt: bool, shift: bool, sup: bool) -> Self {
        let mut key = Self {
            code,
            ctrl,
            alt,
            shift,
            sup,
        };
        let other = ctrl || alt || sup;
        match key.code {
            KeyCode::Tab if shift => {
                key.code = KeyCode::BackTab;
                key.shift = false;
            }
            KeyCode::BackTab => key.shift = false,
            // Shift on these is just a held key while typing.
            KeyCode::Esc | KeyCode::Enter | KeyCode::Backspace if !other => key.shift = false,
            KeyCode::Char(c) if !other => {
                if shift {
                    let mut up = c.to_uppercase();
                    if let (Some(u), None) = (up.next(), up.next()) {
                        key.code = KeyCode::Char(u);
                    }
                }
                key.shift = false;
            }
            KeyCode::Char(c) => {
                let mut low = c.to_lowercase();
                if let (Some(l), None) = (low.next(), low.next())
                    && l != c
                {
                    key.code = KeyCode::Char(l);
                    // Alt+J from a terminal that does not report Shift is Alt+Shift+j; Ctrl+Q and
                    // Ctrl+Shift+Q are told apart by the flag only.
                    key.shift |= !ctrl;
                }
            }
            _ => {}
        }
        key
    }

    pub fn as_char(&self) -> Option<char> {
        match self.code {
            KeyCode::Char(c) if !self.ctrl && !self.alt && !self.sup => Some(c),
            _ => None,
        }
    }
}

const NAMED: &[(&str, KeyCode)] = &[
    ("space", KeyCode::Char(' ')),
    ("esc", KeyCode::Esc),
    ("cr", KeyCode::Enter),
    ("enter", KeyCode::Enter),
    ("return", KeyCode::Enter),
    ("tab", KeyCode::Tab),
    ("s-tab", KeyCode::BackTab),
    ("bs", KeyCode::Backspace),
    ("del", KeyCode::Delete),
    ("up", KeyCode::Up),
    ("down", KeyCode::Down),
    ("left", KeyCode::Left),
    ("right", KeyCode::Right),
    ("home", KeyCode::Home),
    ("end", KeyCode::End),
    ("pageup", KeyCode::PageUp),
    ("pagedown", KeyCode::PageDown),
    ("lt", KeyCode::Char('<')),
    ("bar", KeyCode::Char('|')),
];

/// Parses key notation into a key sequence. `leader` is substituted for `<leader>`.
pub fn parse_keys(s: &str, leader: &[Key]) -> Result<Vec<Key>, String> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '<' {
            out.push(Key::char(c));
            continue;
        }
        let mut name = String::new();
        let mut closed = false;
        for c in chars.by_ref() {
            if c == '>' {
                closed = true;
                break;
            }
            name.push(c);
        }
        if !closed || name.is_empty() {
            // A literal '<'.
            out.push(Key::char('<'));
            out.extend(name.chars().map(Key::char));
            continue;
        }
        if name.eq_ignore_ascii_case("leader") {
            out.extend_from_slice(leader);
            continue;
        }
        out.push(parse_named(&name).map_err(|e| format!("{e} in '{s}'"))?);
    }
    if out.is_empty() {
        return Err("empty key sequence".into());
    }
    Ok(out)
}

fn parse_named(name: &str) -> Result<Key, String> {
    let (mut ctrl, mut alt, mut shift, mut sup) = (false, false, false, false);
    let mut rest = name;
    // Modifier prefixes; "s-tab" is a name of its own and ends the loop.
    while !NAMED.iter().any(|(n, _)| rest.eq_ignore_ascii_case(n)) {
        let Some((prefix, r)) = rest.split_once('-').filter(|(_, r)| !r.is_empty()) else {
            break;
        };
        match prefix.to_ascii_lowercase().as_str() {
            "c" | "ctrl" => ctrl = true,
            "a" | "m" | "alt" | "meta" => alt = true,
            "s" | "shift" => shift = true,
            "d" | "super" | "win" | "cmd" => sup = true,
            p => return Err(format!("unknown modifier '{p}-' in <{name}>")),
        }
        rest = r;
    }
    let lower_rest = rest.to_ascii_lowercase();
    let code = if let Some((_, code)) = NAMED.iter().find(|(n, _)| *n == lower_rest) {
        *code
    } else if let Some(n) = lower_rest
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
    {
        KeyCode::F(n)
    } else if rest.chars().count() == 1 {
        KeyCode::Char(rest.chars().next().unwrap_or('-'))
    } else {
        return Err(format!("unknown key <{name}>"));
    };
    let code = match code {
        // <C-W> is <C-w>; Shift is spelled <C-S-w>.
        KeyCode::Char(c) if ctrl => KeyCode::Char(c.to_ascii_lowercase()),
        other => other,
    };
    Ok(Key::with_mods(code, ctrl, alt, shift, sup))
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.code {
            KeyCode::Char(' ') => "Space".to_string(),
            KeyCode::Char('<') => "lt".to_string(),
            KeyCode::Char(c) if !self.ctrl && !self.alt && !self.sup => return write!(f, "{c}"),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::F(n) => format!("F{n}"),
            KeyCode::Esc => "Esc".into(),
            KeyCode::Enter => "CR".into(),
            KeyCode::Tab => "Tab".into(),
            KeyCode::BackTab => "S-Tab".into(),
            KeyCode::Backspace => "BS".into(),
            KeyCode::Delete => "Del".into(),
            KeyCode::Up => "Up".into(),
            KeyCode::Down => "Down".into(),
            KeyCode::Left => "Left".into(),
            KeyCode::Right => "Right".into(),
            KeyCode::Home => "Home".into(),
            KeyCode::End => "End".into(),
            KeyCode::PageUp => "PageUp".into(),
            KeyCode::PageDown => "PageDown".into(),
        };
        let mut prefix = String::new();
        for (on, p) in [
            (self.ctrl, "C-"),
            (self.alt, "A-"),
            (self.shift, "S-"),
            (self.sup, "D-"),
        ] {
            if on {
                prefix.push_str(p);
            }
        }
        write!(f, "<{prefix}{name}>")
    }
}

pub fn format_keys(keys: &[Key]) -> String {
    keys.iter().map(Key::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_notation() {
        let leader = [Key::char(' ')];
        assert_eq!(
            parse_keys("gg", &leader).unwrap(),
            vec![Key::char('g'), Key::char('g')]
        );
        assert_eq!(
            parse_keys("<C-w>v", &leader).unwrap(),
            vec![Key::ctrl('w'), Key::char('v')]
        );
        assert_eq!(
            parse_keys("<leader>p", &leader).unwrap(),
            vec![Key::char(' '), Key::char('p')]
        );
        assert_eq!(parse_keys("<C-W>", &leader).unwrap(), vec![Key::ctrl('w')]);
        assert_eq!(
            parse_keys("<Esc>", &leader).unwrap(),
            vec![Key::new(KeyCode::Esc)]
        );
        assert!(parse_keys("<Bogus>", &leader).is_err());
    }

    #[test]
    fn formats_round_trip() {
        let keys = parse_keys("<C-d><Space>x<CR>", &[]).unwrap();
        assert_eq!(format_keys(&keys), "<C-d><Space>x<CR>");
    }

    fn rt(s: &str) -> Key {
        let k = parse_keys(s, &[]).unwrap();
        assert_eq!(k.len(), 1, "{s}");
        k[0]
    }

    #[test]
    fn display_round_trips_every_modifier_combination() {
        for bits in 0..16u8 {
            let (c, a, s, d) = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0, bits & 8 != 0);
            for code in [
                KeyCode::Char('q'),
                KeyCode::Char('7'),
                KeyCode::F(5),
                KeyCode::Up,
            ] {
                let k = Key::with_mods(code, c, a, s, d);
                assert_eq!(parse_keys(&k.to_string(), &[]).unwrap(), vec![k], "{k}");
            }
        }
    }

    #[test]
    fn shift_folds_into_characters_only() {
        assert_eq!(rt("<S-q>"), Key::char('Q'));
        assert_eq!(rt("Q"), rt("<S-q>"));
        assert_eq!(rt("<s-Q>"), Key::char('Q'));
        assert_eq!(rt("<S-Tab>"), Key::new(KeyCode::BackTab));
        assert_eq!(rt("<C-S-Tab>").code, KeyCode::BackTab);
        assert!(rt("<C-S-Tab>").ctrl);
        assert_eq!(format_keys(&[rt("<C-S-Tab>")]), "<C-S-Tab>");
        assert!(rt("<S-F5>").shift && rt("<S-Up>").shift);
        assert_eq!(rt("<S-Esc>"), Key::new(KeyCode::Esc));
        assert_ne!(rt("<C-q>"), rt("<C-S-q>"));
        assert_eq!(rt("<C-W>"), Key::ctrl('w'));
        assert_eq!(rt("<A-J>"), rt("<A-S-j>"));
        assert_eq!(rt("<C-q>").to_string(), "<C-q>");
        assert_eq!(rt("<C-A-S-q>").to_string(), "<C-A-S-q>");
        assert_eq!(
            rt("<A-j>"),
            Key {
                alt: true,
                ..Key::char('j')
            }
        );
    }

    #[test]
    fn super_aliases() {
        for n in ["<D-q>", "<Super-q>", "<Win-q>", "<cmd-q>"] {
            let k = rt(n);
            assert!(k.sup && !k.shift && !k.ctrl, "{n}");
            assert_eq!(k.to_string(), "<D-q>");
        }
        assert!(rt("<C-D-S-q>").sup);
    }

    #[test]
    fn unknown_modifier_names_the_key() {
        let e = parse_keys("<X-q>", &[]).unwrap_err();
        assert!(e.contains("<X-q>"), "{e}");
    }
}
