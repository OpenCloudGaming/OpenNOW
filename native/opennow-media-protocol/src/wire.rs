use crate::{FrameProvenance, MAX_CONTROL_BYTES, MEDIA_PROTOCOL_VERSION, SourceStamp};
use opennow_plugin_api::media::{AcceptedMedia, InputCapabilities, MediaLimits};
use opennow_plugin_api::provider::SecretBytes;
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

pub const MEDIA_HEADER_BYTES: usize = 56;
pub const VIDEO_TRACK_ID: u32 = 1;
pub const AUDIO_TRACK_ID: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaHeader {
    pub attempt_generation: u64,
    pub track_id: u32,
    pub payload_bytes: u32,
    pub source: SourceStamp,
    pub keyframe: bool,
    pub contiguous: bool,
}

impl MediaHeader {
    pub fn encode(self) -> [u8; MEDIA_HEADER_BYTES] {
        let mut bytes = [0; MEDIA_HEADER_BYTES];
        bytes[..4].copy_from_slice(b"ONW1");
        bytes[4] = u8::from(self.keyframe)
            | (u8::from(self.contiguous) << 1)
            | (u8::from(self.source.sender_frame_id.is_some()) << 2)
            | (u8::from(self.source.ssrc.is_some()) << 3);
        bytes[8..12].copy_from_slice(&self.track_id.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.payload_bytes.to_le_bytes());
        bytes[16..24].copy_from_slice(&self.attempt_generation.to_le_bytes());
        bytes[24..32].copy_from_slice(&self.source.sender_frame_id.unwrap_or(0).to_le_bytes());
        bytes[32..40].copy_from_slice(&self.source.timestamp.to_le_bytes());
        bytes[40..44].copy_from_slice(&self.source.clock_rate_hz.to_le_bytes());
        bytes[44..48].copy_from_slice(&self.source.ssrc.unwrap_or(0).to_le_bytes());
        bytes
    }

    pub fn decode(bytes: &[u8], limits: &MediaLimits) -> Result<Self, &'static str> {
        if bytes.len() != MEDIA_HEADER_BYTES
            || &bytes[..4] != b"ONW1"
            || bytes[4] & !15 != 0
            || bytes[5..8] != [0; 3]
            || bytes[48..56] != [0; 8]
        {
            return Err("Invalid media header");
        }
        let source = SourceStamp {
            sender_frame_id: (bytes[4] & 4 != 0)
                .then(|| u64::from_le_bytes(bytes[24..32].try_into().unwrap())),
            timestamp: u64::from_le_bytes(bytes[32..40].try_into().unwrap()),
            clock_rate_hz: u32::from_le_bytes(bytes[40..44].try_into().unwrap()),
            ssrc: (bytes[4] & 8 != 0)
                .then(|| u32::from_le_bytes(bytes[44..48].try_into().unwrap())),
        };
        let header = Self {
            attempt_generation: u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
            track_id: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            payload_bytes: u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            source,
            keyframe: bytes[4] & 1 != 0,
            contiguous: bytes[4] & 2 != 0,
        };
        let maximum = match header.track_id {
            VIDEO_TRACK_ID => limits.max_video_access_unit_bytes,
            AUDIO_TRACK_ID => limits.max_audio_packet_bytes,
            _ => return Err("Unknown media track"),
        };
        if header.payload_bytes == 0
            || header.payload_bytes > maximum
            || source.clock_rate_hz == 0
            || header.attempt_generation == 0
        {
            return Err("Media frame exceeds the accepted limits");
        }
        Ok(header)
    }

    pub fn provenance(&self) -> FrameProvenance {
        FrameProvenance {
            attempt_generation: self.attempt_generation,
            track_id: self.track_id,
            source: Some(self.source),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerBootstrap {
    pub version: u32,
    pub binding: crate::lease::WorkerBinding,
    pub attempt_generation: u64,
    pub control_port: u16,
    pub authentication: SecretBytes,
    pub accepted: AcceptedMedia,
    pub limits: MediaLimits,
    pub provider_bootstrap: SecretBytes,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum InputEvent {
    Key {
        virtual_key: u16,
        modifiers: u16,
        pressed: bool,
    },
    MouseRelative {
        x: i16,
        y: i16,
    },
    MouseAbsolute {
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    },
    MouseButton {
        button: u8,
        pressed: bool,
    },
    MouseWheel {
        x: i16,
        y: i16,
    },
    Text {
        paste_id: u64,
        offset: u32,
        final_chunk: bool,
        utf8: String,
    },
    Gamepad {
        controller: u8,
        bitmap: u16,
        buttons: u16,
        left_trigger: u8,
        right_trigger: u8,
        left_x: i16,
        left_y: i16,
        right_x: i16,
        right_y: i16,
        incarnation: u64,
    },
}

impl std::fmt::Debug for InputEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InputEvent([private])")
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FrameStage {
    Accepted,
    Decoded,
    Presented,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AckKind {
    Input,
    Neutral,
    Keyframe,
    Stop,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ControlMessage {
    Hello {
        version: u32,
        authentication: SecretBytes,
        attempt_generation: u64,
    },
    Attached {
        attempt_generation: u64,
    },
    Ready {
        attempt_generation: u64,
        input: InputCapabilities,
    },
    Input {
        attempt_generation: u64,
        sequence: u64,
        captured_us: u64,
        event: InputEvent,
    },
    Neutral {
        attempt_generation: u64,
        sequence: u64,
    },
    Keyframe {
        attempt_generation: u64,
        track_id: u32,
    },
    FrameProgress {
        provenance: FrameProvenance,
        stage: FrameStage,
        local_us: u64,
    },
    Rumble {
        attempt_generation: u64,
        controller: u8,
        incarnation: u64,
        low: u16,
        high: u16,
        duration_ms: u16,
    },
    Stop {
        attempt_generation: u64,
        sequence: u64,
    },
    Ack {
        attempt_generation: u64,
        sequence: u64,
        kind: AckKind,
    },
    Ended {
        attempt_generation: u64,
    },
}

impl std::fmt::Debug for ControlMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ControlMessage([private])")
    }
}

pub fn encode_control(message: &ControlMessage, maximum: usize) -> io::Result<Vec<u8>> {
    let payload = serde_json::to_vec(message).map_err(io::Error::other)?;
    if payload.len() > maximum.min(MAX_CONTROL_BYTES) {
        return Err(io::Error::other("Control frame exceeds its limit"));
    }
    let mut bytes = Vec::with_capacity(payload.len() + 4);
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}

pub fn read_control(reader: &mut impl Read, maximum: usize) -> io::Result<ControlMessage> {
    let mut prefix = [0; 4];
    reader.read_exact(&mut prefix)?;
    let length = u32::from_le_bytes(prefix) as usize;
    if length == 0 || length > maximum.min(MAX_CONTROL_BYTES) {
        return Err(io::Error::other("Invalid control frame length"));
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes).map_err(|_| io::Error::other("Invalid control frame"))
}

pub fn write_control(
    writer: &mut impl Write,
    message: &ControlMessage,
    maximum: usize,
) -> io::Result<()> {
    writer.write_all(&encode_control(message, maximum)?)
}

impl WorkerBootstrap {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != MEDIA_PROTOCOL_VERSION
            || self.attempt_generation == 0
            || self.control_port == 0
            || self.authentication.expose_secret().len() != 32
        {
            return Err("Invalid private media bootstrap");
        }
        self.limits.validate().map_err(|_| "Invalid media limits")?;
        self.accepted
            .video
            .validate()
            .map_err(|_| "Invalid accepted video")?;
        if let Some(audio) = &self.accepted.audio {
            audio.validate().map_err(|_| "Invalid accepted audio")?;
        }
        Ok(())
    }
}
