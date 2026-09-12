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
    pub timestamp_us: u64,
}

impl AudioPacket {
    pub fn new(data: impl Into<Arc<[u8]>>, timestamp_us: u64) -> Result<Self> {
        let data = data.into();
        let packet = Self { data, timestamp_us };
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
        Ok(())
    }
}

type OpusDecoderCreate = unsafe extern "C" fn(c_int, c_int, *mut c_int) -> *mut c_void;
type OpusDecodeFloat =
    unsafe extern "C" fn(*mut c_void, *const u8, c_int, *mut f32, c_int, c_int) -> c_int;
type OpusDecoderDestroy = unsafe extern "C" fn(*mut c_void);
type OpusStrError = unsafe extern "C" fn(c_int) -> *const c_char;

pub(crate) struct OpusDecoder {
    _library: Library,
    handle: NonNull<c_void>,
    decode_float: OpusDecodeFloat,
    destroy: OpusDecoderDestroy,
    strerror: OpusStrError,
    sample_rate: u32,
    channels: usize,
    pcm: Vec<f32>,
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
                destroy,
                strerror,
                sample_rate: config.sample_rate,
                channels,
                pcm: vec![0.0; config.sample_rate as usize * OPUS_MAX_FRAME_MS / 1000 * channels],
            })
        }
    }

    pub fn decode<'a>(&'a mut self, packet: &AudioPacket) -> Result<&'a [f32]> {
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
        Ok(&self.pcm[..samples_per_channel as usize * self.channels])
    }
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
            let frames = (pcm.len() - offset) / self.channels;
            let written = unsafe {
                (self.writei)(
                    self.handle.as_ptr(),
                    pcm[offset..].as_ptr().cast(),
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
        Ok(Self { child, stdin })
    }
}

impl AudioSink for PipeWireSink {
    fn backend(&self) -> AudioBackend {
        AudioBackend::PipeWire
    }

    fn write(&mut self, pcm: &[f32], cancelled: &dyn Fn() -> bool) -> Result<()> {
        let bytes =
            unsafe { slice::from_raw_parts(pcm.as_ptr().cast::<u8>(), mem::size_of_val(pcm)) };
        let mut offset = 0;
        while offset < bytes.len() {
            if cancelled() {
                return Err(Error::QueueClosed);
            }
            match self.stdin.write(&bytes[offset..]) {
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
