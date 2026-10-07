use openh264::encoder::{
    BitRate, Encoder, EncoderConfig, FrameRate, FrameType, IntraFramePeriod, VuiConfig,
};
use openh264::formats::YUVBuffer;
use opennow_media_protocol::SourceStamp;
use opennow_media_protocol::wire::{AUDIO_TRACK_ID, MediaHeader as Header, VIDEO_TRACK_ID};
use std::f32::consts::TAU;
use std::io;

pub const MAX_VIDEO_BYTES: usize = 256 * 1024;
pub const MAX_AUDIO_BYTES: usize = 1275;
pub const VIDEO_SOURCE: SourceStamp = SourceStamp {
    sender_frame_id: Some(u32::MAX as u64 + 1000),
    timestamp: 90_000,
    clock_rate_hz: 90_000,
    ssrc: Some(1234),
};
pub const AUDIO_SOURCE: SourceStamp = SourceStamp {
    sender_frame_id: Some(u32::MAX as u64 + 2000),
    timestamp: 48_000,
    clock_rate_hz: 48_000,
    ssrc: Some(5678),
};
pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 180;
pub const SAMPLES: usize = 960;

pub struct Fixture {
    video: Encoder,
    audio: opus::Encoder,
    patterns: [YUVBuffer; 2],
    pcm: [f32; SAMPLES * 2],
}

fn bt709_pattern(patch: [u8; 3]) -> YUVBuffer {
    let to_yuv = |rgb: [u8; 3]| {
        let [r, g, b] = rgb.map(f64::from);
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        [
            (16.0 + 219.0 * luma / 255.0).round() as u8,
            (128.0 + 224.0 * (b - luma) / (255.0 * 1.8556)).round() as u8,
            (128.0 + 224.0 * (r - luma) / (255.0 * 1.5748)).round() as u8,
        ]
    };
    let patch = to_yuv(patch);
    let border = to_yuv([32, 96, 48]);
    let pixels = WIDTH * HEIGHT;
    let mut planar = vec![0; pixels * 3 / 2];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let color = if (40..280).contains(&x) && (30..150).contains(&y) {
                patch
            } else {
                border
            };
            planar[y * WIDTH + x] = color[0];
            if y.is_multiple_of(2) && x.is_multiple_of(2) {
                let chroma = (y / 2) * (WIDTH / 2) + x / 2;
                planar[pixels + chroma] = color[1];
                planar[pixels * 5 / 4 + chroma] = color[2];
            }
        }
    }
    YUVBuffer::from_vec(planar, WIDTH, HEIGHT)
}

impl Fixture {
    pub fn new() -> io::Result<Self> {
        let config = EncoderConfig::new()
            .debug(false)
            .bitrate(BitRate::from_bps(500_000))
            .max_frame_rate(FrameRate::from_hz(50.0))
            .skip_frames(false)
            .scene_change_detect(false)
            .intra_frame_period(IntraFramePeriod::from_num_frames(100))
            .num_threads(1)
            .vui(VuiConfig::bt709());
        let video = Encoder::with_api_config(openh264::OpenH264API::from_source(), config)
            .map_err(io::Error::other)?;
        let mut audio =
            opus::Encoder::new(48_000, opus::Channels::Stereo, opus::Application::Audio)
                .map_err(io::Error::other)?;
        audio
            .set_bitrate(opus::Bitrate::Bits(96_000))
            .map_err(io::Error::other)?;
        Ok(Self {
            video,
            audio,
            patterns: [bt709_pattern([24, 48, 224]), bt709_pattern([224, 48, 24])],
            pcm: [0.0; SAMPLES * 2],
        })
    }

    pub fn video(
        &mut self,
        index: u64,
        attempt: u64,
        force_keyframe: bool,
    ) -> io::Result<(Header, Vec<u8>)> {
        if index == 0 || force_keyframe {
            self.video.force_intra_frame();
        }
        let encoded = self
            .video
            .encode_at(
                &self.patterns[((index / 50) % 2) as usize],
                openh264::Timestamp::from_millis(index * 20),
            )
            .map_err(io::Error::other)?;
        let keyframe = encoded.frame_type() == FrameType::IDR;
        let payload = encoded.to_vec();
        if payload.is_empty() || payload.len() > MAX_VIDEO_BYTES {
            return Err(io::Error::other("video exceeds fixture limits"));
        }
        Ok((
            Header {
                track_id: VIDEO_TRACK_ID,
                keyframe,
                contiguous: !cfg!(feature = "fault-injection") || !(index + 1).is_multiple_of(50),
                payload_bytes: payload.len() as u32,
                source: SourceStamp {
                    sender_frame_id: VIDEO_SOURCE.sender_frame_id.map(|first| first + index),
                    timestamp: VIDEO_SOURCE.timestamp + index * 1800,
                    ..VIDEO_SOURCE
                },
                attempt_generation: attempt,
            },
            payload,
        ))
    }

    pub fn audio(&mut self, index: u64, attempt: u64) -> io::Result<(Header, Vec<u8>)> {
        for (offset, stereo) in self.pcm.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            let sample = (index * SAMPLES as u64 + offset as u64) % 48_000;
            let value = (sample as f32 * 440.0 * TAU / 48_000.0).sin() * 0.15;
            stereo.fill(value);
        }
        let mut payload = vec![0; MAX_AUDIO_BYTES];
        let size = self
            .audio
            .encode_float(&self.pcm, &mut payload)
            .map_err(io::Error::other)?;
        payload.truncate(size);
        if payload.is_empty() {
            return Err(io::Error::other("empty Opus packet"));
        }
        Ok((
            Header {
                track_id: AUDIO_TRACK_ID,
                keyframe: false,
                contiguous: true,
                payload_bytes: size as u32,
                source: SourceStamp {
                    sender_frame_id: AUDIO_SOURCE.sender_frame_id.map(|first| first + index),
                    timestamp: AUDIO_SOURCE.timestamp + index * 960,
                    ..AUDIO_SOURCE
                },
                attempt_generation: attempt,
            },
            payload,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openh264::formats::YUVSource;

    #[cfg(not(feature = "fault-injection"))]
    #[test]
    fn default_stream_stays_continuous_across_requested_mid_gop_idr() {
        let mut fixture = Fixture::new().unwrap();
        let mut decoder = openh264::decoder::Decoder::new().unwrap();
        for index in 0..121 {
            let (header, bytes) = fixture.video(index, 7, index == 63).unwrap();
            assert!(
                header.contiguous,
                "unexpected discontinuity at frame {index}"
            );
            if index == 0 || index == 63 {
                assert!(header.keyframe, "missing requested IDR at frame {index}");
            }
            if index == 62 || index == 64 {
                assert!(!header.keyframe, "expected inter frame {index}");
            }
            assert_eq!(
                header.source.sender_frame_id,
                VIDEO_SOURCE.sender_frame_id.map(|first| first + index)
            );
            assert_eq!(
                header.source.timestamp,
                VIDEO_SOURCE.timestamp + index * 1800
            );
            let image = decoder
                .decode(&bytes)
                .unwrap()
                .expect("decoded continuous frame");
            assert_eq!(image.dimensions(), (WIDTH, HEIGHT));
        }
    }

    #[cfg(not(feature = "fault-injection"))]
    #[test]
    fn late_recording_gets_decodable_periodic_idr_without_feedback() {
        let mut fixture = Fixture::new().unwrap();
        let mut late_decoder = openh264::decoder::Decoder::new().unwrap();
        let mut recording_started = None;
        for index in 0..151 {
            let (header, bytes) = fixture.video(index, 7, false).unwrap();
            assert!(header.contiguous);
            if index >= 27 && header.keyframe && recording_started.is_none() {
                recording_started = Some(index);
            }
            if recording_started.is_some() {
                let image = late_decoder
                    .decode(&bytes)
                    .unwrap()
                    .expect("late recording frame");
                assert_eq!(image.dimensions(), (WIDTH, HEIGHT));
            }
        }
        assert!(recording_started.is_some_and(|index| index <= 127));
    }

    #[test]
    #[ignore = "requires ffprobe on PATH"]
    fn ffprobe_confirms_receiver_compatible_sps_metadata() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut fixture = Fixture::new().unwrap();
        let (_, bytes) = fixture.video(0, 7, false).unwrap();
        let mut probe = Command::new("ffprobe")
            .args([
                "-v", "error", "-f", "h264", "-show_entries",
                "stream=width,height,pix_fmt,color_range,color_space,color_transfer,color_primaries,chroma_location",
                "-of", "json", "-i", "pipe:0",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn().unwrap();
        probe.stdin.take().unwrap().write_all(&bytes).unwrap();
        let output = probe.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let stream = &metadata["streams"][0];
        assert_eq!(stream["width"], WIDTH);
        assert_eq!(stream["height"], HEIGHT);
        assert_eq!(stream["pix_fmt"], "yuv420p");
        assert_eq!(stream["color_range"], "tv");
        assert_eq!(stream["color_space"], "bt709");
        assert_eq!(stream["color_transfer"], "bt709");
        assert_eq!(stream["color_primaries"], "bt709");
        assert_eq!(stream["chroma_location"], "left");
    }

    #[test]
    fn generate_h264_decodable_known_pattern() {
        let mut fixture = Fixture::new().unwrap();
        let mut decoder = openh264::decoder::Decoder::new().unwrap();
        let mut rgb = vec![0; WIDTH * HEIGHT * 3];
        let mut deltas = 0;
        for index in 0..101 {
            let (header, bytes) = fixture.video(index, 7, index == 60).unwrap();
            assert_eq!(
                header.keyframe,
                openh264::nal_units(&bytes).any(|nal| {
                    let payload = nal
                        .strip_prefix(&[0, 0, 0, 1])
                        .or_else(|| nal.strip_prefix(&[0, 0, 1]))
                        .unwrap();
                    payload[0] & 31 == 5
                })
            );
            assert_eq!(
                header.source.sender_frame_id,
                VIDEO_SOURCE.sender_frame_id.map(|first| first + index)
            );
            assert_eq!(header.source.ssrc, Some(1234));
            assert_eq!(header.source.timestamp, 90_000 + index * 1800);
            if cfg!(feature = "fault-injection") {
                assert_eq!(header.contiguous, !(index + 1).is_multiple_of(50));
            } else {
                assert!(header.contiguous);
            }
            if index == 0 || index == 60 {
                assert!(header.keyframe);
            }
            if !header.keyframe {
                deltas += 1;
            }
            let image = decoder
                .decode(&bytes)
                .unwrap()
                .expect("decoded video frame");
            assert_eq!(image.dimensions(), (WIDTH, HEIGHT));
            image.write_rgb8(&mut rgb);
            let center = (90 * WIDTH + 160) * 3;
            let patch = &rgb[center..center + 3];
            if (index / 50).is_multiple_of(2) {
                assert!(patch[2] > patch[0] + 100, "blue patch: {patch:?}");
            } else {
                assert!(patch[0] > patch[2] + 100, "red patch: {patch:?}");
            }
        }
        assert!(deltas > 80);
    }

    #[test]
    fn decode_opus_stereo_tone() {
        let mut fixture = Fixture::new().unwrap();
        let mut decoder = opus::Decoder::new(48_000, opus::Channels::Stereo).unwrap();
        let mut pcm = [0.0f32; SAMPLES * 2];
        let mut samples = Vec::new();
        for index in 0..10 {
            let (header, bytes) = fixture.audio(index, 7).unwrap();
            assert_eq!(header.source.timestamp, 48_000 + index * 960);
            assert_eq!(header.source.ssrc, Some(5678));
            assert!(bytes.len() <= MAX_AUDIO_BYTES);
            assert_eq!(
                decoder.decode_float(&bytes, &mut pcm, false).unwrap(),
                SAMPLES
            );
            if index > 0 {
                for stereo in pcm.as_chunks::<2>().0 {
                    assert!((stereo[0] - stereo[1]).abs() < 0.02);
                    samples.push(stereo[0]);
                }
            }
        }
        let power = samples.iter().map(|v| v * v).sum::<f32>() / samples.len() as f32;
        assert!((0.005..0.03).contains(&power), "tone power: {power}");
        let crossings = samples
            .windows(2)
            .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
            .count();
        let hz = crossings as f32 * 48_000.0 / samples.len() as f32;
        assert!((430.0..450.0).contains(&hz), "tone frequency: {hz}");
    }
}
