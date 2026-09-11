use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvError, Sender, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
#[cfg(target_os = "windows")]
use std::time::Duration;
use std::time::Instant;

use openh264::OpenH264API;
use openh264::decoder::{Decoder as OpenH264Decoder, DecoderConfig};
use openh264::formats::YUVSource;
use opus::{Channels, Decoder as OpusNativeDecoder};

#[cfg(target_os = "windows")]
use crate::output::WindowsBridge;
use crate::output::{DecodedVideoFrame, OutputBuffers};
use crate::queue::{BoundedQueue, PushResult};
use crate::runtime::HostCommand;
use crate::video_queue::VideoQueue;

#[cfg(target_os = "linux")]
use crate::linux_backend::{LinuxVideoPath, LinuxVideoSelection};

const VIDEO_QUEUE_CAPACITY: usize = 2;
#[cfg(target_os = "macos")]
const MAC_VIDEO_QUEUE_MAX_CAPACITY: usize = 60;
// Ten 20 ms Opus packets cover the official client's 200 ms adaptive ceiling.
// The queue remains bounded and drop-oldest, so recovery cannot grow latency
// without limit under a stalled decoder.
const AUDIO_QUEUE_CAPACITY: usize = 10;
const OPUS_SAMPLE_RATE: u32 = 48_000;
const MAX_OPUS_FRAME_SAMPLES_PER_CHANNEL: usize = 5_760;
const RECORDING_TAP_QUEUE_CAPACITY: usize = 256;

pub struct EncodedRecordingReceiver {
    receiver: Receiver<EncodedFrame>,
    overflowed: Arc<AtomicBool>,
}

impl EncodedRecordingReceiver {
    pub fn recv(&self) -> Result<EncodedFrame, RecvError> {
        self.receiver.recv()
    }

    pub fn overflowed(&self) -> bool {
        self.overflowed.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub(crate) fn from_receiver(receiver: Receiver<EncodedFrame>) -> Self {
        Self {
            receiver,
            overflowed: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Default)]
struct RecordingTap {
    sender: Mutex<Option<SyncSender<EncodedFrame>>>,
    overflowed: Arc<AtomicBool>,
}

impl RecordingTap {
    fn subscribe(&self) -> Result<EncodedRecordingReceiver, String> {
        let mut active = self
            .sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if active.is_some() {
            return Err("a native stream recording is already active".to_owned());
        }
        self.overflowed.store(false, Ordering::Release);
        let (sender, receiver) = sync_channel(RECORDING_TAP_QUEUE_CAPACITY);
        *active = Some(sender);
        Ok(EncodedRecordingReceiver {
            receiver,
            overflowed: Arc::clone(&self.overflowed),
        })
    }

    fn unsubscribe(&self) {
        self.sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }

    fn publish(&self, frame: &EncodedFrame) {
        let Ok(mut active) = self.sender.try_lock() else {
            self.overflowed.store(true, Ordering::Release);
            return;
        };
        if self.overflowed.load(Ordering::Acquire) {
            active.take();
            return;
        }
        let Some(sender) = active.as_ref() else {
            return;
        };
        match sender.try_send(frame.clone()) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.overflowed.store(true, Ordering::Release);
                active.take();
            }
            Err(TrySendError::Disconnected(_)) => {
                active.take();
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_video_queue_capacity(fps: u32) -> usize {
    // FEC/NACK intentionally holds an incomplete block for up to 150 ms. Once repaired, several
    // encoded frames can be released together, so keep 250 ms of compressed video to absorb that
    // bounded recovery burst plus AppKit scheduling jitter. Decoded IOSurfaces remain in the small
    // VideoToolbox/Metal queues and never pass through this buffer.
    let frames_for_recovery_burst = fps.max(1).div_ceil(4);
    usize::try_from(frames_for_recovery_burst)
        .unwrap_or(MAC_VIDEO_QUEUE_MAX_CAPACITY)
        .clamp(VIDEO_QUEUE_CAPACITY, MAC_VIDEO_QUEUE_MAX_CAPACITY)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaVideoCodec {
    H264,
    H265,
    Av1,
}

impl MediaVideoCodec {
    pub const fn label(self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::H265 => "h265",
            Self::Av1 => "av1",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutChord {
    pub virtual_key: u16,
    pub modifiers: u16,
}

impl ShortcutChord {
    pub fn parse(value: &str) -> Option<Self> {
        let mut modifiers = 0_u16;
        let mut virtual_key = None;
        for part in value.split('+').map(str::trim) {
            if part.is_empty() {
                return None;
            }
            let normalized = part.to_ascii_lowercase();
            let modifier = match normalized.as_str() {
                "shift" => Some(0x01),
                "ctrl" | "control" => Some(0x02),
                "alt" | "option" => Some(0x04),
                "meta" | "command" | "cmd" | "super" | "win" => Some(0x08),
                key if virtual_key.is_none() => {
                    virtual_key = shortcut_virtual_key(key);
                    None
                }
                _ => return None,
            };
            if let Some(modifier) = modifier {
                if modifiers & modifier != 0 {
                    return None;
                }
                modifiers |= modifier;
            }
        }
        virtual_key.map(|virtual_key| Self {
            virtual_key,
            modifiers,
        })
    }
}

fn shortcut_virtual_key(value: &str) -> Option<u16> {
    if value.len() == 1 {
        let byte = value.as_bytes()[0].to_ascii_uppercase();
        if byte.is_ascii_alphanumeric() {
            return Some(u16::from(byte));
        }
    }
    if let Some(number) = value
        .strip_prefix('f')
        .and_then(|number| number.parse::<u16>().ok())
        && (1..=24).contains(&number)
    {
        return Some(0x6f + number);
    }
    Some(match value {
        "enter" | "return" => 0x0d,
        "escape" | "esc" => 0x1b,
        "backspace" => 0x08,
        "tab" => 0x09,
        "space" => 0x20,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "insert" => 0x2d,
        "delete" | "del" => 0x2e,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" | "pgup" => 0x21,
        "pagedown" | "pgdown" => 0x22,
        "printscreen" | "print" => 0x2a,
        "pause" => 0x13,
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamShortcutAction {
    ToggleStats,
    TogglePointerLock,
    ToggleFullscreen,
    StopStream,
    ToggleAntiAfk,
    Screenshot,
    ToggleRecording,
    SaveClip,
}

impl StreamShortcutAction {
    pub const fn protocol_name(self) -> &'static str {
        match self {
            Self::ToggleStats => "toggle-stats",
            Self::TogglePointerLock => "toggle-pointer-lock",
            Self::ToggleFullscreen => "toggle-fullscreen",
            Self::StopStream => "stop-stream",
            Self::ToggleAntiAfk => "toggle-anti-afk",
            Self::Screenshot => "screenshot",
            Self::ToggleRecording => "toggle-recording",
            Self::SaveClip => "save-clip",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamShortcutBindings {
    bindings: [(StreamShortcutAction, Option<ShortcutChord>); 8],
}

impl StreamShortcutBindings {
    pub fn from_json(value: &serde_json::Value) -> Self {
        let read = |key: &str, fallback: &str| {
            value
                .get(key)
                .and_then(serde_json::Value::as_str)
                .and_then(ShortcutChord::parse)
                .or_else(|| ShortcutChord::parse(fallback))
        };
        Self {
            bindings: [
                (
                    StreamShortcutAction::ToggleStats,
                    read("toggleStats", "Ctrl+N"),
                ),
                (
                    StreamShortcutAction::TogglePointerLock,
                    read("togglePointerLock", "F8"),
                ),
                (
                    StreamShortcutAction::ToggleFullscreen,
                    read("toggleFullscreen", "F11"),
                ),
                (
                    StreamShortcutAction::StopStream,
                    read("stopStream", "Ctrl+Shift+Q"),
                ),
                (
                    StreamShortcutAction::ToggleAntiAfk,
                    read("toggleAntiAfk", "Ctrl+Shift+K"),
                ),
                (
                    StreamShortcutAction::Screenshot,
                    read("screenshot", "Ctrl+F11"),
                ),
                (
                    StreamShortcutAction::ToggleRecording,
                    read("toggleRecording", "F12"),
                ),
                (StreamShortcutAction::SaveClip, read("saveClip", "Ctrl+F12")),
            ],
        }
    }

    pub const fn action(self, virtual_key: u16, modifiers: u16) -> Option<StreamShortcutAction> {
        let mut index = 0;
        while index < self.bindings.len() {
            let (action, chord) = self.bindings[index];
            if let Some(chord) = chord
                && chord.virtual_key == virtual_key
                && chord.modifiers == modifiers
            {
                return Some(action);
            }
            index += 1;
        }
        None
    }
}

impl Default for StreamShortcutBindings {
    fn default() -> Self {
        Self::from_json(&serde_json::Value::Null)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum MediaColorQuality {
    #[default]
    EightBit420,
    EightBit444,
    TenBit420,
    TenBit444,
}

impl MediaColorQuality {
    pub const fn bit_depth(self) -> u8 {
        match self {
            Self::EightBit420 | Self::EightBit444 => 8,
            Self::TenBit420 | Self::TenBit444 => 10,
        }
    }

    pub const fn is_444(self) -> bool {
        matches!(self, Self::EightBit444 | Self::TenBit444)
    }

    #[cfg(target_os = "linux")]
    pub(crate) const fn linux_pixel_format(self) -> opennow_streamer_platform_linux::PixelFormat {
        use opennow_streamer_platform_linux::PixelFormat;
        match self {
            Self::EightBit420 => PixelFormat::Nv12,
            Self::EightBit444 => PixelFormat::Nv24,
            Self::TenBit420 => PixelFormat::P010,
            Self::TenBit444 => PixelFormat::P410,
        }
    }

    #[cfg(target_os = "macos")]
    const fn macos_bit_depth(self) -> opennow_streamer_platform_macos::VideoBitDepth {
        use opennow_streamer_platform_macos::VideoBitDepth;
        match self {
            Self::EightBit420 | Self::EightBit444 => VideoBitDepth::Eight,
            Self::TenBit420 | Self::TenBit444 => VideoBitDepth::Ten,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaStreamConfig {
    pub codec: MediaVideoCodec,
    /// Color class accepted by CloudMatch and requested again during NVST setup.
    pub color_quality: MediaColorQuality,
    pub hdr: bool,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_bps: u32,
    /// CloudMatch accepted Cloud G-SYNC and the host presentation path is VRR-capable.
    pub cloud_gsync: bool,
    pub shortcuts: StreamShortcutBindings,
}

impl Default for MediaStreamConfig {
    fn default() -> Self {
        Self {
            codec: MediaVideoCodec::H264,
            color_quality: MediaColorQuality::default(),
            hdr: false,
            width: 1920,
            height: 1080,
            fps: 60,
            bitrate_bps: 75_000_000,
            cloud_gsync: false,
            shortcuts: StreamShortcutBindings::default(),
        }
    }
}

#[cfg(target_os = "macos")]
impl MediaStreamConfig {
    fn macos_color_space(self) -> opennow_streamer_platform_macos::VideoColorSpace {
        use opennow_streamer_platform_macos::VideoColorSpace;
        if self.hdr {
            VideoColorSpace::Bt2020
        } else {
            VideoColorSpace::Bt709
        }
    }

    fn macos_transfer(self) -> opennow_streamer_platform_macos::VideoTransfer {
        use opennow_streamer_platform_macos::VideoTransfer;
        if self.hdr {
            VideoTransfer::Pq
        } else {
            VideoTransfer::Sdr
        }
    }

    fn macos_h265_format(
        self,
        parameter_sets: opennow_streamer_platform_macos::H265ParameterSets,
    ) -> opennow_streamer_platform_macos::H265Format {
        use opennow_streamer_platform_macos::{H265Format, VideoChroma};
        H265Format::new(parameter_sets, self.macos_color_space())
            .with_bit_depth(self.color_quality.macos_bit_depth())
            .with_chroma(if self.color_quality.is_444() {
                VideoChroma::Yuv444
            } else {
                VideoChroma::Yuv420
            })
            .with_transfer(self.macos_transfer())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaCodec {
    H264,
    H265,
    Av1,
    Opus { channels: u8 },
    Unsupported(String),
}

#[derive(Debug, Clone)]
pub struct EncodedFrame {
    pub mid: String,
    pub codec: MediaCodec,
    pub data: Arc<[u8]>,
    /// Sender-authored NVST video frame index used by the feedback protocol.
    pub frame_index: Option<u32>,
    pub timestamp: u64,
    pub clock_rate_hz: u32,
    pub keyframe: bool,
    pub contiguous: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapturedInput {
    Text(opennow_streamer_protocol::text_input::UnicodeText),
    Key {
        virtual_key: u16,
        modifiers: u16,
        pressed: bool,
    },
    MouseMove {
        delta_x: i16,
        delta_y: i16,
    },
    MouseAbsolute {
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    },
    MouseButton {
        button: u8,
        pressed: bool,
    },
    MouseWheel {
        delta_x: i16,
        delta_y: i16,
    },
    Gamepad {
        controller_id: u8,
        bitmap: u16,
        buttons: u16,
        left_trigger: u8,
        right_trigger: u8,
        left_stick_x: i16,
        left_stick_y: i16,
        right_stick_x: i16,
        right_stick_y: i16,
    },
    Guide,
    Screenshot,
    RecordingToggle,
    Shortcut(StreamShortcutAction),
}

const CAPTURED_INPUT_CAPACITY: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedInputSample {
    pub input: CapturedInput,
    pub captured_at: Instant,
}

#[derive(Debug, Default)]
pub struct CapturedInputQueue {
    pending: Mutex<VecDeque<CapturedInputSample>>,
    overflowed: AtomicBool,
    text_ready: AtomicBool,
    text_generation: AtomicU64,
    text_slot: opennow_streamer_protocol::text_input::TextInputSlot,
}

impl CapturedInputQueue {
    pub fn set_text_ready(&self, generation: u64, ready: bool) {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = self.text_generation.load(Ordering::Relaxed);
        if generation.wrapping_sub(previous) > u64::MAX / 2 {
            return;
        }
        self.text_generation.store(generation, Ordering::Relaxed);
        self.text_ready.store(ready, Ordering::Release);
        if !ready || generation != previous {
            self.text_slot.cancel();
            pending.retain(|sample| !matches!(sample.input, CapturedInput::Text(_)));
        }
    }

    pub(crate) fn submit_text(
        &self,
        bytes: &[u8],
    ) -> Result<(), opennow_streamer_protocol::text_input::TextInputError> {
        use opennow_streamer_protocol::text_input::TextInputError;
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !self.text_ready.load(Ordering::Acquire) {
            return Err(TextInputError::Unavailable);
        }
        if pending.len() >= CAPTURED_INPUT_CAPACITY {
            return Err(TextInputError::Busy);
        }
        let text = self.text_slot.submit(bytes)?;
        pending.push_back(CapturedInputSample {
            input: CapturedInput::Text(text),
            captured_at: Instant::now(),
        });
        Ok(())
    }

    pub(crate) fn discard_text(&self) {
        self.text_slot.cancel();
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|sample| !matches!(sample.input, CapturedInput::Text(_)));
    }

    pub fn push(&self, input: CapturedInput) {
        self.push_sample(CapturedInputSample {
            input,
            captured_at: Instant::now(),
        });
    }

    pub fn push_sample(&self, sample: CapturedInputSample) {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(&sample.input, CapturedInput::MouseAbsolute { .. })
            && matches!(
                pending.back(),
                Some(CapturedInputSample {
                    input: CapturedInput::MouseAbsolute { .. },
                    ..
                })
            )
        {
            pending.pop_back();
        }
        if pending.len() >= CAPTURED_INPUT_CAPACITY {
            if let Some(index) = pending.iter().position(|event| {
                matches!(
                    &event.input,
                    CapturedInput::MouseMove { .. } | CapturedInput::MouseAbsolute { .. }
                )
            }) {
                pending.remove(index);
            } else {
                self.overflowed.store(true, Ordering::Release);
                return;
            }
        }
        pending.push_back(sample);
    }

    pub(crate) fn release_gamepad(&self, controller_id: u8, bitmap: u16) {
        assert!(controller_id < 4);
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        pending.retain(|sample| {
            !matches!(sample.input,
            CapturedInput::Gamepad { controller_id: id, .. } if id == controller_id)
        });
        pending.push_back(CapturedInputSample {
            input: CapturedInput::Gamepad {
                controller_id,
                bitmap,
                buttons: 0,
                left_trigger: 0,
                right_trigger: 0,
                left_stick_x: 0,
                left_stick_y: 0,
                right_stick_x: 0,
                right_stick_y: 0,
            },
            captured_at: Instant::now(),
        });
        debug_assert!(pending.len() <= CAPTURED_INPUT_CAPACITY + 4);
    }

    pub fn take(&self) -> Option<CapturedInput> {
        self.take_sample().map(|sample| sample.input)
    }

    pub fn take_sample(&self) -> Option<CapturedInputSample> {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop_front()
    }

    pub fn take_overflowed(&self) -> bool {
        self.overflowed.swap(false, Ordering::AcqRel)
    }

    pub fn clear(&self) {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        self.overflowed.store(false, Ordering::Release);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaFeedback {
    VideoFrameAccepted {
        frame_index: Option<u32>,
        timestamp: u64,
        bytes: u32,
        keyframe: bool,
    },
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
    DeviceLost {
        subsystem: &'static str,
        recovered: bool,
        message: Option<String>,
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
    video: Arc<VideoQueue>,
    audio: Arc<BoundedQueue<EncodedFrame>>,
    output: Arc<OutputBuffers>,
    feedback: Sender<MediaFeedback>,
    paused: AtomicBool,
    video_desynced: AtomicBool,
    keyframe_requested: AtomicBool,
    stopped: AtomicBool,
    recording_tap: RecordingTap,
    replay_tap: crate::replay::ReplayTap,
    stream: MediaStreamConfig,
    #[cfg(target_os = "macos")]
    mac_sink: Mutex<Option<opennow_streamer_platform_macos::StreamSink>>,
    #[cfg(target_os = "macos")]
    mac_software_fallback: AtomicBool,
    #[cfg(target_os = "windows")]
    windows_bridge: Arc<WindowsBridge>,
    #[cfg(target_os = "linux")]
    linux_session: Mutex<Option<opennow_streamer_platform_linux::LinuxSession>>,
    #[cfg(target_os = "linux")]
    linux_software_fallback: Arc<AtomicBool>,
    #[cfg(target_os = "linux")]
    linux_video_mid: Mutex<String>,
    #[cfg(target_os = "linux")]
    linux_codec: MediaVideoCodec,
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
            MediaCodec::H264 | MediaCodec::H265 | MediaCodec::Av1 | MediaCodec::Opus { .. } => {
                self.shared.recording_tap.publish(&frame);
                self.shared.replay_tap.publish(&frame);
            }
            MediaCodec::Unsupported(_) => {}
        }
        match frame.codec {
            MediaCodec::H264 | MediaCodec::H265 | MediaCodec::Av1 => self.push_video(frame),
            MediaCodec::Opus { .. } => self.push_audio(frame),
            MediaCodec::Unsupported(_) => PushOutcome::Unsupported,
        }
    }

    fn push_video(&self, frame: EncodedFrame) -> PushOutcome {
        let mid = frame.mid.clone();
        let Ok(result) = self.shared.video.push(frame) else {
            return PushOutcome::Closed;
        };
        if result.request_keyframe {
            opennow_streamer_protocol::log::log_async(
                "WARN",
                "video-reference",
                "compressed chain invalidated; waiting for a fresh keyframe",
            );
            let _ = self.shared.feedback.send(MediaFeedback::RequestKeyframe {
                mid,
                reason: "compressed video discontinuity or queue overflow".to_owned(),
            });
        }
        if result.dropped > 0 {
            let _ = self.shared.feedback.send(MediaFeedback::QueueDropped {
                media: "video",
                count: result.dropped,
            });
            PushOutcome::DroppedOldest
        } else {
            PushOutcome::Queued
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
}

pub(crate) struct MediaSessionConfig<'a> {
    pub(crate) stream: MediaStreamConfig,
    pub(crate) audio_device: &'a opennow_streamer_protocol::AudioOutputDevice,
}

pub struct MediaSession {
    sink: MediaSink,
    video_worker: Option<JoinHandle<()>>,
    audio_worker: Option<JoinHandle<()>>,
    #[cfg(target_os = "linux")]
    linux_monitor: Option<JoinHandle<()>>,
    embedded_host_worker: Option<JoinHandle<()>>,
    #[cfg(target_os = "windows")]
    embedded_d3d11: Option<Arc<Mutex<EmbeddedD3d11State>>>,
    embedded_frames: Option<crate::GraphicsFramePublisher>,
    host_commands: Sender<HostCommand>,
}

#[derive(Clone)]
pub struct MediaControl {
    shared: Arc<SharedPipeline>,
    host_commands: Sender<HostCommand>,
}

impl MediaSession {
    pub fn captured_input(&self) -> Arc<CapturedInputQueue> {
        self.sink.shared.output.captured_input()
    }

    pub(crate) fn spawn(
        output: Arc<OutputBuffers>,
        feedback: Sender<MediaFeedback>,
        host_commands: Sender<HostCommand>,
        use_hardware: bool,
        stream: MediaStreamConfig,
        #[cfg(target_os = "windows")] windows_bridge: Arc<WindowsBridge>,
        #[cfg(target_os = "linux")] linux_selection: LinuxVideoSelection,
        #[cfg(target_os = "linux")] linux_software_fallback: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        #[cfg(not(target_os = "linux"))]
        let _ = &stream;
        #[cfg(target_os = "macos")]
        if use_hardware {
            return Self::spawn_macos(output, feedback, host_commands, stream);
        }
        #[cfg(target_os = "windows")]
        let use_windows_backend = windows_bridge.backend().is_some();
        #[cfg(target_os = "windows")]
        let _ = use_hardware;
        #[cfg(not(target_os = "windows"))]
        let use_windows_backend = false;
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let _ = use_hardware;
        #[cfg(target_os = "linux")]
        if !linux_software_fallback.load(Ordering::Acquire)
            && let LinuxVideoPath::Hardware(decoder_preference) = linux_selection.path
        {
            match Self::spawn_linux(
                Arc::clone(&output),
                feedback.clone(),
                host_commands.clone(),
                stream,
                decoder_preference,
                Arc::clone(&linux_software_fallback),
            ) {
                Ok(session) => return Ok(session),
                Err(reason) => {
                    if stream.codec != MediaVideoCodec::H264 {
                        return Err(format!(
                            "Linux {} decoder startup failed: {reason}",
                            stream.codec.label().to_ascii_uppercase()
                        ));
                    }
                    linux_software_fallback.store(true, Ordering::Release);
                    let _ = host_commands.send(HostCommand::FallbackLinux {
                        reason: format!("Linux hardware media startup failed: {reason}"),
                    });
                }
            }
        }
        if !use_windows_backend && stream.codec != MediaVideoCodec::H264 {
            return Err(format!(
                "{} requires the Windows hardware decoder",
                stream.codec.label().to_ascii_uppercase()
            ));
        }
        let video_decoder = (!use_windows_backend).then(H264Decoder::new).transpose()?;
        let audio_decoder = OpusDecoder::new(2)?;
        let shared = Arc::new(SharedPipeline {
            video: Arc::new(VideoQueue::new(VIDEO_QUEUE_CAPACITY)),
            audio: Arc::new(BoundedQueue::new(AUDIO_QUEUE_CAPACITY)),
            output,
            feedback,
            paused: AtomicBool::new(false),
            video_desynced: AtomicBool::new(true),
            keyframe_requested: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            recording_tap: RecordingTap::default(),
            replay_tap: crate::replay::ReplayTap::default(),
            stream,
            #[cfg(target_os = "macos")]
            mac_sink: Mutex::new(None),
            #[cfg(target_os = "macos")]
            mac_software_fallback: AtomicBool::new(false),
            #[cfg(target_os = "windows")]
            windows_bridge,
            #[cfg(target_os = "linux")]
            linux_session: Mutex::new(None),
            #[cfg(target_os = "linux")]
            linux_software_fallback,
            #[cfg(target_os = "linux")]
            linux_video_mid: Mutex::new(String::new()),
            #[cfg(target_os = "linux")]
            linux_codec: stream.codec,
        });
        let video_shared = Arc::clone(&shared);
        let video_worker = thread::Builder::new()
            .name(format!("opennow-{}-decode", stream.codec.label()))
            .spawn(move || {
                #[cfg(target_os = "windows")]
                if use_windows_backend {
                    run_windows_video(video_shared, stream.fps);
                    return;
                }
                run_video_decoder(
                    video_shared,
                    video_decoder.expect("software decoder was initialized"),
                );
            })
            .map_err(|error| format!("failed to start video decoder worker: {error}"))?;
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
            #[cfg(target_os = "linux")]
            linux_monitor: None,
            embedded_host_worker: None,
            #[cfg(target_os = "windows")]
            embedded_d3d11: None,
            embedded_frames: None,
            host_commands,
        })
    }

    #[cfg(target_os = "macos")]
    fn spawn_macos(
        output: Arc<OutputBuffers>,
        feedback: Sender<MediaFeedback>,
        host_commands: Sender<HostCommand>,
        stream: MediaStreamConfig,
    ) -> Result<Self, String> {
        let shared = Arc::new(SharedPipeline {
            // Keep a bounded scheduler-burst reserve. The VideoToolbox worker drains this queue
            // asynchronously; decoded frames remain latest-first at the Metal presentation edge.
            video: Arc::new(VideoQueue::new(macos_video_queue_capacity(stream.fps))),
            audio: Arc::new(BoundedQueue::new(AUDIO_QUEUE_CAPACITY)),
            output,
            feedback,
            paused: AtomicBool::new(false),
            video_desynced: AtomicBool::new(true),
            keyframe_requested: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            recording_tap: RecordingTap::default(),
            replay_tap: crate::replay::ReplayTap::default(),
            stream,
            mac_sink: Mutex::new(None),
            mac_software_fallback: AtomicBool::new(false),
        });
        let video_shared = Arc::clone(&shared);
        let video_commands = host_commands.clone();
        let video_worker = thread::Builder::new()
            .name(format!(
                "opennow-videotoolbox-{}-submit",
                stream.codec.label()
            ))
            .spawn(move || match stream.codec {
                MediaVideoCodec::H264 => {
                    run_macos_h264_video(video_shared, video_commands, stream.fps, true);
                }
                MediaVideoCodec::H265 => {
                    run_macos_h265_video(video_shared, video_commands, stream.fps);
                }
                MediaVideoCodec::Av1 => {
                    run_macos_av1_video(video_shared, video_commands, stream);
                }
            })
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
            #[cfg(target_os = "linux")]
            linux_monitor: None,
            embedded_host_worker: None,
            embedded_frames: None,
            host_commands,
        })
    }

    #[cfg(target_os = "linux")]
    fn spawn_linux(
        output: Arc<OutputBuffers>,
        feedback: Sender<MediaFeedback>,
        host_commands: Sender<HostCommand>,
        stream: MediaStreamConfig,
        decoder_preference: opennow_streamer_platform_linux::DecoderPreference,
        software_fallback: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let format = opennow_streamer_platform_linux::StreamFormat::video_default(
            stream.width,
            stream.height,
        )
        .map_err(|error| error.to_string())?;
        let mut config = opennow_streamer_platform_linux::SessionConfig::new(format);
        config.codec = match stream.codec {
            MediaVideoCodec::H264 => opennow_streamer_platform_linux::VideoCodec::H264,
            MediaVideoCodec::H265 => opennow_streamer_platform_linux::VideoCodec::H265,
            MediaVideoCodec::Av1 => opennow_streamer_platform_linux::VideoCodec::Av1,
        };
        config.decoder_preference = decoder_preference;
        config.audio = None;
        let session = opennow_streamer_platform_linux::LinuxSession::start(config)
            .map_err(|error| error.to_string())?;
        let shared = Arc::new(SharedPipeline {
            video: Arc::new(VideoQueue::new(VIDEO_QUEUE_CAPACITY)),
            audio: Arc::new(BoundedQueue::new(AUDIO_QUEUE_CAPACITY)),
            output,
            feedback,
            paused: AtomicBool::new(false),
            video_desynced: AtomicBool::new(true),
            keyframe_requested: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            recording_tap: RecordingTap::default(),
            replay_tap: crate::replay::ReplayTap::default(),
            stream,
            linux_session: Mutex::new(Some(session)),
            linux_software_fallback: software_fallback,
            linux_video_mid: Mutex::new(String::new()),
            linux_codec: stream.codec,
        });
        let video_shared = Arc::clone(&shared);
        let video_commands = host_commands.clone();
        let video_worker = thread::Builder::new()
            .name("opennow-linux-video-submit".to_owned())
            .spawn(move || run_linux_video(video_shared, video_commands))
            .map_err(|error| format!("failed to start Linux video submit worker: {error}"))?;
        let audio_decoder = OpusDecoder::new(2)?;
        let audio_shared = Arc::clone(&shared);
        let audio_worker = match thread::Builder::new()
            .name("opennow-opus-decode".to_owned())
            .spawn(move || run_audio_decoder(audio_shared, audio_decoder))
        {
            Ok(worker) => worker,
            Err(error) => {
                shared.video.close();
                let _ = video_worker.join();
                return Err(format!(
                    "failed to start Linux audio submit worker: {error}"
                ));
            }
        };
        let monitor_shared = Arc::clone(&shared);
        let monitor_commands = host_commands.clone();
        let linux_monitor = match thread::Builder::new()
            .name("opennow-linux-media-events".to_owned())
            .spawn(move || run_linux_monitor(monitor_shared, monitor_commands))
        {
            Ok(worker) => worker,
            Err(error) => {
                shared.video.close();
                shared.audio.close();
                let _ = video_worker.join();
                let _ = audio_worker.join();
                return Err(format!("failed to start Linux media monitor: {error}"));
            }
        };
        Ok(Self {
            sink: MediaSink { shared },
            video_worker: Some(video_worker),
            audio_worker: Some(audio_worker),
            linux_monitor: Some(linux_monitor),
            embedded_host_worker: None,
            embedded_frames: None,
            host_commands,
        })
    }

    pub(crate) fn spawn_embedded(
        output: Arc<OutputBuffers>,
        feedback: Sender<MediaFeedback>,
        host_commands: Sender<HostCommand>,
        config: MediaSessionConfig<'_>,
        frames: crate::GraphicsFramePublisher,
        #[cfg(target_os = "linux")] linux_selection: LinuxVideoSelection,
        #[cfg(target_os = "linux")] vulkan_device: Option<Arc<crate::SharedVulkanDevice>>,
    ) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            return Self::spawn_embedded_linux(
                output,
                feedback,
                host_commands,
                config,
                frames,
                linux_selection,
                vulkan_device,
            );
        }
        #[cfg(target_os = "macos")]
        {
            return Self::spawn_embedded_macos(
                output,
                feedback,
                host_commands,
                config.stream,
                config.audio_device,
                frames,
            );
        }
        #[cfg(target_os = "windows")]
        {
            return Self::spawn_embedded_windows(
                output,
                feedback,
                host_commands,
                config.stream,
                frames,
            );
        }
        #[allow(unreachable_code)]
        Err("embedded media is unsupported on this platform".to_owned())
    }

    #[cfg(target_os = "linux")]
    fn spawn_embedded_linux(
        output: Arc<OutputBuffers>,
        feedback: Sender<MediaFeedback>,
        host_commands: Sender<HostCommand>,
        config: MediaSessionConfig<'_>,
        frames: crate::GraphicsFramePublisher,
        linux_selection: LinuxVideoSelection,
        vulkan_device: Option<Arc<crate::SharedVulkanDevice>>,
    ) -> Result<Self, String> {
        let MediaSessionConfig {
            stream,
            audio_device,
        } = config;
        let LinuxVideoPath::Hardware(decoder_preference) = linux_selection.path else {
            return Err(linux_selection.fallback_reason.unwrap_or_else(|| {
                "embedded Linux output requires a native NV12 decoder".to_owned()
            }));
        };
        let format = opennow_streamer_platform_linux::StreamFormat::video_default(
            stream.width,
            stream.height,
        )
        .map_err(|error| error.to_string())?;
        let mut config = opennow_streamer_platform_linux::SessionConfig::new(format);
        config.codec = match stream.codec {
            MediaVideoCodec::H264 => opennow_streamer_platform_linux::VideoCodec::H264,
            MediaVideoCodec::H265 => opennow_streamer_platform_linux::VideoCodec::H265,
            MediaVideoCodec::Av1 => opennow_streamer_platform_linux::VideoCodec::Av1,
        };
        config.decoder_preference = decoder_preference;
        config.embedded_presentation = true;
        config.stream_format.pixel_format = stream.color_quality.linux_pixel_format();
        if stream.hdr {
            config.stream_format.color_transfer =
                opennow_streamer_platform_linux::ColorTransfer::Pq;
            config.stream_format.color_primaries =
                opennow_streamer_platform_linux::ColorPrimaries::Bt2020;
            config.stream_format.color_matrix =
                opennow_streamer_platform_linux::ColorMatrix::Bt2020;
        }
        if let Some(audio) = config.audio.as_mut() {
            audio.output_device = audio_device.device_name().unwrap_or_default().to_owned();
        }
        if stream.color_quality.is_444()
            && (decoder_preference
                != opennow_streamer_platform_linux::DecoderPreference::VulkanOnly
                || !vulkan_device.as_ref().is_some_and(|device| {
                    device.supports_format(
                        config.codec,
                        config.stream_format.pixel_format,
                        stream.width,
                        stream.height,
                    )
                }))
        {
            return Err("embedded Linux decode does not support negotiated 4:4:4 color".to_owned());
        }
        if decoder_preference == opennow_streamer_platform_linux::DecoderPreference::VulkanOnly {
            let device = vulkan_device.as_ref().ok_or_else(|| {
                "embedded Vulkan decode requires an attached shared device".to_owned()
            })?;
            if !device.supports_format(
                config.codec,
                config.stream_format.pixel_format,
                stream.width,
                stream.height,
            ) {
                return Err("the attached Vulkan device does not support the negotiated codec, color depth, or dimensions".to_owned());
            }
        } else if stream.color_quality.bit_depth() == 10
            && decoder_preference != opennow_streamer_platform_linux::DecoderPreference::VaApiOnly
        {
            return Err("negotiated 10-bit embedded Linux decode requires Vulkan Video or VAAPI P010 import".to_owned());
        }
        config.vulkan_device = vulkan_device;
        let session = opennow_streamer_platform_linux::LinuxSession::start(config)
            .map_err(|error| error.to_string())?;
        let shared = Arc::new(SharedPipeline {
            video: Arc::new(VideoQueue::new(VIDEO_QUEUE_CAPACITY)),
            audio: Arc::new(BoundedQueue::new(AUDIO_QUEUE_CAPACITY)),
            output,
            feedback,
            paused: AtomicBool::new(false),
            video_desynced: AtomicBool::new(true),
            keyframe_requested: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            recording_tap: RecordingTap::default(),
            replay_tap: crate::replay::ReplayTap::default(),
            stream,
            linux_session: Mutex::new(Some(session)),
            linux_software_fallback: Arc::new(AtomicBool::new(false)),
            linux_video_mid: Mutex::new(String::new()),
            linux_codec: stream.codec,
        });
        let video_shared = Arc::clone(&shared);
        let video_worker = thread::Builder::new()
            .name("opennow-embedded-linux-video-submit".to_owned())
            .spawn(move || run_embedded_linux_video(video_shared))
            .map_err(|error| format!("failed to start embedded Linux video submitter: {error}"))?;
        let audio_shared = Arc::clone(&shared);
        let audio_worker = match thread::Builder::new()
            .name("opennow-embedded-linux-audio-submit".to_owned())
            .spawn(move || run_embedded_linux_audio(audio_shared))
        {
            Ok(worker) => worker,
            Err(error) => {
                shared.video.close();
                let _ = video_worker.join();
                stop_linux_session(&shared);
                return Err(format!(
                    "failed to start embedded Linux audio submitter: {error}"
                ));
            }
        };
        let producer = crate::LinuxGpuFrameProducer::new(8).map_err(|error| error.to_string())?;
        let embedded_frames = frames.clone();
        let monitor_shared = Arc::clone(&shared);
        let linux_monitor = match thread::Builder::new()
            .name("opennow-embedded-linux-frame-publisher".to_owned())
            .spawn(move || run_embedded_linux_monitor(monitor_shared, frames, producer))
        {
            Ok(worker) => worker,
            Err(error) => {
                shared.video.close();
                shared.audio.close();
                let _ = video_worker.join();
                let _ = audio_worker.join();
                stop_linux_session(&shared);
                return Err(format!(
                    "failed to start embedded Linux frame publisher: {error}"
                ));
            }
        };
        Ok(Self {
            sink: MediaSink { shared },
            video_worker: Some(video_worker),
            audio_worker: Some(audio_worker),
            linux_monitor: Some(linux_monitor),
            embedded_host_worker: None,
            embedded_frames: Some(embedded_frames),
            host_commands,
        })
    }

    #[cfg(target_os = "macos")]
    fn spawn_embedded_macos(
        output: Arc<OutputBuffers>,
        feedback: Sender<MediaFeedback>,
        _host_commands: Sender<HostCommand>,
        stream: MediaStreamConfig,
        audio_device: &opennow_streamer_protocol::AudioOutputDevice,
        frames: crate::GraphicsFramePublisher,
    ) -> Result<Self, String> {
        if stream.color_quality.is_444()
            && (stream.codec != MediaVideoCodec::H265
                || stream.color_quality != MediaColorQuality::TenBit444
                || !opennow_streamer_platform_macos::probe_h265_444_ten_bit_hardware())
        {
            return Err(
                "embedded VideoToolbox decode does not support negotiated 4:4:4 color".to_owned(),
            );
        }
        let shared = Arc::new(SharedPipeline {
            video: Arc::new(VideoQueue::new(macos_video_queue_capacity(stream.fps))),
            audio: Arc::new(BoundedQueue::new(AUDIO_QUEUE_CAPACITY)),
            output,
            feedback,
            paused: AtomicBool::new(false),
            video_desynced: AtomicBool::new(true),
            keyframe_requested: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            recording_tap: RecordingTap::default(),
            replay_tap: crate::replay::ReplayTap::default(),
            stream,
            mac_sink: Mutex::new(None),
            mac_software_fallback: AtomicBool::new(false),
        });
        let audio_output_device = audio_device.device_name().map(str::to_owned);
        let (host_commands, host_receiver) = std::sync::mpsc::channel();
        let embedded_frames = frames.clone();
        let embedded_host_worker = thread::Builder::new()
            .name("opennow-embedded-videotoolbox-host".to_owned())
            .spawn(move || {
                use opennow_streamer_platform_macos::{
                    AudioFormat, EmbeddedBackendConfig, H264Format, MacOsBackend, QueueLimits,
                    VideoColorSpace,
                };

                let mut backend: Option<MacOsBackend> = None;
                let mut paused = false;
                while let Ok(command) = host_receiver.recv() {
                    let start = |video| {
                        let publisher = frames.clone();
                        MacOsBackend::start_embedded_with_publisher(
                            EmbeddedBackendConfig {
                                video,
                                audio: AudioFormat::OPUS_STEREO_48KHZ,
                                audio_output_device: audio_output_device.clone(),
                                queues: QueueLimits::default(),
                            },
                            move |frame| {
                                let Some(lease) = publisher.context() else {
                                    return false;
                                };
                                matches!(
                                    publisher.publish(lease, Arc::new(frame)),
                                    Ok(crate::GraphicsPublishOutcome::Replaced)
                                )
                            },
                        )
                    };
                    match command {
                        HostCommand::Start { reply, .. } => {
                            let _ = reply
                                .send(Err("embedded VideoToolbox host is already session-scoped"
                                    .to_owned()));
                        }
                        HostCommand::ConfigureMacH264 {
                            parameter_sets,
                            reply,
                        } => {
                            let result = start(
                                H264Format::new(parameter_sets, VideoColorSpace::Bt709).into(),
                            )
                            .and_then(|mut started| {
                                started.set_paused(paused)?;
                                let sink = started.sink();
                                backend = Some(started);
                                Ok(crate::runtime::MacH264Configuration::Hardware(sink))
                            })
                            .map_err(|error| error.to_string());
                            let _ = reply.send(result);
                        }
                        HostCommand::ConfigureMacH265 {
                            parameter_sets,
                            reply,
                        } => {
                            let result = start(stream.macos_h265_format(parameter_sets).into())
                                .and_then(|mut started| {
                                    started.set_paused(paused)?;
                                    let sink = started.sink();
                                    backend = Some(started);
                                    Ok(sink)
                                })
                                .map_err(|error| error.to_string());
                            let _ = reply.send(result);
                        }
                        HostCommand::ConfigureMacAv1 { format, reply } => {
                            let result = start(format.into())
                                .and_then(|mut started| {
                                    started.set_paused(paused)?;
                                    let sink = started.sink();
                                    backend = Some(started);
                                    Ok(sink)
                                })
                                .map_err(|error| error.to_string());
                            let _ = reply.send(result);
                        }
                        HostCommand::Pause {
                            paused: new_paused,
                            reply,
                        } => {
                            let result = backend.as_mut().map_or(Ok(()), |backend| {
                                backend
                                    .set_paused(new_paused)
                                    .map_err(|error| error.to_string())
                            });
                            if result.is_ok() {
                                paused = new_paused;
                            }
                            if let Some(reply) = reply {
                                let _ = reply.send(result);
                            }
                        }
                        HostCommand::Stop | HostCommand::Shutdown => break,
                        HostCommand::StartMicrophone { reply, .. } => {
                            let _ = reply.send(Err(
                                "microphone capture is owned by the embedded runtime host"
                                    .to_owned(),
                            ));
                        }
                        HostCommand::Surface { reply, .. } | HostCommand::Control { reply, .. } => {
                            let _ = reply.send(Ok(()));
                        }
                        HostCommand::AudioDevices(request) => {
                            let _ = request.reply.send(Err(
                                "Audio enumeration is owned by the embedded runtime host"
                                    .to_owned(),
                            ));
                        }
                        HostCommand::Cursor(_) => {}
                    }
                }
            })
            .map_err(|error| format!("failed to start embedded VideoToolbox host: {error}"))?;
        let video_shared = Arc::clone(&shared);
        let video_commands = host_commands.clone();
        let video_worker = match thread::Builder::new()
            .name(format!(
                "opennow-embedded-videotoolbox-{}-submit",
                stream.codec.label()
            ))
            .spawn(move || match stream.codec {
                MediaVideoCodec::H264 => {
                    run_macos_h264_video(video_shared, video_commands, stream.fps, false);
                }
                MediaVideoCodec::H265 => {
                    run_macos_h265_video(video_shared, video_commands, stream.fps);
                }
                MediaVideoCodec::Av1 => run_macos_av1_video(video_shared, video_commands, stream),
            }) {
            Ok(worker) => worker,
            Err(error) => {
                let _ = host_commands.send(HostCommand::Shutdown);
                let _ = embedded_host_worker.join();
                return Err(format!(
                    "failed to start embedded VideoToolbox submitter: {error}"
                ));
            }
        };
        let audio_shared = Arc::clone(&shared);
        let audio_worker = match thread::Builder::new()
            .name("opennow-embedded-coreaudio-submit".to_owned())
            .spawn(move || run_macos_audio(audio_shared))
        {
            Ok(worker) => worker,
            Err(error) => {
                shared.video.close();
                let _ = video_worker.join();
                let _ = host_commands.send(HostCommand::Shutdown);
                let _ = embedded_host_worker.join();
                return Err(format!(
                    "failed to start embedded CoreAudio submitter: {error}"
                ));
            }
        };
        Ok(Self {
            sink: MediaSink { shared },
            video_worker: Some(video_worker),
            audio_worker: Some(audio_worker),
            embedded_host_worker: Some(embedded_host_worker),
            embedded_frames: Some(embedded_frames),
            host_commands,
        })
    }

    #[cfg(target_os = "windows")]
    fn spawn_embedded_windows(
        output: Arc<OutputBuffers>,
        feedback: Sender<MediaFeedback>,
        host_commands: Sender<HostCommand>,
        stream: MediaStreamConfig,
        frames: crate::GraphicsFramePublisher,
    ) -> Result<Self, String> {
        let bridge = Arc::new(WindowsBridge::new());
        let shared = Arc::new(SharedPipeline {
            video: Arc::new(VideoQueue::new(VIDEO_QUEUE_CAPACITY)),
            audio: Arc::new(BoundedQueue::new(AUDIO_QUEUE_CAPACITY)),
            output,
            feedback,
            paused: AtomicBool::new(false),
            video_desynced: AtomicBool::new(true),
            keyframe_requested: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            recording_tap: RecordingTap::default(),
            replay_tap: crate::replay::ReplayTap::default(),
            stream,
            windows_bridge: bridge,
        });
        // Keep compressed-frame submission independent from the render-owned decoder state.
        // The old path made the 120 Hz submit worker take the same mutex that Qt held while
        // Media Foundation polled output and D3D11 converted a frame. A single slow AV1/P010
        // decode step could therefore fill the two-frame ingress queue, discard the prediction
        // chain, and repeatedly reset an otherwise healthy decoder while audio continued.
        let submission = Arc::new(Mutex::new(EmbeddedD3d11Submission::new()));
        let producer = Arc::new(Mutex::new(EmbeddedD3d11State::new(
            stream,
            Arc::clone(&submission),
        )));
        let notifier = Arc::new(EmbeddedD3d11FrameNotifier {
            state: Arc::downgrade(&producer),
            frames: frames.clone(),
            shared: Arc::clone(&shared),
            stream,
            mid: Mutex::new(String::new()),
            presentation_time_ns: AtomicU64::new(0),
            sequence: AtomicU64::new(0),
        });
        let weak_notifier = Arc::downgrade(&notifier);
        producer
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .set_frame_ready(Arc::new(move || {
                if let Some(notifier) = weak_notifier.upgrade() {
                    notifier.publish_decoded();
                }
            }));
        let embedded_frames = frames.clone();
        let video_shared = Arc::clone(&shared);
        let video_submission = Arc::clone(&submission);
        let video_notifier = Arc::clone(&notifier);
        let video_worker = thread::Builder::new()
            .name("opennow-embedded-d3d11-submit".to_owned())
            .spawn(move || {
                opennow_streamer_protocol::log::log_line("INFO", "d3d11-submit", &format!(
                    "worker started width={} height={} fps={}; awaiting compressed video", stream.width, stream.height, stream.fps));
                let mut received = 0_u64;
                let mut skipped_paused = 0_u64;
                let mut skipped_reference = 0_u64;
                let mut last_report = Instant::now();
                let mut clock = AdaptiveSampleClock::new(stream.fps);
                while let Some(packet) = video_shared.video.pop_packet() {
                    let frame = packet.frame;
                    received += 1;
                    if received == 1 || last_report.elapsed() >= Duration::from_secs(10) {
                        opennow_streamer_protocol::log::log_async("INFO", "d3d11-submit", &format!(
                            "received={received} skipped_paused={skipped_paused} skipped_reference={skipped_reference} keyframe={} bytes={}", frame.keyframe, frame.data.len()));
                        last_report = Instant::now();
                    }
                    if video_shared.paused.load(Ordering::Acquire) {
                        skipped_paused += 1;
                        continue;
                    }
                    let timestamp_100ns =
                        media_timestamp_100ns(frame.timestamp, frame.clock_rate_hz);
                    let duration_100ns = clock.observe(timestamp_100ns);
                    let encoded = opennow_streamer_platform_windows::EncodedVideoFrame {
                        codec: match frame.codec {
                            MediaCodec::H264 => opennow_streamer_platform_windows::VideoCodec::H264,
                            MediaCodec::H265 => opennow_streamer_platform_windows::VideoCodec::H265,
                            MediaCodec::Av1 => opennow_streamer_platform_windows::VideoCodec::Av1,
                            _ => continue,
                        },
                        data: frame.data.to_vec(),
                        timestamp_100ns,
                        duration_100ns,
                        key_frame: frame.keyframe,
                        reset_decoder: packet.reset_decoder,
                    };
                    // Validate after allocation, under the same lock that publishes loss.
                    // Only compressed-queue admission runs here, never GPU/codec work.
                    let Some(submission_result) = video_shared.video.submit_if_current(
                        packet.generation,
                        || video_submission
                            .lock()
                            .unwrap_or_else(|error| error.into_inner())
                            .push(encoded),
                    ) else {
                        skipped_reference += 1;
                        continue;
                    };
                    let submission = match submission_result {
                        Ok(submission) => submission,
                        Err(message) => {
                            let _ = video_shared.feedback.send(MediaFeedback::DecoderError {
                                codec: match frame.codec {
                                    MediaCodec::H264 => "h264",
                                    MediaCodec::H265 => "h265",
                                    MediaCodec::Av1 => "av1",
                                    _ => "video",
                                },
                                message,
                            });
                            invalidate_embedded_video(
                                &video_shared,
                                &frame.mid,
                                "embedded D3D11 submission failed",
                            );
                            continue;
                        }
                    };
                    if submission.dropped > 0 {
                        eprintln!(
                            "Embedded D3D11 compressed queue overflow: dropped={} recoveryKeyframeRetained={}",
                            submission.dropped,
                            submission.queued && frame.keyframe,
                        );
                        let _ = video_shared.feedback.send(MediaFeedback::QueueDropped {
                            media: "d3d11-video",
                            count: submission.dropped,
                        });
                    }
                    if !submission.queued {
                        invalidate_embedded_video(
                            &video_shared,
                            &frame.mid,
                            "embedded D3D11 compressed-video queue overflow",
                        );
                        continue;
                    }
                    report_video_frame_accepted(&video_shared, &frame);
                    if submission.needs_graphics {
                        video_notifier.publish_initial(
                            &frame.mid,
                            u64::try_from(timestamp_100ns.max(0))
                                .unwrap_or(0)
                                .saturating_mul(100),
                        );
                    }
                }
            })
            .map_err(|error| format!("failed to start embedded D3D11 submitter: {error}"))?;
        let audio_shared = Arc::clone(&shared);
        let audio_decoder = OpusDecoder::new(2)?;
        let audio_worker = match thread::Builder::new()
            .name("opennow-embedded-opus-decode".to_owned())
            .spawn(move || run_audio_decoder(audio_shared, audio_decoder))
        {
            Ok(worker) => worker,
            Err(error) => {
                shared.video.close();
                let _ = video_worker.join();
                return Err(format!("failed to start embedded Opus decoder: {error}"));
            }
        };
        Ok(Self {
            sink: MediaSink { shared },
            video_worker: Some(video_worker),
            audio_worker: Some(audio_worker),
            embedded_host_worker: None,
            embedded_d3d11: Some(producer),
            embedded_frames: Some(embedded_frames),
            host_commands,
        })
    }

    #[cfg(feature = "test-runtime")]
    pub(crate) fn spawn_test(
        output: Arc<OutputBuffers>,
        feedback: Sender<MediaFeedback>,
        host_commands: Sender<HostCommand>,
        stream: MediaStreamConfig,
        #[cfg(target_os = "windows")] windows_bridge: Arc<WindowsBridge>,
        #[cfg(target_os = "linux")] linux_software_fallback: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let shared = Arc::new(SharedPipeline {
            video: Arc::new(VideoQueue::new(VIDEO_QUEUE_CAPACITY)),
            audio: Arc::new(BoundedQueue::new(AUDIO_QUEUE_CAPACITY)),
            output,
            feedback,
            paused: AtomicBool::new(false),
            video_desynced: AtomicBool::new(false),
            keyframe_requested: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            recording_tap: RecordingTap::default(),
            replay_tap: crate::replay::ReplayTap::default(),
            stream,
            #[cfg(target_os = "macos")]
            mac_sink: Mutex::new(None),
            #[cfg(target_os = "macos")]
            mac_software_fallback: AtomicBool::new(false),
            #[cfg(target_os = "windows")]
            windows_bridge,
            #[cfg(target_os = "linux")]
            linux_session: Mutex::new(None),
            #[cfg(target_os = "linux")]
            linux_software_fallback,
            #[cfg(target_os = "linux")]
            linux_video_mid: Mutex::new(String::new()),
            #[cfg(target_os = "linux")]
            linux_codec: stream.codec,
        });
        let video_shared = Arc::clone(&shared);
        let video_worker = thread::Builder::new()
            .name("opennow-test-video-consumer".to_owned())
            .spawn(move || {
                while let Some(frame) = video_shared.video.pop() {
                    report_video_frame_accepted(&video_shared, &frame);
                }
            })
            .map_err(|error| format!("failed to start test video consumer: {error}"))?;
        let audio_shared = Arc::clone(&shared);
        let audio_worker = match thread::Builder::new()
            .name("opennow-test-audio-consumer".to_owned())
            .spawn(move || while audio_shared.audio.pop().is_some() {})
        {
            Ok(worker) => worker,
            Err(error) => {
                shared.video.close();
                let _ = video_worker.join();
                return Err(format!("failed to start test audio consumer: {error}"));
            }
        };
        Ok(Self {
            sink: MediaSink { shared },
            video_worker: Some(video_worker),
            audio_worker: Some(audio_worker),
            #[cfg(target_os = "linux")]
            linux_monitor: None,
            embedded_host_worker: None,
            #[cfg(target_os = "windows")]
            embedded_d3d11: None,
            embedded_frames: None,
            host_commands,
        })
    }

    pub fn sink(&self) -> MediaSink {
        self.sink.clone()
    }

    pub fn control(&self) -> MediaControl {
        MediaControl {
            shared: Arc::clone(&self.sink.shared),
            host_commands: self.host_commands.clone(),
        }
    }

    pub fn set_paused(&self, paused: bool) {
        self.sink.shared.paused.store(paused, Ordering::Release);
        self.sink.shared.video.clear();
        self.sink.shared.audio.clear();
        self.sink.shared.output.clear();
        if let Some(frames) = &self.embedded_frames {
            frames.clear();
        }
        #[cfg(target_os = "windows")]
        if let Some(state) = &self.embedded_d3d11 {
            state
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .reset();
        }
        #[cfg(target_os = "linux")]
        if let Some(session) = self
            .sink
            .shared
            .linux_session
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
        {
            let _ = session.set_paused(paused);
        }
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
        self.control().stop();
        if let Some(worker) = self.video_worker.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.audio_worker.take() {
            let _ = worker.join();
        }
        #[cfg(target_os = "linux")]
        if let Some(worker) = self.linux_monitor.take() {
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
        if let Some(worker) = self.embedded_host_worker.take() {
            let _ = worker.join();
        }
        // Clear after all publishers stop, so a late decoder callback cannot
        // republish the previous session's surface into the next session.
        if let Some(frames) = &self.embedded_frames {
            frames.clear();
        }
    }
}

#[cfg(target_os = "windows")]
struct EmbeddedD3d11State {
    format: opennow_streamer_platform_windows::VideoFormat,
    submission: Arc<Mutex<EmbeddedD3d11Submission>>,
    producer: Option<(crate::GraphicsContext, crate::D3d11FrameProducer)>,
    frame_ready: Arc<dyn Fn() + Send + Sync>,
    keyframe_required: bool,
    first_frame_recorded: bool,
}

#[cfg(target_os = "windows")]
struct EmbeddedD3d11Submission {
    pending: VecDeque<opennow_streamer_platform_windows::EncodedVideoFrame>,
    submitter: Option<crate::D3d11FrameSubmitter>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EmbeddedD3d11SubmissionOutcome {
    queued: bool,
    dropped: usize,
    needs_graphics: bool,
}

#[cfg(target_os = "windows")]
impl EmbeddedD3d11State {
    fn new(stream: MediaStreamConfig, submission: Arc<Mutex<EmbeddedD3d11Submission>>) -> Self {
        use opennow_streamer_platform_windows::{
            VideoChromaFormat, VideoCodec, VideoFormat, VideoPixelFormat,
        };
        use std::num::NonZeroU32;

        Self {
            format: VideoFormat {
                codec: match stream.codec {
                    MediaVideoCodec::H264 => VideoCodec::H264,
                    MediaVideoCodec::H265 => VideoCodec::H265,
                    MediaVideoCodec::Av1 => VideoCodec::Av1,
                },
                width: stream.width,
                height: stream.height,
                frame_rate_numerator: NonZeroU32::new(stream.fps.max(1)).expect("non-zero fps"),
                frame_rate_denominator: NonZeroU32::new(1).expect("one is non-zero"),
                average_bitrate: stream.bitrate_bps.max(1),
                pixel_format: match (
                    stream.color_quality.bit_depth(),
                    stream.color_quality.is_444(),
                ) {
                    (10, true) => VideoPixelFormat::Y410,
                    (10, false) => VideoPixelFormat::P010,
                    (_, true) => VideoPixelFormat::Ayuv,
                    _ => VideoPixelFormat::Nv12,
                },
                chroma_format: if stream.color_quality.is_444() {
                    VideoChromaFormat::Cs444
                } else {
                    VideoChromaFormat::Cs420
                },
                full_range: false,
                chroma_siting: opennow_streamer_platform_windows::VideoChromaSiting::Left,
                transfer_function: if stream.hdr {
                    opennow_streamer_platform_windows::VideoTransferFunction::Pq
                } else {
                    opennow_streamer_platform_windows::VideoTransferFunction::Sdr
                },
                color_primaries: if stream.hdr {
                    opennow_streamer_platform_windows::VideoColorPrimaries::Bt2020
                } else {
                    opennow_streamer_platform_windows::VideoColorPrimaries::Bt709
                },
                color_matrix: if stream.hdr {
                    opennow_streamer_platform_windows::VideoColorMatrix::Bt2020
                } else {
                    opennow_streamer_platform_windows::VideoColorMatrix::Bt709
                },
            },
            submission,
            producer: None,
            frame_ready: Arc::new(|| {}),
            keyframe_required: false,
            first_frame_recorded: false,
        }
    }

    fn set_frame_ready(&mut self, frame_ready: Arc<dyn Fn() + Send + Sync>) {
        self.frame_ready = frame_ready;
    }

    fn reset(&mut self) {
        detach_d3d11_producer(&self.submission, &mut self.producer);
        self.keyframe_required = false;
    }

    fn take_keyframe_required(&mut self) -> bool {
        std::mem::take(&mut self.keyframe_required)
    }

    fn take_first_recorded_frame(&mut self, succeeded: bool) -> bool {
        succeeded && !std::mem::replace(&mut self.first_frame_recorded, true)
    }

    fn record(
        &mut self,
        context: crate::GraphicsContext,
        command: crate::GraphicsRecordCommand,
    ) -> Result<crate::GraphicsRecordedFrame, crate::GraphicsFrameError> {
        use opennow_streamer_platform_windows::{
            AdoptedD3d11Context, D3d11FrameProducer, WindowsDecoderMode,
        };

        if context.api != crate::GraphicsApi::D3d11 {
            return Err(
                "an embedded Windows frame requires a D3D11 graphics context"
                    .to_owned()
                    .into(),
            );
        }
        if self
            .producer
            .as_ref()
            .is_none_or(|(adopted, _)| *adopted != context)
        {
            let replacing_context = self.producer.is_some();
            if replacing_context {
                // Frames submitted through the old Qt device cannot be replayed on the new one.
                // Switch the submit path back to its bounded pre-context queue before adopting
                // the replacement device, then request one clean prediction chain.
                detach_d3d11_producer(&self.submission, &mut self.producer);
                self.keyframe_required = true;
            }
            let (producer, submitter) = unsafe {
                D3d11FrameProducer::new(
                    AdoptedD3d11Context {
                        device: context.device as *mut std::ffi::c_void,
                        immediate_context: context.queue as *mut std::ffi::c_void,
                    },
                    self.format,
                    WindowsDecoderMode::Hardware,
                    Arc::clone(&self.frame_ready),
                )
            }
            .map_err(|error| error.to_string())?;
            let dropped = self
                .submission
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .install(submitter)?;
            if dropped > 0 {
                self.keyframe_required = true;
            }
            self.producer = Some((context, producer));
        }
        let (_, producer) = self.producer.as_ref().expect("producer initialized");
        while let Some(event) = producer.try_event() {
            match event {
                opennow_streamer_platform_windows::BackendEvent::KeyFrameRequired
                | opennow_streamer_platform_windows::BackendEvent::DeviceLost {
                    subsystem: opennow_streamer_platform_windows::Subsystem::VideoDecode,
                    ..
                } => self.keyframe_required = true,
                _ => {}
            }
        }
        let frame = producer
            .acquire_latest()
            .map_err(|error| error.to_string())?
            .ok_or(crate::GraphicsFrameError::NotReady)?;
        let recorded = unsafe {
            frame.record(
                AdoptedD3d11Context {
                    device: context.device as *mut std::ffi::c_void,
                    immediate_context: context.queue as *mut std::ffi::c_void,
                },
                command.frame_slot,
            )
        }
        .map_err(|error| error.to_string())?;
        Ok(crate::GraphicsRecordedFrame {
            resource: recorded.texture as usize as u64,
            resource_view: 0,
            color_space: recorded.color_space.into(),
            texture_format: match recorded.texture_format {
                opennow_streamer_platform_windows::D3d11TextureFormat::Rgba8 => {
                    crate::GraphicsTextureFormat::Rgba8
                }
                opennow_streamer_platform_windows::D3d11TextureFormat::Rgb10A2 => {
                    crate::GraphicsTextureFormat::Rgb10A2
                }
            },
            width: recorded.width,
            height: recorded.height,
            frame_slot: recorded.frame_slot,
            generation: recorded.generation,
            presentation_time_ns: recorded.presentation_time_ns,
        })
    }
}

#[cfg(target_os = "windows")]
fn detach_d3d11_producer<T>(submission: &Mutex<EmbeddedD3d11Submission>, producer: &mut Option<T>) {
    // Drop closes the decoder queue; its worker retains its resources until exit.
    // First remove the submitter under the submission lock. Concurrent frames use the staging queue,
    // never a closed decoder queue (reported as "backend worker disconnected").
    submission
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .reset();
    drop(producer.take());
}

#[cfg(target_os = "windows")]
impl EmbeddedD3d11Submission {
    fn new() -> Self {
        Self {
            pending: VecDeque::with_capacity(
                opennow_streamer_platform_windows::ADAPTIVE_VIDEO_QUEUE_CAPACITY,
            ),
            submitter: None,
        }
    }

    /// Queues one compressed access unit without retaining a broken inter-frame chain.
    /// Once graphics is installed this writes directly to the decoder's thread-safe queue; it
    /// never waits for the render-owned decoder/processor mutex.
    fn push(
        &mut self,
        frame: opennow_streamer_platform_windows::EncodedVideoFrame,
    ) -> Result<EmbeddedD3d11SubmissionOutcome, String> {
        if let Some(submitter) = self.submitter.as_ref() {
            let key_frame = frame.key_frame;
            return submitter
                .submit_video(frame)
                .map(|outcome| match outcome {
                    opennow_streamer_platform_windows::PushOutcome::Queued
                    | opennow_streamer_platform_windows::PushOutcome::Paused => {
                        EmbeddedD3d11SubmissionOutcome {
                            queued: true,
                            dropped: 0,
                            needs_graphics: false,
                        }
                    }
                    opennow_streamer_platform_windows::PushOutcome::DroppedOldest => {
                        EmbeddedD3d11SubmissionOutcome {
                            queued: key_frame,
                            dropped:
                                opennow_streamer_platform_windows::ADAPTIVE_VIDEO_QUEUE_CAPACITY
                                    + usize::from(!key_frame),
                            needs_graphics: false,
                        }
                    }
                })
                .map_err(|error| error.to_string());
        }
        if self.pending.len() == opennow_streamer_platform_windows::ADAPTIVE_VIDEO_QUEUE_CAPACITY {
            let key_frame = frame.key_frame;
            let dropped = self.pending.len().saturating_add(usize::from(!key_frame));
            self.pending.clear();
            if key_frame {
                self.pending.push_back(frame);
            }
            return Ok(EmbeddedD3d11SubmissionOutcome {
                queued: key_frame,
                dropped,
                needs_graphics: true,
            });
        }
        self.pending.push_back(frame);
        Ok(EmbeddedD3d11SubmissionOutcome {
            queued: true,
            dropped: 0,
            needs_graphics: true,
        })
    }

    fn reset(&mut self) {
        self.pending.clear();
        self.submitter = None;
    }

    fn install(&mut self, submitter: crate::D3d11FrameSubmitter) -> Result<usize, String> {
        let mut dropped = 0;
        while let Some(frame) = self.pending.pop_front() {
            let outcome = submitter
                .submit_video(frame)
                .map_err(|error| error.to_string())?;
            if outcome == opennow_streamer_platform_windows::PushOutcome::DroppedOldest {
                dropped = dropped
                    .max(opennow_streamer_platform_windows::ADAPTIVE_VIDEO_QUEUE_CAPACITY + 1);
                self.pending.clear();
                break;
            }
        }
        self.submitter = Some(submitter);
        Ok(dropped)
    }
}

#[cfg(target_os = "windows")]
struct EmbeddedD3d11FrameNotifier {
    state: std::sync::Weak<Mutex<EmbeddedD3d11State>>,
    frames: crate::GraphicsFramePublisher,
    shared: Arc<SharedPipeline>,
    stream: MediaStreamConfig,
    mid: Mutex<String>,
    presentation_time_ns: AtomicU64,
    sequence: AtomicU64,
}

#[cfg(target_os = "windows")]
impl EmbeddedD3d11FrameNotifier {
    fn publish_initial(&self, mid: &str, presentation_time_ns: u64) {
        *self
            .mid
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = mid.to_owned();
        self.presentation_time_ns
            .store(presentation_time_ns, Ordering::Release);
        self.publish();
    }

    fn publish_decoded(&self) {
        self.publish();
    }

    fn publish(&self) {
        let Some(state) = self.state.upgrade() else {
            return;
        };
        let Some(lease) = self.frames.context() else {
            opennow_streamer_protocol::log::log_throttled(
                "d3d11-publish-no-context",
                "WARN",
                "d3d11-publish",
                "frame ready but Qt graphics context is absent",
            );
            return;
        };
        let sequence = self.sequence.fetch_add(1, Ordering::AcqRel) + 1;
        let pending = PendingD3d11Frame {
            state,
            shared: Arc::clone(&self.shared),
            mid: self
                .mid
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
            info: crate::GraphicsFrameInfo {
                width: self.stream.width,
                height: self.stream.height,
                sequence,
                presentation_time_ns: self.presentation_time_ns.load(Ordering::Acquire),
            },
        };
        let result = self.frames.publish(lease, Arc::new(pending));
        if sequence == 1 || result.is_err() {
            opennow_streamer_protocol::log::log_throttled(
                "d3d11-publish-result",
                "INFO",
                "d3d11-publish",
                &format!("sequence={sequence} result={result:?}"),
            );
        }
    }
}

#[cfg(target_os = "windows")]
struct PendingD3d11Frame {
    state: Arc<Mutex<EmbeddedD3d11State>>,
    shared: Arc<SharedPipeline>,
    mid: String,
    info: crate::GraphicsFrameInfo,
}

#[cfg(target_os = "windows")]
impl crate::GraphicsRenderResources for Mutex<EmbeddedD3d11State> {
    fn retire(&self) -> Result<(), String> {
        let mut state = self.lock().unwrap_or_else(|error| error.into_inner());
        state.reset();
        state.keyframe_required = true;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
impl crate::GraphicsFrame for PendingD3d11Frame {
    fn info(&self) -> crate::GraphicsFrameInfo {
        self.info
    }

    fn render_resources(&self) -> Option<Arc<dyn crate::GraphicsRenderResources>> {
        Some(self.state.clone())
    }

    fn record(
        &self,
        context: crate::GraphicsContext,
        command: crate::GraphicsRecordCommand,
    ) -> Result<crate::GraphicsRecordedFrame, crate::GraphicsFrameError> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let result = state.record(context, command);
        let first_frame = state.take_first_recorded_frame(result.is_ok());
        let keyframe_required = state.take_keyframe_required();
        drop(state);
        if first_frame {
            let _ = self.shared.feedback.send(MediaFeedback::PlaybackStarted {
                backend: "D3D11/Qt",
            });
        }
        if keyframe_required {
            invalidate_embedded_video(
                &self.shared,
                &self.mid,
                "embedded D3D11 decoder lost references or graphics device",
            );
            let _ = self.shared.feedback.send(MediaFeedback::QueueDropped {
                media: "d3d11-decode",
                count: 1,
            });
        }
        result
    }
}

impl MediaControl {
    pub fn start_replay(
        &self,
        config: opennow_streamer_protocol::ReplayBufferConfig,
        reserved: Arc<std::sync::atomic::AtomicUsize>,
    ) {
        self.shared.replay_tap.start_with_budget(config, reserved);
    }

    pub fn stop_replay(&self) {
        self.shared.replay_tap.stop();
    }

    pub fn replay_snapshot(
        &self,
    ) -> Result<(MediaStreamConfig, crate::replay::ReplaySnapshot), &'static str> {
        self.shared
            .replay_tap
            .snapshot()
            .map(|snapshot| (self.shared.stream, snapshot))
    }

    pub fn subscribe_recording(
        &self,
    ) -> Result<(MediaStreamConfig, EncodedRecordingReceiver), String> {
        self.shared
            .recording_tap
            .subscribe()
            .map(|receiver| (self.shared.stream, receiver))
    }

    pub fn unsubscribe_recording(&self) {
        self.shared.recording_tap.unsubscribe();
    }

    pub fn update_cursor(&self, bytes: Vec<u8>) {
        let _ = self.host_commands.send(HostCommand::Cursor(bytes));
    }

    pub fn stop(&self) {
        if self.shared.stopped.swap(true, Ordering::AcqRel) {
            return;
        }
        self.shared.video.close();
        self.shared.audio.close();
        self.shared.recording_tap.unsubscribe();
        self.shared.replay_tap.stop();
        self.shared.output.stop_microphone();
        self.shared.output.clear();
        let _ = self.host_commands.send(HostCommand::Stop);
    }
}

#[cfg(target_os = "windows")]
fn run_windows_video(shared: Arc<SharedPipeline>, maximum_fps: u32) {
    use opennow_streamer_platform_windows::{
        EncodedVideoFrame, PushOutcome as WindowsPushOutcome, VideoCodec,
    };

    let mut sample_clock = AdaptiveSampleClock::new(maximum_fps);
    while let Some(frame) = shared.video.pop() {
        if shared.paused.load(Ordering::Acquire) {
            continue;
        }
        if shared.windows_bridge.use_software() {
            shared.video_desynced.store(true, Ordering::Release);
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
        if (shared.video_desynced.load(Ordering::Acquire)
            || shared.windows_bridge.keyframe_required())
            && !frame.keyframe
        {
            continue;
        }
        let Some(backend) = shared.windows_bridge.backend() else {
            shared.video_desynced.store(true, Ordering::Release);
            request_keyframe(&shared, &frame.mid, "D3D11 backend is unavailable");
            continue;
        };
        shared.windows_bridge.set_last_video_mid(&frame.mid);
        let timestamp_100ns = media_timestamp_100ns(frame.timestamp, frame.clock_rate_hz);
        let duration_100ns = sample_clock.observe(timestamp_100ns);
        let reset_decoder = frame.keyframe
            && (shared.video_desynced.load(Ordering::Acquire)
                || shared.windows_bridge.keyframe_required());
        match backend.submit_video(EncodedVideoFrame {
            codec: match frame.codec {
                MediaCodec::H264 => VideoCodec::H264,
                MediaCodec::H265 => VideoCodec::H265,
                MediaCodec::Av1 => VideoCodec::Av1,
                _ => continue,
            },
            data: frame.data.to_vec(),
            timestamp_100ns,
            duration_100ns,
            key_frame: frame.keyframe,
            reset_decoder,
        }) {
            Ok(WindowsPushOutcome::Queued) => {
                report_video_frame_accepted(&shared, &frame);
                if frame.keyframe {
                    shared.video_desynced.store(false, Ordering::Release);
                    shared.keyframe_requested.store(false, Ordering::Release);
                    shared.windows_bridge.accept_keyframe();
                }
            }
            Ok(WindowsPushOutcome::Paused) => {}
            Ok(WindowsPushOutcome::DroppedOldest) => {
                let _ = shared.feedback.send(MediaFeedback::QueueDropped {
                    media: "d3d11-video",
                    count: opennow_streamer_platform_windows::ADAPTIVE_VIDEO_QUEUE_CAPACITY
                        + usize::from(!frame.keyframe),
                });
                if frame.keyframe {
                    // The Windows queue cleared the stale prediction chain but retained this
                    // recovery keyframe. Accept it immediately instead of requesting and then
                    // discarding another keyframe.
                    report_video_frame_accepted(&shared, &frame);
                    shared.video_desynced.store(false, Ordering::Release);
                    shared.keyframe_requested.store(false, Ordering::Release);
                    shared.windows_bridge.accept_keyframe();
                } else {
                    shared.video_desynced.store(true, Ordering::Release);
                    shared.windows_bridge.require_keyframe();
                    request_keyframe(&shared, &frame.mid, "D3D11 input queue overflow");
                }
            }
            Err(error) => {
                shared.video_desynced.store(true, Ordering::Release);
                shared.windows_bridge.require_keyframe();
                let _ = shared.feedback.send(MediaFeedback::DecoderError {
                    codec: match frame.codec {
                        MediaCodec::H264 => "h264",
                        MediaCodec::H265 => "h265",
                        MediaCodec::Av1 => "av1",
                        _ => "video",
                    },
                    message: error.to_string(),
                });
                request_keyframe(&shared, &frame.mid, "D3D11 decoder rejected an access unit");
            }
        }
    }
}

#[cfg(any(target_os = "windows", test))]
struct AdaptiveSampleClock {
    previous_timestamp_100ns: Option<i64>,
    nominal_duration_100ns: i64,
    recent_durations_100ns: VecDeque<i64>,
}

#[cfg(any(target_os = "windows", test))]
impl AdaptiveSampleClock {
    const HISTORY_LENGTH: usize = 120;

    fn new(maximum_fps: u32) -> Self {
        Self {
            previous_timestamp_100ns: None,
            nominal_duration_100ns: 10_000_000_i64 / i64::from(maximum_fps.max(1)),
            recent_durations_100ns: VecDeque::with_capacity(Self::HISTORY_LENGTH),
        }
    }

    fn observe(&mut self, timestamp_100ns: i64) -> i64 {
        let observed = self
            .previous_timestamp_100ns
            .replace(timestamp_100ns)
            .and_then(|previous| timestamp_100ns.checked_sub(previous))
            .filter(|duration| *duration > 0);

        if let Some(duration) = observed {
            if self.recent_durations_100ns.len() == Self::HISTORY_LENGTH {
                self.recent_durations_100ns.pop_front();
            }
            self.recent_durations_100ns.push_back(duration);
            return duration;
        }

        if self.recent_durations_100ns.is_empty() {
            self.nominal_duration_100ns
        } else {
            self.recent_durations_100ns.iter().copied().sum::<i64>()
                / self.recent_durations_100ns.len() as i64
        }
    }
}

#[cfg(any(target_os = "windows", test))]
fn media_timestamp_100ns(timestamp: u64, clock_rate_hz: u32) -> i64 {
    if clock_rate_hz == 0 {
        return 0;
    }
    let value = u128::from(timestamp)
        .saturating_mul(10_000_000)
        .checked_div(u128::from(clock_rate_hz))
        .unwrap_or(0);
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[cfg(target_os = "windows")]
fn invalidate_embedded_video(shared: &SharedPipeline, mid: &str, reason: &str) {
    // Invalidate queued AND already-dequeued access units. An older keyframe
    // submission must never clear a newer decoder/transport failure.
    shared.video.clear();
    opennow_streamer_protocol::log::log_async("WARN", "video-reference", reason);
    let _ = shared.feedback.send(MediaFeedback::RequestKeyframe {
        mid: mid.to_owned(),
        reason: reason.to_owned(),
    });
}

#[cfg(target_os = "windows")]
fn request_keyframe(shared: &SharedPipeline, mid: &str, reason: &str) {
    if !shared.keyframe_requested.swap(true, Ordering::AcqRel) {
        let _ = shared.feedback.send(MediaFeedback::RequestKeyframe {
            mid: mid.to_owned(),
            reason: reason.to_owned(),
        });
    }
}

impl Drop for MediaSession {
    fn drop(&mut self) {
        self.stop_inner();
    }
}

struct H264Decoder {
    decoder: OpenH264Decoder,
    parameter_sets: crate::h264::H264ParameterSets,
}

impl H264Decoder {
    fn new() -> Result<Self, String> {
        OpenH264Decoder::with_api_config(
            OpenH264API::from_source(),
            DecoderConfig::new().debug(false),
        )
        .map(|decoder| Self {
            decoder,
            parameter_sets: crate::h264::H264ParameterSets::default(),
        })
        .map_err(|error| format!("OpenH264 decoder initialization failed: {error}"))
    }

    fn reset(&mut self) -> Result<(), String> {
        let mut replacement = Self::new()?;
        let parameter_sets = self.parameter_sets.replay();
        if !parameter_sets.is_empty() {
            replacement
                .decoder
                .decode(&parameter_sets)
                .map_err(|error| format!("H.264 parameter-set replay failed: {error}"))?;
        }
        self.decoder = replacement.decoder;
        Ok(())
    }

    fn decode(&mut self, encoded: &[u8]) -> Result<Option<DecodedVideoFrame>, String> {
        let yuv = self
            .decoder
            .decode(encoded)
            .map_err(|error| error.to_string())?;
        self.parameter_sets.retain(encoded)?;
        let Some(yuv) = yuv else {
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
    last_frame_samples_per_channel: usize,
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
                // GFN audio uses 20 ms Opus packets. A discontinuity can
                // arrive before the first successful decode establishes the
                // packet duration, so begin with that negotiated baseline.
                last_frame_samples_per_channel: 960,
            })
            .map_err(|error| format!("Opus decoder initialization failed: {error}"))
    }

    fn decode(&mut self, encoded: &[u8]) -> Result<&[f32], String> {
        let samples_per_channel = self
            .decoder
            .decode_float(encoded, &mut self.scratch, false)
            .map_err(|error| error.to_string())?;
        if !encoded.is_empty() {
            self.last_frame_samples_per_channel = samples_per_channel;
        }
        Ok(&self.scratch[..samples_per_channel * self.channels as usize])
    }

    fn decode_packet_loss(&mut self) -> Result<&[f32], String> {
        // An empty Opus packet invokes decoder packet-loss concealment. In-band
        // FEC remains disabled; RFC 2198 recovery is handled by the transport.
        // Bound the output to the preceding packet duration. Giving libopus
        // the full 120 ms scratch buffer makes one loss synthesize 120 ms and
        // creates an audible latency spike.
        let output_len = self.last_frame_samples_per_channel * self.channels as usize;
        let samples_per_channel = self
            .decoder
            .decode_float(&[], &mut self.scratch[..output_len], false)
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
        if !frame.contiguous {
            shared.video_desynced.store(true, Ordering::Release);
        }
        if shared.video_desynced.load(Ordering::Acquire) {
            if !frame.keyframe {
                continue;
            }
            match decoder.reset() {
                Ok(()) => {}
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
                report_video_frame_accepted(&shared, &frame);
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
        if !frame.contiguous {
            match decoder.decode_packet_loss() {
                Ok(samples) => submit_decoded_audio(&shared, samples.to_vec(), configured_channels),
                Err(message) => {
                    let _ = shared.feedback.send(MediaFeedback::DecoderError {
                        codec: "opus-plc",
                        message,
                    });
                }
            }
        }
        match decoder.decode(&frame.data) {
            Ok(samples) => submit_decoded_audio(&shared, samples.to_vec(), configured_channels),
            Err(message) => {
                let _ = shared.feedback.send(MediaFeedback::DecoderError {
                    codec: "opus",
                    message,
                });
            }
        }
    }
}

fn submit_decoded_audio(shared: &SharedPipeline, samples: Vec<f32>, channels: u8) {
    let samples = if channels == 1 {
        samples
            .into_iter()
            .flat_map(|sample| [sample, sample])
            .collect::<Vec<_>>()
    } else {
        samples
    };
    #[cfg(target_os = "windows")]
    if !shared.windows_bridge.use_software()
        && let Some(backend) = shared.windows_bridge.backend()
    {
        use opennow_streamer_platform_windows::{
            AudioFormat, PcmFrame, PushOutcome as WindowsPushOutcome,
        };
        match backend.submit_audio(PcmFrame {
            samples,
            format: AudioFormat {
                sample_rate: OPUS_SAMPLE_RATE,
                channels: 2,
            },
        }) {
            Ok(WindowsPushOutcome::DroppedOldest) => {
                let _ = shared.feedback.send(MediaFeedback::QueueDropped {
                    media: "wasapi",
                    count: 1,
                });
            }
            Ok(WindowsPushOutcome::Queued | WindowsPushOutcome::Paused) => {}
            Err(error) => {
                let _ = shared.feedback.send(MediaFeedback::DecoderError {
                    codec: "opus",
                    message: error.to_string(),
                });
            }
        }
        return;
    }
    let dropped = shared.output.push_audio(&samples);
    if dropped > 0 {
        let _ = shared.feedback.send(MediaFeedback::QueueDropped {
            media: "audio-output",
            count: dropped,
        });
    }
}

#[cfg(target_os = "linux")]
fn run_linux_video(shared: Arc<SharedPipeline>, host_commands: Sender<HostCommand>) {
    while let Some(frame) = shared.video.pop() {
        if shared.linux_software_fallback.load(Ordering::Acquire) {
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
        if shared.paused.load(Ordering::Acquire) {
            continue;
        }
        *shared
            .linux_video_mid
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = frame.mid.clone();
        let timestamp_us = media_timestamp_us(frame.timestamp, frame.clock_rate_hz);
        let encoded = match opennow_streamer_platform_linux::EncodedVideoFrame::new(
            Arc::clone(&frame.data),
            timestamp_us,
            frame.keyframe,
        ) {
            Ok(encoded) => encoded,
            Err(error) => {
                trigger_linux_fallback(
                    &shared,
                    &host_commands,
                    format!("Linux decoder rejected encoded video framing: {error}"),
                );
                continue;
            }
        };
        let result = shared
            .linux_session
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .ok_or_else(|| "Linux hardware session is unavailable".to_owned())
            .and_then(|session| {
                session
                    .submit_video(encoded)
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(opennow_streamer_platform_linux::PushOutcome::Queued) => {
                report_video_frame_accepted(&shared, &frame);
                if frame.keyframe {
                    shared.video_desynced.store(false, Ordering::Release);
                    shared.keyframe_requested.store(false, Ordering::Release);
                }
            }
            Ok(opennow_streamer_platform_linux::PushOutcome::DroppedOldest) => {
                if frame.keyframe {
                    shared.video_desynced.store(false, Ordering::Release);
                    shared.keyframe_requested.store(false, Ordering::Release);
                } else {
                    shared.video_desynced.store(true, Ordering::Release);
                }
            }
            Ok(opennow_streamer_platform_linux::PushOutcome::Paused) => {}
            Err(reason) => trigger_linux_fallback(
                &shared,
                &host_commands,
                format!("Linux hardware video submission failed: {reason}"),
            ),
        }
    }
}

#[cfg(target_os = "linux")]
fn run_embedded_linux_video(shared: Arc<SharedPipeline>) {
    while let Some(frame) = shared.video.pop() {
        if shared.paused.load(Ordering::Acquire) {
            continue;
        }
        *shared
            .linux_video_mid
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = frame.mid.clone();
        let encoded = match opennow_streamer_platform_linux::EncodedVideoFrame::new(
            Arc::clone(&frame.data),
            media_timestamp_us(frame.timestamp, frame.clock_rate_hz),
            frame.keyframe,
        ) {
            Ok(encoded) => encoded,
            Err(error) => {
                let _ = shared.feedback.send(MediaFeedback::DecoderError {
                    codec: shared.linux_codec.label(),
                    message: error.to_string(),
                });
                request_linux_keyframe(&shared, "Linux decoder rejected encoded video framing");
                continue;
            }
        };
        let result = shared
            .linux_session
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .ok_or_else(|| "embedded Linux media session is unavailable".to_owned())
            .and_then(|session| {
                session
                    .submit_video(encoded)
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(opennow_streamer_platform_linux::PushOutcome::Queued) => {
                report_video_frame_accepted(&shared, &frame);
                if frame.keyframe {
                    shared.video_desynced.store(false, Ordering::Release);
                    shared.keyframe_requested.store(false, Ordering::Release);
                }
            }
            Ok(opennow_streamer_platform_linux::PushOutcome::DroppedOldest) => {
                shared.video_desynced.store(true, Ordering::Release);
                request_linux_keyframe(&shared, "embedded Linux decoder queue overflow");
            }
            Ok(opennow_streamer_platform_linux::PushOutcome::Paused) => {}
            Err(message) => {
                let _ = shared.feedback.send(MediaFeedback::DecoderError {
                    codec: shared.linux_codec.label(),
                    message,
                });
                request_linux_keyframe(&shared, "embedded Linux video submission failed");
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn run_embedded_linux_audio(shared: Arc<SharedPipeline>) {
    while let Some(frame) = shared.audio.pop() {
        if shared.paused.load(Ordering::Acquire) {
            continue;
        }
        let MediaCodec::Opus { .. } = frame.codec else {
            continue;
        };
        let packet = match opennow_streamer_platform_linux::AudioPacket::new(
            Arc::clone(&frame.data),
            media_timestamp_us(frame.timestamp, frame.clock_rate_hz),
        ) {
            Ok(packet) => packet,
            Err(error) => {
                let _ = shared.feedback.send(MediaFeedback::DecoderError {
                    codec: "opus",
                    message: error.to_string(),
                });
                continue;
            }
        };
        let result = shared
            .linux_session
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .ok_or_else(|| "embedded Linux media session is unavailable".to_owned())
            .and_then(|session| {
                session
                    .submit_audio(packet)
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(opennow_streamer_platform_linux::PushOutcome::DroppedOldest) => {
                let _ = shared.feedback.send(MediaFeedback::QueueDropped {
                    media: "linux-audio",
                    count: 1,
                });
            }
            Ok(opennow_streamer_platform_linux::PushOutcome::Queued)
            | Ok(opennow_streamer_platform_linux::PushOutcome::Paused) => {}
            Err(message) => {
                let _ = shared.feedback.send(MediaFeedback::DecoderError {
                    codec: "opus",
                    message,
                });
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn run_embedded_linux_monitor(
    shared: Arc<SharedPipeline>,
    publisher: crate::GraphicsFramePublisher,
    producer: crate::LinuxGpuFrameProducer,
) {
    use std::time::Duration;

    let mut playback_started = false;
    while !shared.stopped.load(Ordering::Acquire) {
        let (frames, events) = {
            let session = shared
                .linux_session
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let Some(session) = session.as_ref() else {
                return;
            };
            let mut decoded = Vec::new();
            while let Some(frame) = session.try_recv_frame() {
                decoded.push(frame);
            }
            let mut events = Vec::new();
            while let Some(event) = session.try_recv_event() {
                events.push(event);
            }
            (decoded, events)
        };
        if !shared.paused.load(Ordering::Acquire) {
            for decoded in frames {
                let Some(lease) = publisher.context() else {
                    continue;
                };
                match producer
                    .frame(decoded)
                    .map_err(|error| error.to_string())
                    .and_then(|frame| {
                        publisher
                            .publish(lease, Arc::new(frame))
                            .map_err(|error| error.to_string())
                    }) {
                    Ok(_) if !playback_started => {
                        playback_started = true;
                        let _ = shared.feedback.send(MediaFeedback::PlaybackStarted {
                            backend: "Linux decoder/embedded Vulkan",
                        });
                    }
                    Ok(_) => {}
                    Err(message) => {
                        let _ = shared.feedback.send(MediaFeedback::OutputError { message });
                    }
                }
            }
        }
        for event in events {
            match event {
                opennow_streamer_platform_linux::BackendEvent::DecoderChanged {
                    from,
                    to,
                    reason,
                } => {
                    let _ = shared.feedback.send(MediaFeedback::BackendFallback {
                        from: linux_decoder_name(from),
                        to: linux_decoder_name(to),
                        reason,
                    });
                }
                opennow_streamer_platform_linux::BackendEvent::NeedKeyframe => {
                    request_linux_keyframe(&shared, "Linux decoder requires a fresh keyframe");
                }
                opennow_streamer_platform_linux::BackendEvent::QueueOverflow { media } => {
                    let _ = shared
                        .feedback
                        .send(MediaFeedback::QueueDropped { media, count: 1 });
                }
                opennow_streamer_platform_linux::BackendEvent::DeviceLost { subsystem, reason } => {
                    let _ = shared.feedback.send(MediaFeedback::DeviceLost {
                        subsystem: linux_subsystem_name(subsystem),
                        recovered: false,
                        message: Some(reason),
                    });
                    request_linux_keyframe(&shared, "Linux decoder device was lost");
                }
                opennow_streamer_platform_linux::BackendEvent::Error(message) => {
                    let _ = shared.feedback.send(MediaFeedback::DecoderError {
                        codec: shared.linux_codec.label(),
                        message,
                    });
                    shared.stopped.store(true, Ordering::Release);
                    shared.video.close();
                    shared.audio.close();
                    stop_linux_session(&shared);
                    return;
                }
                opennow_streamer_platform_linux::BackendEvent::StateChanged(
                    opennow_streamer_platform_linux::LifecycleState::Failed,
                ) => {
                    let _ = shared.feedback.send(MediaFeedback::DecoderError {
                        codec: shared.linux_codec.label(),
                        message: "embedded Linux media session failed".to_owned(),
                    });
                    shared.stopped.store(true, Ordering::Release);
                    shared.video.close();
                    shared.audio.close();
                    stop_linux_session(&shared);
                    return;
                }
                opennow_streamer_platform_linux::BackendEvent::StateChanged(_)
                | opennow_streamer_platform_linux::BackendEvent::DecoderSelected(_)
                | opennow_streamer_platform_linux::BackendEvent::AudioSelected(_)
                | opennow_streamer_platform_linux::BackendEvent::FormatChanged(_) => {}
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    stop_linux_session(&shared);
}

#[cfg(target_os = "linux")]
fn run_linux_monitor(shared: Arc<SharedPipeline>, host_commands: Sender<HostCommand>) {
    use std::time::Duration;

    while !shared.stopped.load(Ordering::Acquire) {
        if shared.linux_software_fallback.load(Ordering::Acquire) {
            request_linux_keyframe(&shared, "Linux decoder fallback requires a fresh keyframe");
            stop_linux_session(&shared);
            return;
        }
        let (frames, events) = {
            let session = shared
                .linux_session
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let Some(session) = session.as_ref() else {
                return;
            };
            let mut frames = Vec::new();
            while let Some(frame) = session.try_recv_frame() {
                frames.push(frame);
            }
            let mut events = Vec::new();
            while let Some(event) = session.try_recv_event() {
                events.push(event);
            }
            (frames, events)
        };
        if !shared.paused.load(Ordering::Acquire) {
            for frame in frames {
                if shared.output.queue_linux_video(frame) {
                    let _ = shared.feedback.send(MediaFeedback::QueueDropped {
                        media: "linux-present",
                        count: 1,
                    });
                }
            }
        }
        for event in events {
            match event {
                opennow_streamer_platform_linux::BackendEvent::DecoderChanged {
                    from,
                    to,
                    reason,
                } => {
                    let _ = shared.feedback.send(MediaFeedback::BackendFallback {
                        from: linux_decoder_name(from),
                        to: linux_decoder_name(to),
                        reason,
                    });
                }
                opennow_streamer_platform_linux::BackendEvent::NeedKeyframe => {
                    request_linux_keyframe(&shared, "Linux decoder requires a fresh keyframe");
                }
                opennow_streamer_platform_linux::BackendEvent::QueueOverflow { media } => {
                    let _ = shared
                        .feedback
                        .send(MediaFeedback::QueueDropped { media, count: 1 });
                }
                opennow_streamer_platform_linux::BackendEvent::DeviceLost { subsystem, reason } => {
                    let _ = shared.feedback.send(MediaFeedback::DeviceLost {
                        subsystem: linux_subsystem_name(subsystem),
                        recovered: false,
                        message: Some(reason.clone()),
                    });
                    trigger_linux_fallback(
                        &shared,
                        &host_commands,
                        format!("{subsystem:?} device was lost: {reason}"),
                    );
                }
                opennow_streamer_platform_linux::BackendEvent::Error(reason) => {
                    trigger_linux_fallback(&shared, &host_commands, reason)
                }
                opennow_streamer_platform_linux::BackendEvent::StateChanged(
                    opennow_streamer_platform_linux::LifecycleState::Failed,
                ) => trigger_linux_fallback(
                    &shared,
                    &host_commands,
                    "Linux hardware media session failed".to_owned(),
                ),
                opennow_streamer_platform_linux::BackendEvent::StateChanged(_)
                | opennow_streamer_platform_linux::BackendEvent::DecoderSelected(_)
                | opennow_streamer_platform_linux::BackendEvent::AudioSelected(_)
                | opennow_streamer_platform_linux::BackendEvent::FormatChanged(_) => {}
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    stop_linux_session(&shared);
}

#[cfg(target_os = "linux")]
fn trigger_linux_fallback(
    shared: &SharedPipeline,
    host_commands: &Sender<HostCommand>,
    reason: String,
) {
    if shared.linux_codec != MediaVideoCodec::H264 {
        shared.stopped.store(true, Ordering::Release);
        shared.video.close();
        let _ = shared.feedback.send(MediaFeedback::DecoderError {
            codec: shared.linux_codec.label(),
            message: reason,
        });
        return;
    }
    if shared.linux_software_fallback.swap(true, Ordering::AcqRel) {
        return;
    }
    request_linux_keyframe(shared, "Linux decoder fallback requires a fresh keyframe");
    let _ = host_commands.send(HostCommand::FallbackLinux { reason });
}

#[cfg(target_os = "linux")]
fn request_linux_keyframe(shared: &SharedPipeline, reason: &str) {
    shared.video_desynced.store(true, Ordering::Release);
    if shared.keyframe_requested.swap(true, Ordering::AcqRel) {
        return;
    }
    let mid = shared
        .linux_video_mid
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    if !mid.is_empty() {
        let _ = shared.feedback.send(MediaFeedback::RequestKeyframe {
            mid,
            reason: reason.to_owned(),
        });
    }
}

#[cfg(target_os = "linux")]
fn stop_linux_session(shared: &SharedPipeline) {
    if let Some(mut session) = shared
        .linux_session
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take()
    {
        let _ = session.stop();
    }
}

#[cfg(target_os = "linux")]
const fn linux_decoder_name(
    backend: opennow_streamer_platform_linux::DecoderBackend,
) -> &'static str {
    match backend {
        opennow_streamer_platform_linux::DecoderBackend::Vulkan => "Vulkan Video/Vulkan",
        opennow_streamer_platform_linux::DecoderBackend::Cuda => "CUDA/NVDEC (CPU transfer)/Vulkan",
        opennow_streamer_platform_linux::DecoderBackend::VaApi => "VA-API/Vulkan",
        opennow_streamer_platform_linux::DecoderBackend::V4l2 => "V4L2/Vulkan",
        opennow_streamer_platform_linux::DecoderBackend::Ffmpeg => "FFmpeg software/Vulkan",
    }
}

#[cfg(target_os = "linux")]
const fn linux_subsystem_name(
    subsystem: opennow_streamer_platform_linux::Subsystem,
) -> &'static str {
    match subsystem {
        opennow_streamer_platform_linux::Subsystem::VaApi => "VA-API",
        opennow_streamer_platform_linux::Subsystem::V4l2 => "V4L2",
        opennow_streamer_platform_linux::Subsystem::Vulkan => "Vulkan",
        opennow_streamer_platform_linux::Subsystem::Ffmpeg => "FFmpeg",
        opennow_streamer_platform_linux::Subsystem::Opus => "Opus",
        opennow_streamer_platform_linux::Subsystem::PipeWire => "PipeWire",
        opennow_streamer_platform_linux::Subsystem::Alsa => "ALSA",
        opennow_streamer_platform_linux::Subsystem::Session => "Linux media session",
    }
}

#[cfg(target_os = "linux")]
fn media_timestamp_us(timestamp: u64, clock_rate_hz: u32) -> u64 {
    if clock_rate_hz == 0 {
        return 0;
    }
    timestamp.saturating_mul(1_000_000) / u64::from(clock_rate_hz)
}

#[cfg(target_os = "macos")]
fn run_macos_h264_video(
    shared: Arc<SharedPipeline>,
    host_commands: Sender<HostCommand>,
    stream_fps: u32,
    allow_software_fallback: bool,
) {
    use std::sync::mpsc;
    use std::time::Duration;

    use opennow_streamer_platform_macos::{
        FrameTiming, H264Format, SubmitOutcome, VideoColorSpace,
    };

    use crate::runtime::MacH264Configuration;

    let mut tracker = crate::macos_backend::H264ParameterSetTracker::default();
    let mut configured_parameter_sets = None;
    let mut backend_sink: Option<opennow_streamer_platform_macos::StreamSink> = None;
    let mut playback_started = false;
    while let Some(frame) = shared.video.pop() {
        if shared.paused.load(Ordering::Acquire) {
            continue;
        }
        if let Some(sink) = backend_sink.as_ref() {
            let mut decode_loss = None;
            while let Some(loss) = sink.pop_video_decode_loss() {
                decode_loss = Some(loss);
            }
            if let Some(loss) = decode_loss {
                let reason =
                    loss.status
                        .map_or("VideoToolbox produced no decoded pixel buffer", |status| {
                            if status == -12_909 {
                                "VideoToolbox rejected damaged H.264 data"
                            } else {
                                "VideoToolbox lost decoder synchronization"
                            }
                        });
                mark_macos_video_desynced(&shared, &frame.mid, reason);
            }
            if let Some(failure) = sink.fatal_failure() {
                if !allow_software_fallback {
                    let _ = shared.feedback.send(MediaFeedback::DecoderError {
                        codec: "h264",
                        message: format!(
                            "{}; the Qt Metal stream view requires hardware decoding",
                            failure.message
                        ),
                    });
                    return;
                }
                shared.mac_software_fallback.store(true, Ordering::Release);
                mark_macos_video_desynced(
                    &shared,
                    &frame.mid,
                    &format!(
                        "{} failure requires software decode",
                        failure.subsystem.name()
                    ),
                );
            }
        }
        if shared.mac_software_fallback.load(Ordering::Acquire) {
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
        let parameter_sets = tracker.parameter_sets();
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
                    tracker.commit_parameter_sets(parameter_sets.clone());
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
        } else if let Some(ref parameter_sets) = parameter_sets
            && (configured_parameter_sets.as_ref() != Some(parameter_sets)
                || !frame.contiguous
                || shared.video_desynced.load(Ordering::Acquire))
            && frame.keyframe
        {
            let format = H264Format::new(parameter_sets.clone(), VideoColorSpace::Bt709);
            let Some(sink) = backend_sink.as_ref() else {
                return;
            };
            if let Err(error) = sink.reconfigure_h264(format) {
                eprintln!(
                    "Rejected H.264 parameter-set update; retaining the last known-good VideoToolbox decoder: {error} (spsBytes={}, ppsBytes={})",
                    parameter_sets.sequence().len(),
                    parameter_sets.picture().len(),
                );
                mark_macos_video_desynced(
                    &shared,
                    &frame.mid,
                    "rejected H.264 parameter-set update; waiting for a clean keyframe",
                );
                continue;
            }
            tracker.commit_parameter_sets(parameter_sets.clone());
            configured_parameter_sets = Some(parameter_sets.clone());
        } else if let Some(parameter_sets) = parameter_sets
            && configured_parameter_sets.as_ref() == Some(&parameter_sets)
        {
            // Clear an identical candidate pair without perturbing the active decoder.
            tracker.commit_parameter_sets(parameter_sets);
        }

        shared.video_desynced.store(false, Ordering::Release);
        shared.keyframe_requested.store(false, Ordering::Release);
        let timescale = i32::try_from(frame.clock_rate_hz)
            .ok()
            .filter(|timescale| *timescale > 0)
            .unwrap_or(90_000);
        let timing = FrameTiming::new(
            i64::try_from(frame.timestamp).unwrap_or(i64::MAX),
            i64::from(timescale) / i64::from(stream_fps.max(1)),
            timescale,
        );
        let Some(sink) = backend_sink.as_ref() else {
            return;
        };
        match sink.submit_h264(&frame.data, framing, timing) {
            Ok(SubmitOutcome::Accepted) => report_video_frame_accepted(&shared, &frame),
            Ok(SubmitOutcome::Paused) => {}
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
                mark_macos_video_desynced(
                    &shared,
                    &frame.mid,
                    &format!("VideoToolbox rejected an H.264 access unit: {error}"),
                );
            }
        }
        let stats = sink.stats();
        if !playback_started && (stats.video_presented > 0 || stats.video_metal_completed > 0) {
            playback_started = true;
            let _ = shared.feedback.send(MediaFeedback::PlaybackStarted {
                backend: "VideoToolbox/Metal",
            });
        }
    }
}

#[cfg(target_os = "macos")]
fn run_macos_h265_video(
    shared: Arc<SharedPipeline>,
    host_commands: Sender<HostCommand>,
    stream_fps: u32,
) {
    use std::sync::mpsc;
    use std::time::Duration;

    use opennow_streamer_platform_macos::{FrameTiming, SubmitOutcome};

    let mut tracker = crate::macos_backend::H265ParameterSetTracker::default();
    let mut configured_parameter_sets = None;
    let mut backend_sink: Option<opennow_streamer_platform_macos::StreamSink> = None;
    let mut playback_started = false;
    while let Some(frame) = shared.video.pop() {
        if shared.paused.load(Ordering::Acquire) {
            continue;
        }
        if let Some(sink) = backend_sink.as_ref() {
            let mut decode_loss = None;
            while let Some(loss) = sink.pop_video_decode_loss() {
                decode_loss = Some(loss);
            }
            if let Some(loss) = decode_loss {
                let reason = loss.status.map_or(
                    "VideoToolbox produced no decoded HEVC pixel buffer",
                    |_| "VideoToolbox lost HEVC decoder synchronization",
                );
                mark_macos_video_desynced(&shared, &frame.mid, reason);
            }
            if let Some(failure) = sink.fatal_failure() {
                let _ = shared.feedback.send(MediaFeedback::DecoderError {
                    codec: "h265",
                    message: format!(
                        "{} failed while decoding HEVC: {}",
                        failure.subsystem.name(),
                        failure.message
                    ),
                });
                mark_macos_video_desynced(&shared, &frame.mid, "VideoToolbox HEVC backend failed");
                return;
            }
        }
        let framing = match tracker.observe(&frame.data) {
            Ok(framing) => framing,
            Err(message) => {
                let _ = shared.feedback.send(MediaFeedback::DecoderError {
                    codec: "h265",
                    message,
                });
                mark_macos_video_desynced(&shared, &frame.mid, "invalid H.265 framing");
                continue;
            }
        };
        let parameter_sets = tracker.parameter_sets();
        if shared.video_desynced.load(Ordering::Acquire) && !frame.keyframe {
            continue;
        }
        if backend_sink.is_none() {
            let Some(parameter_sets) = parameter_sets.clone() else {
                mark_macos_video_desynced(
                    &shared,
                    &frame.mid,
                    "VideoToolbox is waiting for H.265 VPS/SPS/PPS",
                );
                continue;
            };
            let (reply, response) = mpsc::channel();
            if host_commands
                .send(HostCommand::ConfigureMacH265 {
                    parameter_sets: parameter_sets.clone(),
                    reply,
                })
                .is_err()
            {
                return;
            }
            match response.recv_timeout(Duration::from_secs(10)) {
                Ok(Ok(sink)) => {
                    *shared
                        .mac_sink
                        .lock()
                        .unwrap_or_else(|error| error.into_inner()) = Some(sink.clone());
                    tracker.commit_parameter_sets(parameter_sets.clone());
                    configured_parameter_sets = Some(parameter_sets);
                    backend_sink = Some(sink);
                }
                Ok(Err(message)) => {
                    let _ = shared.feedback.send(MediaFeedback::DecoderError {
                        codec: "h265",
                        message,
                    });
                    return;
                }
                Err(_) => {
                    let _ = shared.feedback.send(MediaFeedback::DecoderError {
                        codec: "h265",
                        message: "VideoToolbox HEVC initialization timed out on the main thread"
                            .to_owned(),
                    });
                    return;
                }
            }
        } else if let Some(ref parameter_sets) = parameter_sets
            && configured_parameter_sets.as_ref() != Some(parameter_sets)
            && frame.keyframe
        {
            let format = shared.stream.macos_h265_format(parameter_sets.clone());
            let Some(sink) = backend_sink.as_ref() else {
                return;
            };
            if let Err(error) = sink.reconfigure_h265(format) {
                eprintln!(
                    "Rejected H.265 parameter-set update; retaining the last known-good VideoToolbox decoder: {error} (vpsBytes={}, spsBytes={}, ppsBytes={})",
                    parameter_sets.video().len(),
                    parameter_sets.sequence().len(),
                    parameter_sets.picture().len(),
                );
                mark_macos_video_desynced(
                    &shared,
                    &frame.mid,
                    "rejected H.265 parameter-set update; waiting for a clean keyframe",
                );
                continue;
            }
            tracker.commit_parameter_sets(parameter_sets.clone());
            configured_parameter_sets = Some(parameter_sets.clone());
        } else if let Some(parameter_sets) = parameter_sets
            && configured_parameter_sets.as_ref() == Some(&parameter_sets)
        {
            tracker.commit_parameter_sets(parameter_sets);
        }

        shared.video_desynced.store(false, Ordering::Release);
        shared.keyframe_requested.store(false, Ordering::Release);
        let timescale = i32::try_from(frame.clock_rate_hz)
            .ok()
            .filter(|timescale| *timescale > 0)
            .unwrap_or(90_000);
        let timing = FrameTiming::new(
            i64::try_from(frame.timestamp).unwrap_or(i64::MAX),
            i64::from(timescale) / i64::from(stream_fps.max(1)),
            timescale,
        );
        let Some(sink) = backend_sink.as_ref() else {
            return;
        };
        match sink.submit_h265(&frame.data, framing, timing) {
            Ok(SubmitOutcome::Accepted) => report_video_frame_accepted(&shared, &frame),
            Ok(SubmitOutcome::Paused) => {}
            Ok(SubmitOutcome::Backpressured | SubmitOutcome::ReplacedOldest) => {
                let _ = shared.feedback.send(MediaFeedback::QueueDropped {
                    media: "videotoolbox-hevc",
                    count: 1,
                });
                mark_macos_video_desynced(
                    &shared,
                    &frame.mid,
                    "VideoToolbox HEVC decode queue backpressure",
                );
            }
            Err(error) => {
                mark_macos_video_desynced(
                    &shared,
                    &frame.mid,
                    &format!("VideoToolbox rejected an H.265 access unit: {error}"),
                );
            }
        }
        let stats = sink.stats();
        if !playback_started && (stats.video_presented > 0 || stats.video_metal_completed > 0) {
            playback_started = true;
            let _ = shared.feedback.send(MediaFeedback::PlaybackStarted {
                backend: "VideoToolbox HEVC/Metal",
            });
        }
    }
}

#[cfg(target_os = "macos")]
fn run_macos_av1_video(
    shared: Arc<SharedPipeline>,
    host_commands: Sender<HostCommand>,
    stream: MediaStreamConfig,
) {
    use std::sync::mpsc;
    use std::time::Duration;

    use opennow_streamer_platform_macos::{Av1Format, FrameTiming, SubmitOutcome};

    let mut configured_codec_configuration: Option<Vec<u8>> = None;
    let mut backend_sink: Option<opennow_streamer_platform_macos::StreamSink> = None;
    let mut playback_started = false;
    while let Some(frame) = shared.video.pop() {
        if shared.paused.load(Ordering::Acquire) {
            continue;
        }
        if let Some(sink) = backend_sink.as_ref() {
            let mut decode_loss = None;
            while let Some(loss) = sink.pop_video_decode_loss() {
                decode_loss = Some(loss);
            }
            if let Some(loss) = decode_loss {
                let reason = loss.status.map_or(
                    "VideoToolbox produced no decoded AV1 pixel buffer",
                    |_| "VideoToolbox lost AV1 decoder synchronization",
                );
                mark_macos_video_desynced(&shared, &frame.mid, reason);
            }
            if let Some(failure) = sink.fatal_failure() {
                let _ = shared.feedback.send(MediaFeedback::DecoderError {
                    codec: "av1",
                    message: format!(
                        "{} failed while decoding AV1: {}",
                        failure.subsystem.name(),
                        failure.message
                    ),
                });
                mark_macos_video_desynced(&shared, &frame.mid, "VideoToolbox AV1 backend failed");
                return;
            }
        }
        if shared.video_desynced.load(Ordering::Acquire) && !frame.keyframe {
            continue;
        }

        let candidate_configuration = frame
            .keyframe
            .then(|| crate::recording::av1_codec_private(&frame.data));
        if backend_sink.is_none() {
            let codec_configuration = match candidate_configuration {
                Some(Ok(configuration)) => configuration,
                Some(Err(_)) | None => {
                    mark_macos_video_desynced(
                        &shared,
                        &frame.mid,
                        "VideoToolbox is waiting for an AV1 sequence-header keyframe",
                    );
                    continue;
                }
            };
            let format = match Av1Format::new(
                &codec_configuration,
                stream.width,
                stream.height,
                stream.macos_color_space(),
            ) {
                Ok(format) => format.with_transfer(stream.macos_transfer()),
                Err(error) => {
                    let _ = shared.feedback.send(MediaFeedback::DecoderError {
                        codec: "av1",
                        message: format!("invalid AV1 VideoToolbox format: {error}"),
                    });
                    mark_macos_video_desynced(
                        &shared,
                        &frame.mid,
                        "invalid AV1 VideoToolbox configuration",
                    );
                    continue;
                }
            };
            let (reply, response) = mpsc::channel();
            if host_commands
                .send(HostCommand::ConfigureMacAv1 { format, reply })
                .is_err()
            {
                return;
            }
            match response.recv_timeout(Duration::from_secs(10)) {
                Ok(Ok(sink)) => {
                    *shared
                        .mac_sink
                        .lock()
                        .unwrap_or_else(|error| error.into_inner()) = Some(sink.clone());
                    configured_codec_configuration = Some(codec_configuration);
                    backend_sink = Some(sink);
                }
                Ok(Err(message)) => {
                    let _ = shared.feedback.send(MediaFeedback::DecoderError {
                        codec: "av1",
                        message,
                    });
                    return;
                }
                Err(_) => {
                    let _ = shared.feedback.send(MediaFeedback::DecoderError {
                        codec: "av1",
                        message: "VideoToolbox AV1 initialization timed out on the main thread"
                            .to_owned(),
                    });
                    return;
                }
            }
        } else if let Some(Ok(codec_configuration)) = candidate_configuration
            && configured_codec_configuration.as_ref() != Some(&codec_configuration)
        {
            let format = match Av1Format::new(
                &codec_configuration,
                stream.width,
                stream.height,
                stream.macos_color_space(),
            ) {
                Ok(format) => format.with_transfer(stream.macos_transfer()),
                Err(error) => {
                    eprintln!("Rejected AV1 configuration update: {error}");
                    mark_macos_video_desynced(
                        &shared,
                        &frame.mid,
                        "rejected AV1 configuration update; waiting for a clean keyframe",
                    );
                    continue;
                }
            };
            let Some(sink) = backend_sink.as_ref() else {
                return;
            };
            if let Err(error) = sink.reconfigure_av1(format) {
                eprintln!(
                    "Rejected AV1 sequence-header update; retaining the last known-good VideoToolbox decoder: {error} (configurationBytes={})",
                    codec_configuration.len(),
                );
                mark_macos_video_desynced(
                    &shared,
                    &frame.mid,
                    "rejected AV1 sequence-header update; waiting for a clean keyframe",
                );
                continue;
            }
            configured_codec_configuration = Some(codec_configuration);
        }

        shared.video_desynced.store(false, Ordering::Release);
        shared.keyframe_requested.store(false, Ordering::Release);
        let timescale = i32::try_from(frame.clock_rate_hz)
            .ok()
            .filter(|timescale| *timescale > 0)
            .unwrap_or(90_000);
        let timing = FrameTiming::new(
            i64::try_from(frame.timestamp).unwrap_or(i64::MAX),
            i64::from(timescale) / i64::from(stream.fps.max(1)),
            timescale,
        );
        let Some(sink) = backend_sink.as_ref() else {
            return;
        };
        match sink.submit_av1(&frame.data, timing) {
            Ok(SubmitOutcome::Accepted) => report_video_frame_accepted(&shared, &frame),
            Ok(SubmitOutcome::Paused) => {}
            Ok(SubmitOutcome::Backpressured | SubmitOutcome::ReplacedOldest) => {
                let _ = shared.feedback.send(MediaFeedback::QueueDropped {
                    media: "videotoolbox-av1",
                    count: 1,
                });
                mark_macos_video_desynced(
                    &shared,
                    &frame.mid,
                    "VideoToolbox AV1 decode queue backpressure",
                );
            }
            Err(error) => {
                mark_macos_video_desynced(
                    &shared,
                    &frame.mid,
                    &format!("VideoToolbox rejected an AV1 temporal unit: {error}"),
                );
            }
        }
        let stats = sink.stats();
        if !playback_started && (stats.video_presented > 0 || stats.video_metal_completed > 0) {
            playback_started = true;
            let _ = shared.feedback.send(MediaFeedback::PlaybackStarted {
                backend: "VideoToolbox AV1/Metal",
            });
        }
    }
}

fn report_video_frame_accepted(shared: &SharedPipeline, frame: &EncodedFrame) {
    let _ = shared.feedback.send(MediaFeedback::VideoFrameAccepted {
        frame_index: frame.frame_index,
        timestamp: frame.timestamp,
        bytes: u32::try_from(frame.data.len()).unwrap_or(u32::MAX),
        keyframe: frame.keyframe,
    });
}

#[cfg(target_os = "macos")]
fn run_macos_audio(shared: Arc<SharedPipeline>) {
    use opennow_streamer_platform_macos::{AudioFormat, SubmitOutcome};

    let mut configured_channels = 2;
    while let Some(frame) = shared.audio.pop() {
        if shared.paused.load(Ordering::Acquire) {
            continue;
        }
        let native_sink = shared
            .mac_sink
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        if native_sink
            .as_ref()
            .and_then(|sink| sink.fatal_failure())
            .is_some()
        {
            shared.mac_software_fallback.store(true, Ordering::Release);
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
        let Some(sink) = native_sink else {
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

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_color_metadata_tracks_accepted_hdr_without_inference_from_depth() {
        use opennow_streamer_platform_macos::{VideoColorSpace, VideoTransfer};
        for color_quality in [MediaColorQuality::TenBit420, MediaColorQuality::TenBit444] {
            let mut stream = MediaStreamConfig {
                color_quality,
                ..Default::default()
            };
            assert_eq!(stream.macos_color_space(), VideoColorSpace::Bt709);
            assert_eq!(stream.macos_transfer(), VideoTransfer::Sdr);
            stream.hdr = true;
            assert_eq!(stream.macos_color_space(), VideoColorSpace::Bt2020);
            assert_eq!(stream.macos_transfer(), VideoTransfer::Pq);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_stream_color_profiles_preserve_chroma_and_bit_depth() {
        use opennow_streamer_platform_linux::PixelFormat;
        for (color, format) in [
            (MediaColorQuality::EightBit420, PixelFormat::Nv12),
            (MediaColorQuality::EightBit444, PixelFormat::Nv24),
            (MediaColorQuality::TenBit420, PixelFormat::P010),
            (MediaColorQuality::TenBit444, PixelFormat::P410),
        ] {
            assert_eq!(color.linux_pixel_format(), format);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn embedded_linux_rejects_color_downgrades_and_missing_vulkan_owner() {
        use opennow_streamer_platform_linux::DecoderPreference;
        for (color_quality, preference, expected) in [
            (
                MediaColorQuality::EightBit444,
                DecoderPreference::VulkanOnly,
                "4:4:4",
            ),
            (
                MediaColorQuality::TenBit444,
                DecoderPreference::VulkanOnly,
                "4:4:4",
            ),
            (
                MediaColorQuality::TenBit420,
                DecoderPreference::CudaOnly,
                "10-bit",
            ),
            (
                MediaColorQuality::EightBit420,
                DecoderPreference::VulkanOnly,
                "attached shared device",
            ),
            (
                MediaColorQuality::TenBit420,
                DecoderPreference::VulkanOnly,
                "attached shared device",
            ),
        ] {
            let (feedback, _feedback_receiver) = std::sync::mpsc::channel();
            let (commands, _command_receiver) = std::sync::mpsc::channel();
            let (_graphics, frames) = crate::RenderThreadGraphics::new(|| {});
            let result = MediaSession::spawn_embedded_linux(
                Arc::new(OutputBuffers::new()),
                feedback,
                commands,
                MediaSessionConfig {
                    stream: MediaStreamConfig {
                        color_quality,
                        ..MediaStreamConfig::default()
                    },
                    audio_device: &opennow_streamer_protocol::AudioOutputDevice::default(),
                },
                frames,
                LinuxVideoSelection {
                    path: LinuxVideoPath::Hardware(preference),
                    use_vulkan_output: true,
                    fallback_reason: None,
                },
                None,
            );
            assert!(result.err().is_some_and(|error| error.contains(expected)));
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn embedded_playback_starts_only_once_after_a_successful_record() {
        let mut state = EmbeddedD3d11State::new(
            MediaStreamConfig::default(),
            Arc::new(Mutex::new(EmbeddedD3d11Submission::new())),
        );
        assert!(!state.take_first_recorded_frame(false));
        assert!(!state.take_first_recorded_frame(false));
        assert!(state.take_first_recorded_frame(true));
        assert!(!state.take_first_recorded_frame(true));
        state.reset();
        assert!(
            !state.take_first_recorded_frame(true),
            "surface recreation is not a new session"
        );
    }

    #[cfg(target_os = "windows")]
    fn embedded_h264_frame(
        timestamp_100ns: i64,
        key_frame: bool,
    ) -> opennow_streamer_platform_windows::EncodedVideoFrame {
        opennow_streamer_platform_windows::EncodedVideoFrame {
            codec: opennow_streamer_platform_windows::VideoCodec::H264,
            data: vec![0, 0, 0, 1, if key_frame { 0x65 } else { 0x41 }],
            timestamp_100ns,
            duration_100ns: 166_667,
            key_frame,
            reset_decoder: key_frame,
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn embedded_d3d11_retirement_rearms_graphics_and_reference_recovery() {
        use crate::GraphicsFrame;

        let submission = Arc::new(Mutex::new(EmbeddedD3d11Submission::new()));
        let state = Arc::new(Mutex::new(EmbeddedD3d11State::new(
            MediaStreamConfig::default(),
            Arc::clone(&submission),
        )));
        state.lock().unwrap().first_frame_recorded = true;
        let (shared, _feedback) = software_test_pipeline();
        let frame = PendingD3d11Frame {
            state: Arc::clone(&state),
            shared,
            mid: "video".to_owned(),
            info: crate::GraphicsFrameInfo {
                width: 1920,
                height: 1080,
                sequence: 1,
                presentation_time_ns: 0,
            },
        };
        let resources = frame.render_resources().expect("D3D11 render resources");
        assert!(Arc::ptr_eq(&resources, &frame.render_resources().unwrap()));

        for cycle in 0..3 {
            submission
                .lock()
                .unwrap()
                .push(embedded_h264_frame(cycle, true))
                .unwrap();
            resources.retire().unwrap();
            resources.retire().unwrap();

            let mut state = state.lock().unwrap();
            assert!(state.producer.is_none());
            assert!(state.take_keyframe_required());
            assert!(!state.take_keyframe_required());
            assert!(!state.take_first_recorded_frame(true));
            drop(state);

            let mut submission = submission.lock().unwrap();
            assert!(submission.pending.is_empty());
            assert!(submission.submitter.is_none());
            let outcome = submission
                .push(embedded_h264_frame(cycle + 1, false))
                .unwrap();
            assert!(outcome.queued && outcome.needs_graphics);
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn embedded_d3d11_detaches_submission_before_retiring_decoder() {
        struct RetiredDecoder<'a>(&'a Mutex<EmbeddedD3d11Submission>);
        impl Drop for RetiredDecoder<'_> {
            fn drop(&mut self) {
                // Simulate a concurrent submission while decoder teardown joins its
                // worker. No old prediction chain or locked submission mutex may remain.
                let mut submission = self.0.try_lock().expect("teardown held submission lock");
                assert!(submission.pending.is_empty());
                assert!(submission.submitter.is_none());
                let outcome = submission.push(embedded_h264_frame(100, true)).unwrap();
                assert!(outcome.queued && outcome.needs_graphics);
            }
        }
        let submission = Mutex::new(EmbeddedD3d11Submission::new());
        submission
            .lock()
            .unwrap()
            .push(embedded_h264_frame(1, true))
            .unwrap();
        let mut producer = Some(RetiredDecoder(&submission));
        detach_d3d11_producer(&submission, &mut producer);
        assert!(producer.is_none());
        assert_eq!(
            submission
                .lock()
                .unwrap()
                .pending
                .front()
                .unwrap()
                .timestamp_100ns,
            100
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn embedded_d3d11_overflow_discards_the_entire_reference_chain() {
        let mut state = EmbeddedD3d11Submission::new();
        let capacity = opennow_streamer_platform_windows::ADAPTIVE_VIDEO_QUEUE_CAPACITY;
        for index in 0..capacity {
            assert_eq!(
                state.push(embedded_h264_frame(index as i64, index == 0)),
                Ok(EmbeddedD3d11SubmissionOutcome {
                    queued: true,
                    dropped: 0,
                    needs_graphics: true,
                })
            );
        }
        assert_eq!(state.pending.len(), capacity);

        assert_eq!(
            state.push(embedded_h264_frame(capacity as i64, false)),
            Ok(EmbeddedD3d11SubmissionOutcome {
                queued: false,
                dropped: capacity + 1,
                needs_graphics: true,
            })
        );
        assert!(state.pending.is_empty());

        assert_eq!(
            state.push(embedded_h264_frame(100, true)),
            Ok(EmbeddedD3d11SubmissionOutcome {
                queued: true,
                dropped: 0,
                needs_graphics: true,
            })
        );
        assert_eq!(state.pending.len(), 1);
        assert!(state.pending.front().is_some_and(|frame| frame.key_frame));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn embedded_d3d11_overflow_retains_the_incoming_recovery_keyframe() {
        let mut state = EmbeddedD3d11Submission::new();
        let capacity = opennow_streamer_platform_windows::ADAPTIVE_VIDEO_QUEUE_CAPACITY;
        for index in 0..capacity {
            assert!(
                state
                    .push(embedded_h264_frame(index as i64, index == 0))
                    .is_ok_and(|outcome| outcome.queued && outcome.dropped == 0)
            );
        }

        assert_eq!(
            state.push(embedded_h264_frame(capacity as i64, true)),
            Ok(EmbeddedD3d11SubmissionOutcome {
                queued: true,
                dropped: capacity,
                needs_graphics: true,
            })
        );
        assert_eq!(state.pending.len(), 1);
        assert!(state.pending.front().is_some_and(|frame| frame.key_frame));
    }

    #[test]
    fn shortcut_chords_parse_supported_keys_and_reject_ambiguous_input() {
        assert_eq!(
            ShortcutChord::parse("Ctrl+Shift+Q"),
            Some(ShortcutChord {
                virtual_key: u16::from(b'Q'),
                modifiers: 0x03,
            })
        );
        assert_eq!(
            ShortcutChord::parse("command+f24"),
            Some(ShortcutChord {
                virtual_key: 0x87,
                modifiers: 0x08,
            })
        );
        assert_eq!(ShortcutChord::parse("Ctrl"), None);
        assert_eq!(ShortcutChord::parse("Ctrl+Control+Q"), None);
        assert_eq!(ShortcutChord::parse("Ctrl+Q+W"), None);
        assert_eq!(ShortcutChord::parse("Ctrl+"), None);
        assert_eq!(ShortcutChord::parse("F25"), None);
    }

    #[test]
    fn shortcut_bindings_use_custom_values_defaults_and_stable_conflict_order() {
        let bindings = StreamShortcutBindings::from_json(&serde_json::json!({
            "toggleStats":"Alt+S",
            "togglePointerLock":"Alt+S",
            "toggleFullscreen":"not-a-key"
        }));
        assert_eq!(
            bindings.action(u16::from(b'S'), 0x04),
            Some(StreamShortcutAction::ToggleStats)
        );
        assert_eq!(
            bindings.action(0x7a, 0),
            Some(StreamShortcutAction::ToggleFullscreen)
        );
        assert_eq!(
            StreamShortcutBindings::default().action(0x7a, 0x02),
            Some(StreamShortcutAction::Screenshot)
        );
    }

    #[test]
    fn replay_and_manual_recording_share_payloads_and_stop_independently() {
        let recording = RecordingTap::default();
        let receiver = recording.subscribe().unwrap();
        let replay = crate::replay::ReplayTap::default();
        replay.start(
            opennow_streamer_protocol::ReplayBufferConfig::from_settings(
                &serde_json::json!({"replayBufferEnabled":true}),
            ),
        );
        let frame = EncodedFrame {
            mid: "video".to_owned(),
            codec: MediaCodec::H264,
            data: Arc::from([0_u8, 0, 0, 1, 0x65]),
            frame_index: Some(7),
            timestamp: 90_000,
            clock_rate_hz: 90_000,
            keyframe: true,
            contiguous: true,
        };
        recording.publish(&frame);
        replay.publish(&frame);
        let recorded = receiver.recv().unwrap();
        let snapshot = replay.snapshot().unwrap();
        assert!(Arc::ptr_eq(&recorded.data, &snapshot.frames[0].frame.data));
        assert_eq!(recorded.timestamp, snapshot.frames[0].frame.timestamp);
        replay.stop();
        recording.publish(&frame);
        assert_eq!(receiver.recv().unwrap().frame_index, Some(7));
        assert!(recording.subscribe().is_err());
        recording.unsubscribe();
        assert!(receiver.recv().is_err());
    }

    #[test]
    fn replay_shortcut_and_qt_portable_key_aliases_are_supported() {
        assert_eq!(
            StreamShortcutBindings::default().action(0x7b, 0x02),
            Some(StreamShortcutAction::SaveClip)
        );
        assert_eq!(
            StreamShortcutBindings::default().action(0x7b, 0),
            Some(StreamShortcutAction::ToggleRecording)
        );
        assert_eq!(
            StreamShortcutBindings::from_json(&serde_json::json!({"saveClip":"Alt+C"}))
                .action(u16::from(b'C'), 0x04),
            Some(StreamShortcutAction::SaveClip)
        );
        assert_eq!(StreamShortcutAction::SaveClip.protocol_name(), "save-clip");
        for (portable, native) in [
            ("Ctrl+PgUp", "Ctrl+PageUp"),
            ("Alt+PgDown", "Alt+PageDown"),
            ("Print", "PrintScreen"),
        ] {
            assert_eq!(ShortcutChord::parse(portable), ShortcutChord::parse(native));
            assert!(ShortcutChord::parse(portable).is_some());
        }
    }

    #[test]
    fn encoded_recording_tap_fails_closed_on_overflow() {
        let tap = RecordingTap::default();
        let receiver = tap.subscribe().expect("recording subscription");
        let frame = EncodedFrame {
            mid: "video".to_owned(),
            codec: MediaCodec::H264,
            data: Arc::from([0_u8, 0, 0, 1, 0x65]),
            frame_index: Some(1),
            timestamp: 0,
            clock_rate_hz: 90_000,
            keyframe: true,
            contiguous: true,
        };
        for _ in 0..=RECORDING_TAP_QUEUE_CAPACITY {
            tap.publish(&frame);
        }
        assert!(receiver.overflowed());
        for _ in 0..RECORDING_TAP_QUEUE_CAPACITY {
            receiver.recv().expect("queued recording frame");
        }
        assert!(receiver.recv().is_err());
    }
    use openh264::encoder::Encoder;
    use openh264::formats::{RgbSliceU8, YUVBuffer};
    use opus::{Application, Encoder as OpusEncoder};

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_encoded_queue_keeps_bounded_scheduler_burst_tolerance() {
        assert_eq!(macos_video_queue_capacity(30), 8);
        assert_eq!(macos_video_queue_capacity(60), 15);
        assert_eq!(macos_video_queue_capacity(120), 30);
        assert_eq!(macos_video_queue_capacity(240), 60);
    }

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
    fn software_recovery_replays_parameter_sets_for_an_idr_only_access_unit() {
        let rgb = vec![96_u8; 32 * 32 * 3];
        let yuv = YUVBuffer::from_rgb_source(RgbSliceU8::new(&rgb, (32, 32)));
        let mut encoder = Encoder::new().unwrap();
        let initial = encoder.encode(&yuv).unwrap().to_vec();
        encoder.force_intra_frame();
        let recovery = encoder.encode(&yuv).unwrap().to_vec();
        let idr_only: Vec<u8> = openh264::nal_units(&recovery)
            .filter(|nal| nal.get(3).is_some_and(|header| header & 0x1f == 5))
            .flatten()
            .copied()
            .collect();
        assert!(!idr_only.is_empty());
        assert!(H264Decoder::new().unwrap().decode(&idr_only).is_err());

        let mut reference = H264Decoder::new().unwrap();
        reference.decode(&initial).unwrap();
        let expected = reference.decode(&idr_only).unwrap().unwrap();
        let mut decoder = H264Decoder::new().unwrap();
        assert!(decoder.decode(&initial).unwrap().is_some());
        let parameter_sets = decoder.parameter_sets.replay();
        assert!(!parameter_sets.is_empty());
        assert!(decoder.decode(&[0, 0, 1, 0x67, 0xff]).is_err());
        assert_eq!(decoder.parameter_sets.replay(), parameter_sets);
        decoder.reset().unwrap();
        let recovered = decoder.decode(&idr_only).unwrap().unwrap();
        assert_eq!((recovered.width, recovered.height), (32, 32));
        assert_eq!(recovered.rgb.len(), 32 * 32 * 3);
        assert_eq!(recovered.rgb, expected.rgb);
        decoder.reset().unwrap();
        assert!(decoder.decode(&idr_only).unwrap().is_some());
    }

    #[test]
    fn captured_input_queue_preserves_raw_motion_and_fails_closed_on_control_overflow() {
        let queue = CapturedInputQueue::default();
        for _ in 0..3 {
            queue.push(CapturedInput::MouseMove {
                delta_x: 1,
                delta_y: -1,
            });
        }
        for _ in 0..3 {
            assert_eq!(
                queue.take(),
                Some(CapturedInput::MouseMove {
                    delta_x: 1,
                    delta_y: -1,
                })
            );
        }

        for virtual_key in 0..=u16::try_from(CAPTURED_INPUT_CAPACITY).unwrap() {
            queue.push(CapturedInput::Key {
                virtual_key,
                modifiers: 0,
                pressed: true,
            });
        }
        assert!(queue.take_overflowed());
        assert_eq!(
            queue.take(),
            Some(CapturedInput::Key {
                virtual_key: 0,
                modifiers: 0,
                pressed: true,
            })
        );
    }

    fn software_test_pipeline() -> (Arc<SharedPipeline>, Receiver<MediaFeedback>) {
        let (feedback, receiver) = std::sync::mpsc::channel();
        let shared = Arc::new(SharedPipeline {
            video: Arc::new(VideoQueue::new(VIDEO_QUEUE_CAPACITY)),
            audio: Arc::new(BoundedQueue::new(AUDIO_QUEUE_CAPACITY)),
            output: Arc::new(OutputBuffers::new()),
            feedback,
            paused: AtomicBool::new(false),
            video_desynced: AtomicBool::new(true),
            keyframe_requested: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            recording_tap: RecordingTap::default(),
            replay_tap: crate::replay::ReplayTap::default(),
            stream: MediaStreamConfig::default(),
            #[cfg(target_os = "macos")]
            mac_sink: Mutex::new(None),
            #[cfg(target_os = "macos")]
            mac_software_fallback: AtomicBool::new(true),
            #[cfg(target_os = "windows")]
            windows_bridge: Arc::new(WindowsBridge::new()),
            #[cfg(target_os = "linux")]
            linux_session: Mutex::new(None),
            #[cfg(target_os = "linux")]
            linux_software_fallback: Arc::new(AtomicBool::new(true)),
            #[cfg(target_os = "linux")]
            linux_video_mid: Mutex::new(String::new()),
            #[cfg(target_os = "linux")]
            linux_codec: MediaVideoCodec::H264,
        });
        (shared, receiver)
    }

    #[test]
    fn software_handoff_decodes_the_pending_h264_keyframe() {
        let width = 32;
        let height = 32;
        let rgb = vec![96_u8; width * height * 3];
        let yuv = YUVBuffer::from_rgb_source(RgbSliceU8::new(&rgb, (width, height)));
        let mut encoder = Encoder::new().expect("encoder");
        let encoded: Arc<[u8]> = encoder.encode(&yuv).expect("encode").to_vec().into();
        let (shared, _receiver) = software_test_pipeline();
        let output = Arc::clone(&shared.output);
        shared.video.close();
        run_video_decoder_from(
            shared,
            H264Decoder::new().expect("decoder"),
            Some(EncodedFrame {
                mid: "video".to_owned(),
                codec: MediaCodec::H264,
                data: encoded,
                frame_index: Some(1),
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
    fn software_worker_recovers_an_idr_without_repeated_parameter_sets() {
        let rgb = vec![96_u8; 32 * 32 * 3];
        let yuv = YUVBuffer::from_rgb_source(RgbSliceU8::new(&rgb, (32, 32)));
        let mut encoder = Encoder::new().unwrap();
        let initial = encoder.encode(&yuv).unwrap().to_vec();
        encoder.force_intra_frame();
        let recovery = encoder.encode(&yuv).unwrap().to_vec();
        let idr_only: Vec<u8> = openh264::nal_units(&recovery)
            .filter(|nal| nal.get(3).is_some_and(|header| header & 0x1f == 5))
            .flatten()
            .copied()
            .collect();
        let mut decoder = H264Decoder::new().unwrap();
        assert!(decoder.decode(&initial).unwrap().is_some());
        assert!(decoder.decode(&[0, 0, 1, 0x67, 0xff]).is_err());
        let (shared, feedback) = software_test_pipeline();
        let output = Arc::clone(&shared.output);
        shared.video.close();
        run_video_decoder_from(
            shared,
            decoder,
            Some(EncodedFrame {
                mid: "video".to_owned(),
                codec: MediaCodec::H264,
                data: idr_only.into(),
                frame_index: Some(71),
                timestamp: 4500,
                clock_rate_hz: 90_000,
                keyframe: true,
                contiguous: false,
            }),
        );
        let decoded = output.take_video().expect("recovered IDR");
        assert_eq!((decoded.width, decoded.height), (32, 32));
        assert!(matches!(
            feedback.try_recv(),
            Ok(MediaFeedback::VideoFrameAccepted {
                frame_index: Some(71),
                timestamp: 4500,
                keyframe: true,
                ..
            })
        ));
        assert!(feedback.try_recv().is_err());
    }

    #[test]
    fn software_worker_does_not_decode_a_discontinuous_delta_frame() {
        let rgb = vec![96_u8; 32 * 32 * 3];
        let yuv = YUVBuffer::from_rgb_source(RgbSliceU8::new(&rgb, (32, 32)));
        let mut encoder = Encoder::new().unwrap();
        let initial = encoder.encode(&yuv).unwrap().to_vec();
        let delta = encoder.encode(&yuv).unwrap().to_vec();
        assert!(!delta.is_empty());
        assert!(
            !openh264::nal_units(&delta)
                .any(|nal| nal.get(3).is_some_and(|header| header & 0x1f == 5))
        );
        let mut decoder = H264Decoder::new().unwrap();
        assert!(decoder.decode(&initial).unwrap().is_some());
        let (shared, feedback) = software_test_pipeline();
        shared.video_desynced.store(false, Ordering::Release);
        shared.video.close();
        run_video_decoder_from(
            Arc::clone(&shared),
            decoder,
            Some(EncodedFrame {
                mid: "video".to_owned(),
                codec: MediaCodec::H264,
                data: delta.into(),
                frame_index: Some(72),
                timestamp: 5250,
                clock_rate_hz: 90_000,
                keyframe: false,
                contiguous: false,
            }),
        );
        assert!(shared.video_desynced.load(Ordering::Acquire));
        assert!(shared.output.take_video().is_none());
        assert!(feedback.try_recv().is_err());
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

        let concealed = decoder
            .decode_packet_loss()
            .expect("packet-loss concealment");
        assert_eq!(concealed.len(), input.len());
        assert!(concealed.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn converts_rtp_video_timestamps_to_media_foundation_time() {
        assert_eq!(media_timestamp_100ns(0, 90_000), 0);
        assert_eq!(media_timestamp_100ns(90_000, 90_000), 10_000_000);
        assert_eq!(media_timestamp_100ns(45_000, 90_000), 5_000_000);
        assert_eq!(media_timestamp_100ns(90_000, 0), 0);
    }

    #[test]
    fn adaptive_sample_clock_uses_the_negotiated_fps_only_as_a_ceiling() {
        let mut clock = AdaptiveSampleClock::new(120);
        assert_eq!(clock.observe(0), 83_333);
        assert_eq!(clock.observe(83_333), 83_333);
        assert_eq!(clock.observe(250_000), 166_667);
        assert_eq!(clock.observe(583_334), 333_334);
    }

    #[test]
    fn adaptive_sample_clock_recovers_from_a_repeated_timestamp() {
        let mut clock = AdaptiveSampleClock::new(120);
        assert_eq!(clock.observe(0), 83_333);
        assert_eq!(clock.observe(83_333), 83_333);
        assert_eq!(clock.observe(83_333), 83_333);
    }

    #[test]
    fn paused_and_stopped_sessions_reject_frames() {
        let (feedback, _receiver) = std::sync::mpsc::channel();
        let (commands, _host) = std::sync::mpsc::channel();
        let session = MediaSession::spawn(
            Arc::new(OutputBuffers::new()),
            feedback,
            commands,
            false,
            MediaStreamConfig::default(),
            #[cfg(target_os = "windows")]
            Arc::new(WindowsBridge::new()),
            #[cfg(target_os = "linux")]
            LinuxVideoSelection {
                path: LinuxVideoPath::Software,
                use_vulkan_output: false,
                fallback_reason: None,
            },
            #[cfg(target_os = "linux")]
            Arc::new(AtomicBool::new(true)),
        )
        .expect("session");
        let sink = session.sink();
        session.set_paused(true);
        assert_eq!(
            sink.push(EncodedFrame {
                mid: "video".to_owned(),
                codec: MediaCodec::H264,
                data: Arc::from([]),
                frame_index: Some(1),
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
                frame_index: Some(2),
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
        let session = MediaSession::spawn(
            Arc::new(OutputBuffers::new()),
            feedback,
            commands,
            false,
            MediaStreamConfig::default(),
            #[cfg(target_os = "windows")]
            Arc::new(WindowsBridge::new()),
            #[cfg(target_os = "linux")]
            LinuxVideoSelection {
                path: LinuxVideoPath::Software,
                use_vulkan_output: false,
                fallback_reason: None,
            },
            #[cfg(target_os = "linux")]
            Arc::new(AtomicBool::new(true)),
        )
        .expect("session");
        assert_eq!(
            session.sink().push(EncodedFrame {
                mid: "video".to_owned(),
                codec: MediaCodec::H264,
                data: Arc::from([0_u8, 0, 0, 1, 1]),
                frame_index: Some(1),
                timestamp: 0,
                clock_rate_hz: 90_000,
                keyframe: false,
                contiguous: true,
            }),
            PushOutcome::DroppedOldest
        );
        assert!(matches!(
            receiver.recv_timeout(std::time::Duration::from_secs(1)),
            Ok(MediaFeedback::RequestKeyframe { mid, .. }) if mid == "video"
        ));
        session.stop();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn converts_rtp_timestamps_to_microseconds_without_overflow() {
        assert_eq!(media_timestamp_us(180_000, 90_000), 2_000_000);
        assert_eq!(media_timestamp_us(48_000, 48_000), 1_000_000);
        assert_eq!(media_timestamp_us(u64::MAX, 90_000), u64::MAX / 90_000);
        assert_eq!(media_timestamp_us(42, 0), 0);
    }
}
