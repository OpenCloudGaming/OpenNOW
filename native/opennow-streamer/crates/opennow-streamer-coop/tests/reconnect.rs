//! A direct link that goes silent mid-stream: both sides must show 'reconnecting' within
//! 3 s, the host must replace the link through the signal path, and video, input and a
//! fresh keyframe must flow on the new link.

use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opennow_streamer_coop::{
    CoopGuest, CoopHost, GamepadState, GuestConfig, HostConfig, IceConfig, RumbleRouter,
};
use opennow_streamer_transport::{CoopMediaKind, CoopRtpPacket, INPUT_COOP_GAMEPAD};
use serde_json::{Value, json};

mod common;
use common::*;

type Real = Arc<Mutex<Option<SocketAddr>>>;
/// The host and guest sockets of the silenced link.
type Blocked = Arc<Mutex<Option<(Option<SocketAddr>, Option<SocketAddr>)>>>;

/// Forwards between the host and guest sockets named in rewritten candidates. While
/// 'blocked' holds the addresses of a link, every datagram to or from them is dropped, so
/// that link goes silent and only a replacement link with new sockets gets through.
struct Proxy {
    host_side: SocketAddr,
    guest_side: SocketAddr,
    host_real: Real,
    guest_real: Real,
    blocked: Blocked,
    stop: Arc<AtomicBool>,
}

impl Proxy {
    fn start() -> Self {
        let host_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let guest_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        host_socket.set_nonblocking(true).unwrap();
        guest_socket.set_nonblocking(true).unwrap();
        let proxy = Self {
            host_side: host_socket.local_addr().unwrap(),
            guest_side: guest_socket.local_addr().unwrap(),
            host_real: Arc::default(),
            guest_real: Arc::default(),
            blocked: Arc::default(),
            stop: Arc::default(),
        };
        let (host_real, guest_real) = (proxy.host_real.clone(), proxy.guest_real.clone());
        let (blocked, stop) = (proxy.blocked.clone(), proxy.stop.clone());
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 2048];
            let silenced = || {
                let blocked = *blocked.lock().unwrap();
                blocked.is_some_and(|(host, guest)| {
                    *host_real.lock().unwrap() == host || *guest_real.lock().unwrap() == guest
                })
            };
            while !stop.load(Ordering::Acquire) {
                let mut idle = true;
                while let Ok((length, _)) = host_socket.recv_from(&mut buffer) {
                    idle = false;
                    let guest = *guest_real.lock().unwrap();
                    if let Some(guest) = guest
                        && !silenced()
                    {
                        let _ = guest_socket.send_to(&buffer[..length], guest);
                    }
                }
                while let Ok((length, _)) = guest_socket.recv_from(&mut buffer) {
                    idle = false;
                    let host = *host_real.lock().unwrap();
                    if let Some(host) = host
                        && !silenced()
                    {
                        let _ = host_socket.send_to(&buffer[..length], host);
                    }
                }
                if idle {
                    std::thread::sleep(Duration::from_micros(100));
                }
            }
        });
        proxy
    }

    fn silence_current_link(&self) {
        *self.blocked.lock().unwrap() = Some((
            *self.host_real.lock().unwrap(),
            *self.guest_real.lock().unwrap(),
        ));
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

fn rewrite(payload: &Value, via: SocketAddr, real: &Mutex<Option<SocketAddr>>) -> Value {
    let mut payload = payload.clone();
    for key in ["sdp", "candidate"] {
        let Some(text) = payload[key].as_str() else {
            continue;
        };
        let rewritten = text
            .split("\r\n")
            .map(|line| {
                let mut tokens: Vec<String> = line.split(' ').map(str::to_owned).collect();
                if line.contains("candidate:") && tokens.len() > 7 && tokens[7] == "host" {
                    let address: SocketAddr =
                        format!("{}:{}", tokens[4], tokens[5]).parse().unwrap();
                    *real.lock().unwrap() = Some(address);
                    tokens[4] = via.ip().to_string();
                    tokens[5] = via.port().to_string();
                }
                tokens.join(" ")
            })
            .collect::<Vec<_>>()
            .join("\r\n");
        payload[key] = json!(rewritten);
    }
    payload
}

fn state_events<'a>(seen: &'a [Value], kind: &str) -> Vec<&'a str> {
    seen.iter()
        .filter(|value| value["type"] == kind)
        .filter_map(|value| value["state"].as_str())
        .collect()
}

#[test]
fn a_silent_link_reaches_reconnecting_within_three_seconds_and_recovers() {
    let proxy = Proxy::start();
    let backend = Arc::new(Backend::default());
    let (tap, tap_receiver) = sync_channel::<CoopRtpPacket>(4_096);
    let (host_events, host_receiver) = sink();
    let host = CoopHost::start(
        HostConfig {
            ice: IceConfig::default(),
            max_guests: 3,
            net: loopback_net(),
            stream: description(),
        },
        tap_receiver,
        backend.clone(),
        Arc::new(RumbleRouter::default()),
        host_events,
    )
    .unwrap();
    let (frames_sender, frames_receiver) = sync_channel(1);
    let gamepad = Arc::new(Mutex::new(None));
    let (guest_events, guest_receiver) = sink();
    let mut guest = CoopGuest::start(
        GuestConfig {
            ice: IceConfig::default(),
            net: loopback_net(),
            host_peer_id: "host".to_owned(),
            slot: 1,
        },
        Box::new(Media {
            frames: frames_sender,
            gamepad: gamepad.clone(),
            stopped: Arc::default(),
            taken: 0,
            first_frame: None,
        }),
        guest_events,
    )
    .unwrap();
    host.add_peer("guest-1", 1).unwrap();

    // GFN: a frame every 4 ms, an IDR whenever the host asked for a keyframe.
    let stop = Arc::new(AtomicBool::new(false));
    let sent_frames = Arc::new(AtomicU32::new(0));
    let feeder = {
        let (stop, sent_frames, backend) = (stop.clone(), sent_frames.clone(), backend.clone());
        std::thread::spawn(move || {
            let mut seq = 1_000_u16;
            let mut answered = 0;
            for index in 0.. {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                let requested = backend.keyframes.load(Ordering::Relaxed);
                let (packets, _) =
                    keyed_frame_packets(index, seq, index == 0 || requested > answered);
                answered = requested;
                seq = seq.wrapping_add(packets.len() as u16);
                for packet in packets {
                    let _ = tap.try_send(CoopRtpPacket {
                        kind: CoopMediaKind::Video,
                        packet: Arc::from(packet),
                        received_at: Instant::now(),
                    });
                }
                sent_frames.store(index, Ordering::Release);
                std::thread::sleep(Duration::from_millis(4));
            }
        })
    };

    let mut host_seen = Vec::new();
    let mut guest_seen = Vec::new();
    let delivered: Arc<Mutex<Vec<(Instant, u32)>>> = Arc::default();
    let mut collector = None;
    let mut relay_until = |condition: &dyn Fn(&[Value], &[Value]) -> bool,
                           limit: Duration,
                           host_seen: &mut Vec<Value>,
                           guest_seen: &mut Vec<Value>| {
        let end = Instant::now() + limit;
        while !condition(host_seen, guest_seen) {
            assert!(
                Instant::now() < end,
                "timed out; host={host_seen:#?} guest={guest_seen:#?}"
            );
            while let Ok(value) = host_receiver.try_recv() {
                if value["type"] == "coop-signal-out" {
                    let payload = rewrite(&value["payload"], proxy.guest_side, &proxy.host_real);
                    let _ = guest.signal("host", &payload);
                }
                host_seen.push(value);
            }
            while let Ok(value) = guest_receiver.try_recv() {
                if value["type"] == "coop-signal-out" {
                    let payload = rewrite(&value["payload"], proxy.host_side, &proxy.guest_real);
                    let _ = host.signal("guest-1", &payload);
                }
                guest_seen.push(value);
            }
            if collector.is_none()
                && let Ok(media) = frames_receiver.try_recv()
            {
                let delivered = delivered.clone();
                collector = Some(std::thread::spawn(move || {
                    while let Ok(frame) = media.recv_timeout(Duration::from_secs(20)) {
                        delivered
                            .lock()
                            .unwrap()
                            .push((Instant::now(), frame.frame_index.unwrap_or(0)));
                    }
                }));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    };

    relay_until(
        &|_, guest| state_events(guest, "coop-guest-state").contains(&"streaming"),
        Duration::from_secs(10),
        &mut host_seen,
        &mut guest_seen,
    );
    std::thread::sleep(Duration::from_millis(300));
    let before = delivered.lock().unwrap().len();
    assert!(before > 10, "frames flow before the outage: {before}");

    proxy.silence_current_link();
    let silenced_at = Instant::now();
    let keyframes_before = backend.keyframes.load(Ordering::Relaxed);
    relay_until(
        &|host, guest| {
            state_events(guest, "coop-guest-state").last() == Some(&"reconnecting")
                && state_events(host, "coop-peer-state").last() == Some(&"reconnecting")
        },
        Duration::from_secs(3),
        &mut host_seen,
        &mut guest_seen,
    );
    let detected = silenced_at.elapsed();
    println!("both sides reconnecting after {detected:?}");
    assert!(detected < Duration::from_secs(3));

    relay_until(
        &|host, guest| {
            state_events(guest, "coop-guest-state").last() == Some(&"streaming")
                && state_events(host, "coop-peer-state").last() == Some(&"connected")
        },
        Duration::from_secs(20),
        &mut host_seen,
        &mut guest_seen,
    );
    let recovered = silenced_at.elapsed();
    let restart_offer = host_seen
        .iter()
        .find(|value| {
            value["type"] == "coop-signal-out"
                && value["payload"]["kind"] == "offer"
                && value["payload"]["gen"].as_u64() > Some(0)
        })
        .expect("the host offered a replacement link");
    println!(
        "link replaced (gen {}) and connected after {recovered:?}",
        restart_offer["payload"]["gen"]
    );

    let resumed_from = Instant::now();
    let mut resumed = 0;
    while resumed < 20 {
        assert!(
            resumed_from.elapsed() < Duration::from_secs(10),
            "only {resumed} frames after recovery; sent {}",
            sent_frames.load(Ordering::Acquire)
        );
        resumed = delivered
            .lock()
            .unwrap()
            .iter()
            .filter(|(at, _)| *at > silenced_at + recovered)
            .count();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        backend.keyframes.load(Ordering::Relaxed) > keyframes_before,
        "a keyframe was requested for the new link"
    );

    *gamepad.lock().unwrap() = Some(GamepadState {
        bitmap: 1,
        buttons: 0x2000,
        ..GamepadState::default()
    });
    let input_deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let injected = backend.inputs.lock().unwrap().iter().any(|record| {
            u32::from_le_bytes(record[0..4].try_into().unwrap()) == INPUT_COOP_GAMEPAD
                && u16::from_le_bytes([record[12], record[13]]) == 0x2000
        });
        if injected {
            break;
        }
        assert!(Instant::now() < input_deadline, "input never resumed");
        std::thread::sleep(Duration::from_millis(5));
    }
    let guest_states = state_events(&guest_seen, "coop-guest-state");
    assert!(!guest_states.contains(&"ended"), "{guest_states:?}");

    stop.store(true, Ordering::Release);
    feeder.join().unwrap();
    guest.stop();
    drop(host);
    if let Some(collector) = collector {
        collector.join().unwrap();
    }
}
