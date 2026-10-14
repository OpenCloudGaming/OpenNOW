use opennow_plugin_api::media::{NativeOffer, PreparedWorker};
use opennow_plugin_api::provider::{AttemptId, OfferId, SessionKey, Text};
use opennow_plugin_api::{BUILTIN_GFN_ID, PluginId, ValidationError};
use opennow_plugin_package::InstalledManifest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalMediaPolicy {
    #[serde(default = "auto_backend")]
    pub video_backend: String,
    #[serde(default)]
    pub audio_output_device: String,
    #[serde(default = "bitrate")]
    pub max_bitrate_mbps: f64,
    #[serde(default)]
    pub replay_buffer_enabled: bool,
    #[serde(default = "replay_seconds")]
    pub replay_buffer_seconds: u16,
    #[serde(default = "replay_memory")]
    pub replay_buffer_memory_mi_b: u16,
    #[serde(default)]
    pub shortcuts: BTreeMap<String, String>,
}

fn auto_backend() -> String {
    "auto".into()
}
fn bitrate() -> f64 {
    75.0
}
fn replay_seconds() -> u16 {
    30
}
fn replay_memory() -> u16 {
    256
}

impl Default for LocalMediaPolicy {
    fn default() -> Self {
        Self {
            video_backend: auto_backend(),
            audio_output_device: String::new(),
            max_bitrate_mbps: bitrate(),
            replay_buffer_enabled: false,
            replay_buffer_seconds: replay_seconds(),
            replay_buffer_memory_mi_b: replay_memory(),
            shortcuts: BTreeMap::new(),
        }
    }
}

impl LocalMediaPolicy {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.video_backend.is_empty()
            || self.video_backend.len() > 64
            || !self
                .video_backend
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || self.audio_output_device.len() > 1024
            || self.audio_output_device.contains('\0')
            || !self.max_bitrate_mbps.is_finite()
            || !(0.22..=200.0).contains(&self.max_bitrate_mbps)
            || !(15..=120).contains(&self.replay_buffer_seconds)
            || !(64..=512).contains(&self.replay_buffer_memory_mi_b)
        {
            return Err(ValidationError("Invalid host media policy"));
        }
        for (action, chord) in &self.shortcuts {
            if ![
                "toggleStats",
                "togglePointerLock",
                "toggleFullscreen",
                "stopStream",
                "toggleAntiAfk",
                "toggleMicrophone",
                "screenshot",
                "toggleRecording",
                "saveClip",
            ]
            .contains(&action.as_str())
                || chord.len() > 64
                || chord.chars().any(char::is_control)
            {
                return Err(ValidationError("Invalid host shortcut policy"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageReference {
    pub version_root: PathBuf,
    pub data_root: PathBuf,
    pub expected_manifest: InstalledManifest,
}

impl fmt::Debug for PackageReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PackageReference([private])")
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum PreparedMedia {
    Worker {
        package: Box<PackageReference>,
        prepared: PreparedWorker,
    },
    Gfn {
        context: serde_json::Value,
    },
}

impl fmt::Debug for PreparedMedia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Worker { .. } => "Worker([private])",
            Self::Gfn { .. } => "Gfn([private])",
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostBoundPreparedLease {
    pub version: u32,
    pub lease_id: Text<128>,
    pub offer_id: OfferId,
    pub runtime_epoch: u64,
    pub source_id: PluginId,
    pub session: SessionKey,
    pub attempt_id: AttemptId,
    pub expires_at_ms: u64,
    pub media: PreparedMedia,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerBinding {
    pub lease_id: Text<128>,
    pub source_id: PluginId,
    pub session: SessionKey,
    pub attempt_id: AttemptId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NativeMediaState {
    Starting,
    Negotiating,
    Streaming,
    Recovering,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActiveMediaStatus {
    pub lease_id: Text<128>,
    pub source_id: PluginId,
    pub session: SessionKey,
    pub attempt_id: AttemptId,
    pub runtime_epoch: u64,
    pub start_id: Text<128>,
    pub state: NativeMediaState,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeMediaStatus {
    pub runtime_epoch: u64,
    pub native_idle: bool,
    pub legacy_active: bool,
    pub active: Option<ActiveMediaStatus>,
}

impl HostBoundPreparedLease {
    pub fn worker_binding(&self) -> WorkerBinding {
        WorkerBinding {
            lease_id: self.lease_id.clone(),
            source_id: self.source_id.clone(),
            session: self.session.clone(),
            attempt_id: self.attempt_id.clone(),
        }
    }
    pub fn validate_against(
        &self,
        offer: &NativeOffer,
        now_ms: u64,
    ) -> Result<(), ValidationError> {
        offer.validate()?;
        if self.version != 1
            || self.offer_id != offer.offer_id
            || self.runtime_epoch != offer.runtime_epoch
            || now_ms >= self.expires_at_ms
            || self.expires_at_ms > offer.expires_at_ms
        {
            return Err(ValidationError(
                "Prepared lease does not match the native offer",
            ));
        }
        match &self.media {
            PreparedMedia::Worker { package, prepared } => {
                if self.source_id.is_builtin()
                    || !package.version_root.is_absolute()
                    || !package.data_root.is_absolute()
                    || package.expected_manifest.id() != &self.source_id
                    || !matches!(package.expected_manifest, InstalledManifest::Provider(_))
                {
                    return Err(ValidationError(
                        "Prepared lease has an invalid package binding",
                    ));
                }
                prepared.accepted.validate_against(offer, now_ms)?;
            }
            PreparedMedia::Gfn { context } => {
                if self.source_id.as_str() != BUILTIN_GFN_ID
                    || !context.is_object()
                    || context["session"]["sessionId"].as_str()
                        != Some(self.session.remote_id.as_str())
                {
                    return Err(ValidationError(
                        "The private GFN preparation is not a community capability",
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn public_binding(&self) -> serde_json::Value {
        serde_json::json!({"leaseId":self.lease_id,"sourceId":self.source_id,"session":self.session,
            "attemptId":self.attempt_id,"runtimeEpoch":self.runtime_epoch})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OfferRequest {
    pub local_policy: LocalMediaPolicy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartRequest {
    pub lease: HostBoundPreparedLease,
}
