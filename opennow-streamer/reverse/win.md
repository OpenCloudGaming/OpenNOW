# Windows DXVA Video Decoder Research

This document details the video decoding architecture used by NVIDIA GeForce NOW on Windows, based on reverse engineering of the official client and implementation research.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Supported Codecs](#supported-codecs)
3. [DXVA2 API Usage](#dxva2-api-usage)
4. [H.264 (AVC) Decoder](#h264-avc-decoder)
5. [H.265 (HEVC) Decoder](#h265-hevc-decoder)
6. [AV1 Decoder](#av1-decoder)
7. [Reference Frame Management](#reference-frame-management)
8. [Picture Order Count (POC)](#picture-order-count-poc)
9. [Bitstream Formatting](#bitstream-formatting)
10. [Performance Optimizations](#performance-optimizations)

---

## Architecture Overview

### Official GFN Client Components

The GeForce NOW client uses several key DLLs for video decoding:

| DLL | Purpose | Size |
|-----|---------|------|
| `Geronimo.dll` | Main video streaming/decoding engine with DXVADecoder | 28 MB |
| `Bifrost2.dll` | Network streaming protocol, codec negotiation | 21 MB |
| `NvPixels.dll` | NVIDIA pixel processing library | 2.6 MB |
| `gfnmp4mft.dll` | GFN MP4 Media Foundation Transform | 1.3 MB |
| `gfnspfbc.dll` | Sparse Pixel Block Compression | 2.5 MB |

### Decoder Initialization Flow

From `geronimo.log`:

```
1. ConfigureStreamerVideoSettings is called
2. Create DX11 decoder asynchronously  
3. Initialize DX11AsyncVideoFrameRenderer
4. Set adaptive queue and timestamp rendering
5. Query DXVADecoder for codec capabilities
6. Select optimal codec based on resolution/FPS
```

### Key Classes Identified

- `DXVADecoder` - Main DXVA2 hardware decoder wrapper
- `DX11AsyncVideoFrameRenderer` - Async frame rendering with Vsync tracking
- `AsyncVideoFrameRenderer` - Timestamp and queue management

---

## Supported Codecs

### Codec Capabilities (from DXVADecoder logs)

| Codec | Profile | Max Resolution SDR | Max Resolution HDR | Max FPS |
|-------|---------|-------------------|-------------------|---------|
| H.264 | High | 3840x2160 | N/A | 120 |
| HEVC | Main | 3840x2160 | 3840x2160 | 120 |
| HEVC | Main10 | 3840x2160 | 7680x4320 | 120 |
| AV1 | Main | 3840x2160 | 3840x2160 | 60 |

### Codec Selection Priority

For high frame rate (240+ FPS): HEVC > H.264
For HDR content: HEVC Main10 (10-bit)
For compatibility: H.264 (widest hardware support)

### Decoder Quality Scores

From client logs:
- Decoder Score: 0.9999 (excellent)
- GPU Performance: 0.9039
- Overall Stream Quality: 0.9039

---

## DXVA2 API Usage

### D3D11 Video API Interfaces

The client uses these COM interfaces for hardware decoding:

```cpp
ID3D11Device           // Base D3D11 device
ID3D11VideoDevice      // Video decoding device
ID3D11VideoContext     // Video decoding context
ID3D11VideoDecoder     // Decoder instance
ID3D11VideoDecoderOutputView  // Output surface view
```

### Device Creation

```cpp
// Flags for video decoding support
UINT flags = D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT;

D3D11CreateDevice(
    nullptr,                    // Default adapter
    D3D_DRIVER_TYPE_HARDWARE,
    nullptr,
    flags,
    featureLevels,             // 12.1, 12.0, 11.1, 11.0
    D3D11_SDK_VERSION,
    &device,
    &featureLevel,
    &context
);

// Enable multithread protection (required for async decode)
ID3D11Multithread* mt;
device->QueryInterface(&mt);
mt->SetMultithreadProtected(TRUE);
```

### Decoder Profile GUIDs

```cpp
// H.264/AVC
D3D11_DECODER_PROFILE_H264_VLD_NOFGT = {0x1b81be68-0xa0c7-11d3-b984-00c04f2e73c5}

// HEVC/H.265
D3D11_DECODER_PROFILE_HEVC_VLD_MAIN   = {0x5b11d51b-2f4c-4452-bcc3-09f2a1160cc0}
D3D11_DECODER_PROFILE_HEVC_VLD_MAIN10 = {0x107af0e0-ef1a-4d19-aba8-67a163073d13}

// AV1
D3D11_DECODER_PROFILE_AV1_VLD_PROFILE0 = {0xb8be4ccb-cf53-46ba-8d59-d6b8a6da5d2a}
```

### Output Texture Array (RTArray)

The client creates a texture array for zero-copy decoding:

```cpp
D3D11_TEXTURE2D_DESC textureDesc = {
    .Width = width,
    .Height = height,
    .MipLevels = 1,
    .ArraySize = 25,  // 25 surfaces for high bitrate 4K
    .Format = DXGI_FORMAT_NV12,  // or P010 for HDR
    .SampleDesc = { 1, 0 },
    .Usage = D3D11_USAGE_DEFAULT,
    .BindFlags = D3D11_BIND_DECODER,
};
```

### ConfigBitstreamRaw Settings

The decoder configuration's `ConfigBitstreamRaw` field determines bitstream format:

| Value | Format | Description |
|-------|--------|-------------|
| 1 | Annex-B | Include start codes (0x000001) |
| 2 | Raw NAL | No start codes, raw NAL units |

The client prefers `ConfigBitstreamRaw=2` (short slice format) when available.

---

## H.264 (AVC) Decoder

### NAL Unit Types (ITU-T H.264 Table 7-1)

```
0     - Unspecified
1     - Coded slice of a non-IDR picture (P/B frame)
2     - Coded slice data partition A
3     - Coded slice data partition B
4     - Coded slice data partition C
5     - Coded slice of an IDR picture (I frame, keyframe)
6     - SEI (Supplemental Enhancement Information)
7     - SPS (Sequence Parameter Set)
8     - PPS (Picture Parameter Set)
9     - AUD (Access Unit Delimiter)
10    - End of sequence
11    - End of stream
12    - Filler data
13-23 - Reserved
24-31 - Unspecified
```

### SPS (Sequence Parameter Set) Structure

Critical fields for DXVA:

```c
struct H264_SPS {
    uint8_t  profile_idc;           // 66=Baseline, 77=Main, 100=High
    uint8_t  level_idc;             // 30-52 (3.0 to 5.2)
    uint8_t  sps_id;                // 0-31
    uint8_t  chroma_format_idc;     // 0=mono, 1=4:2:0, 2=4:2:2, 3=4:4:4
    uint8_t  bit_depth_luma;        // 8-14
    uint8_t  bit_depth_chroma;      // 8-14
    
    // POC calculation parameters
    uint8_t  log2_max_frame_num;              // 4-16
    uint8_t  pic_order_cnt_type;              // 0, 1, or 2
    uint8_t  log2_max_pic_order_cnt_lsb;      // 4-16 (if type=0)
    bool     delta_pic_order_always_zero;     // (if type=1)
    int32_t  offset_for_non_ref_pic;          // (if type=1)
    int32_t  offset_for_top_to_bottom_field;  // (if type=1)
    uint8_t  num_ref_frames_in_poc_cycle;     // (if type=1)
    int32_t  offset_for_ref_frame[256];       // (if type=1)
    
    uint8_t  max_num_ref_frames;    // Max reference frames in DPB
    uint16_t pic_width_in_mbs;      // Width / 16
    uint16_t pic_height_in_mbs;     // Height / 16
    bool     frame_mbs_only;        // No interlaced support if true
    bool     mb_adaptive_frame_field; // MBAFF
    bool     direct_8x8_inference;
    
    // Cropping
    bool     frame_cropping;
    uint16_t crop_left, crop_right, crop_top, crop_bottom;
};
```

### PPS (Picture Parameter Set) Structure

```c
struct H264_PPS {
    uint8_t  pps_id;                    // 0-255
    uint8_t  sps_id;                    // Reference to SPS
    bool     entropy_coding_mode;       // 0=CAVLC, 1=CABAC
    bool     bottom_field_pic_order;
    uint8_t  num_slice_groups;
    uint8_t  num_ref_idx_l0_active;     // Default L0 ref count
    uint8_t  num_ref_idx_l1_active;     // Default L1 ref count
    bool     weighted_pred;
    uint8_t  weighted_bipred_idc;       // 0=none, 1=explicit, 2=implicit
    int8_t   pic_init_qp;               // -26 to +25
    int8_t   pic_init_qs;
    int8_t   chroma_qp_index_offset;
    bool     deblocking_filter_control;
    bool     constrained_intra_pred;
    bool     redundant_pic_cnt_present;
    bool     transform_8x8_mode;
    int8_t   second_chroma_qp_offset;
};
```

### DXVA_PicParams_H264 Structure

This is the critical structure sent to the hardware decoder:

```c
// Size: approximately 296 bytes
typedef struct _DXVA_PicParams_H264 {
    USHORT wFrameWidthInMbsMinus1;
    USHORT wFrameHeightInMbsMinus1;
    
    DXVA_PicEntry_H264 CurrPic;           // Current picture
    UCHAR  num_ref_frames;
    
    // Bitfield union for flags
    union {
        struct {
            USHORT field_pic_flag : 1;
            USHORT MbsffFrameFlag : 1;
            USHORT residual_colour_transform_flag : 1;
            USHORT sp_for_switch_flag : 1;
            USHORT chroma_format_idc : 2;
            USHORT RefPicFlag : 1;
            USHORT constrained_intra_pred_flag : 1;
            USHORT weighted_pred_flag : 1;
            USHORT weighted_bipred_idc : 2;
            USHORT MbsConsecutiveFlag : 1;
            USHORT frame_mbs_only_flag : 1;
            USHORT transform_8x8_mode_flag : 1;
            USHORT MinLumaBipredSize8x8Flag : 1;
            USHORT IntraPicFlag : 1;
        };
        USHORT wBitFields;
    };
    
    UCHAR  bit_depth_luma_minus8;
    UCHAR  bit_depth_chroma_minus8;
    USHORT Reserved16Bits;
    UINT   StatusReportFeedbackNumber;
    
    // Reference frame list (16 entries)
    DXVA_PicEntry_H264 RefFrameList[16];
    INT    CurrFieldOrderCnt[2];          // Top/Bottom field POC
    INT    FieldOrderCntList[16][2];      // POC for each reference
    
    CHAR   pic_init_qp_minus26;
    CHAR   chroma_qp_index_offset;
    CHAR   second_chroma_qp_index_offset;
    UCHAR  ContinuationFlag;
    CHAR   pic_init_qs_minus26;
    UCHAR  num_ref_idx_l0_active_minus1;
    UCHAR  num_ref_idx_l1_active_minus1;
    UCHAR  Reserved8BitsA;
    
    USHORT FrameNumList[16];              // frame_num for each ref
    UINT   UsedForReferenceFlags;         // Bitmask: which refs are used
    USHORT NonExistingFrameFlags;
    USHORT frame_num;
    
    UCHAR  log2_max_frame_num_minus4;
    UCHAR  pic_order_cnt_type;
    UCHAR  log2_max_pic_order_cnt_lsb_minus4;
    UCHAR  delta_pic_order_always_zero_flag;
    UCHAR  direct_8x8_inference_flag;
    UCHAR  entropy_coding_mode_flag;
    UCHAR  pic_order_present_flag;
    UCHAR  num_slice_groups_minus1;
    
    UCHAR  slice_group_map_type;
    UCHAR  deblocking_filter_control_present_flag;
    UCHAR  redundant_pic_cnt_present_flag;
    UCHAR  Reserved8BitsB;
    USHORT slice_group_change_rate_minus1;
    
    // Slice group map (optional, for FMO)
    UCHAR  SliceGroupMap[810];
} DXVA_PicParams_H264;

// Picture entry format
typedef struct _DXVA_PicEntry_H264 {
    union {
        struct {
            UCHAR Index7Bits : 7;        // Surface index (0-127)
            UCHAR AssociatedFlag : 1;    // 1=bottom field, 0=top/frame
        };
        UCHAR bPicEntry;
    };
} DXVA_PicEntry_H264;
```

### H.264 POC Calculation

H.264 has THREE different POC calculation methods (unlike HEVC which has one):

#### Type 0 (Most Common)

```c
// Uses pic_order_cnt_lsb from slice header
int MaxPicOrderCntLsb = 1 << log2_max_pic_order_cnt_lsb;

if (pic_order_cnt_lsb < prevPicOrderCntLsb && 
    (prevPicOrderCntLsb - pic_order_cnt_lsb) >= MaxPicOrderCntLsb / 2) {
    PicOrderCntMsb = prevPicOrderCntMsb + MaxPicOrderCntLsb;
} else if (pic_order_cnt_lsb > prevPicOrderCntLsb &&
           (pic_order_cnt_lsb - prevPicOrderCntLsb) > MaxPicOrderCntLsb / 2) {
    PicOrderCntMsb = prevPicOrderCntMsb - MaxPicOrderCntLsb;
} else {
    PicOrderCntMsb = prevPicOrderCntMsb;
}

TopFieldOrderCnt = PicOrderCntMsb + pic_order_cnt_lsb;
BottomFieldOrderCnt = TopFieldOrderCnt + delta_pic_order_cnt_bottom;
```

#### Type 1 (Explicit Offsets)

```c
// Uses delta_pic_order_cnt[0] and delta_pic_order_cnt[1]
if (frame_num < prevFrameNum) {
    FrameNumOffset = prevFrameNumOffset + MaxFrameNum;
} else {
    FrameNumOffset = prevFrameNumOffset;
}

int absFrameNum = FrameNumOffset + frame_num;
int expectedPicOrderCnt = 0;

if (num_ref_frames_in_pic_order_cnt_cycle != 0) {
    int picOrderCntCycleCnt = (absFrameNum - 1) / num_ref_frames_in_pic_order_cnt_cycle;
    int frameNumInPicOrderCntCycle = (absFrameNum - 1) % num_ref_frames_in_pic_order_cnt_cycle;
    
    expectedPicOrderCnt = picOrderCntCycleCnt * ExpectedDeltaPerPicOrderCntCycle;
    for (int i = 0; i <= frameNumInPicOrderCntCycle; i++) {
        expectedPicOrderCnt += offset_for_ref_frame[i];
    }
}

if (!nal_ref_idc) {
    expectedPicOrderCnt += offset_for_non_ref_pic;
}

TopFieldOrderCnt = expectedPicOrderCnt + delta_pic_order_cnt[0];
BottomFieldOrderCnt = TopFieldOrderCnt + offset_for_top_to_bottom_field + delta_pic_order_cnt[1];
```

#### Type 2 (Simple Sequential)

```c
// Simplest - POC derived directly from frame_num
if (frame_num < prevFrameNum) {
    FrameNumOffset = prevFrameNumOffset + MaxFrameNum;
} else {
    FrameNumOffset = prevFrameNumOffset;
}

int tempPicOrderCnt;
if (IDR) {
    tempPicOrderCnt = 0;
} else if (!nal_ref_idc) {
    tempPicOrderCnt = 2 * (FrameNumOffset + frame_num) - 1;
} else {
    tempPicOrderCnt = 2 * (FrameNumOffset + frame_num);
}

TopFieldOrderCnt = tempPicOrderCnt;
BottomFieldOrderCnt = tempPicOrderCnt;
```

### H.264 Slice Header

Key fields for DXVA:

```c
struct H264_SliceHeader {
    uint32_t first_mb_in_slice;     // Macroblock address
    uint8_t  slice_type;            // 0=P, 1=B, 2=I, 3=SP, 4=SI
    uint8_t  pps_id;
    uint16_t frame_num;             // For reference management
    
    // Field/frame info (if !frame_mbs_only_flag)
    bool     field_pic_flag;
    bool     bottom_field_flag;
    
    // IDR specific
    uint16_t idr_pic_id;            // For IDR pictures
    
    // POC info (depends on pic_order_cnt_type)
    uint16_t pic_order_cnt_lsb;     // Type 0
    int32_t  delta_pic_order_cnt_bottom;  // Type 0, if present
    int32_t  delta_pic_order_cnt[2];      // Type 1
    
    // Reference picture management
    uint8_t  num_ref_idx_l0_active;
    uint8_t  num_ref_idx_l1_active;
    
    // Reference picture list modification
    bool     ref_pic_list_modification_flag_l0;
    bool     ref_pic_list_modification_flag_l1;
    
    // Decoded reference picture marking
    bool     no_output_of_prior_pics_flag;  // IDR only
    bool     long_term_reference_flag;       // IDR only
    bool     adaptive_ref_pic_marking_mode_flag;
};
```

### DXVA_Slice_H264_Short Structure

```c
typedef struct _DXVA_Slice_H264_Short {
    UINT   BSNALunitDataLocation;   // Offset in bitstream buffer
    UINT   SliceBytesInBuffer;      // Size of this slice
    USHORT wBadSliceChopping;       // 0 = not chopped
} DXVA_Slice_H264_Short;
```

---

## H.265 (HEVC) Decoder

### NAL Unit Types (ITU-T H.265 Table 7-1)

```
0-9   - VCL NAL units (slices)
  0   - TRAIL_N (trailing, non-reference)
  1   - TRAIL_R (trailing, reference)
  19  - IDR_W_RADL (IDR with RADL)
  20  - IDR_N_LP (IDR without leading pictures)
  21  - CRA_NUT (Clean Random Access)
  
32-40 - Non-VCL NAL units
  32  - VPS_NUT (Video Parameter Set)
  33  - SPS_NUT (Sequence Parameter Set)
  34  - PPS_NUT (Picture Parameter Set)
  35  - AUD_NUT (Access Unit Delimiter)
  39  - PREFIX_SEI_NUT
  40  - SUFFIX_SEI_NUT
```

### HEVC VPS/SPS/PPS

See `hevc_parser.rs` for complete structure definitions.

### DXVA_PicParams_HEVC Structure

```c
// Size: 232 bytes (packed)
typedef struct _DXVA_PicParams_HEVC {
    USHORT PicWidthInMinCbsY;
    USHORT PicHeightInMinCbsY;
    
    // Bitfield: chroma_format_idc, bit_depth, log2_max_poc_lsb, etc.
    USHORT wFormatAndSequenceInfoFlags;
    
    DXVA_PicEntry_HEVC CurrPic;
    UCHAR  sps_max_dec_pic_buffering_minus1;
    UCHAR  log2_min_luma_coding_block_size_minus3;
    // ... more SPS parameters ...
    
    UINT   dwCodingParamToolFlags;    // Tool flags bitfield
    UINT   dwCodingSettingPicturePropertyFlags;
    
    CHAR   pps_cb_qp_offset;
    CHAR   pps_cr_qp_offset;
    UCHAR  num_tile_columns_minus1;
    UCHAR  num_tile_rows_minus1;
    USHORT column_width_minus1[19];
    USHORT row_height_minus1[21];
    
    INT    CurrPicOrderCntVal;
    DXVA_PicEntry_HEVC RefPicList[15];
    INT    PicOrderCntValList[15];
    
    UCHAR  RefPicSetStCurrBefore[8];
    UCHAR  RefPicSetStCurrAfter[8];
    UCHAR  RefPicSetLtCurr[8];
    
    UINT   StatusReportFeedbackNumber;
} DXVA_PicParams_HEVC;
```

### HEVC POC Calculation (Single Method)

Unlike H.264, HEVC uses only one POC calculation method:

```c
// From ITU-T H.265 section 8.3.1
int MaxPicOrderCntLsb = 1 << log2_max_pic_order_cnt_lsb;

if (IDR) {
    PicOrderCntMsb = 0;
    PicOrderCntVal = 0;
} else {
    if (pic_order_cnt_lsb < prevPicOrderCntLsb &&
        (prevPicOrderCntLsb - pic_order_cnt_lsb) >= MaxPicOrderCntLsb / 2) {
        PicOrderCntMsb = prevPicOrderCntMsb + MaxPicOrderCntLsb;
    } else if (pic_order_cnt_lsb > prevPicOrderCntLsb &&
               (pic_order_cnt_lsb - prevPicOrderCntLsb) > MaxPicOrderCntLsb / 2) {
        PicOrderCntMsb = prevPicOrderCntMsb - MaxPicOrderCntLsb;
    } else {
        PicOrderCntMsb = prevPicOrderCntMsb;
    }
    
    PicOrderCntVal = PicOrderCntMsb + pic_order_cnt_lsb;
}
```

---

## AV1 Decoder

### AV1 OBU Types

```
0 - Reserved
1 - Sequence Header
2 - Temporal Delimiter
3 - Frame Header
4 - Tile Group
5 - Metadata
6 - Frame (combined header + tile group)
7 - Redundant Frame Header
8-14 - Reserved
15 - Padding
```

AV1 uses the `DXVA_PicParams_AV1` structure (approximately 400 bytes).

---

## Reference Frame Management

### Decoded Picture Buffer (DPB)

The DPB stores decoded pictures that may be used as references:

```c
struct DPB_Entry {
    uint8_t  surface_index;    // Texture array index
    int32_t  poc;              // Picture Order Count
    uint16_t frame_num;        // H.264: frame_num from slice
    bool     is_reference;     // Can be used as reference
    bool     is_long_term;     // Long-term reference (persistent)
    bool     is_output;        // Has been output for display
};
```

### DPB Size Limits

| Codec | Level | Max DPB Frames |
|-------|-------|----------------|
| H.264 | 4.0 | 8 (1080p) |
| H.264 | 4.1 | 4 (1080p) or 8 (720p) |
| H.264 | 5.1 | 16 (1080p) |
| HEVC | 4.0 | 6 (1080p) |
| HEVC | 5.1 | 16 (4K) |

### Surface Allocation Strategy

```c
// Recommended surface count formula
surface_count = dpb_size + 3;  // DPB + decode + display + margin

// GFN client uses:
// 25 surfaces for high bitrate 4K streams
// 20 surfaces for 1080p streams
```

### Avoiding Surface Reuse Conflicts

When allocating a surface for decoding, ensure it's not currently in the DPB:

```c
uint32_t get_free_surface() {
    for (int i = 0; i < surface_count; i++) {
        if (!is_in_dpb(i)) {
            return i;
        }
    }
    // Evict oldest entry if all surfaces in use
    evict_oldest_dpb_entry();
    return get_free_surface();
}
```

---

## Bitstream Formatting

### Annex-B Format

Start codes delimit NAL units:

```
0x00 0x00 0x01        - 3-byte start code
0x00 0x00 0x00 0x01   - 4-byte start code (before SPS/PPS/IDR)
```

### Emulation Prevention

To prevent false start codes in payload:

```
0x00 0x00 0x00 → 0x00 0x00 0x03 0x00
0x00 0x00 0x01 → 0x00 0x00 0x03 0x01
0x00 0x00 0x02 → 0x00 0x00 0x03 0x02
0x00 0x00 0x03 → 0x00 0x00 0x03 0x03
```

Parser must remove `0x03` emulation prevention bytes before parsing.

### Buffer Alignment

DXVA requires 128-byte alignment for bitstream buffers:

```c
size_t aligned_size = (raw_size + 127) & ~127;
// Pad with zeros to reach aligned size
```

---

## Performance Optimizations

### Zero-Copy Rendering

The GFN client achieves zero-copy by:

1. Creating output textures with `D3D11_BIND_DECODER` flag
2. Using texture arrays (RTArray) for all surfaces
3. Passing texture directly to renderer (no CPU staging)
4. Using `D3D11Texture2D` directly with wgpu/DirectX

### Async Frame Rendering

From client logs, `DX11AsyncVideoFrameRenderer` provides:
- Render-on-demand capability
- Vsync tracking (monitors missed Vsyncs)
- Timestamp rendering for latency measurement
- Adaptive queue management

### Frame Timing

The client tracks:
- Decode time (packet receive to decode complete)
- Present time (decode complete to display)
- Vsync alignment
- Queue depth

### Multi-threaded Decoding

```c
// Enable multithread protection
ID3D11Multithread* mt;
device->QueryInterface(&mt);
mt->SetMultithreadProtected(TRUE);
```

This allows:
- Decode thread: Submits decode commands
- Render thread: Reads decoded textures
- No explicit synchronization needed (D3D11 handles it)

---

## Implementation Checklist

### H.264 Decoder Implementation

- [ ] NAL unit parser (types 1-8)
- [ ] SPS parser with all fields
- [ ] PPS parser with all fields
- [ ] Slice header parser
- [ ] POC calculation (all 3 types)
- [ ] DXVA_PicParams_H264 structure
- [ ] DXVA_Slice_H264_Short structure
- [ ] Reference frame list construction
- [ ] DPB management with frame_num
- [ ] Long-term reference support
- [ ] CABAC/CAVLC handling (implicit)

### HEVC Decoder (Implemented)

- [x] NAL unit parser
- [x] VPS/SPS/PPS parsing
- [x] Slice header parsing
- [x] POC calculation
- [x] DXVA_PicParams_HEVC structure
- [x] DXVA_Slice_HEVC_Short structure
- [x] Reference picture set management
- [x] DPB with POC-based ordering
- [x] Zero-copy output

---

## References

- ITU-T H.264 (ISO/IEC 14496-10) - AVC Specification
- ITU-T H.265 (ISO/IEC 23008-2) - HEVC Specification
- Microsoft DXVA 2.0 Specification
- NVIDIA GeForce NOW Client (reverse engineered logs)
- FFmpeg libavcodec (reference implementation)
