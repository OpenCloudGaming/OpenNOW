use super::*;

fn stream(color_quality: MediaColorQuality, hdr: bool) -> MediaStreamConfig {
    MediaStreamConfig {
        codec: MediaVideoCodec::H265,
        color_quality,
        hdr,
        width: 2560,
        height: 1440,
        fps: 120,
        ..MediaStreamConfig::default()
    }
}

#[test]
fn official_wire_values_cover_each_depth_chroma_and_hdr_combination() {
    for (quality, depth, chroma) in [
        (MediaColorQuality::EightBit420, 8, 0),
        (MediaColorQuality::EightBit444, 8, 1),
        (MediaColorQuality::TenBit420, 10, 0),
        (MediaColorQuality::TenBit444, 10, 1),
    ] {
        for hdr in [false, true] {
            if hdr && depth == 8 {
                continue;
            }
            let requested = stream(quality, hdr);
            let resolved = NvstColorNegotiation::resolve(requested, "").unwrap();
            assert_eq!(resolved.stream, requested);
            assert_eq!(
                resolved.announce_lines(),
                vec![
                    format!("a=x-nv-video[0].bitDepth:{depth}"),
                    format!("a=x-nv-video[0].chromaFormat:{chroma}"),
                    format!("a=x-nv-video[0].dynamicRangeMode:{}", u8::from(hdr)),
                ]
            );
        }
    }
}

#[test]
fn baseline_is_for_comparison_not_an_override_of_the_selected_format() {
    let requested = stream(MediaColorQuality::TenBit444, false);
    let resolved = NvstColorNegotiation::resolve(
        requested,
        "a=x-nv-video[0].bitDepth:8\r\na=x-nv-video[0].chromaFormat:0\r\na=x-nv-video[0].dynamicRangeMode:1\r\n",
    )
    .unwrap();
    assert_eq!(resolved.stream, requested);
    assert_eq!(
        resolved.announce_lines(),
        vec![
            "a=x-nv-video[0].bitDepth:10",
            "a=x-nv-video[0].chromaFormat:1",
            "a=x-nv-video[0].dynamicRangeMode:0",
        ]
    );
}

#[test]
fn recognized_server_configuration_suppresses_default_equal_color_fields() {
    let requested = stream(MediaColorQuality::EightBit420, false);
    for describe in [
        "a=x-nv-general.nativeRtcOnBundlePort:1\r\n",
        "a=x-nv-video[0].bitDepth:8\r\n",
        ";;a=x-nv-video[0].chromaFormat:0\r\n",
    ] {
        let resolved = NvstColorNegotiation::resolve(requested, describe).unwrap();
        assert!(resolved.announce_lines().is_empty(), "{describe}");
        assert_eq!(resolved.stream, requested);
    }
    let resolved = NvstColorNegotiation::resolve(
        stream(MediaColorQuality::TenBit420, false),
        "a=x-nv-general.nativeRtcOnBundlePort:1\r\n",
    )
    .unwrap();
    assert_eq!(
        resolved.announce_lines(),
        vec!["a=x-nv-video[0].bitDepth:10"]
    );
}

#[test]
fn nondefault_server_baseline_is_suppressed_only_when_it_matches() {
    let describe = "a=x-nv-video[0].bitDepth:10\r\na=x-nv-video[0].chromaFormat:1\r\na=x-nv-video[0].dynamicRangeMode:1\r\n";
    let resolved =
        NvstColorNegotiation::resolve(stream(MediaColorQuality::TenBit444, true), describe)
            .unwrap();
    assert!(resolved.announce_lines().is_empty());
    let resolved =
        NvstColorNegotiation::resolve(stream(MediaColorQuality::EightBit420, false), describe)
            .unwrap();
    assert_eq!(
        resolved.announce_lines(),
        vec![
            "a=x-nv-video[0].bitDepth:8",
            "a=x-nv-video[0].chromaFormat:0",
            "a=x-nv-video[0].dynamicRangeMode:0",
        ]
    );
}

#[test]
fn suffix_overrides_both_the_wire_format_and_the_decoder_configuration() {
    for (requested, suffix, expected) in [
        (
            stream(MediaColorQuality::TenBit444, true),
            "a=x-nv-video[0].bitDepth:8\r\na=x-nv-video[0].chromaFormat:0\r\na=x-nv-video[0].dynamicRangeMode:0\r\n",
            stream(MediaColorQuality::EightBit420, false),
        ),
        (
            stream(MediaColorQuality::EightBit420, false),
            "a=x-nv-video[0].bitDepth:10\r\na=x-nv-video[0].chromaFormat:1\r\na=x-nv-video[0].dynamicRangeMode:1\r\n",
            stream(MediaColorQuality::TenBit444, true),
        ),
        (
            stream(MediaColorQuality::TenBit444, false),
            "a=x-nv-video[0].chromaFormat:0\r\n",
            stream(MediaColorQuality::TenBit420, false),
        ),
    ] {
        let resolved = NvstColorNegotiation::resolve(
            requested,
            &format!("a=x-nv-general.nativeRtcOnBundlePort:1\r\n;;{suffix}"),
        )
        .unwrap();
        assert_eq!(resolved.stream, expected);
        let effective = WireColor::from_stream(expected);
        let lines = resolved.announce_lines();
        for (name, value, default) in [
            ("bitDepth", effective.bit_depth, 8),
            ("chromaFormat", effective.chroma_format, 0),
            ("dynamicRangeMode", effective.dynamic_range, 0),
        ] {
            assert_eq!(
                lines.contains(&format!("a=x-nv-video[0].{name}:{value}")),
                value != default,
            );
        }
    }
}

#[test]
fn unsupported_server_overrides_fail_without_relabeling_the_decoder() {
    for (codec, quality, hdr, field, value) in [
        (
            MediaVideoCodec::H265,
            MediaColorQuality::TenBit420,
            false,
            "bitDepth",
            "12",
        ),
        (
            MediaVideoCodec::H265,
            MediaColorQuality::TenBit420,
            false,
            "chromaFormat",
            "3",
        ),
        (
            MediaVideoCodec::H265,
            MediaColorQuality::TenBit420,
            false,
            "dynamicRangeMode",
            "2",
        ),
        (
            MediaVideoCodec::H265,
            MediaColorQuality::EightBit420,
            false,
            "dynamicRangeMode",
            "1",
        ),
        (
            MediaVideoCodec::H265,
            MediaColorQuality::TenBit420,
            true,
            "bitDepth",
            "8",
        ),
        (
            MediaVideoCodec::H264,
            MediaColorQuality::EightBit420,
            false,
            "bitDepth",
            "10",
        ),
        (
            MediaVideoCodec::Av1,
            MediaColorQuality::TenBit420,
            false,
            "chromaFormat",
            "1",
        ),
    ] {
        let mut requested = stream(quality, hdr);
        requested.codec = codec;
        let error = NvstColorNegotiation::resolve(
            requested,
            &format!(";;a=x-nv-video[0].{field}:{value}\r\n"),
        )
        .err()
        .expect("unsupported override");
        assert_eq!(error.code, "nvst-color-unsupported", "{field}:{value}");
    }
}

#[test]
fn malformed_and_conflicting_color_fields_fail_without_echoing_remote_data() {
    for suffix in ["", ";;"] {
        for value in ["", "-1", "256", "10.0", "private-token"] {
            let error = NvstColorNegotiation::resolve(
                stream(MediaColorQuality::TenBit420, false),
                &format!("{suffix}a=x-nv-video[0].bitDepth:{value}\r\n"),
            )
            .err()
            .expect("invalid override");
            assert_eq!(error.code, "nvst-color-invalid");
            assert!(!error.message.contains("private-token"));
        }
        let error = NvstColorNegotiation::resolve(
            stream(MediaColorQuality::TenBit420, false),
            &format!("{suffix}a=x-nv-video[0].bitDepth:8\r\na=x-nv-video[0].bitDepth:10\r\n"),
        )
        .err()
        .expect("conflicting override");
        assert_eq!(error.code, "nvst-color-invalid");
    }
}

#[test]
fn only_allowlisted_color_fields_in_the_primary_stream_are_applied() {
    let requested = stream(MediaColorQuality::TenBit420, false);
    let resolved = NvstColorNegotiation::resolve(
        requested,
        ";;a=x-nv-runtime.encryptionKey:private-token\r\na=x-nv-video[1].bitDepth:8\r\na=x-nv-video[0].unknown:1\r\n",
    )
    .unwrap();
    assert_eq!(resolved.stream, requested);
    assert!(
        resolved
            .announce_lines()
            .iter()
            .all(|line| !line.contains("private-token"))
    );
}

#[test]
fn parser_accepts_trimmed_case_insensitive_fields_and_identical_duplicates() {
    let resolved = NvstColorNegotiation::resolve(
        stream(MediaColorQuality::EightBit420, false),
        ";; A=X-NV-VIDEO[0].BITDEPTH: 10 \r\na=video[0].bitDepth:10\r\na=video[0].chromaFormat:1\r\n",
    )
    .unwrap();
    assert_eq!(resolved.stream.color_quality, MediaColorQuality::TenBit444);
    assert!(!resolved.stream.hdr);
}

#[test]
fn a_later_session_does_not_inherit_a_previous_sessions_color_override() {
    let requested = stream(MediaColorQuality::TenBit444, false);
    let first = NvstColorNegotiation::resolve(
        requested,
        ";;a=x-nv-video[0].bitDepth:8\r\na=x-nv-video[0].chromaFormat:0\r\n",
    )
    .unwrap();
    let second = NvstColorNegotiation::resolve(requested, "").unwrap();
    assert_eq!(first.stream.color_quality, MediaColorQuality::EightBit420);
    assert_eq!(second.stream, requested);
    assert_eq!(second.announce_lines().len(), 3);
}
