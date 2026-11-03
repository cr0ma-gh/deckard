//! "Install to disk" dialog: pure state machine and its drawing.
//!
//! Flow: choose a disk → first confirmation → last confirmation → progress →
//! done (restart) or failed. Erasing a disk always takes two explicit
//! confirms; Back leaves at any step before the installation starts.

use crate::i18n::{Strings, fill};
use crate::layout::{Align, Layer, Quad, REFERENCE_HEIGHT, Rect, Text, hint_bar};
use crate::nav::Nav;

/// A disk offered by `deckard-install list`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskEntry {
    pub path: String,
    pub bytes: u64,
    pub removable: bool,
    pub model: String,
}

impl DiskEntry {
    /// "Samsung SSD (512 GB, internal)"
    pub fn describe(&self, strings: &Strings) -> String {
        let kind = if self.removable {
            &strings.install_removable
        } else {
            &strings.install_internal
        };
        format!("{} ({}, {kind})", self.model, format_size(self.bytes))
    }
}

/// Decimal size as disk makers print it: "512 GB", "1.0 TB", "64 GB".
pub fn format_size(bytes: u64) -> String {
    let gb = bytes as f64 / 1e9;
    if gb >= 1000.0 {
        format!("{:.1} TB", gb / 1000.0)
    } else {
        format!("{gb:.0} GB")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InstallerState {
    Choose {
        selected: usize,
    },
    /// `last` is false on the first confirmation, true on the second.
    Confirm {
        disk: usize,
        last: bool,
    },
    Running {
        percent: u8,
        step: String,
    },
    Done,
    Failed(String),
}

/// What the application must do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallerAction {
    /// Run the installer on this disk path (both confirmations given).
    Start(String),
    Close,
    Reboot,
}

/// Messages from the installer process (parsed by `deckard-system`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallerEvent {
    Progress(u8, String),
    Done,
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Installer {
    pub disks: Vec<DiskEntry>,
    pub state: InstallerState,
}

impl Installer {
    pub fn new(disks: Vec<DiskEntry>) -> Self {
        Self {
            disks,
            state: InstallerState::Choose { selected: 0 },
        }
    }

    /// True while the installation runs: the dialog must not be closed.
    pub fn busy(&self) -> bool {
        matches!(self.state, InstallerState::Running { .. })
    }

    pub fn handle(&mut self, nav: Nav) -> Option<InstallerAction> {
        let count = self.disks.len();
        match (&mut self.state, nav) {
            (InstallerState::Choose { .. }, Nav::Back) => return Some(InstallerAction::Close),
            (InstallerState::Choose { .. }, Nav::Confirm) if count == 0 => {
                return Some(InstallerAction::Close);
            }
            (InstallerState::Choose { selected }, Nav::Up) => {
                *selected = selected.saturating_sub(1)
            }
            (InstallerState::Choose { selected }, Nav::Down) => {
                *selected = (*selected + 1).min(count.saturating_sub(1));
            }
            (InstallerState::Choose { selected }, Nav::Confirm) => {
                self.state = InstallerState::Confirm {
                    disk: *selected,
                    last: false,
                };
            }
            (InstallerState::Confirm { disk, .. }, Nav::Back) => {
                self.state = InstallerState::Choose { selected: *disk };
            }
            (InstallerState::Confirm { disk, last: false }, Nav::Confirm) => {
                self.state = InstallerState::Confirm {
                    disk: *disk,
                    last: true,
                };
            }
            (InstallerState::Confirm { disk, last: true }, Nav::Confirm) => {
                let path = self.disks.get(*disk)?.path.clone();
                self.state = InstallerState::Running {
                    percent: 0,
                    step: String::new(),
                };
                return Some(InstallerAction::Start(path));
            }
            (InstallerState::Done, Nav::Confirm) => return Some(InstallerAction::Reboot),
            (InstallerState::Done | InstallerState::Failed(_), Nav::Back | Nav::Confirm) => {
                return Some(InstallerAction::Close);
            }
            _ => {}
        }
        None
    }

    /// Applies a message from the running installer.
    pub fn update(&mut self, event: InstallerEvent) {
        self.state = match event {
            InstallerEvent::Progress(percent, step) => InstallerState::Running {
                percent: percent.min(100),
                step,
            },
            InstallerEvent::Done => InstallerState::Done,
            InstallerEvent::Error(message) => InstallerState::Failed(message),
        };
    }
}

const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const DIM: [f32; 4] = [0.80, 0.85, 0.95, 0.85];
const DANGER: [f32; 4] = [1.0, 0.45, 0.40, 1.0];

fn text(
    text: impl Into<String>,
    x: f32,
    y: f32,
    size: f32,
    color: [f32; 4],
    bold: bool,
    max_width: f32,
) -> Text {
    Text {
        text: text.into(),
        x,
        y,
        size,
        color,
        bold,
        align: Align::Left,
        max_width,
    }
}

/// The dialog, drawn over the menu.
pub fn installer_layer(installer: &Installer, strings: &Strings, width: f32, height: f32) -> Layer {
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
    let panel = Rect::centered(
        width / 2.0,
        height / 2.0,
        (760.0 * s).min(width - 40.0 * s),
        520.0 * s,
    );
    layer.quads.push(Quad {
        rect: panel,
        // Opaque: on sRGB surfaces even 3% transparency lets the menu show through.
        color: [0.05, 0.07, 0.15, 1.0],
        radius: 18.0 * s,
    });
    let x = panel.x + 40.0 * s;
    let w = panel.w - 80.0 * s;
    let mut y = panel.y + 34.0 * s;
    layer
        .texts
        .push(text(&strings.install_title, x, y, 30.0 * s, WHITE, true, w));
    y += 64.0 * s;

    let hints: Vec<(&str, &str)> = match &installer.state {
        InstallerState::Choose { selected } => {
            if installer.disks.is_empty() {
                layer
                    .texts
                    .push(text(&strings.install_none, x, y, 21.0 * s, DIM, false, w));
            } else {
                layer
                    .texts
                    .push(text(&strings.install_choose, x, y, 21.0 * s, DIM, false, w));
                y += 48.0 * s;
                for (i, disk) in installer.disks.iter().enumerate() {
                    if i == *selected {
                        layer.quads.push(Quad {
                            rect: Rect {
                                x: x - 16.0 * s,
                                y: y - 10.0 * s,
                                w: w + 32.0 * s,
                                h: 48.0 * s,
                            },
                            color: [0.40, 0.55, 1.0, 0.30],
                            radius: 12.0 * s,
                        });
                    }
                    let label = disk.describe(strings);
                    layer
                        .texts
                        .push(text(label, x, y, 21.0 * s, WHITE, i == *selected, w));
                    y += 56.0 * s;
                }
            }
            vec![
                ("A", strings.hint_confirm.as_str()),
                ("B", strings.hint_back.as_str()),
            ]
        }
        InstallerState::Confirm { disk, last } => {
            let name = installer
                .disks
                .get(*disk)
                .map(|d| d.describe(strings))
                .unwrap_or_default();
            let warning = fill(&strings.install_warning, &[("disk", &name)]);
            layer
                .texts
                .push(text(warning, x, y, 23.0 * s, DANGER, true, w));
            y += 60.0 * s;
            let ask = if *last {
                &strings.install_confirm_last
            } else {
                &strings.install_confirm_first
            };
            layer.texts.push(text(ask, x, y, 21.0 * s, WHITE, *last, w));
            vec![
                ("A", strings.hint_confirm.as_str()),
                ("B", strings.hint_back.as_str()),
            ]
        }
        InstallerState::Running { percent, step } => {
            layer.texts.push(text(
                &strings.install_running,
                x,
                y,
                22.0 * s,
                WHITE,
                false,
                w,
            ));
            y += 56.0 * s;
            let bar = Rect {
                x,
                y,
                w,
                h: 18.0 * s,
            };
            layer.quads.push(Quad {
                rect: bar,
                color: [1.0, 1.0, 1.0, 0.15],
                radius: 9.0 * s,
            });
            layer.quads.push(Quad {
                rect: Rect {
                    w: bar.w * f32::from(*percent) / 100.0,
                    ..bar
                },
                color: [0.40, 0.60, 1.0, 1.0],
                radius: 9.0 * s,
            });
            y += 40.0 * s;
            layer.texts.push(text(
                format!("{percent}%  {step}"),
                x,
                y,
                18.0 * s,
                DIM,
                false,
                w,
            ));
            Vec::new()
        }
        InstallerState::Done => {
            layer
                .texts
                .push(text(&strings.install_done, x, y, 22.0 * s, WHITE, false, w));
            vec![
                ("A", strings.install_reboot.as_str()),
                ("B", strings.hint_back.as_str()),
            ]
        }
        InstallerState::Failed(error) => {
            let msg = fill(&strings.install_failed, &[("error", error)]);
            layer
                .texts
                .push(text(msg, x, y, 21.0 * s, DANGER, false, w));
            vec![("B", strings.hint_back.as_str())]
        }
    };
    if !hints.is_empty() {
        hint_bar(
            &mut layer,
            &hints,
            panel.x + panel.w - 32.0 * s,
            panel.y + panel.h - 40.0 * s,
            s,
        );
    }
    layer
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disks() -> Vec<DiskEntry> {
        vec![
            DiskEntry {
                path: "/dev/nvme0n1".into(),
                bytes: 512_110_190_592,
                removable: false,
                model: "SSD".into(),
            },
            DiskEntry {
                path: "/dev/mmcblk0".into(),
                bytes: 127_865_454_592,
                removable: true,
                model: "SD".into(),
            },
        ]
    }

    #[test]
    fn needs_two_confirmations_before_starting() {
        let mut i = Installer::new(disks());
        i.handle(Nav::Down);
        assert_eq!(i.handle(Nav::Confirm), None);
        assert_eq!(
            i.state,
            InstallerState::Confirm {
                disk: 1,
                last: false
            }
        );
        assert_eq!(i.handle(Nav::Confirm), None);
        assert_eq!(
            i.state,
            InstallerState::Confirm {
                disk: 1,
                last: true
            }
        );
        assert_eq!(
            i.handle(Nav::Confirm),
            Some(InstallerAction::Start("/dev/mmcblk0".into()))
        );
        assert!(i.busy());
    }

    #[test]
    fn back_cancels_before_the_start() {
        let mut i = Installer::new(disks());
        i.handle(Nav::Confirm);
        i.handle(Nav::Confirm);
        i.handle(Nav::Back);
        assert_eq!(
            i.state,
            InstallerState::Choose { selected: 0 },
            "back from the last confirm"
        );
        assert_eq!(i.handle(Nav::Back), Some(InstallerAction::Close));
    }

    #[test]
    fn running_ignores_input_until_done() {
        let mut i = Installer::new(disks());
        for _ in 0..3 {
            i.handle(Nav::Confirm);
        }
        assert_eq!(i.handle(Nav::Back), None, "cannot leave while installing");
        i.update(InstallerEvent::Progress(42, "Copying".into()));
        assert_eq!(
            i.state,
            InstallerState::Running {
                percent: 42,
                step: "Copying".into()
            }
        );
        i.update(InstallerEvent::Done);
        assert_eq!(i.handle(Nav::Confirm), Some(InstallerAction::Reboot));

        let mut f = Installer::new(disks());
        f.update(InstallerEvent::Error("disk busy".into()));
        assert_eq!(f.handle(Nav::Confirm), Some(InstallerAction::Close));
    }

    #[test]
    fn no_disks_only_closes() {
        let mut i = Installer::new(Vec::new());
        assert_eq!(i.handle(Nav::Down), None);
        assert_eq!(i.handle(Nav::Confirm), Some(InstallerAction::Close));
    }

    #[test]
    fn sizes_and_descriptions() {
        assert_eq!(format_size(512_110_190_592), "512 GB");
        assert_eq!(format_size(2_000_398_934_016), "2.0 TB");
        let strings = Strings::load("it");
        assert_eq!(disks()[1].describe(&strings), "SD (128 GB, rimovibile)");
    }

    #[test]
    fn layout_shows_the_warning_with_the_disk_name() {
        let strings = Strings::load("it");
        let mut i = Installer::new(disks());
        i.handle(Nav::Confirm);
        let layer = installer_layer(&i, &strings, 1280.0, 800.0);
        assert!(
            layer
                .texts
                .iter()
                .any(|t| t.text.contains("TUTTI i dati su SSD (512 GB, interno)"))
        );
    }
}
