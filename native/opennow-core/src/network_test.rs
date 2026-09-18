use hmac::{Hmac, Mac};
use sha2::Sha256;
#[cfg(test)]
use subtle::ConstantTimeEq;

use crate::gfn::ServiceError;

pub const MESSAGE_TYPE_MTU_PROBE: u32 = 7;
pub const MESSAGE_TYPE_MTU_RESPONSE: u32 = 8;
pub const MAC_LEN: usize = 32;

pub const REPLY_PREFIX_THRESHOLD: usize = 484;

const MAX_MESSAGE_BYTES: usize = 64 * 1024;
const MAX_FIELDS: usize = 64;
const MAX_STRING_BYTES: usize = 16 * 1024;
const MAX_SESSION_KEY_BYTES: usize = 4096;
const MAX_SESSION_ID_BYTES: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub enum WireValue {
    Varint(u64),
    Fixed64(u64),
    Fixed32(u32),
    Bytes(Vec<u8>),
}

pub const TYPE_FIELD: u32 = 1;
pub const SESSION_ID_FIELD: u32 = 2;
pub const MAC_FIELD: u32 = 10;
pub const SEQUENCE_FIELD: u32 = 12;
pub const PAYLOAD_SIZE_FIELD: u32 = 13;
pub const KNOWN_FIELD_MAX: u32 = 18;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NetworkTestMessage {
    known: [Option<WireValue>; (KNOWN_FIELD_MAX + 1) as usize],
    unknown: Vec<(u32, WireValue)>,
}

impl NetworkTestMessage {
    pub fn mtu_probe(payload_size: u32, session_id: &[u8], sequence: u32) -> Self {
        let mut message = Self::default();
        message.set_message_type(MESSAGE_TYPE_MTU_PROBE);
        message.set_session_id(session_id.to_vec());
        message.set_sequence(sequence);
        message.set_payload_size(payload_size);
        message
    }

    pub fn message_type(&self) -> u32 {
        match self.known[TYPE_FIELD as usize] {
            Some(WireValue::Varint(raw)) => raw as u32,
            _ => 0,
        }
    }

    pub fn set_message_type(&mut self, value: u32) {
        self.known[TYPE_FIELD as usize] = Some(WireValue::Varint(u64::from(value)));
    }

    pub fn payload_size(&self) -> Option<u32> {
        match self.known[PAYLOAD_SIZE_FIELD as usize] {
            Some(WireValue::Varint(raw)) => u32::try_from(raw).ok(),
            _ => None,
        }
    }

    pub fn set_payload_size(&mut self, value: u32) {
        self.known[PAYLOAD_SIZE_FIELD as usize] = Some(WireValue::Varint(u64::from(value)));
    }

    pub fn session_id(&self) -> &[u8] {
        match &self.known[SESSION_ID_FIELD as usize] {
            Some(WireValue::Bytes(raw)) => raw,
            _ => &[],
        }
    }

    pub fn set_session_id(&mut self, value: Vec<u8>) {
        self.known[SESSION_ID_FIELD as usize] = Some(WireValue::Bytes(value));
    }

    #[cfg(test)]
    pub fn sequence(&self) -> Option<u32> {
        match self.known[SEQUENCE_FIELD as usize] {
            Some(WireValue::Varint(raw)) => u32::try_from(raw).ok(),
            _ => None,
        }
    }

    pub fn set_sequence(&mut self, value: u32) {
        self.known[SEQUENCE_FIELD as usize] = Some(WireValue::Varint(u64::from(value)));
    }

    #[cfg(test)]
    pub fn mac(&self) -> &[u8] {
        match &self.known[MAC_FIELD as usize] {
            Some(WireValue::Bytes(raw)) => raw,
            _ => &[],
        }
    }

    pub fn set_mac(&mut self, value: Vec<u8>) {
        self.known[MAC_FIELD as usize] = Some(WireValue::Bytes(value));
    }

    #[cfg(test)]
    pub fn unknown(&self) -> &[(u32, WireValue)] {
        &self.unknown
    }

    #[cfg(test)]
    pub fn push_unknown(&mut self, number: u32, value: WireValue) {
        self.unknown.push((number, value));
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut body = Vec::new();
        self.encode_body(&mut body, true);
        body
    }

    #[cfg(test)]
    pub fn encode_response(&self, datagram_bytes: usize) -> Vec<u8> {
        let body = self.encode();
        let prefix = u32::try_from(body.len()).unwrap_or(u32::MAX).to_le_bytes();
        let mut datagram = Vec::with_capacity(datagram_bytes.max(body.len() + 4));
        datagram.extend_from_slice(&prefix);
        datagram.extend_from_slice(&body);
        datagram.resize(datagram_bytes.max(datagram.len()), 0x5a);
        datagram
    }

    fn encode_body(&self, out: &mut Vec<u8>, include_mac: bool) {
        for number in 1..=KNOWN_FIELD_MAX {
            let Some(value) = &self.known[number as usize] else {
                continue;
            };
            if number == MAC_FIELD && !include_mac {
                continue;
            }
            write_tag(out, number, wire_type(value));
            write_value(out, value);
        }
        for (number, value) in &self.unknown {
            write_tag(out, *number, wire_type(value));
            write_value(out, value);
        }
    }

    pub fn authenticated(&self, key: &[u8]) -> Result<Vec<u8>, ServiceError> {
        let mut body = Vec::new();
        self.encode_body(&mut body, false);
        Ok(compute_mac(key, &body)?.to_vec())
    }

    pub fn seal(&mut self, key: &[u8]) -> Result<(), ServiceError> {
        let mac = self.authenticated(key)?;
        self.set_mac(mac);
        Ok(())
    }

    #[cfg(test)]
    pub fn verify(&self, key: &[u8]) -> Result<bool, ServiceError> {
        let mac = self.mac();
        if mac.len() != MAC_LEN {
            return Ok(false);
        }
        let expected = self.authenticated(key)?;
        Ok(bool::from(expected.as_slice().ct_eq(mac)))
    }

    pub fn decode_reply(datagram: &[u8]) -> Result<Self, ServiceError> {
        if datagram.len() >= REPLY_PREFIX_THRESHOLD {
            Self::decode_response(datagram)
        } else {
            Self::decode(datagram)
        }
    }

    pub fn decode_response(datagram: &[u8]) -> Result<Self, ServiceError> {
        if datagram.len() > MAX_MESSAGE_BYTES {
            return Err(invalid("network test message is too large"));
        }
        let prefix = datagram
            .get(..4)
            .ok_or_else(|| invalid("network test message is truncated"))?;
        let body_len = usize::try_from(u32::from_le_bytes(prefix.try_into().expect("four bytes")))
            .map_err(|_| invalid("network test length"))?;
        let end = 4_usize
            .checked_add(body_len)
            .filter(|end| *end <= datagram.len())
            .ok_or_else(|| invalid("network test message is truncated"))?;
        Self::decode(&datagram[4..end])
    }

    pub fn decode(body: &[u8]) -> Result<Self, ServiceError> {
        if body.len() > MAX_MESSAGE_BYTES {
            return Err(invalid("network test message is too large"));
        }
        let end = body.len();
        let mut offset = 0_usize;
        let mut message = Self::default();
        let mut fields = 0_usize;
        while offset < end {
            fields += 1;
            if fields > MAX_FIELDS {
                return Err(invalid("network test message has too many fields"));
            }
            let (tag, next) = read_varint(body.get(offset..).unwrap_or_default())?;
            offset += next;
            let number =
                u32::try_from(tag >> 3).map_err(|_| invalid("network test field number"))?;
            if number == 0 {
                return Err(invalid("network test field number is zero"));
            }
            let value = decode_value(body, &mut offset, tag & 0x7)?;
            match known_wire_type(number) {
                Some(expected) if expected == tag & 0x7 => {
                    message.known[number as usize] = Some(value);
                }
                _ => message.unknown.push((number, value)),
            }
        }
        Ok(message)
    }
}

fn known_wire_type(number: u32) -> Option<u64> {
    match number {
        1 | 6 | 8 | 9 | 12 | 13 | 14 | 16 | 17 | 18 => Some(0),
        2 | 3 | 5 | 7 | 10 => Some(2),
        11 | 15 => Some(5),
        _ => None,
    }
}

fn decode_value(body: &[u8], offset: &mut usize, wire: u64) -> Result<WireValue, ServiceError> {
    Ok(match wire {
        0 => {
            let (value, next) = read_varint(body.get(*offset..).unwrap_or_default())?;
            *offset += next;
            WireValue::Varint(value)
        }
        1 => {
            let raw = take(body, *offset, 8)?;
            *offset += 8;
            WireValue::Fixed64(u64::from_le_bytes(raw.try_into().expect("eight bytes")))
        }
        2 => {
            let (len, next) = read_varint(body.get(*offset..).unwrap_or_default())?;
            *offset += next;
            let len = usize::try_from(len).map_err(|_| invalid("network test length"))?;
            if len > MAX_STRING_BYTES {
                return Err(invalid("network test field is too large"));
            }
            let raw = take(body, *offset, len)?;
            *offset += len;
            WireValue::Bytes(raw.to_vec())
        }
        5 => {
            let raw = take(body, *offset, 4)?;
            *offset += 4;
            WireValue::Fixed32(u32::from_le_bytes(raw.try_into().expect("four bytes")))
        }
        _ => return Err(invalid("network test field type is unsupported")),
    })
}

fn wire_type(value: &WireValue) -> u64 {
    match value {
        WireValue::Varint(_) => 0,
        WireValue::Fixed64(_) => 1,
        WireValue::Bytes(_) => 2,
        WireValue::Fixed32(_) => 5,
    }
}

fn write_tag(out: &mut Vec<u8>, number: u32, wire: u64) {
    write_varint(out, (u64::from(number) << 3) | wire);
}

fn write_value(out: &mut Vec<u8>, value: &WireValue) {
    match value {
        WireValue::Varint(raw) => write_varint(out, *raw),
        WireValue::Fixed64(raw) => out.extend_from_slice(&raw.to_le_bytes()),
        WireValue::Fixed32(raw) => out.extend_from_slice(&raw.to_le_bytes()),
        WireValue::Bytes(raw) => {
            write_varint(out, raw.len() as u64);
            out.extend_from_slice(raw);
        }
    }
}

fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn read_varint(input: &[u8]) -> Result<(u64, usize), ServiceError> {
    let mut value = 0_u64;
    let mut shift = 0_u32;
    for (index, byte) in input.iter().enumerate().take(10) {
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok((value, index + 1));
        }
        shift += 7;
    }
    Err(invalid("network test varint is malformed"))
}

fn take(input: &[u8], offset: usize, len: usize) -> Result<&[u8], ServiceError> {
    input
        .get(offset..offset + len)
        .ok_or_else(|| invalid("network test message is truncated"))
}

fn compute_mac(key: &[u8], body: &[u8]) -> Result<[u8; MAC_LEN], ServiceError> {
    if key.is_empty() || key.len() > 4096 {
        return Err(invalid("network test key length is invalid"));
    }
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key)
        .map_err(|_| invalid("network test key is invalid"))?;
    mac.update(body);
    let mut tag = [0_u8; MAC_LEN];
    tag.copy_from_slice(&mac.finalize().into_bytes());
    Ok(tag)
}

fn invalid(message: &str) -> ServiceError {
    ServiceError {
        code: "network-test-invalid",
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [0x5a; 32];

    #[test]
    fn mtu_probe_encodes_the_verified_field_numbers() {
        let message = NetworkTestMessage::mtu_probe(1_280, b"nt-1", 1);
        let encoded = message.encode();
        assert_eq!(
            encoded,
            vec![
                0x08, 0x07, 0x12, 0x04, b'n', b't', b'-', b'1', 0x60, 0x01, 0x68, 0x80, 0x0a,
            ],
            "bare protobuf: type 7, session id, sequence and payload size"
        );
        assert_eq!(message.session_id(), b"nt-1");
        assert_eq!(message.sequence(), Some(1));
    }

    #[test]
    fn mac_is_field_ten_and_covers_the_body_without_it() {
        let mut message = NetworkTestMessage::mtu_probe(1_280, b"nt-1", 1);
        let unsealed_body = message.encode();
        message.seal(&KEY).unwrap();
        let body = message.encode();

        assert_eq!(&body[..2], &[0x08, 0x07], "field 1 carries the type");
        assert_eq!(message.mac().len(), MAC_LEN);
        let expected = compute_mac(&KEY, &unsealed_body).unwrap();
        assert_eq!(message.mac(), expected.as_slice());

        let mac_at = body
            .windows(2)
            .position(|pair| pair == [0x52, 0x20])
            .expect("field 10 tag");
        let mut without_mac = body.clone();
        without_mac.drain(mac_at..mac_at + 2 + MAC_LEN);
        assert_eq!(
            without_mac, unsealed_body,
            "sealing only inserts field 10 and leaves the covered body intact"
        );
        assert_eq!(
            &body[mac_at + 2..mac_at + 2 + MAC_LEN],
            expected.as_slice(),
            "mac covers the body without itself"
        );
    }

    #[test]
    fn response_framing_is_a_length_prefix_then_body_then_padding() {
        let mut message = NetworkTestMessage::mtu_probe(1_280, b"nt-1", 1);
        message.seal(&KEY).unwrap();
        let body = message.encode();
        let datagram = message.encode_response(1_300);
        assert_eq!(
            datagram.len(),
            1_300,
            "datagram is exactly the promised size"
        );
        assert_eq!(
            u32::from_le_bytes(datagram[..4].try_into().unwrap()) as usize,
            body.len(),
            "prefix is the little-endian protobuf body length"
        );
        assert_eq!(&datagram[4..4 + body.len()], body.as_slice());

        let decoded = NetworkTestMessage::decode_response(&datagram).unwrap();
        assert_eq!(decoded, message);
        assert!(decoded.verify(&KEY).unwrap());
        assert_eq!(decoded.payload_size(), Some(1_280));
    }

    #[test]
    fn a_sealed_message_verifies_and_rejects_tampering() {
        let mut message = NetworkTestMessage::mtu_probe(1_280, b"nt-1", 1);
        message.seal(&KEY).unwrap();
        assert!(message.verify(&KEY).unwrap());

        let mut changed = message.clone();
        changed.set_payload_size(1_279);
        assert!(!changed.verify(&KEY).unwrap());

        let mut rekeyed = message.clone();
        assert!(!rekeyed.verify(&[0x11; 32]).unwrap());

        let mut tampered = rekeyed.mac().to_vec();
        tampered[0] ^= 0x01;
        rekeyed.set_mac(tampered);
        assert!(!rekeyed.verify(&KEY).unwrap());
    }

    #[test]
    fn decoding_round_trips_unknown_fields() {
        let mut message = NetworkTestMessage::mtu_probe(1_500, b"nt-1", 1);
        message.push_unknown(4, WireValue::Bytes(b"session".to_vec()));
        message.push_unknown(19, WireValue::Fixed32(1.5_f32.to_bits()));
        message.push_unknown(20, WireValue::Varint(9));
        message.seal(&KEY).unwrap();

        let decoded = NetworkTestMessage::decode(&message.encode()).unwrap();
        assert_eq!(decoded, message);
        assert!(decoded.verify(&KEY).unwrap());
    }

    #[test]
    fn decode_rejects_malformed_and_oversized_input() {
        assert!(NetworkTestMessage::decode(&[0x7f, 0x08, 0x07]).is_err());
        let oversized = vec![0xff; MAX_MESSAGE_BYTES + 1];
        assert!(NetworkTestMessage::decode(&oversized).is_err());

        assert!(NetworkTestMessage::decode_response(&[0x05, 0x00]).is_err());
        assert!(
            NetworkTestMessage::decode_response(&[0xff, 0xff, 0xff, 0xff, 0x08]).is_err(),
            "declared body length must fit in the datagram"
        );
        let truncated = {
            let mut message = NetworkTestMessage::mtu_probe(1_280, b"nt-1", 1);
            message.seal(&KEY).unwrap();
            let mut datagram = message.encode_response(1_300);
            datagram.truncate(datagram.len() - 30);
            datagram
        };
        assert!(
            NetworkTestMessage::decode_response(&truncated).is_ok(),
            "padding is outside the prefixed body"
        );
    }

    #[test]
    fn sealing_fails_closed_without_usable_key_material() {
        let mut message = NetworkTestMessage::mtu_probe(1_280, b"nt-1", 1);
        assert!(message.seal(&[]).is_err());
        assert!(message.authenticated(&[]).is_err());
        assert_eq!(message.mac(), &[] as &[u8]);
        assert!(!message.verify(&[]).unwrap());
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NetworkTestThresholds {
    pub bandwidth_recommended_mbps: f64,
    pub bandwidth_limit_mbps: f64,
    pub latency_recommended_ms: f64,
    pub latency_limit_ms: f64,
    pub packet_loss_recommended_pct: f64,
    pub packet_loss_limit_pct: f64,
}

#[derive(Clone, PartialEq)]
pub struct NetworkTestSession {
    pub session_id: String,
    pub server_id: String,
    pub address: std::net::IpAddr,
    pub port: u16,
    pub secure: bool,
    pub hmac_key: Option<Vec<u8>>,
    pub thresholds: NetworkTestThresholds,
}

impl std::fmt::Debug for NetworkTestSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkTestSession")
            .field("session_id", &self.session_id)
            .field("server_id", &self.server_id)
            .field("address", &self.address)
            .field("port", &self.port)
            .field("secure", &self.secure)
            .field("hmac_key", &self.hmac_key.as_ref().map(|_| "[redacted]"))
            .field("thresholds", &self.thresholds)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeOutcome {
    pub measured_datagram_bytes: Option<u32>,
    pub probes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayProfile {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

pub fn allocation_body(platform: &str, profile: DisplayProfile) -> serde_json::Value {
    let DisplayProfile { width, height, fps } = profile;
    serde_json::json!({
        "netTestRequestData": {
            "clientPlatformName": platform,
            "netTestProfile": {
                "widthInPixels": width,
                "heightInPixels": height,
                "framesPerSecond": fps,
            },
        }
    })
}

pub fn parse_allocation(response: &serde_json::Value) -> Result<NetworkTestSession, ServiceError> {
    let session = response
        .get("netTestSession")
        .ok_or_else(|| invalid("network test response has no session"))?;
    let session_id = session
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= MAX_SESSION_ID_BYTES)
        .ok_or_else(|| invalid("network test session has no id"))?
        .to_owned();
    let server_id = session
        .get("serverId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let connection = session
        .get("connectionInfo")
        .and_then(serde_json::Value::as_array)
        .and_then(|entries| entries.first())
        .ok_or_else(|| invalid("network test session has no connection info"))?;
    let address = connection
        .get("ip")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse::<std::net::IpAddr>().ok())
        .ok_or_else(|| invalid("network test session has no literal address"))?;
    let port = connection
        .get("port")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .filter(|value| *value != 0)
        .ok_or_else(|| invalid("network test session has no port"))?;
    let secure = connection
        .get("appLevelProtocol")
        .and_then(serde_json::Value::as_u64)
        == Some(5);
    let hmac_key = match session.get("hmacKey") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => Some(parse_session_key(value)?),
    };
    let thresholds = session
        .get("netTestThresholds")
        .ok_or_else(|| invalid("network test session has no thresholds"))?;
    let number = |key: &str| {
        thresholds
            .get(key)
            .and_then(serde_json::Value::as_f64)
            .unwrap_or_default()
    };
    Ok(NetworkTestSession {
        session_id,
        server_id,
        address,
        port,
        secure,
        hmac_key,
        thresholds: NetworkTestThresholds {
            bandwidth_recommended_mbps: number("recommendedBandwidthMBPS"),
            bandwidth_limit_mbps: number("requiredBandwidthMBPS"),
            latency_recommended_ms: number("recommendedLatencyMS"),
            latency_limit_ms: number("requiredLatencyMS"),
            packet_loss_recommended_pct: number("recommendedPacketLossPct"),
            packet_loss_limit_pct: number("requiredPacketLossPct"),
        },
    })
}

fn parse_session_key(value: &serde_json::Value) -> Result<Vec<u8>, ServiceError> {
    let raw = value
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid("network test session key is not a string"))?;
    let bytes = raw.as_bytes();
    if bytes.len() > MAX_SESSION_KEY_BYTES {
        return Err(invalid("network test session key is too large"));
    }
    if bytes.is_empty() || bytes.contains(&0) {
        return Err(invalid("network test session key is not usable"));
    }
    Ok(bytes.to_vec())
}

pub fn nettest_url(base: &url::Url) -> Result<url::Url, ServiceError> {
    let host = base.host_str().unwrap_or_default();
    if host.is_empty() {
        return Err(invalid("network test zone has no host"));
    }
    let loopback = matches!(host, "127.0.0.1" | "::1" | "localhost");
    if base.scheme() != "https" && !(loopback && base.scheme() == "http") {
        return Err(invalid("network test zone must use https"));
    }
    base.join("v2/nettestsession")
        .map_err(|_| invalid("invalid network test session URL"))
}

pub const PROBE_BUDGET: std::time::Duration = std::time::Duration::from_millis(400);
pub const PROBE_FLOOR_BYTES: u32 = 548;
pub const PROBE_CEILING_BYTES: u32 = 1_472;
const PROBE_ATTEMPT_WAIT: std::time::Duration = std::time::Duration::from_millis(40);
const MAX_PROBES: usize = 12;
const PROBE_STEP: u32 = 16;

fn reply_is_accepted(reply: &NetworkTestMessage, session_id: &[u8], size: u32) -> bool {
    reply.message_type() == MESSAGE_TYPE_MTU_RESPONSE
        && reply.session_id() == session_id
        && reply.payload_size() == Some(size)
}

fn confirm_datagram(
    socket: &std::net::UdpSocket,
    peer: std::net::SocketAddr,
    key: &[u8],
    session_id: &[u8],
    sequence: u32,
    size: u32,
    deadline: std::time::Instant,
) -> bool {
    let deadline = deadline.min(std::time::Instant::now() + PROBE_ATTEMPT_WAIT);
    let mut message = NetworkTestMessage::mtu_probe(size, session_id, sequence);
    if message.seal(key).is_err() {
        return false;
    }
    if socket.send_to(&message.encode(), peer).is_err() {
        return false;
    }
    let mut buffer = vec![0_u8; MAX_MESSAGE_BYTES];
    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() || socket.set_read_timeout(Some(remaining)).is_err() {
            return false;
        }
        let Ok((length, source)) = socket.recv_from(&mut buffer) else {
            return false;
        };
        if std::time::Instant::now() >= deadline {
            return false;
        }
        if source != peer || length < size as usize {
            continue;
        }
        let Ok(reply) = NetworkTestMessage::decode_reply(&buffer[..length]) else {
            continue;
        };
        if reply_is_accepted(&reply, session_id, size) {
            return true;
        }
    }
    false
}

pub fn probe_mtu(
    socket: &std::net::UdpSocket,
    peer: std::net::SocketAddr,
    key: &[u8],
    session_id: &[u8],
    floor: u32,
    ceiling: u32,
) -> Result<ProbeOutcome, ServiceError> {
    if ceiling <= floor {
        return Err(invalid("network test probe range is empty"));
    }
    let deadline = std::time::Instant::now() + PROBE_BUDGET;
    let mut probes = 0_usize;
    let confirmed = |size: u32, probes: &mut usize| -> bool {
        if *probes >= MAX_PROBES || std::time::Instant::now() >= deadline {
            return false;
        }
        *probes += 1;
        confirm_datagram(
            socket,
            peer,
            key,
            session_id,
            u32::try_from(*probes).unwrap_or(u32::MAX),
            size,
            deadline,
        )
    };
    if confirmed(ceiling, &mut probes) {
        return Ok(ProbeOutcome {
            measured_datagram_bytes: Some(ceiling),
            probes,
        });
    }
    if !confirmed(floor, &mut probes) {
        return Ok(ProbeOutcome {
            measured_datagram_bytes: None,
            probes,
        });
    }
    let mut low = floor;
    let mut high = ceiling;
    while high - low > PROBE_STEP && probes < MAX_PROBES {
        let middle = low + (high - low) / 2;
        if confirmed(middle, &mut probes) {
            low = middle;
        } else {
            high = middle;
        }
    }
    Ok(ProbeOutcome {
        measured_datagram_bytes: Some(low),
        probes,
    })
}

#[cfg(test)]
mod probe_tests {
    use super::*;
    use std::net::{SocketAddr, UdpSocket};
    use std::time::Duration;

    const KEY: [u8; 32] = [0x33; 32];
    const SESSION: &[u8] = b"nt-1";

    fn vendor_reply_body(session: &[u8], size: u32, message_type: u32) -> Vec<u8> {
        let mut body = Vec::new();
        assert!(message_type < 0x80 && session.len() < 0x80);
        body.push(0x08);
        body.push(message_type as u8);
        body.push(0x12);
        body.push(session.len() as u8);
        body.extend_from_slice(session);
        body.push(0x68);
        let mut value = size;
        while value >= 0x80 {
            body.push((value as u8) | 0x80);
            value >>= 7;
        }
        body.push(value as u8);
        body
    }

    fn vendor_datagram(session: &[u8], size: u32, message_type: u32) -> Vec<u8> {
        let body = vendor_reply_body(session, size, message_type);
        let mut datagram = Vec::with_capacity(size as usize);
        datagram.extend_from_slice(&(body.len() as u32).to_le_bytes());
        datagram.extend_from_slice(&body);
        datagram.resize(size as usize, 0x5a);
        datagram
    }

    fn vendor_server<F>(cap: u32, respond: F) -> (SocketAddr, std::thread::JoinHandle<usize>)
    where
        F: Fn(u32) -> Vec<Vec<u8>> + Send + 'static,
    {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(250)))
            .unwrap();
        let address = socket.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut served = 0_usize;
            let mut buffer = vec![0_u8; 4096];
            while let Ok((length, peer)) = socket.recv_from(&mut buffer) {
                let Ok(request) = NetworkTestMessage::decode(&buffer[..length]) else {
                    continue;
                };
                let Some(size) = request.payload_size() else {
                    continue;
                };
                if size > cap {
                    continue;
                }
                for datagram in respond(size) {
                    let _ = socket.send_to(&datagram, peer);
                }
                served += 1;
            }
            served
        });
        (address, handle)
    }

    fn foreign_server<F>(respond: F) -> (SocketAddr, std::thread::JoinHandle<usize>)
    where
        F: Fn(u32) -> Vec<u8> + Send + 'static,
    {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(250)))
            .unwrap();
        let address = socket.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut served = 0_usize;
            let mut buffer = vec![0_u8; 4096];
            while let Ok((length, peer)) = socket.recv_from(&mut buffer) {
                let Ok(request) = NetworkTestMessage::decode(&buffer[..length]) else {
                    continue;
                };
                let Some(size) = request.payload_size() else {
                    continue;
                };
                let datagram = respond(size);
                let deadline = std::time::Instant::now() + Duration::from_millis(150);
                while std::time::Instant::now() < deadline {
                    let _ = sender.send_to(&datagram, peer);
                    std::thread::sleep(Duration::from_millis(5));
                }
                served += 1;
            }
            served
        });
        (address, handle)
    }

    fn client_socket() -> UdpSocket {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(10)))
            .unwrap();
        socket
    }

    #[test]
    fn a_path_that_carries_the_ceiling_needs_no_adjustment() {
        let (peer, server) =
            vendor_server(u32::MAX, |size| vec![vendor_datagram(SESSION, size, 8)]);
        let socket = client_socket();
        let outcome = probe_mtu(&socket, peer, &KEY, SESSION, PROBE_FLOOR_BYTES, 1_340).unwrap();
        assert_eq!(outcome.measured_datagram_bytes, Some(1_340));
        drop(socket);
        let _ = server.join();
    }

    #[test]
    fn a_narrower_path_reports_the_largest_confirmed_size() {
        let (peer, server) = vendor_server(1_280, |size| vec![vendor_datagram(SESSION, size, 8)]);
        let socket = client_socket();
        let outcome = probe_mtu(&socket, peer, &KEY, SESSION, PROBE_FLOOR_BYTES, 1_340).unwrap();
        let measured = outcome.measured_datagram_bytes.expect("measured");
        assert!(measured <= 1_280, "measured {measured}");
        assert!(measured + PROBE_STEP * 2 >= 1_280, "measured {measured}");
        assert!(outcome.probes <= MAX_PROBES);
        drop(socket);
        let _ = server.join();
    }

    #[test]
    fn a_silent_path_yields_no_measurement() {
        let (peer, server) = vendor_server(1_280, |_| Vec::new());
        let socket = client_socket();
        let outcome = probe_mtu(&socket, peer, &KEY, SESSION, PROBE_FLOOR_BYTES, 1_340).unwrap();
        assert_eq!(outcome.measured_datagram_bytes, None);
        drop(socket);
        let _ = server.join();
    }

    #[test]
    fn replies_with_a_foreign_type_are_ignored() {
        let (peer, server) =
            vendor_server(u32::MAX, |size| vec![vendor_datagram(SESSION, size, 7)]);
        let socket = client_socket();
        let outcome = probe_mtu(&socket, peer, &KEY, SESSION, PROBE_FLOOR_BYTES, 1_340).unwrap();
        assert_eq!(outcome.measured_datagram_bytes, None);
        drop(socket);
        let _ = server.join();
    }

    #[test]
    fn replies_for_another_session_are_ignored() {
        let (peer, server) =
            vendor_server(u32::MAX, |size| vec![vendor_datagram(b"other", size, 8)]);
        let socket = client_socket();
        let outcome = probe_mtu(&socket, peer, &KEY, SESSION, PROBE_FLOOR_BYTES, 1_340).unwrap();
        assert_eq!(outcome.measured_datagram_bytes, None);
        drop(socket);
        let _ = server.join();
    }

    #[test]
    fn stale_replies_for_another_size_are_ignored() {
        let (peer, server) = vendor_server(u32::MAX, |size| {
            vec![
                vendor_datagram(SESSION, size + 64, 8),
                vendor_datagram(SESSION, size, 8),
            ]
        });
        let socket = client_socket();
        let outcome = probe_mtu(&socket, peer, &KEY, SESSION, PROBE_FLOOR_BYTES, 1_340).unwrap();
        assert_eq!(outcome.measured_datagram_bytes, Some(1_340));
        drop(socket);
        let _ = server.join();
    }

    #[test]
    fn duplicate_replies_do_not_change_the_measurement() {
        let (peer, server) = vendor_server(u32::MAX, |size| {
            let datagram = vendor_datagram(SESSION, size, 8);
            vec![datagram.clone(), datagram]
        });
        let socket = client_socket();
        let outcome = probe_mtu(&socket, peer, &KEY, SESSION, PROBE_FLOOR_BYTES, 1_340).unwrap();
        assert_eq!(outcome.measured_datagram_bytes, Some(1_340));
        drop(socket);
        let _ = server.join();
    }

    #[test]
    fn replies_from_a_foreign_source_are_ignored() {
        let (peer, server) = foreign_server(|size| vendor_datagram(SESSION, size, 8));
        let socket = client_socket();
        let outcome = probe_mtu(&socket, peer, &KEY, SESSION, PROBE_FLOOR_BYTES, 1_340).unwrap();
        assert_eq!(outcome.measured_datagram_bytes, None);
        drop(socket);
        let _ = server.join();
    }

    #[test]
    fn truncated_and_malformed_replies_are_ignored() {
        let (peer, server) = vendor_server(u32::MAX, |size| {
            let mut overclaimed = vec![0_u8; size as usize];
            overclaimed[..4].copy_from_slice(&(size + 1_000).to_le_bytes());
            overclaimed[4] = 0x08;
            overclaimed[5] = 0x08;
            vec![overclaimed, vec![0xff; size as usize]]
        });
        let socket = client_socket();
        let outcome = probe_mtu(&socket, peer, &KEY, SESSION, PROBE_FLOOR_BYTES, 1_340).unwrap();
        assert_eq!(outcome.measured_datagram_bytes, None);
        drop(socket);
        let _ = server.join();
    }

    #[test]
    fn short_replies_are_ignored() {
        let (peer, server) = vendor_server(u32::MAX, |size| {
            let datagram = vendor_datagram(SESSION, size, 8);
            vec![datagram[..datagram.len() - 16].to_vec()]
        });
        let socket = client_socket();
        let outcome = probe_mtu(&socket, peer, &KEY, SESSION, PROBE_FLOOR_BYTES, 1_340).unwrap();
        assert_eq!(outcome.measured_datagram_bytes, None);
        drop(socket);
        let _ = server.join();
    }

    #[test]
    fn a_floor_below_usefulness_yields_no_measurement() {
        let (peer, server) = vendor_server(100, |size| vec![vendor_datagram(SESSION, size, 8)]);
        let socket = client_socket();
        let outcome = probe_mtu(&socket, peer, &KEY, SESSION, PROBE_FLOOR_BYTES, 1_340).unwrap();
        assert_eq!(outcome.measured_datagram_bytes, None);
        drop(socket);
        let _ = server.join();
    }

    #[test]
    fn an_empty_probe_range_is_rejected() {
        let socket = client_socket();
        let peer: SocketAddr = "127.0.0.1:1".parse().unwrap();
        assert!(probe_mtu(&socket, peer, &KEY, SESSION, 1_340, PROBE_FLOOR_BYTES).is_err());
    }

    #[test]
    fn unsigned_replies_are_accepted_by_the_correlation_rules() {
        let datagram = vendor_datagram(SESSION, 1_280, 8);
        assert_eq!(
            datagram.len(),
            1_280,
            "vendor datagram is the requested size"
        );
        let reply = NetworkTestMessage::decode_reply(&datagram).unwrap();
        assert!(reply_is_accepted(&reply, SESSION, 1_280));
        assert!(!reply_is_accepted(&reply, b"other", 1_280), "session");
        assert!(!reply_is_accepted(&reply, SESSION, 1_279), "size");
        assert!(reply.mac().is_empty(), "vendor replies are unsigned");
    }

    #[test]
    fn bare_replies_below_the_prefix_threshold_are_parsed() {
        let body = vendor_reply_body(SESSION, 300, 8);
        assert!(body.len() < REPLY_PREFIX_THRESHOLD);
        let reply = NetworkTestMessage::decode_reply(&body).unwrap();
        assert!(reply_is_accepted(&reply, SESSION, 300));
    }

    #[test]
    fn unrelated_replies_do_not_renew_a_probe_attempt() {
        use std::time::Instant;

        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let peer = socket.local_addr().unwrap();
        let client = client_socket();
        let client_address = client.local_addr().unwrap();
        let deadline = Instant::now() + PROBE_BUDGET;
        let server = std::thread::spawn(move || {
            while Instant::now() < deadline {
                let _ = socket.send_to(&vendor_datagram(b"other", 1_340, 8), client_address);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let started = Instant::now();
        let confirmed = confirm_datagram(&client, peer, &KEY, SESSION, 1, 1_340, deadline);
        let elapsed = started.elapsed();
        server.join().unwrap();
        assert!(!confirmed);
        assert!(
            elapsed < PROBE_BUDGET / 2,
            "unrelated replies extended one probe attempt to {elapsed:?}"
        );
    }

    #[test]
    fn a_hostile_flood_cannot_stretch_the_probe_deadline() {
        use std::time::Instant;

        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let peer = socket.local_addr().unwrap();
        let client = client_socket();
        let client_address = client.local_addr().unwrap();
        let started = Instant::now();
        let flood_until = started + PROBE_BUDGET - Duration::from_millis(10);
        let server = std::thread::spawn(move || {
            let mut sent = 0_usize;
            while Instant::now() < flood_until {
                let _ = socket.send_to(&vendor_datagram(b"other", 1_340, 8), client_address);
                sent += 1;
                std::thread::sleep(Duration::from_millis(1));
            }
            sent
        });
        let outcome = probe_mtu(&client, peer, &KEY, SESSION, PROBE_FLOOR_BYTES, 1_340).unwrap();
        let elapsed = Instant::now() - started;
        assert_eq!(outcome.measured_datagram_bytes, None);
        assert!(
            elapsed < PROBE_BUDGET + PROBE_ATTEMPT_WAIT / 2,
            "the hostile flood stretched the probe to {elapsed:?}"
        );
        let sent = server.join().unwrap();
        assert!(sent > 20, "the flood never reached the client: {sent}");
    }
}
#[cfg(test)]
mod allocation_tests {
    use super::*;

    fn response(app_level_protocol: u64) -> serde_json::Value {
        serde_json::json!({
            "requestStatus": {"requestId": "req-1", "serverId": "zone-1", "statusCode": "200"},
            "netTestSession": {
                "sessionId": "nt-42",
                "serverId": "srv-9",
                "connectionInfo": [{
                    "ip": "192.0.2.44",
                    "port": 49100,
                    "appLevelProtocol": app_level_protocol,
                }],
                "netTestThresholds": {
                    "recommendedBandwidthMBPS": 50.0,
                    "requiredBandwidthMBPS": 25.0,
                    "recommendedLatencyMS": 40.0,
                    "requiredLatencyMS": 80.0,
                    "recommendedPacketLossPct": 1.0,
                    "requiredPacketLossPct": 3.0,
                }
            }
        })
    }

    #[test]
    fn debug_output_never_contains_the_session_key() {
        let mut payload = response(5);
        payload["netTestSession"]["hmacKey"] = serde_json::json!("super-secret-key-material");
        let session = parse_allocation(&payload).unwrap();
        assert_eq!(
            session.hmac_key.as_deref(),
            Some("super-secret-key-material".as_bytes())
        );
        let debug = format!("{session:?}");
        assert!(
            !debug.contains("super-secret-key-material"),
            "debug leaked the key: {debug}"
        );
        assert!(debug.contains("[redacted]"));
    }

    #[test]
    fn non_literal_addresses_and_oversized_ids_are_rejected() {
        let mut hostname = response(5);
        hostname["netTestSession"]["connectionInfo"][0]["ip"] =
            serde_json::json!("net-test.example.com");
        assert!(
            parse_allocation(&hostname).is_err(),
            "hostnames must not reach a resolver"
        );

        let mut long_id = response(5);
        long_id["netTestSession"]["sessionId"] = serde_json::json!("n".repeat(257));
        assert!(parse_allocation(&long_id).is_err());
    }

    #[test]
    fn unusable_session_keys_are_rejected() {
        let mut nul_key = String::from("nul");
        nul_key.push('\0');
        nul_key.push_str("byte");
        for key in [
            serde_json::json!(""),
            serde_json::json!(7),
            serde_json::json!(nul_key),
        ] {
            let mut payload = response(5);
            payload["netTestSession"]["hmacKey"] = key.clone();
            assert!(parse_allocation(&payload).is_err(), "accepted {key:?}");
        }
        let mut payload = response(5);
        payload["netTestSession"]["hmacKey"] = serde_json::Value::Null;
        assert_eq!(parse_allocation(&payload).unwrap().hmac_key, None);
    }

    #[test]
    fn allocation_body_matches_the_recovered_schema() {
        let body = allocation_body(
            "GFN-PC",
            DisplayProfile {
                width: 1920,
                height: 1080,
                fps: 60,
            },
        );
        assert_eq!(body["netTestRequestData"]["clientPlatformName"], "GFN-PC");
        assert_eq!(
            body["netTestRequestData"]["netTestProfile"]["widthInPixels"],
            1920
        );
        assert_eq!(
            body["netTestRequestData"]["netTestProfile"]["heightInPixels"],
            1080
        );
        assert_eq!(
            body["netTestRequestData"]["netTestProfile"]["framesPerSecond"],
            60
        );
    }

    #[test]
    fn allocation_response_is_parsed_with_thresholds() {
        let session = parse_allocation(&response(5)).unwrap();
        assert_eq!(session.session_id, "nt-42");
        assert_eq!(session.server_id, "srv-9");
        assert_eq!(
            session.address,
            "192.0.2.44".parse::<std::net::IpAddr>().unwrap()
        );
        assert_eq!(session.port, 49_100);
        assert!(session.secure);
        assert_eq!(session.thresholds.bandwidth_limit_mbps, 25.0);
        assert_eq!(session.thresholds.latency_limit_ms, 80.0);
        assert_eq!(session.thresholds.packet_loss_limit_pct, 3.0);
        assert!(!parse_allocation(&response(4)).unwrap().secure);
    }

    #[test]
    fn malformed_allocation_responses_are_rejected() {
        assert!(parse_allocation(&serde_json::json!({})).is_err());
        let mut missing_port = response(5);
        missing_port["netTestSession"]["connectionInfo"][0]
            .as_object_mut()
            .unwrap()
            .remove("port");
        assert!(parse_allocation(&missing_port).is_err());
        let mut empty_session = response(5);
        empty_session["netTestSession"]["sessionId"] = serde_json::json!("");
        assert!(parse_allocation(&empty_session).is_err());
    }

    #[test]
    fn allocation_requires_https_and_a_token() {
        assert!(nettest_url(&url::Url::parse("http://example.test").unwrap()).is_err());
        assert!(nettest_url(&url::Url::parse("http://127.0.0.1:1").unwrap()).is_ok());
        assert!(nettest_url(&url::Url::parse("https://example.test").unwrap()).is_ok());
        assert_eq!(
            nettest_url(&url::Url::parse("https://example.test").unwrap())
                .unwrap()
                .as_str(),
            "https://example.test/v2/nettestsession"
        );
    }
}

#[cfg(test)]
mod canonical_tests {
    use super::*;

    const KEY: [u8; 32] = [0x77; 32];

    #[test]
    fn known_fields_encode_ascending_and_unknown_fields_follow() {
        let mut message = NetworkTestMessage::default();
        message.set_message_type(7);
        message.set_payload_size(1_280);
        message.push_unknown(19, WireValue::Varint(3));
        message.push_unknown(4, WireValue::Bytes(b"b".to_vec()));
        message.push_unknown(4, WireValue::Varint(1));
        let encoded = message.encode();
        assert_eq!(
            decode_field_numbers(&encoded),
            vec![1, 13, 19, 4, 4],
            "known fields ascending, then unknown in received order"
        );
    }

    #[test]
    fn duplicate_scalar_fields_take_the_last_value() {
        let mut body = Vec::new();
        write_varint(&mut body, 0x08);
        write_varint(&mut body, 1);
        write_varint(&mut body, 0x08);
        write_varint(&mut body, 7);
        let decoded = NetworkTestMessage::decode(&body).unwrap();
        assert_eq!(decoded.message_type(), 7);
        let numbers = decode_field_numbers(&decoded.encode());
        assert_eq!(numbers, vec![1], "re-encoding collapses the duplicate");
    }

    #[test]
    fn unknown_fields_are_authenticated_and_survive_re_encoding() {
        let mut sender = NetworkTestMessage::mtu_probe(1_280, b"nt-1", 1);
        sender.push_unknown(4, WireValue::Varint(11));
        sender.seal(&KEY).unwrap();

        let received = NetworkTestMessage::decode(&sender.encode()).unwrap();
        assert!(received.verify(&KEY).unwrap());
        assert_eq!(received.unknown(), &[(4, WireValue::Varint(11))]);
        assert_eq!(received.encode(), sender.encode());
    }

    #[test]
    fn non_canonical_input_order_still_authenticates() {
        let mut sender = NetworkTestMessage::mtu_probe(1_280, b"nt-1", 1);
        sender.push_unknown(19, WireValue::Varint(5));
        sender.seal(&KEY).unwrap();
        let canonical = sender.encode();

        let mut reordered = Vec::new();
        write_varint(&mut reordered, 19 << 3);
        write_varint(&mut reordered, 5);
        reordered.extend_from_slice(&canonical[..canonical.len() - 3]);

        let received = NetworkTestMessage::decode(&reordered).unwrap();
        assert!(
            received.verify(&KEY).unwrap(),
            "official order-independent parse"
        );
        assert_eq!(received.encode(), canonical);
    }

    fn decode_field_numbers(body: &[u8]) -> Vec<u32> {
        let mut numbers = Vec::new();
        let mut offset = 0_usize;
        let end = body.len();
        while offset < end {
            let (tag, next) = read_varint(&body[offset..]).unwrap();
            offset += next;
            numbers.push((tag >> 3) as u32);
            offset += match tag & 7 {
                0 => read_varint(&body[offset..]).unwrap().1,
                1 => 8,
                2 => {
                    let (len, next) = read_varint(&body[offset..]).unwrap();
                    next + len as usize
                }
                5 => 4,
                other => panic!("unexpected wire type {other}"),
            };
        }
        numbers
    }
}

#[cfg(test)]
mod golden_tests {
    use super::*;

    const KEY: [u8; 32] = [0x33; 32];
    const PROBE_BODY: &[u8] = &[
        0x08, 0x07, 0x12, 0x04, b'n', b't', b'-', b'1', 0x60, 0x01, 0x68, 0x80, 0x0a,
    ];
    const PROBE_HMAC: &str = "e891c98a2b313e55ebb343cf4f5aa46db2054fb419e1c0c4f51c23f1ffdfe987";
    const PROBE_SEALED: &str = "080712046e742d315220e891c98a2b313e55ebb343cf4f5aa46db2054fb419e1c0c4f51c23f1ffdfe987600168800a";
    const UNKNOWN_HMAC: &str = "770f593149064511de33d7dc0aaeebd71510eabcd591cfd5fd3b64a159c9f84d";

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len() / 2)
            .map(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).unwrap())
            .collect()
    }

    fn expected_body() -> Vec<u8> {
        PROBE_BODY.to_vec()
    }

    #[test]
    fn golden_probe_matches_an_independent_protobuf_encoder() {
        let message = NetworkTestMessage::mtu_probe(1_280, b"nt-1", 1);
        assert_eq!(message.encode(), expected_body());
        assert_eq!(message.authenticated(&KEY).unwrap(), hex(PROBE_HMAC));
    }

    #[test]
    fn golden_sealed_bytes_match_the_independent_encoder() {
        let mut message = NetworkTestMessage::mtu_probe(1_280, b"nt-1", 1);
        message.seal(&KEY).unwrap();
        assert_eq!(message.encode(), hex(PROBE_SEALED));
        assert!(message.verify(&KEY).unwrap());
        assert_eq!(
            NetworkTestMessage::decode(&message.encode()).unwrap(),
            message,
            "bare request body round-trips"
        );
    }

    #[test]
    fn golden_unknown_duplicates_authenticate_like_the_independent_encoder() {
        let body = hex("080768800a200b200c");
        assert_eq!(body.len(), 9);
        let received = NetworkTestMessage::decode(&body).unwrap();
        assert_eq!(received.unknown().len(), 2, "both duplicates preserved");
        assert_eq!(
            received.unknown(),
            &[(4, WireValue::Varint(11)), (4, WireValue::Varint(12))]
        );
        assert_eq!(received.encode(), body, "unknown re-encodes in order");
        assert_eq!(received.authenticated(&KEY).unwrap(), hex(UNKNOWN_HMAC));
        assert!(!received.verify(&KEY).unwrap(), "no mac field present");
    }

    #[test]
    fn golden_duplicate_known_field_takes_the_last_value() {
        let body = hex("080768800a68800b");
        let decoded = NetworkTestMessage::decode(&body).unwrap();
        assert_eq!(decoded.payload_size(), Some(1_408));
        assert_eq!(
            decoded.encode(),
            hex("080768800b"),
            "re-encoding collapses the duplicate"
        );
    }

    #[test]
    fn golden_unknown_before_known_moves_after_known() {
        let body = hex("200b080768800a");
        let decoded = NetworkTestMessage::decode(&body).unwrap();
        assert_eq!(decoded.encode(), expected_body_with_unknown());
    }

    fn expected_body_with_unknown() -> Vec<u8> {
        hex("080768800a200b")
    }
}

#[cfg(test)]
mod wire_semantics_tests {
    use super::*;

    #[test]
    fn wrong_wire_type_on_a_known_field_is_preserved_as_unknown() {
        let body = hex_bytes("6a02aabb");
        let decoded = NetworkTestMessage::decode(&body).unwrap();
        assert_eq!(decoded.payload_size(), None);
        assert_eq!(
            decoded.unknown(),
            &[(13, WireValue::Bytes(vec![0xAA, 0xBB]))]
        );
        assert_eq!(decoded.encode(), body);
    }

    #[test]
    fn wrong_wire_type_on_the_type_field_is_preserved_as_unknown() {
        let body = hex_bytes("0a020700");
        let decoded = NetworkTestMessage::decode(&body).unwrap();
        assert_eq!(decoded.message_type(), 0);
        assert_eq!(decoded.encode(), body);
    }

    fn hex_bytes(text: &str) -> Vec<u8> {
        (0..text.len() / 2)
            .map(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).unwrap())
            .collect()
    }
}
