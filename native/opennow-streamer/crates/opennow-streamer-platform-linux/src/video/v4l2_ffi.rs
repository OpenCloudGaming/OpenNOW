#![allow(non_camel_case_types, non_upper_case_globals)]

use std::os::raw::{c_int, c_ulong};

pub const VIDEO_MAX_PLANES: u32 = 8;
pub const V4L2_CAP_VIDEO_M2M_MPLANE: u32 = 1 << 14;
pub const V4L2_CAP_VIDEO_M2M: u32 = 1 << 15;
pub const V4L2_CAP_STREAMING: u32 = 1 << 26;
pub const V4L2_CAP_DEVICE_CAPS: u32 = 1 << 31;
pub const V4L2_EVENT_SOURCE_CHANGE: u32 = 5;
pub const V4L2_EVENT_SRC_CH_RESOLUTION: u32 = 1;
pub const V4L2_DEC_CMD_STOP: u32 = 1;
pub const V4L2_BUF_FLAG_ERROR: u32 = 0x0040;
pub const V4L2_BUF_FLAG_LAST: u32 = 0x0010_0000;
pub const V4L2_SEL_TGT_COMPOSE: u32 = 0x0100;

pub type v4l2_buf_type = u32;
pub const v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_CAPTURE: v4l2_buf_type = 1;
pub const v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_OUTPUT: v4l2_buf_type = 2;
pub const v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE: v4l2_buf_type = 9;
pub const v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE: v4l2_buf_type = 10;

pub type v4l2_memory = u32;
pub const v4l2_memory_V4L2_MEMORY_MMAP: v4l2_memory = 1;
pub type v4l2_field = u32;
pub const v4l2_field_V4L2_FIELD_NONE: v4l2_field = 1;
pub type v4l2_colorspace = u32;
pub const v4l2_colorspace_V4L2_COLORSPACE_REC709: v4l2_colorspace = 3;
pub const v4l2_colorspace_V4L2_COLORSPACE_BT2020: v4l2_colorspace = 10;
pub type v4l2_quantization = u32;
pub const v4l2_quantization_V4L2_QUANTIZATION_FULL_RANGE: v4l2_quantization = 1;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct v4l2_capability {
    pub driver: [u8; 16],
    pub card: [u8; 32],
    pub bus_info: [u8; 32],
    pub version: u32,
    pub capabilities: u32,
    pub device_caps: u32,
    pub reserved: [u32; 3],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct v4l2_fmtdesc {
    pub index: u32,
    pub type_: u32,
    pub flags: u32,
    pub description: [u8; 32],
    pub pixelformat: u32,
    pub mbus_code: u32,
    pub reserved: [u32; 3],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct v4l2_pix_format {
    pub width: u32,
    pub height: u32,
    pub pixelformat: u32,
    pub field: u32,
    pub bytesperline: u32,
    pub sizeimage: u32,
    pub colorspace: u32,
    pub priv_: u32,
    pub flags: u32,
    pub __bindgen_anon_1: v4l2_pix_format__bindgen_ty_1,
    pub quantization: u32,
    pub xfer_func: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union v4l2_pix_format__bindgen_ty_1 {
    pub ycbcr_enc: u32,
    pub hsv_enc: u32,
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct v4l2_plane_pix_format {
    pub sizeimage: u32,
    pub bytesperline: u32,
    pub reserved: [u16; 6],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union v4l2_pix_format_mplane__bindgen_ty_1 {
    pub ycbcr_enc: u8,
    pub hsv_enc: u8,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct v4l2_pix_format_mplane {
    pub width: u32,
    pub height: u32,
    pub pixelformat: u32,
    pub field: u32,
    pub colorspace: u32,
    pub plane_fmt: [v4l2_plane_pix_format; 8],
    pub num_planes: u8,
    pub flags: u8,
    pub __bindgen_anon_1: v4l2_pix_format_mplane__bindgen_ty_1,
    pub quantization: u8,
    pub xfer_func: u8,
    pub reserved: [u8; 7],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union v4l2_format__bindgen_ty_1 {
    pub pix: v4l2_pix_format,
    pub pix_mp: v4l2_pix_format_mplane,
    pub raw_data: [u64; 25],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct v4l2_format {
    pub type_: u32,
    pub fmt: v4l2_format__bindgen_ty_1,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct v4l2_requestbuffers {
    pub count: u32,
    pub type_: u32,
    pub memory: u32,
    pub capabilities: u32,
    pub flags: u8,
    pub reserved: [u8; 3],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union v4l2_plane__bindgen_ty_1 {
    pub mem_offset: u32,
    pub userptr: c_ulong,
    pub fd: i32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct v4l2_plane {
    pub bytesused: u32,
    pub length: u32,
    pub m: v4l2_plane__bindgen_ty_1,
    pub data_offset: u32,
    pub reserved: [u32; 11],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct v4l2_timecode {
    pub type_: u32,
    pub flags: u32,
    pub frames: u8,
    pub seconds: u8,
    pub minutes: u8,
    pub hours: u8,
    pub userbits: [u8; 4],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union v4l2_buffer__bindgen_ty_1 {
    pub offset: u32,
    pub userptr: c_ulong,
    pub planes: *mut v4l2_plane,
    pub fd: i32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union v4l2_buffer__bindgen_ty_2 {
    pub request_fd: i32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct v4l2_buffer {
    pub index: u32,
    pub type_: u32,
    pub bytesused: u32,
    pub flags: u32,
    pub field: u32,
    pub timestamp: libc::timeval,
    pub timecode: v4l2_timecode,
    pub sequence: u32,
    pub memory: u32,
    pub m: v4l2_buffer__bindgen_ty_1,
    pub length: u32,
    pub reserved2: u32,
    pub __bindgen_anon_1: v4l2_buffer__bindgen_ty_2,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct v4l2_event_src_change {
    pub changes: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union v4l2_event__bindgen_ty_1 {
    pub src_change: v4l2_event_src_change,
    pub data: [u64; 8],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct v4l2_event {
    pub type_: u32,
    pub u: v4l2_event__bindgen_ty_1,
    pub pending: u32,
    pub sequence: u32,
    pub timestamp: libc::timespec,
    pub id: u32,
    pub reserved: [u32; 8],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct v4l2_event_subscription {
    pub type_: u32,
    pub id: u32,
    pub flags: u32,
    pub reserved: [u32; 5],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union v4l2_decoder_cmd__bindgen_ty_1 {
    pub raw: [u64; 8],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct v4l2_decoder_cmd {
    pub cmd: u32,
    pub flags: u32,
    pub __bindgen_anon_1: v4l2_decoder_cmd__bindgen_ty_1,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct v4l2_rect {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct v4l2_selection {
    pub type_: u32,
    pub target: u32,
    pub flags: u32,
    pub r: v4l2_rect,
    pub reserved: [u32; 9],
}

pub mod vidioc {
    use super::*;

    pub type _IOC_TYPE = c_ulong;

    const IOC_WRITE: c_ulong = 1;
    const IOC_READ: c_ulong = 2;

    const fn code(direction: c_ulong, number: c_ulong, size: usize) -> _IOC_TYPE {
        (direction << 30) | ((b'V' as c_ulong) << 8) | number | ((size as c_ulong) << 16)
    }

    pub const VIDIOC_QUERYCAP: _IOC_TYPE =
        code(IOC_READ, 0, std::mem::size_of::<v4l2_capability>());
    pub const VIDIOC_ENUM_FMT: _IOC_TYPE =
        code(IOC_READ | IOC_WRITE, 2, std::mem::size_of::<v4l2_fmtdesc>());
    pub const VIDIOC_G_FMT: _IOC_TYPE =
        code(IOC_READ | IOC_WRITE, 4, std::mem::size_of::<v4l2_format>());
    pub const VIDIOC_S_FMT: _IOC_TYPE =
        code(IOC_READ | IOC_WRITE, 5, std::mem::size_of::<v4l2_format>());
    pub const VIDIOC_REQBUFS: _IOC_TYPE = code(
        IOC_READ | IOC_WRITE,
        8,
        std::mem::size_of::<v4l2_requestbuffers>(),
    );
    pub const VIDIOC_QUERYBUF: _IOC_TYPE =
        code(IOC_READ | IOC_WRITE, 9, std::mem::size_of::<v4l2_buffer>());
    pub const VIDIOC_QBUF: _IOC_TYPE =
        code(IOC_READ | IOC_WRITE, 15, std::mem::size_of::<v4l2_buffer>());
    pub const VIDIOC_DQBUF: _IOC_TYPE =
        code(IOC_READ | IOC_WRITE, 17, std::mem::size_of::<v4l2_buffer>());
    pub const VIDIOC_STREAMON: _IOC_TYPE = code(IOC_WRITE, 18, std::mem::size_of::<c_int>());
    pub const VIDIOC_STREAMOFF: _IOC_TYPE = code(IOC_WRITE, 19, std::mem::size_of::<c_int>());
    pub const VIDIOC_DECODER_CMD: _IOC_TYPE = code(
        IOC_READ | IOC_WRITE,
        96,
        std::mem::size_of::<v4l2_decoder_cmd>(),
    );
    pub const VIDIOC_G_SELECTION: _IOC_TYPE = code(
        IOC_READ | IOC_WRITE,
        94,
        std::mem::size_of::<v4l2_selection>(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uapi_layout_matches_linux_x64_and_arm64() {
        assert_eq!(std::mem::size_of::<v4l2_capability>(), 104);
        assert_eq!(std::mem::size_of::<v4l2_fmtdesc>(), 64);
        assert_eq!(std::mem::size_of::<v4l2_pix_format>(), 48);
        assert_eq!(std::mem::size_of::<v4l2_pix_format_mplane>(), 192);
        assert_eq!(std::mem::size_of::<v4l2_format>(), 208);
        assert_eq!(std::mem::size_of::<v4l2_plane>(), 64);
        assert_eq!(std::mem::size_of::<v4l2_buffer>(), 88);
        assert_eq!(std::mem::size_of::<v4l2_event>(), 136);
        assert_eq!(std::mem::size_of::<v4l2_event_subscription>(), 32);
        assert_eq!(std::mem::size_of::<v4l2_decoder_cmd>(), 72);
        assert_eq!(std::mem::size_of::<v4l2_selection>(), 64);
        assert_eq!(std::mem::offset_of!(v4l2_pix_format, flags), 32);
        assert_eq!(std::mem::offset_of!(v4l2_pix_format, quantization), 40);
        assert_eq!(std::mem::offset_of!(v4l2_format, fmt), 8);
        assert_eq!(std::mem::offset_of!(v4l2_plane, m), 8);
        assert_eq!(std::mem::offset_of!(v4l2_plane, data_offset), 16);
        assert_eq!(std::mem::offset_of!(v4l2_buffer, timestamp), 24);
        assert_eq!(std::mem::offset_of!(v4l2_buffer, m), 64);
        assert_eq!(std::mem::offset_of!(v4l2_buffer, length), 72);
        assert_eq!(std::mem::offset_of!(v4l2_event, u), 8);
        assert_eq!(std::mem::offset_of!(v4l2_event, timestamp), 80);
    }
}
