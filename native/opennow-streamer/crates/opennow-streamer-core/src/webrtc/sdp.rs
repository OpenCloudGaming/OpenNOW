use opennow_streamer_platform::MediaStreamConfig;

use super::Failure;

pub(super) fn partial_reliability(offer: &str) -> Result<u16, Failure> {
    let value = attribute(offer, "ri.partialReliableThresholdMs").unwrap_or("30");
    value
        .parse::<u16>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| Failure::signaling("Invalid input partial reliability threshold"))
}

fn attribute<'a>(sdp: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("a={name}:");
    sdp.lines()
        .find_map(|line| line.strip_prefix(&prefix).map(str::trim))
}

fn input_mask(offer: &str, name: &str, fallback: u32) -> Result<u32, Failure> {
    match attribute(offer, name) {
        None => Ok(fallback),
        Some(value) => {
            let parsed = if let Some(hex) = value.strip_prefix("0x") {
                u32::from_str_radix(hex, 16).ok()
            } else if value == "-1" {
                Some(u32::MAX)
            } else {
                value.parse::<u32>().ok()
            };
            parsed.ok_or_else(|| Failure::signaling("Invalid input capability mask"))
        }
    }
}

pub(super) fn nvst_answer(
    offer: &str,
    answer: &str,
    stream: MediaStreamConfig,
) -> Result<String, Failure> {
    let ufrag = attribute(answer, "ice-ufrag").filter(|value| !value.is_empty());
    let password = attribute(answer, "ice-pwd").filter(|value| !value.is_empty());
    let fingerprint = answer
        .lines()
        .find_map(|line| line.strip_prefix("a=fingerprint:sha-256 "))
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let (Some(ufrag), Some(password), Some(fingerprint)) = (ufrag, password, fingerprint) else {
        return Err(Failure::signaling(
            "Local WebRTC answer lacks ICE credentials or SHA-256 fingerprint",
        ));
    };
    let threshold = partial_reliability(offer)?;
    let hid = input_mask(offer, "ri.hidDeviceMask", u32::MAX)?;
    let partial_hid = input_mask(offer, "ri.enablePartiallyReliableTransferHid", hid)?;
    let partial_gamepad = input_mask(offer, "ri.enablePartiallyReliableTransferGamepad", 15)?;
    let maximum = (stream.bitrate_bps / 1000).clamp(1000, 200_000);
    let minimum = maximum.min(5000);
    let initial = minimum.max(maximum / 4);
    let frame_time = (950_000 / stream.fps.max(1)).max(1000);
    let mut lines = vec![
        "v=0".to_owned(),
        "o=SdpTest test_id_13 14 IN IPv4 127.0.0.1".to_owned(),
        "s=-".to_owned(),
        "t=0 0".to_owned(),
        format!("a=general.icePassword:{password}"),
        format!("a=general.iceUserNameFragment:{ufrag}"),
        format!("a=general.dtlsFingerprint:{fingerprint}"),
    ];
    lines.extend(
        include_str!("video_attributes.sdp")
            .lines()
            .map(str::to_owned),
    );
    lines.push(format!(
        "a=video.framePacing.pid.minTargetFrameTimeUs:{frame_time}"
    ));
    if stream.fps > 60 {
        lines.extend([
            "a=vqos.resControl.dfc.useClientFpsPerf:0".to_owned(),
            "a=bwe.iirFilterFactor:8".to_owned(),
            "a=video.encoderFeatureSetting:47".to_owned(),
            "a=video.encoderPreset:6".to_owned(),
            format!("a=vqos.maxStreamFpsEstimate:{}", stream.fps),
        ]);
        if let Some((grab, decode)) = match stream.fps {
            90 => Some((9, 11)),
            120 => Some((6, 9)),
            240.. => Some((18, 9)),
            _ => None,
        } {
            lines.push(format!("a=video.fbcDynamicFpsGrabTimeoutMs:{grab}"));
            lines.push(format!(
                "a=vqos.resControl.cpmRtc.decodeTimeThresholdMs:{decode}"
            ));
        }
    }
    if stream.fps >= 240 {
        lines.extend(
            [
                "a=video.enableNextCaptureMode:1",
                "a=video.videoSplitEncodeStripsPerFrame:3",
                "a=video.updateSplitEncodeStateDynamically:1",
                "a=vqos.rtcPreemptiveIdrSettings.minBurstNackSize:65535",
                "a=vqos.rtcPreemptiveIdrSettings.minNackPacketCaptureAgeMs:65535",
            ]
            .map(str::to_owned),
        );
    }
    lines.extend([
        format!(
            "a=packetPacing.numGroups:{}",
            if stream.fps == 120 { 3 } else { 5 }
        ),
        format!("a=video.clientViewportWd:{}", stream.width),
        format!("a=video.clientViewportHt:{}", stream.height),
        format!("a=video.maxFPS:{}", stream.fps),
        format!("a=video.initialBitrateKbps:{initial}"),
        format!("a=video.initialPeakBitrateKbps:{initial}"),
        format!("a=vqos.bw.maximumBitrateKbps:{maximum}"),
        format!("a=vqos.bw.minimumBitrateKbps:{minimum}"),
        format!("a=vqos.bw.peakBitrateKbps:{maximum}"),
        format!("a=vqos.bw.serverPeakBitrateKbps:{maximum}"),
        format!("a=vqos.grc.maximumBitrateKbps:{maximum}"),
        "m=audio 0 RTP/AVP".to_owned(),
        "a=msid:audio".to_owned(),
        "m=application 0 RTP/AVP".to_owned(),
        "a=msid:input_1".to_owned(),
        format!("a=ri.partialReliableThresholdMs:{threshold}"),
        format!("a=ri.hidDeviceMask:{hid}"),
        format!("a=ri.enablePartiallyReliableTransferGamepad:{partial_gamepad}"),
        format!("a=ri.enablePartiallyReliableTransferHid:{partial_hid}"),
        String::new(),
    ]);
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answer_uses_local_credentials_and_echoes_input_masks() {
        let answer = "v=0\r\na=ice-ufrag:local\r\na=ice-pwd:local-password\r\na=fingerprint:sha-256 AA:BB\r\n";
        let offer = "a=ice-ufrag:remote\na=ri.partialReliableThresholdMs:44\na=ri.hidDeviceMask:0xffffffff\na=ri.enablePartiallyReliableTransferHid:128\na=ri.enablePartiallyReliableTransferGamepad:3\n";
        let sdp = nvst_answer(offer, answer, MediaStreamConfig::default()).unwrap();
        for expected in [
            "a=general.iceUserNameFragment:local",
            "a=general.icePassword:local-password",
            "a=general.dtlsFingerprint:AA:BB",
            "a=ri.partialReliableThresholdMs:44",
            "a=ri.hidDeviceMask:4294967295",
            "a=ri.enablePartiallyReliableTransferHid:128",
            "a=ri.enablePartiallyReliableTransferGamepad:3",
            "a=video.bitDepth:8",
            "a=video.dynamicRangeMode:0",
        ] {
            assert!(
                sdp.lines().any(|line| line == expected),
                "missing {expected}"
            );
        }
        assert!(!sdp.contains("remote"));
        assert!(!sdp.contains("m=mic"));
    }

    #[test]
    fn missing_credentials_and_invalid_input_limits_are_rejected() {
        assert!(nvst_answer("", "v=0", MediaStreamConfig::default()).is_err());
        assert!(partial_reliability("a=ri.partialReliableThresholdMs:65536").is_err());
        assert!(partial_reliability("a=ri.partialReliableThresholdMs:0").is_err());
        assert_eq!(partial_reliability("").unwrap(), 30);
    }
}
