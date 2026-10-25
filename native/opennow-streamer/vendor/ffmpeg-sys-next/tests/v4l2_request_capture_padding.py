"""Compile and exercise the bundled FFmpeg capture allocator without a video device."""

import pathlib
import subprocess
import sys
import tempfile


def patched_source(directory):
    patches = pathlib.Path(__file__).resolve().parents[1] / 'patches' / 'v4l2-request'
    target = 'a/libavutil/hwcontext_v4l2request.c b/libavutil/hwcontext_v4l2request.c'
    for patch in sorted(patches.glob('*.patch')):
        chunks = [
            'diff --git ' + chunk
            for chunk in patch.read_text().split('diff --git ')
            if chunk.startswith(target + '\n')
        ]
        if chunks:
            subprocess.run(
                ['patch', '-p1', '--batch', '--fuzz=0'],
                input=''.join(chunks), text=True, cwd=directory, check=True,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
    return (directory / 'libavutil' / 'hwcontext_v4l2request.c').read_text()


with tempfile.TemporaryDirectory(prefix='opennow-capture-padding-') as temporary:
    directory = pathlib.Path(temporary)
    source = pathlib.Path(sys.argv[1]).read_text() if len(sys.argv) > 1 else patched_source(directory)
    start = source.index('static AVBufferRef *v4l2request_v4l2_buffer_alloc(')
    end = source.index('\nstatic AVBufferRef *v4l2request_capture_buffer_alloc(', start)
    allocator = source[start:end]
    harness = r'''
    #include <assert.h>
    #include <errno.h>
    #include <linux/videodev2.h>
    #include <stdint.h>
    #include <stdio.h>
    #include <stdlib.h>
    #include <string.h>
    #include <sys/ioctl.h>
    #ifndef V4L2_PIX_FMT_NV12_COL128
    #define V4L2_PIX_FMT_NV12_COL128 v4l2_fourcc('N', 'C', '1', '2')
    #define V4L2_PIX_FMT_NV12MT_COL128 v4l2_fourcc('N', 'c', '1', '2')
    #endif
    #define AV_LOG_ERROR 0
    #define av_log(...) ((void)0)
    typedef struct { int video_fd; } AVV4L2RequestFramesContextInternal;
    typedef struct { AVV4L2RequestFramesContextInternal *internal; } AVV4L2RequestFramesContext;
    typedef struct { void *hwctx; } AVHWFramesContext;
    typedef struct { uint8_t *data; } AVBufferRef;
    static AVBufferRef *av_buffer_allocz(size_t size) {
        AVBufferRef *ref = calloc(1, sizeof(*ref));
        assert(ref);
        ref->data = calloc(1, size);
        assert(ref->data);
        return ref;
    }
    static void av_buffer_unref(AVBufferRef **ref) {
        free((*ref)->data);
        free(*ref);
        *ref = NULL;
    }
    static struct v4l2_create_buffers created;
    static int calls;
    static int test_ioctl(int fd, unsigned long request, void *arg) {
        assert(fd == 42);
        calls++;
        if (request == VIDIOC_CREATE_BUFS) {
            created = *(struct v4l2_create_buffers *)arg;
            assert(created.count == 1 && created.memory == V4L2_MEMORY_MMAP);
        } else {
            assert(request == VIDIOC_QUERYBUF);
        }
        return 0;
    }
    #define ioctl test_ioctl
    '''
    harness += allocator
    harness += r'''
    int main(void) {
        AVV4L2RequestFramesContextInternal internal = { .video_fd = 42 };
        AVV4L2RequestFramesContext request = { .internal = &internal };
        AVHWFramesContext frames = { .hwctx = &request };
        for (int split = 0; split < 2; split++) {
            for (int output = 0; output < 2; output++) {
                for (int sand = 0; sand < 2; sand++) {
                    for (int resolution = 0; resolution < 2; resolution++) {
                        uint32_t luma_size = resolution ? 3686400 : 2073600;
                        struct v4l2_format format = {0};
                        format.type = output ? V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE :
                                               V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
                        format.fmt.pix_mp.width = resolution ? 2560 : 1920;
                        format.fmt.pix_mp.height = resolution ? 1440 : 1080;
                        format.fmt.pix_mp.pixelformat = sand ? (split ? V4L2_PIX_FMT_NV12MT_COL128 :
                                                                        V4L2_PIX_FMT_NV12_COL128) :
                                                                      V4L2_PIX_FMT_NV12;
                        format.fmt.pix_mp.num_planes = split ? 2 : 1;
                        format.fmt.pix_mp.plane_fmt[0].sizeimage = split ? luma_size : luma_size * 3 / 2;
                        format.fmt.pix_mp.plane_fmt[0].bytesperline = format.fmt.pix_mp.width;
                        format.fmt.pix_mp.plane_fmt[1].sizeimage = resolution ? 1843200 : 1036800;
                        format.fmt.pix_mp.plane_fmt[1].bytesperline = format.fmt.pix_mp.width;
                        struct v4l2_format original = format;
                        calls = 0;
                        AVBufferRef *ref = v4l2request_v4l2_buffer_alloc(&frames, &format);
                        assert(ref && calls == 2);
                        assert(memcmp(&format, &original, sizeof(format)) == 0);
                        for (int i = 0; i < format.fmt.pix_mp.num_planes; i++) {
                            uint32_t expected = original.fmt.pix_mp.plane_fmt[i].sizeimage;
                            if (sand && !output) expected += 4096;
                            assert(created.format.fmt.pix_mp.plane_fmt[i].sizeimage == expected);
                            assert(created.format.fmt.pix_mp.plane_fmt[i].bytesperline ==
                                   original.fmt.pix_mp.plane_fmt[i].bytesperline);
                        }
                        av_buffer_unref(&ref);
                    }
                }
            }
        }
        struct v4l2_format single = { .type = V4L2_BUF_TYPE_VIDEO_CAPTURE };
        single.fmt.pix.pixelformat = V4L2_PIX_FMT_NV12_COL128;
        single.fmt.pix.sizeimage = 5529600;
        AVBufferRef *ref = v4l2request_v4l2_buffer_alloc(&frames, &single);
        assert(ref && created.format.fmt.pix.sizeimage == 5533696);
        assert(single.fmt.pix.sizeimage == 5529600);
        av_buffer_unref(&ref);
        single.fmt.pix.sizeimage = UINT32_MAX;
        calls = 0;
        assert(!v4l2request_v4l2_buffer_alloc(&frames, &single));
        assert(calls == 0);
        struct v4l2_format multi = { .type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE };
        multi.fmt.pix_mp.pixelformat = V4L2_PIX_FMT_NV12MT_COL128;
        multi.fmt.pix_mp.num_planes = 2;
        multi.fmt.pix_mp.plane_fmt[0].sizeimage = 3686400;
        multi.fmt.pix_mp.plane_fmt[1].sizeimage = UINT32_MAX - 4095;
        calls = 0;
        assert(!v4l2request_v4l2_buffer_alloc(&frames, &multi));
        assert(calls == 0);
        uint32_t unchanged_formats[] = {
            v4l2_fourcc('N', 'C', '3', '0'),
            v4l2_fourcc('N', 'c', '3', '0'),
            V4L2_PIX_FMT_NV12,
        };
        for (unsigned i = 0; i < sizeof(unchanged_formats) / sizeof(unchanged_formats[0]); i++) {
            multi.fmt.pix_mp.pixelformat = unchanged_formats[i];
            multi.fmt.pix_mp.plane_fmt[1].sizeimage = 1843200;
            ref = v4l2request_v4l2_buffer_alloc(&frames, &multi);
            assert(ref && memcmp(&created.format, &multi, sizeof(multi)) == 0);
            av_buffer_unref(&ref);
        }
        puts("capture padding policy: 16 format/resolution cases, single-plane and overflow checks passed");
    }
    '''
    cfile = directory / 'capture_padding_test.c'
    binary = directory / 'capture_padding_test'
    cfile.write_text(harness)
    subprocess.run(['cc', '-std=gnu11', '-Wall', '-Wextra', '-Werror', str(cfile), '-o', str(binary)], check=True)
    subprocess.run([str(binary)], check=True)
