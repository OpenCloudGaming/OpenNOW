use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_void};
use std::io::{self, Write};
use std::mem;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::ptr::NonNull;
use std::slice;
use std::sync::Arc;

use libloading::Library;

use crate::{Error, Result, Subsystem};

const OPUS_OK: c_int = 0;
const OPUS_MAX_FRAME_MS: usize = 120;
const OPUS_RESET_STATE: c_int = 4028;
const MAX_PLC_MS: usize = 100;
const PLC_CHUNK_TENTHS_MS: [usize; 6] = [600, 400, 200, 100, 50, 25];
const SND_PCM_STREAM_PLAYBACK: c_int = 0;
const SND_PCM_ACCESS_RW_INTERLEAVED: c_int = 3;
const SND_PCM_FORMAT_FLOAT_LE: c_int = 14;
const SND_PCM_NONBLOCK: c_int = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioBackend {
    PipeWire,
    Alsa,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioBackendPreference {
    PipeWireThenAlsa,
    AlsaThenPipeWire,
    PipeWireOnly,
    AlsaOnly,
}

#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub muted: Arc<std::sync::atomic::AtomicBool>,
    pub sample_rate: u32,
    pub channels: u8,
    pub queue_depth: usize,
    pub preference: AudioBackendPreference,
    pub alsa_device: String,
    pub output_device: String,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            muted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            sample_rate: 48_000,
            channels: 2,
            queue_depth: 12,
            preference: AudioBackendPreference::PipeWireThenAlsa,
            alsa_device: "default".to_owned(),
            output_device: String::new(),
        }
    }
}

impl AudioConfig {
    pub fn validate(&self) -> Result<()> {
        if !self.output_device.is_empty() {
            let name = self
                .output_device
                .strip_prefix("pipewire:")
                .or_else(|| self.output_device.strip_prefix("alsa:"));
            if !crate::audio_devices::valid_id(&self.output_device)
                || name.is_none_or(str::is_empty)
            {
                return Err(Error::InvalidFormat(
                    "Audio output device must be a valid pipewire: or alsa: identifier".to_owned(),
                ));
            }
        }
        if !matches!(self.sample_rate, 8_000 | 12_000 | 16_000 | 24_000 | 48_000) {
            return Err(Error::InvalidFormat(format!(
                "Opus sample rate {} is unsupported",
                self.sample_rate
            )));
        }
        if !matches!(self.channels, 1 | 2) {
            return Err(Error::InvalidFormat(
                "Opus audio must be mono or stereo".to_owned(),
            ));
        }
        if self.queue_depth == 0 || self.queue_depth > 256 {
            return Err(Error::InvalidFormat(
                "audio queue depth must be between 1 and 256".to_owned(),
            ));
        }
        if self.alsa_device.trim().is_empty() {
            return Err(Error::InvalidFormat(
                "ALSA device name cannot be empty".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AudioPacket {
    pub data: Arc<[u8]>,
    pub rtp_timestamp: u32,
    pub clock_rate_hz: u32,
    pub ssrc: u32,
}

impl AudioPacket {
    pub fn new(
        data: impl Into<Arc<[u8]>>,
        rtp_timestamp: u32,
        clock_rate_hz: u32,
        ssrc: u32,
    ) -> Result<Self> {
        let data = data.into();
        let packet = Self {
            data,
            rtp_timestamp,
            clock_rate_hz,
            ssrc,
        };
        packet.validate()?;
        Ok(packet)
    }

    pub fn validate(&self) -> Result<()> {
        if self.data.is_empty() {
            return Err(Error::InvalidFormat("Opus packet is empty".to_owned()));
        }
        if self.data.len() > 1275 {
            return Err(Error::InvalidFormat(
                "Opus packet exceeds the maximum packet size".to_owned(),
            ));
        }
        if self.clock_rate_hz == 0 {
            return Err(Error::InvalidFormat(
                "Opus RTP clock rate cannot be zero".to_owned(),
            ));
        }
        Ok(())
    }
}

type OpusDecoderCreate = unsafe extern "C" fn(c_int, c_int, *mut c_int) -> *mut c_void;
type OpusDecodeFloat =
    unsafe extern "C" fn(*mut c_void, *const u8, c_int, *mut f32, c_int, c_int) -> c_int;
type OpusDecoderCtl = unsafe extern "C" fn(*mut c_void, c_int, ...) -> c_int;
type OpusDecoderDestroy = unsafe extern "C" fn(*mut c_void);
type OpusStrError = unsafe extern "C" fn(c_int) -> *const c_char;

pub(crate) struct OpusDecoder {
    _library: Library,
    handle: NonNull<c_void>,
    decode_float: OpusDecodeFloat,
    decoder_ctl: OpusDecoderCtl,
    destroy: OpusDecoderDestroy,
    strerror: OpusStrError,
    sample_rate: u32,
    channels: usize,
    pcm: Vec<f32>,
    last_ssrc: Option<u32>,
    last_timestamp: Option<u32>,
    last_frame_samples_per_channel: usize,
}

impl OpusDecoder {
    pub fn open(config: &AudioConfig) -> Result<Self> {
        config.validate()?;
        unsafe {
            let library = Library::new("libopus.so.0")
                .or_else(|_| Library::new("libopus.so"))
                .map_err(|error| Error::unavailable(Subsystem::Opus, error.to_string()))?;
            let create: OpusDecoderCreate = *library
                .get(b"opus_decoder_create\0")
                .map_err(|error| Error::unavailable(Subsystem::Opus, error.to_string()))?;
            let decode_float: OpusDecodeFloat = *library
                .get(b"opus_decode_float\0")
                .map_err(|error| Error::unavailable(Subsystem::Opus, error.to_string()))?;
            let decoder_ctl: OpusDecoderCtl = *library
                .get(b"opus_decoder_ctl\0")
                .map_err(|error| Error::unavailable(Subsystem::Opus, error.to_string()))?;
            let destroy: OpusDecoderDestroy = *library
                .get(b"opus_decoder_destroy\0")
                .map_err(|error| Error::unavailable(Subsystem::Opus, error.to_string()))?;
            let strerror: OpusStrError = *library
                .get(b"opus_strerror\0")
                .map_err(|error| Error::unavailable(Subsystem::Opus, error.to_string()))?;
            let mut status = 0;
            let handle = NonNull::new(create(
                config.sample_rate as c_int,
                config.channels as c_int,
                &mut status,
            ));
            if status != OPUS_OK || handle.is_none() {
                return Err(Error::backend(
                    Subsystem::Opus,
                    opus_error(strerror, status),
                ));
            }
            let channels = config.channels as usize;
            let handle = handle.ok_or_else(|| {
                Error::backend(Subsystem::Opus, "libopus returned a null decoder")
            })?;
            Ok(Self {
                _library: library,
                handle,
                decode_float,
                decoder_ctl,
                destroy,
                strerror,
                sample_rate: config.sample_rate,
                channels,
                pcm: vec![0.0; config.sample_rate as usize * OPUS_MAX_FRAME_MS / 1000 * channels],
                last_ssrc: None,
                last_timestamp: None,
                last_frame_samples_per_channel: 0,
            })
        }
    }

    pub fn decode<'a>(&'a mut self, packet: &AudioPacket) -> Result<&'a [f32]> {
        if self.source_changed(packet.ssrc) {
            self.reset_decoder_state()?;
        }
        let max_samples_per_channel = self.sample_rate as usize * OPUS_MAX_FRAME_MS / 1000;
        let samples_per_channel = unsafe {
            (self.decode_float)(
                self.handle.as_ptr(),
                packet.data.as_ptr(),
                packet.data.len().min(c_int::MAX as usize) as c_int,
                self.pcm.as_mut_ptr(),
                max_samples_per_channel as c_int,
                0,
            )
        };
        if samples_per_channel < 0 {
            return Err(Error::backend(Subsystem::Opus, unsafe {
                opus_error(self.strerror, samples_per_channel)
            }));
        }
        let samples_per_channel = samples_per_channel as usize;
        if samples_per_channel > 0 {
            self.last_frame_samples_per_channel = samples_per_channel;
        }
        self.last_ssrc = Some(packet.ssrc);
        self.last_timestamp = Some(packet.rtp_timestamp);
        Ok(&self.pcm[..samples_per_channel * self.channels])
    }

    pub fn record_dropped_packet(&mut self, packet: &AudioPacket) {
        let frame_samples = self.last_frame_samples_per_channel;
        if frame_samples == 0 {
            return;
        }
        let frame_ticks =
            (frame_samples as u64) * u64::from(packet.clock_rate_hz) / u64::from(self.sample_rate);
        self.last_ssrc = Some(packet.ssrc);
        self.last_timestamp = Some(packet.rtp_timestamp.wrapping_sub(frame_ticks as u32));
    }

    pub fn conceal_before<'a>(&'a mut self, packet: &AudioPacket) -> Result<&'a [f32]> {
        if self.source_changed(packet.ssrc) {
            self.reset_decoder_state()?;
            return Ok(&self.pcm[..0]);
        }
        let Some(previous) = self.last_timestamp else {
            return Ok(&self.pcm[..0]);
        };
        let span = packet.rtp_timestamp.wrapping_sub(previous);
        if span >= 1 << 31 {
            return Ok(&self.pcm[..0]);
        }
        let frame_samples = self.last_frame_samples_per_channel;
        if frame_samples == 0 {
            return Ok(&self.pcm[..0]);
        }
        let missing_samples = ticks_to_samples(span, packet.clock_rate_hz, self.sample_rate)
            .saturating_sub(frame_samples);
        if missing_samples == 0 {
            return Ok(&self.pcm[..0]);
        }
        let budget_samples = self.sample_rate as usize * MAX_PLC_MS / 1000;
        let capacity = self.pcm.len() / self.channels;
        let mut remaining = missing_samples.min(budget_samples).min(capacity);
        let decode_float = self.decode_float;
        let mut produced = 0;
        while remaining > 0 {
            let chunk = plc_chunk_samples(remaining, self.sample_rate);
            if chunk == 0 {
                break;
            }
            let offset = produced * self.channels;
            let samples_per_channel = unsafe {
                (decode_float)(
                    self.handle.as_ptr(),
                    std::ptr::null(),
                    0,
                    self.pcm[offset..].as_mut_ptr(),
                    chunk as c_int,
                    0,
                )
            };
            if samples_per_channel < 0 {
                return Err(Error::backend(Subsystem::Opus, unsafe {
                    opus_error(self.strerror, samples_per_channel)
                }));
            }
            let samples_per_channel = samples_per_channel as usize;
            if samples_per_channel == 0 {
                break;
            }
            let samples_per_channel = samples_per_channel.min(chunk);
            produced += samples_per_channel;
            remaining = remaining.saturating_sub(samples_per_channel);
        }
        Ok(&self.pcm[..produced * self.channels])
    }

    fn source_changed(&self, ssrc: u32) -> bool {
        self.last_ssrc.is_some_and(|last| last != ssrc)
    }

    fn reset_decoder_state(&mut self) -> Result<()> {
        let status = unsafe { (self.decoder_ctl)(self.handle.as_ptr(), OPUS_RESET_STATE) };
        if status != OPUS_OK {
            return Err(Error::backend(Subsystem::Opus, unsafe {
                opus_error(self.strerror, status)
            }));
        }
        self.last_ssrc = None;
        self.last_timestamp = None;
        self.last_frame_samples_per_channel = 0;
        Ok(())
    }
}

fn ticks_to_samples(ticks: u32, clock_rate_hz: u32, sample_rate: u32) -> usize {
    (u64::from(ticks) * u64::from(sample_rate) / u64::from(clock_rate_hz)) as usize
}

fn plc_chunk_samples(remaining: usize, sample_rate: u32) -> usize {
    PLC_CHUNK_TENTHS_MS
        .iter()
        .map(|tenths| sample_rate as usize * tenths / 10_000)
        .find(|chunk| *chunk > 0 && *chunk <= remaining)
        .unwrap_or(0)
}

impl Drop for OpusDecoder {
    fn drop(&mut self) {
        unsafe { (self.destroy)(self.handle.as_ptr()) }
    }
}

unsafe fn opus_error(strerror: OpusStrError, code: c_int) -> String {
    let pointer = unsafe { strerror(code) };
    if pointer.is_null() {
        return format!("libopus error {code}");
    }
    unsafe { CStr::from_ptr(pointer) }
        .to_string_lossy()
        .into_owned()
}

pub(crate) trait AudioSink {
    fn backend(&self) -> AudioBackend;
    fn write(&mut self, pcm: &[f32], cancelled: &dyn Fn() -> bool) -> Result<()>;
}

pub(crate) fn open_audio_sink(config: &AudioConfig) -> Result<Box<dyn AudioSink + Send>> {
    config.validate()?;
    let preference = if config.output_device.starts_with("pipewire:") {
        AudioBackendPreference::PipeWireOnly
    } else if config.output_device.starts_with("alsa:") {
        AudioBackendPreference::AlsaOnly
    } else {
        config.preference
    };
    let order: &[AudioBackend] = match preference {
        AudioBackendPreference::PipeWireThenAlsa => &[AudioBackend::PipeWire, AudioBackend::Alsa],
        AudioBackendPreference::AlsaThenPipeWire => &[AudioBackend::Alsa, AudioBackend::PipeWire],
        AudioBackendPreference::PipeWireOnly => &[AudioBackend::PipeWire],
        AudioBackendPreference::AlsaOnly => &[AudioBackend::Alsa],
    };
    let mut failures = Vec::new();
    for backend in order {
        let opened: Result<Box<dyn AudioSink + Send>> = match backend {
            AudioBackend::PipeWire => {
                PipeWireSink::open(config).map(|sink| Box::new(sink) as Box<dyn AudioSink + Send>)
            }
            AudioBackend::Alsa => {
                AlsaSink::open(config).map(|sink| Box::new(sink) as Box<dyn AudioSink + Send>)
            }
        };
        match opened {
            Ok(sink) => return Ok(sink),
            Err(error) => failures.push(error.to_string()),
        }
    }
    Err(Error::unavailable(
        Subsystem::Session,
        format!("no requested audio backend opened: {}", failures.join("; ")),
    ))
}

pub(crate) fn open_audio_fallback(
    config: &AudioConfig,
    current: AudioBackend,
) -> Result<Box<dyn AudioSink + Send>> {
    if !config.output_device.is_empty() {
        return Err(Error::unavailable(
            Subsystem::Session,
            "Fixed audio output device forbids fallback",
        ));
    }
    let preference = match (config.preference, current) {
        (AudioBackendPreference::PipeWireThenAlsa, AudioBackend::PipeWire)
        | (AudioBackendPreference::AlsaThenPipeWire, AudioBackend::PipeWire) => {
            AudioBackendPreference::AlsaOnly
        }
        (AudioBackendPreference::PipeWireThenAlsa, AudioBackend::Alsa)
        | (AudioBackendPreference::AlsaThenPipeWire, AudioBackend::Alsa) => {
            AudioBackendPreference::PipeWireOnly
        }
        _ => {
            return Err(Error::unavailable(
                Subsystem::Session,
                "audio preference forbids fallback",
            ));
        }
    };
    open_audio_sink(&AudioConfig {
        preference,
        ..config.clone()
    })
}

pub(crate) fn probe_audio_backend(backend: AudioBackend) -> std::result::Result<String, String> {
    let config = AudioConfig {
        preference: match backend {
            AudioBackend::PipeWire => AudioBackendPreference::PipeWireOnly,
            AudioBackend::Alsa => AudioBackendPreference::AlsaOnly,
        },
        ..AudioConfig::default()
    };
    open_audio_sink(&config)
        .map(|sink| format!("{:?} opened at 48kHz stereo", sink.backend()))
        .map_err(|error| error.to_string())
}

type SndPcmOpen = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
type SndPcmSetParams =
    unsafe extern "C" fn(*mut c_void, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int;
type SndPcmWriteI = unsafe extern "C" fn(*mut c_void, *const c_void, libc::c_ulong) -> libc::c_long;
type SndPcmRecover = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;
type SndPcmDrop = unsafe extern "C" fn(*mut c_void) -> c_int;
type SndPcmClose = unsafe extern "C" fn(*mut c_void) -> c_int;
type SndStrError = unsafe extern "C" fn(c_int) -> *const c_char;

struct AlsaSink {
    muted: Arc<std::sync::atomic::AtomicBool>,
    silence: Vec<f32>,
    _library: Library,
    handle: NonNull<c_void>,
    writei: SndPcmWriteI,
    recover: SndPcmRecover,
    drop_pcm: SndPcmDrop,
    close: SndPcmClose,
    strerror: SndStrError,
    channels: usize,
}

unsafe impl Send for AlsaSink {}

impl AlsaSink {
    fn open(config: &AudioConfig) -> Result<Self> {
        let device = if let Some(name) = config.output_device.strip_prefix("alsa:") {
            crate::audio_devices::require_device(
                &config.output_device,
                &crate::audio_devices::alsa_devices()?,
            )?;
            name
        } else {
            &config.alsa_device
        };
        unsafe {
            let library = Library::new("libasound.so.2")
                .or_else(|_| Library::new("libasound.so"))
                .map_err(|error| Error::unavailable(Subsystem::Alsa, error.to_string()))?;
            let open: SndPcmOpen = *load_symbol(&library, b"snd_pcm_open\0")?;
            let set_params: SndPcmSetParams = *load_symbol(&library, b"snd_pcm_set_params\0")?;
            let writei: SndPcmWriteI = *load_symbol(&library, b"snd_pcm_writei\0")?;
            let recover: SndPcmRecover = *load_symbol(&library, b"snd_pcm_recover\0")?;
            let drop_pcm: SndPcmDrop = *load_symbol(&library, b"snd_pcm_drop\0")?;
            let close: SndPcmClose = *load_symbol(&library, b"snd_pcm_close\0")?;
            let strerror: SndStrError = *load_symbol(&library, b"snd_strerror\0")?;
            let name = CString::new(device).map_err(|_| {
                Error::InvalidFormat("ALSA device contains an embedded NUL".to_owned())
            })?;
            let mut raw = std::ptr::null_mut();
            let status = open(
                &mut raw,
                name.as_ptr(),
                SND_PCM_STREAM_PLAYBACK,
                SND_PCM_NONBLOCK,
            );
            if status < 0 {
                return Err(Error::unavailable(
                    Subsystem::Alsa,
                    alsa_error(strerror, status),
                ));
            }
            let handle = NonNull::new(raw).ok_or_else(|| {
                Error::backend(Subsystem::Alsa, "ALSA returned a null PCM handle")
            })?;
            let status = set_params(
                handle.as_ptr(),
                SND_PCM_FORMAT_FLOAT_LE,
                SND_PCM_ACCESS_RW_INTERLEAVED,
                config.channels as c_uint,
                config.sample_rate,
                1,
                50_000,
            );
            if status < 0 {
                close(handle.as_ptr());
                return Err(Error::unavailable(
                    Subsystem::Alsa,
                    alsa_error(strerror, status),
                ));
            }
            Ok(Self {
                _library: library,
                muted: Arc::clone(&config.muted),
                silence: vec![
                    0.0;
                    config.sample_rate as usize
                        * config.channels as usize
                        * OPUS_MAX_FRAME_MS
                        / 1000
                ],
                handle,
                writei,
                recover,
                drop_pcm,
                close,
                strerror,
                channels: config.channels as usize,
            })
        }
    }
}

impl AudioSink for AlsaSink {
    fn backend(&self) -> AudioBackend {
        AudioBackend::Alsa
    }

    fn write(&mut self, pcm: &[f32], cancelled: &dyn Fn() -> bool) -> Result<()> {
        let mut offset = 0;
        while offset < pcm.len() {
            if cancelled() {
                return Err(Error::QueueClosed);
            }
            let samples = &pcm[offset..pcm.len().min(offset + self.silence.len())];
            let samples = if self.muted.load(std::sync::atomic::Ordering::Acquire) {
                &self.silence[..samples.len()]
            } else {
                samples
            };
            let frames = samples.len() / self.channels;
            let written = unsafe {
                (self.writei)(
                    self.handle.as_ptr(),
                    samples.as_ptr().cast(),
                    frames as libc::c_ulong,
                )
            };
            if written < 0 {
                if written as c_int == -libc::EAGAIN {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                    continue;
                }
                let recovered =
                    unsafe { (self.recover)(self.handle.as_ptr(), written as c_int, 1) };
                if recovered < 0 {
                    return Err(Error::DeviceLost {
                        subsystem: Subsystem::Alsa,
                        reason: unsafe { alsa_error(self.strerror, recovered) },
                    });
                }
                continue;
            }
            if written == 0 {
                return Err(Error::backend(Subsystem::Alsa, "zero-length PCM write"));
            }
            offset += written as usize * self.channels;
        }
        Ok(())
    }
}

impl Drop for AlsaSink {
    fn drop(&mut self) {
        unsafe {
            (self.drop_pcm)(self.handle.as_ptr());
            (self.close)(self.handle.as_ptr());
        }
    }
}

unsafe fn load_symbol<'a, T>(
    library: &'a Library,
    name: &[u8],
) -> Result<libloading::Symbol<'a, T>> {
    unsafe { library.get(name) }
        .map_err(|error| Error::unavailable(Subsystem::Alsa, error.to_string()))
}

unsafe fn alsa_error(strerror: SndStrError, code: c_int) -> String {
    let pointer = unsafe { strerror(code) };
    if pointer.is_null() {
        return format!("ALSA error {code}");
    }
    unsafe { CStr::from_ptr(pointer) }
        .to_string_lossy()
        .into_owned()
}

struct PipeWireSink {
    muted: Arc<std::sync::atomic::AtomicBool>,
    silence: Vec<u8>,
    child: Child,
    stdin: ChildStdin,
}

impl PipeWireSink {
    fn open(config: &AudioConfig) -> Result<Self> {
        let target = config.output_device.strip_prefix("pipewire:");
        if target.is_some() {
            crate::audio_devices::require_device(
                &config.output_device,
                &crate::audio_devices::pipewire_devices()?,
            )?;
        }
        pipewire_socket().ok_or_else(|| {
            Error::unavailable(
                Subsystem::PipeWire,
                "the PipeWire socket was not found under XDG_RUNTIME_DIR",
            )
        })?;
        let executable = find_in_path("pw-cat").ok_or_else(|| {
            Error::unavailable(Subsystem::PipeWire, "pw-cat was not found in PATH")
        })?;
        let help = crate::audio_devices::bounded_command_output(
            Command::new(&executable).arg("--help"),
            std::time::Duration::from_millis(250),
        )?;
        let mut command = Command::new(executable);
        if String::from_utf8_lossy(&help).contains("--raw") {
            command.arg("--raw");
        }
        if let Some(target) = target {
            command.args([
                "--target",
                target,
                "--properties",
                "{\"node.dont-fallback\":true,\"node.dont-reconnect\":true}",
            ]);
        }
        let mut child = command
            .args([
                "--playback",
                "--format",
                "f32",
                "--rate",
                &config.sample_rate.to_string(),
                "--channels",
                &config.channels.to_string(),
                "-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| Error::io(Subsystem::PipeWire, error))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::backend(Subsystem::PipeWire, "pw-cat stdin was not created"))?;
        let descriptor = std::os::fd::AsRawFd::as_raw_fd(&stdin);
        let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
        if flags < 0
            || unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
        {
            let error = io::Error::last_os_error();
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::io(Subsystem::PipeWire, error));
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
        if let Some(status) = child
            .try_wait()
            .map_err(|error| Error::io(Subsystem::PipeWire, error))?
        {
            return Err(Error::unavailable(
                Subsystem::PipeWire,
                format!("pw-cat exited during startup with {status}"),
            ));
        }
        Ok(Self {
            child,
            stdin,
            muted: Arc::clone(&config.muted),
            silence: vec![
                0;
                config.sample_rate as usize * config.channels as usize * OPUS_MAX_FRAME_MS
                    / 1000
                    * mem::size_of::<f32>()
            ],
        })
    }
}

impl AudioSink for PipeWireSink {
    fn backend(&self) -> AudioBackend {
        AudioBackend::PipeWire
    }

    fn write(&mut self, pcm: &[f32], cancelled: &dyn Fn() -> bool) -> Result<()> {
        write_pipewire_pcm(&mut self.stdin, pcm, &self.silence, &self.muted, cancelled)
    }
}

fn write_pipewire_pcm(
    writer: &mut impl Write,
    pcm: &[f32],
    silence: &[u8],
    muted: &std::sync::atomic::AtomicBool,
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    let bytes = unsafe { slice::from_raw_parts(pcm.as_ptr().cast::<u8>(), mem::size_of_val(pcm)) };
    let mut offset = 0;
    let mut sample_muted = false;
    while offset < bytes.len() {
        if cancelled() {
            return Err(Error::QueueClosed);
        }
        let partial_sample_bytes = offset % mem::size_of::<f32>();
        let end = if partial_sample_bytes == 0 {
            sample_muted = muted.load(std::sync::atomic::Ordering::Acquire);
            bytes.len().min(offset + silence.len())
        } else {
            offset + mem::size_of::<f32>() - partial_sample_bytes
        };
        let chunk = &bytes[offset..end];
        let chunk = if sample_muted {
            &silence[..chunk.len()]
        } else {
            chunk
        };
        match writer.write(chunk) {
            Ok(0) => {
                return Err(Error::DeviceLost {
                    subsystem: Subsystem::PipeWire,
                    reason: "pw-cat closed its input".to_owned(),
                });
            }
            Ok(written) => offset += written,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Err(error) => {
                return Err(Error::DeviceLost {
                    subsystem: Subsystem::PipeWire,
                    reason: error.to_string(),
                });
            }
        }
    }
    Ok(())
}

impl Drop for PipeWireSink {
    fn drop(&mut self) {
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn pipewire_socket() -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let socket = PathBuf::from(runtime).join("pipewire-0");
    socket.exists().then_some(socket)
}

fn find_in_path(executable: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|path| path.join(executable))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opus::{Application, Channels, Encoder as OpusEncoder};

    fn encoded_frame(
        encoder: &mut OpusEncoder,
        samples_per_channel: usize,
        frequency: f32,
    ) -> Arc<[u8]> {
        let input: Vec<f32> = (0..samples_per_channel * 2)
            .map(|sample| {
                let seconds = (sample / 2) as f32 / 48_000.0;
                (seconds * frequency * std::f32::consts::TAU).sin() * 0.25
            })
            .collect();
        let mut buffer = vec![0_u8; 4_000];
        let len = encoder
            .encode_float(&input, &mut buffer)
            .expect("encode frame");
        Arc::from(&buffer[..len])
    }

    fn audio_packet(data: Arc<[u8]>, rtp_timestamp: u32) -> AudioPacket {
        AudioPacket::new(data, rtp_timestamp, 48_000, 7).expect("audio packet")
    }

    fn decoder_and_encoder() -> (OpusDecoder, OpusEncoder) {
        let decoder = OpusDecoder::open(&AudioConfig::default()).expect("decoder");
        let encoder =
            OpusEncoder::new(48_000, Channels::Stereo, Application::Audio).expect("encoder");
        (decoder, encoder)
    }

    #[test]
    fn tick_spans_scale_by_the_clock_and_decoder_rates() {
        assert_eq!(ticks_to_samples(960, 48_000, 48_000), 960);
        assert_eq!(ticks_to_samples(960, 48_000, 24_000), 480);
        assert_eq!(ticks_to_samples(1_440, 48_000, 24_000), 720);
        assert_eq!(ticks_to_samples(0, 48_000, 48_000), 0);
    }

    #[test]
    fn packet_rejects_an_empty_payload_and_a_zero_clock_rate() {
        assert!(AudioPacket::new(Arc::<[u8]>::from([0xf8]), 0, 0, 7).is_err());
        assert!(AudioPacket::new(Arc::<[u8]>::from([]), 0, 48_000, 7).is_err());
    }

    #[test]
    fn concealment_matches_the_gap_measured_from_rtp_timestamps() {
        let (mut decoder, mut encoder) = decoder_and_encoder();
        let first = audio_packet(encoded_frame(&mut encoder, 960, 440.0), 0);
        assert_eq!(decoder.decode(&first).expect("decode").len(), 1_920);

        let contiguous = audio_packet(encoded_frame(&mut encoder, 960, 440.0), 960);
        assert!(
            decoder
                .conceal_before(&contiguous)
                .expect("contiguous gap")
                .is_empty()
        );
        assert_eq!(decoder.decode(&contiguous).expect("decode").len(), 1_920);

        let one_lost = audio_packet(encoded_frame(&mut encoder, 960, 440.0), 2_880);
        let concealed = decoder.conceal_before(&one_lost).expect("concealment");
        assert_eq!(concealed.len(), 1_920);
        assert!(concealed.iter().all(|sample| sample.is_finite()));
        assert!(concealed.iter().any(|sample| sample.abs() > 0.001));
        assert_eq!(decoder.decode(&one_lost).expect("decode").len(), 1_920);
    }

    #[test]
    fn dropped_packet_is_concealed_once_by_the_next_gap() {
        let (mut decoder, mut encoder) = decoder_and_encoder();
        let first = audio_packet(encoded_frame(&mut encoder, 960, 440.0), 0);
        assert_eq!(decoder.decode(&first).expect("decode").len(), 1_920);

        let malformed = AudioPacket::new(Arc::<[u8]>::from(vec![0xff; 40]), 1_920, 48_000, 7)
            .expect("malformed packet");
        let first_concealment = decoder.conceal_before(&malformed).expect("concealment");
        assert_eq!(first_concealment.len(), 1_920);
        assert!(decoder.decode(&malformed).is_err());
        decoder.record_dropped_packet(&malformed);

        let next = audio_packet(encoded_frame(&mut encoder, 960, 440.0), 2_880);
        let second_concealment = decoder.conceal_before(&next).expect("concealment");
        assert_eq!(second_concealment.len(), 1_920);
        assert_eq!(decoder.decode(&next).expect("decode").len(), 1_920);

        let contiguous = audio_packet(encoded_frame(&mut encoder, 960, 440.0), 3_840);
        assert!(
            decoder
                .conceal_before(&contiguous)
                .expect("contiguous")
                .is_empty()
        );
    }

    #[test]
    fn burst_losses_conceal_every_missing_frame_duration() {
        let (mut decoder, mut encoder) = decoder_and_encoder();
        let first = audio_packet(encoded_frame(&mut encoder, 960, 440.0), 0);
        decoder.decode(&first).expect("decode");

        for missing in 1_u32..=4 {
            let concealed = decoder
                .conceal_before(&audio_packet(
                    encoded_frame(&mut encoder, 960, 440.0),
                    (missing + 1) * 960,
                ))
                .expect("concealment");
            assert_eq!(concealed.len(), missing as usize * 1_920);
        }
    }

    #[test]
    fn concealment_is_bounded_by_the_configured_time_budget() {
        let (mut decoder, mut encoder) = decoder_and_encoder();
        let first = audio_packet(encoded_frame(&mut encoder, 960, 440.0), 0);
        decoder.decode(&first).expect("decode");

        let concealed = decoder
            .conceal_before(&audio_packet(
                encoded_frame(&mut encoder, 960, 440.0),
                960_000,
            ))
            .expect("concealment");
        let budget_frames = 48_000 * MAX_PLC_MS / 1000 / 960;
        assert_eq!(concealed.len(), budget_frames * 1_920);
    }

    #[test]
    fn concealment_follows_the_last_decoded_frame_duration() {
        let (mut decoder, mut encoder) = decoder_and_encoder();
        let ten_ms = audio_packet(encoded_frame(&mut encoder, 480, 440.0), 0);
        assert_eq!(decoder.decode(&ten_ms).expect("decode").len(), 960);

        let concealed = decoder
            .conceal_before(&audio_packet(
                encoded_frame(&mut encoder, 960, 440.0),
                1_920,
            ))
            .expect("concealment");
        assert_eq!(concealed.len(), 2_880);
    }

    #[test]
    fn concealment_covers_a_partial_frame_gap_exactly() {
        let (mut decoder, mut encoder) = decoder_and_encoder();
        let prior = audio_packet(encoded_frame(&mut encoder, 960, 440.0), 0);
        decoder.decode(&prior).expect("decode");

        let concealed = decoder
            .conceal_before(&audio_packet(
                encoded_frame(&mut encoder, 960, 440.0),
                1_200,
            ))
            .expect("concealment");
        assert_eq!(concealed.len(), 240 * 2);
        assert!(concealed.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn concealment_covers_a_shorter_gap_after_a_long_frame() {
        let (mut decoder, mut encoder) = decoder_and_encoder();
        let prior = audio_packet(encoded_frame(&mut encoder, 2_880, 440.0), 0);
        assert_eq!(decoder.decode(&prior).expect("decode").len(), 5_760);

        let concealed = decoder
            .conceal_before(&audio_packet(
                encoded_frame(&mut encoder, 960, 440.0),
                3_840,
            ))
            .expect("concealment");
        assert_eq!(concealed.len(), 960 * 2);
        assert!(concealed.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn a_source_change_makes_the_decoder_match_a_fresh_one() {
        let (_, mut encoder) = decoder_and_encoder();
        let mut other =
            OpusEncoder::new(48_000, Channels::Stereo, Application::Audio).expect("encoder");
        let poison = encoded_frame(&mut encoder, 960, 220.0);
        let target = encoded_frame(&mut other, 960, 880.0);

        let mut fresh = OpusDecoder::open(&AudioConfig::default()).expect("decoder");
        let expected = fresh
            .decode(&audio_packet(Arc::clone(&target), 0))
            .expect("decode")
            .to_vec();

        let mut poisoned = OpusDecoder::open(&AudioConfig::default()).expect("decoder");
        for timestamp in [0, 960, 1_920] {
            poisoned
                .decode(&audio_packet(Arc::clone(&poison), timestamp))
                .expect("decode");
        }
        let changed = AudioPacket::new(Arc::clone(&target), 4_000_000, 48_000, 9).expect("packet");
        assert!(
            poisoned
                .conceal_before(&changed)
                .expect("source change")
                .is_empty()
        );
        let actual = poisoned.decode(&changed).expect("decode").to_vec();
        assert_eq!(actual, expected);
    }

    #[test]
    fn a_dropped_first_packet_from_a_new_source_still_resets() {
        let (_, mut encoder) = decoder_and_encoder();
        let mut other =
            OpusEncoder::new(48_000, Channels::Stereo, Application::Audio).expect("encoder");
        let mut fresh = OpusDecoder::open(&AudioConfig::default()).expect("decoder");
        let mut poisoned = OpusDecoder::open(&AudioConfig::default()).expect("decoder");

        let old_source = encoded_frame(&mut encoder, 960, 220.0);
        for timestamp in [0, 960, 1_920] {
            poisoned
                .decode(&audio_packet(Arc::clone(&old_source), timestamp))
                .expect("decode");
        }

        let new_source = encoded_frame(&mut other, 960, 880.0);
        let second =
            AudioPacket::new(Arc::clone(&new_source), 4_000_960, 48_000, 9).expect("packet");
        assert!(
            poisoned
                .conceal_before(&second)
                .expect("no concealment")
                .is_empty()
        );
        let actual = poisoned.decode(&second).expect("decode").to_vec();

        let fresh_second =
            AudioPacket::new(Arc::clone(&new_source), 4_000_960, 48_000, 9).expect("packet");
        let expected = fresh.decode(&fresh_second).expect("decode").to_vec();
        assert_eq!(actual, expected);
    }

    #[test]
    fn backward_timestamps_are_discontinuities_not_gaps() {
        let (mut decoder, mut encoder) = decoder_and_encoder();
        let prior = audio_packet(encoded_frame(&mut encoder, 960, 440.0), 1_920);
        decoder.decode(&prior).expect("decode");

        let reordered = audio_packet(encoded_frame(&mut encoder, 960, 440.0), 960);
        assert!(
            decoder
                .conceal_before(&reordered)
                .expect("backward timestamp")
                .is_empty()
        );
    }

    #[test]
    fn redundancy_recovered_packets_are_not_concealed_again() {
        let (mut decoder, mut encoder) = decoder_and_encoder();
        let first = audio_packet(encoded_frame(&mut encoder, 960, 440.0), 0);
        decoder.decode(&first).expect("decode");

        let recovered_older = audio_packet(encoded_frame(&mut encoder, 960, 440.0), 1_920);
        let concealed = decoder
            .conceal_before(&recovered_older)
            .expect("concealment");
        assert_eq!(concealed.len(), 1_920);
        decoder.decode(&recovered_older).expect("decode");

        let recovered_newer = audio_packet(encoded_frame(&mut encoder, 960, 440.0), 2_880);
        assert!(
            decoder
                .conceal_before(&recovered_newer)
                .expect("recovered gap")
                .is_empty()
        );
    }

    #[test]
    fn a_source_change_resets_instead_of_concealing() {
        let (mut decoder, mut encoder) = decoder_and_encoder();
        let first = audio_packet(encoded_frame(&mut encoder, 960, 440.0), 0);
        decoder.decode(&first).expect("decode");

        let changed = AudioPacket::new(
            encoded_frame(&mut encoder, 960, 440.0),
            4_000_000,
            48_000,
            9,
        )
        .expect("packet");
        assert!(
            decoder
                .conceal_before(&changed)
                .expect("source change")
                .is_empty()
        );
        decoder.decode(&changed).expect("decode");

        let next = audio_packet(encoded_frame(&mut encoder, 960, 440.0), 4_000_960);
        assert!(
            decoder
                .conceal_before(&next)
                .expect("rebased gap")
                .is_empty()
        );
    }

    #[test]
    fn a_fresh_decoder_fabricates_nothing() {
        let mut decoder = OpusDecoder::open(&AudioConfig::default()).expect("decoder");
        let packet = audio_packet(Arc::from([0xf8]), 960_000);
        assert!(
            decoder
                .conceal_before(&packet)
                .expect("no reference")
                .is_empty()
        );
    }

    #[test]
    fn wrap_around_timestamps_keep_the_gap_measurable() {
        let (mut decoder, mut encoder) = decoder_and_encoder();
        let first = audio_packet(encoded_frame(&mut encoder, 960, 440.0), u32::MAX - 959);
        decoder.decode(&first).expect("decode");

        let concealed = decoder
            .conceal_before(&audio_packet(encoded_frame(&mut encoder, 960, 440.0), 960))
            .expect("concealment");
        assert_eq!(concealed.len(), 1_920);
    }

    #[test]
    fn pipewire_playback_mute_preserves_source_and_consumes_silently() {
        let config = AudioConfig::default();
        let mut bytes = Vec::new();
        let samples = [0.25_f32, -0.25, 0.5, -0.5, 0.75, -0.75];
        config
            .muted
            .store(true, std::sync::atomic::Ordering::Release);
        write_pipewire_pcm(&mut bytes, &samples, &[0; 16], &config.muted, &|| false)
            .expect("muted PCM");
        config
            .muted
            .store(false, std::sync::atomic::Ordering::Release);
        write_pipewire_pcm(&mut bytes, &samples, &[0; 16], &config.muted, &|| false)
            .expect("audible PCM");
        assert!(bytes[..24].iter().all(|value| *value == 0));
        let expected: Vec<_> = samples.into_iter().flat_map(f32::to_ne_bytes).collect();
        assert_eq!(&bytes[24..], expected);
        assert_eq!(samples, [0.25, -0.25, 0.5, -0.5, 0.75, -0.75]);
    }

    #[test]
    fn pipewire_mute_changes_never_split_a_partially_written_sample() {
        use std::sync::atomic::{AtomicBool, Ordering};

        struct PartialWriter<'a> {
            bytes: Vec<u8>,
            muted: &'a AtomicBool,
        }

        impl Write for PartialWriter<'_> {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.bytes.push(bytes[0]);
                self.muted.store(self.bytes.len() <= 4, Ordering::Release);
                Ok(1)
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let muted = AtomicBool::new(false);
        let mut writer = PartialWriter {
            bytes: Vec::new(),
            muted: &muted,
        };
        write_pipewire_pcm(&mut writer, &[0.375, -0.375], &[0; 8], &muted, &|| false)
            .expect("partial PCM writes");
        let expected: Vec<_> = [0.375_f32, 0.0]
            .into_iter()
            .flat_map(f32::to_ne_bytes)
            .collect();
        assert_eq!(writer.bytes, expected);
    }

    #[test]
    fn fixed_audio_output_validates_backend_and_forbids_fallback() {
        assert!(AudioConfig::default().validate().is_ok());
        for name in ["pipewire:usb.speakers", "alsa:front:CARD=USB"] {
            let config = AudioConfig {
                output_device: name.to_owned(),
                ..AudioConfig::default()
            };
            assert!(config.validate().is_ok());
            assert!(open_audio_fallback(&config, AudioBackend::PipeWire).is_err());
            assert!(open_audio_fallback(&config, AudioBackend::Alsa).is_err());
        }
        for name in ["unknown", "alsa:", "pipewire:", "alsa:a\0b"] {
            let config = AudioConfig {
                output_device: name.to_owned(),
                ..AudioConfig::default()
            };
            assert!(config.validate().is_err());
        }
    }

    #[test]
    fn missing_alsa_output_never_opens_default() {
        let config = AudioConfig {
            output_device: "alsa:opennow-test-missing-device".to_owned(),
            ..AudioConfig::default()
        };
        assert!(open_audio_sink(&config).is_err());
    }

    #[test]
    #[ignore = "requires ALSA_CONFIG_PATH=tests/fixtures/audio-null.conf"]
    fn selected_alsa_output_opens_and_survives_enumeration() {
        let config = AudioConfig {
            output_device: "alsa:opennow_test_output".to_owned(),
            ..AudioConfig::default()
        };
        let mut sink = open_audio_sink(&config).expect("selected ALSA output");
        assert_eq!(sink.backend(), AudioBackend::Alsa);
        sink.write(&[0.0; 960], &|| false)
            .expect("initial PCM write");
        let devices = crate::audio_devices::alsa_devices().expect("enumeration during playback");
        crate::audio_devices::require_device(&config.output_device, &devices)
            .expect("selected device remains visible");
        sink.write(&[0.0; 960], &|| false)
            .expect("PCM after enumeration");
        let missing = AudioConfig {
            output_device: "alsa:missing".to_owned(),
            ..config
        };
        assert!(open_audio_sink(&missing).is_err());
        sink.write(&[0.0; 960], &|| false)
            .expect("original output survives failed open");
    }

    #[test]
    #[ignore = "requires an isolated PipeWire server with opennow_test_output sink"]
    fn selected_pipewire_output_opens_and_survives_enumeration() {
        let config = AudioConfig {
            output_device: "pipewire:opennow_test_output".to_owned(),
            ..AudioConfig::default()
        };
        let mut sink = PipeWireSink::open(&config).expect("selected PipeWire output");
        assert_eq!(sink.backend(), AudioBackend::PipeWire);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        sink.write(&[0.0; 960], &|| std::time::Instant::now() >= deadline)
            .expect("initial PCM write");
        let pid = sink.child.id();
        loop {
            let bytes = crate::audio_devices::bounded_command_output(
                &mut Command::new("pw-dump"),
                std::time::Duration::from_millis(500),
            )
            .unwrap();
            let graph: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
            let target = graph
                .iter()
                .find(|object| object["info"]["props"]["node.name"] == "opennow_test_output")
                .expect("target node")["id"]
                .clone();
            let client = graph.iter().find(|object| {
                object["type"] == "PipeWire:Interface:Client"
                    && object["info"]["props"]["application.process.id"] == pid
            });
            let stream = client.and_then(|client| {
                graph.iter().find(|object| {
                    object["type"] == "PipeWire:Interface:Node"
                        && object["info"]["props"]["client.id"] == client["id"]
                })
            });
            if let Some(stream) = stream {
                assert_eq!(
                    stream["info"]["props"]["target.object"],
                    "opennow_test_output"
                );
                assert_eq!(stream["info"]["props"]["node.dont-fallback"], true);
                if graph.iter().any(|object| {
                    object["type"] == "PipeWire:Interface:Link"
                        && object["info"]["output-node-id"] == stream["id"]
                        && object["info"]["input-node-id"] == target
                }) {
                    break;
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "playback was not linked to the selected sink"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let devices =
            crate::audio_devices::pipewire_devices().expect("enumeration during playback");
        crate::audio_devices::require_device(&config.output_device, &devices)
            .expect("selected device remains visible");
        let missing = AudioConfig {
            output_device: "pipewire:opennow_missing_output".to_owned(),
            ..config
        };
        assert!(open_audio_sink(&missing).is_err());
        sink.write(&[0.0; 960], &|| std::time::Instant::now() >= deadline)
            .expect("PCM after enumeration");
    }
}
