Synthetic gray 1280x720 Main10 keyframes with BT.2020/PQ metadata, generated locally
with ffmpeg; no game or third-party media. Used only for codec callback lifecycle tests,
not latency measurements. Commands:

ffmpeg -f lavfi -i color=c=gray:s=1280x720:r=120 -frames:v 1 -pix_fmt yuv420p10le -c:v libx265 -x265-params 'pools=1:frame-threads=1:repeat-headers=1:colorprim=9:transfer=16:colormatrix=9' -f hevc hdr-decoder-keyframe.hevc
ffmpeg -f lavfi -i color=c=gray:s=1280x720:r=120 -frames:v 1 -pix_fmt yuv420p10le -c:v libaom-av1 -cpu-used 8 -threads 2 -color_primaries bt2020 -color_trc smpte2084 -colorspace bt2020nc -f obu hdr-decoder-keyframe.obu

The SDR wrapper fixture uses the same HEVC command with yuv420p and colorprim=1:transfer=1:colormatrix=1.
