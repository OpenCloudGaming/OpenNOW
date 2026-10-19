//! OpenNOW co-op: the host forwards its GFN media to up to three gamepad-only guests over
//! peer-to-peer links (ICE with STUN, then TURN; DTLS-SRTP with the link's own keys) and
//! injects their input into GFN controller slots 1..3. See docs/coop-sessions.md section 7.

use std::sync::Arc;

use serde_json::Value;

pub mod config;
pub mod guest;
pub mod host;
pub mod input;
pub mod link;
pub mod protocol;
pub mod ring;
pub mod slots;
pub mod stun;
pub mod turn;

pub use config::{ConfigError, IceConfig, RelaySite, TurnTransport};
pub use guest::{CoopGuest, GuestConfig, GuestMedia};
pub use host::{CoopHost, HostBackend, HostConfig, Rumble, RumbleRouter};
pub use input::{GamepadSelector, GamepadState, InputRecord, InputSamples};
pub use link::NetConfig;
pub use protocol::{AudioDescription, StreamDescription};

/// Receives every JSON event the co-op workers emit. Implementations must not block.
pub type EventSink = Arc<dyn Fn(Value) + Send + Sync>;

/// Longest remote- or platform-provided reason text written to the native log.
const MAX_LOGGED_REASON_CHARS: usize = 256;

/// One line in the native file log under the 'coop' area. Never pass peer ids, SDP,
/// candidates or credentials.
fn log(message: &str) {
    opennow_streamer_protocol::log::log_line("INFO", "coop", message);
}

fn bounded(reason: &str) -> String {
    reason.chars().take(MAX_LOGGED_REASON_CHARS).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoopError {
    Busy,
    Closed,
    Invalid(String),
}

impl std::fmt::Display for CoopError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => formatter.write_str("co-op worker command queue is full"),
            Self::Closed => formatter.write_str("co-op worker is not running"),
            Self::Invalid(reason) => formatter.write_str(reason),
        }
    }
}

impl std::error::Error for CoopError {}
