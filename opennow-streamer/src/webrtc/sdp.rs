//! SDP Manipulation
//!
//! Parse and modify SDP for codec preferences and ICE fixes.

use crate::app::VideoCodec;
use log::{info, debug, warn};
use std::collections::HashMap;

/// Fix 0.0.0.0 in SDP with actual server IP
/// NOTE: Do NOT add ICE candidates to the offer SDP! The offer contains the
/// SERVER's candidates. Adding our own candidates here corrupts ICE negotiation.
/// Server candidates should come via trickle ICE through signaling.
pub fn fix_server_ip(sdp: &str, server_ip: &str) -> String {
    // Only fix the connection line, don't touch candidates
    let modified = sdp.replace("c=IN IP4 0.0.0.0", &format!("c=IN IP4 {}", server_ip));
    info!("Fixed connection IP to {}", server_ip);
    modified
}

/// Normalize codec name (HEVC -> H265)
fn normalize_codec_name(name: &str) -> String {
    let upper = name.to_uppercase();
    match upper.as_str() {
        "HEVC" => "H265".to_string(),
        _ => upper,
    }
}

/// Force a specific video codec in SDP
pub fn prefer_codec(sdp: &str, codec: &VideoCodec) -> String {
    let codec_name = match codec {
        VideoCodec::H264 => "H264",
        VideoCodec::H265 => "H265",
        VideoCodec::AV1 => "AV1",
    };

    info!("Forcing codec: {}", codec_name);

    // Detect line ending style
    let line_ending = if sdp.contains("\r\n") { "\r\n" } else { "\n" };

    // Use .lines() which handles both \r\n and \n correctly
    let lines: Vec<&str> = sdp.lines().collect();
    let mut result: Vec<String> = Vec::new();

    // First pass: collect codec -> payload type mapping
    // Normalize HEVC -> H265 for consistent lookup
    let mut codec_payloads: HashMap<String, Vec<String>> = HashMap::new();
    let mut in_video = false;

    for line in &lines {
        if line.starts_with("m=video") {
            in_video = true;
        } else if line.starts_with("m=") && in_video {
            in_video = false;
        }

        if in_video {
            // Parse a=rtpmap:96 H264/90000
            if let Some(rtpmap) = line.strip_prefix("a=rtpmap:") {
                let parts: Vec<&str> = rtpmap.split_whitespace().collect();
                if parts.len() >= 2 {
                    let pt = parts[0].to_string();
                    let raw_codec = parts[1].split('/').next().unwrap_or("");
                    let normalized_codec = normalize_codec_name(raw_codec);
                    debug!("Found codec {} (normalized: {}) with payload type {}", raw_codec, normalized_codec, pt);
                    codec_payloads.entry(normalized_codec).or_default().push(pt);
                }
            }
        }
    }

    info!("Available video codecs in SDP: {:?}", codec_payloads.keys().collect::<Vec<_>>());

    // Get preferred codec payload types
    let preferred = codec_payloads.get(codec_name).cloned().unwrap_or_default();
    if preferred.is_empty() {
        info!("Codec {} not found in SDP - keeping original SDP unchanged", codec_name);
        return sdp.to_string();
    }

    info!("Found {} payload type(s) for {}: {:?}", preferred.len(), codec_name, preferred);

    // Use HashSet<String> for easier comparison
    let preferred_set: std::collections::HashSet<String> = preferred.iter().cloned().collect();

    // Second pass: filter SDP
    in_video = false;
    for line in &lines {
        if line.starts_with("m=video") {
            in_video = true;

            // Rewrite m=video line to only include preferred payloads
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let header = parts[..3].join(" ");
                let payload_types: Vec<&str> = parts[3..]
                    .iter()
                    .filter(|pt| preferred_set.contains(&pt.to_string()))
                    .copied()
                    .collect();

                if !payload_types.is_empty() {
                    let new_line = format!("{} {}", header, payload_types.join(" "));
                    debug!("Rewritten m=video line: {}", new_line);
                    result.push(new_line);
                    continue;
                } else {
                    // No matching payload types - keep original m=video line
                    warn!("No matching payload types for {} in m=video line, keeping original", codec_name);
                    result.push(line.to_string());
                    continue;
                }
            }
        } else if line.starts_with("m=") && in_video {
            in_video = false;
        }

        if in_video {
            // Filter rtpmap, fmtp, rtcp-fb lines - only keep lines for preferred codec
            if let Some(rest) = line.strip_prefix("a=rtpmap:")
                .or_else(|| line.strip_prefix("a=fmtp:"))
                .or_else(|| line.strip_prefix("a=rtcp-fb:"))
            {
                let pt = rest.split_whitespace().next().unwrap_or("");
                if !preferred_set.contains(pt) {
                    debug!("Filtering out line for payload type {}: {}", pt, line);
                    continue; // Skip non-preferred codec attributes
                }
            }
        }

        result.push(line.to_string());
    }

    let filtered_sdp = result.join(line_ending);
    info!("SDP filtered: {} -> {} bytes", sdp.len(), filtered_sdp.len());
    filtered_sdp
}

/// Extract video codec from SDP
pub fn extract_video_codec(sdp: &str) -> Option<String> {
    let mut in_video = false;

    for line in sdp.lines() {
        if line.starts_with("m=video") {
            in_video = true;
        } else if line.starts_with("m=") && in_video {
            break;
        }

        if in_video && line.starts_with("a=rtpmap:") {
            // a=rtpmap:96 H264/90000
            if let Some(codec_part) = line.split_whitespace().nth(1) {
                return Some(codec_part.split('/').next()?.to_string());
            }
        }
    }

    None
}

/// Extract resolution from SDP
pub fn extract_resolution(sdp: &str) -> Option<(u32, u32)> {
    for line in sdp.lines() {
        // Look for a=imageattr or custom resolution attributes
        if line.starts_with("a=fmtp:") && line.contains("max-fs=") {
            // Parse max-fs for resolution
        }
    }
    None
}

/// Check if the offer SDP indicates an ice-lite server
pub fn is_ice_lite(sdp: &str) -> bool {
    for line in sdp.lines() {
        if line.trim() == "a=ice-lite" {
            return true;
        }
    }
    false
}

/// Fix DTLS setup for ice-lite servers
///
/// When the server is ice-lite and offers `a=setup:actpass`, we MUST respond
/// with `a=setup:active` (not passive). This makes us initiate the DTLS handshake.
///
/// If we respond with `a=setup:passive`, both sides wait for the other to start
/// DTLS, resulting in a handshake timeout.
pub fn fix_dtls_setup_for_ice_lite(answer_sdp: &str) -> String {
    info!("Fixing DTLS setup for ice-lite: changing passive -> active");

    // Replace all instances of a=setup:passive with a=setup:active
    let fixed = answer_sdp.replace("a=setup:passive", "a=setup:active");

    // Log for debugging
    let passive_count = answer_sdp.matches("a=setup:passive").count();
    let active_count = fixed.matches("a=setup:active").count();
    info!("DTLS setup fix: replaced {} passive entries, now have {} active entries",
          passive_count, active_count);

    fixed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fix_server_ip() {
        let sdp = "c=IN IP4 0.0.0.0\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\n";
        let fixed = fix_server_ip(sdp, "192.168.1.1");
        assert!(fixed.contains("c=IN IP4 192.168.1.1"));
        // Should NOT add candidates - that corrupts ICE negotiation
        assert!(!fixed.contains("a=candidate:"));
    }
}
