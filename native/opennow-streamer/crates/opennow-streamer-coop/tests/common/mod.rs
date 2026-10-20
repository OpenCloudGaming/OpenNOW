//! Helpers shared by the co-op integration tests.
#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, channel, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opennow_streamer_coop::{
    EventSink, GamepadState, GuestDecoder, GuestMedia, HostBackend, InputSamples, NetConfig,
    StreamDescription,
};
use opennow_streamer_transport::{EncodedMediaFrame, MediaConsumer};
use serde_json::{Value, json};

pub const FLAG_PIC: u8 = 0x01;
pub const FLAG_EOF: u8 = 0x02;
pub const FLAG_SOF: u8 = 0x04;

#[derive(Default)]
pub struct Backend {
    pub inputs: Mutex<Vec<Vec<u8>>>,
    pub keyframes: AtomicUsize,
}

impl HostBackend for Backend {
    fn request_keyframe(&self) {
        self.keyframes.fetch_add(1, Ordering::Relaxed);
    }

    fn send_input(&self, record: Vec<u8>) -> bool {
        self.inputs.lock().unwrap().push(record);
        true
    }

    fn host_to_gfn_ping_ms(&self) -> Option<f64> {
        Some(11.0)
    }
}

pub struct Media {
    pub frames: SyncSender<Receiver<EncodedMediaFrame>>,
    pub gamepad: Arc<Mutex<Option<GamepadState>>>,
    pub stopped: Arc<AtomicBool>,
    pub taken: u64,
    /// Reported as presented on the first poll after media starts.
    pub first_frame: Option<GuestDecoder>,
}

impl GuestMedia for Media {
    fn start(&mut self, description: &StreamDescription) -> Result<MediaConsumer, String> {
        assert_eq!(description.codec, "H264");
        let (consumer, receiver) = sync_channel(256);
        self.frames.send(receiver).unwrap();
        self.first_frame = Some(GuestDecoder::Hardware);
        Ok(consumer)
    }

    fn take_gamepad(&mut self) -> Option<GamepadState> {
        let state = self.gamepad.lock().unwrap().take();
        self.taken += u64::from(state.is_some());
        state
    }

    fn input_samples(&self) -> InputSamples {
        InputSamples {
            plain: self.taken,
            sony: 0,
        }
    }

    fn take_keyframe_request(&mut self) -> bool {
        false
    }

    fn take_first_frame(&mut self) -> Option<GuestDecoder> {
        self.first_frame.take()
    }

    fn stop(&mut self) {
        self.stopped.store(true, Ordering::Release);
    }
}

pub fn sink() -> (EventSink, Receiver<Value>) {
    let (sender, receiver): (Sender<Value>, Receiver<Value>) = channel();
    let sender = Mutex::new(sender);
    (
        Arc::new(move |value| {
            let _ = sender.lock().unwrap().send(value);
        }),
        receiver,
    )
}

pub fn loopback_net() -> NetConfig {
    NetConfig {
        host_ips: Some(vec!["127.0.0.1".parse().unwrap()]),
        ..NetConfig::default()
    }
}

pub fn description() -> StreamDescription {
    serde_json::from_value(json!({
        "codec":"H264","width":1920,"height":1080,"fps":60,"bitDepth":8,"chroma":"420",
        "colorRange":null,"colorSpace":"bt709","hdr":false,
        "audio":{"codec":"opus","channels":2,"sampleRate":48000,"payloadType":111}
    }))
    .unwrap()
}

/// One NVST video packet. 'stream_index' is the sender's 24-bit stream packet index, which
/// keeps counting when the 16-bit RTP sequence wraps.
pub fn nvst_packet(
    seq: u16,
    stream_index: u32,
    flags: u8,
    frame_index: u32,
    timestamp: u32,
    media: &[u8],
) -> Vec<u8> {
    let marker = if flags & FLAG_EOF != 0 { 0x80 } else { 0 };
    let mut packet = vec![0x90, marker | 96];
    packet.extend_from_slice(&seq.to_be_bytes());
    packet.extend_from_slice(&timestamp.to_be_bytes());
    packet.extend_from_slice(&0x1122_3344_u32.to_be_bytes());
    packet.extend_from_slice(&0x4753_u16.to_be_bytes());
    packet.extend_from_slice(&4_u16.to_be_bytes());
    packet.extend_from_slice(&((stream_index & 0x00ff_ffff) << 8).to_le_bytes());
    packet.extend_from_slice(&frame_index.to_le_bytes());
    packet.push(flags);
    packet.extend_from_slice(&[0; 7]);
    packet.extend_from_slice(media);
    packet
}

/// Frame 'index' as three packets; frame 0 is an IDR.
pub fn frame_packets(index: u32, first_seq: u16) -> (Vec<Vec<u8>>, Vec<u8>) {
    keyed_frame_packets(index, first_seq, index == 0)
}

/// Frame 'index' as three packets, an IDR when 'keyframe'.
pub fn keyed_frame_packets(index: u32, first_seq: u16, keyframe: bool) -> (Vec<Vec<u8>>, Vec<u8>) {
    let nal = if keyframe { 0x65 } else { 0x41 };
    let mut parts = vec![vec![0, 0, 0, 1, nal]];
    parts[0].extend((0..1_000).map(|byte| (byte as u32 + index) as u8));
    parts.push(
        (0..1_100)
            .map(|byte| (byte as u32 * 7 + index) as u8)
            .collect(),
    );
    parts.push(
        (0..300)
            .map(|byte| (byte as u32 * 3 + index) as u8)
            .collect(),
    );
    let timestamp = 90_000 + index * 1_500;
    let flags = [FLAG_SOF | FLAG_PIC, FLAG_PIC, FLAG_EOF | FLAG_PIC];
    let packets = parts
        .iter()
        .enumerate()
        .map(|(part, media)| {
            nvst_packet(
                first_seq.wrapping_add(part as u16),
                1 + index * 3 + part as u32,
                flags[part],
                index + 1,
                timestamp,
                media,
            )
        })
        .collect();
    (packets, parts.concat())
}

pub fn wait_for(
    events: &Receiver<Value>,
    seen: &mut Vec<Value>,
    deadline: Duration,
    matches: impl Fn(&Value) -> bool,
) -> Value {
    let end = Instant::now() + deadline;
    if let Some(found) = seen.iter().find(|value| matches(value)) {
        return found.clone();
    }
    while Instant::now() < end {
        if let Ok(value) = events.recv_timeout(Duration::from_millis(10)) {
            seen.push(value.clone());
            if matches(&value) {
                return value;
            }
        }
    }
    panic!("timed out; events: {seen:#?}");
}
