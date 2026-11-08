use super::*;
use opennow_streamer_platform::{MediaColorQuality, MediaVideoCodec};

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
fn wire_values_use_literal_depth_and_the_444_flag() {
    for (quality, depth, chroma) in [
        (MediaColorQuality::EightBit420, 8, 0),
        (MediaColorQuality::EightBit444, 8, 1),
        (MediaColorQuality::TenBit420, 10, 0),
        (MediaColorQuality::TenBit444, 10, 1),
    ] {
        assert_eq!(
            announce_color_lines(stream(quality, false)),
            vec![
                format!("a=x-nv-video[0].bitDepth:{depth}"),
                format!("a=x-nv-video[0].chromaFormat:{chroma}"),
            ]
        );
    }
}

#[test]
fn dynamic_range_mode_is_hdr_only() {
    assert_eq!(
        announce_color_lines(stream(MediaColorQuality::TenBit420, true)),
        vec![
            "a=x-nv-video[0].bitDepth:10",
            "a=x-nv-video[0].chromaFormat:0",
            "a=x-nv-video[0].dynamicRangeMode:1",
        ]
    );
    assert_eq!(
        announce_color_lines(stream(MediaColorQuality::TenBit444, true)),
        vec![
            "a=x-nv-video[0].bitDepth:10",
            "a=x-nv-video[0].chromaFormat:1",
            "a=x-nv-video[0].dynamicRangeMode:1",
        ]
    );
    for quality in [
        MediaColorQuality::EightBit420,
        MediaColorQuality::EightBit444,
        MediaColorQuality::TenBit420,
        MediaColorQuality::TenBit444,
    ] {
        assert!(
            announce_color_lines(stream(quality, false))
                .iter()
                .all(|line| !line.contains("dynamicRangeMode")),
            "{quality:?}"
        );
    }
}

#[test]
fn ten_bit_444_matches_the_official_client_config() {
    assert_eq!(
        announce_color_lines(stream(MediaColorQuality::TenBit444, false)),
        vec![
            "a=x-nv-video[0].bitDepth:10",
            "a=x-nv-video[0].chromaFormat:1",
        ]
    );
}

#[test]
fn chroma_never_exceeds_the_official_validator_range() {
    for quality in [
        MediaColorQuality::EightBit420,
        MediaColorQuality::EightBit444,
        MediaColorQuality::TenBit420,
        MediaColorQuality::TenBit444,
    ] {
        for hdr in [false, true] {
            let chroma = announce_color_lines(stream(quality, hdr))
                .into_iter()
                .find_map(|line| {
                    line.strip_prefix("a=x-nv-video[0].chromaFormat:")
                        .map(|value| value.parse::<u8>().unwrap())
                })
                .unwrap();
            assert!(chroma <= 1, "{quality:?} hdr={hdr} chroma={chroma}");
        }
    }
}
