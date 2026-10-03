use std::time::Instant;

use str0m::Rtc;
use str0m::channel::{ChannelConfig, ChannelId, Reliability};

use super::TransportError;

const CHANNEL_BUFFER_BYTES: usize = 64 * 1024;

pub(super) fn validate_packet(bytes: &[u8]) -> Result<u32, TransportError> {
    let kind = bytes
        .get(..4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(TransportError::MalformedInput)?;
    let expected = match kind {
        2 => 4,
        3 | 4 | 8 | 9 => 18,
        5 => 26,
        7 | 10 => 22,
        12 => 38,
        13 => 6,
        19 => 5,
        _ => return Err(TransportError::UnsupportedInput),
    };
    if bytes.len() != expected {
        return Err(TransportError::MalformedInput);
    }
    Ok(kind)
}

#[derive(Clone, Copy)]
pub(super) struct InputChannels {
    reliable: ChannelId,
    partial: ChannelId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn versions_and_event_classes_use_verified_wire_wrappers() {
        let start = Instant::now();
        for version in [1, 2, 3, 4] {
            let state = InputChannelState {
                reliable_open: true,
                partial_open: true,
                handshake: Some((version, start)),
                reported: true,
            };
            for (kind, length, batched) in [
                (3_u32, 18, false),
                (4, 18, false),
                (5, 26, true),
                (7, 22, true),
                (8, 18, false),
                (9, 18, false),
                (10, 22, false),
                (12, 38, true),
            ] {
                let mut body = vec![0; length];
                body[..4].copy_from_slice(&kind.to_le_bytes());
                let encoded = state
                    .encode(&body, start + Duration::from_micros(1234))
                    .unwrap();
                if version <= 2 {
                    assert_eq!(encoded, body);
                    continue;
                }
                assert_eq!(encoded[0], 0x23);
                assert_eq!(&encoded[1..9], &1234_u64.to_be_bytes());
                assert_eq!(encoded[9], if batched { 0x21 } else { 0x22 });
                if batched {
                    assert_eq!(&encoded[10..12], &(length as u16).to_be_bytes());
                    assert_eq!(&encoded[12..], body);
                } else {
                    assert_eq!(&encoded[10..], body);
                }
            }
            assert_eq!(state.encode(&[2, 0, 0, 0], start).unwrap(), [2, 0, 0, 0]);
        }
    }

    #[test]
    fn handshake_requires_supported_layout_and_both_input_channels() {
        crate::install_crypto();
        let mut rtc = Rtc::new(Instant::now());
        let channels = InputChannels::create(&mut rtc, 300);
        let mut state = InputChannelState::default();
        assert_eq!(state.channel_opened(channels, channels.reliable), None);
        assert_eq!(
            state.channel_data(channels, channels.reliable, &[0x0d, 2]),
            None
        );
        assert_eq!(
            state.channel_data(channels, channels.partial, &[0x0e, 2, 3, 0]),
            None
        );
        assert_eq!(
            state.channel_data(channels, channels.reliable, &[0x0e, 2, 3, 0]),
            None
        );
        assert_eq!(state.channel_opened(channels, channels.partial), Some(3));
        assert!(state.is_ready());
        assert!(state.channel_closed(channels, channels.partial));
        assert!(!state.is_ready());
    }

    #[test]
    fn handshake_accepts_android_and_historical_legacy_layouts() {
        crate::install_crypto();
        for (packet, version) in [
            (&[0x0e, 2][..], 2),
            (&[0x0e, 2, 3, 0][..], 3),
            (&[0x0e, 3][..], 0x030e),
        ] {
            let mut rtc = Rtc::new(Instant::now());
            let channels = InputChannels::create(&mut rtc, 300);
            let mut state = InputChannelState::default();
            state.channel_opened(channels, channels.reliable);
            state.channel_opened(channels, channels.partial);
            assert_eq!(
                state.channel_data(channels, channels.reliable, packet),
                Some(version)
            );
            assert!(state.is_ready());
        }
    }
}

impl InputChannels {
    pub(super) fn create(rtc: &mut Rtc, lifetime: u16) -> Self {
        rtc.direct_api().create_data_channel(ChannelConfig {
            label: "stats_channel".to_owned(),
            ordered: false,
            reliability: Reliability::MaxRetransmits { retransmits: 0 },
            ..Default::default()
        });
        let reliable = rtc.direct_api().create_data_channel(ChannelConfig {
            label: "input_channel_v1".to_owned(),
            ..Default::default()
        });
        let partial = rtc.direct_api().create_data_channel(ChannelConfig {
            label: "input_channel_partially_reliable".to_owned(),
            ordered: false,
            reliability: Reliability::MaxPacketLifetime { lifetime },
            ..Default::default()
        });
        Self { reliable, partial }
    }

    pub(super) fn send(self, rtc: &mut Rtc, bytes: &[u8], partially_reliable: bool) -> bool {
        let id = if partially_reliable {
            self.partial
        } else {
            self.reliable
        };
        rtc.channel(id).is_some_and(|mut channel| {
            channel.buffered_amount().saturating_add(bytes.len()) <= CHANNEL_BUFFER_BYTES
                && channel.write(true, bytes).unwrap_or(false)
        })
    }
}

#[derive(Default)]
pub(super) struct InputChannelState {
    reliable_open: bool,
    partial_open: bool,
    handshake: Option<(u16, Instant)>,
    reported: bool,
}

impl InputChannelState {
    pub(super) fn channel_opened(&mut self, channels: InputChannels, id: ChannelId) -> Option<u16> {
        if id == channels.reliable {
            self.reliable_open = true;
        }
        if id == channels.partial {
            self.partial_open = true;
        }
        self.ready_version()
    }

    pub(super) fn channel_data(
        &mut self,
        channels: InputChannels,
        id: ChannelId,
        bytes: &[u8],
    ) -> Option<u16> {
        if id == channels.reliable && bytes.len() >= 2 && self.handshake.is_none() {
            let first = u16::from_le_bytes([bytes[0], bytes[1]]);
            let version = if first == 526 {
                Some(if bytes.len() >= 4 {
                    u16::from_le_bytes([bytes[2], bytes[3]])
                } else {
                    2
                })
            } else {
                (bytes[0] == 0x0e).then_some(first)
            };
            if let Some(version) = version.filter(|version| *version != 0) {
                self.handshake = Some((version, Instant::now()));
            }
        }
        self.ready_version()
    }

    pub(super) fn channel_closed(&mut self, channels: InputChannels, id: ChannelId) -> bool {
        if id != channels.reliable && id != channels.partial {
            return false;
        }
        self.reliable_open = false;
        self.partial_open = false;
        self.handshake = None;
        self.reported = false;
        true
    }

    pub(super) fn is_ready(&self) -> bool {
        self.reliable_open && self.partial_open && self.handshake.is_some()
    }

    fn ready_version(&mut self) -> Option<u16> {
        if self.reported || !self.is_ready() {
            return None;
        }
        self.reported = true;
        self.handshake.map(|(version, _)| version)
    }

    pub(super) fn encode(&self, bytes: &[u8], now: Instant) -> Result<Vec<u8>, String> {
        let Some((version, start)) = self.handshake else {
            return Err("input handshake is incomplete".to_owned());
        };
        let kind = validate_packet(bytes).map_err(|error| error.to_string())?;
        if version <= 2 || kind == 2 {
            return Ok(bytes.to_vec());
        }
        let mut framed = Vec::with_capacity(bytes.len() + 12);
        framed.push(0x23);
        let timestamp: u64 = now
            .saturating_duration_since(start)
            .as_micros()
            .try_into()
            .unwrap_or(u64::MAX);
        framed.extend_from_slice(&timestamp.to_be_bytes());
        if matches!(kind, 5 | 7 | 12) {
            framed.push(0x21);
            framed.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        } else {
            framed.push(0x22);
        }
        framed.extend_from_slice(bytes);
        Ok(framed)
    }
}
