//! Key bindings: which keys do what, set on the settings panel's CONTROLS
//! screen and saved with the profile.
//!
//! Every action has two slots, so a control can sit on a letter and an arrow
//! at once. A few keys always work the menus and can't be bound: Esc (back,
//! pause, and cancels a rebind), Backspace (clears a slot while rebinding)
//! and Enter (select). The arrow keys also always move through menus and the
//! garage, whatever the driving keys are.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use winit::keyboard::KeyCode;

/// Something a key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Forward,
    Reverse,
    TurnLeft,
    TurnRight,
    TurretLeft,
    TurretRight,
    CenterTurret,
    Fire,
    CameraUp,
    CameraDown,
    /// Ease the chase camera back to its usual tilt.
    CameraReset,
    Scoreboard,
    /// The whole map, big, on and off.
    Map,
    /// Opens the garage in battle; in the garage it deploys.
    Garage,
    NextHull,
    NextTurret,
    /// Garage: equip the highlighted part, or buy it.
    Equip,
    /// Garage: upgrade the highlighted part.
    Upgrade,
    /// Garage: switch between the hull and turret tabs.
    SwitchTab,
}

/// Where an action works.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Context {
    Battle,
    Garage,
    /// Battle and the garage (and menus): driving keys move through lists.
    Both,
}

impl Context {
    fn overlaps(self, other: Context) -> bool {
        self == Context::Both || other == Context::Both || self == other
    }
}

/// Slots per action.
pub const SLOTS: usize = 2;

/// Every action, in the order the settings panel lists them.
pub const ACTIONS: [Action; 19] = [
    Action::Forward,
    Action::Reverse,
    Action::TurnLeft,
    Action::TurnRight,
    Action::TurretLeft,
    Action::TurretRight,
    Action::CenterTurret,
    Action::Fire,
    Action::CameraUp,
    Action::CameraDown,
    Action::CameraReset,
    Action::Scoreboard,
    Action::Map,
    Action::Garage,
    Action::NextHull,
    Action::NextTurret,
    Action::Equip,
    Action::Upgrade,
    Action::SwitchTab,
];

impl Action {
    pub fn index(self) -> usize {
        self as usize
    }

    /// Its name on the settings panel.
    pub fn name(self) -> &'static str {
        match self {
            Action::Forward => "FORWARD",
            Action::Reverse => "REVERSE",
            Action::TurnLeft => "TURN LEFT",
            Action::TurnRight => "TURN RIGHT",
            Action::TurretLeft => "TURRET LEFT",
            Action::TurretRight => "TURRET RIGHT",
            Action::CenterTurret => "CENTER TURRET",
            Action::Fire => "FIRE",
            Action::CameraUp => "CAMERA UP",
            Action::CameraDown => "CAMERA DOWN",
            Action::CameraReset => "CAMERA RESET",
            Action::Scoreboard => "SCOREBOARD",
            Action::Map => "MAP",
            Action::Garage => "GARAGE",
            Action::NextHull => "NEXT HULL",
            Action::NextTurret => "NEXT TURRET",
            Action::Equip => "EQUIP / BUY",
            Action::Upgrade => "UPGRADE",
            Action::SwitchTab => "HULL / TURRET TAB",
        }
    }

    /// Its name in the profile file.
    fn key(self) -> &'static str {
        match self {
            Action::Forward => "forward",
            Action::Reverse => "reverse",
            Action::TurnLeft => "turn_left",
            Action::TurnRight => "turn_right",
            Action::TurretLeft => "turret_left",
            Action::TurretRight => "turret_right",
            Action::CenterTurret => "center_turret",
            Action::Fire => "fire",
            Action::CameraUp => "camera_up",
            Action::CameraDown => "camera_down",
            Action::CameraReset => "camera_reset",
            Action::Scoreboard => "scoreboard",
            Action::Map => "map",
            Action::Garage => "garage",
            Action::NextHull => "next_hull",
            Action::NextTurret => "next_turret",
            Action::Equip => "equip",
            Action::Upgrade => "upgrade",
            Action::SwitchTab => "switch_tab",
        }
    }

    pub fn context(self) -> Context {
        match self {
            Action::Forward
            | Action::Reverse
            | Action::TurnLeft
            | Action::TurnRight
            | Action::Garage => Context::Both,
            Action::Equip | Action::Upgrade | Action::SwitchTab => Context::Garage,
            _ => Context::Battle,
        }
    }

    fn defaults(self) -> [Option<KeyCode>; SLOTS] {
        use KeyCode as K;
        let (a, b) = match self {
            Action::Forward => (K::KeyW, Some(K::ArrowUp)),
            Action::Reverse => (K::KeyS, Some(K::ArrowDown)),
            Action::TurnLeft => (K::KeyA, Some(K::ArrowLeft)),
            Action::TurnRight => (K::KeyD, Some(K::ArrowRight)),
            Action::TurretLeft => (K::KeyZ, None),
            Action::TurretRight => (K::KeyX, None),
            Action::CenterTurret => (K::KeyC, None),
            Action::Fire => (K::Space, None),
            // Q/E as in Tanki; in the garage they're the tab switch and equip.
            Action::CameraUp => (K::KeyQ, Some(K::PageUp)),
            Action::CameraDown => (K::KeyE, Some(K::PageDown)),
            Action::CameraReset => (K::KeyR, None),
            Action::Scoreboard => (K::Tab, None),
            Action::Map => (K::KeyM, None),
            Action::Garage => (K::KeyG, None),
            Action::NextHull => (K::Digit1, None),
            Action::NextTurret => (K::Digit2, None),
            Action::Equip => (K::KeyE, None),
            Action::Upgrade => (K::KeyU, None),
            Action::SwitchTab => (K::KeyQ, Some(K::Tab)),
        };
        [Some(a), b]
    }
}

/// Keys that always work the menus and can't be bound.
pub fn reserved(code: KeyCode) -> bool {
    matches!(
        code,
        KeyCode::Escape | KeyCode::Backspace | KeyCode::Enter | KeyCode::NumpadEnter
    )
}

/// Keys that can be bound, with their names on screen. Anything else (media
/// keys, Super, keys winit can't name) is ignored while rebinding.
const KEYS: &[(KeyCode, &str)] = {
    use KeyCode as K;
    &[
        (K::KeyA, "A"),
        (K::KeyB, "B"),
        (K::KeyC, "C"),
        (K::KeyD, "D"),
        (K::KeyE, "E"),
        (K::KeyF, "F"),
        (K::KeyG, "G"),
        (K::KeyH, "H"),
        (K::KeyI, "I"),
        (K::KeyJ, "J"),
        (K::KeyK, "K"),
        (K::KeyL, "L"),
        (K::KeyM, "M"),
        (K::KeyN, "N"),
        (K::KeyO, "O"),
        (K::KeyP, "P"),
        (K::KeyQ, "Q"),
        (K::KeyR, "R"),
        (K::KeyS, "S"),
        (K::KeyT, "T"),
        (K::KeyU, "U"),
        (K::KeyV, "V"),
        (K::KeyW, "W"),
        (K::KeyX, "X"),
        (K::KeyY, "Y"),
        (K::KeyZ, "Z"),
        (K::Digit0, "0"),
        (K::Digit1, "1"),
        (K::Digit2, "2"),
        (K::Digit3, "3"),
        (K::Digit4, "4"),
        (K::Digit5, "5"),
        (K::Digit6, "6"),
        (K::Digit7, "7"),
        (K::Digit8, "8"),
        (K::Digit9, "9"),
        (K::ArrowUp, "↑"),
        (K::ArrowDown, "↓"),
        (K::ArrowLeft, "←"),
        (K::ArrowRight, "→"),
        (K::Space, "SPACE"),
        (K::Tab, "TAB"),
        (K::ShiftLeft, "L SHIFT"),
        (K::ShiftRight, "R SHIFT"),
        (K::ControlLeft, "L CTRL"),
        (K::ControlRight, "R CTRL"),
        (K::AltLeft, "L ALT"),
        (K::AltRight, "R ALT"),
        (K::CapsLock, "CAPS"),
        (K::PageUp, "PG UP"),
        (K::PageDown, "PG DN"),
        (K::Home, "HOME"),
        (K::End, "END"),
        (K::Insert, "INS"),
        (K::Delete, "DEL"),
        (K::Backquote, "`"),
        (K::Minus, "-"),
        (K::Equal, "="),
        (K::BracketLeft, "["),
        (K::BracketRight, "]"),
        (K::Backslash, "\\"),
        (K::Semicolon, ";"),
        (K::Quote, "'"),
        (K::Comma, ","),
        (K::Period, "."),
        (K::Slash, "/"),
        (K::F1, "F1"),
        (K::F2, "F2"),
        (K::F3, "F3"),
        (K::F4, "F4"),
        (K::F5, "F5"),
        (K::F6, "F6"),
        (K::F7, "F7"),
        (K::F8, "F8"),
        (K::F9, "F9"),
        (K::F10, "F10"),
        (K::F11, "F11"),
        (K::F12, "F12"),
        (K::Numpad0, "NUM 0"),
        (K::Numpad1, "NUM 1"),
        (K::Numpad2, "NUM 2"),
        (K::Numpad3, "NUM 3"),
        (K::Numpad4, "NUM 4"),
        (K::Numpad5, "NUM 5"),
        (K::Numpad6, "NUM 6"),
        (K::Numpad7, "NUM 7"),
        (K::Numpad8, "NUM 8"),
        (K::Numpad9, "NUM 9"),
        (K::NumpadAdd, "NUM +"),
        (K::NumpadSubtract, "NUM -"),
        (K::NumpadMultiply, "NUM *"),
        (K::NumpadDivide, "NUM /"),
        (K::NumpadDecimal, "NUM ."),
    ]
};

/// A key's name on screen, if it can be bound.
pub fn key_name(code: KeyCode) -> Option<&'static str> {
    KEYS.iter().find(|(k, _)| *k == code).map(|(_, n)| *n)
}

/// A key's name in the profile file: winit's own (`KeyW`, `ArrowUp`).
fn file_name(code: KeyCode) -> String {
    format!("{code:?}")
}

fn from_file_name(name: &str) -> Option<KeyCode> {
    KEYS.iter().map(|(k, _)| *k).find(|&k| file_name(k) == name)
}

/// The keys for every action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Binds([[Option<KeyCode>; SLOTS]; ACTIONS.len()]);

impl Default for Binds {
    fn default() -> Binds {
        Binds(ACTIONS.map(Action::defaults))
    }
}

impl Binds {
    /// The keys on `action`, slot by slot.
    pub fn keys(&self, action: Action) -> [Option<KeyCode>; SLOTS] {
        self.0[action.index()]
    }

    /// Does `code` do `action`?
    pub fn is(&self, action: Action, code: KeyCode) -> bool {
        self.keys(action).contains(&Some(code))
    }

    /// Is any of `action`'s keys among `held`?
    pub fn held(&self, action: Action, held: &impl Fn(KeyCode) -> bool) -> bool {
        self.keys(action).into_iter().flatten().any(held)
    }

    /// The first of `actions` that `code` does.
    pub fn find(&self, code: KeyCode, actions: &[Action]) -> Option<Action> {
        actions.iter().copied().find(|&a| self.is(a, code))
    }

    /// Put `code` on `action`'s `slot`. Anything else that would fire on the
    /// same key in the same place loses it, so one key never does two things.
    pub fn bind(&mut self, action: Action, slot: usize, code: KeyCode) {
        if reserved(code) || key_name(code).is_none() {
            return;
        }
        for other in ACTIONS {
            if other.context().overlaps(action.context()) {
                for k in &mut self.0[other.index()] {
                    if *k == Some(code) {
                        *k = None;
                    }
                }
            }
        }
        self.0[action.index()][slot] = Some(code);
    }

    pub fn unbind(&mut self, action: Action, slot: usize) {
        self.0[action.index()][slot] = None;
    }
}

/// Saved as `{"fire": ["Space"], "forward": ["KeyW", "ArrowUp"], ...}`: an
/// action missing from the file keeps its default (so actions added later
/// get theirs), an empty list means unbound.
impl Serialize for Binds {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let map: BTreeMap<&str, Vec<String>> = ACTIONS
            .iter()
            .map(|&a| {
                let keys = self.keys(a).into_iter().flatten().map(file_name).collect();
                (a.key(), keys)
            })
            .collect();
        map.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Binds {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Binds, D::Error> {
        let map = BTreeMap::<String, Vec<String>>::deserialize(d)?;
        let mut binds = Binds::default();
        for a in ACTIONS {
            let Some(names) = map.get(a.key()) else {
                continue;
            };
            let mut keys = [None; SLOTS];
            for (slot, code) in names
                .iter()
                .filter_map(|n| from_file_name(n))
                .take(SLOTS)
                .enumerate()
            {
                keys[slot] = Some(code);
            }
            binds.0[a.index()] = keys;
        }
        // Saved before Q/E tilted the camera (camera_reset came with them):
        // camera keys still on their old defaults get Q/E too, unless the
        // pilot has put Q or E on something else in battle.
        if !map.contains_key(Action::CameraReset.key()) {
            for (action, old, new) in [
                (Action::CameraUp, KeyCode::PageUp, KeyCode::KeyQ),
                (Action::CameraDown, KeyCode::PageDown, KeyCode::KeyE),
            ] {
                let taken = ACTIONS.iter().any(|&o| {
                    o != action && o.context().overlaps(action.context()) && binds.is(o, new)
                });
                if binds.keys(action) == [Some(old), None] && !taken {
                    binds.0[action.index()] = [Some(new), Some(old)];
                }
            }
        }
        Ok(binds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_are_listed_in_index_order() {
        for (i, a) in ACTIONS.iter().enumerate() {
            assert_eq!(a.index(), i);
        }
    }

    #[test]
    fn defaults_have_names_and_dont_clash() {
        let b = Binds::default();
        for a in ACTIONS {
            for code in b.keys(a).into_iter().flatten() {
                assert!(key_name(code).is_some(), "{code:?}");
                for other in ACTIONS {
                    if other != a && other.context().overlaps(a.context()) {
                        assert!(!b.is(other, code), "{a:?} and {other:?} share {code:?}");
                    }
                }
            }
        }
        // Tab is the scoreboard in battle and the tab switch in the garage.
        assert!(b.is(Action::Scoreboard, KeyCode::Tab));
        assert!(b.is(Action::SwitchTab, KeyCode::Tab));
        // Q/E tilt the camera in battle and work the garage there.
        assert!(b.is(Action::CameraUp, KeyCode::KeyQ));
        assert!(b.is(Action::SwitchTab, KeyCode::KeyQ));
        assert!(b.is(Action::CameraDown, KeyCode::KeyE));
        assert!(b.is(Action::Equip, KeyCode::KeyE));
        assert!(b.is(Action::CameraReset, KeyCode::KeyR));
    }

    #[test]
    fn binding_takes_the_key_from_whatever_had_it() {
        let mut b = Binds::default();
        b.bind(Action::Fire, 1, KeyCode::KeyW);
        assert!(b.is(Action::Fire, KeyCode::KeyW));
        assert!(b.is(Action::Fire, KeyCode::Space));
        assert_eq!(b.keys(Action::Forward), [None, Some(KeyCode::ArrowUp)]);
        // Moving a key between an action's own slots leaves one copy.
        b.bind(Action::Fire, 0, KeyCode::KeyW);
        assert_eq!(b.keys(Action::Fire), [Some(KeyCode::KeyW), None]);
        // The garage's E doesn't clash with a battle-only action...
        b.bind(Action::TurretLeft, 0, KeyCode::KeyE);
        assert!(b.is(Action::Equip, KeyCode::KeyE));
        // ...but driving keys move through the garage, so they do.
        b.bind(Action::Forward, 0, KeyCode::KeyE);
        assert!(!b.is(Action::Equip, KeyCode::KeyE));
        assert!(!b.is(Action::TurretLeft, KeyCode::KeyE));
        // Menu keys stay the menus'.
        b.bind(Action::Fire, 0, KeyCode::Escape);
        b.bind(Action::Fire, 0, KeyCode::Enter);
        assert_eq!(b.keys(Action::Fire), [Some(KeyCode::KeyW), None]);
    }

    #[test]
    fn saves_and_loads() {
        let mut b = Binds::default();
        b.bind(Action::Fire, 0, KeyCode::ControlLeft);
        b.unbind(Action::Forward, 1);
        let text = serde_json::to_string(&b).unwrap();
        let back: Binds = serde_json::from_str(&text).unwrap();
        assert_eq!(back, b);
        // Missing actions keep their defaults; unknown keys and actions are dropped.
        let old: Binds =
            serde_json::from_str(r#"{"fire": ["KeyF", "MediaPlay"], "jump": ["KeyJ"]}"#).unwrap();
        assert_eq!(old.keys(Action::Fire), [Some(KeyCode::KeyF), None]);
        assert_eq!(
            old.keys(Action::Forward),
            Binds::default().keys(Action::Forward)
        );
        let none: Binds = serde_json::from_str(r#"{"fire": []}"#).unwrap();
        assert_eq!(none.keys(Action::Fire), [None, None]);
    }

    #[test]
    fn old_profiles_get_q_and_e_on_the_camera() {
        // Saved with the old camera defaults: Q/E join them.
        let old: Binds =
            serde_json::from_str(r#"{"camera_up": ["PageUp"], "camera_down": ["PageDown"]}"#)
                .unwrap();
        assert_eq!(old, Binds::default());
        // Rebound by the pilot, or Q already firing: left alone.
        let old: Binds = serde_json::from_str(
            r#"{"camera_up": ["Home"], "camera_down": ["PageDown"], "fire": ["KeyE"]}"#,
        )
        .unwrap();
        assert_eq!(old.keys(Action::CameraUp), [Some(KeyCode::Home), None]);
        assert_eq!(
            old.keys(Action::CameraDown),
            [Some(KeyCode::PageDown), None]
        );
        // Saved since: what's there stays.
        let new: Binds = serde_json::from_str(
            r#"{"camera_up": ["PageUp"], "camera_down": ["PageDown"], "camera_reset": ["KeyR"]}"#,
        )
        .unwrap();
        assert_eq!(new.keys(Action::CameraUp), [Some(KeyCode::PageUp), None]);
    }
}
