use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use opennow_streamer_protocol::{RecordingCompletion, RecordingCutReason};
use oxideav_core::{
    CodecId, CodecParameters, Muxer, Packet, Rational, StreamInfo, TimeBase, WriteSeek,
};
use oxideav_mkv::avc::annexb_to_avcc;
use oxideav_mkv::mux::MkvMuxer;
use scuffle_av1::{ObuHeader, ObuType, seq::SequenceHeaderObu};

use crate::media::{
    EncodedFrame, EncodedRecordingReceiver, MediaCodec, MediaStreamConfig, MediaVideoCodec,
};

const VIDEO_STREAM_INDEX: u32 = 0;
const AUDIO_STREAM_INDEX: u32 = 1;
const OPUS_SAMPLE_RATE: u32 = 48_000;
const OPUS_PRE_SKIP: u16 = 312;

pub fn record_replay_matroska(
    output_path: impl AsRef<Path>,
    stream: MediaStreamConfig,
    mut snapshot: crate::replay::ReplaySnapshot,
    cancelled: &AtomicBool,
) -> Result<RecordingSummary, String> {
    static EXPORT_ID: AtomicU64 = AtomicU64::new(0);
    let output_path = validate_output_path(output_path.as_ref())?;
    let part_path = output_path.with_extension(format!(
        "mkv.{}.{}.part",
        std::process::id(),
        EXPORT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    if output_path.exists() || part_path.exists() {
        return Err("clip output already exists".to_owned());
    }
    let result = (|| {
        let first = snapshot.frames.front().ok_or("replay buffer is empty")?;
        let origin = first.time;
        let channels = snapshot
            .frames
            .iter()
            .find_map(|entry| match entry.frame.codec {
                MediaCodec::Opus { channels } => Some(channels.clamp(1, 2)),
                _ => None,
            })
            .unwrap_or(2);
        if cancelled.load(Ordering::Acquire) || snapshot.cancelled.load(Ordering::Acquire) {
            return Err("clip export cancelled".to_owned());
        }
        let mut active = start_muxer(&part_path, stream, channels, &first.frame)?;
        for entry in &snapshot.frames {
            if cancelled.load(Ordering::Acquire) || snapshot.cancelled.load(Ordering::Acquire) {
                return Err("clip export cancelled".to_owned());
            }
            if let Some(time) = entry.time.checked_sub(origin) {
                write_frame_at(&mut active, entry.frame.clone(), Some(time))?;
            }
        }
        active
            .muxer
            .write_trailer()
            .map_err(|error| format!("failed to finalize clip: {error}"))?;
        drop(active.muxer);
        if cancelled.load(Ordering::Acquire) || snapshot.cancelled.load(Ordering::Acquire) {
            return Err("clip export cancelled".to_owned());
        }
        std::fs::hard_link(&part_path, &output_path)
            .map_err(|error| format!("failed to publish clip: {error}"))?;
        if cancelled.load(Ordering::Acquire) || snapshot.cancelled.load(Ordering::Acquire) {
            let _ = std::fs::remove_file(&output_path);
            return Err("clip export cancelled".to_owned());
        }
        Ok(RecordingSummary {
            path: output_path,
            video_packets: active.video_packets,
            audio_packets: active.audio_packets,
        })
    })();
    snapshot.frames.clear();
    let _ = std::fs::remove_file(&part_path);
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingSummary {
    pub path: PathBuf,
    pub video_packets: u64,
    pub audio_packets: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualRecordingSummary {
    pub media: RecordingSummary,
    pub completion: RecordingCompletion,
}

struct ActiveMuxer {
    muxer: MkvMuxer,
    video_codec: MediaVideoCodec,
    video_clock: TrackClock,
    audio_clock: TrackClock,
    video_packets: u64,
    audio_packets: u64,
}

#[derive(Default)]
struct TrackClock {
    base: Option<u64>,
    last_pts: i64,
}

impl TrackClock {
    fn pts(&mut self, timestamp: u64) -> i64 {
        let base = *self.base.get_or_insert(timestamp);
        let delta = rtp_timestamp_delta(base, timestamp);
        let pts = i64::try_from(delta).unwrap_or(i64::MAX);
        self.last_pts = self.last_pts.max(pts);
        self.last_pts
    }
}

pub fn record_matroska(
    output_path: impl AsRef<Path>,
    stream: MediaStreamConfig,
    receiver: EncodedRecordingReceiver,
) -> Result<ManualRecordingSummary, String> {
    record_matroska_with(
        output_path.as_ref(),
        stream,
        receiver,
        |path| {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map(|file| Box::new(file) as Box<dyn WriteSeek>)
                .map_err(|error| format!("failed to create recording: {error}"))
        },
        |part, output| {
            publish_manual_recording(part, output)
                .map_err(|error| format!("failed to publish completed recording: {error}"))
        },
    )
}

pub(crate) fn publish_manual_recording(part: &Path, output: &Path) -> std::io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let part = CString::new(part.as_os_str().as_bytes())?;
        let output = CString::new(output.as_os_str().as_bytes())?;
        #[cfg(target_os = "linux")]
        let result = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                part.as_ptr(),
                libc::AT_FDCWD,
                output.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(target_os = "macos")]
        let result = unsafe { libc::renamex_np(part.as_ptr(), output.as_ptr(), libc::RENAME_EXCL) };
        if result == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        let part = std::fs::canonicalize(part)?;
        let output = std::fs::canonicalize(output.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "recording output has no directory",
            )
        })?)?
        .join(output.file_name().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "recording output has no filename",
            )
        })?);
        let part: Vec<u16> = part.as_os_str().encode_wide().chain(Some(0)).collect();
        let output: Vec<u16> = output.as_os_str().encode_wide().chain(Some(0)).collect();
        let result = unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(part.as_ptr(), output.as_ptr(), 0)
        };
        if result != 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = (part, output);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "atomic recording publication is unavailable on this platform",
        ))
    }
}

pub(crate) fn record_matroska_with(
    output_path: &Path,
    stream: MediaStreamConfig,
    receiver: EncodedRecordingReceiver,
    open: impl FnOnce(&Path) -> Result<Box<dyn WriteSeek>, String>,
    publish: impl FnOnce(&Path, &Path) -> Result<(), String>,
) -> Result<ManualRecordingSummary, String> {
    let output_path = validate_output_path(output_path)?;
    let part_path = part_path_for(&output_path)?;
    if output_path.exists() {
        return Err(format!(
            "recording output already exists: {}",
            output_path.display()
        ));
    }

    let mut owns_partial = false;
    let result = record_matroska_inner(
        &part_path,
        &output_path,
        stream,
        &receiver,
        |path| {
            let output = open(path)?;
            owns_partial = true;
            Ok(output)
        },
        publish,
    );
    if owns_partial && result.is_err() {
        let _ = std::fs::remove_file(&part_path);
    }
    result
}

fn record_matroska_inner(
    part_path: &Path,
    output_path: &Path,
    stream: MediaStreamConfig,
    receiver: &EncodedRecordingReceiver,
    open: impl FnOnce(&Path) -> Result<Box<dyn WriteSeek>, String>,
    publish: impl FnOnce(&Path, &Path) -> Result<(), String>,
) -> Result<ManualRecordingSummary, String> {
    let mut active: Option<ActiveMuxer> = None;
    let mut audio_channels = 2_u8;
    let mut open = Some(open);

    let completion = loop {
        let frame = match receiver.recv() {
            Ok(frame) => frame,
            Err(completion) => break completion,
        };
        if !frame.contiguous {
            if active.is_none() {
                return Err(format!(
                    "recording stopped because the {} stream was discontinuous",
                    frame.mid
                ));
            }
            break RecordingCompletion::Cut {
                reason: RecordingCutReason::Discontinuity,
            };
        }

        if let MediaCodec::Opus { channels } = frame.codec {
            audio_channels = channels.clamp(1, 2);
        }

        if active.is_none() {
            if !frame.keyframe || !is_video_codec(&frame.codec, stream.codec) {
                continue;
            }
            if !has_video_picture(&frame, stream.codec) {
                return Err("recording keyframe did not contain a video picture".to_owned());
            }
            active = Some(start_muxer_with(
                part_path,
                stream,
                audio_channels,
                &frame,
                open.take().expect("recording output opens once"),
            )?);
        }

        write_frame(active.as_mut().expect("muxer was initialized"), frame)?;
    };

    let Some(mut active) = active else {
        return Err("recording ended before a decodable video keyframe arrived".to_owned());
    };
    if active.video_packets == 0 {
        return Err("recording ended without a video picture".to_owned());
    }
    active
        .muxer
        .write_trailer()
        .map_err(|error| format!("failed to finalize Matroska recording: {error}"))?;
    let media = RecordingSummary {
        path: output_path.to_owned(),
        video_packets: active.video_packets,
        audio_packets: active.audio_packets,
    };
    drop(active);
    publish(part_path, output_path)?;
    Ok(ManualRecordingSummary { media, completion })
}

fn has_video_picture(frame: &EncodedFrame, codec: MediaVideoCodec) -> bool {
    match codec {
        MediaVideoCodec::H264 => {
            let nals = split_annex_b(&frame.data);
            nals.iter().any(|nal| nal.len() > 1 && nal[0] & 0x1f == 5)
                && nals.iter().any(|nal| nal.len() >= 4 && nal[0] & 0x1f == 7)
                && nals.iter().any(|nal| nal.len() > 1 && nal[0] & 0x1f == 8)
        }
        MediaVideoCodec::H265 => {
            let nals = split_annex_b(&frame.data);
            nals.iter()
                .any(|nal| nal.len() > 2 && matches!((nal[0] >> 1) & 0x3f, 16..=21))
                && [32, 33, 34].into_iter().all(|kind| {
                    nals.iter()
                        .any(|nal| nal.len() > 2 && (nal[0] >> 1) & 0x3f == kind)
                })
        }
        MediaVideoCodec::Av1 => {
            let mut cursor = std::io::Cursor::new(frame.data.as_ref());
            let mut frame_header = false;
            let mut tiles = false;
            while (cursor.position() as usize) < frame.data.len() {
                let Ok(header) = ObuHeader::parse(&mut cursor) else {
                    return false;
                };
                let Some(size) = header.size else {
                    return false;
                };
                let Some(end) = cursor.position().checked_add(size) else {
                    return false;
                };
                if end > frame.data.len() as u64 {
                    return false;
                }
                if size > 0 {
                    match header.obu_type {
                        ObuType::Frame => {
                            frame_header = true;
                            tiles = true;
                        }
                        ObuType::FrameHeader => frame_header = true,
                        ObuType::TileGroup => tiles = true,
                        _ => {}
                    }
                }
                cursor.set_position(end);
            }
            frame_header && tiles
        }
    }
}

fn start_muxer(
    path: &Path,
    stream: MediaStreamConfig,
    audio_channels: u8,
    first_video: &EncodedFrame,
) -> Result<ActiveMuxer, String> {
    start_muxer_with(path, stream, audio_channels, first_video, |path| {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map(|file| Box::new(file) as Box<dyn WriteSeek>)
            .map_err(|error| format!("failed to create recording: {error}"))
    })
}

fn start_muxer_with(
    path: &Path,
    stream: MediaStreamConfig,
    audio_channels: u8,
    first_video: &EncodedFrame,
    open: impl FnOnce(&Path) -> Result<Box<dyn WriteSeek>, String>,
) -> Result<ActiveMuxer, String> {
    let (codec_id, codec_private) = match stream.codec {
        MediaVideoCodec::H264 => {
            let repacked = annexb_to_avcc(&first_video.data);
            if repacked.config_record.is_empty() {
                return Err(
                    "H.264 recording keyframe did not include SPS/PPS codec configuration"
                        .to_owned(),
                );
            }
            ("h264", repacked.config_record)
        }
        MediaVideoCodec::H265 => {
            let repacked = annexb_to_hvcc(&first_video.data)?;
            if repacked.config_record.is_empty() {
                return Err(
                    "H.265 recording keyframe did not include VPS/SPS/PPS codec configuration"
                        .to_owned(),
                );
            }
            ("h265", repacked.config_record)
        }
        MediaVideoCodec::Av1 => ("av1", av1_codec_private(&first_video.data)?),
    };

    let mut video_params = CodecParameters::video(CodecId::new(codec_id));
    video_params.width = Some(stream.width);
    video_params.height = Some(stream.height);
    video_params.frame_rate = Some(Rational::new(i64::from(stream.fps.max(1)), 1));
    video_params.bit_rate = Some(u64::from(stream.bitrate_bps));
    video_params.extradata = codec_private;
    let video_time_base = TimeBase::from_rate(first_video.clock_rate_hz.max(1));
    let video_stream = StreamInfo {
        index: VIDEO_STREAM_INDEX,
        time_base: video_time_base,
        duration: None,
        start_time: Some(0),
        params: video_params,
    };

    let mut audio_params = CodecParameters::audio(CodecId::new("opus"));
    audio_params.sample_rate = Some(OPUS_SAMPLE_RATE);
    audio_params.channels = Some(u16::from(audio_channels));
    audio_params.extradata = opus_head(audio_channels);
    let audio_stream = StreamInfo {
        index: AUDIO_STREAM_INDEX,
        time_base: TimeBase::from_rate(OPUS_SAMPLE_RATE),
        duration: None,
        start_time: Some(0),
        params: audio_params,
    };

    let output = open(path)?;
    let mut muxer = MkvMuxer::new_matroska(output, &[video_stream, audio_stream])
        .map_err(|error| format!("failed to configure Matroska recording: {error}"))?;
    muxer
        .write_header()
        .map_err(|error| format!("failed to write Matroska header: {error}"))?;
    Ok(ActiveMuxer {
        muxer,
        video_codec: stream.codec,
        video_clock: TrackClock::default(),
        audio_clock: TrackClock::default(),
        video_packets: 0,
        audio_packets: 0,
    })
}

fn write_frame(active: &mut ActiveMuxer, frame: EncodedFrame) -> Result<(), String> {
    write_frame_at(active, frame, None)
}

fn write_frame_at(
    active: &mut ActiveMuxer,
    frame: EncodedFrame,
    time: Option<Duration>,
) -> Result<(), String> {
    let aligned_pts = time.map(|time| {
        (time.as_nanos() * u128::from(frame.clock_rate_hz) / 1_000_000_000).min(i64::MAX as u128)
            as i64
    });
    let (stream_index, pts, data, keyframe) = match frame.codec {
        MediaCodec::H264 if active.video_codec == MediaVideoCodec::H264 => {
            let repacked = annexb_to_avcc(&frame.data);
            if repacked.packetized.is_empty() {
                return Ok(());
            }
            (
                VIDEO_STREAM_INDEX,
                aligned_pts.unwrap_or_else(|| active.video_clock.pts(frame.timestamp)),
                repacked.packetized,
                frame.keyframe,
            )
        }
        MediaCodec::H265 if active.video_codec == MediaVideoCodec::H265 => {
            let repacked = annexb_to_hvcc(&frame.data)?;
            if repacked.packetized.is_empty() {
                return Ok(());
            }
            (
                VIDEO_STREAM_INDEX,
                aligned_pts.unwrap_or_else(|| active.video_clock.pts(frame.timestamp)),
                repacked.packetized,
                frame.keyframe,
            )
        }
        MediaCodec::Av1 if active.video_codec == MediaVideoCodec::Av1 => (
            VIDEO_STREAM_INDEX,
            aligned_pts.unwrap_or_else(|| active.video_clock.pts(frame.timestamp)),
            frame.data.to_vec(),
            frame.keyframe,
        ),
        MediaCodec::Opus { .. } => (
            AUDIO_STREAM_INDEX,
            aligned_pts.unwrap_or_else(|| active.audio_clock.pts(frame.timestamp)),
            frame.data.to_vec(),
            true,
        ),
        MediaCodec::Unsupported(_) | MediaCodec::H264 | MediaCodec::H265 | MediaCodec::Av1 => {
            return Err("recording stream codec changed during the session".to_owned());
        }
    };

    let time_base = if stream_index == VIDEO_STREAM_INDEX {
        TimeBase::from_rate(frame.clock_rate_hz.max(1))
    } else {
        TimeBase::from_rate(OPUS_SAMPLE_RATE)
    };
    let packet = Packet::new(stream_index, time_base, data)
        .with_pts(pts)
        .with_dts(pts)
        .with_keyframe(keyframe);
    active
        .muxer
        .write_packet(&packet)
        .map_err(|error| format!("failed to write Matroska packet: {error}"))?;
    if stream_index == VIDEO_STREAM_INDEX {
        active.video_packets = active.video_packets.saturating_add(1);
    } else {
        active.audio_packets = active.audio_packets.saturating_add(1);
    }
    Ok(())
}

fn validate_output_path(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("recording output path must be absolute".to_owned());
    }
    if path.extension().and_then(|value| value.to_str()) != Some("mkv") {
        return Err("native stream recordings must use the .mkv extension".to_owned());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "recording output must have a parent directory".to_owned())?;
    if !parent.is_dir() {
        return Err("recording output directory does not exist".to_owned());
    }
    Ok(path.to_owned())
}

fn part_path_for(path: &Path) -> Result<PathBuf, String> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "recording output file name must be valid UTF-8".to_owned())?;
    Ok(path.with_file_name(format!(".{file_name}.part")))
}

fn is_video_codec(codec: &MediaCodec, expected: MediaVideoCodec) -> bool {
    matches!(
        (codec, expected),
        (MediaCodec::H264, MediaVideoCodec::H264)
            | (MediaCodec::H265, MediaVideoCodec::H265)
            | (MediaCodec::Av1, MediaVideoCodec::Av1)
    )
}

fn opus_head(channels: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(19);
    out.extend_from_slice(b"OpusHead");
    out.push(1);
    out.push(channels.clamp(1, 2));
    out.extend_from_slice(&OPUS_PRE_SKIP.to_le_bytes());
    out.extend_from_slice(&OPUS_SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&0_i16.to_le_bytes());
    out.push(0);
    out
}

fn rtp_timestamp_delta(base: u64, timestamp: u64) -> u64 {
    if timestamp >= base {
        return timestamp - base;
    }
    let base_low = base as u32;
    let timestamp_low = timestamp as u32;
    if base_low.wrapping_sub(timestamp_low) > (u32::MAX / 2) {
        u64::from(timestamp_low.wrapping_sub(base_low))
    } else {
        0
    }
}

pub(crate) fn av1_codec_private(temporal_unit: &[u8]) -> Result<Vec<u8>, String> {
    let mut cursor = std::io::Cursor::new(temporal_unit);
    while usize::try_from(cursor.position()).unwrap_or(usize::MAX) < temporal_unit.len() {
        let obu_start = usize::try_from(cursor.position())
            .map_err(|_| "AV1 OBU position is out of range".to_owned())?;
        let header = ObuHeader::parse(&mut cursor)
            .map_err(|error| format!("invalid AV1 OBU header: {error}"))?;
        let payload_start = usize::try_from(cursor.position())
            .map_err(|_| "AV1 OBU position is out of range".to_owned())?;
        let payload_length = header
            .size
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| "AV1 sequence-header OBU must carry an explicit size".to_owned())?;
        let payload_end = payload_start
            .checked_add(payload_length)
            .filter(|end| *end <= temporal_unit.len())
            .ok_or_else(|| "AV1 OBU payload is truncated".to_owned())?;
        if header.obu_type == ObuType::SequenceHeader {
            let sequence = SequenceHeaderObu::parse(
                header,
                &mut std::io::Cursor::new(&temporal_unit[payload_start..payload_end]),
            )
            .map_err(|error| format!("invalid AV1 sequence header: {error}"))?;
            let operating_point = sequence
                .operating_points
                .first()
                .ok_or_else(|| "AV1 sequence header has no operating point".to_owned())?;
            let mut config = Vec::with_capacity(4 + payload_end - obu_start);
            config.push(0x81);
            config.push((sequence.seq_profile << 5) | (operating_point.seq_level_idx & 0x1f));
            config.push(
                (u8::from(operating_point.seq_tier) << 7)
                    | (u8::from(sequence.color_config.bit_depth > 8) << 6)
                    | (u8::from(sequence.color_config.bit_depth == 12) << 5)
                    | (u8::from(sequence.color_config.mono_chrome) << 4)
                    | (u8::from(sequence.color_config.subsampling_x) << 3)
                    | (u8::from(sequence.color_config.subsampling_y) << 2)
                    | (sequence.color_config.chroma_sample_position & 0x03),
            );
            config.push(0);
            config.extend_from_slice(&temporal_unit[obu_start..payload_end]);
            return Ok(config);
        }
        cursor.set_position(
            u64::try_from(payload_end)
                .map_err(|_| "AV1 OBU position is out of range".to_owned())?,
        );
    }
    Err("AV1 recording keyframe did not include a sequence-header OBU".to_owned())
}

struct HvccRepack {
    config_record: Vec<u8>,
    packetized: Vec<u8>,
}

#[derive(Clone, Copy)]
struct HevcProfile {
    profile_space: u8,
    tier_flag: bool,
    profile_idc: u8,
    compatibility_flags: u32,
    constraint_flags: [u8; 6],
    level_idc: u8,
    max_sub_layers_minus_one: u8,
    temporal_id_nested: bool,
    chroma_format_idc: u8,
    bit_depth_luma_minus_eight: u8,
    bit_depth_chroma_minus_eight: u8,
}

fn annexb_to_hvcc(stream: &[u8]) -> Result<HvccRepack, String> {
    let mut vps = Vec::new();
    let mut sps = Vec::new();
    let mut pps = Vec::new();
    let mut packetized = Vec::with_capacity(stream.len());
    for nal in split_annex_b(stream) {
        if nal.len() < 2 {
            continue;
        }
        match (nal[0] >> 1) & 0x3f {
            32 => push_unique(&mut vps, nal),
            33 => push_unique(&mut sps, nal),
            34 => push_unique(&mut pps, nal),
            _ => {
                let length = u32::try_from(nal.len())
                    .map_err(|_| "HEVC NAL unit exceeds the Matroska packet limit".to_owned())?;
                packetized.extend_from_slice(&length.to_be_bytes());
                packetized.extend_from_slice(nal);
            }
        }
    }
    if sps.is_empty() || pps.is_empty() {
        if packetized.is_empty() {
            return Ok(HvccRepack {
                config_record: Vec::new(),
                packetized,
            });
        }
        return Ok(HvccRepack {
            config_record: Vec::new(),
            packetized,
        });
    }
    let profile = parse_hevc_sps(sps[0])?;
    let config_record = build_hvcc(profile, &[(&vps, 32), (&sps, 33), (&pps, 34)])?;
    Ok(HvccRepack {
        config_record,
        packetized,
    })
}

fn push_unique<'a>(target: &mut Vec<&'a [u8]>, nal: &'a [u8]) {
    if !target.contains(&nal) {
        target.push(nal);
    }
}

fn build_hvcc(profile: HevcProfile, arrays: &[(&Vec<&[u8]>, u8)]) -> Result<Vec<u8>, String> {
    let populated = arrays.iter().filter(|(nals, _)| !nals.is_empty()).count();
    let mut out = Vec::new();
    out.push(1);
    out.push(
        (profile.profile_space << 6)
            | (u8::from(profile.tier_flag) << 5)
            | (profile.profile_idc & 0x1f),
    );
    out.extend_from_slice(&profile.compatibility_flags.to_be_bytes());
    out.extend_from_slice(&profile.constraint_flags);
    out.push(profile.level_idc);
    out.extend_from_slice(&0xf000_u16.to_be_bytes());
    out.push(0xfc);
    out.push(0xfc | (profile.chroma_format_idc & 0x03));
    out.push(0xf8 | (profile.bit_depth_luma_minus_eight & 0x07));
    out.push(0xf8 | (profile.bit_depth_chroma_minus_eight & 0x07));
    out.extend_from_slice(&0_u16.to_be_bytes());
    out.push(
        ((profile.max_sub_layers_minus_one.saturating_add(1) & 0x07) << 3)
            | (u8::from(profile.temporal_id_nested) << 2)
            | 0x03,
    );
    out.push(u8::try_from(populated).map_err(|_| "too many HEVC parameter arrays".to_owned())?);
    for (nals, nal_type) in arrays.iter().filter(|(nals, _)| !nals.is_empty()) {
        out.push(0x80 | (*nal_type & 0x3f));
        out.extend_from_slice(
            &u16::try_from(nals.len())
                .map_err(|_| "too many HEVC parameter sets".to_owned())?
                .to_be_bytes(),
        );
        for nal in *nals {
            out.extend_from_slice(
                &u16::try_from(nal.len())
                    .map_err(|_| "HEVC parameter set is too large".to_owned())?
                    .to_be_bytes(),
            );
            out.extend_from_slice(nal);
        }
    }
    Ok(out)
}

fn parse_hevc_sps(nal: &[u8]) -> Result<HevcProfile, String> {
    if nal.len() < 4 {
        return Err("HEVC SPS is truncated".to_owned());
    }
    let rbsp = remove_emulation_prevention(&nal[2..]);
    let mut bits = BitReader::new(&rbsp);
    bits.skip(4)?;
    let max_sub_layers_minus_one = bits.read(3)? as u8;
    let temporal_id_nested = bits.read(1)? != 0;
    let profile_space = bits.read(2)? as u8;
    let tier_flag = bits.read(1)? != 0;
    let profile_idc = bits.read(5)? as u8;
    let compatibility_flags = bits.read(32)? as u32;
    let mut constraint_flags = [0_u8; 6];
    for value in &mut constraint_flags {
        *value = bits.read(8)? as u8;
    }
    let level_idc = bits.read(8)? as u8;
    let mut sub_layer_profile_present = [false; 8];
    let mut sub_layer_level_present = [false; 8];
    for index in 0..usize::from(max_sub_layers_minus_one) {
        sub_layer_profile_present[index] = bits.read(1)? != 0;
        sub_layer_level_present[index] = bits.read(1)? != 0;
    }
    if max_sub_layers_minus_one > 0 {
        for _ in max_sub_layers_minus_one..8 {
            bits.skip(2)?;
        }
    }
    for index in 0..usize::from(max_sub_layers_minus_one) {
        if sub_layer_profile_present[index] {
            bits.skip(88)?;
        }
        if sub_layer_level_present[index] {
            bits.skip(8)?;
        }
    }
    let _sps_id = bits.read_ue()?;
    let chroma_format_idc =
        u8::try_from(bits.read_ue()?).map_err(|_| "invalid HEVC chroma format".to_owned())?;
    if chroma_format_idc == 3 {
        bits.skip(1)?;
    }
    let _width = bits.read_ue()?;
    let _height = bits.read_ue()?;
    if bits.read(1)? != 0 {
        for _ in 0..4 {
            let _ = bits.read_ue()?;
        }
    }
    let bit_depth_luma_minus_eight =
        u8::try_from(bits.read_ue()?).map_err(|_| "invalid HEVC luma bit depth".to_owned())?;
    let bit_depth_chroma_minus_eight =
        u8::try_from(bits.read_ue()?).map_err(|_| "invalid HEVC chroma bit depth".to_owned())?;
    Ok(HevcProfile {
        profile_space,
        tier_flag,
        profile_idc,
        compatibility_flags,
        constraint_flags,
        level_idc,
        max_sub_layers_minus_one,
        temporal_id_nested,
        chroma_format_idc,
        bit_depth_luma_minus_eight,
        bit_depth_chroma_minus_eight,
    })
}

fn remove_emulation_prevention(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut zeroes = 0_u8;
    for &value in bytes {
        if zeroes >= 2 && value == 3 {
            zeroes = 0;
            continue;
        }
        out.push(value);
        if value == 0 {
            zeroes = zeroes.saturating_add(1);
        } else {
            zeroes = 0;
        }
    }
    out
}

struct BitReader<'a> {
    bytes: &'a [u8],
    bit: usize,
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, bit: 0 }
    }

    fn read(&mut self, count: usize) -> Result<u64, String> {
        if count > 64 || self.bit.saturating_add(count) > self.bytes.len().saturating_mul(8) {
            return Err("HEVC SPS bitstream is truncated".to_owned());
        }
        let mut value = 0_u64;
        for _ in 0..count {
            let byte = self.bytes[self.bit / 8];
            let shift = 7 - (self.bit % 8);
            value = (value << 1) | u64::from((byte >> shift) & 1);
            self.bit += 1;
        }
        Ok(value)
    }

    fn skip(&mut self, count: usize) -> Result<(), String> {
        self.read(count).map(|_| ())
    }

    fn read_ue(&mut self) -> Result<u64, String> {
        let mut leading_zeroes = 0_usize;
        while self.read(1)? == 0 {
            leading_zeroes += 1;
            if leading_zeroes > 31 {
                return Err("HEVC SPS Exp-Golomb value is too large".to_owned());
            }
        }
        if leading_zeroes == 0 {
            return Ok(0);
        }
        Ok(((1_u64 << leading_zeroes) - 1) + self.read(leading_zeroes)?)
    }
}

fn split_annex_b(data: &[u8]) -> Vec<&[u8]> {
    let mut result = Vec::new();
    let mut cursor = 0_usize;
    while let Some((start, prefix)) = find_start_code(&data[cursor..]) {
        let nal_start = cursor + start + prefix;
        let next = find_start_code(&data[nal_start..])
            .map(|(offset, _)| nal_start + offset)
            .unwrap_or(data.len());
        let mut nal_end = next;
        while nal_end > nal_start && data[nal_end - 1] == 0 {
            nal_end -= 1;
        }
        if nal_end > nal_start {
            result.push(&data[nal_start..nal_end]);
        }
        cursor = next;
        if cursor >= data.len() {
            break;
        }
    }
    result
}

fn find_start_code(data: &[u8]) -> Option<(usize, usize)> {
    let mut index = 0_usize;
    while index + 2 < data.len() {
        if data[index] == 0 && data[index + 1] == 0 {
            if data[index + 2] == 1 {
                return Some((index, 3));
            }
            if index + 3 < data.len() && data[index + 2] == 0 && data[index + 3] == 1 {
                return Some((index, 4));
            }
        }
        index += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::mpsc::channel;
    use std::time::{SystemTime, UNIX_EPOCH};

    use oxideav_core::{NullCodecResolver, ReadSeek};

    use super::*;

    #[test]
    fn replay_exports_source_packets_with_shared_audio_offset_and_no_clobber() {
        let directory = std::env::temp_dir().join(format!(
            "opennow-replay-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let output = directory.join("clip.mkv");
        let tap = crate::replay::ReplayTap::default();
        tap.start(
            opennow_streamer_protocol::ReplayBufferConfig::from_settings(
                &serde_json::json!({"replayBufferEnabled":true}),
            ),
        );
        let video = frame(MediaCodec::H264, h264_keyframe(), 9_000_000, true);
        let audio = frame(
            MediaCodec::Opus { channels: 2 },
            vec![0x80, 1, 2, 3],
            400_000,
            false,
        );
        tap.publish(&video);
        tap.publish(&audio);
        tap.publish(&frame(
            MediaCodec::H264,
            vec![0, 0, 0, 1, 0x41, 0x9a, 0x22],
            9_018_000,
            false,
        ));
        let mut snapshot = tap.snapshot().unwrap();
        snapshot.frames[0].time = Duration::from_secs(4);
        snapshot.frames[1].time = Duration::from_millis(4_100);
        snapshot.frames[2].time = Duration::from_millis(4_200);
        assert_eq!(snapshot.frames[1].frame.timestamp, 400_000);
        assert!(Arc::ptr_eq(&snapshot.frames[0].frame.data, &video.data));
        let summary = record_replay_matroska(
            &output,
            MediaStreamConfig::default(),
            snapshot,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!((summary.video_packets, summary.audio_packets), (2, 1));
        let file: Box<dyn ReadSeek> = Box::new(std::fs::File::open(&output).unwrap());
        let mut demuxer = oxideav_mkv::demux::open(file, &NullCodecResolver).unwrap();
        let mut packets = Vec::new();
        while let Ok(packet) = demuxer.next_packet() {
            packets.push(packet);
        }
        assert_eq!(packets.len(), 3);
        assert_eq!(packets[0].pts, Some(0));
        assert_eq!(packets[1].pts, Some(100));
        assert_eq!(packets[2].pts, Some(200));
        assert_eq!(packets[1].data.as_slice(), audio.data.as_ref());
        let original = std::fs::read(&output).unwrap();
        tap.publish(&frame(MediaCodec::H264, h264_keyframe(), 9_036_000, true));
        assert!(
            record_replay_matroska(
                &output,
                MediaStreamConfig::default(),
                tap.snapshot().unwrap(),
                &AtomicBool::new(false)
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&output).unwrap(), original);
        tap.publish(&frame(MediaCodec::H264, h264_keyframe(), 9_054_000, true));
        let cancelled = tap.snapshot().unwrap();
        tap.stop();
        let cancelled_path = directory.join("cancelled.mkv");
        assert!(
            record_replay_matroska(
                &cancelled_path,
                MediaStreamConfig::default(),
                cancelled,
                &AtomicBool::new(false)
            )
            .is_err()
        );
        assert!(!cancelled_path.exists());
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        std::fs::remove_dir_all(directory).unwrap();
    }

    fn frame(codec: MediaCodec, data: Vec<u8>, timestamp: u64, keyframe: bool) -> EncodedFrame {
        let is_audio = matches!(codec, MediaCodec::Opus { .. });
        EncodedFrame {
            mid: if is_audio { "audio" } else { "video" }.to_owned(),
            codec,
            data: Arc::from(data),
            frame_index: (!is_audio).then_some(1),
            timestamp,
            clock_rate_hz: if is_audio { 48_000 } else { 90_000 },
            keyframe,
            contiguous: true,
            ssrc: None,
        }
    }

    fn h264_keyframe() -> Vec<u8> {
        let mut out = Vec::new();
        for nal in [
            &[0x67, 0x64, 0x00, 0x28, 0xde, 0xad][..],
            &[0x68, 0xee, 0x3c, 0x80][..],
            &[0x65, 0x88, 0x84, 0x00, 0x10][..],
        ] {
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(nal);
        }
        out
    }

    fn real_h264_keyframe() -> Vec<u8> {
        use openh264::encoder::Encoder;
        use openh264::formats::{RgbSliceU8, YUVBuffer};

        let rgb = vec![64_u8; 64 * 64 * 3];
        let yuv = YUVBuffer::from_rgb_source(RgbSliceU8::new(&rgb, (64, 64)));
        Encoder::new().unwrap().encode(&yuv).unwrap().to_vec()
    }

    fn recording_test_directory(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "opennow-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn discontinuity_saves_only_the_committed_video_and_audio_prefix() {
        let directory = recording_test_directory("recording-prefix");
        let output = directory.join("cut.mkv");
        let (sender, receiver) = channel();
        let video = frame(MediaCodec::H264, real_h264_keyframe(), 90_000, true);
        let mut encoder = opus::Encoder::new(
            OPUS_SAMPLE_RATE,
            opus::Channels::Stereo,
            opus::Application::Audio,
        )
        .unwrap();
        let mut packet = [0_u8; 1275];
        let samples: Vec<f32> = (0..960)
            .flat_map(|index| {
                let sample = (index as f32 * std::f32::consts::TAU * 440.0 / 48_000.0).sin() * 0.1;
                [sample, sample]
            })
            .collect();
        let length = encoder.encode_float(&samples, &mut packet).unwrap();
        let audio = frame(
            MediaCodec::Opus { channels: 2 },
            packet[..length].to_vec(),
            48_000,
            false,
        );
        sender.send(video.clone()).unwrap();
        sender.send(audio.clone()).unwrap();
        let mut broken = frame(
            MediaCodec::H264,
            vec![0, 0, 0, 1, 0x41, 0xff],
            93_000,
            false,
        );
        broken.contiguous = false;
        sender.send(broken).unwrap();
        sender
            .send(frame(MediaCodec::H264, real_h264_keyframe(), 96_000, true))
            .unwrap();
        drop(sender);
        let result = record_matroska(
            &output,
            MediaStreamConfig {
                width: 64,
                height: 64,
                ..MediaStreamConfig::default()
            },
            EncodedRecordingReceiver::from_receiver(receiver),
        );
        let summary = result.expect("a valid prefix must survive the first discontinuity");
        assert_eq!(
            (summary.media.video_packets, summary.media.audio_packets),
            (1, 1)
        );
        assert_eq!(
            summary.completion,
            RecordingCompletion::Cut {
                reason: RecordingCutReason::Discontinuity
            }
        );
        assert!(!part_path_for(&output).unwrap().exists());
        let file: Box<dyn ReadSeek> = Box::new(std::fs::File::open(&output).unwrap());
        let mut demuxer = oxideav_mkv::demux::open(file, &NullCodecResolver).unwrap();
        let saved_video = demuxer.next_packet().unwrap();
        let saved_audio = demuxer.next_packet().unwrap();
        assert_eq!(
            saved_video.data.as_slice(),
            annexb_to_avcc(&video.data).packetized
        );
        assert_eq!(saved_audio.data.as_slice(), audio.data.as_ref());
        assert!(demuxer.next_packet().is_err());
        let mut decoder = opus::Decoder::new(OPUS_SAMPLE_RATE, opus::Channels::Stereo).unwrap();
        let mut decoded = [0_f32; 1920];
        assert_eq!(
            decoder
                .decode_float(&saved_audio.data, &mut decoded, false)
                .unwrap(),
            960
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn create_failure_does_not_remove_an_existing_partial_file() {
        let directory = recording_test_directory("recording-owned-partial");
        let output = directory.join("existing.mkv");
        let partial = part_path_for(&output).unwrap();
        std::fs::write(&partial, b"another writer owns this file").unwrap();
        let (sender, receiver) = channel();
        sender
            .send(frame(MediaCodec::H264, real_h264_keyframe(), 90_000, true))
            .unwrap();
        drop(sender);
        assert!(
            record_matroska(
                &output,
                MediaStreamConfig::default(),
                EncodedRecordingReceiver::from_receiver(receiver),
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read(&partial).unwrap(),
            b"another writer owns this file"
        );
        assert!(!output.exists());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn empty_missing_and_configuration_only_prefixes_are_not_saved() {
        let directory = recording_test_directory("recording-empty-prefix");
        let mut broken = frame(MediaCodec::H264, real_h264_keyframe(), 90_000, true);
        broken.contiguous = false;
        let config_only = split_annex_b(&real_h264_keyframe())
            .into_iter()
            .filter(|nal| matches!(nal[0] & 0x1f, 7 | 8))
            .flat_map(|nal| [0, 0, 0, 1].into_iter().chain(nal.iter().copied()))
            .collect();
        for (index, frames) in [
            vec![],
            vec![broken],
            vec![frame(
                MediaCodec::H264,
                vec![0, 0, 1, 0x41, 1],
                90_000,
                false,
            )],
            vec![frame(MediaCodec::H264, vec![], 90_000, true)],
            vec![frame(MediaCodec::H264, config_only, 90_000, true)],
            vec![frame(
                MediaCodec::H264,
                vec![0, 0, 1, 0x65, 1],
                90_000,
                true,
            )],
        ]
        .into_iter()
        .enumerate()
        {
            let output = directory.join(format!("{index}.mkv"));
            let (sender, receiver) = channel();
            for frame in frames {
                sender.send(frame).unwrap();
            }
            drop(sender);
            assert!(
                record_matroska(
                    &output,
                    MediaStreamConfig::default(),
                    EncodedRecordingReceiver::from_receiver(receiver)
                )
                .is_err()
            );
            assert!(!output.exists());
            assert!(!part_path_for(&output).unwrap().exists());
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn hevc_and_av1_configuration_without_a_picture_cannot_be_saved() {
        let directory = recording_test_directory("recording-codec-empty");
        let sps = synthetic_hevc_sps();
        let hevc: Vec<u8> = [&[0x40, 1, 0x0c][..], sps.as_slice(), &[0x44, 1, 0xc0][..]]
            .into_iter()
            .flat_map(|nal| [0, 0, 0, 1].into_iter().chain(nal.iter().copied()))
            .collect();
        for (codec, media, bytes, name) in [
            (MediaVideoCodec::H265, MediaCodec::H265, hevc, "hevc"),
            (
                MediaVideoCodec::Av1,
                MediaCodec::Av1,
                b"\x0a\x0f\0\0\0j\xef\xbf\xe1\xbc\x02\x19\x90\x10\x10\x10@".to_vec(),
                "av1",
            ),
        ] {
            let output = directory.join(format!("{name}.mkv"));
            let (sender, receiver) = channel();
            sender.send(frame(media, bytes, 90_000, true)).unwrap();
            drop(sender);
            assert!(
                record_matroska(
                    &output,
                    MediaStreamConfig {
                        codec,
                        ..MediaStreamConfig::default()
                    },
                    EncodedRecordingReceiver::from_receiver(receiver)
                )
                .is_err()
            );
            assert!(!output.exists());
            assert!(!part_path_for(&output).unwrap().exists());
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    struct FailingFile {
        file: std::fs::File,
        payload: Vec<u8>,
        packets: usize,
        fail_packet: bool,
        fail_flush: bool,
        failed: Arc<AtomicBool>,
    }

    impl std::io::Write for FailingFile {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.ends_with(&self.payload) {
                self.packets += 1;
                if self.fail_packet && self.packets == 2 {
                    self.failed.store(true, Ordering::Release);
                    return Err(std::io::Error::other("injected packet write failure"));
                }
            }
            std::io::Write::write(&mut self.file, bytes)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            if self.fail_flush {
                self.failed.store(true, Ordering::Release);
                return Err(std::io::Error::other("injected trailer flush failure"));
            }
            std::io::Write::flush(&mut self.file)
        }
    }

    impl std::io::Seek for FailingFile {
        fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
            std::io::Seek::seek(&mut self.file, position)
        }
    }

    #[test]
    fn packet_trailer_and_publication_failures_never_publish_a_cut() {
        let directory = recording_test_directory("recording-io-failures");
        let video = real_h264_keyframe();
        let payload = annexb_to_avcc(&video).packetized;
        for failure in ["packet", "trailer", "publication"] {
            let output = directory.join(format!("{failure}.mkv"));
            let (sender, receiver) = channel();
            sender
                .send(frame(MediaCodec::H264, video.clone(), 90_000, true))
                .unwrap();
            sender
                .send(frame(MediaCodec::H264, video.clone(), 93_000, true))
                .unwrap();
            let mut broken = frame(MediaCodec::H264, vec![], 96_000, false);
            broken.contiguous = false;
            sender.send(broken).unwrap();
            drop(sender);
            let failed = Arc::new(AtomicBool::new(false));
            let published = AtomicBool::new(false);
            let result = record_matroska_with(
                &output,
                MediaStreamConfig::default(),
                EncodedRecordingReceiver::from_receiver(receiver),
                |path| {
                    let file = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(path)
                        .unwrap();
                    Ok(Box::new(FailingFile {
                        file,
                        payload: payload.clone(),
                        packets: 0,
                        fail_packet: failure == "packet",
                        fail_flush: failure == "trailer",
                        failed: Arc::clone(&failed),
                    }))
                },
                |_, _| {
                    published.store(true, Ordering::Release);
                    Err("injected publication failure".to_owned())
                },
            );
            let error = result.unwrap_err();
            assert!(error.contains(failure), "{failure}: {error}");
            assert_eq!(failed.load(Ordering::Acquire), failure != "publication");
            assert_eq!(published.load(Ordering::Acquire), failure == "publication");
            assert!(!output.exists());
            assert!(!part_path_for(&output).unwrap().exists());
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn manual_publication_does_not_overwrite_a_destination_created_during_recording() {
        let directory = recording_test_directory("recording-publish-race");
        let output = directory.join("existing.mkv");
        let (sender, receiver) = channel();
        sender
            .send(frame(MediaCodec::H264, real_h264_keyframe(), 90_000, true))
            .unwrap();
        drop(sender);
        let result = record_matroska_with(
            &output,
            MediaStreamConfig::default(),
            EncodedRecordingReceiver::from_receiver(receiver),
            |path| {
                std::fs::write(&output, b"existing destination").unwrap();
                Ok(Box::new(
                    OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(path)
                        .unwrap(),
                ))
            },
            |part, output| {
                publish_manual_recording(part, output).map_err(|error| error.to_string())
            },
        );
        assert!(result.is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"existing destination");
        assert!(!part_path_for(&output).unwrap().exists());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_manual_publication_preserves_long_unicode_paths_and_existing_targets() {
        let directory = recording_test_directory("recording-long-path");
        let mut nested = directory.clone();
        for _ in 0..8 {
            nested.push("recording-長い名前-abcdefghijklmnopqrstuvwxyz");
        }
        std::fs::create_dir_all(&nested).unwrap();
        let part = nested.join(".capture.mkv.part");
        let output = nested.join("capture.mkv");
        std::fs::write(&part, b"valid prefix").unwrap();
        publish_manual_recording(&part, &output).unwrap();
        assert!(!part.exists());
        assert_eq!(std::fs::read(&output).unwrap(), b"valid prefix");
        std::fs::write(&part, b"later prefix").unwrap();
        assert!(publish_manual_recording(&part, &output).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"valid prefix");
        assert_eq!(std::fs::read(&part).unwrap(), b"later prefix");
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn writes_atomic_h264_and_opus_matroska() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "opennow-recording-test-{}-{}",
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(&directory).expect("temporary directory");
        let output = directory.join("capture.mkv");
        let (sender, raw_receiver) = channel();
        sender
            .send(frame(MediaCodec::H264, h264_keyframe(), 90_000, true))
            .unwrap();
        sender
            .send(frame(
                MediaCodec::Opus { channels: 2 },
                vec![0x80, 1, 2, 3],
                48_000,
                false,
            ))
            .unwrap();
        sender
            .send(frame(
                MediaCodec::H264,
                vec![0, 0, 0, 1, 0x41, 0x9a, 0x22],
                91_500,
                false,
            ))
            .unwrap();
        drop(sender);

        let summary = record_matroska(
            &output,
            MediaStreamConfig {
                width: 1920,
                height: 1080,
                fps: 60,
                ..MediaStreamConfig::default()
            },
            EncodedRecordingReceiver::from_receiver(raw_receiver),
        )
        .expect("recording");
        assert_eq!(summary.media.video_packets, 2);
        assert_eq!(summary.media.audio_packets, 1);
        assert_eq!(summary.completion, RecordingCompletion::Complete);
        assert!(output.exists());
        assert!(!part_path_for(&output).unwrap().exists());

        let file: Box<dyn ReadSeek> = Box::new(std::fs::File::open(&output).unwrap());
        let mut demuxer = oxideav_mkv::demux::open(file, &NullCodecResolver).unwrap();
        assert_eq!(demuxer.streams().len(), 2);
        assert_eq!(demuxer.streams()[0].params.codec_id.as_str(), "h264");
        assert_eq!(demuxer.streams()[1].params.codec_id.as_str(), "opus");
        let mut packets = 0;
        while demuxer.next_packet().is_ok() {
            packets += 1;
        }
        assert_eq!(packets, 3);

        std::fs::remove_file(output).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn timestamp_delta_handles_rtp_wrap() {
        assert_eq!(rtp_timestamp_delta(u64::from(u32::MAX) - 10, 20), 31);
        assert_eq!(rtp_timestamp_delta(100, 90), 0);
    }

    #[test]
    fn h265_repack_builds_configuration_and_length_prefixes() {
        // Minimal synthetic SPS bitstream for parser coverage. The profile fields are Main,
        // level 4.0, one temporal layer, 4:2:0, 8-bit, 1920x1080.
        let sps = synthetic_hevc_sps();
        let mut input = Vec::new();
        for nal in [
            &[0x40, 0x01, 0x0c][..],
            sps.as_slice(),
            &[0x44, 0x01, 0xc0][..],
            &[0x26, 0x01, 0xaa, 0xbb][..],
        ] {
            input.extend_from_slice(&[0, 0, 0, 1]);
            input.extend_from_slice(nal);
        }
        let repacked = annexb_to_hvcc(&input).expect("HEVC repack");
        assert_eq!(repacked.config_record[0], 1);
        assert_eq!(repacked.config_record[22], 3);
        assert_eq!(
            repacked.packetized,
            vec![0, 0, 0, 4, 0x26, 0x01, 0xaa, 0xbb]
        );
    }

    #[test]
    fn av1_configuration_is_derived_from_the_sequence_header_obu() {
        let sequence_header = b"\x0a\x0f\0\0\0j\xef\xbf\xe1\xbc\x02\x19\x90\x10\x10\x10@";
        let config = av1_codec_private(sequence_header).expect("AV1 configuration");
        assert_eq!(&config[..4], &[0x81, 0x0d, 0x0c, 0x00]);
        assert_eq!(&config[4..], sequence_header);
    }

    fn synthetic_hevc_sps() -> Vec<u8> {
        let mut bits = BitWriter::default();
        bits.write(0, 4); // sps_video_parameter_set_id
        bits.write(0, 3); // max_sub_layers_minus1
        bits.write(1, 1); // temporal nesting
        bits.write(0, 2); // profile space
        bits.write(0, 1); // tier
        bits.write(1, 5); // Main profile
        bits.write(0x6000_0000, 32); // compatibility
        bits.write(0, 48); // constraints
        bits.write(120, 8); // level 4.0
        bits.write_ue(0); // sps id
        bits.write_ue(1); // 4:2:0
        bits.write_ue(1920);
        bits.write_ue(1080);
        bits.write(0, 1); // no conformance window
        bits.write_ue(0); // 8-bit luma
        bits.write_ue(0); // 8-bit chroma
        let mut nal = vec![0x42, 0x01];
        nal.extend_from_slice(&bits.finish());
        nal
    }

    #[derive(Default)]
    struct BitWriter {
        bits: Vec<bool>,
    }

    impl BitWriter {
        fn write(&mut self, value: u64, count: usize) {
            for shift in (0..count).rev() {
                self.bits.push(((value >> shift) & 1) != 0);
            }
        }

        fn write_ue(&mut self, value: u64) {
            let code = value + 1;
            let width = (64 - code.leading_zeros()) as usize;
            self.write(0, width - 1);
            self.write(code, width);
        }

        fn finish(mut self) -> Vec<u8> {
            self.bits.push(true);
            while !self.bits.len().is_multiple_of(8) {
                self.bits.push(false);
            }
            self.bits
                .chunks(8)
                .map(|chunk| {
                    chunk
                        .iter()
                        .fold(0_u8, |value, bit| (value << 1) | u8::from(*bit))
                })
                .collect()
        }
    }
}
