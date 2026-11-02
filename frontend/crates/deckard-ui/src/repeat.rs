//! Turns held buttons into navigation steps, with key repeat for directions.

use std::time::{Duration, Instant};

use crate::nav::Nav;

/// Delay before a held direction starts repeating.
pub const REPEAT_DELAY: Duration = Duration::from_millis(380);
/// Interval between repeats.
pub const REPEAT_INTERVAL: Duration = Duration::from_millis(70);

/// Buttons currently held, from any device.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub confirm: bool,
    pub back: bool,
}

impl Held {
    /// The held direction, if exactly one wins (vertical first).
    fn direction(&self) -> Option<Nav> {
        match (self.up, self.down, self.left, self.right) {
            (true, false, _, _) => Some(Nav::Up),
            (false, true, _, _) => Some(Nav::Down),
            (_, _, true, false) => Some(Nav::Left),
            (_, _, false, true) => Some(Nav::Right),
            _ => None,
        }
    }
}

/// Edge detection for confirm/back, delay + repeat for directions.
#[derive(Debug, Default)]
pub struct Repeater {
    direction: Option<(Nav, Instant)>,
    next_repeat: Option<Instant>,
    previous: Held,
}

impl Repeater {
    pub fn new() -> Self {
        Self::default()
    }

    /// Navigation steps for this frame.
    pub fn update(&mut self, held: Held, now: Instant) -> Vec<Nav> {
        let mut out = Vec::new();
        if held.confirm && !self.previous.confirm {
            out.push(Nav::Confirm);
        }
        if held.back && !self.previous.back {
            out.push(Nav::Back);
        }

        match (held.direction(), self.direction) {
            (Some(dir), Some((current, _))) if dir == current => {
                if self.next_repeat.is_some_and(|t| now >= t) {
                    out.push(dir);
                    self.next_repeat = Some(now + REPEAT_INTERVAL);
                }
            }
            (Some(dir), _) => {
                out.push(dir);
                self.direction = Some((dir, now));
                self.next_repeat = Some(now + REPEAT_DELAY);
            }
            (None, _) => {
                self.direction = None;
                self.next_repeat = None;
            }
        }
        self.previous = held;
        out
    }

    /// Forgets held state (e.g. when coming back from a game, so the button
    /// that closed it does not trigger anything).
    pub fn reset(&mut self, held: Held) {
        self.direction = held.direction().map(|d| (d, Instant::now()));
        self.next_repeat = None;
        self.previous = held;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn down() -> Held {
        Held {
            down: true,
            ..Held::default()
        }
    }

    #[test]
    fn confirm_and_back_fire_once_per_press() {
        let mut r = Repeater::new();
        let t = Instant::now();
        let confirm = Held {
            confirm: true,
            ..Held::default()
        };
        assert_eq!(r.update(confirm, t), [Nav::Confirm]);
        assert!(r.update(confirm, t + Duration::from_secs(1)).is_empty());
        assert!(r.update(Held::default(), t).is_empty());
        assert_eq!(
            r.update(
                Held {
                    back: true,
                    ..Held::default()
                },
                t
            ),
            [Nav::Back]
        );
    }

    #[test]
    fn directions_repeat_after_a_delay() {
        let mut r = Repeater::new();
        let t = Instant::now();
        assert_eq!(r.update(down(), t), [Nav::Down]);
        assert!(r.update(down(), t + REPEAT_DELAY / 2).is_empty());
        assert_eq!(r.update(down(), t + REPEAT_DELAY), [Nav::Down]);
        assert!(
            r.update(down(), t + REPEAT_DELAY + REPEAT_INTERVAL / 2)
                .is_empty()
        );
        assert_eq!(
            r.update(down(), t + REPEAT_DELAY + REPEAT_INTERVAL),
            [Nav::Down]
        );
    }

    #[test]
    fn changing_direction_fires_immediately() {
        let mut r = Repeater::new();
        let t = Instant::now();
        r.update(down(), t);
        let right = Held {
            right: true,
            ..Held::default()
        };
        assert_eq!(r.update(right, t + Duration::from_millis(10)), [Nav::Right]);
        assert!(
            r.update(
                Held {
                    up: true,
                    down: true,
                    ..Held::default()
                },
                t
            )
            .is_empty(),
            "opposite keys cancel"
        );
    }

    #[test]
    fn reset_swallows_held_buttons() {
        let mut r = Repeater::new();
        let held = Held {
            confirm: true,
            down: true,
            ..Held::default()
        };
        r.reset(held);
        assert!(r.update(held, Instant::now()).is_empty());
    }
}
