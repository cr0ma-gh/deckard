//! Interface strings, loaded from the embedded `assets/i18n/<lang>.toml` files.

use serde::Deserialize;

/// Every string shown by the menu. Missing keys in a translation are a
/// compile-time-tested error (see the tests), never a silent fallback.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Strings {
    pub settings: String,
    pub games: String,
    pub rescan: String,
    pub rescan_info: String,
    pub quit: String,
    pub quit_info: String,
    pub switch_to_steam: String,
    pub switch_to_steam_info: String,
    pub no_games: String,
    pub no_games_info: String,
    pub game_count: String,
    pub core_missing: String,
    pub launch_failed: String,
    pub library_updated: String,
    pub hint_confirm: String,
    pub hint_back: String,
    pub hint_change: String,
    pub integer_scale: String,
    pub integer_scale_info: String,
    pub smooth_filter: String,
    pub smooth_filter_info: String,
    pub shader: String,
    pub shader_info: String,
    pub shader_none: String,
    pub shader_scanlines: String,
    pub shader_crt: String,
    pub language: String,
    pub language_info: String,
    pub language_name: String,
    pub value_on: String,
    pub value_off: String,
    pub resume: String,
    pub save_state: String,
    pub load_state: String,
    pub state_slot: String,
    pub core_options: String,
    pub core_options_empty: String,
    pub reset: String,
    pub quit_game: String,
    pub state_saved: String,
    pub state_loaded: String,
    pub state_error: String,
}

const EN: &str = include_str!("../assets/i18n/en.toml");
const IT: &str = include_str!("../assets/i18n/it.toml");

/// Languages with a translation file.
pub const LANGUAGES: [&str; 2] = ["en", "it"];

impl Strings {
    /// Strings for `language`; unknown languages fall back to English.
    pub fn load(language: &str) -> Self {
        let text = match language {
            "it" => IT,
            "en" => EN,
            other => {
                tracing::warn!(language = other, "no translation, using English");
                EN
            }
        };
        match toml::from_str(text) {
            Ok(strings) => strings,
            Err(e) => {
                // Unreachable in tested builds: every file is parsed by the tests.
                tracing::error!("broken translation for '{language}': {e}");
                toml::from_str(EN).unwrap_or_default()
            }
        }
    }

    /// "On"/"Off" for a boolean setting.
    pub fn on_off(&self, value: bool) -> &str {
        if value {
            &self.value_on
        } else {
            &self.value_off
        }
    }

    /// Label for a shader mode identifier ("none", "scanlines", "crt").
    pub fn shader_label(&self, mode: &str) -> &str {
        match mode {
            "scanlines" => &self.shader_scanlines,
            "crt" => &self.shader_crt,
            _ => &self.shader_none,
        }
    }
}

/// Next language in [`LANGUAGES`] after `current` (wraps around).
pub fn next_language(current: &str) -> &'static str {
    let i = LANGUAGES
        .iter()
        .position(|l| *l == current)
        .map_or(0, |i| i + 1);
    LANGUAGES[i % LANGUAGES.len()]
}

/// Replaces `{name}` placeholders: `fill("{n} games", &[("n", "3")])`.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    values
        .iter()
        .fold(template.to_owned(), |text, (name, value)| {
            text.replace(&format!("{{{name}}}"), value)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_translation_is_complete() {
        for (lang, text) in [("en", EN), ("it", IT)] {
            let parsed: Result<Strings, _> = toml::from_str(text);
            assert!(parsed.is_ok(), "{lang}.toml: {:?}", parsed.err());
        }
        assert_eq!(Strings::load("it").settings, "Impostazioni");
        assert_eq!(Strings::load("xx").settings, "Settings");
    }

    #[test]
    fn placeholders_match_between_languages() {
        let en = Strings::load("en");
        let it = Strings::load("it");
        let pairs = [
            (&en.no_games_info, &it.no_games_info, "{path}"),
            (&en.game_count, &it.game_count, "{count}"),
            (&en.core_missing, &it.core_missing, "{core}"),
            (&en.launch_failed, &it.launch_failed, "{error}"),
            (&en.library_updated, &it.library_updated, "{count}"),
            (&en.state_slot, &it.state_slot, "{slot}"),
            (&en.state_saved, &it.state_saved, "{slot}"),
            (&en.state_loaded, &it.state_loaded, "{slot}"),
            (&en.state_error, &it.state_error, "{error}"),
        ];
        for (a, b, placeholder) in pairs {
            assert!(
                a.contains(placeholder) && b.contains(placeholder),
                "{placeholder}"
            );
        }
    }

    #[test]
    fn fills_placeholders() {
        assert_eq!(
            fill("{count} games, {count}!", &[("count", "3")]),
            "3 games, 3!"
        );
        assert_eq!(fill("no placeholder", &[("x", "y")]), "no placeholder");
    }
}
