use std::fmt;
use std::time::{Duration, Instant};

use str0m::Rtc;
use str0m::channel::{ChannelConfig, ChannelId, Reliability};

pub(crate) const CONTROL_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(3);
pub(crate) const INPUT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

const CONTROL_RELIABLE_LABEL: &str = "control_channel_reliable";
const CUSTOM_RELIABLE_LABEL: &str = "custom_message_on_sctp_private_reliable";
const CUSTOM_PARTIAL_LABEL: &str = "custom_message_on_sctp_private_partially_reliable";
const CONTROL_PARTIAL_LABEL: &str = "control_channel_partially_reliable";
const CONTROL_UNRELIABLE_LABEL: &str = "control_channel_unreliable";
const INPUT_PARTIAL_LABEL: &str = "input_channel_partially_reliable";
const CURSOR_LABEL: &str = "cursor_channel";
const RTCP_ON_SCTP_LABEL: &str = "rtcp_on_sctp_private";
const PARTIAL_RELIABLE_LIFETIME_MS: u16 = 300;

const COMMAND_SYSTEM_CURSOR: u16 = 0x010f;
const COMMAND_BITMAP_CURSOR: u16 = 0x0110;
const COMMAND_KEEPALIVE: u16 = 0x0200;
const COMMAND_REMOTE_INPUT: u16 = 0x0206;
const COMMAND_ENABLE_INPUT: u16 = 0x020b;
const COMMAND_GAMEPAD: u16 = 0x020d;
const COMMAND_INPUT_VERSION: u16 = 0x020e;
// Controls whether the server composites its cursor into the video. The
// official client enables this for startup, waits for the first ServerControl
// cursor notification, then disables it and renders the reported cursor
// locally. Notifications continue after the server-rendered cursor is hidden.
const COMMAND_MOUSE_CURSOR_CAPTURE: u16 = 0x0308;
// NVB feature type 8 ("track remote cursor image") is distinct from cursor
// capture. Bifrost keeps this enabled after it turns capture back off so the
// server continues publishing cursor shape and mode changes.
const COMMAND_TRACK_REMOTE_CURSOR_IMAGE: u16 = 0x030d;
const COMMAND_WINDOW_STATE: u16 = 0x0320;
const COMMAND_SYSTEM_STATE: u16 = 0x0321;

const INPUT_KEY_DOWN: u32 = 3;
const INPUT_KEY_UP: u32 = 4;
const INPUT_HEARTBEAT: u32 = 2;
const INPUT_MOUSE_ABSOLUTE: u32 = 5;
const INPUT_MOUSE_RELATIVE: u32 = 7;
const INPUT_MOUSE_BUTTON_DOWN: u32 = 8;
const INPUT_MOUSE_BUTTON_UP: u32 = 9;
const INPUT_MOUSE_WHEEL: u32 = 10;
const INPUT_GAMEPAD: u32 = 12;
const INPUT_HAPTICS_ENABLED: u32 = 13;
const INPUT_LOCK_KEYS_SYNC: u32 = 19;
const INPUT_TEXT: u32 = 23;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NvstChannelReliability {
    Reliable,
    Lifetime(u16),
    MaxRetransmits(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NvstChannelDefinition {
    pub(crate) sid: u16,
    pub(crate) label: &'static str,
    pub(crate) ordered: bool,
    pub(crate) reliability: NvstChannelReliability,
}

pub(crate) const NVST_CHANNEL_PROFILE: [NvstChannelDefinition; 8] = [
    NvstChannelDefinition {
        sid: 0,
        label: CONTROL_RELIABLE_LABEL,
        ordered: true,
        reliability: NvstChannelReliability::Reliable,
    },
    NvstChannelDefinition {
        sid: 2,
        label: CUSTOM_RELIABLE_LABEL,
        ordered: true,
        reliability: NvstChannelReliability::Reliable,
    },
    NvstChannelDefinition {
        sid: 4,
        label: CUSTOM_PARTIAL_LABEL,
        ordered: false,
        reliability: NvstChannelReliability::Lifetime(PARTIAL_RELIABLE_LIFETIME_MS),
    },
    NvstChannelDefinition {
        sid: 6,
        label: CONTROL_PARTIAL_LABEL,
        // Mouse motion uses this stream. It must be unordered: with ordered
        // PR-SCTP, one lost report blocks every newer report until its 300 ms
        // lifetime expires, producing delayed motion followed by catch-up.
        ordered: false,
        reliability: NvstChannelReliability::Lifetime(PARTIAL_RELIABLE_LIFETIME_MS),
    },
    NvstChannelDefinition {
        sid: 8,
        label: CONTROL_UNRELIABLE_LABEL,
        ordered: false,
        reliability: NvstChannelReliability::MaxRetransmits(0),
    },
    NvstChannelDefinition {
        sid: 10,
        label: INPUT_PARTIAL_LABEL,
        ordered: false,
        reliability: NvstChannelReliability::Lifetime(PARTIAL_RELIABLE_LIFETIME_MS),
    },
    NvstChannelDefinition {
        sid: 12,
        label: CURSOR_LABEL,
        ordered: true,
        reliability: NvstChannelReliability::Reliable,
    },
    NvstChannelDefinition {
        sid: 14,
        label: RTCP_ON_SCTP_LABEL,
        ordered: true,
        reliability: NvstChannelReliability::Reliable,
    },
];

#[derive(Debug, Clone, Copy)]
pub(crate) struct NvstInputChannels {
    pub(crate) control_reliable: ChannelId,
    pub(crate) custom_reliable: ChannelId,
    pub(crate) custom_partial: ChannelId,
    pub(crate) control_partial: ChannelId,
    pub(crate) control_unreliable: ChannelId,
    pub(crate) input_partial: ChannelId,
    pub(crate) cursor: ChannelId,
    pub(crate) rtcp: ChannelId,
}

impl NvstInputChannels {
    pub(crate) fn create(rtc: &mut Rtc) -> Self {
        let mut ids = Vec::with_capacity(NVST_CHANNEL_PROFILE.len());
        for definition in NVST_CHANNEL_PROFILE {
            ids.push(
                rtc.direct_api()
                    .create_data_channel(channel_config(definition)),
            );
        }
        Self {
            control_reliable: ids[0],
            custom_reliable: ids[1],
            custom_partial: ids[2],
            control_partial: ids[3],
            control_unreliable: ids[4],
            input_partial: ids[5],
            cursor: ids[6],
            rtcp: ids[7],
        }
    }

    pub(crate) fn contains(self, id: ChannelId) -> bool {
        self.all().contains(&id)
    }

    pub(crate) fn label(self, id: ChannelId) -> &'static str {
        if id == self.control_reliable {
            CONTROL_RELIABLE_LABEL
        } else if id == self.custom_reliable {
            CUSTOM_RELIABLE_LABEL
        } else if id == self.custom_partial {
            CUSTOM_PARTIAL_LABEL
        } else if id == self.control_partial {
            CONTROL_PARTIAL_LABEL
        } else if id == self.control_unreliable {
            CONTROL_UNRELIABLE_LABEL
        } else if id == self.input_partial {
            INPUT_PARTIAL_LABEL
        } else if id == self.cursor {
            CURSOR_LABEL
        } else if id == self.rtcp {
            RTCP_ON_SCTP_LABEL
        } else {
            "unknown"
        }
    }

    pub(crate) fn send_control(self, rtc: &mut Rtc, bytes: &[u8]) -> bool {
        write_channel(rtc, self.control_reliable, bytes)
    }

    pub(crate) fn send_partial_control(self, rtc: &mut Rtc, bytes: &[u8]) -> bool {
        write_channel(rtc, self.control_partial, bytes)
    }

    pub(crate) fn send_keepalive(self, rtc: &mut Rtc, stream_value: u32) -> bool {
        self.send_control(rtc, &control_keepalive(stream_value))
    }

    pub(crate) fn send_activation(self, rtc: &mut Rtc, timestamp_us: u64) -> bool {
        activation_chain(timestamp_us)
            .iter()
            .all(|bytes| self.send_control(rtc, bytes))
    }

    pub(crate) fn send_mouse_cursor_capture(self, rtc: &mut Rtc, enabled: bool) -> bool {
        self.send_control(rtc, &mouse_cursor_capture(enabled))
    }

    pub(crate) fn send_remote_cursor_tracking(self, rtc: &mut Rtc, enabled: bool) -> bool {
        self.send_control(rtc, &remote_cursor_tracking(enabled))
    }

    pub(crate) fn send_encoded(self, rtc: &mut Rtc, message: &NvstEncodedInput) -> bool {
        let id = match message.route {
            NvstInputRoute::ControlReliable => self.control_reliable,
            NvstInputRoute::ControlPartial => self.control_partial,
            NvstInputRoute::InputPartial => self.input_partial,
        };
        write_channel(rtc, id, &message.bytes)
    }

    fn all(self) -> [ChannelId; 7] {
        [
            self.control_reliable,
            self.custom_reliable,
            self.custom_partial,
            self.control_partial,
            self.control_unreliable,
            self.input_partial,
            self.cursor,
        ]
    }
}

fn channel_config(definition: NvstChannelDefinition) -> ChannelConfig {
    let reliability = match definition.reliability {
        NvstChannelReliability::Reliable => Reliability::Reliable,
        NvstChannelReliability::Lifetime(lifetime) => Reliability::MaxPacketLifetime { lifetime },
        NvstChannelReliability::MaxRetransmits(retransmits) => {
            Reliability::MaxRetransmits { retransmits }
        }
    };
    ChannelConfig {
        label: definition.label.to_owned(),
        ordered: definition.ordered,
        reliability,
        negotiated: None,
        protocol: String::new(),
    }
}

fn write_channel(rtc: &mut Rtc, id: ChannelId, bytes: &[u8]) -> bool {
    rtc.channel(id)
        .is_some_and(|mut channel| channel.write(true, bytes).unwrap_or(false))
}

#[derive(Debug, Default)]
pub(crate) struct NvstInputChannelState {
    control_open: bool,
    negotiated_version: Option<u16>,
    ready_reported: bool,
    activation_sent: bool,
}

impl NvstInputChannelState {
    pub(crate) fn channel_opened(
        &mut self,
        channels: NvstInputChannels,
        id: ChannelId,
    ) -> Option<u16> {
        if id == channels.control_reliable {
            self.control_open = true;
        }
        self.take_new_ready_version()
    }

    pub(crate) fn channel_data(
        &mut self,
        channels: NvstInputChannels,
        id: ChannelId,
        bytes: &[u8],
    ) -> Option<u16> {
        if id == channels.control_reliable
            && let Some(version) = input_protocol_version(bytes)
        {
            self.negotiated_version = Some(version);
        }
        self.take_new_ready_version()
    }

    pub(crate) fn is_ready(&self) -> bool {
        self.control_open && self.negotiated_version.is_some()
    }

    pub(crate) fn control_is_open(&self) -> bool {
        self.control_open
    }

    pub(crate) fn handshake_timed_out(&self, started_at: Option<Instant>, now: Instant) -> bool {
        !self.is_ready()
            && started_at.is_some_and(|started| {
                now.saturating_duration_since(started) >= INPUT_HANDSHAKE_TIMEOUT
            })
    }

    pub(crate) fn activation_sent(&self) -> bool {
        self.activation_sent
    }

    pub(crate) fn mark_activation_sent(&mut self) {
        self.activation_sent = true;
    }

    pub(crate) fn channel_closed(&mut self, channels: NvstInputChannels, id: ChannelId) -> bool {
        if id == channels.input_partial {
            return self.is_ready();
        }
        if id != channels.control_reliable {
            return false;
        }
        let was_ready = self.is_ready();
        self.control_open = false;
        self.negotiated_version = None;
        self.ready_reported = false;
        self.activation_sent = false;
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

pub(crate) fn next_control_keepalive(now: Instant) -> Instant {
    now + CONTROL_KEEPALIVE_INTERVAL
}

pub(crate) fn input_protocol_version(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 2 {
        return None;
    }
    let mut cursor = 0;
    while bytes.len().saturating_sub(cursor) >= 4 {
        let code = u16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
        let payload_len = usize::from(u16::from_le_bytes([bytes[cursor + 2], bytes[cursor + 3]]));
        let payload_start = cursor + 4;
        let payload_end = payload_start.checked_add(payload_len)?;
        if payload_end > bytes.len() {
            break;
        }
        if code == COMMAND_INPUT_VERSION && payload_len >= 2 {
            return Some(u16::from_le_bytes([
                bytes[payload_start],
                bytes[payload_start + 1],
            ]));
        }
        cursor = payload_end;
    }
    let first = u16::from_le_bytes([bytes[0], bytes[1]]);
    if first == COMMAND_INPUT_VERSION {
        return Some(if bytes.len() >= 4 {
            u16::from_le_bytes([bytes[2], bytes[3]])
        } else {
            2
        });
    }
    (bytes[0] == 0x0e).then_some(first)
}

/// Extracts server cursor notifications from the reliable control stream and
/// normalizes them for the platform output layer.
///
/// Bifrost's current Windows client dispatches 0x010f (system cursor) and
/// 0x0110 (bitmap cursor) through ServerControl. The separately negotiated
/// `cursor_channel` is not used for these notifications by current GFN hosts.
#[cfg(test)]
pub(crate) fn server_cursor_updates(bytes: &[u8]) -> Vec<Vec<u8>> {
    server_cursor_messages(bytes)
        .into_iter()
        .filter_map(|message| message.normalized)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NvstServerCursorMessage {
    pub(crate) command: u16,
    pub(crate) offset: usize,
    pub(crate) raw: Vec<u8>,
    pub(crate) cursor_id: Option<u32>,
    pub(crate) position: Option<(u16, u16)>,
    pub(crate) visible: Option<bool>,
    pub(crate) normalized: Option<Vec<u8>>,
}

/// Scans a data-channel message for cursor commands instead of assuming that
/// the host always places them at byte zero on `control_channel_reliable`.
/// This is deliberately cursor-specific: arbitrary custom-channel payloads
/// must not be interpreted as general NVST control traffic.
pub(crate) fn server_cursor_messages(bytes: &[u8]) -> Vec<NvstServerCursorMessage> {
    let mut updates = Vec::new();
    let mut offset = 0_usize;
    while bytes.len().saturating_sub(offset) >= 4 {
        let code = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        let payload_len = usize::from(u16::from_le_bytes([bytes[offset + 2], bytes[offset + 3]]));
        let payload_start = offset + 4;
        let Some(payload_end) = payload_start.checked_add(payload_len) else {
            break;
        };
        if payload_end > bytes.len() {
            offset += 1;
            continue;
        }
        let payload = &bytes[payload_start..payload_end];
        match code {
            COMMAND_SYSTEM_CURSOR if payload.len() >= 4 => {
                let cursor_id =
                    u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
                let position = (payload.len() >= 8).then(|| {
                    (
                        u16::from_le_bytes([payload[4], payload[5]]),
                        u16::from_le_bytes([payload[6], payload[7]]),
                    )
                });
                let visible = payload.get(8).map(|value| *value != 0);
                let normalized = u8::try_from(cursor_id).ok().map(|cursor_id| {
                    // Platform cursor wire shape:
                    // type, id, hotspot x/y, empty MIME/image, optional x/y.
                    //
                    // The optional byte after x/y is cursor visibility metadata,
                    // not the relative-mouse sentinel. Geronimo preserves the
                    // cursor ID verbatim; predefined ID 0 is what selects locked
                    // input in the official SDL cursor manager.
                    let mut normalized = vec![0, cursor_id, 0, 0, 0, 0, 0];
                    if payload.len() >= 8 {
                        normalized.extend_from_slice(&payload[4..8]);
                    }
                    normalized
                });
                updates.push(NvstServerCursorMessage {
                    command: code,
                    offset,
                    raw: bytes[offset..payload_end].to_vec(),
                    cursor_id: Some(cursor_id),
                    position,
                    visible,
                    normalized,
                });
            }
            COMMAND_BITMAP_CURSOR => {
                // Bitmap cursor payloads have a distinct native pixel layout.
                // Keep detecting them explicitly so they cannot be mistaken for
                // input/control commands while raw bitmap support is added.
                updates.push(NvstServerCursorMessage {
                    command: code,
                    offset,
                    raw: bytes[offset..payload_end].to_vec(),
                    cursor_id: None,
                    position: None,
                    visible: None,
                    normalized: None,
                });
            }
            _ => {
                offset += 1;
                continue;
            }
        }
        offset = payload_end;
    }
    updates
}

pub(crate) fn native_input_types(packet: &[u8]) -> Result<Vec<u32>, NvstInputCodecError> {
    native_events(packet, 0).map(|events| {
        events
            .into_iter()
            .filter_map(|event| read_u32_le(event.bytes, 0))
            .collect()
    })
}

pub(crate) fn native_input_type_name(input_type: u32) -> &'static str {
    match input_type {
        INPUT_HEARTBEAT => "heartbeat",
        INPUT_KEY_DOWN => "key-down",
        INPUT_KEY_UP => "key-up",
        INPUT_MOUSE_ABSOLUTE => "mouse-absolute",
        INPUT_MOUSE_RELATIVE => "mouse-relative",
        INPUT_MOUSE_BUTTON_DOWN => "mouse-button-down",
        INPUT_MOUSE_BUTTON_UP => "mouse-button-up",
        INPUT_MOUSE_WHEEL => "mouse-wheel",
        INPUT_GAMEPAD => "gamepad",
        INPUT_HAPTICS_ENABLED => "haptics-enabled",
        INPUT_LOCK_KEYS_SYNC => "lock-keys-sync",
        INPUT_TEXT => "text",
        _ => "unknown",
    }
}

pub(crate) fn native_input_type_is_motion(input_type: u32) -> bool {
    matches!(input_type, INPUT_MOUSE_ABSOLUTE | INPUT_MOUSE_RELATIVE)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NvstInputRoute {
    ControlReliable,
    ControlPartial,
    InputPartial,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NvstEncodedInput {
    pub(crate) route: NvstInputRoute,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NvstInputCodecError {
    Malformed(&'static str),
    UnsupportedType(u32),
    UnsupportedText,
}

impl fmt::Display for NvstInputCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(reason) => write!(formatter, "malformed native input: {reason}"),
            Self::UnsupportedType(input_type) => {
                write!(formatter, "unsupported native input type {input_type}")
            }
            Self::UnsupportedText => {
                formatter.write_str("native NVST text contains an unmappable character")
            }
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct NvstInputCodec {
    gamepad_sequence: u8,
    gamepad_registered: bool,
}

impl NvstInputCodec {
    pub(crate) fn encode(
        &mut self,
        packet: &[u8],
        fallback_timestamp_us: u64,
    ) -> Result<Vec<NvstEncodedInput>, NvstInputCodecError> {
        let events = native_events(packet, fallback_timestamp_us)?;
        let mut encoded = Vec::new();
        for event in events {
            let input_type = read_u32_le(event.bytes, 0)
                .ok_or(NvstInputCodecError::Malformed("missing input type"))?;
            match input_type {
                // The native bundle has its own control-channel keepalive. Renderer heartbeat
                // packets are transport keepalives, not remote HID events, so consuming them is
                // correct and prevents a benign packet from disabling all native input.
                INPUT_HEARTBEAT => {}
                INPUT_KEY_DOWN | INPUT_KEY_UP => {
                    require_len(event.bytes, 18, "short keyboard packet")?;
                    let key = read_u16_be(event.bytes, 4).expect("keyboard length checked");
                    let modifiers = read_u16_be(event.bytes, 6).expect("keyboard length checked");
                    let timestamp = read_u64_be(event.bytes, 10).unwrap_or(event.timestamp_us);
                    encoded.push(remote_input_message(
                        remote_input_packet(
                            input_type,
                            &[
                                (key >> 8) as u8,
                                key as u8,
                                (modifiers >> 8) as u8,
                                modifiers as u8,
                                0,
                                0,
                                0,
                                0,
                                0,
                                0,
                                0,
                                0,
                                0,
                                0,
                            ],
                        ),
                        timestamp,
                    ));
                }
                INPUT_MOUSE_RELATIVE => {
                    require_len(event.bytes, 22, "short relative mouse packet")?;
                    let body = [
                        event.bytes[4],
                        event.bytes[5],
                        event.bytes[6],
                        event.bytes[7],
                        0,
                        0,
                    ];
                    encoded.push(remote_input_message(
                        remote_input_packet(input_type, &body),
                        read_u64_be(event.bytes, 14).unwrap_or(event.timestamp_us),
                    ));
                }
                INPUT_MOUSE_ABSOLUTE => {
                    require_len(event.bytes, 26, "short absolute mouse packet")?;
                    let body = [
                        event.bytes[4],
                        event.bytes[5],
                        event.bytes[6],
                        event.bytes[7],
                        0x08,
                        0x00,
                        event.bytes[10],
                        event.bytes[11],
                        event.bytes[12],
                        event.bytes[13],
                    ];
                    encoded.push(remote_input_message(
                        remote_input_packet(input_type, &body),
                        read_u64_be(event.bytes, 18).unwrap_or(event.timestamp_us),
                    ));
                }
                INPUT_MOUSE_BUTTON_DOWN | INPUT_MOUSE_BUTTON_UP => {
                    require_len(event.bytes, 18, "short mouse button packet")?;
                    encoded.push(remote_input_message(
                        remote_input_packet(input_type, &[event.bytes[4], 0]),
                        read_u64_be(event.bytes, 10).unwrap_or(event.timestamp_us),
                    ));
                }
                INPUT_MOUSE_WHEEL => {
                    require_len(event.bytes, 22, "short mouse wheel packet")?;
                    let body = [0, 0, event.bytes[6], event.bytes[7], 0, 0];
                    encoded.push(remote_input_message(
                        remote_input_packet(input_type, &body),
                        read_u64_be(event.bytes, 14).unwrap_or(event.timestamp_us),
                    ));
                }
                INPUT_GAMEPAD => {
                    require_len(event.bytes, 38, "short gamepad packet")?;
                    let timestamp = read_u64_le(event.bytes, 30).unwrap_or(event.timestamp_us);
                    if !self.gamepad_registered {
                        self.gamepad_registered = true;
                        encoded.push(NvstEncodedInput {
                            route: NvstInputRoute::ControlReliable,
                            bytes: device_descriptor(timestamp, 3),
                        });
                    }
                    self.gamepad_sequence = self.gamepad_sequence.wrapping_add(1);
                    encoded.push(NvstEncodedInput {
                        route: NvstInputRoute::InputPartial,
                        bytes: gamepad_command(event.bytes, timestamp, self.gamepad_sequence),
                    });
                }
                INPUT_HAPTICS_ENABLED => {
                    // Haptics capability is negotiated by the native gamepad descriptor. The
                    // browser-side toggle has no separate NVST control command to forward.
                }
                INPUT_LOCK_KEYS_SYNC => {
                    require_len(event.bytes, 5, "short lock-key sync packet")?;
                    encoded.push(remote_input_message(
                        remote_input_packet(input_type, &[event.bytes[4]]),
                        event.timestamp_us,
                    ));
                }
                INPUT_TEXT => encoded.extend(text_messages(&event)?),
                other => return Err(NvstInputCodecError::UnsupportedType(other)),
            }
        }
        Ok(encoded)
    }
}

#[derive(Debug, Clone, Copy)]
struct NativeEvent<'a> {
    bytes: &'a [u8],
    timestamp_us: u64,
}

fn native_events(
    packet: &[u8],
    fallback_timestamp_us: u64,
) -> Result<Vec<NativeEvent<'_>>, NvstInputCodecError> {
    if packet.is_empty() {
        return Err(NvstInputCodecError::Malformed("empty packet"));
    }
    if packet[0] == 0x22 {
        if packet.len() < 5 || read_u32_le(packet, 1) != Some(INPUT_TEXT) {
            return Err(NvstInputCodecError::Malformed(
                "unknown leading 0x22 packet",
            ));
        }
        return Ok(vec![NativeEvent {
            bytes: &packet[1..],
            timestamp_us: fallback_timestamp_us,
        }]);
    }
    if packet[0] != 0x23 {
        return Ok(vec![NativeEvent {
            bytes: packet,
            timestamp_us: fallback_timestamp_us,
        }]);
    }
    require_len(packet, 10, "short protocol-v3 wrapper")?;
    let timestamp_us = read_u64_be(packet, 1).expect("wrapper length checked");
    let mut cursor = 9;
    let mut events = Vec::new();
    while cursor < packet.len() {
        match packet[cursor] {
            0x22 => {
                let start = cursor + 1;
                let input_type = read_u32_le(packet, start)
                    .ok_or(NvstInputCodecError::Malformed("short single-event wrapper"))?;
                let length = native_event_length(input_type, packet.len() - start)?;
                let end = start + length;
                if end > packet.len() {
                    return Err(NvstInputCodecError::Malformed("truncated single event"));
                }
                events.push(NativeEvent {
                    bytes: &packet[start..end],
                    timestamp_us,
                });
                cursor = end;
            }
            0x21 => {
                require_remaining(packet, cursor, 3, "short length-prefixed wrapper")?;
                let length =
                    usize::from(u16::from_be_bytes([packet[cursor + 1], packet[cursor + 2]]));
                let start = cursor + 3;
                let end = start + length;
                if end > packet.len() {
                    return Err(NvstInputCodecError::Malformed(
                        "truncated length-prefixed event",
                    ));
                }
                events.push(NativeEvent {
                    bytes: &packet[start..end],
                    timestamp_us,
                });
                cursor = end;
            }
            0x26 => {
                require_remaining(packet, cursor, 7, "short partially-reliable wrapper")?;
                if packet[cursor + 4] != 0x21 {
                    return Err(NvstInputCodecError::Malformed(
                        "missing gamepad payload marker",
                    ));
                }
                let length =
                    usize::from(u16::from_be_bytes([packet[cursor + 5], packet[cursor + 6]]));
                let start = cursor + 7;
                let end = start + length;
                if end > packet.len() {
                    return Err(NvstInputCodecError::Malformed("truncated gamepad event"));
                }
                events.push(NativeEvent {
                    bytes: &packet[start..end],
                    timestamp_us,
                });
                cursor = end;
            }
            _ => return Err(NvstInputCodecError::Malformed("unknown protocol-v3 marker")),
        }
    }
    Ok(events)
}

fn native_event_length(input_type: u32, remaining: usize) -> Result<usize, NvstInputCodecError> {
    match input_type {
        INPUT_HEARTBEAT => Ok(4),
        INPUT_KEY_DOWN | INPUT_KEY_UP | INPUT_MOUSE_BUTTON_DOWN | INPUT_MOUSE_BUTTON_UP => Ok(18),
        INPUT_MOUSE_RELATIVE | INPUT_MOUSE_WHEEL => Ok(22),
        INPUT_MOUSE_ABSOLUTE => Ok(26),
        INPUT_GAMEPAD => Ok(38),
        INPUT_HAPTICS_ENABLED => Ok(6),
        INPUT_LOCK_KEYS_SYNC => Ok(5),
        INPUT_TEXT => Ok(remaining),
        other => Err(NvstInputCodecError::UnsupportedType(other)),
    }
}

fn remote_input_message(inner: Vec<u8>, timestamp_us: u64) -> NvstEncodedInput {
    let input_type = read_u32_le(&inner, 4).expect("remote input packet has type");
    let mut body = inner;
    let padded = 24.max(body.len().div_ceil(8) * 8);
    body.resize(padded, 0);
    let timestamp_count = usize::from(matches!(
        input_type,
        INPUT_KEY_DOWN | INPUT_KEY_UP | INPUT_MOUSE_ABSOLUTE
    )) + 1;
    for _ in 0..timestamp_count {
        body.extend_from_slice(&timestamp_us.to_le_bytes());
    }
    NvstEncodedInput {
        route: if matches!(input_type, INPUT_MOUSE_RELATIVE | INPUT_MOUSE_ABSOLUTE) {
            NvstInputRoute::ControlPartial
        } else {
            NvstInputRoute::ControlReliable
        },
        bytes: control_command(COMMAND_REMOTE_INPUT, &remote_input_packet(0x0e, &body)),
    }
}

fn remote_input_packet(input_type: u32, body: &[u8]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(8 + body.len());
    packet.extend_from_slice(&(4_u32 + body.len() as u32).to_be_bytes());
    packet.extend_from_slice(&input_type.to_le_bytes());
    packet.extend_from_slice(body);
    packet
}

fn gamepad_command(packet: &[u8], timestamp_us: u64, sequence: u8) -> Vec<u8> {
    let mut payload = Vec::with_capacity(52);
    payload.push(0x23);
    payload.extend_from_slice(&timestamp_us.to_be_bytes());
    let mut body = [
        0x26, 0x00, 0x00, 0x00, 0x22, 0x0c, 0x00, 0x00, 0x00, 0x1a, 0x00, 0x00, 0x00, 0x03, 0x00,
        0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x55, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    body[3] = sequence;
    body[17..19].copy_from_slice(&packet[12..14]);
    body[19] = packet[14];
    body[20] = packet[15];
    body[21..29].copy_from_slice(&packet[16..24]);
    payload.extend_from_slice(&body);
    control_command(COMMAND_GAMEPAD, &payload)
}

fn text_messages(event: &NativeEvent<'_>) -> Result<Vec<NvstEncodedInput>, NvstInputCodecError> {
    require_len(event.bytes, 5, "short text packet")?;
    let text =
        std::str::from_utf8(&event.bytes[5..]).map_err(|_| NvstInputCodecError::UnsupportedText)?;
    let mut messages = Vec::with_capacity(text.chars().count() * 2);
    for character in text.chars() {
        let (virtual_key, modifiers) =
            text_keystroke(character).ok_or(NvstInputCodecError::UnsupportedText)?;
        for input_type in [INPUT_KEY_DOWN, INPUT_KEY_UP] {
            let mut body = Vec::with_capacity(14);
            body.extend_from_slice(&virtual_key.to_be_bytes());
            body.extend_from_slice(&modifiers.to_be_bytes());
            body.resize(14, 0);
            messages.push(remote_input_message(
                remote_input_packet(input_type, &body),
                event.timestamp_us,
            ));
        }
    }
    Ok(messages)
}

fn text_keystroke(character: char) -> Option<(u16, u16)> {
    let virtual_key = match character {
        'A'..='Z' => return Some((character as u16, 1)),
        'a'..='z' => return Some((character.to_ascii_uppercase() as u16, 0)),
        '0'..='9' => return Some((character as u16, 0)),
        ' ' => return Some((0x20, 0)),
        '\t' => return Some((0x09, 0)),
        '\n' | '\r' => return Some((0x0d, 0)),
        '!' => 0x31,
        '@' => 0x32,
        '#' => 0x33,
        '$' => 0x34,
        '%' => 0x35,
        '^' => 0x36,
        '&' => 0x37,
        '*' => 0x38,
        '(' => 0x39,
        ')' => 0x30,
        '_' => 0xbd,
        '+' => 0xbb,
        '{' => 0xdb,
        '}' => 0xdd,
        '|' => 0xdc,
        ':' => 0xba,
        '"' => 0xde,
        '<' => 0xbc,
        '>' => 0xbe,
        '?' => 0xbf,
        '~' => 0xc0,
        '-' => return Some((0xbd, 0)),
        '=' => return Some((0xbb, 0)),
        '[' => return Some((0xdb, 0)),
        ']' => return Some((0xdd, 0)),
        '\\' => return Some((0xdc, 0)),
        ';' => return Some((0xba, 0)),
        '\'' => return Some((0xde, 0)),
        ',' => return Some((0xbc, 0)),
        '.' => return Some((0xbe, 0)),
        '/' => return Some((0xbf, 0)),
        '`' => return Some((0xc0, 0)),
        _ => return None,
    };
    Some((virtual_key, 1))
}

fn activation_chain(timestamp_us: u64) -> [Vec<u8>; 7] {
    [
        enable_input(1, false),
        device_descriptor(timestamp_us, 2),
        mouse_cursor_capture(true),
        remote_cursor_tracking(true),
        state_change(COMMAND_WINDOW_STATE, 19, 0),
        state_change(COMMAND_SYSTEM_STATE, 0, 0),
        enable_input(1, true),
    ]
}

fn mouse_cursor_capture(enabled: bool) -> Vec<u8> {
    control_command(COMMAND_MOUSE_CURSOR_CAPTURE, &[u8::from(enabled)])
}

fn remote_cursor_tracking(enabled: bool) -> Vec<u8> {
    control_command(COMMAND_TRACK_REMOTE_CURSOR_IMAGE, &[u8::from(enabled)])
}

fn control_keepalive(stream_value: u32) -> Vec<u8> {
    control_command(COMMAND_KEEPALIVE, &stream_value.to_le_bytes())
}

fn enable_input(counter: u32, enabled: bool) -> Vec<u8> {
    let mut payload = Vec::with_capacity(12);
    payload.extend_from_slice(&0_u32.to_le_bytes());
    payload.extend_from_slice(&counter.to_le_bytes());
    payload.extend_from_slice(&u32::from(enabled).to_le_bytes());
    control_command(COMMAND_ENABLE_INPUT, &payload)
}

fn device_descriptor(timestamp_us: u64, descriptor_index: u8) -> Vec<u8> {
    let mut payload = Vec::with_capacity(48);
    payload.push(0x23);
    payload.extend_from_slice(&timestamp_us.to_be_bytes());
    let mut body = [
        0x22, 0x0c, 0x00, 0x00, 0x00, 0x1a, 0x00, 0x00, 0x00, 0x02, 0x00, 0x14, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x55, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    body[9] = descriptor_index;
    payload.extend_from_slice(&body);
    control_command(COMMAND_GAMEPAD, &payload)
}

fn state_change(code: u16, state: u32, frame_number: u32) -> Vec<u8> {
    // Bifrost serializes these as three little-endian u32 fields: stream
    // index, requested state, and frame number. The desktop client announces
    // window state 19 at frame zero once input is activated. Advertising the
    // previous all-zero window state leaves the session looking inactive and
    // prevents the server from sending its system-cursor mode updates.
    let mut payload = Vec::with_capacity(12);
    payload.extend_from_slice(&0_u32.to_le_bytes());
    payload.extend_from_slice(&state.to_le_bytes());
    payload.extend_from_slice(&frame_number.to_le_bytes());
    control_command(code, &payload)
}

fn control_command(code: u16, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(4 + payload.len());
    bytes.extend_from_slice(&code.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

fn require_len(
    bytes: &[u8],
    minimum: usize,
    reason: &'static str,
) -> Result<(), NvstInputCodecError> {
    if bytes.len() < minimum {
        return Err(NvstInputCodecError::Malformed(reason));
    }
    Ok(())
}

fn require_remaining(
    bytes: &[u8],
    offset: usize,
    minimum: usize,
    reason: &'static str,
) -> Result<(), NvstInputCodecError> {
    if bytes.len().saturating_sub(offset) < minimum {
        return Err(NvstInputCodecError::Malformed(reason));
    }
    Ok(())
}

fn read_u16_be(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_u64_be(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_be_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

fn read_u64_le(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(value: &str) -> Vec<u8> {
        assert_eq!(value.len() % 2, 0, "hex input must contain whole bytes");
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                u8::from_str_radix(std::str::from_utf8(pair).expect("hex utf8"), 16)
                    .expect("hex byte")
            })
            .collect()
    }

    #[test]
    fn channel_profile_matches_official_sids_labels_and_reliability() {
        assert_eq!(
            NVST_CHANNEL_PROFILE.map(|definition| definition.sid),
            [0, 2, 4, 6, 8, 10, 12, 14]
        );
        assert_eq!(
            NVST_CHANNEL_PROFILE.map(|definition| definition.label),
            [
                CONTROL_RELIABLE_LABEL,
                CUSTOM_RELIABLE_LABEL,
                CUSTOM_PARTIAL_LABEL,
                CONTROL_PARTIAL_LABEL,
                CONTROL_UNRELIABLE_LABEL,
                INPUT_PARTIAL_LABEL,
                CURSOR_LABEL,
                RTCP_ON_SCTP_LABEL,
            ]
        );
        assert_eq!(
            NVST_CHANNEL_PROFILE.map(|definition| definition.ordered),
            [true, true, false, false, false, false, true, true]
        );
        assert_eq!(
            NVST_CHANNEL_PROFILE.map(|definition| definition.reliability),
            [
                NvstChannelReliability::Reliable,
                NvstChannelReliability::Reliable,
                NvstChannelReliability::Lifetime(300),
                NvstChannelReliability::Lifetime(300),
                NvstChannelReliability::MaxRetransmits(0),
                NvstChannelReliability::Lifetime(300),
                NvstChannelReliability::Reliable,
                NvstChannelReliability::Reliable,
            ]
        );
        let configs = NVST_CHANNEL_PROFILE.map(channel_config);
        assert_eq!(
            configs.clone().map(|config| config.ordered),
            [true, true, false, false, false, false, true, true]
        );
        assert_eq!(
            configs.map(|config| config.reliability),
            [
                Reliability::Reliable,
                Reliability::Reliable,
                Reliability::MaxPacketLifetime { lifetime: 300 },
                Reliability::MaxPacketLifetime { lifetime: 300 },
                Reliability::MaxRetransmits { retransmits: 0 },
                Reliability::MaxPacketLifetime { lifetime: 300 },
                Reliability::Reliable,
                Reliability::Reliable,
            ]
        );

        let mut rtc = Rtc::new(Instant::now());
        let channels = NvstInputChannels::create(&mut rtc);
        assert_eq!(channels.label(channels.rtcp), RTCP_ON_SCTP_LABEL);
        assert!(!channels.contains(channels.rtcp));
    }

    #[test]
    fn parses_full_control_handshake_and_requires_only_control_plus_version() {
        assert_eq!(
            input_protocol_version(&[0x0e, 0x02, 0x02, 0x00, 0x03, 0x00]),
            Some(3)
        );
        assert_eq!(input_protocol_version(&[0x0e, 0x02, 0x03, 0x00]), Some(3));
        assert_eq!(input_protocol_version(&[0x0e, 0x03]), Some(0x030e));
        assert_eq!(input_protocol_version(&[0x0e, 0x02]), Some(2));
        assert_eq!(input_protocol_version(&[1]), None);
        assert_eq!(
            input_protocol_version(&[0x01, 0x01, 0x00, 0x00, 0x0e, 0x02, 0x02, 0x00, 0x03, 0x00,]),
            Some(3)
        );

        let mut rtc = Rtc::new(Instant::now());
        let channels = NvstInputChannels::create(&mut rtc);
        let mut state = NvstInputChannelState::default();
        assert_eq!(
            state.channel_opened(channels, channels.control_reliable),
            None
        );
        assert!(!state.is_ready());
        assert_eq!(
            state.channel_data(
                channels,
                channels.input_partial,
                &[0x0e, 0x02, 0x02, 0x00, 0x03, 0x00],
            ),
            None
        );
        assert!(!state.is_ready());
        assert_eq!(
            state.channel_data(
                channels,
                channels.control_reliable,
                &[0x0e, 0x02, 0x02, 0x00, 0x03, 0x00],
            ),
            Some(3)
        );
        assert!(state.is_ready());
    }

    #[test]
    fn extracts_system_cursor_notifications_from_server_control() {
        let visible = hex("0f010900010000000c80168001");
        assert_eq!(
            server_cursor_updates(&visible),
            vec![hex("000100000000000c801680")]
        );

        let mut concatenated = visible;
        concatenated.extend_from_slice(&hex("0f0109000c0000000000000000"));
        assert_eq!(
            server_cursor_updates(&concatenated),
            vec![hex("000100000000000c801680"), hex("000c000000000000000000")]
        );

        assert_eq!(
            server_cursor_updates(&hex("0f010800010000000c801680")),
            vec![hex("000100000000000c801680")]
        );
        assert_eq!(
            server_cursor_updates(&hex("0f01040000000000")),
            vec![hex("00000000000000")]
        );
        assert!(server_cursor_updates(&hex("0f010400010000")).is_empty());
        assert!(server_cursor_updates(&hex("0a0102007b7d")).is_empty());
    }

    #[test]
    fn control_keepalive_and_activation_are_byte_exact() {
        assert_eq!(control_keepalive(0), hex("0002040000000000"));
        assert_eq!(mouse_cursor_capture(false), hex("0803010000"));
        assert_eq!(mouse_cursor_capture(true), hex("0803010001"));
        assert_eq!(remote_cursor_tracking(false), hex("0d03010000"));
        assert_eq!(remote_cursor_tracking(true), hex("0d03010001"));
        let now = Instant::now();
        assert_eq!(
            next_control_keepalive(now).duration_since(now),
            Duration::from_secs(3)
        );

        let chain = activation_chain(20_102_193);
        assert_eq!(chain[0], hex("0b020c00000000000100000000000000"));
        assert_eq!(
            chain[1],
            hex(
                "0d02300023000000000132bc31220c0000001a000000020014000000000000000000000000000000550000000000000000000000"
            )
        );
        assert_eq!(chain[2], hex("0803010001"));
        assert_eq!(chain[3], hex("0d03010001"));
        assert_eq!(chain[4], hex("20030c00000000001300000000000000"));
        assert_eq!(chain[5], hex("21030c00000000000000000000000000"));
        assert_eq!(chain[6], hex("0b020c00000000000100000001000000"));
    }

    #[test]
    fn keyboard_and_mouse_translate_to_byte_exact_control_commands() {
        let mut codec = NvstInputCodec::default();
        let mut key = vec![0; 18];
        key[..4].copy_from_slice(&INPUT_KEY_DOWN.to_le_bytes());
        key[4..6].copy_from_slice(&0x41_u16.to_be_bytes());
        key[6..8].copy_from_slice(&1_u16.to_be_bytes());
        key[10..18].copy_from_slice(&0x015b_15b1_u64.to_be_bytes());
        let mut wrapped_key = vec![0x23];
        wrapped_key.extend_from_slice(&99_u64.to_be_bytes());
        wrapped_key.push(0x22);
        wrapped_key.extend_from_slice(&key);
        let key = codec.encode(&wrapped_key, 0).expect("key");
        assert_eq!(key.len(), 1);
        assert_eq!(key[0].route, NvstInputRoute::ControlReliable);
        assert_eq!(
            key[0].bytes,
            hex(
                "060230000000002c0e000000000000120300000000410001000000000000000000000000b1155b0100000000b1155b0100000000"
            )
        );

        let mut mouse = vec![0; 22];
        mouse[..4].copy_from_slice(&INPUT_MOUSE_RELATIVE.to_le_bytes());
        mouse[4..6].copy_from_slice(&24_i16.to_be_bytes());
        mouse[6..8].copy_from_slice(&24_i16.to_be_bytes());
        mouse[14..22].copy_from_slice(&0x0186_1330_u64.to_be_bytes());
        let mut wrapped_mouse = vec![0x23];
        wrapped_mouse.extend_from_slice(&100_u64.to_be_bytes());
        wrapped_mouse.push(0x21);
        wrapped_mouse.extend_from_slice(&(mouse.len() as u16).to_be_bytes());
        wrapped_mouse.extend_from_slice(&mouse);
        let mouse = codec.encode(&wrapped_mouse, 0).expect("mouse");
        assert_eq!(mouse[0].route, NvstInputRoute::ControlPartial);
        assert_eq!(
            mouse[0].bytes,
            hex(
                "06022800000000240e0000000000000a07000000001800180000000000000000000000003013860100000000"
            )
        );
    }

    #[test]
    fn standard_gamepad_translates_to_input_partial_and_registers_once() {
        let mut codec = NvstInputCodec::default();
        let mut gamepad = vec![0; 38];
        gamepad[..4].copy_from_slice(&INPUT_GAMEPAD.to_le_bytes());
        gamepad[12..14].copy_from_slice(&0x1000_u16.to_le_bytes());
        gamepad[30..38].copy_from_slice(&0x015b_171a_u64.to_le_bytes());
        let mut wrapped_gamepad = vec![0x23];
        wrapped_gamepad.extend_from_slice(&101_u64.to_be_bytes());
        wrapped_gamepad.extend_from_slice(&[0x26, 0, 0, 1, 0x21]);
        wrapped_gamepad.extend_from_slice(&(gamepad.len() as u16).to_be_bytes());
        wrapped_gamepad.extend_from_slice(&gamepad);

        let first = codec.encode(&wrapped_gamepad, 0).expect("gamepad");
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].route, NvstInputRoute::ControlReliable);
        assert_eq!(first[1].route, NvstInputRoute::InputPartial);
        assert_eq!(
            first[1].bytes,
            hex(
                "0d0234002300000000015b171a26000001220c0000001a000000030014000010000000000000000000000000550000000000000000000000"
            )
        );

        let second = codec.encode(&wrapped_gamepad, 0).expect("second gamepad");
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].route, NvstInputRoute::InputPartial);
        assert_eq!(second[0].bytes[16], 2);
    }

    #[test]
    fn transport_control_inputs_do_not_disable_native_input() {
        let mut codec = NvstInputCodec::default();

        assert!(
            codec
                .encode(&INPUT_HEARTBEAT.to_le_bytes(), 0)
                .unwrap()
                .is_empty()
        );

        let mut haptics = vec![0x23];
        haptics.extend_from_slice(&11_u64.to_be_bytes());
        haptics.push(0x22);
        haptics.extend_from_slice(&INPUT_HAPTICS_ENABLED.to_le_bytes());
        haptics.extend_from_slice(&1_u16.to_be_bytes());
        assert!(codec.encode(&haptics, 0).unwrap().is_empty());

        let mut lock_keys = vec![0x23];
        lock_keys.extend_from_slice(&12_u64.to_be_bytes());
        lock_keys.push(0x22);
        lock_keys.extend_from_slice(&INPUT_LOCK_KEYS_SYNC.to_le_bytes());
        lock_keys.push(0b011);
        let encoded = codec.encode(&lock_keys, 0).expect("lock-key sync");
        assert_eq!(encoded.len(), 1);
        assert_eq!(encoded[0].route, NvstInputRoute::ControlReliable);
        let lock_keys_type = INPUT_LOCK_KEYS_SYNC.to_le_bytes();
        assert!(
            encoded[0]
                .bytes
                .windows(lock_keys_type.len())
                .any(|bytes| bytes == lock_keys_type)
        );
        assert!(encoded[0].bytes.contains(&0b011));
    }

    #[test]
    fn unsupported_packets_fail_closed() {
        let mut codec = NvstInputCodec::default();
        assert_eq!(
            codec.encode(&99_u32.to_le_bytes(), 0),
            Err(NvstInputCodecError::UnsupportedType(99))
        );
        let mut text = vec![0x22];
        text.extend_from_slice(&INPUT_TEXT.to_le_bytes());
        text.extend_from_slice("é".as_bytes());
        assert_eq!(
            codec.encode(&text, 0),
            Err(NvstInputCodecError::UnsupportedText)
        );
    }

    #[test]
    fn closure_and_timeout_state_are_predictable() {
        let now = Instant::now();
        let mut rtc = Rtc::new(now);
        let channels = NvstInputChannels::create(&mut rtc);
        let mut state = NvstInputChannelState::default();
        state.channel_opened(channels, channels.control_reliable);
        assert!(!state.handshake_timed_out(Some(now), now + Duration::from_millis(4_999)));
        assert!(state.handshake_timed_out(Some(now), now + INPUT_HANDSHAKE_TIMEOUT));
        state.channel_data(
            channels,
            channels.control_reliable,
            &[0x0e, 0x02, 0x02, 0x00, 0x03, 0x00],
        );
        assert!(!state.handshake_timed_out(Some(now), now + INPUT_HANDSHAKE_TIMEOUT));
        state.mark_activation_sent();
        assert_eq!(
            state.channel_data(
                channels,
                channels.control_reliable,
                &[0x0e, 0x02, 0x02, 0x00, 0x03, 0x00],
            ),
            None
        );
        assert!(state.activation_sent());
        assert!(state.channel_closed(channels, channels.input_partial));
        assert!(state.is_ready());
        assert!(state.channel_closed(channels, channels.control_reliable));
        assert!(!state.is_ready());
        assert!(!state.channel_closed(channels, channels.input_partial));
        assert!(!state.is_ready());
    }
}
