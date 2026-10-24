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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key {
    pub code: KeyCode,
    pub ctrl: bool,
    pub alt: bool,
}

impl Key {
    pub const fn new(code: KeyCode) -> Self {
        Self {
            code,
            ctrl: false,
            alt: false,
        }
    }

    pub const fn char(c: char) -> Self {
        Self::new(KeyCode::Char(c))
    }

    pub const fn ctrl(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            ctrl: true,
            alt: false,
        }
    }

    pub fn as_char(&self) -> Option<char> {
        match self.code {
            KeyCode::Char(c) if !self.ctrl && !self.alt => Some(c),
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
        out.push(parse_named(&name).ok_or_else(|| format!("unknown key <{name}> in '{s}'"))?);
    }
    if out.is_empty() {
        return Err("empty key sequence".into());
    }
    Ok(out)
}

fn parse_named(name: &str) -> Option<Key> {
    let lower = name.to_ascii_lowercase();
    if let Some((_, code)) = NAMED.iter().find(|(n, _)| *n == lower) {
        return Some(Key::new(*code));
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        return Some(Key::new(KeyCode::F(n)));
    }
    let (prefix, rest) = name.split_once('-')?;
    let mut key = if rest.chars().count() == 1 {
        Key::char(rest.chars().next()?)
    } else {
        parse_named(rest)?
    };
    match prefix.to_ascii_lowercase().as_str() {
        "c" => {
            key.ctrl = true;
            if let KeyCode::Char(c) = key.code {
                key.code = KeyCode::Char(c.to_ascii_lowercase());
            }
        }
        "a" | "m" => key.alt = true,
        "s" => {
            if let KeyCode::Char(c) = key.code {
                key.code = KeyCode::Char(c.to_ascii_uppercase());
            }
        }
        _ => return None,
    }
    Some(key)
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.code {
            KeyCode::Char(' ') => "Space".to_string(),
            KeyCode::Char('<') => "lt".to_string(),
            KeyCode::Char(c) if !self.ctrl && !self.alt => return write!(f, "{c}"),
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
        let prefix = match (self.ctrl, self.alt) {
            (true, true) => "C-A-",
            (true, false) => "C-",
            (false, true) => "A-",
            _ => "",
        };
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
}
