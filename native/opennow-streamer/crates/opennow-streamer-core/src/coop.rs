//! Co-op commands on the engine: the host fan-out during an NVST session and the guest
//! pipeline without one. See docs/coop-sessions.md section 7.

use std::sync::mpsc::Receiver;
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use opennow_streamer_coop::{
    AudioDescription, CoopError, CoopGuest, CoopHost, EventSink, GamepadSelector, GamepadState,
    GuestConfig, GuestMedia, HostBackend, HostConfig, IceConfig, InputSamples, NetConfig,
    StreamDescription,
};
use opennow_streamer_hid::HidRuntime;
use opennow_streamer_platform::{
    CapturedInput, CapturedInputQueue, MediaColorQuality, MediaFeedback, MediaRuntime,
    MediaSession, MediaStreamConfig, MediaVideoCodec,
};
use opennow_streamer_protocol::{AudioOutputDevice, Command, error, event, response};
use opennow_streamer_transport::{
    MediaConsumer, NvstAudioTrack, NvstUdpReceiverControl, SharedNvstFeedback,
};
use serde_json::{Value, json};

use super::{
    ENCODED_MEDIA_QUEUE_CAPACITY, Engine, EventSender, State, consume_encoded_media, lock_lifecycle,
};

/// Packets the NVST receive threads may queue for the co-op host before dropping.
const COOP_TAP_CAPACITY: usize = 2_048;
const MAX_INPUT_SAMPLES_PER_POLL: usize = 64;

/// What the engine keeps from the running NVST session so a co-op host can attach later.
pub(super) struct NvstCoopSession {
    pub(super) bundle: NvstUdpReceiverControl,
    pub(super) feedback: SharedNvstFeedback,
    pub(super) description: StreamDescription,
}

struct NvstHostBackend {
    bundle: NvstUdpReceiverControl,
    feedback: SharedNvstFeedback,
}

impl HostBackend for NvstHostBackend {
    fn request_keyframe(&self) {
        self.feedback.request_keyframe();
    }

    fn send_input(&self, record: Vec<u8>) -> bool {
        self.bundle.queue_input(record, false).is_ok()
    }

    fn host_to_gfn_ping_ms(&self) -> Option<f64> {
        self.feedback.ping_ms(Instant::now())
    }
}

/// The description guests receive first on 'coop-control', from the negotiated session.
pub(super) fn stream_description(
    config: &MediaStreamConfig,
    audio: Option<&NvstAudioTrack>,
) -> StreamDescription {
    let chroma = match config.color_quality {
        MediaColorQuality::EightBit444 | MediaColorQuality::TenBit444 => "444",
        MediaColorQuality::EightBit420 | MediaColorQuality::TenBit420 => "420",
    };
    StreamDescription {
        codec: config.codec.label().to_owned(),
        width: config.width,
        height: config.height,
        fps: config.fps,
        bit_depth: config.color_quality.bit_depth(),
        chroma: chroma.to_owned(),
        color_range: None,
        color_space: Some(if config.hdr { "bt2020" } else { "bt709" }.to_owned()),
        hdr: config.hdr,
        audio: audio.map(|track| AudioDescription {
            codec: "opus".to_owned(),
            channels: track.channels,
            sample_rate: track.clock_rate_hz,
            payload_type: track.payload_type,
        }),
    }
}

fn guest_stream_config(description: &StreamDescription) -> Result<MediaStreamConfig, String> {
    let codec = match description.codec.to_ascii_lowercase().as_str() {
        "h264" => MediaVideoCodec::H264,
        "h265" | "hevc" => MediaVideoCodec::H265,
        "av1" => MediaVideoCodec::Av1,
        other => return Err(format!("unsupported co-op codec {other}")),
    };
    let color_quality = match (description.bit_depth, description.chroma.as_str()) {
        (8, "420") => MediaColorQuality::EightBit420,
        (8, "444") => MediaColorQuality::EightBit444,
        (10, "420") => MediaColorQuality::TenBit420,
        (10, "444") => MediaColorQuality::TenBit444,
        (depth, chroma) => return Err(format!("unsupported co-op color {depth}-bit {chroma}")),
    };
    if description.width == 0 || description.height == 0 || description.fps == 0 {
        return Err("co-op stream description has no video size or rate".to_owned());
    }
    Ok(MediaStreamConfig {
        codec,
        color_quality,
        hdr: description.hdr,
        width: description.width,
        height: description.height,
        fps: description.fps.min(super::MAX_STREAM_FPS),
        ..MediaStreamConfig::default()
    })
}

/// The guest's local decode, presentation and audio, on the same runtime path as a GFN session.
struct EngineGuestMedia {
    runtime: MediaRuntime,
    events: EventSender,
    session: Option<MediaSession>,
    worker: Option<JoinHandle<()>>,
    feedback: Option<Receiver<MediaFeedback>>,
    captured: Option<Arc<CapturedInputQueue>>,
    gamepads: GamepadSelector,
    /// Tells Sony controllers, whose snapshots arrive as ordinary reports on their claimed
    /// slot, apart from plain gamepads.
    hid: Arc<HidRuntime>,
    samples: InputSamples,
}

impl GuestMedia for EngineGuestMedia {
    fn start(&mut self, description: &StreamDescription) -> Result<MediaConsumer, String> {
        self.stop();
        let config = guest_stream_config(description)?;
        let (feedback, feedback_receiver) = mpsc::channel();
        let session = self.runtime.start_with_audio_device(
            feedback,
            config,
            "auto",
            AudioOutputDevice::default(),
        )?;
        let sink = session.sink();
        let (consumer, receiver) = mpsc::sync_channel(ENCODED_MEDIA_QUEUE_CAPACITY);
        let output = self.events.clone();
        let worker = match thread::Builder::new()
            .name("opennow-coop-media".to_owned())
            .spawn(move || consume_encoded_media(&output, receiver, sink))
        {
            Ok(worker) => worker,
            Err(spawn_error) => {
                session.stop();
                return Err(spawn_error.to_string());
            }
        };
        self.captured = Some(session.captured_input());
        self.session = Some(session);
        self.worker = Some(worker);
        self.feedback = Some(feedback_receiver);
        Ok(consumer)
    }

    fn take_gamepad(&mut self) -> Option<GamepadState> {
        let captured = self.captured.as_ref()?;
        let mut inventory = None;
        for _ in 0..MAX_INPUT_SAMPLES_PER_POLL {
            let Some(sample) = captured.take_sample() else {
                break;
            };
            match sample.input {
                CapturedInput::Gamepad {
                    controller_id,
                    bitmap,
                    buttons,
                    left_trigger,
                    right_trigger,
                    left_stick_x,
                    left_stick_y,
                    right_stick_x,
                    right_stick_y,
                } => {
                    if inventory
                        .get_or_insert_with(|| self.hid.inventory())
                        .claim(controller_id)
                        .is_some()
                    {
                        self.samples.sony += 1;
                    } else {
                        self.samples.plain += 1;
                    }
                    self.gamepads.observe(
                        controller_id,
                        GamepadState {
                            bitmap,
                            buttons,
                            left_trigger,
                            right_trigger,
                            left_stick_x,
                            left_stick_y,
                            right_stick_x,
                            right_stick_y,
                        },
                    )
                }
                CapturedInput::Guide => {
                    let _ = self
                        .events
                        .send(event("overlay-request", json!({"source":"gamepad"})));
                }
                _ => {}
            }
        }
        if captured.take_overflowed() {
            captured.clear();
        }
        self.gamepads.take()
    }

    fn input_samples(&self) -> InputSamples {
        self.samples
    }

    fn take_keyframe_request(&mut self) -> bool {
        let Some(feedback) = self.feedback.as_ref() else {
            return false;
        };
        let mut requested = false;
        for feedback in feedback.try_iter() {
            if matches!(
                feedback,
                MediaFeedback::RequestKeyframe { .. } | MediaFeedback::DecoderError { .. }
            ) {
                requested = true;
            }
        }
        requested
    }

    fn stop(&mut self) {
        self.captured = None;
        if let Some(session) = self.session.take() {
            session.stop();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.feedback = None;
    }
}

impl Drop for EngineGuestMedia {
    fn drop(&mut self) {
        self.stop();
    }
}

fn coop_error(id: &str, failure: CoopError) -> Value {
    let code = match failure {
        CoopError::Busy => "coop-busy",
        CoopError::Closed => "coop-not-running",
        CoopError::Invalid(_) => "invalid-coop-command",
    };
    error(Some(id), code, failure.to_string())
}

fn required<'a, T>(id: &str, value: Option<&'a T>, field: &str) -> Result<&'a T, Value> {
    value.ok_or_else(|| {
        error(
            Some(id),
            "invalid-coop-command",
            format!("co-op command requires {field}"),
        )
    })
}

fn ice_config(command: &Command) -> Result<IceConfig, Value> {
    let ice_servers = required(&command.id, command.ice_servers.as_ref(), "iceServers")?;
    IceConfig::from_json(ice_servers, command.relays.as_ref())
        .map_err(|failure| error(Some(&command.id), "invalid-coop-command", failure.0))
}

impl Engine {
    fn coop_sink(&self) -> EventSink {
        let events = self.events.clone();
        Arc::new(move |value| {
            let _ = events.send(value);
        })
    }

    pub(super) fn coop_command(&mut self, command: Command) -> Result<Vec<Value>, Value> {
        let id = command.id.clone();
        match command.kind.as_str() {
            "coop-host-start" => self.coop_host_start(&command)?,
            "coop-peer-add" => {
                let host = self.coop_host_ref(&id)?;
                let peer_id = required(&id, command.peer_id.as_ref(), "peerId")?;
                let slot = *required(&id, command.slot.as_ref(), "slot")?;
                host.add_peer(peer_id, slot)
                    .map_err(|failure| coop_error(&id, failure))?;
            }
            "coop-signal-in" => {
                let peer_id = required(&id, command.peer_id.as_ref(), "peerId")?;
                let payload = required(&id, command.payload.as_ref(), "payload")?;
                let result = match (&self.coop_host, &self.coop_guest) {
                    (Some(host), _) => host.signal(peer_id, payload),
                    (None, Some(guest)) => guest.signal(peer_id, payload),
                    (None, None) => Err(CoopError::Closed),
                };
                result.map_err(|failure| coop_error(&id, failure))?;
            }
            "coop-peer-slot" => {
                let host = self.coop_host_ref(&id)?;
                let peer_id = required(&id, command.peer_id.as_ref(), "peerId")?;
                let slot = *required(&id, command.slot.as_ref(), "slot")?;
                host.set_slot(peer_id, slot)
                    .map_err(|failure| coop_error(&id, failure))?;
            }
            "coop-peer-input-muted" => {
                let host = self.coop_host_ref(&id)?;
                let peer_id = required(&id, command.peer_id.as_ref(), "peerId")?;
                let muted = *required(&id, command.muted.as_ref(), "muted")?;
                host.set_muted(peer_id, muted)
                    .map_err(|failure| coop_error(&id, failure))?;
            }
            "coop-peer-remove" => {
                let host = self.coop_host_ref(&id)?;
                let peer_id = required(&id, command.peer_id.as_ref(), "peerId")?;
                host.remove_peer(peer_id, command.reason.as_deref().unwrap_or("removed"))
                    .map_err(|failure| coop_error(&id, failure))?;
            }
            "coop-host-stop" => self.stop_coop_host(),
            "coop-guest-start" => self.coop_guest_start(&command)?,
            "coop-guest-stop" => self.stop_coop_guest(),
            other => {
                return Err(error(
                    Some(&id),
                    "unknown-command",
                    format!("Unknown command: {other}"),
                ));
            }
        }
        Ok(vec![response(id, "ok")])
    }

    fn coop_host_ref(&self, id: &str) -> Result<&CoopHost, Value> {
        self.coop_host
            .as_ref()
            .ok_or_else(|| error(Some(id), "coop-not-running", "Co-op hosting is not running"))
    }

    fn coop_host_start(&mut self, command: &Command) -> Result<(), Value> {
        let id = &command.id;
        let max_guests = *required(id, command.max_guests.as_ref(), "maxGuests")?;
        if let Some(host) = self.coop_host.as_ref() {
            return host
                .set_max_guests(max_guests)
                .map_err(|failure| coop_error(id, failure));
        }
        let ice = ice_config(command)?;
        let Some(session) = self.nvst_coop.as_ref() else {
            return Err(error(
                Some(id),
                "coop-unavailable",
                "Co-op hosting requires an active native NVST session",
            ));
        };
        let tap = session.feedback.coop_tap.attach(COOP_TAP_CAPACITY);
        let backend = Arc::new(NvstHostBackend {
            bundle: session.bundle.clone(),
            feedback: Arc::clone(&session.feedback),
        });
        let config = HostConfig {
            ice,
            max_guests,
            net: NetConfig::default(),
            stream: session.description.clone(),
        };
        match CoopHost::start(
            config,
            tap,
            backend,
            Arc::clone(&self.coop_rumble),
            self.coop_sink(),
        ) {
            Ok(host) => {
                self.coop_host = Some(host);
                Ok(())
            }
            Err(failure) => {
                session.feedback.coop_tap.detach();
                Err(coop_error(id, failure))
            }
        }
    }

    /// Removes every guest with neutral input; the host's own stream continues.
    pub(super) fn stop_coop_host(&mut self) {
        if let Some(mut host) = self.coop_host.take() {
            host.stop();
        }
        if let Some(session) = self.nvst_coop.as_ref() {
            session.feedback.coop_tap.detach();
        }
    }

    fn coop_guest_start(&mut self, command: &Command) -> Result<(), Value> {
        let id = &command.id;
        if self.coop_guest.is_some() {
            return Err(error(
                Some(id),
                "coop-guest-active",
                "A co-op guest session is already running",
            ));
        }
        if lock_lifecycle(&self.lifecycle).state != State::Idle || self.coop_host.is_some() {
            return Err(error(
                Some(id),
                "invalid-state",
                "A co-op guest cannot start while this client streams a GFN session",
            ));
        }
        let Some(runtime) = self.media_runtime.clone() else {
            return Err(error(
                Some(id),
                "media-output-unavailable",
                "Co-op guests need the embedded media runtime",
            ));
        };
        let config = GuestConfig {
            ice: ice_config(command)?,
            net: NetConfig::default(),
            host_peer_id: required(id, command.host_peer_id.as_ref(), "hostPeerId")?.clone(),
            slot: *required(id, command.slot.as_ref(), "slot")?,
        };
        let events = self.events.for_session(id);
        let media = Box::new(EngineGuestMedia {
            runtime,
            events: events.clone(),
            session: None,
            worker: None,
            feedback: None,
            captured: None,
            gamepads: GamepadSelector::default(),
            hid: self.hid_runtime.clone(),
            samples: InputSamples::default(),
        });
        let start_id = id.clone();
        let sink: EventSink = Arc::new(move |mut value: Value| {
            if value["type"] == "controller-rumble" {
                value["startId"] = Value::String(start_id.clone());
            }
            let _ = events.send(value);
        });
        self.coop_guest =
            Some(CoopGuest::start(config, media, sink).map_err(|failure| coop_error(id, failure))?);
        self.hid_runtime.set_ordinary_without_session(true);
        Ok(())
    }

    pub(super) fn stop_coop_guest(&mut self) {
        if let Some(mut guest) = self.coop_guest.take() {
            guest.stop();
            self.hid_runtime.set_ordinary_without_session(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guest_media_forwards_one_controller_from_any_slot_and_counts_sony_samples_apart() {
        let (_host, runtime) = opennow_streamer_platform::create_test_runtime();
        let (sender, _events) = mpsc::channel();
        let hid = Arc::new(HidRuntime::new());
        hid.replace_inventory(&[
            None,
            None,
            opennow_streamer_hid::SdlDeviceClaim::new(2, 5, 0x054c, 0x0ce6),
            None,
        ]);
        let captured = Arc::new(CapturedInputQueue::default());
        let mut media = EngineGuestMedia {
            runtime,
            events: EventSender::unbounded(sender),
            session: None,
            worker: None,
            feedback: None,
            captured: Some(captured.clone()),
            gamepads: GamepadSelector::default(),
            hid,
            samples: InputSamples::default(),
        };
        let pad = |controller_id, buttons| CapturedInput::Gamepad {
            controller_id,
            bitmap: 0x0606,
            buttons,
            left_trigger: 0,
            right_trigger: 0,
            left_stick_x: 0,
            left_stick_y: 0,
            right_stick_x: 0,
            right_stick_y: 0,
        };
        let mut buttons = || media.take_gamepad().map(|state| state.buttons);
        captured.push(pad(1, 0x1000));
        assert_eq!(
            buttons(),
            Some(0x1000),
            "a plain pad on slot 1 is forwarded"
        );
        captured.push(pad(2, 0x2000));
        assert_eq!(buttons(), None, "the Sony pad waits while slot 1 is held");
        captured.push(pad(1, 0));
        captured.push(pad(2, 0x2000));
        assert_eq!(buttons(), Some(0), "the old owner's neutral goes first");
        assert_eq!(buttons(), Some(0x2000));
        assert_eq!(media.input_samples(), InputSamples { plain: 2, sony: 2 });
    }

    #[test]
    fn a_coop_guest_admits_sony_controllers_as_ordinary_input_until_it_leaves() {
        use opennow_streamer_hid::{SdlDeviceClaim, SnapshotAdmission, SonySnapshot};
        let (_host, runtime) = opennow_streamer_platform::create_test_runtime();
        let (events, _received) = mpsc::channel();
        let mut engine = Engine::with_media_runtime(events, runtime);
        engine.hid_runtime.replace_inventory(&[
            None,
            SdlDeviceClaim::new(1, 3, 0x054c, 0x0ce6),
            None,
            None,
        ]);
        let mut snapshot = SonySnapshot::neutral(1, 3, 0);
        snapshot.buttons = 0x1000;
        let admitted = |engine: &Engine| {
            let mut forwarded = None;
            let admission = engine
                .hid_runtime
                .submit_snapshot_with(snapshot, |snapshot, _| forwarded = Some(snapshot.buttons));
            (admission, forwarded)
        };
        assert_eq!(admitted(&engine), (SnapshotAdmission::Unbound, None));
        let start: Command = serde_json::from_value(json!({
            "id":"guest-1","type":"coop-guest-start","iceServers":[],"relays":[],
            "hostPeerId":"usr_host","slot":2
        }))
        .unwrap();
        let (responses, _) = engine.handle(start);
        assert_eq!(responses[0]["type"], "ok", "{responses:?}");
        assert_eq!(
            admitted(&engine),
            (SnapshotAdmission::Admitted, Some(0x1000))
        );
        let stop: Command =
            serde_json::from_value(json!({"id":"guest-2","type":"coop-guest-stop"})).unwrap();
        engine.handle(stop);
        assert_eq!(admitted(&engine), (SnapshotAdmission::Unbound, None));
    }

    #[test]
    fn description_round_trips_into_the_guest_stream_config() {
        let config = MediaStreamConfig {
            codec: MediaVideoCodec::H265,
            color_quality: MediaColorQuality::TenBit420,
            hdr: true,
            width: 2560,
            height: 1440,
            fps: 120,
            ..MediaStreamConfig::default()
        };
        let track = NvstAudioTrack {
            payload_type: 111,
            clock_rate_hz: 48_000,
            channels: 2,
            mid: "1".to_owned(),
            ssrc: Some(7),
        };
        let description = stream_description(&config, Some(&track));
        assert_eq!(
            serde_json::to_value(&description).unwrap(),
            json!({"codec":"h265","width":2560,"height":1440,"fps":120,"bitDepth":10,
                "chroma":"420","colorRange":null,"colorSpace":"bt2020","hdr":true,
                "audio":{"codec":"opus","channels":2,"sampleRate":48000,"payloadType":111}})
        );
        let guest = guest_stream_config(&description).unwrap();
        assert_eq!(
            (
                guest.codec,
                guest.color_quality,
                guest.hdr,
                guest.width,
                guest.height,
                guest.fps
            ),
            (config.codec, config.color_quality, true, 2560, 1440, 120)
        );
        let mut bad = description;
        bad.chroma = "422".to_owned();
        assert!(guest_stream_config(&bad).is_err());
    }
}
