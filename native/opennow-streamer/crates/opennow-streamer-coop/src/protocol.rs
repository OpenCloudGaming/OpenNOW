//! JSON shapes carried in signal payloads and on the 'coop-control' data channel.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MAX_CONTROL_MESSAGE_BYTES: usize = 4 * 1024;
pub const MAX_SDP_BYTES: usize = 12 * 1024;

/// The negotiated NVST session, sent first on 'coop-control'.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamDescription {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bit_depth: u8,
    /// "420" or "444".
    pub chroma: String,
    /// Not negotiated by NVST; decoders read it from the bitstream VUI. Null until known.
    pub color_range: Option<String>,
    /// "bt709" for SDR and "bt2020" for HDR, the same mapping the decoders apply.
    pub color_space: Option<String>,
    pub hdr: bool,
    pub audio: Option<AudioDescription>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDescription {
    pub codec: String,
    pub channels: u8,
    pub sample_rate: u32,
    /// RTP payload type of the forwarded audio packets (GFN's Opus or its RED wrapper).
    pub payload_type: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    Offer(String),
    Answer(String),
    Candidate(String),
    EndOfCandidates,
    Bye(String),
    /// From the guest: the link of this generation went silent; the host answers with a
    /// new offer on a fresh link.
    Restart,
}

/// Link generation a payload belongs to. The host starts at 0 and counts up with every
/// restart offer; answers, candidates and restart requests carry the generation of the
/// offer they belong to, so signals for a replaced link are dropped. Absent means 0.
pub fn generation(payload: &Value) -> u32 {
    payload["gen"]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0)
}

impl Signal {
    pub fn parse(payload: &Value) -> Option<Self> {
        let text = |key: &str, limit: usize| {
            payload[key]
                .as_str()
                .filter(|value| value.len() <= limit)
                .map(str::to_owned)
        };
        match payload["kind"].as_str()? {
            "offer" => text("sdp", MAX_SDP_BYTES).map(Self::Offer),
            "answer" => text("sdp", MAX_SDP_BYTES).map(Self::Answer),
            "candidate" => text("candidate", 1024).map(Self::Candidate),
            "end-of-candidates" => Some(Self::EndOfCandidates),
            "bye" => Some(Self::Bye(text("reason", 256).unwrap_or_default())),
            "restart" => Some(Self::Restart),
            _ => None,
        }
    }

    pub fn to_json(&self, generation: u32) -> Value {
        let mut payload = match self {
            Self::Offer(sdp) => json!({"kind":"offer","sdp":sdp}),
            Self::Answer(sdp) => json!({"kind":"answer","sdp":sdp}),
            Self::Candidate(candidate) => {
                json!({"kind":"candidate","candidate":candidate,"mid":null})
            }
            Self::EndOfCandidates => json!({"kind":"end-of-candidates"}),
            Self::Bye(reason) => json!({"kind":"bye","reason":reason}),
            Self::Restart => json!({"kind":"restart"}),
        };
        payload["gen"] = json!(generation);
        payload
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ControlMessage {
    Slot {
        slot: u8,
    },
    #[serde(rename_all = "camelCase")]
    InputMuted {
        muted: bool,
    },
    #[serde(rename_all = "camelCase")]
    Stats {
        host_to_gfn_ping_ms: Option<f64>,
    },
    #[serde(rename_all = "camelCase")]
    Rumble {
        low_frequency: u16,
        high_frequency: u16,
        duration_ms: u16,
    },
    KeyframeRequest,
}

impl ControlMessage {
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        (bytes.len() <= MAX_CONTROL_MESSAGE_BYTES)
            .then(|| serde_json::from_slice(bytes).ok())
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_messages_use_contract_type_names() {
        assert_eq!(
            serde_json::to_value(ControlMessage::KeyframeRequest).unwrap(),
            json!({"type":"keyframe-request"})
        );
        assert_eq!(
            serde_json::to_value(ControlMessage::InputMuted { muted: true }).unwrap(),
            json!({"type":"input-muted","muted":true})
        );
        assert_eq!(
            serde_json::to_value(ControlMessage::Stats {
                host_to_gfn_ping_ms: Some(12.5)
            })
            .unwrap(),
            json!({"type":"stats","hostToGfnPingMs":12.5})
        );
        let rumble = ControlMessage::Rumble {
            low_frequency: 1,
            high_frequency: 2,
            duration_ms: 3,
        };
        assert_eq!(ControlMessage::decode(&rumble.encode()), Some(rumble));
        assert_eq!(
            ControlMessage::decode(&vec![b' '; MAX_CONTROL_MESSAGE_BYTES + 1]),
            None
        );
    }

    #[test]
    fn signal_payloads_match_the_contract() {
        for payload in [
            json!({"kind":"offer","sdp":"v=0","gen":0}),
            json!({"kind":"answer","sdp":"v=0","gen":2}),
            json!({"kind":"candidate","candidate":"candidate:1 1 udp 1 127.0.0.1 9 typ host","mid":null,"gen":1}),
            json!({"kind":"end-of-candidates","gen":0}),
            json!({"kind":"bye","reason":"left","gen":0}),
            json!({"kind":"restart","gen":3}),
        ] {
            assert_eq!(
                Signal::parse(&payload)
                    .unwrap()
                    .to_json(generation(&payload)),
                payload
            );
        }
        assert_eq!(generation(&json!({"kind":"answer","sdp":"v=0"})), 0);
        assert!(
            Signal::parse(&json!({"kind":"offer","sdp":"x".repeat(MAX_SDP_BYTES + 1)})).is_none()
        );
        assert!(Signal::parse(&json!({"kind":"nope"})).is_none());
    }
}
