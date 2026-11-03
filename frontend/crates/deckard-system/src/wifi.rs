//! Wi-Fi through NetworkManager (`nmcli`), run on a worker thread so the menu
//! never waits for a scan or a connection.
//!
//! `DECKARD_FAKE_WIFI=1` replaces `nmcli` with a simulated network list, to
//! work on the menu where there is no Wi-Fi (Windows, virtual machines).

use std::process::Command;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

/// A network seen by the last scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network {
    pub ssid: String,
    /// Signal strength, 0–100.
    pub signal: u8,
    /// True if a password is needed.
    pub secured: bool,
    /// True if currently connected.
    pub active: bool,
}

/// Work for the Wi-Fi thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Scan,
    Connect {
        ssid: String,
        password: Option<String>,
    },
    Disconnect,
}

/// Results from the Wi-Fi thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Networks(Vec<Network>),
    Connected(String),
    Disconnected,
    Error(String),
}

/// Splits one line of `nmcli -t` output: fields separated by `:`, with `\:`
/// and `\\` escaping literal characters.
pub fn split_terse(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let (Some(next), Some(field)) = (chars.next(), fields.last_mut()) {
                    field.push(next);
                }
            }
            ':' => fields.push(String::new()),
            c => {
                if let Some(field) = fields.last_mut() {
                    field.push(c);
                }
            }
        }
    }
    fields
}

/// Parses `nmcli -t -f IN-USE,SSID,SIGNAL,SECURITY device wifi list`:
/// hidden networks are skipped, duplicates (several access points) keep the
/// strongest, the list is sorted by connection state and signal.
pub fn parse_networks(text: &str) -> Vec<Network> {
    let mut networks: Vec<Network> = Vec::new();
    for line in text.lines() {
        let f = split_terse(line);
        if f.len() < 4 || f[1].is_empty() {
            continue;
        }
        let network = Network {
            ssid: f[1].clone(),
            signal: f[2].parse().unwrap_or(0).min(100),
            secured: !f[3].trim().is_empty() && f[3].trim() != "--",
            active: f[0].trim() == "*",
        };
        match networks.iter_mut().find(|n| n.ssid == network.ssid) {
            Some(existing) => {
                existing.active |= network.active;
                existing.signal = existing.signal.max(network.signal);
            }
            None => networks.push(network),
        }
    }
    networks.sort_by(|a, b| b.active.cmp(&a.active).then(b.signal.cmp(&a.signal)));
    networks
}

/// Turns an `nmcli` failure into a short message ("Error: ..." prefix removed).
fn nmcli_error(output: &std::process::Output) -> String {
    let text = String::from_utf8_lossy(&output.stderr);
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("nmcli failed");
    line.trim().trim_start_matches("Error: ").to_owned()
}

fn nmcli(args: &[&str]) -> Result<String, String> {
    let output = Command::new("nmcli")
        .args(args)
        .output()
        .map_err(|e| format!("cannot run nmcli: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(nmcli_error(&output))
    }
}

trait Backend: Send {
    fn handle(&mut self, request: Request) -> Reply;
}

/// The real backend: NetworkManager through `nmcli`.
struct NmcliBackend;

impl Backend for NmcliBackend {
    fn handle(&mut self, request: Request) -> Reply {
        let result = match request {
            Request::Scan => nmcli(&[
                "-t",
                "-f",
                "IN-USE,SSID,SIGNAL,SECURITY",
                "device",
                "wifi",
                "list",
                "--rescan",
                "yes",
            ])
            .map(|out| Reply::Networks(parse_networks(&out))),
            Request::Connect { ssid, password } => {
                let mut args = vec!["--wait", "30", "device", "wifi", "connect", ssid.as_str()];
                if let Some(password) = password.as_deref() {
                    args.extend(["password", password]);
                }
                nmcli(&args).map(|_| Reply::Connected(ssid))
            }
            Request::Disconnect => {
                nmcli(&["-t", "-f", "DEVICE,TYPE,STATE", "device"]).and_then(|out| {
                    let device = out
                        .lines()
                        .map(split_terse)
                        .find(|f| f.len() >= 3 && f[1] == "wifi" && f[2] == "connected")
                        .map(|f| f[0].clone())
                        .ok_or_else(|| "not connected".to_owned())?;
                    nmcli(&["device", "disconnect", &device]).map(|_| Reply::Disconnected)
                })
            }
        };
        result.unwrap_or_else(Reply::Error)
    }
}

/// Simulated networks for development (`DECKARD_FAKE_WIFI=1`). The password of
/// every secured network is "password".
struct FakeBackend {
    active: Option<String>,
}

impl Backend for FakeBackend {
    fn handle(&mut self, request: Request) -> Reply {
        std::thread::sleep(Duration::from_millis(700));
        match request {
            Request::Scan => {
                let mut networks = vec![
                    ("Casa", 82, true),
                    ("Ufficio-5G", 64, true),
                    ("Biblioteca Ospiti", 41, false),
                    ("Vicino_2.4", 18, true),
                ]
                .into_iter()
                .map(|(ssid, signal, secured)| Network {
                    ssid: ssid.into(),
                    signal,
                    secured,
                    active: self.active.as_deref() == Some(ssid),
                })
                .collect::<Vec<_>>();
                networks.sort_by(|a, b| b.active.cmp(&a.active).then(b.signal.cmp(&a.signal)));
                Reply::Networks(networks)
            }
            Request::Connect { ssid, password } => {
                let open = ssid == "Biblioteca Ospiti";
                if open || password.as_deref() == Some("password") {
                    self.active = Some(ssid.clone());
                    Reply::Connected(ssid)
                } else {
                    Reply::Error("Secrets were required, but not provided".into())
                }
            }
            Request::Disconnect => {
                self.active = None;
                Reply::Disconnected
            }
        }
    }
}

/// True when Wi-Fi can be managed here (Linux, or the fake backend).
pub fn available() -> bool {
    fake_mode() || cfg!(target_os = "linux")
}

fn fake_mode() -> bool {
    std::env::var_os("DECKARD_FAKE_WIFI").is_some_and(|v| v == "1")
}

/// Runs Wi-Fi requests one at a time on a background thread.
pub struct WifiWorker {
    requests: Sender<Request>,
    replies: Receiver<Reply>,
}

impl WifiWorker {
    pub fn start() -> Self {
        let backend: Box<dyn Backend> = if fake_mode() {
            tracing::info!("using the simulated Wi-Fi backend");
            Box::new(FakeBackend { active: None })
        } else {
            Box::new(NmcliBackend)
        };
        Self::with_backend(backend)
    }

    fn with_backend(mut backend: Box<dyn Backend>) -> Self {
        let (requests, worker_rx) = channel::<Request>();
        let (worker_tx, replies) = channel::<Reply>();
        let spawned = std::thread::Builder::new()
            .name("deckard-wifi".into())
            .spawn(move || {
                for request in worker_rx {
                    let reply = backend.handle(request);
                    if worker_tx.send(reply).is_err() {
                        break;
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::error!("cannot start the Wi-Fi thread: {e}");
        }
        Self { requests, replies }
    }

    pub fn request(&self, request: Request) {
        if self.requests.send(request).is_err() {
            tracing::error!("the Wi-Fi thread has stopped");
        }
    }

    /// Replies received since the last call.
    pub fn poll(&self) -> Vec<Reply> {
        self.replies.try_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_escaped_fields() {
        assert_eq!(
            split_terse(r"*:My\:Net:78:WPA2"),
            ["*", "My:Net", "78", "WPA2"]
        );
        assert_eq!(
            split_terse(r":Back\\slash:5:"),
            ["", r"Back\slash", "5", ""]
        );
    }

    #[test]
    fn parses_networks_deduplicated_and_sorted() {
        let out = " :Casa:60:WPA2\n\
                   *:Ufficio:40:WPA1 WPA2\n\
                   :Casa:80:WPA2\n\
                   :Aperta:90:\n\
                   ::70:WPA2\n";
        let n = parse_networks(out);
        let ssids: Vec<_> = n.iter().map(|n| n.ssid.as_str()).collect();
        assert_eq!(
            ssids,
            ["Ufficio", "Aperta", "Casa"],
            "active first, then by signal"
        );
        assert!(n[0].active && n[0].secured);
        assert!(!n[1].secured, "empty security = open network");
        assert_eq!(n[2].signal, 80, "strongest access point kept");
    }

    #[test]
    fn fake_backend_flow() {
        let worker = WifiWorker::with_backend(Box::new(FakeBackend { active: None }));
        let wait = || loop {
            if let Some(reply) = worker.poll().into_iter().next() {
                break reply;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        worker.request(Request::Connect {
            ssid: "Casa".into(),
            password: Some("wrong".into()),
        });
        assert!(matches!(wait(), Reply::Error(_)));
        worker.request(Request::Connect {
            ssid: "Casa".into(),
            password: Some("password".into()),
        });
        assert_eq!(wait(), Reply::Connected("Casa".into()));
        worker.request(Request::Scan);
        match wait() {
            Reply::Networks(n) => assert!(n[0].active && n[0].ssid == "Casa"),
            other => panic!("unexpected {other:?}"),
        }
    }
}
