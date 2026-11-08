use serde_json::{Value, json};

pub const SUPPORTED_FRAME_RATES: [i64; 8] = [30, 60, 90, 120, 144, 165, 240, 360];

pub fn snap_frame_rate(requested: i64) -> i64 {
    if requested == 59 {
        return 60;
    }
    SUPPORTED_FRAME_RATES
        .into_iter()
        .rev()
        .find(|rate| *rate <= requested)
        .unwrap_or(30)
}

pub fn resolution_ceiling(width: i64, height: i64) -> i64 {
    if width == 1920 && matches!(height, 1080 | 1200) {
        360
    } else {
        240
    }
}

fn is_full_hd(width: i64, height: i64) -> bool {
    resolution_ceiling(width, height) > 240
}

fn resolution(settings: &Value) -> (i64, i64) {
    settings["resolution"]
        .as_str()
        .and_then(|value| value.split_once(['x', 'X']))
        .and_then(|(width, height)| Some((width.parse().ok()?, height.parse().ok()?)))
        .unwrap_or((1920, 1080))
}

fn hardware_decode_available(settings: &Value, capabilities: &Value) -> bool {
    let requested_backend = crate::streamer::requested_embedded_backend(settings);
    let requested_codec = settings["codec"]
        .as_str()
        .unwrap_or("auto")
        .trim()
        .to_ascii_lowercase();
    let explicit_codec = match requested_codec.as_str() {
        "" | "auto" => None,
        value => Some(value),
    };
    capabilities["videoBackends"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|backend| backend["available"].as_bool().unwrap_or(false))
        .filter(|backend| {
            let name = backend["backend"].as_str().unwrap_or("");
            !matches!(name, "software" | "ffmpeg")
                && (requested_backend == "auto"
                    || requested_backend == name
                    || (requested_backend == "nvdec" && name == "cuda"))
        })
        .flat_map(|backend| backend["codecs"].as_array().into_iter().flatten())
        .any(|entry| {
            if entry["available"].as_bool() != Some(true) {
                return false;
            }
            let name = entry["codec"].as_str().unwrap_or("");
            match &explicit_codec {
                Some(codec) => name.eq_ignore_ascii_case(codec),
                None => ["h264", "h265", "av1"]
                    .iter()
                    .any(|codec| name.eq_ignore_ascii_case(codec)),
            }
        })
}

const BASE_FRAME_RATE_CEILING: i64 = 240;

pub fn request_frame_rate(settings: &Value, params: &Value, width: i64, height: i64) -> i64 {
    let requested = settings["fps"].as_i64().unwrap_or(60).clamp(30, 360);
    let mut rate = requested.min(resolution_ceiling(width, height));
    if rate > BASE_FRAME_RATE_CEILING {
        let capabilities = &params["runtimeCapabilities"];
        let entitled = params["maxEntitledFps"].as_i64().unwrap_or(0).clamp(0, 360);
        let cap = if entitled > 0 {
            entitled.min(BASE_FRAME_RATE_CEILING)
        } else {
            BASE_FRAME_RATE_CEILING
        };
        if !hardware_decode_available(settings, capabilities) || entitled < rate {
            rate = cap;
        }
    }
    snap_frame_rate(rate)
}

pub fn frame_rate_choices(settings: &Value, capabilities: &Value) -> Value {
    let (width, height) = resolution(settings);
    let full_hd = is_full_hd(width, height);
    let hardware = hardware_decode_available(settings, capabilities);
    json!(
        SUPPORTED_FRAME_RATES
            .into_iter()
            .map(|value| {
                let reason = if value <= BASE_FRAME_RATE_CEILING {
                    None
                } else if !full_hd {
                    Some(
                        "360 FPS is offered at full HD (1920x1080 or 1920x1200) only. Choose a full HD resolution first."
                            .to_owned(),
                    )
                } else if capabilities["videoBackends"].as_array().is_none() {
                    Some(
                        "360 FPS needs a confirmed hardware video decoder for the selected codec. This device has not reported one."
                            .to_owned(),
                    )
                } else if !hardware {
                    Some(
                        "360 FPS needs a hardware video decoder for the selected codec. This device has none available."
                            .to_owned(),
                    )
                } else {
                    None
                };
                json!({"value":value, "disabled":reason.is_some(), "reason":reason})
            })
            .collect::<Vec<_>>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capabilities(backend: &str, codec: &str) -> Value {
        json!({"protocolVersion":8, "videoBackends":[{"backend":backend, "platform":"linux",
            "available":true, "codecs":[{"codec":codec, "available":true,
                "colorQualities":["8bit_420"]}]}]})
    }

    fn descriptor(choices: &Value, value: i64) -> Value {
        choices
            .as_array()
            .unwrap()
            .iter()
            .find(|choice| choice["value"] == value)
            .cloned()
            .unwrap()
    }

    #[test]
    fn supported_frame_rates_end_at_the_documented_top_tier() {
        assert_eq!(SUPPORTED_FRAME_RATES, [30, 60, 90, 120, 144, 165, 240, 360]);
        assert_eq!(SUPPORTED_FRAME_RATES.last(), Some(&360));
    }

    #[test]
    fn frame_rates_snap_to_supported_modes_before_requesting_allocation() {
        for (requested, expected) in [
            (0, 30),
            (58, 30),
            (59, 60),
            (89, 60),
            (90, 90),
            (119, 90),
            (120, 120),
            (143, 120),
            (144, 144),
            (164, 144),
            (165, 165),
            (239, 165),
            (240, 240),
            (359, 240),
            (360, 360),
            (999, 360),
        ] {
            assert_eq!(snap_frame_rate(requested), expected);
        }
        assert_eq!(
            request_frame_rate(&json!({"fps":164,"codec":"h265"}), &json!({}), 1920, 1080),
            144
        );
    }

    #[test]
    fn resolution_ceiling_is_documented_full_hd_only() {
        assert_eq!(resolution_ceiling(1920, 1080), 360);
        assert_eq!(resolution_ceiling(1920, 1200), 360);
        for (width, height) in [
            (1280, 720),
            (1600, 900),
            (1920, 1440),
            (2560, 1440),
            (2560, 1600),
            (3440, 1440),
            (3840, 2160),
            (5120, 1440),
            (3840, 1080),
            (2560, 1080),
        ] {
            assert_eq!(
                resolution_ceiling(width, height),
                240,
                "{width}x{height} must not request the full-HD-only tier"
            );
        }
    }

    #[test]
    fn only_the_top_tier_is_conditional_and_every_rate_reports_a_reason() {
        let settings =
            json!({"resolution":"1920x1080", "codec":"h265", "nativeVideoBackend":"auto"});
        let choices = frame_rate_choices(&settings, &capabilities("vaapi", "h265"));
        assert_eq!(
            choices.as_array().unwrap().len(),
            SUPPORTED_FRAME_RATES.len()
        );
        for rate in [30, 60, 90, 120, 144, 165, 240] {
            let entry = descriptor(&choices, rate);
            assert_eq!(entry["disabled"], false, "{rate} stays selectable");
            assert_eq!(entry["reason"], Value::Null, "{rate} needs no reason");
        }
        assert_eq!(descriptor(&choices, 360)["disabled"], false);

        let non_full_hd = frame_rate_choices(
            &json!({"resolution":"2560x1440", "codec":"h265", "nativeVideoBackend":"auto"}),
            &capabilities("vaapi", "h265"),
        );
        let entry = descriptor(&non_full_hd, 360);
        assert_eq!(entry["disabled"], true);
        assert!(
            entry["reason"]
                .as_str()
                .unwrap()
                .contains("full HD (1920x1080 or 1920x1200) only")
        );
        assert_eq!(descriptor(&non_full_hd, 240)["disabled"], false);
    }

    #[test]
    fn top_tier_requires_a_hardware_decoder_for_the_selected_codec() {
        let full_hd = json!({"resolution":"1920x1200", "codec":"av1", "nativeVideoBackend":"auto"});
        let software_only = json!({"videoBackends":[{"backend":"software", "available":true,
            "codecs":[{"codec":"av1", "available":true}]}]});
        let entry = descriptor(&frame_rate_choices(&full_hd, &software_only), 360);
        assert_eq!(entry["disabled"], true);
        assert!(
            entry["reason"]
                .as_str()
                .unwrap()
                .contains("needs a hardware video decoder")
        );

        let mismatched_codec = capabilities("vaapi", "h265");
        assert_eq!(
            descriptor(&frame_rate_choices(&full_hd, &mismatched_codec), 360)["disabled"],
            true,
            "an unrelated hardware decoder must not unlock the top tier"
        );

        let explicit_backend =
            json!({"resolution":"1920x1200", "codec":"h264", "nativeVideoBackend":"cuda"});
        assert_eq!(
            descriptor(
                &frame_rate_choices(&explicit_backend, &capabilities("vaapi", "h264")),
                360
            )["disabled"],
            true,
            "a backend the preference excludes cannot unlock the top tier"
        );
        let nvdec_alias =
            json!({"resolution":"1920x1200", "codec":"av1", "nativeVideoBackend":"nvdec"});
        assert_eq!(
            descriptor(
                &frame_rate_choices(&nvdec_alias, &capabilities("cuda", "av1")),
                360
            )["disabled"],
            false,
            "the nvdec alias still matches the CUDA backend"
        );

        let unavailable = json!({"videoBackends":[{"backend":"vaapi", "available":false,
            "codecs":[{"codec":"h265", "available":true}]}]});
        assert_eq!(
            descriptor(&frame_rate_choices(&full_hd, &unavailable), 360)["disabled"],
            true,
            "an unavailable backend cannot unlock the top tier"
        );
        assert_eq!(
            descriptor(&frame_rate_choices(&full_hd, &json!({})), 360)["disabled"],
            true,
            "unreported capabilities never unlock the top tier"
        );
    }

    #[test]
    fn request_rate_applies_the_resolution_device_and_entitlement_ceiling() {
        let hardware = capabilities("vaapi", "h265");
        let software = json!({"videoBackends":[{"backend":"software", "available":true,
            "codecs":[{"codec":"h265", "available":true}]}]});
        let settings = json!({"resolution":"1920x1080", "fps":360, "codec":"h265",
            "nativeVideoBackend":"auto"});
        let request = |capabilities: &Value, entitled: i64, width: i64, height: i64| {
            request_frame_rate(
                &settings,
                &json!({"runtimeCapabilities":capabilities, "maxEntitledFps":entitled}),
                width,
                height,
            )
        };
        assert_eq!(request(&hardware, 360, 1920, 1080), 360);
        assert_eq!(
            request(&software, 360, 1920, 1080),
            240,
            "a reported software-only probe cannot request the top tier"
        );
        assert_eq!(
            request(&json!({}), 360, 1920, 1080),
            240,
            "an unreported probe is not affirmative capability"
        );
        assert_eq!(
            request(&json!({"videoBackends":[]}), 360, 1920, 1080),
            240,
            "a probe that reported no backend cannot request the top tier"
        );
        assert_eq!(
            request(&hardware, 0, 1920, 1080),
            240,
            "unconfirmed entitlement cannot request the top tier"
        );
        assert_eq!(
            request(&hardware, 240, 1920, 1080),
            240,
            "a 240 FPS entitlement cannot request the top tier"
        );
        assert_eq!(
            request(&hardware, 120, 1920, 1080),
            120,
            "a lower entitlement bounds the request"
        );
        assert_eq!(
            request(&hardware, 360, 2560, 1440),
            240,
            "the resolution ceiling applies regardless of entitlement"
        );
        assert_eq!(
            request_frame_rate(
                &json!({"resolution":"1920x1080", "fps":999, "codec":"h265"}),
                &json!({"runtimeCapabilities":hardware, "maxEntitledFps":360}),
                1920,
                1080
            ),
            360,
            "a stored value above the ceiling is still bounded"
        );
        assert_eq!(
            request_frame_rate(
                &json!({"resolution":"1920x1080", "fps":120, "codec":"h265"}),
                &json!({"runtimeCapabilities":software, "maxEntitledFps":0}),
                1920,
                1080
            ),
            120,
            "base rates ignore the capability and entitlement verdicts"
        );
    }

    #[test]
    fn legacy_software_preference_never_qualifies_for_the_top_tier() {
        let hardware = capabilities("vaapi", "h264");
        let settings = json!({"resolution":"1920x1080", "fps":360, "codec":"h264",
            "nativeVideoBackend":"auto", "decoderPreference":"software"});
        let params = json!({"runtimeCapabilities":hardware, "maxEntitledFps":360});
        assert_eq!(
            request_frame_rate(&settings, &params, 1920, 1080),
            240,
            "the legacy software preference is a software decode path"
        );
        assert_eq!(
            request_frame_rate(
                &json!({"resolution":"1920x1080", "fps":360, "codec":"h264",
                    "nativeVideoBackend":"auto", "decoderPreference":"auto"}),
                &params,
                1920,
                1080
            ),
            360,
            "the same hardware profile stays eligible without the software preference"
        );
    }

    #[test]
    fn auto_selection_accepts_any_hardware_codec_without_mutating_inputs() {
        let settings =
            json!({"resolution":"1920x1080", "codec":"auto", "nativeVideoBackend":"auto"});
        let original = settings.clone();
        let choices = frame_rate_choices(&settings, &capabilities("vulkan", "h265"));
        assert_eq!(descriptor(&choices, 360)["disabled"], false);
        assert_eq!(settings, original);

        let missing = frame_rate_choices(&settings, &json!({}));
        assert_eq!(descriptor(&missing, 240)["disabled"], false);
        assert_eq!(descriptor(&missing, 360)["disabled"], true);
    }
}
