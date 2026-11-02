//! Gamepad and keyboard input, mapped to RetroPads.
//!
//! Port 0 merges the keyboard with the first gamepad, so the same code works on a
//! desktop and on the Steam Deck (whose built-in controls appear as a gamepad
//! through the kernel `hid-steam` driver when Steam is not running).

use deckard_core::{Button, MAX_PORTS, PadState};
use gilrs::{Axis, Gilrs};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

/// Frontend actions triggered by input (not sent to the core).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hotkeys {
    /// `Esc`: leave the game (or quit when started from the command line).
    pub quit: bool,
    /// Guide/Steam button or `F1`: open the menu.
    pub menu: bool,
    pub save_state: bool,
    pub load_state: bool,
    pub next_slot: bool,
    pub previous_slot: bool,
}

/// Menu controls currently held. Confirm is the bottom face button (A on the
/// Deck and Xbox pads, RetroPad B), back the right one; on a keyboard
/// `Enter`/`Z` confirm and `Backspace`/`Esc`/`X` go back.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MenuButtons {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub confirm: bool,
    pub back: bool,
}

/// Stick deflection that counts as a direction in menus.
const MENU_STICK_THRESHOLD: i16 = 16_000;

/// Collects keyboard and gamepad state.
pub struct Input {
    gilrs: Option<Gilrs>,
    keyboard: PadState,
    /// Keyboard keys used only by menus: Enter, Backspace, Escape.
    menu_keys: [bool; 3],
    hotkeys: Hotkeys,
    last_port0: PadState,
    /// Menu presses seen since the last [`Input::menu_buttons`] call, so a tap
    /// shorter than a frame still counts.
    menu_latch: MenuButtons,
}

impl MenuButtons {
    fn or(self, other: Self) -> Self {
        Self {
            up: self.up || other.up,
            down: self.down || other.down,
            left: self.left || other.left,
            right: self.right || other.right,
            confirm: self.confirm || other.confirm,
            back: self.back || other.back,
        }
    }
}

impl Default for Input {
    fn default() -> Self {
        Self::new()
    }
}

impl Input {
    pub fn new() -> Self {
        let gilrs = match Gilrs::new() {
            Ok(g) => {
                for (_, pad) in g.gamepads() {
                    tracing::info!(name = pad.name(), "gamepad found");
                }
                Some(g)
            }
            Err(e) => {
                tracing::warn!("gamepads unavailable: {e}");
                None
            }
        };
        Self {
            gilrs,
            keyboard: PadState::default(),
            menu_keys: [false; 3],
            hotkeys: Hotkeys::default(),
            last_port0: PadState::default(),
            menu_latch: MenuButtons::default(),
        }
    }

    /// Feeds a keyboard event from the window.
    pub fn handle_key(&mut self, key: KeyCode, pressed: bool) {
        match key {
            KeyCode::Enter | KeyCode::NumpadEnter => self.menu_keys[0] = pressed,
            KeyCode::Backspace => self.menu_keys[1] = pressed,
            KeyCode::Escape => self.menu_keys[2] = pressed,
            _ => {}
        }
        if pressed {
            let latch = &mut self.menu_latch;
            match key {
                KeyCode::ArrowUp => latch.up = true,
                KeyCode::ArrowDown => latch.down = true,
                KeyCode::ArrowLeft => latch.left = true,
                KeyCode::ArrowRight => latch.right = true,
                KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::KeyZ => latch.confirm = true,
                KeyCode::Backspace | KeyCode::Escape | KeyCode::KeyX => latch.back = true,
                _ => {}
            }
        }
        if pressed {
            // Same keys as RetroArch's defaults.
            match key {
                KeyCode::Escape => self.hotkeys.quit = true,
                KeyCode::F1 => self.hotkeys.menu = true,
                KeyCode::F2 => self.hotkeys.save_state = true,
                KeyCode::F4 => self.hotkeys.load_state = true,
                KeyCode::F6 => self.hotkeys.previous_slot = true,
                KeyCode::F7 => self.hotkeys.next_slot = true,
                _ => {}
            }
        }
        if let Some(button) = keyboard_button(key) {
            self.keyboard.set(button, pressed);
        }
    }

    /// Releases all keys (e.g. when the window loses focus).
    pub fn clear_keyboard(&mut self) {
        self.keyboard = PadState::default();
        self.menu_keys = [false; 3];
        self.menu_latch = MenuButtons::default();
    }

    /// Menu controls held right now (from the last [`Input::poll`]) or pressed
    /// since the previous call.
    pub fn menu_buttons(&mut self) -> MenuButtons {
        let latched = std::mem::take(&mut self.menu_latch);
        self.held_menu_buttons().or(latched)
    }

    fn held_menu_buttons(&self) -> MenuButtons {
        let pad = &self.last_port0;
        let [x, y] = pad.sticks[0];
        MenuButtons {
            up: pad.is_pressed(Button::Up) || y < -MENU_STICK_THRESHOLD,
            down: pad.is_pressed(Button::Down) || y > MENU_STICK_THRESHOLD,
            left: pad.is_pressed(Button::Left) || x < -MENU_STICK_THRESHOLD,
            right: pad.is_pressed(Button::Right) || x > MENU_STICK_THRESHOLD,
            confirm: pad.is_pressed(Button::B) || self.menu_keys[0],
            back: pad.is_pressed(Button::A) || self.menu_keys[1] || self.menu_keys[2],
        }
    }

    /// Returns hotkeys pressed since the last call.
    pub fn take_hotkeys(&mut self) -> Hotkeys {
        std::mem::take(&mut self.hotkeys)
    }

    /// Reads all devices and returns the state of every port.
    pub fn poll(&mut self) -> [PadState; MAX_PORTS] {
        let mut ports = [PadState::default(); MAX_PORTS];
        ports[0] = self.keyboard;

        let Some(gilrs) = self.gilrs.as_mut() else {
            self.last_port0 = ports[0];
            return ports;
        };
        // Processing events is what updates gilrs' cached gamepad state.
        while let Some(event) = gilrs.next_event() {
            match event.event {
                gilrs::EventType::ButtonPressed(gilrs::Button::Mode, _) => self.hotkeys.menu = true,
                gilrs::EventType::ButtonPressed(button, _) => {
                    let latch = &mut self.menu_latch;
                    match button {
                        gilrs::Button::DPadUp => latch.up = true,
                        gilrs::Button::DPadDown => latch.down = true,
                        gilrs::Button::DPadLeft => latch.left = true,
                        gilrs::Button::DPadRight => latch.right = true,
                        gilrs::Button::South => latch.confirm = true,
                        gilrs::Button::East => latch.back = true,
                        _ => {}
                    }
                }
                gilrs::EventType::Connected => {
                    tracing::info!(name = gilrs.gamepad(event.id).name(), "gamepad connected");
                }
                gilrs::EventType::Disconnected => tracing::info!("gamepad disconnected"),
                _ => {}
            }
        }

        for (port, (_, gamepad)) in gilrs.gamepads().take(MAX_PORTS).enumerate() {
            let mut pad = PadState::default();
            for (gil, retro) in GAMEPAD_BUTTONS {
                pad.set(retro, gamepad.is_pressed(gil));
            }
            pad.sticks = [
                [
                    axis(gamepad.value(Axis::LeftStickX)),
                    axis(-gamepad.value(Axis::LeftStickY)),
                ],
                [
                    axis(gamepad.value(Axis::RightStickX)),
                    axis(-gamepad.value(Axis::RightStickY)),
                ],
            ];
            ports[port].merge(&pad);
        }
        self.last_port0 = ports[0];
        ports
    }
}

/// Converts a gilrs axis value (-1.0..=1.0) to the libretro range.
fn axis(value: f32) -> i16 {
    (value.clamp(-1.0, 1.0) * 32767.0) as i16
}

/// Physical gamepad buttons to RetroPad buttons (RetroPad uses the Nintendo
/// layout: B is the bottom face button, A the right one).
const GAMEPAD_BUTTONS: [(gilrs::Button, Button); 16] = [
    (gilrs::Button::South, Button::B),
    (gilrs::Button::East, Button::A),
    (gilrs::Button::West, Button::Y),
    (gilrs::Button::North, Button::X),
    (gilrs::Button::LeftTrigger, Button::L),
    (gilrs::Button::RightTrigger, Button::R),
    (gilrs::Button::LeftTrigger2, Button::L2),
    (gilrs::Button::RightTrigger2, Button::R2),
    (gilrs::Button::LeftThumb, Button::L3),
    (gilrs::Button::RightThumb, Button::R3),
    (gilrs::Button::Select, Button::Select),
    (gilrs::Button::Start, Button::Start),
    (gilrs::Button::DPadUp, Button::Up),
    (gilrs::Button::DPadDown, Button::Down),
    (gilrs::Button::DPadLeft, Button::Left),
    (gilrs::Button::DPadRight, Button::Right),
];

/// Key code for a window keyboard event. Navigation keys are taken from their
/// meaning, so numpad arrows (Num Lock off) and virtual keyboards work too;
/// everything else from its physical position (layout-independent game keys).
pub fn key_code(physical: PhysicalKey, logical: &Key) -> Option<KeyCode> {
    if let Key::Named(named) = logical {
        let code = match named {
            NamedKey::ArrowUp => Some(KeyCode::ArrowUp),
            NamedKey::ArrowDown => Some(KeyCode::ArrowDown),
            NamedKey::ArrowLeft => Some(KeyCode::ArrowLeft),
            NamedKey::ArrowRight => Some(KeyCode::ArrowRight),
            NamedKey::Enter => Some(KeyCode::Enter),
            NamedKey::Escape => Some(KeyCode::Escape),
            NamedKey::Backspace => Some(KeyCode::Backspace),
            _ => None,
        };
        if code.is_some() {
            return code;
        }
    }
    match physical {
        PhysicalKey::Code(code) => Some(code),
        PhysicalKey::Unidentified(_) => None,
    }
}

/// Default keyboard layout for desktop development.
fn keyboard_button(key: KeyCode) -> Option<Button> {
    Some(match key {
        KeyCode::ArrowUp => Button::Up,
        KeyCode::ArrowDown => Button::Down,
        KeyCode::ArrowLeft => Button::Left,
        KeyCode::ArrowRight => Button::Right,
        KeyCode::KeyX => Button::A,
        KeyCode::KeyZ => Button::B,
        KeyCode::KeyS => Button::X,
        KeyCode::KeyA => Button::Y,
        KeyCode::KeyQ => Button::L,
        KeyCode::KeyW => Button::R,
        KeyCode::Digit1 => Button::L2,
        KeyCode::Digit2 => Button::R2,
        KeyCode::Enter => Button::Start,
        KeyCode::ShiftRight => Button::Select,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offline_input() -> Input {
        Input {
            gilrs: None,
            keyboard: PadState::default(),
            menu_keys: [false; 3],
            hotkeys: Hotkeys::default(),
            last_port0: PadState::default(),
            menu_latch: MenuButtons::default(),
        }
    }

    #[test]
    fn numpad_arrows_count_as_arrows() {
        let down = Key::Named(NamedKey::ArrowDown);
        assert_eq!(
            key_code(PhysicalKey::Code(KeyCode::Numpad2), &down),
            Some(KeyCode::ArrowDown)
        );
        // AZERTY: the physical Z key types "w"; games keep the physical key.
        let w = Key::Character("w".into());
        assert_eq!(
            key_code(PhysicalKey::Code(KeyCode::KeyZ), &w),
            Some(KeyCode::KeyZ)
        );
    }

    #[test]
    fn short_taps_are_not_lost() {
        let mut input = offline_input();
        // Pressed and released between two frames.
        input.handle_key(KeyCode::ArrowDown, true);
        input.handle_key(KeyCode::ArrowDown, false);
        input.poll();
        assert!(input.menu_buttons().down, "seen once");
        input.poll();
        assert!(!input.menu_buttons().down, "and only once");
    }

    #[test]
    fn menu_buttons_from_keyboard() {
        let mut input = offline_input();
        input.handle_key(KeyCode::ArrowDown, true);
        input.handle_key(KeyCode::Enter, true);
        input.poll();
        let m = input.menu_buttons();
        assert!(m.down && m.confirm && !m.back && !m.up);
        input.handle_key(KeyCode::Enter, false);
        input.handle_key(KeyCode::Escape, true);
        input.poll();
        let m = input.menu_buttons();
        assert!(m.back && !m.confirm);
        input.handle_key(KeyCode::KeyZ, true); // RetroPad B = bottom button = confirm
        input.poll();
        assert!(input.menu_buttons().confirm);
        input.clear_keyboard();
        input.poll();
        assert_eq!(input.menu_buttons(), MenuButtons::default());
        input.handle_key(KeyCode::F1, true);
        assert!(input.take_hotkeys().menu);
    }

    #[test]
    fn axis_conversion() {
        assert_eq!(axis(0.0), 0);
        assert_eq!(axis(1.0), 32767);
        assert_eq!(axis(-1.0), -32767);
        assert_eq!(axis(5.0), 32767, "clamped");
    }

    #[test]
    fn keyboard_maps_to_port_zero_and_escape_quits() {
        let mut input = offline_input();
        input.handle_key(KeyCode::KeyX, true);
        input.handle_key(KeyCode::ArrowLeft, true);
        let ports = input.poll();
        assert!(ports[0].is_pressed(Button::A));
        assert!(ports[0].is_pressed(Button::Left));
        assert_eq!(ports[1], PadState::default());

        input.handle_key(KeyCode::KeyX, false);
        assert!(!input.poll()[0].is_pressed(Button::A));

        assert!(!input.take_hotkeys().quit);
        input.handle_key(KeyCode::Escape, true);
        assert!(input.take_hotkeys().quit);
        assert!(!input.take_hotkeys().quit, "hotkeys are one-shot");

        input.handle_key(KeyCode::F2, true);
        input.handle_key(KeyCode::F7, true);
        input.handle_key(KeyCode::F4, false); // release does nothing
        let hk = input.take_hotkeys();
        assert!(hk.save_state && hk.next_slot && !hk.load_state && !hk.previous_slot);
    }

    #[test]
    fn every_retropad_button_has_a_gamepad_mapping() {
        let mut pad = PadState::default();
        for (_, retro) in GAMEPAD_BUTTONS {
            pad.set(retro, true);
        }
        assert_eq!(pad.buttons, u16::MAX);
    }
}
