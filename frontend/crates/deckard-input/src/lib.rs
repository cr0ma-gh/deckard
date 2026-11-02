//! Gamepad and keyboard input, mapped to RetroPads.
//!
//! Port 0 merges the keyboard with the first gamepad, so the same code works on a
//! desktop and on the Steam Deck (whose built-in controls appear as a gamepad
//! through the kernel `hid-steam` driver when Steam is not running).

use deckard_core::{Button, MAX_PORTS, PadState};
use gilrs::{Axis, Gilrs};
use winit::keyboard::KeyCode;

/// Frontend actions triggered by input (not sent to the core).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hotkeys {
    pub quit: bool,
    pub save_state: bool,
    pub load_state: bool,
    pub next_slot: bool,
    pub previous_slot: bool,
}

/// Collects keyboard and gamepad state.
pub struct Input {
    gilrs: Option<Gilrs>,
    keyboard: PadState,
    hotkeys: Hotkeys,
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
            hotkeys: Hotkeys::default(),
        }
    }

    /// Feeds a keyboard event from the window.
    pub fn handle_key(&mut self, key: KeyCode, pressed: bool) {
        if pressed {
            // Same keys as RetroArch's defaults.
            match key {
                KeyCode::Escape => self.hotkeys.quit = true,
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
            return ports;
        };
        // Processing events is what updates gilrs' cached gamepad state.
        while let Some(event) = gilrs.next_event() {
            match event.event {
                gilrs::EventType::ButtonPressed(gilrs::Button::Mode, _) => self.hotkeys.quit = true,
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

    #[test]
    fn axis_conversion() {
        assert_eq!(axis(0.0), 0);
        assert_eq!(axis(1.0), 32767);
        assert_eq!(axis(-1.0), -32767);
        assert_eq!(axis(5.0), 32767, "clamped");
    }

    #[test]
    fn keyboard_maps_to_port_zero_and_escape_quits() {
        let mut input = Input {
            gilrs: None,
            keyboard: PadState::default(),
            hotkeys: Hotkeys::default(),
        };
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
