//! Independently authored NVST video receive transport.
//!
//! This module only implements the receive side of the classic NVST video handoff:
//! authenticated SRTP video datagrams from the negotiated peer become bounded H.264
//! Annex-B access units. It deliberately does not implement NVST audio, control, input,
//! FEC repair, or NACK transmission because the current handoff does not contain enough
//! wire information to implement those features safely.

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use aes::cipher::{BlockEncrypt, KeyIvInit, StreamCipher};
use aes::{Aes128, Aes256};
use crc32fast::hash as crc32;
use ctr::{Ctr128BE, Ctr32BE};
use ghash::{GHash, universal_hash::UniversalHash};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha1::Sha1;
use socket2::{Domain, Protocol, Socket, Type};
use subtle::ConstantTimeEq;
use thiserror::Error;

use str0m::channel::{ChannelConfig, ChannelId, Reliability};
use str0m::config::Fingerprint;
use str0m::media::{MediaKind, Mid};
use str0m::net::{Protocol as RtcProtocol, Receive};
use str0m::rtp::Ssrc;
use str0m::{Candidate, Event, IceCreds, Input, Output, Rtc, RtcConfig};

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
const SRTCP_RR_INTERVAL: Duration = Duration::from_secs(1);
/// How many even SCTP stream ids to try for the `rtcp1` feedback channel. The
/// server resets the DCEP open on the wrong id, so we probe the low even ids.
const RTCP_STREAM_CANDIDATES: usize = 8;
const NV_VIDEO_PACKET_LEN: usize = 16;
const DEFAULT_REORDER_WINDOW: usize = 32;
const MAX_REORDER_WINDOW: usize = 128;
const DEFAULT_MAX_ACCESS_UNIT_BYTES: usize = 2 * 1024 * 1024;
const MAX_ACCESS_UNIT_BYTES: usize = 16 * 1024 * 1024;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const MIN_TIMEOUT: Duration = Duration::from_millis(250);
const MAX_TIMEOUT: Duration = Duration::from_secs(90);
const MAX_PING_BYTES: usize = 512;
const PING_INTERVAL_BEFORE_CONNECTION: Duration = Duration::from_millis(20);
const PING_INTERVAL_AFTER_CONNECTION: Duration = Duration::from_millis(100);
const UDP_RECEIVE_POLL_INTERVAL: Duration = Duration::from_millis(10);
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

/// The only media codec this receive path currently exposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvstVideoCodec {
    H264,
}

impl NvstVideoCodec {
    fn parse(value: &str) -> Result<Self, NvstConfigError> {
        match value.trim().to_ascii_uppercase().as_str() {
            "H264" | "AVC" => Ok(Self::H264),
            other => Err(NvstConfigError::UnsupportedCodec(other.to_owned())),
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
    #[error("NVST video codec {0} is not implemented; only H264 Annex-B is available")]
    UnsupportedCodec(String),
    #[error("nvstTransport.tracks is not implemented yet; retain the legacy nvstVideo handoff")]
    RichHandoffUnsupported,
}

/// Explicitly records transport features that cannot be sent correctly from current wire data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvstUnsupportedFeature {
    Audio,
    Input,
    Nack,
    FecRepair,
}

impl fmt::Display for NvstUnsupportedFeature {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Audio => "audio",
            Self::Input => "input",
            Self::Nack => "NACK transmission",
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
pub struct NvstFeedbackState {
    /// Bound video stream SSRC (0 until the first packet is authenticated).
    video_ssrc: AtomicU32,
    /// Highest extended sequence number received on the video stream.
    highest_sequence: AtomicU32,
    /// Set when the receiver hits unrecoverable loss and needs a fresh keyframe.
    keyframe_needed: AtomicBool,
}

impl NvstFeedbackState {
    fn publish_stream(&self, ssrc: u32, highest_sequence: u32) {
        self.video_ssrc.store(ssrc, Ordering::Release);
        self.highest_sequence
            .fetch_max(highest_sequence, Ordering::AcqRel);
    }

    fn request_keyframe(&self) {
        self.keyframe_needed.store(true, Ordering::Release);
    }

    /// SSRC + highest sequence for the next Receiver Report, if a stream is bound.
    fn stream_snapshot(&self) -> Option<(u32, u32)> {
        let ssrc = self.video_ssrc.load(Ordering::Acquire);
        (ssrc != 0).then(|| (ssrc, self.highest_sequence.load(Ordering::Acquire)))
    }

    /// Atomically takes the pending keyframe request, returning true if one was set.
    fn take_keyframe_request(&self) -> bool {
        self.keyframe_needed.swap(false, Ordering::AcqRel)
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
    srtp: NvstSrtpMaterial,
    ping_payload: Vec<u8>,
    ping_version: Option<u8>,
    stun_credentials: Option<NvstStunCredentials>,
    remote_dtls_fingerprint: Option<String>,
    /// Dedicated NATT-only video (Mjolnir) socket port in the official two-socket
    /// cloud model. When set, video RTP/SRTP arrives on this socket while the
    /// ICE/DTLS bundle socket only carries control/audio keepalive traffic.
    mjolnir_udp_port: Option<u16>,
    codec: NvstVideoCodec,
    expected_payload_type: Option<u8>,
    expected_ssrc: Option<u32>,
    reorder_window_packets: usize,
    max_access_unit_bytes: usize,
    timeout: Duration,
    /// Feedback plane shared with the ICE/DTLS bundle (cloned configs share it).
    feedback: SharedNvstFeedback,
}

impl fmt::Debug for NvstVideoConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NvstVideoConfig")
            .field("client_udp_port", &self.client_udp_port)
            .field("video_peer", &self.video_peer)
            .field("srtp", &self.srtp)
            .field("ping_payload_len", &self.ping_payload.len())
            .field("ping_version", &self.ping_version)
            .field("stun_credentials", &self.stun_credentials)
            .field(
                "remote_dtls_fingerprint_bytes",
                &self.remote_dtls_fingerprint.as_ref().map(String::len),
            )
            .field("mjolnir_udp_port", &self.mjolnir_udp_port)
            .field("codec", &self.codec)
            .field("expected_payload_type", &self.expected_payload_type)
            .field("expected_ssrc", &self.expected_ssrc)
            .field("reorder_window_packets", &self.reorder_window_packets)
            .field("max_access_unit_bytes", &self.max_access_unit_bytes)
            .field("timeout", &self.timeout)
            .finish()
    }
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
    /// Parses the stable legacy `nvstVideo` object. The parsing boundary is intentionally
    /// isolated here so a future `nvstTransport.tracks[]` handoff can normalize into this
    /// same configuration without changing the SRTP/RTP receiver.
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

        Ok(Self {
            client_udp_port,
            video_peer: SocketAddr::new(peer_ip, video_peer_port),
            srtp,
            ping_payload,
            ping_version,
            stun_credentials,
            remote_dtls_fingerprint,
            mjolnir_udp_port,
            codec,
            expected_payload_type,
            expected_ssrc,
            reorder_window_packets,
            max_access_unit_bytes,
            timeout,
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

    pub fn codec(&self) -> NvstVideoCodec {
        self.codec
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

    pub fn mjolnir_udp_port(&self) -> Option<u16> {
        self.mjolnir_udp_port
    }
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
            return Err(NvstConfigError::RichHandoffUnsupported);
        }
        return Ok(None);
    };
    let settings_codec = context.pointer("/settings/codec").and_then(Value::as_str);
    NvstVideoConfig::from_legacy_handoff(handoff, settings_codec).map(Some)
}

/// The transport selector always prefers a valid NVST video handoff, while making every
/// incomplete or unsupported handoff a typed WebRTC fallback instead of an optimistic start.
#[derive(Debug)]
pub enum PreferredVideoTransport {
    Nvst(NvstVideoConfig),
    WebRtcFallback(NvstFallbackReason),
}

#[derive(Debug)]
pub enum NvstFallbackReason {
    NoNvstHandoff,
    InvalidNvstHandoff(NvstConfigError),
}

pub fn select_preferred_video_transport(context: &Value) -> PreferredVideoTransport {
    match parse_nvst_video_handoff(context) {
        Ok(Some(config)) => PreferredVideoTransport::Nvst(config),
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

/// A received H.264 byte-stream access unit ready for a decoder queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedH264Frame {
    pub timestamp: u32,
    pub frame_index: u32,
    pub first_stream_packet_index: u32,
    pub keyframe: bool,
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
    AccessUnitTooLarge {
        limit: usize,
    },
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
        nack: NvstUnsupportedFeature,
    },
    Timeout {
        idle_for: Duration,
    },
}

/// All receive decisions are explicit so callers can collect operational metrics without
/// treating malformed network traffic as a fatal thread error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NvstReceiveEvent {
    Frame(EncodedH264Frame),
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
            if profile == GS_VIDEO_EXTENSION_PROFILE && extension_len >= NV_VIDEO_PACKET_LEN + 4 {
                let body = &packet[payload_offset + 4..payload_offset + extension_len];
                gs_video_header = Some(
                    body[..NV_VIDEO_PACKET_LEN]
                        .try_into()
                        .expect("length checked"),
                );
            }
            payload_offset = payload_offset
                .checked_add(extension_len)
                .ok_or(RtpParseError::InvalidExtensionLength)?;
            if packet.len() < payload_offset {
                return Err(RtpParseError::InvalidExtensionLength);
            }
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

#[derive(Debug)]
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
                encryption_key: derive_aes128_cm_key::<16>(master_key, master_salt, GFN_SRTP_KEY_LABEL),
                session_salt: derive_aes128_cm_key::<12>(master_key, master_salt, GFN_SRTP_SALT_LABEL),
                authentication_tag_len: *authentication_tag_len,
            },
            NvstSrtpMaterial::AeadAes256Gcm {
                master_key,
                master_salt,
                authentication_tag_len,
            } => SrtpCipher::AeadAes256Gcm {
                encryption_key: derive_aes_cm_key::<32>(master_key, master_salt, GFN_SRTP_KEY_LABEL),
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
    Aes128(Ctr32BE<Aes128>),
    Aes256(Ctr32BE<Aes256>),
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
            GcmCtr::Aes128(ctr)
        }
        32 => {
            let key: &[u8; 32] = encryption_key.try_into().expect("32-byte GCM key");
            let aes = <Aes256 as aes::cipher::KeyInit>::new(key.into());
            aes.encrypt_block((&mut hash_key).into());
            let mut ctr = Ctr32BE::<Aes256>::new(key.into(), (&j0).into());
            ctr.apply_keystream(&mut tag_mask);
            GcmCtr::Aes256(ctr)
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
    let (tag, _) = aes_gcm_tag_and_ctr(
        encryption_key,
        iv,
        &aad_owned,
        &packet[payload_offset..],
    );
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

/// Builds and SRTCP-protects an RTCP Receiver Report (RFC 3550 §6.4.1) carrying one
/// report block for `media_ssrc`. GCM layout (RFC 7714 §9): the first 8 bytes stay
/// cleartext, only the report block is encrypted, then the E|index word and the
/// truncated auth tag are appended. AAD = cleartext header || E|index.
#[allow(clippy::too_many_arguments)]
fn protect_srtcp_receiver_report_gcm(
    sender_ssrc: u32,
    media_ssrc: u32,
    highest_sequence: u32,
    srtcp_index: u32,
    encryption_key: &[u8],
    session_salt: &[u8; 12],
    tag_len: usize,
) -> Vec<u8> {
    let mut packet = Vec::with_capacity(32 + 4 + tag_len);
    packet.push(0x81); // V=2, P=0, RC=1 (one report block)
    packet.push(201); // PT=RR
    packet.extend_from_slice(&7_u16.to_be_bytes()); // length: 8 words - 1
    packet.extend_from_slice(&sender_ssrc.to_be_bytes());
    packet.extend_from_slice(&media_ssrc.to_be_bytes());
    packet.push(0); // fraction lost
    packet.extend_from_slice(&[0, 0, 0]); // cumulative packets lost (24-bit)
    packet.extend_from_slice(&highest_sequence.to_be_bytes());
    packet.extend_from_slice(&0_u32.to_be_bytes()); // interarrival jitter
    packet.extend_from_slice(&0_u32.to_be_bytes()); // LSR
    packet.extend_from_slice(&0_u32.to_be_bytes()); // DLSR

    let e_index = SRTCP_ENCRYPTED_FLAG | (srtcp_index & !SRTCP_ENCRYPTED_FLAG);
    let iv = srtcp_gcm_iv(*session_salt, sender_ssrc, srtcp_index);
    let mut aad = packet[..8].to_vec();
    aad.extend_from_slice(&e_index.to_be_bytes());
    let (_, mut ctr) = aes_gcm_tag_and_ctr(encryption_key, &iv, &aad, &[]);
    ctr.apply_keystream(&mut packet[8..]);
    let (tag, _) = aes_gcm_tag_and_ctr(encryption_key, &iv, &aad, &packet[8..]);
    packet.extend_from_slice(&e_index.to_be_bytes());
    packet.extend_from_slice(&tag[..tag_len]);
    packet
}

/// Builds a plain (unencrypted) RTCP Receiver Report (RFC 3550 §6.4.1) with one
/// report block for `media_ssrc`. Sent over the `rtcp1` SCTP data channel, which
/// is already encrypted by DTLS, so no SRTCP layer is applied.
fn build_rtcp_receiver_report(sender_ssrc: u32, media_ssrc: u32, highest_sequence: u32) -> Vec<u8> {
    let mut packet = Vec::with_capacity(32);
    packet.push(0x81); // V=2, P=0, RC=1 (one report block)
    packet.push(201); // PT=RR
    packet.extend_from_slice(&7_u16.to_be_bytes()); // length: 8 words - 1
    packet.extend_from_slice(&sender_ssrc.to_be_bytes());
    packet.extend_from_slice(&media_ssrc.to_be_bytes());
    packet.push(0); // fraction lost
    packet.extend_from_slice(&[0, 0, 0]); // cumulative packets lost (24-bit)
    packet.extend_from_slice(&highest_sequence.to_be_bytes());
    packet.extend_from_slice(&0_u32.to_be_bytes()); // interarrival jitter
    packet.extend_from_slice(&0_u32.to_be_bytes()); // LSR
    packet.extend_from_slice(&0_u32.to_be_bytes()); // DLSR
    packet
}

/// Builds a plain RTCP Picture Loss Indication (RFC 4585 §6.3.1) asking the
/// sender of `media_ssrc` for a fresh keyframe.
fn build_rtcp_pli(sender_ssrc: u32, media_ssrc: u32) -> Vec<u8> {
    let mut packet = Vec::with_capacity(12);
    packet.push(0x81); // V=2, P=0, FMT=1 (PLI)
    packet.push(192); // PT=PSFB (payload-specific feedback)
    packet.extend_from_slice(&2_u16.to_be_bytes()); // length: 3 words - 1
    packet.extend_from_slice(&sender_ssrc.to_be_bytes());
    packet.extend_from_slice(&media_ssrc.to_be_bytes());
    packet
}

#[derive(Clone, Default)]
struct ReplayWindow {
    highest_index: Option<u64>,
    seen: u64,
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
        if age >= 64 || self.seen & (1_u64 << age) != 0 {
            return Err(NvstDropReason::ReplayRejected);
        }
        Ok(())
    }

    fn commit(&mut self, index: u64) {
        match self.highest_index {
            None => {
                self.highest_index = Some(index);
                self.seen = 1;
            }
            Some(highest_index) if index > highest_index => {
                let advance = index - highest_index;
                self.seen = if advance >= 64 {
                    1
                } else {
                    (self.seen << advance) | 1
                };
                self.highest_index = Some(index);
            }
            Some(highest_index) => {
                self.seen |= 1_u64 << (highest_index - index);
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct NvVideoPacket {
    stream_packet_index: u32,
    frame_index: u32,
    flags: u8,
    is_fec: bool,
}

impl NvVideoPacket {
    /// Reads the Mjolnir video metadata from the `0x4753` ("GS") RTP extension:
    /// a 16-byte little-endian block holding the stream packet index, frame index,
    /// the packet-type nibble (picture data / SOF / EOF), and FEC group
    /// coordinates. The RTP payload itself is pure H.264 access-unit data.
    fn parse<'a>(header: &RtpHeader, payload: &'a [u8]) -> Result<(Self, &'a [u8]), RtpParseError> {
        let Some(extension) = header.gs_video_header else {
            return Err(RtpParseError::MissingNvVideoHeader);
        };
        let packet_word = u32::from_le_bytes(extension[0..4].try_into().expect("length checked"));
        let flags_word = u32::from_le_bytes(extension[8..12].try_into().expect("length checked"));
        let fec_word = u32::from_le_bytes(extension[12..16].try_into().expect("length checked"));
        let fec_index = (fec_word >> 12) & 0x3ff;
        let fec_source_packets = (fec_word >> 22) & 0x3ff;
        let packet = Self {
            stream_packet_index: (packet_word >> 8) & STREAM_PACKET_INDEX_MASK,
            frame_index: u32::from_le_bytes(extension[4..8].try_into().expect("length checked")),
            flags: (flags_word & 0x0f) as u8,
            is_fec: (fec_word >> 8) & 0xff != 0 && fec_index >= fec_source_packets,
        };
        Ok((packet, payload))
    }

    fn contains_picture_data(self) -> bool {
        self.flags & FLAG_CONTAINS_PIC_DATA != 0
    }

    fn is_start_of_frame(self) -> bool {
        self.flags & FLAG_SOF != 0
    }

    fn is_end_of_frame(self) -> bool {
        self.flags & FLAG_EOF != 0
    }
}

struct H264AccessUnitAssembler {
    current_frame: Option<u32>,
    first_stream_packet_index: Option<u32>,
    bytes: Vec<u8>,
    max_access_unit_bytes: usize,
}

impl H264AccessUnitAssembler {
    fn new(max_access_unit_bytes: usize) -> Self {
        Self {
            current_frame: None,
            first_stream_packet_index: None,
            bytes: Vec::new(),
            max_access_unit_bytes,
        }
    }

    fn reset(&mut self) {
        self.current_frame = None;
        self.first_stream_packet_index = None;
        self.bytes.clear();
    }

    fn push(
        &mut self,
        header: NvVideoPacket,
        timestamp: u32,
        payload: &[u8],
    ) -> Result<Option<EncodedH264Frame>, NvstDropReason> {
        if header.is_fec || !header.contains_picture_data() {
            return Err(NvstDropReason::Unsupported(
                NvstUnsupportedFeature::FecRepair,
            ));
        }
        if header.is_start_of_frame() {
            self.reset();
            let Some(payload) = h264_picture_payload(payload) else {
                return Err(NvstDropReason::MissingAnnexBStartCode);
            };
            self.current_frame = Some(header.frame_index);
            self.first_stream_packet_index = Some(header.stream_packet_index);
            let remaining = self.max_access_unit_bytes;
            if payload.len() > remaining {
                self.reset();
                return Err(NvstDropReason::AccessUnitTooLarge {
                    limit: self.max_access_unit_bytes,
                });
            }
            self.bytes.extend_from_slice(payload);
        } else if self.current_frame != Some(header.frame_index) {
            self.reset();
            return Err(NvstDropReason::AwaitingStartOfFrame);
        } else {
            let remaining = self.max_access_unit_bytes.saturating_sub(self.bytes.len());
            if payload.len() > remaining {
                self.reset();
                return Err(NvstDropReason::AccessUnitTooLarge {
                    limit: self.max_access_unit_bytes,
                });
            }
            self.bytes.extend_from_slice(payload);
        }
        if !header.is_end_of_frame() {
            return Ok(None);
        }

        let bytes = std::mem::take(&mut self.bytes);
        self.current_frame = None;
        let first_stream_packet_index = self
            .first_stream_packet_index
            .take()
            .expect("start-of-frame initializes the packet index");
        Ok(Some(EncodedH264Frame {
            timestamp,
            frame_index: header.frame_index,
            first_stream_packet_index,
            keyframe: h264_access_unit_is_keyframe(&bytes),
            bytes,
        }))
    }
}

static NVST_DEBUG_DUMP_REMAINING: AtomicU64 = AtomicU64::new(96);

/// Temporary ground-truth dump of decrypted Mjolnir video packets so the
/// RTP extension + NV_VIDEO_PACKET layout can be verified against live traffic.
fn debug_dump_nv_packet(path: &str, packet: &[u8], payload_offset: usize) {
    use std::fmt::Write as _;
    if NVST_DEBUG_DUMP_REMAINING
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
            remaining.checked_sub(1)
        })
        .is_err()
    {
        return;
    }
    let dump_len = packet.len().min(56);
    let mut packet_hex = String::with_capacity(dump_len * 2 + 8);
    for (index, byte) in packet[..dump_len].iter().enumerate() {
        if index == 12 || index == payload_offset {
            let _ = write!(packet_hex, "|");
        }
        let _ = write!(packet_hex, "{byte:02x}");
    }
        eprintln!(
            "NVST pkt-dump[{path}] len={} payloadOff={payload_offset} bytes={packet_hex}",
            packet.len(),
        );
    }

    static NVST_DEBUG_FRAME_DUMP_REMAINING: AtomicU64 = AtomicU64::new(24);

    /// Temporary dump of assembled access units to verify NAL layout/keyframe
    /// detection against live traffic.
    fn debug_dump_frame(frame: &EncodedH264Frame) {
        use std::fmt::Write as _;
        if NVST_DEBUG_FRAME_DUMP_REMAINING
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                remaining.checked_sub(1)
            })
            .is_err()
        {
            return;
        }
        let dump_len = frame.bytes.len().min(40);
        let mut hex = String::with_capacity(dump_len * 2);
        for byte in &frame.bytes[..dump_len] {
            let _ = write!(hex, "{byte:02x}");
        }
        eprintln!(
            "NVST frame-dump len={} keyframe={} ts={} bytes={hex}",
            frame.bytes.len(),
            frame.keyframe,
            frame.timestamp,
        );
    }

fn h264_picture_payload(payload: &[u8]) -> Option<&[u8]> {
    let search_len = payload.len().min(MAX_GS_FRAME_HEADER_BYTES + 4);
    let (offset, _) = find_annex_b_start_code(&payload[..search_len])?;
    Some(&payload[offset..])
}

fn h264_access_unit_is_keyframe(bytes: &[u8]) -> bool {
    let mut offset = 0;
    while let Some((start, prefix_len)) = find_annex_b_start_code(&bytes[offset..]) {
        let nal_start = offset + start + prefix_len;
        if let Some(nal_header) = bytes.get(nal_start)
            && nal_header & 0x1f == 5
        {
            return true;
        }
        offset = nal_start;
    }
    false
}

fn find_annex_b_start_code(bytes: &[u8]) -> Option<(usize, usize)> {
    bytes
        .windows(4)
        .position(|window| window == [0, 0, 0, 1])
        .map_or_else(
            || {
                bytes
                    .windows(3)
                    .position(|window| window == [0, 0, 1])
                    .map(|position| (position, 3))
            },
            |position| Some((position, 4)),
        )
}

struct RtpReorderBuffer {
    next_index: Option<u64>,
    packets: BTreeMap<u64, RtpPacket>,
    max_packets: usize,
}

struct ReorderResult {
    ready: Vec<RtpPacket>,
    recovery: Option<NvstRecovery>,
    dropped: Option<NvstDropReason>,
}

impl RtpReorderBuffer {
    fn new(max_packets: usize) -> Self {
        Self {
            next_index: None,
            packets: BTreeMap::new(),
            max_packets,
        }
    }

    fn reset(&mut self) {
        self.next_index = None;
        self.packets.clear();
    }

    fn push(&mut self, packet: RtpPacket) -> ReorderResult {
        let index = packet.index;
        let next_index = *self.next_index.get_or_insert(index);
        if index < next_index {
            return ReorderResult {
                ready: Vec::new(),
                recovery: None,
                dropped: Some(NvstDropReason::StaleRtpPacket { index }),
            };
        }
        if self.packets.contains_key(&index) {
            return ReorderResult {
                ready: Vec::new(),
                recovery: None,
                dropped: Some(NvstDropReason::DuplicateRtpPacket { index }),
            };
        }

        let mut recovery = None;
        if index.saturating_sub(next_index) >= self.max_packets as u64 {
            recovery = Some(NvstRecovery::PacketGap {
                first_missing_index: next_index,
                last_missing_index: index - 1,
                nack: NvstUnsupportedFeature::Nack,
            });
            self.packets.clear();
            self.next_index = Some(index);
        }
        self.packets.insert(index, packet);

        if self.packets.len() >= self.max_packets {
            let first_available = *self
                .packets
                .first_key_value()
                .expect("non-empty after insertion")
                .0;
            let expected = self.next_index.expect("next index set above");
            if first_available > expected {
                recovery = Some(NvstRecovery::PacketGap {
                    first_missing_index: expected,
                    last_missing_index: first_available - 1,
                    nack: NvstUnsupportedFeature::Nack,
                });
                self.next_index = Some(first_available);
            }
        }

        let mut ready = Vec::new();
        while let Some(next) = self.next_index {
            let Some(packet) = self.packets.remove(&next) else {
                break;
            };
            ready.push(packet);
            self.next_index = Some(next + 1);
        }
        ReorderResult {
            ready,
            recovery,
            dropped: None,
        }
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
    sender_ssrc: u32,
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
        let mut sender_ssrc = [0_u8; 4];
        if getrandom::fill(&mut sender_ssrc).is_err() {
            sender_ssrc = 0x4f4e_4f57_u32.to_be_bytes(); // "ONOW"
        }
        Some(Self {
            encryption_key,
            session_salt,
            tag_len,
            sender_ssrc: u32::from_be_bytes(sender_ssrc),
            next_index: 0,
            last_sent: None,
        })
    }

    /// Returns an SRTCP Receiver Report to send once per interval, after the media
    /// SSRC is known. Returns `None` when it is not yet time or nothing to report on.
    fn poll_receiver_report(
        &mut self,
        media_ssrc: Option<u32>,
        highest_sequence: u32,
        now: Instant,
    ) -> Option<Vec<u8>> {
        let media_ssrc = media_ssrc?;
        if let Some(last) = self.last_sent
            && now.duration_since(last) < SRTCP_RR_INTERVAL
        {
            return None;
        }
        self.last_sent = Some(now);
        let index = self.next_index;
        self.next_index = self.next_index.wrapping_add(1);
        Some(protect_srtcp_receiver_report_gcm(
            self.sender_ssrc,
            media_ssrc,
            highest_sequence,
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
    assembler: H264AccessUnitAssembler,
    state: NvstReceiverState,
    bound_ssrc: Option<u32>,
    highest_sequence_received: u32,
    authenticated_packets: u64,
    fec_packets: u64,
    frames_emitted: u64,
    timeout_origin: Instant,
    last_authenticated_packet: Option<Instant>,
}

impl NvstVideoReceiver {
    pub fn new(config: NvstVideoConfig) -> Self {
        let srtp = SrtpReceiver::from_material(&config.srtp);
        let srtcp = SrtcpSender::from_material(&config.srtp);
        let reorder = RtpReorderBuffer::new(config.reorder_window_packets);
        let assembler = H264AccessUnitAssembler::new(config.max_access_unit_bytes);
        Self {
            config,
            srtp,
            srtcp,
            reorder,
            assembler,
            state: NvstReceiverState::Running,
            bound_ssrc: None,
            highest_sequence_received: 0,
            authenticated_packets: 0,
            fec_packets: 0,
            frames_emitted: 0,
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
            Err(reason) => return vec![NvstReceiveEvent::Dropped(reason)],
        };
        debug_dump_nv_packet("raw", &packet.plaintext, packet.header.payload_offset);
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
        self.config
            .feedback
            .publish_stream(packet.header.ssrc, self.highest_sequence_received);

        let result = self.reorder.push(packet);
        let mut events = Vec::new();
        if let Some(reason) = result.dropped {
            events.push(NvstReceiveEvent::Dropped(reason));
        }
        if let Some(recovery) = result.recovery {
            self.assembler.reset();
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
                    events.push(NvstReceiveEvent::Dropped(NvstDropReason::MalformedRtp(
                        error,
                    )));
                    continue;
                }
            };
            match self
                .assembler
                .push(nv_packet, packet.header.timestamp, media)
            {
                Ok(Some(frame)) => {
                    self.frames_emitted += 1;
                    debug_dump_frame(&frame);
                    events.push(NvstReceiveEvent::Frame(frame));
                }
                Ok(None) => {}
                Err(reason) => {
                    if matches!(reason, NvstDropReason::Unsupported(NvstUnsupportedFeature::FecRepair))
                    {
                        self.fec_packets += 1;
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
        self.srtcp.as_mut()?.poll_receiver_report(
            self.bound_ssrc,
            self.highest_sequence_received,
            now,
        )
    }

    /// Elapsed-since-start receive counters for ground-truth timing that does not
    /// depend on when buffered log lines happen to flush.
    pub fn stats_line(&self, origin: Instant) -> String {
        format!(
            "elapsed={:.1}s auth={} fec={} frames={} ssrc={:?}",
            origin.elapsed().as_secs_f64(),
            self.authenticated_packets,
            self.fec_packets,
            self.frames_emitted,
            self.bound_ssrc,
        )
    }

    fn process_mjolnir_payload(
        &mut self,
        ssrc: u32,
        rtp_timestamp: u32,
        payload: &[u8],
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
        debug_dump_nv_packet("bundle", payload, 0);
        // The bundle path cannot assemble video: the Mjolnir frame metadata lives
        // in the `0x4753` RTP extension, which str0m does not surface. The official
        // cloud path delivers video exclusively on the raw Mjolnir socket, so bundle
        // RTP (audio/control) is intentionally ignored here.
        let _ = rtp_timestamp;
        Vec::new()
    }

    fn reset_media_state(&mut self) {
        self.reorder.reset();
        self.assembler.reset();
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
    Stop,
}

/// Owns the bounded UDP receive worker. Frames go through the same bounded `MediaConsumer` used
/// by WebRTC, so a slow decoder cannot make UDP receive unbounded.
pub struct NvstUdpReceiverSession {
    commands: Sender<UdpReceiverCommand>,
    join: Option<JoinHandle<()>>,
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
    pub fn pause(&self) -> Result<(), NvstUdpReceiverError> {
        self.send(UdpReceiverCommand::Pause)
    }

    pub fn resume(&self) -> Result<(), NvstUdpReceiverError> {
        self.send(UdpReceiverCommand::Resume)
    }

    pub fn recover(&self) -> Result<(), NvstUdpReceiverError> {
        self.send(UdpReceiverCommand::Recover)
    }

    pub fn stop(mut self) {
        let _ = self.commands.send(UdpReceiverCommand::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
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
        let socket = reserve_nvst_udp_socket().map_err(NvstUdpReceiverError::Bind)?;
        let rtc = create_nvst_bundle_rtc(&socket)?;
        let mjolnir_socket =
            reserve_nvst_mjolnir_udp_socket().map_err(NvstUdpReceiverError::Bind)?;
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

fn generate_gfn_local_ice_credentials() -> IceCreds {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+/";
    let mut random = [0_u8; 26];
    let _ = getrandom::fill(&mut random);
    let encode = |start: usize, length: usize| {
        random[start..start + length]
            .iter()
            .map(|value| ALPHABET[usize::from(*value) & 0x3f] as char)
            .collect::<String>()
    };
    IceCreds {
        ufrag: encode(0, 4),
        pass: encode(4, 22),
    }
}

fn create_nvst_bundle_rtc(socket: &UdpSocket) -> Result<Rtc, NvstUdpReceiverError> {
    install_crypto();
    let _ = socket;
    // Official GenerateIceCredentials() is 4-char ufrag / 22-char password.
    // str0m's default 16-char ufrag is rejected by Bifrost length checks.
    let mut rtc = RtcConfig::new().set_rtp_mode(true).build(Instant::now());
    rtc.direct_api()
        .set_local_ice_credentials(generate_gfn_local_ice_credentials());
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
    let transport_origin = Instant::now();
    let join = thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            run_nvst_udp_receiver(
                socket,
                config,
                receiver,
                media_consumer,
                event_sender,
                transport_origin,
                rtc,
            )
        })
        .map_err(NvstUdpReceiverError::Spawn)?;
    Ok(NvstUdpReceiverSession {
        commands,
        join: Some(join),
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
    let local_candidate =
        Candidate::host(routed_host_addr(Some(config.video_peer), local_addr), "udp")
            .map_err(|error| NvstUdpReceiverError::WebrtcBundle(error.to_string()))?;
    let remote_candidate = Candidate::host(config.video_peer, "udp")
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
        api.declare_media(Mid::from("0"), MediaKind::Video);
        api.start_dtls(true)
            .map_err(|error| NvstUdpReceiverError::WebrtcBundle(error.to_string()))?;
    }
    eprintln!(
        "NVST WebRTC bundle armed (local={}, peer={}, remoteFingerprintBytes={})",
        routed_host_addr(Some(config.video_peer), local_addr),
        config.video_peer,
        fingerprint.len()
    );
    Ok(rtc)
}

fn looks_like_rtp(datagram: &[u8]) -> bool {
    datagram.len() >= RTP_FIXED_HEADER_LEN
        && datagram[0] >> 6 == 2
        && !looks_like_stun(datagram)
        && !looks_like_dtls(datagram)
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

fn run_nvst_webrtc_bundle(
    socket: UdpSocket,
    config: NvstVideoConfig,
    commands: Receiver<UdpReceiverCommand>,
    media_consumer: MediaConsumer,
    event_sender: Sender<NvstReceiveEvent>,
    transport_origin: Instant,
    mut rtc: Rtc,
) {
    let video_peer = config.video_peer;
    let stun_credentials = config.stun_credentials.clone();
    let ping_payload = config.ping_payload.clone();
    // Feedback plane shared with the Mjolnir video receiver: it publishes the
    // stream SSRC/sequence and keyframe requests; this bundle sends the RTCP
    // Receiver Reports / PLI over the `rtcp1` SCTP data channel.
    let feedback = config.feedback();
    // With a dedicated Mjolnir video socket the bundle only carries
    // control/audio keepalive traffic; the Mjolnir receiver owns the media
    // timeout, so the bundle must not raise a spurious media recovery.
    let owns_media_timeout = config.mjolnir_udp_port.is_none();
    let receive_destination = socket.local_addr().ok().map_or_else(
        || video_peer,
        |local| routed_host_addr(Some(video_peer), local),
    );
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
    let mut rtcp_sender_ssrc = 0x4f4e_4f57_u32; // "ONOW"
    rtcp_sender_ssrc ^= (transport_origin.elapsed().subsec_nanos()) & 0xffff;
    let mut last_rtcp_send = Instant::now() - SRTCP_RR_INTERVAL;
    let mut rtcp_reports_sent = 0_u64;
    // The official client opens `rtcp1` once the RTSP session is established,
    // not cold during the SCTP handshake (which gets the stream reset). Defer
    // creation to shortly after the handshake. Do NOT gate on video flowing:
    // the server may wait for rtcp1 before sending the keyframe, so gating on
    // video would deadlock.
    let mut rtcp_create_attempted = false;
    let mut sctp_started_at: Option<Instant> = None;
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
                        let _ = socket.send_to(&ice, video_peer);
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
                let _ = socket.send_to(&natt, video_peer);
                Some(natt)
            } else {
                None
            };
            hole_punch_pings += 1;
            if hole_punch_pings == 1 || hole_punch_pings % 50 == 0 {
                eprintln!(
                    "NVST hole-punch ping={hole_punch_pings} dest={video_peer} iceBurst={} iceBytes={ice_bytes} iceUsername={}:{} nattBytes={} nattUsername={}:{}",
                    if dtls_ready { 0 } else { 3 },
                    credentials.remote_username_fragment,
                    credentials.local_username_fragment,
                    natt.as_ref().map_or(0, Vec::len),
                    String::from_utf8_lossy(&ping_payload),
                    credentials.local_username_fragment
                );
            }
            last_hole_punch = now;
        }

        // Open the rtcp1 channel ~1s after the SCTP handshake, once the RTSP
        // session has had time to establish on the server.
        if sctp_started
            && !rtcp_create_attempted
            && rtcp_channel.is_none()
            && sctp_started_at.is_some_and(|t| now.duration_since(t) >= Duration::from_secs(1))
        {
            rtcp_create_attempted = true;
            // The official client's RTCP feedback channel is labelled
            // `rtcp_on_sctp_private` (not `rtcp1`). The server resets a DCEP open
            // whose label it doesn't recognize — which is why `rtcp1` was reset on
            // every stream id. Open candidates across the low even stream ids (client
            // parity) with the correct label; the server ACKs one with ChannelOpen and
            // resets the rest, and we adopt whichever opens.
            for _ in 0..RTCP_STREAM_CANDIDATES {
                let config = ChannelConfig {
                    label: "rtcp_on_sctp_private".to_string(),
                    negotiated: None,
                    reliability: Reliability::Reliable,
                    ordered: true,
                    protocol: String::new(),
                };
                let _ = rtc.direct_api().create_data_channel(config);
            }
            eprintln!("NVST creating rtcp_on_sctp_private across {RTCP_STREAM_CANDIDATES} stream-id candidates");
        }

        // Send RTCP feedback over the rtcp1 SCTP channel once it is open and the
        // Mjolnir receiver has bound the video stream. A Receiver Report goes out
        // every second; a PLI goes out whenever the receiver flags it needs a
        // keyframe (rate-limited to the same cadence).
        if rtcp_channel_open
            && now.duration_since(last_rtcp_send) >= SRTCP_RR_INTERVAL
            && let Some(channel_id) = rtcp_channel
            && let Some((media_ssrc, highest_sequence)) = feedback.stream_snapshot()
        {
            let mut channel = rtc.channel(channel_id);
            if let Some(channel) = channel.as_mut() {
                let report =
                    build_rtcp_receiver_report(rtcp_sender_ssrc, media_ssrc, highest_sequence);
                if channel.write(true, &report).unwrap_or(false) {
                    rtcp_reports_sent += 1;
                    if rtcp_reports_sent == 1 || rtcp_reports_sent % 10 == 0 {
                        eprintln!(
                            "NVST rtcp1 RR sent={rtcp_reports_sent} mediaSsrc={media_ssrc} highestSeq={highest_sequence}"
                        );
                    }
                }
                if feedback.take_keyframe_request() {
                    let pli = build_rtcp_pli(rtcp_sender_ssrc, media_ssrc);
                    if channel.write(true, &pli).unwrap_or(false) {
                        eprintln!("NVST rtcp1 PLI sent for mediaSsrc={media_ssrc}");
                    }
                }
            }
            last_rtcp_send = now;
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
                    if outbound_datagrams <= 8 || outbound_datagrams % 50 == 0 {
                        eprintln!(
                            "NVST WebRTC outbound={outbound_datagrams} kind={kind} dest={} bytes={}",
                            transmit.destination,
                            transmit.contents.len()
                        );
                    }
                    let _ = socket.send_to(&transmit.contents, transmit.destination);
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
                        eprintln!("NVST DTLS handshake complete; waiting for SRTP/Mjolnir");
                        // Bring up SCTP over the established DTLS transport. The server
                        // opens the `rtcp1` channel itself (matching the official client,
                        // which receives server-created channels); we just listen for it.
                        if !sctp_started {
                            sctp_started = true;
                            sctp_started_at = Some(Instant::now());
                            rtc.direct_api().start_sctp(true);
                            eprintln!("NVST SCTP started; will open rtcp1 after handshake settles");
                        }
                    }
                    Event::ChannelOpen(id, label) => {
                        eprintln!("NVST data channel open: id={id:?} label={label}");
                        if label.contains("rtcp") {
                            rtcp_channel = Some(id);
                            rtcp_channel_open = true;
                            // Ask for a keyframe immediately so the decoder can start.
                            feedback.request_keyframe();
                        }
                    }
                    Event::ChannelData(data) => {
                        // The server may send Sender Reports / control on rtcp1; log to
                        // learn the exact on-wire format it expects back.
                        eprintln!(
                            "NVST rtcp1 inbound: id={:?} binary={} bytes={} data={:02x?}",
                            data.id,
                            data.binary,
                            data.data.len(),
                            &data.data[..data.data.len().min(16)]
                        );
                    }
                    Event::RtpPacket(packet) => {
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
            .min(UDP_RECEIVE_POLL_INTERVAL);
        if wait.is_zero() {
            let _ = rtc.handle_input(Input::Timeout(Instant::now()));
        } else {
            let _ = socket.set_read_timeout(Some(wait));
            match socket.recv_from(&mut datagram) {
                Ok((length, source)) => {
                    inbound_datagrams += 1;
                    if inbound_datagrams == 1 || inbound_datagrams % 50 == 0 {
                        eprintln!(
                            "NVST WebRTC inbound={inbound_datagrams} source={source} bytes={length} dtlsReady={dtls_ready}"
                        );
                    }
                    if datagram[..length] == *b"PING" {
                        let _ = socket.send_to(b"PONG", source);
                        continue;
                    }
                    if let Some(ssrc) = peek_rtp_ssrc(&datagram[..length])
                        && seen_ssrcs.insert(ssrc)
                    {
                        rtc.direct_api().expect_stream_rx(
                            Ssrc::from(ssrc),
                            None,
                            Mid::from("0"),
                            None,
                        );
                        eprintln!("NVST expecting SSRC {ssrc} on bundle mid=0");
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
                            source,
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
    media_consumer: MediaConsumer,
    event_sender: Sender<NvstReceiveEvent>,
    transport_origin: Instant,
    rtc: Option<Rtc>,
) {
    if let Some(rtc) = rtc {
        run_nvst_webrtc_bundle(
            socket,
            config,
            commands,
            media_consumer,
            event_sender,
            transport_origin,
            rtc,
        );
        return;
    }
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
                let mut transaction_id = [0_u8; 12];
                if getrandom::fill(&mut transaction_id).is_ok() {
                    let ping = build_natt_hole_punch_request(
                        &credentials.local_username_fragment,
                        &receiver.config.ping_payload,
                        &credentials.remote_password,
                        &transaction_id,
                    );
                    let _ = socket.send_to(&ping, receiver.config.video_peer);
                    pings_sent += 1;
                }
            } else {
                let _ = socket.send_to(&receiver.config.ping_payload, receiver.config.video_peer);
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
                            if let Some(response) = response {
                                let _ = socket.send_to(&response, source);
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
                peer_seen |= source == receiver.config.video_peer;
                for event in receiver.process_datagram(source, &datagram[..length], Instant::now())
                {
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
            if socket.send_to(&report, receiver.config.video_peer).is_ok() {
                receiver_reports_sent += 1;
            }
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
            codec: "H264".to_owned(),
            payload: Arc::from(frame.bytes),
            rtp_timestamp: u64::from(frame.timestamp),
            clock_rate_hz: 90_000,
            received_at_us: transport_origin
                .elapsed()
                .as_micros()
                .try_into()
                .unwrap_or(u64::MAX),
            keyframe: frame.keyframe,
            contiguous: true,
        };
        let result = match deliver_media_frame(media_consumer, media_frame) {
            Ok(()) => return true,
            Err(TransportError::MediaConsumerBackpressured) => {
                NvstDropReason::MediaConsumerBackpressured
            }
            Err(TransportError::MediaConsumerClosed) => NvstDropReason::MediaConsumerClosed,
            Err(_) => NvstDropReason::MediaConsumerClosed,
        };
        let _ = event_sender.send(NvstReceiveEvent::Dropped(result));
        return false;
    }
    let _ = event_sender.send(event);
    true
}

/// There is no safe NVST NACK wire encoder in the current handoff. Expose this explicit answer
/// rather than emitting an invented control datagram.
pub fn nack_transmission_support() -> Result<(), NvstUnsupportedFeature> {
    Err(NvstUnsupportedFeature::Nack)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TEST_KEY: &str = "000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F";
    const TEST_SALT: &str = "000102030405060708090A0B0C0D";
    const TEST_PEER: &str = "192.0.2.20";

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

    fn build_plaintext_rtp(sequence: u16, flags: u8, frame_index: u32, media: &[u8]) -> Vec<u8> {
        let mut packet = vec![0x90, 0xe0];
        packet.extend_from_slice(&sequence.to_be_bytes());
        packet.extend_from_slice(&0x01020304u32.to_be_bytes());
        packet.extend_from_slice(&0x11223344u32.to_be_bytes());
        packet.extend_from_slice(&GS_VIDEO_EXTENSION_PROFILE.to_be_bytes());
        packet.extend_from_slice(&4_u16.to_be_bytes());
        packet.extend_from_slice(&(u32::from(sequence) << 8).to_le_bytes());
        packet.extend_from_slice(&frame_index.to_le_bytes());
        packet.push(flags);
        packet.extend_from_slice(&[0, 0, 0]);
        packet.extend_from_slice(&[0; 4]);
        packet.extend_from_slice(media);
        packet
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
    fn decrypt_captured_official_packet() {
        // Diagnostic: decrypt a real captured official GFN video packet with the
        // session's master key/salt, comparing the KDF-derived session key (the
        // current SrtpReceiver path) against using the master key/salt directly.
        let master_key = decode_fixed_hex::<32>(
            "D3CB0D52DC2D9CFFE439CB69DBDCEB8725D0F5230145D92D360F17505B7F0520",
            NvstConfigError::InvalidAesKey,
        )
        .expect("key");
        let master_salt = decode_fixed_hex::<12>(
            "0000000000000000D4818BDE",
            NvstConfigError::InvalidSrtpSalt,
        )
        .expect("salt");

        // Diagnostic harness for live captures; skip when no dump was captured this run.
        let dump = match std::fs::read("/tmp/opennow-video-dump.bin") {
            Ok(dump) => dump,
            Err(_) => {
                eprintln!("no /tmp/opennow-video-dump.bin capture; skipping");
                return;
            }
        };
        assert!(dump.len() > 12 && &dump[0..4] == b"NVST", "dump has a packet");
        let pkt_len = u16::from_be_bytes([dump[10], dump[11]]) as usize;
        let packet = &dump[12..12 + pkt_len];
        let header = RtpHeader::parse(packet).expect("header");
        eprintln!(
            "packet: len={} pt={} seq={} ssrc={:08x} payload_offset={}",
            packet.len(),
            header.payload_type,
            header.sequence_number,
            header.ssrc,
            header.payload_offset
        );

        // Path A: KDF-derived session key (current SrtpReceiver code).
        let material = NvstSrtpMaterial::AeadAes256Gcm {
            master_key,
            master_salt,
            authentication_tag_len: SRTP_AEAD_AES_GCM_8_TAG_LEN,
        };
        let mut receiver = SrtpReceiver::from_material(&material);
        match receiver.unprotect(packet) {
            Ok(p) => eprintln!(
                "KDF-path DECRYPT OK: plaintext={} payload[0..16]={:02x?}",
                p.plaintext.len(),
                &p.plaintext[header.payload_offset..header.payload_offset + 16]
            ),
            Err(e) => eprintln!("KDF-path DECRYPT FAIL: {:?}", e),
        }

        // Path B: master key/salt used directly as the session key/salt (no KDF).
        match unprotect_aes_gcm(
            packet,
            header,
            0,
            &master_key,
            &master_salt,
            SRTP_AEAD_AES_GCM_8_TAG_LEN,
        ) {
            Ok(pt) => eprintln!(
                "NO-KDF-path DECRYPT OK: payload[0..16]={:02x?}",
                &pt[header.payload_offset..header.payload_offset + 16]
            ),
            Err(error) => eprintln!("NO-KDF-path DECRYPT FAIL: {error:?}"),
        }
    }

    #[test]
    fn legacy_schema_defaults_to_aes_256_gcm_8_with_explicit_salt() {
        let config = config();
        assert_eq!(config.video_peer(), peer());
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
    fn natt_hole_punch_uses_setup_ping_payload_not_v2_ufrag() {
        let credentials = stun_credentials();
        let packet = build_natt_hole_punch_request(
            &credentials.local_username_fragment,
            b"srv1",
            &credentials.remote_password,
            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        );
        let (_, username) =
            find_stun_attribute(&packet, STUN_ATTR_USERNAME).expect("USERNAME");
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
        let creds = generate_gfn_local_ice_credentials();
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
        assert!(matches!(
            select_preferred_video_transport(&json!({ "nvstTransport": { "tracks": [] } })),
            PreferredVideoTransport::WebRtcFallback(NvstFallbackReason::InvalidNvstHandoff(
                NvstConfigError::RichHandoffUnsupported
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
        invalid["codec"] = json!("H265");
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
        let plaintext = hex_bytes("80E01234010203041122334434120000070000000700000000000000000000016588");
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
    fn parses_the_wire_stream_packet_index_as_a_24_bit_value() {
        let packet = build_plaintext_rtp(
            1,
            FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
            7,
            &[],
        );
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
            is_fec: false,
        };
        let mut payload = vec![0x01, 0, 0, 2, 0, 0, 0, 0];
        payload.extend_from_slice(&[0, 0, 0, 1, 0x67, 0xaa, 0, 0, 1, 0x65, 0xbb]);

        let mut assembler = H264AccessUnitAssembler::new(4096);
        let frame = assembler
            .push(header, 90_000, &payload)
            .expect("valid frame")
            .expect("complete frame");

        assert_eq!(frame.bytes, [0, 0, 0, 1, 0x67, 0xaa, 0, 0, 1, 0x65, 0xbb]);
        assert!(frame.keyframe);
    }

    #[test]
    fn rejects_a_start_packet_without_nearby_annex_b_video() {
        let header = NvVideoPacket {
            stream_packet_index: 1,
            frame_index: 9,
            flags: FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
            is_fec: false,
        };
        let payload = vec![0x81; MAX_GS_FRAME_HEADER_BYTES + 8];
        let mut assembler = H264AccessUnitAssembler::new(4096);

        assert!(matches!(
            assembler.push(header, 90_000, &payload),
            Err(NvstDropReason::MissingAnnexBStartCode)
        ));
    }

    #[test]
    fn receiver_reorders_authenticated_packets_and_emits_annex_b_frame() {
        let config = config();
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
            NvstReceiveEvent::RecoveryNeeded(NvstRecovery::PacketGap {
                nack: NvstUnsupportedFeature::Nack,
                ..
            })
        )));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, NvstReceiveEvent::Frame(_)))
        );
        assert_eq!(
            nack_transmission_support(),
            Err(NvstUnsupportedFeature::Nack)
        );
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
    fn h264_frame_queue_is_bounded_and_prefers_current_frames() {
        let mut queue = BoundedFrameQueue::new(2);
        for frame_index in 1..=3 {
            queue.push(EncodedH264Frame {
                timestamp: frame_index,
                frame_index,
                first_stream_packet_index: frame_index,
                keyframe: false,
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
    frames: VecDeque<EncodedH264Frame>,
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

    pub fn push(&mut self, frame: EncodedH264Frame) {
        if self.frames.len() == self.capacity {
            let _ = self.frames.pop_front();
            self.dropped_frames += 1;
        }
        self.frames.push_back(frame);
    }

    pub fn pop(&mut self) -> Option<EncodedH264Frame> {
        self.frames.pop_front()
    }

    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }
}
