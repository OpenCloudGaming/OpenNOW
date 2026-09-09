use std::fs;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};

use super::v4l2::ffi::*;
use super::v4l2::{enum_formats, ioctl, open_device, set_format, zeroed};

const HEVC_SLICE: u32 = u32::from_le_bytes(*b"S265");
const SAND_NV12: [u32; 2] = [u32::from_le_bytes(*b"NC12"), u32::from_le_bytes(*b"Nc12")];
const MEDIA_IOC_REQUEST_ALLOC: vidioc::_IOC_TYPE =
    ((2_u64 << 30) | (4 << 16) | ((b'|' as u64) << 8) | 5) as vidioc::_IOC_TYPE;

pub(super) fn probe() -> std::result::Result<String, String> {
    for index in 0..64 {
        let path = PathBuf::from(format!("/dev/video{index}"));
        if let Ok(media) = inspect(&path) {
            return Ok(format!(
                "HEVC request decode with SAND128 NV12 via {} and {}",
                path.display(),
                media.display()
            ));
        }
    }
    Err(
        "no accessible HEVC_SLICE/SAND128 NV12 decoder with a linked media request device"
            .to_owned(),
    )
}

fn inspect(path: &Path) -> io::Result<PathBuf> {
    let video = open_device(path)?;
    let fd = video.as_raw_fd();
    let mut capability: v4l2_capability = zeroed();
    ioctl(fd, vidioc::VIDIOC_QUERYCAP, &mut capability)?;
    let caps = if capability.capabilities & V4L2_CAP_DEVICE_CAPS != 0 {
        capability.device_caps
    } else {
        capability.capabilities
    };
    if caps & V4L2_CAP_STREAMING == 0
        || caps & (V4L2_CAP_VIDEO_M2M | V4L2_CAP_VIDEO_M2M_MPLANE) == 0
    {
        return Err(io::ErrorKind::Unsupported.into());
    }
    let multiplanar = caps & V4L2_CAP_VIDEO_M2M_MPLANE != 0;
    let (output, capture) = if multiplanar {
        (
            v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE,
            v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
        )
    } else {
        (
            v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_OUTPUT,
            v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_CAPTURE,
        )
    };
    if !enum_formats(fd, output)?.contains(&HEVC_SLICE) {
        return Err(io::ErrorKind::Unsupported.into());
    }
    set_format(fd, output, HEVC_SLICE, 1920, 1080, Some(1024 * 1024))
        .map_err(|error| io::Error::other(error.to_string()))?;
    if !supports_capture(&enum_formats(fd, capture)?) {
        return Err(io::ErrorKind::Unsupported.into());
    }
    let video_name = path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    let parent = fs::canonicalize(
        Path::new("/sys/class/video4linux")
            .join(video_name)
            .join("device"),
    )?;
    for index in 0..64 {
        let media_name = format!("media{index}");
        let Ok(media_parent) = fs::canonicalize(
            Path::new("/sys/class/media")
                .join(&media_name)
                .join("device"),
        ) else {
            continue;
        };
        if media_parent != parent {
            continue;
        }
        let media_path = Path::new("/dev").join(media_name);
        let Ok(media) = open_device(&media_path) else {
            continue;
        };
        let mut request_fd: libc::c_int = -1;
        if ioctl(media.as_raw_fd(), MEDIA_IOC_REQUEST_ALLOC, &mut request_fd).is_ok()
            && request_fd >= 0
        {
            drop(unsafe { OwnedFd::from_raw_fd(request_fd) });
            return Ok(media_path);
        }
    }
    Err(io::ErrorKind::NotFound.into())
}

fn supports_capture(formats: &[u32]) -> bool {
    formats.iter().any(|format| SAND_NV12.contains(format))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_capture_requires_eight_bit_sand() {
        assert!(supports_capture(&[u32::from_le_bytes(*b"NC12")]));
        assert!(supports_capture(&[u32::from_le_bytes(*b"Nc12")]));
        assert!(!supports_capture(&[u32::from_le_bytes(*b"NC30")]));
        assert!(!supports_capture(&[u32::from_le_bytes(*b"Nc30")]));
        assert!(!supports_capture(&[u32::from_le_bytes(*b"NV12")]));
        assert!(!supports_capture(&[]));
    }
}
