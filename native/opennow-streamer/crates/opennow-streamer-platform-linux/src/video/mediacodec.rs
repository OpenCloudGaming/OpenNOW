use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::VideoDecoder;
use crate::{
    ChromaLocation, ColorMatrix, ColorPrimaries, ColorRange, ColorTransfer, DecodedVideoFrame,
    EncodedVideoFrame, Error, FramePlane, PixelFormat, Result, StreamFormat, Subsystem, VideoCodec,
};

const MAGIC: u32 = 0x44444d43;
const VERSION: u32 = 1;
const PROBE: u32 = 1;
const OPEN: u32 = 2;
const SUBMIT: u32 = 3;
const POLL: u32 = 4;
const DRAIN: u32 = 5;
const CLOSE: u32 = 6;
const CAPABILITY: u32 = 101;
const OPENED: u32 = 102;
const RESULT: u32 = 103;
const CLOSED: u32 = 104;
const ERROR: u32 = 199;
const EXCHANGE_TIMEOUT: Duration = Duration::from_secs(1);
const OPEN_TIMEOUT: Duration = Duration::from_secs(2);
const DRAIN_TIMEOUT: Duration = Duration::from_millis(500);
const RESIDENCE_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_PENDING: usize = 32;
pub const MEDIACODEC_MAX_WIDTH: u32 = 2560;
pub const MEDIACODEC_MAX_HEIGHT: u32 = 1440;

fn protocol(reason: impl Into<String>) -> Error {
    Error::backend(
        Subsystem::MediaCodec,
        format!("protocol: {}", reason.into()),
    )
}

fn reference_lost(reason: impl Into<String>) -> Error {
    Error::ReferenceLost {
        subsystem: Subsystem::MediaCodec,
        reason: reason.into(),
    }
}

fn idle_backoff(progress: bool, deadline: Instant) {
    if !progress {
        std::thread::sleep(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(1)),
        );
    }
}

fn endpoint(explicit: Option<&Path>) -> Result<PathBuf> {
    explicit
        .map(Path::to_path_buf)
        .or_else(|| {
            std::env::var_os("OPENNOW_MEDIACODEC_SOCKET")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
        .ok_or_else(|| {
            Error::unavailable(
                Subsystem::MediaCodec,
                "OPENNOW_MEDIACODEC_SOCKET is not set",
            )
        })
}

pub fn probe_mediacodec() -> std::result::Result<String, String> {
    probe_at(&endpoint(None).map_err(|error| error.to_string())?).map_err(|error| error.to_string())
}

fn probe_at(path: &Path) -> Result<String> {
    let deadline = Instant::now() + OPEN_TIMEOUT;
    let mut wire = Wire::connect(path, deadline)?;
    let response = wire.exchange(PROBE, &[], CAPABILITY, 268, deadline)?;
    capability(&response)
}

fn capability(payload: &[u8]) -> Result<String> {
    let mut cursor = Cursor::new(payload);
    let hardware = cursor.u32()?;
    let software = cursor.u32()?;
    let length = cursor.u32()? as usize;
    if hardware != 1 || software != 0 || !(1..=256).contains(&length) {
        return Err(Error::unavailable(
            Subsystem::MediaCodec,
            "bridge did not identify a hardware-only codec",
        ));
    }
    let name = std::str::from_utf8(cursor.bytes(length)?)
        .map_err(|_| protocol("invalid codec name UTF-8"))?;
    if name.trim().is_empty() || name.chars().any(char::is_control) {
        return Err(protocol("invalid codec name"));
    }
    cursor.finish()?;
    Ok(name.to_owned())
}

struct Wire {
    stream: Option<UnixStream>,
}

impl Wire {
    fn connect(path: &Path, deadline: Instant) -> Result<Self> {
        let bytes = path.as_os_str().as_bytes();
        let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
        if bytes.is_empty() || bytes.len() >= address.sun_path.len() || bytes.contains(&0) {
            return Err(protocol("invalid Unix socket path"));
        }
        address.sun_family = libc::AF_UNIX as libc::sa_family_t;
        for (destination, byte) in address.sun_path.iter_mut().zip(bytes) {
            *destination = *byte as libc::c_char;
        }
        let raw = unsafe {
            libc::socket(
                libc::AF_UNIX,
                libc::SOCK_STREAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
                0,
            )
        };
        if raw < 0 {
            return Err(Error::io(
                Subsystem::MediaCodec,
                std::io::Error::last_os_error(),
            ));
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let result = unsafe {
            libc::connect(
                fd.as_raw_fd(),
                (&address as *const libc::sockaddr_un).cast(),
                std::mem::size_of_val(&address) as libc::socklen_t,
            )
        };
        if result < 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EINPROGRESS) {
                return Err(Error::io(Subsystem::MediaCodec, error));
            }
            wait_fd(fd.as_raw_fd(), libc::POLLOUT, deadline)?;
            let mut error = 0i32;
            let mut size = std::mem::size_of_val(&error) as libc::socklen_t;
            if unsafe {
                libc::getsockopt(
                    fd.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_ERROR,
                    (&mut error as *mut i32).cast(),
                    &mut size,
                )
            } < 0
            {
                return Err(Error::io(
                    Subsystem::MediaCodec,
                    std::io::Error::last_os_error(),
                ));
            }
            if error != 0 {
                return Err(Error::io(
                    Subsystem::MediaCodec,
                    std::io::Error::from_raw_os_error(error),
                ));
            }
        }
        Ok(Self {
            stream: Some(UnixStream::from(fd)),
        })
    }

    fn exchange(
        &mut self,
        request: u32,
        payload: &[u8],
        response: u32,
        limit: usize,
        deadline: Instant,
    ) -> Result<Vec<u8>> {
        let result = self.exchange_inner(request, payload, response, limit, deadline);
        if result
            .as_ref()
            .is_err_and(|error| !matches!(error, Error::ReferenceLost { .. }))
        {
            self.stream.take();
        }
        result
    }

    fn exchange_inner(
        &mut self,
        request: u32,
        payload: &[u8],
        response: u32,
        limit: usize,
        deadline: Instant,
    ) -> Result<Vec<u8>> {
        let stream = self.stream.as_mut().ok_or(Error::QueueClosed)?;
        let mut header = Vec::with_capacity(16);
        for value in [MAGIC, VERSION, request, payload.len() as u32] {
            header.extend_from_slice(&value.to_be_bytes());
        }
        write_all(stream, &header, deadline)?;
        write_all(stream, payload, deadline)?;
        let mut header = [0u8; 16];
        read_all(stream, &mut header, deadline)?;
        let mut cursor = Cursor::new(&header);
        if cursor.u32()? != MAGIC || cursor.u32()? != VERSION {
            return Err(protocol("invalid magic or version"));
        }
        let kind = cursor.u32()?;
        let size = cursor.u32()? as usize;
        if (kind != response && kind != ERROR) || size > if kind == ERROR { 1028 } else { limit } {
            return Err(protocol("unexpected response kind or oversized payload"));
        }
        let mut payload = vec![0u8; size];
        read_all(stream, &mut payload, deadline)?;
        if kind == ERROR {
            let mut cursor = Cursor::new(&payload);
            let code = cursor.u32()?;
            let text = cursor.bytes(payload.len().saturating_sub(4))?;
            if text.is_empty() {
                return Err(protocol("empty remote error"));
            }
            let reason = std::str::from_utf8(text).map_err(|_| protocol("invalid error UTF-8"))?;
            return Err(match code {
                1 => Error::unavailable(Subsystem::MediaCodec, reason),
                2 => reference_lost(reason),
                3 => protocol(reason),
                4 => Error::backend(Subsystem::MediaCodec, reason),
                _ => protocol("unknown remote error code"),
            });
        }
        Ok(payload)
    }
}

fn wait_fd(fd: i32, events: i16, deadline: Instant) -> Result<()> {
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| {
                Error::io(
                    Subsystem::MediaCodec,
                    std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "absolute socket deadline expired",
                    ),
                )
            })?;
        let millis = remaining
            .as_millis()
            .saturating_add(1)
            .min(i32::MAX as u128) as i32;
        let mut poll = libc::pollfd {
            fd,
            events,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut poll, 1, millis) };
        if result > 0 {
            if Instant::now() >= deadline {
                return Err(Error::io(
                    Subsystem::MediaCodec,
                    std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "absolute socket deadline expired",
                    ),
                ));
            }
            if poll.revents & libc::POLLNVAL != 0 {
                return Err(protocol("invalid socket"));
            }
            return Ok(());
        }
        if result < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::Interrupted {
                return Err(Error::io(Subsystem::MediaCodec, error));
            }
        }
    }
}

fn write_all(stream: &mut UnixStream, mut bytes: &[u8], deadline: Instant) -> Result<()> {
    while !bytes.is_empty() {
        wait_fd(stream.as_raw_fd(), libc::POLLOUT, deadline)?;
        match stream.write(bytes) {
            Ok(0) => return Err(protocol("socket write returned zero")),
            Ok(count) => bytes = &bytes[count..],
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(Error::io(Subsystem::MediaCodec, error)),
        }
    }
    Ok(())
}

fn read_all(stream: &mut UnixStream, mut bytes: &mut [u8], deadline: Instant) -> Result<()> {
    while !bytes.is_empty() {
        wait_fd(stream.as_raw_fd(), libc::POLLIN, deadline)?;
        match stream.read(bytes) {
            Ok(0) => return Err(protocol("socket EOF inside response")),
            Ok(count) => bytes = &mut bytes[count..],
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(Error::io(Subsystem::MediaCodec, error)),
        }
    }
    Ok(())
}

struct Cursor<'a> {
    bytes: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
    fn bytes(&mut self, count: usize) -> Result<&'a [u8]> {
        if count > self.bytes.len() {
            return Err(protocol("truncated payload"));
        }
        let (value, remaining) = self.bytes.split_at(count);
        self.bytes = remaining;
        Ok(value)
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.bytes(8)?.try_into().unwrap()))
    }
    fn finish(self) -> Result<()> {
        if self.bytes.is_empty() {
            Ok(())
        } else {
            Err(protocol("trailing payload bytes"))
        }
    }
}

pub(crate) struct MediaCodecDecoder {
    wire: Wire,
    format: StreamFormat,
    pending: HashMap<u64, Instant>,
    draining: bool,
    drained: bool,
    closed: bool,
}

impl MediaCodecDecoder {
    pub(crate) fn open(
        codec: VideoCodec,
        format: StreamFormat,
        path: Option<&Path>,
    ) -> Result<Self> {
        format.validate()?;
        if codec != VideoCodec::H264
            || format.width > MEDIACODEC_MAX_WIDTH
            || format.height > MEDIACODEC_MAX_HEIGHT
            || format.pixel_format != PixelFormat::Nv12
            || format.color_matrix != ColorMatrix::Bt709
            || format.color_primaries != ColorPrimaries::Bt709
            || format.color_transfer != ColorTransfer::Sdr
            || format.chroma_location != ChromaLocation::Left
        {
            return Err(Error::unavailable(
                Subsystem::MediaCodec,
                "bridge requires H.264 NV12 SDR BT.709 left-chroma up to 2560x1440",
            ));
        }
        let deadline = Instant::now() + OPEN_TIMEOUT;
        let wire = Wire::connect(&endpoint(path)?, deadline)?;
        let mut decoder = Self {
            wire,
            format,
            pending: HashMap::new(),
            draining: false,
            drained: false,
            closed: false,
        };
        let mut payload = Vec::with_capacity(12);
        for value in [
            format.width,
            format.height,
            u32::from(format.color_range == ColorRange::Full),
        ] {
            payload.extend_from_slice(&value.to_be_bytes());
        }
        let response = decoder
            .wire
            .exchange(OPEN, &payload, OPENED, 268, deadline)?;
        capability(&response)?;
        Ok(decoder)
    }

    fn check_residence(&self) -> Result<()> {
        if self
            .pending
            .values()
            .any(|accepted| accepted.elapsed() >= RESIDENCE_TIMEOUT)
        {
            return Err(reference_lost(
                "accepted input exceeded two-second output residence",
            ));
        }
        Ok(())
    }

    fn result(
        &mut self,
        request: u32,
        payload: &[u8],
        submitted: Option<(u64, Instant)>,
        deadline: Instant,
    ) -> Result<(u32, Vec<DecodedVideoFrame>)> {
        let pixels = self.format.width as usize * self.format.height as usize;
        let limit = 8 + 2 * (16 + pixels + pixels / 2);
        let response = self
            .wire
            .exchange(request, payload, RESULT, limit, deadline)?;
        let parsed = self.parse_result(&response, submitted);
        if parsed
            .as_ref()
            .is_err_and(|error| !matches!(error, Error::ReferenceLost { .. }))
        {
            self.wire.stream.take();
        }
        parsed
    }

    fn retry_result(
        &mut self,
        request: u32,
        payload: &[u8],
        submitted: Option<(u64, Instant)>,
        deadline: Instant,
    ) -> Result<(u32, Vec<DecodedVideoFrame>)> {
        self.result(request, payload, submitted, deadline).map_err(|error| {
            if matches!(&error, Error::Io { source, .. } if source.kind() == std::io::ErrorKind::TimedOut) {
                reference_lost("codec admission retry exceeded 500 ms; incomplete exchange invalidated socket")
            } else {
                error
            }
        })
    }

    fn parse_result(
        &mut self,
        response: &[u8],
        submitted: Option<(u64, Instant)>,
    ) -> Result<(u32, Vec<DecodedVideoFrame>)> {
        let pixels = self.format.width as usize * self.format.height as usize;
        let mut cursor = Cursor::new(response);
        let status = cursor.u32()?;
        let count = cursor.u32()?;
        if status > 2
            || count > 2
            || (status == 1 && !self.draining)
            || (status == 2 && submitted.is_none())
        {
            return Err(protocol("invalid result status or frame count"));
        }
        if status == 0 {
            if let Some((pts, accepted_at)) = submitted {
                if self.pending.len() >= MAX_PENDING
                    || self.pending.insert(pts, accepted_at).is_some()
                {
                    return Err(protocol("duplicate or overflowing submission ledger"));
                }
            }
        }
        let mut frames = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let timestamp_us = cursor.u64()?;
            let y_bytes = cursor.u32()? as usize;
            let uv_bytes = cursor.u32()? as usize;
            if timestamp_us > i64::MAX as u64 || y_bytes != pixels || uv_bytes != pixels / 2 {
                return Err(protocol("invalid output PTS or NV12 lengths"));
            }
            if self.pending.remove(&timestamp_us).is_none() {
                return Err(protocol("duplicate or unmatched output PTS"));
            }
            frames.push(DecodedVideoFrame {
                provenance: Default::default(),
                correlation_timestamp_us: Some(timestamp_us),
                format: self.format,
                planes: vec![
                    FramePlane {
                        data: Arc::from(cursor.bytes(y_bytes)?),
                        stride: self.format.width as usize,
                        rows: self.format.height as usize,
                    },
                    FramePlane {
                        data: Arc::from(cursor.bytes(uv_bytes)?),
                        stride: self.format.width as usize,
                        rows: self.format.height as usize / 2,
                    },
                ],
                dmabuf: None,
                vulkan: None,
                timestamp_us,
            });
        }
        cursor.finish()?;
        if status == 1 {
            if !self.pending.is_empty() {
                return Err(reference_lost("EOS left accepted inputs without output"));
            }
            self.drained = true;
        }
        self.check_residence()?;
        Ok((status, frames))
    }
}

impl VideoDecoder for MediaCodecDecoder {
    fn decode(&mut self, frame: &EncodedVideoFrame) -> Result<Vec<DecodedVideoFrame>> {
        frame.validate()?;
        if self.draining || self.closed {
            return Err(protocol("submission after drain or close"));
        }
        if frame.timestamp_us > i64::MAX as u64 || self.pending.contains_key(&frame.timestamp_us) {
            return Err(protocol(
                "input PTS must be signed-i64 and unique among pending inputs",
            ));
        }
        self.check_residence()?;
        let deadline = Instant::now() + DRAIN_TIMEOUT;
        let mut frames = Vec::new();
        while self.pending.len() >= MAX_PENDING {
            if Instant::now() >= deadline {
                return Err(reference_lost("codec submission ledger remained full"));
            }
            let outputs = self.retry_result(POLL, &[], None, deadline)?.1;
            idle_backoff(!outputs.is_empty(), deadline);
            frames.extend(outputs);
        }
        let mut payload = Vec::with_capacity(12 + frame.data.len());
        payload.extend_from_slice(&frame.timestamp_us.to_be_bytes());
        payload.extend_from_slice(&u32::from(frame.keyframe).to_be_bytes());
        payload.extend_from_slice(&frame.data);
        loop {
            if Instant::now() >= deadline {
                return Err(reference_lost("codec input remained busy"));
            }
            let (status, outputs) = self.retry_result(
                SUBMIT,
                &payload,
                Some((frame.timestamp_us, Instant::now())),
                deadline,
            )?;
            let mut progress = !outputs.is_empty();
            frames.extend(outputs);
            if status == 0 {
                return Ok(frames);
            }
            let outputs = self.retry_result(POLL, &[], None, deadline)?.1;
            progress |= !outputs.is_empty();
            frames.extend(outputs);
            idle_backoff(progress, deadline);
        }
    }

    fn poll(&mut self) -> Result<Vec<DecodedVideoFrame>> {
        if self.closed {
            return Err(Error::QueueClosed);
        }
        if self.drained {
            return Ok(Vec::new());
        }
        self.check_residence()?;
        Ok(self
            .result(POLL, &[], None, Instant::now() + EXCHANGE_TIMEOUT)?
            .1)
    }

    fn flush(&mut self) -> Result<Vec<DecodedVideoFrame>> {
        if self.closed || self.drained {
            return Ok(Vec::new());
        }
        let deadline = Instant::now() + DRAIN_TIMEOUT;
        let mut frames = Vec::new();
        if !self.draining {
            self.draining = true;
            frames.extend(self.result(DRAIN, &[], None, deadline)?.1);
        }
        while !self.drained {
            if Instant::now() >= deadline {
                return Err(reference_lost("codec drain exceeded 500 ms"));
            }
            let outputs = self.result(POLL, &[], None, deadline)?.1;
            idle_backoff(self.drained || !outputs.is_empty(), deadline);
            frames.extend(outputs);
        }
        Ok(frames)
    }

    fn take_format_change(&mut self) -> Option<StreamFormat> {
        None
    }

    fn close(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        let result = self
            .wire
            .exchange(CLOSE, &[], CLOSED, 0, Instant::now() + EXCHANGE_TIMEOUT);
        self.wire.stream.take();
        result.map(|_| ())
    }
}

impl Drop for MediaCodecDecoder {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_SOCKET: AtomicU64 = AtomicU64::new(0);

    fn server<T>(
        serve: impl FnOnce(UnixStream) + Send + 'static,
        client: impl FnOnce(&Path) -> T,
    ) -> T {
        listener_server(
            move |listener| {
                let (stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                serve(stream);
            },
            client,
        )
    }

    fn listener_server<T>(
        serve: impl FnOnce(UnixListener) + Send + 'static,
        client: impl FnOnce(&Path) -> T,
    ) -> T {
        let path = std::env::temp_dir().join(format!(
            "opennow-mc-{}-{}",
            std::process::id(),
            NEXT_SOCKET.fetch_add(1, Ordering::Relaxed)
        ));
        let listener = UnixListener::bind(&path).unwrap();
        let worker = std::thread::spawn(move || serve(listener));
        let result = client(&path);
        worker.join().unwrap();
        std::fs::remove_file(path).unwrap();
        result
    }

    fn request(stream: &mut UnixStream) -> (u32, Vec<u8>) {
        let mut header = [0; 16];
        stream.read_exact(&mut header).unwrap();
        let mut cursor = Cursor::new(&header);
        assert_eq!(cursor.u32().unwrap(), MAGIC);
        assert_eq!(cursor.u32().unwrap(), VERSION);
        let kind = cursor.u32().unwrap();
        let mut payload = vec![0; cursor.u32().unwrap() as usize];
        stream.read_exact(&mut payload).unwrap();
        (kind, payload)
    }

    fn response(stream: &mut UnixStream, kind: u32, payload: &[u8]) {
        let mut bytes = Vec::new();
        for value in [MAGIC, VERSION, kind, payload.len() as u32] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes.extend_from_slice(payload);
        for chunk in bytes.chunks(3) {
            stream.write_all(chunk).unwrap();
        }
    }

    fn identity(hardware: u32, software: u32, name: &[u8]) -> Vec<u8> {
        let mut payload = Vec::new();
        for value in [hardware, software, name.len() as u32] {
            payload.extend_from_slice(&value.to_be_bytes());
        }
        payload.extend_from_slice(name);
        payload
    }

    fn results(status: u32, frames: &[u64]) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&status.to_be_bytes());
        payload.extend_from_slice(&(frames.len() as u32).to_be_bytes());
        for pts in frames {
            payload.extend_from_slice(&pts.to_be_bytes());
            payload.extend_from_slice(&4u32.to_be_bytes());
            payload.extend_from_slice(&2u32.to_be_bytes());
            payload.extend_from_slice(&[16; 4]);
            payload.extend_from_slice(&[128; 2]);
        }
        payload
    }

    fn opened(stream: &mut UnixStream) {
        let (kind, payload) = request(stream);
        assert_eq!(kind, OPEN);
        assert_eq!(payload, [0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 0]);
        response(stream, OPENED, &identity(1, 0, b"test.hardware.avc"));
    }

    fn closed(stream: &mut UnixStream) {
        assert_eq!(request(stream), (CLOSE, Vec::new()));
        response(stream, CLOSED, &[]);
    }

    fn decoder(path: &Path) -> MediaCodecDecoder {
        MediaCodecDecoder::open(
            VideoCodec::H264,
            StreamFormat::video_default(2, 2).unwrap(),
            Some(path),
        )
        .unwrap()
    }

    fn input(pts: u64) -> EncodedVideoFrame {
        EncodedVideoFrame::new(vec![0, 0, 0, 1, 0x65], pts, true).unwrap()
    }

    #[test]
    fn mediacodec_correlates_reordered_output_to_original_sender_provenance() {
        use opennow_media_protocol::{FrameProvenance, SourceStamp};
        server(
            |mut stream| {
                opened(&mut stream);
                let mut tokens = Vec::new();
                for _ in 0..3 {
                    let (kind, payload) = request(&mut stream);
                    assert_eq!(kind, SUBMIT);
                    tokens.push(u64::from_be_bytes(payload[..8].try_into().unwrap()));
                    response(&mut stream, RESULT, &results(0, &[]));
                }
                assert_eq!(request(&mut stream).0, POLL);
                response(&mut stream, RESULT, &results(0, &[tokens[2], tokens[0]]));
                assert_eq!(request(&mut stream).0, POLL);
                response(&mut stream, RESULT, &results(0, &[tokens[1]]));
                closed(&mut stream);
            },
            |path| {
                let mut decoder = crate::video::correlate_decoder(Box::new(decoder(path)));
                let provenance = |sender_frame_id| FrameProvenance {
                    attempt_generation: 9,
                    track_id: 2,
                    source: Some(SourceStamp {
                        sender_frame_id,
                        timestamp: u64::MAX - 2,
                        clock_rate_hz: 90_000,
                        ssrc: Some(u32::MAX),
                    }),
                };
                let stamps = [
                    provenance(Some(u64::MAX)),
                    provenance(Some(0)),
                    FrameProvenance::default(),
                ];
                for stamp in stamps {
                    assert!(
                        decoder
                            .decode(&input(42).with_provenance(stamp))
                            .unwrap()
                            .is_empty()
                    );
                }
                let mut output = decoder.poll().unwrap();
                output.extend(decoder.poll().unwrap());
                assert_eq!(
                    output
                        .iter()
                        .map(|frame| frame.provenance)
                        .collect::<Vec<_>>(),
                    [stamps[2], stamps[0], stamps[1]]
                );
                assert!(
                    output.iter().all(|frame| frame.timestamp_us == 42
                        && frame.correlation_timestamp_us.is_none())
                );
                decoder.close().unwrap();
            },
        );
    }

    #[test]
    fn mediacodec_correlating_wrapper_propagates_close_acknowledgement() {
        for acknowledged in [false, true] {
            server(
                move |mut stream| {
                    opened(&mut stream);
                    assert_eq!(request(&mut stream).0, CLOSE);
                    response(&mut stream, if acknowledged { CLOSED } else { RESULT }, &[]);
                },
                |path| {
                    let mut decoder = crate::video::correlate_decoder(Box::new(decoder(path)));
                    assert_eq!(decoder.close().is_ok(), acknowledged);
                    assert!(matches!(decoder.poll(), Err(Error::QueueClosed)));
                    decoder.close().unwrap();
                },
            );
        }
    }

    #[test]
    fn mediacodec_probe_requires_strict_hardware_identity_and_utf8_name() {
        for (hardware, software, name) in [
            (0, 0, &b"codec"[..]),
            (1, 1, &b"codec"[..]),
            (1, 0, &b""[..]),
            (1, 0, &b"\xff"[..]),
            (1, 0, &b"\ncodec"[..]),
        ] {
            assert!(capability(&identity(hardware, software, name)).is_err());
        }
        server(
            |mut stream| {
                assert_eq!(request(&mut stream), (PROBE, vec![]));
                response(
                    &mut stream,
                    CAPABILITY,
                    &identity(1, 0, b"test.hardware.avc"),
                );
            },
            |path| assert_eq!(probe_at(path).unwrap(), "test.hardware.avc"),
        );
    }

    #[test]
    fn mediacodec_reordered_pts_and_delayed_poll_output_are_exact() {
        server(
            |mut stream| {
                opened(&mut stream);
                for pts in [1234u64, 1220] {
                    let (kind, payload) = request(&mut stream);
                    assert_eq!(kind, SUBMIT);
                    assert_eq!(&payload[..8], &pts.to_be_bytes());
                    response(&mut stream, RESULT, &results(0, &[]));
                }
                assert_eq!(request(&mut stream).0, POLL);
                response(&mut stream, RESULT, &results(0, &[1220, 1234]));
                closed(&mut stream);
            },
            |path| {
                let mut decoder = decoder(path);
                assert!(decoder.decode(&input(1234)).unwrap().is_empty());
                assert!(decoder.decode(&input(1220)).unwrap().is_empty());
                let frames = decoder.poll().unwrap();
                assert_eq!(
                    frames
                        .iter()
                        .map(|frame| frame.timestamp_us)
                        .collect::<Vec<_>>(),
                    [1220, 1234]
                );
                assert_eq!(frames[0].planes[0].data.as_ref(), &[16; 4]);
                assert_eq!(frames[0].planes[1].data.as_ref(), &[128; 2]);
                assert!(decoder.pending.is_empty());
                decoder.close().unwrap();
                decoder.close().unwrap();
            },
        );
    }

    #[test]
    fn mediacodec_busy_retries_same_au_without_retiring_admission() {
        server(
            |mut stream| {
                opened(&mut stream);
                let submitted = request(&mut stream);
                assert_eq!(submitted.0, SUBMIT);
                response(&mut stream, RESULT, &results(2, &[]));
                assert_eq!(request(&mut stream).0, POLL);
                response(&mut stream, RESULT, &results(0, &[]));
                assert_eq!(request(&mut stream), submitted);
                response(&mut stream, RESULT, &results(0, &[]));
                closed(&mut stream);
            },
            |path| {
                let mut decoder = decoder(path);
                assert!(decoder.decode(&input(42)).unwrap().is_empty());
                assert_eq!(decoder.pending.len(), 1);
                assert!(decoder.pending.contains_key(&42));
                assert!(decoder.decode(&input(42)).is_err());
                assert!(decoder.decode(&input(i64::MAX as u64 + 1)).is_err());
                decoder.close().unwrap();
            },
        );
    }

    #[test]
    fn mediacodec_drain_retrieves_frames_then_requires_eos_and_close_ack() {
        server(
            |mut stream| {
                opened(&mut stream);
                assert_eq!(request(&mut stream).0, SUBMIT);
                response(&mut stream, RESULT, &results(0, &[]));
                assert_eq!(request(&mut stream).0, DRAIN);
                response(&mut stream, RESULT, &results(0, &[42]));
                assert_eq!(request(&mut stream).0, POLL);
                response(&mut stream, RESULT, &results(1, &[]));
                closed(&mut stream);
            },
            |path| {
                let mut decoder = decoder(path);
                decoder.decode(&input(42)).unwrap();
                assert_eq!(decoder.flush().unwrap()[0].timestamp_us, 42);
                assert!(decoder.flush().unwrap().is_empty());
                assert!(decoder.decode(&input(43)).is_err());
                decoder.close().unwrap();
            },
        );
    }

    #[test]
    fn mediacodec_reference_loss_retains_connection_for_graceful_close() {
        server(
            |mut stream| {
                opened(&mut stream);
                assert_eq!(request(&mut stream).0, SUBMIT);
                let mut payload = 2u32.to_be_bytes().to_vec();
                payload.extend_from_slice(b"reference lost");
                response(&mut stream, ERROR, &payload);
                closed(&mut stream);
            },
            |path| {
                let mut decoder = decoder(path);
                assert!(matches!(
                    decoder.decode(&input(42)),
                    Err(Error::ReferenceLost { .. })
                ));
                decoder.close().unwrap();
            },
        );
    }

    #[test]
    fn mediacodec_oldest_pts_not_last_submission_controls_stall_detection() {
        server(
            |mut stream| {
                opened(&mut stream);
                closed(&mut stream);
            },
            |path| {
                let mut decoder = decoder(path);
                decoder
                    .pending
                    .insert(42, Instant::now() - RESIDENCE_TIMEOUT);
                decoder.pending.insert(43, Instant::now());
                assert!(matches!(decoder.poll(), Err(Error::ReferenceLost { .. })));
                decoder.close().unwrap();
            },
        );
    }

    #[test]
    fn mediacodec_rejects_unmatched_duplicate_and_malformed_output() {
        server(
            |mut stream| {
                opened(&mut stream);
                closed(&mut stream);
            },
            |path| {
                let mut decoder = decoder(path);
                assert!(decoder.parse_result(&results(0, &[1]), None).is_err());
                decoder.pending.insert(1, Instant::now());
                assert!(decoder.parse_result(&results(0, &[1, 1]), None).is_err());
                let mut bad = results(0, &[1]);
                bad[16..20].copy_from_slice(&5u32.to_be_bytes());
                decoder.pending.insert(1, Instant::now());
                assert!(decoder.parse_result(&bad, None).is_err());
                assert!(decoder.parse_result(&results(2, &[]), None).is_err());
                decoder.pending.clear();
                for pts in 0..MAX_PENDING {
                    decoder.pending.insert(pts as u64, Instant::now());
                }
                assert!(
                    decoder
                        .parse_result(&results(0, &[]), Some((99, Instant::now())))
                        .is_err()
                );
                decoder.draining = true;
                assert!(matches!(
                    decoder.parse_result(&results(1, &[]), None),
                    Err(Error::ReferenceLost { .. })
                ));
                decoder.close().unwrap();
            },
        );
    }

    #[test]
    fn mediacodec_frame_count_and_payload_are_bounded_before_allocation() {
        server(
            |mut stream| {
                assert_eq!(request(&mut stream).0, PROBE);
                let mut header = Vec::new();
                for value in [MAGIC, VERSION, CAPABILITY, u32::MAX] {
                    header.extend_from_slice(&value.to_be_bytes());
                }
                stream.write_all(&header).unwrap();
            },
            |path| assert!(probe_at(path).is_err()),
        );
        server(
            |mut stream| {
                opened(&mut stream);
                closed(&mut stream);
            },
            |path| {
                let mut decoder = decoder(path);
                let mut payload = results(0, &[]);
                payload[4..8].copy_from_slice(&3u32.to_be_bytes());
                assert!(decoder.parse_result(&payload, None).is_err());
                decoder.close().unwrap();
            },
        );
    }

    #[test]
    fn mediacodec_incomplete_exchange_invalidates_socket_and_cannot_close_ack() {
        server(
            |mut stream| {
                opened(&mut stream);
                assert_eq!(request(&mut stream).0, POLL);
                stream.write_all(&MAGIC.to_be_bytes()).unwrap();
            },
            |path| {
                let mut decoder = decoder(path);
                assert!(decoder.poll().is_err());
                assert!(decoder.wire.stream.is_none());
                assert!(decoder.close().is_err());
            },
        );
    }

    #[test]
    fn mediacodec_partial_progress_does_not_reset_absolute_deadline() {
        server(
            |mut stream| {
                assert_eq!(request(&mut stream).0, PROBE);
                for byte in MAGIC.to_be_bytes() {
                    if stream.write_all(&[byte]).is_err() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            },
            |path| {
                let mut wire = Wire::connect(path, Instant::now() + OPEN_TIMEOUT).unwrap();
                let start = Instant::now();
                assert!(
                    wire.exchange(
                        PROBE,
                        &[],
                        CAPABILITY,
                        268,
                        start + Duration::from_millis(45)
                    )
                    .is_err()
                );
                assert!(start.elapsed() < Duration::from_millis(150));
                assert!(wire.stream.is_none());
            },
        );
    }

    #[test]
    fn mediacodec_close_eof_does_not_claim_codec_release() {
        server(
            |mut stream| {
                opened(&mut stream);
                assert_eq!(request(&mut stream).0, CLOSE);
            },
            |path| {
                let mut decoder = decoder(path);
                assert!(decoder.close().is_err());
            },
        );
    }

    #[test]
    fn mediacodec_rejects_unsupported_format_without_connecting() {
        let format = StreamFormat::video_default(2, 2).unwrap();
        assert!(
            MediaCodecDecoder::open(VideoCodec::H265, format, Some(Path::new("/not-a-socket")))
                .is_err()
        );
        let mut format = format;
        format.color_matrix = ColorMatrix::Bt601;
        assert!(
            MediaCodecDecoder::open(VideoCodec::H264, format, Some(Path::new("/not-a-socket")))
                .is_err()
        );
    }

    #[test]
    fn mediacodec_retry_outputs_are_bounded_by_existing_ledger_plus_new_input() {
        server(
            |mut stream| {
                opened(&mut stream);
                closed(&mut stream);
            },
            |path| {
                let mut decoder = decoder(path);
                for pts in 0..MAX_PENDING {
                    decoder.pending.insert(pts as u64, Instant::now());
                }
                let mut outputs = Vec::new();
                for pts in (0..MAX_PENDING).step_by(2) {
                    outputs.extend(
                        decoder
                            .parse_result(&results(0, &[pts as u64, pts as u64 + 1]), None)
                            .unwrap()
                            .1,
                    );
                }
                outputs.extend(
                    decoder
                        .parse_result(&results(0, &[100]), Some((100, Instant::now())))
                        .unwrap()
                        .1,
                );
                assert_eq!(outputs.len(), MAX_PENDING + 1);
                assert!(decoder.pending.is_empty());
                decoder.close().unwrap();
            },
        );
    }

    #[test]
    fn mediacodec_full_unix_listener_backlog_fails_without_blocking_or_spinning() {
        let path = std::env::temp_dir().join(format!(
            "opennow-mc-full-{}-{}",
            std::process::id(),
            NEXT_SOCKET.fetch_add(1, Ordering::Relaxed)
        ));
        let listener = UnixListener::bind(&path).unwrap();
        assert_eq!(unsafe { libc::listen(listener.as_raw_fd(), 0) }, 0);
        let queued = UnixStream::connect(&path).unwrap();
        let start = Instant::now();
        assert!(Wire::connect(&path, start + Duration::from_millis(50)).is_err());
        assert!(start.elapsed() < Duration::from_millis(100));
        drop(queued);
        drop(listener);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn mediacodec_busy_budget_is_bounded_and_incomplete_exchange_is_not_reused() {
        server(
            |mut stream| {
                opened(&mut stream);
                let mut requests = 0;
                loop {
                    let mut header = [0; 16];
                    if stream.read_exact(&mut header).is_err() {
                        break;
                    }
                    let kind = u32::from_be_bytes(header[8..12].try_into().unwrap());
                    let mut payload =
                        vec![0; u32::from_be_bytes(header[12..16].try_into().unwrap()) as usize];
                    if stream.read_exact(&mut payload).is_err() {
                        break;
                    }
                    let (response_kind, payload) = match kind {
                        SUBMIT => (RESULT, results(2, &[])),
                        POLL => (RESULT, results(0, &[])),
                        CLOSE => (CLOSED, Vec::new()),
                        _ => panic!("unexpected request {kind}"),
                    };
                    let mut bytes = Vec::new();
                    for value in [MAGIC, VERSION, response_kind, payload.len() as u32] {
                        bytes.extend_from_slice(&value.to_be_bytes());
                    }
                    bytes.extend_from_slice(&payload);
                    if stream.write_all(&bytes).is_err() || kind == CLOSE {
                        break;
                    }
                    requests += 1;
                }
                assert!(requests < 1200);
            },
            |path| {
                let mut decoder = decoder(path);
                let start = Instant::now();
                assert!(matches!(
                    decoder.decode(&input(42)),
                    Err(Error::ReferenceLost { .. })
                ));
                assert!(start.elapsed() < Duration::from_millis(700));
                assert!(decoder.pending.is_empty());
                if decoder.wire.stream.is_some() {
                    decoder.close().unwrap();
                } else {
                    assert!(decoder.close().is_err());
                }
            },
        );
    }

    fn session(path: &Path) -> crate::LinuxSession {
        let mut config = crate::SessionConfig::new(StreamFormat::video_default(2, 2).unwrap());
        config.decoder_preference = crate::DecoderPreference::MediaCodecOnly;
        config.mediacodec_socket = Some(path.to_path_buf());
        config.audio = None;
        crate::LinuxSession::start(config).unwrap()
    }

    #[test]
    fn mediacodec_session_stop_exposes_missing_release_ack() {
        server(
            |mut stream| {
                opened(&mut stream);
                loop {
                    let (kind, _) = request(&mut stream);
                    match kind {
                        POLL => response(&mut stream, RESULT, &results(0, &[])),
                        DRAIN => response(&mut stream, RESULT, &results(1, &[])),
                        CLOSE => break,
                        _ => panic!("unexpected request {kind}"),
                    }
                }
            },
            |path| {
                let mut session = session(path);
                assert!(session.stop().is_err());
            },
        );
    }

    #[test]
    fn mediacodec_reconfiguration_waits_for_release_before_next_open() {
        listener_server(
            |listener| {
                for _ in 0..2 {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    opened(&mut stream);
                    loop {
                        let (kind, _) = request(&mut stream);
                        match kind {
                            POLL => response(&mut stream, RESULT, &results(0, &[])),
                            DRAIN => response(&mut stream, RESULT, &results(1, &[])),
                            CLOSE => {
                                response(&mut stream, CLOSED, &[]);
                                break;
                            }
                            _ => panic!("unexpected request {kind}"),
                        }
                    }
                }
            },
            |path| {
                let mut session = session(path);
                session
                    .reconfigure(StreamFormat::video_default(2, 2).unwrap())
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(2);
                loop {
                    if matches!(
                        session.try_recv_event(),
                        Some(crate::BackendEvent::FormatChanged(_))
                    ) {
                        break;
                    }
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(1));
                }
                session.stop().unwrap();
            },
        );
    }

    #[test]
    fn mediacodec_reconfiguration_close_failure_prevents_replacement() {
        listener_server(
            |listener| {
                let (mut stream, _) = listener.accept().unwrap();
                opened(&mut stream);
                loop {
                    let (kind, _) = request(&mut stream);
                    match kind {
                        POLL => response(&mut stream, RESULT, &results(0, &[])),
                        DRAIN => response(&mut stream, RESULT, &results(1, &[])),
                        CLOSE => break,
                        _ => panic!("unexpected request {kind}"),
                    }
                }
                drop(stream);
                listener.set_nonblocking(true).unwrap();
                std::thread::sleep(Duration::from_millis(100));
                assert!(
                    matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
                );
            },
            |path| {
                let mut session = session(path);
                session
                    .reconfigure(StreamFormat::video_default(2, 2).unwrap())
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(2);
                while session.state() != crate::LifecycleState::Failed {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert!(session.stop().is_err());
            },
        );
    }
}
