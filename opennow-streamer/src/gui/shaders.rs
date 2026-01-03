//! GPU Shaders for video rendering
//!
//! WGSL shaders for YUV to RGB conversion on the GPU.
//! BT.709 Full range conversion for GFN streams.

/// WGSL shader for YUV420P format (3 separate planes)
/// Fallback path when NV12 is not available
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

    // BT.709 Full Range (PC range: Y 0-255, UV 0-255)
    // GFN streams report Full range - use direct values with no scaling
    let y = y_raw;
    let u = u_raw - 0.5;
    let v = v_raw - 0.5;

    // BT.709 color matrix (Full range coefficients)
    let r = y + 1.5748 * v;
    let g = y - 0.1873 * u - 0.4681 * v;
    let b = y + 1.8556 * u;

    return vec4<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0), 1.0);
}
"#;

/// WGSL shader for NV12 format (CUVID on Windows, VideoToolbox on macOS)
/// Primary GPU path - Y plane (R8) + interleaved UV plane (Rg8)
/// BT.709 Full range YUV to RGB conversion (GFN streams use Full range)
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

    // BT.709 Full Range (PC range: Y 0-255, UV 0-255)
    // GFN streams report Full range - use direct values with no scaling
    let y = y_raw;
    let u = u_raw - 0.5;
    let v = v_raw - 0.5;

    // BT.709 color matrix (Full range coefficients)
    let r = y + 1.5748 * v;
    let g = y - 0.1873 * u - 0.4681 * v;
    let b = y + 1.8556 * u;

    return vec4<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0), 1.0);
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
