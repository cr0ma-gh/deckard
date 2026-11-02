//! In-game quick menu: pure state, no drawing.
//!
//! Opened with the Guide/Steam button or `F1` while a game runs (the game is
//! paused). A main page with game actions and a page listing the core options.

use crate::i18n::{Strings, fill};
use crate::nav::Nav;

/// A core option as shown in the quick menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionEntry {
    pub key: String,
    pub label: String,
    pub values: Vec<String>,
    /// Index of the current value in `values`.
    pub current: usize,
}

/// What the application must do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickAction {
    Resume,
    SaveState,
    LoadState,
    NextSlot,
    PreviousSlot,
    CycleShader,
    Reset,
    CloseGame,
    SetOption { key: String, value: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    Resume,
    Save,
    Load,
    Slot,
    Shader,
    Options,
    Reset,
    Close,
}

const MAIN: [Entry; 8] = [
    Entry::Resume,
    Entry::Save,
    Entry::Load,
    Entry::Slot,
    Entry::Shader,
    Entry::Options,
    Entry::Reset,
    Entry::Close,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Main,
    Options,
}

/// One line to draw: a label, an optional value on the right, and whether it
/// can be changed with left/right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub label: String,
    pub value: Option<String>,
    pub adjustable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickMenu {
    pub title: String,
    page: Page,
    main_selected: usize,
    option_selected: usize,
    slot: u8,
    shader: String,
    options: Vec<OptionEntry>,
}

impl QuickMenu {
    pub fn new(
        title: impl Into<String>,
        slot: u8,
        shader: impl Into<String>,
        options: Vec<OptionEntry>,
    ) -> Self {
        Self {
            title: title.into(),
            page: Page::Main,
            main_selected: 0,
            option_selected: 0,
            slot,
            shader: shader.into(),
            options,
        }
    }

    pub fn page(&self) -> Page {
        self.page
    }

    pub fn selected(&self) -> usize {
        match self.page {
            Page::Main => self.main_selected,
            Page::Options => self.option_selected,
        }
    }

    /// Keeps the shown slot in sync with the game's.
    pub fn set_slot(&mut self, slot: u8) {
        self.slot = slot;
    }

    /// Updates the current shader label.
    pub fn set_shader(&mut self, shader: impl Into<String>) {
        self.shader = shader.into();
    }

    /// Applies one navigation step.
    pub fn handle(&mut self, nav: Nav) -> Option<QuickAction> {
        match self.page {
            Page::Main => self.handle_main(nav),
            Page::Options => self.handle_options(nav),
        }
    }

    fn handle_main(&mut self, nav: Nav) -> Option<QuickAction> {
        let entry = MAIN[self.main_selected];
        match (nav, entry) {
            (Nav::Up, _) => self.main_selected = self.main_selected.saturating_sub(1),
            (Nav::Down, _) => self.main_selected = (self.main_selected + 1).min(MAIN.len() - 1),
            (Nav::Back, _) => return Some(QuickAction::Resume),
            (Nav::Right | Nav::Confirm, Entry::Slot) => return Some(QuickAction::NextSlot),
            (Nav::Left, Entry::Slot) => return Some(QuickAction::PreviousSlot),
            (Nav::Right | Nav::Left | Nav::Confirm, Entry::Shader) => {
                return Some(QuickAction::CycleShader);
            }
            (Nav::Left | Nav::Right, _) => {}
            (Nav::Confirm, Entry::Resume) => return Some(QuickAction::Resume),
            (Nav::Confirm, Entry::Save) => return Some(QuickAction::SaveState),
            (Nav::Confirm, Entry::Load) => return Some(QuickAction::LoadState),
            (Nav::Confirm, Entry::Reset) => return Some(QuickAction::Reset),
            (Nav::Confirm, Entry::Close) => return Some(QuickAction::CloseGame),
            (Nav::Confirm, Entry::Options) => {
                self.page = Page::Options;
                self.option_selected = 0;
            }
        }
        None
    }

    fn handle_options(&mut self, nav: Nav) -> Option<QuickAction> {
        let count = self.options.len();
        match nav {
            Nav::Back => self.page = Page::Main,
            Nav::Up => self.option_selected = self.option_selected.saturating_sub(1),
            Nav::Down => {
                self.option_selected = (self.option_selected + 1).min(count.saturating_sub(1))
            }
            Nav::Left | Nav::Right | Nav::Confirm => {
                let option = self.options.get_mut(self.option_selected)?;
                let n = option.values.len();
                if n < 2 {
                    return None;
                }
                option.current = if nav == Nav::Left {
                    (option.current + n - 1) % n
                } else {
                    (option.current + 1) % n
                };
                return Some(QuickAction::SetOption {
                    key: option.key.clone(),
                    value: option.values[option.current].clone(),
                });
            }
        }
        None
    }

    /// The lines of the current page.
    pub fn lines(&self, strings: &Strings) -> Vec<Line> {
        match self.page {
            Page::Main => MAIN
                .iter()
                .map(|entry| {
                    let (label, value, adjustable) = match entry {
                        Entry::Resume => (strings.resume.clone(), None, false),
                        Entry::Save => (strings.save_state.clone(), None, false),
                        Entry::Load => (strings.load_state.clone(), None, false),
                        Entry::Slot => (
                            fill(&strings.state_slot, &[("slot", &self.slot.to_string())]),
                            None,
                            true,
                        ),
                        Entry::Shader => (
                            strings.shader.clone(),
                            Some(strings.shader_label(&self.shader).to_owned()),
                            true,
                        ),
                        Entry::Options => (strings.core_options.clone(), None, false),
                        Entry::Reset => (strings.reset.clone(), None, false),
                        Entry::Close => (strings.quit_game.clone(), None, false),
                    };
                    Line {
                        label,
                        value,
                        adjustable,
                    }
                })
                .collect(),
            Page::Options if self.options.is_empty() => vec![Line {
                label: strings.core_options_empty.clone(),
                value: None,
                adjustable: false,
            }],
            Page::Options => self
                .options
                .iter()
                .map(|o| Line {
                    label: o.label.clone(),
                    value: o.values.get(o.current).cloned(),
                    adjustable: o.values.len() > 1,
                })
                .collect(),
        }
    }

    /// Title of the current page.
    pub fn page_title(&self, strings: &Strings) -> String {
        match self.page {
            Page::Main => self.title.clone(),
            Page::Options => strings.core_options.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(key: &str, values: &[&str], current: usize) -> OptionEntry {
        OptionEntry {
            key: key.into(),
            label: key.to_uppercase(),
            values: values.iter().map(|v| (*v).into()).collect(),
            current,
        }
    }

    fn menu() -> QuickMenu {
        QuickMenu::new(
            "Game",
            0,
            "none",
            vec![
                option("speed", &["1", "2", "4"], 0),
                option("fixed", &["x"], 0),
            ],
        )
    }

    #[test]
    fn main_page_actions() {
        let mut q = menu();
        assert_eq!(q.handle(Nav::Confirm), Some(QuickAction::Resume));
        q.handle(Nav::Down);
        assert_eq!(q.handle(Nav::Confirm), Some(QuickAction::SaveState));
        q.handle(Nav::Down);
        assert_eq!(q.handle(Nav::Confirm), Some(QuickAction::LoadState));
        q.handle(Nav::Down);
        assert_eq!(q.handle(Nav::Right), Some(QuickAction::NextSlot));
        assert_eq!(q.handle(Nav::Left), Some(QuickAction::PreviousSlot));
        q.handle(Nav::Down);
        assert_eq!(q.handle(Nav::Right), Some(QuickAction::CycleShader));
        assert_eq!(q.handle(Nav::Left), Some(QuickAction::CycleShader));
        assert_eq!(q.handle(Nav::Confirm), Some(QuickAction::CycleShader));
        q.handle(Nav::Down);
        q.handle(Nav::Down);
        assert_eq!(q.handle(Nav::Confirm), Some(QuickAction::Reset));
        q.handle(Nav::Down);
        q.handle(Nav::Down); // clamped
        assert_eq!(q.handle(Nav::Confirm), Some(QuickAction::CloseGame));
        assert_eq!(q.handle(Nav::Back), Some(QuickAction::Resume));
        assert_eq!(
            q.handle(Nav::Left),
            None,
            "left/right do nothing on plain entries"
        );
    }

    #[test]
    fn options_page_cycles_values() {
        let mut q = menu();
        for _ in 0..5 {
            q.handle(Nav::Down);
        }
        assert_eq!(q.handle(Nav::Confirm), None);
        assert_eq!(q.page(), Page::Options);

        let set = |v: &str| {
            Some(QuickAction::SetOption {
                key: "speed".into(),
                value: v.into(),
            })
        };
        assert_eq!(q.handle(Nav::Right), set("2"));
        assert_eq!(q.handle(Nav::Confirm), set("4"));
        assert_eq!(q.handle(Nav::Right), set("1"), "wraps around");
        assert_eq!(q.handle(Nav::Left), set("4"));

        q.handle(Nav::Down);
        assert_eq!(q.handle(Nav::Right), None, "single-value option");
        q.handle(Nav::Back);
        assert_eq!(q.page(), Page::Main);
        assert_eq!(q.selected(), 5, "back where we were");
    }

    #[test]
    fn lines_show_slot_and_values() {
        let strings = Strings::load("it");
        let mut q = menu();
        q.set_slot(3);
        q.set_shader("crt");
        let lines = q.lines(&strings);
        assert_eq!(lines.len(), 8);
        assert_eq!(lines[3].label, "Slot di salvataggio: 3");
        assert!(lines[3].adjustable);
        assert_eq!(lines[4].label, "Shader video");
        assert_eq!(lines[4].value.as_deref(), Some("CRT"));
        assert!(lines[4].adjustable);

        for _ in 0..5 {
            q.handle(Nav::Down);
        }
        q.handle(Nav::Confirm);
        q.handle(Nav::Right);
        let lines = q.lines(&strings);
        assert_eq!(lines[0].value.as_deref(), Some("2"));
        assert!(!lines[1].adjustable);
        assert_eq!(q.page_title(&strings), "Opzioni del core");
    }

    #[test]
    fn empty_options_page_explains_itself() {
        let strings = Strings::load("en");
        let mut q = QuickMenu::new("Game", 0, "none", Vec::new());
        for _ in 0..5 {
            q.handle(Nav::Down);
        }
        q.handle(Nav::Confirm);
        assert_eq!(q.handle(Nav::Right), None);
        assert_eq!(q.lines(&strings)[0].label, "This core has no options");
    }
}
