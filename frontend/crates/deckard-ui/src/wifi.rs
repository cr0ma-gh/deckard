//! Wi-Fi page: network list, password entry with the on-screen keyboard,
//! connection status. Pure state; the application runs the requests.

use crate::i18n::{Strings, fill};
use crate::keyboard::{Keyboard, KeyboardLabels, KeyboardResult, Shortcut, keyboard_layer};
use crate::layout::{Align, Layer, Quad, REFERENCE_HEIGHT, Rect, Text, hint_bar, scroll_start};
use crate::nav::Nav;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiNetwork {
    pub ssid: String,
    pub signal: u8,
    pub secured: bool,
    pub active: bool,
}

/// What the application must do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WifiAction {
    Scan,
    Connect {
        ssid: String,
        password: Option<String>,
    },
    Disconnect,
    Close,
}

/// A result coming back from the Wi-Fi backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WifiUpdate {
    Networks(Vec<WifiNetwork>),
    Connected(String),
    Disconnected,
    Error(String),
}

/// What is happening right now (shown under the title).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WifiStatus {
    Idle,
    Scanning,
    Connecting(String),
    Message(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct WifiPage {
    pub networks: Vec<WifiNetwork>,
    /// 0 = "Search again", 1.. = networks.
    selected: usize,
    pub status: WifiStatus,
    /// Password entry for this network, when open.
    keyboard: Option<(String, Keyboard)>,
}

impl WifiPage {
    /// A new page; the application should start a scan right away.
    pub fn new() -> Self {
        Self {
            networks: Vec::new(),
            selected: 0,
            status: WifiStatus::Scanning,
            keyboard: None,
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn keyboard_open(&self) -> bool {
        self.keyboard.is_some()
    }

    /// Text typed by a physical keyboard goes to the password field.
    pub fn type_char(&mut self, c: char) {
        if let Some((_, kb)) = &mut self.keyboard {
            kb.type_char(c);
        }
    }

    pub fn backspace(&mut self) {
        if let Some((_, kb)) = &mut self.keyboard {
            kb.backspace();
        }
    }

    /// Enter on a physical keyboard: connect with the typed password.
    pub fn submit(&mut self) -> Option<WifiAction> {
        let (ssid, kb) = self.keyboard.take()?;
        self.status = WifiStatus::Connecting(ssid.clone());
        Some(WifiAction::Connect {
            ssid,
            password: Some(kb.text),
        })
    }

    /// Escape on a physical keyboard: close the password entry.
    pub fn cancel_keyboard(&mut self) {
        self.keyboard = None;
    }

    /// A button shortcut (X, Y, L1/R1, L2, R2) on the open keyboard.
    pub fn keyboard_shortcut(&mut self, shortcut: Shortcut) -> Option<WifiAction> {
        let result = self.keyboard.as_mut()?.1.shortcut(shortcut);
        self.keyboard_result(result)
    }

    /// A tap or click on a `width`×`height` screen while the keyboard is open.
    pub fn keyboard_tap(&mut self, x: f32, y: f32, width: f32, height: f32) -> Option<WifiAction> {
        let result = self.keyboard.as_mut()?.1.tap(x, y, width, height);
        self.keyboard_result(result)
    }

    /// Done connects with the typed password; Cancel closes the keyboard.
    fn keyboard_result(&mut self, result: Option<KeyboardResult>) -> Option<WifiAction> {
        match result? {
            KeyboardResult::Done(password) => {
                let (ssid, _) = self.keyboard.take()?;
                self.status = WifiStatus::Connecting(ssid.clone());
                Some(WifiAction::Connect {
                    ssid,
                    password: Some(password),
                })
            }
            KeyboardResult::Cancel => {
                self.keyboard = None;
                None
            }
        }
    }

    fn busy(&self) -> bool {
        matches!(
            self.status,
            WifiStatus::Scanning | WifiStatus::Connecting(_)
        )
    }

    pub fn handle(&mut self, nav: Nav, strings: &Strings) -> Option<WifiAction> {
        if let Some((_, kb)) = &mut self.keyboard {
            let result = kb.handle(nav);
            return self.keyboard_result(result);
        }
        match nav {
            Nav::Back => return Some(WifiAction::Close),
            Nav::Up => self.selected = self.selected.saturating_sub(1),
            Nav::Down => self.selected = (self.selected + 1).min(self.networks.len()),
            Nav::Confirm if self.busy() => {}
            Nav::Confirm if self.selected == 0 => {
                self.status = WifiStatus::Scanning;
                return Some(WifiAction::Scan);
            }
            Nav::Confirm => {
                let network = self.networks.get(self.selected - 1)?.clone();
                if network.active {
                    self.status = WifiStatus::Scanning;
                    return Some(WifiAction::Disconnect);
                }
                if network.secured {
                    let title = fill(&strings.wifi_password, &[("ssid", &network.ssid)]);
                    self.keyboard = Some((network.ssid, Keyboard::new(title)));
                    return None;
                }
                self.status = WifiStatus::Connecting(network.ssid.clone());
                return Some(WifiAction::Connect {
                    ssid: network.ssid,
                    password: None,
                });
            }
            Nav::Left | Nav::Right => {}
        }
        None
    }

    /// Applies a backend result. Connecting or disconnecting returns
    /// `Some(WifiAction::Scan)` so the list (and the active network) refreshes.
    pub fn update(&mut self, update: WifiUpdate, strings: &Strings) -> Option<WifiAction> {
        match update {
            WifiUpdate::Networks(networks) => {
                self.networks = networks;
                self.selected = self.selected.min(self.networks.len());
                if self.status == WifiStatus::Scanning {
                    self.status = WifiStatus::Idle;
                }
                None
            }
            WifiUpdate::Connected(ssid) => {
                self.status =
                    WifiStatus::Message(fill(&strings.wifi_connected, &[("ssid", &ssid)]));
                Some(WifiAction::Scan)
            }
            WifiUpdate::Disconnected => {
                self.status = WifiStatus::Message(strings.wifi_disconnected.clone());
                Some(WifiAction::Scan)
            }
            WifiUpdate::Error(error) => {
                self.status = WifiStatus::Message(fill(&strings.wifi_failed, &[("error", &error)]));
                None
            }
        }
    }
}

impl Default for WifiPage {
    fn default() -> Self {
        Self::new()
    }
}

use crate::theme::{PANEL, SCRIM, TEXT as WHITE, TEXT_DIM as DIM};

/// A left-aligned line of text (use struct update syntax for other alignments).
fn line(text: String, x: f32, y: f32, size: f32, color: [f32; 4], bold: bool, w: f32) -> Text {
    Text {
        text,
        x,
        y,
        size,
        color,
        bold,
        align: Align::Left,
        max_width: w,
    }
}

/// The Wi-Fi page (and the keyboard, when open), drawn over the menu.
pub fn wifi_layers(page: &WifiPage, strings: &Strings, width: f32, height: f32) -> Vec<Layer> {
    let s = height / REFERENCE_HEIGHT;
    let mut layer = Layer::default();
    layer.quads.push(Quad {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            w: width,
            h: height,
        },
        color: SCRIM,
        radius: 0.0,
    });
    let panel = Rect::centered(
        width / 2.0,
        height / 2.0,
        (760.0 * s).min(width - 40.0 * s),
        620.0 * s,
    );
    layer.quads.push(Quad {
        rect: panel,
        color: PANEL,
        radius: 18.0 * s,
    });
    let x = panel.x + 40.0 * s;
    let w = panel.w - 80.0 * s;
    layer.texts.push(line(
        strings.wifi.clone(),
        x,
        panel.y + 30.0 * s,
        30.0 * s,
        WHITE,
        true,
        w,
    ));

    let status = match &page.status {
        WifiStatus::Idle => page
            .networks
            .iter()
            .find(|n| n.active)
            .map(|n| fill(&strings.wifi_connected, &[("ssid", &n.ssid)]))
            .unwrap_or_default(),
        WifiStatus::Scanning => strings.wifi_scanning.clone(),
        WifiStatus::Connecting(ssid) => fill(&strings.wifi_connecting, &[("ssid", ssid)]),
        WifiStatus::Message(m) => m.clone(),
    };
    layer
        .texts
        .push(line(status, x, panel.y + 80.0 * s, 19.0 * s, DIM, false, w));

    // Rows: "Search again", then the networks (or a "none found" line).
    let row_h = 56.0 * s;
    let top = panel.y + 130.0 * s;
    let visible = 7;
    let mut rows: Vec<(String, String, bool)> =
        vec![(strings.wifi_rescan.clone(), String::new(), false)];
    rows.extend(page.networks.iter().map(|n| {
        let kind = if n.active {
            &strings.wifi_active
        } else if n.secured {
            &strings.wifi_secured
        } else {
            &strings.wifi_open
        };
        (n.ssid.clone(), format!("{kind} · {}%", n.signal), n.active)
    }));
    let start = scroll_start(page.selected, rows.len(), visible);
    for (row, (i, (label, detail, active))) in rows
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .enumerate()
    {
        let y = top + row as f32 * row_h;
        let selected = i == page.selected;
        if selected {
            layer.quads.push(Quad {
                rect: Rect {
                    x: x - 16.0 * s,
                    y: y - 8.0 * s,
                    w: w + 32.0 * s,
                    h: 46.0 * s,
                },
                color: crate::theme::ACCENT,
                radius: 12.0 * s,
            });
        }
        layer.texts.push(line(
            label.clone(),
            x,
            y,
            21.0 * s,
            WHITE,
            selected || *active,
            w * 0.65,
        ));
        layer.texts.push(Text {
            align: Align::Right,
            ..line(
                detail.clone(),
                x + w,
                y + 2.0 * s,
                18.0 * s,
                DIM,
                false,
                w * 0.35,
            )
        });
    }
    if page.networks.is_empty() && page.status != WifiStatus::Scanning {
        layer.texts.push(line(
            strings.wifi_none.clone(),
            x,
            top + row_h,
            19.0 * s,
            DIM,
            false,
            w,
        ));
    }
    hint_bar(
        &mut layer,
        &[("A", &strings.hint_confirm), ("B", &strings.hint_back)],
        panel.x + panel.w - 32.0 * s,
        panel.y + panel.h - 40.0 * s,
        s,
    );

    let mut layers = vec![layer];
    if let Some((_, kb)) = &page.keyboard {
        let labels = KeyboardLabels {
            shift: strings.keyboard_shift.clone(),
            caps: strings.keyboard_caps.clone(),
            enter: strings.keyboard_enter.clone(),
            symbols: strings.keyboard_symbols.clone(),
            letters: strings.keyboard_letters.clone(),
            space: strings.keyboard_space.clone(),
            delete: strings.keyboard_delete.clone(),
            hide: strings.keyboard_hide.clone(),
        };
        layers.push(keyboard_layer(kb, &labels, width, height));
    }
    layers
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nets() -> Vec<WifiNetwork> {
        vec![
            WifiNetwork {
                ssid: "Casa".into(),
                signal: 80,
                secured: true,
                active: false,
            },
            WifiNetwork {
                ssid: "Ospiti".into(),
                signal: 40,
                secured: false,
                active: false,
            },
        ]
    }

    #[test]
    fn secured_network_asks_for_a_password() {
        let strings = Strings::load("it");
        let mut page = WifiPage::new();
        page.update(WifiUpdate::Networks(nets()), &strings);
        assert_eq!(page.status, WifiStatus::Idle);
        page.handle(Nav::Down, &strings);
        assert_eq!(page.handle(Nav::Confirm, &strings), None);
        assert!(page.keyboard_open());
        for c in "segreta".chars() {
            page.type_char(c);
        }
        // Y types a space, X deletes it, R2 confirms.
        page.keyboard_shortcut(Shortcut::Space);
        page.keyboard_shortcut(Shortcut::Backspace);
        let action = page.keyboard_shortcut(Shortcut::Enter);
        assert_eq!(
            action,
            Some(WifiAction::Connect {
                ssid: "Casa".into(),
                password: Some("segreta".into())
            })
        );
        assert_eq!(page.status, WifiStatus::Connecting("Casa".into()));
        assert!(!page.keyboard_open());
        assert_eq!(page.keyboard_shortcut(Shortcut::Enter), None, "no keyboard");
        assert_eq!(page.keyboard_tap(100.0, 700.0, 1280.0, 800.0), None);

        // B closes the keyboard without connecting.
        page.update(WifiUpdate::Error("wrong password".into()), &strings);
        page.handle(Nav::Confirm, &strings);
        assert!(page.keyboard_open());
        assert_eq!(page.handle(Nav::Back, &strings), None);
        assert!(!page.keyboard_open());
    }

    #[test]
    fn open_network_connects_directly_and_refreshes() {
        let strings = Strings::load("en");
        let mut page = WifiPage::new();
        page.update(WifiUpdate::Networks(nets()), &strings);
        page.handle(Nav::Down, &strings);
        page.handle(Nav::Down, &strings);
        assert_eq!(
            page.handle(Nav::Confirm, &strings),
            Some(WifiAction::Connect {
                ssid: "Ospiti".into(),
                password: None
            })
        );
        assert_eq!(
            page.handle(Nav::Confirm, &strings),
            None,
            "busy while connecting"
        );
        assert_eq!(
            page.update(WifiUpdate::Connected("Ospiti".into()), &strings),
            Some(WifiAction::Scan)
        );
        assert!(matches!(page.status, WifiStatus::Message(ref m) if m.contains("Ospiti")));
    }

    #[test]
    fn active_network_disconnects_and_errors_are_shown() {
        let strings = Strings::load("en");
        let mut page = WifiPage::new();
        let mut list = nets();
        list[0].active = true;
        page.update(WifiUpdate::Networks(list), &strings);
        page.handle(Nav::Down, &strings);
        assert_eq!(
            page.handle(Nav::Confirm, &strings),
            Some(WifiAction::Disconnect)
        );
        page.update(WifiUpdate::Error("no secrets".into()), &strings);
        assert!(matches!(page.status, WifiStatus::Message(ref m) if m.contains("no secrets")));
        assert_eq!(page.handle(Nav::Back, &strings), Some(WifiAction::Close));
    }

    #[test]
    fn layout_shows_networks_and_keyboard() {
        let strings = Strings::load("it");
        let mut page = WifiPage::new();
        page.update(WifiUpdate::Networks(nets()), &strings);
        let layers = wifi_layers(&page, &strings, 1280.0, 800.0);
        assert_eq!(layers.len(), 1);
        assert!(layers[0].texts.iter().any(|t| t.text == "Casa"));
        assert!(layers[0].texts.iter().any(|t| t.text == "protetta · 80%"));
        page.handle(Nav::Down, &strings);
        page.handle(Nav::Confirm, &strings);
        assert_eq!(
            wifi_layers(&page, &strings, 1280.0, 800.0).len(),
            2,
            "keyboard on top"
        );
    }
}
