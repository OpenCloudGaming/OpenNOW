//! SDP Manipulation
//!
//! Parse and modify SDP for codec preferences and ICE fixes.

use crate::app::VideoCodec;
use log::{debug, info, warn};
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
                    debug!(
                        "Found codec {} (normalized: {}) with payload type {}",
                        raw_codec, normalized_codec, pt
                    );
                    codec_payloads.entry(normalized_codec).or_default().push(pt);
                }
            }
        }
    }

    info!(
        "Available video codecs in SDP: {:?}",
        codec_payloads.keys().collect::<Vec<_>>()
    );

    // Get preferred codec payload types
    let preferred = codec_payloads.get(codec_name).cloned().unwrap_or_default();
    if preferred.is_empty() {
        info!(
            "Codec {} not found in SDP - keeping original SDP unchanged",
            codec_name
        );
        return sdp.to_string();
    }

    info!(
        "Found {} payload type(s) for {}: {:?}",
        preferred.len(),
        codec_name,
        preferred
    );

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
                    warn!(
                        "No matching payload types for {} in m=video line, keeping original",
                        codec_name
                    );
                    result.push(line.to_string());
                    continue;
                }
            }
        } else if line.starts_with("m=") && in_video {
            in_video = false;
        }

        if in_video {
            // Filter rtpmap, fmtp, rtcp-fb lines - only keep lines for preferred codec
            if let Some(rest) = line
                .strip_prefix("a=rtpmap:")
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
    info!(
        "SDP filtered: {} -> {} bytes",
        sdp.len(),
        filtered_sdp.len()
    );
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
    info!(
        "DTLS setup fix: replaced {} passive entries, now have {} active entries",
        passive_count, active_count
    );

    fixed
}

/// Inject additional SSRCs into the video section of the offer SDP
///
/// GFN server uses sequential SSRCs (1, 2, 3, 4...) for video streams when
/// resolution changes occur. However, webrtc-rs requires SSRCs to be declared
/// in the SDP or have MID header extensions (which GFN doesn't send).
///
/// This function injects `a=ssrc:N` lines for SSRCs 2, 3, 4 into the video
/// section, similar to how the official GFN client (Bifrost2.dll) does it.
/// This allows webrtc-rs to accept packets from these SSRCs when the server
/// switches resolution.
///
/// Based on reverse engineering of official GFN client:
/// - Bifrost2.dll contains: "a=ssrc:2 cname:odrerir", "a=ssrc:3 cname:odrerir"
/// - Uses "provisional stream" concept to handle SSRC changes
pub fn inject_provisional_ssrcs(sdp: &str) -> String {
    let line_ending = if sdp.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<&str> = sdp.lines().collect();
    let mut result: Vec<String> = Vec::new();

    let mut in_video = false;
    let mut video_msid: Option<(String, String)> = None; // (stream_id, track_id)
    let mut existing_ssrcs: Vec<u32> = Vec::new();
    let mut injected = false;

    // First pass: find existing video SSRCs and msid
    for line in &lines {
        if line.starts_with("m=video") {
            in_video = true;
        } else if line.starts_with("m=") && in_video {
            in_video = false;
        }

        if in_video {
            // Parse a=ssrc:N ...
            if let Some(rest) = line.strip_prefix("a=ssrc:") {
                if let Some(ssrc_str) = rest.split_whitespace().next() {
                    if let Ok(ssrc) = ssrc_str.parse::<u32>() {
                        existing_ssrcs.push(ssrc);
                    }
                }
            }
            // Parse a=msid:stream_id track_id
            if let Some(rest) = line.strip_prefix("a=msid:") {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if parts.len() >= 2 {
                    video_msid = Some((parts[0].to_string(), parts[1].to_string()));
                }
            }
        }
    }

    // Determine which SSRCs to inject (2, 3, 4 that don't already exist)
    let ssrcs_to_inject: Vec<u32> = (2..=4)
        .filter(|ssrc| !existing_ssrcs.contains(ssrc))
        .collect();

    if ssrcs_to_inject.is_empty() {
        debug!("No provisional SSRCs needed - all already declared");
        return sdp.to_string();
    }

    info!(
        "Injecting provisional SSRCs {:?} for video (existing: {:?})",
        ssrcs_to_inject, existing_ssrcs
    );

    // Second pass: find injection point
    // - If there are existing a=ssrc lines, inject after the last one
    // - If no a=ssrc lines exist, inject before the next m= line (end of video section)
    in_video = false;
    let mut last_ssrc_line_idx: Option<usize> = None;
    let mut video_section_end_idx: Option<usize> = None;
    let mut video_section_start_idx: Option<usize> = None;

    for (idx, line) in lines.iter().enumerate() {
        if line.starts_with("m=video") {
            in_video = true;
            video_section_start_idx = Some(idx);
        } else if line.starts_with("m=") && in_video {
            // Found start of next section - this is where video section ends
            video_section_end_idx = Some(idx);
            in_video = false;
        }

        if in_video && line.starts_with("a=ssrc:") {
            last_ssrc_line_idx = Some(idx);
        }
    }

    // If we're still in video section at end of file, end is after last line
    if in_video && video_section_end_idx.is_none() {
        video_section_end_idx = Some(lines.len());
    }

    // Determine injection point:
    // - After last a=ssrc line if exists
    // - Otherwise, before the next m= section (or end of file)
    // - If still no valid point, inject after m=video line
    let _injection_after_idx = last_ssrc_line_idx
        .or_else(|| video_section_end_idx.map(|idx| idx.saturating_sub(1)))
        .or(video_section_start_idx);

    // Third pass: build result with injected SSRCs
    in_video = false;
    for (idx, line) in lines.iter().enumerate() {
        // If we need to inject BEFORE this line (when inserting at section end)
        if !injected && video_section_end_idx == Some(idx) && last_ssrc_line_idx.is_none() {
            // Inject at end of video section (before next m= line)
            let (stream_id, track_id) = video_msid
                .clone()
                .unwrap_or_else(|| ("odrerir".to_string(), "video".to_string()));

            for ssrc in &ssrcs_to_inject {
                result.push(format!("a=ssrc:{} msid:{} {}", ssrc, stream_id, track_id));
                result.push(format!("a=ssrc:{} cname:odrerir", ssrc));
            }

            injected = true;
            info!(
                "Injected {} provisional SSRCs at end of video section",
                ssrcs_to_inject.len()
            );
        }

        result.push(line.to_string());

        if line.starts_with("m=video") {
            in_video = true;
        } else if line.starts_with("m=") && in_video {
            in_video = false;
        }

        // Inject after the last a=ssrc line in video section
        if in_video && Some(idx) == last_ssrc_line_idx && !injected {
            // Use the same msid as existing video track, or generate one
            let (stream_id, track_id) = video_msid
                .clone()
                .unwrap_or_else(|| ("odrerir".to_string(), "video".to_string()));

            for ssrc in &ssrcs_to_inject {
                // Add ssrc with msid (required for webrtc-rs to create track)
                result.push(format!("a=ssrc:{} msid:{} {}", ssrc, stream_id, track_id));
                result.push(format!("a=ssrc:{} cname:odrerir", ssrc));
            }

            injected = true;
            info!(
                "Injected {} provisional SSRCs after existing SSRC declarations",
                ssrcs_to_inject.len()
            );
        }
    }

    // Handle edge case: video section at end of file with no ssrc lines
    if !injected && video_section_start_idx.is_some() {
        let (stream_id, track_id) = video_msid
            .unwrap_or_else(|| ("odrerir".to_string(), "video".to_string()));

        for ssrc in &ssrcs_to_inject {
            result.push(format!("a=ssrc:{} msid:{} {}", ssrc, stream_id, track_id));
            result.push(format!("a=ssrc:{} cname:odrerir", ssrc));
        }

        info!(
            "Injected {} provisional SSRCs at end of SDP (video section at EOF)",
            ssrcs_to_inject.len()
        );
    }

    result.join(line_ending)
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

    #[test]
    fn test_inject_provisional_ssrcs_with_existing() {
        // SDP with existing SSRC 1
        let sdp = "v=0\r\n\
            m=video 9 UDP/TLS/RTP/SAVPF 96\r\n\
            a=msid:stream1 video1\r\n\
            a=ssrc:1 msid:stream1 video1\r\n\
            a=ssrc:1 cname:test\r\n\
            m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n";

        let result = inject_provisional_ssrcs(sdp);

        // Should inject SSRCs 2, 3, 4
        assert!(result.contains("a=ssrc:2 msid:stream1 video1"));
        assert!(result.contains("a=ssrc:3 msid:stream1 video1"));
        assert!(result.contains("a=ssrc:4 msid:stream1 video1"));
        assert!(result.contains("a=ssrc:2 cname:odrerir"));
        assert!(result.contains("a=ssrc:3 cname:odrerir"));
        assert!(result.contains("a=ssrc:4 cname:odrerir"));

        // Original SSRC should still be there
        assert!(result.contains("a=ssrc:1 msid:stream1 video1"));
    }

    #[test]
    fn test_inject_provisional_ssrcs_without_existing() {
        // SDP without any SSRC lines
        let sdp = "v=0\r\n\
            m=video 9 UDP/TLS/RTP/SAVPF 96\r\n\
            a=msid:stream1 video1\r\n\
            a=rtpmap:96 H264/90000\r\n\
            m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n";

        let result = inject_provisional_ssrcs(sdp);

        // Should inject SSRCs 2, 3, 4 (no SSRC 1 since none existed)
        assert!(result.contains("a=ssrc:2 msid:stream1 video1"));
        assert!(result.contains("a=ssrc:3 msid:stream1 video1"));
        assert!(result.contains("a=ssrc:4 msid:stream1 video1"));

        // SSRCs should be injected before the audio section
        let video_pos = result.find("m=video").unwrap();
        let audio_pos = result.find("m=audio").unwrap();
        let ssrc2_pos = result.find("a=ssrc:2").unwrap();
        assert!(ssrc2_pos > video_pos && ssrc2_pos < audio_pos);
    }

    #[test]
    fn test_inject_provisional_ssrcs_already_declared() {
        // SDP with SSRCs 1, 2, 3, 4 already declared
        let sdp = "v=0\r\n\
            m=video 9 UDP/TLS/RTP/SAVPF 96\r\n\
            a=ssrc:1 cname:test\r\n\
            a=ssrc:2 cname:test\r\n\
            a=ssrc:3 cname:test\r\n\
            a=ssrc:4 cname:test\r\n\
            m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n";

        let result = inject_provisional_ssrcs(sdp);

        // Should not inject anything - all SSRCs already exist
        // Count occurrences of a=ssrc:2
        let count = result.matches("a=ssrc:2").count();
        assert_eq!(count, 1, "Should not duplicate existing SSRC 2");
    }
}
