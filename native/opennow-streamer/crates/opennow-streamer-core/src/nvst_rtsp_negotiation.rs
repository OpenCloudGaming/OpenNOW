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

#[cfg(test)]
mod tests {
    use super::*;

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
