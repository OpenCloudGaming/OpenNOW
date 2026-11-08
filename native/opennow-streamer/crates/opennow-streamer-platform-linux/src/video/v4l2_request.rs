use std::fs;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::v4l2::ffi::*;
use super::v4l2::{enum_formats, ioctl, open_device, set_format, zeroed};
use crate::VideoCodec;

const PRESENTABLE_CAPTURE: [u32; 3] = [
    u32::from_le_bytes(*b"NV12"),
    u32::from_le_bytes(*b"NC12"),
    u32::from_le_bytes(*b"Nc12"),
];
const MEDIA_IOC_REQUEST_ALLOC: vidioc::_IOC_TYPE =
    ((2_u64 << 30) | (4 << 16) | ((b'|' as u64) << 8) | 5) as vidioc::_IOC_TYPE;
const MEDIA_IOC_G_TOPOLOGY: vidioc::_IOC_TYPE =
    ((3_u64 << 30) | (72 << 16) | ((b'|' as u64) << 8) | 4) as vidioc::_IOC_TYPE;
const VIDIOC_ENUM_FRAMESIZES: vidioc::_IOC_TYPE =
    ((3_u64 << 30) | (44 << 16) | ((b'V' as u64) << 8) | 74) as vidioc::_IOC_TYPE;
const V4L2_FRMSIZE_TYPE_DISCRETE: u32 = 1;
const MEDIA_INTF_T_V4L_VIDEO: u32 = 0x200;
const MAX_DEVICE_NODES: usize = 256;
const MAX_INTERFACES: usize = 1024;
const MAX_FAILURES: usize = 4;

#[repr(C, packed)]
#[derive(Default)]
struct MediaTopology {
    topology_version: u64,
    num_entities: u32,
    reserved1: u32,
    ptr_entities: u64,
    num_interfaces: u32,
    reserved2: u32,
    ptr_interfaces: u64,
    num_pads: u32,
    reserved3: u32,
    ptr_pads: u64,
    num_links: u32,
    reserved4: u32,
    ptr_links: u64,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
struct MediaInterface {
    id: u32,
    intf_type: u32,
    flags: u32,
    reserved: [u32; 9],
    devnode: [u32; 16],
}

#[repr(C)]
#[derive(Default)]
struct FrameSizeEnum {
    index: u32,
    pixel_format: u32,
    kind: u32,
    sizes: [u32; 6],
    reserved: [u32; 2],
}

pub(super) struct RequestProbe {
    pub(super) detail: String,
    pub(super) max_dimensions: Option<[u32; 2]>,
}

struct RequestDevice {
    file: fs::File,
    output_type: u32,
    capture_type: u32,
}

const fn coded_format(codec: VideoCodec) -> u32 {
    match codec {
        VideoCodec::H264 => u32::from_le_bytes(*b"S264"),
        VideoCodec::H265 => u32::from_le_bytes(*b"S265"),
        VideoCodec::Av1 => u32::from_le_bytes(*b"AV1F"),
    }
}

fn fourcc(format: u32) -> String {
    String::from_utf8_lossy(&format.to_le_bytes()).into_owned()
}

pub(super) fn probe(
    codec: VideoCodec,
    width: u32,
    height: u32,
) -> std::result::Result<RequestProbe, String> {
    let coded = coded_format(codec);
    let videos = device_paths(Path::new("/dev"), "video")
        .map_err(|error| format!("enumerating /dev/video* failed: {error}"))?;
    let media = device_paths(Path::new("/dev"), "media")
        .map_err(|error| format!("enumerating /dev/media* failed: {error}"))?;
    let mut failures = Vec::new();
    let mut discovery_failures = Vec::new();
    for path in videos {
        let video = match open_decoder(&path, coded) {
            Ok(Some(video)) => video,
            Ok(None) => continue,
            Err(error) => {
                if discovery_failures.len() < MAX_FAILURES {
                    discovery_failures.push(format!("{}: {error}", path.display()));
                }
                continue;
            }
        };
        match inspect(&video, coded, (width, height), &media) {
            Ok((capture, media)) => {
                let max_dimensions = max_frame_size(
                    |frame_size| ioctl(video.file.as_raw_fd(), VIDIOC_ENUM_FRAMESIZES, frame_size),
                    coded,
                );
                return Ok(RequestProbe {
                    detail: format!(
                        "{} request decode to {} via {} and {}",
                        codec.label(),
                        fourcc(capture),
                        path.display(),
                        media.display()
                    ),
                    max_dimensions,
                });
            }
            Err(error) => {
                if failures.len() < MAX_FAILURES {
                    failures.push(format!("{}: {error}", path.display()));
                }
            }
        }
    }
    if failures.is_empty() {
        failures = discovery_failures;
    }
    if failures.is_empty() {
        Err(format!(
            "no accessible streaming {} stateless ({}) decoder under /dev/video*",
            codec.label(),
            fourcc(coded)
        ))
    } else {
        Err(format!(
            "{} request discovery failed: {}",
            codec.label(),
            failures.join("; ")
        ))
    }
}

fn device_paths(directory: &Path, prefix: &str) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(index) = name.to_str().and_then(|name| name.strip_prefix(prefix)) else {
            continue;
        };
        if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let Ok(index) = index.parse::<u32>() else {
            continue;
        };
        if paths.len() == MAX_DEVICE_NODES {
            return Err(io::Error::other(format!(
                "more than {MAX_DEVICE_NODES} {prefix} nodes"
            )));
        }
        paths.push((index, entry.path()));
    }
    paths.sort_by_key(|(index, _)| *index);
    Ok(paths.into_iter().map(|(_, path)| path).collect())
}

fn open_decoder(path: &Path, coded: u32) -> std::result::Result<Option<RequestDevice>, String> {
    let video = open_device(path).map_err(|error| format!("open failed: {error}"))?;
    let fd = video.as_raw_fd();
    let mut capability: v4l2_capability = zeroed();
    ioctl(fd, vidioc::VIDIOC_QUERYCAP, &mut capability)
        .map_err(|error| format!("VIDIOC_QUERYCAP failed: {error}"))?;
    let caps = if capability.capabilities & V4L2_CAP_DEVICE_CAPS != 0 {
        capability.device_caps
    } else {
        capability.capabilities
    };
    if caps & V4L2_CAP_STREAMING == 0
        || caps & (V4L2_CAP_VIDEO_M2M | V4L2_CAP_VIDEO_M2M_MPLANE) == 0
    {
        return Ok(None);
    }
    let multiplanar = caps & V4L2_CAP_VIDEO_M2M_MPLANE != 0;
    let (output_type, capture_type) = if multiplanar {
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
    let formats = enum_formats(fd, output_type)
        .map_err(|error| format!("VIDIOC_ENUM_FMT output failed: {error}"))?;
    Ok(formats.contains(&coded).then_some(RequestDevice {
        file: video,
        output_type,
        capture_type,
    }))
}

fn inspect(
    video: &RequestDevice,
    coded: u32,
    (width, height): (u32, u32),
    media_paths: &[PathBuf],
) -> std::result::Result<(u32, PathBuf), String> {
    let fd = video.file.as_raw_fd();
    let negotiated = set_format(
        fd,
        video.output_type,
        coded,
        width,
        height,
        Some(1024 * 1024),
    )
    .map_err(|error| format!("VIDIOC_S_FMT {} failed: {error}", fourcc(coded)))?;
    if negotiated.width < width || negotiated.height < height {
        return Err(format!(
            "decoder limits {} to {}x{}, below {width}x{height}",
            fourcc(coded),
            negotiated.width,
            negotiated.height
        ));
    }
    let formats = enum_formats(fd, video.capture_type)
        .map_err(|error| format!("VIDIOC_ENUM_FMT capture failed: {error}"))?;
    let Some(capture) = presentable_capture(&formats) else {
        return Err("capture formats lack 8-bit NV12 or SAND128 NC12/Nc12".to_owned());
    };
    let metadata = video
        .file
        .metadata()
        .map_err(|error| format!("fstat failed: {error}"))?;
    let device = (libc::major(metadata.rdev()), libc::minor(metadata.rdev()));
    let mut failures = Vec::new();
    for path in media_paths {
        let result = (|| {
            let media = open_device(path).map_err(|error| format!("open failed: {error}"))?;
            if !media_contains_video(
                |topology| ioctl(media.as_raw_fd(), MEDIA_IOC_G_TOPOLOGY, topology),
                device,
            )
            .map_err(|error| format!("MEDIA_IOC_G_TOPOLOGY failed: {error}"))?
            {
                return Ok(None);
            }
            let mut request_fd: libc::c_int = -1;
            ioctl(media.as_raw_fd(), MEDIA_IOC_REQUEST_ALLOC, &mut request_fd)
                .map_err(|error| format!("MEDIA_IOC_REQUEST_ALLOC failed: {error}"))?;
            if request_fd < 0 {
                return Err("MEDIA_IOC_REQUEST_ALLOC returned an invalid descriptor".to_owned());
            }
            drop(unsafe { OwnedFd::from_raw_fd(request_fd) });
            Ok(Some(path.clone()))
        })();
        match result {
            Ok(Some(path)) => return Ok((capture, path)),
            Ok(None) => {}
            Err(error) => {
                if failures.len() < MAX_FAILURES {
                    failures.push(format!("{}: {error}", path.display()));
                }
            }
        }
    }
    Err(if failures.is_empty() {
        format!(
            "no /dev/media* topology contains video device {}:{}",
            device.0, device.1
        )
    } else {
        failures.join("; ")
    })
}

fn media_contains_video(
    mut query: impl FnMut(&mut MediaTopology) -> io::Result<()>,
    device: (u32, u32),
) -> io::Result<bool> {
    for _ in 0..3 {
        let mut topology = MediaTopology::default();
        query(&mut topology)?;
        let count = topology.num_interfaces as usize;
        if count > MAX_INTERFACES {
            return Err(io::Error::other(format!(
                "more than {MAX_INTERFACES} media interfaces"
            )));
        }
        if count == 0 {
            return Ok(false);
        }
        let version = topology.topology_version;
        let mut interfaces = vec![MediaInterface::default(); count];
        topology.ptr_interfaces = interfaces.as_mut_ptr() as usize as u64;
        match query(&mut topology) {
            Err(error) if error.raw_os_error() == Some(libc::ENOSPC) => continue,
            Err(error) => return Err(error),
            Ok(()) => {}
        }
        if topology.topology_version != version || topology.num_interfaces as usize > count {
            continue;
        }
        return Ok(interfaces[..topology.num_interfaces as usize]
            .iter()
            .any(|interface| {
                interface.intf_type == MEDIA_INTF_T_V4L_VIDEO
                    && interface.devnode[0] == device.0
                    && interface.devnode[1] == device.1
            }));
    }
    Err(io::Error::other(
        "media topology changed during three attempts",
    ))
}

fn max_frame_size(
    mut query: impl FnMut(&mut FrameSizeEnum) -> io::Result<()>,
    pixel_format: u32,
) -> Option<[u32; 2]> {
    let mut largest: Option<[u32; 2]> = None;
    for index in 0..MAX_DEVICE_NODES as u32 {
        let mut frame_size = FrameSizeEnum {
            index,
            pixel_format,
            ..Default::default()
        };
        if query(&mut frame_size).is_err() {
            break;
        }
        let size = if frame_size.kind == V4L2_FRMSIZE_TYPE_DISCRETE {
            [frame_size.sizes[0], frame_size.sizes[1]]
        } else {
            return Some([frame_size.sizes[1], frame_size.sizes[4]]);
        };
        largest = Some(largest.map_or(size, |[width, height]| {
            [width.max(size[0]), height.max(size[1])]
        }));
    }
    largest.filter(|[width, height]| *width > 0 && *height > 0)
}

fn presentable_capture(formats: &[u32]) -> Option<u32> {
    formats
        .iter()
        .copied()
        .find(|format| PRESENTABLE_CAPTURE.contains(format))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interface(device: (u32, u32), intf_type: u32) -> MediaInterface {
        let mut interface = MediaInterface {
            intf_type,
            ..Default::default()
        };
        interface.devnode[0] = device.0;
        interface.devnode[1] = device.1;
        interface
    }

    fn fill_topology(topology: &mut MediaTopology, interfaces: &[MediaInterface], version: u64) {
        if topology.ptr_interfaces != 0 {
            assert!(topology.num_interfaces as usize >= interfaces.len());
            unsafe {
                std::ptr::copy_nonoverlapping(
                    interfaces.as_ptr(),
                    topology.ptr_interfaces as usize as *mut MediaInterface,
                    interfaces.len(),
                );
            }
        }
        topology.num_interfaces = interfaces.len() as u32;
        topology.topology_version = version;
    }

    #[test]
    fn topology_abi_matches_linux_uapi() {
        assert_eq!(std::mem::size_of::<MediaTopology>(), 72);
        assert_eq!(std::mem::size_of::<MediaInterface>(), 112);
        assert_eq!(std::mem::offset_of!(MediaTopology, ptr_interfaces), 32);
        assert_eq!(std::mem::offset_of!(MediaInterface, devnode), 48);
        assert_eq!(MEDIA_IOC_G_TOPOLOGY, 0xc0487c04);
        assert_eq!(MEDIA_IOC_REQUEST_ALLOC, 0x80047c05);
    }

    #[test]
    fn topology_matches_the_video_device_number_not_its_filename_or_parent() {
        let interfaces = [interface((81, 16), MEDIA_INTF_T_V4L_VIDEO)];
        for (device, expected) in [((81, 16), true), ((81, 19), false), ((82, 16), false)] {
            let mut calls = 0;
            let found = media_contains_video(
                |topology| {
                    calls += 1;
                    fill_topology(topology, &interfaces, 7);
                    Ok(())
                },
                device,
            )
            .unwrap();
            assert_eq!(found, expected);
            assert_eq!(calls, 2);
        }
    }

    #[test]
    fn topology_ignores_other_interface_types_and_empty_graphs() {
        for interfaces in [vec![], vec![interface((81, 16), 0x201)]] {
            assert!(
                !media_contains_video(
                    |topology| {
                        fill_topology(topology, &interfaces, 7);
                        Ok(())
                    },
                    (81, 16)
                )
                .unwrap()
            );
        }
    }

    #[test]
    fn topology_retries_version_changes_and_growth() {
        for errno in [None, Some(libc::ENOSPC)] {
            let interfaces = [interface((81, 16), MEDIA_INTF_T_V4L_VIDEO)];
            let mut calls = 0;
            assert!(
                media_contains_video(
                    |topology| {
                        calls += 1;
                        if calls == 2 {
                            if let Some(errno) = errno {
                                return Err(io::Error::from_raw_os_error(errno));
                            }
                        }
                        fill_topology(topology, &interfaces, if calls == 1 { 7 } else { 8 });
                        Ok(())
                    },
                    (81, 16)
                )
                .unwrap()
            );
            assert_eq!(calls, 4);
        }
    }

    #[test]
    fn topology_churn_is_bounded_and_errors_are_preserved() {
        let mut calls = 0;
        let error = media_contains_video(
            |topology| {
                calls += 1;
                fill_topology(
                    topology,
                    &[interface((81, 16), MEDIA_INTF_T_V4L_VIDEO)],
                    calls,
                );
                Ok(())
            },
            (81, 16),
        )
        .unwrap_err();
        assert_eq!(calls, 6);
        assert!(error.to_string().contains("three attempts"));
        for errno in [libc::EACCES, libc::ENOTTY, libc::ENODEV] {
            let error =
                media_contains_video(|_| Err(io::Error::from_raw_os_error(errno)), (81, 16))
                    .unwrap_err();
            assert_eq!(error.raw_os_error(), Some(errno));
        }
    }

    #[test]
    fn topology_rejects_unbounded_allocations() {
        let mut calls = 0;
        let error = media_contains_video(
            |topology| {
                calls += 1;
                topology.num_interfaces = MAX_INTERFACES as u32 + 1;
                Ok(())
            },
            (81, 16),
        )
        .unwrap_err();
        assert_eq!(calls, 1);
        assert!(error.to_string().contains("media interfaces"));
    }

    #[test]
    fn device_enumeration_accepts_high_indices_and_is_bounded() {
        let directory =
            std::env::temp_dir().join(format!("opennow-v4l2-request-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        for name in [
            "video128",
            "video19",
            "video2",
            "video-old",
            "video",
            "media71",
        ] {
            fs::write(directory.join(name), []).unwrap();
        }
        assert_eq!(
            device_paths(&directory, "video").unwrap(),
            ["video2", "video19", "video128"].map(|name| directory.join(name))
        );
        assert_eq!(
            device_paths(&directory, "media").unwrap(),
            vec![directory.join("media71")]
        );
        for index in 0..=MAX_DEVICE_NODES {
            fs::write(directory.join(format!("media{index}")), []).unwrap();
        }
        assert!(
            device_paths(&directory, "media")
                .unwrap_err()
                .to_string()
                .contains("media nodes")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn request_capture_requires_eight_bit_linear_or_sand_nv12() {
        let format = |name: &[u8; 4]| u32::from_le_bytes(*name);
        for accepted in [b"NV12", b"NC12", b"Nc12"] {
            assert_eq!(
                presentable_capture(&[format(accepted)]),
                Some(format(accepted))
            );
        }
        for rejected in [b"NC30", b"Nc30", b"NV15", b"NV16", b"ST12"] {
            assert_eq!(presentable_capture(&[format(rejected)]), None);
        }
        assert_eq!(
            presentable_capture(&[format(b"NV15"), format(b"NV12")]),
            Some(format(b"NV12"))
        );
        assert_eq!(presentable_capture(&[]), None);
    }

    #[test]
    fn frame_size_abi_matches_linux_uapi() {
        assert_eq!(std::mem::size_of::<FrameSizeEnum>(), 44);
        assert_eq!(VIDIOC_ENUM_FRAMESIZES, 0xc02c564a);
    }

    #[test]
    fn stepwise_frame_sizes_report_the_driver_maximum() {
        let coded = coded_format(VideoCodec::H264);
        let mut calls = 0;
        let limit = max_frame_size(
            |frame_size| {
                calls += 1;
                assert_eq!((frame_size.index, frame_size.pixel_format), (0, coded));
                frame_size.kind = 3;
                frame_size.sizes = [48, 1920, 16, 48, 1088, 16];
                Ok(())
            },
            coded,
        );
        assert_eq!(limit, Some([1920, 1088]));
        assert_eq!(calls, 1);
    }

    #[test]
    fn discrete_frame_sizes_report_the_largest_and_unsupported_queries_report_none() {
        let sizes = [[1280, 720], [3840, 2160], [1920, 1080]];
        let limit = max_frame_size(
            |frame_size| {
                let size = sizes
                    .get(frame_size.index as usize)
                    .ok_or_else(|| io::Error::from_raw_os_error(libc::EINVAL))?;
                frame_size.kind = V4L2_FRMSIZE_TYPE_DISCRETE;
                frame_size.sizes[..2].copy_from_slice(size);
                Ok(())
            },
            coded_format(VideoCodec::Av1),
        );
        assert_eq!(limit, Some([3840, 2160]));
        assert_eq!(
            max_frame_size(|_| Err(io::Error::from_raw_os_error(libc::ENOTTY)), 0),
            None
        );
    }

    #[test]
    fn coded_formats_match_the_stateless_uapi_fourccs() {
        assert_eq!(fourcc(coded_format(VideoCodec::H264)), "S264");
        assert_eq!(fourcc(coded_format(VideoCodec::H265)), "S265");
        assert_eq!(fourcc(coded_format(VideoCodec::Av1)), "AV1F");
    }
}
