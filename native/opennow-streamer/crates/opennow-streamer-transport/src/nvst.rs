//! Independently authored NVST video receive transport.
//!
//! This module only implements the receive side of the classic NVST video handoff:
//! authenticated SRTP video datagrams from the negotiated peer become bounded H.264
//! Annex-B access units. Standard Opus RTP and the existing input data-channel contract
//! use the negotiated DTLS bundle. NVIDIA's systematic Reed-Solomon video FEC is repaired before
//! access-unit assembly so isolated UDP loss does not flush the hardware decoder reference chain.

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use aes::cipher::{BlockEncrypt, KeyIvInit, StreamCipher};
use aes::{Aes128, Aes256};
use crc32fast::hash as crc32;
use ctr::{Ctr32BE, Ctr128BE};
use ghash::{GHash, universal_hash::UniversalHash};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha1::Sha1;
use socket2::{Domain, Protocol, Socket, Type};
use subtle::ConstantTimeEq;
use thiserror::Error;

use str0m::channel::ChannelId;
use str0m::config::Fingerprint;
use str0m::format::{Codec, FormatParams};
use str0m::media::{Frequency, MediaKind, Mid};
use str0m::net::{Protocol as RtcProtocol, Receive};
use str0m::rtp::Ssrc;
use str0m::{Candidate, Event, IceCreds, Input, Output, Rtc, RtcConfig};

use super::nvst_control::{
    DEFAULT_FRAME_TIME_US, FRAMES_PER_PACING_REPORT, QOS_REPORT_INTERVAL, QOS_WARM_UP, QosReport,
    frame_ack, frame_pacing_report, idr_request,
};
use super::nvst_input::{
    NvstInputChannelState, NvstInputChannels, NvstInputCodec, native_input_type_is_motion,
    native_input_type_name, native_input_types, next_control_keepalive, server_cursor_messages,
};
use super::{
    EncodedMediaFrame, MediaConsumer, TransportError, deliver_media_frame, install_crypto,
};

const RTP_FIXED_HEADER_LEN: usize = 12;
const SRTP_AES_CM_HMAC_SHA1_80_TAG_LEN: usize = 10;
/// RFC 7714 `AEAD_AES_*_GCM` profiles carry a 16-byte authentication tag.
const SRTP_AEAD_AES_GCM_TAG_LEN: usize = 16;
/// NVIDIA's `SecureRtp` (libBifrost2) maps `sec_serv_conf_and_auth` + 256-bit keys
/// to `srtp_crypto_policy_set_aes_gcm_256_8_auth` — an 8-byte tag, not RFC 7714's 16.
const SRTP_AEAD_AES_GCM_8_TAG_LEN: usize = 8;

/// RFC 3711 / libsrtp SRTP labels. Hook captures of 0x03/0x05 were SRTCP (same
/// master key, separate session keys). Mjolnir video is RTP, so use 0x00/0x02.
const GFN_SRTP_KEY_LABEL: u8 = 0x00;
const GFN_SRTP_SALT_LABEL: u8 = 0x02;
/// RFC 3711 SRTCP KDF labels (same master key, separate session keys).
const GFN_SRTCP_KEY_LABEL: u8 = 0x03;
const GFN_SRTCP_SALT_LABEL: u8 = 0x05;
const SRTCP_ENCRYPTED_FLAG: u32 = 0x8000_0000;
const RTCP_SENDER_SSRC: u32 = 0x4f4e_4f57; // "ONOW"
const SRTCP_RR_INTERVAL: Duration = Duration::from_secs(1);
// Start recovery promptly without retransmitting the same tiny range on every poll. Four
// milliseconds still fits inside the reorder grace period while avoiding self-inflicted bursts.
const RTCP_RECOVERY_INTERVAL: Duration = Duration::from_millis(10);
const KEYFRAME_REQUEST_COOLDOWN: Duration = Duration::from_millis(250);
const MAX_PENDING_NACK_RANGES: usize = 16;
const MAX_PENDING_FRAME_ACKS: usize = 512;

fn verbose_diagnostics_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("OPENNOW_NVST_TRACE")
            .ok()
            .is_some_and(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "on"))
    })
}
const MAX_NACK_PACKET_COUNT: usize = 64;
const MAX_NACK_FCI_ENTRIES: usize = MAX_NACK_PACKET_COUNT.div_ceil(17);
// At 120 FPS several FEC blocks can arrive during a single retransmission round trip. Keep them
// ordered instead of throwing away the incomplete head block when its successor arrives.
const MAX_PENDING_FEC_BLOCKS: usize = 128;
const NV_VIDEO_PACKET_LEN: usize = 16;
// GameStream's Reed-Solomon shards cover packetSize plus room for the fixed RTP header and
// extension prefix. This is MAX_RTP_HEADER_SIZE in Moonlight's reference receive path.
const NVST_FEC_RTP_HEADER_ALLOWANCE: usize = 16;
const DEFAULT_NVST_VIDEO_PACKET_SIZE: usize = 1_280;
const MIN_NVST_VIDEO_PACKET_SIZE: usize = 256;
const MAX_NVST_VIDEO_PACKET_SIZE: usize = 65_519;
// Match the official client's bounded NACK/dejitter envelope: it keeps up to
// 1,024 RTP packets available for late or retransmitted packets and permits a
// 2,048-entry NACK queue. A 32-packet window is only a few milliseconds at
// 150 Mbps and turns recoverable reordering into visible keyframe hitches.
const DEFAULT_REORDER_WINDOW: usize = 1_024;
const MAX_REORDER_WINDOW: usize = 2_048;
// Cloud packet reordering plus RTCP-over-SCTP NACK round trips can exceed one or two frame times.
// Keep the wait exceptional and bounded, but long enough for a retransmission to beat an IDR
// reset. This adds latency only while a sequence gap is open; the normal path still drains at once.
const MJOLNIR_REORDER_DEQUEUE_TIMEOUT: Duration = Duration::from_millis(150);
// The replay filter must be at least as deep as the reorder/NACK window. The old 64-packet bitmap
// rejected valid retransmissions after roughly 10 ms on a high-bitrate stream, guaranteeing that
// every NACK eventually degraded into a keyframe reset.
const SRTP_REPLAY_WINDOW_PACKETS: usize = MAX_REORDER_WINDOW;
const SRTP_REPLAY_WINDOW_WORDS: usize = SRTP_REPLAY_WINDOW_PACKETS.div_ceil(u64::BITS as usize);
const NVST_UDP_RECEIVE_BUFFER_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_MAX_ACCESS_UNIT_BYTES: usize = 2 * 1024 * 1024;
const MAX_ACCESS_UNIT_BYTES: usize = 16 * 1024 * 1024;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const MIN_TIMEOUT: Duration = Duration::from_millis(250);
const MAX_TIMEOUT: Duration = Duration::from_secs(90);
const MAX_PING_BYTES: usize = 512;
const PING_INTERVAL_BEFORE_CONNECTION: Duration = Duration::from_millis(20);
const PING_INTERVAL_AFTER_CONNECTION: Duration = Duration::from_millis(100);
const CURSOR_CAPTURE_RETRY_INTERVAL: Duration = Duration::from_millis(250);
const MAX_CURSOR_CAPTURE_ATTEMPTS: u8 = 8;
const UDP_RECEIVE_POLL_INTERVAL: Duration = Duration::from_millis(10);
// The WebRTC bundle owns the SCTP input channels. A 10 ms socket wait batches
// raw mouse reports and makes high-refresh streams feel closer to 100 Hz.
const CONTROL_RECEIVE_POLL_INTERVAL: Duration = Duration::from_millis(1);
const STUN_HEADER_LEN: usize = 20;
const STUN_MAGIC_COOKIE: u32 = 0x2112_a442;
const STUN_BINDING_REQUEST: u16 = 0x0001;
const STUN_BINDING_SUCCESS_RESPONSE: u16 = 0x0101;
const STUN_ATTR_USERNAME: u16 = 0x0006;
const STUN_ATTR_MESSAGE_INTEGRITY: u16 = 0x0008;
const STUN_ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;
const STUN_ATTR_FINGERPRINT: u16 = 0x8028;
const STUN_FINGERPRINT_XOR: u32 = 0x5354_554e;
const MAX_ICE_CREDENTIAL_BYTES: usize = 256;

/// The independently documented `NV_VIDEO_PACKET` flag values used by an earlier OpenNOW
/// implementation. This module does not borrow code or binaries from NVIDIA.
const FLAG_CONTAINS_PIC_DATA: u8 = 0x01;
const FLAG_EOF: u8 = 0x02;
const FLAG_SOF: u8 = 0x04;
const STREAM_PACKET_INDEX_MASK: u32 = 0x00ff_ffff;
/// Mjolnir video RTP packets carry the per-packet video metadata in a fixed
/// 16-byte RTP extension block with profile `0x4753` ("GS"), not in the payload.
const GS_VIDEO_EXTENSION_PROFILE: u16 = 0x4753;
const MAX_GS_FRAME_HEADER_BYTES: usize = 64;
const GS_SHORT_FRAME_HEADER_BYTES: usize = 8;
// GFN's current cloud NVST protocol uses a compact extended frame header. It
// is not the 44-byte extended header used by current consumer GameStream.
const GFN_EXTENDED_FRAME_HEADER_BYTES: usize = 20;
const GFN_RED_PAYLOAD_TYPE: u8 = 63;
const GFN_OPUS_PAYLOAD_TYPE: u8 = 111;
const MAX_REDUNDANT_AUDIO_BLOCKS: usize = 8;
const MAX_OPUS_PACKET_BYTES: usize = 1_275;

fn diagnostic_hex(bytes: &[u8], limit: usize) -> String {
    let mut value = bytes
        .iter()
        .take(limit)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if bytes.len() > limit {
        value.push_str("...");
    }
    value
}

type Aes256Ctr = Ctr128BE<Aes256>;
type Aes128Ctr = Ctr128BE<Aes128>;
type HmacSha1 = Hmac<Sha1>;

/// The SRTP profile must come from negotiated metadata. The legacy `nvstVideo` handoff has no
/// profile field; Bifrost's Mjolnir video path hardcodes `aes_gcm_256_8_auth`, so the legacy
/// default selects the 8-byte-tag GCM variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvstSrtpProfile {
    AeadAes128Gcm,
    AeadAes256Gcm,
    AeadAes128Gcm8,
    AeadAes256Gcm8,
    AesCm128HmacSha1_32,
    AesCm128HmacSha1_80,
    AesCm256HmacSha1_32,
    AesCm256HmacSha1_80,
}

impl NvstSrtpProfile {
    fn parse(value: &str) -> Result<Self, NvstConfigError> {
        match value.trim().to_ascii_uppercase().as_str() {
            "AEAD_AES_128_GCM" | "SRTP_AEAD_AES_128_GCM" => Ok(Self::AeadAes128Gcm),
            "AEAD_AES_256_GCM" | "SRTP_AEAD_AES_256_GCM" => Ok(Self::AeadAes256Gcm),
            "AEAD_AES_128_GCM_8" | "SRTP_AEAD_AES_128_GCM_8" => Ok(Self::AeadAes128Gcm8),
            "AEAD_AES_256_GCM_8" | "SRTP_AEAD_AES_256_GCM_8" => Ok(Self::AeadAes256Gcm8),
            "AES_CM_128_HMAC_SHA1_32" | "SRTP_AES_CM_128_HMAC_SHA1_32" => {
                Ok(Self::AesCm128HmacSha1_32)
            }
            "AES_CM_128_HMAC_SHA1_80" | "SRTP_AES_CM_128_HMAC_SHA1_80" => {
                Ok(Self::AesCm128HmacSha1_80)
            }
            "AES_CM_256_HMAC_SHA1_32" | "SRTP_AES_CM_256_HMAC_SHA1_32" => {
                Ok(Self::AesCm256HmacSha1_32)
            }
            "AES_CM_256_HMAC_SHA1_80" | "SRTP_AES_CM_256_HMAC_SHA1_80" => {
                Ok(Self::AesCm256HmacSha1_80)
            }
            other => Err(NvstConfigError::UnsupportedSrtpProfile(other.to_owned())),
        }
    }
}

/// Video codecs accepted by the native NVST receive path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvstVideoCodec {
    H264,
    H265,
    Av1,
}

impl NvstVideoCodec {
    fn parse(value: &str) -> Result<Self, NvstConfigError> {
        match value.trim().to_ascii_uppercase().as_str() {
            "H264" | "AVC" => Ok(Self::H264),
            "H265" | "HEVC" => Ok(Self::H265),
            "AV1" => Ok(Self::Av1),
            other => Err(NvstConfigError::UnsupportedCodec(other.to_owned())),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::H264 => "H264",
            Self::H265 => "H265",
            Self::Av1 => "AV1",
        }
    }
}

/// Why an NVST handoff cannot be selected. Callers should retain their WebRTC fallback.
#[derive(Debug, Error)]
pub enum NvstConfigError {
    #[error("nvstVideo must be an object")]
    HandoffNotObject,
    #[error("nvstVideo is missing {0}")]
    MissingField(&'static str),
    #[error("nvstVideo.{field} must be a {expected}")]
    InvalidFieldType {
        field: &'static str,
        expected: &'static str,
    },
    #[error("nvstVideo.{field} is out of range")]
    OutOfRange { field: &'static str },
    #[error("nvstVideo.videoPeerIp is not a routable unicast IP address: {0}")]
    InvalidPeerIp(String),
    #[error("nvstVideo.srtpAesKeyHex is invalid for the selected SRTP profile")]
    InvalidAesKey,
    #[error("nvstVideo.srtpSaltHex is invalid for the selected SRTP profile")]
    InvalidSrtpSalt,
    #[error("nvstVideo.srtpProfile {0} is not implemented")]
    UnsupportedSrtpProfile(String),
    #[error("NVST video codec {0} is not implemented; supported codecs are H264, H265, and AV1")]
    UnsupportedCodec(String),
    #[error("nvstVideo.audioTrack must contain standard negotiated Opus RTP metadata")]
    InvalidAudioTrack,
    #[error("nvstTransport cannot be used without the required nvstVideo handoff")]
    MissingNvstVideoHandoff,
}

/// Explicitly records transport features that cannot be sent correctly from current wire data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvstUnsupportedFeature {
    FecRepair,
}

impl fmt::Display for NvstUnsupportedFeature {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::FecRepair => "FEC repair",
        };
        formatter.write_str(name)
    }
}

/// Feedback plane shared between the Mjolnir video receiver (which learns the
/// stream SSRC/sequence and detects unrecoverable loss) and the ICE/DTLS bundle
/// (which owns the `rtcp1` SCTP data channel used for RTCP feedback).
///
/// The official client sends RTCP Receiver Reports / PLI over an SCTP data
/// channel on the bundle ("RTCP over SCTP is a must for One SDK video to
/// function"). Without it the server stops video after a short provisional
/// window. This state lets the bundle build accurate reports for the stream the
/// Mjolnir receiver is actually seeing.
#[derive(Debug, Default)]
struct ReceptionTiming {
    first_arrival: Option<Instant>,
    first_rtp_timestamp: u32,
    last_rtp_timestamp: u32,
    last_transit: i64,
    jitter: f64,
}

#[derive(Debug, Clone, Copy)]
struct RtcpReportBlock {
    media_ssrc: u32,
    fraction_lost: u8,
    cumulative_lost: i32,
    highest_sequence: u32,
    jitter: u32,
}

#[derive(Debug, Clone, Copy)]
struct CompletedFrameFeedback {
    frame_number: u32,
    bytes: u32,
    accepted_at: Instant,
}

#[derive(Debug)]
pub struct NvstFeedbackState {
    /// Bound video stream SSRC (0 until the first packet is authenticated).
    video_ssrc: AtomicU32,
    /// Highest extended sequence number received on the video stream.
    highest_sequence: AtomicU32,
    base_sequence: AtomicU32,
    received_packets: AtomicU32,
    report_prior: Mutex<(u32, u32)>,
    reception_timing: Mutex<ReceptionTiming>,
    /// Set when the receiver hits unrecoverable loss and needs a fresh keyframe.
    keyframe_needed: AtomicBool,
    /// Missing extended RTP sequence ranges awaiting RFC 4585 generic NACK.
    pending_nacks: Mutex<VecDeque<(u64, u64)>>,
    completed_frames: AtomicU32,
    completed_frame_bytes: AtomicU64,
    last_completed_rtp_timestamp: AtomicU32,
    accepted_frame_sequence: AtomicU32,
    pending_frame_acks: Mutex<VecDeque<CompletedFrameFeedback>>,
}

impl Default for NvstFeedbackState {
    fn default() -> Self {
        Self {
            video_ssrc: AtomicU32::new(0),
            highest_sequence: AtomicU32::new(0),
            base_sequence: AtomicU32::new(u32::MAX),
            received_packets: AtomicU32::new(0),
            report_prior: Mutex::new((0, 0)),
            reception_timing: Mutex::new(ReceptionTiming::default()),
            keyframe_needed: AtomicBool::new(false),
            pending_nacks: Mutex::new(VecDeque::new()),
            completed_frames: AtomicU32::new(0),
            completed_frame_bytes: AtomicU64::new(0),
            last_completed_rtp_timestamp: AtomicU32::new(0),
            accepted_frame_sequence: AtomicU32::new(0),
            pending_frame_acks: Mutex::new(VecDeque::new()),
        }
    }
}

impl NvstFeedbackState {
    fn publish_stream(&self, ssrc: u32, highest_sequence: u32, rtp_timestamp: u32, now: Instant) {
        self.video_ssrc.store(ssrc, Ordering::Release);
        let _ = self.base_sequence.compare_exchange(
            u32::MAX,
            highest_sequence,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        self.highest_sequence
            .fetch_max(highest_sequence, Ordering::AcqRel);
        self.received_packets.fetch_add(1, Ordering::AcqRel);

        let mut timing = self
            .reception_timing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(first_arrival) = timing.first_arrival else {
            timing.first_arrival = Some(now);
            timing.first_rtp_timestamp = rtp_timestamp;
            timing.last_rtp_timestamp = rtp_timestamp;
            return;
        };
        if timing.last_rtp_timestamp == rtp_timestamp {
            return;
        }
        let arrival_ticks = now
            .saturating_duration_since(first_arrival)
            .as_secs_f64()
            .mul_add(90_000.0, 0.0) as i64;
        let rtp_ticks = i64::from(rtp_timestamp.wrapping_sub(timing.first_rtp_timestamp));
        let transit = arrival_ticks - rtp_ticks;
        let delta = transit.abs_diff(timing.last_transit) as f64;
        timing.last_rtp_timestamp = rtp_timestamp;
        timing.last_transit = transit;
        timing.jitter += (delta - timing.jitter) / 16.0;
    }

    pub fn request_keyframe(&self) {
        self.keyframe_needed.store(true, Ordering::Release);
    }

    fn request_nack(&self, first_missing_index: u64, last_missing_index: u64) {
        if first_missing_index > last_missing_index {
            return;
        }
        let mut pending = self
            .pending_nacks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((_, queued_last)) = pending.back_mut()
            && first_missing_index <= queued_last.saturating_add(1)
        {
            *queued_last = (*queued_last).max(last_missing_index);
            return;
        }
        if pending.len() == MAX_PENDING_NACK_RANGES {
            pending.pop_front();
        }
        pending.push_back((first_missing_index, last_missing_index));
    }

    fn resolve_nack(&self, received_index: u64) -> bool {
        let mut pending = self
            .pending_nacks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut updated = VecDeque::with_capacity(pending.len().saturating_add(1));
        let mut resolved = false;
        while let Some((first, last)) = pending.pop_front() {
            if received_index < first || received_index > last {
                updated.push_back((first, last));
                continue;
            }
            resolved = true;
            if first < received_index {
                updated.push_back((first, received_index - 1));
            }
            if received_index < last {
                updated.push_back((received_index + 1, last));
            }
        }
        *pending = updated;
        resolved
    }

    fn publish_completed_frame(&self, frame: &EncodedVideoAccessUnit) {
        self.completed_frames.fetch_add(1, Ordering::AcqRel);
        self.completed_frame_bytes.fetch_add(
            u64::try_from(frame.bytes.len()).unwrap_or(u64::MAX),
            Ordering::AcqRel,
        );
        self.last_completed_rtp_timestamp
            .store(frame.timestamp, Ordering::Release);
        if frame.keyframe {
            self.keyframe_needed.store(false, Ordering::Release);
        }
    }

    pub fn publish_accepted_frame(&self, bytes: u32, accepted_at: Instant) {
        let frame_number = self
            .accepted_frame_sequence
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        let mut pending = self
            .pending_frame_acks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if pending.len() == MAX_PENDING_FRAME_ACKS {
            pending.pop_front();
        }
        pending.push_back(CompletedFrameFeedback {
            frame_number,
            bytes,
            accepted_at,
        });
    }

    fn take_completed_frame(&self) -> Option<CompletedFrameFeedback> {
        self.pending_frame_acks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop_front()
    }

    fn completed_frame_snapshot(&self) -> (u32, u32, u32) {
        (
            self.completed_frames.load(Ordering::Acquire),
            self.completed_frame_bytes.load(Ordering::Acquire) as u32,
            self.last_completed_rtp_timestamp.load(Ordering::Acquire),
        )
    }

    /// SSRC + highest sequence for the next Receiver Report, if a stream is bound.
    fn stream_snapshot(&self) -> Option<(u32, u32)> {
        let ssrc = self.video_ssrc.load(Ordering::Acquire);
        (ssrc != 0).then(|| (ssrc, self.highest_sequence.load(Ordering::Acquire)))
    }

    fn report_snapshot(&self, update_interval: bool) -> Option<RtcpReportBlock> {
        let media_ssrc = self.video_ssrc.load(Ordering::Acquire);
        let base = self.base_sequence.load(Ordering::Acquire);
        if media_ssrc == 0 || base == u32::MAX {
            return None;
        }
        let highest_sequence = self.highest_sequence.load(Ordering::Acquire);
        let received = self.received_packets.load(Ordering::Acquire);
        let expected = highest_sequence.wrapping_sub(base).wrapping_add(1);
        let cumulative_lost = i64::from(expected) - i64::from(received);
        let cumulative_lost = cumulative_lost.clamp(-0x80_0000, 0x7f_ffff) as i32;
        let fraction_lost = if update_interval {
            let mut prior = self
                .report_prior
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let expected_interval = expected.wrapping_sub(prior.0);
            let received_interval = received.wrapping_sub(prior.1);
            *prior = (expected, received);
            let lost_interval = expected_interval.saturating_sub(received_interval);
            if expected_interval == 0 {
                0
            } else {
                ((u64::from(lost_interval) << 8) / u64::from(expected_interval)).min(255) as u8
            }
        } else {
            0
        };
        let jitter = self
            .reception_timing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .jitter
            .clamp(0.0, f64::from(u32::MAX)) as u32;
        Some(RtcpReportBlock {
            media_ssrc,
            fraction_lost,
            cumulative_lost,
            highest_sequence,
            jitter,
        })
    }

    /// Atomically takes the pending keyframe request, returning true if one was set.
    fn take_keyframe_request(&self) -> bool {
        self.keyframe_needed.swap(false, Ordering::AcqRel)
    }

    fn take_nack(&self) -> Option<(u64, u64)> {
        let mut pending = self
            .pending_nacks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (first, last) = pending.pop_front()?;
        let send_last =
            last.min(first.saturating_add((MAX_NACK_PACKET_COUNT.saturating_sub(1)) as u64));
        if send_last < last {
            pending.push_front((send_last + 1, last));
        }
        Some((first, send_last))
    }
}

/// Shared handle to the NVST feedback plane (cheap to clone, shared across threads).
pub type SharedNvstFeedback = Arc<NvstFeedbackState>;

/// Legacy `nvstVideo` configuration normalized into the bounded receive transport.
///
/// Secret material is never exposed through `Debug`. The legacy `nvstVideo` handoff defaults to
/// `AEAD_AES_256_GCM`; every SRTP profile requires an explicit `srtpSaltHex`.
#[derive(Clone)]
pub struct NvstVideoConfig {
    client_udp_port: u16,
    video_peer: SocketAddr,
    bundle_peer: Option<SocketAddr>,
    srtp: NvstSrtpMaterial,
    ping_payload: Vec<u8>,
    ping_version: Option<u8>,
    stun_credentials: Option<NvstStunCredentials>,
    remote_dtls_fingerprint: Option<String>,
    /// The peer assigned RTCP feedback to the `rtcp1` SCTP data channel. When true, the
    /// dedicated Mjolnir socket must not send a second raw SRTCP Receiver Report.
    rtcp_on_sctp: bool,
    /// Dedicated NATT-only video (Mjolnir) socket port in the official two-socket
    /// cloud model. When set, video RTP/SRTP arrives on this socket while the
    /// ICE/DTLS bundle socket only carries control/audio keepalive traffic.
    mjolnir_udp_port: Option<u16>,
    codec: NvstVideoCodec,
    audio_track: Option<NvstAudioTrack>,
    expected_payload_type: Option<u8>,
    expected_ssrc: Option<u32>,
    reorder_window_packets: usize,
    max_access_unit_bytes: usize,
    timeout: Duration,
    frame_time_us: u32,
    /// Negotiated `x-nv-video[0].packetSize`; FEC shards are this plus 16 RTP bytes.
    video_packet_size: usize,
    /// Feedback plane shared with the ICE/DTLS bundle (cloned configs share it).
    feedback: SharedNvstFeedback,
}

impl fmt::Debug for NvstVideoConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NvstVideoConfig")
            .field("client_udp_port", &self.client_udp_port)
            .field("video_peer", &self.video_peer)
            .field("bundle_peer", &self.bundle_peer)
            .field("srtp", &self.srtp)
            .field("ping_payload_len", &self.ping_payload.len())
            .field("ping_version", &self.ping_version)
            .field("stun_credentials", &self.stun_credentials)
            .field(
                "remote_dtls_fingerprint_bytes",
                &self.remote_dtls_fingerprint.as_ref().map(String::len),
            )
            .field("rtcp_on_sctp", &self.rtcp_on_sctp)
            .field("mjolnir_udp_port", &self.mjolnir_udp_port)
            .field("codec", &self.codec)
            .field("audio_track", &self.audio_track)
            .field("expected_payload_type", &self.expected_payload_type)
            .field("expected_ssrc", &self.expected_ssrc)
            .field("reorder_window_packets", &self.reorder_window_packets)
            .field("max_access_unit_bytes", &self.max_access_unit_bytes)
            .field("timeout", &self.timeout)
            .field("frame_time_us", &self.frame_time_us)
            .field("video_packet_size", &self.video_packet_size)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvstAudioTrack {
    pub payload_type: u8,
    pub clock_rate_hz: u32,
    pub channels: u8,
    pub mid: String,
    pub ssrc: Option<u32>,
}

#[derive(Clone)]
struct NvstStunCredentials {
    local_username_fragment: String,
    local_password: String,
    remote_username_fragment: String,
    remote_password: String,
}

impl fmt::Debug for NvstStunCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NvstStunCredentials")
            .field("local_username_fragment", &"[redacted]")
            .field("local_password", &"[redacted]")
            .field("remote_username_fragment", &"[redacted]")
            .field("remote_password", &"[redacted]")
            .finish()
    }
}

#[derive(Clone)]
enum NvstSrtpMaterial {
    AeadAes128Gcm {
        master_key: [u8; 16],
        master_salt: [u8; 12],
        authentication_tag_len: usize,
    },
    AeadAes256Gcm {
        master_key: [u8; 32],
        master_salt: [u8; 12],
        authentication_tag_len: usize,
    },
    AesCm128HmacSha1 {
        master_key: [u8; 16],
        master_salt: [u8; 14],
        authentication_tag_len: usize,
    },
    AesCm256HmacSha1 {
        master_key: [u8; 32],
        master_salt: [u8; 14],
        authentication_tag_len: usize,
    },
}

impl fmt::Debug for NvstSrtpMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NvstSrtpMaterial")
            .field("profile", &self.profile())
            .field("master_key", &"[redacted]")
            .field("master_salt", &"[redacted]")
            .finish()
    }
}

impl NvstSrtpMaterial {
    fn profile(&self) -> NvstSrtpProfile {
        match self {
            Self::AeadAes128Gcm {
                authentication_tag_len: SRTP_AEAD_AES_GCM_8_TAG_LEN,
                ..
            } => NvstSrtpProfile::AeadAes128Gcm8,
            Self::AeadAes128Gcm { .. } => NvstSrtpProfile::AeadAes128Gcm,
            Self::AeadAes256Gcm {
                authentication_tag_len: SRTP_AEAD_AES_GCM_8_TAG_LEN,
                ..
            } => NvstSrtpProfile::AeadAes256Gcm8,
            Self::AeadAes256Gcm { .. } => NvstSrtpProfile::AeadAes256Gcm,
            Self::AesCm128HmacSha1 {
                authentication_tag_len: SRTP_AES_CM_HMAC_SHA1_80_TAG_LEN,
                ..
            } => NvstSrtpProfile::AesCm128HmacSha1_80,
            Self::AesCm128HmacSha1 { .. } => NvstSrtpProfile::AesCm128HmacSha1_32,
            Self::AesCm256HmacSha1 {
                authentication_tag_len: SRTP_AES_CM_HMAC_SHA1_80_TAG_LEN,
                ..
            } => NvstSrtpProfile::AesCm256HmacSha1_80,
            Self::AesCm256HmacSha1 { .. } => NvstSrtpProfile::AesCm256HmacSha1_32,
        }
    }
}

impl NvstVideoConfig {
    /// Parses the stable `nvstVideo` handoff supplied by the RTSP probe.
    pub fn from_legacy_handoff(
        handoff: &Value,
        settings_codec: Option<&str>,
    ) -> Result<Self, NvstConfigError> {
        let object = handoff
            .as_object()
            .ok_or(NvstConfigError::HandoffNotObject)?;
        let client_udp_port = required_u16(object, "clientUdpPort")?;
        if client_udp_port == 0 {
            return Err(NvstConfigError::OutOfRange {
                field: "clientUdpPort",
            });
        }

        let peer_ip_text = required_string(object, "videoPeerIp")?;
        let peer_ip: IpAddr = peer_ip_text
            .parse()
            .map_err(|_| NvstConfigError::InvalidPeerIp(peer_ip_text.to_owned()))?;
        if !is_unicast_peer(peer_ip) {
            return Err(NvstConfigError::InvalidPeerIp(peer_ip_text.to_owned()));
        }
        let video_peer_port = required_u16(object, "videoPeerPort")?;
        if video_peer_port == 0 {
            return Err(NvstConfigError::OutOfRange {
                field: "videoPeerPort",
            });
        }
        let bundle_peer =
            if object.contains_key("bundlePeerIp") || object.contains_key("bundlePeerPort") {
                let bundle_ip_text = required_string(object, "bundlePeerIp")?;
                let bundle_ip: IpAddr = bundle_ip_text
                    .parse()
                    .map_err(|_| NvstConfigError::InvalidPeerIp(bundle_ip_text.to_owned()))?;
                if !is_unicast_peer(bundle_ip) {
                    return Err(NvstConfigError::InvalidPeerIp(bundle_ip_text.to_owned()));
                }
                let bundle_port = required_u16(object, "bundlePeerPort")?;
                if bundle_port == 0 {
                    return Err(NvstConfigError::OutOfRange {
                        field: "bundlePeerPort",
                    });
                }
                Some(SocketAddr::new(bundle_ip, bundle_port))
            } else {
                None
            };

        let master_key = required_string(object, "srtpAesKeyHex")?;
        // Bifrost's SignalingHandler initializes Mjolnir video as
        // sec_serv_conf_and_auth + 256-bit keys → AES-256-GCM with an 8-byte tag.
        let srtp_profile = optional_string(object, "srtpProfile")?
            .map(NvstSrtpProfile::parse)
            .transpose()?
            .unwrap_or(NvstSrtpProfile::AeadAes256Gcm8);
        let srtp = match srtp_profile {
            NvstSrtpProfile::AeadAes128Gcm | NvstSrtpProfile::AeadAes128Gcm8 => {
                let master_salt = required_string(object, "srtpSaltHex").and_then(|salt| {
                    decode_fixed_hex::<12>(salt, NvstConfigError::InvalidSrtpSalt)
                })?;
                NvstSrtpMaterial::AeadAes128Gcm {
                    master_key: decode_fixed_hex::<16>(master_key, NvstConfigError::InvalidAesKey)?,
                    master_salt,
                    authentication_tag_len: match srtp_profile {
                        NvstSrtpProfile::AeadAes128Gcm8 => SRTP_AEAD_AES_GCM_8_TAG_LEN,
                        _ => SRTP_AEAD_AES_GCM_TAG_LEN,
                    },
                }
            }
            NvstSrtpProfile::AeadAes256Gcm | NvstSrtpProfile::AeadAes256Gcm8 => {
                let master_salt = required_string(object, "srtpSaltHex").and_then(|salt| {
                    decode_fixed_hex::<12>(salt, NvstConfigError::InvalidSrtpSalt)
                })?;
                NvstSrtpMaterial::AeadAes256Gcm {
                    master_key: decode_fixed_hex::<32>(master_key, NvstConfigError::InvalidAesKey)?,
                    master_salt,
                    authentication_tag_len: match srtp_profile {
                        NvstSrtpProfile::AeadAes256Gcm8 => SRTP_AEAD_AES_GCM_8_TAG_LEN,
                        _ => SRTP_AEAD_AES_GCM_TAG_LEN,
                    },
                }
            }
            NvstSrtpProfile::AesCm128HmacSha1_32 | NvstSrtpProfile::AesCm128HmacSha1_80 => {
                let master_salt = required_string(object, "srtpSaltHex").and_then(|salt| {
                    decode_salt_hex::<14>(salt, NvstConfigError::InvalidSrtpSalt)
                })?;
                NvstSrtpMaterial::AesCm128HmacSha1 {
                    master_key: decode_fixed_hex::<16>(master_key, NvstConfigError::InvalidAesKey)?,
                    master_salt,
                    authentication_tag_len: match srtp_profile {
                        NvstSrtpProfile::AesCm128HmacSha1_32 => 4,
                        NvstSrtpProfile::AesCm128HmacSha1_80 => SRTP_AES_CM_HMAC_SHA1_80_TAG_LEN,
                        _ => unreachable!("AES-CM profile selected above"),
                    },
                }
            }
            NvstSrtpProfile::AesCm256HmacSha1_32 | NvstSrtpProfile::AesCm256HmacSha1_80 => {
                let master_salt = required_string(object, "srtpSaltHex").and_then(|salt| {
                    decode_salt_hex::<14>(salt, NvstConfigError::InvalidSrtpSalt)
                })?;
                NvstSrtpMaterial::AesCm256HmacSha1 {
                    master_key: decode_fixed_hex::<32>(master_key, NvstConfigError::InvalidAesKey)?,
                    master_salt,
                    authentication_tag_len: match srtp_profile {
                        NvstSrtpProfile::AesCm256HmacSha1_32 => 4,
                        NvstSrtpProfile::AesCm256HmacSha1_80 => SRTP_AES_CM_HMAC_SHA1_80_TAG_LEN,
                        _ => unreachable!("AES-CM profile selected above"),
                    },
                }
            }
        };

        let codec_name = optional_string(object, "codec")?
            .or(settings_codec)
            .ok_or(NvstConfigError::MissingField("codec"))?;
        let codec = NvstVideoCodec::parse(codec_name)?;
        let audio_track = object
            .get("audioTrack")
            .map(parse_audio_track)
            .transpose()?;
        let ping_payload = optional_string(object, "pingPayload")?
            .map_or_else(|| b"PING".to_vec(), |payload| payload.as_bytes().to_vec());
        if ping_payload.is_empty() || ping_payload.len() > MAX_PING_BYTES {
            return Err(NvstConfigError::OutOfRange {
                field: "pingPayload",
            });
        }
        let ping_version = optional_u8(object, "pingVersion")?;
        let remote_dtls_fingerprint =
            optional_string(object, "remoteDtlsFingerprint")?.map(str::to_owned);
        let rtcp_on_sctp = match object.get("rtcpOnSctp") {
            Some(Value::Bool(value)) => *value,
            Some(_) => {
                return Err(NvstConfigError::InvalidFieldType {
                    field: "rtcpOnSctp",
                    expected: "boolean",
                });
            }
            None => false,
        };
        let mjolnir_udp_port = optional_u16(object, "mjolnirUdpPort")?;
        if mjolnir_udp_port == Some(0) {
            return Err(NvstConfigError::OutOfRange {
                field: "mjolnirUdpPort",
            });
        }
        let stun_credentials = if ping_version == Some(6) || remote_dtls_fingerprint.is_some() {
            Some(NvstStunCredentials {
                local_username_fragment: required_ice_credential(
                    object,
                    "localIceUsernameFragment",
                )?,
                local_password: required_ice_credential(object, "localIcePassword")?,
                remote_username_fragment: required_ice_credential(
                    object,
                    "remoteIceUsernameFragment",
                )?,
                remote_password: required_ice_credential(object, "remoteIcePassword")?,
            })
        } else {
            None
        };

        let expected_payload_type = optional_u8(object, "rtpPayloadType")?;
        let expected_ssrc = optional_u32(object, "rtpSsrc")?;
        let reorder_window_packets =
            optional_usize(object, "reorderWindowPackets")?.unwrap_or(DEFAULT_REORDER_WINDOW);
        if !(1..=MAX_REORDER_WINDOW).contains(&reorder_window_packets) {
            return Err(NvstConfigError::OutOfRange {
                field: "reorderWindowPackets",
            });
        }
        let max_access_unit_bytes =
            optional_usize(object, "maxAccessUnitBytes")?.unwrap_or(DEFAULT_MAX_ACCESS_UNIT_BYTES);
        if !(1..=MAX_ACCESS_UNIT_BYTES).contains(&max_access_unit_bytes) {
            return Err(NvstConfigError::OutOfRange {
                field: "maxAccessUnitBytes",
            });
        }
        let timeout_ms = optional_u64(object, "timeoutMs")?;
        let timeout = timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_TIMEOUT);
        if !(MIN_TIMEOUT..=MAX_TIMEOUT).contains(&timeout) {
            return Err(NvstConfigError::OutOfRange { field: "timeoutMs" });
        }
        let video_packet_size =
            optional_usize(object, "packetSize")?.unwrap_or(DEFAULT_NVST_VIDEO_PACKET_SIZE);
        if !(MIN_NVST_VIDEO_PACKET_SIZE..=MAX_NVST_VIDEO_PACKET_SIZE).contains(&video_packet_size) {
            return Err(NvstConfigError::OutOfRange {
                field: "packetSize",
            });
        }

        Ok(Self {
            client_udp_port,
            video_peer: SocketAddr::new(peer_ip, video_peer_port),
            bundle_peer,
            srtp,
            ping_payload,
            ping_version,
            stun_credentials,
            remote_dtls_fingerprint,
            rtcp_on_sctp,
            mjolnir_udp_port,
            codec,
            audio_track,
            expected_payload_type,
            expected_ssrc,
            reorder_window_packets,
            max_access_unit_bytes,
            timeout,
            frame_time_us: DEFAULT_FRAME_TIME_US,
            video_packet_size,
            feedback: Arc::new(NvstFeedbackState::default()),
        })
    }

    pub fn client_udp_port(&self) -> u16 {
        self.client_udp_port
    }

    /// Shared feedback plane (RTCP-over-SCTP state) for this session.
    pub fn feedback(&self) -> SharedNvstFeedback {
        self.feedback.clone()
    }

    pub fn video_peer(&self) -> SocketAddr {
        self.video_peer
    }

    pub fn bundle_peer(&self) -> SocketAddr {
        self.bundle_peer.unwrap_or(self.video_peer)
    }

    pub fn codec(&self) -> NvstVideoCodec {
        self.codec
    }

    pub fn audio_track(&self) -> Option<&NvstAudioTrack> {
        self.audio_track.as_ref()
    }

    pub fn srtp_profile(&self) -> NvstSrtpProfile {
        self.srtp.profile()
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn remote_dtls_fingerprint(&self) -> Option<&str> {
        self.remote_dtls_fingerprint.as_deref()
    }

    pub fn rtcp_on_sctp(&self) -> bool {
        self.rtcp_on_sctp
    }

    pub fn mjolnir_udp_port(&self) -> Option<u16> {
        self.mjolnir_udp_port
    }
}

fn parse_audio_track(value: &Value) -> Result<NvstAudioTrack, NvstConfigError> {
    let object = value
        .as_object()
        .ok_or(NvstConfigError::InvalidAudioTrack)?;
    let payload_type = object
        .get("payloadType")
        .and_then(Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
        .filter(|value| *value <= 127)
        .ok_or(NvstConfigError::InvalidAudioTrack)?;
    object
        .get("codec")
        .and_then(Value::as_str)
        .filter(|value| value.eq_ignore_ascii_case("opus"))
        .ok_or(NvstConfigError::InvalidAudioTrack)?;
    let clock_rate_hz = object
        .get("clockRateHz")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value == 48_000)
        .ok_or(NvstConfigError::InvalidAudioTrack)?;
    let channels = object
        .get("channels")
        .and_then(Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
        .filter(|value| matches!(*value, 1 | 2))
        .ok_or(NvstConfigError::InvalidAudioTrack)?;
    let mid = match object.get("mid") {
        Some(Value::String(value)) if !value.is_empty() && value.len() <= 64 => value.clone(),
        Some(_) => return Err(NvstConfigError::InvalidAudioTrack),
        None => "audio".to_owned(),
    };
    let ssrc = match object.get("ssrc") {
        Some(value) => Some(
            value
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .filter(|value| *value != 0)
                .ok_or(NvstConfigError::InvalidAudioTrack)?,
        ),
        None => None,
    };
    Ok(NvstAudioTrack {
        payload_type,
        clock_rate_hz,
        channels,
        mid,
        ssrc,
    })
}

fn required_ice_credential(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<String, NvstConfigError> {
    let value = required_string(object, field)?;
    if value.is_empty() || value.len() > MAX_ICE_CREDENTIAL_BYTES {
        return Err(NvstConfigError::OutOfRange { field });
    }
    Ok(value.to_owned())
}

/// Parses `context.nvstVideo` without tying the rest of the transport to JSON field names.
/// `None` means the legacy handoff was not supplied, which is a normal WebRTC fallback case.
pub fn parse_nvst_video_handoff(
    context: &Value,
) -> Result<Option<NvstVideoConfig>, NvstConfigError> {
    let Some(handoff) = context.get("nvstVideo") else {
        if context.get("nvstTransport").is_some() {
            return Err(NvstConfigError::MissingNvstVideoHandoff);
        }
        return Ok(None);
    };
    let settings_codec = context.pointer("/settings/codec").and_then(Value::as_str);
    let mut config = NvstVideoConfig::from_legacy_handoff(handoff, settings_codec)?;
    if let Some(fps) = context
        .pointer("/session/negotiatedStreamProfile/fps")
        .or_else(|| context.pointer("/settings/fps"))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .map(|value| value.clamp(1, 240))
    {
        config.frame_time_us = 1_000_000 / fps;
    }
    Ok(Some(config))
}

/// The transport selector always prefers a valid NVST video handoff, while making every
/// incomplete or unsupported handoff a typed WebRTC fallback instead of an optimistic start.
#[derive(Debug)]
pub enum PreferredVideoTransport {
    Nvst(Box<NvstVideoConfig>),
    WebRtcFallback(NvstFallbackReason),
}

#[derive(Debug)]
pub enum NvstFallbackReason {
    NoNvstHandoff,
    InvalidNvstHandoff(NvstConfigError),
}

pub fn select_preferred_video_transport(context: &Value) -> PreferredVideoTransport {
    match parse_nvst_video_handoff(context) {
        Ok(Some(config)) => PreferredVideoTransport::Nvst(Box::new(config)),
        Ok(None) => PreferredVideoTransport::WebRtcFallback(NvstFallbackReason::NoNvstHandoff),
        Err(error) => {
            PreferredVideoTransport::WebRtcFallback(NvstFallbackReason::InvalidNvstHandoff(error))
        }
    }
}

fn required_string<'a>(
    object: &'a serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<&'a str, NvstConfigError> {
    let value = object
        .get(field)
        .ok_or(NvstConfigError::MissingField(field))?;
    value.as_str().ok_or(NvstConfigError::InvalidFieldType {
        field,
        expected: "string",
    })
}

fn optional_string<'a>(
    object: &'a serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<Option<&'a str>, NvstConfigError> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(Some)
            .ok_or(NvstConfigError::InvalidFieldType {
                field,
                expected: "string",
            }),
    }
}

fn required_u16(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<u16, NvstConfigError> {
    let value = required_u64(object, field)?;
    u16::try_from(value).map_err(|_| NvstConfigError::OutOfRange { field })
}

fn required_u64(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<u64, NvstConfigError> {
    let value = object
        .get(field)
        .ok_or(NvstConfigError::MissingField(field))?;
    value.as_u64().ok_or(NvstConfigError::InvalidFieldType {
        field,
        expected: "unsigned integer",
    })
}

fn optional_u64(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<Option<u64>, NvstConfigError> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or(NvstConfigError::InvalidFieldType {
                field,
                expected: "unsigned integer",
            }),
    }
}

fn optional_u16(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<Option<u16>, NvstConfigError> {
    optional_u64(object, field)?.map_or(Ok(None), |value| {
        u16::try_from(value)
            .map(Some)
            .map_err(|_| NvstConfigError::OutOfRange { field })
    })
}

fn optional_u8(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<Option<u8>, NvstConfigError> {
    optional_u64(object, field)?.map_or(Ok(None), |value| {
        u8::try_from(value)
            .map(Some)
            .map_err(|_| NvstConfigError::OutOfRange { field })
    })
}

fn optional_u32(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<Option<u32>, NvstConfigError> {
    optional_u64(object, field)?.map_or(Ok(None), |value| {
        u32::try_from(value)
            .map(Some)
            .map_err(|_| NvstConfigError::OutOfRange { field })
    })
}

fn optional_usize(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<Option<usize>, NvstConfigError> {
    optional_u64(object, field)?.map_or(Ok(None), |value| {
        usize::try_from(value)
            .map(Some)
            .map_err(|_| NvstConfigError::OutOfRange { field })
    })
}

fn decode_fixed_hex<const N: usize>(
    value: &str,
    error: NvstConfigError,
) -> Result<[u8; N], NvstConfigError> {
    if value.len() != N * 2 {
        return Err(error);
    }
    let mut decoded = [0_u8; N];
    for (index, output) in decoded.iter_mut().enumerate() {
        let offset = index * 2;
        let pair = match value.get(offset..offset + 2) {
            Some(pair) => pair,
            None => return Err(error),
        };
        *output = match u8::from_str_radix(pair, 16) {
            Ok(byte) => byte,
            Err(_) => return Err(error),
        };
    }
    Ok(decoded)
}

/// GFN packs the keyId into the low 4 bytes of a 12-byte salt; libsrtp right-pads the
/// AES-CM master salt to 14 bytes. Accept the 12-byte probe form and right-pad with zeros.
fn decode_salt_hex<const N: usize>(
    value: &str,
    error: NvstConfigError,
) -> Result<[u8; N], NvstConfigError> {
    if value.len() % 2 != 0 || value.len() > N * 2 {
        return Err(error);
    }
    let mut decoded = [0_u8; N];
    for (index, output) in decoded.iter_mut().take(value.len() / 2).enumerate() {
        let offset = index * 2;
        let pair = match value.get(offset..offset + 2) {
            Some(pair) => pair,
            None => return Err(error),
        };
        *output = match u8::from_str_radix(pair, 16) {
            Ok(byte) => byte,
            Err(_) => return Err(error),
        };
    }
    Ok(decoded)
}

fn is_unicast_peer(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => !ip.is_unspecified() && !ip.is_multicast() && ip != Ipv4Addr::BROADCAST,
        IpAddr::V6(ip) => !ip.is_unspecified() && !ip.is_multicast() && ip != Ipv6Addr::UNSPECIFIED,
    }
}

/// A received encoded video access unit ready for a decoder queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedVideoAccessUnit {
    pub codec: NvstVideoCodec,
    pub timestamp: u32,
    pub frame_index: u32,
    pub first_stream_packet_index: u32,
    pub keyframe: bool,
    pub contiguous: bool,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvstReceiverState {
    Running,
    Paused,
    RecoveryRequired,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NvstDropReason {
    UnexpectedSource {
        expected: SocketAddr,
        actual: SocketAddr,
    },
    Paused,
    Stopped,
    RecoveryRequired,
    MalformedRtp(RtpParseError),
    AuthenticationFailed,
    ReplayRejected,
    UnexpectedPayloadType {
        expected: u8,
        actual: u8,
    },
    UnexpectedSsrc {
        expected: u32,
        actual: u32,
    },
    StaleRtpPacket {
        index: u64,
    },
    DuplicateRtpPacket {
        index: u64,
    },
    Unsupported(NvstUnsupportedFeature),
    AwaitingStartOfFrame,
    FrameDiscontinuity,
    MissingAnnexBStartCode,
    InvalidLastPacketPayloadLength {
        reported: usize,
        frame_header_size: usize,
        available: usize,
    },
    AccessUnitTooLarge {
        limit: usize,
    },
    MalformedRedAudio,
    MediaConsumerBackpressured,
    MediaConsumerClosed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RtpParseError {
    TooShort,
    InvalidVersion,
    InvalidCsrcLength,
    InvalidExtensionLength,
    MissingAuthenticationTag,
    InvalidPadding,
    MissingNvVideoHeader,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NvstRecovery {
    PacketGap {
        first_missing_index: u64,
        last_missing_index: u64,
    },
    Timeout {
        idle_for: Duration,
    },
}

/// All receive decisions are explicit so callers can collect operational metrics without
/// treating malformed network traffic as a fatal thread error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NvstReceiveEvent {
    Frame(EncodedVideoAccessUnit),
    TransportReady(&'static str),
    InputReady(u16),
    InputUnavailable(String),
    Cursor(Vec<u8>),
    Dropped(NvstDropReason),
    RecoveryNeeded(NvstRecovery),
    Lifecycle(NvstReceiverState),
}

#[derive(Debug, Clone, Copy)]
struct RtpHeader {
    payload_type: u8,
    sequence_number: u16,
    timestamp: u32,
    ssrc: u32,
    payload_offset: usize,
    has_padding: bool,
    gs_video_header: Option<[u8; NV_VIDEO_PACKET_LEN]>,
}

impl RtpHeader {
    fn parse(packet: &[u8]) -> Result<Self, RtpParseError> {
        if packet.len() < RTP_FIXED_HEADER_LEN {
            return Err(RtpParseError::TooShort);
        }
        let first = packet[0];
        if first >> 6 != 2 {
            return Err(RtpParseError::InvalidVersion);
        }
        let csrc_count = usize::from(first & 0x0f);
        let mut payload_offset = RTP_FIXED_HEADER_LEN
            .checked_add(csrc_count.saturating_mul(4))
            .ok_or(RtpParseError::InvalidCsrcLength)?;
        if packet.len() < payload_offset {
            return Err(RtpParseError::InvalidCsrcLength);
        }
        let mut gs_video_header = None;
        if first & 0x10 != 0 {
            if packet.len() < payload_offset + 4 {
                return Err(RtpParseError::InvalidExtensionLength);
            }
            let profile = u16::from_be_bytes([packet[payload_offset], packet[payload_offset + 1]]);
            let words =
                u16::from_be_bytes([packet[payload_offset + 2], packet[payload_offset + 3]]);
            let extension_len = usize::from(words)
                .checked_mul(4)
                .and_then(|length| length.checked_add(4))
                .ok_or(RtpParseError::InvalidExtensionLength)?;
            let extension_end = payload_offset
                .checked_add(extension_len)
                .ok_or(RtpParseError::InvalidExtensionLength)?;
            if packet.len() < extension_end {
                return Err(RtpParseError::InvalidExtensionLength);
            }
            if profile == GS_VIDEO_EXTENSION_PROFILE {
                if usize::from(words) * 4 != NV_VIDEO_PACKET_LEN {
                    return Err(RtpParseError::InvalidExtensionLength);
                }
                let body = &packet[payload_offset + 4..extension_end];
                gs_video_header = Some(
                    body[..NV_VIDEO_PACKET_LEN]
                        .try_into()
                        .expect("length checked"),
                );
            }
            payload_offset = extension_end;
        }
        Ok(Self {
            payload_type: packet[1] & 0x7f,
            sequence_number: u16::from_be_bytes([packet[2], packet[3]]),
            timestamp: u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]),
            ssrc: u32::from_be_bytes([packet[8], packet[9], packet[10], packet[11]]),
            payload_offset,
            has_padding: first & 0x20 != 0,
            gs_video_header,
        })
    }

    fn payload<'a>(&self, packet: &'a [u8]) -> Result<&'a [u8], RtpParseError> {
        if packet.len() < self.payload_offset {
            return Err(RtpParseError::TooShort);
        }
        let mut end = packet.len();
        if self.has_padding {
            let padding = usize::from(*packet.last().ok_or(RtpParseError::InvalidPadding)?);
            if padding == 0 || padding > end.saturating_sub(self.payload_offset) {
                return Err(RtpParseError::InvalidPadding);
            }
            end -= padding;
        }
        Ok(&packet[self.payload_offset..end])
    }
}

#[derive(Debug, Clone)]
struct RtpPacket {
    index: u64,
    header: RtpHeader,
    plaintext: Vec<u8>,
}

#[derive(Clone)]
struct SrtpReceiver {
    cipher: SrtpCipher,
    replay: ReplayWindow,
}

#[derive(Clone)]
enum SrtpCipher {
    AeadAes128Gcm {
        encryption_key: [u8; 16],
        session_salt: [u8; 12],
        authentication_tag_len: usize,
    },
    AeadAes256Gcm {
        encryption_key: [u8; 32],
        session_salt: [u8; 12],
        authentication_tag_len: usize,
    },
    AesCm128HmacSha1 {
        encryption_key: [u8; 16],
        authentication_key: [u8; 20],
        session_salt: [u8; 14],
        authentication_tag_len: usize,
    },
    AesCm256HmacSha1 {
        encryption_key: [u8; 32],
        authentication_key: [u8; 20],
        session_salt: [u8; 14],
        authentication_tag_len: usize,
    },
}

impl SrtpReceiver {
    fn from_material(material: &NvstSrtpMaterial) -> Self {
        let cipher = match material {
            NvstSrtpMaterial::AeadAes128Gcm {
                master_key,
                master_salt,
                authentication_tag_len,
            } => SrtpCipher::AeadAes128Gcm {
                encryption_key: derive_aes128_cm_key::<16>(
                    master_key,
                    master_salt,
                    GFN_SRTP_KEY_LABEL,
                ),
                session_salt: derive_aes128_cm_key::<12>(
                    master_key,
                    master_salt,
                    GFN_SRTP_SALT_LABEL,
                ),
                authentication_tag_len: *authentication_tag_len,
            },
            NvstSrtpMaterial::AeadAes256Gcm {
                master_key,
                master_salt,
                authentication_tag_len,
            } => SrtpCipher::AeadAes256Gcm {
                encryption_key: derive_aes_cm_key::<32>(
                    master_key,
                    master_salt,
                    GFN_SRTP_KEY_LABEL,
                ),
                session_salt: derive_aes_cm_key::<12>(master_key, master_salt, GFN_SRTP_SALT_LABEL),
                authentication_tag_len: *authentication_tag_len,
            },
            NvstSrtpMaterial::AesCm128HmacSha1 {
                master_key,
                master_salt,
                authentication_tag_len,
            } => SrtpCipher::AesCm128HmacSha1 {
                encryption_key: derive_aes128_cm_key::<16>(master_key, master_salt, 0x00),
                authentication_key: derive_aes128_cm_key::<20>(master_key, master_salt, 0x01),
                session_salt: derive_aes128_cm_key::<14>(master_key, master_salt, 0x02),
                authentication_tag_len: *authentication_tag_len,
            },
            NvstSrtpMaterial::AesCm256HmacSha1 {
                master_key,
                master_salt,
                authentication_tag_len,
            } => SrtpCipher::AesCm256HmacSha1 {
                encryption_key: derive_aes_cm_key::<32>(master_key, master_salt, 0x00),
                authentication_key: derive_aes_cm_key::<20>(master_key, master_salt, 0x01),
                session_salt: derive_aes_cm_key::<14>(master_key, master_salt, 0x02),
                authentication_tag_len: *authentication_tag_len,
            },
        };
        Self {
            cipher,
            replay: ReplayWindow::default(),
        }
    }

    fn unprotect(&mut self, datagram: &[u8]) -> Result<RtpPacket, NvstDropReason> {
        let header = RtpHeader::parse(datagram).map_err(NvstDropReason::MalformedRtp)?;
        let packet_index = self.replay.guess_packet_index(header.sequence_number)?;
        let roc = u32::try_from(packet_index >> 16).map_err(|_| NvstDropReason::ReplayRejected)?;
        let plaintext = match &self.cipher {
            SrtpCipher::AeadAes128Gcm {
                encryption_key,
                session_salt,
                authentication_tag_len,
            } => unprotect_aes_gcm(
                datagram,
                header,
                roc,
                encryption_key,
                session_salt,
                *authentication_tag_len,
            )?,
            SrtpCipher::AeadAes256Gcm {
                encryption_key,
                session_salt,
                authentication_tag_len,
            } => unprotect_aes_gcm(
                datagram,
                header,
                roc,
                encryption_key,
                session_salt,
                *authentication_tag_len,
            )?,
            SrtpCipher::AesCm128HmacSha1 {
                encryption_key,
                authentication_key,
                session_salt,
                authentication_tag_len,
            } => unprotect_aes_cm_hmac_sha1(
                datagram,
                header,
                packet_index,
                roc,
                encryption_key,
                authentication_key,
                session_salt,
                *authentication_tag_len,
            )?,
            SrtpCipher::AesCm256HmacSha1 {
                encryption_key,
                authentication_key,
                session_salt,
                authentication_tag_len,
            } => unprotect_aes_cm_hmac_sha1(
                datagram,
                header,
                packet_index,
                roc,
                encryption_key,
                authentication_key,
                session_salt,
                *authentication_tag_len,
            )?,
        };
        self.replay.check(packet_index)?;
        header
            .payload(&plaintext)
            .map_err(NvstDropReason::MalformedRtp)?;
        self.replay.commit(packet_index);
        Ok(RtpPacket {
            index: packet_index,
            header,
            plaintext,
        })
    }
}

/// AES-GCM with a truncated tag. `aes-gcm` 0.10 only seals 12–16 byte tags;
/// NVIDIA's Mjolnir path uses the 8-byte `aes_gcm_*_8_auth` libsrtp policy.
fn unprotect_aes_gcm(
    datagram: &[u8],
    header: RtpHeader,
    roc: u32,
    encryption_key: &[u8],
    session_salt: &[u8; 12],
    tag_len: usize,
) -> Result<Vec<u8>, NvstDropReason> {
    let ciphertext_end =
        datagram
            .len()
            .checked_sub(tag_len)
            .ok_or(NvstDropReason::MalformedRtp(
                RtpParseError::MissingAuthenticationTag,
            ))?;
    if ciphertext_end < header.payload_offset {
        return Err(NvstDropReason::MalformedRtp(
            RtpParseError::MissingAuthenticationTag,
        ));
    }
    let iv = srtp_gcm_iv(*session_salt, header.ssrc, roc, header.sequence_number);
    let aad = &datagram[..header.payload_offset];
    let ciphertext = &datagram[header.payload_offset..ciphertext_end];
    let received_tag = &datagram[ciphertext_end..];
    let (expected_tag, mut ctr) = aes_gcm_tag_and_ctr(encryption_key, &iv, aad, ciphertext);
    if expected_tag[..tag_len].ct_eq(received_tag).unwrap_u8() != 1 {
        return Err(NvstDropReason::AuthenticationFailed);
    }
    let mut plaintext = datagram[..ciphertext_end].to_vec();
    ctr.apply_keystream(&mut plaintext[header.payload_offset..]);
    Ok(plaintext)
}

enum GcmCtr {
    Aes128(Box<Ctr32BE<Aes128>>),
    Aes256(Box<Ctr32BE<Aes256>>),
}

impl GcmCtr {
    fn apply_keystream(&mut self, buffer: &mut [u8]) {
        match self {
            Self::Aes128(cipher) => cipher.apply_keystream(buffer),
            Self::Aes256(cipher) => cipher.apply_keystream(buffer),
        }
    }
}

fn aes_gcm_tag_and_ctr(
    encryption_key: &[u8],
    iv: &[u8; 12],
    aad: &[u8],
    ciphertext: &[u8],
) -> ([u8; 16], GcmCtr) {
    let mut j0 = [0_u8; 16];
    j0[..12].copy_from_slice(iv);
    j0[15] = 1;
    let mut hash_key = [0_u8; 16];
    let mut tag_mask = [0_u8; 16];
    let ctr = match encryption_key.len() {
        16 => {
            let key: &[u8; 16] = encryption_key.try_into().expect("16-byte GCM key");
            let aes = <Aes128 as aes::cipher::KeyInit>::new(key.into());
            aes.encrypt_block((&mut hash_key).into());
            let mut ctr = Ctr32BE::<Aes128>::new(key.into(), (&j0).into());
            ctr.apply_keystream(&mut tag_mask);
            GcmCtr::Aes128(Box::new(ctr))
        }
        32 => {
            let key: &[u8; 32] = encryption_key.try_into().expect("32-byte GCM key");
            let aes = <Aes256 as aes::cipher::KeyInit>::new(key.into());
            aes.encrypt_block((&mut hash_key).into());
            let mut ctr = Ctr32BE::<Aes256>::new(key.into(), (&j0).into());
            ctr.apply_keystream(&mut tag_mask);
            GcmCtr::Aes256(Box::new(ctr))
        }
        _ => unreachable!("AES-GCM key is 16 or 32 bytes"),
    };
    let mut hasher = <GHash as ghash::universal_hash::KeyInit>::new((&hash_key).into());
    hasher.update_padded(aad);
    hasher.update_padded(ciphertext);
    let mut len_block = ghash::Block::default();
    len_block[..8].copy_from_slice(&((aad.len() as u64) * 8).to_be_bytes());
    len_block[8..].copy_from_slice(&((ciphertext.len() as u64) * 8).to_be_bytes());
    hasher.update(&[len_block]);
    let mut tag = hasher.finalize();
    for (byte, mask) in tag.iter_mut().zip(tag_mask) {
        *byte ^= mask;
    }
    let mut expected = [0_u8; 16];
    expected.copy_from_slice(&tag);
    (expected, ctr)
}

#[cfg(test)]
fn protect_aes_gcm(
    packet: &mut Vec<u8>,
    payload_offset: usize,
    encryption_key: &[u8],
    iv: &[u8; 12],
    tag_len: usize,
) {
    let aad_owned = packet[..payload_offset].to_vec();
    let (_, mut ctr) = aes_gcm_tag_and_ctr(encryption_key, iv, &aad_owned, &[]);
    ctr.apply_keystream(&mut packet[payload_offset..]);
    let (tag, _) = aes_gcm_tag_and_ctr(encryption_key, iv, &aad_owned, &packet[payload_offset..]);
    packet.extend_from_slice(&tag[..tag_len]);
}

#[allow(clippy::too_many_arguments)]
fn unprotect_aes_cm_hmac_sha1(
    datagram: &[u8],
    header: RtpHeader,
    packet_index: u64,
    roc: u32,
    encryption_key: &[u8],
    authentication_key: &[u8; 20],
    session_salt: &[u8; 14],
    authentication_tag_len: usize,
) -> Result<Vec<u8>, NvstDropReason> {
    let authenticated_len =
        datagram
            .len()
            .checked_sub(authentication_tag_len)
            .ok_or(NvstDropReason::MalformedRtp(
                RtpParseError::MissingAuthenticationTag,
            ))?;
    if authenticated_len < header.payload_offset {
        return Err(NvstDropReason::MalformedRtp(
            RtpParseError::MissingAuthenticationTag,
        ));
    }
    let mut mac =
        HmacSha1::new_from_slice(authentication_key).expect("HMAC-SHA1 accepts a fixed-size key");
    mac.update(&datagram[..authenticated_len]);
    mac.update(&roc.to_be_bytes());
    let expected_tag = mac.finalize().into_bytes();
    let received_tag = &datagram[authenticated_len..];
    if expected_tag[..authentication_tag_len]
        .ct_eq(received_tag)
        .unwrap_u8()
        != 1
    {
        return Err(NvstDropReason::AuthenticationFailed);
    }
    let mut plaintext = datagram[..authenticated_len].to_vec();
    let iv = srtp_aes_cm_iv(session_salt, header.ssrc, packet_index);
    match encryption_key.len() {
        16 => {
            let key: &[u8; 16] = encryption_key.try_into().expect("16-byte AES-CM key");
            let mut cipher = Aes128Ctr::new(key.into(), (&iv).into());
            cipher.apply_keystream(&mut plaintext[header.payload_offset..]);
        }
        32 => {
            let key: &[u8; 32] = encryption_key.try_into().expect("32-byte AES-CM key");
            let mut cipher = Aes256Ctr::new(key.into(), (&iv).into());
            cipher.apply_keystream(&mut plaintext[header.payload_offset..]);
        }
        _ => unreachable!("AES-CM key is 16 or 32 bytes"),
    }
    Ok(plaintext)
}

fn derive_aes_cm_key<const N: usize>(
    master_key: &[u8; 32],
    master_salt: &[u8],
    label: u8,
) -> [u8; N] {
    let mut iv = [0_u8; 16];
    iv[..master_salt.len()].copy_from_slice(master_salt);
    // RFC 3711 key_id = label * 2^48, then x = master_salt * 2^16 XOR key_id * 2^16.
    iv[7] ^= label;
    let mut output = [0_u8; N];
    let mut cipher = Aes256Ctr::new(master_key.into(), (&iv).into());
    cipher.apply_keystream(&mut output);
    output
}

fn derive_aes128_cm_key<const N: usize>(
    master_key: &[u8; 16],
    master_salt: &[u8],
    label: u8,
) -> [u8; N] {
    let mut iv = [0_u8; 16];
    iv[..master_salt.len()].copy_from_slice(master_salt);
    iv[7] ^= label;
    let mut output = [0_u8; N];
    let mut cipher = Aes128Ctr::new(master_key.into(), (&iv).into());
    cipher.apply_keystream(&mut output);
    output
}

fn srtp_aes_cm_iv(session_salt: &[u8; 14], ssrc: u32, packet_index: u64) -> [u8; 16] {
    let mut iv = [0_u8; 16];
    iv[..14].copy_from_slice(session_salt);
    for (target, source) in iv[4..8].iter_mut().zip(ssrc.to_be_bytes()) {
        *target ^= source;
    }
    let index_bytes = packet_index.to_be_bytes();
    for (target, source) in iv[8..14].iter_mut().zip(&index_bytes[2..]) {
        *target ^= source;
    }
    iv
}

fn srtp_gcm_iv(session_salt: [u8; 12], ssrc: u32, roc: u32, sequence_number: u16) -> [u8; 12] {
    let mut iv = session_salt;
    for (target, source) in iv[2..6].iter_mut().zip(ssrc.to_be_bytes()) {
        *target ^= source;
    }
    for (target, source) in iv[6..10].iter_mut().zip(roc.to_be_bytes()) {
        *target ^= source;
    }
    for (target, source) in iv[10..].iter_mut().zip(sequence_number.to_be_bytes()) {
        *target ^= source;
    }
    iv
}

/// RFC 7714 §9.2 SRTCP GCM IV: salt XOR (SSRC at bytes 2..6, SRTCP index at 6..10).
fn srtcp_gcm_iv(session_salt: [u8; 12], ssrc: u32, srtcp_index: u32) -> [u8; 12] {
    let mut iv = session_salt;
    for (target, source) in iv[2..6].iter_mut().zip(ssrc.to_be_bytes()) {
        *target ^= source;
    }
    for (target, source) in iv[6..10].iter_mut().zip(srtcp_index.to_be_bytes()) {
        *target ^= source;
    }
    iv
}

/// Builds and SRTCP-protects an RTCP Receiver Report carrying one report block.
/// The raw Mjolnir transport keeps the fixed eight-byte ONOW receiver header clear,
/// sets the report payload E/S bit before encryption, and appends the eight-byte GCM
/// tag before E|index. AAD is the clear header followed by the first eight ciphertext
/// bytes; the sender SSRC in that clear header selects the nonce context.
#[allow(clippy::too_many_arguments)]
fn protect_srtcp_receiver_report_gcm(
    report: RtcpReportBlock,
    srtcp_index: u32,
    encryption_key: &[u8],
    session_salt: &[u8; 12],
    tag_len: usize,
) -> Vec<u8> {
    let mut packet = Vec::with_capacity(32 + 4 + tag_len);
    packet.push(0x81); // V=2, P=0, RC=1 (one report block)
    packet.push(201); // PT=RR
    packet.extend_from_slice(&7_u16.to_be_bytes()); // length: 8 words - 1
    packet.extend_from_slice(&RTCP_SENDER_SSRC.to_be_bytes());
    packet.extend_from_slice(&report.media_ssrc.to_be_bytes());
    packet.push(report.fraction_lost);
    packet.extend_from_slice(&(report.cumulative_lost as u32).to_be_bytes()[1..]);
    packet.extend_from_slice(&report.highest_sequence.to_be_bytes());
    packet.extend_from_slice(&report.jitter.to_be_bytes());
    packet.extend_from_slice(&0_u32.to_be_bytes()); // LSR
    packet.extend_from_slice(&0_u32.to_be_bytes()); // DLSR

    let e_index = SRTCP_ENCRYPTED_FLAG | (srtcp_index & !SRTCP_ENCRYPTED_FLAG);
    let iv = srtcp_gcm_iv(*session_salt, RTCP_SENDER_SSRC, srtcp_index);
    packet[8] |= 0x80;
    let (_, mut ctr) = aes_gcm_tag_and_ctr(encryption_key, &iv, &[], &[]);
    ctr.apply_keystream(&mut packet[8..]);
    let mut aad = packet[..8].to_vec();
    aad.extend_from_slice(&packet[8..16]);
    let (tag, _) = aes_gcm_tag_and_ctr(encryption_key, &iv, &aad, &packet[8..]);
    packet.extend_from_slice(&tag[..tag_len]);
    packet.extend_from_slice(&e_index.to_be_bytes());
    packet
}

/// Builds a plain (unencrypted) RTCP Receiver Report (RFC 3550 §6.4.1) with one
/// report block for `media_ssrc`. Sent over the `rtcp1` SCTP data channel, which
/// is already encrypted by DTLS, so no SRTCP layer is applied.
fn build_rtcp_receiver_report(sender_ssrc: u32, report: RtcpReportBlock) -> Vec<u8> {
    let mut packet = Vec::with_capacity(32);
    packet.push(0x81); // V=2, P=0, RC=1 (one report block)
    packet.push(201); // PT=RR
    packet.extend_from_slice(&7_u16.to_be_bytes()); // length: 8 words - 1
    packet.extend_from_slice(&sender_ssrc.to_be_bytes());
    packet.extend_from_slice(&report.media_ssrc.to_be_bytes());
    packet.push(report.fraction_lost);
    packet.extend_from_slice(&(report.cumulative_lost as u32).to_be_bytes()[1..]);
    packet.extend_from_slice(&report.highest_sequence.to_be_bytes());
    packet.extend_from_slice(&report.jitter.to_be_bytes());
    packet.extend_from_slice(&0_u32.to_be_bytes()); // LSR
    packet.extend_from_slice(&0_u32.to_be_bytes()); // DLSR
    packet
}

/// Builds a plain RTCP Picture Loss Indication (RFC 4585 §6.3.1) asking the
/// sender of `media_ssrc` for a fresh keyframe.
fn build_rtcp_pli(sender_ssrc: u32, media_ssrc: u32) -> Vec<u8> {
    let mut packet = Vec::with_capacity(12);
    packet.push(0x81); // V=2, P=0, FMT=1 (PLI)
    packet.push(206); // PT=PSFB (payload-specific feedback)
    packet.extend_from_slice(&2_u16.to_be_bytes()); // length: 3 words - 1
    packet.extend_from_slice(&sender_ssrc.to_be_bytes());
    packet.extend_from_slice(&media_ssrc.to_be_bytes());
    packet
}

/// Builds an RFC 4585 §6.2.1 generic NACK for a continuous range of missing
/// extended RTP sequence numbers. Each FCI entry represents its PID plus up to
/// 16 following packets in the bitmask; the packet is bounded to avoid turning
/// a malicious sequence jump into an unbounded control message.
fn build_rtcp_nack(
    sender_ssrc: u32,
    media_ssrc: u32,
    first_missing_index: u64,
    last_missing_index: u64,
) -> Vec<u8> {
    let mut entries = Vec::with_capacity(MAX_NACK_FCI_ENTRIES);
    let mut current = first_missing_index;
    let effective_last = last_missing_index
        .min(first_missing_index.saturating_add((MAX_NACK_PACKET_COUNT.saturating_sub(1)) as u64));
    while current <= effective_last && entries.len() < MAX_NACK_FCI_ENTRIES {
        let pid = current as u16;
        let represented_following = (effective_last - current).min(16) as u16;
        let blp = if represented_following == 16 {
            u16::MAX
        } else {
            ((1_u32 << represented_following) - 1) as u16
        };
        entries.push((pid, blp));
        current = current.saturating_add(17);
    }

    let mut packet = Vec::with_capacity(12 + entries.len() * 4);
    packet.push(0x81); // V=2, P=0, FMT=1 (generic NACK)
    packet.push(205); // PT=RTPFB (transport-layer feedback)
    packet.extend_from_slice(&(2_u16 + entries.len() as u16).to_be_bytes());
    packet.extend_from_slice(&sender_ssrc.to_be_bytes());
    packet.extend_from_slice(&media_ssrc.to_be_bytes());
    for (pid, blp) in entries {
        packet.extend_from_slice(&pid.to_be_bytes());
        packet.extend_from_slice(&blp.to_be_bytes());
    }
    packet
}

#[derive(Clone)]
struct ReplayWindow {
    highest_index: Option<u64>,
    // Bit N records `highest_index - N`. Words are little-endian by packet age.
    seen: [u64; SRTP_REPLAY_WINDOW_WORDS],
}

impl Default for ReplayWindow {
    fn default() -> Self {
        Self {
            highest_index: None,
            seen: [0; SRTP_REPLAY_WINDOW_WORDS],
        }
    }
}

impl ReplayWindow {
    fn guess_packet_index(&self, sequence_number: u16) -> Result<u64, NvstDropReason> {
        let Some(highest_index) = self.highest_index else {
            return Ok(u64::from(sequence_number));
        };
        let roc = highest_index >> 16;
        let highest_sequence = (highest_index & 0xffff) as u16;
        let delta = i32::from(sequence_number) - i32::from(highest_sequence);
        let guessed_roc = if delta < -32_768 {
            roc.checked_add(1).ok_or(NvstDropReason::ReplayRejected)?
        } else if delta > 32_768 {
            roc.checked_sub(1).ok_or(NvstDropReason::ReplayRejected)?
        } else {
            roc
        };
        Ok((guessed_roc << 16) | u64::from(sequence_number))
    }

    fn check(&self, index: u64) -> Result<(), NvstDropReason> {
        let Some(highest_index) = self.highest_index else {
            return Ok(());
        };
        if index > highest_index {
            return Ok(());
        }
        let age = highest_index - index;
        let Ok(age) = usize::try_from(age) else {
            return Err(NvstDropReason::ReplayRejected);
        };
        if age >= SRTP_REPLAY_WINDOW_PACKETS
            || self.seen[age / u64::BITS as usize] & (1_u64 << (age % u64::BITS as usize)) != 0
        {
            return Err(NvstDropReason::ReplayRejected);
        }
        Ok(())
    }

    fn commit(&mut self, index: u64) {
        match self.highest_index {
            None => {
                self.highest_index = Some(index);
                self.seen.fill(0);
                self.seen[0] = 1;
            }
            Some(highest_index) if index > highest_index => {
                let advance = usize::try_from(index - highest_index).unwrap_or(usize::MAX);
                if advance >= SRTP_REPLAY_WINDOW_PACKETS {
                    self.seen.fill(0);
                } else {
                    let word_shift = advance / u64::BITS as usize;
                    let bit_shift = advance % u64::BITS as usize;
                    for destination in (0..SRTP_REPLAY_WINDOW_WORDS).rev() {
                        self.seen[destination] = if destination < word_shift {
                            0
                        } else {
                            let source = destination - word_shift;
                            let mut shifted = self.seen[source] << bit_shift;
                            if bit_shift != 0 && source > 0 {
                                shifted |=
                                    self.seen[source - 1] >> (u64::BITS as usize - bit_shift);
                            }
                            shifted
                        };
                    }
                }
                self.seen[0] |= 1;
                self.highest_index = Some(index);
            }
            Some(highest_index) => {
                let age = usize::try_from(highest_index - index)
                    .expect("replay age was validated before commit");
                self.seen[age / u64::BITS as usize] |= 1_u64 << (age % u64::BITS as usize);
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct NvVideoPacket {
    stream_packet_index: u32,
    frame_index: u32,
    flags: u8,
    fec_current_block: u8,
    fec_last_block: u8,
    is_fec: bool,
}

impl NvVideoPacket {
    /// Reads the Mjolnir video metadata from the `0x4753` ("GS") RTP extension:
    /// a 16-byte little-endian block holding the stream packet index, frame index,
    /// the packet-type nibble (picture data / SOF / EOF), and FEC group
    /// coordinates. The RTP payload starts with the GameStream frame header on an
    /// SOF packet and otherwise continues the H.264 access-unit data directly.
    fn parse<'a>(header: &RtpHeader, payload: &'a [u8]) -> Result<(Self, &'a [u8]), RtpParseError> {
        let Some(extension) = header.gs_video_header else {
            return Err(RtpParseError::MissingNvVideoHeader);
        };
        let packet_word = u32::from_le_bytes(extension[0..4].try_into().expect("length checked"));
        let flags_word = u32::from_le_bytes(extension[8..12].try_into().expect("length checked"));
        let fec_word = u32::from_le_bytes(extension[12..16].try_into().expect("length checked"));
        let multi_fec_blocks = extension[11];
        let fec_percentage = (fec_word >> 4) & 0xff;
        let fec_index = (fec_word >> 12) & 0x3ff;
        let fec_source_packets = (fec_word >> 22) & 0x3ff;
        let packet = Self {
            stream_packet_index: (packet_word >> 8) & STREAM_PACKET_INDEX_MASK,
            frame_index: u32::from_le_bytes(extension[4..8].try_into().expect("length checked")),
            flags: (flags_word & 0x0f) as u8,
            fec_current_block: (multi_fec_blocks >> 4) & 0x03,
            fec_last_block: (multi_fec_blocks >> 6) & 0x03,
            is_fec: fec_percentage != 0 && fec_index >= fec_source_packets,
        };
        Ok((packet, payload))
    }

    fn is_start_of_frame(self) -> bool {
        self.flags & FLAG_SOF != 0 && self.fec_current_block == 0
    }

    fn is_end_of_frame(self) -> bool {
        self.flags & FLAG_EOF != 0 && self.fec_current_block == self.fec_last_block
    }
}

struct VideoAccessUnitAssembler {
    codec: NvstVideoCodec,
    current_frame: Option<u32>,
    first_stream_packet_index: Option<u32>,
    keyframe_hint: bool,
    last_packet_payload_length: Option<usize>,
    expected_access_unit_length: Option<usize>,
    invalid_av1_length_logged: bool,
    first_start_payload_logged: bool,
    first_access_unit_logged: bool,
    bytes: Vec<u8>,
    max_access_unit_bytes: usize,
}

impl VideoAccessUnitAssembler {
    fn new(codec: NvstVideoCodec, max_access_unit_bytes: usize) -> Self {
        Self {
            codec,
            current_frame: None,
            first_stream_packet_index: None,
            keyframe_hint: false,
            last_packet_payload_length: None,
            expected_access_unit_length: None,
            invalid_av1_length_logged: false,
            first_start_payload_logged: false,
            first_access_unit_logged: false,
            bytes: Vec::new(),
            max_access_unit_bytes,
        }
    }

    fn reset(&mut self) {
        self.current_frame = None;
        self.first_stream_packet_index = None;
        self.keyframe_hint = false;
        self.last_packet_payload_length = None;
        self.expected_access_unit_length = None;
        self.bytes.clear();
    }

    fn push(
        &mut self,
        header: NvVideoPacket,
        timestamp: u32,
        payload: &[u8],
    ) -> Result<Option<EncodedVideoAccessUnit>, NvstDropReason> {
        if header.is_fec {
            return Err(NvstDropReason::Unsupported(
                NvstUnsupportedFeature::FecRepair,
            ));
        }
        let mut frame_header_size = 0;
        let mut picture_payload = payload;
        if header.is_start_of_frame() {
            self.reset();
            self.keyframe_hint = payload.get(3).is_some_and(|value| *value == 2);
            picture_payload = video_picture_payload(self.codec, payload)
                .ok_or(NvstDropReason::MissingAnnexBStartCode)?;
            frame_header_size = payload.len() - picture_payload.len();
            if !self.first_start_payload_logged {
                self.first_start_payload_logged = true;
                eprintln!(
                    "NVST first {} start payload: frame={} bytes={} frameHeader={} raw={}",
                    self.codec.label(),
                    header.frame_index,
                    payload.len(),
                    frame_header_size,
                    diagnostic_hex(payload, 64),
                );
            }
            if self.codec == NvstVideoCodec::Av1 {
                match payload.first() {
                    Some(0x01) => {
                        self.last_packet_payload_length = payload
                            .get(4..6)
                            .map(|length| usize::from(u16::from_le_bytes([length[0], length[1]])));
                    }
                    Some(0x81) => {
                        self.expected_access_unit_length = payload.get(16..20).map(|length| {
                            u32::from_le_bytes([length[0], length[1], length[2], length[3]])
                                as usize
                        });
                    }
                    _ => {}
                }
            }
            self.current_frame = Some(header.frame_index);
            self.first_stream_packet_index = Some(header.stream_packet_index);
        } else if self.current_frame != Some(header.frame_index) {
            self.reset();
            return Err(NvstDropReason::AwaitingStartOfFrame);
        }

        // GFN pads the final packet to its FEC block size. AVC/HEVC Annex B
        // decoders tolerate those trailing zeroes, while AV1 decoders require
        // exact access units. The short header's bytes 4..6 describe the final
        // packet; the cloud 0x81 header instead advertises the complete AV1
        // access-unit size in bytes 16..20 and is handled after assembly below.
        if self.codec == NvstVideoCodec::Av1
            && header.is_end_of_frame()
            && let Some(reported) = self.last_packet_payload_length
        {
            let exact_payload_length = reported.checked_sub(frame_header_size);
            if let Some(exact_payload_length) = exact_payload_length
                && exact_payload_length > 0
                && exact_payload_length <= picture_payload.len()
            {
                picture_payload = &picture_payload[..exact_payload_length];
            } else if !self.invalid_av1_length_logged {
                self.invalid_av1_length_logged = true;
                eprintln!(
                    "NVST AV1 final-packet length hint ignored: reported={reported} frameHeader={frame_header_size} available={}",
                    picture_payload.len()
                );
            }
        }

        let remaining = self.max_access_unit_bytes.saturating_sub(self.bytes.len());
        if picture_payload.len() > remaining {
            self.reset();
            return Err(NvstDropReason::AccessUnitTooLarge {
                limit: self.max_access_unit_bytes,
            });
        }
        self.bytes.extend_from_slice(picture_payload);
        if !header.is_end_of_frame() {
            return Ok(None);
        }

        let mut bytes = std::mem::take(&mut self.bytes);
        if self.codec == NvstVideoCodec::Av1
            && let Some(reported) = self.expected_access_unit_length
        {
            if reported > 0 && reported <= bytes.len() {
                bytes.truncate(reported);
            } else {
                let available = bytes.len();
                self.reset();
                return Err(NvstDropReason::InvalidLastPacketPayloadLength {
                    reported,
                    frame_header_size,
                    available,
                });
            }
        }
        strip_trailing_access_unit_delimiter(self.codec, &mut bytes);
        if !self.first_access_unit_logged {
            self.first_access_unit_logged = true;
            eprintln!(
                "NVST first {} access unit: frame={} bytes={} keyframeHint={} raw={}",
                self.codec.label(),
                header.frame_index,
                bytes.len(),
                self.keyframe_hint,
                diagnostic_hex(&bytes, 64),
            );
        }
        self.current_frame = None;
        let first_stream_packet_index = self
            .first_stream_packet_index
            .take()
            .expect("start-of-frame initializes the packet index");
        Ok(Some(EncodedVideoAccessUnit {
            codec: self.codec,
            timestamp,
            frame_index: header.frame_index,
            first_stream_packet_index,
            keyframe: video_access_unit_is_keyframe(self.codec, &bytes, self.keyframe_hint),
            contiguous: true,
            bytes,
        }))
    }
}

fn strip_trailing_access_unit_delimiter(codec: NvstVideoCodec, bytes: &mut Vec<u8>) {
    if codec == NvstVideoCodec::Av1 {
        return;
    }
    let mut offset = 0;
    let mut last_nal = None;
    while let Some((start, prefix_len)) = find_annex_b_start_code(&bytes[offset..]) {
        let start = offset + start;
        let nal_start = start + prefix_len;
        last_nal = Some((start, nal_start));
        offset = nal_start + 1;
    }
    if let Some((start, nal_start)) = last_nal
        && bytes.get(nal_start).is_some_and(|header| match codec {
            NvstVideoCodec::H264 => header & 0x1f == 9,
            NvstVideoCodec::H265 => (header >> 1) & 0x3f == 35,
            NvstVideoCodec::Av1 => false,
        })
    {
        bytes.truncate(start);
    }
}

fn video_picture_payload(codec: NvstVideoCodec, payload: &[u8]) -> Option<&[u8]> {
    match codec {
        NvstVideoCodec::H264 | NvstVideoCodec::H265 => {
            let search_len = payload.len().min(MAX_GS_FRAME_HEADER_BYTES + 4);
            let (offset, _) = find_annex_b_start_code(&payload[..search_len])?;
            Some(&payload[offset..])
        }
        NvstVideoCodec::Av1 => match payload.first()? {
            // Unlike AVC/HEVC, AV1 has no Annex-B start code that can be used
            // to discover this boundary. Use the GFN cloud header sizes seen
            // on the same NVST video track instead of the similarly named but
            // different consumer GameStream extended-header layout.
            0x01 => payload.get(GS_SHORT_FRAME_HEADER_BYTES..),
            0x81 => payload.get(GFN_EXTENDED_FRAME_HEADER_BYTES..),
            _ => None,
        },
    }
}

fn video_access_unit_is_keyframe(
    codec: NvstVideoCodec,
    bytes: &[u8],
    frame_header_hint: bool,
) -> bool {
    if codec == NvstVideoCodec::Av1 {
        return frame_header_hint;
    }
    let mut offset = 0;
    while let Some((start, prefix_len)) = find_annex_b_start_code(&bytes[offset..]) {
        let nal_start = offset + start + prefix_len;
        if let Some(nal_header) = bytes.get(nal_start)
            && match codec {
                NvstVideoCodec::H264 => nal_header & 0x1f == 5,
                NvstVideoCodec::H265 => matches!((nal_header >> 1) & 0x3f, 16..=21),
                NvstVideoCodec::Av1 => false,
            }
        {
            return true;
        }
        offset = nal_start;
    }
    false
}

fn find_annex_b_start_code(bytes: &[u8]) -> Option<(usize, usize)> {
    let four_byte = bytes.windows(4).position(|window| window == [0, 0, 0, 1]);
    let three_byte = bytes.windows(3).position(|window| window == [0, 0, 1]);
    match (four_byte, three_byte) {
        (Some(four_byte), Some(three_byte)) if four_byte <= three_byte => Some((four_byte, 4)),
        (_, Some(three_byte)) => Some((three_byte, 3)),
        (Some(four_byte), None) => Some((four_byte, 4)),
        (None, None) => None,
    }
}

struct RtpReorderBuffer {
    next_index: Option<u64>,
    packets: BTreeMap<u64, RtpPacket>,
    max_packets: usize,
    gap_wait: Option<(u64, Instant)>,
}

struct ReorderResult {
    ready: Vec<RtpPacket>,
    nack: Option<(u64, u64)>,
    recovery: Option<NvstRecovery>,
    dropped: Option<NvstDropReason>,
    fec_repaired: usize,
}

impl RtpReorderBuffer {
    fn new(max_packets: usize) -> Self {
        Self {
            next_index: None,
            packets: BTreeMap::new(),
            max_packets,
            gap_wait: None,
        }
    }

    fn reset(&mut self) {
        self.next_index = None;
        self.packets.clear();
        self.gap_wait = None;
    }

    fn push(&mut self, packet: RtpPacket, now: Instant) -> ReorderResult {
        let index = packet.index;
        let next_index = *self.next_index.get_or_insert(index);
        if index < next_index {
            return ReorderResult {
                ready: Vec::new(),
                nack: None,
                recovery: None,
                dropped: Some(NvstDropReason::StaleRtpPacket { index }),
                fec_repaired: 0,
            };
        }
        if self.packets.contains_key(&index) {
            return ReorderResult {
                ready: Vec::new(),
                nack: None,
                recovery: None,
                dropped: Some(NvstDropReason::DuplicateRtpPacket { index }),
                fec_repaired: 0,
            };
        }

        let buffered_before = self.packets.len();
        let mut recovery = None;
        if buffered_before == 0 && index.saturating_sub(next_index) >= self.max_packets as u64 {
            recovery = Some(NvstRecovery::PacketGap {
                first_missing_index: next_index,
                last_missing_index: index - 1,
            });
            self.next_index = Some(index);
            self.gap_wait = None;
        }
        self.packets.insert(index, packet);
        let mut ready = Vec::new();
        let mut nack = None;
        loop {
            while let Some(next) = self.next_index {
                let Some(packet) = self.packets.remove(&next) else {
                    break;
                };
                ready.push(packet);
                self.next_index = Some(next + 1);
            }

            let Some((&first_available, _)) = self.packets.first_key_value() else {
                self.gap_wait = None;
                break;
            };
            let expected = self.next_index.expect("next index remains initialized");
            debug_assert!(first_available > expected);

            // Only NACK sequence numbers that are genuinely absent. Using the
            // newest packet index as the range end also requested every packet
            // already held in the reorder map, causing duplicate retransmission
            // bursts that competed with live video traffic.
            nack = Some((expected, first_available.saturating_sub(1)));
            let gap_started = match self.gap_wait {
                Some((waiting_for, started)) if waiting_for == expected => started,
                _ => {
                    self.gap_wait = Some((expected, now));
                    now
                }
            };
            let gap_exceeded_window = first_available.saturating_sub(expected)
                >= self.max_packets as u64
                || self.packets.len() >= self.max_packets;
            let gap_timed_out =
                now.saturating_duration_since(gap_started) >= MJOLNIR_REORDER_DEQUEUE_TIMEOUT;
            if !gap_exceeded_window && !gap_timed_out {
                break;
            }

            recovery = Some(NvstRecovery::PacketGap {
                first_missing_index: expected,
                last_missing_index: first_available - 1,
            });
            self.next_index = Some(first_available);
            self.gap_wait = None;
            nack = None;
        }
        ReorderResult {
            ready,
            nack,
            recovery,
            dropped: None,
            fec_repaired: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FecPacketLayout {
    frame_index: u32,
    block_index: u8,
    last_block_index: u8,
    shard_index: usize,
    data_shards: usize,
    parity_shards: usize,
}

impl FecPacketLayout {
    fn from_packet(packet: &RtpPacket) -> Option<Self> {
        let extension = packet.header.gs_video_header?;
        let frame_index = u32::from_le_bytes(extension[4..8].try_into().ok()?);
        let fec_word = u32::from_le_bytes(extension[12..16].try_into().ok()?);
        let repair_percent = usize::try_from((fec_word >> 4) & 0xff).ok()?;
        let shard_index = usize::try_from((fec_word >> 12) & 0x3ff).ok()?;
        let data_shards = usize::try_from((fec_word >> 22) & 0x3ff).ok()?;
        if data_shards == 0 || repair_percent == 0 {
            return None;
        }
        let parity_shards = data_shards.saturating_mul(repair_percent).div_ceil(100);
        let total_shards = data_shards.checked_add(parity_shards)?;
        if parity_shards == 0 || shard_index >= total_shards || total_shards > 1_023 {
            return None;
        }
        Some(Self {
            frame_index,
            block_index: (extension[11] >> 4) & 0x03,
            last_block_index: (extension[11] >> 6) & 0x03,
            shard_index,
            data_shards,
            parity_shards,
        })
    }
}

#[derive(Clone)]
struct FecBlock {
    layout: FecPacketLayout,
    base_index: u64,
    shards: Vec<Option<RtpPacket>>,
    repair_failed: bool,
}

struct Gf256Tables {
    exponent: [u8; 512],
    logarithm: [u8; 256],
}

fn gf256_tables() -> &'static Gf256Tables {
    static TABLES: OnceLock<Gf256Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        // NVIDIA's GameStream FEC uses GF(2^8) with primitive polynomial 0x11d.
        let mut exponent = [0_u8; 512];
        let mut logarithm = [0_u8; 256];
        let mut value = 1_u16;
        for (power, slot) in exponent.iter_mut().take(255).enumerate() {
            *slot = value as u8;
            logarithm[value as usize] = power as u8;
            value <<= 1;
            if value & 0x100 != 0 {
                value ^= 0x11d;
            }
        }
        let mut power = 255;
        while power < exponent.len() {
            exponent[power] = exponent[power - 255];
            power += 1;
        }
        Gf256Tables {
            exponent,
            logarithm,
        }
    })
}

fn gf256_multiply(left: u8, right: u8) -> u8 {
    if left == 0 || right == 0 {
        return 0;
    }
    let tables = gf256_tables();
    tables.exponent[usize::from(tables.logarithm[left as usize])
        + usize::from(tables.logarithm[right as usize])]
}

fn gf256_inverse(value: u8) -> Option<u8> {
    (value != 0).then(|| {
        let tables = gf256_tables();
        tables.exponent[255 - usize::from(tables.logarithm[value as usize])]
    })
}

fn gf256_axpy(destination: &mut [u8], source: &[u8], coefficient: u8) {
    debug_assert_eq!(destination.len(), source.len());
    if coefficient == 0 {
        return;
    }
    if coefficient == 1 {
        for (destination, source) in destination.iter_mut().zip(source) {
            *destination ^= *source;
        }
        return;
    }
    for (destination, source) in destination.iter_mut().zip(source) {
        *destination ^= gf256_multiply(coefficient, *source);
    }
}

fn gf256_scale(bytes: &mut [u8], coefficient: u8) {
    if coefficient == 1 {
        return;
    }
    for byte in bytes {
        *byte = gf256_multiply(*byte, coefficient);
    }
}

fn nvst_cauchy_coefficient(
    data_index: usize,
    parity_index: usize,
    parity_shards: usize,
) -> Option<u8> {
    let data_coordinate = u8::try_from(parity_shards.checked_add(data_index)?).ok()?;
    let parity_coordinate = u8::try_from(parity_index).ok()?;
    gf256_inverse(data_coordinate ^ parity_coordinate)
}

/// Reconstructs GameStream's systematic Cauchy Reed-Solomon data shards. Common generic Reed-
/// Solomon crates use a different Vandermonde generator matrix; feeding NVIDIA parity to one
/// produces mathematically valid but corrupt H.264 packets.
fn reconstruct_nvst_cauchy_data(
    shards: &mut [Option<Vec<u8>>],
    data_shards: usize,
    parity_shards: usize,
    shard_len: usize,
) -> Result<(), NvstDropReason> {
    if data_shards == 0
        || parity_shards == 0
        || data_shards + parity_shards > u8::MAX as usize
        || shards.len() != data_shards + parity_shards
    {
        return Err(NvstDropReason::Unsupported(
            NvstUnsupportedFeature::FecRepair,
        ));
    }
    let missing = shards[..data_shards]
        .iter()
        .enumerate()
        .filter_map(|(index, shard)| shard.is_none().then_some(index))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(());
    }
    let parity_rows = shards[data_shards..]
        .iter()
        .enumerate()
        .filter_map(|(index, shard)| shard.is_some().then_some(index))
        .take(missing.len())
        .collect::<Vec<_>>();
    if parity_rows.len() != missing.len() {
        return Err(NvstDropReason::Unsupported(
            NvstUnsupportedFeature::FecRepair,
        ));
    }

    let mut matrix = parity_rows
        .iter()
        .map(|&parity_index| {
            missing
                .iter()
                .map(|&data_index| {
                    nvst_cauchy_coefficient(data_index, parity_index, parity_shards).ok_or(
                        NvstDropReason::Unsupported(NvstUnsupportedFeature::FecRepair),
                    )
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut recovered = parity_rows
        .iter()
        .map(|&parity_index| {
            let mut bytes = shards[data_shards + parity_index]
                .as_ref()
                .expect("selected parity shard is present")
                .clone();
            bytes.resize(shard_len, 0);
            bytes
        })
        .collect::<Vec<_>>();

    for (row, &parity_index) in parity_rows.iter().enumerate() {
        for (data_index, known) in shards[..data_shards].iter().enumerate() {
            let Some(known) = known.as_ref() else {
                continue;
            };
            let coefficient = nvst_cauchy_coefficient(data_index, parity_index, parity_shards)
                .ok_or(NvstDropReason::Unsupported(
                    NvstUnsupportedFeature::FecRepair,
                ))?;
            let known_len = known.len().min(shard_len);
            gf256_axpy(
                &mut recovered[row][..known_len],
                &known[..known_len],
                coefficient,
            );
        }
    }

    // Reduce the missing-data coefficient matrix to identity while applying the same row
    // operations to whole packet shards.
    for column in 0..missing.len() {
        let pivot = (column..missing.len())
            .find(|&row| matrix[row][column] != 0)
            .ok_or(NvstDropReason::Unsupported(
                NvstUnsupportedFeature::FecRepair,
            ))?;
        matrix.swap(column, pivot);
        recovered.swap(column, pivot);
        let inverse = gf256_inverse(matrix[column][column]).ok_or(NvstDropReason::Unsupported(
            NvstUnsupportedFeature::FecRepair,
        ))?;
        gf256_scale(&mut matrix[column][column..], inverse);
        gf256_scale(&mut recovered[column], inverse);
        for row in 0..missing.len() {
            if row == column {
                continue;
            }
            let coefficient = matrix[row][column];
            if coefficient == 0 {
                continue;
            }
            let (target_matrix, pivot_matrix) = if row < column {
                let (before, after) = matrix.split_at_mut(column);
                (&mut before[row], &after[0])
            } else {
                let (before, after) = matrix.split_at_mut(row);
                (&mut after[0], &before[column])
            };
            gf256_axpy(
                &mut target_matrix[column..],
                &pivot_matrix[column..],
                coefficient,
            );
            let (target_shard, pivot_shard) = if row < column {
                let (before, after) = recovered.split_at_mut(column);
                (&mut before[row], &after[0])
            } else {
                let (before, after) = recovered.split_at_mut(row);
                (&mut after[0], &before[column])
            };
            gf256_axpy(target_shard, pivot_shard, coefficient);
        }
    }

    for (data_index, bytes) in missing.into_iter().zip(recovered) {
        shards[data_index] = Some(bytes);
    }
    Ok(())
}

impl FecBlock {
    fn new(layout: FecPacketLayout, packet_index: u64) -> Self {
        let total = layout.data_shards + layout.parity_shards;
        Self {
            layout,
            base_index: packet_index.saturating_sub(layout.shard_index as u64),
            shards: (0..total).map(|_| None).collect(),
            repair_failed: false,
        }
    }

    fn matches(&self, layout: FecPacketLayout, packet_index: u64) -> bool {
        self.layout.frame_index == layout.frame_index
            && self.layout.block_index == layout.block_index
            && self.layout.last_block_index == layout.last_block_index
            && self.layout.data_shards == layout.data_shards
            && self.layout.parity_shards == layout.parity_shards
            && self.base_index == packet_index.saturating_sub(layout.shard_index as u64)
    }

    fn missing_data_range(&self) -> Option<(u64, u64)> {
        let first = self.shards[..self.layout.data_shards]
            .iter()
            .position(Option::is_none)?;
        let last = self.shards[..self.layout.data_shards]
            .iter()
            .rposition(Option::is_none)
            .unwrap_or(first);
        Some((
            self.base_index + first as u64,
            self.base_index + last as u64,
        ))
    }

    fn first_missing_data_run(&self) -> Option<(u64, u64)> {
        let first = self.shards[..self.layout.data_shards]
            .iter()
            .position(Option::is_none)?;
        let length = self.shards[first..self.layout.data_shards]
            .iter()
            .take_while(|shard| shard.is_none())
            .count();
        Some((
            self.base_index + first as u64,
            self.base_index + first as u64 + length.saturating_sub(1) as u64,
        ))
    }

    fn insert(&mut self, layout: FecPacketLayout, packet: RtpPacket) -> Option<NvstDropReason> {
        let slot = &mut self.shards[layout.shard_index];
        if slot.is_some() {
            return Some(NvstDropReason::DuplicateRtpPacket {
                index: packet.index,
            });
        }
        *slot = Some(packet);
        // A newly arrived data or parity shard can make a previously failed reconstruction
        // solvable. Permit one new attempt, but do not spin on every unrelated packet.
        self.repair_failed = false;
        None
    }

    fn has_enough_shards(&self) -> bool {
        self.shards.iter().filter(|shard| shard.is_some()).count() >= self.layout.data_shards
    }

    fn finish(mut self, shard_len: usize) -> Result<(Vec<RtpPacket>, usize), NvstDropReason> {
        let missing_data = self.shards[..self.layout.data_shards]
            .iter()
            .filter(|shard| shard.is_none())
            .count();
        if missing_data == 0 {
            return Ok((
                self.shards[..self.layout.data_shards]
                    .iter_mut()
                    .filter_map(Option::take)
                    .collect(),
                0,
            ));
        }

        if self
            .shards
            .iter()
            .flatten()
            .any(|packet| packet.plaintext.len() > shard_len)
        {
            return Err(NvstDropReason::Unsupported(
                NvstUnsupportedFeature::FecRepair,
            ));
        }
        let template = self.shards[..self.layout.data_shards]
            .iter()
            .flatten()
            .next()
            .ok_or(NvstDropReason::Unsupported(
                NvstUnsupportedFeature::FecRepair,
            ))?;
        let template_header = template.header;
        let mut shard_bytes = self
            .shards
            .iter()
            .map(|shard| {
                shard.as_ref().map(|packet| {
                    let mut bytes = packet.plaintext.clone();
                    bytes.resize(shard_len, 0);
                    bytes
                })
            })
            .collect::<Vec<_>>();
        reconstruct_nvst_cauchy_data(
            &mut shard_bytes,
            self.layout.data_shards,
            self.layout.parity_shards,
            shard_len,
        )?;

        let extension_start = template_header
            .payload_offset
            .checked_sub(NV_VIDEO_PACKET_LEN)
            .ok_or(NvstDropReason::MalformedRtp(
                RtpParseError::InvalidExtensionLength,
            ))?;
        let template_rtp_envelope = template.plaintext[..extension_start].to_vec();
        for (index, shard) in shard_bytes[..self.layout.data_shards]
            .iter_mut()
            .enumerate()
        {
            if self.shards[index].is_some() {
                continue;
            }
            let plaintext = shard.as_mut().ok_or(NvstDropReason::Unsupported(
                NvstUnsupportedFeature::FecRepair,
            ))?;
            if plaintext.len() < extension_start + NV_VIDEO_PACKET_LEN {
                return Err(NvstDropReason::MalformedRtp(
                    RtpParseError::InvalidExtensionLength,
                ));
            }
            let sequence = (self.base_index + index as u64) as u16;
            // A parity packet has its own valid RTP envelope, so those bytes are not necessarily
            // the raw Reed-Solomon parity of the data-packet envelopes. Moonlight restores the
            // recovered RTP header from a received packet before consuming NV_VIDEO_PACKET. Do
            // the equivalent here, including the GS extension profile/length that our strict RTP
            // parser validates. The NVIDIA metadata and encoded payload remain FEC output.
            plaintext[..extension_start].copy_from_slice(&template_rtp_envelope);
            plaintext[2..4].copy_from_slice(&sequence.to_be_bytes());
            plaintext[extension_start + 4..extension_start + 8]
                .copy_from_slice(&self.layout.frame_index.to_le_bytes());
            plaintext[extension_start + 11] =
                (self.layout.last_block_index << 6) | (self.layout.block_index << 4);
            let header = RtpHeader::parse(plaintext).map_err(NvstDropReason::MalformedRtp)?;
            let flags = header.gs_video_header.ok_or(NvstDropReason::MalformedRtp(
                RtpParseError::MissingNvVideoHeader,
            ))?[8]
                & 0x0f;
            let invalid_flags = flags & !(FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA) != 0;
            if invalid_flags {
                return Err(NvstDropReason::Unsupported(
                    NvstUnsupportedFeature::FecRepair,
                ));
            }
            let recovered = RtpPacket {
                index: self.base_index + index as u64,
                header,
                plaintext: std::mem::take(plaintext),
            };
            // Do not validate the reconstructed FEC word. NVIDIA's reference-compatible
            // receivers explicitly exclude fecInfo from recovered-packet validation because
            // it is transport metadata and can differ in a valid repaired data shard. The
            // frame and multi-FEC block coordinates above are restored from the already
            // authenticated block, just like the RTP envelope. Requiring fecInfo to describe
            // a data shard caused every real cloud repair to be rejected even though the H.264
            // payload had been reconstructed successfully.
            self.shards[index] = Some(recovered);
        }
        Ok((
            self.shards[..self.layout.data_shards]
                .iter_mut()
                .filter_map(Option::take)
                .collect(),
            missing_data,
        ))
    }
}

struct FecReorderBuffer {
    blocks: BTreeMap<u64, FecBlock>,
    completed_through: Option<u64>,
    gap_wait: Option<(u64, Instant)>,
    shard_len: usize,
}

impl FecReorderBuffer {
    fn new(video_packet_size: usize) -> Self {
        Self {
            blocks: BTreeMap::new(),
            completed_through: None,
            gap_wait: None,
            shard_len: video_packet_size + NVST_FEC_RTP_HEADER_ALLOWANCE,
        }
    }

    fn reset(&mut self) {
        self.blocks.clear();
        self.completed_through = None;
        self.gap_wait = None;
    }

    fn push(&mut self, packet: RtpPacket, layout: FecPacketLayout, now: Instant) -> ReorderResult {
        let packet_base = packet.index.saturating_sub(layout.shard_index as u64);
        if self
            .completed_through
            .is_some_and(|completed_through| packet.index < completed_through)
        {
            return ReorderResult {
                ready: Vec::new(),
                nack: None,
                recovery: None,
                dropped: None,
                fec_repaired: 0,
            };
        }
        let block = self
            .blocks
            .entry(packet_base)
            .or_insert_with(|| FecBlock::new(layout, packet.index));
        if !block.matches(layout, packet.index) {
            return ReorderResult {
                ready: Vec::new(),
                nack: None,
                recovery: None,
                dropped: Some(NvstDropReason::Unsupported(
                    NvstUnsupportedFeature::FecRepair,
                )),
                fec_repaired: 0,
            };
        }
        let dropped = block.insert(layout, packet);
        if dropped.is_some() {
            return ReorderResult {
                ready: Vec::new(),
                nack: None,
                recovery: None,
                dropped,
                fec_repaired: 0,
            };
        }

        let mut result = ReorderResult {
            ready: Vec::new(),
            nack: None,
            recovery: None,
            dropped: None,
            fec_repaired: 0,
        };
        loop {
            let Some((&base, head)) = self.blocks.first_key_value() else {
                self.gap_wait = None;
                break;
            };

            if let Some(expected) = self.completed_through
                && base > expected
            {
                result.nack = Some((expected, base - 1));
                let started = match self.gap_wait {
                    Some((waiting_for, started)) if waiting_for == expected => started,
                    _ => {
                        self.gap_wait = Some((expected, now));
                        now
                    }
                };
                if now.saturating_duration_since(started) < MJOLNIR_REORDER_DEQUEUE_TIMEOUT
                    && base.saturating_sub(expected) < MAX_REORDER_WINDOW as u64
                    && self.blocks.len() < MAX_PENDING_FEC_BLOCKS
                {
                    break;
                }
                result.recovery = Some(NvstRecovery::PacketGap {
                    first_missing_index: expected,
                    last_missing_index: base - 1,
                });
                result.nack = None;
                self.completed_through = Some(base);
                self.gap_wait = None;
                continue;
            }

            if head.has_enough_shards() && !head.repair_failed {
                let completed = self
                    .blocks
                    .remove(&base)
                    .expect("head FEC block is present");
                let completed_through =
                    base + (completed.layout.data_shards + completed.layout.parity_shards) as u64;
                match completed.clone().finish(self.shard_len) {
                    Ok((mut ready, repaired)) => {
                        result.ready.append(&mut ready);
                        result.fec_repaired += repaired;
                        self.completed_through = Some(completed_through);
                        self.gap_wait = None;
                        continue;
                    }
                    Err(reason) => {
                        let mut pending = completed;
                        pending.repair_failed = true;
                        let (first_missing, last_missing) = pending
                            .first_missing_data_run()
                            .expect("failed FEC reconstruction has missing data");
                        eprintln!(
                            "NVST FEC reconstruction deferred to NACK: frame={} block={}/{} data={} parity={} missing={}..={} reason={reason:?}",
                            pending.layout.frame_index,
                            pending.layout.block_index,
                            pending.layout.last_block_index,
                            pending.layout.data_shards,
                            pending.layout.parity_shards,
                            first_missing,
                            last_missing,
                        );
                        self.blocks.insert(base, pending);
                        result.nack = Some((first_missing, last_missing));
                        self.gap_wait = Some((first_missing, now));
                        break;
                    }
                }
            }

            // A later block proves that the head block has stopped arriving normally. Ask for
            // only the first contiguous missing run and retain all subsequent blocks while the
            // retransmission is in flight.
            if self.blocks.len() == 1 {
                break;
            }
            let (first_missing, last_missing) = head
                .first_missing_data_run()
                .expect("an incomplete FEC block has missing data");
            result.nack = Some((first_missing, last_missing));
            let started = match self.gap_wait {
                Some((waiting_for, started)) if waiting_for == first_missing => started,
                _ => {
                    self.gap_wait = Some((first_missing, now));
                    now
                }
            };
            let block_span = self
                .blocks
                .last_key_value()
                .map_or(0, |(&last_base, _)| last_base.saturating_sub(base));
            if now.saturating_duration_since(started) < MJOLNIR_REORDER_DEQUEUE_TIMEOUT
                && block_span < MAX_REORDER_WINDOW as u64
                && self.blocks.len() < MAX_PENDING_FEC_BLOCKS
            {
                break;
            }

            let failed = self
                .blocks
                .remove(&base)
                .expect("head FEC block is present");
            let (first_missing_index, last_missing_index) = failed
                .missing_data_range()
                .expect("an incomplete FEC block has missing data");
            result.recovery = Some(NvstRecovery::PacketGap {
                first_missing_index,
                last_missing_index,
            });
            result.nack = None;
            self.completed_through =
                Some(base + (failed.layout.data_shards + failed.layout.parity_shards) as u64);
            self.gap_wait = None;
            // Emit any complete successors after the assembler has been marked discontinuous.
        }
        result
    }
}

/// Sends periodic SRTCP Receiver Reports so the peer keeps the video flowing.
/// The official client maintains an SRTCP session (hook captures show the 0x03/0x05
/// KDF labels); without any receiver feedback the server stops video after an
/// initial burst. Only the GCM profiles are supported (the active Mjolnir policy).
struct SrtcpSender {
    encryption_key: SrtcpKey,
    session_salt: [u8; 12],
    tag_len: usize,
    next_index: u32,
    last_sent: Option<Instant>,
}

enum SrtcpKey {
    Aes128([u8; 16]),
    Aes256([u8; 32]),
}

impl SrtcpKey {
    fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Aes128(key) => key,
            Self::Aes256(key) => key,
        }
    }
}

impl SrtcpSender {
    fn from_material(material: &NvstSrtpMaterial) -> Option<Self> {
        let (encryption_key, session_salt, tag_len) = match material {
            NvstSrtpMaterial::AeadAes128Gcm {
                master_key,
                master_salt,
                authentication_tag_len,
            } => (
                SrtcpKey::Aes128(derive_aes128_cm_key::<16>(
                    master_key,
                    master_salt,
                    GFN_SRTCP_KEY_LABEL,
                )),
                derive_aes128_cm_key::<12>(master_key, master_salt, GFN_SRTCP_SALT_LABEL),
                *authentication_tag_len,
            ),
            NvstSrtpMaterial::AeadAes256Gcm {
                master_key,
                master_salt,
                authentication_tag_len,
            } => (
                SrtcpKey::Aes256(derive_aes_cm_key::<32>(
                    master_key,
                    master_salt,
                    GFN_SRTCP_KEY_LABEL,
                )),
                derive_aes_cm_key::<12>(master_key, master_salt, GFN_SRTCP_SALT_LABEL),
                *authentication_tag_len,
            ),
            _ => return None,
        };
        Some(Self {
            encryption_key,
            session_salt,
            tag_len,
            next_index: 0,
            last_sent: None,
        })
    }

    /// Returns an SRTCP Receiver Report to send once per interval, after the media
    /// SSRC is known. Returns `None` when it is not yet time or nothing to report on.
    fn poll_receiver_report(
        &mut self,
        report: Option<RtcpReportBlock>,
        now: Instant,
    ) -> Option<Vec<u8>> {
        let report = report?;
        if let Some(last) = self.last_sent
            && now.duration_since(last) < SRTCP_RR_INTERVAL
        {
            return None;
        }
        self.last_sent = Some(now);
        let index = self.next_index;
        self.next_index = self.next_index.wrapping_add(1);
        Some(protect_srtcp_receiver_report_gcm(
            report,
            index,
            self.encryption_key.as_bytes(),
            &self.session_salt,
            self.tag_len,
        ))
    }
}

/// Stateful, non-blocking NVST video receiver. `process_datagram` is deterministic and testable;
/// `spawn_nvst_udp_receiver` below is a thin UDP/thread wrapper for the production path.
pub struct NvstVideoReceiver {
    config: NvstVideoConfig,
    srtp: SrtpReceiver,
    srtcp: Option<SrtcpSender>,
    reorder: RtpReorderBuffer,
    fec_reorder: FecReorderBuffer,
    assembler: VideoAccessUnitAssembler,
    last_stream_packet_index: Option<u32>,
    next_frame_contiguous: bool,
    state: NvstReceiverState,
    bound_ssrc: Option<u32>,
    highest_sequence_received: u32,
    authenticated_packets: u64,
    fec_packets: u64,
    fec_repaired_packets: u64,
    frames_emitted: u64,
    replay_rejections: u64,
    stale_packets: u64,
    recovered_retransmissions: u64,
    packet_gap_recoveries: u64,
    timeout_origin: Instant,
    last_authenticated_packet: Option<Instant>,
}

impl NvstVideoReceiver {
    pub fn new(config: NvstVideoConfig) -> Self {
        let srtp = SrtpReceiver::from_material(&config.srtp);
        let srtcp = (!config.rtcp_on_sctp)
            .then(|| SrtcpSender::from_material(&config.srtp))
            .flatten();
        let reorder = RtpReorderBuffer::new(config.reorder_window_packets);
        let fec_reorder = FecReorderBuffer::new(config.video_packet_size);
        let assembler = VideoAccessUnitAssembler::new(config.codec, config.max_access_unit_bytes);
        Self {
            config,
            srtp,
            srtcp,
            reorder,
            fec_reorder,
            assembler,
            last_stream_packet_index: None,
            next_frame_contiguous: true,
            state: NvstReceiverState::Running,
            bound_ssrc: None,
            highest_sequence_received: 0,
            authenticated_packets: 0,
            fec_packets: 0,
            fec_repaired_packets: 0,
            frames_emitted: 0,
            replay_rejections: 0,
            stale_packets: 0,
            recovered_retransmissions: 0,
            packet_gap_recoveries: 0,
            timeout_origin: Instant::now(),
            last_authenticated_packet: None,
        }
    }

    pub fn state(&self) -> NvstReceiverState {
        self.state
    }

    pub fn pause(&mut self) -> Option<NvstReceiveEvent> {
        if self.state != NvstReceiverState::Running {
            return None;
        }
        self.reset_media_state();
        self.state = NvstReceiverState::Paused;
        Some(NvstReceiveEvent::Lifecycle(self.state))
    }

    pub fn resume(&mut self) -> Option<NvstReceiveEvent> {
        if self.state != NvstReceiverState::Paused {
            return None;
        }
        self.reset_media_state();
        self.timeout_origin = Instant::now();
        self.state = NvstReceiverState::Running;
        Some(NvstReceiveEvent::Lifecycle(self.state))
    }

    pub fn recover(&mut self) -> Option<NvstReceiveEvent> {
        if self.state != NvstReceiverState::RecoveryRequired {
            return None;
        }
        self.reset_media_state();
        self.timeout_origin = Instant::now();
        self.state = NvstReceiverState::Running;
        Some(NvstReceiveEvent::Lifecycle(self.state))
    }

    pub fn stop(&mut self) -> Option<NvstReceiveEvent> {
        if self.state == NvstReceiverState::Stopped {
            return None;
        }
        self.reset_media_state();
        self.state = NvstReceiverState::Stopped;
        Some(NvstReceiveEvent::Lifecycle(self.state))
    }

    /// Returns a typed timeout only once. The caller must deliberately call `recover` before
    /// more media is accepted, preventing a stale stream from silently resuming.
    pub fn poll_timeout(&mut self, now: Instant) -> Option<NvstReceiveEvent> {
        if self.state != NvstReceiverState::Running {
            return None;
        }
        let last_packet = self
            .last_authenticated_packet
            .unwrap_or(self.timeout_origin);
        let idle_for = now.saturating_duration_since(last_packet);
        if idle_for < self.config.timeout {
            return None;
        }
        self.reset_media_state();
        self.state = NvstReceiverState::RecoveryRequired;
        Some(NvstReceiveEvent::RecoveryNeeded(NvstRecovery::Timeout {
            idle_for,
        }))
    }

    pub fn process_datagram(
        &mut self,
        source: SocketAddr,
        datagram: &[u8],
        now: Instant,
    ) -> Vec<NvstReceiveEvent> {
        if source != self.config.video_peer {
            return vec![NvstReceiveEvent::Dropped(
                NvstDropReason::UnexpectedSource {
                    expected: self.config.video_peer,
                    actual: source,
                },
            )];
        }
        match self.state {
            NvstReceiverState::Paused => {
                return vec![NvstReceiveEvent::Dropped(NvstDropReason::Paused)];
            }
            NvstReceiverState::Stopped => {
                return vec![NvstReceiveEvent::Dropped(NvstDropReason::Stopped)];
            }
            NvstReceiverState::RecoveryRequired => {
                return vec![NvstReceiveEvent::Dropped(NvstDropReason::RecoveryRequired)];
            }
            NvstReceiverState::Running => {}
        }
        let packet = match self.srtp.unprotect(datagram) {
            Ok(packet) => packet,
            Err(reason) => {
                if reason == NvstDropReason::ReplayRejected {
                    self.replay_rejections += 1;
                }
                return vec![NvstReceiveEvent::Dropped(reason)];
            }
        };
        if let Some(expected) = self.config.expected_payload_type
            && packet.header.payload_type != expected
        {
            return vec![NvstReceiveEvent::Dropped(
                NvstDropReason::UnexpectedPayloadType {
                    expected,
                    actual: packet.header.payload_type,
                },
            )];
        }
        let expected_ssrc = self.config.expected_ssrc.or(self.bound_ssrc);
        if let Some(expected) = expected_ssrc
            && packet.header.ssrc != expected
        {
            return vec![NvstReceiveEvent::Dropped(NvstDropReason::UnexpectedSsrc {
                expected,
                actual: packet.header.ssrc,
            })];
        }
        self.bound_ssrc.get_or_insert(packet.header.ssrc);
        self.last_authenticated_packet = Some(now);
        self.authenticated_packets += 1;
        let sequence = u32::try_from(packet.index & 0xffff_ffff).unwrap_or(u32::MAX);
        self.highest_sequence_received = self.highest_sequence_received.max(sequence);
        self.config.feedback.publish_stream(
            packet.header.ssrc,
            self.highest_sequence_received,
            packet.header.timestamp,
            now,
        );
        if self.config.feedback.resolve_nack(packet.index) {
            self.recovered_retransmissions += 1;
        }

        let result = if let Some(layout) = FecPacketLayout::from_packet(&packet) {
            if layout.shard_index >= layout.data_shards {
                self.fec_packets += 1;
            }
            self.fec_reorder.push(packet, layout, now)
        } else {
            self.reorder.push(packet, now)
        };
        self.fec_repaired_packets += result.fec_repaired as u64;
        let mut events = Vec::new();
        if let Some((first_missing_index, last_missing_index)) = result.nack {
            self.config
                .feedback
                .request_nack(first_missing_index, last_missing_index);
        }
        if let Some(reason) = result.dropped {
            if matches!(reason, NvstDropReason::StaleRtpPacket { .. }) {
                self.stale_packets += 1;
            }
            events.push(NvstReceiveEvent::Dropped(reason));
        }
        if let Some(recovery) = result.recovery {
            self.packet_gap_recoveries += 1;
            self.assembler.reset();
            self.next_frame_contiguous = false;
            // A sequence gap breaks the decoder's reference chain; ask (via the
            // bundle's rtcp1 channel) for a fresh keyframe to recover.
            self.config.feedback.request_keyframe();
            events.push(NvstReceiveEvent::RecoveryNeeded(recovery));
        }
        for packet in result.ready {
            let payload = match packet.header.payload(&packet.plaintext) {
                Ok(payload) => payload,
                Err(error) => {
                    events.push(NvstReceiveEvent::Dropped(NvstDropReason::MalformedRtp(
                        error,
                    )));
                    continue;
                }
            };
            let (nv_packet, media) = match NvVideoPacket::parse(&packet.header, payload) {
                Ok(value) => value,
                Err(error) => {
                    self.assembler.reset();
                    self.last_stream_packet_index = None;
                    self.next_frame_contiguous = false;
                    self.config.feedback.request_keyframe();
                    events.push(NvstReceiveEvent::Dropped(NvstDropReason::MalformedRtp(
                        error,
                    )));
                    continue;
                }
            };
            if nv_packet.is_fec {
                self.fec_packets += 1;
                events.push(NvstReceiveEvent::Dropped(NvstDropReason::Unsupported(
                    NvstUnsupportedFeature::FecRepair,
                )));
                continue;
            }
            let same_frame_continuation = !nv_packet.is_start_of_frame()
                && self.assembler.current_frame == Some(nv_packet.frame_index);
            if same_frame_continuation
                && self.last_stream_packet_index.is_some_and(|last| {
                    last.wrapping_add(1) & STREAM_PACKET_INDEX_MASK != nv_packet.stream_packet_index
                })
            {
                self.assembler.reset();
                self.last_stream_packet_index = Some(nv_packet.stream_packet_index);
                self.next_frame_contiguous = false;
                self.config.feedback.request_keyframe();
                events.push(NvstReceiveEvent::Dropped(
                    NvstDropReason::FrameDiscontinuity,
                ));
                continue;
            }
            self.last_stream_packet_index = Some(nv_packet.stream_packet_index);
            match self
                .assembler
                .push(nv_packet, packet.header.timestamp, media)
            {
                Ok(Some(mut frame)) => {
                    frame.contiguous = self.next_frame_contiguous;
                    self.next_frame_contiguous = true;
                    self.frames_emitted += 1;
                    self.config.feedback.publish_completed_frame(&frame);
                    events.push(NvstReceiveEvent::Frame(frame));
                }
                Ok(None) => {}
                Err(reason) => {
                    if matches!(
                        reason,
                        NvstDropReason::FrameDiscontinuity
                            | NvstDropReason::InvalidLastPacketPayloadLength { .. }
                    ) {
                        self.next_frame_contiguous = false;
                        self.config.feedback.request_keyframe();
                    }
                    events.push(NvstReceiveEvent::Dropped(reason));
                }
            }
        }
        events
    }

    /// Returns an SRTCP Receiver Report to send to the video peer, at most once per
    /// interval, once the media SSRC is known. Keeps the peer's video flowing.
    pub fn poll_receiver_report(&mut self, now: Instant) -> Option<Vec<u8>> {
        if self.state != NvstReceiverState::Running {
            return None;
        }
        self.srtcp
            .as_mut()?
            .poll_receiver_report(self.config.feedback.report_snapshot(true), now)
    }

    /// Elapsed-since-start receive counters for ground-truth timing that does not
    /// depend on when buffered log lines happen to flush.
    pub fn stats_line(&self, origin: Instant) -> String {
        format!(
            "elapsed={:.1}s auth={} fec={} fecRecovered={} frames={} replay={} stale={} nackRecovered={} gaps={} ssrc={:?}",
            origin.elapsed().as_secs_f64(),
            self.authenticated_packets,
            self.fec_packets,
            self.fec_repaired_packets,
            self.frames_emitted,
            self.replay_rejections,
            self.stale_packets,
            self.recovered_retransmissions,
            self.packet_gap_recoveries,
            self.bound_ssrc,
        )
    }

    fn process_mjolnir_payload(
        &mut self,
        ssrc: u32,
        rtp_timestamp: u32,
        _payload: &[u8],
        now: Instant,
    ) -> Vec<NvstReceiveEvent> {
        match self.state {
            NvstReceiverState::Paused => {
                return vec![NvstReceiveEvent::Dropped(NvstDropReason::Paused)];
            }
            NvstReceiverState::Stopped => {
                return vec![NvstReceiveEvent::Dropped(NvstDropReason::Stopped)];
            }
            NvstReceiverState::RecoveryRequired => {
                return vec![NvstReceiveEvent::Dropped(NvstDropReason::RecoveryRequired)];
            }
            NvstReceiverState::Running => {}
        }
        self.bound_ssrc.get_or_insert(ssrc);
        self.last_authenticated_packet = Some(now);
        // The bundle path cannot assemble video: the Mjolnir frame metadata lives
        // in the `0x4753` RTP extension, which str0m does not surface. The official
        // cloud path delivers video exclusively on the raw Mjolnir socket, so bundle
        // RTP (audio/control) is intentionally ignored here.
        let _ = rtp_timestamp;
        Vec::new()
    }

    fn reset_media_state(&mut self) {
        self.reorder.reset();
        self.fec_reorder.reset();
        self.assembler.reset();
        self.last_stream_packet_index = None;
        self.next_frame_contiguous = false;
    }
}

enum StunDatagram {
    NotStun,
    Invalid,
    Handled(Option<Vec<u8>>),
}

fn append_stun_attribute(packet: &mut Vec<u8>, attribute_type: u16, value: &[u8]) {
    packet.extend_from_slice(&attribute_type.to_be_bytes());
    packet.extend_from_slice(&(value.len() as u16).to_be_bytes());
    packet.extend_from_slice(value);
    packet.resize(packet.len().next_multiple_of(4), 0);
}

fn build_authenticated_stun_packet(
    message_type: u16,
    transaction_id: &[u8; 12],
    key: &[u8],
    attributes: &[(u16, Vec<u8>)],
) -> Vec<u8> {
    let mut packet = Vec::with_capacity(128);
    packet.extend_from_slice(&message_type.to_be_bytes());
    packet.extend_from_slice(&0_u16.to_be_bytes());
    packet.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
    packet.extend_from_slice(transaction_id);
    for (attribute_type, value) in attributes {
        append_stun_attribute(&mut packet, *attribute_type, value);
    }

    let length_through_integrity = packet.len() - STUN_HEADER_LEN + 24;
    packet[2..4].copy_from_slice(&(length_through_integrity as u16).to_be_bytes());
    let mut mac = HmacSha1::new_from_slice(key).expect("HMAC accepts variable-size ICE passwords");
    mac.update(&packet);
    append_stun_attribute(
        &mut packet,
        STUN_ATTR_MESSAGE_INTEGRITY,
        &mac.finalize().into_bytes(),
    );

    let final_length = packet.len() - STUN_HEADER_LEN + 8;
    packet[2..4].copy_from_slice(&(final_length as u16).to_be_bytes());
    let fingerprint = crc32(&packet) ^ STUN_FINGERPRINT_XOR;
    append_stun_attribute(
        &mut packet,
        STUN_ATTR_FINGERPRINT,
        &fingerprint.to_be_bytes(),
    );
    packet
}

fn build_stun_binding_request(
    credentials: &NvstStunCredentials,
    transaction_id: &[u8; 12],
) -> Vec<u8> {
    let username = format!(
        "{}:{}",
        credentials.remote_username_fragment, credentials.local_username_fragment
    );
    build_authenticated_stun_packet(
        STUN_BINDING_REQUEST,
        transaction_id,
        credentials.remote_password.as_bytes(),
        &[(STUN_ATTR_USERNAME, username.into_bytes())],
    )
}

/// Official `NattHolePunch::SendPing` (pingVersion=6) encodes
/// `SetStunCredentials(local, pingPayload, …, describePassword)` as
/// USERNAME `pingPayload:localUfrag` and HMAC-SHA1 with the DESCRIBE password.
/// WebRtcTransport ICE uses the V2 ufrag via [`build_stun_binding_request`].
fn build_natt_hole_punch_request(
    local_username_fragment: &str,
    ping_payload: &[u8],
    remote_password: &str,
    transaction_id: &[u8; 12],
) -> Vec<u8> {
    let username = format!(
        "{}:{local_username_fragment}",
        String::from_utf8_lossy(ping_payload)
    );
    build_authenticated_stun_packet(
        STUN_BINDING_REQUEST,
        transaction_id,
        remote_password.as_bytes(),
        &[(STUN_ATTR_USERNAME, username.into_bytes())],
    )
}

fn xor_mapped_address(source: SocketAddr, transaction_id: &[u8; 12]) -> Vec<u8> {
    let mut value = Vec::with_capacity(20);
    value.push(0);
    value.push(if source.is_ipv4() { 1 } else { 2 });
    value.extend_from_slice(&(source.port() ^ ((STUN_MAGIC_COOKIE >> 16) as u16)).to_be_bytes());
    match source.ip() {
        IpAddr::V4(ip) => {
            for (octet, mask) in ip.octets().into_iter().zip(STUN_MAGIC_COOKIE.to_be_bytes()) {
                value.push(octet ^ mask);
            }
        }
        IpAddr::V6(ip) => {
            let mut mask = [0_u8; 16];
            mask[..4].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
            mask[4..].copy_from_slice(transaction_id);
            for (octet, mask) in ip.octets().into_iter().zip(mask) {
                value.push(octet ^ mask);
            }
        }
    }
    value
}

fn find_stun_attribute(packet: &[u8], wanted_type: u16) -> Option<(usize, &[u8])> {
    let mut offset = STUN_HEADER_LEN;
    while offset + 4 <= packet.len() {
        let attribute_type = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
        let length = usize::from(u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]));
        let value_start = offset + 4;
        let value_end = value_start.checked_add(length)?;
        if value_end > packet.len() {
            return None;
        }
        if attribute_type == wanted_type {
            return Some((offset, &packet[value_start..value_end]));
        }
        offset = value_end.next_multiple_of(4);
    }
    None
}

fn valid_stun_fingerprint(packet: &[u8]) -> bool {
    let Some((offset, value)) = find_stun_attribute(packet, STUN_ATTR_FINGERPRINT) else {
        return false;
    };
    if value.len() != 4 || offset + 8 != packet.len() {
        return false;
    }
    let expected = crc32(&packet[..offset]) ^ STUN_FINGERPRINT_XOR;
    expected.to_be_bytes().ct_eq(value).into()
}

fn valid_stun_message_integrity(packet: &[u8], key: &[u8]) -> bool {
    let Some((integrity_offset, integrity)) =
        find_stun_attribute(packet, STUN_ATTR_MESSAGE_INTEGRITY)
    else {
        return false;
    };
    if integrity.len() != 20 {
        return false;
    }
    let fingerprint_bytes = if find_stun_attribute(packet, STUN_ATTR_FINGERPRINT).is_some() {
        8
    } else {
        0
    };
    let Some(adjusted_length) = packet
        .len()
        .checked_sub(STUN_HEADER_LEN + fingerprint_bytes)
    else {
        return false;
    };
    let mut authenticated = packet[..integrity_offset].to_vec();
    authenticated[2..4].copy_from_slice(&(adjusted_length as u16).to_be_bytes());
    let mut mac = HmacSha1::new_from_slice(key).expect("HMAC accepts variable-size ICE passwords");
    mac.update(&authenticated);
    mac.finalize().into_bytes().ct_eq(integrity).into()
}

fn handle_stun_datagram(
    datagram: &[u8],
    source: SocketAddr,
    credentials: &NvstStunCredentials,
) -> StunDatagram {
    if datagram.len() < STUN_HEADER_LEN
        || datagram[0] & 0xc0 != 0
        || u32::from_be_bytes([datagram[4], datagram[5], datagram[6], datagram[7]])
            != STUN_MAGIC_COOKIE
    {
        return StunDatagram::NotStun;
    }
    let message_length = usize::from(u16::from_be_bytes([datagram[2], datagram[3]]));
    if STUN_HEADER_LEN + message_length != datagram.len() || !valid_stun_fingerprint(datagram) {
        return StunDatagram::Invalid;
    }
    let message_type = u16::from_be_bytes([datagram[0], datagram[1]]);
    let transaction_id: [u8; 12] = datagram[8..20]
        .try_into()
        .expect("STUN header length checked above");
    match message_type {
        STUN_BINDING_REQUEST => {
            let Some((_, username)) = find_stun_attribute(datagram, STUN_ATTR_USERNAME) else {
                return StunDatagram::Invalid;
            };
            let expected_username = format!(
                "{}:{}",
                credentials.local_username_fragment, credentials.remote_username_fragment
            );
            if !bool::from(expected_username.as_bytes().ct_eq(username))
                || !valid_stun_message_integrity(datagram, credentials.local_password.as_bytes())
            {
                return StunDatagram::Invalid;
            }
            let mapped_address = xor_mapped_address(source, &transaction_id);
            StunDatagram::Handled(Some(build_authenticated_stun_packet(
                STUN_BINDING_SUCCESS_RESPONSE,
                &transaction_id,
                credentials.local_password.as_bytes(),
                &[(STUN_ATTR_XOR_MAPPED_ADDRESS, mapped_address)],
            )))
        }
        STUN_BINDING_SUCCESS_RESPONSE => {
            if valid_stun_message_integrity(datagram, credentials.remote_password.as_bytes()) {
                StunDatagram::Handled(None)
            } else {
                StunDatagram::Invalid
            }
        }
        _ => StunDatagram::Invalid,
    }
}

enum UdpReceiverCommand {
    Pause,
    Resume,
    Recover,
    SendInput {
        bytes: Vec<u8>,
        reply: Option<mpsc::SyncSender<Result<(), TransportError>>>,
    },
    Stop,
}

/// Owns the bounded UDP receive worker. Frames go through the same bounded `MediaConsumer` used
/// by WebRTC, so a slow decoder cannot make UDP receive unbounded.
pub struct NvstUdpReceiverSession {
    commands: Sender<UdpReceiverCommand>,
    join: Option<JoinHandle<()>>,
    input_ready: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct NvstUdpReceiverControl {
    commands: Sender<UdpReceiverCommand>,
    input_ready: Arc<AtomicBool>,
}

#[derive(Debug, Error)]
pub enum NvstUdpReceiverError {
    #[error("failed to bind NVST UDP socket: {0}")]
    Bind(#[source] std::io::Error),
    #[error("failed to configure NVST UDP socket: {0}")]
    Configure(#[source] std::io::Error),
    #[error("failed to start NVST receive worker: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("NVST receive worker is no longer running")]
    Closed,
    #[error("failed to prepare NVST WebRTC bundle: {0}")]
    WebrtcBundle(String),
}

impl NvstUdpReceiverSession {
    pub fn control(&self) -> NvstUdpReceiverControl {
        NvstUdpReceiverControl {
            commands: self.commands.clone(),
            input_ready: Arc::clone(&self.input_ready),
        }
    }

    pub fn pause(&self) -> Result<(), NvstUdpReceiverError> {
        self.control().pause()
    }

    pub fn resume(&self) -> Result<(), NvstUdpReceiverError> {
        self.control().resume()
    }

    pub fn recover(&self) -> Result<(), NvstUdpReceiverError> {
        self.control().recover()
    }

    pub fn send_input(
        &self,
        bytes: Vec<u8>,
        partially_reliable: bool,
    ) -> Result<(), TransportError> {
        self.control().send_input(bytes, partially_reliable)
    }

    pub fn stop(mut self) {
        let _ = self.commands.send(UdpReceiverCommand::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl NvstUdpReceiverControl {
    pub fn pause(&self) -> Result<(), NvstUdpReceiverError> {
        self.send(UdpReceiverCommand::Pause)
    }

    pub fn resume(&self) -> Result<(), NvstUdpReceiverError> {
        self.send(UdpReceiverCommand::Resume)
    }

    pub fn recover(&self) -> Result<(), NvstUdpReceiverError> {
        self.send(UdpReceiverCommand::Recover)
    }

    pub fn send_input(
        &self,
        bytes: Vec<u8>,
        _partially_reliable: bool,
    ) -> Result<(), TransportError> {
        if !self.input_ready.load(Ordering::Acquire) {
            return Err(TransportError::InputNotReady);
        }
        let (reply, result) = mpsc::sync_channel(1);
        self.commands
            .send(UdpReceiverCommand::SendInput {
                bytes,
                reply: Some(reply),
            })
            .map_err(|_| TransportError::Closed)?;
        result
            .recv_timeout(Duration::from_millis(500))
            .map_err(|_| TransportError::Closed)?
    }

    /// Queues latency-sensitive locally captured input without waiting for the
    /// receive worker to round-trip an acknowledgement. Readiness is still
    /// checked before enqueueing and the ordered command channel preserves the
    /// exact keyboard/button/motion sequence.
    pub fn queue_input(
        &self,
        bytes: Vec<u8>,
        _partially_reliable: bool,
    ) -> Result<(), TransportError> {
        if !self.input_ready.load(Ordering::Acquire) {
            return Err(TransportError::InputNotReady);
        }
        self.commands
            .send(UdpReceiverCommand::SendInput { bytes, reply: None })
            .map_err(|_| TransportError::Closed)
    }

    pub fn stop(&self) -> Result<(), NvstUdpReceiverError> {
        self.send(UdpReceiverCommand::Stop)
    }

    fn send(&self, command: UdpReceiverCommand) -> Result<(), NvstUdpReceiverError> {
        self.commands
            .send(command)
            .map_err(|_| NvstUdpReceiverError::Closed)
    }
}

impl Drop for NvstUdpReceiverSession {
    fn drop(&mut self) {
        let _ = self.commands.send(UdpReceiverCommand::Stop);
    }
}

fn discover_routed_ipv4() -> Option<IpAddr> {
    let probe = UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)).ok()?;
    probe
        .connect(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 9))
        .ok()?;
    let addr = probe.local_addr().ok()?;
    if addr.ip().is_unspecified() || addr.ip().is_loopback() {
        return None;
    }
    Some(addr.ip())
}

pub fn reserve_nvst_udp_socket() -> std::io::Result<UdpSocket> {
    // Official binds the bundle sockets on 0.0.0.0 and advertises the routed
    // NIC IPv4 separately via clientPorts.localAddress + host a=candidate.
    bind_nvst_udp(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)
}

/// IPv4 OpenNOW should put in ANNOUNCE `localAddress` / host candidate.
/// Independent of the bind address: official listens on 0.0.0.0.
pub fn advertised_nvst_ipv4() -> Option<IpAddr> {
    discover_routed_ipv4()
}

/// ICE + DTLS identity that already owns the reserved bundle socket.
#[derive(Debug, Clone)]
pub struct NvstBundleIdentity {
    pub ice_username_fragment: String,
    pub ice_password: String,
    pub dtls_fingerprint: String,
}

/// UDP socket plus the `Rtc` that will speak ICE/DTLS on it, plus the dedicated
/// NATT-only video (Mjolnir) socket the official two-socket cloud model uses for
/// raw-SRTP video.
pub struct ReservedNvstBundle {
    socket: UdpSocket,
    rtc: Rtc,
    mjolnir_socket: UdpSocket,
}

impl ReservedNvstBundle {
    pub fn reserve() -> Result<Self, NvstUdpReceiverError> {
        // Bifrost's legacy dedicated-socket layout assigns video to N and the
        // native ICE/DTLS bundle to N+1. The server still relies on this pairing
        // even though ANNOUNCE advertises clientPorts.video=0 and routes video
        // after the Mjolnir NATT ping. Reserving the bundle first reverses those
        // ports and causes some seats to keep all video off the client entirely.
        let (socket, mjolnir_socket) =
            reserve_nvst_socket_pair().map_err(NvstUdpReceiverError::Bind)?;
        let rtc = create_nvst_bundle_rtc(&socket)?;
        Ok(Self {
            socket,
            rtc,
            mjolnir_socket,
        })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    pub fn mjolnir_local_addr(&self) -> std::io::Result<SocketAddr> {
        self.mjolnir_socket.local_addr()
    }

    pub fn advertised_local_address(&self) -> Option<String> {
        match self.socket.local_addr().ok()?.ip() {
            IpAddr::V4(ip) if !ip.is_unspecified() && !ip.is_loopback() => Some(ip.to_string()),
            _ => advertised_nvst_ipv4().map(|ip| ip.to_string()),
        }
    }

    pub fn identity(&mut self) -> NvstBundleIdentity {
        nvst_local_bundle_identity(&mut self.rtc)
    }

    pub fn send_to(&self, payload: &[u8], host: &str, port: u16) -> std::io::Result<usize> {
        self.socket.send_to(payload, (host, port))
    }

    pub fn try_clone_socket(&self) -> std::io::Result<UdpSocket> {
        self.socket.try_clone()
    }

    pub fn into_parts(self) -> (UdpSocket, Rtc, UdpSocket) {
        (self.socket, self.rtc, self.mjolnir_socket)
    }
}

fn generate_gfn_local_ice_credentials() -> Result<IceCreds, getrandom::Error> {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+/";
    let mut random = [0_u8; 26];
    getrandom::fill(&mut random)?;
    let encode = |start: usize, length: usize| {
        random[start..start + length]
            .iter()
            .map(|value| ALPHABET[usize::from(*value) & 0x3f] as char)
            .collect::<String>()
    };
    Ok(IceCreds {
        ufrag: encode(0, 4),
        pass: encode(4, 22),
    })
}

fn create_nvst_bundle_rtc(socket: &UdpSocket) -> Result<Rtc, NvstUdpReceiverError> {
    install_crypto();
    let _ = socket;
    // Official GenerateIceCredentials() is 4-char ufrag / 22-char password.
    // str0m's default 16-char ufrag is rejected by Bifrost length checks.
    let mut rtc_config = RtcConfig::new().set_rtp_mode(true);
    rtc_config.codec_config().add_config(
        GFN_RED_PAYLOAD_TYPE.into(),
        None,
        Codec::Opus,
        Frequency::FORTY_EIGHT_KHZ,
        Some(2),
        FormatParams::default(),
    );
    let mut rtc = rtc_config.build(Instant::now());
    let credentials = generate_gfn_local_ice_credentials()
        .map_err(|error| NvstUdpReceiverError::WebrtcBundle(error.to_string()))?;
    rtc.direct_api().set_local_ice_credentials(credentials);
    Ok(rtc)
}

fn nvst_local_bundle_identity(rtc: &mut Rtc) -> NvstBundleIdentity {
    let creds = rtc.direct_api().local_ice_credentials();
    let fingerprint = rtc.direct_api().local_dtls_fingerprint().clone();
    NvstBundleIdentity {
        ice_username_fragment: creds.ufrag,
        ice_password: creds.pass,
        dtls_fingerprint: nvst_fingerprint_hex(&fingerprint),
    }
}

fn nvst_fingerprint_hex(fingerprint: &Fingerprint) -> String {
    fingerprint
        .bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn routed_host_addr(peer: Option<SocketAddr>, local: SocketAddr) -> SocketAddr {
    if !local.ip().is_unspecified() && !local.ip().is_loopback() {
        return local;
    }
    if let Some(peer) = peer
        && let Ok(probe) = UdpSocket::bind(SocketAddr::new(
            if peer.is_ipv4() {
                IpAddr::V4(Ipv4Addr::UNSPECIFIED)
            } else {
                IpAddr::V6(Ipv6Addr::UNSPECIFIED)
            },
            0,
        ))
    {
        let _ = probe.connect(peer);
        if let Ok(addr) = probe.local_addr()
            && !addr.ip().is_unspecified()
            && !addr.ip().is_loopback()
        {
            return SocketAddr::new(addr.ip(), local.port());
        }
    }
    local
}

fn logical_ice_addr(addr: SocketAddr, loopback_octet: u8) -> SocketAddr {
    match addr.ip() {
        IpAddr::V4(ip) if ip.is_link_local() => SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, loopback_octet)),
            addr.port(),
        ),
        _ => addr,
    }
}

fn parse_nvst_fingerprint(value: &str) -> Result<Fingerprint, String> {
    let trimmed = value.trim();
    let sdp = if trimmed.contains(' ') {
        trimmed.to_owned()
    } else {
        format!("sha-256 {trimmed}")
    };
    sdp.parse()
}

fn bind_nvst_udp(bind_ip: IpAddr, port: u16) -> std::io::Result<UdpSocket> {
    #[cfg(unix)]
    if let Ok(fd_text) = std::env::var("OPENNOW_NVST_VIDEO_UDP_FD") {
        if let Ok(fd) = fd_text.parse::<std::os::unix::io::RawFd>() {
            use std::os::unix::io::FromRawFd;
            // Electron dups the probe socket onto this fd so native never rebinds.
            eprintln!("NVST inheriting video UDP socket from fd {fd}");
            return Ok(unsafe { UdpSocket::from_raw_fd(fd) });
        }
    }
    bind_nvst_udp_socket(bind_ip, port)
}

fn bind_nvst_udp_socket(bind_ip: IpAddr, port: u16) -> std::io::Result<UdpSocket> {
    let domain = if bind_ip.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };
    let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    if let Err(error) = socket.set_recv_buffer_size(NVST_UDP_RECEIVE_BUFFER_BYTES) {
        eprintln!(
            "NVST could not enlarge UDP receive buffer to {NVST_UDP_RECEIVE_BUFFER_BYTES} bytes: {error}"
        );
    }
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.bind(&SocketAddr::new(bind_ip, port).into())?;
    Ok(socket.into())
}

/// Reserves the dedicated NATT-only video (Mjolnir) socket. The native streamer
/// always owns this socket outright, so it never inherits an Electron-owned fd.
pub fn reserve_nvst_mjolnir_udp_socket() -> std::io::Result<UdpSocket> {
    bind_nvst_udp_socket(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)
}

fn reserve_nvst_socket_pair() -> std::io::Result<(UdpSocket, UdpSocket)> {
    // Bifrost's Windows client reserves this exact pair first
    // (`general.clientPorts.useReserved=1`) and only falls back to dynamic
    // ports when it is unavailable. Some cloud seats do not route the
    // dedicated Mjolnir video flow back to an arbitrary source port even
    // though the version-6 NATT request is otherwise valid. Match the
    // official preference while preserving a non-fatal fallback for users
    // that already have either port occupied.
    const OFFICIAL_MJOLNIR_PORT: u16 = 49_005;
    const OFFICIAL_BUNDLE_PORT: u16 = 49_006;
    const MAX_PAIR_ATTEMPTS: usize = 32;
    let bind_ip = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
    let mut last_error = match bind_nvst_udp_socket(bind_ip, OFFICIAL_MJOLNIR_PORT) {
        Ok(mjolnir) => match bind_nvst_udp_socket(bind_ip, OFFICIAL_BUNDLE_PORT) {
            Ok(bundle) => return Ok((bundle, mjolnir)),
            Err(error) => error,
        },
        Err(error) => error,
    };

    for _ in 0..MAX_PAIR_ATTEMPTS {
        let mjolnir = reserve_nvst_mjolnir_udp_socket()?;
        let video_port = mjolnir.local_addr()?.port();
        let Some(bundle_port) = video_port.checked_add(1) else {
            continue;
        };
        match bind_nvst_udp_socket(bind_ip, bundle_port) {
            Ok(bundle) => return Ok((bundle, mjolnir)),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

pub fn spawn_nvst_udp_receiver(
    config: NvstVideoConfig,
    media_consumer: MediaConsumer,
    event_sender: Sender<NvstReceiveEvent>,
) -> Result<NvstUdpReceiverSession, NvstUdpReceiverError> {
    spawn_nvst_udp_receiver_with_socket(config, media_consumer, event_sender, None, None)
}

pub fn spawn_nvst_udp_receiver_with_socket(
    config: NvstVideoConfig,
    media_consumer: MediaConsumer,
    event_sender: Sender<NvstReceiveEvent>,
    reserved_socket: Option<UdpSocket>,
    reserved_rtc: Option<Rtc>,
) -> Result<NvstUdpReceiverSession, NvstUdpReceiverError> {
    let bind_ip = match config.video_peer.ip() {
        IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
    };
    let socket = match reserved_socket {
        Some(socket) => {
            eprintln!(
                "NVST using UDP socket reserved before ANNOUNCE ({})",
                socket
                    .local_addr()
                    .map(|addr| addr.to_string())
                    .unwrap_or_else(|_| "unknown".to_owned())
            );
            socket
        }
        None => {
            bind_nvst_udp(bind_ip, config.client_udp_port).map_err(NvstUdpReceiverError::Bind)?
        }
    };
    socket
        .set_read_timeout(Some(UDP_RECEIVE_POLL_INTERVAL))
        .map_err(NvstUdpReceiverError::Configure)?;
    let rtc = if config.remote_dtls_fingerprint().is_some() {
        let rtc = match reserved_rtc {
            Some(rtc) => rtc,
            None => create_nvst_bundle_rtc(&socket)?,
        };
        Some(prepare_nvst_webrtc_bundle(&socket, &config, rtc)?)
    } else {
        None
    };
    spawn_receiver_thread(
        "opennow-nvst-video",
        socket,
        config,
        media_consumer,
        event_sender,
        rtc,
    )
}

/// Spawns the raw-SRTP NATT video receiver on the reserved Mjolnir socket.
///
/// Official GFN cloud (`nativeRtcOnBundlePort=1`) delivers video RTP/SRTP to this
/// dedicated NATT-only socket while the ICE/DTLS bundle socket carries
/// control/audio. The raw receiver's NATT keepalive pings are what route video
/// here, and it decrypts with the runtime encryptionKey sent in ANNOUNCE.
pub fn spawn_nvst_mjolnir_receiver(
    socket: UdpSocket,
    config: NvstVideoConfig,
    media_consumer: MediaConsumer,
    event_sender: Sender<NvstReceiveEvent>,
) -> Result<NvstUdpReceiverSession, NvstUdpReceiverError> {
    eprintln!(
        "NVST Mjolnir raw-SRTP video receiver arming on {}",
        socket
            .local_addr()
            .map(|addr| addr.to_string())
            .unwrap_or_else(|_| "unknown".to_owned())
    );
    spawn_receiver_thread(
        "opennow-nvst-mjolnir",
        socket,
        config,
        media_consumer,
        event_sender,
        None,
    )
}

struct NvstReceiverOutputs {
    media_consumer: MediaConsumer,
    event_sender: Sender<NvstReceiveEvent>,
    input_ready: Arc<AtomicBool>,
}

fn spawn_receiver_thread(
    name: &str,
    socket: UdpSocket,
    config: NvstVideoConfig,
    media_consumer: MediaConsumer,
    event_sender: Sender<NvstReceiveEvent>,
    rtc: Option<Rtc>,
) -> Result<NvstUdpReceiverSession, NvstUdpReceiverError> {
    socket
        .set_read_timeout(Some(UDP_RECEIVE_POLL_INTERVAL))
        .map_err(NvstUdpReceiverError::Configure)?;
    let (commands, receiver) = mpsc::channel();
    let input_ready = Arc::new(AtomicBool::new(false));
    let worker_input_ready = input_ready.clone();
    let transport_origin = Instant::now();
    let join = thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            run_nvst_udp_receiver(
                socket,
                config,
                receiver,
                NvstReceiverOutputs {
                    media_consumer,
                    event_sender,
                    input_ready: worker_input_ready.clone(),
                },
                transport_origin,
                rtc,
            );
            worker_input_ready.store(false, Ordering::Release);
        })
        .map_err(NvstUdpReceiverError::Spawn)?;
    Ok(NvstUdpReceiverSession {
        commands,
        join: Some(join),
        input_ready,
    })
}

fn prepare_nvst_webrtc_bundle(
    socket: &UdpSocket,
    config: &NvstVideoConfig,
    mut rtc: Rtc,
) -> Result<Rtc, NvstUdpReceiverError> {
    let fingerprint = config.remote_dtls_fingerprint().ok_or_else(|| {
        NvstUdpReceiverError::WebrtcBundle("missing remote DTLS fingerprint".into())
    })?;
    let remote_fingerprint =
        parse_nvst_fingerprint(fingerprint).map_err(NvstUdpReceiverError::WebrtcBundle)?;
    let credentials = config.stun_credentials.as_ref().ok_or_else(|| {
        NvstUdpReceiverError::WebrtcBundle(
            "DTLS bundle requires local and remote ICE credentials".into(),
        )
    })?;
    let local_addr = socket
        .local_addr()
        .map_err(NvstUdpReceiverError::Configure)?;
    let bundle_peer = config.bundle_peer();
    let physical_local = routed_host_addr(Some(bundle_peer), local_addr);
    let local_candidate = Candidate::host(logical_ice_addr(physical_local, 1), "udp")
        .map_err(|error| NvstUdpReceiverError::WebrtcBundle(error.to_string()))?;
    let remote_candidate = Candidate::host(logical_ice_addr(bundle_peer, 2), "udp")
        .map_err(|error| NvstUdpReceiverError::WebrtcBundle(error.to_string()))?;
    rtc.add_local_candidate(local_candidate);
    rtc.add_remote_candidate(remote_candidate);
    {
        let mut api = rtc.direct_api();
        api.set_ice_controlling(true);
        api.set_local_ice_credentials(IceCreds {
            ufrag: credentials.local_username_fragment.clone(),
            pass: credentials.local_password.clone(),
        });
        api.set_remote_ice_credentials(IceCreds {
            ufrag: credentials.remote_username_fragment.clone(),
            pass: credentials.remote_password.clone(),
        });
        api.set_remote_fingerprint(remote_fingerprint);
        if let Some(audio) = config.audio_track() {
            api.declare_media(Mid::from(audio.mid.as_str()), MediaKind::Audio);
        }
        api.start_dtls(true)
            .map_err(|error| NvstUdpReceiverError::WebrtcBundle(error.to_string()))?;
    }
    eprintln!(
        "NVST WebRTC bundle armed (local={}, peer={}, remoteFingerprintBytes={})",
        physical_local,
        bundle_peer,
        fingerprint.len()
    );
    Ok(rtc)
}

fn looks_like_rtp(datagram: &[u8]) -> bool {
    datagram.len() >= RTP_FIXED_HEADER_LEN
        && datagram[0] >> 6 == 2
        && !looks_like_stun(datagram)
        && !looks_like_dtls(datagram)
        && !looks_like_rtcp(datagram)
}

fn looks_like_rtcp(datagram: &[u8]) -> bool {
    datagram.len() >= 4 && datagram[0] >> 6 == 2 && matches!(datagram[1], 192..=223)
}

fn looks_like_stun(datagram: &[u8]) -> bool {
    datagram.len() >= STUN_HEADER_LEN && datagram[4..8] == STUN_MAGIC_COOKIE.to_be_bytes()
}

#[cfg_attr(not(test), allow(dead_code))]
fn synthesize_ice_binding_success(
    transaction_id: &[u8; 12],
    mapped: SocketAddr,
    remote_password: &str,
) -> Vec<u8> {
    build_authenticated_stun_packet(
        STUN_BINDING_SUCCESS_RESPONSE,
        transaction_id,
        remote_password.as_bytes(),
        &[(
            STUN_ATTR_XOR_MAPPED_ADDRESS,
            xor_mapped_address(mapped, transaction_id),
        )],
    )
}

fn looks_like_dtls(datagram: &[u8]) -> bool {
    matches!(datagram.first().copied(), Some(20..=63))
}

fn peek_rtp_ssrc(datagram: &[u8]) -> Option<u32> {
    looks_like_rtp(datagram)
        .then(|| u32::from_be_bytes([datagram[8], datagram[9], datagram[10], datagram[11]]))
}

fn peek_rtp_payload_type(datagram: &[u8]) -> Option<u8> {
    looks_like_rtp(datagram).then(|| datagram[1] & 0x7f)
}

fn matches_audio_track(track: &NvstAudioTrack, payload_type: u8, ssrc: u32) -> bool {
    let matches_payload_type = payload_type == track.payload_type
        || (track.payload_type == GFN_OPUS_PAYLOAD_TYPE && payload_type == GFN_RED_PAYLOAD_TYPE);
    matches_payload_type && track.ssrc.is_none_or(|expected| expected == ssrc)
}

fn extract_red_opus_primary(payload: &[u8]) -> Option<&[u8]> {
    let mut header_offset = 0_usize;
    let mut redundant_payload_bytes = 0_usize;
    let mut redundant_blocks = 0_usize;
    loop {
        let header = *payload.get(header_offset)?;
        let payload_type = header & 0x7f;
        if header & 0x80 == 0 {
            if payload_type != GFN_OPUS_PAYLOAD_TYPE {
                return None;
            }
            let primary_offset = header_offset
                .checked_add(1)?
                .checked_add(redundant_payload_bytes)?;
            let primary = payload.get(primary_offset..)?;
            return (!primary.is_empty() && primary.len() <= MAX_OPUS_PACKET_BYTES)
                .then_some(primary);
        }
        if redundant_blocks == MAX_REDUNDANT_AUDIO_BLOCKS {
            return None;
        }
        let header_bytes = payload.get(header_offset..header_offset.checked_add(4)?)?;
        let block_len = (usize::from(header_bytes[2] & 0x03) << 8) | usize::from(header_bytes[3]);
        redundant_payload_bytes = redundant_payload_bytes.checked_add(block_len)?;
        redundant_blocks += 1;
        header_offset = header_offset.checked_add(4)?;
    }
}

fn finish_nvst_input_handshake(
    input_state: &mut NvstInputChannelState,
    channels: NvstInputChannels,
    rtc: &mut Rtc,
    transport_origin: Instant,
    input_ready: &AtomicBool,
    event_sender: &Sender<NvstReceiveEvent>,
    version: u16,
) -> bool {
    if !input_state.activation_sent() {
        let timestamp_us = transport_origin
            .elapsed()
            .as_micros()
            .try_into()
            .unwrap_or(u64::MAX);
        if !channels.send_activation(rtc, timestamp_us) {
            let _ = event_sender.send(NvstReceiveEvent::InputUnavailable(
                "input activation could not be queued".to_owned(),
            ));
            return false;
        }
        eprintln!(
            "NVST cursor capture tx: channel={} command=0x0308 enabled=true reason=input-activation",
            channels.label(channels.control_reliable),
        );
        eprintln!(
            "NVST remote cursor tracking tx: channel={} command=0x030d enabled=true reason=input-activation",
            channels.label(channels.control_reliable),
        );
        eprintln!(
            "NVST client state tx: channel={} window_state=19 system_state=0 frame=0",
            channels.label(channels.control_reliable),
        );
        input_state.mark_activation_sent();
    }
    input_ready.store(true, Ordering::Release);
    let _ = event_sender.send(NvstReceiveEvent::InputReady(version));
    true
}

fn run_nvst_webrtc_bundle(
    socket: UdpSocket,
    config: NvstVideoConfig,
    commands: Receiver<UdpReceiverCommand>,
    outputs: NvstReceiverOutputs,
    transport_origin: Instant,
    mut rtc: Rtc,
) {
    let NvstReceiverOutputs {
        media_consumer,
        event_sender,
        input_ready,
    } = outputs;
    let bundle_peer = config.bundle_peer();
    let stun_credentials = config.stun_credentials.clone();
    let ping_payload = config.ping_payload.clone();
    // Feedback plane shared with the Mjolnir video receiver: it publishes the
    // stream SSRC/sequence and recovery requests; this bundle sends the RTCP
    // Receiver Reports / NACK / PLI over the `rtcp1` SCTP data channel.
    let feedback = config.feedback();
    let audio_track = config.audio_track().cloned();
    let frame_time_us = config.frame_time_us;
    // With a dedicated Mjolnir video socket the bundle only carries
    // control/audio keepalive traffic; the Mjolnir receiver owns the media
    // timeout, so the bundle must not raise a spurious media recovery.
    let owns_media_timeout = config.mjolnir_udp_port.is_none();
    let physical_local = socket.local_addr().ok().map_or_else(
        || bundle_peer,
        |local| routed_host_addr(Some(bundle_peer), local),
    );
    let receive_destination = logical_ice_addr(physical_local, 1);
    let receive_source = logical_ice_addr(bundle_peer, 2);
    let mut receiver = NvstVideoReceiver::new(config);
    let mut datagram = vec![0_u8; 65_536];
    let mut inbound_datagrams = 0_u64;
    let mut outbound_datagrams = 0_u64;
    let mut hole_punch_pings = 0_u64;
    let mut last_hole_punch = Instant::now() - PING_INTERVAL_BEFORE_CONNECTION;
    let mut seen_ssrcs = HashSet::new();
    let mut dtls_ready = false;
    // RTCP-over-SCTP (`rtcp1`) feedback channel state.
    let mut sctp_started = false;
    let mut rtcp_channel: Option<ChannelId> = None;
    let mut rtcp_channel_open = false;
    let mut control_partial_open = false;
    let rtcp_sender_ssrc = RTCP_SENDER_SSRC;
    let mut last_rtcp_send = Instant::now() - SRTCP_RR_INTERVAL;
    let mut last_recovery_send = Instant::now();
    let mut last_keyframe_send = Instant::now() - KEYFRAME_REQUEST_COOLDOWN;
    let mut rtcp_reports_sent = 0_u64;
    let mut qos_sequence = 0_u32;
    let mut last_qos_send = Instant::now() - QOS_REPORT_INTERVAL;
    let mut sctp_started_at: Option<Instant> = None;
    let mut input_channels: Option<NvstInputChannels> = None;
    let mut input_state = NvstInputChannelState::default();
    let mut input_codec = NvstInputCodec::default();
    let mut last_input_types = Vec::new();
    let mut mouse_motion_packets = 0_u64;
    let mut server_cursor_hidden = false;
    let mut cursor_capture_retry_at: Option<Instant> = None;
    let mut cursor_capture_attempts = 0_u8;
    let mut control_keepalive_at = next_control_keepalive(Instant::now());
    let mut input_timeout_reported = false;
    let mut last_audio_sequence: Option<(u32, u16)> = None;
    loop {
        loop {
            match commands.try_recv() {
                Ok(UdpReceiverCommand::Pause) => forward_optional(&event_sender, receiver.pause()),
                Ok(UdpReceiverCommand::Resume) => {
                    forward_optional(&event_sender, receiver.resume())
                }
                Ok(UdpReceiverCommand::Recover) => {
                    forward_optional(&event_sender, receiver.recover())
                }
                Ok(UdpReceiverCommand::SendInput { bytes, reply }) => {
                    let mut sent = true;
                    if input_state.is_ready()
                        && let Some(channels) = input_channels
                    {
                        let input_types = native_input_types(&bytes);
                        let should_log = verbose_diagnostics_enabled()
                            && match input_types.as_ref() {
                                Ok(types) => {
                                    let only_motion = !types.is_empty()
                                        && types.iter().all(|input_type| {
                                            native_input_type_is_motion(*input_type)
                                        });
                                    if only_motion {
                                        mouse_motion_packets = mouse_motion_packets.wrapping_add(1);
                                    }
                                    let changed = *types != last_input_types;
                                    if changed {
                                        last_input_types.clone_from(types);
                                    }
                                    changed || !only_motion || mouse_motion_packets % 120 == 1
                                }
                                Err(_) => true,
                            };
                        if should_log {
                            match input_types.as_ref() {
                                Ok(types) => {
                                    let names = types
                                        .iter()
                                        .map(|input_type| {
                                            format!(
                                                "{}({input_type})",
                                                native_input_type_name(*input_type)
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                        .join(",");
                                    eprintln!(
                                        "NVST native input rx: events=[{names}] bytes={} raw={}",
                                        bytes.len(),
                                        diagnostic_hex(&bytes, 128),
                                    );
                                }
                                Err(error) => eprintln!(
                                    "NVST native input rx undecodable: error={error} bytes={} raw={}",
                                    bytes.len(),
                                    diagnostic_hex(&bytes, 128),
                                ),
                            }
                        }
                        let timestamp_us = transport_origin
                            .elapsed()
                            .as_micros()
                            .try_into()
                            .unwrap_or(u64::MAX);
                        match input_codec.encode(&bytes, timestamp_us) {
                            Ok(messages) => {
                                for message in messages {
                                    if should_log {
                                        eprintln!(
                                            "NVST encoded input tx: route={:?} bytes={} raw={}",
                                            message.route,
                                            message.bytes.len(),
                                            diagnostic_hex(&message.bytes, 128),
                                        );
                                    }
                                    if !channels.send_encoded(&mut rtc, &message) {
                                        sent = false;
                                        eprintln!(
                                            "NVST input packet could not be queued on {:?}",
                                            message.route
                                        );
                                    }
                                }
                            }
                            Err(error) => {
                                sent = false;
                                eprintln!("NVST input packet rejected: {error}");
                            }
                        }
                    } else {
                        sent = false;
                    }
                    if let Some(reply) = reply {
                        let _ = reply.send(if sent {
                            Ok(())
                        } else {
                            Err(TransportError::InputNotReady)
                        });
                    }
                }
                Ok(UdpReceiverCommand::Stop) | Err(TryRecvError::Disconnected) => {
                    rtc.disconnect();
                    forward_optional(&event_sender, receiver.stop());
                    return;
                }
                Err(TryRecvError::Empty) => break,
            }
        }

        // Official first burst is three ICE Binding Requests, plus NATT
        // ping-string PING. After DTLS they keep pinging at 100ms.
        let now = Instant::now();
        if input_state.control_is_open() && now >= control_keepalive_at {
            if let Some(channels) = input_channels
                && !channels.send_keepalive(&mut rtc, 0)
            {
                eprintln!("NVST control keepalive could not be queued");
            }
            control_keepalive_at = next_control_keepalive(now);
        }
        if !server_cursor_hidden
            && cursor_capture_attempts < MAX_CURSOR_CAPTURE_ATTEMPTS
            && cursor_capture_retry_at.is_some_and(|retry_at| now >= retry_at)
            && let Some(channels) = input_channels
        {
            cursor_capture_attempts += 1;
            let capture_sent = channels.send_mouse_cursor_capture(&mut rtc, true);
            let tracking_sent = channels.send_remote_cursor_tracking(&mut rtc, true);
            if capture_sent && tracking_sent {
                eprintln!(
                    "NVST cursor capture tx: channel={} command=0x0308 enabled=true reason=post-play-retry attempt={cursor_capture_attempts}",
                    channels.label(channels.control_reliable),
                );
                eprintln!(
                    "NVST remote cursor tracking tx: channel={} command=0x030d enabled=true reason=post-play-retry attempt={cursor_capture_attempts}",
                    channels.label(channels.control_reliable),
                );
            } else {
                eprintln!(
                    "NVST cursor feature retry could not be queued: attempt={cursor_capture_attempts} captureSent={capture_sent} trackingSent={tracking_sent}"
                );
            }
            cursor_capture_retry_at = Some(now + CURSOR_CAPTURE_RETRY_INTERVAL);
        }
        if !input_timeout_reported && input_state.handshake_timed_out(sctp_started_at, now) {
            input_timeout_reported = true;
            let _ = event_sender.send(NvstReceiveEvent::InputUnavailable(
                "input handshake timed out".to_owned(),
            ));
        }
        let ping_interval = if dtls_ready {
            PING_INTERVAL_AFTER_CONNECTION
        } else {
            PING_INTERVAL_BEFORE_CONNECTION
        };
        if now.duration_since(last_hole_punch) >= ping_interval
            && let Some(credentials) = stun_credentials.as_ref()
        {
            let mut ice_bytes = 0_usize;
            if !dtls_ready {
                for _ in 0..3 {
                    let mut ice_tid = [0_u8; 12];
                    if getrandom::fill(&mut ice_tid).is_ok() {
                        let ice = build_stun_binding_request(credentials, &ice_tid);
                        ice_bytes = ice.len();
                        if let Err(error) = socket.send_to(&ice, bundle_peer) {
                            eprintln!("NVST ICE send failed: {error}");
                            forward_optional(&event_sender, receiver.stop());
                            return;
                        }
                    }
                }
            }
            let mut natt_tid = [0_u8; 12];
            let natt = if getrandom::fill(&mut natt_tid).is_ok() {
                let natt = build_natt_hole_punch_request(
                    &credentials.local_username_fragment,
                    &ping_payload,
                    &credentials.remote_password,
                    &natt_tid,
                );
                if let Err(error) = socket.send_to(&natt, bundle_peer) {
                    eprintln!("NVST NATT send failed: {error}");
                    forward_optional(&event_sender, receiver.stop());
                    return;
                }
                Some(natt)
            } else {
                None
            };
            hole_punch_pings += 1;
            if hole_punch_pings == 1 || hole_punch_pings % 50 == 0 {
                eprintln!(
                    "NVST hole-punch ping={hole_punch_pings} dest={bundle_peer} iceBurst={} iceBytes={ice_bytes} nattBytes={} credentials=redacted",
                    if dtls_ready { 0 } else { 3 },
                    natt.as_ref().map_or(0, Vec::len),
                );
            }
            last_hole_punch = now;
        }

        if control_partial_open && let Some(channels) = input_channels {
            while let Some(frame) = feedback.take_completed_frame() {
                if frame.frame_number % FRAMES_PER_PACING_REPORT == 1 {
                    // Packet-completion intervals are intentionally bursty and are not display
                    // pacing error. Feeding that network jitter into the server PID made its
                    // encoder cadence oscillate. Until a real vsync timestamp is available,
                    // report a neutral error exactly as the unavailable ACK stage metrics do.
                    let pacing =
                        frame_pacing_report(frame.frame_number, frame_time_us, 0).encoded();
                    let _ = channels.send_partial_control(&mut rtc, &pacing);
                }
                let client_time_ms = frame
                    .accepted_at
                    .saturating_duration_since(transport_origin)
                    .as_secs_f64()
                    * 1_000.0;
                let ack = frame_ack(
                    frame.frame_number,
                    client_time_ms,
                    frame.bytes,
                    frame_time_us,
                    None,
                )
                .encoded();
                if !channels.send_partial_control(&mut rtc, &ack) {
                    break;
                }
            }
        }

        if control_partial_open
            && now.duration_since(last_qos_send) >= QOS_REPORT_INTERVAL
            && let Some(channels) = input_channels
        {
            let (frames_received, bytes_received, rtp_timestamp) =
                feedback.completed_frame_snapshot();
            qos_sequence = qos_sequence.wrapping_add(1);
            let command = QosReport {
                sequence: qos_sequence,
                frames_received,
                bytes_received,
                rtp_timestamp,
                previous_bytes_received: bytes_received,
                warmed_up: now.saturating_duration_since(transport_origin) >= QOS_WARM_UP,
            }
            .command()
            .encoded();
            let _ = channels.send_partial_control(&mut rtc, &command);
            last_qos_send = now;
        }

        // Send RTCP feedback over the rtcp1 SCTP channel once it is open and the
        // Mjolnir receiver has bound the video stream. A Receiver Report goes out
        // every second.
        if rtcp_channel_open
            && now.duration_since(last_rtcp_send) >= SRTCP_RR_INTERVAL
            && let Some(channel_id) = rtcp_channel
            && let Some(report_block) = feedback.report_snapshot(true)
        {
            let mut channel = rtc.channel(channel_id);
            if let Some(channel) = channel.as_mut() {
                let report = build_rtcp_receiver_report(rtcp_sender_ssrc, report_block);
                if channel.write(true, &report).unwrap_or(false) {
                    rtcp_reports_sent += 1;
                    if rtcp_reports_sent == 1 || rtcp_reports_sent % 10 == 0 {
                        eprintln!(
                            "NVST rtcp1 RR sent={rtcp_reports_sent} mediaSsrc={} highestSeq={}",
                            report_block.media_ssrc, report_block.highest_sequence,
                        );
                    }
                }
            }
            last_rtcp_send = now;
        }

        // Loss feedback cannot wait for the one-second Receiver Report cadence:
        // request retransmission while the reorder buffer still holds later
        // packets, then request a keyframe if bounded recovery was exhausted.
        if now.duration_since(last_recovery_send) >= RTCP_RECOVERY_INTERVAL
            && let Some((media_ssrc, _)) = feedback.stream_snapshot()
        {
            if rtcp_channel_open
                && let Some(channel_id) = rtcp_channel
                && let Some(mut channel) = rtc.channel(channel_id)
            {
                if let Some((first_missing_index, last_missing_index)) = feedback.take_nack() {
                    let nack = build_rtcp_nack(
                        rtcp_sender_ssrc,
                        media_ssrc,
                        first_missing_index,
                        last_missing_index,
                    );
                    if channel.write(true, &nack).unwrap_or(false) {
                        eprintln!(
                            "NVST rtcp1 NACK sent for mediaSsrc={media_ssrc} missing={first_missing_index}..={last_missing_index}"
                        );
                    } else {
                        feedback.request_nack(first_missing_index, last_missing_index);
                    }
                }
            }
            if feedback.take_keyframe_request() {
                if now.duration_since(last_keyframe_send) < KEYFRAME_REQUEST_COOLDOWN {
                    feedback.request_keyframe();
                } else {
                    let mut sent = false;
                    if rtcp_channel_open
                        && let Some(channel_id) = rtcp_channel
                        && let Some(mut channel) = rtc.channel(channel_id)
                    {
                        let pli = build_rtcp_pli(rtcp_sender_ssrc, media_ssrc);
                        if channel.write(true, &pli).unwrap_or(false) {
                            eprintln!("NVST rtcp1 PLI sent for mediaSsrc={media_ssrc}");
                            sent = true;
                        }
                    }
                    if input_state.control_is_open()
                        && let Some(channels) = input_channels
                        && channels.send_control(&mut rtc, &idr_request().encoded())
                    {
                        eprintln!("NVST control 0x302 IDR request sent");
                        sent = true;
                    }
                    if !sent {
                        feedback.request_keyframe();
                    } else {
                        last_keyframe_send = now;
                    }
                }
            }
            last_recovery_send = now;
        }

        let timeout = loop {
            match rtc.poll_output() {
                Ok(Output::Timeout(timeout)) => break timeout,
                Ok(Output::Transmit(transmit)) => {
                    outbound_datagrams += 1;
                    let kind = if looks_like_stun(&transmit.contents) {
                        "stun"
                    } else if looks_like_dtls(&transmit.contents) {
                        "dtls"
                    } else {
                        "other"
                    };
                    if outbound_datagrams <= 8
                        || (verbose_diagnostics_enabled() && outbound_datagrams % 50 == 0)
                    {
                        eprintln!(
                            "NVST WebRTC outbound={outbound_datagrams} kind={kind} dest={} bytes={}",
                            bundle_peer,
                            transmit.contents.len()
                        );
                    }
                    if let Err(error) = socket.send_to(&transmit.contents, bundle_peer) {
                        eprintln!("NVST WebRTC send failed: {error}");
                        forward_optional(&event_sender, receiver.stop());
                        return;
                    }
                    // Official ICE-on WebRtcTransport skips setupDtls until a real
                    // inbound STUN. Do not synthesize Binding Success — that only
                    // unblocks str0m and sends ClientHello before GFN has a pair.
                }
                Ok(Output::Event(event)) => match event {
                    Event::IceConnectionStateChange(state) => {
                        eprintln!("NVST ICE state: {state:?}");
                        // Official GFN treats hole-punch / ICE receive failure as
                        // non-fatal. Media is gated on DTLS, not ICE success.
                    }
                    Event::Connected => {
                        dtls_ready = true;
                        let _ = event_sender.send(NvstReceiveEvent::TransportReady("dtls"));
                        eprintln!("NVST DTLS handshake complete; waiting for SRTP/Mjolnir");
                        if !sctp_started {
                            sctp_started = true;
                            sctp_started_at = Some(Instant::now());
                            rtc.direct_api().start_sctp(true);
                            let channels = NvstInputChannels::create(&mut rtc);
                            rtcp_channel = Some(channels.rtcp);
                            input_channels = Some(channels);
                            let _ = event_sender.send(NvstReceiveEvent::TransportReady("sctp"));
                            eprintln!("NVST SCTP started with the eight-channel Bifrost profile");
                        }
                    }
                    Event::ChannelOpen(id, label) => {
                        eprintln!("NVST data channel open: id={id:?} label={label}");
                        if let Some(channels) = input_channels {
                            if id == channels.control_reliable {
                                if !channels.send_keepalive(&mut rtc, 0) {
                                    eprintln!("NVST initial control keepalive could not be queued");
                                }
                                control_keepalive_at = next_control_keepalive(Instant::now());
                            }
                            if id == channels.control_partial {
                                control_partial_open = true;
                            }
                            if let Some(version) = input_state.channel_opened(channels, id) {
                                if !finish_nvst_input_handshake(
                                    &mut input_state,
                                    channels,
                                    &mut rtc,
                                    transport_origin,
                                    &input_ready,
                                    &event_sender,
                                    version,
                                ) {
                                    continue;
                                }
                                cursor_capture_attempts = 1;
                                cursor_capture_retry_at =
                                    Some(Instant::now() + CURSOR_CAPTURE_RETRY_INTERVAL);
                            }
                        }
                        if Some(id) == rtcp_channel {
                            rtcp_channel_open = true;
                            // Ask for a keyframe immediately so the decoder can start.
                            feedback.request_keyframe();
                        }
                    }
                    Event::ChannelData(data) => {
                        if let Some(channels) = input_channels
                            && channels.contains(data.id)
                        {
                            let label = channels.label(data.id);
                            let cursor_messages = server_cursor_messages(&data.data);
                            for message in cursor_messages {
                                eprintln!(
                                    "NVST cursor wire rx: channel={label} id={:?} command=0x{:04x} offset={} cursorId={:?} position={:?} visible={:?} bytes={} raw={}",
                                    data.id,
                                    message.command,
                                    message.offset,
                                    message.cursor_id,
                                    message.position,
                                    message.visible,
                                    message.raw.len(),
                                    diagnostic_hex(&message.raw, 512),
                                );
                                if let Some(cursor) = message.normalized {
                                    cursor_capture_retry_at = None;
                                    if !server_cursor_hidden
                                        && channels.send_mouse_cursor_capture(&mut rtc, false)
                                    {
                                        server_cursor_hidden = true;
                                        eprintln!(
                                            "NVST cursor capture tx: channel={} command=0x0308 enabled=false reason=first-local-cursor",
                                            channels.label(channels.control_reliable),
                                        );
                                    }
                                    eprintln!(
                                        "NVST cursor dispatch: source={label} type={} id={} bytes={} raw={}",
                                        cursor[0],
                                        cursor[1],
                                        cursor.len(),
                                        diagnostic_hex(&cursor, 512),
                                    );
                                    let _ = event_sender.send(NvstReceiveEvent::Cursor(cursor));
                                }
                            }

                            if data.id == channels.cursor {
                                cursor_capture_retry_at = None;
                                if !server_cursor_hidden
                                    && channels.send_mouse_cursor_capture(&mut rtc, false)
                                {
                                    server_cursor_hidden = true;
                                    eprintln!(
                                        "NVST cursor capture tx: channel={} command=0x0308 enabled=false reason=cursor-channel",
                                        channels.label(channels.control_reliable),
                                    );
                                }
                                eprintln!(
                                    "NVST cursor-channel raw rx: id={:?} bytes={} type={:?} cursorId={:?} raw={}",
                                    data.id,
                                    data.data.len(),
                                    data.data.first(),
                                    data.data.get(1),
                                    diagnostic_hex(&data.data, 512),
                                );
                                let _ =
                                    event_sender.send(NvstReceiveEvent::Cursor(data.data.to_vec()));
                                continue;
                            }

                            if verbose_diagnostics_enabled() {
                                eprintln!(
                                    "NVST data channel rx: channel={label} id={:?} bytes={} raw={}",
                                    data.id,
                                    data.data.len(),
                                    diagnostic_hex(&data.data, 128),
                                );
                            }
                        }
                        if let Some(channels) = input_channels
                            && let Some(version) =
                                input_state.channel_data(channels, data.id, &data.data)
                        {
                            if !finish_nvst_input_handshake(
                                &mut input_state,
                                channels,
                                &mut rtc,
                                transport_origin,
                                &input_ready,
                                &event_sender,
                                version,
                            ) {
                                continue;
                            }
                            cursor_capture_attempts = 1;
                            cursor_capture_retry_at =
                                Some(Instant::now() + CURSOR_CAPTURE_RETRY_INTERVAL);
                        } else if Some(data.id) == rtcp_channel {
                            eprintln!(
                                "NVST rtcp1 inbound: id={:?} binary={} bytes={}",
                                data.id,
                                data.binary,
                                data.data.len(),
                            );
                        }
                    }
                    Event::ChannelClose(id) => {
                        if let Some(channels) = input_channels {
                            if id == channels.control_reliable {
                                server_cursor_hidden = false;
                                cursor_capture_retry_at = None;
                                cursor_capture_attempts = 0;
                            }
                            let input_channel_closed = id == channels.input_partial;
                            if input_state.channel_closed(channels, id) || input_channel_closed {
                                input_ready.store(false, Ordering::Release);
                                let reason = if input_channel_closed {
                                    "partially reliable input data channel closed"
                                } else {
                                    "reliable input data channel closed"
                                };
                                let _ = event_sender
                                    .send(NvstReceiveEvent::InputUnavailable(reason.to_owned()));
                            }
                        }
                        if input_channels.is_some_and(|channels| id == channels.control_partial) {
                            control_partial_open = false;
                        }
                        if Some(id) == rtcp_channel {
                            rtcp_channel_open = false;
                            rtcp_channel = None;
                        }
                    }
                    Event::RtpPacket(packet) => {
                        let outer_payload_type = *packet.header.payload_type;
                        let is_audio = audio_track.as_ref().is_some_and(|audio| {
                            matches_audio_track(audio, outer_payload_type, *packet.header.ssrc)
                        });
                        if is_audio {
                            let audio = audio_track.as_ref().expect("audio track checked above");
                            let payload = if outer_payload_type == GFN_RED_PAYLOAD_TYPE {
                                let Some(primary) = extract_red_opus_primary(&packet.payload)
                                else {
                                    let _ = event_sender.send(NvstReceiveEvent::Dropped(
                                        NvstDropReason::MalformedRedAudio,
                                    ));
                                    continue;
                                };
                                Arc::from(primary)
                            } else {
                                packet.payload
                            };
                            let audio_ssrc = *packet.header.ssrc;
                            let contiguous = last_audio_sequence.is_none_or(|(ssrc, previous)| {
                                ssrc != audio_ssrc
                                    || previous.wrapping_add(1) == packet.header.sequence_number
                            });
                            last_audio_sequence = Some((audio_ssrc, packet.header.sequence_number));
                            let received_at_us = packet
                                .timestamp
                                .saturating_duration_since(transport_origin)
                                .as_micros()
                                .try_into()
                                .unwrap_or(u64::MAX);
                            let frame = EncodedMediaFrame {
                                mid: audio.mid.clone(),
                                codec: "opus".to_owned(),
                                payload,
                                rtp_timestamp: u64::from(packet.header.timestamp),
                                clock_rate_hz: audio.clock_rate_hz,
                                channels: Some(audio.channels),
                                received_at_us,
                                keyframe: false,
                                contiguous,
                            };
                            if let Err(error) = deliver_media_frame(&media_consumer, frame) {
                                if matches!(error, TransportError::MediaConsumerBackpressured) {
                                    // Audio is real-time data. If the decoder briefly falls
                                    // behind, discard this packet and keep the transport alive;
                                    // queued audio is more harmful than a short concealment gap.
                                    continue;
                                }
                                let _ = event_sender.send(NvstReceiveEvent::Dropped(
                                    NvstDropReason::MediaConsumerClosed,
                                ));
                                rtc.disconnect();
                                forward_optional(&event_sender, receiver.stop());
                                return;
                            }
                        } else {
                            for event in receiver.process_mjolnir_payload(
                                *packet.header.ssrc,
                                packet.header.timestamp,
                                &packet.payload,
                                Instant::now(),
                            ) {
                                if !forward_receive_event(
                                    &media_consumer,
                                    &event_sender,
                                    transport_origin,
                                    event,
                                ) {
                                    rtc.disconnect();
                                    forward_optional(&event_sender, receiver.stop());
                                    return;
                                }
                            }
                        }
                    }
                    _ => {}
                },
                Err(error) => {
                    eprintln!("NVST WebRTC bundle failed: {error}");
                    forward_optional(&event_sender, receiver.stop());
                    return;
                }
            }
        };

        let wait = timeout
            .saturating_duration_since(Instant::now())
            .min(CONTROL_RECEIVE_POLL_INTERVAL);
        if wait.is_zero() {
            if let Err(error) = rtc.handle_input(Input::Timeout(Instant::now())) {
                eprintln!("NVST WebRTC timer failed: {error}");
                forward_optional(&event_sender, receiver.stop());
                return;
            }
        } else {
            if let Err(error) = socket.set_read_timeout(Some(wait)) {
                eprintln!("NVST UDP timeout configuration failed: {error}");
                forward_optional(&event_sender, receiver.stop());
                return;
            }
            match socket.recv_from(&mut datagram) {
                Ok((length, source)) => {
                    inbound_datagrams += 1;
                    if inbound_datagrams == 1
                        || (verbose_diagnostics_enabled() && inbound_datagrams % 50 == 0)
                    {
                        eprintln!(
                            "NVST WebRTC inbound={inbound_datagrams} source={source} bytes={length} dtlsReady={dtls_ready}"
                        );
                    }
                    if source != bundle_peer {
                        continue;
                    }
                    if datagram[..length] == *b"PING" {
                        if let Err(error) = socket.send_to(b"PONG", source) {
                            eprintln!("NVST PONG send failed: {error}");
                            forward_optional(&event_sender, receiver.stop());
                            return;
                        }
                        continue;
                    }
                    if looks_like_rtp(&datagram[..length])
                        && let Some(ssrc) = peek_rtp_ssrc(&datagram[..length])
                        && seen_ssrcs.insert(ssrc)
                    {
                        let mid = audio_track
                            .as_ref()
                            .filter(|audio| {
                                peek_rtp_payload_type(&datagram[..length]).is_some_and(
                                    |payload_type| matches_audio_track(audio, payload_type, ssrc),
                                )
                            })
                            .map_or_else(|| Mid::from("0"), |audio| Mid::from(audio.mid.as_str()));
                        rtc.direct_api()
                            .expect_stream_rx(Ssrc::from(ssrc), None, mid, None);
                        eprintln!("NVST expecting SSRC {ssrc} on bundle mid={mid}");
                    }
                    let destination = receive_destination;
                    let contents = match datagram[..length].try_into() {
                        Ok(value) => value,
                        Err(error) => {
                            eprintln!("NVST dropping oversized UDP packet: {error}");
                            continue;
                        }
                    };
                    if let Err(error) = rtc.handle_input(Input::Receive(
                        Instant::now(),
                        Receive {
                            proto: RtcProtocol::Udp,
                            source: receive_source,
                            destination,
                            contents,
                        },
                    )) {
                        eprintln!("NVST WebRTC handle_input failed: {error}");
                        forward_optional(&event_sender, receiver.stop());
                        return;
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    let _ = rtc.handle_input(Input::Timeout(Instant::now()));
                }
                Err(_) => {
                    forward_optional(&event_sender, receiver.stop());
                    return;
                }
            }
        }

        let timeout = if owns_media_timeout {
            receiver.poll_timeout(Instant::now())
        } else {
            None
        };
        if timeout.is_some() {
            eprintln!(
                "NVST WebRTC timeout counters: inbound={inbound_datagrams}, dtlsReady={dtls_ready}"
            );
        }
        forward_optional(&event_sender, timeout);
    }
}

fn run_nvst_udp_receiver(
    socket: UdpSocket,
    config: NvstVideoConfig,
    commands: Receiver<UdpReceiverCommand>,
    outputs: NvstReceiverOutputs,
    transport_origin: Instant,
    rtc: Option<Rtc>,
) {
    if let Some(rtc) = rtc {
        run_nvst_webrtc_bundle(socket, config, commands, outputs, transport_origin, rtc);
        return;
    }
    let NvstReceiverOutputs {
        media_consumer,
        event_sender,
        ..
    } = outputs;
    let mut receiver = NvstVideoReceiver::new(config);
    let stun_credentials = receiver.config.stun_credentials.clone();
    let mut datagram = vec![0_u8; 65_536];
    let mut peer_seen = false;
    let mut last_ping = Instant::now() - PING_INTERVAL_BEFORE_CONNECTION;
    let mut pings_sent = 0_u64;
    let mut inbound_datagrams = 0_u64;
    let mut handled_stun = 0_u64;
    let mut invalid_stun = 0_u64;
    let mut non_stun = 0_u64;
    let mut wrong_source = 0_u64;
    let mut receiver_reports_sent = 0_u64;
    let stats_origin = Instant::now();
    let mut last_stats_log = Instant::now();
    loop {
        loop {
            match commands.try_recv() {
                Ok(UdpReceiverCommand::Pause) => forward_optional(&event_sender, receiver.pause()),
                Ok(UdpReceiverCommand::Resume) => {
                    forward_optional(&event_sender, receiver.resume())
                }
                Ok(UdpReceiverCommand::Recover) => {
                    forward_optional(&event_sender, receiver.recover())
                }
                Ok(UdpReceiverCommand::SendInput { reply, .. }) => {
                    if let Some(reply) = reply {
                        let _ = reply.send(Err(TransportError::InputNotReady));
                    }
                }
                Ok(UdpReceiverCommand::Stop) | Err(TryRecvError::Disconnected) => {
                    forward_optional(&event_sender, receiver.stop());
                    return;
                }
                Err(TryRecvError::Empty) => break,
            }
        }

        let now = Instant::now();
        let ping_interval = if peer_seen {
            PING_INTERVAL_AFTER_CONNECTION
        } else {
            PING_INTERVAL_BEFORE_CONNECTION
        };
        if now.duration_since(last_ping) >= ping_interval {
            if let Some(credentials) = stun_credentials.as_ref() {
                // Ping version 6 is an authenticated STUN Binding request. The official
                // NattHolePunch::SendPing path does not prepend a legacy raw `PING`; doing so
                // can make the relay retain its legacy route instead of publishing Mjolnir.
                let mut transaction_id = [0_u8; 12];
                if let Err(error) = getrandom::fill(&mut transaction_id) {
                    eprintln!("NVST NATT transaction generation failed: {error}");
                    forward_optional(&event_sender, receiver.stop());
                    return;
                }
                let ping = build_natt_hole_punch_request(
                    &credentials.local_username_fragment,
                    &receiver.config.ping_payload,
                    &credentials.remote_password,
                    &transaction_id,
                );
                if let Err(error) = socket.send_to(&ping, receiver.config.video_peer) {
                    eprintln!("NVST NATT send failed: {error}");
                    forward_optional(&event_sender, receiver.stop());
                    return;
                }
                pings_sent += 1;
            } else {
                if let Err(error) =
                    socket.send_to(&receiver.config.ping_payload, receiver.config.video_peer)
                {
                    eprintln!("NVST ping send failed: {error}");
                    forward_optional(&event_sender, receiver.stop());
                    return;
                }
                pings_sent += 1;
            }
            last_ping = now;
        }

        match socket.recv_from(&mut datagram) {
            Ok((length, source)) => {
                inbound_datagrams += 1;
                if inbound_datagrams == 1 {
                    eprintln!(
                        "NVST raw-SRTP inbound first datagram: source={source} bytes={length} peer={}",
                        receiver.config.video_peer
                    );
                }
                if source != receiver.config.video_peer {
                    wrong_source += 1;
                }
                if source == receiver.config.video_peer
                    && let Some(credentials) = stun_credentials.as_ref()
                {
                    match handle_stun_datagram(&datagram[..length], source, credentials) {
                        StunDatagram::Handled(response) => {
                            handled_stun += 1;
                            peer_seen = true;
                            if let Some(response) = response
                                && let Err(error) = socket.send_to(&response, source)
                            {
                                eprintln!("NVST STUN response send failed: {error}");
                                forward_optional(&event_sender, receiver.stop());
                                return;
                            }
                            continue;
                        }
                        StunDatagram::Invalid => {
                            invalid_stun += 1;
                            continue;
                        }
                        StunDatagram::NotStun => non_stun += 1,
                    }
                }
                let events = receiver.process_datagram(source, &datagram[..length], Instant::now());
                for event in events {
                    if !forward_receive_event(
                        &media_consumer,
                        &event_sender,
                        transport_origin,
                        event,
                    ) {
                        forward_optional(&event_sender, receiver.stop());
                        return;
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => {
                forward_optional(&event_sender, receiver.stop());
                return;
            }
        }
        let now = Instant::now();
        if let Some(report) = receiver.poll_receiver_report(now) {
            if let Err(error) = socket.send_to(&report, receiver.config.video_peer) {
                eprintln!("NVST receiver report send failed: {error}");
                forward_optional(&event_sender, receiver.stop());
                return;
            }
            receiver_reports_sent += 1;
        }
        if now.duration_since(last_stats_log) >= Duration::from_secs(2) {
            last_stats_log = now;
            eprintln!(
                "NVST rx-stats {} inbound={inbound_datagrams} pings={pings_sent} rr={receiver_reports_sent}",
                receiver.stats_line(stats_origin),
            );
        }
        let timeout = receiver.poll_timeout(now);
        if timeout.is_some() {
            eprintln!(
                "NVST transport timeout counters: pings={pings_sent}, inbound={inbound_datagrams}, stunHandled={handled_stun}, stunInvalid={invalid_stun}, nonStun={non_stun}, wrongSource={wrong_source}"
            );
        }
        forward_optional(&event_sender, timeout);
    }
}

fn forward_optional(sender: &Sender<NvstReceiveEvent>, event: Option<NvstReceiveEvent>) {
    if let Some(event) = event {
        let _ = sender.send(event);
    }
}

fn forward_receive_event(
    media_consumer: &MediaConsumer,
    event_sender: &Sender<NvstReceiveEvent>,
    transport_origin: Instant,
    event: NvstReceiveEvent,
) -> bool {
    if let NvstReceiveEvent::Frame(frame) = event {
        let media_frame = EncodedMediaFrame {
            mid: "nvst-video-0".to_owned(),
            codec: frame.codec.label().to_owned(),
            payload: Arc::from(frame.bytes),
            rtp_timestamp: u64::from(frame.timestamp),
            clock_rate_hz: 90_000,
            channels: None,
            received_at_us: transport_origin
                .elapsed()
                .as_micros()
                .try_into()
                .unwrap_or(u64::MAX),
            keyframe: frame.keyframe,
            contiguous: frame.contiguous,
        };
        let (reason, keep_running) = match deliver_media_frame(media_consumer, media_frame) {
            Ok(()) => return true,
            Err(TransportError::MediaConsumerBackpressured) => {
                (NvstDropReason::MediaConsumerBackpressured, true)
            }
            Err(TransportError::MediaConsumerClosed) => {
                (NvstDropReason::MediaConsumerClosed, false)
            }
            Err(_) => (NvstDropReason::MediaConsumerClosed, false),
        };
        let _ = event_sender.send(NvstReceiveEvent::Dropped(reason));
        return keep_running;
    }
    let _ = event_sender.send(event);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TEST_KEY: &str = "000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F";
    const TEST_SALT: &str = "000102030405060708090A0B0C0D";
    const TEST_PEER: &str = "192.0.2.20";

    #[test]
    fn transient_video_consumer_backpressure_does_not_stop_receiver() {
        let (media_consumer, _media_receiver) = std::sync::mpsc::sync_channel(1);
        let (event_sender, event_receiver) = std::sync::mpsc::channel();
        let frame = || {
            NvstReceiveEvent::Frame(EncodedVideoAccessUnit {
                codec: NvstVideoCodec::H265,
                timestamp: 90_000,
                frame_index: 1,
                first_stream_packet_index: 1,
                keyframe: true,
                contiguous: true,
                bytes: vec![0, 0, 0, 1, 0x26],
            })
        };

        assert!(forward_receive_event(
            &media_consumer,
            &event_sender,
            Instant::now(),
            frame(),
        ));
        assert!(forward_receive_event(
            &media_consumer,
            &event_sender,
            Instant::now(),
            frame(),
        ));
        assert!(matches!(
            event_receiver.recv().expect("backpressure event"),
            NvstReceiveEvent::Dropped(NvstDropReason::MediaConsumerBackpressured)
        ));
    }

    fn legacy_handoff() -> Value {
        json!({
            "clientUdpPort": 49005,
            "videoPeerIp": TEST_PEER,
            "videoPeerPort": 5004,
            "srtpAesKeyHex": TEST_KEY,
            "srtpSaltHex": "00000000000000009ECA935E",
            "codec": "H264",
            "rtpPayloadType": 96,
            "rtpSsrc": 0x11223344u32,
            "reorderWindowPackets": 4,
            "maxAccessUnitBytes": 4096,
            "timeoutMs": 500
        })
    }

    fn config() -> NvstVideoConfig {
        NvstVideoConfig::from_legacy_handoff(&legacy_handoff(), None).expect("valid config")
    }

    fn peer() -> SocketAddr {
        SocketAddr::new(TEST_PEER.parse().expect("test IP"), 5004)
    }

    fn stun_credentials() -> NvstStunCredentials {
        NvstStunCredentials {
            local_username_fragment: "loc1".to_owned(),
            local_password: "local-password-value-01".to_owned(),
            remote_username_fragment: "remote01".to_owned(),
            remote_password: "remote-password-with-36-byte-value-001".to_owned(),
        }
    }

    fn build_plaintext_rtp_with_fec_blocks(
        sequence: u16,
        flags: u8,
        frame_index: u32,
        fec_current_block: u8,
        fec_last_block: u8,
        media: &[u8],
    ) -> Vec<u8> {
        let mut packet = vec![0x90, 0xe0];
        packet.extend_from_slice(&sequence.to_be_bytes());
        packet.extend_from_slice(&0x01020304u32.to_be_bytes());
        packet.extend_from_slice(&0x11223344u32.to_be_bytes());
        packet.extend_from_slice(&GS_VIDEO_EXTENSION_PROFILE.to_be_bytes());
        packet.extend_from_slice(&4_u16.to_be_bytes());
        packet.extend_from_slice(&(u32::from(sequence) << 8).to_le_bytes());
        packet.extend_from_slice(&frame_index.to_le_bytes());
        packet.push(flags);
        packet.extend_from_slice(&[
            0,
            0,
            (fec_current_block & 0x03) << 4 | (fec_last_block & 0x03) << 6,
        ]);
        packet.extend_from_slice(&[0; 4]);
        packet.extend_from_slice(media);
        packet
    }

    fn build_plaintext_rtp(sequence: u16, flags: u8, frame_index: u32, media: &[u8]) -> Vec<u8> {
        build_plaintext_rtp_with_fec_blocks(sequence, flags, frame_index, 0, 0, media)
    }

    fn test_srtp(config: &NvstVideoConfig) -> SrtpReceiver {
        SrtpReceiver::from_material(&config.srtp)
    }

    fn protect_for_test(crypto: &SrtpReceiver, mut packet: Vec<u8>, roc: u32) -> Vec<u8> {
        let header = RtpHeader::parse(&packet).expect("RTP header");
        let packet_index = (u64::from(roc) << 16) | u64::from(header.sequence_number);
        match &crypto.cipher {
            SrtpCipher::AeadAes128Gcm {
                encryption_key,
                session_salt,
                authentication_tag_len,
            } => {
                let iv = srtp_gcm_iv(*session_salt, header.ssrc, roc, header.sequence_number);
                protect_aes_gcm(
                    &mut packet,
                    header.payload_offset,
                    encryption_key,
                    &iv,
                    *authentication_tag_len,
                );
            }
            SrtpCipher::AeadAes256Gcm {
                encryption_key,
                session_salt,
                authentication_tag_len,
            } => {
                let iv = srtp_gcm_iv(*session_salt, header.ssrc, roc, header.sequence_number);
                protect_aes_gcm(
                    &mut packet,
                    header.payload_offset,
                    encryption_key,
                    &iv,
                    *authentication_tag_len,
                );
            }
            SrtpCipher::AesCm128HmacSha1 {
                encryption_key,
                authentication_key,
                session_salt,
                authentication_tag_len,
            } => {
                let iv = srtp_aes_cm_iv(session_salt, header.ssrc, packet_index);
                let mut cipher = Aes128Ctr::new(encryption_key.into(), (&iv).into());
                cipher.apply_keystream(&mut packet[header.payload_offset..]);
                let mut mac = HmacSha1::new_from_slice(authentication_key).expect("fixed key");
                mac.update(&packet);
                mac.update(&roc.to_be_bytes());
                packet.extend_from_slice(&mac.finalize().into_bytes()[..*authentication_tag_len]);
            }
            SrtpCipher::AesCm256HmacSha1 {
                encryption_key,
                authentication_key,
                session_salt,
                authentication_tag_len,
            } => {
                let iv = srtp_aes_cm_iv(session_salt, header.ssrc, packet_index);
                let mut cipher = Aes256Ctr::new(encryption_key.into(), (&iv).into());
                cipher.apply_keystream(&mut packet[header.payload_offset..]);
                let mut mac = HmacSha1::new_from_slice(authentication_key).expect("fixed key");
                mac.update(&packet);
                mac.update(&roc.to_be_bytes());
                packet.extend_from_slice(&mac.finalize().into_bytes()[..*authentication_tag_len]);
            }
        }
        packet
    }

    #[test]
    fn legacy_schema_defaults_to_aes_256_gcm_8_with_explicit_salt() {
        let config = config();
        assert_eq!(config.video_peer(), peer());
        assert_eq!(config.bundle_peer(), peer());
        assert_eq!(config.srtp_profile(), NvstSrtpProfile::AeadAes256Gcm8);
        let NvstSrtpMaterial::AeadAes256Gcm { master_salt, .. } = config.srtp else {
            panic!("legacy handoff must choose AES-256-GCM");
        };
        assert_eq!(
            master_salt,
            decode_fixed_hex::<12>("00000000000000009ECA935E", NvstConfigError::InvalidSrtpSalt)
                .expect("salt"),
        );
    }

    #[test]
    fn handoff_uses_distinct_routable_bundle_peer_when_supplied() {
        let mut handoff = legacy_handoff();
        handoff["videoPeerIp"] = json!("169.254.0.21");
        handoff["bundlePeerIp"] = json!("198.51.100.20");
        handoff["bundlePeerPort"] = json!(5006);
        let config = NvstVideoConfig::from_legacy_handoff(&handoff, None).expect("valid peers");
        assert_eq!(config.video_peer(), "169.254.0.21:5004".parse().unwrap());
        assert_eq!(config.bundle_peer(), "198.51.100.20:5006".parse().unwrap());
    }

    #[test]
    fn handoff_accepts_only_explicit_standard_opus_track_metadata() {
        let mut handoff = legacy_handoff();
        handoff["audioTrack"] = json!({
            "payloadType": 97,
            "codec": "opus",
            "clockRateHz": 48_000,
            "channels": 2,
            "mid": "audio-main",
            "ssrc": 424242,
        });
        let config =
            NvstVideoConfig::from_legacy_handoff(&handoff, None).expect("standard Opus track");
        let audio = config.audio_track().expect("audio track");
        assert_eq!(audio.payload_type, 97);
        assert_eq!(audio.clock_rate_hz, 48_000);
        assert_eq!(audio.channels, 2);
        assert_eq!(audio.mid, "audio-main");
        assert!(matches_audio_track(audio, 97, 424242));
        assert!(!matches_audio_track(audio, 96, 424242));
        assert!(!matches_audio_track(audio, 97, 7));

        handoff["audioTrack"]["codec"] = json!("proprietary");
        assert!(matches!(
            NvstVideoConfig::from_legacy_handoff(&handoff, None),
            Err(NvstConfigError::InvalidAudioTrack)
        ));
    }

    #[test]
    fn red_pt63_routes_to_negotiated_opus_pt111_and_extracts_the_primary_block() {
        let track = NvstAudioTrack {
            payload_type: GFN_OPUS_PAYLOAD_TYPE,
            clock_rate_hz: 48_000,
            channels: 2,
            mid: "audio".to_owned(),
            ssrc: Some(42),
        };
        assert!(matches_audio_track(&track, GFN_OPUS_PAYLOAD_TYPE, 42));
        assert!(matches_audio_track(&track, GFN_RED_PAYLOAD_TYPE, 42));
        assert!(!matches_audio_track(&track, GFN_RED_PAYLOAD_TYPE, 7));

        let red_payload = [
            0x80 | GFN_OPUS_PAYLOAD_TYPE,
            0x00,
            0x00,
            0x03,
            GFN_OPUS_PAYLOAD_TYPE,
            0x11,
            0x22,
            0x33,
            0xf8,
            0xff,
        ];
        assert_eq!(
            extract_red_opus_primary(&red_payload),
            Some(&[0xf8, 0xff][..])
        );
    }

    #[test]
    fn red_primary_extraction_rejects_malformed_or_unbounded_payloads() {
        assert_eq!(extract_red_opus_primary(&[]), None);
        assert_eq!(extract_red_opus_primary(&[110, 0xf8]), None);
        assert_eq!(extract_red_opus_primary(&[GFN_OPUS_PAYLOAD_TYPE]), None);

        let mut too_many_blocks = Vec::new();
        for _ in 0..=MAX_REDUNDANT_AUDIO_BLOCKS {
            too_many_blocks.extend_from_slice(&[0x80 | GFN_OPUS_PAYLOAD_TYPE, 0, 0, 0]);
        }
        too_many_blocks.extend_from_slice(&[GFN_OPUS_PAYLOAD_TYPE, 0xf8]);
        assert_eq!(extract_red_opus_primary(&too_many_blocks), None);

        let mut oversized_primary = vec![GFN_OPUS_PAYLOAD_TYPE];
        oversized_primary.resize(MAX_OPUS_PACKET_BYTES + 2, 0xf8);
        assert_eq!(extract_red_opus_primary(&oversized_primary), None);
    }

    #[test]
    fn ping_version_six_requires_all_ice_credentials() {
        let mut handoff = legacy_handoff();
        handoff["pingVersion"] = json!(6);
        assert!(matches!(
            NvstVideoConfig::from_legacy_handoff(&handoff, None),
            Err(NvstConfigError::MissingField("localIceUsernameFragment"))
        ));

        handoff["localIceUsernameFragment"] = json!("loc1");
        handoff["localIcePassword"] = json!("local-password-value-01");
        handoff["remoteIceUsernameFragment"] = json!("remote01");
        handoff["remoteIcePassword"] = json!("remote-password-with-36-byte-value-001");
        let config = NvstVideoConfig::from_legacy_handoff(&handoff, None)
            .expect("complete version-six credentials");
        assert_eq!(config.ping_version, Some(6));
        assert!(config.stun_credentials.is_some());
        assert!(!format!("{config:?}").contains("local-password-value-01"));
    }

    #[test]
    fn reserved_bundle_socket_binds_unspecified_ipv4() {
        let socket = reserve_nvst_udp_socket().expect("reserve");
        let addr = socket.local_addr().expect("local");
        assert!(
            addr.ip().is_unspecified(),
            "official binds 0.0.0.0, advertised NIC IPv4 is separate"
        );
        assert_ne!(addr.port(), 0);
    }

    #[test]
    fn reserved_nvst_pair_places_bundle_immediately_after_video() {
        let (bundle, video) = reserve_nvst_socket_pair().expect("reserve pair");
        let bundle_addr = bundle.local_addr().expect("bundle address");
        let video_addr = video.local_addr().expect("video address");

        assert!(bundle_addr.ip().is_unspecified());
        assert!(video_addr.ip().is_unspecified());
        assert_eq!(bundle_addr.port(), video_addr.port() + 1);
    }

    #[test]
    fn link_local_interfaces_use_distinct_logical_ice_addresses() {
        let local: SocketAddr = "169.254.0.21:49000".parse().unwrap();
        let remote: SocketAddr = "169.254.0.22:5006".parse().unwrap();
        assert_eq!(
            logical_ice_addr(local, 1),
            "127.0.0.1:49000".parse().unwrap()
        );
        assert_eq!(
            logical_ice_addr(remote, 2),
            "127.0.0.2:5006".parse().unwrap()
        );
        assert!(Candidate::host(logical_ice_addr(local, 1), "udp").is_ok());
        assert!(Candidate::host(logical_ice_addr(remote, 2), "udp").is_ok());
    }

    #[test]
    fn natt_hole_punch_uses_setup_ping_payload_not_v2_ufrag() {
        let credentials = stun_credentials();
        let packet = build_natt_hole_punch_request(
            &credentials.local_username_fragment,
            b"srv1",
            &credentials.remote_password,
            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        );
        let (_, username) = find_stun_attribute(&packet, STUN_ATTR_USERNAME).expect("USERNAME");
        assert_eq!(username, b"srv1:loc1");
        assert_ne!(
            username,
            format!(
                "{}:{}",
                credentials.remote_username_fragment, credentials.local_username_fragment
            )
            .as_bytes()
        );
        assert!(valid_stun_fingerprint(&packet));
        assert!(valid_stun_message_integrity(
            &packet,
            credentials.remote_password.as_bytes()
        ));
    }

    #[test]
    fn version_six_binding_request_matches_known_answer() {
        let packet = build_stun_binding_request(
            &stun_credentials(),
            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        );
        assert_eq!(
            packet,
            hex_bytes(
                "000100342112A442000102030405060708090A0B0006000D72656D6F746530313A6C6F633100000000080014B276DC1C7949494C7EF7EB226BE8BB5E0EE5AABD802800045A8349EF"
            ),
        );
        assert!(valid_stun_fingerprint(&packet));
        assert!(valid_stun_message_integrity(
            &packet,
            b"remote-password-with-36-byte-value-001"
        ));
    }

    #[test]
    fn version_six_validates_requests_and_builds_authenticated_responses() {
        let credentials = stun_credentials();
        let transaction_id = [0x11; 12];
        let request = build_authenticated_stun_packet(
            STUN_BINDING_REQUEST,
            &transaction_id,
            credentials.local_password.as_bytes(),
            &[(STUN_ATTR_USERNAME, b"loc1:remote01".to_vec())],
        );
        let source: SocketAddr = "192.0.2.20:5004".parse().expect("source");
        let StunDatagram::Handled(Some(response)) =
            handle_stun_datagram(&request, source, &credentials)
        else {
            panic!("authenticated binding request must produce a response");
        };
        assert_eq!(
            u16::from_be_bytes([response[0], response[1]]),
            STUN_BINDING_SUCCESS_RESPONSE
        );
        assert_eq!(&response[8..20], &transaction_id);
        assert!(valid_stun_fingerprint(&response));
        assert!(valid_stun_message_integrity(
            &response,
            credentials.local_password.as_bytes()
        ));
        let (_, mapped) = find_stun_attribute(&response, STUN_ATTR_XOR_MAPPED_ADDRESS)
            .expect("XOR-MAPPED-ADDRESS");
        assert_eq!(mapped[1], 1);
        assert_eq!(
            u16::from_be_bytes([mapped[2], mapped[3]]) ^ ((STUN_MAGIC_COOKIE >> 16) as u16),
            5004
        );

        let mut tampered = request;
        tampered[24] ^= 1;
        assert!(matches!(
            handle_stun_datagram(&tampered, source, &credentials),
            StunDatagram::Invalid
        ));
    }

    #[test]
    fn gfn_local_ice_credentials_match_official_lengths() {
        let creds = generate_gfn_local_ice_credentials().expect("OS randomness");
        assert_eq!(creds.ufrag.len(), 4);
        assert_eq!(creds.pass.len(), 22);
    }

    #[test]
    fn synthesized_ice_success_matches_the_request_transaction() {
        let credentials = stun_credentials();
        let transaction_id = [7_u8; 12];
        let mapped = "192.168.1.104:54454".parse().expect("mapped");
        let response =
            synthesize_ice_binding_success(&transaction_id, mapped, &credentials.remote_password);
        assert_eq!(
            u16::from_be_bytes([response[0], response[1]]),
            STUN_BINDING_SUCCESS_RESPONSE
        );
        assert_eq!(&response[8..20], &transaction_id);
        assert!(valid_stun_fingerprint(&response));
        assert!(valid_stun_message_integrity(
            &response,
            credentials.remote_password.as_bytes()
        ));
    }

    #[test]
    fn legacy_gcm_key_derivation_matches_known_answer() {
        let config = config();
        let receiver = test_srtp(&config);
        let SrtpCipher::AeadAes256Gcm {
            encryption_key,
            session_salt,
            ..
        } = receiver.cipher
        else {
            panic!("legacy handoff must derive an AES-256-GCM session");
        };
        assert_eq!(
            encryption_key,
            decode_fixed_hex::<32>(
                "0E44A0B0E7F1BDBB298CBEE52C9F8AC1C37726768C946F59BDAAA099608CBF66",
                NvstConfigError::InvalidAesKey,
            )
            .expect("key"),
        );
        assert_eq!(
            session_salt,
            decode_fixed_hex::<12>("78B9D97B7FFB37CAD539A29D", NvstConfigError::InvalidSrtpSalt)
                .expect("salt"),
        );
    }

    #[test]
    fn selector_prefers_valid_legacy_nvst_and_falls_back_for_h265() {
        let context = json!({ "nvstVideo": legacy_handoff() });
        assert!(matches!(
            select_preferred_video_transport(&context),
            PreferredVideoTransport::Nvst(_)
        ));
        let configured = parse_nvst_video_handoff(&json!({
            "nvstVideo": legacy_handoff(),
            "settings": { "fps": 60 },
            "session": { "negotiatedStreamProfile": { "fps": 120 } }
        }))
        .expect("valid NVST context")
        .expect("NVST handoff");
        assert_eq!(configured.frame_time_us, 8_333);
        let capped = parse_nvst_video_handoff(&json!({
            "nvstVideo": legacy_handoff(),
            "settings": { "fps": 360 }
        }))
        .expect("valid high-FPS NVST context")
        .expect("NVST handoff");
        assert_eq!(capped.frame_time_us, 4_166);
        assert!(matches!(
            select_preferred_video_transport(&json!({ "nvstTransport": { "tracks": [] } })),
            PreferredVideoTransport::WebRtcFallback(NvstFallbackReason::InvalidNvstHandoff(
                NvstConfigError::MissingNvstVideoHandoff
            ))
        ));

        let mut missing_gcm_salt = legacy_handoff();
        missing_gcm_salt
            .as_object_mut()
            .expect("handoff object")
            .remove("srtpSaltHex");
        assert!(matches!(
            NvstVideoConfig::from_legacy_handoff(&missing_gcm_salt, None),
            Err(NvstConfigError::MissingField("srtpSaltHex"))
        ));

        let mut invalid = legacy_handoff();
        invalid["codec"] = json!("VP9");
        let context = json!({ "nvstVideo": invalid });
        assert!(matches!(
            select_preferred_video_transport(&context),
            PreferredVideoTransport::WebRtcFallback(NvstFallbackReason::InvalidNvstHandoff(
                NvstConfigError::UnsupportedCodec(_)
            ))
        ));

        let mut missing_cm_salt = legacy_handoff();
        missing_cm_salt
            .as_object_mut()
            .expect("handoff object")
            .remove("srtpSaltHex");
        missing_cm_salt["srtpProfile"] = json!("AES_CM_128_HMAC_SHA1_80");
        missing_cm_salt["srtpAesKeyHex"] = json!("000102030405060708090A0B0C0D0E0F");
        assert!(matches!(
            NvstVideoConfig::from_legacy_handoff(&missing_cm_salt, None),
            Err(NvstConfigError::MissingField("srtpSaltHex"))
        ));
        missing_cm_salt["srtpSaltHex"] = json!(TEST_SALT);
        let explicit_cm = NvstVideoConfig::from_legacy_handoff(&missing_cm_salt, None)
            .expect("explicit AES-CM profile");
        assert_eq!(
            explicit_cm.srtp_profile(),
            NvstSrtpProfile::AesCm128HmacSha1_80
        );

        let mut gcm_128 = legacy_handoff();
        gcm_128["srtpProfile"] = json!("AEAD_AES_128_GCM");
        gcm_128["srtpAesKeyHex"] = json!("000102030405060708090A0B0C0D0E0F");
        gcm_128["srtpSaltHex"] = json!("00000000000000009ECA935E");
        assert_eq!(
            NvstVideoConfig::from_legacy_handoff(&gcm_128, None)
                .expect("explicit AES-128-GCM profile")
                .srtp_profile(),
            NvstSrtpProfile::AeadAes128Gcm
        );

        let mut cm_32 = gcm_128;
        cm_32["srtpProfile"] = json!("AES_CM_128_HMAC_SHA1_32");
        cm_32["srtpSaltHex"] = json!(TEST_SALT);
        assert_eq!(
            NvstVideoConfig::from_legacy_handoff(&cm_32, None)
                .expect("explicit AES-CM-32 profile")
                .srtp_profile(),
            NvstSrtpProfile::AesCm128HmacSha1_32
        );
    }

    #[test]
    fn rejects_invalid_peer_and_secret_material() {
        let mut handoff = legacy_handoff();
        handoff["videoPeerIp"] = json!("0.0.0.0");
        assert!(matches!(
            NvstVideoConfig::from_legacy_handoff(&handoff, None),
            Err(NvstConfigError::InvalidPeerIp(_))
        ));
        let mut handoff = legacy_handoff();
        handoff["srtpAesKeyHex"] = json!("not-a-key");
        assert!(matches!(
            NvstVideoConfig::from_legacy_handoff(&handoff, None),
            Err(NvstConfigError::InvalidAesKey)
        ));

        let mut aes_128 = legacy_handoff();
        aes_128["srtpProfile"] = json!("AEAD_AES_128_GCM");
        aes_128["srtpAesKeyHex"] = json!("000102030405060708090A0B0C0D0E0G");
        assert!(matches!(
            NvstVideoConfig::from_legacy_handoff(&aes_128, None),
            Err(NvstConfigError::InvalidAesKey)
        ));
    }

    #[test]
    fn srtp_aes_cm_hmac_sha1_known_answer_unprotects_packet_when_explicitly_selected() {
        let key = decode_fixed_hex::<16>(
            "000102030405060708090A0B0C0D0E0F",
            NvstConfigError::InvalidAesKey,
        )
        .expect("key");
        let salt =
            decode_fixed_hex::<14>(TEST_SALT, NvstConfigError::InvalidSrtpSalt).expect("salt");
        let material = NvstSrtpMaterial::AesCm128HmacSha1 {
            master_key: key,
            master_salt: salt,
            authentication_tag_len: SRTP_AES_CM_HMAC_SHA1_80_TAG_LEN,
        };
        let crypto = SrtpReceiver::from_material(&material);
        // Frozen plaintext so the crypto known-answer vector stays independent of
        // the RTP packet-layout helper: 12-byte header + 16-byte inline metadata
        // + 6 bytes of media.
        let plaintext =
            hex_bytes("80E01234010203041122334434120000070000000700000000000000000000016588");
        let protected = protect_for_test(&crypto, plaintext.clone(), 0);
        assert_eq!(
            protected,
            hex_bytes(
                "80E01234010203041122334408BA995FCB62EA430BE6EDEF8D4DE06268F0D6D702702082DDFADA13AE83402B"
            ),
            "independent AES-CTR/HMAC known-answer vector",
        );
        let mut receiver = SrtpReceiver::from_material(&material);
        let unprotected = receiver.unprotect(&protected).expect("authenticated SRTP");
        assert_eq!(unprotected.plaintext, plaintext);
        assert_eq!(unprotected.index, 0x1234);

        let mut tampered = protected;
        let last = tampered.last_mut().expect("authentication tag");
        *last ^= 0x01;
        let mut receiver = SrtpReceiver::from_material(&material);
        assert!(matches!(
            receiver.unprotect(&tampered),
            Err(NvstDropReason::AuthenticationFailed)
        ));
    }

    #[test]
    fn srtp_aead_aes_256_gcm_rfc_known_answer_unprotects_and_rejects_tampering_and_replay() {
        let session_key =
            decode_fixed_hex::<32>(TEST_KEY, NvstConfigError::InvalidAesKey).expect("key");
        let session_salt =
            decode_fixed_hex::<12>("517569642070726F2071756F", NvstConfigError::InvalidSrtpSalt)
                .expect("salt");
        let mut receiver = SrtpReceiver {
            cipher: SrtpCipher::AeadAes256Gcm {
                encryption_key: session_key,
                session_salt,
                authentication_tag_len: SRTP_AEAD_AES_GCM_TAG_LEN,
            },
            replay: ReplayWindow::default(),
        };
        let protected = hex_bytes(
            "8040F17B8041F8D35501A0B232B1DE78A822FE12EF9F78FA332E33AAB18012389A58E2F3B50B2A0276FFAE0F1BA63799B87B7AA3DB36DFFFD6B0F9BB7878D7A76C13",
        );
        let expected = hex_bytes(
            "8040F17B8041F8D35501A0B247616C6C696120657374206F6D6E69732064697669736120696E207061727465732074726573",
        );
        let unprotected = receiver.unprotect(&protected).expect("RFC 7714 packet");
        assert_eq!(unprotected.plaintext, expected);
        assert!(matches!(
            receiver.unprotect(&protected),
            Err(NvstDropReason::ReplayRejected)
        ));

        let mut tampered = protected;
        tampered[20] ^= 0x01;
        let mut receiver = SrtpReceiver {
            cipher: SrtpCipher::AeadAes256Gcm {
                encryption_key: session_key,
                session_salt,
                authentication_tag_len: SRTP_AEAD_AES_GCM_TAG_LEN,
            },
            replay: ReplayWindow::default(),
        };
        assert!(matches!(
            receiver.unprotect(&tampered),
            Err(NvstDropReason::AuthenticationFailed)
        ));
    }

    #[test]
    fn srtp_aead_aes_128_gcm_rfc_known_answer_unprotects_and_rejects_tampering_and_replay() {
        let session_key = decode_fixed_hex::<16>(
            "000102030405060708090A0B0C0D0E0F",
            NvstConfigError::InvalidAesKey,
        )
        .expect("key");
        let session_salt =
            decode_fixed_hex::<12>("517569642070726F2071756F", NvstConfigError::InvalidSrtpSalt)
                .expect("salt");
        let mut receiver = SrtpReceiver {
            cipher: SrtpCipher::AeadAes128Gcm {
                encryption_key: session_key,
                session_salt,
                authentication_tag_len: SRTP_AEAD_AES_GCM_TAG_LEN,
            },
            replay: ReplayWindow::default(),
        };
        let protected = hex_bytes(
            "8040F17B8041F8D35501A0B2F24DE3A3FB34DE6CACBA861C9D7E4BCABE633BD50D294E6F42A5F47A51C7D19B36DE3ADF8833899D7F27BEB16A9152CF765EE4390CCE",
        );
        let expected = hex_bytes(
            "8040F17B8041F8D35501A0B247616C6C696120657374206F6D6E69732064697669736120696E207061727465732074726573",
        );
        let unprotected = receiver.unprotect(&protected).expect("RFC 7714 packet");
        assert_eq!(unprotected.plaintext, expected);
        assert!(matches!(
            receiver.unprotect(&protected),
            Err(NvstDropReason::ReplayRejected)
        ));

        let mut tampered = protected;
        tampered[20] ^= 0x01;
        let mut receiver = SrtpReceiver {
            cipher: SrtpCipher::AeadAes128Gcm {
                encryption_key: session_key,
                session_salt,
                authentication_tag_len: SRTP_AEAD_AES_GCM_TAG_LEN,
            },
            replay: ReplayWindow::default(),
        };
        assert!(matches!(
            receiver.unprotect(&tampered),
            Err(NvstDropReason::AuthenticationFailed)
        ));
    }

    #[test]
    fn parses_rtp_extensions_and_padding_after_srtp_unprotect() {
        let mut packet = vec![0xb0, 0x60, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1];
        packet.extend_from_slice(&[0xbe, 0xde, 0, 1, 0xaa, 0xbb, 0xcc, 0xdd]);
        packet.extend_from_slice(&[1, 2, 3, 4, 2, 2]);
        let header = RtpHeader::parse(&packet).expect("extension parsed");
        assert_eq!(header.payload_offset, 20);
        assert_eq!(
            header.payload(&packet).expect("padding removed"),
            &[1, 2, 3, 4]
        );
    }

    #[test]
    fn gs_extension_requires_exactly_four_words() {
        let exact = build_plaintext_rtp(1, FLAG_SOF, 7, &[0, 0, 1, 0x65]);
        assert!(RtpHeader::parse(&exact).is_ok());

        let mut oversized = exact.clone();
        oversized[14..16].copy_from_slice(&5_u16.to_be_bytes());
        oversized.splice(32..32, [0_u8; 4]);
        assert!(matches!(
            RtpHeader::parse(&oversized),
            Err(RtpParseError::InvalidExtensionLength)
        ));

        let mut truncated = exact;
        truncated[14..16].copy_from_slice(&5_u16.to_be_bytes());
        assert!(matches!(
            RtpHeader::parse(&truncated),
            Err(RtpParseError::InvalidExtensionLength)
        ));
    }

    #[test]
    fn parses_the_wire_stream_packet_index_as_a_24_bit_value() {
        let packet = build_plaintext_rtp(1, FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA, 7, &[]);
        // Point the packet index at a 24-bit value to verify the wire shift.
        let mut packet = packet;
        packet[16..20].copy_from_slice(&(0x12_34_56_u32 << 8).to_le_bytes());

        let header = RtpHeader::parse(&packet).expect("RTP header");
        let payload = header.payload(&packet).expect("payload");
        let (video, media) = NvVideoPacket::parse(&header, payload).expect("NV video header");
        assert_eq!(video.stream_packet_index, 0x12_34_56);
        assert_eq!(video.frame_index, 7);
        assert!(!video.is_fec);
        assert!(media.is_empty());
    }

    #[test]
    fn classifies_fec_packets_from_the_extension_group_coordinates() {
        let mut packet = build_plaintext_rtp(3, FLAG_CONTAINS_PIC_DATA, 7, &[0xaa]);
        // FecId=3, SrcPkts=3 with the FEC-group marker bits set => correction packet.
        packet[28..32].copy_from_slice(&0x00c0_3420_u32.to_le_bytes());
        let header = RtpHeader::parse(&packet).expect("RTP header");
        let payload = header.payload(&packet).expect("payload");
        let (video, _) = NvVideoPacket::parse(&header, payload).expect("NV video header");
        assert!(video.is_fec);

        // Source packet: FecId=2 < SrcPkts=3.
        let mut packet = build_plaintext_rtp(2, FLAG_CONTAINS_PIC_DATA, 7, &[0xaa]);
        packet[28..32].copy_from_slice(&0x00c0_2420_u32.to_le_bytes());
        let header = RtpHeader::parse(&packet).expect("RTP header");
        let payload = header.payload(&packet).expect("payload");
        let (video, _) = NvVideoPacket::parse(&header, payload).expect("NV video header");
        assert!(!video.is_fec);
    }

    #[test]
    fn strips_the_gamestream_frame_header_before_annex_b_video() {
        let header = NvVideoPacket {
            stream_packet_index: 1,
            frame_index: 9,
            flags: FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
            fec_current_block: 0,
            fec_last_block: 0,
            is_fec: false,
        };
        let mut payload = vec![0x01, 0, 0, 2, 0, 0, 0, 0];
        payload.extend_from_slice(&[
            0, 0, 1, 0x67, 0xaa, 0, 0, 0, 1, 0x68, 0xbb, 0, 0, 1, 0x65, 0xcc,
        ]);

        let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::H264, 4096);
        let frame = assembler
            .push(header, 90_000, &payload)
            .expect("valid frame")
            .expect("complete frame");

        assert_eq!(
            frame.bytes,
            [
                0, 0, 0, 1, 0x67, 0xaa, 0, 0, 0, 1, 0x68, 0xbb, 0, 0, 1, 0x65, 0xcc,
            ]
        );
        assert!(frame.keyframe);
    }

    #[test]
    fn assembles_h265_annex_b_and_detects_irap_keyframes() {
        let header = NvVideoPacket {
            stream_packet_index: 1,
            frame_index: 10,
            flags: FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
            fec_current_block: 0,
            fec_last_block: 0,
            is_fec: false,
        };
        let mut payload = vec![0x01, 0, 0, 2, 0, 0, 0, 0];
        payload.extend_from_slice(&[0, 0, 0, 1, 19 << 1, 1, 0xaa]);
        let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::H265, 4096);

        let frame = assembler
            .push(header, 90_000, &payload)
            .expect("valid H.265 frame")
            .expect("complete frame");

        assert_eq!(frame.codec, NvstVideoCodec::H265);
        assert_eq!(frame.bytes, [0, 0, 0, 1, 19 << 1, 1, 0xaa]);
        assert!(frame.keyframe);
    }

    #[test]
    fn assembles_av1_payload_after_the_gamestream_frame_header() {
        let header = NvVideoPacket {
            stream_packet_index: 1,
            frame_index: 11,
            flags: FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
            fec_current_block: 0,
            fec_last_block: 0,
            is_fec: false,
        };
        let payload = [0x01, 0, 0, 2, 11, 0, 0, 0, 0x12, 0x34, 0x56];
        let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::Av1, 4096);

        let frame = assembler
            .push(header, 90_000, &payload)
            .expect("valid AV1 frame")
            .expect("complete frame");

        assert_eq!(frame.codec, NvstVideoCodec::Av1);
        assert_eq!(frame.bytes, [0x12, 0x34, 0x56]);
        assert!(frame.keyframe);
    }

    #[test]
    fn strips_the_gfn_cloud_extended_av1_frame_header() {
        let header = NvVideoPacket {
            stream_packet_index: 1,
            frame_index: 12,
            flags: FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
            fec_current_block: 0,
            fec_last_block: 0,
            is_fec: false,
        };
        let mut payload = vec![0_u8; GFN_EXTENDED_FRAME_HEADER_BYTES];
        payload[0] = 0x81;
        payload[3] = 2;
        payload[16..20].copy_from_slice(&3_u32.to_le_bytes());
        // A temporal-unit length byte is not necessarily a valid OBU header.
        payload.extend_from_slice(&[0x81, 0x01, 0x12]);
        let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::Av1, 4096);

        let frame = assembler
            .push(header, 90_000, &payload)
            .expect("valid extended AV1 frame")
            .expect("complete frame");

        assert_eq!(frame.bytes, [0x81, 0x01, 0x12]);
        assert!(frame.keyframe);
    }

    #[test]
    fn trims_gfn_cloud_av1_padding_to_the_advertised_access_unit_size() {
        let header = NvVideoPacket {
            stream_packet_index: 1,
            frame_index: 13,
            flags: FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
            fec_current_block: 0,
            fec_last_block: 0,
            is_fec: false,
        };
        let mut payload = vec![0_u8; GFN_EXTENDED_FRAME_HEADER_BYTES];
        payload[0] = 0x81;
        payload[3] = 2;
        payload[16..20].copy_from_slice(&5_u32.to_le_bytes());
        payload.extend_from_slice(&[0x12, 0x00, 0x32, 0x01, 0xaa, 0, 0, 0]);
        let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::Av1, 4096);

        let frame = assembler
            .push(header, 90_000, &payload)
            .expect("valid padded AV1 frame")
            .expect("complete frame");

        assert_eq!(frame.bytes, [0x12, 0x00, 0x32, 0x01, 0xaa]);
        assert!(frame.keyframe);
    }

    #[test]
    fn trims_av1_fec_padding_from_the_final_packet() {
        let first = NvVideoPacket {
            stream_packet_index: 1,
            frame_index: 12,
            flags: FLAG_SOF | FLAG_CONTAINS_PIC_DATA,
            fec_current_block: 0,
            fec_last_block: 0,
            is_fec: false,
        };
        let last = NvVideoPacket {
            stream_packet_index: 2,
            frame_index: 12,
            flags: FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
            fec_current_block: 0,
            fec_last_block: 0,
            is_fec: false,
        };
        let first_payload = [0x01, 0, 0, 2, 3, 0, 0, 0, 0x12, 0x00];
        let padded_last_payload = [0x32, 0x01, 0xaa, 0, 0, 0, 0];
        let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::Av1, 4096);

        assert!(
            assembler
                .push(first, 90_000, &first_payload)
                .expect("valid AV1 first packet")
                .is_none()
        );
        let frame = assembler
            .push(last, 90_000, &padded_last_payload)
            .expect("valid AV1 final packet")
            .expect("complete frame");

        assert_eq!(frame.bytes, [0x12, 0x00, 0x32, 0x01, 0xaa]);
        assert!(frame.keyframe);
    }

    #[test]
    fn parses_all_supported_nvst_video_codec_names() {
        assert_eq!(
            NvstVideoCodec::parse("AVC").expect("AVC"),
            NvstVideoCodec::H264
        );
        assert_eq!(
            NvstVideoCodec::parse("HEVC").expect("HEVC"),
            NvstVideoCodec::H265
        );
        assert_eq!(
            NvstVideoCodec::parse("AV1").expect("AV1"),
            NvstVideoCodec::Av1
        );
    }

    #[test]
    fn annex_b_scanner_returns_the_earliest_mixed_width_start_code() {
        assert_eq!(
            find_annex_b_start_code(&[0, 0, 1, 0x67, 0, 0, 0, 1, 0x68]),
            Some((0, 3))
        );
        assert_eq!(
            find_annex_b_start_code(&[0, 0, 0, 1, 0x67, 0, 0, 1, 0x68]),
            Some((0, 4))
        );
    }

    #[test]
    fn strips_a_terminal_access_unit_delimiter_before_decoder_submission() {
        let mut bytes = vec![0, 0, 0, 1, 0x65, 0xaa, 0, 0, 0, 1, 0x09, 0xf0];
        strip_trailing_access_unit_delimiter(NvstVideoCodec::H264, &mut bytes);
        assert_eq!(bytes, [0, 0, 0, 1, 0x65, 0xaa]);

        let mut bytes = vec![0, 0, 0, 1, 0x09, 0xf0, 0, 0, 1, 0x61, 0xbb];
        strip_trailing_access_unit_delimiter(NvstVideoCodec::H264, &mut bytes);
        assert_eq!(bytes, [0, 0, 0, 1, 0x09, 0xf0, 0, 0, 1, 0x61, 0xbb]);
    }

    #[test]
    fn assembles_a_single_packet_frame_without_the_picture_data_flag() {
        let header = NvVideoPacket {
            stream_packet_index: 25,
            frame_index: 12,
            flags: FLAG_SOF | FLAG_EOF,
            fec_current_block: 0,
            fec_last_block: 0,
            is_fec: false,
        };
        let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::H264, 4096);
        let frame = assembler
            .push(header, 90_000, &[0x81, 0x08, 0, 0, 0, 1, 0x61, 0xaa])
            .expect("source packet")
            .expect("complete frame");
        assert_eq!(frame.bytes, [0, 0, 0, 1, 0x61, 0xaa]);
    }

    #[test]
    fn rejects_a_start_packet_without_nearby_annex_b_video() {
        let header = NvVideoPacket {
            stream_packet_index: 1,
            frame_index: 9,
            flags: FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
            fec_current_block: 0,
            fec_last_block: 0,
            is_fec: false,
        };
        let payload = vec![0x81; MAX_GS_FRAME_HEADER_BYTES + 8];
        let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::H264, 4096);

        assert!(matches!(
            assembler.push(header, 90_000, &payload),
            Err(NvstDropReason::MissingAnnexBStartCode)
        ));
    }

    #[test]
    fn receiver_reorders_authenticated_packets_and_emits_annex_b_frame() {
        let config = config();
        let feedback = config.feedback();
        let crypto = test_srtp(&config);
        let first = protect_for_test(
            &crypto,
            build_plaintext_rtp(
                10,
                FLAG_SOF | FLAG_CONTAINS_PIC_DATA,
                9,
                &[0, 0, 0, 1, 0x65],
            ),
            0,
        );
        let middle = protect_for_test(
            &crypto,
            build_plaintext_rtp(11, FLAG_CONTAINS_PIC_DATA, 9, &[0xaa]),
            0,
        );
        let last = protect_for_test(
            &crypto,
            build_plaintext_rtp(12, FLAG_EOF | FLAG_CONTAINS_PIC_DATA, 9, &[0xbb]),
            0,
        );
        let mut receiver = NvstVideoReceiver::new(config);
        assert!(
            receiver
                .process_datagram(peer(), &first, Instant::now())
                .is_empty()
        );
        assert!(
            receiver
                .process_datagram(peer(), &last, Instant::now())
                .is_empty()
        );
        let events = receiver.process_datagram(peer(), &middle, Instant::now());
        assert_eq!(events.len(), 1);
        let NvstReceiveEvent::Frame(frame) = &events[0] else {
            panic!("expected frame, got {events:?}");
        };
        assert_eq!(frame.frame_index, 9);
        assert!(frame.keyframe);
        assert_eq!(frame.bytes, [0, 0, 0, 1, 0x65, 0xaa, 0xbb]);
        assert_eq!(feedback.take_nack(), None);
        assert_eq!(feedback.completed_frame_snapshot(), (1, 7, frame.timestamp));
        assert!(feedback.take_completed_frame().is_none());
        feedback.publish_accepted_frame(7, Instant::now());
        let pending_ack = feedback
            .take_completed_frame()
            .expect("accepted frame acknowledgment");
        assert_eq!(pending_ack.frame_number, 1);
        assert_eq!(pending_ack.bytes, 7);
    }

    #[test]
    fn accepted_frame_queue_discards_stale_feedback_and_preserves_sequence_numbers() {
        let feedback = NvstFeedbackState::default();
        for bytes in 1..=u32::try_from(MAX_PENDING_FRAME_ACKS + 2).unwrap() {
            feedback.publish_accepted_frame(bytes, Instant::now());
        }

        let first = feedback.take_completed_frame().expect("newest queue head");
        assert_eq!(first.frame_number, 3);
        assert_eq!(first.bytes, 3);

        let mut last = first;
        while let Some(frame) = feedback.take_completed_frame() {
            last = frame;
        }
        assert_eq!(last.frame_number, 514);
        assert_eq!(last.bytes, 514);
    }

    #[test]
    fn continuous_rtp_with_a_gs_index_gap_discards_the_frame_and_requests_pli() {
        let config = config();
        let feedback = config.feedback();
        let crypto = test_srtp(&config);
        let first = protect_for_test(
            &crypto,
            build_plaintext_rtp(10, FLAG_SOF, 9, &[0, 0, 1, 0x65]),
            0,
        );
        let mut skipped_gs_index = build_plaintext_rtp(11, FLAG_EOF, 9, &[0xaa]);
        skipped_gs_index[16..20].copy_from_slice(&(12_u32 << 8).to_le_bytes());
        let skipped_gs_index = protect_for_test(&crypto, skipped_gs_index, 0);
        let mut receiver = NvstVideoReceiver::new(config);

        assert!(
            receiver
                .process_datagram(peer(), &first, Instant::now())
                .is_empty()
        );
        assert_eq!(
            receiver.process_datagram(peer(), &skipped_gs_index, Instant::now()),
            [NvstReceiveEvent::Dropped(
                NvstDropReason::FrameDiscontinuity
            )]
        );
        assert!(feedback.take_keyframe_request());
    }

    #[test]
    fn a_new_frame_start_recovers_after_a_gs_index_gap() {
        let config = config();
        let crypto = test_srtp(&config);
        let first = protect_for_test(
            &crypto,
            build_plaintext_rtp(
                10,
                FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
                9,
                &[0, 0, 1, 0x65],
            ),
            0,
        );
        let mut next_frame = build_plaintext_rtp(
            11,
            FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
            10,
            &[0, 0, 1, 0x65],
        );
        next_frame[16..20].copy_from_slice(&(30_u32 << 8).to_le_bytes());
        let next_frame = protect_for_test(&crypto, next_frame, 0);
        let mut receiver = NvstVideoReceiver::new(config);

        assert!(
            receiver
                .process_datagram(peer(), &first, Instant::now())
                .iter()
                .any(|event| matches!(event, NvstReceiveEvent::Frame(_)))
        );
        assert!(
            receiver
                .process_datagram(peer(), &next_frame, Instant::now())
                .iter()
                .any(|event| matches!(event, NvstReceiveEvent::Frame(_)))
        );
    }

    #[test]
    fn fec_repair_packets_are_not_forwarded_as_video_payload() {
        let config = config();
        let crypto = test_srtp(&config);
        let first = protect_for_test(
            &crypto,
            build_plaintext_rtp(10, FLAG_SOF, 9, &[0, 0, 1, 0x65]),
            0,
        );
        let mut repair = build_plaintext_rtp(11, FLAG_CONTAINS_PIC_DATA, 9, &[0xee]);
        repair[28..32].copy_from_slice(&0x00c0_3420_u32.to_le_bytes());
        let repair = protect_for_test(&crypto, repair, 0);
        let mut receiver = NvstVideoReceiver::new(config);

        assert!(
            receiver
                .process_datagram(peer(), &first, Instant::now())
                .is_empty()
        );
        assert!(
            receiver
                .process_datagram(peer(), &repair, Instant::now())
                .is_empty(),
            "a parity shard must stay in the FEC buffer rather than reaching H.264 assembly",
        );
    }

    #[test]
    fn authenticated_packet_without_gs_metadata_discards_reassembly_and_requests_pli() {
        let config = config();
        let feedback = config.feedback();
        let crypto = test_srtp(&config);
        let first = protect_for_test(
            &crypto,
            build_plaintext_rtp(10, FLAG_SOF, 9, &[0, 0, 1, 0x65]),
            0,
        );
        let mut missing_gs = build_plaintext_rtp(11, FLAG_EOF, 9, &[0xaa]);
        missing_gs[12..14].copy_from_slice(&0xbede_u16.to_be_bytes());
        let missing_gs = protect_for_test(&crypto, missing_gs, 0);
        let mut receiver = NvstVideoReceiver::new(config);

        assert!(
            receiver
                .process_datagram(peer(), &first, Instant::now())
                .is_empty()
        );
        assert_eq!(
            receiver.process_datagram(peer(), &missing_gs, Instant::now()),
            [NvstReceiveEvent::Dropped(NvstDropReason::MalformedRtp(
                RtpParseError::MissingNvVideoHeader
            ))]
        );
        assert!(feedback.take_keyframe_request());
    }

    #[test]
    fn receiver_assembles_one_frame_across_multiple_fec_blocks() {
        let config = config();
        let crypto = test_srtp(&config);
        let block_zero_start = protect_for_test(
            &crypto,
            build_plaintext_rtp_with_fec_blocks(
                20,
                FLAG_SOF | FLAG_CONTAINS_PIC_DATA,
                9,
                0,
                1,
                &[0, 0, 0, 1, 0x65],
            ),
            0,
        );
        let block_zero_end = protect_for_test(
            &crypto,
            build_plaintext_rtp_with_fec_blocks(
                21,
                FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
                9,
                0,
                1,
                &[0xaa],
            ),
            0,
        );
        let block_one_start = protect_for_test(
            &crypto,
            build_plaintext_rtp_with_fec_blocks(
                22,
                FLAG_SOF | FLAG_CONTAINS_PIC_DATA,
                9,
                1,
                1,
                &[0xbb],
            ),
            0,
        );
        let block_one_end = protect_for_test(
            &crypto,
            build_plaintext_rtp_with_fec_blocks(
                23,
                FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
                9,
                1,
                1,
                &[0xcc],
            ),
            0,
        );
        let mut receiver = NvstVideoReceiver::new(config);
        for packet in [&block_zero_start, &block_zero_end, &block_one_start] {
            assert!(
                receiver
                    .process_datagram(peer(), packet, Instant::now())
                    .is_empty()
            );
        }
        let events = receiver.process_datagram(peer(), &block_one_end, Instant::now());
        let [NvstReceiveEvent::Frame(frame)] = events.as_slice() else {
            panic!("expected one complete frame, got {events:?}");
        };
        assert_eq!(frame.frame_index, 9);
        assert!(frame.keyframe);
        assert_eq!(frame.bytes, [0, 0, 0, 1, 0x65, 0xaa, 0xbb, 0xcc]);
    }

    #[test]
    fn receiver_rejects_wrong_peer_and_duplicate_authenticated_packet() {
        let config = config();
        let crypto = test_srtp(&config);
        let packet = protect_for_test(
            &crypto,
            build_plaintext_rtp(
                1,
                FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
                1,
                &[0, 0, 1, 0x65],
            ),
            0,
        );
        let mut receiver = NvstVideoReceiver::new(config);
        let wrong_peer = SocketAddr::new("192.0.2.21".parse().expect("IP"), 5004);
        assert!(matches!(
            receiver
                .process_datagram(wrong_peer, &packet, Instant::now())
                .as_slice(),
            [NvstReceiveEvent::Dropped(
                NvstDropReason::UnexpectedSource { .. }
            )]
        ));
        assert!(matches!(
            receiver
                .process_datagram(peer(), &packet, Instant::now())
                .as_slice(),
            [NvstReceiveEvent::Frame(_)]
        ));
        assert!(matches!(
            receiver
                .process_datagram(peer(), &packet, Instant::now())
                .as_slice(),
            [NvstReceiveEvent::Dropped(NvstDropReason::ReplayRejected)]
        ));
    }

    #[test]
    fn replay_window_accepts_late_unseen_packets_across_the_full_reorder_window() {
        let mut replay = ReplayWindow::default();
        replay.check(10).expect("first packet");
        replay.commit(10);
        replay.check(1_500).expect("new highest packet");
        replay.commit(1_500);

        // This packet is far beyond the old 64-entry filter but is still inside the bounded
        // 2,048-packet reorder/NACK envelope and has never been authenticated before.
        replay.check(11).expect("late retransmission");
        replay.commit(11);
        assert_eq!(replay.check(11), Err(NvstDropReason::ReplayRejected));

        replay.check(2_058).expect("advance to replay edge");
        replay.commit(2_058);
        assert_eq!(replay.check(10), Err(NvstDropReason::ReplayRejected));
        assert_eq!(replay.check(11), Err(NvstDropReason::ReplayRejected));
        replay
            .check(12)
            .expect("old unseen packet inside replay edge");
        assert_eq!(replay.check(2_057), Ok(()));
    }

    #[test]
    fn replay_window_shift_preserves_seen_bits_across_word_boundaries() {
        let mut replay = ReplayWindow::default();
        for index in [100, 99, 63, 1] {
            replay.check(index).expect("unseen packet");
            replay.commit(index);
        }
        replay.check(165).expect("advance across a bitmap word");
        replay.commit(165);
        for index in [100, 99, 63, 1, 165] {
            assert_eq!(replay.check(index), Err(NvstDropReason::ReplayRejected));
        }
        replay
            .check(98)
            .expect("nearby unseen packet remains admissible");
    }

    #[test]
    fn fec_block_reconstructs_a_missing_shard_inside_a_multiblock_frame() {
        let base_index = 4_000_u64;
        let mut data = (0..3_u16)
            .map(|offset| {
                // This is block 1 of 0..=2, so neither its first nor last shard carries the
                // whole-frame SOF/EOF markers.
                let mut packet = build_plaintext_rtp_with_fec_blocks(
                    (base_index as u16).wrapping_add(offset),
                    FLAG_CONTAINS_PIC_DATA,
                    77,
                    1,
                    2,
                    &[offset as u8; 32],
                );
                let fec_word = (33_u32 << 4) | (u32::from(offset) << 12) | (3_u32 << 22);
                let extension_start = RtpHeader::parse(&packet)
                    .expect("RTP header")
                    .payload_offset
                    - NV_VIDEO_PACKET_LEN;
                packet[extension_start + 12..extension_start + 16]
                    .copy_from_slice(&fec_word.to_le_bytes());
                packet
            })
            .collect::<Vec<_>>();
        let expected_middle = data[1].clone();
        let mut parity = vec![0; data[0].len()];
        for (data_index, shard) in data.iter().enumerate() {
            let coefficient =
                nvst_cauchy_coefficient(data_index, 0, 1).expect("Cauchy parity coefficient");
            gf256_axpy(&mut parity, shard, coefficient);
        }
        // Parity is transported inside its own parseable RTP envelope. These bytes are not part
        // of the useful recovered NV payload and must be restored before strict RTP parsing.
        parity[..16].copy_from_slice(&data[0][..16]);
        parity[2..4].copy_from_slice(&(base_index as u16 + 3).to_be_bytes());
        // NVIDIA's fecInfo bytes are transport metadata and are not guaranteed to reconstruct
        // to the original data-shard value. Moonlight deliberately excludes them from its
        // recovered-packet checks. Model that cloud behavior while keeping the encoded video
        // bytes themselves recoverable.
        let fec_info_start = RtpHeader::parse(&expected_middle)
            .expect("RTP header")
            .payload_offset;
        parity[fec_info_start - 4..fec_info_start].fill(0xa5);
        data.push(parity);

        let layout = |shard_index| FecPacketLayout {
            frame_index: 77,
            block_index: 1,
            last_block_index: 2,
            shard_index,
            data_shards: 3,
            parity_shards: 1,
        };
        let packet = |shard_index: usize, plaintext: Vec<u8>| RtpPacket {
            index: base_index + shard_index as u64,
            header: RtpHeader::parse(&expected_middle).expect("template header"),
            plaintext,
        };
        let mut block = FecBlock::new(layout(0), base_index);
        assert_eq!(block.insert(layout(0), packet(0, data[0].clone())), None);
        assert_eq!(block.insert(layout(2), packet(2, data[2].clone())), None);
        assert_eq!(block.insert(layout(3), packet(3, data[3].clone())), None);
        assert!(block.has_enough_shards());

        let (repaired, repaired_count) = block.finish(expected_middle.len()).expect("FEC repair");
        assert_eq!(repaired_count, 1);
        assert_eq!(repaired.len(), 3);
        assert_eq!(repaired[1].index, base_index + 1);
        assert_eq!(
            &repaired[1].plaintext[..fec_info_start - 4],
            &expected_middle[..fec_info_start - 4]
        );
        assert_ne!(
            &repaired[1].plaintext[fec_info_start - 4..fec_info_start],
            &expected_middle[fec_info_start - 4..fec_info_start]
        );
        assert_eq!(
            &repaired[1].plaintext[fec_info_start..],
            &expected_middle[fec_info_start..]
        );

        let mut reorder = FecReorderBuffer::new(data[0].len() - NVST_FEC_RTP_HEADER_ALLOWANCE);
        assert!(
            reorder
                .push(packet(0, data[0].clone()), layout(0), Instant::now())
                .ready
                .is_empty()
        );
        assert!(
            reorder
                .push(packet(1, data[1].clone()), layout(1), Instant::now())
                .ready
                .is_empty()
        );
        assert_eq!(
            reorder
                .push(packet(2, data[2].clone()), layout(2), Instant::now())
                .ready
                .len(),
            3
        );
        let late_parity = reorder.push(packet(3, data[3].clone()), layout(3), Instant::now());
        assert!(late_parity.ready.is_empty());
        assert!(late_parity.recovery.is_none());
    }

    #[test]
    fn nvst_cauchy_fec_reconstructs_multiple_missing_data_shards() {
        let data = (0..5_usize)
            .map(|shard| {
                (0..97_usize)
                    .map(|byte| (shard * 41 + byte * 17) as u8)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let mut shards = data.iter().cloned().map(Some).collect::<Vec<_>>();
        for parity_index in 0..2 {
            let mut parity = vec![0_u8; data[0].len()];
            for (data_index, source) in data.iter().enumerate() {
                let coefficient = nvst_cauchy_coefficient(data_index, parity_index, 2)
                    .expect("Cauchy coefficient");
                gf256_axpy(&mut parity, source, coefficient);
            }
            shards.push(Some(parity));
        }
        shards[1] = None;
        shards[3] = None;

        reconstruct_nvst_cauchy_data(&mut shards, 5, 2, data[0].len()).expect("two-shard repair");
        assert_eq!(shards[1].as_deref(), Some(data[1].as_slice()));
        assert_eq!(shards[3].as_deref(), Some(data[3].as_slice()));
    }

    #[test]
    fn fec_reorder_retains_following_blocks_while_a_retransmission_is_pending() {
        fn packet(index: u64) -> RtpPacket {
            let mut plaintext = vec![0_u8; RTP_FIXED_HEADER_LEN];
            plaintext[0] = 0x80;
            plaintext[1] = 96;
            plaintext[2..4].copy_from_slice(&(index as u16).to_be_bytes());
            plaintext[4..8].copy_from_slice(&90_000_u32.to_be_bytes());
            plaintext[8..12].copy_from_slice(&1_u32.to_be_bytes());
            RtpPacket {
                index,
                header: RtpHeader::parse(&plaintext).expect("RTP header"),
                plaintext,
            }
        }
        fn layout(block_index: u8, shard_index: usize) -> FecPacketLayout {
            FecPacketLayout {
                frame_index: 7,
                block_index,
                last_block_index: 1,
                shard_index,
                data_shards: 2,
                parity_shards: 1,
            }
        }

        let started = Instant::now();
        let mut reorder = FecReorderBuffer::new(48);
        assert!(
            reorder
                .push(packet(100), layout(0, 0), started)
                .ready
                .is_empty()
        );

        let successor = reorder.push(
            packet(103),
            layout(1, 0),
            started + Duration::from_millis(1),
        );
        assert_eq!(successor.nack, Some((101, 101)));
        assert!(successor.ready.is_empty());
        assert!(successor.recovery.is_none());

        let retransmitted = reorder.push(
            packet(101),
            layout(0, 1),
            started + Duration::from_millis(20),
        );
        assert_eq!(
            retransmitted
                .ready
                .iter()
                .map(|packet| packet.index)
                .collect::<Vec<_>>(),
            [100, 101]
        );
        assert!(retransmitted.recovery.is_none());
        assert_eq!(reorder.blocks.len(), 1, "the successor stays buffered");
    }

    #[test]
    fn reorder_nack_excludes_packets_already_buffered_after_the_gap() {
        let config = config();
        let feedback = Arc::clone(&config.feedback);
        let crypto = test_srtp(&config);
        let packets = [1_u16, 3, 4].map(|sequence| {
            protect_for_test(
                &crypto,
                build_plaintext_rtp(
                    sequence,
                    FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
                    u32::from(sequence),
                    &[0, 0, 1, 0x65],
                ),
                0,
            )
        });
        let mut receiver = NvstVideoReceiver::new(config);
        for packet in &packets {
            let _ = receiver.process_datagram(peer(), packet, Instant::now());
        }
        assert_eq!(feedback.take_nack(), Some((2, 2)));
        assert_eq!(feedback.take_nack(), None);
    }

    #[test]
    fn reorder_gap_uses_mjolnir_dequeue_deadline_instead_of_filling_the_window() {
        fn packet(index: u64) -> RtpPacket {
            RtpPacket {
                index,
                header: RtpHeader {
                    payload_type: 96,
                    sequence_number: index as u16,
                    timestamp: 90_000,
                    ssrc: 1,
                    payload_offset: RTP_FIXED_HEADER_LEN,
                    has_padding: false,
                    gs_video_header: None,
                },
                plaintext: vec![0; RTP_FIXED_HEADER_LEN],
            }
        }

        let started = Instant::now();
        let mut reorder = RtpReorderBuffer::new(DEFAULT_REORDER_WINDOW);
        assert_eq!(reorder.push(packet(1), started).ready.len(), 1);

        let waiting = reorder.push(packet(3), started + Duration::from_millis(1));
        assert_eq!(waiting.nack, Some((2, 2)));
        assert!(waiting.ready.is_empty());
        assert!(waiting.recovery.is_none());

        let still_waiting = reorder.push(packet(4), started + Duration::from_millis(140));
        assert_eq!(still_waiting.nack, Some((2, 2)));
        assert!(still_waiting.ready.is_empty());
        assert!(still_waiting.recovery.is_none());

        let recovered = reorder.push(packet(5), started + Duration::from_millis(151));
        assert_eq!(
            recovered.recovery,
            Some(NvstRecovery::PacketGap {
                first_missing_index: 2,
                last_missing_index: 2,
            })
        );
        assert_eq!(
            recovered
                .ready
                .iter()
                .map(|packet| packet.index)
                .collect::<Vec<_>>(),
            [3, 4, 5]
        );
        assert_eq!(recovered.nack, None);
    }

    #[test]
    fn gaps_require_recovery_and_never_emit_an_incomplete_frame() {
        let config = config();
        let crypto = test_srtp(&config);
        let first = protect_for_test(
            &crypto,
            build_plaintext_rtp(1, FLAG_SOF | FLAG_CONTAINS_PIC_DATA, 1, &[0, 0, 1, 0x65]),
            0,
        );
        let far = protect_for_test(
            &crypto,
            build_plaintext_rtp(6, FLAG_EOF | FLAG_CONTAINS_PIC_DATA, 1, &[0xaa]),
            0,
        );
        let mut receiver = NvstVideoReceiver::new(config);
        let _ = receiver.process_datagram(peer(), &first, Instant::now());
        let events = receiver.process_datagram(peer(), &far, Instant::now());
        assert!(events.iter().any(|event| matches!(
            event,
            NvstReceiveEvent::RecoveryNeeded(NvstRecovery::PacketGap { .. })
        )));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, NvstReceiveEvent::Frame(_)))
        );

        let fresh = protect_for_test(
            &crypto,
            build_plaintext_rtp(
                7,
                FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
                2,
                &[0, 0, 1, 0x65, 0xbb],
            ),
            0,
        );
        let fresh_events = receiver.process_datagram(peer(), &fresh, Instant::now());
        let Some(NvstReceiveEvent::Frame(frame)) = fresh_events
            .iter()
            .find(|event| matches!(event, NvstReceiveEvent::Frame(_)))
        else {
            panic!("expected a fresh frame after the gap, got {fresh_events:?}");
        };
        assert!(!frame.contiguous);
    }

    #[test]
    fn generic_nack_encodes_wrapping_sequence_range() {
        let packet = build_rtcp_nack(0x0102_0304, 0x0506_0708, 65_534, 65_536);
        assert_eq!(
            packet,
            [0x81, 205, 0, 3, 1, 2, 3, 4, 5, 6, 7, 8, 0xff, 0xfe, 0, 3,]
        );
    }

    #[test]
    fn picture_loss_indication_uses_rfc_4585_psfb_layout() {
        assert_eq!(
            build_rtcp_pli(0x0102_0304, 0x0506_0708),
            [0x81, 206, 0, 2, 1, 2, 3, 4, 5, 6, 7, 8]
        );
    }

    #[test]
    fn raw_srtcp_gcm8_matches_the_macforce_pr66_layout() {
        let key = decode_fixed_hex::<32>(
            "000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F",
            NvstConfigError::InvalidAesKey,
        )
        .expect("key");
        let salt =
            decode_fixed_hex::<12>("A0A1A2A3A4A5A6A7A8A9AAAB", NvstConfigError::InvalidSrtpSalt)
                .expect("salt");
        let packet = protect_srtcp_receiver_report_gcm(
            RtcpReportBlock {
                media_ssrc: 0x1122_3344,
                fraction_lost: 0x55,
                cumulative_lost: -2,
                highest_sequence: 0x0102_0304,
                jitter: 0x0506_0708,
            },
            0x0123_4567,
            &key,
            &salt,
            SRTP_AEAD_AES_GCM_8_TAG_LEN,
        );

        assert_eq!(
            packet,
            hex_bytes(
                "81C900074F4E4F57ECBBE4F812433B089B30BF8BA442E7BB3C3C706A5BC548980601CFF1E416E06F81234567"
            )
        );
        assert_eq!(&packet[..8], &[0x81, 201, 0, 7, b'O', b'N', b'O', b'W']);
        assert_eq!(&packet[packet.len() - 4..], &0x8123_4567_u32.to_be_bytes());
    }

    #[test]
    fn receiver_report_tracks_loss_interval_and_late_recovery() {
        let feedback = NvstFeedbackState::default();
        let now = Instant::now();
        feedback.publish_stream(7, 10, 1_000, now);
        feedback.publish_stream(7, 12, 2_800, now + Duration::from_millis(20));

        let report = feedback.report_snapshot(true).expect("report");
        assert_eq!(report.media_ssrc, 7);
        assert_eq!(report.highest_sequence, 12);
        assert_eq!(report.cumulative_lost, 1);
        assert_eq!(report.fraction_lost, 85);
        assert_eq!(report.jitter, 0);

        feedback.publish_stream(7, 12, 1_900, now + Duration::from_millis(10));
        let recovered = feedback.report_snapshot(true).expect("report");
        assert_eq!(recovered.cumulative_lost, 0);
        assert_eq!(recovered.fraction_lost, 0);
    }

    #[test]
    fn raw_receiver_report_advances_interval_loss_counters() {
        let config = config();
        let feedback = config.feedback();
        let crypto = test_srtp(&config);
        let first = protect_for_test(
            &crypto,
            build_plaintext_rtp(10, FLAG_SOF | FLAG_EOF, 1, &[0, 0, 1, 0x65]),
            0,
        );
        let later = protect_for_test(
            &crypto,
            build_plaintext_rtp(12, FLAG_SOF | FLAG_EOF, 2, &[0, 0, 1, 0x65]),
            0,
        );
        let now = Instant::now();
        let mut receiver = NvstVideoReceiver::new(config);
        let _ = receiver.process_datagram(peer(), &first, now);
        let _ = receiver.process_datagram(peer(), &later, now + Duration::from_millis(20));

        assert!(receiver.poll_receiver_report(now).is_some());
        assert_eq!(
            *feedback
                .report_prior
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            (3, 2)
        );
    }

    #[test]
    fn raw_receiver_report_is_disabled_when_rtcp_is_on_sctp() {
        let mut handoff = legacy_handoff();
        handoff["rtcpOnSctp"] = json!(true);
        let config = NvstVideoConfig::from_legacy_handoff(&handoff, None).expect("valid config");
        assert!(config.rtcp_on_sctp());
        let crypto = test_srtp(&config);
        let packet = protect_for_test(
            &crypto,
            build_plaintext_rtp(10, FLAG_SOF | FLAG_EOF, 1, &[0, 0, 1, 0x65]),
            0,
        );
        let now = Instant::now();
        let mut receiver = NvstVideoReceiver::new(config);
        let _ = receiver.process_datagram(peer(), &packet, now);

        assert!(receiver.poll_receiver_report(now).is_none());
    }

    #[test]
    fn duplicate_rtp_timestamps_do_not_inflate_interarrival_jitter() {
        let feedback = NvstFeedbackState::default();
        let now = Instant::now();
        feedback.publish_stream(7, 10, 1_000, now);
        feedback.publish_stream(7, 11, 1_000, now + Duration::from_millis(10));
        feedback.publish_stream(7, 12, 2_800, now + Duration::from_millis(20));

        assert_eq!(feedback.report_snapshot(false).expect("report").jitter, 0);
    }

    #[test]
    fn rtp_classifier_does_not_bind_rtcp_or_dtls_as_media_streams() {
        let rtcp = build_rtcp_receiver_report(
            1,
            RtcpReportBlock {
                media_ssrc: 2,
                fraction_lost: 0,
                cumulative_lost: 0,
                highest_sequence: 3,
                jitter: 0,
            },
        );
        assert!(looks_like_rtcp(&rtcp));
        assert!(!looks_like_rtp(&rtcp));
        assert_eq!(peek_rtp_ssrc(&rtcp), None);

        let mut dtls = vec![0_u8; RTP_FIXED_HEADER_LEN];
        dtls[0] = 23;
        assert!(looks_like_dtls(&dtls));
        assert!(!looks_like_rtp(&dtls));
    }

    #[test]
    fn generic_nack_is_bounded() {
        let packet = build_rtcp_nack(1, 2, 10, 10_000);
        assert_eq!(packet.len(), 12 + MAX_NACK_FCI_ENTRIES * 4);
        assert_eq!(
            &packet[2..4],
            &(2_u16 + MAX_NACK_FCI_ENTRIES as u16).to_be_bytes()
        );
        assert_eq!(&packet[packet.len() - 4..], &[0, 61, 0x0f, 0xff]);
    }

    #[test]
    fn pending_nack_ranges_are_chunked_without_discarding_tail() {
        let feedback = NvstFeedbackState::default();
        feedback.request_nack(10, 100);
        feedback.resolve_nack(12);
        assert_eq!(feedback.take_nack(), Some((10, 11)));
        assert_eq!(feedback.take_nack(), Some((13, 76)));
        assert_eq!(feedback.take_nack(), Some((77, 100)));
        assert_eq!(feedback.take_nack(), None);

        feedback.request_nack(10, 100);
        assert_eq!(feedback.take_nack(), Some((10, 73)));
        assert_eq!(feedback.take_nack(), Some((74, 100)));
        assert_eq!(feedback.take_nack(), None);
    }

    #[test]
    fn pause_timeout_recovery_and_stop_fail_closed() {
        let mut receiver = NvstVideoReceiver::new(config());
        assert_eq!(
            receiver.pause(),
            Some(NvstReceiveEvent::Lifecycle(NvstReceiverState::Paused))
        );
        assert!(matches!(
            receiver
                .process_datagram(peer(), &[], Instant::now())
                .as_slice(),
            [NvstReceiveEvent::Dropped(NvstDropReason::Paused)]
        ));
        assert_eq!(
            receiver.resume(),
            Some(NvstReceiveEvent::Lifecycle(NvstReceiverState::Running))
        );
        receiver.last_authenticated_packet = Some(Instant::now() - Duration::from_secs(1));
        assert!(matches!(
            receiver.poll_timeout(Instant::now()),
            Some(NvstReceiveEvent::RecoveryNeeded(
                NvstRecovery::Timeout { .. }
            ))
        ));
        assert_eq!(receiver.state(), NvstReceiverState::RecoveryRequired);
        assert_eq!(
            receiver.recover(),
            Some(NvstReceiveEvent::Lifecycle(NvstReceiverState::Running))
        );
        assert_eq!(
            receiver.stop(),
            Some(NvstReceiveEvent::Lifecycle(NvstReceiverState::Stopped))
        );
        assert!(matches!(
            receiver
                .process_datagram(peer(), &[], Instant::now())
                .as_slice(),
            [NvstReceiveEvent::Dropped(NvstDropReason::Stopped)]
        ));
    }

    #[test]
    fn startup_without_authenticated_packets_times_out() {
        let mut receiver = NvstVideoReceiver::new(config());
        receiver.timeout_origin = Instant::now() - Duration::from_secs(1);

        assert!(matches!(
            receiver.poll_timeout(Instant::now()),
            Some(NvstReceiveEvent::RecoveryNeeded(
                NvstRecovery::Timeout { .. }
            ))
        ));
        assert_eq!(receiver.state(), NvstReceiverState::RecoveryRequired);
    }

    #[test]
    fn udp_receiver_repeats_the_negotiated_ping_before_media_arrives() {
        let server = UdpSocket::bind("127.0.0.1:0").expect("server socket");
        server
            .set_read_timeout(Some(Duration::from_millis(250)))
            .expect("server timeout");
        let client_reservation = UdpSocket::bind("127.0.0.1:0").expect("client reservation");
        let client_port = client_reservation
            .local_addr()
            .expect("client address")
            .port();
        drop(client_reservation);

        let mut config = config();
        config.client_udp_port = client_port;
        config.video_peer = server.local_addr().expect("server address");
        config.ping_payload = b"negotiated-ping".to_vec();
        let (media_consumer, _media_receiver) = mpsc::sync_channel(1);
        let (event_sender, _event_receiver) = mpsc::channel();
        let session =
            spawn_nvst_udp_receiver(config, media_consumer, event_sender).expect("UDP receiver");

        let mut datagram = [0_u8; 64];
        let (first_len, _) = server.recv_from(&mut datagram).expect("first ping");
        assert_eq!(&datagram[..first_len], b"negotiated-ping");
        let (second_len, _) = server.recv_from(&mut datagram).expect("repeated ping");
        assert_eq!(&datagram[..second_len], b"negotiated-ping");

        session.stop();
    }

    #[test]
    fn mjolnir_receiver_sends_only_version_six_stun_with_the_setup_identity() {
        let server = UdpSocket::bind("127.0.0.1:0").expect("server socket");
        server
            .set_read_timeout(Some(Duration::from_millis(250)))
            .expect("server timeout");
        let client = UdpSocket::bind("127.0.0.1:0").expect("client socket");

        let mut config = config();
        config.client_udp_port = client.local_addr().expect("client address").port();
        config.video_peer = server.local_addr().expect("server address");
        config.ping_payload = b"setup-ping".to_vec();
        config.stun_credentials = Some(stun_credentials());
        let (media_consumer, _media_receiver) = mpsc::sync_channel(1);
        let (event_sender, _event_receiver) = mpsc::channel();
        let session = spawn_nvst_mjolnir_receiver(client, config, media_consumer, event_sender)
            .expect("Mjolnir receiver");

        let mut datagram = [0_u8; 512];
        let (setup_len, _) = server
            .recv_from(&mut datagram)
            .expect("SETUP identity probe");
        let (_, setup_username) =
            find_stun_attribute(&datagram[..setup_len], STUN_ATTR_USERNAME).expect("USERNAME");
        assert_eq!(setup_username, b"setup-ping:loc1");

        let (repeated_len, _) = server
            .recv_from(&mut datagram)
            .expect("repeated SETUP identity probe");
        let (_, repeated_username) =
            find_stun_attribute(&datagram[..repeated_len], STUN_ATTR_USERNAME).expect("USERNAME");
        assert_eq!(repeated_username, b"setup-ping:loc1");

        session.stop();
    }

    #[test]
    fn h264_frame_queue_is_bounded_and_prefers_current_frames() {
        let mut queue = BoundedFrameQueue::new(2);
        for frame_index in 1..=3 {
            queue.push(EncodedVideoAccessUnit {
                codec: NvstVideoCodec::H264,
                timestamp: frame_index,
                frame_index,
                first_stream_packet_index: frame_index,
                keyframe: false,
                contiguous: true,
                bytes: vec![frame_index as u8],
            });
        }
        assert_eq!(queue.dropped_frames(), 1);
        assert_eq!(queue.pop().expect("frame").frame_index, 2);
        assert_eq!(queue.pop().expect("frame").frame_index, 3);
    }

    fn hex_bytes(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .chunks(2)
            .map(|chunk| {
                u8::from_str_radix(std::str::from_utf8(chunk).expect("hex"), 16).expect("hex")
            })
            .collect()
    }
}

/// In-process bounded queue for callers that drive `NvstVideoReceiver` directly rather than
/// using the UDP worker. It drops the oldest frame to keep interactive latency bounded.
#[derive(Debug)]
pub struct BoundedFrameQueue {
    frames: VecDeque<EncodedVideoAccessUnit>,
    capacity: usize,
    dropped_frames: u64,
}

impl BoundedFrameQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            frames: VecDeque::with_capacity(capacity),
            capacity: capacity.max(1),
            dropped_frames: 0,
        }
    }

    pub fn push(&mut self, frame: EncodedVideoAccessUnit) {
        if self.frames.len() == self.capacity {
            let _ = self.frames.pop_front();
            self.dropped_frames += 1;
        }
        self.frames.push_back(frame);
    }

    pub fn pop(&mut self) -> Option<EncodedVideoAccessUnit> {
        self.frames.pop_front()
    }

    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }
}
