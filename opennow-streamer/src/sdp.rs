pub fn extract_public_ip(host_or_ip: &str) -> Option<String> {
    if host_or_ip.is_empty() {
        return None;
    }
    if host_or_ip.split('.').count() == 4 && host_or_ip.split('.').all(|p| p.parse::<u8>().is_ok()) {
        return Some(host_or_ip.to_string());
    }
    let first = host_or_ip.split('.').next().unwrap_or_default();
    let parts: Vec<_> = first.split('-').collect();
    if parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok()) {
        return Some(parts.join("."));
    }
    None
}

pub fn fix_server_ip(sdp: &str, server_ip: &str) -> String {
    let Some(ip) = extract_public_ip(server_ip) else {
        return sdp.to_string();
    };
    let fixed = sdp.replace("c=IN IP4 0.0.0.0", &format!("c=IN IP4 {ip}"));
    fixed
        .lines()
        .map(|line| {
            if line.starts_with("a=candidate:") && line.contains(" 0.0.0.0 ") {
                line.replacen(" 0.0.0.0 ", &format!(" {ip} "), 1)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\r\n")
}

pub fn parse_partial_reliable_threshold_ms(sdp: &str) -> Option<u16> {
    sdp.lines()
        .find_map(|line| line.trim().strip_prefix("a=ri.partialReliableThresholdMs:"))
        .and_then(|value| value.trim().parse::<u16>().ok())
}

pub fn extract_ice_ufrag_from_offer(sdp: &str) -> String {
    sdp.lines()
        .find_map(|line| line.trim().strip_prefix("a=ice-ufrag:"))
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

pub struct IceCredentials {
    pub ufrag: String,
    pub pwd: String,
    pub fingerprint: String,
}

pub fn extract_ice_credentials(sdp: &str) -> IceCredentials {
    let mut ufrag = String::new();
    let mut pwd = String::new();
    let mut fingerprint = String::new();
    for line in sdp.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("a=ice-ufrag:") {
            ufrag = value.trim().to_string();
        } else if let Some(value) = trimmed.strip_prefix("a=ice-pwd:") {
            pwd = value.trim().to_string();
        } else if let Some(value) = trimmed.strip_prefix("a=fingerprint:sha-256 ") {
            fingerprint = value.trim().to_string();
        }
    }
    IceCredentials { ufrag, pwd, fingerprint }
}

pub fn munge_answer_sdp(sdp: &str, max_bitrate_kbps: u32) -> String {
    let line_ending = if sdp.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = Vec::new();
    for line in sdp.lines() {
        out.push(line.to_string());
        if line.starts_with("m=video") {
            out.push(format!("b=AS:{max_bitrate_kbps}"));
        } else if line.starts_with("m=audio") {
            out.push("b=AS:128".to_string());
        } else if line.starts_with("a=fmtp:") && line.contains("minptime=") && !line.contains("stereo=1") {
            out.pop();
            out.push(format!("{line};stereo=1"));
        }
    }
    out.join(line_ending)
}

pub fn normalize_answer_transport_sdp(sdp: &str) -> String {
    let line_ending = if sdp.contains("\r\n") { "\r\n" } else { "\n" };
    let session_fingerprint = sdp
        .lines()
        .find(|line| line.starts_with("a=fingerprint:"))
        .map(ToOwned::to_owned);

    let mut result = Vec::new();
    let mut current_media = Vec::new();

    let flush_media = |result: &mut Vec<String>, current_media: &mut Vec<String>| {
        if current_media.is_empty() {
            return;
        }
        let mut setup_seen = false;
        let mut fingerprint_seen = false;
        let mut normalized = Vec::with_capacity(current_media.len() + 2);
        for line in current_media.drain(..) {
            if line.starts_with("a=setup:") {
                if !setup_seen {
                    normalized.push("a=setup:active".to_string());
                    setup_seen = true;
                }
                continue;
            }
            if line.starts_with("a=fingerprint:") {
                fingerprint_seen = true;
            }
            normalized.push(line);
        }
        if !setup_seen {
            normalized.push("a=setup:active".to_string());
        }
        if !fingerprint_seen {
            if let Some(fingerprint) = &session_fingerprint {
                normalized.push(fingerprint.clone());
            }
        }
        result.extend(normalized);
    };

    for line in sdp.lines() {
        if line.starts_with("m=") {
            flush_media(&mut result, &mut current_media);
        }
        if current_media.is_empty() && !line.starts_with("m=") {
            result.push(line.to_string());
        } else {
            current_media.push(line.to_string());
        }
    }
    flush_media(&mut result, &mut current_media);

    result.join(line_ending)
}

pub fn prefer_codec(sdp: &str, codec: &str) -> String {
    let target = codec.to_uppercase();
    let mut allowed_pts = Vec::<String>::new();
    let mut rtx_by_pt = Vec::<(String, String)>::new();
    let mut in_video = false;
    for line in sdp.lines() {
        if line.starts_with("m=video") {
            in_video = true;
            continue;
        }
        if line.starts_with("m=") && in_video {
            in_video = false;
        }
        if !in_video { continue; }
        if let Some(rest) = line.strip_prefix("a=rtpmap:") {
            let mut parts = rest.split_whitespace();
            let pt = parts.next().unwrap_or_default();
            let codec_name = parts.next().unwrap_or_default().split('/').next().unwrap_or_default().to_uppercase();
            let normalized = if codec_name == "HEVC" { "H265".to_string() } else { codec_name };
            if normalized == target {
                allowed_pts.push(pt.to_string());
            }
        }
    }
    in_video = false;
    for line in sdp.lines() {
        if line.starts_with("m=video") {
            in_video = true;
            continue;
        }
        if line.starts_with("m=") && in_video { in_video = false; }
        if !in_video { continue; }
        if let Some(rest) = line.strip_prefix("a=fmtp:") {
            let mut parts = rest.split_whitespace();
            let pt = parts.next().unwrap_or_default().to_string();
            let params = parts.next().unwrap_or_default();
            if let Some(apt) = params.split(';').find_map(|entry| entry.trim().strip_prefix("apt=")) {
                if allowed_pts.iter().any(|allowed| allowed == apt) {
                    rtx_by_pt.push((pt, apt.to_string()));
                }
            }
        }
    }
    let mut keep = allowed_pts.clone();
    for (pt, _) in &rtx_by_pt {
        keep.push(pt.clone());
    }
    let mut result = Vec::new();
    in_video = false;
    for line in sdp.lines() {
        if line.starts_with("m=video") {
            in_video = true;
            let mut parts: Vec<_> = line.split_whitespace().map(ToString::to_string).collect();
            if parts.len() > 3 {
                parts.truncate(3);
                parts.extend(keep.iter().cloned());
            }
            result.push(parts.join(" "));
            continue;
        }
        if line.starts_with("m=") && in_video { in_video = false; }
        if in_video && (line.starts_with("a=rtpmap:") || line.starts_with("a=rtcp-fb:") || line.starts_with("a=fmtp:")) {
            let pt = line.split(':').nth(1).unwrap_or_default().split_whitespace().next().unwrap_or_default();
            if !keep.iter().any(|keep_pt| keep_pt == pt) {
                continue;
            }
        }
        result.push(line.to_string());
    }
    result.join("\r\n")
}

pub fn rewrite_h265_offer(sdp: &str) -> String {
    sdp.lines()
        .map(|line| {
            if line.starts_with("a=fmtp:") {
                line.replace("tier-flag=1", "tier-flag=0")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\r\n")
}

pub fn build_nvst_sdp(
    resolution: &str,
    client_viewport_width: u32,
    client_viewport_height: u32,
    fps: u16,
    max_bitrate_mbps: u16,
    codec: &str,
    color_quality: &str,
    partial_reliable_threshold_ms: u16,
    credentials: &IceCredentials,
) -> String {
    let mut parts = resolution.split('x');
    let width = parts.next().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1920);
    let height = parts.next().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1080);
    let max_bitrate_kbps = u32::from(max_bitrate_mbps) * 1000;
    let min_bitrate = max_bitrate_kbps.max(5000) * 35 / 100;
    let initial_bitrate = (max_bitrate_kbps * 70 / 100).max(min_bitrate);
    let is_high_fps = fps >= 90;
    let is_120_fps = fps == 120;
    let is_240_fps = fps >= 240;
    let is_av1 = codec.eq_ignore_ascii_case("AV1");
    let bit_depth = if color_quality.starts_with("10bit") && !codec.eq_ignore_ascii_case("H264") { 10 } else { 8 };
    let mut lines = vec![
        "v=0".to_string(),
        "o=SdpTest test_id_13 14 IN IPv4 127.0.0.1".to_string(),
        "s=-".to_string(),
        "t=0 0".to_string(),
        format!("a=general.icePassword:{}", credentials.pwd),
        format!("a=general.icePasswordV2:{}", credentials.pwd),
        format!("a=general.iceUserNameFragment:{}", credentials.ufrag),
        format!("a=general.iceUserNameFragmentV2:{}", credentials.ufrag),
        format!("a=general.dtlsFingerprint:{}", credentials.fingerprint),
        format!("a=general.dtlsFingerprintV2:{}", credentials.fingerprint),
        "a=general.rtcDataChannelOnNativeBundle:1".to_string(),
        "a=general.rtcAudioOnNativeBundle:1".to_string(),
        "a=general.rtcMicOnNativeBundle:1".to_string(),
        "a=general.rtcVideoOnNativeBundle:1".to_string(),
        "a=general.rtcpOnSctp:1".to_string(),
        "m=video 0 RTP/AVP".to_string(),
        "a=msid:fbc-video-0".to_string(),
        "a=vqos.fec.rateDropWindow:10".to_string(),
        "a=vqos.fec.minRequiredFecPackets:2".to_string(),
        "a=vqos.fec.repairMinPercent:5".to_string(),
        "a=vqos.fec.repairPercent:5".to_string(),
        "a=vqos.fec.repairMaxPercent:35".to_string(),
        "a=vqos.drc.enable:0".to_string(),
        "a=vqos.dfc.enable:0".to_string(),
        "a=video.dx9EnableNv12:1".to_string(),
        "a=video.dx9EnableHdr:1".to_string(),
        "a=vqos.qpg.enable:1".to_string(),
        "a=vqos.resControl.qp.qpg.featureSetting:7".to_string(),
        "a=bwe.useOwdCongestionControl:1".to_string(),
        "a=video.enableRtpNack:1".to_string(),
        "a=vqos.bw.txRxLag.minFeedbackTxDeltaMs:200".to_string(),
        "a=vqos.drc.bitrateIirFilterFactor:18".to_string(),
        "a=video.packetSize:1140".to_string(),
        "a=packetPacing.minNumPacketsPerGroup:15".to_string(),
    ];
    if is_high_fps {
        lines.extend([
            "a=bwe.iirFilterFactor:8".to_string(),
            "a=video.encoderFeatureSetting:47".to_string(),
            "a=video.encoderPreset:6".to_string(),
            "a=vqos.resControl.cpmRtc.badNwSkipFramesCount:600".to_string(),
            "a=vqos.resControl.cpmRtc.decodeTimeThresholdMs:9".to_string(),
            format!("a=video.fbcDynamicFpsGrabTimeoutMs:{}", if is_120_fps { 6 } else { 18 }),
            format!("a=vqos.resControl.cpmRtc.serverResolutionUpdateCoolDownCount:{}", if is_120_fps { 6000 } else { 12000 }),
        ]);
    }
    if is_240_fps {
        lines.extend([
            "a=video.enableNextCaptureMode:1".to_string(),
            "a=vqos.maxStreamFpsEstimate:240".to_string(),
            "a=video.videoSplitEncodeStripsPerFrame:3".to_string(),
            "a=video.updateSplitEncodeStateDynamically:1".to_string(),
        ]);
    }
    lines.extend([
        "a=vqos.adjustStreamingFpsDuringOutOfFocus:1".to_string(),
        "a=vqos.resControl.cpmRtc.ignoreOutOfFocusWindowState:1".to_string(),
        "a=vqos.resControl.perfHistory.rtcIgnoreOutOfFocusWindowState:1".to_string(),
        "a=vqos.resControl.cpmRtc.featureMask:0".to_string(),
        "a=vqos.resControl.cpmRtc.enable:0".to_string(),
        "a=vqos.resControl.cpmRtc.minResolutionPercent:100".to_string(),
        "a=vqos.resControl.cpmRtc.resolutionChangeHoldonMs:999999".to_string(),
        format!("a=packetPacing.numGroups:{}", if is_120_fps { 3 } else { 5 }),
        "a=packetPacing.maxDelayUs:1000".to_string(),
        "a=packetPacing.minNumPacketsFrame:10".to_string(),
        "a=video.rtpNackQueueLength:1024".to_string(),
        "a=video.rtpNackQueueMaxPackets:512".to_string(),
        "a=video.rtpNackMaxPacketCount:25".to_string(),
        "a=vqos.drc.qpMaxResThresholdAdj:4".to_string(),
        "a=vqos.grc.qpMaxResThresholdAdj:4".to_string(),
        "a=vqos.drc.iirFilterFactor:100".to_string(),
    ]);
    if is_av1 {
        lines.extend([
            "a=vqos.drc.minQpHeadroom:20".to_string(),
            "a=vqos.drc.lowerQpThreshold:100".to_string(),
            "a=vqos.drc.upperQpThreshold:200".to_string(),
            "a=vqos.drc.minAdaptiveQpThreshold:180".to_string(),
            "a=vqos.drc.qpCodecThresholdAdj:0".to_string(),
            "a=vqos.drc.qpMaxResThresholdAdj:20".to_string(),
            "a=vqos.dfc.minQpHeadroom:20".to_string(),
            "a=vqos.dfc.qpLowerLimit:100".to_string(),
            "a=vqos.dfc.qpMaxUpperLimit:200".to_string(),
            "a=vqos.dfc.qpMinUpperLimit:180".to_string(),
            "a=vqos.dfc.qpMaxResThresholdAdj:20".to_string(),
            "a=vqos.dfc.qpCodecThresholdAdj:0".to_string(),
            "a=vqos.grc.minQpHeadroom:20".to_string(),
            "a=vqos.grc.lowerQpThreshold:100".to_string(),
            "a=vqos.grc.upperQpThreshold:200".to_string(),
            "a=vqos.grc.minAdaptiveQpThreshold:180".to_string(),
            "a=vqos.grc.qpMaxResThresholdAdj:20".to_string(),
            "a=vqos.grc.qpCodecThresholdAdj:0".to_string(),
            "a=video.minQp:25".to_string(),
            "a=video.enableAv1RcPrecisionFactor:1".to_string(),
        ]);
    }
    lines.extend([
        format!("a=video.clientViewportWd:{client_viewport_width}"),
        format!("a=video.clientViewportHt:{client_viewport_height}"),
        format!("a=video.maxFPS:{fps}"),
        format!("a=video.initialBitrateKbps:{initial_bitrate}"),
        format!("a=video.initialPeakBitrateKbps:{max_bitrate_kbps}"),
        format!("a=vqos.bw.maximumBitrateKbps:{max_bitrate_kbps}"),
        format!("a=vqos.bw.minimumBitrateKbps:{min_bitrate}"),
        format!("a=vqos.bw.peakBitrateKbps:{max_bitrate_kbps}"),
        format!("a=vqos.bw.serverPeakBitrateKbps:{max_bitrate_kbps}"),
        "a=vqos.bw.enableBandwidthEstimation:1".to_string(),
        "a=vqos.bw.disableBitrateLimit:0".to_string(),
        format!("a=vqos.grc.maximumBitrateKbps:{max_bitrate_kbps}"),
        "a=vqos.grc.enable:0".to_string(),
        "a=video.maxNumReferenceFrames:4".to_string(),
        "a=video.mapRtpTimestampsToFrames:1".to_string(),
        "a=video.encoderCscMode:3".to_string(),
        "a=video.dynamicRangeMode:0".to_string(),
        format!("a=video.bitDepth:{bit_depth}"),
        format!("a=video.scalingFeature1:{}", if is_av1 { 1 } else { 0 }),
        "a=video.prefilterParams.prefilterModel:0".to_string(),
        "m=audio 0 RTP/AVP".to_string(),
        "a=msid:audio".to_string(),
        "m=mic 0 RTP/AVP".to_string(),
        "a=msid:mic".to_string(),
        "a=rtpmap:0 PCMU/8000".to_string(),
        "m=application 0 RTP/AVP".to_string(),
        "a=msid:input_1".to_string(),
        format!("a=ri.partialReliableThresholdMs:{partial_reliable_threshold_ms}"),
        "a=ri.enablePartiallyReliableTransferHid:1".to_string(),
        "a=ri.enablePartiallyReliableTransferGamepad:1".to_string(),
        String::new(),
    ]);
    let _ = (width, height);
    lines.join("\n")
}
