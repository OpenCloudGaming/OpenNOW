use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub mod log;

pub const PROTOCOL_VERSION: u64 = 6;

#[derive(Debug, Clone, Copy)]
pub struct ReplayBufferConfig {
    pub enabled: bool,
    pub duration: std::time::Duration,
    pub memory_bytes: usize,
}

impl ReplayBufferConfig {
    pub fn from_settings(settings: &Value) -> Self {
        let bounded = |key: &str, default: u64, minimum: u64, maximum: u64| {
            settings[key]
                .as_i64()
                .map(|value| value.clamp(minimum as i64, maximum as i64) as u64)
                .or_else(|| {
                    settings[key]
                        .as_u64()
                        .map(|value| value.clamp(minimum, maximum))
                })
                .unwrap_or(default)
        };
        Self {
            enabled: settings["replayBufferEnabled"].as_bool() == Some(true),
            duration: std::time::Duration::from_secs(bounded("replayBufferSeconds", 30, 15, 120)),
            memory_bytes: bounded("replayBufferMemoryMiB", 256, 64, 512) as usize * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioOutputDevice(String);

impl AudioOutputDevice {
    pub fn from_settings(settings: &Value) -> Result<Self, String> {
        match settings.get("audioOutputDevice") {
            None => Ok(Self::default()),
            Some(Value::String(name)) => Self::new(name.clone()),
            Some(_) => Err("audioOutputDevice must be a string".to_owned()),
        }
    }

    pub fn new(name: String) -> Result<Self, String> {
        if name.len() > 1024 || name.contains('\0') {
            return Err(
                "audioOutputDevice must be at most 1024 UTF-8 bytes and contain no NUL".to_owned(),
            );
        }
        Ok(Self(name))
    }

    pub fn device_name(&self) -> Option<&str> {
        (!self.0.is_empty()).then_some(self.0.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
}

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
    pub paused: Option<bool>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub microphone_enabled: Option<bool>,
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
    #[serde(default)]
    pub output_path: Option<String>,
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
    pub supports_input: bool,
    pub supports_video_decode: bool,
    pub supports_video_present: bool,
    pub supports_audio_decode: bool,
    pub supports_audio_output: bool,
    pub supports_microphone: bool,
    pub supports_owned_nvst_negotiation: bool,
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
    #[serde(rename = "colorQualities", skip_serializing_if = "Option::is_none")]
    pub color_qualities: Option<Vec<&'static str>>,
    #[serde(rename = "hdrSupported", skip_serializing_if = "Option::is_none")]
    pub hdr_supported: Option<bool>,
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
    #[test]
    fn replay_settings_are_opt_in_and_bounded_and_commands_are_additive() {
        use super::{Command, ReplayBufferConfig};
        use serde_json::json;
        let defaults = ReplayBufferConfig::from_settings(&json!({}));
        assert!(!defaults.enabled);
        assert_eq!(defaults.duration.as_secs(), 30);
        assert_eq!(defaults.memory_bytes, 256 * 1024 * 1024);
        for enabled in [json!(false), json!("true"), json!(1), json!(null)] {
            assert!(
                !ReplayBufferConfig::from_settings(&json!({"replayBufferEnabled":enabled})).enabled
            );
        }
        for (seconds, memory, expected_seconds, expected_memory) in [
            (-1, -1, 15, 64),
            (5, 20, 15, 64),
            (60, 128, 60, 128),
            (999, 999, 120, 512),
        ] {
            let config = ReplayBufferConfig::from_settings(
                &json!({"replayBufferEnabled":true,"replayBufferSeconds":seconds,"replayBufferMemoryMiB":memory}),
            );
            assert!(config.enabled);
            assert_eq!(config.duration.as_secs(), expected_seconds);
            assert_eq!(config.memory_bytes, expected_memory * 1024 * 1024);
        }
        let save: Command = serde_json::from_value(
            json!({"id":"clip-1","type":"clip-save","outputPath":"/clips/game.mkv"}),
        )
        .unwrap();
        assert_eq!(save.output_path.as_deref(), Some("/clips/game.mkv"));
        let stop: Command =
            serde_json::from_value(json!({"id":"stop-1","type":"replay-stop"})).unwrap();
        assert_eq!(stop.kind, "replay-stop");
    }

    #[test]
    fn audio_output_device_defaults_and_preserves_exact_names() {
        use super::AudioOutputDevice;
        use serde_json::json;

        for settings in [json!({}), json!({"audioOutputDevice": ""})] {
            assert_eq!(
                AudioOutputDevice::from_settings(&settings)
                    .unwrap()
                    .device_name(),
                None
            );
        }
        let settings = json!({"audioOutputDevice": " USB Speakers — 音声 "});
        let selected = AudioOutputDevice::from_settings(&settings).unwrap();
        assert_eq!(selected.device_name(), Some(" USB Speakers — 音声 "));
    }

    #[test]
    fn audio_output_device_rejects_invalid_names_and_types() {
        use super::AudioOutputDevice;
        use serde_json::json;

        for value in [
            json!(null),
            json!(1),
            json!(true),
            json!([]),
            json!({}),
            json!("a\0b"),
            json!("é".repeat(513)),
        ] {
            assert!(
                AudioOutputDevice::from_settings(&json!({"audioOutputDevice": value})).is_err()
            );
        }
        assert!(AudioOutputDevice::new("é".repeat(512)).is_ok());
    }

    use super::*;

    #[test]
    fn hdr_capabilities_distinguish_unknown_supported_and_unsupported() {
        let mut codec = CodecCapability {
            codec: "h265",
            available: true,
            color_qualities: None,
            hdr_supported: None,
            reason: None,
        };
        assert!(
            serde_json::to_value(&codec)
                .unwrap()
                .get("hdrSupported")
                .is_none()
        );
        for supported in [false, true] {
            codec.hdr_supported = Some(supported);
            let value = serde_json::to_value(&codec).unwrap();
            assert_eq!(value["hdrSupported"], supported);
            assert!(value.get("colorQualities").is_none());
        }
    }

    #[test]
    fn codec_color_profiles_are_additive_and_preserve_explicit_empty_support() {
        let mut codec = CodecCapability {
            hdr_supported: None,
            codec: "h265",
            available: true,
            color_qualities: None,
            reason: None,
        };
        let value = serde_json::to_value(&codec).unwrap();
        assert!(value.get("colorQualities").is_none());
        codec.color_qualities = Some(vec!["8bit_420", "10bit_420"]);
        let value = serde_json::to_value(&codec).unwrap();
        assert_eq!(
            value["colorQualities"],
            serde_json::json!(["8bit_420", "10bit_420"])
        );
        codec.color_qualities = Some(Vec::new());
        let value = serde_json::to_value(&codec).unwrap();
        assert_eq!(value["colorQualities"], serde_json::json!([]));
    }

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
