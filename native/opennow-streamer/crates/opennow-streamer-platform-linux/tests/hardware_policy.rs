#![cfg(target_os = "linux")]

use opennow_streamer_platform_linux::{
    DecoderPreference, LinuxSession, SessionConfig, StreamFormat, VideoCodec,
    probe_video_capabilities,
};

#[test]
fn hardware_only_public_capabilities_never_offer_software() {
    let (decoders, _) = probe_video_capabilities();
    if cfg!(feature = "hardware-only") {
        assert!(
            decoders.iter().all(|decoder| {
                !decoder.name.starts_with("ffmpeg-software") || !decoder.available
            })
        );
    } else {
        assert!(
            decoders
                .iter()
                .any(|decoder| decoder.name == "ffmpeg-software-h264")
        );
    }
}

#[test]
fn hardware_only_public_session_rejects_software_before_device_initialization() {
    if !cfg!(feature = "hardware-only") {
        return;
    }
    for codec in [VideoCodec::H264, VideoCodec::H265, VideoCodec::Av1] {
        for embedded in [false, true] {
            let mut config = SessionConfig::new(StreamFormat::video_default(64, 64).unwrap());
            config.codec = codec;
            config.embedded_presentation = embedded;
            config.decoder_preference = DecoderPreference::SoftwareOnly;
            config.audio = None;
            let error = LinuxSession::start(config)
                .err()
                .expect("software must be forbidden");
            assert!(error.to_string().contains("hardware-only"), "{error}");
        }
    }
}
