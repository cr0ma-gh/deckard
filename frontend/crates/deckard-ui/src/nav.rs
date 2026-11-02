//! Menu model and navigation: pure state, no drawing.
//!
//! Categories run horizontally (Settings first, then one per system with
//! games), items vertically. Each category remembers its own selected item.

use std::path::{Path, PathBuf};

use deckard_library::Library;

use crate::i18n::{Strings, fill};

/// One navigation step, produced by input (see [`crate::repeat`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
}

/// What the application must do after a confirm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Start `library.systems[system].games[game]`.
    Launch {
        system: usize,
        game: usize,
    },
    Rescan,
    Quit,
    /// Switch to another session (e.g. Steam Big Picture).
    SwitchSession,
    /// Flip or cycle a setting.
    Toggle(Setting),
    /// Show a short message (e.g. why a game cannot start).
    Notice(String),
}

/// Category icon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Badge {
    Settings,
    /// Short system label (e.g. "SNES") drawn on a colored tile.
    Text(String),
}

/// Settings changeable from the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    IntegerScale,
    Smooth,
    Shader,
    Language,
}

/// Current values of the settings shown in the menu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsView {
    pub integer_scale: bool,
    pub smooth: bool,
    pub shader: String,
    /// Whether the "Switch to Steam" item should appear.
    pub session_switching: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Stable identity (game path or setting name), kept when labels change.
    pub id: String,
    pub label: String,
    pub sublabel: Option<String>,
    pub thumbnail: Option<PathBuf>,
    pub action: Option<Action>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Category {
    /// Stable identity, kept across library rescans (system id or "settings").
    pub key: String,
    pub title: String,
    pub badge: Badge,
    /// Accent color of the category, RGB 0..1.
    pub color: [f32; 3],
    pub items: Vec<Item>,
}

/// The whole menu and its selection.
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub categories: Vec<Category>,
    selected_category: usize,
    selected_items: Vec<usize>,
}

impl Menu {
    /// Builds the menu from the library. Starts on the first system.
    pub fn build(
        library: &Library,
        strings: &Strings,
        roms_dir: &Path,
        settings: &SettingsView,
    ) -> Self {
        let setting = |id: &str, label: &str, value: &str, info: &str, s: Setting| Item {
            id: id.into(),
            label: format!("{label}: {value}"),
            sublabel: Some(info.to_owned()),
            thumbnail: None,
            action: Some(Action::Toggle(s)),
        };
        let mut categories = vec![Category {
            key: "settings".into(),
            title: strings.settings.clone(),
            badge: Badge::Settings,
            color: [0.55, 0.58, 0.66],
            items: vec![
                setting(
                    "integer_scale",
                    &strings.integer_scale,
                    strings.on_off(settings.integer_scale),
                    &strings.integer_scale_info,
                    Setting::IntegerScale,
                ),
                setting(
                    "smooth",
                    &strings.smooth_filter,
                    strings.on_off(settings.smooth),
                    &strings.smooth_filter_info,
                    Setting::Smooth,
                ),
                setting(
                    "shader",
                    &strings.shader,
                    strings.shader_label(&settings.shader),
                    &strings.shader_info,
                    Setting::Shader,
                ),
                setting(
                    "language",
                    &strings.language,
                    &strings.language_name,
                    &strings.language_info,
                    Setting::Language,
                ),
                Item {
                    id: "rescan".into(),
                    label: strings.rescan.clone(),
                    sublabel: Some(strings.rescan_info.clone()),
                    thumbnail: None,
                    action: Some(Action::Rescan),
                },
                Item {
                    id: "quit".into(),
                    label: strings.quit.clone(),
                    sublabel: Some(strings.quit_info.clone()),
                    thumbnail: None,
                    action: Some(Action::Quit),
                },
            ],
        }];

        if settings.session_switching {
            categories[0].items.push(Item {
                id: "switch_session".into(),
                label: strings.switch_to_steam.clone(),
                sublabel: Some(strings.switch_to_steam_info.clone()),
                thumbnail: None,
                action: Some(Action::SwitchSession),
            });
        }

        for (si, system) in library.systems.iter().enumerate() {
            if system.games.is_empty() {
                continue;
            }
            let core_file = system
                .core_path
                .file_name()
                .map_or_else(String::new, |f| f.to_string_lossy().into_owned());
            let items = system
                .games
                .iter()
                .enumerate()
                .map(|(gi, game)| {
                    let (sublabel, action) = if system.core_available {
                        (
                            None,
                            Action::Launch {
                                system: si,
                                game: gi,
                            },
                        )
                    } else {
                        let msg = fill(&strings.core_missing, &[("core", &core_file)]);
                        (Some(msg.clone()), Action::Notice(msg))
                    };
                    Item {
                        id: game.path.to_string_lossy().into_owned(),
                        label: game.title.clone(),
                        sublabel,
                        thumbnail: game.thumbnail.clone(),
                        action: Some(action),
                    }
                })
                .collect();
            categories.push(Category {
                key: system.id.clone(),
                title: system.name.clone(),
                badge: Badge::Text(badge_text(&system.id)),
                color: accent_color(&system.id),
                items,
            });
        }

        if categories.len() == 1 {
            let path = roms_dir.display().to_string();
            categories.push(Category {
                key: "games".into(),
                title: strings.games.clone(),
                badge: Badge::Text("ROM".into()),
                color: [0.35, 0.55, 0.85],
                items: vec![Item {
                    id: "no_games".into(),
                    label: strings.no_games.clone(),
                    sublabel: Some(fill(&strings.no_games_info, &[("path", &path)])),
                    thumbnail: None,
                    action: None,
                }],
            });
        }

        let selected_items = vec![0; categories.len()];
        Self {
            categories,
            selected_category: 1,
            selected_items,
        }
    }

    pub fn selected_category(&self) -> usize {
        self.selected_category
    }

    /// Selected item index of category `index`.
    pub fn selected_item_of(&self, index: usize) -> usize {
        self.selected_items.get(index).copied().unwrap_or(0)
    }

    pub fn selected_item(&self) -> Option<&Item> {
        let cat = self.categories.get(self.selected_category)?;
        cat.items.get(self.selected_item_of(self.selected_category))
    }

    /// Applies one navigation step; returns an action on confirm.
    pub fn handle(&mut self, nav: Nav) -> Option<Action> {
        let cat = self.selected_category;
        let count = self.categories.get(cat).map_or(0, |c| c.items.len());
        let item = &mut self.selected_items[cat];
        match nav {
            Nav::Left => self.selected_category = cat.saturating_sub(1),
            Nav::Right => self.selected_category = (cat + 1).min(self.categories.len() - 1),
            Nav::Up => *item = item.saturating_sub(1),
            Nav::Down => *item = (*item + 1).min(count.saturating_sub(1)),
            Nav::Back => *item = 0,
            Nav::Confirm => return self.selected_item().and_then(|i| i.action.clone()),
        }
        None
    }

    /// Replaces the content (after a rescan) keeping the selection where possible.
    pub fn replace(&mut self, mut new: Menu) {
        let current_key = self
            .categories
            .get(self.selected_category)
            .map(|c| c.key.clone());
        for (i, cat) in new.categories.iter().enumerate() {
            let old = self.categories.iter().position(|c| c.key == cat.key);
            if let Some(old) = old {
                let id = self.categories[old]
                    .items
                    .get(self.selected_items[old])
                    .map(|it| &it.id);
                new.selected_items[i] = id
                    .and_then(|id| cat.items.iter().position(|it| &it.id == id))
                    .unwrap_or(0);
            }
            if Some(&cat.key) == current_key.as_ref() {
                new.selected_category = i;
            }
        }
        *self = new;
    }
}

/// Short label for a system tile.
pub fn badge_text(system_id: &str) -> String {
    match system_id {
        "megadrive" | "genesis" => "MD".into(),
        "psx" => "PS1".into(),
        id => id.chars().take(4).collect::<String>().to_uppercase(),
    }
}

/// A stable, pleasant accent color per system id.
pub fn accent_color(system_id: &str) -> [f32; 3] {
    const PALETTE: [[f32; 3]; 8] = [
        [0.91, 0.30, 0.36],
        [0.36, 0.48, 0.95],
        [0.25, 0.75, 0.55],
        [0.95, 0.62, 0.22],
        [0.66, 0.40, 0.92],
        [0.20, 0.70, 0.85],
        [0.90, 0.40, 0.70],
        [0.55, 0.75, 0.30],
    ];
    let hash = system_id
        .bytes()
        .fold(5381u32, |h, b| h.wrapping_mul(33) ^ u32::from(b));
    PALETTE[hash as usize % PALETTE.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use deckard_library::{Game, SystemEntry};

    fn game(title: &str) -> Game {
        Game {
            title: title.into(),
            path: PathBuf::from(format!("{title}.sfc")),
            thumbnail: None,
        }
    }

    fn library() -> Library {
        Library {
            systems: vec![
                SystemEntry {
                    id: "snes".into(),
                    name: "Super Nintendo".into(),
                    core_path: "cores/snes9x_libretro.so".into(),
                    core_available: true,
                    games: vec![game("A"), game("B"), game("C")],
                },
                SystemEntry {
                    id: "nes".into(),
                    name: "NES".into(),
                    core_path: "cores/fceumm_libretro.so".into(),
                    core_available: true,
                    games: vec![],
                },
                SystemEntry {
                    id: "gba".into(),
                    name: "Game Boy Advance".into(),
                    core_path: "cores/mgba_libretro.so".into(),
                    core_available: false,
                    games: vec![game("Metroid")],
                },
            ],
        }
    }

    fn menu() -> Menu {
        Menu::build(
            &library(),
            &Strings::load("en"),
            Path::new("roms"),
            &SettingsView::default(),
        )
    }

    #[test]
    fn builds_settings_then_non_empty_systems() {
        let m = menu();
        let keys: Vec<_> = m.categories.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(keys, ["settings", "snes", "gba"]);
        assert_eq!(m.selected_category(), 1, "starts on the first system");
        assert_eq!(m.categories[1].badge, Badge::Text("SNES".into()));
    }

    #[test]
    fn navigation_is_clamped_and_per_category() {
        let mut m = menu();
        assert_eq!(m.handle(Nav::Down), None);
        m.handle(Nav::Down);
        m.handle(Nav::Down); // clamped at the last item
        assert_eq!(m.selected_item().map(|i| i.label.as_str()), Some("C"));

        m.handle(Nav::Right);
        m.handle(Nav::Right); // clamped at the last category
        assert_eq!(m.selected_category(), 2);
        m.handle(Nav::Left);
        assert_eq!(
            m.selected_item().map(|i| i.label.as_str()),
            Some("C"),
            "remembered"
        );

        m.handle(Nav::Back);
        assert_eq!(m.selected_item().map(|i| i.label.as_str()), Some("A"));
        m.handle(Nav::Up); // clamped at the first item
        m.handle(Nav::Left);
        m.handle(Nav::Left);
        assert_eq!(m.selected_category(), 0);
    }

    #[test]
    fn confirm_returns_actions() {
        let mut m = menu();
        m.handle(Nav::Down);
        assert_eq!(
            m.handle(Nav::Confirm),
            Some(Action::Launch { system: 0, game: 1 })
        );

        m.handle(Nav::Right); // GBA: core missing
        match m.handle(Nav::Confirm) {
            Some(Action::Notice(msg)) => assert!(msg.contains("mgba_libretro.so")),
            other => panic!("expected a notice, got {other:?}"),
        }

        m.handle(Nav::Left);
        m.handle(Nav::Left);
        let toggles = [
            Setting::IntegerScale,
            Setting::Smooth,
            Setting::Shader,
            Setting::Language,
        ];
        for (i, setting) in toggles.into_iter().enumerate() {
            if i > 0 {
                m.handle(Nav::Down);
            }
            assert_eq!(m.handle(Nav::Confirm), Some(Action::Toggle(setting)));
        }
        m.handle(Nav::Down);
        assert_eq!(m.handle(Nav::Confirm), Some(Action::Rescan));
        m.handle(Nav::Down);
        assert_eq!(m.handle(Nav::Confirm), Some(Action::Quit));
    }

    #[test]
    fn empty_library_shows_a_hint() {
        let m = Menu::build(
            &Library::default(),
            &Strings::load("en"),
            Path::new("/data/roms"),
            &SettingsView::default(),
        );
        assert_eq!(m.categories.len(), 2);
        let item = m.selected_item().unwrap();
        assert_eq!(item.label, "No games found");
        assert!(item.sublabel.as_deref().unwrap().contains("/data/roms"));
        assert_eq!(item.action, None);
    }

    #[test]
    fn replace_keeps_selection() {
        let mut m = menu();
        m.handle(Nav::Down);
        m.handle(Nav::Down); // "C"
        let mut lib = library();
        lib.systems[0].games.insert(0, game("0 New"));
        m.replace(Menu::build(
            &lib,
            &Strings::load("en"),
            Path::new("roms"),
            &SettingsView::default(),
        ));
        assert_eq!(m.selected_category(), 1);
        assert_eq!(m.selected_item().map(|i| i.label.as_str()), Some("C"));
    }

    #[test]
    fn settings_show_values_and_keep_selection_when_toggled() {
        let strings = Strings::load("it");
        let lib = library();
        let off = SettingsView::default();
        let mut m = Menu::build(&lib, &strings, Path::new("roms"), &off);
        m.handle(Nav::Left);
        m.handle(Nav::Down); // smooth filter
        assert_eq!(
            m.selected_item().map(|i| i.label.as_str()),
            Some("Filtro morbido: No")
        );

        let on = SettingsView {
            integer_scale: false,
            smooth: true,
            shader: "none".into(),
            session_switching: false,
        };
        m.replace(Menu::build(&lib, &strings, Path::new("roms"), &on));
        assert_eq!(m.selected_category(), 0);
        assert_eq!(
            m.selected_item().map(|i| i.label.as_str()),
            Some("Filtro morbido: Sì")
        );
    }

    #[test]
    fn session_switching_item_presence() {
        let strings = Strings::load("en");
        let lib = library();
        let without_switch =
            Menu::build(&lib, &strings, Path::new("roms"), &SettingsView::default());
        assert!(
            !without_switch.categories[0]
                .items
                .iter()
                .any(|i| i.id == "switch_session")
        );

        let with_switch = Menu::build(
            &lib,
            &strings,
            Path::new("roms"),
            &SettingsView {
                session_switching: true,
                ..Default::default()
            },
        );
        let switch_item = with_switch.categories[0]
            .items
            .iter()
            .find(|i| i.id == "switch_session");
        assert!(switch_item.is_some());
        assert_eq!(switch_item.unwrap().action, Some(Action::SwitchSession));
    }

    #[test]
    fn badges_and_colors_are_stable() {
        assert_eq!(badge_text("megadrive"), "MD");
        assert_eq!(badge_text("gbc"), "GBC");
        assert_eq!(badge_text("neogeo"), "NEOG");
        assert_eq!(accent_color("snes"), accent_color("snes"));
    }
}
