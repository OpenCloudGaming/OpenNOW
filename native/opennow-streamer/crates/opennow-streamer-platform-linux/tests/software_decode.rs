#![cfg(all(target_os = "linux", feature = "ffmpeg"))]

use opennow_streamer_platform_linux::{
    ColorTransfer, DecoderPreference, EncodedVideoFrame, LifecycleState, LinuxSession, PixelFormat,
    PushOutcome, SessionConfig, StreamFormat,
};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const BITSTREAM: &[u8] = include_bytes!("fixtures/software_decode_64x64.h264");
const EXPECTED_FRAMES: usize = 30;
const SECOND_KEYFRAME: usize = 15;
const FRAME_INTERVAL_US: u64 = 33_333;
const ADMISSION_TIMEOUT: Duration = Duration::from_secs(5);

fn access_units(stream: &[u8]) -> Vec<(Vec<u8>, bool)> {
    let mut starts = Vec::new();
    for index in 0..stream.len().saturating_sub(3) {
        if stream[index..index + 3] == [0, 0, 1] {
            starts.push(index);
        }
    }
    starts.push(stream.len());
    let mut units: Vec<(Vec<u8>, bool)> = Vec::new();
    let mut pending = Vec::new();
    let mut keyframe = false;
    for window in starts.windows(2) {
        let nal = stream[window[0] + 3] & 0x1f;
        pending.extend_from_slice(&stream[window[0]..window[1]]);
        keyframe |= matches!(nal, 5 | 7);
        if matches!(nal, 1 | 5) {
            units.push((std::mem::take(&mut pending), keyframe));
            keyframe = false;
        }
    }
    units
}

fn drain_until_silent(
    session: &LinuxSession,
    frames: &mut Vec<opennow_streamer_platform_linux::DecodedVideoFrame>,
    silence: Duration,
) {
    while let Some(frame) = session.recv_frame_timeout(silence) {
        frames.push(frame);
    }
}

#[test]
fn software_only_decodes_cpu_nv12_sdr_frames_for_embedded_presentation() {
    let mut config = SessionConfig::new(StreamFormat::h264_default(64, 64).unwrap());
    config.decoder_preference = DecoderPreference::SoftwareOnly;
    config.embedded_presentation = true;
    config.audio = None;
    let mut session =
        LinuxSession::start(config).expect("software decoder opens without a shared GPU device");
    let ready = Instant::now() + Duration::from_secs(3);
    while session.state() != LifecycleState::Running {
        assert!(
            Instant::now() < ready,
            "video worker did not reach Running: {:?}",
            session.state()
        );
        thread::sleep(Duration::from_millis(1));
    }

    let units = access_units(BITSTREAM);
    assert_eq!(units.len(), EXPECTED_FRAMES);
    let mut frames = Vec::new();
    let mut timestamp_us = 0;
    for (index, (data, keyframe)) in units.iter().enumerate() {
        assert!(matches!(
            session
                .submit_video(
                    EncodedVideoFrame::new(
                        Arc::<[u8]>::from(data.as_slice()),
                        timestamp_us,
                        *keyframe
                    )
                    .unwrap()
                )
                .expect("access unit is admitted"),
            PushOutcome::Queued
        ));
        let admitted = Instant::now() + ADMISSION_TIMEOUT;
        while session.decode_timings().submissions_total < index as u64 + 1 {
            while let Some(frame) = session.try_recv_frame() {
                frames.push(frame);
            }
            assert!(
                Instant::now() < admitted,
                "software decoder never admitted access unit {index}"
            );
            thread::sleep(Duration::from_millis(1));
        }
        while let Some(frame) = session.try_recv_frame() {
            frames.push(frame);
        }
        timestamp_us += FRAME_INTERVAL_US;
    }
    drain_until_silent(&session, &mut frames, Duration::from_millis(500));

    assert!(
        frames.len() > SECOND_KEYFRAME,
        "decode must continue across the second keyframe, got {}",
        frames.len()
    );
    for (index, frame) in frames.iter().enumerate() {
        assert_eq!(
            frame.timestamp_us,
            index as u64 * FRAME_INTERVAL_US,
            "delivered frames must be a contiguous prefix"
        );
        assert_eq!((frame.format.width, frame.format.height), (64, 64));
        assert_eq!(frame.format.pixel_format, PixelFormat::Nv12);
        assert_eq!(frame.format.color_transfer, ColorTransfer::Sdr);
        assert!(frame.vulkan.is_none() && frame.dmabuf.is_none());
        assert_eq!(frame.planes.len(), 2);
        assert_eq!(frame.planes[0].stride, 64);
        assert_eq!(frame.planes[0].rows, 64);
        assert_eq!(frame.planes[0].data.len(), 64 * 64);
        assert_eq!(frame.planes[1].stride, 64);
        assert_eq!(frame.planes[1].rows, 32);
        assert_eq!(frame.planes[1].data.len(), 64 * 32);
    }

    let delivered = frames.len();
    session
        .submit_video(
            EncodedVideoFrame::new(Arc::<[u8]>::from(units[0].0.as_slice()), 1_000_000, true)
                .unwrap(),
        )
        .expect("fresh access unit is admitted");
    drain_until_silent(&session, &mut frames, Duration::from_millis(500));
    assert!(
        frames.len() > delivered,
        "new input must release look-ahead frames held by the decoder"
    );
    session.stop().expect("session stops");
}
