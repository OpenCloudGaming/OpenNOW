use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;

pub const PROTOCOL_VERSION: u64 = 1;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandEnvelope {
    pub id: String,
    #[serde(rename = "type")]
    pub command_type: String,
    #[serde(default)]
    pub protocol_version: Option<u64>,
    #[serde(default)]
    pub context: Option<NativeStreamerSessionContext>,
    #[serde(default)]
    pub sdp: Option<String>,
    #[serde(default)]
    pub candidate: Option<IceCandidatePayload>,
    #[serde(default)]
    pub input: Option<NativeInputPacket>,
    #[serde(default)]
    pub surface: Option<NativeRenderSurface>,
    #[serde(default)]
    pub max_bitrate_kbps: Option<u32>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NativeStreamerSessionContext {
    pub session: SessionInfo,
    pub settings: StreamSettings,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub session_id: String,
    pub server_ip: String,
    #[serde(default)]
    pub media_connection_info: Option<MediaConnectionInfo>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaConnectionInfo {
    pub ip: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamSettings {
    pub resolution: String,
    pub fps: u32,
    pub max_bitrate_mbps: u32,
    pub codec: VideoCodec,
    pub color_quality: ColorQuality,
    #[serde(default)]
    #[allow(dead_code)]
    pub enable_cloud_gsync: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    H265,
    AV1,
}

impl VideoCodec {
    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::H264 => "H264",
            Self::H265 => "H265",
            Self::AV1 => "AV1",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub enum ColorQuality {
    #[serde(rename = "8bit_420")]
    EightBit420,
    #[serde(rename = "8bit_444")]
    EightBit444,
    #[serde(rename = "10bit_420")]
    TenBit420,
    #[serde(rename = "10bit_444")]
    TenBit444,
}

impl ColorQuality {
    pub fn bit_depth(self) -> u8 {
        match self {
            Self::EightBit420 | Self::EightBit444 => 8,
            Self::TenBit420 | Self::TenBit444 => 10,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IceCandidatePayload {
    pub candidate: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdp_mid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdp_m_line_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username_fragment: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeInputPacket {
    #[serde(default)]
    pub payload: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_base64: Option<String>,
    #[serde(default)]
    pub partially_reliable: bool,
}

impl NativeInputPacket {
    pub fn payload_bytes(&self) -> Result<Cow<'_, [u8]>, String> {
        let Some(payload_base64) = self.payload_base64.as_deref() else {
            return Ok(Cow::Borrowed(&self.payload));
        };

        BASE64_STANDARD
            .decode(payload_base64)
            .map(Cow::Owned)
            .map_err(|error| format!("Invalid base64 input payload: {error}"))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeRenderSurface {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_handle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect: Option<NativeRenderRect>,
    #[serde(default)]
    pub visible: bool,
    #[serde(default)]
    pub device_scale_factor: f64,
    #[serde(default)]
    pub show_stats: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeRenderRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct NativeStreamerCapabilities {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: u64,
    pub backend: &'static str,
    #[serde(rename = "requestedBackend", skip_serializing_if = "Option::is_none")]
    pub requested_backend: Option<String>,
    #[serde(rename = "fallbackReason", skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
    #[serde(rename = "supportsOfferAnswer")]
    pub supports_offer_answer: bool,
    #[serde(rename = "supportsRemoteIce")]
    pub supports_remote_ice: bool,
    #[serde(rename = "supportsLocalIce")]
    pub supports_local_ice: bool,
    #[serde(rename = "supportsInput")]
    pub supports_input: bool,
    #[serde(
        rename = "videoBackends",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub video_backends: Vec<NativeVideoBackendCapability>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeVideoBackendCapability {
    pub backend: String,
    pub platform: String,
    pub codecs: Vec<NativeVideoCodecCapability>,
    #[serde(rename = "zeroCopyModes")]
    pub zero_copy_modes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sink: Option<String>,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeVideoCodecCapability {
    pub codec: String,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decoder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parser: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depayloader: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
#[serde(tag = "type")]
pub enum Response {
    #[serde(rename = "ready")]
    Ready {
        id: String,
        capabilities: NativeStreamerCapabilities,
    },
    #[serde(rename = "ok")]
    Ok { id: String },
    #[serde(rename = "answer")]
    Answer {
        id: String,
        answer: SendAnswerRequest,
    },
    #[serde(rename = "error")]
    Error {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendAnswerRequest {
    pub sdp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nvst_sdp: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoStallEvent {
    pub stall_ms: u64,
    pub encoded_kbps: f64,
    pub decoded_fps: f64,
    pub sink_fps: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoded_age_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decoded_age_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sink_age_ms: Option<u64>,
    pub likely_stage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sink_rendered: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sink_dropped: Option<u64>,
    pub memory_mode: String,
    pub zero_copy: bool,
    pub zero_copy_d3d11: bool,
    pub zero_copy_d3d12: bool,
    pub recovery_attempt: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeStatsEvent {
    pub codec: String,
    pub resolution: String,
    pub hardware_acceleration: String,
    pub bitrate_kbps: u32,
    pub target_bitrate_kbps: u32,
    pub bitrate_performance_percent: f64,
    pub decoded_fps: f64,
    pub render_fps: f64,
    pub frames_decoded: u64,
    pub frames_rendered: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sink_rendered: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sink_dropped: Option<u64>,
    pub memory_mode: String,
    pub zero_copy: bool,
    pub zero_copy_d3d11: bool,
    pub zero_copy_d3d12: bool,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
#[serde(tag = "type")]
pub enum Event {
    #[serde(rename = "log")]
    Log {
        level: &'static str,
        message: String,
    },
    #[serde(rename = "status")]
    Status {
        status: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename = "local-ice")]
    LocalIce { candidate: IceCandidatePayload },
    #[serde(rename = "input-ready")]
    InputReady {
        #[serde(rename = "protocolVersion")]
        protocol_version: u16,
    },
    #[serde(rename = "video-stall")]
    VideoStall(VideoStallEvent),
    #[serde(rename = "stats")]
    Stats { stats: NativeStatsEvent },
    #[serde(rename = "error")]
    Error { code: String, message: String },
}

pub fn parse_command(value: Value) -> Result<CommandEnvelope, String> {
    serde_json::from_value(value).map_err(|error| error.to_string())
}

pub fn missing_field(id: &str, field: &str) -> Response {
    Response::Error {
        id: Some(id.to_owned()),
        code: "missing-field".to_owned(),
        message: format!("Command is missing required field: {field}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_packet_prefers_base64_payload() {
        let packet = NativeInputPacket {
            payload: vec![1, 2, 3],
            payload_base64: Some("BAUG".to_owned()),
            partially_reliable: true,
        };

        assert_eq!(
            packet.payload_bytes().expect("valid base64").as_ref(),
            &[4, 5, 6]
        );
    }

    #[test]
    fn input_packet_keeps_legacy_byte_array_payload() {
        let packet = NativeInputPacket {
            payload: vec![7, 8, 9],
            payload_base64: None,
            partially_reliable: false,
        };

        assert_eq!(
            packet.payload_bytes().expect("legacy payload").as_ref(),
            &[7, 8, 9]
        );
    }

    #[test]
    fn video_stall_event_serializes_as_flat_native_event() {
        let event = Event::VideoStall(VideoStallEvent {
            stall_ms: 2_500,
            encoded_kbps: 0.0,
            decoded_fps: 0.0,
            sink_fps: 0.0,
            encoded_age_ms: Some(2_500),
            decoded_age_ms: Some(2_500),
            sink_age_ms: Some(2_500),
            likely_stage: "video-output-stalled".to_owned(),
            sink_rendered: Some(42),
            sink_dropped: Some(1),
            memory_mode: "D3D11Memory".to_owned(),
            zero_copy: true,
            zero_copy_d3d11: true,
            zero_copy_d3d12: false,
            recovery_attempt: 1,
        });
        let value = serde_json::to_value(event).expect("serializes");

        assert_eq!(value["type"], "video-stall");
        assert_eq!(value["stallMs"], 2_500);
        assert_eq!(value["encodedAgeMs"], 2_500);
        assert_eq!(value["likelyStage"], "video-output-stalled");
        assert_eq!(value["sinkRendered"], 42);
        assert_eq!(value["recoveryAttempt"], 1);
    }
}
