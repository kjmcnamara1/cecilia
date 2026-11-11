//! winit key events to cecilia keys.

use cecilia_app::{Key, KeyCode};
use winit::event::KeyEvent;
use winit::keyboard::{Key as WKey, ModifiersState, NamedKey};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

pub fn convert(event: &KeyEvent, mods: ModifiersState) -> Option<Key> {
    let code = match &event.logical_key {
        WKey::Named(n) => match n {
            NamedKey::Escape => KeyCode::Esc,
            NamedKey::Enter => KeyCode::Enter,
            NamedKey::Tab => KeyCode::Tab,
            NamedKey::Backspace => KeyCode::Backspace,
            NamedKey::Delete => KeyCode::Delete,
            NamedKey::ArrowUp => KeyCode::Up,
            NamedKey::ArrowDown => KeyCode::Down,
            NamedKey::ArrowLeft => KeyCode::Left,
            NamedKey::ArrowRight => KeyCode::Right,
            NamedKey::Home => KeyCode::Home,
            NamedKey::End => KeyCode::End,
            NamedKey::PageUp => KeyCode::PageUp,
            NamedKey::PageDown => KeyCode::PageDown,
            NamedKey::Space => KeyCode::Char(' '),
            NamedKey::F1 => KeyCode::F(1),
            NamedKey::F2 => KeyCode::F(2),
            NamedKey::F3 => KeyCode::F(3),
            NamedKey::F4 => KeyCode::F(4),
            NamedKey::F5 => KeyCode::F(5),
            NamedKey::F6 => KeyCode::F(6),
            NamedKey::F7 => KeyCode::F(7),
            NamedKey::F8 => KeyCode::F(8),
            NamedKey::F9 => KeyCode::F(9),
            NamedKey::F10 => KeyCode::F(10),
            NamedKey::F11 => KeyCode::F(11),
            NamedKey::F12 => KeyCode::F(12),
            _ => return None,
        },
        WKey::Character(s) => {
            let produced = s.chars().next()?;
            // With Ctrl/Alt/Super held, use the unmodified character (Ctrl-W gives 'w', not a
            // control code; Ctrl-Shift-Q gives 'q' with the shift flag set).
            if mods.control_key() || mods.alt_key() || mods.super_key() {
                let base = match event.key_without_modifiers() {
                    WKey::Character(base) => base.chars().next()?,
                    _ => produced,
                };
                if is_altgr_text(cfg!(windows), mods, produced, base) {
                    // Windows reports AltGr as Ctrl+Alt: the character it typed is the key.
                    return Some(with_modifiers(
                        KeyCode::Char(produced),
                        mods & !(ModifiersState::CONTROL | ModifiersState::ALT),
                    ));
                }
                KeyCode::Char(base)
            } else {
                KeyCode::Char(produced)
            }
        }
        _ => return None,
    };
    Some(with_modifiers(code, mods))
}

/// Whether a key press is AltGr typing a character (Windows reports AltGr as Ctrl+Alt held
/// together): both are down and the layout produced a printable character other than the
/// key's own.
fn is_altgr_text(windows: bool, mods: ModifiersState, produced: char, base: char) -> bool {
    windows && mods.control_key() && mods.alt_key() && produced != base && !produced.is_control()
}

/// The key for `code` with the modifier state (Shift folded per `Key::with_mods`).
fn with_modifiers(code: KeyCode, mods: ModifiersState) -> Key {
    Key::with_mods(
        code,
        mods.control_key(),
        mods.alt_key(),
        mods.shift_key(),
        mods.super_key(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn altgr_text_only_on_windows_with_a_produced_character() {
        let both = ModifiersState::CONTROL | ModifiersState::ALT;
        assert!(is_altgr_text(true, both, '@', 'q'));
        assert!(!is_altgr_text(false, both, '@', 'q'));
        assert!(!is_altgr_text(true, ModifiersState::ALT, '@', 'q'));
        // A real Ctrl+Alt chord: the key's own character, or a control code.
        assert!(!is_altgr_text(true, both, 'q', 'q'));
        assert!(!is_altgr_text(true, both, '\u{11}', 'q'));
    }

    #[test]
    fn modifiers_become_flags() {
        let ctrl_shift = ModifiersState::CONTROL | ModifiersState::SHIFT;
        let k = with_modifiers(KeyCode::Char('q'), ctrl_shift);
        assert_eq!((k.ctrl, k.shift), (true, true));
        assert_ne!(
            k,
            with_modifiers(KeyCode::Char('q'), ModifiersState::CONTROL)
        );
        // Shift folds into a plain character, and Shift+Tab is BackTab.
        assert_eq!(
            with_modifiers(KeyCode::Char('Q'), ModifiersState::SHIFT),
            Key::char('Q')
        );
        assert_eq!(
            with_modifiers(KeyCode::Tab, ModifiersState::SHIFT),
            Key::new(KeyCode::BackTab)
        );
        let k = with_modifiers(KeyCode::Char('q'), ModifiersState::SUPER);
        assert!(k.sup && !k.shift && k.as_char().is_none());
        assert!(with_modifiers(KeyCode::F(5), ModifiersState::SHIFT).shift);
    }
}
