//! OSC remote control: plugins can be enabled, disabled and tuned over UDP.
//!
//! The server runs on its own thread, decodes packets into `OscRequest`s and
//! hands them to the UI thread, which owns all plugin state and applies them
//! (see `AppController::poll_osc`). Links are deliberately not controllable.
//!
//! Addresses (`<plugin>` is the plugin's name as shown, lowercased with
//! anything but letters and digits turned into `_` -- "Reverb 1" is
//! `reverb_1` -- or its stable id):
//!
//! ```text
//! /zestbay/plugins                           query: lists the plugins
//! /zestbay/sessions                          query: lists the sessions
//! /zestbay/session/load             "name"   switch to another session
//! /zestbay/plugin/<plugin>/enable   1|0      on / off (off = bypassed)
//! /zestbay/plugin/<plugin>/bypass   1|0      the reverse of enable
//! /zestbay/plugin/<plugin>/toggle            flip on/off
//! /zestbay/plugin/<plugin>/param/<param> v   set a value in the parameter's own range
//! /zestbay/plugin/<plugin>/param/<param>/normalized v   set it from 0.0 to 1.0
//! /zestbay/plugin/<plugin>/preset   "name"   apply a saved preset
//! /zestbay/plugin/<plugin>/params            query: lists the parameters
//! ```
//!
//! `<param>` is the parameter's symbol, its name in the same lowercase form,
//! or its port index. Queries are answered to the sender's address.

use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use rosc::{OscMessage, OscPacket, OscType};

pub const DEFAULT_PORT: u16 = 9000;

const ROOT: &str = "zestbay";

#[derive(Debug, Clone, PartialEq)]
pub enum OscAction {
    /// `enabled` false bypasses the plugin.
    Enable { plugin: String, enabled: bool },
    Toggle { plugin: String },
    Param {
        plugin: String,
        param: String,
        value: f32,
        normalized: bool,
    },
    Preset { plugin: String, name: String },
    ListPlugins,
    ListParams { plugin: String },
    ListSessions,
    LoadSession { name: String },
}

#[derive(Debug, Clone)]
pub struct OscRequest {
    pub action: OscAction,
    pub from: SocketAddr,
}

/// Lowercase letters and digits, everything else folded into single `_`.
pub fn slug(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

fn number(arg: &OscType) -> Option<f32> {
    match arg {
        OscType::Float(v) => Some(*v),
        OscType::Double(v) => Some(*v as f32),
        OscType::Int(v) => Some(*v as f32),
        OscType::Long(v) => Some(*v as f32),
        OscType::Bool(v) => Some(if *v { 1.0 } else { 0.0 }),
        _ => None,
    }
    .filter(|v| v.is_finite())
}

fn string(arg: &OscType) -> Option<String> {
    match arg {
        OscType::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// Turn one message into an action. Err says why it isn't one (for the log).
pub fn parse(addr: &str, args: &[OscType]) -> Result<OscAction, String> {
    let parts: Vec<&str> = addr.split('/').skip(1).collect();
    match parts.as_slice() {
        [r, "plugins"] if *r == ROOT => Ok(OscAction::ListPlugins),
        [r, "sessions"] if *r == ROOT => Ok(OscAction::ListSessions),
        [r, "session", "load"] if *r == ROOT => args
            .first()
            .and_then(string)
            .map(|name| OscAction::LoadSession { name })
            .ok_or_else(|| "session/load needs a name argument".to_string()),
        [r, "plugin", plugin, rest @ ..] if *r == ROOT && !plugin.is_empty() => {
            let plugin = plugin.to_string();
            match rest {
                ["enable"] => {
                    let on = args.first().and_then(number).map_or(true, |v| v > 0.5);
                    Ok(OscAction::Enable { plugin, enabled: on })
                }
                ["bypass"] => {
                    let on = args.first().and_then(number).map_or(true, |v| v > 0.5);
                    Ok(OscAction::Enable { plugin, enabled: !on })
                }
                ["toggle"] => {
                    // A button sends 1 on press and 0 on release: act on press.
                    match args.first().and_then(number) {
                        Some(v) if v <= 0.5 => Err("toggle release ignored".into()),
                        _ => Ok(OscAction::Toggle { plugin }),
                    }
                }
                ["preset"] => args
                    .first()
                    .and_then(string)
                    .map(|name| OscAction::Preset { plugin, name })
                    .ok_or_else(|| "preset needs a name argument".to_string()),
                ["params"] => Ok(OscAction::ListParams { plugin }),
                ["param", param] | ["param", param, "normalized"] if !param.is_empty() => {
                    let normalized = rest.len() == 3;
                    let value = args
                        .first()
                        .and_then(number)
                        .ok_or_else(|| "parameter needs a number argument".to_string())?;
                    Ok(OscAction::Param {
                        plugin,
                        param: param.to_string(),
                        value,
                        normalized,
                    })
                }
                _ => Err(format!("unknown address {addr}")),
            }
        }
        _ => Err(format!("unknown address {addr}")),
    }
}

fn flatten(packet: OscPacket, out: &mut Vec<OscMessage>) {
    match packet {
        OscPacket::Message(m) => out.push(m),
        OscPacket::Bundle(b) => b.content.into_iter().for_each(|p| flatten(p, out)),
    }
}

pub struct OscServer {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    rx: Receiver<OscRequest>,
    socket: UdpSocket,
}

impl OscServer {
    pub fn start(listen_all: bool, port: u16) -> std::io::Result<Self> {
        let host = if listen_all { "0.0.0.0" } else { "127.0.0.1" };
        let socket = UdpSocket::bind((host, port))?;
        socket.set_read_timeout(Some(Duration::from_millis(200)))?;
        let reader = socket.try_clone()?;
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let thread = {
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("osc".into())
                .spawn(move || serve(reader, tx, stop))?
        };
        log::info!("OSC listening on {host}:{port}");
        Ok(Self {
            stop,
            thread: Some(thread),
            rx,
            socket,
        })
    }

    /// Requests received since the last call. A parameter set again before
    /// it is applied only counts once, so a fast fader doesn't pile up.
    pub fn drain(&self) -> Vec<OscRequest> {
        let mut out: Vec<OscRequest> = Vec::new();
        while let Ok(req) = self.rx.try_recv() {
            if let OscAction::Param {
                plugin,
                param,
                normalized,
                ..
            } = &req.action
                && let Some(existing) = out.iter_mut().find(|r| {
                    matches!(&r.action, OscAction::Param { plugin: p, param: q, normalized: n, .. }
                        if p == plugin && q == param && n == normalized)
                })
            {
                *existing = req;
                continue;
            }
            out.push(req);
        }
        out
    }

    pub fn reply(&self, to: SocketAddr, addr: &str, args: Vec<OscType>) {
        let packet = OscPacket::Message(OscMessage {
            addr: addr.to_string(),
            args,
        });
        if let Ok(bytes) = rosc::encoder::encode(&packet) {
            let _ = self.socket.send_to(&bytes, to);
        }
    }
}

impl Drop for OscServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn serve(socket: UdpSocket, tx: Sender<OscRequest>, stop: Arc<AtomicBool>) {
    let mut buf = vec![0u8; 65536];
    while !stop.load(Ordering::Relaxed) {
        let (n, from) = match socket.recv_from(&mut buf) {
            Ok(r) => r,
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => continue,
            Err(e) => {
                log::warn!("OSC receive failed: {e}");
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
        };
        let packet = match rosc::decoder::decode_udp(&buf[..n]) {
            Ok((_, packet)) => packet,
            Err(e) => {
                log::debug!("OSC: undecodable packet from {from}: {e:?}");
                continue;
            }
        };
        let mut messages = Vec::new();
        flatten(packet, &mut messages);
        for m in messages {
            match parse(&m.addr, &m.args) {
                Ok(action) => {
                    if tx.send(OscRequest { action, from }).is_err() {
                        return;
                    }
                }
                Err(why) => log::debug!("OSC: ignored {} from {from}: {why}", m.addr),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(v: f32) -> Vec<OscType> {
        vec![OscType::Float(v)]
    }

    #[test]
    fn slug_folds_names() {
        assert_eq!(slug("Reverb 1"), "reverb_1");
        assert_eq!(slug("  LSP -- Compressor (stereo) "), "lsp_compressor_stereo");
        assert_eq!(slug("reverb_1"), "reverb_1");
    }

    #[test]
    fn enable_and_bypass() {
        assert_eq!(
            parse("/zestbay/plugin/reverb/enable", &f(1.0)),
            Ok(OscAction::Enable { plugin: "reverb".into(), enabled: true })
        );
        assert_eq!(
            parse("/zestbay/plugin/reverb/enable", &[OscType::Int(0)]),
            Ok(OscAction::Enable { plugin: "reverb".into(), enabled: false })
        );
        assert_eq!(
            parse("/zestbay/plugin/reverb/bypass", &[OscType::Bool(true)]),
            Ok(OscAction::Enable { plugin: "reverb".into(), enabled: false })
        );
        // No argument means "on" (enable) / "bypassed" (bypass).
        assert_eq!(
            parse("/zestbay/plugin/reverb/enable", &[]),
            Ok(OscAction::Enable { plugin: "reverb".into(), enabled: true })
        );
    }

    #[test]
    fn toggle_acts_on_press_only() {
        assert_eq!(
            parse("/zestbay/plugin/eq/toggle", &f(1.0)),
            Ok(OscAction::Toggle { plugin: "eq".into() })
        );
        assert_eq!(
            parse("/zestbay/plugin/eq/toggle", &[]),
            Ok(OscAction::Toggle { plugin: "eq".into() })
        );
        assert!(parse("/zestbay/plugin/eq/toggle", &f(0.0)).is_err());
    }

    #[test]
    fn parameters() {
        assert_eq!(
            parse("/zestbay/plugin/comp/param/threshold", &[OscType::Int(-12)]),
            Ok(OscAction::Param {
                plugin: "comp".into(),
                param: "threshold".into(),
                value: -12.0,
                normalized: false
            })
        );
        assert_eq!(
            parse("/zestbay/plugin/comp/param/threshold/normalized", &f(0.5)),
            Ok(OscAction::Param {
                plugin: "comp".into(),
                param: "threshold".into(),
                value: 0.5,
                normalized: true
            })
        );
        assert!(parse("/zestbay/plugin/comp/param/threshold", &[]).is_err());
        assert!(parse("/zestbay/plugin/comp/param/threshold", &f(f32::NAN)).is_err());
    }

    #[test]
    fn presets_and_queries() {
        assert_eq!(
            parse("/zestbay/plugin/comp/preset", &[OscType::String("Vocal".into())]),
            Ok(OscAction::Preset { plugin: "comp".into(), name: "Vocal".into() })
        );
        assert!(parse("/zestbay/plugin/comp/preset", &f(1.0)).is_err());
        assert_eq!(parse("/zestbay/plugins", &[]), Ok(OscAction::ListPlugins));
        assert_eq!(
            parse("/zestbay/plugin/comp/params", &[]),
            Ok(OscAction::ListParams { plugin: "comp".into() })
        );
    }

    #[test]
    fn sessions() {
        assert_eq!(parse("/zestbay/sessions", &[]), Ok(OscAction::ListSessions));
        assert_eq!(
            parse("/zestbay/session/load", &[OscType::String("live".into())]),
            Ok(OscAction::LoadSession { name: "live".into() })
        );
        assert!(parse("/zestbay/session/load", &[]).is_err());
        assert!(parse("/zestbay/session/load", &f(1.0)).is_err());
    }

    #[test]
    fn links_and_unknown_addresses_are_not_controllable() {
        assert!(parse("/zestbay/link/connect", &[]).is_err());
        assert!(parse("/zestbay/plugin/comp/nonsense", &[]).is_err());
        assert!(parse("/other/plugins", &[]).is_err());
        assert!(parse("/zestbay/plugin//enable", &f(1.0)).is_err());
    }

    #[test]
    fn server_receives_and_coalesces() {
        let server = OscServer::start(false, 0).expect("bind");
        let port = server.socket.local_addr().unwrap().port();
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        let send = |addr: &str, args: Vec<OscType>| {
            let bytes = rosc::encoder::encode(&OscPacket::Message(OscMessage {
                addr: addr.into(),
                args,
            }))
            .unwrap();
            client.send_to(&bytes, ("127.0.0.1", port)).unwrap();
        };
        send("/zestbay/plugin/a/param/gain", f(1.0));
        send("/zestbay/plugin/a/param/gain", f(2.0));
        send("/zestbay/plugin/a/enable", f(0.0));
        std::thread::sleep(Duration::from_millis(400));
        let got = server.drain();
        assert_eq!(got.len(), 2, "{got:?}");
        assert!(matches!(&got[0].action, OscAction::Param { value, .. } if *value == 2.0));
        assert!(matches!(&got[1].action, OscAction::Enable { enabled: false, .. }));
    }
}
