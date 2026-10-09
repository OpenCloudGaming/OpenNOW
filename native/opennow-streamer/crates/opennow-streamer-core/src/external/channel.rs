use opennow_media_protocol::wire::{
    AUDIO_TRACK_ID, ControlMessage, FrameStage, MEDIA_HEADER_BYTES, MediaHeader, VIDEO_TRACK_ID,
    encode_control,
};
use opennow_plugin_api::media::{InputCapabilities, MediaLimits};
use std::collections::VecDeque;
use std::io::{self, Read};

#[derive(Debug)]
pub struct MediaFrame {
    pub header: MediaHeader,
    pub payload: Vec<u8>,
}

#[derive(Debug)]
pub enum WorkerEvent {
    Ready { input: InputCapabilities },
    Control(ControlMessage),
    Discontinuity { track_id: u32 },
    Failed(&'static str),
    Exited,
}

pub(super) struct Channels {
    limits: MediaLimits,
    video: VecDeque<MediaFrame>,
    video_bytes: usize,
    audio: VecDeque<(MediaFrame, u32)>,
    audio_bytes: usize,
    audio_ticks: u32,
    awaiting_keyframe: bool,
    keyframe_requested: bool,
    discontinuity: [bool; 2],
    ready: Option<InputCapabilities>,
    terminal: Option<WorkerEvent>,
    incoming: VecDeque<ControlMessage>,
    outgoing: VecDeque<(ControlMessage, usize)>,
    outgoing_bytes: usize,
    progress: [Option<ControlMessage>; 3],
    coalesced_frame_progress: u64,
    neutral: Option<ControlMessage>,
    keyframe: Option<ControlMessage>,
    stop: Option<ControlMessage>,
}

impl Channels {
    pub fn new(limits: MediaLimits) -> Self {
        Self {
            limits,
            video: VecDeque::new(),
            video_bytes: 0,
            audio: VecDeque::new(),
            audio_bytes: 0,
            audio_ticks: 0,
            awaiting_keyframe: true,
            keyframe_requested: false,
            discontinuity: [false; 2],
            ready: None,
            terminal: None,
            incoming: VecDeque::new(),
            outgoing: VecDeque::new(),
            outgoing_bytes: 0,
            progress: [None, None, None],
            coalesced_frame_progress: 0,
            neutral: None,
            keyframe: None,
            stop: None,
        }
    }

    pub fn send(&mut self, message: ControlMessage) -> io::Result<()> {
        let bytes = encode_control(&message, self.limits.max_control_message_bytes as usize)?.len();
        match message {
            ControlMessage::Stop { .. } => {
                self.outgoing.clear();
                self.outgoing_bytes = 0;
                self.progress = [None, None, None];
                self.stop = Some(message);
            }
            ControlMessage::Neutral { .. } => {
                self.outgoing.retain(|(message, bytes)| {
                    if matches!(message, ControlMessage::Input { .. }) {
                        self.outgoing_bytes -= bytes;
                        false
                    } else {
                        true
                    }
                });
                self.neutral = Some(message);
            }
            ControlMessage::Keyframe { .. } => self.keyframe = Some(message),
            ControlMessage::FrameProgress { stage, .. } => {
                let slot = match stage {
                    FrameStage::Accepted => 0,
                    FrameStage::Decoded => 1,
                    FrameStage::Presented => 2,
                };
                if self.progress[slot].replace(message).is_some() {
                    self.coalesced_frame_progress = self.coalesced_frame_progress.saturating_add(1);
                }
            }
            ControlMessage::Input { .. }
                if self.outgoing.len() >= usize::from(self.limits.max_pending_input_events)
                    || self.outgoing_bytes + bytes > 1024 * 1024 =>
            {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            ControlMessage::Input { .. } => {
                self.outgoing_bytes += bytes;
                self.outgoing.push_back((message, bytes));
            }
            _ => return Err(io::Error::other("Invalid host worker control message")),
        }
        Ok(())
    }

    pub fn outgoing(&mut self) -> Option<ControlMessage> {
        self.stop
            .take()
            .or_else(|| self.neutral.take())
            .or_else(|| self.keyframe.take())
            .or_else(|| {
                let (message, bytes) = self.outgoing.pop_front()?;
                self.outgoing_bytes -= bytes;
                Some(message)
            })
            .or_else(|| self.progress.iter_mut().find_map(Option::take))
    }

    pub fn coalesced_frame_progress(&self) -> u64 {
        self.coalesced_frame_progress
    }

    pub fn ready(&mut self, input: InputCapabilities) {
        self.ready = Some(input);
    }

    pub fn terminal(&mut self, event: WorkerEvent) {
        self.terminal = Some(event);
    }

    pub fn incoming(&mut self, message: ControlMessage) -> io::Result<()> {
        if matches!(
            message,
            ControlMessage::Ack {
                kind: opennow_media_protocol::wire::AckKind::Keyframe,
                ..
            }
        ) && self.incoming.iter().any(|pending| {
            matches!(
                pending,
                ControlMessage::Ack {
                    kind: opennow_media_protocol::wire::AckKind::Keyframe,
                    ..
                }
            )
        }) {
            return Ok(());
        }
        if self.incoming.len() >= usize::from(self.limits.max_pending_input_events) {
            return Err(io::Error::other("Worker control event queue overflow"));
        }
        self.incoming.push_back(message);
        Ok(())
    }

    pub fn recv_control(&mut self) -> Option<WorkerEvent> {
        if let Some(terminal) = self.terminal.take() {
            return Some(terminal);
        }
        if let Some(input) = self.ready.take() {
            return Some(WorkerEvent::Ready { input });
        }
        if let Some(message) = self.incoming.pop_front() {
            return Some(WorkerEvent::Control(message));
        }
        for (index, pending) in self.discontinuity.iter_mut().enumerate() {
            if std::mem::take(pending) {
                return Some(WorkerEvent::Discontinuity {
                    track_id: index as u32 + 1,
                });
            }
        }
        None
    }

    pub fn push_media(&mut self, mut frame: MediaFrame) -> io::Result<()> {
        if frame.header.track_id == VIDEO_TRACK_ID {
            if !frame.header.contiguous
                || self.video.len() >= usize::from(self.limits.max_buffered_video_frames)
                || self.video_bytes + frame.payload.len()
                    > self.limits.max_buffered_video_bytes as usize
            {
                self.video.clear();
                self.video_bytes = 0;
                if !self.awaiting_keyframe {
                    self.keyframe_requested = false;
                }
                self.awaiting_keyframe = true;
                self.discontinuity[0] = true;
            }
            if self.awaiting_keyframe {
                if !frame.header.keyframe {
                    self.discontinuity[0] = true;
                    if !self.keyframe_requested {
                        self.keyframe = Some(ControlMessage::Keyframe {
                            attempt_generation: frame.header.attempt_generation,
                            track_id: VIDEO_TRACK_ID,
                        });
                        self.keyframe_requested = true;
                    }
                    return Ok(());
                }
                frame.header.contiguous = false;
                self.awaiting_keyframe = false;
                self.keyframe_requested = false;
            }
            self.video_bytes += frame.payload.len();
            self.video.push_back(frame);
        } else {
            let ticks = opus_duration_ticks(&frame.payload)?;
            let max_ticks = u32::from(self.limits.max_buffered_audio_ms) * 2;
            let max_bytes =
                self.limits.max_audio_packet_bytes as usize * (max_ticks as usize / 5).max(1);
            if ticks > max_ticks {
                self.discontinuity[1] = true;
                return Ok(());
            }
            while self.audio_ticks + ticks > max_ticks
                || self.audio_bytes + frame.payload.len() > max_bytes
            {
                self.recv_audio();
                self.discontinuity[1] = true;
            }
            self.audio_ticks += ticks;
            self.audio_bytes += frame.payload.len();
            self.audio.push_back((frame, ticks));
        }
        Ok(())
    }

    pub fn recv_video(&mut self) -> Option<MediaFrame> {
        let frame = self.video.pop_front()?;
        self.video_bytes -= frame.payload.len();
        Some(frame)
    }

    pub fn recv_audio(&mut self) -> Option<MediaFrame> {
        let (frame, ticks) = self.audio.pop_front()?;
        self.audio_ticks -= ticks;
        self.audio_bytes -= frame.payload.len();
        Some(frame)
    }
}

fn opus_duration_ticks(packet: &[u8]) -> io::Result<u32> {
    let toc = *packet
        .first()
        .ok_or_else(|| io::Error::other("Empty Opus packet"))?;
    let per_frame = if toc & 0x80 != 0 {
        5 << ((toc >> 3) & 3)
    } else if toc & 0x60 == 0x60 {
        if toc & 8 != 0 { 40 } else { 20 }
    } else {
        [20, 40, 80, 120][((toc >> 3) & 3) as usize]
    };
    let frames = match toc & 3 {
        0 => 1,
        1 | 2 => 2,
        _ => u32::from(
            *packet
                .get(1)
                .ok_or_else(|| io::Error::other("Truncated Opus packet"))?
                & 63,
        ),
    };
    let ticks = per_frame * frames;
    if ticks == 0 || ticks > 240 {
        return Err(io::Error::other("Invalid Opus duration"));
    }
    Ok(ticks)
}

pub(super) struct MediaReader {
    header_bytes: [u8; MEDIA_HEADER_BYTES],
    header_read: usize,
    header: Option<MediaHeader>,
    payload: Vec<u8>,
    payload_read: usize,
}

impl MediaReader {
    pub fn new() -> Self {
        Self {
            header_bytes: [0; MEDIA_HEADER_BYTES],
            header_read: 0,
            header: None,
            payload: Vec::new(),
            payload_read: 0,
        }
    }

    pub fn position(&self) -> (usize, usize) {
        (self.header_read, self.payload_read)
    }

    pub fn poll(
        &mut self,
        reader: &mut impl Read,
        limits: &MediaLimits,
        attempt: u64,
        audio: bool,
    ) -> io::Result<Option<MediaFrame>> {
        if self.header.is_none() {
            if !read_part(reader, &mut self.header_bytes, &mut self.header_read)? {
                return Ok(None);
            }
            let header =
                MediaHeader::decode(&self.header_bytes, limits).map_err(io::Error::other)?;
            if header.attempt_generation != attempt || (header.track_id == AUDIO_TRACK_ID && !audio)
            {
                return Err(io::Error::other(
                    "Media does not match the accepted attempt and tracks",
                ));
            }
            self.payload = vec![0; header.payload_bytes as usize];
            self.header = Some(header);
        }
        if !read_part(reader, &mut self.payload, &mut self.payload_read)? {
            return Ok(None);
        }
        let frame = MediaFrame {
            header: self.header.take().unwrap(),
            payload: std::mem::take(&mut self.payload),
        };
        self.header_read = 0;
        self.payload_read = 0;
        Ok(Some(frame))
    }
}

pub(super) struct ControlReader {
    prefix: [u8; 4],
    prefix_read: usize,
    payload: Vec<u8>,
    payload_read: usize,
}

impl ControlReader {
    pub fn new() -> Self {
        Self {
            prefix: [0; 4],
            prefix_read: 0,
            payload: Vec::new(),
            payload_read: 0,
        }
    }

    pub fn in_progress(&self) -> bool {
        self.prefix_read != 0
    }

    pub fn position(&self) -> (usize, usize) {
        (self.prefix_read, self.payload_read)
    }

    pub fn poll(
        &mut self,
        reader: &mut impl Read,
        maximum: usize,
    ) -> io::Result<Option<ControlMessage>> {
        if self.payload.is_empty() {
            if !read_part(reader, &mut self.prefix, &mut self.prefix_read)? {
                return Ok(None);
            }
            let length = u32::from_le_bytes(self.prefix) as usize;
            if length == 0 || length > maximum.min(opennow_media_protocol::MAX_CONTROL_BYTES) {
                return Err(io::Error::other("Invalid worker control frame length"));
            }
            self.payload = vec![0; length];
        }
        if !read_part(reader, &mut self.payload, &mut self.payload_read)? {
            return Ok(None);
        }
        let message = serde_json::from_slice(&self.payload)
            .map_err(|_| io::Error::other("Invalid worker control message"))?;
        self.prefix_read = 0;
        self.payload.clear();
        self.payload_read = 0;
        Ok(Some(message))
    }
}

fn read_part(reader: &mut impl Read, bytes: &mut [u8], offset: &mut usize) -> io::Result<bool> {
    if *offset == bytes.len() {
        return Ok(true);
    }
    let end = bytes.len().min(*offset + 64 * 1024);
    match reader.read(&mut bytes[*offset..end]) {
        Ok(0) => Err(io::ErrorKind::UnexpectedEof.into()),
        Ok(read) => {
            *offset += read;
            Ok(*offset == bytes.len())
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opennow_media_protocol::SourceStamp;
    use opennow_media_protocol::wire::encode_control;
    use std::io::Cursor;

    pub(super) fn limits() -> MediaLimits {
        MediaLimits {
            max_video_access_unit_bytes: 1024,
            max_audio_packet_bytes: 100,
            max_buffered_video_bytes: 2048,
            max_buffered_video_frames: 2,
            max_buffered_audio_ms: 20,
            max_control_message_bytes: 1024,
            max_pending_input_events: 2,
        }
    }

    fn frame(track_id: u32, keyframe: bool, payload: Vec<u8>) -> MediaFrame {
        MediaFrame {
            header: MediaHeader {
                attempt_generation: 7,
                track_id,
                payload_bytes: payload.len() as u32,
                source: SourceStamp {
                    sender_frame_id: Some(u64::MAX),
                    timestamp: u64::MAX - 9,
                    clock_rate_hz: 90_000,
                    ssrc: Some(u32::MAX),
                },
                keyframe,
                contiguous: true,
            },
            payload,
        }
    }

    #[test]
    fn video_overflow_invalidates_the_reference_chain_and_reserves_keyframe_control() {
        let mut channels = Channels::new(limits());
        for _ in 0..2 {
            channels
                .send(ControlMessage::Neutral {
                    attempt_generation: 7,
                    sequence: 1,
                })
                .unwrap();
        }
        channels
            .push_media(frame(VIDEO_TRACK_ID, true, vec![1; 1024]))
            .unwrap();
        channels
            .push_media(frame(VIDEO_TRACK_ID, false, vec![2; 1024]))
            .unwrap();
        channels
            .push_media(frame(VIDEO_TRACK_ID, false, vec![3; 1024]))
            .unwrap();
        assert!(channels.recv_video().is_none());
        assert!(matches!(
            channels.recv_control(),
            Some(WorkerEvent::Discontinuity {
                track_id: VIDEO_TRACK_ID
            })
        ));
        assert!(matches!(
            channels.outgoing(),
            Some(ControlMessage::Neutral { .. })
        ));
        assert!(matches!(
            channels.outgoing(),
            Some(ControlMessage::Keyframe {
                track_id: VIDEO_TRACK_ID,
                ..
            })
        ));
        channels
            .push_media(frame(VIDEO_TRACK_ID, false, vec![4]))
            .unwrap();
        assert!(channels.recv_video().is_none());
        channels
            .push_media(frame(VIDEO_TRACK_ID, true, vec![5]))
            .unwrap();
        assert!(!channels.recv_video().unwrap().header.contiguous);
    }

    #[test]
    fn video_bytes_and_frame_counts_are_independently_bounded() {
        let mut configured = limits();
        configured.max_buffered_video_frames = 8;
        let mut channels = Channels::new(configured);
        for _ in 0..3 {
            channels
                .push_media(frame(VIDEO_TRACK_ID, true, vec![1; 1024]))
                .unwrap();
        }
        assert_eq!(channels.video.len(), 1);
        assert_eq!(channels.video_bytes, 1024);
        let mut channels = Channels::new(limits());
        for _ in 0..3 {
            channels
                .push_media(frame(VIDEO_TRACK_ID, true, vec![1]))
                .unwrap();
        }
        assert_eq!(channels.video.len(), 1);
    }

    #[test]
    fn audio_is_bounded_by_actual_opus_duration_and_bytes() {
        let mut channels = Channels::new(limits());
        for _ in 0..100 {
            channels
                .push_media(frame(AUDIO_TRACK_ID, false, vec![0x80; 100]))
                .unwrap();
        }
        assert_eq!(channels.audio.len(), 8);
        assert_eq!(channels.audio_ticks, 40);
        assert_eq!(channels.audio_bytes, 800);
        assert!(matches!(
            channels.recv_control(),
            Some(WorkerEvent::Discontinuity {
                track_id: AUDIO_TRACK_ID
            })
        ));
        assert!(opus_duration_ticks(&[0x83]).is_err());
        assert!(opus_duration_ticks(&[0x83, 0]).is_err());
        assert!(opus_duration_ticks(&[0x83, 63]).is_err());
        assert_eq!(opus_duration_ticks(&[0]).unwrap(), 20);
    }

    #[test]
    fn reserved_control_is_not_starved_by_a_full_input_queue() {
        let mut channels = Channels::new(limits());
        let input = ControlMessage::Input {
            attempt_generation: 7,
            sequence: 1,
            captured_us: 0,
            event: opennow_media_protocol::wire::InputEvent::Key {
                virtual_key: 65,
                modifiers: 0,
                pressed: true,
            },
        };
        channels.send(input.clone()).unwrap();
        channels.send(input.clone()).unwrap();
        assert_eq!(
            channels.send(input).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        channels
            .send(ControlMessage::Keyframe {
                attempt_generation: 7,
                track_id: 1,
            })
            .unwrap();
        channels
            .send(ControlMessage::Neutral {
                attempt_generation: 7,
                sequence: 1,
            })
            .unwrap();
        channels
            .send(ControlMessage::Stop {
                attempt_generation: 7,
                sequence: 2,
            })
            .unwrap();
        assert!(matches!(
            channels.outgoing(),
            Some(ControlMessage::Stop { .. })
        ));
        assert!(matches!(
            channels.outgoing(),
            Some(ControlMessage::Neutral { .. })
        ));
        assert!(matches!(
            channels.outgoing(),
            Some(ControlMessage::Keyframe { .. })
        ));
        assert!(channels.outgoing().is_none());
    }

    #[test]
    fn progress_flood_coalesces_three_latest_stages_without_consuming_input_budget() {
        let mut channels = Channels::new(limits());
        channels
            .send(ControlMessage::Neutral {
                attempt_generation: 7,
                sequence: 0,
            })
            .unwrap();
        let input = |sequence| ControlMessage::Input {
            attempt_generation: 7,
            sequence,
            captured_us: 0,
            event: opennow_media_protocol::wire::InputEvent::Key {
                virtual_key: 65,
                modifiers: 0,
                pressed: true,
            },
        };
        channels.send(input(1)).unwrap();
        channels.send(input(2)).unwrap();
        let input_bytes = channels.outgoing_bytes;
        for local_us in 0..1000 {
            let mut provenance = frame(VIDEO_TRACK_ID, true, vec![0]).header.provenance();
            provenance.source.as_mut().unwrap().sender_frame_id = Some(local_us);
            for stage in [
                FrameStage::Accepted,
                FrameStage::Decoded,
                FrameStage::Presented,
            ] {
                channels
                    .send(ControlMessage::FrameProgress {
                        provenance,
                        stage,
                        local_us,
                    })
                    .unwrap();
            }
        }
        assert_eq!(channels.outgoing_bytes, input_bytes);
        assert_eq!(channels.outgoing.len(), 2);
        assert_eq!(
            channels
                .progress
                .iter()
                .filter(|slot| slot.is_some())
                .count(),
            3
        );
        assert_eq!(channels.coalesced_frame_progress(), 2997);
        assert_eq!(
            channels.send(input(3)).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        channels
            .send(ControlMessage::Keyframe {
                attempt_generation: 7,
                track_id: VIDEO_TRACK_ID,
            })
            .unwrap();
        assert!(matches!(
            channels.outgoing(),
            Some(ControlMessage::Neutral { .. })
        ));
        assert!(matches!(
            channels.outgoing(),
            Some(ControlMessage::Keyframe { .. })
        ));
        for expected in [1, 2] {
            assert!(
                matches!(channels.outgoing(), Some(ControlMessage::Input { sequence, .. }) if sequence == expected)
            );
        }
        for expected in [
            FrameStage::Accepted,
            FrameStage::Decoded,
            FrameStage::Presented,
        ] {
            let Some(ControlMessage::FrameProgress {
                provenance,
                stage,
                local_us,
            }) = channels.outgoing()
            else {
                panic!("missing coalesced stage")
            };
            assert_eq!(
                std::mem::discriminant(&stage),
                std::mem::discriminant(&expected)
            );
            assert_eq!(local_us, 999);
            assert_eq!(provenance.track_id, VIDEO_TRACK_ID);
            assert_eq!(provenance.source.unwrap().sender_frame_id, Some(999));
        }
        assert!(channels.outgoing().is_none());
        assert_eq!(channels.coalesced_frame_progress(), 2997);
        channels.send(input(3)).unwrap();
        assert!(matches!(
            channels.outgoing(),
            Some(ControlMessage::Input { sequence: 3, .. })
        ));
        assert!(
            channels
                .send(ControlMessage::Ended {
                    attempt_generation: 7
                })
                .is_err()
        );
    }

    #[test]
    fn repeated_media_discontinuities_cannot_starve_control_acknowledgments() {
        let mut channels = Channels::new(limits());
        channels
            .incoming(ControlMessage::Ack {
                attempt_generation: 7,
                sequence: 9,
                kind: opennow_media_protocol::wire::AckKind::Neutral,
            })
            .unwrap();
        channels
            .push_media(frame(VIDEO_TRACK_ID, false, vec![0]))
            .unwrap();
        assert!(matches!(
            channels.recv_control(),
            Some(WorkerEvent::Control(ControlMessage::Ack {
                sequence: 9,
                ..
            }))
        ));
        assert!(matches!(
            channels.recv_control(),
            Some(WorkerEvent::Discontinuity {
                track_id: VIDEO_TRACK_ID
            })
        ));
    }

    #[test]
    fn pending_control_has_a_byte_budget_and_neutral_discards_stale_inputs() {
        let mut configured = limits();
        configured.max_pending_input_events = 1024;
        configured.max_control_message_bytes = 64 * 1024;
        let mut channels = Channels::new(configured);
        let input = ControlMessage::Input {
            attempt_generation: 7,
            sequence: 1,
            captured_us: 0,
            event: opennow_media_protocol::wire::InputEvent::Text {
                paste_id: 1,
                offset: 0,
                final_chunk: true,
                utf8: "a".repeat(60 * 1024),
            },
        };
        let mut accepted = 0;
        while channels.send(input.clone()).is_ok() {
            accepted += 1;
        }
        assert!(accepted > 0 && accepted < 1024);
        assert!(channels.outgoing_bytes <= 1024 * 1024);
        channels
            .send(ControlMessage::Neutral {
                attempt_generation: 7,
                sequence: 2,
            })
            .unwrap();
        assert_eq!(channels.outgoing_bytes, 0);
        assert!(matches!(
            channels.outgoing(),
            Some(ControlMessage::Neutral { .. })
        ));
        assert!(channels.outgoing().is_none());
        channels.send(input).unwrap();
    }

    struct Fragmented {
        bytes: Cursor<Vec<u8>>,
        blocked: bool,
    }
    impl Read for Fragmented {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            self.blocked = !self.blocked;
            if self.blocked {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            self.bytes.read(&mut out[..1])
        }
    }

    #[test]
    fn fragmented_binary_frames_preserve_full_sender_provenance() {
        let expected = frame(VIDEO_TRACK_ID, true, vec![1, 2, 3]);
        let mut bytes = expected.header.encode().to_vec();
        bytes.extend_from_slice(&expected.payload);
        let mut input = Fragmented {
            bytes: Cursor::new(bytes),
            blocked: false,
        };
        let mut reader = MediaReader::new();
        let actual = loop {
            if let Some(frame) = reader.poll(&mut input, &limits(), 7, false).unwrap() {
                break frame;
            }
        };
        assert_eq!(actual.header, expected.header);
        assert_eq!(actual.payload, expected.payload);
    }

    #[test]
    fn rejects_malformed_oversized_wrong_attempt_and_unaccepted_track_before_payload() {
        for variant in 0..5 {
            let mut header = frame(VIDEO_TRACK_ID, true, vec![0]).header;
            match variant {
                0 => header.payload_bytes = 1025,
                1 => header.attempt_generation = 8,
                2 => header.track_id = AUDIO_TRACK_ID,
                3 => header.track_id = 3,
                _ => header.source.clock_rate_hz = 0,
            }
            assert!(
                MediaReader::new()
                    .poll(&mut Cursor::new(header.encode()), &limits(), 7, false)
                    .is_err()
            );
        }
        let mut bytes = frame(VIDEO_TRACK_ID, true, vec![0]).header.encode();
        bytes[0] = 0;
        assert!(
            MediaReader::new()
                .poll(&mut Cursor::new(bytes), &limits(), 7, false)
                .is_err()
        );
    }

    #[test]
    fn control_parser_bounds_lengths_and_survives_partial_io() {
        for length in [0u32, 1025, u32::MAX] {
            let mut reader = ControlReader::new();
            assert!(
                reader
                    .poll(&mut Cursor::new(length.to_le_bytes()), 1024)
                    .is_err()
            );
            assert!(reader.payload.is_empty());
        }
        let bytes = encode_control(
            &ControlMessage::Ended {
                attempt_generation: 7,
            },
            1024,
        )
        .unwrap();
        let mut input = Fragmented {
            bytes: Cursor::new(bytes),
            blocked: false,
        };
        let mut reader = ControlReader::new();
        let message = loop {
            if let Some(message) = reader.poll(&mut input, 1024).unwrap() {
                break message;
            }
        };
        assert!(matches!(
            message,
            ControlMessage::Ended {
                attempt_generation: 7
            }
        ));
        assert!(!reader.in_progress());
        assert!(
            ControlReader::new()
                .poll(&mut Cursor::new([1, 0, 0, 0, b'!']), 1024)
                .is_err()
        );
    }
}
