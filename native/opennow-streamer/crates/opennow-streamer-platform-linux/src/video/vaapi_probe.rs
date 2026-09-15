use std::cell::Cell;
use std::ffi::{CString, c_int, c_void};
use std::os::fd::AsRawFd;

use crate::{VaapiColorSupport, VideoCodec};

const VA_PROFILE_HEVC_MAIN: c_int = 17;
const VA_PROFILE_HEVC_MAIN10: c_int = 18;
const VA_PROFILE_AV1_PROFILE0: c_int = 32;
const VA_ENTRYPOINT_VLD: c_int = 1;
const VA_CONFIG_ATTRIB_RT_FORMAT: c_int = 0;
const VA_RT_FORMAT_YUV420: u32 = 0x1;
const VA_RT_FORMAT_YUV420_10: u32 = 0x100;
const RENDER_NODE_RANGE: std::ops::Range<u32> = 128..192;

#[repr(C)]
struct ConfigAttribute {
    kind: c_int,
    value: u32,
}

type GetDisplayDrm = unsafe extern "C" fn(c_int) -> *mut c_void;
type Initialize = unsafe extern "C" fn(*mut c_void, *mut c_int, *mut c_int) -> c_int;
type Terminate = unsafe extern "C" fn(*mut c_void) -> c_int;
type MaxEntrypoints = unsafe extern "C" fn(*mut c_void) -> c_int;
type QueryEntrypoints = unsafe extern "C" fn(*mut c_void, c_int, *mut c_int, *mut c_int) -> c_int;
type GetAttributes =
    unsafe extern "C" fn(*mut c_void, c_int, c_int, *mut ConfigAttribute, c_int) -> c_int;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VaapiDepth {
    EightBit420,
    TenBit420,
}

impl VaapiDepth {
    pub(crate) fn query(self, codec: VideoCodec) -> Option<(c_int, u32)> {
        let queries = color_queries(codec)?;
        Some(match self {
            Self::EightBit420 => queries[0],
            Self::TenBit420 => queries[1],
        })
    }
}

struct Display {
    handle: *mut c_void,
    _file: std::fs::File,
    terminate: Terminate,
    max_entrypoints: MaxEntrypoints,
    query_entrypoints: QueryEntrypoints,
    get_attributes: GetAttributes,
}

impl Display {
    fn supports(&self, profile: c_int, rt_format: u32) -> bool {
        let maximum = unsafe { (self.max_entrypoints)(self.handle) };
        if !(1..=1024).contains(&maximum) {
            return false;
        }
        let mut entrypoints = vec![0; maximum as usize];
        let mut count = 0;
        let status = unsafe {
            (self.query_entrypoints)(self.handle, profile, entrypoints.as_mut_ptr(), &mut count)
        };
        if status != 0
            || !(0..=maximum).contains(&count)
            || !entrypoints[..count as usize].contains(&VA_ENTRYPOINT_VLD)
        {
            return false;
        }
        let mut attribute = ConfigAttribute {
            kind: VA_CONFIG_ATTRIB_RT_FORMAT,
            value: u32::MAX,
        };
        (unsafe {
            (self.get_attributes)(self.handle, profile, VA_ENTRYPOINT_VLD, &mut attribute, 1)
        }) == 0
            && attribute.value != u32::MAX
            && attribute.value & rt_format != 0
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        unsafe { (self.terminate)(self.handle) };
    }
}

struct VaapiLibrary {
    _va: libloading::Library,
    _drm: libloading::Library,
    get_display: GetDisplayDrm,
    initialize: Initialize,
    terminate: Terminate,
    max_entrypoints: MaxEntrypoints,
    query_entrypoints: QueryEntrypoints,
    get_attributes: GetAttributes,
}

impl VaapiLibrary {
    fn load() -> Option<Self> {
        let va = unsafe { libloading::Library::new("libva.so.2") }.ok()?;
        let drm = unsafe { libloading::Library::new("libva-drm.so.2") }.ok()?;
        let get_display = *unsafe { drm.get::<GetDisplayDrm>(b"vaGetDisplayDRM\0") }.ok()?;
        let initialize = *unsafe { va.get::<Initialize>(b"vaInitialize\0") }.ok()?;
        let terminate = *unsafe { va.get::<Terminate>(b"vaTerminate\0") }.ok()?;
        let max_entrypoints =
            *unsafe { va.get::<MaxEntrypoints>(b"vaMaxNumEntrypoints\0") }.ok()?;
        let query_entrypoints =
            *unsafe { va.get::<QueryEntrypoints>(b"vaQueryConfigEntrypoints\0") }.ok()?;
        let get_attributes =
            *unsafe { va.get::<GetAttributes>(b"vaGetConfigAttributes\0") }.ok()?;
        Some(Self {
            _va: va,
            _drm: drm,
            get_display,
            initialize,
            terminate,
            max_entrypoints,
            query_entrypoints,
            get_attributes,
        })
    }

    fn display(&self, path: &str) -> Option<Display> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .ok()?;
        let handle = unsafe { (self.get_display)(file.as_raw_fd()) };
        if handle.is_null() {
            return None;
        }
        let (mut major, mut minor) = (0, 0);
        if unsafe { (self.initialize)(handle, &mut major, &mut minor) } != 0 {
            return None;
        }
        Some(Display {
            handle,
            _file: file,
            terminate: self.terminate,
            max_entrypoints: self.max_entrypoints,
            query_entrypoints: self.query_entrypoints,
            get_attributes: self.get_attributes,
        })
    }
}

fn candidate_paths() -> impl Iterator<Item = String> {
    RENDER_NODE_RANGE.map(|index| format!("/dev/dri/renderD{index}"))
}

fn select_device(
    candidates: impl IntoIterator<Item = String>,
    mut supports: impl FnMut(&str) -> Option<bool>,
) -> Option<String> {
    candidates
        .into_iter()
        .find(|path| supports(path) == Some(true))
}

pub(crate) const fn color_queries(codec: VideoCodec) -> Option<[(c_int, u32); 2]> {
    match codec {
        VideoCodec::H264 => None,
        VideoCodec::H265 => Some([
            (VA_PROFILE_HEVC_MAIN, VA_RT_FORMAT_YUV420),
            (VA_PROFILE_HEVC_MAIN10, VA_RT_FORMAT_YUV420_10),
        ]),
        VideoCodec::Av1 => Some([
            (VA_PROFILE_AV1_PROFILE0, VA_RT_FORMAT_YUV420),
            (VA_PROFILE_AV1_PROFILE0, VA_RT_FORMAT_YUV420_10),
        ]),
    }
}

pub(crate) fn color_support_from(
    codec: VideoCodec,
    mut supported: impl FnMut(c_int, u32) -> bool,
) -> VaapiColorSupport {
    let Some([eight, ten]) = color_queries(codec) else {
        return VaapiColorSupport::default();
    };
    VaapiColorSupport {
        eight_bit_420: supported(eight.0, eight.1),
        ten_bit_420: supported(ten.0, ten.1),
    }
}

pub(crate) fn color_support(codec: VideoCodec) -> VaapiColorSupport {
    let Some(queries) = color_queries(codec) else {
        return VaapiColorSupport::default();
    };
    let answers = [const { Cell::new(false) }; 2];
    if let Some(library) = VaapiLibrary::load() {
        select_device(candidate_paths(), |path| {
            let display = library.display(path)?;
            for (index, (profile, rt_format)) in queries.iter().enumerate() {
                if !answers[index].get() && display.supports(*profile, *rt_format) {
                    answers[index].set(true);
                }
            }
            Some(answers.iter().all(|answer| answer.get()))
        });
    }
    color_support_from(codec, |profile, rt_format| {
        queries
            .iter()
            .position(|query| *query == (profile, rt_format))
            .is_some_and(|index| answers[index].get())
    })
}

pub(crate) fn device_for_profile(codec: VideoCodec, depth: VaapiDepth) -> Option<CString> {
    let (profile, rt_format) = depth.query(codec)?;
    let library = VaapiLibrary::load()?;
    select_device(candidate_paths(), |path| {
        library
            .display(path)
            .map(|display| display.supports(profile, rt_format))
    })
    .and_then(|path| CString::new(path).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h264_never_advertises_a_main10_profile() {
        assert!(device_for_profile(VideoCodec::H264, VaapiDepth::TenBit420).is_none());
        assert!(device_for_profile(VideoCodec::H264, VaapiDepth::EightBit420).is_none());
        assert_eq!(
            color_support(VideoCodec::H264),
            VaapiColorSupport::default()
        );
    }

    #[test]
    fn h265_queries_main_and_main10_with_their_own_rt_formats() {
        assert_eq!(
            color_queries(VideoCodec::H265),
            Some([(17, 0x1), (18, 0x100)])
        );
        let mut requested = Vec::new();
        let support = color_support_from(VideoCodec::H265, |profile, rt_format| {
            requested.push((profile, rt_format));
            profile == 18
        });
        assert_eq!(requested, [(17, 0x1), (18, 0x100)]);
        assert_eq!(
            support,
            VaapiColorSupport {
                eight_bit_420: false,
                ten_bit_420: true,
            }
        );
    }

    #[test]
    fn av1_queries_profile0_for_both_depths_through_the_rt_format_attribute() {
        assert_eq!(
            color_queries(VideoCodec::Av1),
            Some([(32, 0x1), (32, 0x100)])
        );
        let support = color_support_from(VideoCodec::Av1, |_, rt_format| rt_format == 0x1);
        assert_eq!(
            support,
            VaapiColorSupport {
                eight_bit_420: true,
                ten_bit_420: false,
            }
        );
    }

    #[test]
    fn color_support_aggregates_each_query_independently() {
        for codec in [VideoCodec::H265, VideoCodec::Av1] {
            assert_eq!(
                color_support_from(codec, |_, _| false),
                VaapiColorSupport::default()
            );
            assert_eq!(
                color_support_from(codec, |_, _| true),
                VaapiColorSupport {
                    eight_bit_420: true,
                    ten_bit_420: true,
                }
            );
            let first = color_support_from(codec, |_, rt_format| rt_format == 0x1);
            assert!(first.eight_bit_420 && !first.ten_bit_420);
            let second = color_support_from(codec, |_, rt_format| rt_format == 0x100);
            assert!(!second.eight_bit_420 && second.ten_bit_420);
        }
    }

    #[test]
    fn h264_never_queries_the_device() {
        let mut queried = false;
        let support = color_support_from(VideoCodec::H264, |_, _| {
            queried = true;
            true
        });
        assert!(!queried);
        assert_eq!(support, VaapiColorSupport::default());
    }

    #[test]
    fn each_depth_selects_the_profile_query_of_its_own_depth() {
        assert_eq!(
            VaapiDepth::EightBit420.query(VideoCodec::H265),
            Some((17, 0x1))
        );
        assert_eq!(
            VaapiDepth::TenBit420.query(VideoCodec::H265),
            Some((18, 0x100))
        );
        assert_eq!(
            VaapiDepth::EightBit420.query(VideoCodec::Av1),
            Some((32, 0x1))
        );
        assert_eq!(
            VaapiDepth::TenBit420.query(VideoCodec::Av1),
            Some((32, 0x100))
        );
        assert_eq!(VaapiDepth::EightBit420.query(VideoCodec::H264), None);
        assert_eq!(VaapiDepth::TenBit420.query(VideoCodec::H264), None);
    }

    #[test]
    fn device_selection_skips_failed_devices_and_takes_the_first_supported_one() {
        let candidates = ["a", "b", "c", "d"].map(str::to_owned);
        let mut probed = Vec::new();
        let chosen = select_device(candidates, |path| {
            probed.push(path.to_owned());
            match path {
                "a" => Some(false),
                "b" => None,
                "c" => Some(true),
                _ => panic!("selection must stop at the first supported device"),
            }
        });
        assert_eq!(probed, ["a", "b", "c"]);
        assert_eq!(chosen.as_deref(), Some("c"));

        assert_eq!(
            select_device(["a", "b"].map(str::to_owned), |_| Some(false)),
            None
        );
        assert_eq!(select_device(["a", "b"].map(str::to_owned), |_| None), None);
        assert_eq!(select_device(Vec::<String>::new(), |_| Some(true)), None);
    }

    #[test]
    fn display_holds_its_render_node_descriptor_until_termination() {
        use std::os::fd::{AsRawFd, RawFd};

        thread_local! {
            static TEST_DESCRIPTOR: Cell<i32> = const { Cell::new(-1) };
            static TERMINATED: Cell<bool> = const { Cell::new(false) };
            static DESCRIPTOR_OPEN_AT_TERMINATE: Cell<bool> = const { Cell::new(false) };
        }

        fn descriptor_is_open(descriptor: RawFd) -> bool {
            (unsafe { libc::fcntl(descriptor, libc::F_GETFD) }) != -1
        }

        unsafe extern "C" fn terminate(_handle: *mut c_void) -> c_int {
            TERMINATED.with(|value| value.set(true));
            let descriptor = TEST_DESCRIPTOR.with(Cell::get);
            DESCRIPTOR_OPEN_AT_TERMINATE.with(|value| value.set(descriptor_is_open(descriptor)));
            0
        }

        unsafe extern "C" fn no_entrypoints(_handle: *mut c_void) -> c_int {
            0
        }

        unsafe extern "C" fn no_entrypoints_query(
            _handle: *mut c_void,
            _profile: c_int,
            _entrypoints: *mut c_int,
            _count: *mut c_int,
        ) -> c_int {
            0
        }

        unsafe extern "C" fn no_attributes(
            _handle: *mut c_void,
            _profile: c_int,
            _entrypoint: c_int,
            _attribute: *mut ConfigAttribute,
            _count: c_int,
        ) -> c_int {
            1
        }

        let file = std::fs::File::open("/dev/null").expect("/dev/null is available");
        let descriptor = file.as_raw_fd();
        TEST_DESCRIPTOR.with(|value| value.set(descriptor));
        let display = Display {
            handle: std::ptr::null_mut(),
            _file: file,
            terminate,
            max_entrypoints: no_entrypoints,
            query_entrypoints: no_entrypoints_query,
            get_attributes: no_attributes,
        };
        assert!(descriptor_is_open(descriptor));
        assert!(!display.supports(17, 0x1));
        assert!(descriptor_is_open(descriptor));
        drop(display);
        assert!(TERMINATED.with(Cell::get));
        assert!(
            DESCRIPTOR_OPEN_AT_TERMINATE.with(Cell::get),
            "vaTerminate must run before the render node descriptor is closed"
        );
        assert!(!descriptor_is_open(descriptor));
    }
}
