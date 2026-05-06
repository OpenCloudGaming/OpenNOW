#![allow(dead_code)]

use regex::Regex;
use std::collections::{HashMap, HashSet};

use crate::input::{PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL, PARTIALLY_RELIABLE_HID_DEVICE_MASK_ALL};
use crate::protocol::{ColorQuality, VideoCodec};

// Match the official web client's 240 FPS profile. Disabling split encode at
// this frame rate can leave H265 streams smeared because the server/client
// repair and frame-state assumptions no longer line up.
const ENABLE_OUT_OF_FOCUS_FPS_ADJUSTMENT: bool = false;
const ENABLE_240_FPS_SPLIT_ENCODE: bool = true;
const ENABLE_DYNAMIC_SPLIT_ENCODE_UPDATES: bool = true;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IceCredentials {
    pub ufrag: String,
    pub pwd: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone)]
pub struct NvstParams {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub max_bitrate_kbps: u32,
    pub partial_reliable_threshold_ms: u32,
    pub codec: VideoCodec,
    pub color_quality: ColorQuality,
    pub credentials: IceCredentials,
    pub hid_device_mask: Option<u32>,
    pub enable_partially_reliable_transfer_gamepad: Option<u32>,
    pub enable_partially_reliable_transfer_hid: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub struct PreferCodecOptions {
    pub prefer_hevc_profile_id: Option<u8>,
}

pub fn parse_resolution(value: &str) -> Option<(u32, u32)> {
    let (width, height) = value.split_once('x')?;
    let width = width.parse().ok()?;
    let height = height.parse().ok()?;
    Some((width, height))
}

pub fn extract_public_ip(host_or_ip: &str) -> Option<String> {
    if host_or_ip.is_empty() {
        return None;
    }

    let ipv4 = Regex::new(r"^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}$").expect("valid regex");
    if ipv4.is_match(host_or_ip) {
        return Some(host_or_ip.to_owned());
    }

    let first_label = host_or_ip.split('.').next().unwrap_or_default();
    let parts: Vec<&str> = first_label.split('-').collect();
    if parts.len() == 4
        && parts.iter().all(|part| {
            !part.is_empty() && part.len() <= 3 && part.as_bytes().iter().all(u8::is_ascii_digit)
        })
    {
        return Some(parts.join("."));
    }

    None
}

pub fn fix_server_ip(sdp: &str, server_ip: &str) -> String {
    let Some(ip) = extract_public_ip(server_ip) else {
        return sdp.to_owned();
    };

    let fixed = sdp.replace("c=IN IP4 0.0.0.0", &format!("c=IN IP4 {ip}"));
    let candidate_re =
        Regex::new(r"(a=candidate:\S+\s+\d+\s+\w+\s+\d+\s+)0\.0\.0\.0(\s+)").expect("valid regex");
    candidate_re
        .replace_all(&fixed, format!("${{1}}{ip}${{2}}"))
        .into_owned()
}

pub fn duplicate_session_webrtc_attributes_to_media(sdp: &str) -> String {
    let ending = line_ending(sdp);
    let lines = split_lines_lossless(sdp);
    let first_media_index = lines.iter().position(|line| line.starts_with("m="));
    let Some(first_media_index) = first_media_index else {
        return sdp.to_owned();
    };

    let session_attributes: Vec<&str> = lines[..first_media_index]
        .iter()
        .copied()
        .filter(|line| {
            line.starts_with("a=ice-ufrag:")
                || line.starts_with("a=ice-pwd:")
                || line.starts_with("a=ice-options:")
                || line.starts_with("a=fingerprint:")
                || line.starts_with("a=setup:")
        })
        .collect();

    if session_attributes.is_empty() {
        return sdp.to_owned();
    }

    let mut output: Vec<String> = lines[..first_media_index]
        .iter()
        .filter(|line| !is_media_transport_attribute(line))
        .map(|line| (*line).to_owned())
        .collect();

    let mut index = first_media_index;
    while index < lines.len() {
        let section_start = index;
        index += 1;
        while index < lines.len() && !lines[index].starts_with("m=") {
            index += 1;
        }
        let section = &lines[section_start..index];
        let insert_index = section
            .iter()
            .position(|line| line.starts_with("a="))
            .unwrap_or(section.len());

        for line in &section[..insert_index] {
            output.push((*line).to_owned());
        }
        for attribute in &session_attributes {
            let prefix = attribute
                .split_once(':')
                .map(|(prefix, _)| format!("{prefix}:"))
                .unwrap_or_else(|| (*attribute).to_owned());
            if !section.iter().any(|line| line.starts_with(&prefix)) {
                output.push((*attribute).to_owned());
            }
        }
        for line in &section[insert_index..] {
            output.push((*line).to_owned());
        }
    }

    output.join(ending)
}

pub fn summarize_media_transport_attributes(sdp: &str) -> String {
    let lines = split_lines_lossless(sdp);
    let session_has_fingerprint = lines
        .iter()
        .take_while(|line| !line.starts_with("m="))
        .any(|line| line.starts_with("a=fingerprint:"));

    let mut media_count = 0usize;
    let mut fingerprint_count = 0usize;
    let mut setup_count = 0usize;
    let mut ice_ufrag_count = 0usize;
    let mut ice_pwd_count = 0usize;

    let mut index = 0usize;
    while index < lines.len() {
        if !lines[index].starts_with("m=") {
            index += 1;
            continue;
        }

        media_count += 1;
        index += 1;
        let mut has_fingerprint = false;
        let mut has_setup = false;
        let mut has_ice_ufrag = false;
        let mut has_ice_pwd = false;
        while index < lines.len() && !lines[index].starts_with("m=") {
            has_fingerprint |= lines[index].starts_with("a=fingerprint:");
            has_setup |= lines[index].starts_with("a=setup:");
            has_ice_ufrag |= lines[index].starts_with("a=ice-ufrag:");
            has_ice_pwd |= lines[index].starts_with("a=ice-pwd:");
            index += 1;
        }

        fingerprint_count += usize::from(has_fingerprint);
        setup_count += usize::from(has_setup);
        ice_ufrag_count += usize::from(has_ice_ufrag);
        ice_pwd_count += usize::from(has_ice_pwd);
    }

    format!(
        "mediaSections={media_count}, mediaFingerprints={fingerprint_count}, mediaSetup={setup_count}, mediaIceUfrag={ice_ufrag_count}, mediaIcePwd={ice_pwd_count}, sessionFingerprint={session_has_fingerprint}"
    )
}

pub fn sanitize_ice_pwd_for_gstreamer(sdp: &str) -> (String, usize) {
    let ending = line_ending(sdp);
    let mut replacements = 0usize;
    let lines = split_lines_lossless(sdp)
        .into_iter()
        .map(|line| {
            let Some(value) = line.strip_prefix("a=ice-pwd:") else {
                return line.to_owned();
            };

            let sanitized = sanitize_ice_pwd_value(value);
            if sanitized == value {
                return line.to_owned();
            }

            replacements += 1;
            format!("a=ice-pwd:{sanitized}")
        })
        .collect::<Vec<_>>();

    (lines.join(ending), replacements)
}

fn sanitize_ice_pwd_value(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || *character == '+' || *character == '/'
        })
        .collect()
}

fn is_media_transport_attribute(line: &str) -> bool {
    line.starts_with("a=ice-ufrag:")
        || line.starts_with("a=ice-pwd:")
        || line.starts_with("a=fingerprint:")
        || line.starts_with("a=setup:")
}

pub fn extract_ice_ufrag_from_offer(sdp: &str) -> String {
    sdp.lines()
        .find_map(|line| line.strip_prefix("a=ice-ufrag:"))
        .map(str::trim)
        .unwrap_or_default()
        .to_owned()
}

pub fn extract_ice_credentials(sdp: &str) -> IceCredentials {
    let mut ufrag = String::new();
    let mut pwd = String::new();
    let mut fingerprint = String::new();

    for line in sdp.lines() {
        if ufrag.is_empty() {
            if let Some(value) = line.strip_prefix("a=ice-ufrag:") {
                ufrag = value.trim().to_owned();
                continue;
            }
        }
        if pwd.is_empty() {
            if let Some(value) = line.strip_prefix("a=ice-pwd:") {
                pwd = value.trim().to_owned();
                continue;
            }
        }
        if fingerprint.is_empty() {
            if let Some(value) = extract_fingerprint_value(line) {
                fingerprint = value.to_owned();
            }
        }

        if !ufrag.is_empty() && !pwd.is_empty() && !fingerprint.is_empty() {
            break;
        }
    }

    IceCredentials {
        ufrag,
        pwd,
        fingerprint,
    }
}

fn extract_fingerprint_value(line: &str) -> Option<&str> {
    let value = line.strip_prefix("a=fingerprint:")?;
    let (_, fingerprint) = value.trim().split_once(' ')?;
    let fingerprint = fingerprint.trim();
    if fingerprint.is_empty() {
        None
    } else {
        Some(fingerprint)
    }
}

pub fn build_nvst_sdp_for_answer(params: &NvstParams, answer_sdp: &str) -> Result<String, String> {
    let credentials = extract_ice_credentials(answer_sdp);
    if credentials.ufrag.is_empty()
        || credentials.pwd.is_empty()
        || credentials.fingerprint.is_empty()
    {
        return Err(
            "Local answer SDP is missing ICE ufrag, ICE password, or DTLS fingerprint.".to_owned(),
        );
    }

    let mut answer_params = params.clone();
    answer_params.credentials = credentials;
    if let Some(codec) = extract_negotiated_video_codec(answer_sdp) {
        answer_params.codec = codec;
    }
    Ok(build_nvst_sdp(&answer_params))
}

pub fn extract_negotiated_video_codec(sdp: &str) -> Option<VideoCodec> {
    let lines = split_lines_lossless(sdp);
    let mut in_video_section = false;
    let mut video_payloads = Vec::new();
    let mut codec_by_payload_type: HashMap<String, String> = HashMap::new();

    for line in &lines {
        if line.starts_with("m=video") {
            in_video_section = true;
            video_payloads = line.split_whitespace().skip(3).map(str::to_owned).collect();
            continue;
        }
        if line.starts_with("m=") && in_video_section {
            in_video_section = false;
        }
        if !in_video_section || !line.starts_with("a=rtpmap:") {
            continue;
        }

        let rest = line.strip_prefix("a=rtpmap:").unwrap_or_default();
        let mut parts = rest.split_whitespace();
        let Some(pt) = parts.next() else {
            continue;
        };
        let Some(codec_part) = parts.next() else {
            continue;
        };
        let codec_name = normalize_codec(codec_part.split('/').next().unwrap_or_default());
        if !codec_name.is_empty() {
            codec_by_payload_type.insert(pt.to_owned(), codec_name);
        }
    }

    video_payloads
        .iter()
        .filter_map(|pt| codec_by_payload_type.get(pt))
        .find_map(|codec| match codec.as_str() {
            "H264" => Some(VideoCodec::H264),
            "H265" => Some(VideoCodec::H265),
            "AV1" => Some(VideoCodec::AV1),
            _ => None,
        })
}

fn line_ending(sdp: &str) -> &'static str {
    if sdp.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn normalize_codec(name: &str) -> String {
    let upper = name.to_ascii_uppercase();
    if upper == "HEVC" {
        "H265".to_owned()
    } else {
        upper
    }
}

fn split_lines_lossless(sdp: &str) -> Vec<&str> {
    sdp.split(['\r', '\n'])
        .filter(|line| !line.is_empty())
        .collect()
}

pub fn prefer_codec(sdp: &str, codec: VideoCodec, options: PreferCodecOptions) -> String {
    let ending = line_ending(sdp);
    let lines = split_lines_lossless(sdp);
    let target_codec = codec.as_str();
    let mut in_video_section = false;
    let mut payload_types_by_codec: HashMap<String, Vec<String>> = HashMap::new();
    let mut codec_by_payload_type: HashMap<String, String> = HashMap::new();
    let mut rtx_apt_by_payload_type: HashMap<String, String> = HashMap::new();
    let mut fmtp_by_payload_type: HashMap<String, String> = HashMap::new();

    for line in &lines {
        if line.starts_with("m=video") {
            in_video_section = true;
            continue;
        }
        if line.starts_with("m=") && in_video_section {
            in_video_section = false;
        }
        if !in_video_section || !line.starts_with("a=rtpmap:") {
            continue;
        }

        let rest = line.strip_prefix("a=rtpmap:").unwrap_or_default();
        let mut parts = rest.split_whitespace();
        let Some(pt) = parts.next() else {
            continue;
        };
        let Some(codec_part) = parts.next() else {
            continue;
        };
        let codec_name = normalize_codec(codec_part.split('/').next().unwrap_or_default());
        if codec_name.is_empty() {
            continue;
        }
        payload_types_by_codec
            .entry(codec_name.clone())
            .or_default()
            .push(pt.to_owned());
        codec_by_payload_type.insert(pt.to_owned(), codec_name);
    }

    in_video_section = false;
    let apt_re = Regex::new(r"(?i)(?:^|;)\s*apt=(\d+)").expect("valid regex");
    for line in &lines {
        if line.starts_with("m=video") {
            in_video_section = true;
            continue;
        }
        if line.starts_with("m=") && in_video_section {
            in_video_section = false;
        }
        if !in_video_section || !line.starts_with("a=fmtp:") {
            continue;
        }

        let rest = line
            .split_once(':')
            .map(|(_, rest)| rest)
            .unwrap_or_default();
        let mut parts = rest.splitn(2, char::is_whitespace);
        let Some(pt) = parts.next() else {
            continue;
        };
        let params = parts.next().unwrap_or_default().trim();
        if pt.is_empty() || params.is_empty() {
            continue;
        }

        if let Some(captures) = apt_re.captures(params) {
            if let Some(apt) = captures.get(1) {
                rtx_apt_by_payload_type.insert(pt.to_owned(), apt.as_str().to_owned());
            }
        }
        fmtp_by_payload_type.insert(pt.to_owned(), params.to_owned());
    }

    let Some(preferred_payloads) = payload_types_by_codec.get(target_codec) else {
        return sdp.to_owned();
    };
    if preferred_payloads.is_empty() {
        return sdp.to_owned();
    }

    let mut ordered_preferred_payloads = preferred_payloads.clone();
    if codec == VideoCodec::H265 {
        if let Some(preferred_profile) = options.prefer_hevc_profile_id {
            ordered_preferred_payloads.sort_by_key(|pt| {
                let fmtp = fmtp_by_payload_type
                    .get(pt)
                    .map(String::as_str)
                    .unwrap_or_default();
                let profile = capture_numeric_param(fmtp, "profile-id");
                if profile == Some(preferred_profile as u32) {
                    0
                } else if profile.is_none() {
                    1
                } else {
                    2
                }
            });
        }
    }

    let preferred: HashSet<String> = ordered_preferred_payloads.iter().cloned().collect();
    let mut allowed = preferred.clone();

    for (rtx_pt, apt) in rtx_apt_by_payload_type {
        if preferred.contains(&apt)
            && codec_by_payload_type
                .get(&rtx_pt)
                .is_some_and(|name| name == "RTX")
        {
            allowed.insert(rtx_pt);
        }
    }

    for (pt, codec_name) in &codec_by_payload_type {
        if matches!(codec_name.as_str(), "FLEXFEC-03") {
            allowed.insert(pt.clone());
        }
    }

    let mut filtered = Vec::new();
    in_video_section = false;

    for line in lines {
        if line.starts_with("m=video") {
            in_video_section = true;
            let parts: Vec<&str> = line.split_whitespace().collect();
            let header = &parts[..parts.len().min(3)];
            let available: Vec<&str> = parts
                .iter()
                .skip(3)
                .copied()
                .filter(|pt| allowed.contains(*pt))
                .collect();
            let mut ordered = Vec::new();

            for pt in &ordered_preferred_payloads {
                if available.contains(&pt.as_str()) {
                    ordered.push(pt.as_str());
                }
            }
            for pt in available {
                if !preferred.contains(pt) {
                    ordered.push(pt);
                }
            }

            if ordered.is_empty() {
                filtered.push(line.to_owned());
            } else {
                filtered.push(
                    header
                        .iter()
                        .chain(ordered.iter())
                        .copied()
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            }
            continue;
        }

        if line.starts_with("m=") && in_video_section {
            in_video_section = false;
        }

        if in_video_section
            && (line.starts_with("a=rtpmap:")
                || line.starts_with("a=fmtp:")
                || line.starts_with("a=rtcp-fb:"))
        {
            let rest = line
                .split_once(':')
                .map(|(_, rest)| rest)
                .unwrap_or_default();
            let pt = rest.split_whitespace().next().unwrap_or_default();
            if !pt.is_empty() && !allowed.contains(pt) {
                continue;
            }
        }

        filtered.push(line.to_owned());
    }

    filtered.join(ending)
}

fn capture_numeric_param(params: &str, key: &str) -> Option<u32> {
    for part in params.split(';') {
        let trimmed = part.trim();
        let Some((candidate_key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if candidate_key.eq_ignore_ascii_case(key) {
            return value.trim().parse().ok();
        }
    }
    None
}

pub fn munge_answer_sdp(sdp: &str, max_bitrate_kbps: u32) -> String {
    let ending = line_ending(sdp);
    let lines = split_lines_lossless(sdp);
    let mut result = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        let mut current = (*line).to_owned();
        if current.starts_with("a=fmtp:")
            && current.contains("minptime=")
            && !current.contains("stereo=1")
        {
            current.push_str(";stereo=1");
        }
        result.push(current);

        if line.starts_with("m=video") || line.starts_with("m=audio") {
            let bitrate = if line.starts_with("m=video") {
                max_bitrate_kbps
            } else {
                128
            };
            let next_line = lines.get(index + 1).copied().unwrap_or_default();
            if !next_line.starts_with("b=") {
                result.push(format!("b=AS:{bitrate}"));
            }
        }
    }

    result.join(ending)
}

pub fn build_nvst_sdp(params: &NvstParams) -> String {
    let min_bitrate = 5000.max(params.max_bitrate_kbps * 35 / 100);
    let initial_bitrate = min_bitrate.max(params.max_bitrate_kbps * 70 / 100);
    let is_high_fps = params.fps >= 90;
    let is_120_fps = params.fps == 120;
    let is_240_fps = params.fps >= 240;
    let is_av1 = params.codec == VideoCodec::AV1;
    let bit_depth = params.color_quality.bit_depth();
    let hid_device_mask = params
        .hid_device_mask
        .unwrap_or(PARTIALLY_RELIABLE_HID_DEVICE_MASK_ALL);
    let enable_partially_reliable_transfer_gamepad = params
        .enable_partially_reliable_transfer_gamepad
        .unwrap_or(PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL);
    let enable_partially_reliable_transfer_hid = params
        .enable_partially_reliable_transfer_hid
        .unwrap_or(hid_device_mask);

    let mut lines = vec![
        "v=0".to_owned(),
        "o=SdpTest test_id_13 14 IN IPv4 127.0.0.1".to_owned(),
        "s=-".to_owned(),
        "t=0 0".to_owned(),
        format!("a=general.icePassword:{}", params.credentials.pwd),
        format!("a=general.iceUserNameFragment:{}", params.credentials.ufrag),
        format!(
            "a=general.dtlsFingerprint:{}",
            params.credentials.fingerprint
        ),
        "m=video 0 RTP/AVP".to_owned(),
        "a=msid:fbc-video-0".to_owned(),
        "a=vqos.fec.rateDropWindow:10".to_owned(),
        "a=vqos.fec.minRequiredFecPackets:2".to_owned(),
        "a=vqos.fec.repairMinPercent:5".to_owned(),
        "a=vqos.fec.repairPercent:5".to_owned(),
        "a=vqos.fec.repairMaxPercent:35".to_owned(),
        "a=vqos.dynamicStreamingMode:0".to_owned(),
        "a=vqos.drc.enable:0".to_owned(),
        "a=video.dx9EnableNv12:1".to_owned(),
        "a=video.dx9EnableHdr:1".to_owned(),
        "a=vqos.qpg.enable:1".to_owned(),
        "a=vqos.resControl.qp.qpg.featureSetting:7".to_owned(),
        "a=bwe.useOwdCongestionControl:1".to_owned(),
        "a=video.enableRtpNack:1".to_owned(),
        "a=vqos.bw.txRxLag.minFeedbackTxDeltaMs:200".to_owned(),
        "a=vqos.drc.bitrateIirFilterFactor:18".to_owned(),
        "a=video.packetSize:1140".to_owned(),
        "a=packetPacing.minNumPacketsPerGroup:15".to_owned(),
    ];

    if is_high_fps {
        lines.extend([
            "a=vqos.dfc.enable:1".to_owned(),
            "a=vqos.dfc.decodeFpsAdjPercent:85".to_owned(),
            "a=vqos.dfc.targetDownCooldownMs:250".to_owned(),
            format!(
                "a=vqos.dfc.dfcAlgoVersion:{}",
                if is_120_fps || is_240_fps { 2 } else { 1 }
            ),
            format!(
                "a=vqos.dfc.minTargetFps:{}",
                if is_120_fps || is_240_fps { 100 } else { 60 }
            ),
            "a=vqos.resControl.dfc.useClientFpsPerf:0".to_owned(),
            "a=vqos.dfc.adjustResAndFps:0".to_owned(),
        ]);
        lines.extend([
            "a=bwe.iirFilterFactor:8".to_owned(),
            "a=video.encoderFeatureSetting:47".to_owned(),
            "a=video.encoderPreset:6".to_owned(),
            "a=vqos.resControl.cpmRtc.badNwSkipFramesCount:600".to_owned(),
            "a=vqos.resControl.cpmRtc.decodeTimeThresholdMs:9".to_owned(),
            format!(
                "a=video.fbcDynamicFpsGrabTimeoutMs:{}",
                if is_120_fps { 6 } else { 18 }
            ),
            format!(
                "a=vqos.resControl.cpmRtc.serverResolutionUpdateCoolDownCount:{}",
                if is_120_fps { 6000 } else { 12000 }
            ),
        ]);
    } else {
        lines.extend([
            "a=vqos.dfc.enable:0".to_owned(),
            "a=vqos.dfc.adjustResAndFps:0".to_owned(),
        ]);
    }

    if is_240_fps {
        lines.extend([
            "a=video.enableNextCaptureMode:1".to_owned(),
            "a=vqos.maxStreamFpsEstimate:240".to_owned(),
        ]);
        if ENABLE_240_FPS_SPLIT_ENCODE {
            lines.push("a=video.videoSplitEncodeStripsPerFrame:3".to_owned());
            lines.push(format!(
                "a=video.updateSplitEncodeStateDynamically:{}",
                if ENABLE_DYNAMIC_SPLIT_ENCODE_UPDATES {
                    1
                } else {
                    0
                }
            ));
            lines.push("a=vqos.rtcPreemptiveIdrSettings.minBurstNackSize:65535".to_owned());
            lines
                .push("a=vqos.rtcPreemptiveIdrSettings.minNackPacketCaptureAgeMs:65535".to_owned());
        }
    }

    lines.extend([
        format!(
            "a=vqos.adjustStreamingFpsDuringOutOfFocus:{}",
            if ENABLE_OUT_OF_FOCUS_FPS_ADJUSTMENT {
                1
            } else {
                0
            }
        ),
        "a=vqos.resControl.cpmRtc.ignoreOutOfFocusWindowState:1".to_owned(),
        "a=vqos.resControl.perfHistory.rtcIgnoreOutOfFocusWindowState:1".to_owned(),
        "a=vqos.resControl.cpmRtc.featureMask:0".to_owned(),
        "a=vqos.resControl.cpmRtc.enable:0".to_owned(),
        "a=vqos.resControl.cpmRtc.minResolutionPercent:100".to_owned(),
        "a=vqos.resControl.cpmRtc.resolutionChangeHoldonMs:999999".to_owned(),
        format!(
            "a=packetPacing.numGroups:{}",
            if is_120_fps { 3 } else { 5 }
        ),
        "a=packetPacing.maxDelayUs:1000".to_owned(),
        "a=packetPacing.minNumPacketsFrame:10".to_owned(),
        "a=video.rtpNackQueueLength:1024".to_owned(),
        "a=video.rtpNackQueueMaxPackets:512".to_owned(),
        "a=video.rtpNackMaxPacketCount:25".to_owned(),
        "a=vqos.drc.qpMaxResThresholdAdj:4".to_owned(),
        "a=vqos.grc.qpMaxResThresholdAdj:4".to_owned(),
        "a=vqos.drc.iirFilterFactor:100".to_owned(),
    ]);

    if is_av1 {
        lines.extend([
            "a=vqos.drc.minQpHeadroom:20".to_owned(),
            "a=vqos.drc.lowerQpThreshold:100".to_owned(),
            "a=vqos.drc.upperQpThreshold:200".to_owned(),
            "a=vqos.drc.minAdaptiveQpThreshold:180".to_owned(),
            "a=vqos.drc.qpCodecThresholdAdj:0".to_owned(),
            "a=vqos.drc.qpMaxResThresholdAdj:20".to_owned(),
            "a=vqos.dfc.minQpHeadroom:20".to_owned(),
            "a=vqos.dfc.qpLowerLimit:100".to_owned(),
            "a=vqos.dfc.qpMaxUpperLimit:200".to_owned(),
            "a=vqos.dfc.qpMinUpperLimit:180".to_owned(),
            "a=vqos.dfc.qpMaxResThresholdAdj:20".to_owned(),
            "a=vqos.dfc.qpCodecThresholdAdj:0".to_owned(),
            "a=vqos.grc.minQpHeadroom:20".to_owned(),
            "a=vqos.grc.lowerQpThreshold:100".to_owned(),
            "a=vqos.grc.upperQpThreshold:200".to_owned(),
            "a=vqos.grc.minAdaptiveQpThreshold:180".to_owned(),
            "a=vqos.grc.qpMaxResThresholdAdj:20".to_owned(),
            "a=vqos.grc.qpCodecThresholdAdj:0".to_owned(),
            "a=video.minQp:25".to_owned(),
            "a=video.enableAv1RcPrecisionFactor:1".to_owned(),
        ]);
    }

    lines.extend([
        format!("a=video.clientViewportWd:{}", params.width),
        format!("a=video.clientViewportHt:{}", params.height),
        format!("a=video.maxFPS:{}", params.fps),
        format!("a=video.initialBitrateKbps:{initial_bitrate}"),
        format!(
            "a=video.initialPeakBitrateKbps:{}",
            params.max_bitrate_kbps
        ),
        format!("a=vqos.bw.maximumBitrateKbps:{}", params.max_bitrate_kbps),
        format!("a=vqos.bw.minimumBitrateKbps:{min_bitrate}"),
        format!("a=vqos.bw.peakBitrateKbps:{}", params.max_bitrate_kbps),
        format!(
            "a=vqos.bw.serverPeakBitrateKbps:{}",
            params.max_bitrate_kbps
        ),
        "a=vqos.bw.enableBandwidthEstimation:1".to_owned(),
        "a=vqos.bw.disableBitrateLimit:0".to_owned(),
        format!("a=vqos.grc.maximumBitrateKbps:{}", params.max_bitrate_kbps),
        "a=vqos.grc.enable:0".to_owned(),
        "a=video.maxNumReferenceFrames:4".to_owned(),
        "a=video.mapRtpTimestampsToFrames:1".to_owned(),
        "a=video.encoderCscMode:3".to_owned(),
        "a=video.dynamicRangeMode:0".to_owned(),
        format!("a=video.bitDepth:{bit_depth}"),
        format!("a=video.scalingFeature1:{}", if is_av1 { 1 } else { 0 }),
        "a=video.prefilterParams.prefilterModel:0".to_owned(),
        "m=audio 0 RTP/AVP".to_owned(),
        "a=msid:audio".to_owned(),
        "m=mic 0 RTP/AVP".to_owned(),
        "a=msid:mic".to_owned(),
        "a=rtpmap:0 PCMU/8000".to_owned(),
        "m=application 0 RTP/AVP".to_owned(),
        "a=msid:input_1".to_owned(),
        format!(
            "a=ri.partialReliableThresholdMs:{}",
            params.partial_reliable_threshold_ms
        ),
        format!("a=ri.hidDeviceMask:{hid_device_mask}"),
        format!(
            "a=ri.enablePartiallyReliableTransferGamepad:{enable_partially_reliable_transfer_gamepad}"
        ),
        format!(
            "a=ri.enablePartiallyReliableTransferHid:{enable_partially_reliable_transfer_hid}"
        ),
        String::new(),
    ]);

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_public_ip_from_host_or_ip() {
        assert_eq!(
            extract_public_ip("80-250-97-40.cloudmatchbeta.nvidiagrid.net").as_deref(),
            Some("80.250.97.40"),
        );
        assert_eq!(
            extract_public_ip("161.248.11.132").as_deref(),
            Some("161.248.11.132")
        );
        assert_eq!(extract_public_ip("not-an-ip.example.com"), None);
    }

    #[test]
    fn fixes_connection_and_candidate_ips() {
        let offer = "v=0\nc=IN IP4 0.0.0.0\na=candidate:1 1 udp 1 0.0.0.0 49000 typ host\n";
        let fixed = fix_server_ip(offer, "80-250-97-40.cloudmatchbeta.nvidiagrid.net");
        assert!(fixed.contains("c=IN IP4 80.250.97.40"));
        assert!(fixed.contains("a=candidate:1 1 udp 1 80.250.97.40 49000 typ host"));
    }

    #[test]
    fn duplicates_session_webrtc_attributes_into_media_sections() {
        let offer = [
            "v=0",
            "a=ice-options:trickle",
            "a=ice-ufrag:user",
            "a=ice-pwd:pass",
            "a=fingerprint:sha-256 AA:BB",
            "a=setup:actpass",
            "m=audio 9 UDP/TLS/RTP/SAVPF 111",
            "c=IN IP4 10.0.0.1",
            "a=mid:0",
            "m=video 9 UDP/TLS/RTP/SAVPF 96",
            "c=IN IP4 10.0.0.1",
            "a=mid:1",
        ]
        .join("\n");

        let normalized = duplicate_session_webrtc_attributes_to_media(&offer);
        let session_part = normalized.split("\nm=").next().expect("session section");
        let media_sections = normalized
            .split("\nm=")
            .skip(1)
            .map(|section| format!("m={section}"))
            .collect::<Vec<_>>();

        assert_eq!(media_sections.len(), 2);
        for section in media_sections {
            assert!(section.contains("a=ice-options:trickle"));
            assert!(section.contains("a=ice-ufrag:user"));
            assert!(section.contains("a=ice-pwd:pass"));
            assert!(section.contains("a=fingerprint:sha-256 AA:BB"));
            assert!(section.contains("a=setup:actpass"));
        }
        assert!(!session_part.contains("a=ice-ufrag:user"));
        assert!(!session_part.contains("a=ice-pwd:pass"));
        assert!(!session_part.contains("a=fingerprint:sha-256 AA:BB"));
        assert!(!session_part.contains("a=setup:actpass"));
    }

    #[test]
    fn keeps_existing_media_webrtc_attributes() {
        let offer = [
            "v=0",
            "a=ice-ufrag:session",
            "a=ice-pwd:session-pass",
            "m=audio 9 UDP/TLS/RTP/SAVPF 111",
            "a=ice-ufrag:media",
            "a=ice-pwd:media-pass",
            "a=mid:0",
        ]
        .join("\n");

        let normalized = duplicate_session_webrtc_attributes_to_media(&offer);

        assert!(normalized.contains("a=ice-ufrag:media"));
        assert!(normalized.contains("a=ice-pwd:media-pass"));
        assert_eq!(normalized.matches("a=ice-ufrag:session").count(), 0);
        assert_eq!(normalized.matches("a=ice-pwd:session-pass").count(), 0);
    }

    #[test]
    fn summarizes_media_transport_attributes() {
        let offer = [
            "v=0",
            "a=group:BUNDLE 0 1",
            "m=audio 9 UDP/TLS/RTP/SAVPF 111",
            "a=fingerprint:sha-256 AA:BB",
            "a=setup:actpass",
            "a=ice-ufrag:user",
            "a=ice-pwd:pass",
            "m=video 9 UDP/TLS/RTP/SAVPF 96",
            "a=setup:actpass",
        ]
        .join("\n");

        assert_eq!(
            summarize_media_transport_attributes(&offer),
            "mediaSections=2, mediaFingerprints=1, mediaSetup=2, mediaIceUfrag=1, mediaIcePwd=1, sessionFingerprint=false"
        );
    }

    #[test]
    fn sanitizes_nonstandard_ice_password_for_gstreamer() {
        let sdp = [
            "v=0",
            "m=audio 9 UDP/TLS/RTP/SAVPF 111",
            "a=ice-pwd:48ca4c4b-199a-454c-b58a-3d14739335a3",
            "m=video 9 UDP/TLS/RTP/SAVPF 96",
            "a=ice-pwd:alreadyValidPassword123456",
        ]
        .join("\n");

        let (sanitized, replacements) = sanitize_ice_pwd_for_gstreamer(&sdp);

        assert_eq!(replacements, 1);
        assert!(sanitized.contains("a=ice-pwd:48ca4c4b199a454cb58a3d14739335a3"));
        assert!(sanitized.contains("a=ice-pwd:alreadyValidPassword123456"));
    }

    #[test]
    fn extracts_ice_credentials() {
        let sdp = "a=ice-ufrag:user\r\na=ice-pwd:pass\r\na=fingerprint:sha-256 AA:BB\r\na=ice-ufrag:other\r\na=ice-pwd:other-pass\r\na=fingerprint:sha-256 CC:DD\r\n";
        assert_eq!(extract_ice_ufrag_from_offer(sdp), "user");
        assert_eq!(
            extract_ice_credentials(sdp),
            IceCredentials {
                ufrag: "user".to_owned(),
                pwd: "pass".to_owned(),
                fingerprint: "AA:BB".to_owned(),
            },
        );
    }

    #[test]
    fn builds_nvst_sdp_with_local_answer_credentials() {
        let params = NvstParams {
            width: 1920,
            height: 1080,
            fps: 60,
            max_bitrate_kbps: 75_000,
            partial_reliable_threshold_ms: 16,
            codec: VideoCodec::H265,
            color_quality: ColorQuality::EightBit420,
            credentials: IceCredentials {
                ufrag: "remote-user".to_owned(),
                pwd: "remote-password".to_owned(),
                fingerprint: "AA:BB".to_owned(),
            },
            hid_device_mask: None,
            enable_partially_reliable_transfer_gamepad: None,
            enable_partially_reliable_transfer_hid: None,
        };
        let answer_sdp = [
            "v=0",
            "m=audio 9 UDP/TLS/RTP/SAVPF 111",
            "a=ice-ufrag:local-user",
            "a=ice-pwd:local-password",
            "a=fingerprint:sha-256 CC:DD",
            "m=video 9 UDP/TLS/RTP/SAVPF 96",
            "a=ice-ufrag:video-user",
            "a=ice-pwd:video-password",
            "a=fingerprint:sha-256 EE:FF",
        ]
        .join("\n");

        let nvst = build_nvst_sdp_for_answer(&params, &answer_sdp).expect("nvst sdp");

        assert!(nvst.contains("a=general.icePassword:local-password"));
        assert!(nvst.contains("a=general.iceUserNameFragment:local-user"));
        assert!(nvst.contains("a=general.dtlsFingerprint:CC:DD"));
        assert!(!nvst.contains("remote-password"));
        assert!(!nvst.contains("video-password"));
    }

    fn nvst_params_for_fps(fps: u32) -> NvstParams {
        NvstParams {
            width: 1920,
            height: 1080,
            fps,
            max_bitrate_kbps: 75_000,
            partial_reliable_threshold_ms: 16,
            codec: VideoCodec::H265,
            color_quality: ColorQuality::EightBit420,
            credentials: IceCredentials {
                ufrag: "user".to_owned(),
                pwd: "password".to_owned(),
                fingerprint: "AA:BB".to_owned(),
            },
            hid_device_mask: None,
            enable_partially_reliable_transfer_gamepad: None,
            enable_partially_reliable_transfer_hid: None,
        }
    }

    #[test]
    fn builds_nvst_sdp_disables_dynamic_streaming_for_normal_fps() {
        let nvst = build_nvst_sdp(&nvst_params_for_fps(60));

        assert!(nvst.contains("a=vqos.dynamicStreamingMode:0"));
        assert!(nvst.contains("a=vqos.dfc.adjustResAndFps:0"));
        assert!(nvst.contains("a=vqos.dfc.enable:0"));
        assert!(!nvst.contains("a=vqos.dfc.decodeFpsAdjPercent:85"));
        assert!(!nvst.contains("a=vqos.resControl.dfc.useClientFpsPerf:0"));
    }

    #[test]
    fn builds_nvst_sdp_disables_dynamic_streaming_for_high_fps() {
        for fps in [120, 240] {
            let nvst = build_nvst_sdp(&nvst_params_for_fps(fps));

            assert!(nvst.contains("a=vqos.dynamicStreamingMode:0"));
            assert!(nvst.contains("a=vqos.dfc.adjustResAndFps:0"));
            assert!(nvst.contains("a=vqos.dfc.enable:1"));
            assert!(nvst.contains("a=vqos.resControl.dfc.useClientFpsPerf:0"));
            assert!(nvst.contains("a=vqos.dfc.dfcAlgoVersion:2"));
            assert!(nvst.contains("a=vqos.dfc.minTargetFps:100"));
            assert!(!nvst.contains("a=vqos.dfc.enable:0"));
        }
    }

    #[test]
    fn extracts_negotiated_video_codec_from_answer_payload_order() {
        let answer_sdp = [
            "v=0",
            "m=audio 9 UDP/TLS/RTP/SAVPF 111",
            "a=rtpmap:111 OPUS/48000/2",
            "m=video 9 UDP/TLS/RTP/SAVPF 101 102",
            "a=rtpmap:101 AV1/90000",
            "a=rtpmap:102 rtx/90000",
            "a=fmtp:102 apt=101",
        ]
        .join("\n");

        assert_eq!(
            extract_negotiated_video_codec(&answer_sdp),
            Some(VideoCodec::AV1)
        );
    }

    #[test]
    fn builds_nvst_sdp_for_answer_uses_negotiated_av1_codec() {
        let params = NvstParams {
            width: 2560,
            height: 1440,
            fps: 120,
            max_bitrate_kbps: 150_000,
            partial_reliable_threshold_ms: 16,
            codec: VideoCodec::H265,
            color_quality: ColorQuality::EightBit420,
            credentials: IceCredentials {
                ufrag: "remote-user".to_owned(),
                pwd: "remote-password".to_owned(),
                fingerprint: "AA:BB".to_owned(),
            },
            hid_device_mask: None,
            enable_partially_reliable_transfer_gamepad: None,
            enable_partially_reliable_transfer_hid: None,
        };
        let answer_sdp = [
            "v=0",
            "m=audio 9 UDP/TLS/RTP/SAVPF 111",
            "a=ice-ufrag:local-user",
            "a=ice-pwd:local-password",
            "a=fingerprint:sha-256 CC:DD",
            "m=video 9 UDP/TLS/RTP/SAVPF 101",
            "a=rtpmap:101 AV1/90000",
        ]
        .join("\n");

        let nvst = build_nvst_sdp_for_answer(&params, &answer_sdp).expect("nvst sdp");

        assert!(nvst.contains("a=video.scalingFeature1:1"));
        assert!(nvst.contains("a=video.enableAv1RcPrecisionFactor:1"));
        assert!(nvst.contains("a=vqos.drc.minQpHeadroom:20"));
    }

    #[test]
    fn munges_answer_bitrate_and_opus_stereo() {
        let sdp = "m=video 9 UDP/TLS/RTP/SAVPF 96\nm=audio 9 UDP/TLS/RTP/SAVPF 111\na=fmtp:111 minptime=10;useinbandfec=1";
        let munged = munge_answer_sdp(sdp, 75000);
        assert!(munged.contains("m=video 9 UDP/TLS/RTP/SAVPF 96\nb=AS:75000"));
        assert!(munged.contains("m=audio 9 UDP/TLS/RTP/SAVPF 111\nb=AS:128"));
        assert!(munged.contains("a=fmtp:111 minptime=10;useinbandfec=1;stereo=1"));
    }

    #[test]
    fn filters_video_codec_and_keeps_matching_rtx() {
        let sdp = [
            "v=0",
            "m=video 9 UDP/TLS/RTP/SAVPF 96 97 98 99 100 101",
            "a=rtpmap:96 H264/90000",
            "a=rtpmap:97 rtx/90000",
            "a=fmtp:97 apt=96",
            "a=rtpmap:98 H265/90000",
            "a=fmtp:98 profile-id=1;level-id=186",
            "a=rtpmap:99 rtx/90000",
            "a=fmtp:99 apt=98",
            "a=rtpmap:100 AV1/90000",
            "a=rtpmap:101 flexfec-03/90000",
            "m=audio 9 UDP/TLS/RTP/SAVPF 111",
        ]
        .join("\n");
        let filtered = prefer_codec(
            &sdp,
            VideoCodec::H265,
            PreferCodecOptions {
                prefer_hevc_profile_id: Some(1),
            },
        );
        assert!(filtered.contains("m=video 9 UDP/TLS/RTP/SAVPF 98 99 101"));
        assert!(!filtered.contains("a=rtpmap:96 H264/90000"));
        assert!(filtered.contains("a=rtpmap:99 rtx/90000"));
        assert!(filtered.contains("a=rtpmap:101 flexfec-03/90000"));
        assert!(filtered.contains("m=audio 9 UDP/TLS/RTP/SAVPF 111"));
    }

    #[test]
    fn builds_nvst_sdp_with_core_attributes() {
        let nvst = build_nvst_sdp(&NvstParams {
            width: 1920,
            height: 1080,
            fps: 120,
            max_bitrate_kbps: 75_000,
            partial_reliable_threshold_ms: 16,
            codec: VideoCodec::H265,
            color_quality: ColorQuality::TenBit420,
            credentials: IceCredentials {
                ufrag: "ufrag".to_owned(),
                pwd: "pwd".to_owned(),
                fingerprint: "AA:BB".to_owned(),
            },
            hid_device_mask: None,
            enable_partially_reliable_transfer_gamepad: None,
            enable_partially_reliable_transfer_hid: None,
        });

        assert!(nvst.contains("a=general.icePassword:pwd"));
        assert!(nvst.contains("a=video.clientViewportWd:1920"));
        assert!(nvst.contains("a=video.clientViewportHt:1080"));
        assert!(nvst.contains("a=video.maxFPS:120"));
        assert!(nvst.contains("a=video.bitDepth:10"));
        assert!(nvst.contains("a=packetPacing.numGroups:3"));
        assert!(nvst.contains("a=vqos.adjustStreamingFpsDuringOutOfFocus:0"));
        assert!(!nvst.contains("a=video.updateSplitEncodeStateDynamically:1"));
        assert!(nvst.contains("a=ri.partialReliableThresholdMs:16"));
        assert!(nvst.ends_with('\n'));
    }

    #[test]
    fn advertises_official_240_fps_split_encode_profile() {
        let nvst = build_nvst_sdp(&NvstParams {
            width: 1920,
            height: 1080,
            fps: 240,
            max_bitrate_kbps: 75_000,
            partial_reliable_threshold_ms: 16,
            codec: VideoCodec::H265,
            color_quality: ColorQuality::EightBit420,
            credentials: IceCredentials {
                ufrag: "ufrag".to_owned(),
                pwd: "pwd".to_owned(),
                fingerprint: "AA:BB".to_owned(),
            },
            hid_device_mask: None,
            enable_partially_reliable_transfer_gamepad: None,
            enable_partially_reliable_transfer_hid: None,
        });

        assert!(nvst.contains("a=vqos.maxStreamFpsEstimate:240"));
        assert!(nvst.contains("a=video.videoSplitEncodeStripsPerFrame:3"));
        assert!(nvst.contains("a=video.updateSplitEncodeStateDynamically:1"));
        assert!(nvst.contains("a=vqos.rtcPreemptiveIdrSettings.minBurstNackSize:65535"));
        assert!(nvst.contains("a=vqos.rtcPreemptiveIdrSettings.minNackPacketCaptureAgeMs:65535"));
    }
}
