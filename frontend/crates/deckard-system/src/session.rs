//! Session switching: write the `next-session` file so the session manager
//! (`deckard-session`) starts the requested session after this process exits.

use std::fs;
use std::path::Path;

use crate::SystemError;

/// Which session to switch to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTarget {
    /// The Deckard frontend (default).
    Deckard,
    /// Steam Big Picture under gamescope.
    Steam,
}

impl SessionTarget {
    /// Value written to the `next-session` file.
    fn as_str(self) -> &'static str {
        match self {
            Self::Deckard => "deckard",
            Self::Steam => "steam",
        }
    }
}

/// Writes the `next-session` file atomically. Logs a warning on non-Linux
/// platforms where no session manager is running to consume it.
pub(crate) fn request(target: SessionTarget, state_dir: &Path) -> Result<(), SystemError> {
    if !crate::session_switching_available() {
        tracing::warn!(
            target = target.as_str(),
            "session switching is not available on this platform"
        );
    }
    let path = state_dir.join("next-session");
    let io_err = |source| SystemError::Io {
        path: path.clone(),
        source,
    };
    fs::create_dir_all(state_dir).map_err(io_err)?;

    // Atomic write: temp file + rename.
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    fs::write(&tmp, target.as_str()).map_err(io_err)?;
    fs::rename(&tmp, &path).map_err(io_err)?;

    tracing::info!(target = target.as_str(), path = %path.display(), "session switch requested");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_next_session_file() {
        let dir = std::env::temp_dir().join(format!("deckard-session-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        request(SessionTarget::Steam, &dir).unwrap();
        let content = fs::read_to_string(dir.join("next-session")).unwrap();
        assert_eq!(content, "steam");

        request(SessionTarget::Deckard, &dir).unwrap();
        let content = fs::read_to_string(dir.join("next-session")).unwrap();
        assert_eq!(content, "deckard");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn session_target_as_str() {
        assert_eq!(SessionTarget::Deckard.as_str(), "deckard");
        assert_eq!(SessionTarget::Steam.as_str(), "steam");
    }
}
