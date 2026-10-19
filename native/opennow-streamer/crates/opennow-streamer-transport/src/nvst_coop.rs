//! Co-op hooks owned by the NVST transport: the host-side media tap after SRTP
//! decryption and the guest-side receiver that feeds forwarded plaintext RTP through
//! the same reorder, assembly and media-consumer path as a GFN session.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use str0m::media::Pt;
use str0m::rtp::{ExtensionValues, RtpHeader as BundleRtpHeader, Ssrc};

use super::{
    EncodedVideoAccessUnit, FecPacketLayout, NvstAudioReceiver, NvstAudioTrack, NvstDropReason,
    NvstReceiveEvent, NvstVideoCodec, NvstVideoConfig, NvstVideoReceiver, RtpHeader, RtpPacket,
    matches_audio_track,
};
use crate::{EncodedMediaFrame, MediaConsumer, TransportError, deliver_media_frame};

/// Upper bound for the tap queue between the NVST receive threads and the co-op worker.
pub const MAX_COOP_TAP_CAPACITY: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoopMediaKind {
    Video,
    Audio,
}

/// One decrypted NVST RTP packet, header and payload exactly as GFN sent them.
#[derive(Debug, Clone)]
pub struct CoopRtpPacket {
    pub kind: CoopMediaKind,
    pub packet: Arc<[u8]>,
    pub received_at: Instant,
}

/// Bounded hook between NVST receive and the co-op host. While detached it costs one
/// relaxed atomic load per packet and never allocates.
#[derive(Debug, Default)]
pub struct NvstCoopTap {
    active: AtomicBool,
    sender: Mutex<Option<SyncSender<CoopRtpPacket>>>,
    dropped: AtomicU64,
}

impl NvstCoopTap {
    pub fn attach(&self, capacity: usize) -> Receiver<CoopRtpPacket> {
        let (sender, receiver) = sync_channel(capacity.clamp(1, MAX_COOP_TAP_CAPACITY));
        *self
            .sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sender);
        self.active.store(true, Ordering::Release);
        receiver
    }

    pub fn detach(&self) {
        self.active.store(false, Ordering::Release);
        *self
            .sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }

    pub fn is_attached(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    /// Packets discarded because the co-op worker fell behind.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    pub(super) fn offer(&self, kind: CoopMediaKind, packet: &[u8], received_at: Instant) {
        if !self.active.load(Ordering::Relaxed) {
            return;
        }
        let mut sender = self
            .sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(channel) = sender.as_ref() else {
            return;
        };
        let item = CoopRtpPacket {
            kind,
            packet: Arc::from(packet),
            received_at,
        };
        match channel.try_send(item) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
            Err(TrySendError::Disconnected(_)) => {
                *sender = None;
                self.active.store(false, Ordering::Release);
            }
        }
    }

    pub(super) fn offer_video(&self, packet: &RtpPacket, received_at: Instant) {
        if !self.active.load(Ordering::Relaxed) {
            return;
        }
        if FecPacketLayout::from_packet(packet)
            .is_some_and(|layout| layout.shard_index >= layout.data_shards)
        {
            return;
        }
        self.offer(CoopMediaKind::Video, &packet.plaintext, received_at);
    }

    /// str0m hands the bundle audio over as a parsed header plus payload; rebuild the
    /// RTP fixed header with the sender's sequence number, timestamp, SSRC and payload type.
    pub(super) fn offer_audio(
        &self,
        header: &BundleRtpHeader,
        payload: &[u8],
        received_at: Instant,
    ) {
        if !self.active.load(Ordering::Relaxed) {
            return;
        }
        let mut packet = Vec::with_capacity(12 + payload.len());
        packet.push(0x80);
        packet.push((*header.payload_type & 0x7f) | if header.marker { 0x80 } else { 0 });
        packet.extend_from_slice(&header.sequence_number.to_be_bytes());
        packet.extend_from_slice(&header.timestamp.to_be_bytes());
        packet.extend_from_slice(&(*header.ssrc).to_be_bytes());
        packet.extend_from_slice(payload);
        self.offer(CoopMediaKind::Audio, &packet, received_at);
    }
}

/// Counters a guest exposes for its stats event and diagnostics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CoopGuestReceiveStats {
    pub video_packets: u64,
    pub audio_packets: u64,
    pub frames_delivered: u64,
    pub keyframes_delivered: u64,
    pub frames_dropped: u64,
    pub packets_rejected: u64,
}

/// Guest-side receiver: forwarded plaintext NVST RTP in, assembled access units and Opus
/// frames out through the normal bounded media consumer.
pub struct NvstCoopGuestReceiver {
    video: NvstVideoReceiver,
    audio: NvstAudioReceiver,
    audio_track: Option<NvstAudioTrack>,
    media_consumer: MediaConsumer,
    origin: Instant,
    delivery_gap: bool,
    stats: CoopGuestReceiveStats,
}

impl NvstCoopGuestReceiver {
    pub fn new(
        codec: NvstVideoCodec,
        audio_track: Option<NvstAudioTrack>,
        media_consumer: MediaConsumer,
    ) -> Self {
        let video = NvstVideoReceiver::new(NvstVideoConfig::coop_guest(codec, audio_track.clone()));
        video.config.feedback.request_keyframe();
        Self {
            video,
            audio: NvstAudioReceiver::default(),
            audio_track,
            media_consumer,
            origin: Instant::now(),
            delivery_gap: false,
            stats: CoopGuestReceiveStats::default(),
        }
    }

    pub fn stats(&self) -> CoopGuestReceiveStats {
        self.stats
    }

    /// Set while the decoder needs a reference frame; cleared once a keyframe assembles.
    pub fn keyframe_needed(&self) -> bool {
        self.video.config.feedback.keyframe_needed()
    }

    pub fn process_video(&mut self, packet: &[u8], now: Instant) -> Result<(), TransportError> {
        self.stats.video_packets += 1;
        let events = self.video.process_plain_rtp(packet, now);
        self.consume(events, now)
    }

    pub fn poll(&mut self, now: Instant) -> Result<(), TransportError> {
        let mut events = self.video.poll_recovery(now);
        events.extend(self.video.poll_timeout(now));
        self.consume(events, now)
    }

    pub fn process_audio(&mut self, packet: &[u8], now: Instant) -> Result<(), TransportError> {
        let Some(track) = self.audio_track.as_ref() else {
            return Ok(());
        };
        let Ok(header) = RtpHeader::parse(packet) else {
            self.stats.packets_rejected += 1;
            return Ok(());
        };
        let Ok(payload) = header.payload(packet) else {
            self.stats.packets_rejected += 1;
            return Ok(());
        };
        if !matches_audio_track(track, header.payload_type, header.ssrc) {
            self.stats.packets_rejected += 1;
            return Ok(());
        }
        self.stats.audio_packets += 1;
        let bundle_header = BundleRtpHeader {
            version: 2,
            has_padding: false,
            has_extension: false,
            csrc_count: 0,
            marker: packet[1] & 0x80 != 0,
            payload_type: Pt::from(header.payload_type),
            sequence_number: header.sequence_number,
            timestamp: header.timestamp,
            ssrc: Ssrc::from(header.ssrc),
            csrc: [0; 15],
            ext_vals: ExtensionValues::default(),
            header_len: header.payload_offset,
        };
        let received_at_us = now
            .saturating_duration_since(self.origin)
            .as_micros()
            .try_into()
            .unwrap_or(u64::MAX);
        let frames =
            match self
                .audio
                .depacketize(track, &bundle_header, Arc::from(payload), received_at_us)
            {
                Ok(frames) => frames,
                Err(_) => {
                    self.stats.packets_rejected += 1;
                    return Ok(());
                }
            };
        for frame in frames {
            match self.audio.deliver(&self.media_consumer, frame) {
                Ok(()) | Err(TransportError::MediaConsumerBackpressured) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn consume(
        &mut self,
        events: Vec<NvstReceiveEvent>,
        now: Instant,
    ) -> Result<(), TransportError> {
        for event in events {
            match event {
                NvstReceiveEvent::Frame(frame) => self.deliver_video(frame, now)?,
                NvstReceiveEvent::RecoveryNeeded(_) => {
                    self.video.recover();
                    self.video.config.feedback.request_keyframe();
                }
                NvstReceiveEvent::Dropped(
                    NvstDropReason::MalformedRtp(_)
                    | NvstDropReason::UnexpectedPayloadType { .. }
                    | NvstDropReason::UnexpectedSsrc { .. }
                    | NvstDropReason::ReplayRejected,
                ) => self.stats.packets_rejected += 1,
                _ => {}
            }
        }
        Ok(())
    }

    fn deliver_video(
        &mut self,
        frame: EncodedVideoAccessUnit,
        now: Instant,
    ) -> Result<(), TransportError> {
        let feedback = &self.video.config.feedback;
        let frame_index = frame.frame_index;
        let keyframe = frame.keyframe;
        let media_frame = EncodedMediaFrame {
            mid: "nvst-video-0".to_owned(),
            codec: frame.codec.label().to_owned(),
            payload: Arc::from(frame.bytes),
            frame_index: Some(frame_index),
            rtp_timestamp: u64::from(frame.timestamp),
            clock_rate_hz: 90_000,
            channels: None,
            received_at_us: now
                .saturating_duration_since(self.origin)
                .as_micros()
                .try_into()
                .unwrap_or(u64::MAX),
            keyframe: frame.keyframe,
            contiguous: frame.contiguous && !self.delivery_gap,
            ssrc: None,
        };
        feedback.publish_assembled_frame(frame_index, now);
        match deliver_media_frame(&self.media_consumer, media_frame) {
            Ok(()) => {
                self.delivery_gap = false;
                self.stats.frames_delivered += 1;
                self.stats.keyframes_delivered += u64::from(keyframe);
                Ok(())
            }
            Err(TransportError::MediaConsumerBackpressured) => {
                feedback.retire_undelivered_frame(frame_index);
                feedback.request_keyframe();
                self.delivery_gap = true;
                self.stats.frames_dropped += 1;
                Ok(())
            }
            Err(error) => {
                feedback.retire_undelivered_frame(frame_index);
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::RtpPacketOrigin;
    use super::*;

    const FLAG_PIC: u8 = 0x01;
    const FLAG_EOF: u8 = 0x02;
    const FLAG_SOF: u8 = 0x04;

    fn plaintext(seq: u16, flags: u8, fec_word: u32, media: &[u8]) -> Vec<u8> {
        let mut packet = vec![0x90, 0x80 | 96];
        packet.extend_from_slice(&seq.to_be_bytes());
        packet.extend_from_slice(&9_000_u32.to_be_bytes());
        packet.extend_from_slice(&0x1122_3344_u32.to_be_bytes());
        packet.extend_from_slice(&0x4753_u16.to_be_bytes());
        packet.extend_from_slice(&4_u16.to_be_bytes());
        packet.extend_from_slice(&(u32::from(seq) << 8).to_le_bytes());
        packet.extend_from_slice(&1_u32.to_le_bytes());
        packet.extend_from_slice(&[flags, 0, 0, 0]);
        packet.extend_from_slice(&fec_word.to_le_bytes());
        packet.extend_from_slice(media);
        packet
    }

    fn packet(bytes: Vec<u8>) -> RtpPacket {
        RtpPacket {
            index: 0,
            header: RtpHeader::parse(&bytes).unwrap(),
            plaintext: bytes,
            origin: RtpPacketOrigin::Authenticated,
        }
    }

    #[test]
    fn detached_tap_forwards_nothing_and_attached_tap_skips_fec_repair() {
        let tap = NvstCoopTap::default();
        let data = plaintext(
            7,
            FLAG_SOF | FLAG_EOF | FLAG_PIC,
            0,
            &[0, 0, 0, 1, 0x65, 1, 2],
        );
        let now = Instant::now();
        tap.offer_video(&packet(data.clone()), now);
        assert!(!tap.is_attached());
        let receiver = tap.attach(2);
        tap.offer_video(&packet(data.clone()), now);
        let forwarded = receiver.try_recv().unwrap();
        assert_eq!(forwarded.kind, CoopMediaKind::Video);
        assert_eq!(&forwarded.packet[..], &data[..]);
        let repair_word = (2_u32 << 22) | (2 << 12) | (50 << 4);
        let data_word = (2_u32 << 22) | (1 << 12) | (50 << 4);
        tap.offer_video(&packet(plaintext(8, FLAG_PIC, repair_word, &[9; 8])), now);
        assert!(
            receiver.try_recv().is_err(),
            "repair shards are not forwarded"
        );
        tap.offer_video(&packet(plaintext(9, FLAG_PIC, data_word, &[9; 8])), now);
        assert!(
            receiver.try_recv().is_ok(),
            "data shards of FEC blocks are forwarded"
        );
        for _ in 0..3 {
            tap.offer_video(&packet(data.clone()), now);
        }
        assert_eq!(
            tap.dropped(),
            1,
            "a full tap drops instead of blocking NVST receive"
        );
        tap.detach();
        drop(receiver);
        tap.offer_video(&packet(data), now);
        assert_eq!(tap.dropped(), 1);
    }

    #[test]
    fn audio_tap_rebuilds_the_sender_header() {
        let tap = NvstCoopTap::default();
        let receiver = tap.attach(1);
        let header = BundleRtpHeader {
            version: 2,
            has_padding: false,
            has_extension: false,
            csrc_count: 0,
            marker: true,
            payload_type: Pt::from(111),
            sequence_number: 0xabcd,
            timestamp: 0x0102_0304,
            ssrc: Ssrc::from(0x0a0b_0c0d),
            csrc: [0; 15],
            ext_vals: ExtensionValues::default(),
            header_len: 12,
        };
        tap.offer_audio(&header, &[1, 2, 3], Instant::now());
        let forwarded = receiver.try_recv().unwrap();
        assert_eq!(forwarded.kind, CoopMediaKind::Audio);
        assert_eq!(
            &forwarded.packet[..],
            &[
                0x80,
                0x80 | 111,
                0xab,
                0xcd,
                1,
                2,
                3,
                4,
                0x0a,
                0x0b,
                0x0c,
                0x0d,
                1,
                2,
                3
            ]
        );
    }

    #[test]
    fn guest_receiver_assembles_forwarded_packets_with_sender_identifiers() {
        let (consumer, frames) = std::sync::mpsc::sync_channel(4);
        let mut receiver = NvstCoopGuestReceiver::new(NvstVideoCodec::H264, None, consumer);
        assert!(
            receiver.keyframe_needed(),
            "a joining guest asks for a keyframe before any frame arrives"
        );
        let now = Instant::now();
        let first = plaintext(40, FLAG_SOF | FLAG_PIC, 0, &[0, 0, 0, 1, 0x65, 1, 2]);
        let mut last = plaintext(41, FLAG_EOF | FLAG_PIC, 0, &[3, 4]);
        last[1] |= 0x80;
        receiver.process_video(&first, now).unwrap();
        receiver.process_video(&last, now).unwrap();
        receiver.process_video(&last, now).unwrap();
        let frame = frames.try_recv().unwrap();
        assert_eq!(&frame.payload[..], &[0, 0, 0, 1, 0x65, 1, 2, 3, 4]);
        assert_eq!(frame.frame_index, Some(1));
        assert_eq!(frame.rtp_timestamp, 9_000);
        assert!(frame.keyframe);
        assert!(!receiver.keyframe_needed());
        assert_eq!(receiver.stats().keyframes_delivered, 1);
        assert_eq!(receiver.stats().video_packets, 3);
        assert_eq!(
            receiver.stats().packets_rejected,
            1,
            "the replayed packet is rejected"
        );
        assert!(frames.try_recv().is_err());
    }
}
