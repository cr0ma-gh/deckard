//! On-screen keyboard for text entry (Wi-Fi passwords and similar), modelled
//! on the SteamOS keyboard: a full-width PC layout docked at the bottom of the
//! screen, usable by touch, with the D-pad/stick and with button shortcuts.
//!
//! Controls: A presses the selected key, B closes, X deletes, Y types a space,
//! L2 is Shift, R2 is Enter, L1/R1 move the text cursor ([`Shortcut`]).
//! A tap or click presses the key under it ([`Keyboard::tap`]); a physical
//! keyboard types directly ([`Keyboard::type_char`]).

use crate::layout::{Align, Layer, Quad, REFERENCE_HEIGHT, Rect, Text, hint_bar};
use crate::nav::Nav;

/// Width of every row, in key units.
const ROW_UNITS: f32 = 15.0;

/// Character rows of a page: (normal, shifted), same length, one char per key.
type PageRows = [(&'static str, &'static str); 4];

const LETTERS: PageRows = [
    ("`1234567890-=", "~!@#$%^&*()_+"),
    ("qwertyuiop[]\\", "QWERTYUIOP{}|"),
    ("asdfghjkl;'", "ASDFGHJKL:\""),
    ("zxcvbnm,./", "ZXCVBNM<>?"),
];

const ACCENTS: PageRows = [
    ("`1234567890-=", "~!@#$%^&*()_+"),
    ("àèéìòù€£§°«»¿", "ÀÈÉÌÒÙ¢¥©®“”¡"),
    ("áíóúñçß¤µ¨´", "ÁÍÓÚÑÇ±×÷¬¦"),
    ("äöüâêôûë·¶", "ÄÖÜÂÊÔÛË•†"),
];

/// What a key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    /// The n-th character of the row on the current page.
    Char(usize),
    Backspace,
    Caps,
    Enter,
    Shift,
    /// Switch between letters and accents/symbols.
    Symbols,
    Space,
    Left,
    Right,
    Hide,
}

/// A key's kind and horizontal span in key units.
#[derive(Debug, Clone, Copy, PartialEq)]
struct KeyDef {
    kind: KeyKind,
    x: f32,
    w: f32,
}

/// Builds a row from `(kind, width)` pairs starting at `indent`.
fn row_of(indent: f32, keys: impl IntoIterator<Item = (KeyKind, f32)>) -> Vec<KeyDef> {
    let mut x = indent;
    keys.into_iter()
        .map(|(kind, w)| {
            let key = KeyDef { kind, x, w };
            x += w;
            key
        })
        .collect()
}

fn chars(n: usize) -> impl Iterator<Item = (KeyKind, f32)> {
    (0..n).map(|i| (KeyKind::Char(i), 1.0))
}

/// The five rows of keys (the same on every page).
fn rows() -> [Vec<KeyDef>; 5] {
    use KeyKind::*;
    [
        row_of(0.0, chars(13).chain([(Backspace, 2.0)])),
        row_of(1.5, chars(12).chain([(Char(12), 1.5)])),
        row_of(
            0.0,
            [(Caps, 1.75)]
                .into_iter()
                .chain(chars(11))
                .chain([(Enter, 2.25)]),
        ),
        row_of(
            0.0,
            [(Shift, 2.25)]
                .into_iter()
                .chain(chars(10))
                .chain([(Shift, 2.75)]),
        ),
        row_of(
            0.0,
            [
                (Symbols, 2.25),
                (Space, 7.25),
                (Left, 1.5),
                (Right, 1.5),
                (Hide, 2.5),
            ],
        ),
    ]
}

/// Button shortcuts (SteamOS-style).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    /// X button.
    Backspace,
    /// Y button.
    Space,
    /// L2: Shift for the next character.
    Shift,
    /// R2 or Start: confirm the text.
    Enter,
    /// L1: cursor left.
    CursorLeft,
    /// R1: cursor right.
    CursorRight,
}

/// Result of a keyboard interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyboardResult {
    Done(String),
    Cancel,
}

/// Labels of the special keys and hints (from the translations).
#[derive(Debug, Clone, Default)]
pub struct KeyboardLabels {
    pub shift: String,
    pub caps: String,
    pub enter: String,
    pub symbols: String,
    pub letters: String,
    pub space: String,
    pub delete: String,
    pub hide: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Keyboard {
    pub title: String,
    pub text: String,
    /// Text cursor, in characters from the start.
    cursor: usize,
    accents: bool,
    /// One-shot Shift: applies to the next character only.
    shift: bool,
    caps: bool,
    row: usize,
    col: usize,
    max_len: usize,
}

impl Keyboard {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            text: String::new(),
            cursor: 0,
            accents: false,
            shift: false,
            caps: false,
            // Start on 'q', like the SteamOS keyboard starts on the letters.
            row: 1,
            col: 0,
            max_len: 63, // WPA passphrases are at most 63 characters
        }
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn shift(&self) -> bool {
        self.shift
    }

    pub fn caps(&self) -> bool {
        self.caps
    }

    /// The character a `Char(i)` key types in `row` right now.
    fn char_at(&self, row: usize, i: usize) -> Option<char> {
        let page = if self.accents { ACCENTS } else { LETTERS };
        let (normal, shifted) = page.get(row)?;
        let base = normal.chars().nth(i)?;
        // Caps Lock only affects letters; Shift affects every key.
        let upper = if base.is_alphabetic() {
            self.shift != self.caps
        } else {
            self.shift
        };
        if upper {
            shifted.chars().nth(i)
        } else {
            Some(base)
        }
    }

    fn byte_index(&self, chars: usize) -> usize {
        self.text
            .char_indices()
            .nth(chars)
            .map_or(self.text.len(), |(i, _)| i)
    }

    /// Inserts `c` at the cursor (physical keyboards call this directly).
    pub fn type_char(&mut self, c: char) {
        if self.text.chars().count() < self.max_len && !c.is_control() {
            let at = self.byte_index(self.cursor);
            self.text.insert(at, c);
            self.cursor += 1;
        }
    }

    /// Deletes the character before the cursor.
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            let at = self.byte_index(self.cursor);
            self.text.remove(at);
        }
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let len = self.text.chars().count();
        self.cursor = self.cursor.saturating_add_signed(delta).min(len);
    }

    fn press_kind(&mut self, kind: KeyKind) -> Option<KeyboardResult> {
        match kind {
            KeyKind::Char(i) => {
                if let Some(c) = self.char_at(self.row, i) {
                    self.type_char(c);
                    self.shift = false;
                }
            }
            KeyKind::Backspace => self.backspace(),
            KeyKind::Caps => {
                self.caps = !self.caps;
                self.shift = false;
            }
            KeyKind::Shift => self.shift = !self.shift,
            KeyKind::Symbols => self.accents = !self.accents,
            KeyKind::Space => self.type_char(' '),
            KeyKind::Left => self.move_cursor(-1),
            KeyKind::Right => self.move_cursor(1),
            KeyKind::Enter => return Some(KeyboardResult::Done(self.text.clone())),
            KeyKind::Hide => return Some(KeyboardResult::Cancel),
        }
        None
    }

    fn selected(&self) -> KeyDef {
        rows()[self.row][self.col]
    }

    /// Key in `row` under (or closest to) horizontal position `x`.
    fn col_at(row: &[KeyDef], x: f32) -> usize {
        row.iter()
            .position(|k| x < k.x + k.w)
            .unwrap_or(row.len() - 1)
    }

    /// D-pad / stick navigation and the A/B buttons.
    pub fn handle(&mut self, nav: Nav) -> Option<KeyboardResult> {
        let rows = rows();
        match nav {
            Nav::Left => {
                let len = rows[self.row].len();
                self.col = (self.col + len - 1) % len;
            }
            Nav::Right => self.col = (self.col + 1) % rows[self.row].len(),
            Nav::Up | Nav::Down => {
                let key = self.selected();
                let x = key.x + key.w / 2.0;
                self.row = if nav == Nav::Up {
                    (self.row + rows.len() - 1) % rows.len()
                } else {
                    (self.row + 1) % rows.len()
                };
                self.col = Self::col_at(&rows[self.row], x);
            }
            Nav::Confirm => return self.press_kind(self.selected().kind),
            Nav::Back => return Some(KeyboardResult::Cancel),
        }
        None
    }

    /// Button shortcuts.
    pub fn shortcut(&mut self, shortcut: Shortcut) -> Option<KeyboardResult> {
        match shortcut {
            Shortcut::Backspace => self.backspace(),
            Shortcut::Space => self.type_char(' '),
            Shortcut::Shift => self.shift = !self.shift,
            Shortcut::Enter => return Some(KeyboardResult::Done(self.text.clone())),
            Shortcut::CursorLeft => self.move_cursor(-1),
            Shortcut::CursorRight => self.move_cursor(1),
        }
        None
    }

    /// A tap or click at (`x`, `y`) on a `width`×`height` screen: selects and
    /// presses the key under it. Taps elsewhere do nothing.
    pub fn tap(&mut self, x: f32, y: f32, width: f32, height: f32) -> Option<KeyboardResult> {
        let geo = Geometry::new(width, height);
        for (r, row) in rows().iter().enumerate() {
            for (c, key) in row.iter().enumerate() {
                if geo.key_rect(r, key).contains(x, y) {
                    self.row = r;
                    self.col = c;
                    return self.press_kind(key.kind);
                }
            }
        }
        None
    }
}

/// Screen positions shared by drawing and touch.
struct Geometry {
    s: f32,
    dock: Rect,
    field: Rect,
    left: f32,
    unit: f32,
    keys_top: f32,
    key_h: f32,
}

impl Geometry {
    fn new(width: f32, height: f32) -> Self {
        let s = height / REFERENCE_HEIGHT;
        let pad = 12.0 * s;
        let key_h = 60.0 * s;
        let dock_h = 112.0 * s + 5.0 * key_h + 46.0 * s;
        let dock = Rect {
            x: 0.0,
            y: height - dock_h,
            w: width,
            h: dock_h,
        };
        let field = Rect {
            x: pad + 4.0 * s,
            y: dock.y + 44.0 * s,
            w: width - 2.0 * pad - 8.0 * s,
            h: 54.0 * s,
        };
        Self {
            s,
            dock,
            field,
            left: pad,
            unit: (width - 2.0 * pad) / ROW_UNITS,
            keys_top: dock.y + 112.0 * s,
            key_h,
        }
    }

    fn key_rect(&self, row: usize, key: &KeyDef) -> Rect {
        let gap = 6.0 * self.s;
        Rect {
            x: self.left + key.x * self.unit + gap / 2.0,
            y: self.keys_top + row as f32 * self.key_h + gap / 2.0,
            w: key.w * self.unit - gap,
            h: self.key_h - gap,
        }
    }
}

use crate::theme::{
    ACCENT as KEY_SELECTED, ACCENT_DARK as KEY_ACTIVE, PANEL_DEEP, SURFACE as KEY,
    SURFACE_DARK as KEY_SPECIAL, TEXT as WHITE, TEXT_DIM as DIM,
};

/// Dock behind the keys.
const DOCK: [f32; 4] = crate::theme::hex(0x171b21, 1.0);

/// The keyboard, docked at the bottom of the screen.
pub fn keyboard_layer(kb: &Keyboard, labels: &KeyboardLabels, width: f32, height: f32) -> Layer {
    let geo = Geometry::new(width, height);
    let s = geo.s;
    let mut layer = Layer::default();
    // Light dim over the page above, like SteamOS.
    layer.quads.push(Quad {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            w: width,
            h: geo.dock.y,
        },
        color: [0.0, 0.0, 0.0, 0.45],
        radius: 0.0,
    });
    layer.quads.push(Quad {
        rect: geo.dock,
        color: DOCK,
        radius: 0.0,
    });

    // Title and text field with the cursor.
    layer.texts.push(Text {
        text: kb.title.clone(),
        x: geo.field.x + 4.0 * s,
        y: geo.dock.y + 12.0 * s,
        size: 19.0 * s,
        color: DIM,
        bold: false,
        align: Align::Left,
        max_width: geo.field.w,
    });
    layer.quads.push(Quad {
        rect: geo.field,
        color: PANEL_DEEP,
        radius: 8.0 * s,
    });
    let split = kb.byte_index(kb.cursor);
    layer.texts.push(Text {
        text: format!("{}|{}", &kb.text[..split], &kb.text[split..]),
        x: geo.field.x + 16.0 * s,
        y: geo.field.y + 13.0 * s,
        size: 24.0 * s,
        color: WHITE,
        bold: false,
        align: Align::Left,
        max_width: geo.field.w - 32.0 * s,
    });

    let selected = (kb.row, kb.col);
    for (r, row) in rows().iter().enumerate() {
        for (c, key) in row.iter().enumerate() {
            let rect = geo.key_rect(r, key);
            let (label, special) = match key.kind {
                KeyKind::Char(i) => (
                    kb.char_at(r, i).map(String::from).unwrap_or_default(),
                    false,
                ),
                KeyKind::Backspace => (labels.delete.clone(), true),
                KeyKind::Caps => (labels.caps.clone(), true),
                KeyKind::Enter => (labels.enter.clone(), true),
                KeyKind::Shift => (labels.shift.clone(), true),
                KeyKind::Symbols => (
                    if kb.accents {
                        labels.letters.clone()
                    } else {
                        labels.symbols.clone()
                    },
                    true,
                ),
                KeyKind::Space => (labels.space.clone(), false),
                KeyKind::Left => ("←".to_owned(), true),
                KeyKind::Right => ("→".to_owned(), true),
                KeyKind::Hide => (labels.hide.clone(), true),
            };
            let active = match key.kind {
                KeyKind::Shift => kb.shift,
                KeyKind::Caps => kb.caps,
                _ => false,
            };
            let is_selected = selected == (r, c);
            let color = if is_selected {
                KEY_SELECTED
            } else if active {
                KEY_ACTIVE
            } else if special {
                KEY_SPECIAL
            } else {
                KEY
            };
            layer.quads.push(Quad {
                rect,
                color,
                radius: 6.0 * s,
            });
            let size = if special { 18.0 } else { 24.0 } * s;
            layer.texts.push(Text {
                text: label,
                x: rect.x + rect.w / 2.0,
                y: rect.y + rect.h / 2.0 - size * 0.62,
                size,
                color: WHITE,
                bold: false,
                align: Align::Center,
                max_width: rect.w,
            });
        }
    }

    hint_bar(
        &mut layer,
        &[
            ("X", &labels.delete),
            ("Y", &labels.space),
            ("L2", &labels.shift),
            ("R2", &labels.enter),
            ("B", &labels.hide),
        ],
        width - 24.0 * s,
        geo.dock.y + geo.dock.h - 23.0 * s,
        s,
    );
    layer
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Moves to the key that types `target` (switching Shift/page if needed)
    /// and presses it with A, like a player would.
    fn press_char(kb: &mut Keyboard, target: char) {
        for attempt in 0..4 {
            for (r, row) in rows().iter().enumerate() {
                for (c, key) in row.iter().enumerate() {
                    if let KeyKind::Char(i) = key.kind
                        && kb.char_at(r, i) == Some(target)
                    {
                        go(kb, r, c);
                        kb.handle(Nav::Confirm);
                        return;
                    }
                }
            }
            // Not visible: try Shift, then the other page.
            if attempt % 2 == 0 {
                kb.shortcut(Shortcut::Shift);
            } else {
                kb.shortcut(Shortcut::Shift);
                go_kind(kb, KeyKind::Symbols);
                kb.handle(Nav::Confirm);
            }
        }
        panic!("character {target:?} not on the keyboard");
    }

    fn go(kb: &mut Keyboard, r: usize, c: usize) {
        while kb.row != r {
            kb.handle(Nav::Down);
        }
        while kb.col != c {
            kb.handle(Nav::Right);
        }
    }

    fn go_kind(kb: &mut Keyboard, kind: KeyKind) {
        let (r, c) = rows()
            .iter()
            .enumerate()
            .find_map(|(r, row)| row.iter().position(|k| k.kind == kind).map(|c| (r, c)))
            .unwrap();
        go(kb, r, c);
    }

    #[test]
    fn rows_fill_the_width() {
        for row in rows() {
            let last = row.last().unwrap();
            assert!((last.x + last.w - ROW_UNITS).abs() < 1e-4, "{row:?}");
        }
        for page in [LETTERS, ACCENTS] {
            for (r, (normal, shifted)) in page.iter().enumerate() {
                let keys = rows()[r]
                    .iter()
                    .filter(|k| matches!(k.kind, KeyKind::Char(_)))
                    .count();
                assert_eq!(normal.chars().count(), keys, "row {r}");
                assert_eq!(shifted.chars().count(), keys, "row {r}");
            }
        }
    }

    #[test]
    fn types_with_the_dpad() {
        let mut kb = Keyboard::new("Password");
        for c in "aB3!è".chars() {
            press_char(&mut kb, c);
        }
        assert_eq!(kb.text, "aB3!è");
        go_kind(&mut kb, KeyKind::Backspace);
        kb.handle(Nav::Confirm);
        go_kind(&mut kb, KeyKind::Space);
        kb.handle(Nav::Confirm);
        assert_eq!(kb.text, "aB3! ");
        go_kind(&mut kb, KeyKind::Enter);
        assert_eq!(
            kb.handle(Nav::Confirm),
            Some(KeyboardResult::Done("aB3! ".into()))
        );
    }

    #[test]
    fn shift_is_one_shot_and_caps_sticks() {
        let mut kb = Keyboard::new("x");
        kb.shortcut(Shortcut::Shift);
        press_char(&mut kb, 'Q');
        press_char(&mut kb, 'q');
        assert!(!kb.shift());
        go_kind(&mut kb, KeyKind::Caps);
        kb.handle(Nav::Confirm);
        assert!(kb.caps());
        // Caps Lock: letters upper case, digits unchanged.
        go(&mut kb, 1, 0);
        kb.handle(Nav::Confirm);
        go(&mut kb, 0, 1);
        kb.handle(Nav::Confirm);
        assert_eq!(kb.text, "QqQ1");
    }

    #[test]
    fn shortcuts_and_cursor() {
        let mut kb = Keyboard::new("x");
        for c in "abc".chars() {
            kb.type_char(c);
        }
        kb.shortcut(Shortcut::CursorLeft);
        kb.shortcut(Shortcut::CursorLeft);
        kb.shortcut(Shortcut::Space);
        assert_eq!(kb.text, "a bc");
        kb.shortcut(Shortcut::Backspace);
        kb.shortcut(Shortcut::Backspace);
        assert_eq!((kb.text.as_str(), kb.cursor()), ("bc", 0));
        kb.shortcut(Shortcut::Backspace);
        assert_eq!(kb.text, "bc", "nothing before the cursor");
        for _ in 0..5 {
            kb.shortcut(Shortcut::CursorRight);
        }
        assert_eq!(kb.cursor(), 2);
        kb.type_char('é');
        kb.shortcut(Shortcut::CursorLeft);
        kb.type_char('x');
        assert_eq!(kb.text, "bcxé", "multi-byte characters");
        assert_eq!(
            kb.shortcut(Shortcut::Enter),
            Some(KeyboardResult::Done("bcxé".into()))
        );
    }

    #[test]
    fn moves_between_rows_by_position() {
        let mut kb = Keyboard::new("x");
        // 'q' sits half a key right of '1' (indented row): up lands on '2'.
        kb.handle(Nav::Up);
        assert_eq!(kb.char_at(0, kb.col), Some('2'));
        // Down from 'g' area goes through the rows to the wide Space key.
        let mut kb = Keyboard::new("x");
        go(&mut kb, 2, 6); // 'g'
        kb.handle(Nav::Down);
        kb.handle(Nav::Down);
        assert_eq!(kb.selected().kind, KeyKind::Space);
        kb.handle(Nav::Down);
        assert_eq!(kb.row, 0, "wraps to the top row");
        assert_eq!(kb.handle(Nav::Back), Some(KeyboardResult::Cancel));
    }

    #[test]
    fn taps_press_the_key_under_the_finger() {
        let (w, h) = (1280.0, 800.0);
        let geo = Geometry::new(w, h);
        let mut kb = Keyboard::new("x");
        let center = |r: usize, kind: KeyKind| {
            let key = rows()[r].iter().find(|k| k.kind == kind).copied().unwrap();
            let rect = geo.key_rect(r, &key);
            (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0)
        };
        let (x, y) = center(2, KeyKind::Char(0)); // 'a'
        kb.tap(x, y, w, h);
        let (x, y) = center(3, KeyKind::Shift);
        kb.tap(x, y, w, h);
        let (x, y) = center(3, KeyKind::Char(0)); // 'Z'
        kb.tap(x, y, w, h);
        assert_eq!(kb.text, "aZ");
        assert_eq!(kb.tap(5.0, 5.0, w, h), None, "outside the keyboard");
        assert_eq!(kb.text, "aZ");
        let (x, y) = center(4, KeyKind::Hide);
        assert_eq!(kb.tap(x, y, w, h), Some(KeyboardResult::Cancel));
    }

    #[test]
    fn layer_fits_the_screen() {
        let kb = Keyboard::new("Password for Casa");
        let layer = keyboard_layer(&kb, &KeyboardLabels::default(), 1280.0, 800.0);
        for q in &layer.quads {
            assert!(
                q.rect.y >= 0.0 && q.rect.y + q.rect.h <= 800.01,
                "{:?}",
                q.rect
            );
        }
        let dock = Geometry::new(1280.0, 800.0).dock;
        assert!(dock.h < 800.0 * 0.6, "leaves the page title visible");
    }
}
