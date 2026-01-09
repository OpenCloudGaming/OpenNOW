//! GPU Shaders for video rendering
//!
//! WGSL shaders for YUV to RGB conversion on the GPU.
//! Supports SDR (BT.709) and HDR (BT.2020 + PQ) with automatic tone mapping.
//!
//! HDR Detection:
//! The VideoFrame struct carries color_space and transfer_function metadata.
//! When HDR content is detected (BT.2020 + PQ), the shader will apply:
//! 1. PQ EOTF to convert to linear light
//! 2. BT.2020 to BT.709 color space conversion
//! 3. ACES tone mapping to compress HDR to SDR range
//! 4. SDR gamma for display

/// WGSL shader for YUV420P format (3 separate planes)
/// Uses BT.709 Limited range conversion (standard for video)
pub const VIDEO_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Full-screen quad (2 triangles = 6 vertices)
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),  // bottom-left
        vec2<f32>( 1.0, -1.0),  // bottom-right
        vec2<f32>(-1.0,  1.0),  // top-left
        vec2<f32>(-1.0,  1.0),  // top-left
        vec2<f32>( 1.0, -1.0),  // bottom-right
        vec2<f32>( 1.0,  1.0),  // top-right
    );

    var tex_coords = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),  // bottom-left
        vec2<f32>(1.0, 1.0),  // bottom-right
        vec2<f32>(0.0, 0.0),  // top-left
        vec2<f32>(0.0, 0.0),  // top-left
        vec2<f32>(1.0, 1.0),  // bottom-right
        vec2<f32>(1.0, 0.0),  // top-right
    );

    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    output.tex_coord = tex_coords[vertex_index];
    return output;
}

// YUV planar textures (Y = full res, U/V = half res)
@group(0) @binding(0)
var y_texture: texture_2d<f32>;
@group(0) @binding(1)
var u_texture: texture_2d<f32>;
@group(0) @binding(2)
var v_texture: texture_2d<f32>;
@group(0) @binding(3)
var video_sampler: sampler;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Sample Y, U, V planes
    let y_raw = textureSample(y_texture, video_sampler, input.tex_coord).r;
    let u_raw = textureSample(u_texture, video_sampler, input.tex_coord).r;
    let v_raw = textureSample(v_texture, video_sampler, input.tex_coord).r;

    // BT.709 Limited Range (TV range: Y 16-235, UV 16-240)
    // Scale from limited to full range
    let y = (y_raw - 16.0/255.0) * (255.0/219.0);
    let u = (u_raw - 16.0/255.0) * (255.0/224.0) - 0.5;
    let v = (v_raw - 16.0/255.0) * (255.0/224.0) - 0.5;

    // BT.709 color matrix
    let r = y + 1.5748 * v;
    let g = y - 0.1873 * u - 0.4681 * v;
    let b = y + 1.8556 * u;

    return vec4<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0), 1.0);
}
"#;

/// WGSL shader for NV12 format (D3D11 on Windows, VideoToolbox on macOS)
/// Primary GPU path - Y plane (R8) + interleaved UV plane (Rg8)
/// Uses BT.709 Limited range conversion (standard for video)
pub const NV12_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
};

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

// NV12 textures: Y (R8, full res) and UV (Rg8, half res, interleaved)
@group(0) @binding(0)
var y_texture: texture_2d<f32>;
@group(0) @binding(1)
var uv_texture: texture_2d<f32>;
@group(0) @binding(2)
var video_sampler: sampler;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Sample Y (full res) and UV (half res, interleaved)
    let y_raw = textureSample(y_texture, video_sampler, input.tex_coord).r;
    let uv = textureSample(uv_texture, video_sampler, input.tex_coord);

    // NV12 format: U in R channel, V in G channel
    let u_raw = uv.r;
    let v_raw = uv.g;

    // BT.709 Limited Range (TV range: Y 16-235, UV 16-240)
    // Scale from limited to full range
    let y = (y_raw - 16.0/255.0) * (255.0/219.0);
    let u = (u_raw - 16.0/255.0) * (255.0/224.0) - 0.5;
    let v = (v_raw - 16.0/255.0) * (255.0/224.0) - 0.5;

    // BT.709 color matrix
    let r = y + 1.5748 * v;
    let g = y - 0.1873 * u - 0.4681 * v;
    let b = y + 1.8556 * u;

    return vec4<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0), 1.0);
}
"#;

/// WGSL shader for NV12 HDR content (BT.2020 + PQ to SDR tone mapping)
/// Used when HDR content is detected and display is in SDR mode
pub const NV12_HDR_TONEMAP_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
};

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

// NV12 textures: Y (R8/R16, full res) and UV (Rg8/Rg16, half res, interleaved)
@group(0) @binding(0)
var y_texture: texture_2d<f32>;
@group(0) @binding(1)
var uv_texture: texture_2d<f32>;
@group(0) @binding(2)
var video_sampler: sampler;

// PQ EOTF (SMPTE ST 2084) - converts PQ signal to linear light (nits)
fn pq_eotf(pq: vec3<f32>) -> vec3<f32> {
    let m1 = 0.1593017578125;   // 2610/16384
    let m2 = 78.84375;          // 2523/32 * 128
    let c1 = 0.8359375;         // 3424/4096
    let c2 = 18.8515625;        // 2413/128
    let c3 = 18.6875;           // 2392/128

    let pq_pow = pow(max(pq, vec3<f32>(0.0)), vec3<f32>(1.0 / m2));
    let num = max(pq_pow - c1, vec3<f32>(0.0));
    let den = c2 - c3 * pq_pow;

    // Output is in units of 10000 nits (PQ reference white)
    return pow(num / max(den, vec3<f32>(0.0001)), vec3<f32>(1.0 / m1)) * 10000.0;
}

// BT.2020 to BT.709 color gamut conversion
fn bt2020_to_bt709(color: vec3<f32>) -> vec3<f32> {
    // 3x3 matrix for BT.2020 to BT.709 conversion
    let r = color.r *  1.6605 + color.g * -0.5876 + color.b * -0.0728;
    let g = color.r * -0.1246 + color.g *  1.1329 + color.b * -0.0083;
    let b = color.r * -0.0182 + color.g * -0.1006 + color.b *  1.1187;
    return vec3<f32>(r, g, b);
}

// ACES-inspired filmic tone mapping
fn tonemap_aces(hdr: vec3<f32>) -> vec3<f32> {
    // Normalize from nits to [0,1] range
    // Assume 203 nits as SDR reference white (BT.2408)
    let x = hdr / 203.0;

    // ACES filmic curve approximation (Narkowicz 2015)
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

// Apply SDR gamma curve (sRGB approximation)
fn linear_to_srgb(linear: vec3<f32>) -> vec3<f32> {
    let cutoff = vec3<f32>(0.0031308);
    let low = linear * 12.92;
    let high = 1.055 * pow(linear, vec3<f32>(1.0/2.4)) - 0.055;
    return select(high, low, linear <= cutoff);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Sample Y (full res) and UV (half res, interleaved)
    let y_raw = textureSample(y_texture, video_sampler, input.tex_coord).r;
    let uv = textureSample(uv_texture, video_sampler, input.tex_coord);

    // NV12 format: U in R channel, V in G channel
    let u_raw = uv.r;
    let v_raw = uv.g;

    // BT.2020 Limited Range (TV range: Y 64-940, UV 64-960 for 10-bit)
    // For 8-bit textures, this is normalized to 0-1 range
    // Scale from limited to full range
    let y = (y_raw - 16.0/255.0) * (255.0/219.0);
    let u = (u_raw - 16.0/255.0) * (255.0/224.0) - 0.5;
    let v = (v_raw - 16.0/255.0) * (255.0/224.0) - 0.5;

    // BT.2020 YCbCr to RGB matrix (non-constant luminance)
    let r = y + 1.4746 * v;
    let g = y - 0.1646 * u - 0.5714 * v;
    let b = y + 1.8814 * u;

    var rgb = vec3<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0));

    // Apply PQ EOTF to get linear light in nits
    let linear_hdr = pq_eotf(rgb);

    // Convert from BT.2020 to BT.709 color gamut
    let bt709_linear = bt2020_to_bt709(linear_hdr);

    // Tone map HDR to SDR range
    let tonemapped = tonemap_aces(max(bt709_linear, vec3<f32>(0.0)));

    // Apply sRGB gamma for SDR display
    let sdr = linear_to_srgb(tonemapped);

    return vec4<f32>(sdr, 1.0);
}
"#;

/// WGSL shader for ExternalTexture (wgpu 28+ zero-copy video)
/// Uses texture_external which provides hardware-accelerated YUV->RGB conversion
/// This is the fastest path - no manual color conversion needed
pub const EXTERNAL_TEXTURE_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
};

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

// External texture - hardware-accelerated YUV->RGB conversion
@group(0) @binding(0)
var video_texture: texture_external;

// Sampler for external texture
@group(0) @binding(1)
var video_sampler: sampler;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // textureSampleBaseClampToEdge automatically converts YUV to RGB
    // using the color space information from the ExternalTexture descriptor
    return textureSampleBaseClampToEdge(video_texture, video_sampler, input.tex_coord);
}
"#;
