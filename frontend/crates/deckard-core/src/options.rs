//! Core options (the libretro "variables" a core declares).

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char};

use libretro_sys::{
    retro_core_option_definition, retro_core_option_v2_definition, retro_core_option_value,
    retro_variable,
};

/// One option declared by the core.
#[derive(Debug, Clone)]
pub struct CoreOption {
    pub key: String,
    pub description: String,
    pub values: Vec<String>,
    pub default: String,
    value: CString,
}

impl CoreOption {
    /// The current value.
    pub fn value(&self) -> &str {
        self.value.to_str().unwrap_or_default()
    }
}

/// All options of the loaded core, plus user overrides.
#[derive(Debug, Default)]
pub struct CoreOptions {
    list: Vec<CoreOption>,
    overrides: HashMap<String, String>,
    updated: bool,
}

impl CoreOptions {
    /// Creates the store with user-chosen values, applied when the core declares the key.
    pub fn with_overrides(overrides: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            overrides: overrides.into_iter().collect(),
            ..Self::default()
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &CoreOption> {
        self.list.iter()
    }

    pub fn get(&self, key: &str) -> Option<&CoreOption> {
        self.list.iter().find(|o| o.key == key)
    }

    /// Sets an option value. Returns false if the key or value is unknown.
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        let Some(opt) = self.list.iter_mut().find(|o| o.key == key) else {
            return false;
        };
        if !opt.values.iter().any(|v| v == value) {
            return false;
        }
        let Ok(c) = CString::new(value) else {
            return false;
        };
        if opt.value != c {
            opt.value = c;
            self.updated = true;
        }
        true
    }

    /// Returns whether any value changed since the last call, and clears the flag.
    pub fn take_updated(&mut self) -> bool {
        std::mem::take(&mut self.updated)
    }

    /// Pointer to the current value, valid until the option is changed or redefined.
    pub(crate) fn value_ptr(&self, key: &str) -> Option<*const c_char> {
        self.get(key).map(|o| o.value.as_ptr())
    }

    fn define(
        &mut self,
        key: String,
        description: String,
        values: Vec<String>,
        default: Option<String>,
    ) {
        let Some(first) = values.first().cloned() else {
            tracing::warn!(key, "core option without values ignored");
            return;
        };
        let default = default.filter(|d| values.contains(d)).unwrap_or(first);
        let chosen = self
            .overrides
            .get(&key)
            .filter(|v| {
                let valid = values.contains(v);
                if !valid {
                    tracing::warn!(key, value = v.as_str(), "invalid override for core option");
                }
                valid
            })
            .cloned()
            .unwrap_or_else(|| default.clone());
        let Ok(value) = CString::new(chosen) else {
            return;
        };
        let opt = CoreOption {
            key,
            description,
            values,
            default,
            value,
        };
        match self.list.iter_mut().find(|o| o.key == opt.key) {
            Some(existing) => *existing = opt,
            None => self.list.push(opt),
        }
        self.updated = true;
    }

    /// Replaces all options with a `SET_VARIABLES` array (ends with a NULL key).
    ///
    /// # Safety
    /// `vars` must point to a valid, NULL-key-terminated array of valid C strings.
    pub(crate) unsafe fn set_variables(&mut self, vars: *const retro_variable) {
        self.list.clear();
        let mut i = 0;
        loop {
            // SAFETY: the caller guarantees the array is terminated by a NULL key.
            let var = unsafe { &*vars.add(i) };
            if var.key.is_null() {
                break;
            }
            // SAFETY: valid C strings per the caller's contract.
            let (key, value) = unsafe { (cstr(var.key), cstr(var.value)) };
            if let Some((desc, values)) = parse_legacy_variable(&value) {
                let default = values.first().cloned();
                self.define(key, desc, values, default);
            }
            i += 1;
        }
    }

    /// Replaces all options with a `SET_CORE_OPTIONS` (v1) array.
    ///
    /// # Safety
    /// `defs` must point to a valid, NULL-key-terminated definition array.
    pub(crate) unsafe fn set_v1(&mut self, defs: *const retro_core_option_definition) {
        self.list.clear();
        let mut i = 0;
        loop {
            // SAFETY: the caller guarantees the array is terminated by a NULL key.
            let def = unsafe { &*defs.add(i) };
            if def.key.is_null() {
                break;
            }
            // SAFETY: valid C strings and value arrays per the caller's contract.
            unsafe {
                self.define(
                    cstr(def.key),
                    cstr(def.desc),
                    option_values(&def.values),
                    opt_cstr(def.default_value),
                );
            }
            i += 1;
        }
    }

    /// Replaces all options with a `SET_CORE_OPTIONS_V2` definition array.
    ///
    /// # Safety
    /// `defs` must point to a valid, NULL-key-terminated definition array.
    pub(crate) unsafe fn set_v2(&mut self, defs: *const retro_core_option_v2_definition) {
        self.list.clear();
        let mut i = 0;
        loop {
            // SAFETY: the caller guarantees the array is terminated by a NULL key.
            let def = unsafe { &*defs.add(i) };
            if def.key.is_null() {
                break;
            }
            // SAFETY: valid C strings and value arrays per the caller's contract.
            unsafe {
                self.define(
                    cstr(def.key),
                    cstr(def.desc),
                    option_values(&def.values),
                    opt_cstr(def.default_value),
                );
            }
            i += 1;
        }
    }
}

/// Parses a legacy `SET_VARIABLES` value: `"Description; first|second|third"`.
/// The first value is the default.
pub fn parse_legacy_variable(value: &str) -> Option<(String, Vec<String>)> {
    let (desc, values) = value.split_once("; ")?;
    let values: Vec<String> = values.split('|').map(str::to_owned).collect();
    if values.iter().any(String::is_empty) {
        return None;
    }
    Some((desc.to_owned(), values))
}

/// # Safety
/// `ptr` must be NULL or a valid C string.
unsafe fn cstr(ptr: *const c_char) -> String {
    // SAFETY: forwarded from the caller.
    unsafe { opt_cstr(ptr) }.unwrap_or_default()
}

/// # Safety
/// `ptr` must be NULL or a valid C string.
unsafe fn opt_cstr(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: non-null and valid per the caller's contract.
    Some(
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned(),
    )
}

/// # Safety
/// Each entry up to the first NULL `value` must hold valid C strings.
unsafe fn option_values(values: &[retro_core_option_value]) -> Vec<String> {
    values
        .iter()
        .take_while(|v| !v.value.is_null())
        // SAFETY: non-null and valid per the caller's contract.
        .map(|v| unsafe { cstr(v.value) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_variable() {
        let (desc, values) = parse_legacy_variable("Scroll speed; 1|2|4").unwrap_or_default();
        assert_eq!(desc, "Scroll speed");
        assert_eq!(values, ["1", "2", "4"]);
        assert!(parse_legacy_variable("no separator").is_none());
        assert!(parse_legacy_variable("Empty; a||b").is_none());
    }

    #[test]
    fn overrides_and_updates() {
        let mut opts = CoreOptions::with_overrides([("speed".to_owned(), "4".to_owned())]);
        opts.define(
            "speed".into(),
            "Speed".into(),
            vec!["1".into(), "4".into()],
            None,
        );
        assert_eq!(opts.get("speed").map(CoreOption::value), Some("4"));
        assert!(opts.take_updated());
        assert!(!opts.take_updated());

        assert!(opts.set("speed", "1"));
        assert!(opts.take_updated());
        assert!(!opts.set("speed", "3"), "unknown value must be rejected");
        assert!(!opts.set("missing", "1"));
    }

    #[test]
    fn invalid_default_falls_back_to_first_value() {
        let mut opts = CoreOptions::default();
        opts.define(
            "k".into(),
            "K".into(),
            vec!["a".into(), "b".into()],
            Some("zzz".into()),
        );
        assert_eq!(opts.get("k").map(|o| o.default.as_str()), Some("a"));
    }
}
