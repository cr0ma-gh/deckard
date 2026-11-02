//! Interface strings, loaded from the embedded `assets/i18n/<lang>.toml` files.

use serde::Deserialize;

/// Every string shown by the menu. Missing keys in a translation are a
/// compile-time-tested error (see the tests), never a silent fallback.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Strings {
    pub settings: String,
    pub games: String,
    pub rescan: String,
    pub rescan_info: String,
    pub quit: String,
    pub quit_info: String,
    pub no_games: String,
    pub no_games_info: String,
    pub game_count: String,
    pub core_missing: String,
    pub launch_failed: String,
    pub library_updated: String,
    pub hint_confirm: String,
    pub hint_back: String,
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
                toml::from_str(EN).unwrap_or_else(|_| Self::fallback())
            }
        }
    }

    /// Last-resort strings if even English failed to parse.
    fn fallback() -> Self {
        let s = |v: &str| v.to_owned();
        Self {
            settings: s("Settings"),
            games: s("Games"),
            rescan: s("Rescan"),
            rescan_info: String::new(),
            quit: s("Quit"),
            quit_info: String::new(),
            no_games: s("No games"),
            no_games_info: s("{path}"),
            game_count: s("{count}"),
            core_missing: s("{core}"),
            launch_failed: s("{error}"),
            library_updated: s("{count}"),
            hint_confirm: s("OK"),
            hint_back: s("Back"),
        }
    }
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
