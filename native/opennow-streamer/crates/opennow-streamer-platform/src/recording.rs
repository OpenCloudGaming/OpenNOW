use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingSummary {
    pub path: PathBuf,
    pub video_packets: u64,
    pub audio_packets: u64,
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
) -> Result<RecordingSummary, String> {
    let output_path = validate_output_path(output_path.as_ref())?;
    let part_path = part_path_for(&output_path)?;
    if output_path.exists() {
        return Err(format!(
            "recording output already exists: {}",
            output_path.display()
        ));
    }

    let result = record_matroska_inner(&part_path, &output_path, stream, &receiver);
    if result.is_err() {
        let _ = std::fs::remove_file(&part_path);
    }
    result
}

fn record_matroska_inner(
    part_path: &Path,
    output_path: &Path,
    stream: MediaStreamConfig,
    receiver: &EncodedRecordingReceiver,
) -> Result<RecordingSummary, String> {
    let mut active: Option<ActiveMuxer> = None;
    let mut audio_channels = 2_u8;

    while let Ok(frame) = receiver.recv() {
        if !frame.contiguous {
            return Err(format!(
                "recording stopped because the {} stream was discontinuous",
                frame.mid
            ));
        }

        if let MediaCodec::Opus { channels } = frame.codec {
            audio_channels = channels.clamp(1, 2);
        }

        if active.is_none() {
            if !frame.keyframe || !is_video_codec(&frame.codec, stream.codec) {
                continue;
            }
            active = Some(start_muxer(part_path, stream, audio_channels, &frame)?);
        }

        write_frame(active.as_mut().expect("muxer was initialized"), frame)?;
    }

    if receiver.overflowed() {
        return Err("recording stopped because its bounded media queue overflowed".to_owned());
    }

    let Some(mut active) = active else {
        return Err("recording ended before a decodable video keyframe arrived".to_owned());
    };
    active
        .muxer
        .write_trailer()
        .map_err(|error| format!("failed to finalize Matroska recording: {error}"))?;
    std::fs::rename(part_path, output_path)
        .map_err(|error| format!("failed to publish completed recording: {error}"))?;
    Ok(RecordingSummary {
        path: output_path.to_owned(),
        video_packets: active.video_packets,
        audio_packets: active.audio_packets,
    })
}

fn start_muxer(
    path: &Path,
    stream: MediaStreamConfig,
    audio_channels: u8,
    first_video: &EncodedFrame,
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

    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to create recording: {error}"))?;
    let output: Box<dyn WriteSeek> = Box::new(file);
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
    let (stream_index, pts, data, keyframe) = match frame.codec {
        MediaCodec::H264 if active.video_codec == MediaVideoCodec::H264 => {
            let repacked = annexb_to_avcc(&frame.data);
            if repacked.packetized.is_empty() {
                return Ok(());
            }
            (
                VIDEO_STREAM_INDEX,
                active.video_clock.pts(frame.timestamp),
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
                active.video_clock.pts(frame.timestamp),
                repacked.packetized,
                frame.keyframe,
            )
        }
        MediaCodec::Av1 if active.video_codec == MediaVideoCodec::Av1 => (
            VIDEO_STREAM_INDEX,
            active.video_clock.pts(frame.timestamp),
            frame.data.to_vec(),
            frame.keyframe,
        ),
        MediaCodec::Opus { .. } => (
            AUDIO_STREAM_INDEX,
            active.audio_clock.pts(frame.timestamp),
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
        assert_eq!(summary.video_packets, 2);
        assert_eq!(summary.audio_packets, 1);
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
