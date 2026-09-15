use std::fmt::Write;
use std::net::{IpAddr, SocketAddr};

use opennow_streamer_transport::nvst::MAX_NVST_VIDEO_PEER_PORTS;

pub(super) fn summarize(transport: &str) -> String {
    let bytes = transport.as_bytes();
    let prefix = String::from_utf8_lossy(&bytes[..bytes.len().min(2048)]);
    let lower = prefix.to_ascii_lowercase();
    let mut summary = format!(
        "bytes={} input_truncated={} source_marker={} xgs_port_marker={} standard_port_marker={}",
        bytes.len(),
        bytes.len() > 2048,
        lower.contains("source="),
        lower.contains("x-gs-serverport="),
        lower.contains("server_port="),
    );
    let mut fields = prefix.split([';', ',']);
    for (index, field) in fields.by_ref().take(16).enumerate() {
        let Some((name, value)) = field.trim().split_once('=') else {
            let kind = match field.trim().to_ascii_lowercase().as_str() {
                "unicast" => "unicast",
                "multicast" => "multicast",
                "rtp/avp" | "rtp/avp/udp" => "rtp-avp",
                "rtp/savp" | "rtp/savp/udp" => "rtp-savp",
                "" => "empty",
                _ => "other-redacted",
            };
            let _ = write!(summary, " field{index}={kind}");
            continue;
        };
        let (name_kind, value_kind) = match name.trim().to_ascii_lowercase().as_str() {
            "source" => ("source", source_kind(value.trim())),
            "x-gs-serverport" => ("xgs-port", port_kind(value.trim())),
            "server_port" => ("standard-port", port_kind(value.trim())),
            _ => ("other", "redacted"),
        };
        let _ = write!(
            summary,
            " field{index}={name_kind}:{value_kind}:key-spacing-{}",
            name != name.trim(),
        );
    }
    let _ = write!(summary, " fields_truncated={}", fields.next().is_some());
    summary
}

fn source_kind(value: &str) -> &'static str {
    if value.is_empty() {
        return "empty";
    }
    if let Ok(ip) = value.parse::<IpAddr>() {
        return if ip.is_ipv4() { "ipv4" } else { "ipv6" };
    }
    if value.starts_with('"') && value.ends_with('"') {
        return "quoted";
    }
    if value
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .is_some_and(|v| v.parse::<IpAddr>().is_ok())
    {
        return "bracketed-ip";
    }
    if value.parse::<SocketAddr>().is_ok() {
        return "ip-with-port";
    }
    if value
        .split_whitespace()
        .next()
        .is_some_and(|v| v.parse::<IpAddr>().is_ok())
    {
        return "ip-with-trailing-data";
    }
    "non-ip"
}

fn port_kind(value: &str) -> &'static str {
    if value.is_empty() {
        return "empty";
    }
    if value.starts_with('"') && value.ends_with('"') {
        return "quoted";
    }
    if value.chars().any(char::is_whitespace) {
        return "internal-whitespace";
    }
    let (first, last) = value.split_once('-').unwrap_or((value, value));
    let Ok(first) = first.parse::<u64>() else {
        return "invalid-start";
    };
    if first == 0 || first > u64::from(u16::MAX) {
        return "start-out-of-range";
    }
    if !value.contains('-') {
        return "single-port";
    }
    if last.parse::<u64>().ok().is_some_and(|last| {
        last <= u64::from(u16::MAX)
            && last >= first
            && last - first < u64::from(MAX_NVST_VIDEO_PEER_PORTS)
    }) {
        "valid-range"
    } else {
        "range-falls-back-to-start"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_diagnostics_identify_expected_fields_without_their_values() {
        let summary = summarize("unicast;source=192.0.2.10;X-GS-ServerPort=5004-5005");
        assert!(
            summary.contains("source_marker=true xgs_port_marker=true standard_port_marker=false")
        );
        assert!(summary.contains("field0=unicast field1=source:ipv4:key-spacing-false field2=xgs-port:valid-range:key-spacing-false"));
        assert!(!summary.contains("192.0.2.10"));
        assert!(!summary.contains("5004"));
    }

    #[test]
    fn transport_diagnostics_separate_source_and_port_incompatibilities() {
        for (value, expected) in [
            ("source=", "source:empty"),
            ("source=seat.example.test", "source:non-ip"),
            ("source=\"192.0.2.10\"", "source:quoted"),
            ("source=[2001:db8::1]", "source:bracketed-ip"),
            ("source=192.0.2.10:5004", "source:ip-with-port"),
            ("source=192.0.2.10 extra", "source:ip-with-trailing-data"),
            ("source =192.0.2.10", "source:ipv4:key-spacing-true"),
            ("server_port=5004", "standard-port:single-port"),
            ("X-GS-ServerPort=0", "xgs-port:start-out-of-range"),
            ("X-GS-ServerPort=65536", "xgs-port:start-out-of-range"),
            ("X-GS-ServerPort=not-a-port", "xgs-port:invalid-start"),
            (
                "X-GS-ServerPort=5004 - 5005",
                "xgs-port:internal-whitespace",
            ),
            ("X-GS-ServerPort=\"5004\"", "xgs-port:quoted"),
            (
                "X-GS-ServerPort=5004-65535",
                "xgs-port:range-falls-back-to-start",
            ),
        ] {
            assert!(summarize(value).contains(expected), "{expected}");
        }
    }

    #[test]
    fn transport_diagnostics_never_echo_unknown_fields_or_sensitive_values() {
        let summary = summarize(
            "secret-flag;secret-key=secret-value;source=secret-host;X-GS-ServerPort=secret-port;token=Bearer-secret\r\nAuthorization: secret-auth",
        );
        assert!(!summary.contains("secret"));
        assert!(!summary.contains("Bearer"));
        assert!(!summary.contains("Authorization"));
        assert!(!summary.contains('\r'));
        assert!(!summary.contains('\n'));
    }

    #[test]
    fn transport_diagnostics_bound_input_fields_and_unicode_output() {
        let summary = summarize(&"source=secret;".repeat(1000));
        assert!(summary.contains("input_truncated=true"));
        assert!(summary.contains("fields_truncated=true"));
        assert!(!summary.contains("field16="));
        assert!(summary.len() < 2048);
        let unicode = summarize(&format!("{}🦀", "a".repeat(2047)));
        assert!(unicode.contains("input_truncated=true"));
        assert!(!unicode.contains('🦀'));
        assert!(unicode.len() < 2048);
    }
}
