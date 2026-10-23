#include <assert.h>
#include <stddef.h>
#include <stdint.h>
#include <ffnvcodec/dynlink_cuda.h>
#include <ffnvcodec/dynlink_cuviddec.h>

static CUcontext current;
static int depth;

_Static_assert(sizeof(CUVIDDECODECAPS) == 88, "NVDEC capabilities ABI changed");
_Static_assert(offsetof(CUVIDDECODECAPS, bIsSupported) == 24, "support offset changed");
_Static_assert(offsetof(CUVIDDECODECAPS, nMaxMBCount) == 36, "macroblock offset changed");
_Static_assert(offsetof(CUVIDDECODECAPS, nMinWidth) == 40, "minimum offset changed");

CUresult CUDAAPI cuCtxPushCurrent_v2(CUcontext context)
{
    assert(context == (CUcontext)(uintptr_t)0x1234);
    assert(depth == 0);
    current = context;
    depth++;
    return CUDA_SUCCESS;
}

CUresult CUDAAPI cuCtxPopCurrent_v2(CUcontext *context)
{
    assert(depth == 1);
    *context = current;
    current = NULL;
    depth--;
    return CUDA_SUCCESS;
}

int mock_context_depth(void)
{
    return depth;
}

CUresult CUDAAPI cuvidGetDecoderCaps(CUVIDDECODECAPS *caps)
{
    assert(depth == 1);
    assert(current == (CUcontext)(uintptr_t)0x1234);
    assert(caps->eChromaFormat == cudaVideoChromaFormat_420);
    assert(caps->nBitDepthMinus8 == 0);
    for (size_t i = 0; i < 3; i++) assert(caps->reserved1[i] == 0);
    for (size_t i = 0; i < 9; i++) assert(caps->reserved3[i] == 0);
    for (size_t i = 0; i < 3; i++) assert(caps->reserved4[i] == 0);
    assert(caps->bIsSupported == 0);
    assert(caps->nOutputFormatMask == 0);
    assert(caps->nMaxWidth == 0 && caps->nMaxHeight == 0);
    if (caps->eCodecType == cudaVideoCodec_AV1) return CUDA_SUCCESS;
    if (caps->eCodecType == cudaVideoCodec_HEVC) return (CUresult)801;
    assert(caps->eCodecType == cudaVideoCodec_H264);
    caps->bIsSupported = 1;
    caps->nOutputFormatMask = 1 << cudaVideoSurfaceFormat_NV12;
    caps->nMinWidth = 48;
    caps->nMinHeight = 16;
    caps->nMaxWidth = 4096;
    caps->nMaxHeight = 4096;
    caps->nMaxMBCount = 32768;
    return CUDA_SUCCESS;
}
