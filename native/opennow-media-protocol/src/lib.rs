pub mod lease;
pub mod wire;
pub use opennow_plugin_api::media::{
    ChromaLocation, ColorDescription, ColorRange, Matrix, Primaries, Transfer,
};

use serde::{Deserialize, Serialize};

pub const MEDIA_PROTOCOL_VERSION: u32 = 1;
pub const MAX_BOOTSTRAP_BYTES: usize = 512 * 1024;
pub const MAX_CONTROL_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceStamp {
    pub sender_frame_id: Option<u64>,
    pub timestamp: u64,
    pub clock_rate_hz: u32,
    pub ssrc: Option<u32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameProvenance {
    pub attempt_generation: u64,
    pub track_id: u32,
    pub source: Option<SourceStamp>,
}

impl FrameProvenance {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.source.is_some_and(|source| source.clock_rate_hz == 0)
            || (self.source.is_some() && (self.attempt_generation == 0 || self.track_id == 0))
        {
            return Err("Invalid source frame provenance");
        }
        Ok(())
    }
}
