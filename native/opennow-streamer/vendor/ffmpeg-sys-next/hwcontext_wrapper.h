// Binds FFmpeg's per-API hwcontext structs from the real
// <libavutil/hwcontext_*.h> headers without pulling in the vendor SDKs. Each
// block is `__has_include`-gated on its libavutil header, so a build binds only
// what it ships. The vendor SDK header each one #includes is handled two ways:
//   * CUDA / D3D11 / D3D12 - suppressed via its include guard (CUDA: CUDA_VERSION;
//     D3D: __d3dNN_h__) with opaque handle / int-enum stand-ins inline; D3D is
//     also gated on <d3dNN.h> (Windows-only).
//   * Vulkan / VideoToolbox / VAAPI / QSV - SDK header not reliably present at
//     bindgen time, so a shim in hwcontext_stubs/ stands in for it.

// ── CUDA (hwcontext_cuda.h guards its <cuda.h> include with CUDA_VERSION) ──
#if __has_include(<libavutil/hwcontext_cuda.h>)
#define CUDA_VERSION 12000
typedef struct CUctx_st *CUcontext;
typedef struct CUstream_st *CUstream;
#include <libavutil/hwcontext_cuda.h>
#endif

// ── MediaCodec (no external SDK dependency) ───────────────────────────────
#if __has_include(<libavutil/hwcontext_mediacodec.h>)
#include <libavutil/hwcontext_mediacodec.h>
#endif

// ── D3D11VA (Windows only: gated on <d3d11.h>) ────────────────────────────
#if __has_include(<libavutil/hwcontext_d3d11va.h>) && __has_include(<d3d11.h>)
#define __d3d11_h__
typedef struct ID3D11Device ID3D11Device;
typedef struct ID3D11DeviceContext ID3D11DeviceContext;
typedef struct ID3D11VideoDevice ID3D11VideoDevice;
typedef struct ID3D11VideoContext ID3D11VideoContext;
typedef struct ID3D11Texture2D ID3D11Texture2D;
typedef unsigned int UINT;
#include <libavutil/hwcontext_d3d11va.h>
#endif

// ── D3D12VA (Windows only: gated on <d3d12.h>) ────────────────────────────
#if __has_include(<libavutil/hwcontext_d3d12va.h>) && __has_include(<d3d12.h>)
#define __d3d12_h__
#define __d3d12sdklayers_h__
#define __d3d12video_h__
typedef struct ID3D12Device ID3D12Device;
typedef struct ID3D12VideoDevice ID3D12VideoDevice;
typedef struct ID3D12Fence ID3D12Fence;
typedef struct ID3D12Resource ID3D12Resource;
typedef void *HANDLE;
typedef int DXGI_FORMAT;
// int-width flag enums; emit as u32 (ABI-identical, 4 bytes).
typedef unsigned int D3D12_RESOURCE_FLAGS;
typedef unsigned int D3D12_HEAP_FLAGS;
#include <libavutil/hwcontext_d3d12va.h>
#endif

// ── VideoToolbox (shim: hwcontext_stubs/VideoToolbox/VideoToolbox.h) ──────
#if __has_include(<libavutil/hwcontext_videotoolbox.h>)
#include <libavutil/hwcontext_videotoolbox.h>
#endif

// ── Vulkan (shim: hwcontext_stubs/vulkan/vulkan.h) ────────────────────────
#if __has_include(<libavutil/hwcontext_vulkan.h>)
#include <libavutil/hwcontext_vulkan.h>
#endif

// ── VAAPI (shim: hwcontext_stubs/va/va.h) ─────────────────────────────────
#if __has_include(<libavutil/hwcontext_vaapi.h>)
#include <libavutil/hwcontext_vaapi.h>
#endif

// ── QSV (shim: hwcontext_stubs/mfxvideo.h) ────────────────────────────────
#if __has_include(<libavutil/hwcontext_qsv.h>)
#include <libavutil/hwcontext_qsv.h>
#endif
