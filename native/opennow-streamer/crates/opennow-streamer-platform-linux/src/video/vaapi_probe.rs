use std::ffi::{CString, c_int, c_void};
use std::os::fd::AsRawFd;

use crate::VideoCodec;

const VA_PROFILE_HEVC_MAIN10: c_int = 18;
const VA_PROFILE_AV1_PROFILE0: c_int = 32;
const VA_ENTRYPOINT_VLD: c_int = 1;
const VA_CONFIG_ATTRIB_RT_FORMAT: c_int = 0;
const VA_RT_FORMAT_YUV420_10: u32 = 0x100;

#[repr(C)]
struct ConfigAttribute {
    kind: c_int,
    value: u32,
}

pub(crate) fn ten_bit_device(codec: VideoCodec) -> Option<CString> {
    let profile = match codec {
        VideoCodec::H264 => return None,
        VideoCodec::H265 => VA_PROFILE_HEVC_MAIN10,
        VideoCodec::Av1 => VA_PROFILE_AV1_PROFILE0,
    };
    let va = unsafe { libloading::Library::new("libva.so.2") }.ok()?;
    let drm = unsafe { libloading::Library::new("libva-drm.so.2") }.ok()?;
    let get_display =
        unsafe { drm.get::<unsafe extern "C" fn(c_int) -> *mut c_void>(b"vaGetDisplayDRM\0") }
            .ok()?;
    let initialize = unsafe {
        va.get::<unsafe extern "C" fn(*mut c_void, *mut c_int, *mut c_int) -> c_int>(
            b"vaInitialize\0",
        )
    }
    .ok()?;
    let terminate =
        unsafe { va.get::<unsafe extern "C" fn(*mut c_void) -> c_int>(b"vaTerminate\0") }.ok()?;
    let max_entrypoints =
        unsafe { va.get::<unsafe extern "C" fn(*mut c_void) -> c_int>(b"vaMaxNumEntrypoints\0") }
            .ok()?;
    let query_entrypoints = unsafe {
        va.get::<unsafe extern "C" fn(*mut c_void, c_int, *mut c_int, *mut c_int) -> c_int>(
            b"vaQueryConfigEntrypoints\0",
        )
    }
    .ok()?;
    let get_attributes = unsafe {
        va.get::<unsafe extern "C" fn(*mut c_void, c_int, c_int, *mut ConfigAttribute, c_int) -> c_int>(b"vaGetConfigAttributes\0")
    }.ok()?;
    for index in 128..192 {
        let path = format!("/dev/dri/renderD{index}");
        let Ok(file) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
        else {
            continue;
        };
        let display = unsafe { get_display(file.as_raw_fd()) };
        if display.is_null() {
            continue;
        }
        let (mut major, mut minor) = (0, 0);
        if unsafe { initialize(display, &mut major, &mut minor) } != 0 {
            continue;
        }
        let maximum = unsafe { max_entrypoints(display) };
        let mut supported = false;
        if (1..=1024).contains(&maximum) {
            let mut entrypoints = vec![0; maximum as usize];
            let mut count = 0;
            let status = unsafe {
                query_entrypoints(display, profile, entrypoints.as_mut_ptr(), &mut count)
            };
            if status == 0
                && (0..=maximum).contains(&count)
                && entrypoints[..count as usize].contains(&VA_ENTRYPOINT_VLD)
            {
                let mut attribute = ConfigAttribute {
                    kind: VA_CONFIG_ATTRIB_RT_FORMAT,
                    value: u32::MAX,
                };
                supported = unsafe {
                    get_attributes(display, profile, VA_ENTRYPOINT_VLD, &mut attribute, 1)
                } == 0
                    && attribute.value != u32::MAX
                    && attribute.value & VA_RT_FORMAT_YUV420_10 != 0;
            }
        }
        unsafe { terminate(display) };
        if supported {
            return CString::new(path).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h264_never_advertises_a_main10_profile() {
        assert!(ten_bit_device(VideoCodec::H264).is_none());
    }
}
