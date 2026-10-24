//! winit key events to muse keys.

use muse_app::{Key, KeyCode};
use winit::event::KeyEvent;
use winit::keyboard::{Key as WKey, ModifiersState, NamedKey};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

pub fn convert(event: &KeyEvent, mods: ModifiersState) -> Option<Key> {
    let ctrl = mods.control_key();
    let alt = mods.alt_key();
    let code = match &event.logical_key {
        WKey::Named(n) => match n {
            NamedKey::Escape => KeyCode::Esc,
            NamedKey::Enter => KeyCode::Enter,
            NamedKey::Tab if mods.shift_key() => KeyCode::BackTab,
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
            // With Ctrl held, use the unmodified character (Ctrl-W gives 'w', not a control code).
            let c = if ctrl || alt {
                match event.key_without_modifiers() {
                    WKey::Character(base) => base.chars().next()?,
                    _ => s.chars().next()?,
                }
            } else {
                s.chars().next()?
            };
            KeyCode::Char(if ctrl { c.to_ascii_lowercase() } else { c })
        }
        _ => return None,
    };
    Some(Key { code, ctrl, alt })
}
