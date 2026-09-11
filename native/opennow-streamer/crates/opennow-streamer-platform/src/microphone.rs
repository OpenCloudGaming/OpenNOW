use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use sdl2::audio::AudioCallback;
use sdl2::sys::{self, SDL_AudioStatus};

pub const MICROPHONE_SAMPLE_RATE: u32 = 48_000;
pub const MICROPHONE_FRAME_SAMPLES: usize = 960;
const QUEUE_CAPACITY: usize = 5;
const MAX_FRAME_AGE: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedMicrophoneFrame {
    pub opus: Vec<u8>,
    pub timestamp: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicrophoneStatus {
    pub enabled: bool,
    pub error: Option<String>,
    pub dropped_pcm_frames: u64,
    pub dropped_encoded_frames: u64,
}

#[derive(Debug)]
struct PcmFrame {
    samples: [i16; MICROPHONE_FRAME_SAMPLES],
    timestamp: u32,
    captured_at: Instant,
}

#[derive(Debug)]
struct EncodedPacket {
    frame: EncodedMicrophoneFrame,
    captured_at: Instant,
}

#[derive(Debug)]
struct QueueState {
    pcm: VecDeque<PcmFrame>,
    encoded: VecDeque<EncodedPacket>,
    stopped: bool,
    error: Option<String>,
    last_pcm: Instant,
}

#[derive(Debug)]
pub(crate) struct MicrophoneShared {
    state: Mutex<QueueState>,
    ready: Condvar,
    dropped_pcm: AtomicU64,
    dropped_encoded: AtomicU64,
    stopped: AtomicBool,
    device: Mutex<Option<u32>>,
}

impl MicrophoneShared {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(QueueState {
                pcm: VecDeque::with_capacity(QUEUE_CAPACITY),
                encoded: VecDeque::with_capacity(QUEUE_CAPACITY),
                stopped: false,
                error: None,
                last_pcm: Instant::now(),
            }),
            ready: Condvar::new(),
            dropped_pcm: AtomicU64::new(0),
            dropped_encoded: AtomicU64::new(0),
            stopped: AtomicBool::new(false),
            device: Mutex::new(None),
        })
    }

    pub(crate) fn close(&self, error: Option<String>) {
        self.stopped.store(true, Ordering::Release);
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if !state.stopped {
            state.stopped = true;
            if error.is_some() {
                state.error = error;
            }
            state.pcm.clear();
            state.encoded.clear();
            self.ready.notify_all();
        }
        drop(state);
        self.pause_device();
    }

    pub(crate) fn stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }

    fn pause_device(&self) {
        let device = self
            .device
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(id) = *device {
            unsafe { sys::SDL_PauseAudioDevice(id, 1) };
        }
    }

    fn device_status(&self) -> Option<SDL_AudioStatus> {
        let device = self
            .device
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        device.map(|id| unsafe { sys::SDL_GetAudioDeviceStatus(id) })
    }

    fn poll_device(&self) {
        if self.stopped() {
            return;
        }
        let failed = self
            .device_status()
            .is_some_and(|status| status != SDL_AudioStatus::SDL_AUDIO_PLAYING);
        if failed {
            self.close(Some(
                "microphone capture device stopped or disconnected".to_owned(),
            ));
        }
    }

    fn push_pcm(
        &self,
        samples: &[i16; MICROPHONE_FRAME_SAMPLES],
        timestamp: u32,
        captured_at: Instant,
    ) {
        let Ok(mut state) = self.state.try_lock() else {
            self.dropped_pcm.fetch_add(1, Ordering::Relaxed);
            return;
        };
        if state.stopped {
            return;
        }
        state.last_pcm = Instant::now();
        if state.pcm.len() == QUEUE_CAPACITY {
            state.pcm.pop_front();
            self.dropped_pcm.fetch_add(1, Ordering::Relaxed);
        }
        state.pcm.push_back(PcmFrame {
            samples: *samples,
            timestamp,
            captured_at,
        });
        self.ready.notify_one();
    }

    fn pop_pcm(&self) -> Option<PcmFrame> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        loop {
            if state.stopped {
                return None;
            }
            if let Some(frame) = state.pcm.pop_front() {
                if frame.captured_at.elapsed() > MAX_FRAME_AGE {
                    self.dropped_pcm.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                return Some(frame);
            }
            let (next, _) = self
                .ready
                .wait_timeout(state, Duration::from_secs(1))
                .unwrap_or_else(|error| error.into_inner());
            state = next;
            if !state.stopped && state.last_pcm.elapsed() >= Duration::from_secs(2) {
                drop(state);
                self.close(Some("microphone capture stopped delivering PCM".to_owned()));
                return None;
            }
        }
    }

    fn push_encoded(&self, frame: EncodedMicrophoneFrame, captured_at: Instant) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.stopped {
            return;
        }
        if captured_at.elapsed() > MAX_FRAME_AGE {
            self.dropped_encoded.fetch_add(1, Ordering::Relaxed);
            return;
        }
        if state.encoded.len() == QUEUE_CAPACITY {
            state.encoded.pop_front();
            self.dropped_encoded.fetch_add(1, Ordering::Relaxed);
        }
        state
            .encoded
            .push_back(EncodedPacket { frame, captured_at });
    }
}

struct CaptureCallback {
    shared: Arc<MicrophoneShared>,
    samples: [i16; MICROPHONE_FRAME_SAMPLES],
    filled: usize,
    timestamp: u32,
    captured_at: Instant,
}

impl CaptureCallback {
    fn new(shared: Arc<MicrophoneShared>) -> Self {
        Self {
            shared,
            samples: [0; MICROPHONE_FRAME_SAMPLES],
            filled: 0,
            timestamp: 0,
            captured_at: Instant::now(),
        }
    }
}

impl AudioCallback for CaptureCallback {
    type Channel = i16;

    fn callback(&mut self, mut input: &mut [i16]) {
        if self.shared.stopped() {
            self.filled = 0;
            self.samples.fill(0);
            return;
        }
        while !input.is_empty() {
            if self.filled == 0 {
                self.captured_at = Instant::now();
            }
            let count = input.len().min(MICROPHONE_FRAME_SAMPLES - self.filled);
            self.samples[self.filled..self.filled + count].copy_from_slice(&input[..count]);
            self.filled += count;
            input = &mut input[count..];
            if self.filled == MICROPHONE_FRAME_SAMPLES {
                self.shared
                    .push_pcm(&self.samples, self.timestamp, self.captured_at);
                self.timestamp = self.timestamp.wrapping_add(MICROPHONE_FRAME_SAMPLES as u32);
                self.filled = 0;
            }
        }
    }
}

fn encoder() -> Result<opus::Encoder, String> {
    let mut encoder = opus::Encoder::new(
        MICROPHONE_SAMPLE_RATE,
        opus::Channels::Mono,
        opus::Application::Voip,
    )
    .map_err(|error| format!("microphone Opus encoder initialization failed: {error}"))?;
    encoder
        .set_bitrate(opus::Bitrate::Bits(32_000))
        .map_err(|error| format!("microphone Opus bitrate configuration failed: {error}"))?;
    Ok(encoder)
}

fn run_encoder(shared: Arc<MicrophoneShared>, mut encoder: opus::Encoder) {
    let mut packet = [0_u8; 1275];
    while let Some(frame) = shared.pop_pcm() {
        match encoder.encode(&frame.samples, &mut packet) {
            Ok(size) => shared.push_encoded(
                EncodedMicrophoneFrame {
                    opus: packet[..size].to_vec(),
                    timestamp: frame.timestamp,
                },
                frame.captured_at,
            ),
            Err(error) => {
                shared.close(Some(format!("microphone Opus encoding failed: {error}")));
                break;
            }
        }
    }
}

struct CaptureDevice {
    shared: Arc<MicrophoneShared>,
    _callback: Box<UnsafeCell<CaptureCallback>>,
    _audio: sdl2::AudioSubsystem,
}

unsafe extern "C" fn capture_callback(
    userdata: *mut std::ffi::c_void,
    input: *mut u8,
    length: i32,
) {
    if userdata.is_null() || input.is_null() || length <= 0 || length % 2 != 0 {
        return;
    }
    let callback = unsafe { &mut *userdata.cast::<CaptureCallback>() };
    let samples =
        unsafe { std::slice::from_raw_parts_mut(input.cast::<i16>(), length as usize / 2) };
    callback.callback(samples);
}

impl CaptureDevice {
    fn open(
        audio: sdl2::AudioSubsystem,
        shared: Arc<MicrophoneShared>,
        origin: Instant,
    ) -> Result<Self, String> {
        let callback = Box::new(UnsafeCell::new(CaptureCallback::new(Arc::clone(&shared))));
        let desired = sys::SDL_AudioSpec {
            freq: MICROPHONE_SAMPLE_RATE as i32,
            format: sys::AUDIO_S16SYS as u16,
            channels: 1,
            silence: 0,
            samples: 1024,
            padding: 0,
            size: 0,
            callback: Some(capture_callback),
            userdata: callback.get().cast(),
        };
        let mut obtained = desired;
        let id =
            unsafe { sys::SDL_OpenAudioDevice(std::ptr::null(), 1, &desired, &mut obtained, 0) };
        if id == 0 {
            return Err(format!(
                "microphone capture could not open the default device: {}",
                sdl2::get_error()
            ));
        }
        *shared
            .device
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(id);
        unsafe {
            (*callback.get()).timestamp = (origin.elapsed().as_micros()
                * u128::from(MICROPHONE_SAMPLE_RATE)
                / 1_000_000) as u32;
        }
        let device = Self {
            shared,
            _callback: callback,
            _audio: audio,
        };
        if obtained.freq != MICROPHONE_SAMPLE_RATE as i32
            || obtained.channels != 1
            || obtained.format != sys::AUDIO_S16SYS as u16
        {
            return Err(
                "microphone device did not provide signed 16-bit mono 48 kHz PCM".to_owned(),
            );
        }
        Ok(device)
    }

    fn resume(&self) -> Result<(), String> {
        let device = self
            .shared
            .device
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self.shared.stopped() {
            return Err("microphone capture request was cancelled".to_owned());
        }
        let id = device.ok_or_else(|| "microphone capture device was closed".to_owned())?;
        unsafe { sys::SDL_PauseAudioDevice(id, 0) };
        Ok(())
    }

    fn pause(&self) {
        self.shared.pause_device();
    }

    #[cfg(test)]
    fn status(&self) -> SDL_AudioStatus {
        self.shared
            .device_status()
            .unwrap_or(SDL_AudioStatus::SDL_AUDIO_STOPPED)
    }
}

impl Drop for CaptureDevice {
    fn drop(&mut self) {
        let mut device = self
            .shared
            .device
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(id) = device.take() {
            unsafe { sys::SDL_CloseAudioDevice(id) };
        }
    }
}

pub(crate) struct MicrophoneCapture {
    device: CaptureDevice,
    worker: Option<JoinHandle<()>>,
    shared: Arc<MicrophoneShared>,
    _sdl: sdl2::Sdl,
}

impl MicrophoneCapture {
    pub(crate) fn start(
        device_id: &str,
        shared: Arc<MicrophoneShared>,
        origin: Instant,
    ) -> Result<Self, String> {
        if shared.stopped() {
            return Err("microphone capture request was cancelled".to_owned());
        }
        if !device_id.is_empty() {
            return Err(
                "microphone device selection is not supported; select the default input device"
                    .to_owned(),
            );
        }
        let encoder = encoder()?;
        let sdl = sdl2::init()
            .map_err(|error| format!("microphone SDL initialization failed: {error}"))?;
        let audio = sdl
            .audio()
            .map_err(|error| format!("microphone audio initialization failed: {error}"))?;
        let device = CaptureDevice::open(audio, Arc::clone(&shared), origin)?;
        shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .last_pcm = Instant::now();
        let worker_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("opennow-microphone-opus".to_owned())
            .spawn(move || run_encoder(worker_shared, encoder))
            .map_err(|error| format!("microphone encoder worker could not start: {error}"))?;
        let mut capture = Self {
            device,
            worker: Some(worker),
            shared: Arc::clone(&shared),
            _sdl: sdl,
        };
        if shared.stopped() {
            return Err("microphone capture request was cancelled".to_owned());
        }
        capture.device.resume()?;
        capture.poll();
        if let Some(error) = shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .error
            .clone()
        {
            return Err(error);
        }
        Ok(capture)
    }

    pub(crate) fn poll(&mut self) {
        self.shared.poll_device();
        if self.shared.stopped() {
            return;
        }
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            self.shared.close(Some(
                "microphone encoder worker stopped unexpectedly".to_owned(),
            ));
        }
    }

    pub(crate) fn stopped(&self) -> bool {
        self.shared.stopped()
    }
}

impl Drop for MicrophoneCapture {
    fn drop(&mut self) {
        self.shared.close(None);
        self.device.pause();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub struct MicrophoneSession {
    shared: Arc<MicrophoneShared>,
}

pub struct MicrophoneReceiver {
    shared: Arc<MicrophoneShared>,
}

impl MicrophoneReceiver {
    #[cfg(test)]
    pub(crate) fn capture_device_open(&self) -> bool {
        self.shared.device_status().is_some()
    }

    #[cfg(test)]
    pub(crate) fn pause_capture_device(&self) {
        self.shared.pause_device();
    }

    pub fn try_recv(&self) -> Result<Option<EncodedMicrophoneFrame>, String> {
        self.shared.poll_device();
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(error) = &state.error {
            return Err(error.clone());
        }
        while let Some(packet) = state.encoded.pop_front() {
            if packet.captured_at.elapsed() > MAX_FRAME_AGE {
                self.shared.dropped_encoded.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            return Ok(Some(packet.frame));
        }
        Ok(None)
    }

    pub fn status(&self) -> MicrophoneStatus {
        self.shared.poll_device();
        let state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        MicrophoneStatus {
            enabled: !state.stopped,
            error: state.error.clone(),
            dropped_pcm_frames: self.shared.dropped_pcm.load(Ordering::Relaxed),
            dropped_encoded_frames: self.shared.dropped_encoded.load(Ordering::Relaxed),
        }
    }

    pub fn stop(&self) {
        self.shared.close(None);
    }
}

impl MicrophoneSession {
    pub(crate) fn from_shared(shared: Arc<MicrophoneShared>) -> Self {
        Self { shared }
    }

    pub fn receiver(&self) -> MicrophoneReceiver {
        MicrophoneReceiver {
            shared: Arc::clone(&self.shared),
        }
    }

    pub fn try_recv(&mut self) -> Result<Option<EncodedMicrophoneFrame>, String> {
        self.receiver().try_recv()
    }

    pub fn status(&mut self) -> MicrophoneStatus {
        self.receiver().status()
    }

    pub fn stop(&mut self) {
        self.shared.close(None);
    }
}

impl Drop for MicrophoneSession {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct LocalMicrophoneSession {
        session: MicrophoneSession,
        capture: Option<MicrophoneCapture>,
    }

    impl std::ops::Deref for LocalMicrophoneSession {
        type Target = MicrophoneSession;

        fn deref(&self) -> &Self::Target {
            &self.session
        }
    }

    impl std::ops::DerefMut for LocalMicrophoneSession {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.session
        }
    }

    impl LocalMicrophoneSession {
        fn stop(&mut self) {
            self.session.stop();
            self.capture = None;
        }
    }

    impl MicrophoneSession {
        fn start_local(device_id: &str, origin: Instant) -> Result<LocalMicrophoneSession, String> {
            let shared = MicrophoneShared::new();
            let session = Self::from_shared(Arc::clone(&shared));
            let capture = MicrophoneCapture::start(device_id, shared, origin)?;
            Ok(LocalMicrophoneSession {
                session,
                capture: Some(capture),
            })
        }
    }

    fn wait_for_packet(receiver: &MicrophoneReceiver) -> EncodedMicrophoneFrame {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(packet) = receiver.try_recv().expect("capture remains healthy") {
                return packet;
            }
            assert!(
                Instant::now() < deadline,
                "encoder did not produce a packet"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn receiver_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MicrophoneReceiver>();
        assert_send_sync::<MicrophoneSession>();
    }

    #[test]
    fn stalled_encoder_discards_expired_pcm_without_rewriting_new_frame_clock() {
        let shared = MicrophoneShared::new();
        shared.push_pcm(&[1; MICROPHONE_FRAME_SAMPLES], 960, Instant::now());
        shared
            .state
            .lock()
            .unwrap()
            .pcm
            .front_mut()
            .unwrap()
            .captured_at = Instant::now() - Duration::from_secs(1);
        shared.push_pcm(&[2; MICROPHONE_FRAME_SAMPLES], 48_000, Instant::now());
        let frame = shared.pop_pcm().unwrap();
        assert_eq!(frame.timestamp, 48_000);
        assert_eq!(shared.dropped_pcm.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn stalled_consumer_never_receives_expired_encoded_audio() {
        let shared = MicrophoneShared::new();
        let session = MicrophoneSession::from_shared(Arc::clone(&shared));
        shared.push_encoded(
            EncodedMicrophoneFrame {
                opus: vec![1],
                timestamp: 960,
            },
            Instant::now(),
        );
        shared
            .state
            .lock()
            .unwrap()
            .encoded
            .front_mut()
            .unwrap()
            .captured_at = Instant::now() - Duration::from_secs(1);
        shared.push_encoded(
            EncodedMicrophoneFrame {
                opus: vec![2],
                timestamp: 48_000,
            },
            Instant::now(),
        );
        let receiver = session.receiver();
        assert_eq!(receiver.try_recv().unwrap().unwrap().timestamp, 48_000);
        assert_eq!(receiver.status().dropped_encoded_frames, 1);
        shared.push_encoded(
            EncodedMicrophoneFrame {
                opus: vec![3],
                timestamp: 999,
            },
            Instant::now() - Duration::from_secs(1),
        );
        assert_eq!(receiver.try_recv(), Ok(None));
        assert_eq!(receiver.status().dropped_encoded_frames, 2);
    }

    #[test]
    fn callback_assembles_exact_frames_and_preserves_sample_clock_wrap() {
        let shared = MicrophoneShared::new();
        let mut callback = CaptureCallback::new(Arc::clone(&shared));
        callback.timestamp = u32::MAX - 479;
        callback.callback(&mut [11; 480]);
        assert!(shared.state.lock().unwrap().pcm.is_empty());
        callback.callback(&mut [22; 1440]);
        let first = shared.pop_pcm().unwrap();
        assert_eq!(first.timestamp, u32::MAX - 479);
        assert_eq!(&first.samples[..480], &[11; 480]);
        assert_eq!(&first.samples[480..], &[22; 480]);
        let second = shared.pop_pcm().unwrap();
        assert_eq!(second.timestamp, 480);
        assert_eq!(second.samples, [22; MICROPHONE_FRAME_SAMPLES]);
    }

    #[test]
    fn partial_frame_retains_capture_age_across_callback_stall() {
        let shared = MicrophoneShared::new();
        let mut callback = CaptureCallback::new(Arc::clone(&shared));
        callback.callback(&mut [1; 480]);
        callback.captured_at = Instant::now() - Duration::from_secs(1);
        callback.callback(&mut [2; 1440]);
        let frame = shared.pop_pcm().unwrap();
        assert_eq!(frame.timestamp, MICROPHONE_FRAME_SAMPLES as u32);
        assert_eq!(frame.samples, [2; MICROPHONE_FRAME_SAMPLES]);
        assert_eq!(shared.dropped_pcm.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn pcm_overflow_discards_oldest_and_preserves_timestamp_gaps() {
        let shared = MicrophoneShared::new();
        let mut callback = CaptureCallback::new(Arc::clone(&shared));
        for value in 0..QUEUE_CAPACITY + 2 {
            callback.callback(&mut [value as i16; MICROPHONE_FRAME_SAMPLES]);
        }
        assert_eq!(shared.state.lock().unwrap().pcm.len(), QUEUE_CAPACITY);
        assert_eq!(shared.dropped_pcm.load(Ordering::Relaxed), 2);
        let frame = shared.pop_pcm().unwrap();
        assert_eq!(frame.timestamp, 2 * MICROPHONE_FRAME_SAMPLES as u32);
        assert_eq!(frame.samples, [2; MICROPHONE_FRAME_SAMPLES]);
    }

    #[test]
    fn callback_drops_instead_of_waiting_on_busy_queue() {
        let shared = MicrophoneShared::new();
        let mut callback = CaptureCallback::new(Arc::clone(&shared));
        let guard = shared.state.lock().unwrap();
        callback.callback(&mut [1; MICROPHONE_FRAME_SAMPLES]);
        drop(guard);
        assert_eq!(shared.dropped_pcm.load(Ordering::Relaxed), 1);
        callback.callback(&mut [2; MICROPHONE_FRAME_SAMPLES]);
        assert_eq!(
            shared.pop_pcm().unwrap().timestamp,
            MICROPHONE_FRAME_SAMPLES as u32
        );
    }

    #[test]
    fn encoded_queue_is_bounded_and_stop_discards_pending_and_in_flight_frames() {
        let shared = MicrophoneShared::new();
        let mut session = MicrophoneSession::from_shared(Arc::clone(&shared));
        for timestamp in 0..QUEUE_CAPACITY as u32 + 2 {
            shared.push_encoded(
                EncodedMicrophoneFrame {
                    opus: vec![1],
                    timestamp,
                },
                Instant::now(),
            );
        }
        assert_eq!(session.status().dropped_encoded_frames, 2);
        assert_eq!(session.try_recv().unwrap().unwrap().timestamp, 2);
        shared.push_pcm(&[1; MICROPHONE_FRAME_SAMPLES], 0, Instant::now());
        session.stop();
        shared.push_encoded(
            EncodedMicrophoneFrame {
                opus: vec![2],
                timestamp: 999,
            },
            Instant::now(),
        );
        shared.push_pcm(&[2; MICROPHONE_FRAME_SAMPLES], 999, Instant::now());
        assert!(!session.status().enabled);
        assert_eq!(session.try_recv(), Ok(None));
        assert!(shared.pop_pcm().is_none());
    }

    #[test]
    fn worker_encodes_decodable_mono_twenty_millisecond_opus() {
        let shared = MicrophoneShared::new();
        let mut session = MicrophoneSession::from_shared(Arc::clone(&shared));
        let receiver = session.receiver();
        let worker_shared = Arc::clone(&shared);
        let worker = thread::spawn(move || run_encoder(worker_shared, encoder().unwrap()));
        shared.push_pcm(&[100; MICROPHONE_FRAME_SAMPLES], 12_345, Instant::now());
        let packet = wait_for_packet(&receiver);
        assert_eq!(packet.timestamp, 12_345);
        let mut decoder = opus::Decoder::new(MICROPHONE_SAMPLE_RATE, opus::Channels::Mono).unwrap();
        let mut pcm = [0; MICROPHONE_FRAME_SAMPLES * 2];
        assert_eq!(
            decoder.decode(&packet.opus, &mut pcm, false).unwrap(),
            MICROPHONE_FRAME_SAMPLES
        );
        session.stop();
        worker.join().unwrap();
        assert_eq!(receiver.try_recv(), Ok(None));
    }

    #[test]
    fn dropping_session_stops_idle_worker_and_receiver() {
        let shared = MicrophoneShared::new();
        let session = MicrophoneSession::from_shared(Arc::clone(&shared));
        let receiver = session.receiver();
        let worker = thread::spawn(move || run_encoder(shared, encoder().unwrap()));
        drop(session);
        worker.join().unwrap();
        assert!(!receiver.status().enabled);
    }

    #[test]
    fn device_failure_disables_and_flushes_without_leaking_into_next_session() {
        let shared = MicrophoneShared::new();
        let mut session = MicrophoneSession::from_shared(Arc::clone(&shared));
        shared.push_encoded(
            EncodedMicrophoneFrame {
                opus: vec![1],
                timestamp: 0,
            },
            Instant::now(),
        );
        shared.close(Some("device disconnected".to_owned()));
        assert_eq!(session.try_recv(), Err("device disconnected".to_owned()));
        assert!(!session.status().enabled);
        session.stop();
        assert_eq!(
            session.status().error.as_deref(),
            Some("device disconnected")
        );
        let mut replacement = MicrophoneSession::from_shared(MicrophoneShared::new());
        assert_eq!(replacement.status().error, None);
        assert_eq!(replacement.try_recv(), Ok(None));
    }

    #[test]
    fn missing_pcm_reports_failure_instead_of_claiming_enabled_forever() {
        let shared = MicrophoneShared::new();
        let session = MicrophoneSession::from_shared(Arc::clone(&shared));
        let receiver = session.receiver();
        shared.state.lock().unwrap().last_pcm = Instant::now() - Duration::from_secs(3);
        let worker = thread::spawn(move || run_encoder(shared, encoder().unwrap()));
        worker.join().unwrap();
        assert!(!receiver.status().enabled);
        assert_eq!(
            receiver.status().error.as_deref(),
            Some("microphone capture stopped delivering PCM")
        );
    }

    #[test]
    fn explicit_device_selection_fails_before_opening_sdl() {
        let error = MicrophoneSession::start_local("unsupported-device-id", Instant::now())
            .err()
            .expect("nondefault device rejected");
        assert!(error.contains("device selection is not supported"));
    }

    #[test]
    fn cancelled_host_request_never_opens_a_device() {
        let shared = MicrophoneShared::new();
        shared.close(None);
        let error = MicrophoneCapture::start("", shared, Instant::now())
            .err()
            .expect("cancelled request rejected");
        assert_eq!(error, "microphone capture request was cancelled");
    }

    #[test]
    #[ignore = "requires SDL_AUDIODRIVER=dummy and a dedicated test process"]
    fn dummy_capture_restarts_with_continuous_clock_and_no_stale_audio() {
        assert_eq!(std::env::var("SDL_AUDIODRIVER").as_deref(), Ok("dummy"));
        let origin = Instant::now();
        let mut session = MicrophoneSession::start_local("", origin).unwrap();
        assert!(session.status().enabled);
        let receiver = session.receiver();
        let first = wait_for_packet(&receiver);
        let stop_receiver = session.receiver();
        thread::spawn(move || stop_receiver.stop()).join().unwrap();
        assert!(
            session.capture.as_ref().unwrap().device.status() == SDL_AudioStatus::SDL_AUDIO_PAUSED
        );
        session.stop();
        assert!(!receiver.status().enabled);
        assert_eq!(receiver.try_recv(), Ok(None));
        thread::sleep(Duration::from_millis(25));
        let mut restarted = MicrophoneSession::start_local("", origin).unwrap();
        receiver.stop();
        assert!(
            restarted.capture.as_ref().unwrap().device.status()
                == SDL_AudioStatus::SDL_AUDIO_PLAYING
        );
        let next = wait_for_packet(&restarted.receiver());
        assert!(next.timestamp.wrapping_sub(first.timestamp) >= MICROPHONE_FRAME_SAMPLES as u32);
        restarted.capture.as_ref().unwrap().device.pause();
        let stopped_receiver = restarted.receiver();
        let status = stopped_receiver.status();
        assert!(!status.enabled);
        assert_eq!(
            status.error.as_deref(),
            Some("microphone capture device stopped or disconnected")
        );
        assert!(stopped_receiver.try_recv().is_err());
        restarted.stop();
        assert!(restarted.capture.is_none());
    }
}
