# CUDA decode probe fixtures

These synthetic black keyframes contain no report data. Each fixture encodes one
1920x1080 8-bit 4:2:0 frame without inter-frame references. CUDA capability probing
submits one packet, drains the decoder, and requires one CUDA output frame. It does
not convert or download that frame.

Regenerate from this directory with FFmpeg CLI and the libx264, libx265, and libaom
encoders:

```sh
ffmpeg -hide_banner -loglevel error -f lavfi -i color=c=black:size=1920x1080:rate=60 -frames:v 1 -pix_fmt yuv420p -c:v libx264 -preset ultrafast -tune zerolatency -x264-params 'threads=1:repeat-headers=1' -f h264 -y cuda_probe.h264
ffmpeg -hide_banner -loglevel error -f lavfi -i color=c=black:size=1920x1080:rate=60 -frames:v 1 -pix_fmt yuv420p -c:v libx265 -preset ultrafast -x265-params 'pools=none:frame-threads=1:bframes=0:info=0:log-level=error' -f hevc -y cuda_probe.hevc
ffmpeg -hide_banner -loglevel error -f lavfi -i color=c=black:size=1920x1080:rate=60 -frames:v 1 -pix_fmt yuv420p -c:v libaom-av1 -cpu-used 8 -threads 1 -f obu -y cuda_probe.obu
```

The ordinary tests check successful H.264 and HEVC decode, deferred format rejection,
and refusal of a software frame when hardware output is required. The AV1 fixture
can be checked with `ffmpeg -i cuda_probe.obu -f null -`. The bundled native AV1
decoder requires hardware, so validate CUDA separately on an NVIDIA device:

```sh
cargo test -p opennow-streamer-platform-linux --features ffmpeg-bundled cuda_probe_decodes_each_advertised_codec -- --ignored --nocapture
```

On a device without AV1 decode support, AV1 must be unavailable while supported
H.264 or HEVC codecs remain available.

The following regression requires an NVIDIA GPU with H.264 decode but no AV1
decode, such as the MX150 from the report. It fails with the former open-only
probe because that probe reports AV1 as available:

```sh
cargo test -p opennow-streamer-platform-linux --features ffmpeg-bundled cuda_probe_rejects_av1_on_h264_only_hardware -- --ignored --nocapture
```
