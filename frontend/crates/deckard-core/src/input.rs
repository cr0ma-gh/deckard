//! Controller state handed to the core (RetroPad model).

use libretro_sys::*;

/// Number of controller ports exposed to cores.
pub const MAX_PORTS: usize = 4;

/// RetroPad buttons, as bit positions matching `RETRO_DEVICE_ID_JOYPAD_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Button {
    B = RETRO_DEVICE_ID_JOYPAD_B,
    Y = RETRO_DEVICE_ID_JOYPAD_Y,
    Select = RETRO_DEVICE_ID_JOYPAD_SELECT,
    Start = RETRO_DEVICE_ID_JOYPAD_START,
    Up = RETRO_DEVICE_ID_JOYPAD_UP,
    Down = RETRO_DEVICE_ID_JOYPAD_DOWN,
    Left = RETRO_DEVICE_ID_JOYPAD_LEFT,
    Right = RETRO_DEVICE_ID_JOYPAD_RIGHT,
    A = RETRO_DEVICE_ID_JOYPAD_A,
    X = RETRO_DEVICE_ID_JOYPAD_X,
    L = RETRO_DEVICE_ID_JOYPAD_L,
    R = RETRO_DEVICE_ID_JOYPAD_R,
    L2 = RETRO_DEVICE_ID_JOYPAD_L2,
    R2 = RETRO_DEVICE_ID_JOYPAD_R2,
    L3 = RETRO_DEVICE_ID_JOYPAD_L3,
    R3 = RETRO_DEVICE_ID_JOYPAD_R3,
}

/// State of one RetroPad.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PadState {
    /// Bitmask of pressed buttons, bit `n` = `RETRO_DEVICE_ID_JOYPAD_*` id `n`.
    pub buttons: u16,
    /// `[left, right]` sticks as `[x, y]`, range -32768..=32767, y grows downwards.
    pub sticks: [[i16; 2]; 2],
}

impl PadState {
    pub fn set(&mut self, button: Button, pressed: bool) {
        let bit = 1u16 << button as u32;
        if pressed {
            self.buttons |= bit;
        } else {
            self.buttons &= !bit;
        }
    }

    pub fn is_pressed(&self, button: Button) -> bool {
        self.buttons & (1 << button as u32) != 0
    }

    /// Merges another pad into this one (buttons OR-ed, largest stick deflection wins).
    pub fn merge(&mut self, other: &PadState) {
        self.buttons |= other.buttons;
        for (mine, theirs) in self
            .sticks
            .iter_mut()
            .flatten()
            .zip(other.sticks.iter().flatten())
        {
            if theirs.unsigned_abs() > mine.unsigned_abs() {
                *mine = *theirs;
            }
        }
    }

    /// Answers a libretro `input_state` query for this pad.
    pub(crate) fn query(&self, device: u32, index: u32, id: u32) -> i16 {
        match device & RETRO_DEVICE_MASK {
            RETRO_DEVICE_JOYPAD if id == RETRO_DEVICE_ID_JOYPAD_MASK => self.buttons as i16,
            RETRO_DEVICE_JOYPAD if id < 16 => i16::from(self.buttons & (1 << id) != 0),
            RETRO_DEVICE_ANALOG => match (index, id) {
                (RETRO_DEVICE_INDEX_ANALOG_LEFT | RETRO_DEVICE_INDEX_ANALOG_RIGHT, 0..=1) => {
                    self.sticks[index as usize][id as usize]
                }
                // Analog buttons: report digital state at full pressure.
                (RETRO_DEVICE_INDEX_ANALOG_BUTTON, id)
                    if id < 16 && self.buttons & (1 << id) != 0 =>
                {
                    i16::MAX
                }
                _ => 0,
            },
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joypad_queries() {
        let mut pad = PadState::default();
        pad.set(Button::A, true);
        pad.set(Button::Up, true);
        assert_eq!(
            pad.query(RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_A),
            1
        );
        assert_eq!(
            pad.query(RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_B),
            0
        );
        let mask = pad.query(RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_MASK);
        assert_eq!(mask as u16, (1 << 8) | (1 << 4));
        pad.set(Button::Up, false);
        assert!(!pad.is_pressed(Button::Up));
    }

    #[test]
    fn analog_queries() {
        let pad = PadState {
            buttons: 0,
            sticks: [[100, -200], [300, 400]],
        };
        let q = |index, id| pad.query(RETRO_DEVICE_ANALOG, index, id);
        assert_eq!(
            q(RETRO_DEVICE_INDEX_ANALOG_LEFT, RETRO_DEVICE_ID_ANALOG_Y),
            -200
        );
        assert_eq!(
            q(RETRO_DEVICE_INDEX_ANALOG_RIGHT, RETRO_DEVICE_ID_ANALOG_X),
            300
        );
        assert_eq!(q(5, 0), 0);
    }

    #[test]
    fn merge_keeps_largest_deflection() {
        let mut a = PadState {
            buttons: 1,
            sticks: [[10, -500], [0, 0]],
        };
        let b = PadState {
            buttons: 2,
            sticks: [[-20, 100], [0, 0]],
        };
        a.merge(&b);
        assert_eq!(a.buttons, 3);
        assert_eq!(a.sticks[0], [-20, -500]);
    }
}
