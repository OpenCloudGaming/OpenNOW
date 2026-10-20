//! Direct localhost co-op: host fan-out to one guest through a lossy UDP proxy.
//!
//! The test rewrites the host candidates in the SDP so every datagram crosses the proxy,
//! which drops the first copy of one video packet to prove the NACK ring repairs it.

use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opennow_streamer_coop::{
    CoopGuest, CoopHost, GamepadState, GuestConfig, HostConfig, IceConfig, Rumble, RumbleRouter,
};
use opennow_streamer_transport::{
    CoopMediaKind, CoopRtpPacket, INPUT_COOP_GAMEPAD, INPUT_COOP_NEUTRAL, INPUT_COOP_SLOTS,
};
use serde_json::{Value, json};

mod common;
use common::*;

/// Two sockets: 'host_side' faces the host, 'guest_side' faces the guest.
struct Proxy {
    host_side: SocketAddr,
    guest_side: SocketAddr,
    host_real: Arc<Mutex<Option<SocketAddr>>>,
    guest_real: Arc<Mutex<Option<SocketAddr>>>,
    drop_seq: Arc<Mutex<Option<u16>>>,
    copies_of_dropped: Arc<AtomicUsize>,
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
            drop_seq: Arc::default(),
            copies_of_dropped: Arc::default(),
            stop: Arc::default(),
        };
        let (host_real, guest_real) = (proxy.host_real.clone(), proxy.guest_real.clone());
        let (drop_seq, copies, stop) = (
            proxy.drop_seq.clone(),
            proxy.copies_of_dropped.clone(),
            proxy.stop.clone(),
        );
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 2048];
            while !stop.load(Ordering::Acquire) {
                let mut idle = true;
                while let Ok((length, _)) = host_socket.recv_from(&mut buffer) {
                    idle = false;
                    let datagram = &buffer[..length];
                    let is_video = length > 12
                        && (128..192).contains(&datagram[0])
                        && datagram[1] & 0x7f == 96;
                    if is_video {
                        let seq = u16::from_be_bytes([datagram[2], datagram[3]]);
                        let mut target = drop_seq.lock().unwrap();
                        if *target == Some(seq) {
                            if copies.fetch_add(1, Ordering::AcqRel) == 0 {
                                continue;
                            }
                            *target = None;
                        }
                    }
                    if let Some(guest) = *guest_real.lock().unwrap() {
                        let _ = guest_socket.send_to(datagram, guest);
                    }
                }
                while let Ok((length, _)) = guest_socket.recv_from(&mut buffer) {
                    idle = false;
                    if let Some(host) = *host_real.lock().unwrap() {
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
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

/// Replaces the address of every host candidate with 'via' and reports the real one.
fn rewrite_candidates(text: &str, via: SocketAddr, real: &Mutex<Option<SocketAddr>>) -> String {
    text.split("\r\n")
        .map(|line| {
            let mut tokens: Vec<String> = line.split(' ').map(str::to_owned).collect();
            if line.contains("candidate:") && tokens.len() > 7 && tokens[7] == "host" {
                let address: SocketAddr = format!("{}:{}", tokens[4], tokens[5]).parse().unwrap();
                *real.lock().unwrap() = Some(address);
                tokens[4] = via.ip().to_string();
                tokens[5] = via.port().to_string();
            }
            tokens.join(" ")
        })
        .collect::<Vec<_>>()
        .join("\r\n")
}

fn rewrite(payload: &Value, via: SocketAddr, real: &Mutex<Option<SocketAddr>>) -> Value {
    let mut payload = payload.clone();
    for key in ["sdp", "candidate"] {
        if let Some(text) = payload[key].as_str() {
            payload[key] = json!(rewrite_candidates(text, via, real));
        }
    }
    payload
}

#[test]
fn direct_loopback_forwards_repairs_and_injects_input() {
    run_loopback(&[], 65_000, 20);
}

/// The host's own sequence has wrapped before the guest joins; the guest's SRTP context
/// still starts at rollover counter zero. Frame 29 crosses the guest's own wrap and loses
/// its middle packet (wire sequence 2), which the ring must repair.
#[test]
fn late_joiner_after_host_wrap_authenticates_and_repairs_across_its_own_wrap() {
    run_loopback(
        &[65_530, 65_535, 0, 9, 20_000, 40_000, 60_000, 65_440],
        65_450,
        29,
    );
}

/// Feeds 'pre_join_wire' video sequences through the tap before the guest exists, then
/// forwards 60 frames starting at 'first_seq' and drops the first copy of the middle packet
/// of 'lost_frame'.
fn run_loopback(pre_join_wire: &[u16], first_seq: u16, lost_frame: u32) {
    let proxy = Proxy::start();
    let backend = Arc::new(Backend::default());
    let rumble = Arc::new(RumbleRouter::default());
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
        rumble.clone(),
        host_events,
    )
    .unwrap();
    let (frames_sender, frames_receiver) = sync_channel(1);
    let gamepad = Arc::new(Mutex::new(None));
    let stopped = Arc::new(AtomicBool::new(false));
    let (guest_events, guest_receiver) = sink();
    let mut guest = CoopGuest::start(
        GuestConfig {
            ice: IceConfig::default(),
            net: loopback_net(),
            host_peer_id: "host".to_owned(),
            slot: 2,
        },
        Box::new(Media {
            frames: frames_sender,
            gamepad: gamepad.clone(),
            stopped: stopped.clone(),
            taken: 0,
            first_frame: None,
        }),
        guest_events,
    )
    .unwrap();
    for wire in pre_join_wire {
        tap.send(CoopRtpPacket {
            kind: CoopMediaKind::Video,
            packet: Arc::from(nvst_packet(*wire, 0, 0, 0, 0, &[0])),
            received_at: Instant::now(),
        })
        .unwrap();
    }
    std::thread::sleep(Duration::from_millis(50));
    host.add_peer("guest-1", 2).unwrap();

    let started = Instant::now();
    let mut host_seen = Vec::new();
    let mut guest_seen = Vec::new();
    let mut media = None;
    while media.is_none() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "no media; host={host_seen:#?} guest={guest_seen:#?}"
        );
        while let Ok(value) = host_receiver.try_recv() {
            if value["type"] == "coop-signal-out" {
                let payload = rewrite(&value["payload"], proxy.guest_side, &proxy.host_real);
                guest.signal("host", &payload).unwrap();
            }
            host_seen.push(value);
        }
        while let Ok(value) = guest_receiver.try_recv() {
            if value["type"] == "coop-signal-out" {
                let payload = rewrite(&value["payload"], proxy.host_side, &proxy.guest_real);
                host.signal("guest-1", &payload).unwrap();
            }
            guest_seen.push(value);
        }
        media = frames_receiver.try_recv().ok();
        std::thread::sleep(Duration::from_millis(1));
    }
    let media = media.unwrap();
    println!("connected and described in {:?}", started.elapsed());
    let joined = Instant::now();
    while backend.keyframes.load(Ordering::Relaxed) < 2 {
        assert!(
            joined.elapsed() < Duration::from_millis(500),
            "the host's request on connect and the guest's first request both reach GFN at once"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let states: Vec<_> = guest_seen
        .iter()
        .filter(|value| value["type"] == "coop-guest-state")
        .map(|value| value["state"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(states[..3], ["negotiating", "trying_direct", "connected"]);

    let frames = 60_u32;
    let collector = std::thread::spawn(move || {
        let mut received = Vec::new();
        while received.len() < frames as usize {
            let Ok(frame) = media.recv_timeout(Duration::from_secs(3)) else {
                break;
            };
            received.push((Instant::now(), frame));
        }
        received
    });
    let mut seq = first_seq;
    let mut expected = Vec::new();
    let mut sent_at = Vec::new();
    for index in 0..frames {
        let (packets, bytes) = frame_packets(index, seq);
        if index == lost_frame {
            *proxy.drop_seq.lock().unwrap() = Some(seq.wrapping_add(1));
        }
        seq = seq.wrapping_add(packets.len() as u16);
        expected.push(bytes);
        for packet in packets {
            tap.send(CoopRtpPacket {
                kind: CoopMediaKind::Video,
                packet: Arc::from(packet),
                received_at: Instant::now(),
            })
            .unwrap();
        }
        sent_at.push(Instant::now());
        std::thread::sleep(Duration::from_millis(4));
    }
    let collected = collector.join().unwrap();
    let missing: Vec<u32> = (0..frames)
        .filter(|index| {
            !collected
                .iter()
                .any(|(_, frame)| frame.frame_index == Some(index + 1))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "frames {missing:?} never reached the guest decoder queue; dropped-packet copies: {}",
        proxy.copies_of_dropped.load(Ordering::Acquire)
    );
    let mut latencies = Vec::new();
    let mut received = Vec::new();
    for (arrived, frame) in collected {
        let index = frame.frame_index.unwrap() as usize - 1;
        latencies.push(arrived.saturating_duration_since(sent_at[index]));
        received.push((index, frame));
    }
    for (index, frame) in &received {
        assert_eq!(
            &frame.payload[..],
            &expected[*index][..],
            "frame {index} differs"
        );
        assert_eq!(
            frame.rtp_timestamp,
            u64::from(90_000 + *index as u32 * 1_500)
        );
    }
    let order: Vec<usize> = received.iter().map(|(index, _)| *index).collect();
    assert_eq!(order, (0..frames as usize).collect::<Vec<_>>());
    assert_eq!(
        proxy.copies_of_dropped.load(Ordering::Acquire),
        2,
        "the dropped packet was retransmitted once"
    );
    let mut sorted = latencies.clone();
    sorted.sort();
    let repaired = latencies[lost_frame as usize];
    sorted.retain(|latency| *latency != repaired);
    println!(
        "forwarding latency over {} frames: p50={:?} p95={:?} max={:?}; repaired frame {:?}",
        sorted.len(),
        sorted[sorted.len() / 2],
        sorted[sorted.len() * 95 / 100],
        sorted[sorted.len() - 1],
        repaired,
    );
    assert!(sorted[sorted.len() / 2] < Duration::from_millis(20));

    *gamepad.lock().unwrap() = Some(GamepadState {
        bitmap: 1,
        buttons: 0x1000,
        left_trigger: 200,
        right_trigger: 0,
        left_stick_x: 1_234,
        left_stick_y: -1_234,
        right_stick_x: 0,
        right_stick_y: 0,
    });
    let input_deadline = Instant::now() + Duration::from_secs(2);
    let gamepad_record = loop {
        let found = backend
            .inputs
            .lock()
            .unwrap()
            .iter()
            .find(|record| {
                u32::from_le_bytes(record[0..4].try_into().unwrap()) == INPUT_COOP_GAMEPAD
                    && u16::from_le_bytes([record[12], record[13]]) == 0x1000
            })
            .cloned();
        if let Some(record) = found {
            break record;
        }
        assert!(
            Instant::now() < input_deadline,
            "guest input never reached the host"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(
        u16::from_le_bytes([gamepad_record[6], gamepad_record[7]]),
        2
    );
    assert_eq!(
        i16::from_le_bytes([gamepad_record[16], gamepad_record[17]]),
        1_234
    );
    {
        let inputs = backend.inputs.lock().unwrap();
        let kind = |record: &Vec<u8>| u32::from_le_bytes(record[0..4].try_into().unwrap());
        let registration = inputs
            .iter()
            .position(|record| kind(record) == INPUT_COOP_SLOTS && record[4] != 0)
            .expect("the guest slot is registered");
        let first_gamepad = inputs
            .iter()
            .position(|record| kind(record) == INPUT_COOP_GAMEPAD)
            .unwrap();
        assert_eq!(inputs[registration][4], 0b0100);
        assert!(
            registration < first_gamepad,
            "the slot is registered before its first report"
        );
    }

    assert!(rumble.route(
        2,
        Rumble {
            low_frequency: 100,
            high_frequency: 200,
            duration_ms: 50
        }
    ));
    let rumble_event = wait_for(
        &guest_receiver,
        &mut guest_seen,
        Duration::from_secs(2),
        |value| value["type"] == "controller-rumble",
    );
    assert_eq!(rumble_event["controllerId"], 0);
    assert_eq!(rumble_event["lowFrequency"], 100);

    let stats = wait_for(
        &guest_receiver,
        &mut guest_seen,
        Duration::from_secs(3),
        |value| {
            value["type"] == "coop-guest-stats"
                && value["hostToGfnPingMs"] == 11.0
                && value["inputStatesSent"].as_u64() > Some(0)
        },
    );
    assert_eq!(stats["slot"], 2);
    assert_eq!(
        (&stats["codec"], &stats["decoder"]),
        (&json!("H264"), &json!("hardware"))
    );
    let connect_ms = stats["connectMs"].as_u64().expect("connectMs");
    assert!(stats["firstFrameMs"].as_u64().expect("firstFrameMs") >= connect_ms);
    let states: Vec<&str> = guest_seen
        .iter()
        .filter(|value| value["type"] == "coop-guest-state")
        .filter_map(|value| value["state"].as_str())
        .collect();
    let connected = states
        .iter()
        .position(|state| *state == "connected")
        .unwrap();
    assert_eq!(
        states.iter().position(|state| *state == "streaming"),
        Some(connected + 1),
        "{states:?}"
    );
    assert_eq!(
        (&stats["inputSamplesPlain"], &stats["inputSamplesSony"]),
        (&json!(1), &json!(0))
    );
    let host_stats = wait_for(
        &host_receiver,
        &mut host_seen,
        Duration::from_secs(3),
        |value| {
            value["type"] == "coop-host-stats"
                && value["peers"][0]["path"] == "direct"
                && value["peers"][0]["inputInjected"].as_u64() > Some(0)
        },
    );
    let peer = &host_stats["peers"][0];
    assert!(peer["inputReceived"].as_u64() >= peer["inputInjected"].as_u64());
    for dropped in [
        "inputDroppedMuted",
        "inputDroppedSlotNotOwned",
        "inputDroppedMalformed",
        "inputDroppedNvstRefused",
    ] {
        assert_eq!(peer[dropped], 0, "{dropped}: {peer}");
    }
    host.set_muted("guest-1", true).unwrap();
    wait_for(
        &host_receiver,
        &mut host_seen,
        Duration::from_secs(3),
        |value| {
            value["type"] == "coop-host-stats"
                && value["peers"][0]["inputDroppedMuted"].as_u64() > Some(0)
        },
    );
    host.set_muted("guest-1", false).unwrap();
    println!("host stats: {host_stats}");
    println!("guest stats: {stats}");
    assert!(backend.keyframes.load(Ordering::Relaxed) <= 2 + started.elapsed().as_secs() as usize);

    guest.stop();
    assert!(stopped.load(Ordering::Acquire));
    let ended = wait_for(
        &guest_receiver,
        &mut guest_seen,
        Duration::from_secs(1),
        |value| value["type"] == "coop-guest-state" && value["state"] == "ended",
    );
    assert_eq!(ended["reason"], "left");
    host.remove_peer("guest-1", "left").unwrap();
    let inputs = backend.inputs.lock().unwrap();
    let kinds: Vec<u32> = inputs
        .iter()
        .map(|record| u32::from_le_bytes(record[0..4].try_into().unwrap()))
        .collect();
    let neutral = kinds
        .iter()
        .rposition(|kind| *kind == INPUT_COOP_NEUTRAL)
        .expect("neutral on leave");
    let last = inputs.last().unwrap();
    assert!(neutral < inputs.len());
    assert_eq!(
        u32::from_le_bytes(last[0..4].try_into().unwrap()),
        INPUT_COOP_SLOTS
    );
    assert_eq!(last[4], 0);
}
