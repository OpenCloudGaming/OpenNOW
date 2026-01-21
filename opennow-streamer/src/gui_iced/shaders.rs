//! GPU Shaders for video rendering
//!
//! WGSL shaders for YUV to RGB conversion on the GPU.
//! Supports BT.709 and BT.2020 color matrices with Limited/Full range conversion.

/// WGSL shader for NV12 format (D3D11 on Windows, VideoToolbox on macOS)
/// Primary GPU path - Y plane (R8) + interleaved UV plane (Rg8)
/// 
/// Supports dynamic color parameters via uniform:
/// - color_range: 0 = Limited Range (TV), 1 = Full Range (PC)
/// - color_space: 0 = BT.709 (SDR), 1 = BT.2020 (HDR)
pub const NV12_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
};

struct ColorParams {
    // 0 = Limited Range, 1 = Full Range
    color_range: u32,
    // 0 = BT.709, 1 = BT.2020
    color_space: u32,
    // Padding to ensure 16-byte alignment
    _padding: vec2<u32>,
};

@group(0) @binding(0)
var y_texture: texture_2d<f32>;
@group(0) @binding(1)
var uv_texture: texture_2d<f32>;
@group(0) @binding(2)
var video_sampler: sampler;
@group(0) @binding(3)
var<uniform> color_params: ColorParams;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );

    var tex_coords = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );

    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    output.tex_coord = tex_coords[vertex_index];
    return output;
}

// YUV to RGB conversion with color space and range support
fn yuv_to_rgb(y_raw: f32, u_raw: f32, v_raw: f32, color_range: u32, color_space: u32) -> vec3<f32> {
    var y: f32;
    var u: f32;
    var v: f32;

    if (color_range == 1u) {
        // Full Range (PC): Y 0-255, UV 0-255, neutral UV at 128
        y = y_raw;
        u = u_raw - 0.5;
        v = v_raw - 0.5;
    } else {
        // Limited Range (TV): Y 16-235, UV 16-240, neutral UV at 128
        // Y: offset 16, range 219 (235-16)
        // UV: offset 128 (neutral), range 224 (240-16)
        y = (y_raw - 16.0/255.0) * (255.0/219.0);
        // UV neutral point is 128/255 = 0.502, range is 224/255
        u = (u_raw - 128.0/255.0) * (255.0/224.0);
        v = (v_raw - 128.0/255.0) * (255.0/224.0);
    }

    var r: f32;
    var g: f32;
    var b: f32;

    if (color_space == 1u) {
        // BT.2020 color matrix coefficients (for HDR/Wide Color Gamut)
        // R = Y + 1.4746 * V
        // G = Y - 0.1646 * U - 0.5714 * V
        // B = Y + 1.8814 * U
        r = y + 1.4746 * v;
        g = y - 0.1646 * u - 0.5714 * v;
        b = y + 1.8814 * u;
    } else {
        // BT.709 color matrix coefficients (for SDR)
        // R = Y + 1.5748 * V
        // G = Y - 0.1873 * U - 0.4681 * V
        // B = Y + 1.8556 * U
        r = y + 1.5748 * v;
        g = y - 0.1873 * u - 0.4681 * v;
        b = y + 1.8556 * u;
    }

    return vec3<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0));
}

// BT.709 EOTF: Convert gamma-encoded RGB to linear RGB
// The video is encoded with BT.709 OETF, we need to decode it to linear
// for correct display on sRGB monitors (wgpu sRGB surface will re-encode)
fn bt709_eotf(v: f32) -> f32 {
    // BT.709 transfer function (same as sRGB for practical purposes)
    // Linear segment for very dark values, power function otherwise
    if (v < 0.081) {
        return v / 4.5;
    } else {
        return pow((v + 0.099) / 1.099, 1.0 / 0.45);
    }
}

fn linearize_rgb(rgb: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        bt709_eotf(rgb.r),
        bt709_eotf(rgb.g),
        bt709_eotf(rgb.b)
    );
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Sample Y (full res) and UV (half res, interleaved)
    let y_raw = textureSample(y_texture, video_sampler, input.tex_coord).r;
    let uv = textureSample(uv_texture, video_sampler, input.tex_coord);

    // NV12 format: U in R channel, V in G channel
    let rgb = yuv_to_rgb(y_raw, uv.r, uv.g, color_params.color_range, color_params.color_space);

    // Linearize: BT.709 gamma-encoded RGB -> linear RGB
    // The sRGB surface will then apply sRGB encoding for correct display
    let linear_rgb = linearize_rgb(rgb);

    return vec4<f32>(linear_rgb, 1.0);
}
"#;

/// WGSL shader for YUV420P format (3 separate planes)
/// Supports dynamic color parameters via uniform:
/// - color_range: 0 = Limited Range (TV), 1 = Full Range (PC)
/// - color_space: 0 = BT.709 (SDR), 1 = BT.2020 (HDR)
pub const YUV420P_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
};

struct ColorParams {
    // 0 = Limited Range, 1 = Full Range
    color_range: u32,
    // 0 = BT.709, 1 = BT.2020
    color_space: u32,
    // Padding to ensure 16-byte alignment
    _padding: vec2<u32>,
};

// YUV planar textures (Y = full res, U/V = half res)
@group(0) @binding(0)
var y_texture: texture_2d<f32>;
@group(0) @binding(1)
var u_texture: texture_2d<f32>;
@group(0) @binding(2)
var v_texture: texture_2d<f32>;
@group(0) @binding(3)
var video_sampler: sampler;
@group(0) @binding(4)
var<uniform> color_params: ColorParams;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );

    var tex_coords = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );

    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    output.tex_coord = tex_coords[vertex_index];
    return output;
}

// YUV to RGB conversion with color space and range support
fn yuv_to_rgb(y_raw: f32, u_raw: f32, v_raw: f32, color_range: u32, color_space: u32) -> vec3<f32> {
    var y: f32;
    var u: f32;
    var v: f32;

    if (color_range == 1u) {
        // Full Range (PC): Y 0-255, UV 0-255, neutral UV at 128
        y = y_raw;
        u = u_raw - 0.5;
        v = v_raw - 0.5;
    } else {
        // Limited Range (TV): Y 16-235, UV 16-240, neutral UV at 128
        // Y: offset 16, range 219 (235-16)
        // UV: offset 128 (neutral), range 224 (240-16)
        y = (y_raw - 16.0/255.0) * (255.0/219.0);
        // UV neutral point is 128/255 = 0.502, range is 224/255
        u = (u_raw - 128.0/255.0) * (255.0/224.0);
        v = (v_raw - 128.0/255.0) * (255.0/224.0);
    }

    var r: f32;
    var g: f32;
    var b: f32;

    if (color_space == 1u) {
        // BT.2020 color matrix coefficients (for HDR/Wide Color Gamut)
        r = y + 1.4746 * v;
        g = y - 0.1646 * u - 0.5714 * v;
        b = y + 1.8814 * u;
    } else {
        // BT.709 color matrix coefficients (for SDR)
        r = y + 1.5748 * v;
        g = y - 0.1873 * u - 0.4681 * v;
        b = y + 1.8556 * u;
    }

    return vec3<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0));
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Sample Y, U, V planes
    let y_raw = textureSample(y_texture, video_sampler, input.tex_coord).r;
    let u_raw = textureSample(u_texture, video_sampler, input.tex_coord).r;
    let v_raw = textureSample(v_texture, video_sampler, input.tex_coord).r;

    let rgb = yuv_to_rgb(y_raw, u_raw, v_raw, color_params.color_range, color_params.color_space);

    return vec4<f32>(rgb, 1.0);
}
"#;

/// WGSL shader for P010 format (10-bit HDR, semi-planar)
/// Y plane (R16) + interleaved UV plane (Rg16)
/// Uses the same color conversion as NV12 but with 10-bit precision
pub const P010_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
};

struct ColorParams {
    // 0 = Limited Range, 1 = Full Range
    color_range: u32,
    // 0 = BT.709, 1 = BT.2020
    color_space: u32,
    // Padding to ensure 16-byte alignment
    _padding: vec2<u32>,
};

@group(0) @binding(0)
var y_texture: texture_2d<f32>;
@group(0) @binding(1)
var uv_texture: texture_2d<f32>;
@group(0) @binding(2)
var video_sampler: sampler;
@group(0) @binding(3)
var<uniform> color_params: ColorParams;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );

    var tex_coords = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );

    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    output.tex_coord = tex_coords[vertex_index];
    return output;
}

// YUV to RGB conversion with color space and range support (same as NV12/YUV420P)
fn yuv_to_rgb(y_raw: f32, u_raw: f32, v_raw: f32, color_range: u32, color_space: u32) -> vec3<f32> {
    var y: f32;
    var u: f32;
    var v: f32;

    // P010 is typically 10-bit in 16-bit container, but normalized to 0-1 by GPU
    // The texture sampling already gives us normalized values
    if (color_range == 1u) {
        // Full Range: Y 0-1, UV 0-1, neutral UV at 0.5
        y = y_raw;
        u = u_raw - 0.5;
        v = v_raw - 0.5;
    } else {
        // Limited Range (10-bit): Y 64-940, UV 64-960 (in 1024 scale)
        // Y: offset 64, range 876 (940-64)
        // UV: neutral is 512/1023, range is 896 (960-64)
        y = (y_raw - 64.0/1023.0) * (1023.0/876.0);
        // UV neutral point is 512/1023 = 0.5, range is 896/1023
        u = (u_raw - 512.0/1023.0) * (1023.0/896.0);
        v = (v_raw - 512.0/1023.0) * (1023.0/896.0);
    }

    var r: f32;
    var g: f32;
    var b: f32;

    if (color_space == 1u) {
        // BT.2020 color matrix (typical for HDR P010 content)
        r = y + 1.4746 * v;
        g = y - 0.1646 * u - 0.5714 * v;
        b = y + 1.8814 * u;
    } else {
        // BT.709 color matrix
        r = y + 1.5748 * v;
        g = y - 0.1873 * u - 0.4681 * v;
        b = y + 1.8556 * u;
    }

    return vec3<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0));
}

// BT.709 EOTF: Convert gamma-encoded RGB to linear RGB
fn bt709_eotf(v: f32) -> f32 {
    if (v < 0.081) {
        return v / 4.5;
    } else {
        return pow((v + 0.099) / 1.099, 1.0 / 0.45);
    }
}

fn linearize_rgb(rgb: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        bt709_eotf(rgb.r),
        bt709_eotf(rgb.g),
        bt709_eotf(rgb.b)
    );
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Sample Y (full res, 16-bit) and UV (half res, interleaved 16-bit)
    let y_raw = textureSample(y_texture, video_sampler, input.tex_coord).r;
    let uv = textureSample(uv_texture, video_sampler, input.tex_coord);

    // P010 format: U in R channel, V in G channel (same as NV12)
    let rgb = yuv_to_rgb(y_raw, uv.r, uv.g, color_params.color_range, color_params.color_space);

    // Linearize: gamma-encoded RGB -> linear RGB
    // The sRGB surface will then apply sRGB encoding for correct display
    let linear_rgb = linearize_rgb(rgb);

    return vec4<f32>(linear_rgb, 1.0);
}
"#;
