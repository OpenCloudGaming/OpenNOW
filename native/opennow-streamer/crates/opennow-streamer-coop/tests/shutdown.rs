//! Guest teardown must finish in bounded time: the media consumer thread exits only after
//! every sender into its queue is gone, so the guest drops its receiver before joining it.

use std::mem::ManuallyDrop;
use std::sync::mpsc::{Receiver, channel, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use opennow_streamer_coop::{
    CoopGuest, CoopHost, GamepadState, GuestConfig, GuestDecoder, GuestMedia, HostConfig,
    IceConfig, InputSamples, RumbleRouter, StreamDescription,
};
use opennow_streamer_transport::MediaConsumer;
use serde_json::Value;

mod common;
use common::*;

const BOUND: Duration = Duration::from_secs(3);

/// Behaves like the engine's media: a consumer thread drains the queue until every sender
/// is dropped, and stop joins it.
struct JoiningMedia {
    consumer: Option<JoinHandle<()>>,
    started: std::sync::mpsc::Sender<()>,
    stopped: Arc<Mutex<bool>>,
}

impl GuestMedia for JoiningMedia {
    fn start(&mut self, _: &StreamDescription) -> Result<MediaConsumer, String> {
        let (consumer, frames) = sync_channel(8);
        self.consumer = Some(std::thread::spawn(move || for _ in frames {}));
        let _ = self.started.send(());
        Ok(consumer)
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

    fn take_first_frame(&mut self) -> Option<GuestDecoder> {
        None
    }

    fn stop(&mut self) {
        if let Some(consumer) = self.consumer.take() {
            consumer.join().unwrap();
        }
        *self.stopped.lock().unwrap() = true;
    }
}

/// The guest is never dropped implicitly: a test that fails while teardown is broken must
/// report the failure instead of hanging in the guest's drop.
struct Pair {
    host: CoopHost,
    guest: ManuallyDrop<CoopGuest>,
    host_events: Receiver<Value>,
    guest_events: Receiver<Value>,
    stopped: Arc<Mutex<bool>>,
}

/// Connects one guest directly and waits until its media pipeline has started.
fn connect() -> Pair {
    let (_tap, tap_receiver) = sync_channel(16);
    let (host_sink, host_events) = sink();
    let host = CoopHost::start(
        HostConfig {
            ice: IceConfig::default(),
            max_guests: 1,
            net: loopback_net(),
            stream: description(),
        },
        tap_receiver,
        Arc::new(Backend::default()),
        Arc::new(RumbleRouter::default()),
        host_sink,
    )
    .unwrap();
    let (started, media_started) = channel();
    let stopped = Arc::new(Mutex::new(false));
    let (guest_sink, guest_events) = sink();
    let guest = CoopGuest::start(
        GuestConfig {
            ice: IceConfig::default(),
            net: loopback_net(),
            host_peer_id: "host".to_owned(),
            slot: 1,
        },
        Box::new(JoiningMedia {
            consumer: None,
            started,
            stopped: stopped.clone(),
        }),
        guest_sink,
    )
    .unwrap();
    host.add_peer("guest-1", 1).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while media_started.try_recv().is_err() {
        assert!(Instant::now() < deadline, "guest media never started");
        while let Ok(value) = host_events.try_recv() {
            if value["type"] == "coop-signal-out" {
                guest.signal("host", &value["payload"]).unwrap();
            }
        }
        while let Ok(value) = guest_events.try_recv() {
            if value["type"] == "coop-signal-out" {
                host.signal("guest-1", &value["payload"]).unwrap();
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Pair {
        host,
        guest: ManuallyDrop::new(guest),
        host_events,
        guest_events,
        stopped,
    }
}

/// Runs 'stop' on another thread and fails instead of hanging when it does not return.
fn returns_within(stop: impl FnOnce() + Send + 'static) {
    let (done, finished) = channel();
    std::thread::spawn(move || {
        stop();
        let _ = done.send(());
    });
    assert!(
        finished.recv_timeout(BOUND).is_ok(),
        "teardown did not return within {BOUND:?}"
    );
}

#[test]
fn guest_stop_returns_after_media_started() {
    let Pair {
        host,
        guest,
        stopped,
        ..
    } = connect();
    returns_within(move || ManuallyDrop::into_inner(guest).stop());
    assert!(*stopped.lock().unwrap(), "guest media was stopped");
    drop(host);
}

#[test]
fn host_bye_ends_the_guest_and_stops_media() {
    let Pair {
        host,
        guest,
        host_events,
        guest_events,
        stopped,
    } = connect();
    host.remove_peer("guest-1", "kicked").unwrap();
    let bye = host_events
        .iter()
        .find(|value| value["type"] == "coop-signal-out" && value["payload"]["kind"] == "bye")
        .unwrap();
    guest.signal("host", &bye["payload"]).unwrap();
    let deadline = Instant::now() + BOUND;
    let ended = loop {
        let value = guest_events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("the guest ends within the bound after a host bye");
        if value["type"] == "coop-guest-state" && value["state"] == "ended" {
            break value;
        }
    };
    assert_eq!(ended["reason"], "kicked");
    assert!(
        *stopped.lock().unwrap(),
        "guest media was stopped before ended"
    );
    returns_within(move || ManuallyDrop::into_inner(guest).stop());
}
