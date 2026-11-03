//! Runs `deckard-install` (installer/deckard-install in the repository) for the
//! menu: lists disks and follows an installation's progress.
//!
//! The installer needs root: it is started with `sudo -n`, which works without
//! a password on the live image. Output lines are parsed into [`Event`]s on a
//! background thread so the menu keeps running.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};

use crate::SystemError;

const INSTALLER: &str = "deckard-install";

/// A disk the installer accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disk {
    pub path: String,
    pub bytes: u64,
    pub transport: String,
    pub removable: bool,
    pub model: String,
}

/// A message from a running installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Progress(u8, String),
    Done,
    Error(String),
}

/// True when running from the Deckard live image (archiso), the only place
/// where installing makes sense.
pub fn is_live_system() -> bool {
    cfg!(target_os = "linux") && Path::new("/run/archiso/airootfs").is_dir()
}

/// Parses `deckard-install list` output: `path\tbytes\ttransport\tremovable\tmodel`.
pub fn parse_disks(text: &str) -> Vec<Disk> {
    text.lines()
        .filter_map(|line| {
            let mut f = line.split('\t');
            let path = f.next()?.to_owned();
            let bytes = f.next()?.parse().ok()?;
            let transport = f.next()?.to_owned();
            let removable = f.next()? == "1";
            let model = f.next().unwrap_or("Disk").trim().to_owned();
            path.starts_with("/dev/").then_some(Disk {
                path,
                bytes,
                transport,
                removable,
                model,
            })
        })
        .collect()
}

/// Parses one line of `deckard-install install` output.
pub fn parse_event(line: &str) -> Option<Event> {
    let line = line.trim_end();
    if line == "DONE" {
        return Some(Event::Done);
    }
    if let Some(message) = line.strip_prefix("ERROR ") {
        return Some(Event::Error(message.to_owned()));
    }
    let rest = line.strip_prefix("PROGRESS ")?;
    let (percent, step) = rest.split_once(' ').unwrap_or((rest, ""));
    Some(Event::Progress(
        percent.parse::<u8>().ok()?.min(100),
        step.to_owned(),
    ))
}

fn spawn_error(source: std::io::Error) -> SystemError {
    SystemError::Command {
        command: INSTALLER.into(),
        message: source.to_string(),
    }
}

/// Disks that can be installed to.
pub fn list_disks() -> Result<Vec<Disk>, SystemError> {
    let output = Command::new("sudo")
        .args(["-n", INSTALLER, "list"])
        .output()
        .map_err(spawn_error)?;
    if !output.status.success() {
        return Err(SystemError::Command {
            command: INSTALLER.into(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(parse_disks(&String::from_utf8_lossy(&output.stdout)))
}

/// A running installation.
pub struct InstallJob {
    child: Child,
    events: Receiver<Event>,
    finished: bool,
}

impl InstallJob {
    /// Starts erasing `disk` and installing Deckard on it.
    pub fn start(disk: &str) -> Result<Self, SystemError> {
        let mut child = Command::new("sudo")
            .args(["-n", INSTALLER, "install", disk, "--yes"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(spawn_error)?;
        let stdout = child.stdout.take().ok_or_else(|| SystemError::Command {
            command: INSTALLER.into(),
            message: "no output pipe".into(),
        })?;
        let (tx, events) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(event) = parse_event(&line)
                    && tx.send(event).is_err()
                {
                    break;
                }
            }
        });
        tracing::info!(disk, "installation started");
        Ok(Self {
            child,
            events,
            finished: false,
        })
    }

    /// Events received since the last call. If the process ends without
    /// reporting, an error event is produced.
    pub fn poll(&mut self) -> Vec<Event> {
        let mut out: Vec<Event> = self.events.try_iter().collect();
        if out
            .iter()
            .any(|e| matches!(e, Event::Done | Event::Error(_)))
        {
            self.finished = true;
        }
        if !self.finished
            && let Ok(Some(status)) = self.child.try_wait()
        {
            // The reader thread may still hold the last lines: drain them.
            out.extend(self.events.iter());
            if !out
                .iter()
                .any(|e| matches!(e, Event::Done | Event::Error(_)))
            {
                out.push(Event::Error(format!("the installer stopped ({status})")));
            }
            self.finished = true;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_disk_list() {
        let text = "/dev/nvme0n1\t512110190592\tnvme\t0\tSamsung SSD\n\
                    /dev/mmcblk0\t127865454592\t?\t1\tSD Card\n\
                    garbage line\n";
        let disks = parse_disks(text);
        assert_eq!(disks.len(), 2);
        assert_eq!(disks[0].path, "/dev/nvme0n1");
        assert!(!disks[0].removable);
        assert_eq!(disks[1].model, "SD Card");
        assert!(disks[1].removable);
    }

    #[test]
    fn parses_installer_events() {
        assert_eq!(
            parse_event("PROGRESS 42 Copying the system"),
            Some(Event::Progress(42, "Copying the system".into()))
        );
        assert_eq!(
            parse_event("PROGRESS 100"),
            Some(Event::Progress(100, String::new()))
        );
        assert_eq!(parse_event("DONE\r"), Some(Event::Done));
        assert_eq!(
            parse_event("ERROR disk busy"),
            Some(Event::Error("disk busy".into()))
        );
        assert_eq!(parse_event("PROGRESS x y"), None);
        assert_eq!(parse_event("random output"), None);
    }

    #[test]
    fn not_live_in_tests() {
        // Tests never run from the archiso live image.
        assert!(!is_live_system() || Path::new("/run/archiso").exists());
    }
}
