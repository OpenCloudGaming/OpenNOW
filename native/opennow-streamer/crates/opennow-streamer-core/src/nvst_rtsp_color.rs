use opennow_streamer_platform::MediaStreamConfig;

/// NVST ANNOUNCE color encoding, matching the official wire format.
///
/// Bifrost serializes its video config bytes unchanged: depth is literal (8 or
/// 10) and chroma is the 4:4:4 flag (0 = 4:2:0, 1 = 4:4:4). Its validator
/// rejects any chroma value above 1, and an official 10-bit 4:4:4 session
/// carries `bitDepth:10 chromaFormat:1`. `chroma_format_idc` values (1/3) are
/// not the wire enum: seats answer a 10-bit ANNOUNCE carrying them with a
/// 0x800e8605 termination before the first video frame. Both lines are always
/// sent explicitly. `dynamicRangeMode` is sent only for HDR (`1`).
pub(super) fn announce_color_lines(stream: MediaStreamConfig) -> Vec<String> {
    let mut lines = vec![
        format!(
            "a=x-nv-video[0].bitDepth:{}",
            stream.color_quality.bit_depth()
        ),
        format!(
            "a=x-nv-video[0].chromaFormat:{}",
            u8::from(stream.color_quality.is_444())
        ),
    ];
    if stream.hdr {
        lines.push("a=x-nv-video[0].dynamicRangeMode:1".to_owned());
    }
    lines
}

#[cfg(test)]
#[path = "nvst_rtsp_color_tests.rs"]
mod tests;
