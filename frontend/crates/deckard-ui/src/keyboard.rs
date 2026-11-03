//! On-screen keyboard for gamepad text entry (Wi-Fi passwords and similar).
//!
//! Four rows of characters plus a row of special keys (Shift, Symbols, Space,
//! Delete, Done). The D-pad moves, Confirm presses the key, Back cancels.
//! A physical keyboard can type directly ([`Keyboard::type_char`]).

use crate::layout::{Align, Layer, Quad, REFERENCE_HEIGHT, Rect, Text};
use crate::nav::Nav;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Lower,
    Upper,
    Symbols,
}

const LOWER: [&str; 4] = ["1234567890", "qwertyuiop", "asdfghjkl-", "zxcvbnm_.@"];
const UPPER: [&str; 4] = ["1234567890", "QWERTYUIOP", "ASDFGHJKL-", "ZXCVBNM_.@"];
const SYMBOLS: [&str; 4] = ["!@#$%^&*()", "-_=+[]{};:", "'\",.<>/?\\|", "`~€£§°¡¿  "];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Special {
    Shift,
    Symbols,
    Space,
    Delete,
    Done,
}

/// Special keys with their width in key units (the row is 10 units wide).
const SPECIALS: [(Special, f32); 5] = [
    (Special::Shift, 1.5),
    (Special::Symbols, 1.5),
    (Special::Space, 4.0),
    (Special::Delete, 1.5),
    (Special::Done, 1.5),
];

/// Result of a keyboard interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyboardResult {
    Done(String),
    Cancel,
}

/// Labels of the special keys (from the translations).
#[derive(Debug, Clone)]
pub struct KeyboardLabels {
    pub shift: String,
    pub symbols: String,
    pub space: String,
    pub delete: String,
    pub done: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Keyboard {
    pub title: String,
    pub text: String,
    page: Page,
    /// 0..=3 character rows, 4 = special keys.
    row: usize,
    /// Index of the key in its row.
    col: usize,
    max_len: usize,
}

impl Keyboard {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            text: String::new(),
            page: Page::Lower,
            row: 1,
            col: 0,
            max_len: 63, // WPA passphrases are at most 63 characters
        }
    }

    fn chars(&self, row: usize) -> Vec<char> {
        let rows = match self.page {
            Page::Lower => LOWER,
            Page::Upper => UPPER,
            Page::Symbols => SYMBOLS,
        };
        rows[row].chars().filter(|c| *c != ' ').collect()
    }

    fn row_len(&self, row: usize) -> usize {
        if row == 4 {
            SPECIALS.len()
        } else {
            self.chars(row).len()
        }
    }

    /// Horizontal center of the selected key, in key units.
    fn center(&self) -> f32 {
        if self.row == 4 {
            let start: f32 = SPECIALS[..self.col].iter().map(|(_, w)| w).sum();
            start + SPECIALS[self.col].1 / 2.0
        } else {
            self.col as f32 + 0.5
        }
    }

    /// Key index in `row` closest to horizontal position `x` (key units).
    fn col_at(&self, row: usize, x: f32) -> usize {
        if row == 4 {
            let mut start = 0.0;
            for (i, (_, w)) in SPECIALS.iter().enumerate() {
                if x < start + w {
                    return i;
                }
                start += w;
            }
            SPECIALS.len() - 1
        } else {
            (x.max(0.0) as usize).min(self.row_len(row) - 1)
        }
    }

    pub fn type_char(&mut self, c: char) {
        if self.text.chars().count() < self.max_len && !c.is_control() {
            self.text.push(c);
        }
    }

    pub fn backspace(&mut self) {
        self.text.pop();
    }

    fn press(&mut self) -> Option<KeyboardResult> {
        if self.row < 4 {
            let c = self.chars(self.row)[self.col];
            self.type_char(c);
            return None;
        }
        match SPECIALS[self.col].0 {
            Special::Shift => {
                self.page = if self.page == Page::Upper {
                    Page::Lower
                } else {
                    Page::Upper
                };
            }
            Special::Symbols => {
                self.page = if self.page == Page::Symbols {
                    Page::Lower
                } else {
                    Page::Symbols
                };
            }
            Special::Space => self.type_char(' '),
            Special::Delete => self.backspace(),
            Special::Done => return Some(KeyboardResult::Done(self.text.clone())),
        }
        None
    }

    pub fn handle(&mut self, nav: Nav) -> Option<KeyboardResult> {
        match nav {
            Nav::Left => {
                let len = self.row_len(self.row);
                self.col = (self.col + len - 1) % len;
            }
            Nav::Right => self.col = (self.col + 1) % self.row_len(self.row),
            Nav::Up | Nav::Down => {
                let x = self.center();
                self.row = if nav == Nav::Up {
                    (self.row + 4) % 5
                } else {
                    (self.row + 1) % 5
                };
                self.col = self.col_at(self.row, x);
            }
            Nav::Confirm => return self.press(),
            Nav::Back => return Some(KeyboardResult::Cancel),
        }
        None
    }
}

const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// The keyboard, drawn over the bottom of the screen.
pub fn keyboard_layer(kb: &Keyboard, labels: &KeyboardLabels, width: f32, height: f32) -> Layer {
    let s = height / REFERENCE_HEIGHT;
    let mut layer = Layer::default();
    layer.quads.push(Quad {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            w: width,
            h: height,
        },
        color: [0.0, 0.0, 0.0, 0.75],
        radius: 0.0,
    });
    let panel_w = (980.0 * s).min(width - 20.0 * s);
    let panel = Rect {
        x: (width - panel_w) / 2.0,
        y: height - 560.0 * s,
        w: panel_w,
        h: 540.0 * s,
    };
    layer.quads.push(Quad {
        rect: panel,
        color: [0.05, 0.07, 0.15, 1.0],
        radius: 18.0 * s,
    });

    let pad = 24.0 * s;
    let inner_x = panel.x + pad;
    let inner_w = panel.w - 2.0 * pad;
    layer.texts.push(Text {
        text: kb.title.clone(),
        x: inner_x,
        y: panel.y + 18.0 * s,
        size: 24.0 * s,
        color: WHITE,
        bold: true,
        align: Align::Left,
        max_width: inner_w,
    });
    // Text field with a cursor bar.
    let field = Rect {
        x: inner_x,
        y: panel.y + 62.0 * s,
        w: inner_w,
        h: 54.0 * s,
    };
    layer.quads.push(Quad {
        rect: field,
        color: [1.0, 1.0, 1.0, 0.10],
        radius: 10.0 * s,
    });
    layer.texts.push(Text {
        text: format!("{}▏", kb.text),
        x: field.x + 16.0 * s,
        y: field.y + 13.0 * s,
        size: 24.0 * s,
        color: WHITE,
        bold: false,
        align: Align::Left,
        max_width: field.w - 32.0 * s,
    });

    let unit = inner_w / 10.0;
    let key_h = 62.0 * s;
    let gap = 6.0 * s;
    let top = field.y + field.h + 20.0 * s;
    let draw_key = |x: f32, y: f32, w: f32, label: &str, selected: bool, layer: &mut Layer| {
        let rect = Rect {
            x: x + gap / 2.0,
            y,
            w: w - gap,
            h: key_h - gap,
        };
        layer.quads.push(Quad {
            rect,
            color: if selected {
                [0.40, 0.55, 1.0, 0.95]
            } else {
                [1.0, 1.0, 1.0, 0.12]
            },
            radius: 10.0 * s,
        });
        layer.texts.push(Text {
            text: label.to_owned(),
            x: rect.x + rect.w / 2.0,
            y: rect.y + rect.h / 2.0 - 13.0 * s,
            size: 22.0 * s,
            color: WHITE,
            bold: selected,
            align: Align::Center,
            max_width: rect.w,
        });
    };
    for row in 0..4 {
        let y = top + row as f32 * key_h;
        for (col, c) in kb.chars(row).into_iter().enumerate() {
            let selected = kb.row == row && kb.col == col;
            draw_key(
                inner_x + col as f32 * unit,
                y,
                unit,
                &c.to_string(),
                selected,
                &mut layer,
            );
        }
    }
    let y = top + 4.0 * key_h;
    let mut x = inner_x;
    for (col, (special, w)) in SPECIALS.iter().enumerate() {
        let label = match special {
            Special::Shift => &labels.shift,
            Special::Symbols => &labels.symbols,
            Special::Space => &labels.space,
            Special::Delete => &labels.delete,
            Special::Done => &labels.done,
        };
        draw_key(
            x,
            y,
            unit * w,
            label,
            kb.row == 4 && kb.col == col,
            &mut layer,
        );
        x += unit * w;
    }
    layer
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press_char(kb: &mut Keyboard, target: char) {
        // Find the key and move there with the D-pad, like a player would.
        for _ in 0..3 {
            for row in 0..4 {
                if let Some(col) = kb.chars(row).iter().position(|c| *c == target) {
                    while kb.row != row {
                        kb.handle(Nav::Down);
                    }
                    while kb.col != col {
                        kb.handle(Nav::Right);
                    }
                    kb.handle(Nav::Confirm);
                    return;
                }
            }
            // Not on this page: switch with Shift or Symbols.
            go_special(
                kb,
                if target.is_ascii_alphanumeric() {
                    Special::Shift
                } else {
                    Special::Symbols
                },
            );
            kb.handle(Nav::Confirm);
        }
        panic!("character {target:?} not on the keyboard");
    }

    fn go_special(kb: &mut Keyboard, special: Special) {
        while kb.row != 4 {
            kb.handle(Nav::Down);
        }
        while SPECIALS[kb.col].0 != special {
            kb.handle(Nav::Right);
        }
    }

    #[test]
    fn types_with_the_dpad() {
        let mut kb = Keyboard::new("Password");
        for c in "aB3!".chars() {
            press_char(&mut kb, c);
        }
        assert_eq!(kb.text, "aB3!");
        go_special(&mut kb, Special::Delete);
        kb.handle(Nav::Confirm);
        go_special(&mut kb, Special::Space);
        kb.handle(Nav::Confirm);
        assert_eq!(kb.text, "aB3 ");
        go_special(&mut kb, Special::Done);
        assert_eq!(
            kb.handle(Nav::Confirm),
            Some(KeyboardResult::Done("aB3 ".into()))
        );
    }

    #[test]
    fn moves_between_rows_by_position() {
        let mut kb = Keyboard::new("x");
        // Row 1 starts on 'q'; down three times reaches the special row under 'q' → Shift.
        kb.handle(Nav::Down);
        kb.handle(Nav::Down);
        kb.handle(Nav::Down);
        assert_eq!((kb.row, kb.col), (4, 0));
        // From the middle of the row, down lands on the wide Space key.
        let mut kb = Keyboard::new("x");
        for _ in 0..5 {
            kb.handle(Nav::Right);
        }
        for _ in 0..3 {
            kb.handle(Nav::Down);
        }
        assert_eq!(SPECIALS[kb.col].0, Special::Space);
        kb.handle(Nav::Down);
        assert_eq!(kb.row, 0, "wraps to the top row");
        kb.handle(Nav::Left);
        kb.handle(Nav::Left);
        assert_eq!(kb.chars(0)[kb.col], '4');
    }

    #[test]
    fn physical_keys_cancel_and_limit() {
        let mut kb = Keyboard::new("x");
        for c in "abc\n".chars() {
            kb.type_char(c);
        }
        kb.backspace();
        assert_eq!(kb.text, "ab", "control characters ignored");
        for _ in 0..100 {
            kb.type_char('x');
        }
        assert_eq!(kb.text.chars().count(), 63, "WPA maximum");
        assert_eq!(kb.handle(Nav::Back), Some(KeyboardResult::Cancel));
    }
}
