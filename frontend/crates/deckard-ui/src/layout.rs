//! Turns the menu state into a [`Scene`]: positioned shapes, text and images.
//!
//! Geometry is designed for 1280x800 (the Deck's landscape resolution) and
//! scaled with the window height, so the menu looks the same at any size.

use std::path::PathBuf;
use std::time::Duration;

use crate::i18n::Strings;
use crate::nav::{Badge, Menu};
use crate::quick::QuickMenu;

/// Reference height the layout constants are written for.
pub const REFERENCE_HEIGHT: f32 = 800.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn centered(cx: f32, cy: f32, w: f32, h: f32) -> Self {
        Self {
            x: cx - w / 2.0,
            y: cy - h / 2.0,
            w,
            h,
        }
    }
}

/// RGBA, 0..1, not premultiplied.
pub type Color = [f32; 4];

/// Horizontal anchor of a text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    /// `x` is the right edge.
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub text: String,
    /// Left edge (Align::Left) or center (Align::Center).
    pub x: f32,
    /// Top of the line box.
    pub y: f32,
    pub size: f32,
    pub color: Color,
    pub bold: bool,
    pub align: Align,
    /// Text wider than this is clipped.
    pub max_width: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub path: PathBuf,
    /// The image is scaled to fit inside this box, keeping its aspect ratio.
    pub fit: Rect,
    pub alpha: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Quad {
    pub rect: Rect,
    pub color: Color,
    pub radius: f32,
}

/// One drawing layer: quads, then images, then text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layer {
    pub quads: Vec<Quad>,
    pub images: Vec<Image>,
    pub texts: Vec<Text>,
}

/// Everything to draw this frame, back to front.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    /// Seconds since start, drives the animated background.
    pub time: f32,
    pub layers: Vec<Layer>,
}

/// A value that eases towards a target (frame-rate independent).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Smooth {
    pub value: f32,
    pub target: f32,
}

impl Smooth {
    pub fn new(value: f32) -> Self {
        Self {
            value,
            target: value,
        }
    }

    /// Moves towards the target; `speed` ~ 1/time-constant in seconds.
    pub fn update(&mut self, dt: Duration, speed: f32) {
        let k = 1.0 - (-dt.as_secs_f32() * speed).exp();
        self.value += (self.target - self.value) * k;
        if (self.target - self.value).abs() < 0.001 {
            self.value = self.target;
        }
    }
}

/// A short message at the bottom of the screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub text: String,
    pub remaining: Duration,
}

impl Toast {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            remaining: TOAST_TIME,
        }
    }

    /// Advances time; returns false once the toast has expired.
    pub fn update(&mut self, dt: Duration) -> bool {
        self.remaining = self.remaining.saturating_sub(dt);
        !self.remaining.is_zero()
    }
}

/// Animation state of the menu (what is on screen, as opposed to the target selection).
#[derive(Debug, Clone)]
pub struct MenuView {
    category: Smooth,
    items: Vec<Smooth>,
    toast: Option<Toast>,
    time: f32,
}

const SPEED: f32 = 14.0;
const TOAST_TIME: Duration = Duration::from_secs(4);

impl MenuView {
    pub fn new(menu: &Menu) -> Self {
        let mut view = Self {
            category: Smooth::new(menu.selected_category() as f32),
            items: Vec::new(),
            toast: None,
            time: 0.0,
        };
        view.sync(menu, true);
        view
    }

    /// Points animations at the current selection; `snap` jumps there.
    fn sync(&mut self, menu: &Menu, snap: bool) {
        self.items.resize(menu.categories.len(), Smooth::new(0.0));
        self.category.target = menu.selected_category() as f32;
        for (i, item) in self.items.iter_mut().enumerate() {
            item.target = menu.selected_item_of(i) as f32;
        }
        if snap {
            self.category.value = self.category.target;
            for item in &mut self.items {
                item.value = item.target;
            }
        }
    }

    pub fn show_toast(&mut self, text: impl Into<String>) {
        self.toast = Some(Toast::new(text));
    }

    /// Advances animations by `dt`.
    pub fn update(&mut self, menu: &Menu, dt: Duration) {
        self.sync(menu, false);
        self.category.update(dt, SPEED);
        for item in &mut self.items {
            item.update(dt, SPEED);
        }
        self.time += dt.as_secs_f32();
        if self.toast.as_mut().is_some_and(|t| !t.update(dt)) {
            self.toast = None;
        }
    }

    /// True while something is moving (the app can render less often when idle).
    pub fn animating(&self) -> bool {
        self.category.value != self.category.target
            || self.items.iter().any(|i| i.value != i.target)
            || self.toast.is_some()
    }

    pub fn category_position(&self) -> f32 {
        self.category.value
    }

    pub fn item_position(&self, category: usize) -> f32 {
        self.items.get(category).map_or(0.0, |s| s.value)
    }
}

fn rgba(rgb: [f32; 3], a: f32) -> Color {
    [rgb[0], rgb[1], rgb[2], a]
}

const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

/// Labels of the bottom hint bar.
#[derive(Debug, Clone)]
pub struct Hints {
    pub confirm: String,
    pub back: String,
}

/// Vertical position of item `i` given the animated selection `pos`.
/// The selected item sits below the icon row; earlier items move above it.
pub fn item_y(i: usize, pos: f32, s: f32) -> f32 {
    let row_above = ICON_Y - 112.0; // first slot above the category row
    let selected = SELECTED_Y;
    let d = i as f32 - pos;
    let y = if d >= 0.0 {
        selected + d * ITEM_SPACING
    } else if d >= -1.0 {
        // Continuous slide from the selected slot up past the icons.
        selected + (selected - row_above) * d
    } else {
        row_above + (d + 1.0) * ITEM_SPACING
    };
    y * s
}

const FOCAL_X: f32 = 0.24; // fraction of the width
const ICON_Y: f32 = 190.0;
const ICON_SPACING: f32 = 168.0;
const ICON_SIZE: f32 = 92.0;
const SELECTED_Y: f32 = 400.0;
const ITEM_SPACING: f32 = 66.0;

/// Builds the scene for a `width` x `height` window.
pub fn layout(menu: &Menu, view: &MenuView, hints: &Hints, width: f32, height: f32) -> Scene {
    let s = height / REFERENCE_HEIGHT;
    let focal = width * FOCAL_X;
    let mut main = Layer::default();
    let mut overlay = Layer::default();

    // Category icons.
    let cat_pos = view.category_position();
    for (i, cat) in menu.categories.iter().enumerate() {
        let d = i as f32 - cat_pos;
        let x = focal + d * ICON_SPACING * s;
        if x < -ICON_SPACING * s || x > width + ICON_SPACING * s {
            continue;
        }
        let closeness = (1.0 - d.abs()).clamp(0.0, 1.0);
        let size = (ICON_SIZE + 20.0 * closeness) * s;
        let alpha = 0.5 + 0.5 * closeness;
        let tile = Rect::centered(x, ICON_Y * s, size, size);
        main.quads.push(Quad {
            rect: tile,
            color: rgba(cat.color, 0.85 * alpha),
            radius: size * 0.22,
        });
        match &cat.badge {
            Badge::Text(label) => main.texts.push(Text {
                text: label.clone(),
                x,
                y: ICON_Y * s - 17.0 * s,
                size: 28.0 * s,
                color: rgba(WHITE, alpha),
                bold: true,
                align: Align::Center,
                max_width: size,
            }),
            Badge::Settings => {
                // Three bars: a simple "settings" glyph that needs no icon font.
                for k in -1..=1 {
                    main.quads.push(Quad {
                        rect: Rect::centered(
                            x,
                            ICON_Y * s + k as f32 * 14.0 * s,
                            size * 0.46,
                            6.0 * s,
                        ),
                        color: rgba(WHITE, alpha),
                        radius: 3.0 * s,
                    });
                }
            }
        }
        if closeness > 0.0 {
            main.texts.push(Text {
                text: cat.title.clone(),
                x,
                y: ICON_Y * s + size / 2.0 + 12.0 * s,
                size: 22.0 * s,
                color: rgba(WHITE, closeness),
                bold: false,
                align: Align::Center,
                max_width: ICON_SPACING * 1.6 * s,
            });
        }
    }

    // Items of the categories near the focus (crossfade while switching).
    let text_x = focal + 64.0 * s;
    let text_width = width * 0.56 - text_x;
    for (ci, cat) in menu.categories.iter().enumerate() {
        let visibility = (1.0 - (ci as f32 - cat_pos).abs()).clamp(0.0, 1.0);
        if visibility <= 0.0 {
            continue;
        }
        let pos = view.item_position(ci);
        let selected = menu.selected_item_of(ci);
        for (i, item) in cat.items.iter().enumerate() {
            let y = item_y(i, pos, s);
            if y < -60.0 * s || y > height + 60.0 * s {
                continue;
            }
            let is_selected = i == selected;
            let above = (i as f32) < pos - 0.5;
            let base = if is_selected {
                1.0
            } else if above {
                0.4
            } else {
                0.62
            };
            // Fade out items that would collide with the icon row.
            let row_gap = ((y - ICON_Y * s).abs() - 80.0 * s).clamp(0.0, 40.0 * s) / (40.0 * s);
            let alpha = base * visibility * row_gap;
            if alpha <= 0.01 {
                continue;
            }
            main.quads.push(Quad {
                rect: Rect::centered(focal, y, 40.0 * s, 40.0 * s),
                color: rgba(cat.color, 0.9 * alpha),
                radius: 10.0 * s,
            });
            main.texts.push(Text {
                text: item.label.clone(),
                x: text_x,
                y: y - if item.sublabel.is_some() { 26.0 } else { 15.0 } * s,
                size: if is_selected { 27.0 } else { 24.0 } * s,
                color: rgba(WHITE, alpha),
                bold: is_selected,
                align: Align::Left,
                max_width: text_width,
            });
            if let Some(sub) = &item.sublabel {
                main.texts.push(Text {
                    text: sub.clone(),
                    x: text_x,
                    y: y + 6.0 * s,
                    size: 16.0 * s,
                    color: [0.82, 0.86, 0.95, 0.8 * alpha],
                    bold: false,
                    align: Align::Left,
                    max_width: text_width,
                });
            }
        }
    }

    // Cover of the selected game.
    if let Some(thumb) = menu.selected_item().and_then(|i| i.thumbnail.clone()) {
        let cat = menu.selected_category() as f32;
        let alpha = (1.0 - (cat - cat_pos).abs() * 2.0).clamp(0.0, 1.0);
        if alpha > 0.0 {
            main.images.push(Image {
                path: thumb,
                fit: Rect {
                    x: width * 0.60,
                    y: 280.0 * s,
                    w: width * 0.34,
                    h: 420.0 * s,
                },
                alpha,
            });
        }
    }

    hint_bar(
        &mut overlay,
        &[("A", &hints.confirm), ("B", &hints.back)],
        width - 40.0 * s,
        height - 44.0 * s,
        s,
    );
    if let Some(toast) = &view.toast {
        toast_layer(&mut overlay, toast, width, height, s);
    }

    Scene {
        time: view.time,
        layers: vec![main, overlay],
    }
}

/// Button hints laid out right to left from `right`: `[("A", "Confirm"), …]`
/// is drawn as "A Confirm  B Back" with the first entry leftmost.
pub fn hint_bar(layer: &mut Layer, hints: &[(&str, &str)], right: f32, y: f32, s: f32) {
    let mut x = right;
    for (button, label) in hints.iter().rev() {
        let label_width = label.chars().count() as f32 * 10.5 * s;
        x -= label_width;
        layer.texts.push(Text {
            text: (*label).to_owned(),
            x,
            y: y - 11.0 * s,
            size: 18.0 * s,
            color: rgba(WHITE, 0.85),
            bold: false,
            align: Align::Left,
            max_width: label_width + 20.0 * s,
        });
        let badge_w = (button.chars().count() as f32 * 13.0 + 15.0).max(28.0) * s;
        x -= 10.0 * s + badge_w / 2.0;
        layer.quads.push(Quad {
            rect: Rect::centered(x, y, badge_w, 28.0 * s),
            color: [1.0, 1.0, 1.0, 0.85],
            radius: 14.0 * s,
        });
        layer.texts.push(Text {
            text: (*button).to_owned(),
            x,
            y: y - 10.0 * s,
            size: 16.0 * s,
            color: [0.10, 0.12, 0.20, 1.0],
            bold: true,
            align: Align::Center,
            max_width: badge_w,
        });
        x -= badge_w / 2.0 + 30.0 * s;
    }
}

/// A short message centered near the bottom.
pub fn toast_layer(layer: &mut Layer, toast: &Toast, width: f32, height: f32, s: f32) {
    let fade = (toast.remaining.as_secs_f32() / 0.4).min(1.0);
    let w = (toast.text.chars().count() as f32 * 11.0 + 60.0) * s;
    let rect = Rect::centered(
        width / 2.0,
        height - 120.0 * s,
        w.min(width - 40.0 * s),
        52.0 * s,
    );
    layer.quads.push(Quad {
        rect,
        color: [0.06, 0.07, 0.12, 0.85 * fade],
        radius: 14.0 * s,
    });
    layer.texts.push(Text {
        text: toast.text.clone(),
        x: width / 2.0,
        y: rect.y + 14.0 * s,
        size: 19.0 * s,
        color: rgba(WHITE, fade),
        bold: false,
        align: Align::Center,
        max_width: rect.w - 24.0 * s,
    });
}

/// Labels for the quick menu's hint bar.
#[derive(Debug, Clone)]
pub struct QuickHints {
    pub confirm: String,
    pub change: String,
    pub back: String,
}

const QUICK_LINE: f32 = 58.0;

/// Index of the first visible line so that `selected` stays on screen.
pub fn scroll_start(selected: usize, total: usize, visible: usize) -> usize {
    if total <= visible || visible == 0 {
        return 0;
    }
    selected.saturating_sub(visible / 2).min(total - visible)
}

/// What is drawn over a running game: the quick menu (if open) and a toast.
pub fn game_overlay(
    quick: Option<(&QuickMenu, &Strings)>,
    toast: Option<&Toast>,
    hints: &QuickHints,
    width: f32,
    height: f32,
) -> Scene {
    let s = height / REFERENCE_HEIGHT;
    let mut layer = Layer::default();
    if let Some((menu, strings)) = quick {
        layer.quads.push(Quad {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                w: width,
                h: height,
            },
            color: [0.0, 0.0, 0.0, 0.55],
            radius: 0.0,
        });
        let panel_w = (width * 0.42).max(500.0 * s).min(width);
        layer.quads.push(Quad {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                w: panel_w,
                h: height,
            },
            color: [0.05, 0.07, 0.15, 0.97],
            radius: 0.0,
        });
        layer.texts.push(Text {
            text: menu.page_title(strings),
            x: 56.0 * s,
            y: 44.0 * s,
            size: 30.0 * s,
            color: rgba(WHITE, 1.0),
            bold: true,
            align: Align::Left,
            max_width: panel_w - 112.0 * s,
        });

        let lines = menu.lines(strings);
        let top = 130.0 * s;
        let visible = ((height - top - 110.0 * s) / (QUICK_LINE * s))
            .floor()
            .max(1.0) as usize;
        let selected = menu.selected();
        let start = scroll_start(selected, lines.len(), visible);
        for (row, (i, line)) in lines
            .iter()
            .enumerate()
            .skip(start)
            .take(visible)
            .enumerate()
        {
            let y = top + row as f32 * QUICK_LINE * s;
            let is_selected = i == selected;
            if is_selected {
                layer.quads.push(Quad {
                    rect: Rect {
                        x: 32.0 * s,
                        y,
                        w: panel_w - 64.0 * s,
                        h: 48.0 * s,
                    },
                    color: [0.40, 0.55, 1.0, 0.30],
                    radius: 12.0 * s,
                });
            }
            let alpha = if is_selected { 1.0 } else { 0.75 };
            let value_width = line
                .value
                .as_ref()
                .map_or(0.0, |v| (v.chars().count() as f32 * 11.0 + 50.0) * s);
            layer.texts.push(Text {
                text: line.label.clone(),
                x: 56.0 * s,
                y: y + 11.0 * s,
                size: 21.0 * s,
                color: rgba(WHITE, alpha),
                bold: is_selected,
                align: Align::Left,
                max_width: panel_w - 112.0 * s - value_width,
            });
            if let Some(value) = &line.value {
                let text = if line.adjustable && is_selected {
                    format!("‹ {value} ›")
                } else {
                    value.clone()
                };
                layer.texts.push(Text {
                    text,
                    x: panel_w - 56.0 * s,
                    y: y + 12.0 * s,
                    size: 19.0 * s,
                    color: [0.75, 0.85, 1.0, alpha],
                    bold: false,
                    align: Align::Right,
                    max_width: value_width,
                });
            }
        }
        hint_bar(
            &mut layer,
            &[
                ("A", &hints.confirm),
                ("←→", &hints.change),
                ("B", &hints.back),
            ],
            panel_w - 32.0 * s,
            height - 44.0 * s,
            s,
        );
    }
    if let Some(toast) = toast {
        toast_layer(&mut layer, toast, width, height, s);
    }
    Scene {
        time: 0.0,
        layers: vec![layer],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::{Nav, SettingsView};
    use crate::quick::OptionEntry;
    use deckard_library::{Game, Library, SystemEntry};
    use std::path::Path;

    fn menu() -> Menu {
        let games = (0..5)
            .map(|i| Game {
                title: format!("Game {i}"),
                path: format!("g{i}.sfc").into(),
                thumbnail: (i == 0).then(|| PathBuf::from("cover.png")),
            })
            .collect();
        let lib = Library {
            systems: vec![SystemEntry {
                id: "snes".into(),
                name: "Super Nintendo".into(),
                core_path: "c.so".into(),
                core_available: true,
                games,
            }],
        };
        Menu::build(
            &lib,
            &Strings::load("en"),
            Path::new("roms"),
            &SettingsView::default(),
        )
    }

    fn hints() -> Hints {
        Hints {
            confirm: "Confirm".into(),
            back: "Back".into(),
        }
    }

    #[test]
    fn smooth_converges_without_overshoot() {
        let mut v = Smooth::new(0.0);
        v.target = 1.0;
        let mut last = 0.0;
        for _ in 0..120 {
            v.update(Duration::from_millis(16), SPEED);
            assert!(v.value >= last && v.value <= 1.0);
            last = v.value;
        }
        assert_eq!(v.value, 1.0, "snaps once close enough");
    }

    #[test]
    fn item_positions_are_continuous() {
        let s = 1.0;
        assert_eq!(item_y(3, 3.0, s), SELECTED_Y);
        assert_eq!(item_y(4, 3.0, s), SELECTED_Y + ITEM_SPACING);
        // Continuity at the two joints of the piecewise curve.
        for joint in [0.0f32, -1.0] {
            let i = 5usize;
            let pos = i as f32 - joint;
            let a = item_y(i, pos - 1e-4, s);
            let b = item_y(i, pos + 1e-4, s);
            assert!((a - b).abs() < 0.1, "jump at d={joint}: {a} vs {b}");
        }
        assert!(
            item_y(1, 3.0, s) < ICON_Y,
            "earlier items go above the icons"
        );
    }

    #[test]
    fn selected_category_sits_at_the_focal_point() {
        let m = menu();
        let view = MenuView::new(&m);
        let scene = layout(&m, &view, &hints(), 1280.0, 800.0);
        let title = scene.layers[0]
            .texts
            .iter()
            .find(|t| t.text == "Super Nintendo")
            .expect("category title");
        assert!((title.x - 1280.0 * FOCAL_X).abs() < 0.01);
        assert!(
            scene.layers[0]
                .images
                .iter()
                .any(|i| i.path == Path::new("cover.png"))
        );
    }

    #[test]
    fn scales_with_window_height() {
        let m = menu();
        let view = MenuView::new(&m);
        let small = layout(&m, &view, &hints(), 640.0, 400.0);
        let big = layout(&m, &view, &hints(), 1280.0, 800.0);
        let size = |s: &Scene| {
            s.layers[0]
                .texts
                .iter()
                .find(|t| t.text == "Game 0")
                .map(|t| t.size)
        };
        assert_eq!(size(&big), size(&small).map(|v| v * 2.0));
    }

    #[test]
    fn view_follows_selection_and_toast_expires() {
        let mut m = menu();
        let mut view = MenuView::new(&m);
        m.handle(Nav::Down);
        view.show_toast("hello");
        assert!(view.animating());
        for _ in 0..400 {
            view.update(&m, Duration::from_millis(16));
        }
        assert_eq!(view.item_position(1), 1.0);
        assert!(!view.animating(), "settled and toast gone");
        let scene = layout(&m, &view, &hints(), 1280.0, 800.0);
        assert!(scene.layers[1].texts.iter().all(|t| t.text != "hello"));
    }

    #[test]
    fn scroll_keeps_the_selection_visible() {
        assert_eq!(scroll_start(0, 5, 10), 0, "everything fits");
        assert_eq!(scroll_start(0, 30, 10), 0);
        assert_eq!(scroll_start(12, 30, 10), 7);
        assert_eq!(scroll_start(29, 30, 10), 20, "clamped at the end");
    }

    #[test]
    fn game_overlay_draws_quick_menu_and_toast() {
        let strings = Strings::load("it");
        let hints = QuickHints {
            confirm: "Conferma".into(),
            change: "Cambia".into(),
            back: "Indietro".into(),
        };
        let option = OptionEntry {
            key: "k".into(),
            label: "Velocità".into(),
            values: vec!["1".into(), "2".into()],
            current: 1,
        };
        let mut quick = QuickMenu::new("Mario", 2, "none", 100, None, vec![option]);
        let toast = Toast::new("Stato salvato");

        let scene = game_overlay(
            Some((&quick, &strings)),
            Some(&toast),
            &hints,
            1280.0,
            800.0,
        );
        let texts: Vec<_> = scene.layers[0]
            .texts
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert!(texts.contains(&"Mario"));
        assert!(texts.contains(&"Riprendi"));
        assert!(texts.contains(&"Slot di salvataggio: 2"));
        assert!(texts.contains(&"Stato salvato"));

        for _ in 0..6 {
            quick.handle(Nav::Down);
        }
        quick.handle(Nav::Confirm);
        let scene = game_overlay(Some((&quick, &strings)), None, &hints, 1280.0, 800.0);
        let value = scene.layers[0]
            .texts
            .iter()
            .find(|t| t.text.starts_with('‹'))
            .unwrap();
        assert_eq!(value.text, "‹ 2 ›");
        assert_eq!(value.align, Align::Right);

        let empty = game_overlay(None, None, &hints, 1280.0, 800.0);
        assert!(empty.layers[0].quads.is_empty(), "nothing over the game");
    }
}
