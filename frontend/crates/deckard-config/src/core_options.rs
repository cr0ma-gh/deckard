//! Saved core option values, one TOML file per core (`<core>.toml`).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::{ConfigError, write_atomic};

/// Reads and writes the option values the user chose for each core.
#[derive(Debug, Clone)]
pub struct CoreOptionsStore {
    dir: PathBuf,
}

impl CoreOptionsStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn file(&self, core_name: &str) -> PathBuf {
        // Core names come from file names; keep only safe characters.
        let safe: String = core_name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.dir.join(format!("{safe}.toml"))
    }

    /// Saved `(key, value)` pairs for a core; empty if none were saved.
    pub fn load(&self, core_name: &str) -> Result<Vec<(String, String)>, ConfigError> {
        let path = self.file(core_name);
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(ConfigError::Io { path, source }),
        };
        let map: BTreeMap<String, String> =
            toml::from_str(&text).map_err(|source| ConfigError::Parse { path, source })?;
        Ok(map.into_iter().collect())
    }

    /// Saves the values that differ from the core's defaults. Removes the file
    /// when everything is back to default.
    pub fn save<'a>(
        &self,
        core_name: &str,
        changed: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<(), ConfigError> {
        let map: BTreeMap<&str, &str> = changed.into_iter().collect();
        let path = self.file(core_name);
        if map.is_empty() {
            return match fs::remove_file(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => {
                    Err(ConfigError::Io { path, source: e })
                }
                _ => Ok(()),
            };
        }
        write_atomic(&path, toml::to_string_pretty(&map)?.as_bytes())
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_and_reset() {
        let dir = std::env::temp_dir().join(format!("deckard-coreopts-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let store = CoreOptionsStore::new(&dir);

        assert!(store.load("snes9x").unwrap().is_empty());
        store
            .save("snes9x", [("snes9x_region", "pal"), ("a.b", "x y")])
            .unwrap();
        let loaded = store.load("snes9x").unwrap();
        assert_eq!(
            loaded,
            [
                ("a.b".to_owned(), "x y".to_owned()),
                ("snes9x_region".into(), "pal".into())
            ]
        );

        store.save("snes9x", []).unwrap();
        assert!(!store.file("snes9x").exists());
        assert_eq!(store.file("../evil"), dir.join("___evil.toml"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
