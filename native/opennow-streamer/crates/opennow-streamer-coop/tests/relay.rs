//! Relay path through a real coturn 4.18 started in Docker with 'use-auth-secret'.
//!
//! Ignored by default because it needs Docker. Run it with
//! 'cargo test -p opennow-streamer-coop --test relay -- --ignored --nocapture'.
//! Host candidates are filtered from signaling, so the direct attempt times out and media
//! must cross the relay over UDP, TCP or TLS TURN; both sides must report path "relay" with
//! the configured site id.

use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use hmac::{Hmac, Mac};
use opennow_streamer_coop::turn::tls_config_with_roots;
use opennow_streamer_coop::{
    CoopGuest, CoopHost, GuestConfig, HostConfig, IceConfig, NetConfig, RumbleRouter,
};
use opennow_streamer_transport::{CoopMediaKind, CoopRtpPacket};
use serde_json::{Value, json};
use sha1::Sha1;

mod common;
use common::*;

const IMAGE: &str = "coturn/coturn:4.18.0-r0-debian";
const SECRET: &str = "opennow-relay-test-secret";
const REALM: &str = "opennow.test";
const PORT: u16 = 34_780;
const TLS_PORT: u16 = 53_490;

struct Coturn {
    name: String,
    certs: std::path::PathBuf,
    root: rustls::pki_types::CertificateDer<'static>,
}

impl Coturn {
    fn start() -> Self {
        let certified = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()]).unwrap();
        let certs = std::env::temp_dir().join(format!("opennow-coturn-{}", std::process::id()));
        std::fs::create_dir_all(&certs).unwrap();
        std::fs::write(
            certs.join("cert.pem"),
            pem("CERTIFICATE", certified.cert.der()),
        )
        .unwrap();
        std::fs::write(
            certs.join("key.pem"),
            pem("PRIVATE KEY", &certified.signing_key.serialize_der()),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for file in ["cert.pem", "key.pem"] {
                std::fs::set_permissions(certs.join(file), std::fs::Permissions::from_mode(0o644))
                    .unwrap();
            }
            std::fs::set_permissions(&certs, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let name = format!("opennow-coturn-test-{}", std::process::id());
        let _ = Command::new("docker").args(["rm", "-f", &name]).output();
        let status = Command::new("docker")
            .args(["run", "-d", "--rm", "--name", &name, "--network", "host"])
            .arg("-v")
            .arg(format!("{}:/certs:ro", certs.display()))
            .arg(IMAGE)
            .args([
                "-n",
                "--log-file=stdout",
                "--listening-ip=127.0.0.1",
                "--relay-ip=127.0.0.1",
                "--allow-loopback-peers",
                "--use-auth-secret",
                &format!("--static-auth-secret={SECRET}"),
                &format!("--realm={REALM}"),
                &format!("--listening-port={PORT}"),
                &format!("--tls-listening-port={TLS_PORT}"),
                "--min-port=49160",
                "--max-port=49200",
                "--cert=/certs/cert.pem",
                "--pkey=/certs/key.pem",
                "--no-cli",
                "--fingerprint",
            ])
            .status()
            .expect("docker is available");
        assert!(status.success(), "coturn container started");
        std::thread::sleep(Duration::from_secs(2));
        Self {
            name,
            certs,
            root: certified.cert.der().clone(),
        }
    }

    fn logs(&self) -> String {
        let output = Command::new("docker")
            .args(["logs", &self.name])
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).into_owned()
            + &String::from_utf8_lossy(&output.stderr)
    }
}

impl Drop for Coturn {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.name])
            .output();
        let _ = std::fs::remove_dir_all(&self.certs);
    }
}

fn pem(label: &str, der: &[u8]) -> String {
    let body = base64::engine::general_purpose::STANDARD.encode(der);
    let lines: Vec<&str> = body
        .as_bytes()
        .chunks(64)
        .map(|line| std::str::from_utf8(line).unwrap())
        .collect();
    format!(
        "-----BEGIN {label}-----
{}
-----END {label}-----
",
        lines.join(
            "
"
        )
    )
}

/// TURN REST API credentials, as the backend mints them for 'use-auth-secret'.
fn credentials(user: &str, secret: &str) -> (String, String) {
    let expiry = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 6 * 3_600;
    let username = format!("{expiry}:{user}");
    let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(username.as_bytes());
    let credential = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    (username, credential)
}

fn ice(url: &str, secret: &str) -> IceConfig {
    let (username, credential) = credentials("usr_relay_test", secret);
    IceConfig::from_json(
        &json!([{"urls":[url],"username":username,"credential":credential}]),
        Some(&json!([{"id":"nl","name":"Netherlands","stunUrl":format!("stun:127.0.0.1:{PORT}"),"turnUrls":[url]}])),
    )
    .unwrap()
}

fn relay_net(coturn: &Coturn) -> NetConfig {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(coturn.root.clone()).unwrap();
    NetConfig {
        host_ips: Some(vec!["127.0.0.1".parse().unwrap()]),
        allow_direct: true,
        direct_timeout: Duration::from_millis(300),
        tls: Some(tls_config_with_roots(roots)),
    }
}

/// Blocks the direct path: host candidates never reach the other side, as behind a
/// firewall that drops peer-to-peer UDP.
fn without_host_candidates(payload: &Value) -> Option<Value> {
    if payload["candidate"]
        .as_str()
        .is_some_and(|candidate| candidate.contains("typ host"))
    {
        return None;
    }
    let mut payload = payload.clone();
    if let Some(sdp) = payload["sdp"].as_str() {
        let kept: Vec<&str> = sdp
            .split(
                "
",
            )
            .filter(|line| !line.contains("typ host"))
            .collect();
        payload["sdp"] = json!(kept.join(
            "
"
        ));
    }
    Some(payload)
}

struct Run {
    guest_states: Vec<Value>,
    host_events: Vec<Value>,
    frames: usize,
}

fn run(
    host_ice: IceConfig,
    guest_ice: IceConfig,
    host_net: NetConfig,
    guest_net: NetConfig,
) -> Run {
    let backend = Arc::new(Backend::default());
    let (tap, tap_receiver) = sync_channel::<CoopRtpPacket>(4_096);
    let (host_events, host_receiver) = sink();
    let host = CoopHost::start(
        HostConfig {
            ice: host_ice,
            max_guests: 3,
            net: host_net,
            stream: description(),
        },
        tap_receiver,
        backend,
        Arc::new(RumbleRouter::default()),
        host_events,
    )
    .unwrap();
    let (frames_sender, frames_receiver) = sync_channel(1);
    let (guest_events, guest_receiver) = sink();
    let mut guest = CoopGuest::start(
        GuestConfig {
            ice: guest_ice,
            net: guest_net,
            host_peer_id: "host".to_owned(),
            slot: 1,
        },
        Box::new(Media {
            frames: frames_sender,
            gamepad: Arc::new(Mutex::new(None)),
            stopped: Arc::new(AtomicBool::new(false)),
            taken: 0,
            first_frame: None,
        }),
        guest_events,
    )
    .unwrap();
    host.add_peer("guest-1", 1).unwrap();
    let started = Instant::now();
    let mut host_seen = Vec::new();
    let mut guest_seen: Vec<Value> = Vec::new();
    let mut media = None;
    while media.is_none() && started.elapsed() < Duration::from_secs(15) {
        if guest_seen.iter().any(|value| value["state"] == "ended") {
            break;
        }
        while let Ok(value) = host_receiver.try_recv() {
            if value["type"] == "coop-signal-out"
                && let Some(payload) = without_host_candidates(&value["payload"])
            {
                let _ = guest.signal("host", &payload);
            }
            host_seen.push(value);
        }
        while let Ok(value) = guest_receiver.try_recv() {
            if value["type"] == "coop-signal-out"
                && let Some(payload) = without_host_candidates(&value["payload"])
            {
                let _ = host.signal("guest-1", &payload);
            }
            guest_seen.push(value);
        }
        media = frames_receiver.try_recv().ok();
        std::thread::sleep(Duration::from_millis(2));
    }
    let mut frames = 0;
    if let Some(media) = media {
        let mut seq = 100_u16;
        for index in 0..20 {
            let (packets, _) = frame_packets(index, seq);
            seq = seq.wrapping_add(packets.len() as u16);
            for packet in packets {
                tap.send(CoopRtpPacket {
                    kind: CoopMediaKind::Video,
                    packet: Arc::from(packet),
                    received_at: Instant::now(),
                })
                .unwrap();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        while media.recv_timeout(Duration::from_secs(2)).is_ok() {
            frames += 1;
            if frames == 20 {
                break;
            }
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline
            && !host_seen.iter().any(|value| {
                value["type"] == "coop-host-stats" && value["peers"][0]["path"] == "relay"
            })
        {
            while let Ok(value) = host_receiver.try_recv() {
                host_seen.push(value);
            }
            while let Ok(value) = guest_receiver.try_recv() {
                guest_seen.push(value);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    guest.stop();
    while let Ok(value) = guest_receiver.try_recv() {
        guest_seen.push(value);
    }
    drop(host);
    Run {
        guest_states: guest_seen
            .into_iter()
            .filter(|value| value["type"] == "coop-guest-state")
            .collect(),
        host_events: host_seen,
        frames,
    }
}

#[test]
#[ignore = "needs Docker and the coturn image"]
fn relay_carries_media_over_udp_tcp_and_tls_turn() {
    let coturn = Coturn::start();
    for url in [
        format!("turn:127.0.0.1:{PORT}?transport=udp"),
        format!("turn:127.0.0.1:{PORT}?transport=tcp"),
        format!("turns:127.0.0.1:{TLS_PORT}?transport=tcp"),
    ] {
        let result = run(
            ice(&url, SECRET),
            ice(&url, SECRET),
            relay_net(&coturn),
            relay_net(&coturn),
        );
        let states: Vec<&str> = result
            .guest_states
            .iter()
            .map(|value| value["state"].as_str().unwrap())
            .collect();
        println!("{url}: guest states {states:?}, frames {}", result.frames);
        assert_eq!(
            result.frames,
            20,
            "{url}: frames over the relay; logs:\n{}",
            coturn.logs()
        );
        assert_eq!(
            states[..6],
            [
                "negotiating",
                "trying_direct",
                "trying_relay",
                "direct_blocked",
                "relay",
                "connected"
            ],
            "{url}"
        );
        let connected = result
            .guest_states
            .iter()
            .rev()
            .find(|value| value["state"] == "connected")
            .unwrap();
        assert_eq!(connected["path"], "relay", "{url}: guest path");
        assert_eq!(connected["relayId"], "nl", "{url}: guest relay site");
        let stats = result
            .host_events
            .iter()
            .rev()
            .find(|value| {
                value["type"] == "coop-host-stats" && !value["peers"].as_array().unwrap().is_empty()
            })
            .expect("host stats");
        assert_eq!(stats["peers"][0]["path"], "relay", "{url}: host path");
        assert_eq!(stats["peers"][0]["relayId"], "nl", "{url}: host relay site");
    }
}

#[test]
#[ignore = "needs Docker and the coturn image"]
fn rejected_turn_credentials_end_the_guest_with_a_refresh_reason() {
    let coturn = Coturn::start();
    let url = format!("turn:127.0.0.1:{PORT}?transport=udp");
    let result = run(
        ice(&url, SECRET),
        ice(&url, "wrong-secret"),
        relay_net(&coturn),
        relay_net(&coturn),
    );
    let ended = result
        .guest_states
        .iter()
        .find(|value| value["state"] == "ended")
        .unwrap();
    println!("ended: {ended}");
    assert_eq!(ended["reason"], "relay-credentials-rejected");
    assert_eq!(result.frames, 0);
}

/// The live relay: 'OPENNOW_COOP_LIVE_ICE' names a file holding a GET /v1/ice-servers
/// response. The guest runs relay-only and the host normally, so every pair joins the
/// guest's relayed address to the host's own addresses and media crosses the live TURN
/// server over UDP, then TCP, then TLS. Relaying both sides through one relay is not tested:
/// a relay behind a home router without hairpin NAT cannot reach its own public address.
#[test]
#[ignore = "needs a live relay and fresh TURN credentials in OPENNOW_COOP_LIVE_ICE"]
fn live_relay_carries_media_over_udp_tcp_and_tls_turn() {
    let path = std::env::var("OPENNOW_COOP_LIVE_ICE").expect("OPENNOW_COOP_LIVE_ICE");
    let response: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let relays = &response["relays"];
    let servers = response["iceServers"].as_array().unwrap();
    let turn = servers
        .iter()
        .find(|server| server["credential"].is_string())
        .expect("a TURN entry");
    let stun: Vec<&Value> = servers
        .iter()
        .filter(|server| !server["credential"].is_string())
        .collect();
    let guest_net = NetConfig {
        allow_direct: false,
        ..NetConfig::default()
    };
    for url in turn["urls"].as_array().unwrap() {
        let url = url.as_str().unwrap();
        let mut servers = stun.clone();
        let turn =
            json!({"urls":[url],"username":turn["username"],"credential":turn["credential"]});
        servers.push(&turn);
        let ice = IceConfig::from_json(&json!(servers), Some(relays)).unwrap();
        let started = Instant::now();
        let result = run(ice.clone(), ice, NetConfig::default(), guest_net.clone());
        let states: Vec<&str> = result
            .guest_states
            .iter()
            .map(|value| value["state"].as_str().unwrap())
            .collect();
        let ended = result
            .guest_states
            .iter()
            .find(|value| value["state"] == "ended");
        println!(
            "{url}: guest states {states:?}, frames {}, {:?}, end {}",
            result.frames,
            started.elapsed(),
            ended.map_or(Value::Null, |value| value["reason"].clone())
        );
        assert_eq!(result.frames, 20, "{url}: frames over the relay");
        let connected = result
            .guest_states
            .iter()
            .rev()
            .find(|value| value["state"] == "connected")
            .unwrap();
        assert_eq!(connected["path"], "relay", "{url}: guest path");
        let stats = result
            .host_events
            .iter()
            .rev()
            .find(|value| {
                value["type"] == "coop-host-stats" && !value["peers"].as_array().unwrap().is_empty()
            })
            .expect("host stats");
        assert_eq!(stats["peers"][0]["path"], "relay", "{url}: host path");
        println!(
            "{url}: guest relayId {}, host relayId {}",
            connected["relayId"], stats["peers"][0]["relayId"]
        );
    }
}
