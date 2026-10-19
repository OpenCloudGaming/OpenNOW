from pathlib import Path
import subprocess


encoders = [
    ("H264", "libx264", "h264", ["-tune", "zerolatency", "-bsf:v", "filter_units=remove_types=6"]),
    ("H265", "libx265", "hevc", ["-x265-params", "log-level=error:pools=1", "-bsf:v", "filter_units=remove_types=39"]),
    ("Av1", "libaom-av1", "obu", ["-cpu-used", "8", "-threads", "2"]),
]
source = [
    "use crate::VideoCodec;",
    "",
    "pub(super) fn keyframe(codec: VideoCodec) -> &'static [u8] {",
    "    match codec {",
]
for codec, encoder, muxer, options in encoders:
    data = subprocess.check_output([
        "ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error",
        "-f", "lavfi", "-i", "color=c=blue:size=1920x1080:rate=60",
        "-frames:v", "1", "-pix_fmt", "yuv420p", "-c:v", encoder,
        *options, "-f", muxer, "pipe:1",
    ])
    decoded = subprocess.check_output([
        "ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error",
        "-f", muxer, "-i", "pipe:0", "-pix_fmt", "yuv420p",
        "-f", "rawvideo", "pipe:1",
    ], input=data)
    if len(decoded) != 1920 * 1080 * 3 // 2:
        raise RuntimeError(f"{codec} probe sample did not decode to one full HD frame")
    source.append(f"        VideoCodec::{codec} => &[")
    for start in range(0, len(data), 16):
        source.append("            " + ", ".join(f"0x{byte:02x}" for byte in data[start:start + 16]) + ",")
    source.append("        ],")
source.extend(["    }", "}", ""])
output = Path(__file__).resolve().parent.parent / "src/video/probe_samples.rs"
output.write_text("\n".join(source))
subprocess.run(["rustfmt", "--edition", "2024", str(output)], check=True)
