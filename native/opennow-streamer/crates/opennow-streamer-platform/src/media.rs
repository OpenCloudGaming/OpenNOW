use std::sync::Arc;
#[cfg(target_os = "macos")]
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};

use openh264::OpenH264API;
use openh264::decoder::{Decoder as OpenH264Decoder, DecoderConfig};
use openh264::formats::YUVSource;
use opus::{Channels, Decoder as OpusNativeDecoder};

use crate::output::{DecodedVideoFrame, OutputBuffers};
use crate::queue::{BoundedQueue, PushResult};
use crate::runtime::HostCommand;

const VIDEO_QUEUE_CAPACITY: usize = 3;
const AUDIO_QUEUE_CAPACITY: usize = 12;
const OPUS_SAMPLE_RATE: u32 = 48_000;
const MAX_OPUS_FRAME_SAMPLES_PER_CHANNEL: usize = 5_760;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaCodec {
    H264,
    Opus { channels: u8 },
    Unsupported(String),
}

#[derive(Debug, Clone)]
pub struct EncodedFrame {
    pub mid: String,
    pub codec: MediaCodec,
    pub data: Arc<[u8]>,
    pub timestamp: u64,
    pub clock_rate_hz: u32,
    pub keyframe: bool,
    pub contiguous: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaFeedback {
    PlaybackStarted {
        backend: &'static str,
    },
    BackendFallback {
        from: &'static str,
        to: &'static str,
        reason: String,
    },
    RequestKeyframe {
        mid: String,
        reason: String,
    },
    DecoderError {
        codec: &'static str,
        message: String,
    },
    QueueDropped {
        media: &'static str,
        count: usize,
    },
    OutputError {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    Queued,
    DroppedOldest,
    Paused,
    Unsupported,
    Closed,
}

struct SharedPipeline {
    video: Arc<BoundedQueue<EncodedFrame>>,
    audio: Arc<BoundedQueue<EncodedFrame>>,
    output: Arc<OutputBuffers>,
    feedback: Sender<MediaFeedback>,
    paused: AtomicBool,
    video_desynced: AtomicBool,
    keyframe_requested: AtomicBool,
    stopped: AtomicBool,
    #[cfg(target_os = "macos")]
    mac_sink: Mutex<Option<opennow_streamer_platform_macos::StreamSink>>,
    #[cfg(target_os = "macos")]
    mac_software_fallback: AtomicBool,
}

#[derive(Clone)]
pub struct MediaSink {
    shared: Arc<SharedPipeline>,
}

impl MediaSink {
    pub fn push(&self, frame: EncodedFrame) -> PushOutcome {
        if self.shared.stopped.load(Ordering::Acquire) {
            return PushOutcome::Closed;
        }
        if self.shared.paused.load(Ordering::Acquire) {
            return PushOutcome::Paused;
        }
        match frame.codec {
            MediaCodec::H264 => self.push_video(frame),
            MediaCodec::Opus { .. } => self.push_audio(frame),
            MediaCodec::Unsupported(_) => PushOutcome::Unsupported,
        }
    }

    fn push_video(&self, frame: EncodedFrame) -> PushOutcome {
        if !frame.keyframe && self.shared.video_desynced.load(Ordering::Acquire) {
            self.mark_video_desynced(&frame.mid, "waiting for a decodable H.264 keyframe");
        } else if !frame.contiguous {
            self.mark_video_desynced(&frame.mid, "RTP video discontinuity");
        }
        let mid = frame.mid.clone();
        match self.shared.video.push(frame) {
            PushResult::Queued => PushOutcome::Queued,
            PushResult::DroppedOldest => {
                self.mark_video_desynced(&mid, "encoded video queue overflow");
                let _ = self.shared.feedback.send(MediaFeedback::QueueDropped {
                    media: "video",
                    count: 1,
                });
                PushOutcome::DroppedOldest
            }
            PushResult::Closed => PushOutcome::Closed,
        }
    }

    fn push_audio(&self, frame: EncodedFrame) -> PushOutcome {
        match self.shared.audio.push(frame) {
            PushResult::Queued => PushOutcome::Queued,
            PushResult::DroppedOldest => {
                let _ = self.shared.feedback.send(MediaFeedback::QueueDropped {
                    media: "audio",
                    count: 1,
                });
                PushOutcome::DroppedOldest
            }
            PushResult::Closed => PushOutcome::Closed,
        }
    }

    fn mark_video_desynced(&self, mid: &str, reason: &str) {
        self.shared.video_desynced.store(true, Ordering::Release);
        if !self.shared.keyframe_requested.swap(true, Ordering::AcqRel) {
            let _ = self.shared.feedback.send(MediaFeedback::RequestKeyframe {
                mid: mid.to_owned(),
                reason: reason.to_owned(),
            });
        }
    }
}

pub struct MediaSession {
    sink: MediaSink,
    video_worker: Option<JoinHandle<()>>,
    audio_worker: Option<JoinHandle<()>>,
    host_commands: Sender<HostCommand>,
}

impl MediaSession {
    pub(crate) fn spawn(
        output: Arc<OutputBuffers>,
        feedback: Sender<MediaFeedback>,
        host_commands: Sender<HostCommand>,
        use_macos_hardware: bool,
    ) -> Result<Self, String> {
        #[cfg(target_os = "macos")]
        if use_macos_hardware {
            return Self::spawn_macos(output, feedback, host_commands);
        }
        let _ = use_macos_hardware;
        let video_decoder = H264Decoder::new()?;
        let audio_decoder = OpusDecoder::new(2)?;
        let shared = Arc::new(SharedPipeline {
            video: Arc::new(BoundedQueue::new(VIDEO_QUEUE_CAPACITY)),
            audio: Arc::new(BoundedQueue::new(AUDIO_QUEUE_CAPACITY)),
            output,
            feedback,
            paused: AtomicBool::new(false),
            video_desynced: AtomicBool::new(true),
            keyframe_requested: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            #[cfg(target_os = "macos")]
            mac_sink: Mutex::new(None),
            #[cfg(target_os = "macos")]
            mac_software_fallback: AtomicBool::new(false),
        });
        let video_shared = Arc::clone(&shared);
        let video_worker = thread::Builder::new()
            .name("opennow-h264-decode".to_owned())
            .spawn(move || run_video_decoder(video_shared, video_decoder))
            .map_err(|error| format!("failed to start H.264 decoder worker: {error}"))?;
        let audio_shared = Arc::clone(&shared);
        let audio_worker = match thread::Builder::new()
            .name("opennow-opus-decode".to_owned())
            .spawn(move || run_audio_decoder(audio_shared, audio_decoder))
        {
            Ok(worker) => worker,
            Err(error) => {
                shared.video.close();
                let _ = video_worker.join();
                return Err(format!("failed to start Opus decoder worker: {error}"));
            }
        };
        Ok(Self {
            sink: MediaSink { shared },
            video_worker: Some(video_worker),
            audio_worker: Some(audio_worker),
            host_commands,
        })
    }

    #[cfg(target_os = "macos")]
    fn spawn_macos(
        output: Arc<OutputBuffers>,
        feedback: Sender<MediaFeedback>,
        host_commands: Sender<HostCommand>,
    ) -> Result<Self, String> {
        let shared = Arc::new(SharedPipeline {
            video: Arc::new(BoundedQueue::new(VIDEO_QUEUE_CAPACITY)),
            audio: Arc::new(BoundedQueue::new(AUDIO_QUEUE_CAPACITY)),
            output,
            feedback,
            paused: AtomicBool::new(false),
            video_desynced: AtomicBool::new(true),
            keyframe_requested: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            mac_sink: Mutex::new(None),
            mac_software_fallback: AtomicBool::new(false),
        });
        let video_shared = Arc::clone(&shared);
        let video_commands = host_commands.clone();
        let video_worker = thread::Builder::new()
            .name("opennow-videotoolbox-submit".to_owned())
            .spawn(move || run_macos_video(video_shared, video_commands))
            .map_err(|error| format!("failed to start VideoToolbox submit worker: {error}"))?;
        let audio_shared = Arc::clone(&shared);
        let audio_worker = match thread::Builder::new()
            .name("opennow-coreaudio-submit".to_owned())
            .spawn(move || run_macos_audio(audio_shared))
        {
            Ok(worker) => worker,
            Err(error) => {
                shared.video.close();
                let _ = video_worker.join();
                return Err(format!("failed to start CoreAudio submit worker: {error}"));
            }
        };
        Ok(Self {
            sink: MediaSink { shared },
            video_worker: Some(video_worker),
            audio_worker: Some(audio_worker),
            host_commands,
        })
    }

    pub fn sink(&self) -> MediaSink {
        self.sink.clone()
    }

    pub fn set_paused(&self, paused: bool) {
        self.sink.shared.paused.store(paused, Ordering::Release);
        self.sink.shared.video.clear();
        self.sink.shared.audio.clear();
        self.sink.shared.output.clear();
        if !paused {
            self.sink
                .shared
                .video_desynced
                .store(true, Ordering::Release);
            self.sink
                .shared
                .keyframe_requested
                .store(false, Ordering::Release);
        }
        let _ = self.host_commands.send(HostCommand::Pause {
            paused,
            reply: None,
        });
    }

    pub fn stop(mut self) {
        self.stop_inner();
    }

    fn stop_inner(&mut self) {
        if self.sink.shared.stopped.swap(true, Ordering::AcqRel) {
            return;
        }
        self.sink.shared.video.close();
        self.sink.shared.audio.close();
        if let Some(worker) = self.video_worker.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.audio_worker.take() {
            let _ = worker.join();
        }
        #[cfg(target_os = "macos")]
        {
            self.sink
                .shared
                .mac_sink
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .take();
        }
        self.sink.shared.output.clear();
        let _ = self.host_commands.send(HostCommand::Stop);
    }
}

impl Drop for MediaSession {
    fn drop(&mut self) {
        self.stop_inner();
    }
}

struct H264Decoder {
    decoder: OpenH264Decoder,
}

impl H264Decoder {
    fn new() -> Result<Self, String> {
        OpenH264Decoder::with_api_config(
            OpenH264API::from_source(),
            DecoderConfig::new().debug(false),
        )
        .map(|decoder| Self { decoder })
        .map_err(|error| format!("OpenH264 decoder initialization failed: {error}"))
    }

    fn decode(&mut self, encoded: &[u8]) -> Result<Option<DecodedVideoFrame>, String> {
        let Some(yuv) = self
            .decoder
            .decode(encoded)
            .map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        let (width, height) = yuv.dimensions();
        let mut rgb = vec![0; yuv.rgb8_len()];
        yuv.write_rgb8(&mut rgb);
        Ok(Some(DecodedVideoFrame {
            width: width as u32,
            height: height as u32,
            rgb,
        }))
    }
}

struct OpusDecoder {
    decoder: OpusNativeDecoder,
    channels: u8,
    scratch: Vec<f32>,
}

impl OpusDecoder {
    fn new(channels: u8) -> Result<Self, String> {
        let opus_channels = match channels {
            1 => Channels::Mono,
            2 => Channels::Stereo,
            other => return Err(format!("unsupported Opus channel count: {other}")),
        };
        OpusNativeDecoder::new(OPUS_SAMPLE_RATE, opus_channels)
            .map(|decoder| Self {
                decoder,
                channels,
                scratch: vec![0.0; MAX_OPUS_FRAME_SAMPLES_PER_CHANNEL * channels as usize],
            })
            .map_err(|error| format!("Opus decoder initialization failed: {error}"))
    }

    fn decode(&mut self, encoded: &[u8]) -> Result<&[f32], String> {
        let samples_per_channel = self
            .decoder
            .decode_float(encoded, &mut self.scratch, false)
            .map_err(|error| error.to_string())?;
        Ok(&self.scratch[..samples_per_channel * self.channels as usize])
    }
}

fn run_video_decoder(shared: Arc<SharedPipeline>, decoder: H264Decoder) {
    run_video_decoder_from(shared, decoder, None);
}

fn run_video_decoder_from(
    shared: Arc<SharedPipeline>,
    mut decoder: H264Decoder,
    mut pending: Option<EncodedFrame>,
) {
    loop {
        let Some(frame) = pending.take().or_else(|| shared.video.pop()) else {
            return;
        };
        if shared.paused.load(Ordering::Acquire) {
            continue;
        }
        if shared.video_desynced.load(Ordering::Acquire) {
            if !frame.keyframe {
                continue;
            }
            match H264Decoder::new() {
                Ok(new_decoder) => decoder = new_decoder,
                Err(message) => {
                    let _ = shared.feedback.send(MediaFeedback::DecoderError {
                        codec: "h264",
                        message,
                    });
                    continue;
                }
            }
            shared.video_desynced.store(false, Ordering::Release);
            shared.keyframe_requested.store(false, Ordering::Release);
        }
        match decoder.decode(&frame.data) {
            Ok(Some(decoded)) => {
                if shared.output.replace_video(decoded) {
                    let _ = shared.feedback.send(MediaFeedback::QueueDropped {
                        media: "present",
                        count: 1,
                    });
                }
            }
            Ok(None) => {}
            Err(message) => {
                let _ = shared.feedback.send(MediaFeedback::DecoderError {
                    codec: "h264",
                    message,
                });
                shared.video_desynced.store(true, Ordering::Release);
                if !shared.keyframe_requested.swap(true, Ordering::AcqRel) {
                    let _ = shared.feedback.send(MediaFeedback::RequestKeyframe {
                        mid: frame.mid,
                        reason: "H.264 decoder rejected an access unit".to_owned(),
                    });
                }
            }
        }
    }
}

fn run_audio_decoder(shared: Arc<SharedPipeline>, decoder: OpusDecoder) {
    run_audio_decoder_from(shared, decoder, None);
}

fn run_audio_decoder_from(
    shared: Arc<SharedPipeline>,
    mut decoder: OpusDecoder,
    mut pending: Option<EncodedFrame>,
) {
    let mut configured_channels = 2;
    loop {
        let Some(frame) = pending.take().or_else(|| shared.audio.pop()) else {
            return;
        };
        if shared.paused.load(Ordering::Acquire) {
            continue;
        }
        let MediaCodec::Opus { channels } = frame.codec else {
            continue;
        };
        let channels = channels.clamp(1, 2);
        if channels != configured_channels {
            match OpusDecoder::new(channels) {
                Ok(new_decoder) => {
                    decoder = new_decoder;
                    configured_channels = channels;
                }
                Err(message) => {
                    let _ = shared.feedback.send(MediaFeedback::DecoderError {
                        codec: "opus",
                        message,
                    });
                    continue;
                }
            }
        }
        match decoder.decode(&frame.data) {
            Ok(samples) => {
                if configured_channels == 1 {
                    let mut stereo = Vec::with_capacity(samples.len() * 2);
                    for sample in samples {
                        stereo.extend([*sample, *sample]);
                    }
                    let dropped = shared.output.push_audio(&stereo);
                    if dropped > 0 {
                        let _ = shared.feedback.send(MediaFeedback::QueueDropped {
                            media: "audio-output",
                            count: dropped,
                        });
                    }
                } else {
                    let dropped = shared.output.push_audio(samples);
                    if dropped > 0 {
                        let _ = shared.feedback.send(MediaFeedback::QueueDropped {
                            media: "audio-output",
                            count: dropped,
                        });
                    }
                }
            }
            Err(message) => {
                let _ = shared.feedback.send(MediaFeedback::DecoderError {
                    codec: "opus",
                    message,
                });
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn run_macos_video(shared: Arc<SharedPipeline>, host_commands: Sender<HostCommand>) {
    use std::sync::mpsc;
    use std::time::Duration;

    use opennow_streamer_platform_macos::{
        FrameTiming, H264Format, SubmitOutcome, VideoColorSpace,
    };

    use crate::runtime::MacH264Configuration;

    let mut tracker = crate::macos_backend::H264ParameterSetTracker::default();
    let mut configured_parameter_sets = None;
    let mut backend_sink = None;
    let mut playback_started = false;
    while let Some(frame) = shared.video.pop() {
        if shared.paused.load(Ordering::Acquire) {
            continue;
        }
        let framing = match tracker.observe(&frame.data) {
            Ok(framing) => framing,
            Err(message) => {
                let _ = shared.feedback.send(MediaFeedback::DecoderError {
                    codec: "h264",
                    message,
                });
                mark_macos_video_desynced(&shared, &frame.mid, "invalid H.264 framing");
                continue;
            }
        };
        let parameter_sets = match tracker.parameter_sets() {
            Ok(parameter_sets) => parameter_sets,
            Err(message) => {
                let _ = shared.feedback.send(MediaFeedback::DecoderError {
                    codec: "h264",
                    message,
                });
                mark_macos_video_desynced(&shared, &frame.mid, "invalid H.264 parameter sets");
                continue;
            }
        };
        if shared.video_desynced.load(Ordering::Acquire) && !frame.keyframe {
            continue;
        }
        if backend_sink.is_none() {
            let Some(parameter_sets) = parameter_sets.clone() else {
                mark_macos_video_desynced(
                    &shared,
                    &frame.mid,
                    "VideoToolbox is waiting for H.264 SPS/PPS",
                );
                continue;
            };
            let (reply, response) = mpsc::channel();
            if host_commands
                .send(HostCommand::ConfigureMacH264 {
                    parameter_sets: parameter_sets.clone(),
                    reply,
                })
                .is_err()
            {
                return;
            }
            match response.recv_timeout(Duration::from_secs(10)) {
                Ok(Ok(MacH264Configuration::Hardware(sink))) => {
                    *shared
                        .mac_sink
                        .lock()
                        .unwrap_or_else(|error| error.into_inner()) = Some(sink.clone());
                    configured_parameter_sets = Some(parameter_sets);
                    backend_sink = Some(sink);
                }
                Ok(Ok(MacH264Configuration::SoftwareFallback { reason })) => {
                    shared.mac_software_fallback.store(true, Ordering::Release);
                    let _ = shared.feedback.send(MediaFeedback::BackendFallback {
                        from: "VideoToolbox/Metal",
                        to: "OpenH264/SDL",
                        reason,
                    });
                    match H264Decoder::new() {
                        Ok(decoder) => run_video_decoder_from(shared, decoder, Some(frame)),
                        Err(message) => {
                            let _ = shared.feedback.send(MediaFeedback::DecoderError {
                                codec: "h264",
                                message,
                            });
                        }
                    }
                    return;
                }
                Ok(Err(message)) => {
                    let _ = shared.feedback.send(MediaFeedback::DecoderError {
                        codec: "h264",
                        message,
                    });
                    return;
                }
                Err(_) => {
                    let _ = shared.feedback.send(MediaFeedback::DecoderError {
                        codec: "h264",
                        message: "VideoToolbox initialization timed out on the main thread"
                            .to_owned(),
                    });
                    return;
                }
            }
        } else if let Some(parameter_sets) = parameter_sets
            && configured_parameter_sets.as_ref() != Some(&parameter_sets)
        {
            let format = H264Format::new(parameter_sets.clone(), VideoColorSpace::Bt709);
            let Some(sink) = backend_sink.as_ref() else {
                return;
            };
            if let Err(error) = sink.reconfigure_h264(format) {
                let _ = shared.feedback.send(MediaFeedback::DecoderError {
                    codec: "h264",
                    message: error.to_string(),
                });
                mark_macos_video_desynced(
                    &shared,
                    &frame.mid,
                    "VideoToolbox reconfiguration failed",
                );
                continue;
            }
            configured_parameter_sets = Some(parameter_sets);
        }

        shared.video_desynced.store(false, Ordering::Release);
        shared.keyframe_requested.store(false, Ordering::Release);
        let timescale = i32::try_from(frame.clock_rate_hz)
            .ok()
            .filter(|timescale| *timescale > 0)
            .unwrap_or(90_000);
        let timing = FrameTiming::new(
            i64::try_from(frame.timestamp).unwrap_or(i64::MAX),
            0,
            timescale,
        );
        let Some(sink) = backend_sink.as_ref() else {
            return;
        };
        match sink.submit_h264(&frame.data, framing, timing) {
            Ok(SubmitOutcome::Accepted | SubmitOutcome::Paused) => {}
            Ok(SubmitOutcome::Backpressured | SubmitOutcome::ReplacedOldest) => {
                let _ = shared.feedback.send(MediaFeedback::QueueDropped {
                    media: "videotoolbox",
                    count: 1,
                });
                mark_macos_video_desynced(
                    &shared,
                    &frame.mid,
                    "VideoToolbox decode queue backpressure",
                );
            }
            Err(error) => {
                let _ = shared.feedback.send(MediaFeedback::DecoderError {
                    codec: "h264",
                    message: error.to_string(),
                });
                mark_macos_video_desynced(
                    &shared,
                    &frame.mid,
                    "VideoToolbox rejected an H.264 access unit",
                );
            }
        }
        if !playback_started && sink.stats().video_presented > 0 {
            playback_started = true;
            let _ = shared.feedback.send(MediaFeedback::PlaybackStarted {
                backend: "VideoToolbox/Metal",
            });
        }
    }
}

#[cfg(target_os = "macos")]
fn run_macos_audio(shared: Arc<SharedPipeline>) {
    use opennow_streamer_platform_macos::{AudioFormat, SubmitOutcome};

    let mut configured_channels = 2;
    while let Some(frame) = shared.audio.pop() {
        if shared.paused.load(Ordering::Acquire) {
            continue;
        }
        if shared.mac_software_fallback.load(Ordering::Acquire) {
            match OpusDecoder::new(2) {
                Ok(decoder) => run_audio_decoder_from(shared, decoder, Some(frame)),
                Err(message) => {
                    let _ = shared.feedback.send(MediaFeedback::DecoderError {
                        codec: "opus",
                        message,
                    });
                }
            }
            return;
        }
        let MediaCodec::Opus { channels } = frame.codec else {
            continue;
        };
        let Some(sink) = shared
            .mac_sink
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
        else {
            continue;
        };
        let channels = channels.clamp(1, 2);
        if channels != configured_channels {
            if let Err(error) = sink.reconfigure_audio(AudioFormat::new(48_000, channels)) {
                let _ = shared.feedback.send(MediaFeedback::DecoderError {
                    codec: "opus",
                    message: error.to_string(),
                });
                continue;
            }
            configured_channels = channels;
        }
        match sink.submit_opus(&frame.data) {
            Ok(SubmitOutcome::Accepted | SubmitOutcome::Backpressured | SubmitOutcome::Paused) => {}
            Ok(SubmitOutcome::ReplacedOldest) => {
                let _ = shared.feedback.send(MediaFeedback::QueueDropped {
                    media: "coreaudio",
                    count: 1,
                });
            }
            Err(error) => {
                if !shared.stopped.load(Ordering::Acquire) {
                    let _ = shared.feedback.send(MediaFeedback::DecoderError {
                        codec: "opus",
                        message: error.to_string(),
                    });
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn mark_macos_video_desynced(shared: &SharedPipeline, mid: &str, reason: &str) {
    shared.video_desynced.store(true, Ordering::Release);
    if !shared.keyframe_requested.swap(true, Ordering::AcqRel) {
        let _ = shared.feedback.send(MediaFeedback::RequestKeyframe {
            mid: mid.to_owned(),
            reason: reason.to_owned(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openh264::encoder::Encoder;
    use openh264::formats::{RgbSliceU8, YUVBuffer};
    use opus::{Application, Encoder as OpusEncoder};

    #[test]
    fn decodes_a_synthetic_h264_keyframe() {
        let width = 32;
        let height = 32;
        let mut rgb = vec![0_u8; width * height * 3];
        for (index, pixel) in rgb.chunks_exact_mut(3).enumerate() {
            pixel.copy_from_slice(&[(index % 255) as u8, 64, 192]);
        }
        let yuv = YUVBuffer::from_rgb_source(RgbSliceU8::new(&rgb, (width, height)));
        let mut encoder = Encoder::new().expect("encoder");
        let encoded = encoder.encode(&yuv).expect("encode").to_vec();
        let mut decoder = H264Decoder::new().expect("decoder");
        let decoded = decoder
            .decode(&encoded)
            .expect("decode")
            .expect("decoded frame");
        assert_eq!(
            (decoded.width, decoded.height),
            (width as u32, height as u32)
        );
        assert_eq!(decoded.rgb.len(), width * height * 3);
    }

    #[test]
    fn software_handoff_decodes_the_pending_h264_keyframe() {
        let width = 32;
        let height = 32;
        let rgb = vec![96_u8; width * height * 3];
        let yuv = YUVBuffer::from_rgb_source(RgbSliceU8::new(&rgb, (width, height)));
        let mut encoder = Encoder::new().expect("encoder");
        let encoded: Arc<[u8]> = encoder.encode(&yuv).expect("encode").to_vec().into();
        let output = Arc::new(OutputBuffers::new());
        let (feedback, _receiver) = std::sync::mpsc::channel();
        let shared = Arc::new(SharedPipeline {
            video: Arc::new(BoundedQueue::new(VIDEO_QUEUE_CAPACITY)),
            audio: Arc::new(BoundedQueue::new(AUDIO_QUEUE_CAPACITY)),
            output: Arc::clone(&output),
            feedback,
            paused: AtomicBool::new(false),
            video_desynced: AtomicBool::new(true),
            keyframe_requested: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            #[cfg(target_os = "macos")]
            mac_sink: Mutex::new(None),
            #[cfg(target_os = "macos")]
            mac_software_fallback: AtomicBool::new(true),
        });
        shared.video.close();
        run_video_decoder_from(
            shared,
            H264Decoder::new().expect("decoder"),
            Some(EncodedFrame {
                mid: "video".to_owned(),
                codec: MediaCodec::H264,
                data: encoded,
                timestamp: 0,
                clock_rate_hz: 90_000,
                keyframe: true,
                contiguous: true,
            }),
        );
        let decoded = output.take_video().expect("decoded pending frame");
        assert_eq!(
            (decoded.width, decoded.height),
            (width as u32, height as u32)
        );
    }

    #[test]
    fn decodes_synthetic_stereo_opus() {
        let mut encoder = OpusEncoder::new(OPUS_SAMPLE_RATE, Channels::Stereo, Application::Audio)
            .expect("encoder");
        let input: Vec<f32> = (0..960 * 2)
            .map(|sample| ((sample as f32 / 24.0).sin()) * 0.25)
            .collect();
        let mut packet = vec![0_u8; 4_000];
        let encoded_len = encoder.encode_float(&input, &mut packet).expect("encode");
        let mut decoder = OpusDecoder::new(2).expect("decoder");
        let decoded = decoder.decode(&packet[..encoded_len]).expect("decode");
        assert_eq!(decoded.len(), input.len());
        assert!(decoded.iter().any(|sample| sample.abs() > 0.001));
    }

    #[test]
    fn paused_and_stopped_sessions_reject_frames() {
        let (feedback, _receiver) = std::sync::mpsc::channel();
        let (commands, _host) = std::sync::mpsc::channel();
        let session =
            MediaSession::spawn(Arc::new(OutputBuffers::new()), feedback, commands, false)
                .expect("session");
        let sink = session.sink();
        session.set_paused(true);
        assert_eq!(
            sink.push(EncodedFrame {
                mid: "video".to_owned(),
                codec: MediaCodec::H264,
                data: Arc::from([]),
                timestamp: 0,
                clock_rate_hz: 90_000,
                keyframe: false,
                contiguous: true,
            }),
            PushOutcome::Paused
        );
        session.stop();
        assert_eq!(
            sink.push(EncodedFrame {
                mid: "video".to_owned(),
                codec: MediaCodec::H264,
                data: Arc::from([]),
                timestamp: 0,
                clock_rate_hz: 90_000,
                keyframe: false,
                contiguous: true,
            }),
            PushOutcome::Closed
        );
    }

    #[test]
    fn requests_a_keyframe_when_video_starts_mid_gop() {
        let (feedback, receiver) = std::sync::mpsc::channel();
        let (commands, _host) = std::sync::mpsc::channel();
        let session =
            MediaSession::spawn(Arc::new(OutputBuffers::new()), feedback, commands, false)
                .expect("session");
        assert_eq!(
            session.sink().push(EncodedFrame {
                mid: "video".to_owned(),
                codec: MediaCodec::H264,
                data: Arc::from([0_u8, 0, 0, 1, 1]),
                timestamp: 0,
                clock_rate_hz: 90_000,
                keyframe: false,
                contiguous: true,
            }),
            PushOutcome::Queued
        );
        assert!(matches!(
            receiver.recv_timeout(std::time::Duration::from_secs(1)),
            Ok(MediaFeedback::RequestKeyframe { mid, .. }) if mid == "video"
        ));
        session.stop();
    }
}
