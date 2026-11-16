//! Which key the player meant.
//!
//! The game binds `KeyCode`s, which winit reports by where a key sits on the
//! board. That's right for letters (WASD stays under the left hand on any
//! layout) but wrong for keys the player has remapped in the OS: with Esc and
//! Caps Lock swapped, the key they press as Esc sits where Caps Lock is, so
//! its physical code is `CapsLock` and the game never saw an Esc. Function
//! keys like these (Esc, Enter, Tab, arrows, modifiers…) go by what the OS
//! says the key is instead; printable keys keep their place.

use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

/// The `KeyCode` the game should act on for a key event.
pub fn code(physical: PhysicalKey, logical: &Key) -> Option<KeyCode> {
    let at = match physical {
        PhysicalKey::Code(code) => Some(code),
        PhysicalKey::Unidentified(_) => None,
    };
    if let Key::Named(named) = logical
        && let Some(code) = named_code(*named)
    {
        // A modifier already on a modifier key keeps its side.
        if at.is_some_and(|at| same_modifier(at, code)) {
            return at;
        }
        return Some(code);
    }
    at
}

/// Both are the same modifier, either side.
fn same_modifier(a: KeyCode, b: KeyCode) -> bool {
    use KeyCode as C;
    let pair = |k| match k {
        C::ShiftLeft | C::ShiftRight => 1,
        C::ControlLeft | C::ControlRight => 2,
        C::AltLeft | C::AltRight => 3,
        C::SuperLeft | C::SuperRight => 4,
        _ => 0,
    };
    pair(a) != 0 && pair(a) == pair(b)
}

/// The key a named (non-printing) key normally sits on.
fn named_code(key: NamedKey) -> Option<KeyCode> {
    use KeyCode as C;
    use NamedKey as N;
    Some(match key {
        N::Escape => C::Escape,
        N::Enter => C::Enter,
        N::Tab => C::Tab,
        N::Backspace => C::Backspace,
        N::Space => C::Space,
        N::Delete => C::Delete,
        N::Insert => C::Insert,
        N::Home => C::Home,
        N::End => C::End,
        N::PageUp => C::PageUp,
        N::PageDown => C::PageDown,
        N::ArrowUp => C::ArrowUp,
        N::ArrowDown => C::ArrowDown,
        N::ArrowLeft => C::ArrowLeft,
        N::ArrowRight => C::ArrowRight,
        N::CapsLock => C::CapsLock,
        // Logical modifiers don't say which side: the left one, unless the
        // key pressed is already the right one.
        N::Shift => C::ShiftLeft,
        N::Control => C::ControlLeft,
        N::Alt => C::AltLeft,
        N::Super => C::SuperLeft,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swapped_caps_lock_is_escape() {
        // Esc and Caps Lock swapped: the key in Caps Lock's place says Escape.
        let at_caps = PhysicalKey::Code(KeyCode::CapsLock);
        assert_eq!(
            code(at_caps, &Key::Named(NamedKey::Escape)),
            Some(KeyCode::Escape)
        );
        // …and the real Esc key is now Caps Lock, which does nothing.
        let at_esc = PhysicalKey::Code(KeyCode::Escape);
        assert_eq!(
            code(at_esc, &Key::Named(NamedKey::CapsLock)),
            Some(KeyCode::CapsLock)
        );
    }

    #[test]
    fn letters_keep_their_place() {
        // AZERTY: the key in W's place types Z, and still drives forward.
        let w = PhysicalKey::Code(KeyCode::KeyW);
        assert_eq!(code(w, &Key::Character("z".into())), Some(KeyCode::KeyW));
    }

    #[test]
    fn unmapped_keys_fall_back_to_their_place() {
        let f5 = PhysicalKey::Code(KeyCode::F5);
        assert_eq!(code(f5, &Key::Named(NamedKey::F5)), Some(KeyCode::F5));
        let space = PhysicalKey::Code(KeyCode::Space);
        assert_eq!(
            code(space, &Key::Named(NamedKey::Space)),
            Some(KeyCode::Space)
        );
    }

    #[test]
    fn modifiers_keep_their_side() {
        let right = PhysicalKey::Code(KeyCode::ShiftRight);
        assert_eq!(
            code(right, &Key::Named(NamedKey::Shift)),
            Some(KeyCode::ShiftRight)
        );
        // Caps Lock remapped to Control is Control.
        let caps = PhysicalKey::Code(KeyCode::CapsLock);
        assert_eq!(
            code(caps, &Key::Named(NamedKey::Control)),
            Some(KeyCode::ControlLeft)
        );
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let odd = PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Unidentified);
        assert_eq!(
            code(
                odd,
                &Key::Unidentified(winit::keyboard::NativeKey::Unidentified)
            ),
            None
        );
    }
}
