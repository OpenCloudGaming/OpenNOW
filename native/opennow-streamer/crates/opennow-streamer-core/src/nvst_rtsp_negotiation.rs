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
    let result = forward_server_features(&result, describe);
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

/// Server feature attributes that ANNOUNCE returns with the server's own value.
///
/// DESCRIBE answers with `main ;; features || upstream`, and the `;;` section is
/// separately signed (`o=NvStreamer`, `k=HMAC:`). A key is listed here only when
/// returning the server's signed value has a measured effect. No value is
/// invented: a key the server does not sign in `;;` is not sent.
/// - `video[0].videoSplitEncodeStripsPerFrame` (split-frame encoding on the
///   host): 3840x2160@120 sessions stayed at about 96 fps without it and
///   reached 120 fps with it (docs/streamer-comparison/rtsp-negotiation.md).
///
/// These keys must stay out of `SERVER_NEGOTIATED`. That list drops a value
/// equal to the offer, and a returned value always equals the signed offer.
const FORWARDED_SERVER_FEATURES: &[&str] = &["video[0].videoSplitEncodeStripsPerFrame"];

fn forwards_server_feature(name: &str) -> bool {
    FORWARDED_SERVER_FEATURES
        .iter()
        .any(|forwarded| forwarded.eq_ignore_ascii_case(name))
}

/// `a=x-nv-NAME:VALUE` attributes from the DESCRIBE features section, in order.
fn describe_feature_attributes(describe: &str) -> Vec<(String, String)> {
    let Some((_, features)) = describe
        .split("||")
        .next()
        .unwrap_or_default()
        .split_once(";;")
    else {
        return Vec::new();
    };
    features
        .split(";;")
        .flat_map(str::lines)
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("a=x-nv-"))
        .filter_map(|attribute| attribute.split_once(':'))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .filter(|(name, value)| !name.is_empty() && !value.is_empty())
        .collect()
}

/// Returns the allowlisted server features in ANNOUNCE with their signed `;;`
/// values. The last value of a key wins, as in `sdp_attribute`; a value that is
/// not an unsigned integer is logged as ignored and nothing is sent for it.
fn forward_server_features(announce: &str, describe: &str) -> String {
    let mut pending: Vec<(String, String)> = Vec::new();
    for (name, value) in describe_feature_attributes(describe) {
        if !forwards_server_feature(&name) {
            continue;
        }
        pending.retain(|(existing, _)| !existing.eq_ignore_ascii_case(&name));
        match value.parse::<u32>() {
            Ok(value) => pending.push((name, value.to_string())),
            Err(_) => ignore_offer(&name, &value),
        }
    }
    if pending.is_empty() {
        return announce.to_owned();
    }
    opennow_streamer_protocol::log::log_line(
        "INFO",
        "transport",
        &format!(
            "server-features returned=[{}]",
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

    /// Shape of a live DESCRIBE body: `main ;; signed features || upstream`.
    const DESCRIBE_WITH_FEATURES: &str = "v=0\r\n\
        a=x-nv-video[0].videoSplitEncodeStripsPerFrame:1\r\n\
        a=x-nv-general.maxQosMessagesSize:1071\r\n\
        m=video 5004 RTP/AVP\r\n\
        ;;v=0\r\n\
        o=NvStreamer 1 14 IN IPv4 host\r\n\
        k=HMAC:00\r\n\
        a=x-nv-general.customMessageOnCC:1 \r\n\
        a=x-nv-video[0].vbvMultiplier:109 \r\n\
        a=x-nv-video[0].videoSplitEncodeStripsPerFrame:64 \r\n\
        a=x-nv-general.dtlsFingerprint:AA:BB\r\n\
        t=0 0\r\n\
        m=video 0 RTP/AVP\r\n\
        ||v=0\r\n\
        a=x-nv-video[0].videoSplitEncodeStripsPerFrame:2\r\n";

    const ANNOUNCE: &str = "v=0\r\n\
        a=x-nv-video[0].maxFPS:120\r\n\
        t=0 0\r\n\
        m=video 5004\r\n";

    #[test]
    fn feature_attributes_come_only_from_the_signed_features_section() {
        let features = describe_feature_attributes(DESCRIBE_WITH_FEATURES);
        let names: Vec<_> = features.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            [
                "general.customMessageOnCC",
                "video[0].vbvMultiplier",
                "video[0].videoSplitEncodeStripsPerFrame",
                "general.dtlsFingerprint",
            ]
        );
        assert_eq!(
            features[2].1, "64",
            "values are trimmed and taken from features, not main or upstream"
        );
        assert!(
            describe_feature_attributes(
                "v=0\r\na=x-nv-video[0].videoSplitEncodeStripsPerFrame:64\r\n"
            )
            .is_empty()
        );
        assert!(
            describe_feature_attributes(
                "v=0\r\n||;;a=x-nv-video[0].videoSplitEncodeStripsPerFrame:64\r\n"
            )
            .is_empty()
        );
    }

    #[test]
    fn split_encode_feature_is_returned_with_session_attributes() {
        assert_eq!(
            forward_server_features(ANNOUNCE, DESCRIBE_WITH_FEATURES),
            "v=0\r\n\
             a=x-nv-video[0].maxFPS:120\r\n\
             a=x-nv-video[0].videoSplitEncodeStripsPerFrame:64\r\n\
             t=0 0\r\n\
             m=video 5004\r\n"
        );
    }

    #[test]
    fn returned_feature_replaces_a_client_line_with_the_same_name() {
        let announce = "v=0\r\na=x-nv-video[0].videoSplitEncodeStripsPerFrame:0\r\nt=0 0\r\n";
        assert_eq!(
            forward_server_features(announce, DESCRIBE_WITH_FEATURES),
            "v=0\r\na=x-nv-video[0].videoSplitEncodeStripsPerFrame:64\r\nt=0 0\r\n"
        );
    }

    #[test]
    fn unlisted_and_credential_features_are_not_returned() {
        let announce = forward_server_features(ANNOUNCE, DESCRIBE_WITH_FEATURES);
        for withheld in [
            "customMessageOnCC",
            "vbvMultiplier",
            "dtlsFingerprint",
            "HMAC",
        ] {
            assert!(
                !announce.contains(withheld),
                "{withheld} must not be returned"
            );
        }
    }

    #[test]
    fn announce_is_unchanged_without_a_features_section() {
        for describe in [
            "",
            "v=0\r\na=x-nv-video[0].videoSplitEncodeStripsPerFrame:64\r\n",
        ] {
            assert_eq!(forward_server_features(ANNOUNCE, describe), ANNOUNCE);
        }
    }

    #[test]
    fn only_a_last_signed_unsigned_integer_is_returned() {
        let feature = "video[0].videoSplitEncodeStripsPerFrame";
        for (first, last, expected) in [
            ("1", "64", Some("64")),
            ("64", "auto", None),
            ("64", "-1", None),
            ("auto", "64", Some("64")),
        ] {
            let describe =
                format!("v=0\r\n;;v=0\r\na=x-nv-{feature}:{first}\r\na=x-nv-{feature}:{last}\r\n");
            let announce = forward_server_features(ANNOUNCE, &describe);
            assert_eq!(
                sdp_attribute(&announce, feature).as_deref(),
                expected,
                "{first} then {last}"
            );
            assert_eq!(
                announce.matches(feature).count(),
                usize::from(expected.is_some())
            );
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
