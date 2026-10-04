use std::collections::HashSet;
use std::net::IpAddr;

use serde::Serialize;
use serde_json::{Value, json};
use str0m::Candidate;

#[derive(Default, Serialize)]
pub(super) struct PacketCounts {
    stun: u64,
    dtls: u64,
    rtp: u64,
    other: u64,
}

impl PacketCounts {
    pub(super) fn record(&mut self, bytes: &[u8]) {
        let count = match bytes.first().copied() {
            Some(0..=3) if bytes.get(4..8) == Some(&[0x21, 0x12, 0xa4, 0x42]) => &mut self.stun,
            Some(20..=63) => &mut self.dtls,
            Some(128..=191) => &mut self.rtp,
            _ => &mut self.other,
        };
        *count = count.saturating_add(1);
    }
}

#[derive(Default, Serialize)]
pub(super) struct Traffic {
    pub(super) transmitted: PacketCounts,
    pub(super) received: PacketCounts,
    pub(super) routed: PacketCounts,
    pub(super) send_errors: u64,
    pub(super) tx_to_known_peer: u64,
    pub(super) rx_from_known_peer: u64,
    pub(super) last_tx_port: Option<u16>,
    pub(super) last_rx_port: Option<u16>,
}

pub(super) fn address_class(ip: IpAddr) -> &'static str {
    if ip.is_unspecified() {
        "unspecified"
    } else if ip.is_loopback() {
        "loopback"
    } else if ip.is_multicast() {
        "multicast"
    } else {
        match ip {
            IpAddr::V4(ip) if ip.is_private() => "private",
            IpAddr::V4(ip) if ip.is_link_local() => "link-local",
            IpAddr::V6(ip) if ip.is_unique_local() => "private",
            IpAddr::V6(ip) if ip.is_unicast_link_local() => "link-local",
            _ => "public",
        }
    }
}

pub(super) fn candidate_summary(raw: &str, parsed: Option<&Candidate>) -> Value {
    let parts: Vec<_> = raw.split_ascii_whitespace().take(8).collect();
    let advertised = parts.get(4).and_then(|value| value.parse::<IpAddr>().ok());
    let protocol = match parts
        .get(2)
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("udp") => "udp",
        Some("tcp") => "tcp",
        _ => "other",
    };
    let kind = match parts.get(7).copied() {
        Some("host") => "host",
        Some("srflx") => "srflx",
        Some("prflx") => "prflx",
        Some("relay") => "relay",
        _ => "other",
    };
    json!({"protocol":protocol,"kind":kind,
        "advertisedClass":advertised.map(address_class).unwrap_or("hostname"),
        "advertisedPort":parts.get(5).and_then(|value| value.parse::<u16>().ok()),
        "accepted":parsed.is_some(),
        "selectedClass":parsed.map(|candidate| address_class(candidate.addr().ip())),
        "selectedPort":parsed.map(|candidate| candidate.addr().port())})
}

pub(super) fn sdp_summary(sdp: &str) -> Value {
    let values = |prefix: &str| {
        sdp.lines()
            .filter_map(|line| line.strip_prefix(prefix))
            .take(16)
            .collect::<Vec<_>>()
    };
    let ufrags = values("a=ice-ufrag:");
    let passwords = values("a=ice-pwd:");
    let roles = values("a=setup:")
        .into_iter()
        .map(|role| match role {
            "active" => "active",
            "passive" => "passive",
            "actpass" => "actpass",
            _ => "other",
        })
        .collect::<Vec<_>>();
    json!({"iceLite":sdp.lines().any(|line| line == "a=ice-lite"),
        "bundleSize":sdp.lines().find_map(|line| line.strip_prefix("a=group:BUNDLE ")).map(|mids| mids.split_whitespace().count()),
        "candidateCount":sdp.lines().filter(|line| line.starts_with("a=candidate:")).count(),
        "ufragLengths":ufrags.iter().map(|value| value.len()).collect::<Vec<_>>(),
        "passwordLengths":passwords.iter().map(|value| value.len()).collect::<Vec<_>>(),
        "ufragVariants":ufrags.iter().collect::<HashSet<_>>().len(),
        "passwordVariants":passwords.iter().collect::<HashSet<_>>().len(),
        "setupRoles":roles})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_summaries_exclude_credentials_addresses_and_payloads() {
        let sdp = "a=ice-ufrag:private-user\na=ice-pwd:private-password\na=setup:private-role\na=group:BUNDLE private-mid\na=candidate:private-foundation 1 udp 123 192.0.2.42 5555 typ host\n";
        let summary = sdp_summary(sdp);
        assert_eq!(summary["candidateCount"], 1);
        assert_eq!(summary["bundleSize"], 1);
        assert!(!summary.to_string().contains("private"));
        let summary = candidate_summary(
            "candidate:private-foundation 1 udp 123 private-host 5555 typ host ufrag private-user",
            None,
        );
        assert_eq!(summary["advertisedClass"], "hostname");
        assert_eq!(summary["advertisedPort"], 5555);
        assert!(!summary.to_string().contains("private"));
        let mut packets = PacketCounts::default();
        packets.record(&[0, 1, 0, 0, 0x21, 0x12, 0xa4, 0x42]);
        packets.record(&[22, 0, 0]);
        packets.record(&[128, 0, 0]);
        packets.record(b"private-payload");
        assert_eq!(
            serde_json::to_value(packets).unwrap(),
            json!({"stun":1,"dtls":1,"rtp":1,"other":1})
        );
    }
}
