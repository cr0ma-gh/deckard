//! OS integration for Deckard: session switching, and (later) battery,
//! brightness, Wi-Fi.
//!
//! Platform-specific code lives behind `#[cfg(target_os = "linux")]`.
//! On other platforms the public functions are stubs that log a warning.

use std::path::{Path, PathBuf};

pub mod installer;
mod session;

pub use session::SessionTarget;

/// Returns the state directory: `$XDG_STATE_HOME/deckard` or
/// `~/.local/state/deckard`.
pub fn state_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("state")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("deckard")
}

/// Errors from OS integration.
#[derive(Debug, thiserror::Error)]
pub enum SystemError {
    #[error("cannot write session file {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{command} failed: {message}")]
    Command { command: String, message: String },
}

/// Path of the `next-session` file inside `state_dir`.
pub fn next_session_path(state_dir: &Path) -> PathBuf {
    state_dir.join("next-session")
}

/// Whether session switching is available on this platform.
///
/// Returns `true` on Linux (where the session manager can act on the file)
/// and `false` everywhere else.
pub fn session_switching_available() -> bool {
    cfg!(target_os = "linux")
}

/// Requests a session switch: writes the target to the `next-session` file
/// in the given `state_dir`. On non-Linux platforms this is a no-op that
/// logs a warning.
pub fn request_session_switch(target: SessionTarget, state_dir: &Path) -> Result<(), SystemError> {
    session::request(target, state_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_dir_ends_with_deckard() {
        let dir = state_dir();
        assert!(dir.ends_with("deckard"), "{}", dir.display());
    }

    #[test]
    fn next_session_path_is_inside_state_dir() {
        let dir = Path::new("/tmp/deckard-state");
        let path = next_session_path(dir);
        assert_eq!(path, dir.join("next-session"));
    }
}
