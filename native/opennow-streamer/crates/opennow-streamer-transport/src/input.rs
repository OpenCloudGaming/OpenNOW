use std::time::{Duration, Instant};

use str0m::Rtc;
use str0m::channel::{ChannelConfig, ChannelId, Reliability};

pub(crate) const INPUT_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);

const RELIABLE_INPUT_LABEL: &str = "input_channel_v1";
const PARTIAL_INPUT_LABEL: &str = "input_channel_partially_reliable";
const STATS_LABEL: &str = "stats_channel";
const INPUT_HEARTBEAT: &[u8] = &[2, 0, 0, 0];

#[derive(Debug, Clone, Copy)]
pub(crate) struct InputChannels {
    pub(crate) reliable: ChannelId,
    pub(crate) partial: ChannelId,
    pub(crate) stats: ChannelId,
}

impl InputChannels {
    pub(crate) fn create(rtc: &mut Rtc, partial_reliable_lifetime_ms: u16) -> Self {
        let stats = rtc.direct_api().create_data_channel(ChannelConfig {
            label: STATS_LABEL.to_owned(),
            ordered: false,
            reliability: Reliability::MaxRetransmits { retransmits: 0 },
            ..Default::default()
        });
        let reliable = rtc.direct_api().create_data_channel(ChannelConfig {
            label: RELIABLE_INPUT_LABEL.to_owned(),
            ..Default::default()
        });
        let partial = rtc.direct_api().create_data_channel(ChannelConfig {
            label: PARTIAL_INPUT_LABEL.to_owned(),
            ordered: false,
            reliability: Reliability::MaxPacketLifetime {
                lifetime: partial_reliable_lifetime_ms,
            },
            ..Default::default()
        });
        Self {
            reliable,
            partial,
            stats,
        }
    }

    pub(crate) fn send(self, rtc: &mut Rtc, bytes: &[u8], partially_reliable: bool) -> bool {
        let id = if partially_reliable {
            self.partial
        } else {
            self.reliable
        };
        rtc.channel(id)
            .is_some_and(|mut channel| channel.write(true, bytes).unwrap_or(false))
    }

    pub(crate) fn send_heartbeat(self, rtc: &mut Rtc) -> bool {
        self.send(rtc, INPUT_HEARTBEAT, false)
    }
}

#[derive(Debug, Default)]
pub(crate) struct InputChannelState {
    reliable_open: bool,
    partial_open: bool,
    negotiated_version: Option<u16>,
    ready_reported: bool,
}

impl InputChannelState {
    pub(crate) fn channel_opened(&mut self, channels: InputChannels, id: ChannelId) -> Option<u16> {
        if id == channels.reliable {
            self.reliable_open = true;
        } else if id == channels.partial {
            self.partial_open = true;
        }
        self.take_new_ready_version()
    }

    pub(crate) fn channel_data(
        &mut self,
        channels: InputChannels,
        id: ChannelId,
        bytes: &[u8],
    ) -> Option<u16> {
        if id == channels.stats {
            return None;
        }
        if id == channels.reliable
            && let Some(version) = input_protocol_version(bytes)
        {
            self.negotiated_version = Some(version);
        }
        self.take_new_ready_version()
    }

    pub(crate) fn is_ready(&self) -> bool {
        self.reliable_open && self.partial_open && self.negotiated_version.is_some()
    }

    pub(crate) fn channel_closed(&mut self, channels: InputChannels, id: ChannelId) -> bool {
        let was_ready = self.is_ready();
        if id == channels.reliable {
            self.reliable_open = false;
            self.negotiated_version = None;
        } else if id == channels.partial {
            self.partial_open = false;
        } else {
            return false;
        }
        self.ready_reported = false;
        was_ready
    }

    fn take_new_ready_version(&mut self) -> Option<u16> {
        if self.ready_reported || !self.is_ready() {
            return None;
        }
        self.ready_reported = true;
        self.negotiated_version
    }
}

pub(crate) fn next_input_heartbeat(now: Instant) -> Instant {
    now + INPUT_HEARTBEAT_INTERVAL
}

fn input_protocol_version(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 2 {
        return None;
    }
    let first = u16::from_le_bytes([bytes[0], bytes[1]]);
    if first == 526 {
        return Some(if bytes.len() >= 4 {
            u16::from_le_bytes([bytes[2], bytes[3]])
        } else {
            2
        });
    }
    (bytes[0] == 0x0e).then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channels() -> InputChannels {
        let mut rtc = Rtc::new(Instant::now());
        InputChannels::create(&mut rtc, 300)
    }

    #[test]
    fn parses_both_input_handshake_layouts() {
        assert_eq!(input_protocol_version(&[0x0e, 0x02, 0x03, 0x00]), Some(3));
        assert_eq!(input_protocol_version(&[0x0e, 0x03]), Some(0x030e));
        assert_eq!(input_protocol_version(&[1]), None);
    }

    #[test]
    fn input_is_ready_only_after_both_channels_and_handshake() {
        let channels = channels();
        let mut state = InputChannelState::default();

        assert_eq!(state.channel_opened(channels, channels.reliable), None);
        assert_eq!(
            state.channel_data(channels, channels.reliable, &[0x0e, 0x02, 0x03, 0x00]),
            None
        );
        assert!(!state.is_ready());
        assert_eq!(state.channel_opened(channels, channels.partial), Some(3));
        assert!(state.is_ready());
        assert!(state.channel_closed(channels, channels.partial));
        assert!(!state.is_ready());
        assert!(!state.channel_closed(channels, channels.stats));
    }
}
