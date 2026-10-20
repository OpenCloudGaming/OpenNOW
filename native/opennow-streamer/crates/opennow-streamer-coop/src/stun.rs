//! Minimal STUN (RFC 8489) message codec for server-reflexive discovery and TURN.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use hmac::{Hmac, Mac};
use md5::{Digest, Md5};
use sha1::Sha1;

pub const MAGIC_COOKIE: u32 = 0x2112_a442;
pub const HEADER_LEN: usize = 20;
pub const MAX_MESSAGE_LEN: usize = 4096;

pub const BINDING: u16 = 0x0001;
pub const ALLOCATE: u16 = 0x0003;
pub const REFRESH: u16 = 0x0004;
pub const SEND: u16 = 0x0006;
pub const DATA: u16 = 0x0007;
pub const CREATE_PERMISSION: u16 = 0x0008;
pub const CHANNEL_BIND: u16 = 0x0009;

pub const CLASS_REQUEST: u16 = 0x0000;
pub const CLASS_INDICATION: u16 = 0x0010;
pub const CLASS_SUCCESS: u16 = 0x0100;
pub const CLASS_ERROR: u16 = 0x0110;

pub const ATTR_MAPPED_ADDRESS: u16 = 0x0001;
pub const ATTR_USERNAME: u16 = 0x0006;
pub const ATTR_MESSAGE_INTEGRITY: u16 = 0x0008;
pub const ATTR_ERROR_CODE: u16 = 0x0009;
pub const ATTR_CHANNEL_NUMBER: u16 = 0x000c;
pub const ATTR_LIFETIME: u16 = 0x000d;
pub const ATTR_XOR_PEER_ADDRESS: u16 = 0x0012;
pub const ATTR_DATA: u16 = 0x0013;
pub const ATTR_REALM: u16 = 0x0014;
pub const ATTR_NONCE: u16 = 0x0015;
pub const ATTR_XOR_RELAYED_ADDRESS: u16 = 0x0016;
pub const ATTR_REQUESTED_TRANSPORT: u16 = 0x0019;
pub const ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;
pub const ATTR_SOFTWARE: u16 = 0x8022;
pub const ATTR_FINGERPRINT: u16 = 0x8028;

pub type TransactionId = [u8; 12];

pub fn new_transaction_id() -> TransactionId {
    let mut id = [0_u8; 12];
    if getrandom::fill(&mut id).is_err() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default();
        id.copy_from_slice(&nanos.to_le_bytes()[..12]);
    }
    id
}

pub fn is_stun(bytes: &[u8]) -> bool {
    bytes.len() >= HEADER_LEN
        && bytes[0] & 0xc0 == 0
        && u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) == MAGIC_COOKIE
}

/// TURN ChannelData uses channel numbers 0x4000..=0x4fff; the first two bits are 01.
pub fn is_channel_data(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && (0x40..=0x4f).contains(&bytes[0])
}

/// Long-term credential key: MD5(username ":" realm ":" password).
pub fn long_term_key(username: &str, realm: &str, password: &str) -> [u8; 16] {
    let mut hasher = Md5::new();
    hasher.update(username.as_bytes());
    hasher.update(b":");
    hasher.update(realm.as_bytes());
    hasher.update(b":");
    hasher.update(password.as_bytes());
    hasher.finalize().into()
}

pub struct MessageBuilder {
    bytes: Vec<u8>,
}

impl MessageBuilder {
    pub fn new(method: u16, class: u16, transaction: &TransactionId) -> Self {
        let mut bytes = Vec::with_capacity(128);
        bytes.extend_from_slice(&encode_type(method, class).to_be_bytes());
        bytes.extend_from_slice(&0_u16.to_be_bytes());
        bytes.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        bytes.extend_from_slice(transaction);
        Self { bytes }
    }

    pub fn attribute(mut self, kind: u16, value: &[u8]) -> Self {
        self.bytes.extend_from_slice(&kind.to_be_bytes());
        self.bytes
            .extend_from_slice(&(value.len() as u16).to_be_bytes());
        self.bytes.extend_from_slice(value);
        while self.bytes.len() % 4 != 0 {
            self.bytes.push(0);
        }
        self.set_length();
        self
    }

    pub fn xor_address(self, kind: u16, addr: SocketAddr) -> Self {
        let transaction: TransactionId = self.bytes[8..20].try_into().expect("header");
        let value = encode_xor_address(addr, &transaction);
        self.attribute(kind, &value)
    }

    fn set_length(&mut self) {
        let length = (self.bytes.len() - HEADER_LEN) as u16;
        self.bytes[2..4].copy_from_slice(&length.to_be_bytes());
    }

    /// Appends MESSAGE-INTEGRITY computed over the message with its final length.
    pub fn integrity(mut self, key: &[u8]) -> Self {
        let length = (self.bytes.len() - HEADER_LEN + 24) as u16;
        self.bytes[2..4].copy_from_slice(&length.to_be_bytes());
        let mut mac = Hmac::<Sha1>::new_from_slice(key).expect("HMAC accepts any key");
        mac.update(&self.bytes);
        let digest = mac.finalize().into_bytes();
        self.attribute(ATTR_MESSAGE_INTEGRITY, &digest)
    }

    pub fn fingerprint(mut self) -> Self {
        let length = (self.bytes.len() - HEADER_LEN + 8) as u16;
        self.bytes[2..4].copy_from_slice(&length.to_be_bytes());
        let crc = crc32fast::hash(&self.bytes) ^ 0x5354_554e;
        self.attribute(ATTR_FINGERPRINT, &crc.to_be_bytes())
    }

    pub fn build(self) -> Vec<u8> {
        self.bytes
    }
}

const fn encode_type(method: u16, class: u16) -> u16 {
    (method & 0x000f) | ((method & 0x0070) << 1) | ((method & 0x0f80) << 2) | class
}

fn decode_type(value: u16) -> (u16, u16) {
    let class = value & 0x0110;
    let method = (value & 0x000f) | ((value >> 1) & 0x0070) | ((value >> 2) & 0x0f80);
    (method, class)
}

fn encode_xor_address(addr: SocketAddr, transaction: &TransactionId) -> Vec<u8> {
    let port = addr.port() ^ (MAGIC_COOKIE >> 16) as u16;
    let mut value = vec![0, 0];
    value.extend_from_slice(&port.to_be_bytes());
    match addr.ip() {
        IpAddr::V4(ip) => {
            value[1] = 0x01;
            let xored = u32::from(ip) ^ MAGIC_COOKIE;
            value.extend_from_slice(&xored.to_be_bytes());
        }
        IpAddr::V6(ip) => {
            value[1] = 0x02;
            let mut mask = [0_u8; 16];
            mask[..4].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
            mask[4..].copy_from_slice(transaction);
            for (byte, key) in ip.octets().iter().zip(mask) {
                value.push(byte ^ key);
            }
        }
    }
    value
}

#[derive(Debug, Clone)]
pub struct Message<'a> {
    pub method: u16,
    pub class: u16,
    pub transaction: TransactionId,
    raw: &'a [u8],
    attributes: Vec<(u16, usize, usize)>,
}

impl<'a> Message<'a> {
    pub fn parse(bytes: &'a [u8]) -> Option<Self> {
        if !is_stun(bytes) {
            return None;
        }
        let length = usize::from(u16::from_be_bytes([bytes[2], bytes[3]]));
        if length % 4 != 0
            || HEADER_LEN + length > bytes.len()
            || HEADER_LEN + length > MAX_MESSAGE_LEN
        {
            return None;
        }
        let raw = &bytes[..HEADER_LEN + length];
        let (method, class) = decode_type(u16::from_be_bytes([raw[0], raw[1]]));
        let mut attributes = Vec::new();
        let mut cursor = HEADER_LEN;
        while cursor + 4 <= raw.len() {
            let kind = u16::from_be_bytes([raw[cursor], raw[cursor + 1]]);
            let len = usize::from(u16::from_be_bytes([raw[cursor + 2], raw[cursor + 3]]));
            let start = cursor + 4;
            if start + len > raw.len() || attributes.len() == 32 {
                return None;
            }
            attributes.push((kind, start, len));
            cursor = start + len.div_ceil(4) * 4;
        }
        Some(Self {
            method,
            class,
            transaction: raw[8..20].try_into().ok()?,
            raw,
            attributes,
        })
    }

    pub fn len(&self) -> usize {
        self.raw.len()
    }

    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    pub fn attribute(&self, kind: u16) -> Option<&'a [u8]> {
        self.attributes
            .iter()
            .find(|(candidate, _, _)| *candidate == kind)
            .map(|(_, start, len)| &self.raw[*start..*start + *len])
    }

    pub fn xor_address(&self, kind: u16) -> Option<SocketAddr> {
        let value = self.attribute(kind)?;
        if value.len() < 8 {
            return None;
        }
        let port = u16::from_be_bytes([value[2], value[3]]) ^ (MAGIC_COOKIE >> 16) as u16;
        let ip = match value[1] {
            0x01 => IpAddr::V4(Ipv4Addr::from(
                u32::from_be_bytes(value[4..8].try_into().ok()?) ^ MAGIC_COOKIE,
            )),
            0x02 if value.len() >= 20 => {
                let mut mask = [0_u8; 16];
                mask[..4].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
                mask[4..].copy_from_slice(&self.transaction);
                let mut octets = [0_u8; 16];
                for (index, octet) in octets.iter_mut().enumerate() {
                    *octet = value[4 + index] ^ mask[index];
                }
                IpAddr::V6(Ipv6Addr::from(octets))
            }
            _ => return None,
        };
        Some(SocketAddr::new(ip, port))
    }

    pub fn mapped_address(&self) -> Option<SocketAddr> {
        if let Some(addr) = self.xor_address(ATTR_XOR_MAPPED_ADDRESS) {
            return Some(addr);
        }
        let value = self.attribute(ATTR_MAPPED_ADDRESS)?;
        if value.len() < 8 || value[1] != 0x01 {
            return None;
        }
        Some(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(value[4], value[5], value[6], value[7])),
            u16::from_be_bytes([value[2], value[3]]),
        ))
    }

    pub fn error_code(&self) -> Option<u16> {
        let value = self.attribute(ATTR_ERROR_CODE)?;
        (value.len() >= 4).then(|| u16::from(value[2] & 0x07) * 100 + u16::from(value[3]))
    }

    pub fn text(&self, kind: u16) -> Option<String> {
        std::str::from_utf8(self.attribute(kind)?)
            .ok()
            .map(str::to_owned)
    }

    pub fn u32_attribute(&self, kind: u16) -> Option<u32> {
        let value = self.attribute(kind)?;
        Some(u32::from_be_bytes(value.get(..4)?.try_into().ok()?))
    }

    /// Verifies MESSAGE-INTEGRITY with the long-term key.
    pub fn verify_integrity(&self, key: &[u8]) -> bool {
        let Some((_, start, len)) = self
            .attributes
            .iter()
            .find(|(kind, _, _)| *kind == ATTR_MESSAGE_INTEGRITY)
            .copied()
        else {
            return false;
        };
        if len != 20 {
            return false;
        }
        let attribute_start = start - 4;
        let mut covered = self.raw[..attribute_start].to_vec();
        let length = (attribute_start + 24 - HEADER_LEN) as u16;
        covered[2..4].copy_from_slice(&length.to_be_bytes());
        let mut mac = Hmac::<Sha1>::new_from_slice(key).expect("HMAC accepts any key");
        mac.update(&covered);
        mac.verify_slice(&self.raw[start..start + 20]).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_and_class_bits_interleave_per_rfc() {
        assert_eq!(encode_type(BINDING, CLASS_REQUEST), 0x0001);
        assert_eq!(encode_type(BINDING, CLASS_SUCCESS), 0x0101);
        assert_eq!(encode_type(ALLOCATE, CLASS_ERROR), 0x0113);
        assert_eq!(encode_type(DATA, CLASS_INDICATION), 0x0017);
        assert_eq!(decode_type(0x0113), (ALLOCATE, CLASS_ERROR));
        assert_eq!(decode_type(0x0109), (CHANNEL_BIND, CLASS_SUCCESS));
    }

    #[test]
    fn long_term_key_is_md5_of_user_realm_password() {
        assert_eq!(
            long_term_key("alice", "opennow.test", "secret"),
            [
                0x35, 0x32, 0xf2, 0x4d, 0x27, 0x4f, 0x22, 0xf8, 0x18, 0x81, 0x64, 0x75, 0xbe, 0x7c,
                0x78, 0x91
            ]
        );
    }

    #[test]
    fn builder_round_trips_addresses_and_integrity() {
        let transaction = [7_u8; 12];
        let peer: SocketAddr = "192.0.2.1:32853".parse().unwrap();
        let peer6: SocketAddr = "[2001:db8::1]:3478".parse().unwrap();
        let key = long_term_key("user", "realm", "pass");
        let bytes = MessageBuilder::new(CREATE_PERMISSION, CLASS_REQUEST, &transaction)
            .xor_address(ATTR_XOR_PEER_ADDRESS, peer)
            .xor_address(ATTR_XOR_RELAYED_ADDRESS, peer6)
            .attribute(ATTR_USERNAME, b"user")
            .integrity(&key)
            .fingerprint()
            .build();
        let message = Message::parse(&bytes).unwrap();
        assert_eq!(
            (message.method, message.class),
            (CREATE_PERMISSION, CLASS_REQUEST)
        );
        assert_eq!(message.xor_address(ATTR_XOR_PEER_ADDRESS), Some(peer));
        assert_eq!(message.xor_address(ATTR_XOR_RELAYED_ADDRESS), Some(peer6));
        assert!(message.verify_integrity(&key));
        assert!(!message.verify_integrity(&long_term_key("user", "realm", "wrong")));
        assert_eq!(message.len(), bytes.len());
        assert!(Message::parse(&bytes[..bytes.len() - 4]).is_none());
    }
}
