//! Save state files: `<states>/<system id>/<game>.state<slot>`.
//! Slot 0 is `<game>.state`, matching the usual libretro naming.

use std::path::{Path, PathBuf};

/// File for a save state slot.
pub fn state_path(states_dir: &Path, system_id: &str, game_stem: &str, slot: u8) -> PathBuf {
    let name = if slot == 0 {
        format!("{game_stem}.state")
    } else {
        format!("{game_stem}.state{slot}")
    };
    states_dir.join(system_id).join(name)
}

/// The selected slot, cycling through `0..count`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateSlots {
    current: u8,
    count: u8,
}

impl StateSlots {
    pub fn new(count: u8) -> Self {
        Self {
            current: 0,
            count: count.max(1),
        }
    }

    pub fn current(&self) -> u8 {
        self.current
    }

    pub fn select_next(&mut self) -> u8 {
        self.current = (self.current + 1) % self.count;
        self.current
    }

    pub fn select_previous(&mut self) -> u8 {
        self.current = (self.current + self.count - 1) % self.count;
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_per_slot() {
        let dir = Path::new("states");
        assert_eq!(
            state_path(dir, "snes", "Mario", 0),
            dir.join("snes").join("Mario.state")
        );
        assert_eq!(
            state_path(dir, "snes", "Mario", 3),
            dir.join("snes").join("Mario.state3")
        );
    }

    #[test]
    fn slots_wrap_around() {
        let mut s = StateSlots::new(3);
        assert_eq!(s.current(), 0);
        assert_eq!(s.select_previous(), 2);
        assert_eq!(s.select_next(), 0);
        assert_eq!(s.select_next(), 1);
        assert_eq!(StateSlots::new(0).select_next(), 0, "at least one slot");
    }
}
