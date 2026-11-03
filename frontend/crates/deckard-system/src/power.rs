//! Power off and reboot through systemd (logind lets the user of the active
//! local session do this without a password).

use std::process::Command;

use crate::SystemError;

/// What to do with the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Reboot,
    PowerOff,
}

impl PowerAction {
    fn verb(self) -> &'static str {
        match self {
            Self::Reboot => "reboot",
            Self::PowerOff => "poweroff",
        }
    }
}

/// Whether the menu should offer power off and reboot: only on Linux inside
/// the Deckard session (`deckard-session` sets `DECKARD_SESSION`), so a
/// development run on a desktop never turns the machine off.
pub fn available() -> bool {
    cfg!(target_os = "linux") && std::env::var_os("DECKARD_SESSION").is_some()
}

/// Asks systemd to reboot or power off. Returns once the request is sent.
pub fn request(action: PowerAction) -> Result<(), SystemError> {
    let verb = action.verb();
    tracing::info!(action = verb, "power request");
    let status =
        Command::new("systemctl")
            .arg(verb)
            .status()
            .map_err(|e| SystemError::Command {
                command: format!("systemctl {verb}"),
                message: e.to_string(),
            })?;
    if status.success() {
        Ok(())
    } else {
        Err(SystemError::Command {
            command: format!("systemctl {verb}"),
            message: status.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbs_match_systemctl() {
        assert_eq!(PowerAction::Reboot.verb(), "reboot");
        assert_eq!(PowerAction::PowerOff.verb(), "poweroff");
    }
}
