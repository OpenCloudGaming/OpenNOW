//! GPU Shaders for video rendering
//!
//! WGSL shaders for YUV to RGB conversion on the GPU.

/// WGSL shader for full-screen video quad with YUV to RGB conversion
/// Uses 3 separate textures (Y, U, V) for GPU-accelerated color conversion
/// This eliminates the CPU bottleneck of converting ~600M pixels/sec at 1440p165
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
    // Y is full resolution, U/V are half resolution (4:2:0 subsampling)
    // The sampler handles the upscaling of U/V automatically
    let y_raw = textureSample(y_texture, video_sampler, input.tex_coord).r;
    let u_raw = textureSample(u_texture, video_sampler, input.tex_coord).r;
    let v_raw = textureSample(v_texture, video_sampler, input.tex_coord).r;

    // BT.709 YUV to RGB conversion (limited/TV range)
    // Video uses limited range: Y [16-235], UV [16-240]
    // First convert from limited range to full range
    let y = (y_raw - 0.0625) * 1.1644;  // (Y - 16/255) * (255/219)
    let u = (u_raw - 0.5) * 1.1384;      // (U - 128/255) * (255/224)
    let v = (v_raw - 0.5) * 1.1384;      // (V - 128/255) * (255/224)

    // BT.709 color matrix (HD content: 720p and above)
    // R = Y + 1.5748 * V
    // G = Y - 0.1873 * U - 0.4681 * V
    // B = Y + 1.8556 * U
    let r = y + 1.5748 * v;
    let g = y - 0.1873 * u - 0.4681 * v;
    let b = y + 1.8556 * u;

    return vec4<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0), 1.0);
}
"#;

/// WGSL shader for NV12 format (VideoToolbox on macOS, CUVID on Windows)
/// NV12 has Y plane (R8) and interleaved UV plane (Rg8)
/// This shader deinterleaves UV on the GPU - much faster than CPU scaler
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
    // NVIDIA CUVID outputs NV12 with standard UV order (U in R, V in G)
    // but some decoders output NV21 (V in R, U in G) - try swapping if colors are wrong
    let u_raw = uv.r;  // U is in red channel (first byte of pair)
    let v_raw = uv.g;  // V is in green channel (second byte of pair)

    // BT.709 YUV to RGB conversion (full range for NVIDIA CUVID)
    // NVIDIA hardware decoders typically output full range [0-255]
    let y = y_raw;
    let u = u_raw - 0.5;  // Center around 0 (128/255 = 0.5)
    let v = v_raw - 0.5;  // Center around 0

    // BT.709 color matrix (HD content: 720p and above)
    let r = y + 1.5748 * v;
    let g = y - 0.1873 * u - 0.4681 * v;
    let b = y + 1.8556 * u;

    return vec4<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0), 1.0);
}
"#;
