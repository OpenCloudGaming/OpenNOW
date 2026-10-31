use super::{NvstRtspError, sdp_attribute};
use opennow_streamer_transport::NvstRecoveryPolicy;
use serde_json::{Value, json};
use std::ops::RangeInclusive;

fn ignore_offer(name: &str, value: &str) {
    opennow_streamer_protocol::log::log_line(
        "WARN",
        "transport",
        &format!("ignored-server-offer name={name} value={value}"),
    );
}

pub(super) fn offered_bool(sdp: &str, name: &str) -> Option<bool> {
    let value = sdp_attribute(sdp, name)?;
    match value.as_str() {
        "0" => Some(false),
        "1" => Some(true),
        _ => {
            ignore_offer(name, &value);
            None
        }
    }
}

fn offered_u32(sdp: &str, name: &str) -> Option<u32> {
    let value = sdp_attribute(sdp, name)?;
    value
        .parse()
        .inspect_err(|_| ignore_offer(name, &value))
        .ok()
}

fn offered_in(sdp: &str, name: &str, range: RangeInclusive<u32>) -> Option<u32> {
    let value = offered_u32(sdp, name)?;
    if range.contains(&value) {
        return Some(value);
    }
    ignore_offer(name, &value.to_string());
    None
}

pub(super) fn recovery_policy(sdp: &str) -> NvstRecoveryPolicy {
    NvstRecoveryPolicy {
        ref_pic_invalidation: offered_bool(sdp, "video[0].refPicInvalidation"),
        stream_recovery_type: offered_u32(sdp, "video[0].streamRecoveryType"),
        frames_with_invalid_ref_threshold: offered_u32(
            sdp,
            "video[0].framesWithInvalidRefThreshold",
        ),
        enable_recovery_mode: offered_bool(sdp, "general.enableRecoveryMode"),
        recovery_mode_trigger_threshold_ms: offered_u32(
            sdp,
            "general.recoveryModeTriggerThresholdMs",
        ),
        enable_intra_refresh_in_recovery_mode: offered_bool(
            sdp,
            "general.enableIntraRefreshInRecoveryMode",
        ),
        encoder_ltr_feature_setting: offered_u32(sdp, "video[0].encoderLtrFeatureSetting"),
    }
}

pub(super) fn negotiated_announce(
    announce: &str,
    describe: &str,
    handoff: &mut Value,
) -> Result<String, NvstRtspError> {
    let mut values = serde_json::Map::new();
    for (name, minimum, maximum) in [
        ("vqos[0].fec.enable", 0, 1),
        ("vqos[0].fec.rateDropWindow", 0, u32::MAX),
        ("vqos[0].fec.minRequiredFecPackets", 0, u32::MAX),
        ("vqos[0].fec.repairPercent", 0, 100),
        ("vqos[0].fec.repairMinPercent", 0, 100),
        ("vqos[0].fec.repairMaxPercent", 0, 100),
        ("vqos[0].bllFec.enable", 0, 1),
        ("packetPacing.version", 3, 3),
        ("packetPacing.mode", 1, 1),
        ("packetPacing.numGroups", 1, u32::MAX),
        ("packetPacing.maxDelayUs", 0, u32::MAX),
        ("packetPacing.minNumPacketsFrame", 0, u32::MAX),
        ("packetPacing.minNumPacketsPerGroup", 0, u32::MAX),
        ("packetPacing.enableAccurateSleep", 0, 1),
        ("packetPacing.enableSmoothTransition", 0, 1),
        ("packetPacing.allowFpsBasedToggle", 0, 1),
    ] {
        let value = offered_in(describe, name, minimum..=maximum)
            .or(offered_u32(announce, name))
            .unwrap();
        values.insert(name.to_owned(), json!(value));
    }
    let mut min = values["vqos[0].fec.repairMinPercent"].as_u64().unwrap();
    let mut max = values["vqos[0].fec.repairMaxPercent"].as_u64().unwrap();
    if min > max {
        ignore_offer("vqos[0].fec.repairMinPercent", &format!("{min}>{max}"));
        min = u64::from(offered_u32(announce, "vqos[0].fec.repairMinPercent").unwrap());
        max = u64::from(offered_u32(announce, "vqos[0].fec.repairMaxPercent").unwrap());
        values["vqos[0].fec.repairMinPercent"] = json!(min);
        values["vqos[0].fec.repairMaxPercent"] = json!(max);
    }
    let repair = values["vqos[0].fec.repairPercent"].as_u64().unwrap();
    values["vqos[0].fec.repairPercent"] = json!(repair.clamp(min, max));
    let ceiling = offered_u32(announce, "vqos[0].bw.maximumBitrateKbps").unwrap();
    let initial = offered_in(describe, "video[0].initialBitrateKbps", 1..=u32::MAX)
        .unwrap_or(ceiling)
        .min(ceiling);
    let peak = offered_u32(describe, "video[0].initialPeakBitrateKbps")
        .unwrap_or(ceiling)
        .clamp(initial, ceiling);
    values.insert("video[0].initialBitrateKbps".to_owned(), json!(initial));
    values.insert("video[0].initialPeakBitrateKbps".to_owned(), json!(peak));
    values.insert("vqos[0].bw.maximumBitrateKbps".to_owned(), json!(ceiling));
    opennow_streamer_protocol::log::log_line(
        "INFO",
        "transport",
        &format!("negotiated-encoding={}", Value::Object(values.clone())),
    );
    let mut result = String::with_capacity(announce.len());
    for line in announce.split_inclusive("\r\n") {
        let attribute = line
            .strip_prefix("a=x-nv-")
            .and_then(|line| line.split_once(':'));
        if let Some((name, _)) = attribute
            && let Some(value) = values.get(name)
        {
            result.push_str(&format!("a=x-nv-{name}:{value}\r\n"));
        } else {
            result.push_str(line);
        }
    }
    let result = av1_qp_settings(&result, describe);
    handoff["framePacingFeedbackMode"] =
        json!(super::video_qos_offers(describe).pacing_feedback_mode(&result)?);
    let mut estimator = serde_json::Map::new();
    for (field, attribute) in [
        ("jitterMode", "jitterHistory.jitterEstimationMode"),
        ("jitterN", "jitterHistory.n"),
        ("jitterStddevsX1000", "jitterHistory.stddevsX1000"),
        ("jitterOutlierUs", "jitterHistory.outlierUs"),
        ("jitterMinAdvanceUs", "jitterHistory.minAdvanceUs"),
        ("jitterMaxAdvanceUs", "jitterHistory.maxAdvanceUs"),
        ("renderMode", "renderHistory.renderEstimationMode"),
        ("renderN", "renderHistory.n"),
        ("renderStddevsX1000", "renderHistory.stddevsX1000"),
        ("renderMinTimeUs", "renderHistory.minTimeUs"),
        ("renderMaxTimeUs", "renderHistory.maxTimeUs"),
    ] {
        if let Some(value) = offered_u32(describe, &format!("video[0].framePacing.{attribute}")) {
            estimator.insert(field.to_owned(), json!(value));
        }
    }
    handoff["presentationEstimator"] = Value::Object(estimator);
    Ok(result)
}

/// Writes `a=x-nv-NAME:VALUE` lines into ANNOUNCE. A value replaces a line with
/// the same name; otherwise it is inserted with the session-level attributes
/// before `t=`.
fn set_session_attributes(announce: &str, mut pending: Vec<(String, String)>) -> String {
    let mut result = String::with_capacity(announce.len() + pending.len() * 64);
    for line in announce.split_inclusive("\r\n") {
        if let Some(name) = line
            .strip_prefix("a=x-nv-")
            .and_then(|attribute| attribute.split_once(':'))
            .map(|(name, _)| name)
            && let Some(index) = pending
                .iter()
                .position(|(feature, _)| feature.eq_ignore_ascii_case(name))
        {
            let (feature, value) = pending.remove(index);
            result.push_str(&format!("a=x-nv-{feature}:{value}\r\n"));
            continue;
        }
        if line.starts_with("t=") {
            for (feature, value) in pending.drain(..) {
                result.push_str(&format!("a=x-nv-{feature}:{value}\r\n"));
            }
        }
        result.push_str(line);
    }
    for (feature, value) in pending {
        result.push_str(&format!("a=x-nv-{feature}:{value}\r\n"));
    }
    result
}

/// Largest `drc.upperQpThreshold` the official client treats as still being on
/// the H.264/H.265 QP scale (0..=51).
const H26X_SCALE_UPPER_QP_LIMIT: u32 = 49;
/// Smallest stream, in pixels, that gets the top AV1 adaptive QP limit: the
/// 3440x1440 minimum of level 3 in the official client's resolution-level
/// table.
///
/// The official client compares pixel counts, not width and height, and so does
/// this client: 2560x1600 (4,096,000 px) gets 180, while 3840x1600 (6,144,000
/// px) and 3200x1800 (5,760,000 px, narrower than 3440) get 200. No live
/// session at these sizes has been checked.
const AV1_HIGH_RES_PIXELS: u64 = 3440 * 1440;

/// Moves the host's dynamic quality controls onto the AV1 QP scale.
///
/// DRC, DFC and GRC compare the encoder's QP against thresholds that the
/// server offers on the H.264/H.265 scale (0..=51, e.g. `drc.upperQpThreshold`
/// 39). AV1 QP runs 0..=255, so against those thresholds every AV1 frame looks
/// badly over-quantized and the host keeps stepping quality down. With the
/// save-bandwidth dynamic streaming policy, 10-bit AV1 sessions ended with
/// `0x800e8605` 7-20 s after PLAY and 8-bit AV1 received 70-99 fps instead of
/// 120; see "AV1 QP scale" in `docs/streamer-comparison/rtsp-negotiation.md`.
///
/// The official client rewrites these thresholds in its "Applying default AV1
/// QP settings" step (libBifrost2), when the codec is AV1 and
/// `drc.upperQpThreshold <= 49`. Its config setters name the 18 bytes it
/// writes:
/// - `drc`/`grc`: `minQpHeadroom` 20, `lowerQpThreshold` 100,
///   `upperQpThreshold` 200;
/// - `dfc`: `qpMaxUpperLimit` 200, `qpLowerLimit` 100, `minQpHeadroom` 20;
/// - `drc`/`grc.minAdaptiveQpThreshold` and `dfc.qpMinUpperLimit`: 200 from
///   `AV1_HIGH_RES_PIXELS` up, else 180;
/// - every `qpMaxResThresholdAdj` multiplied by 5 (byte arithmetic), every
///   `qpCodecThresholdAdj` 0.
///
/// All 18 are sent even when one equals the DESCRIBE offer: the official
/// routine writes them unconditionally, re-sending an offered value does not
/// change it, and the full set keeps the applied scale visible in ANNOUNCE and
/// in the log. None of them is in the server-negotiated list that
/// `omit_server_announce_attributes` drops when it matches DESCRIBE.
fn av1_qp_settings(announce: &str, describe: &str) -> String {
    if offered_u32(announce, "vqos[0].bitStreamFormat") != Some(2) {
        return announce.to_owned();
    }
    if let Some(upper) = offered_u32(describe, "vqos[0].drc.upperQpThreshold")
        && upper > H26X_SCALE_UPPER_QP_LIMIT
    {
        opennow_streamer_protocol::log::log_line(
            "INFO",
            "transport",
            &format!("av1-qp-settings skipped drc.upperQpThreshold={upper}"),
        );
        return announce.to_owned();
    }
    let pixels = u64::from(offered_u32(announce, "video[0].clientViewportWd").unwrap_or(0))
        * u64::from(offered_u32(announce, "video[0].clientViewportHt").unwrap_or(0));
    let adaptive: u32 = if pixels >= AV1_HIGH_RES_PIXELS {
        200
    } else {
        180
    };
    let mut pending: Vec<(String, String)> = Vec::new();
    for (control, fixed) in [
        (
            "drc",
            [
                ("minQpHeadroom", 20),
                ("lowerQpThreshold", 100),
                ("upperQpThreshold", 200),
                ("minAdaptiveQpThreshold", adaptive),
                ("qpCodecThresholdAdj", 0),
            ],
        ),
        (
            "dfc",
            [
                ("qpMaxUpperLimit", 200),
                ("qpMinUpperLimit", adaptive),
                ("qpLowerLimit", 100),
                ("qpCodecThresholdAdj", 0),
                ("minQpHeadroom", 20),
            ],
        ),
        (
            "grc",
            [
                ("minQpHeadroom", 20),
                ("lowerQpThreshold", 100),
                ("upperQpThreshold", 200),
                ("minAdaptiveQpThreshold", adaptive),
                ("qpCodecThresholdAdj", 0),
            ],
        ),
    ] {
        for (field, value) in fixed {
            pending.push((format!("vqos[0].{control}.{field}"), value.to_string()));
        }
        // The official client scales its own merged config value. This client
        // only knows the value DESCRIBE offers, so it scales that one and sends
        // nothing when none is offered, leaving the server's default in place.
        let name = format!("vqos[0].{control}.qpMaxResThresholdAdj");
        if let Some(base) = offered_in(describe, &name, 0..=u32::from(u8::MAX)) {
            // Byte arithmetic on purpose: 60 * 5 = 300 wraps to 44, as the
            // official client's u8 field does. Mirror it, do not clamp it.
            let scaled = u8::try_from(base).unwrap_or(u8::MAX).wrapping_mul(5);
            pending.push((name, scaled.to_string()));
        }
    }
    opennow_streamer_protocol::log::log_line(
        "INFO",
        "transport",
        &format!(
            "av1-qp-settings applied=[{}]",
            pending
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join(",")
        ),
    );
    set_session_attributes(announce, pending)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Server baseline QP controls on the H.26x scale, as offered in DESCRIBE
    /// (values match the official client's config for an H.265 session).
    const DESCRIBE_H26X_QP: &str = "v=0\r\n\
        a=x-nv-vqos[0].drc.upperQpThreshold:39\r\n\
        a=x-nv-vqos[0].drc.qpMaxResThresholdAdj:6\r\n\
        a=x-nv-vqos[0].dfc.qpMaxResThresholdAdj:6\r\n\
        a=x-nv-vqos[0].grc.qpMaxResThresholdAdj:4\r\n\
        m=video 5004 RTP/AVP\r\n";

    fn qp_announce(format: u32, width: u32, height: u32) -> String {
        format!(
            "v=0\r\n\
             a=x-nv-video[0].clientViewportWd:{width}\r\n\
             a=x-nv-video[0].clientViewportHt:{height}\r\n\
             a=x-nv-vqos[0].bitStreamFormat:{format}\r\n\
             a=x-nv-vqos[0].drc.enable:0\r\n\
             t=0 0\r\n\
             m=video 5004\r\n"
        )
    }

    fn announced(announce: &str, name: &str) -> Option<String> {
        announce
            .lines()
            .find_map(|line| line.strip_prefix(&format!("a=x-nv-{name}:")))
            .map(str::to_owned)
    }

    #[test]
    fn av1_moves_quality_controls_to_the_av1_qp_scale() {
        let announce = av1_qp_settings(&qp_announce(2, 3840, 2160), DESCRIBE_H26X_QP);
        for (name, value) in [
            ("vqos[0].drc.minQpHeadroom", "20"),
            ("vqos[0].drc.lowerQpThreshold", "100"),
            ("vqos[0].drc.upperQpThreshold", "200"),
            ("vqos[0].drc.minAdaptiveQpThreshold", "200"),
            ("vqos[0].drc.qpMaxResThresholdAdj", "30"),
            ("vqos[0].drc.qpCodecThresholdAdj", "0"),
            ("vqos[0].dfc.qpMaxUpperLimit", "200"),
            ("vqos[0].dfc.qpMinUpperLimit", "200"),
            ("vqos[0].dfc.qpLowerLimit", "100"),
            ("vqos[0].dfc.qpMaxResThresholdAdj", "30"),
            ("vqos[0].dfc.qpCodecThresholdAdj", "0"),
            ("vqos[0].dfc.minQpHeadroom", "20"),
            ("vqos[0].grc.minQpHeadroom", "20"),
            ("vqos[0].grc.lowerQpThreshold", "100"),
            ("vqos[0].grc.upperQpThreshold", "200"),
            ("vqos[0].grc.minAdaptiveQpThreshold", "200"),
            ("vqos[0].grc.qpMaxResThresholdAdj", "20"),
            ("vqos[0].grc.qpCodecThresholdAdj", "0"),
        ] {
            assert_eq!(announced(&announce, name).as_deref(), Some(value), "{name}");
        }
        assert!(
            announce.find("a=x-nv-vqos[0].drc.minQpHeadroom").unwrap()
                < announce.find("t=0 0").unwrap(),
            "QP settings are session-level attributes"
        );
        assert_eq!(
            announced(&announce, "vqos[0].drc.enable").as_deref(),
            Some("0"),
            "unrelated client lines are kept"
        );
    }

    #[test]
    fn session_attributes_replace_a_line_of_the_same_name_or_go_before_t() {
        let announce = "v=0\r\na=x-nv-vqos[0].drc.upperQpThreshold:39\r\nt=0 0\r\nm=video 5004\r\n";
        assert_eq!(
            set_session_attributes(
                announce,
                vec![
                    ("vqos[0].dfc.qpMaxUpperLimit".to_owned(), "200".to_owned()),
                    ("vqos[0].drc.upperQpThreshold".to_owned(), "200".to_owned()),
                ]
            ),
            "v=0\r\n\
             a=x-nv-vqos[0].drc.upperQpThreshold:200\r\n\
             a=x-nv-vqos[0].dfc.qpMaxUpperLimit:200\r\n\
             t=0 0\r\n\
             m=video 5004\r\n"
        );
        assert_eq!(
            set_session_attributes("v=0\r\n", vec![("a.b".to_owned(), "1".to_owned())]),
            "v=0\r\na=x-nv-a.b:1\r\n",
            "without a t= line the attribute is appended"
        );
    }

    #[test]
    fn av1_adaptive_qp_limits_follow_the_top_resolution_level() {
        for (width, height, adaptive) in [
            (3440, 1440, "200"),
            (3440, 1439, "180"),
            (2560, 1440, "180"),
            (1920, 1080, "180"),
            // The official test compares pixel counts. 2560x1600 is below the
            // 3440x1440 count; 3840x1600 is above it.
            (2560, 1600, "180"),
            (3840, 1600, "200"),
            // Narrower than 3440 but with more pixels: a per-axis test would
            // give 180 here.
            (3200, 1800, "200"),
        ] {
            let announce = av1_qp_settings(&qp_announce(2, width, height), DESCRIBE_H26X_QP);
            for name in [
                "vqos[0].drc.minAdaptiveQpThreshold",
                "vqos[0].dfc.qpMinUpperLimit",
                "vqos[0].grc.minAdaptiveQpThreshold",
            ] {
                assert_eq!(
                    announced(&announce, name).as_deref(),
                    Some(adaptive),
                    "{width}x{height} {name}"
                );
            }
            assert_eq!(
                announced(&announce, "vqos[0].dfc.qpMaxUpperLimit").as_deref(),
                Some("200")
            );
        }
    }

    #[test]
    fn av1_qp_settings_leave_other_codecs_and_av1_scale_servers_alone() {
        for format in [0, 1] {
            let announce = qp_announce(format, 3840, 2160);
            assert_eq!(av1_qp_settings(&announce, DESCRIBE_H26X_QP), announce);
        }
        let announce = qp_announce(2, 3840, 2160);
        for upper in [50, 200] {
            let av1_scale = format!("v=0\r\na=x-nv-vqos[0].drc.upperQpThreshold:{upper}\r\n");
            assert_eq!(av1_qp_settings(&announce, &av1_scale), announce, "{upper}");
        }
        // 49 is the last H.26x-scale value; it is still rewritten.
        let boundary = "v=0\r\na=x-nv-vqos[0].drc.upperQpThreshold:49\r\n";
        assert_eq!(
            announced(
                &av1_qp_settings(&announce, boundary),
                "vqos[0].drc.upperQpThreshold"
            )
            .as_deref(),
            Some("200")
        );
    }

    #[test]
    fn av1_qp_settings_scale_only_server_offered_resolution_adjustments() {
        let announce = av1_qp_settings(&qp_announce(2, 3840, 2160), "v=0\r\n");
        assert_eq!(
            announced(&announce, "vqos[0].drc.upperQpThreshold").as_deref(),
            Some("200"),
            "an absent baseline is treated as the H.26x scale"
        );
        for control in ["drc", "dfc", "grc"] {
            assert_eq!(
                announced(
                    &announce,
                    &format!("vqos[0].{control}.qpMaxResThresholdAdj")
                ),
                None,
                "{control}: nothing to scale without a server value"
            );
        }
        // Byte wrap, mirrored from the official client: 60 * 5 = 300 -> 44.
        let wrapped = av1_qp_settings(
            &qp_announce(2, 3840, 2160),
            "v=0\r\na=x-nv-vqos[0].drc.qpMaxResThresholdAdj:60\r\n",
        );
        assert_eq!(
            announced(&wrapped, "vqos[0].drc.qpMaxResThresholdAdj").as_deref(),
            Some("44"),
            "byte arithmetic, as in the official client"
        );
        // A value that does not fit the official byte field is ignored, not scaled.
        let oversized = av1_qp_settings(
            &qp_announce(2, 3840, 2160),
            "v=0\r\na=x-nv-vqos[0].drc.qpMaxResThresholdAdj:256\r\n",
        );
        assert_eq!(
            announced(&oversized, "vqos[0].drc.qpMaxResThresholdAdj"),
            None
        );
    }

    #[test]
    fn announce_body_carries_av1_qp_settings_even_when_offered() {
        let mut announce = qp_announce(2, 3840, 2160);
        for line in [
            "vqos[0].fec.enable:1",
            "vqos[0].fec.rateDropWindow:10",
            "vqos[0].fec.minRequiredFecPackets:2",
            "vqos[0].fec.repairPercent:20",
            "vqos[0].fec.repairMinPercent:20",
            "vqos[0].fec.repairMaxPercent:35",
            "vqos[0].bllFec.enable:0",
            "packetPacing.version:3",
            "packetPacing.mode:1",
            "packetPacing.numGroups:5",
            "packetPacing.maxDelayUs:4000",
            "packetPacing.minNumPacketsFrame:10",
            "packetPacing.minNumPacketsPerGroup:15",
            "packetPacing.enableAccurateSleep:1",
            "packetPacing.enableSmoothTransition:1",
            "packetPacing.allowFpsBasedToggle:1",
            "vqos[0].bw.maximumBitrateKbps:200000",
            "video[0].framePacing.feedbackMode:1",
        ] {
            announce = announce.replacen("t=0 0", &format!("a=x-nv-{line}\r\nt=0 0"), 1);
        }
        // The server already offers two of the AV1 values. They are still sent,
        // and the server-baseline filter that runs after negotiation keeps them.
        let describe = "v=0\r\n\
            a=x-nv-vqos[0].drc.upperQpThreshold:39\r\n\
            a=x-nv-vqos[0].dfc.minQpHeadroom:20\r\n\
            a=x-nv-vqos[0].grc.qpCodecThresholdAdj:0\r\n\
            m=video 5004 RTP/AVP\r\n";
        let effective = negotiated_announce(&announce, describe, &mut json!({})).unwrap();
        let body = super::super::omit_server_announce_attributes(&effective, describe);
        for (name, value) in [
            ("vqos[0].drc.upperQpThreshold", "200"),
            ("vqos[0].dfc.qpMaxUpperLimit", "200"),
            ("vqos[0].dfc.minQpHeadroom", "20"),
            ("vqos[0].grc.qpCodecThresholdAdj", "0"),
        ] {
            assert_eq!(announced(&body, name).as_deref(), Some(value), "{name}");
        }
    }

    #[test]
    fn recovery_describe_is_typed_and_does_not_guess_defaults() {
        let policy = recovery_policy(
            "a=x-nv-video[0].refPicInvalidation:1\r\na=x-nv-video[0].streamRecoveryType:2\r\na=x-nv-video[0].framesWithInvalidRefThreshold:4\r\na=x-nv-general.enableRecoveryMode:1\r\na=x-nv-general.recoveryModeTriggerThresholdMs:6000\r\na=x-nv-general.enableIntraRefreshInRecoveryMode:1\r\na=x-nv-video[0].encoderLtrFeatureSetting:2\r\n",
        );
        assert_eq!(policy.ref_pic_invalidation, Some(true));
        assert_eq!(policy.stream_recovery_type, Some(2));
        assert_eq!(policy.frames_with_invalid_ref_threshold, Some(4));
        assert_eq!(policy.recovery_mode_trigger_threshold_ms, Some(6000));
        assert_eq!(policy.encoder_ltr_feature_setting, Some(2));
        assert_eq!(recovery_policy(""), NvstRecoveryPolicy::default());
        assert_eq!(
            recovery_policy(
                "a=x-nv-video[0].refPicInvalidation:2\r\na=x-nv-video[0].framesWithInvalidRefThreshold:-1\r\n"
            ),
            NvstRecoveryPolicy::default()
        );
    }
}
