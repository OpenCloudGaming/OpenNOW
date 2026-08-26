use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const PROTOCOL_VERSION: u64 = 4;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Command {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub protocol_version: Option<u64>,
    #[serde(default)]
    pub context: Option<Value>,
    #[serde(default)]
    pub sdp: Option<String>,
    #[serde(default)]
    pub candidate: Option<IceCandidate>,
    #[serde(default)]
    pub input: Option<NativeInput>,
    #[serde(default)]
    pub paused: Option<bool>,
    #[serde(default)]
    pub surface: Option<RenderSurface>,
    #[serde(default)]
    pub max_bitrate_kbps: Option<u32>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub shortcuts: Option<Value>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub payload_base64: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IceCandidate {
    pub candidate: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdp_mid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdp_m_line_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username_fragment: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeInput {
    #[serde(default)]
    pub payload_base64: String,
    #[serde(default)]
    pub partially_reliable: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderSurface {
    #[serde(default)]
    pub rect: Option<RenderSurfaceRect>,
    #[serde(default)]
    pub visible: bool,
    #[serde(default = "default_device_scale_factor")]
    pub device_scale_factor: f32,
    #[serde(default)]
    pub window_handle: Option<String>,
    #[serde(default)]
    pub screen_rect: Option<RenderSurfaceRect>,
}

impl Default for RenderSurface {
    fn default() -> Self {
        Self {
            rect: None,
            visible: false,
            device_scale_factor: default_device_scale_factor(),
            window_handle: None,
            screen_rect: None,
        }
    }
}

fn default_device_scale_factor() -> f32 {
    1.0
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderSurfaceRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContext {
    pub session: Session,
    pub settings: Value,
    pub shortcuts: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nvst_video: Option<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_session_id: Option<String>,
    pub server_ip: String,
    #[serde(default)]
    pub ice_servers: Vec<IceServer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_connection_info: Option<MediaConnectionInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_info: Option<Vec<ConnectionInfo>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    pub port: u32,
    pub usage: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_level_protocol: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_path: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaConnectionInfo {
    pub ip: String,
    pub port: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<u32>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub protocol_version: u64,
    pub backend: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<&'static str>,
    pub supports_offer_answer: bool,
    pub supports_remote_ice: bool,
    pub supports_local_ice: bool,
    pub supports_input: bool,
    pub supports_video_decode: bool,
    pub supports_video_present: bool,
    pub supports_audio_decode: bool,
    pub supports_audio_output: bool,
    pub video_backends: Vec<VideoBackendCapability>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoBackendCapability {
    pub backend: &'static str,
    pub platform: &'static str,
    pub codecs: Vec<CodecCapability>,
    pub zero_copy_modes: Vec<&'static str>,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodecCapability {
    pub codec: &'static str,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
}

pub fn response(id: impl Into<String>, kind: &str) -> Value {
    serde_json::json!({ "id": id.into(), "type": kind })
}

pub fn error(id: Option<&str>, code: &str, message: impl Into<String>) -> Value {
    let mut value = serde_json::json!({
        "type": "error",
        "code": code,
        "message": message.into(),
    });
    if let Some(id) = id {
        value["id"] = Value::String(id.to_owned());
    }
    value
}

pub fn event(kind: &str, fields: Value) -> Value {
    let mut object = fields.as_object().cloned().unwrap_or_default();
    object.insert("type".to_owned(), Value::String(kind.to_owned()));
    Value::Object(object)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_forward_compatible_commands() {
        let command: Command = serde_json::from_value(serde_json::json!({
            "id": "1",
            "type": "start",
            "context": { "session": { "sessionId": "session" } },
            "futureField": true
        }))
        .expect("command");
        assert_eq!(command.kind, "start");
        assert!(command.context.is_some());
    }

    #[test]
    fn serializes_candidate_for_app_contract() {
        let candidate = IceCandidate {
            candidate: "candidate:1 1 udp 1 127.0.0.1 5000 typ host".to_owned(),
            sdp_mid: Some("0".to_owned()),
            sdp_m_line_index: Some(0),
            username_fragment: None,
        };
        let value = serde_json::to_value(candidate).expect("candidate");
        assert_eq!(value["sdpMLineIndex"], 0);
    }

    #[test]
    fn unsolicited_errors_do_not_serialize_a_null_request_id() {
        let value = error(None, "invalid-command", "bad JSON");
        assert!(value.get("id").is_none());
        assert_eq!(value["type"], "error");
    }

    #[test]
    fn session_context_round_trips_required_and_forward_compatible_fields() {
        let fixture = serde_json::json!({
            "session": {
                "sessionId": "synthetic-session",
                "subSessionId": "synthetic-subsession",
                "serverIp": "127-0-0-1.synthetic.invalid",
                "iceServers": [],
                "mediaConnectionInfo": {
                    "ip": "198.51.100.20",
                    "port": 18_784,
                    "usage": 17,
                    "futureEndpointField": true
                },
                "connectionInfo": [
                    {
                        "ip": "198.51.100.10",
                        "port": 443,
                        "usage": 14,
                        "protocol": 1,
                        "resourcePath": "/nvst/"
                    },
                    {
                        "ip": "198.51.100.20",
                        "port": 48322,
                        "usage": 16,
                        "protocol": 1,
                        "appLevelProtocol": 6,
                        "resourcePath": "rtsps://198.51.100.20:48322/session",
                        "futureConnectionField": true
                    }
                ],
                "futureSessionField": "preserved"
            },
            "settings": { "codec": "H264", "fps": 60 },
            "shortcuts": { "stopStream": "Ctrl+Shift+Q" },
            "futureContextField": 42
        });

        let context: SessionContext = serde_json::from_value(fixture.clone()).expect("context");
        assert_eq!(context.session.session_id, "synthetic-session");
        assert_eq!(
            serde_json::to_value(context).expect("serializable context"),
            fixture
        );
    }
}
