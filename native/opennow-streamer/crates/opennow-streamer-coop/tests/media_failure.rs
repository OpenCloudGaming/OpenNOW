//! A guest whose local media cannot start ends with a precise reason and tells the host.

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use opennow_streamer_coop::{
    CoopGuest, CoopHost, GamepadState, GuestConfig, GuestMedia, HostConfig, IceConfig,
    InputSamples, RumbleRouter, StreamDescription,
};
use opennow_streamer_transport::MediaConsumer;
use serde_json::Value;
use std::sync::Arc;
use std::sync::mpsc::sync_channel;

mod common;
use common::*;

const DECODER_REASON: &str =
    "No compatible embedded auto decoder supports the negotiated h264 color profile";

struct UndecodableMedia;

impl GuestMedia for UndecodableMedia {
    fn start(&mut self, description: &StreamDescription) -> Result<MediaConsumer, String> {
        assert_eq!(description.codec, "H264");
        Err(DECODER_REASON.to_owned())
    }

    fn take_gamepad(&mut self) -> Option<GamepadState> {
        None
    }

    fn input_samples(&self) -> InputSamples {
        InputSamples::default()
    }

    fn take_keyframe_request(&mut self) -> bool {
        false
    }

    fn stop(&mut self) {}
}

fn relay(from: &Receiver<Value>, seen: &mut Vec<Value>, deliver: impl Fn(&Value)) {
    while let Ok(value) = from.try_recv() {
        if value["type"] == "coop-signal-out" {
            deliver(&value["payload"]);
        }
        seen.push(value);
    }
}

#[test]
fn a_guest_that_cannot_start_media_ends_with_the_decoder_reason_and_says_bye() {
    let (_tap, tap_receiver) = sync_channel(16);
    let (host_events, host_receiver) = sink();
    let mut host = CoopHost::start(
        HostConfig {
            ice: IceConfig::default(),
            max_guests: 3,
            net: loopback_net(),
            stream: description(),
        },
        tap_receiver,
        Arc::new(Backend::default()),
        Arc::new(RumbleRouter::default()),
        host_events,
    )
    .unwrap();
    let (guest_events, guest_receiver) = sink();
    let mut guest = CoopGuest::start(
        GuestConfig {
            ice: IceConfig::default(),
            net: loopback_net(),
            host_peer_id: "host".to_owned(),
            slot: 1,
        },
        Box::new(UndecodableMedia),
        guest_events,
    )
    .unwrap();
    host.add_peer("guest-1", 1).unwrap();

    let started = Instant::now();
    let (mut host_seen, mut guest_seen) = (Vec::new(), Vec::new());
    let ended = loop {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "guest never ended; host={host_seen:#?} guest={guest_seen:#?}"
        );
        relay(&host_receiver, &mut host_seen, |payload| {
            guest.signal("host", payload).unwrap();
        });
        relay(&guest_receiver, &mut guest_seen, |payload| {
            let _ = host.signal("guest-1", payload);
        });
        if let Some(ended) = guest_seen
            .iter()
            .find(|value| value["type"] == "coop-guest-state" && value["state"] == "ended")
        {
            break ended.clone();
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(
        ended["reason"],
        format!("media-unavailable: {DECODER_REASON}")
    );
    let bye = guest_seen
        .iter()
        .find(|value| value["type"] == "coop-signal-out" && value["payload"]["kind"] == "bye")
        .expect("the guest tells the host why it left");
    assert_eq!(bye["payload"]["reason"], ended["reason"]);
    wait_for(
        &host_receiver,
        &mut host_seen,
        Duration::from_secs(3),
        |value| value["type"] == "coop-peer-state" && value["state"] == "disconnected",
    );
    guest.stop();
    host.stop();
}
