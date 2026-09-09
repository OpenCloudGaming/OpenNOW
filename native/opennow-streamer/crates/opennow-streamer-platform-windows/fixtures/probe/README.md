# Windows advanced-format decode probes

These synthetic black access units exercise the actual Media Foundation decoder before
advertising 10-bit, 4:4:4, or HDR support. Each contains one independent 1920x1080 frame
at 60 Hz with no reordering. HEVC uses Annex B; AV1 uses low-overhead OBU framing.
All samples use limited range. SDR is BT.709; HDR10 is BT.2020 nonconstant-luminance/PQ.
They contain no captured user or game content.
HEVC uses `keyint=60` even though only one IDR is emitted: `keyint=1` would select
the intra-only RExt profile instead of Main10 for the P010 samples.

Generated with FFmpeg 6.1.1, libx265 and libaom-av1. FFmpeg is a development-only
fixture generator, not a Windows runtime dependency. From this directory:

```sh
for spec in 'p010 yuv420p10le' 'ayuv yuv444p' 'y410 yuv444p10le'; do
    set -- $spec
    ffmpeg -hide_banner -loglevel error \
        -f lavfi -i 'color=black:size=1920x1080:rate=60' -frames:v 1 \
        -pix_fmt "$2" -c:v libx265 -preset ultrafast \
        -x265-params 'log-level=error:pools=1:frame-threads=1:info=0:keyint=60:bframes=0:repeat-headers=1:colorprim=bt709:transfer=bt709:colormatrix=bt709:chromaloc=0' \
        -f hevc -y "hevc-$1-sdr.hevc"
done
ffmpeg -hide_banner -loglevel error \
    -f lavfi -i 'color=black:size=1920x1080:rate=60' -frames:v 1 \
    -pix_fmt yuv420p10le -c:v libx265 -preset ultrafast \
    -x265-params 'log-level=error:pools=1:frame-threads=1:info=0:keyint=60:bframes=0:repeat-headers=1:colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc:chromaloc=0' \
    -f hevc -y hevc-p010-pq.hevc
for transfer in sdr pq; do
    if [ "$transfer" = pq ]; then
        colors='-color_primaries bt2020 -color_trc smpte2084 -colorspace bt2020nc'
    else
        colors='-color_primaries bt709 -color_trc bt709 -colorspace bt709'
    fi
    ffmpeg -hide_banner -loglevel error \
        -f lavfi -i 'color=black:size=1920x1080:rate=60' -frames:v 1 \
        -pix_fmt yuv420p10le -c:v libaom-av1 -cpu-used 8 -threads 2 \
        -lag-in-frames 0 -g 1 $colors -color_range tv \
        -f obu -y "av1-p010-$transfer.obu"
done
```

Verify the encoded dimensions, pixel format, and color metadata with:

```sh
for file in *.hevc *.obu; do
    ffprobe -v error -show_entries \
        stream=codec_name,profile,width,height,pix_fmt,color_space,color_transfer,color_primaries \
        -of compact "$file"
    ffmpeg -v error -i "$file" -frames:v 1 -f null -
done
```
