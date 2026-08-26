use std::fs::{File, OpenOptions};
use std::io;
use std::mem;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::slice;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[path = "v4l2_ffi.rs"]
mod ffi;
use ffi::*;

use super::VideoDecoder;
use crate::{
    ChromaLocation, ColorMatrix, ColorRange, DecodedVideoFrame, EncodedVideoFrame, Error,
    FramePlane, PixelFormat, Result, StreamFormat, Subsystem,
};

const OUTPUT_BUFFER_COUNT: u32 = 6;
const CAPTURE_BUFFER_COUNT: u32 = 8;
const MAX_VIDEO_DEVICES: usize = 64;
const MAX_PLANES: usize = VIDEO_MAX_PLANES as usize;
const H264: u32 = fourcc(*b"H264");
const NV12: u32 = fourcc(*b"NV12");
const NV12M: u32 = fourcc(*b"NM12");
const YUV420: u32 = fourcc(*b"YU12");
const YUV420M: u32 = fourcc(*b"YM12");
const IOC_WRITE: u64 = 1;
const IOC_READ: u64 = 2;
const VIDIOC_DQEVENT: vidioc::_IOC_TYPE =
    ioctl_code(IOC_READ, 89, mem::size_of::<v4l2_event>() as u64);
const VIDIOC_SUBSCRIBE_EVENT: vidioc::_IOC_TYPE = ioctl_code(
    IOC_WRITE,
    90,
    mem::size_of::<v4l2_event_subscription>() as u64,
);

const fn fourcc(bytes: [u8; 4]) -> u32 {
    bytes[0] as u32
        | ((bytes[1] as u32) << 8)
        | ((bytes[2] as u32) << 16)
        | ((bytes[3] as u32) << 24)
}

const fn ioctl_code(direction: u64, number: u64, size: u64) -> vidioc::_IOC_TYPE {
    ((direction << 30) | ((b'V' as u64) << 8) | number | (size << 16)) as vidioc::_IOC_TYPE
}

#[derive(Debug, Clone)]
pub(crate) struct V4l2DeviceInfo {
    pub path: PathBuf,
    pub driver: String,
    pub card: String,
    pub multiplanar: bool,
    pub capture_fourcc: u32,
}

impl V4l2DeviceInfo {
    pub fn description(&self) -> String {
        format!(
            "{} ({}, {}, {})",
            self.path.display(),
            self.driver,
            self.card,
            fourcc_name(self.capture_fourcc)
        )
    }
}

#[derive(Debug)]
struct MappedPlane {
    address: NonNull<u8>,
    length: usize,
}

impl MappedPlane {
    fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.address.as_ptr(), self.length) }
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.address.as_ptr(), self.length) }
    }
}

impl Drop for MappedPlane {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.address.as_ptr().cast(), self.length);
        }
    }
}

#[derive(Debug)]
struct QueueBuffer {
    planes: Vec<MappedPlane>,
    queued: bool,
}

pub(crate) struct V4l2Decoder {
    file: File,
    device: V4l2DeviceInfo,
    format: StreamFormat,
    output_type: u32,
    capture_type: u32,
    output: Vec<QueueBuffer>,
    capture: Vec<QueueBuffer>,
    output_streaming: bool,
    capture_streaming: bool,
    capture_format: Option<NegotiatedFormat>,
    pending_source_change: bool,
    capture_last_seen: bool,
    capture_frames_seen: u64,
    pending_format_change: Option<StreamFormat>,
}

impl V4l2Decoder {
    pub fn open(format: StreamFormat, requested: Option<PathBuf>) -> Result<Self> {
        format.validate()?;
        let devices = probe_v4l2_devices();
        let device = match requested {
            Some(path) => devices
                .into_iter()
                .find(|device| device.path == path)
                .ok_or_else(|| {
                    Error::unavailable(
                        Subsystem::V4l2,
                        format!("{} is not a usable stateful H.264 decoder", path.display()),
                    )
                })?,
            None => devices.into_iter().next().ok_or_else(|| {
                Error::unavailable(
                    Subsystem::V4l2,
                    "no /dev/video* node advertises stateful H.264 M2M decode with NV12/I420 output",
                )
            })?,
        };
        let file = open_device(&device.path).map_err(|error| Error::io(Subsystem::V4l2, error))?;
        let output_type = if device.multiplanar {
            v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE
        } else {
            v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_OUTPUT
        };
        let capture_type = if device.multiplanar {
            v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE
        } else {
            v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_CAPTURE
        };
        let mut decoder = Self {
            file,
            device,
            format,
            output_type,
            capture_type,
            output: Vec::new(),
            capture: Vec::new(),
            output_streaming: false,
            capture_streaming: false,
            capture_format: None,
            pending_source_change: false,
            capture_last_seen: false,
            capture_frames_seen: 0,
            pending_format_change: None,
        };
        decoder.configure()?;
        Ok(decoder)
    }

    fn configure(&mut self) -> Result<()> {
        let mut subscription: v4l2_event_subscription = zeroed();
        subscription.type_ = V4L2_EVENT_SOURCE_CHANGE;
        if let Err(error) = ioctl(
            self.file.as_raw_fd(),
            VIDIOC_SUBSCRIBE_EVENT,
            &mut subscription,
        ) {
            if error.raw_os_error() != Some(libc::EINVAL) {
                return Err(Error::io(Subsystem::V4l2, error));
            }
        }
        set_format(
            self.file.as_raw_fd(),
            self.output_type,
            H264,
            self.format.width,
            self.format.height,
            Some(compressed_buffer_size(
                self.format.width,
                self.format.height,
            )),
        )?;
        let mut negotiated = set_format(
            self.file.as_raw_fd(),
            self.capture_type,
            self.device.capture_fourcc,
            self.format.width,
            self.format.height,
            None,
        )?;
        apply_visible_selection(self.file.as_raw_fd(), self.capture_type, &mut negotiated);
        self.apply_negotiated_format(&negotiated)?;

        self.output =
            request_and_map_buffers(self.file.as_raw_fd(), self.output_type, OUTPUT_BUFFER_COUNT)?;
        self.capture = request_and_map_buffers(
            self.file.as_raw_fd(),
            self.capture_type,
            CAPTURE_BUFFER_COUNT,
        )?;
        for index in 0..self.capture.len() {
            queue_buffer(
                self.file.as_raw_fd(),
                self.capture_type,
                index,
                &self.capture[index],
                None,
                0,
            )?;
            self.capture[index].queued = true;
        }
        stream_on(self.file.as_raw_fd(), self.capture_type)?;
        self.capture_streaming = true;
        stream_on(self.file.as_raw_fd(), self.output_type)?;
        self.output_streaming = true;
        Ok(())
    }

    fn apply_negotiated_format(&mut self, negotiated: &NegotiatedFormat) -> Result<()> {
        if negotiated.width == 0
            || negotiated.height == 0
            || negotiated.width > 16_384
            || negotiated.height > 16_384
            || negotiated.visible_left + negotiated.visible_width > negotiated.width
            || negotiated.visible_top + negotiated.visible_height > negotiated.height
        {
            return Err(Error::InvalidFormat(
                "V4L2 returned invalid coded or visible dimensions".to_owned(),
            ));
        }
        let previous = self.format;
        self.format.width = negotiated.visible_width;
        self.format.height = negotiated.visible_height;
        self.format.pixel_format = match negotiated.fourcc {
            NV12 | NV12M => PixelFormat::Nv12,
            YUV420 | YUV420M => PixelFormat::I420,
            other => {
                return Err(Error::InvalidFormat(format!(
                    "V4L2 selected unsupported capture format {}",
                    fourcc_name(other)
                )));
            }
        };
        self.format.color_matrix = negotiated.color_matrix;
        self.format.color_range = negotiated.color_range;
        self.format.validate()?;
        self.capture_format = Some(negotiated.clone());
        if self.format != previous {
            self.pending_format_change = Some(self.format);
        }
        Ok(())
    }

    fn submit(&mut self, frame: &EncodedVideoFrame) -> Result<Vec<DecodedVideoFrame>> {
        let deadline = Instant::now() + Duration::from_millis(100);
        let mut ready = Vec::new();
        let index = loop {
            self.drain_output()?;
            ready.extend(self.collect_frames()?);
            if let Some(index) = self.output.iter().position(|buffer| !buffer.queued) {
                break index;
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(Error::backend(
                    Subsystem::V4l2,
                    "decoder did not return an output buffer within 100ms",
                ));
            };
            self.wait_for_progress(remaining)?;
        };
        let plane = self.output[index]
            .planes
            .first_mut()
            .ok_or_else(|| Error::backend(Subsystem::V4l2, "output buffer has no planes"))?;
        if frame.data.len() > plane.length {
            return Err(Error::InvalidFormat(format!(
                "H.264 access unit is {} bytes but the V4L2 buffer is {} bytes",
                frame.data.len(),
                plane.length
            )));
        }
        plane.as_mut_slice()[..frame.data.len()].copy_from_slice(&frame.data);
        queue_buffer(
            self.file.as_raw_fd(),
            self.output_type,
            index,
            &self.output[index],
            Some(frame.data.len()),
            frame.timestamp_us,
        )?;
        self.output[index].queued = true;
        Ok(ready)
    }

    fn collect_frames(&mut self) -> Result<Vec<DecodedVideoFrame>> {
        let mut frames = Vec::new();
        loop {
            match dequeue_buffer(
                self.file.as_raw_fd(),
                self.capture_type,
                self.capture.first().map_or(1, |buffer| buffer.planes.len()),
            ) {
                Ok(dequeued) => {
                    let index = dequeued.index;
                    if index >= self.capture.len() {
                        return Err(Error::backend(
                            Subsystem::V4l2,
                            format!("driver dequeued invalid capture buffer {index}"),
                        ));
                    }
                    self.capture[index].queued = false;
                    let is_last = dequeued.flags & V4L2_BUF_FLAG_LAST != 0;
                    let is_error = dequeued.flags & V4L2_BUF_FLAG_ERROR != 0;
                    let has_data = dequeued.bytes_used.iter().any(|used| *used > 0);
                    if !is_error && has_data {
                        let geometry = self.capture_format.as_ref().ok_or_else(|| {
                            Error::backend(Subsystem::V4l2, "capture format is unavailable")
                        })?;
                        frames.push(copy_capture_frame(
                            &self.capture[index],
                            &dequeued,
                            self.format,
                            geometry,
                        )?);
                        self.capture_frames_seen = self.capture_frames_seen.saturating_add(1);
                    }
                    if is_last {
                        self.capture_last_seen = true;
                    } else {
                        queue_buffer(
                            self.file.as_raw_fd(),
                            self.capture_type,
                            index,
                            &self.capture[index],
                            None,
                            0,
                        )?;
                        self.capture[index].queued = true;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(Error::io(Subsystem::V4l2, error)),
            }
        }
        if self.pending_source_change && self.capture_last_seen {
            self.reconfigure_capture()?;
        }
        Ok(frames)
    }

    fn drain_output(&mut self) -> Result<()> {
        loop {
            match dequeue_buffer(
                self.file.as_raw_fd(),
                self.output_type,
                self.output.first().map_or(1, |buffer| buffer.planes.len()),
            ) {
                Ok(dequeued) => {
                    let buffer = self.output.get_mut(dequeued.index).ok_or_else(|| {
                        Error::backend(
                            Subsystem::V4l2,
                            format!("driver dequeued invalid output buffer {}", dequeued.index),
                        )
                    })?;
                    buffer.queued = false;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(error) => return Err(Error::io(Subsystem::V4l2, error)),
            }
        }
    }

    fn wait_for_progress(&mut self, timeout: Duration) -> Result<()> {
        let mut descriptor = libc::pollfd {
            fd: self.file.as_raw_fd(),
            events: libc::POLLIN | libc::POLLOUT | libc::POLLPRI,
            revents: 0,
        };
        let result = unsafe {
            libc::poll(
                &mut descriptor,
                1,
                timeout.as_millis().min(i32::MAX as u128) as i32,
            )
        };
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                return Ok(());
            }
            return Err(Error::io(Subsystem::V4l2, error));
        }
        if descriptor.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err(Error::DeviceLost {
                subsystem: Subsystem::V4l2,
                reason: format!("decoder poll failed with revents={:#x}", descriptor.revents),
            });
        }
        if result > 0 && descriptor.revents & libc::POLLPRI != 0 {
            self.handle_events()?;
        }
        Ok(())
    }

    fn handle_events(&mut self) -> Result<()> {
        loop {
            let mut event: v4l2_event = zeroed();
            match ioctl(self.file.as_raw_fd(), VIDIOC_DQEVENT, &mut event) {
                Ok(()) => {
                    if event.type_ != V4L2_EVENT_SOURCE_CHANGE {
                        continue;
                    }
                    let changes = unsafe { event.u.src_change.changes };
                    if changes & V4L2_EVENT_SRC_CH_RESOLUTION != 0 {
                        if self.capture_frames_seen == 0 {
                            self.reconfigure_capture()?;
                        } else {
                            self.pending_source_change = true;
                            self.capture_last_seen = false;
                        }
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(error) => return Err(Error::io(Subsystem::V4l2, error)),
            }
        }
    }

    fn reconfigure_capture(&mut self) -> Result<()> {
        if self.capture_streaming {
            stream_off(self.file.as_raw_fd(), self.capture_type)?;
            self.capture_streaming = false;
        }
        self.capture.clear();
        release_buffers(self.file.as_raw_fd(), self.capture_type)?;
        let mut negotiated = get_format(self.file.as_raw_fd(), self.capture_type)?;
        apply_visible_selection(self.file.as_raw_fd(), self.capture_type, &mut negotiated);
        self.apply_negotiated_format(&negotiated)?;
        self.capture = request_and_map_buffers(
            self.file.as_raw_fd(),
            self.capture_type,
            CAPTURE_BUFFER_COUNT,
        )?;
        for index in 0..self.capture.len() {
            queue_buffer(
                self.file.as_raw_fd(),
                self.capture_type,
                index,
                &self.capture[index],
                None,
                0,
            )?;
            self.capture[index].queued = true;
        }
        stream_on(self.file.as_raw_fd(), self.capture_type)?;
        self.capture_streaming = true;
        self.pending_source_change = false;
        self.capture_last_seen = false;
        self.capture_frames_seen = 0;
        Ok(())
    }

    fn shutdown(&mut self) {
        if self.output_streaming {
            let _ = stream_off(self.file.as_raw_fd(), self.output_type);
            self.output_streaming = false;
        }
        if self.capture_streaming {
            let _ = stream_off(self.file.as_raw_fd(), self.capture_type);
            self.capture_streaming = false;
        }
        self.output.clear();
        self.capture.clear();
        let _ = release_buffers(self.file.as_raw_fd(), self.output_type);
        let _ = release_buffers(self.file.as_raw_fd(), self.capture_type);
    }
}

impl VideoDecoder for V4l2Decoder {
    fn decode(&mut self, frame: &EncodedVideoFrame) -> Result<Vec<DecodedVideoFrame>> {
        let mut frames = self.submit(frame)?;
        self.wait_for_progress(Duration::from_millis(1))?;
        self.drain_output()?;
        frames.extend(self.collect_frames()?);
        Ok(frames)
    }

    fn flush(&mut self) -> Result<Vec<DecodedVideoFrame>> {
        let mut command: v4l2_decoder_cmd = zeroed();
        command.cmd = V4L2_DEC_CMD_STOP;
        self.pending_source_change = false;
        self.capture_last_seen = false;
        let command_supported = match ioctl(
            self.file.as_raw_fd(),
            vidioc::VIDIOC_DECODER_CMD,
            &mut command,
        ) {
            Ok(()) => true,
            Err(error) if matches!(error.raw_os_error(), Some(libc::EINVAL | libc::ENOTTY)) => {
                false
            }
            Err(error) => return Err(Error::io(Subsystem::V4l2, error)),
        };
        let deadline = Instant::now() + Duration::from_millis(250);
        let mut last_frame = Instant::now();
        let mut frames = Vec::new();
        while Instant::now() < deadline {
            self.wait_for_progress(Duration::from_millis(10))?;
            self.drain_output()?;
            let ready = self.collect_frames()?;
            if !ready.is_empty() {
                last_frame = Instant::now();
                frames.extend(ready);
            }
            let output_idle = !self.output.iter().any(|buffer| buffer.queued);
            if output_idle
                && ((command_supported && self.capture_last_seen)
                    || (!command_supported && last_frame.elapsed() >= Duration::from_millis(30)))
            {
                break;
            }
        }
        Ok(frames)
    }

    fn take_format_change(&mut self) -> Option<StreamFormat> {
        self.pending_format_change.take()
    }
}

impl Drop for V4l2Decoder {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub(crate) fn probe_v4l2_devices() -> Vec<V4l2DeviceInfo> {
    (0..MAX_VIDEO_DEVICES)
        .filter_map(|index| inspect_device(PathBuf::from(format!("/dev/video{index}"))).ok())
        .collect()
}

fn inspect_device(path: PathBuf) -> io::Result<V4l2DeviceInfo> {
    let file = open_device(&path)?;
    let fd = file.as_raw_fd();
    let mut capability: v4l2_capability = zeroed();
    ioctl(fd, vidioc::VIDIOC_QUERYCAP, &mut capability)?;
    let caps = if capability.capabilities & V4L2_CAP_DEVICE_CAPS != 0 {
        capability.device_caps
    } else {
        capability.capabilities
    };
    if caps & V4L2_CAP_STREAMING == 0 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "device does not support streaming I/O",
        ));
    }
    let multiplanar = caps & V4L2_CAP_VIDEO_M2M_MPLANE != 0;
    if !multiplanar && caps & V4L2_CAP_VIDEO_M2M == 0 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "device is not a V4L2 M2M decoder",
        ));
    }
    let output_type = if multiplanar {
        v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE
    } else {
        v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_OUTPUT
    };
    let capture_type = if multiplanar {
        v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE
    } else {
        v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_CAPTURE
    };
    if !enum_formats(fd, output_type)?.contains(&H264) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "device does not accept H.264",
        ));
    }
    let capture = enum_formats(fd, capture_type)?;
    let capture_fourcc = [NV12, NV12M, YUV420, YUV420M]
        .into_iter()
        .find(|format| capture.contains(format))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Unsupported,
                "device does not output NV12 or I420",
            )
        })?;
    Ok(V4l2DeviceInfo {
        path,
        driver: c_string(&capability.driver),
        card: c_string(&capability.card),
        multiplanar,
        capture_fourcc,
    })
}

fn open_device(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
}

fn enum_formats(fd: RawFd, buffer_type: u32) -> io::Result<Vec<u32>> {
    let mut formats = Vec::new();
    for index in 0..256 {
        let mut description: v4l2_fmtdesc = zeroed();
        description.index = index;
        description.type_ = buffer_type;
        match ioctl(fd, vidioc::VIDIOC_ENUM_FMT, &mut description) {
            Ok(()) => formats.push(description.pixelformat),
            Err(error) if error.raw_os_error() == Some(libc::EINVAL) => break,
            Err(error) => return Err(error),
        }
    }
    Ok(formats)
}

#[derive(Debug, Clone)]
struct PlaneGeometry {
    stride: usize,
    size: usize,
}

#[derive(Debug, Clone)]
struct NegotiatedFormat {
    width: u32,
    height: u32,
    visible_left: u32,
    visible_top: u32,
    visible_width: u32,
    visible_height: u32,
    fourcc: u32,
    planes: Vec<PlaneGeometry>,
    color_matrix: ColorMatrix,
    color_range: ColorRange,
}

fn set_format(
    fd: RawFd,
    buffer_type: u32,
    pixel_format: u32,
    width: u32,
    height: u32,
    size_image: Option<u32>,
) -> Result<NegotiatedFormat> {
    let mut format: v4l2_format = zeroed();
    format.type_ = buffer_type;
    if is_multiplanar(buffer_type) {
        let mut pixel: v4l2_pix_format_mplane = zeroed();
        pixel.width = width;
        pixel.height = height;
        pixel.pixelformat = pixel_format;
        pixel.field = v4l2_field_V4L2_FIELD_NONE;
        pixel.colorspace = v4l2_colorspace_V4L2_COLORSPACE_REC709;
        pixel.num_planes = match pixel_format {
            NV12M => 2,
            YUV420M => 3,
            _ => 1,
        };
        if let Some(size_image) = size_image {
            pixel.plane_fmt[0].sizeimage = size_image;
        }
        format.fmt.pix_mp = pixel;
    } else {
        let mut pixel: v4l2_pix_format = zeroed();
        pixel.width = width;
        pixel.height = height;
        pixel.pixelformat = pixel_format;
        pixel.field = v4l2_field_V4L2_FIELD_NONE;
        pixel.colorspace = v4l2_colorspace_V4L2_COLORSPACE_REC709;
        if let Some(size_image) = size_image {
            pixel.sizeimage = size_image;
        }
        format.fmt.pix = pixel;
    }
    ioctl(fd, vidioc::VIDIOC_S_FMT, &mut format)
        .map_err(|error| Error::io(Subsystem::V4l2, error))?;

    negotiated_format(&format)
}

fn get_format(fd: RawFd, buffer_type: u32) -> Result<NegotiatedFormat> {
    let mut format: v4l2_format = zeroed();
    format.type_ = buffer_type;
    ioctl(fd, vidioc::VIDIOC_G_FMT, &mut format)
        .map_err(|error| Error::io(Subsystem::V4l2, error))?;
    negotiated_format(&format)
}

fn negotiated_format(format: &v4l2_format) -> Result<NegotiatedFormat> {
    let (width, height, fourcc, colorspace, quantization, planes) = unsafe {
        if is_multiplanar(format.type_) {
            let pixel = format.fmt.pix_mp;
            let plane_count = pixel.num_planes as usize;
            if plane_count == 0 || plane_count > MAX_PLANES {
                return Err(Error::InvalidFormat(format!(
                    "V4L2 returned invalid plane count {plane_count}"
                )));
            }
            let plane_formats = pixel.plane_fmt;
            let planes = plane_formats[..plane_count]
                .iter()
                .enumerate()
                .map(|(index, plane)| PlaneGeometry {
                    stride: (plane.bytesperline as usize).max(match (pixel.pixelformat, index) {
                        (YUV420M, 1 | 2) => pixel.width as usize / 2,
                        _ => pixel.width as usize,
                    }),
                    size: plane.sizeimage as usize,
                })
                .collect();
            (
                pixel.width,
                pixel.height,
                pixel.pixelformat,
                pixel.colorspace,
                pixel.quantization as u32,
                planes,
            )
        } else {
            let pixel = format.fmt.pix;
            (
                pixel.width,
                pixel.height,
                pixel.pixelformat,
                pixel.colorspace,
                pixel.quantization,
                vec![PlaneGeometry {
                    stride: (pixel.bytesperline as usize).max(pixel.width as usize),
                    size: pixel.sizeimage as usize,
                }],
            )
        }
    };
    Ok(NegotiatedFormat {
        width,
        height,
        visible_left: 0,
        visible_top: 0,
        visible_width: width,
        visible_height: height,
        fourcc,
        planes,
        color_matrix: if colorspace == v4l2_colorspace_V4L2_COLORSPACE_BT2020 {
            ColorMatrix::Bt2020
        } else if colorspace == v4l2_colorspace_V4L2_COLORSPACE_REC709 {
            ColorMatrix::Bt709
        } else {
            ColorMatrix::Bt601
        },
        color_range: if quantization == v4l2_quantization_V4L2_QUANTIZATION_FULL_RANGE {
            ColorRange::Full
        } else {
            ColorRange::Limited
        },
    })
}

fn apply_visible_selection(fd: RawFd, buffer_type: u32, format: &mut NegotiatedFormat) {
    let mut selection: v4l2_selection = zeroed();
    selection.type_ = buffer_type;
    selection.target = V4L2_SEL_TGT_COMPOSE;
    if ioctl(fd, vidioc::VIDIOC_G_SELECTION, &mut selection).is_ok()
        && selection.r.left >= 0
        && selection.r.top >= 0
        && selection.r.width > 0
        && selection.r.height > 0
    {
        let left = selection.r.left as u32;
        let top = selection.r.top as u32;
        if left < format.width && top < format.height {
            format.visible_left = left;
            format.visible_top = top;
            format.visible_width = selection.r.width.min(format.width - left);
            format.visible_height = selection.r.height.min(format.height - top);
        }
    }
}

fn request_and_map_buffers(fd: RawFd, buffer_type: u32, count: u32) -> Result<Vec<QueueBuffer>> {
    let mut request: v4l2_requestbuffers = zeroed();
    request.count = count;
    request.type_ = buffer_type;
    request.memory = v4l2_memory_V4L2_MEMORY_MMAP;
    ioctl(fd, vidioc::VIDIOC_REQBUFS, &mut request)
        .map_err(|error| Error::io(Subsystem::V4l2, error))?;
    if request.count < 2 {
        return Err(Error::backend(
            Subsystem::V4l2,
            format!("driver allocated only {} MMAP buffers", request.count),
        ));
    }
    (0..request.count)
        .map(|index| map_buffer(fd, buffer_type, index as usize))
        .collect()
}

fn map_buffer(fd: RawFd, buffer_type: u32, index: usize) -> Result<QueueBuffer> {
    let mut planes = [zeroed::<v4l2_plane>(); MAX_PLANES];
    let mut buffer: v4l2_buffer = zeroed();
    buffer.index = index as u32;
    buffer.type_ = buffer_type;
    buffer.memory = v4l2_memory_V4L2_MEMORY_MMAP;
    if is_multiplanar(buffer_type) {
        buffer.length = MAX_PLANES as u32;
        buffer.m.planes = planes.as_mut_ptr();
    }
    ioctl(fd, vidioc::VIDIOC_QUERYBUF, &mut buffer)
        .map_err(|error| Error::io(Subsystem::V4l2, error))?;
    let plane_count = if is_multiplanar(buffer_type) {
        buffer.length as usize
    } else {
        1
    };
    let mut mappings = Vec::with_capacity(plane_count);
    for plane in planes.iter().take(plane_count) {
        let (length, offset) = unsafe {
            if is_multiplanar(buffer_type) {
                (plane.length as usize, plane.m.mem_offset as libc::off_t)
            } else {
                (buffer.length as usize, buffer.m.offset as libc::off_t)
            }
        };
        let address = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                offset,
            )
        };
        if address == libc::MAP_FAILED {
            return Err(Error::io(Subsystem::V4l2, io::Error::last_os_error()));
        }
        let Some(address) = NonNull::new(address.cast()) else {
            unsafe { libc::munmap(address, length) };
            return Err(Error::backend(
                Subsystem::V4l2,
                "MMAP returned a null address",
            ));
        };
        mappings.push(MappedPlane { address, length });
    }
    Ok(QueueBuffer {
        planes: mappings,
        queued: false,
    })
}

fn queue_buffer(
    fd: RawFd,
    buffer_type: u32,
    index: usize,
    mapped: &QueueBuffer,
    bytes_used: Option<usize>,
    timestamp_us: u64,
) -> Result<()> {
    let mut planes = [zeroed::<v4l2_plane>(); MAX_PLANES];
    let mut buffer: v4l2_buffer = zeroed();
    buffer.index = index as u32;
    buffer.type_ = buffer_type;
    buffer.memory = v4l2_memory_V4L2_MEMORY_MMAP;
    buffer.timestamp.tv_sec = (timestamp_us / 1_000_000) as libc::time_t;
    buffer.timestamp.tv_usec = (timestamp_us % 1_000_000) as libc::suseconds_t;
    if is_multiplanar(buffer_type) {
        for (plane, mapping) in planes.iter_mut().zip(&mapped.planes) {
            plane.length = mapping.length as u32;
        }
        if let Some(bytes_used) = bytes_used {
            planes[0].bytesused = bytes_used as u32;
        }
        buffer.length = mapped.planes.len() as u32;
        buffer.m.planes = planes.as_mut_ptr();
    } else {
        buffer.length = mapped.planes[0].length as u32;
        buffer.bytesused = bytes_used.unwrap_or(0) as u32;
    }
    ioctl(fd, vidioc::VIDIOC_QBUF, &mut buffer).map_err(|error| Error::io(Subsystem::V4l2, error))
}

#[derive(Debug)]
struct DequeuedBuffer {
    index: usize,
    flags: u32,
    timestamp_us: u64,
    bytes_used: Vec<usize>,
    data_offsets: Vec<usize>,
}

fn dequeue_buffer(fd: RawFd, buffer_type: u32, plane_count: usize) -> io::Result<DequeuedBuffer> {
    let mut planes = [zeroed::<v4l2_plane>(); MAX_PLANES];
    let mut buffer: v4l2_buffer = zeroed();
    buffer.type_ = buffer_type;
    buffer.memory = v4l2_memory_V4L2_MEMORY_MMAP;
    if is_multiplanar(buffer_type) {
        buffer.length = plane_count as u32;
        buffer.m.planes = planes.as_mut_ptr();
    }
    ioctl(fd, vidioc::VIDIOC_DQBUF, &mut buffer)?;
    let (bytes_used, data_offsets) = if is_multiplanar(buffer_type) {
        (
            planes[..buffer.length as usize]
                .iter()
                .map(|plane| plane.bytesused as usize)
                .collect(),
            planes[..buffer.length as usize]
                .iter()
                .map(|plane| plane.data_offset as usize)
                .collect(),
        )
    } else {
        (vec![buffer.bytesused as usize], vec![0])
    };
    Ok(DequeuedBuffer {
        index: buffer.index as usize,
        flags: buffer.flags,
        timestamp_us: (buffer.timestamp.tv_sec as u64)
            .saturating_mul(1_000_000)
            .saturating_add(buffer.timestamp.tv_usec.max(0) as u64),
        bytes_used,
        data_offsets,
    })
}

fn copy_capture_frame(
    mapped: &QueueBuffer,
    dequeued: &DequeuedBuffer,
    format: StreamFormat,
    geometry: &NegotiatedFormat,
) -> Result<DecodedVideoFrame> {
    let width = format.width as usize;
    let height = format.height as usize;
    let coded_height = geometry.height as usize;
    let left = geometry.visible_left as usize;
    let top = geometry.visible_top as usize;
    let mut planes = Vec::new();
    match (format.pixel_format, mapped.planes.len()) {
        (PixelFormat::Nv12, 1) => {
            let source = capture_plane(mapped, dequeued, geometry, 0)?;
            let stride = geometry.planes[0].stride;
            let uv_offset = stride * coded_height;
            planes.push(FramePlane {
                data: copy_rows(source, top * stride + left, stride, width, height)?,
                stride: width,
                rows: height,
            });
            planes.push(FramePlane {
                data: copy_rows(
                    source,
                    uv_offset + (top / 2) * stride + (left / 2) * 2,
                    stride,
                    width,
                    height / 2,
                )?,
                stride: width,
                rows: height / 2,
            });
        }
        (PixelFormat::Nv12, _) => {
            for (index, rows) in [height, height / 2].into_iter().enumerate() {
                let source = capture_plane(mapped, dequeued, geometry, index)?;
                let offset = if index == 0 {
                    top * geometry.planes[index].stride + left
                } else {
                    (top / 2) * geometry.planes[index].stride + (left / 2) * 2
                };
                planes.push(FramePlane {
                    data: copy_rows(source, offset, geometry.planes[index].stride, width, rows)?,
                    stride: width,
                    rows,
                });
            }
        }
        (PixelFormat::I420, 1) => {
            let source = capture_plane(mapped, dequeued, geometry, 0)?;
            let y_stride = geometry.planes[0].stride;
            let y_len = y_stride * coded_height;
            let chroma_stride = y_stride / 2;
            let chroma_len = chroma_stride * coded_height / 2;
            planes.push(FramePlane {
                data: copy_rows(source, top * y_stride + left, y_stride, width, height)?,
                stride: width,
                rows: height,
            });
            planes.push(FramePlane {
                data: copy_rows(
                    source,
                    y_len + (top / 2) * chroma_stride + left / 2,
                    chroma_stride,
                    width / 2,
                    height / 2,
                )?,
                stride: width / 2,
                rows: height / 2,
            });
            planes.push(FramePlane {
                data: copy_rows(
                    source,
                    y_len + chroma_len + (top / 2) * chroma_stride + left / 2,
                    chroma_stride,
                    width / 2,
                    height / 2,
                )?,
                stride: width / 2,
                rows: height / 2,
            });
        }
        (PixelFormat::I420, _) => {
            for (index, (rows, minimum_stride)) in [
                (height, width),
                (height / 2, width / 2),
                (height / 2, width / 2),
            ]
            .into_iter()
            .enumerate()
            {
                let source = capture_plane(mapped, dequeued, geometry, index)?;
                let offset = if index == 0 {
                    top * geometry.planes[index].stride + left
                } else {
                    (top / 2) * geometry.planes[index].stride + left / 2
                };
                planes.push(FramePlane {
                    data: copy_rows(
                        source,
                        offset,
                        geometry.planes[index].stride,
                        minimum_stride,
                        rows,
                    )?,
                    stride: minimum_stride,
                    rows,
                });
            }
        }
        _ => {
            return Err(Error::InvalidFormat(
                "V4L2 returned an unsupported frame layout".to_owned(),
            ));
        }
    }
    let frame = DecodedVideoFrame {
        format: StreamFormat {
            chroma_location: ChromaLocation::Left,
            ..format
        },
        planes,
        dmabuf: None,
        vulkan: None,
        overlay: None,
        timestamp_us: dequeued.timestamp_us,
    };
    frame.validate()?;
    Ok(frame)
}

fn valid_plane_slice<'a>(
    mapped: &'a QueueBuffer,
    dequeued: &DequeuedBuffer,
    index: usize,
) -> Result<&'a [u8]> {
    let mapping = mapped
        .planes
        .get(index)
        .ok_or_else(|| Error::InvalidFormat(format!("capture plane {index} is missing")))?;
    let offset = dequeued.data_offsets.get(index).copied().unwrap_or(0);
    let used = dequeued
        .bytes_used
        .get(index)
        .copied()
        .unwrap_or(0)
        .min(mapping.length);
    if offset > used {
        return Err(Error::InvalidFormat(format!(
            "capture plane {index} has an invalid data offset"
        )));
    }
    Ok(&mapping.as_slice()[offset..used])
}

fn capture_plane<'a>(
    mapped: &'a QueueBuffer,
    dequeued: &DequeuedBuffer,
    geometry: &NegotiatedFormat,
    index: usize,
) -> Result<&'a [u8]> {
    let source = valid_plane_slice(mapped, dequeued, index)?;
    let plane = geometry
        .planes
        .get(index)
        .ok_or_else(|| Error::InvalidFormat(format!("capture plane {index} has no geometry")))?;
    let length = if plane.size == 0 {
        source.len()
    } else {
        source.len().min(plane.size)
    };
    Ok(&source[..length])
}

fn copy_rows(
    source: &[u8],
    offset: usize,
    source_stride: usize,
    row_bytes: usize,
    rows: usize,
) -> Result<Arc<[u8]>> {
    if source_stride < row_bytes {
        return Err(Error::InvalidFormat(
            "capture plane stride is smaller than the visible row".to_owned(),
        ));
    }
    let required = offset
        .checked_add(source_stride.saturating_mul(rows.saturating_sub(1)))
        .and_then(|value| value.checked_add(row_bytes))
        .ok_or_else(|| Error::InvalidFormat("capture plane size overflow".to_owned()))?;
    if source.len() < required {
        return Err(Error::InvalidFormat(format!(
            "capture plane has {} bytes but geometry requires {required}",
            source.len()
        )));
    }
    let mut output = Vec::with_capacity(row_bytes * rows);
    for row in 0..rows {
        let start = offset + row * source_stride;
        output.extend_from_slice(&source[start..start + row_bytes]);
    }
    Ok(Arc::from(output))
}

fn stream_on(fd: RawFd, buffer_type: u32) -> Result<()> {
    let mut buffer_type = buffer_type as libc::c_int;
    ioctl(fd, vidioc::VIDIOC_STREAMON, &mut buffer_type)
        .map_err(|error| Error::io(Subsystem::V4l2, error))
}

fn stream_off(fd: RawFd, buffer_type: u32) -> Result<()> {
    let mut buffer_type = buffer_type as libc::c_int;
    ioctl(fd, vidioc::VIDIOC_STREAMOFF, &mut buffer_type)
        .map_err(|error| Error::io(Subsystem::V4l2, error))
}

fn release_buffers(fd: RawFd, buffer_type: u32) -> Result<()> {
    let mut request: v4l2_requestbuffers = zeroed();
    request.type_ = buffer_type;
    request.memory = v4l2_memory_V4L2_MEMORY_MMAP;
    ioctl(fd, vidioc::VIDIOC_REQBUFS, &mut request)
        .map_err(|error| Error::io(Subsystem::V4l2, error))
}

fn ioctl<T>(fd: RawFd, request: vidioc::_IOC_TYPE, value: &mut T) -> io::Result<()> {
    let result = unsafe { libc::ioctl(fd, request, (value as *mut T).cast::<libc::c_void>()) };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn zeroed<T>() -> T {
    unsafe { mem::zeroed() }
}

fn is_multiplanar(buffer_type: u32) -> bool {
    buffer_type == v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE
        || buffer_type == v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE
}

fn compressed_buffer_size(width: u32, height: u32) -> u32 {
    width
        .saturating_mul(height)
        .clamp(1024 * 1024, 16 * 1024 * 1024)
}

fn c_string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(
        &bytes[..bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len())],
    )
    .into_owned()
}

fn fourcc_name(value: u32) -> String {
    String::from_utf8_lossy(&value.to_le_bytes()).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_visible_rows_without_coded_padding() {
        let source = [1, 2, 3, 4, 90, 90, 5, 6, 7, 8, 90, 90];
        let copied = copy_rows(&source, 0, 6, 4, 2).unwrap();
        assert_eq!(&*copied, &[1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn copies_visible_crop_from_coded_plane() {
        let source = [90, 1, 2, 90, 90, 3, 4, 90];
        let copied = copy_rows(&source, 1, 4, 2, 2).unwrap();
        assert_eq!(&*copied, &[1, 2, 3, 4]);
    }

    #[test]
    fn rejects_capture_geometry_beyond_bytes_used() {
        let error = copy_rows(&[0; 7], 0, 4, 4, 2).unwrap_err();
        assert!(matches!(error, Error::InvalidFormat(_)));
    }
}
