//! GPU Renderer
//!
//! wgpu-based rendering for video frames and UI overlays.

use anyhow::{Result, Context};
use log::{info, debug, warn};
use std::sync::Arc;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes, Fullscreen, CursorGrabMode};

#[cfg(target_os = "macos")]
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::app::{App, AppState, UiAction, GamesTab, SettingChange};
use crate::media::{VideoFrame, PixelFormat};
use super::StatsPanel;
use super::image_cache;
use std::collections::HashMap;

/// WGSL shader for full-screen video quad with YUV to RGB conversion
/// Uses 3 separate textures (Y, U, V) for GPU-accelerated color conversion
/// This eliminates the CPU bottleneck of converting ~600M pixels/sec at 1440p165
const VIDEO_SHADER: &str = r#"
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

/// WGSL shader for NV12 format (VideoToolbox on macOS)
/// NV12 has Y plane (R8) and interleaved UV plane (Rg8)
/// This shader deinterleaves UV on the GPU - much faster than CPU scaler
const NV12_SHADER: &str = r#"
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
    let u_raw = uv.r;  // U is in red channel
    let v_raw = uv.g;  // V is in green channel

    // BT.709 YUV to RGB conversion (limited/TV range - same as YUV420P path)
    // VideoToolbox outputs limited range: Y [16-235], UV [16-240]
    let y = (y_raw - 0.0625) * 1.1644;  // (Y - 16/255) * (255/219)
    let u = (u_raw - 0.5) * 1.1384;      // (U - 128/255) * (255/224)
    let v = (v_raw - 0.5) * 1.1384;      // (V - 128/255) * (255/224)

    // BT.709 color matrix (HD content: 720p and above)
    let r = y + 1.5748 * v;
    let g = y - 0.1873 * u - 0.4681 * v;
    let b = y + 1.8556 * u;

    return vec4<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0), 1.0);
}
"#;

/// Main renderer
pub struct Renderer {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,

    // egui integration
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,

    // Video rendering pipeline (GPU YUV->RGB conversion)
    video_pipeline: wgpu::RenderPipeline,
    video_bind_group_layout: wgpu::BindGroupLayout,
    video_sampler: wgpu::Sampler,
    // YUV420P planar textures (Y = full res, U/V = half res for 4:2:0)
    y_texture: Option<wgpu::Texture>,
    u_texture: Option<wgpu::Texture>,
    v_texture: Option<wgpu::Texture>,
    video_bind_group: Option<wgpu::BindGroup>,
    video_size: (u32, u32),

    // NV12 pipeline (for VideoToolbox on macOS - faster than CPU scaler)
    nv12_pipeline: wgpu::RenderPipeline,
    nv12_bind_group_layout: wgpu::BindGroupLayout,
    // NV12 textures: Y (R8) and UV interleaved (Rg8)
    uv_texture: Option<wgpu::Texture>,
    nv12_bind_group: Option<wgpu::BindGroup>,
    // Current pixel format
    current_format: PixelFormat,

    // Stats panel
    stats_panel: StatsPanel,

    // Fullscreen state
    fullscreen: bool,

    // Swapchain error recovery state
    // Tracks consecutive Outdated errors to avoid panic-fixing with wrong resolution
    consecutive_surface_errors: u32,

    // Game art texture cache (URL -> TextureHandle)
    game_textures: HashMap<String, egui::TextureHandle>,
}

impl Renderer {
    /// Create a new renderer
    pub async fn new(event_loop: &ActiveEventLoop) -> Result<Self> {
        // Create window attributes
        let window_attrs = WindowAttributes::default()
            .with_title("OpenNOW")
            .with_inner_size(PhysicalSize::new(1280, 720))
            .with_min_inner_size(PhysicalSize::new(640, 480))
            .with_resizable(true);

        // Create window and wrap in Arc for surface creation
        let window = Arc::new(
            event_loop.create_window(window_attrs)
                .context("Failed to create window")?
        );

        let size = window.inner_size();

        info!("Window created: {}x{}", size.width, size.height);

        // On macOS, enable high-performance mode and disable App Nap
        #[cfg(target_os = "macos")]
        Self::enable_macos_high_performance();

        // On macOS, set display to 120Hz immediately (before fullscreen)
        // This ensures Direct mode uses high refresh rate
        #[cfg(target_os = "macos")]
        Self::set_macos_display_mode_120hz();

        // Create wgpu instance
        // Force DX12 on Windows for better exclusive fullscreen support and lower latency
        // Vulkan on Windows has issues with exclusive fullscreen transitions causing DWM composition
        #[cfg(target_os = "windows")]
        let backends = wgpu::Backends::DX12;
        #[cfg(not(target_os = "windows"))]
        let backends = wgpu::Backends::all();

        info!("Using wgpu backend: {:?}", backends);

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });

        // Create surface from Arc<Window>
        let surface = instance.create_surface(window.clone())
            .context("Failed to create surface")?;

        // Get adapter
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("Failed to find GPU adapter")?;

        let adapter_info = adapter.get_info();
        info!("GPU: {} (Backend: {:?}, Driver: {})", 
            adapter_info.name, 
            adapter_info.backend,
            adapter_info.driver_info
        );
        
        // Print to console directly for visibility (bypasses log filter)
        crate::utils::console_print(&format!(
            "[GPU] {} using {:?} backend",
            adapter_info.name,
            adapter_info.backend
        ));

        // Create device and queue
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .context("Failed to create device")?;

        // Configure surface
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        // Use Immediate for lowest latency - frame pacing is handled by our render loop
        let present_mode = if surface_caps.present_modes.contains(&wgpu::PresentMode::Immediate) {
            wgpu::PresentMode::Immediate
        } else if surface_caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else {
            wgpu::PresentMode::Fifo
        };
        info!("Using present mode: {:?}", present_mode);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 1, // Minimum latency for streaming
        };

        surface.configure(&device, &config);

        // Create egui context
        let egui_ctx = egui::Context::default();

        // Create egui-winit state (egui 0.33 API)
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::default(),
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        // Create egui-wgpu renderer (egui 0.33 API)
        let egui_renderer = egui_wgpu::Renderer::new(
            &device,
            surface_format,
            egui_wgpu::RendererOptions::default(),
        );

        // Create video rendering pipeline
        let video_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Video Shader"),
            source: wgpu::ShaderSource::Wgsl(VIDEO_SHADER.into()),
        });

        // Bind group layout for YUV planar textures (GPU color conversion)
        let video_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Video YUV Bind Group Layout"),
            entries: &[
                // Y texture (full resolution)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // U texture (half resolution)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // V texture (half resolution)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let video_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Video Pipeline Layout"),
            bind_group_layouts: &[&video_bind_group_layout],
            push_constant_ranges: &[],
        });

        let video_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Video Pipeline"),
            layout: Some(&video_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &video_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &video_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let video_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Video Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create NV12 pipeline (for VideoToolbox on macOS - GPU deinterleaving)
        let nv12_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("NV12 Shader"),
            source: wgpu::ShaderSource::Wgsl(NV12_SHADER.into()),
        });

        let nv12_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("NV12 Bind Group Layout"),
            entries: &[
                // Y texture (full resolution, R8)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // UV texture (half resolution, Rg8 interleaved)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let nv12_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("NV12 Pipeline Layout"),
            bind_group_layouts: &[&nv12_bind_group_layout],
            push_constant_ranges: &[],
        });

        let nv12_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("NV12 Pipeline"),
            layout: Some(&nv12_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &nv12_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &nv12_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Create stats panel
        let stats_panel = StatsPanel::new();

        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            egui_ctx,
            egui_state,
            egui_renderer,
            video_pipeline,
            video_bind_group_layout,
            video_sampler,
            y_texture: None,
            u_texture: None,
            v_texture: None,
            video_bind_group: None,
            video_size: (0, 0),
            nv12_pipeline,
            nv12_bind_group_layout,
            uv_texture: None,
            nv12_bind_group: None,
            current_format: PixelFormat::YUV420P,
            stats_panel,
            fullscreen: false,
            consecutive_surface_errors: 0,
            game_textures: HashMap::new(),
        })
    }

    /// Get window reference
    pub fn window(&self) -> &Window {
        &self.window
    }

    /// Handle window event
    pub fn handle_event(&mut self, event: &WindowEvent) -> bool {
        let response = self.egui_state.on_window_event(&self.window, event);
        response.consumed
    }

    /// Resize the renderer
    /// Filters out spurious resize events that occur during fullscreen transitions
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }

        // If we're in fullscreen mode, STRICTLY enforce that the resize matches the monitor
        // This prevents the race condition where the old windowed size (e.g., 1296x759)
        // is briefly reported during the fullscreen transition, causing DWM composition.
        if self.fullscreen {
            if let Some(monitor) = self.window.current_monitor() {
                let monitor_size = monitor.size();

                // Calculate deviation from monitor size (must be within 5%)
                let width_ratio = new_size.width as f32 / monitor_size.width as f32;
                let height_ratio = new_size.height as f32 / monitor_size.height as f32;

                // Reject if not within 95-105% of monitor resolution
                if width_ratio < 0.95 || width_ratio > 1.05 || height_ratio < 0.95 || height_ratio > 1.05 {
                    debug!(
                        "Ignoring resize to {}x{} while in fullscreen (monitor: {}x{}, ratio: {:.2}x{:.2})",
                        new_size.width, new_size.height,
                        monitor_size.width, monitor_size.height,
                        width_ratio, height_ratio
                    );
                    return;
                }
            }
        }

        self.size = new_size;
        self.configure_surface();
    }

    /// Configure the surface with current size and optimal present mode
    /// Called on resize and to recover from swapchain errors
    fn configure_surface(&mut self) {
        self.config.width = self.size.width;
        self.config.height = self.size.height;
        self.surface.configure(&self.device, &self.config);
        info!(
            "Surface configured: {}x{} @ {:?} (frame latency: {})",
            self.config.width,
            self.config.height,
            self.config.present_mode,
            self.config.desired_maximum_frame_latency
        );

        // On macOS, set ProMotion frame rate and disable VSync on every configure
        // This ensures the Metal layer always requests 120fps from ProMotion
        #[cfg(target_os = "macos")]
        Self::disable_macos_vsync(&self.window);
    }

    /// Recover from swapchain errors (Outdated/Lost)
    /// Returns true if recovery was successful
    fn recover_swapchain(&mut self) -> bool {
        // Get current window size - it may have changed (e.g., fullscreen toggle)
        let current_size = self.window.inner_size();
        if current_size.width == 0 || current_size.height == 0 {
            warn!("Cannot recover swapchain: window size is zero");
            return false;
        }

        // Update size and reconfigure
        self.size = current_size;
        self.configure_surface();
        info!(
            "Swapchain recovered: {}x{} @ {:?}",
            self.size.width,
            self.size.height,
            self.config.present_mode
        );
        true
    }

    /// Toggle fullscreen with high refresh rate support
    /// Uses exclusive fullscreen to bypass the desktop compositor (DWM) for lowest latency
    /// and selects the highest available refresh rate for the current resolution
    pub fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;

        if self.fullscreen {
            // On macOS, use Core Graphics to force 120Hz display mode
            #[cfg(target_os = "macos")]
            Self::set_macos_display_mode_120hz();

            // Use borderless fullscreen on macOS (exclusive doesn't work well)
            // The display mode is set separately via Core Graphics
            #[cfg(target_os = "macos")]
            {
                info!("Entering borderless fullscreen with 120Hz display mode");
                self.window.set_fullscreen(Some(Fullscreen::Borderless(None)));
                Self::disable_macos_vsync(&self.window);
                return;
            }

            // On other platforms, try exclusive fullscreen
            #[cfg(not(target_os = "macos"))]
            {
                let current_monitor = self.window.current_monitor();

                if let Some(monitor) = current_monitor {
                    let current_size = self.window.inner_size();
                    let mut best_mode: Option<winit::monitor::VideoModeHandle> = None;
                    let mut best_refresh_rate: u32 = 0;

                    info!("Searching for video modes on monitor: {:?}", monitor.name());
                    info!("Current window size: {}x{}", current_size.width, current_size.height);

                    let mut mode_count = 0;
                    let mut high_refresh_modes = Vec::new();
                    for mode in monitor.video_modes() {
                        let mode_size = mode.size();
                        let refresh_rate = mode.refresh_rate_millihertz() / 1000;

                        if refresh_rate >= 100 {
                            high_refresh_modes.push(format!("{}x{}@{}Hz", mode_size.width, mode_size.height, refresh_rate));
                        }
                        mode_count += 1;

                        if mode_size.width >= current_size.width && mode_size.height >= current_size.height {
                            if refresh_rate > best_refresh_rate {
                                best_refresh_rate = refresh_rate;
                                best_mode = Some(mode);
                            }
                        }
                    }
                    info!("Total video modes: {} (high refresh >=100Hz: {:?})", mode_count, high_refresh_modes);

                    if let Some(mode) = best_mode {
                        let refresh_hz = mode.refresh_rate_millihertz() / 1000;
                        info!(
                            "SELECTED exclusive fullscreen: {}x{} @ {}Hz",
                            mode.size().width,
                            mode.size().height,
                            refresh_hz
                        );
                        self.window.set_fullscreen(Some(Fullscreen::Exclusive(mode)));
                        return;
                    } else {
                        info!("No suitable exclusive fullscreen mode found");
                    }
                } else {
                    info!("No current monitor detected");
                }

                info!("Entering borderless fullscreen");
                self.window.set_fullscreen(Some(Fullscreen::Borderless(None)));
            }
        } else {
            info!("Exiting fullscreen");
            self.window.set_fullscreen(None);
        }
    }

    /// Enter fullscreen with a specific target refresh rate
    /// Useful when the stream FPS is known (e.g., 120fps stream -> 120Hz mode)
    pub fn set_fullscreen_with_refresh(&mut self, target_fps: u32) {
        let current_monitor = self.window.current_monitor();

        if let Some(monitor) = current_monitor {
            let current_size = self.window.inner_size();
            let mut best_mode: Option<winit::monitor::VideoModeHandle> = None;
            let mut best_refresh_diff: i32 = i32::MAX;

            // Find mode closest to target FPS
            for mode in monitor.video_modes() {
                let mode_size = mode.size();
                let refresh_rate = mode.refresh_rate_millihertz() / 1000;

                if mode_size.width >= current_size.width && mode_size.height >= current_size.height {
                    let diff = (refresh_rate as i32 - target_fps as i32).abs();
                    // Prefer modes >= target FPS
                    let adjusted_diff = if refresh_rate >= target_fps { diff } else { diff + 1000 };

                    if adjusted_diff < best_refresh_diff {
                        best_refresh_diff = adjusted_diff;
                        best_mode = Some(mode);
                    }
                }
            }

            if let Some(mode) = best_mode {
                let refresh_hz = mode.refresh_rate_millihertz() / 1000;
                info!(
                    "Entering exclusive fullscreen for {}fps stream: {}x{} @ {}Hz",
                    target_fps,
                    mode.size().width,
                    mode.size().height,
                    refresh_hz
                );
                self.fullscreen = true;
                self.window.set_fullscreen(Some(Fullscreen::Exclusive(mode)));

                #[cfg(target_os = "macos")]
                Self::disable_macos_vsync(&self.window);

                return;
            }
        }

        // Fallback
        self.fullscreen = true;
        self.window.set_fullscreen(Some(Fullscreen::Borderless(None)));

        #[cfg(target_os = "macos")]
        Self::disable_macos_vsync(&self.window);
    }

    /// Disable VSync on macOS Metal layer for unlimited FPS
    /// This prevents the compositor from limiting frame rate
    #[cfg(target_os = "macos")]
    fn disable_macos_vsync(window: &Window) {
        use cocoa::base::id;
        use objc::{msg_send, sel, sel_impl};

        // Get NSView from raw window handle
        let ns_view = match window.window_handle() {
            Ok(handle) => {
                match handle.as_raw() {
                    RawWindowHandle::AppKit(appkit) => appkit.ns_view.as_ptr() as id,
                    _ => {
                        warn!("macOS: Unexpected window handle type");
                        return;
                    }
                }
            }
            Err(e) => {
                warn!("macOS: Could not get window handle: {:?}", e);
                return;
            }
        };

        unsafe {
            // Get the layer from NSView
            let layer: id = msg_send![ns_view, layer];
            if layer.is_null() {
                warn!("macOS: Could not get layer for VSync disable");
                return;
            }

            // Check if it's a CAMetalLayer by checking class name
            let class: id = msg_send![layer, class];
            let class_name: id = msg_send![class, description];
            let name_cstr: *const i8 = msg_send![class_name, UTF8String];

            if !name_cstr.is_null() {
                let name = std::ffi::CStr::from_ptr(name_cstr).to_string_lossy();
                if name.contains("CAMetalLayer") {
                    // Set preferredFrameRateRange for ProMotion displays FIRST
                    // This tells macOS we want 120fps, preventing dynamic drop to 60Hz
                    #[repr(C)]
                    struct CAFrameRateRange {
                        minimum: f32,
                        maximum: f32,
                        preferred: f32,
                    }

                    let frame_rate_range = CAFrameRateRange {
                        minimum: 120.0,  // Minimum 120fps - don't allow lower
                        maximum: 120.0,
                        preferred: 120.0,
                    };

                    // Check if the layer responds to setPreferredFrameRateRange: (macOS 12+)
                    let responds: bool = msg_send![layer, respondsToSelector: sel!(setPreferredFrameRateRange:)];
                    if responds {
                        let _: () = msg_send![layer, setPreferredFrameRateRange: frame_rate_range];
                        info!("macOS: Set preferredFrameRateRange to 120fps fixed (ProMotion)");
                    }

                    // Keep displaySync ENABLED for ProMotion - it needs VSync to pace at 120Hz
                    // Disabling it causes ProMotion to fall back to 60Hz
                    let _: () = msg_send![layer, setDisplaySyncEnabled: true];
                    info!("macOS: Configured CAMetalLayer for 120Hz ProMotion");
                }
            }
        }
    }

    /// Set macOS display to 120Hz using Core Graphics
    /// This bypasses winit's video mode selection which doesn't work well on macOS
    #[cfg(target_os = "macos")]
    fn set_macos_display_mode_120hz() {
        use std::ffi::c_void;

        // Core Graphics FFI
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGMainDisplayID() -> u32;
            fn CGDisplayCopyAllDisplayModes(display: u32, options: *const c_void) -> *const c_void;
            fn CFArrayGetCount(array: *const c_void) -> isize;
            fn CFArrayGetValueAtIndex(array: *const c_void, idx: isize) -> *const c_void;
            fn CGDisplayModeGetWidth(mode: *const c_void) -> usize;
            fn CGDisplayModeGetHeight(mode: *const c_void) -> usize;
            fn CGDisplayModeGetRefreshRate(mode: *const c_void) -> f64;
            fn CGDisplaySetDisplayMode(display: u32, mode: *const c_void, options: *const c_void) -> i32;
            fn CGDisplayPixelsWide(display: u32) -> usize;
            fn CGDisplayPixelsHigh(display: u32) -> usize;
            fn CFRelease(cf: *const c_void);
        }

        unsafe {
            let display_id = CGMainDisplayID();
            let current_width = CGDisplayPixelsWide(display_id);
            let current_height = CGDisplayPixelsHigh(display_id);

            info!("macOS: Searching for 120Hz mode on display {} (current: {}x{})",
                display_id, current_width, current_height);

            let modes = CGDisplayCopyAllDisplayModes(display_id, std::ptr::null());
            if modes.is_null() {
                warn!("macOS: Could not enumerate display modes");
                return;
            }

            let count = CFArrayGetCount(modes);
            let mut best_mode: *const c_void = std::ptr::null();
            let mut best_refresh: f64 = 0.0;

            for i in 0..count {
                let mode = CFArrayGetValueAtIndex(modes, i);
                let width = CGDisplayModeGetWidth(mode);
                let height = CGDisplayModeGetHeight(mode);
                let refresh = CGDisplayModeGetRefreshRate(mode);

                // Look for modes matching current resolution with high refresh rate
                if width == current_width && height == current_height {
                    if refresh > best_refresh {
                        best_refresh = refresh;
                        best_mode = mode;
                    }
                    if refresh >= 100.0 {
                        info!("  Found mode: {}x{} @ {:.1}Hz", width, height, refresh);
                    }
                }
            }

            if !best_mode.is_null() && best_refresh >= 119.0 {
                let width = CGDisplayModeGetWidth(best_mode);
                let height = CGDisplayModeGetHeight(best_mode);
                info!("macOS: Setting display mode to {}x{} @ {:.1}Hz", width, height, best_refresh);

                let result = CGDisplaySetDisplayMode(display_id, best_mode, std::ptr::null());
                if result == 0 {
                    info!("macOS: Successfully set 120Hz display mode!");
                } else {
                    warn!("macOS: Failed to set display mode, error: {}", result);
                }
            } else if best_refresh > 0.0 {
                info!("macOS: No 120Hz mode found, best is {:.1}Hz - display may not support it", best_refresh);
            } else {
                warn!("macOS: No matching display modes found");
            }

            CFRelease(modes);
        }
    }

    /// Enable high-performance mode on macOS
    /// This disables App Nap and other power throttling that can limit FPS
    #[cfg(target_os = "macos")]
    fn enable_macos_high_performance() {
        use cocoa::base::{id, nil};
        use objc::{msg_send, sel, sel_impl, class};

        unsafe {
            // Get NSProcessInfo
            let process_info: id = msg_send![class!(NSProcessInfo), processInfo];
            if process_info == nil {
                warn!("macOS: Could not get NSProcessInfo");
                return;
            }

            // Activity options for high performance:
            // NSActivityUserInitiated = 0x00FFFFFF (prevents App Nap, system sleep)
            // NSActivityLatencyCritical = 0xFF00000000 (requests low latency scheduling)
            let options: u64 = 0x00FFFFFF | 0xFF00000000;

            // Create reason string
            let reason: id = msg_send![class!(NSString), stringWithUTF8String: b"Streaming requires consistent frame timing\0".as_ptr()];

            // Begin activity - this returns an object we should retain
            let activity: id = msg_send![process_info, beginActivityWithOptions:options reason:reason];
            if activity != nil {
                // Retain the activity object to keep it alive for the app lifetime
                let _: id = msg_send![activity, retain];
                info!("macOS: High-performance mode enabled (App Nap disabled, latency-critical scheduling)");
            } else {
                warn!("macOS: Failed to enable high-performance mode");
            }

            // Also try to disable automatic termination
            let _: () = msg_send![process_info, disableAutomaticTermination: reason];

            // Disable sudden termination
            let _: () = msg_send![process_info, disableSuddenTermination];
        }
    }

    /// Lock cursor for streaming (captures mouse)
    pub fn lock_cursor(&self) {
        // Try confined first, then locked mode
        if let Err(e) = self.window.set_cursor_grab(CursorGrabMode::Confined) {
            info!("Confined cursor grab failed ({}), trying locked mode", e);
            if let Err(e) = self.window.set_cursor_grab(CursorGrabMode::Locked) {
                log::warn!("Failed to lock cursor: {}", e);
            }
        }
        self.window.set_cursor_visible(false);
        info!("Cursor locked for streaming");
    }

    /// Unlock cursor
    pub fn unlock_cursor(&self) {
        let _ = self.window.set_cursor_grab(CursorGrabMode::None);
        self.window.set_cursor_visible(true);
        info!("Cursor unlocked");
    }

    /// Check if fullscreen
    pub fn is_fullscreen(&self) -> bool {
        self.fullscreen
    }

    /// Update video textures from frame (GPU YUV->RGB conversion)
    /// Supports both YUV420P (3 planes) and NV12 (2 planes) formats
    /// NV12 is faster on macOS as it skips CPU-based scaler
    pub fn update_video(&mut self, frame: &VideoFrame) {
        let uv_width = frame.width / 2;
        let uv_height = frame.height / 2;

        // Check if we need to recreate textures (size or format change)
        let format_changed = self.current_format != frame.format;
        let size_changed = self.video_size != (frame.width, frame.height);

        if size_changed || format_changed {
            self.current_format = frame.format;
            self.video_size = (frame.width, frame.height);

            // Y texture is same for both formats (full resolution, R8)
            let y_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Y Texture"),
                size: wgpu::Extent3d {
                    width: frame.width,
                    height: frame.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            match frame.format {
                PixelFormat::NV12 => {
                    // NV12: UV plane is interleaved (Rg8, 2 bytes per pixel)
                    let uv_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("UV Texture (NV12)"),
                        size: wgpu::Extent3d {
                            width: uv_width,
                            height: uv_height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rg8Unorm, // 2-channel for interleaved UV
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });

                    let y_view = y_texture.create_view(&wgpu::TextureViewDescriptor::default());
                    let uv_view = uv_texture.create_view(&wgpu::TextureViewDescriptor::default());

                    let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("NV12 Bind Group"),
                        layout: &self.nv12_bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&y_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&uv_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::Sampler(&self.video_sampler),
                            },
                        ],
                    });

                    self.y_texture = Some(y_texture);
                    self.uv_texture = Some(uv_texture);
                    self.nv12_bind_group = Some(bind_group);
                    // Clear YUV420P textures
                    self.u_texture = None;
                    self.v_texture = None;
                    self.video_bind_group = None;

                    info!("NV12 textures created: {}x{} (UV: {}x{}) - GPU deinterleaving enabled (CPU scaler bypassed!)",
                        frame.width, frame.height, uv_width, uv_height);
                }
                PixelFormat::YUV420P => {
                    // YUV420P: Separate U and V planes (R8 each)
                    let u_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("U Texture"),
                        size: wgpu::Extent3d {
                            width: uv_width,
                            height: uv_height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::R8Unorm,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });

                    let v_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("V Texture"),
                        size: wgpu::Extent3d {
                            width: uv_width,
                            height: uv_height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::R8Unorm,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });

                    let y_view = y_texture.create_view(&wgpu::TextureViewDescriptor::default());
                    let u_view = u_texture.create_view(&wgpu::TextureViewDescriptor::default());
                    let v_view = v_texture.create_view(&wgpu::TextureViewDescriptor::default());

                    let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Video YUV Bind Group"),
                        layout: &self.video_bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&y_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&u_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(&v_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: wgpu::BindingResource::Sampler(&self.video_sampler),
                            },
                        ],
                    });

                    self.y_texture = Some(y_texture);
                    self.u_texture = Some(u_texture);
                    self.v_texture = Some(v_texture);
                    self.video_bind_group = Some(bind_group);
                    // Clear NV12 textures
                    self.uv_texture = None;
                    self.nv12_bind_group = None;

                    info!("YUV420P textures created: {}x{} (UV: {}x{}) - GPU color conversion enabled",
                        frame.width, frame.height, uv_width, uv_height);
                }
            }
        }

        // Upload Y plane (same for both formats)
        if let Some(ref texture) = self.y_texture {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &frame.y_plane,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(frame.y_stride),
                    rows_per_image: Some(frame.height),
                },
                wgpu::Extent3d {
                    width: frame.width,
                    height: frame.height,
                    depth_or_array_layers: 1,
                },
            );
        }

        match frame.format {
            PixelFormat::NV12 => {
                // Upload interleaved UV plane (Rg8)
                if let Some(ref texture) = self.uv_texture {
                    self.queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        &frame.u_plane, // NV12: u_plane contains interleaved UV data
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(frame.u_stride), // stride for interleaved UV
                            rows_per_image: Some(uv_height),
                        },
                        wgpu::Extent3d {
                            width: uv_width,
                            height: uv_height,
                            depth_or_array_layers: 1,
                        },
                    );
                }
            }
            PixelFormat::YUV420P => {
                // Upload separate U and V planes
                if let Some(ref texture) = self.u_texture {
                    self.queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        &frame.u_plane,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(frame.u_stride),
                            rows_per_image: Some(uv_height),
                        },
                        wgpu::Extent3d {
                            width: uv_width,
                            height: uv_height,
                            depth_or_array_layers: 1,
                        },
                    );
                }

                if let Some(ref texture) = self.v_texture {
                    self.queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        &frame.v_plane,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(frame.v_stride),
                            rows_per_image: Some(uv_height),
                        },
                        wgpu::Extent3d {
                            width: uv_width,
                            height: uv_height,
                            depth_or_array_layers: 1,
                        },
                    );
                }
            }
        }
    }

    /// Render video frame to screen
    /// Automatically selects the correct pipeline based on current pixel format
    fn render_video(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        // Determine which pipeline and bind group to use based on format
        let (pipeline, bind_group) = match self.current_format {
            PixelFormat::NV12 => {
                if let Some(ref bg) = self.nv12_bind_group {
                    (&self.nv12_pipeline, bg)
                } else {
                    return; // No bind group ready
                }
            }
            PixelFormat::YUV420P => {
                if let Some(ref bg) = self.video_bind_group {
                    (&self.video_pipeline, bg)
                } else {
                    return; // No bind group ready
                }
            }
        };

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Video Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });

        render_pass.set_pipeline(pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.draw(0..6, 0..1); // Draw 6 vertices (2 triangles = 1 quad)
    }

    /// Render frame and return UI actions
    pub fn render(&mut self, app: &App) -> Result<Vec<UiAction>> {
        // Get surface texture with SMART error recovery for swapchain issues
        // Key insight: During fullscreen transitions, the window size updates AFTER
        // the surface error occurs. If we immediately "recover" with the old size,
        // we force DWM composition (scaling), causing 60Hz lock and input lag.
        // Instead, we YIELD to the event loop to let the Resized event propagate.
        let output = match self.surface.get_current_texture() {
            Ok(texture) => {
                // Success - reset error counter
                self.consecutive_surface_errors = 0;
                texture
            }
            Err(wgpu::SurfaceError::Outdated) | Err(wgpu::SurfaceError::Lost) => {
                self.consecutive_surface_errors += 1;
                
                // Check if window size differs from our config (resize pending)
                let current_window_size = self.window.inner_size();
                let config_matches_window = 
                    current_window_size.width == self.config.width &&
                    current_window_size.height == self.config.height;
                
                if !config_matches_window {
                    // Window size changed - resize event should handle this
                    // Call resize directly to sync up
                    debug!(
                        "Swapchain outdated: window {}x{} != config {}x{} - resizing",
                        current_window_size.width, current_window_size.height,
                        self.config.width, self.config.height
                    );
                    self.resize(current_window_size);
                    
                    // Retry after resize
                    match self.surface.get_current_texture() {
                        Ok(texture) => {
                            self.consecutive_surface_errors = 0;
                            info!("Swapchain recovered after resize to {}x{}", 
                                current_window_size.width, current_window_size.height);
                            texture
                        }
                        Err(e) => {
                            debug!("Still failing after resize: {} - yielding", e);
                            return Ok(vec![]);
                        }
                    }
                } else if self.consecutive_surface_errors < 10 {
                    // Sizes match but surface is outdated - likely a race condition
                    // YIELD to event loop to let Resized event arrive with correct size
                    debug!(
                        "Swapchain outdated (attempt {}/10): sizes match {}x{} - yielding to event loop",
                        self.consecutive_surface_errors,
                        self.config.width, self.config.height
                    );
                    return Ok(vec![]);
                } else {
                    // Persistent error (10+ frames) - force recovery as fallback
                    warn!(
                        "Swapchain persistently outdated ({} attempts) - forcing recovery",
                        self.consecutive_surface_errors
                    );
                    if !self.recover_swapchain() {
                        return Ok(vec![]);
                    }
                    match self.surface.get_current_texture() {
                        Ok(texture) => {
                            self.consecutive_surface_errors = 0;
                            texture
                        }
                        Err(e) => {
                            warn!("Failed to get texture after forced recovery: {}", e);
                            return Ok(vec![]);
                        }
                    }
                }
            }
            Err(wgpu::SurfaceError::Timeout) => {
                // GPU is busy, skip this frame
                debug!("Surface timeout - skipping frame");
                return Ok(vec![]);
            }
            Err(e) => {
                // Fatal error (e.g., OutOfMemory)
                return Err(anyhow::anyhow!("Surface error: {}", e));
            }
        };

        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        // Update video texture if we have a frame
        if let Some(ref frame) = app.current_frame {
            self.update_video(frame);
        }

        // Render video or clear based on state
        // Check for either YUV420P (video_bind_group) or NV12 (nv12_bind_group)
        let has_video = self.video_bind_group.is_some() || self.nv12_bind_group.is_some();
        if app.state == AppState::Streaming && has_video {
            // Render video full-screen
            self.render_video(&mut encoder, &view);
        } else {
            // Clear pass for non-streaming states
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.08,
                            b: 0.12,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
        }

        // Draw egui UI and collect actions
        let raw_input = self.egui_state.take_egui_input(&self.window);
        let mut actions: Vec<UiAction> = Vec::new();

        // Extract state needed for UI rendering
        let app_state = app.state;
        let stats = app.stats.clone();
        let show_stats = app.show_stats;
        let status_message = app.status_message.clone();
        let error_message = app.error_message.clone();
        let selected_game = app.selected_game.clone();
        let stats_position = self.stats_panel.position;
        let stats_visible = self.stats_panel.visible;
        let show_settings = app.show_settings;
        let settings = app.settings.clone();
        let login_providers = app.login_providers.clone();
        let selected_provider_index = app.selected_provider_index;
        let is_loading = app.is_loading;
        let mut search_query = app.search_query.clone();
        let runtime = app.runtime.clone();

        // New state for tabs, subscription, library, popup
        let current_tab = app.current_tab;
        let subscription = app.subscription.clone();
        let selected_game_popup = app.selected_game_popup.clone();

        // Server/region state
        let servers = app.servers.clone();
        let selected_server_index = app.selected_server_index;
        let auto_server_selection = app.auto_server_selection;
        let ping_testing = app.ping_testing;
        let show_settings_modal = app.show_settings_modal;

        // Get games based on current tab
        let games_list: Vec<_> = match current_tab {
            GamesTab::AllGames => {
                app.filtered_games().into_iter()
                    .map(|(i, g)| (i, g.clone()))
                    .collect()
            }
            GamesTab::MyLibrary => {
                let query = app.search_query.to_lowercase();
                app.library_games.iter()
                    .enumerate()
                    .filter(|(_, g)| query.is_empty() || g.title.to_lowercase().contains(&query))
                    .map(|(i, g)| (i, g.clone()))
                    .collect()
            }
        };

        // Clone texture map for rendering (avoid borrow issues)
        let game_textures = self.game_textures.clone();
        let mut new_textures: Vec<(String, egui::TextureHandle)> = Vec::new();

        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            // Custom styling
            let mut style = (*ctx.style()).clone();
            style.visuals.window_fill = egui::Color32::from_rgb(20, 20, 30);
            style.visuals.panel_fill = egui::Color32::from_rgb(25, 25, 35);
            style.visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(35, 35, 50);
            style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(45, 45, 65);
            style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(60, 60, 90);
            style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(80, 180, 80);
            style.visuals.selection.bg_fill = egui::Color32::from_rgb(60, 120, 60);
            ctx.set_style(style);

            match app_state {
                AppState::Login => {
                    self.render_login_screen(ctx, &login_providers, selected_provider_index, &status_message, is_loading, &mut actions);
                }
                AppState::Games => {
                    // Update image cache for async loading
                    image_cache::update_cache();
                    self.render_games_screen(
                        ctx,
                        &games_list,
                        &mut search_query,
                        &status_message,
                        show_settings,
                        &settings,
                        &runtime,
                        &game_textures,
                        &mut new_textures,
                        current_tab,
                        subscription.as_ref(),
                        selected_game_popup.as_ref(),
                        &servers,
                        selected_server_index,
                        auto_server_selection,
                        ping_testing,
                        show_settings_modal,
                        &mut actions
                    );
                }
                AppState::Session => {
                    self.render_session_screen(ctx, &selected_game, &status_message, &error_message, &mut actions);
                }
                AppState::Streaming => {
                    // Render stats overlay
                    if show_stats && stats_visible {
                        render_stats_panel(ctx, &stats, stats_position);
                    }

                    // Small overlay hint
                    egui::Area::new(egui::Id::new("stream_hint"))
                        .anchor(egui::Align2::CENTER_TOP, [0.0, 10.0])
                        .interactable(false)
                        .show(ctx, |ui| {
                            ui.label(
                                egui::RichText::new("Ctrl+Shift+Q to stop • F3 stats • F11 fullscreen")
                                    .color(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 100))
                                    .size(12.0)
                            );
                        });
                }
            }
        });

        // Check if search query changed
        if search_query != app.search_query {
            actions.push(UiAction::UpdateSearch(search_query));
        }

        // Apply newly loaded textures to the cache
        for (url, texture) in new_textures {
            self.game_textures.insert(url, texture);
        }

        self.egui_state.handle_platform_output(&self.window, full_output.platform_output);

        let clipped_primitives = self.egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);

        // Update egui textures
        for (id, image_delta) in &full_output.textures_delta.set {
            self.egui_renderer.update_texture(&self.device, &self.queue, *id, image_delta);
        }

        // Render egui
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.size.width, self.size.height],
            pixels_per_point: self.window.scale_factor() as f32,
        };

        self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &clipped_primitives,
            &screen_descriptor,
        );

        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Egui Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });

            // forget_lifetime is safe here as render_pass is dropped before encoder.finish()
            let mut render_pass = render_pass.forget_lifetime();
            self.egui_renderer.render(&mut render_pass, &clipped_primitives, &screen_descriptor);
        }

        // Free egui textures
        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(actions)
    }

    fn render_login_screen(
        &self,
        ctx: &egui::Context,
        login_providers: &[crate::auth::LoginProvider],
        selected_provider_index: usize,
        status_message: &str,
        is_loading: bool,
        actions: &mut Vec<UiAction>
    ) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let available_height = ui.available_height();
            let content_height = 400.0;
            let top_padding = ((available_height - content_height) / 2.0).max(40.0);

            ui.vertical_centered(|ui| {
                ui.add_space(top_padding);

                // Logo/Title with gradient-like effect
                ui.label(
                    egui::RichText::new("OpenNOW")
                        .size(48.0)
                        .color(egui::Color32::from_rgb(118, 185, 0)) // NVIDIA green
                        .strong()
                );

                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("GeForce NOW Client")
                        .size(14.0)
                        .color(egui::Color32::from_rgb(150, 150, 150))
                );

                ui.add_space(60.0);

                // Login card container
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(30, 30, 40))
                    .corner_radius(12.0)
                    .inner_margin(egui::Margin { left: 40, right: 40, top: 30, bottom: 30 })
                    .show(ui, |ui| {
                        ui.set_min_width(320.0);

                        ui.vertical(|ui| {
                            // Region selection label - centered
                            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                                ui.label(
                                    egui::RichText::new("Select Region")
                                        .size(13.0)
                                        .color(egui::Color32::from_rgb(180, 180, 180))
                                );
                            });

                            ui.add_space(10.0);

                            // Provider dropdown - centered using horizontal with spacing
                            ui.horizontal(|ui| {
                                let available_width = ui.available_width();
                                let combo_width = 240.0;
                                let padding = (available_width - combo_width) / 2.0;
                                ui.add_space(padding.max(0.0));

                                let selected_name = login_providers.get(selected_provider_index)
                                    .map(|p| p.login_provider_display_name.as_str())
                                    .unwrap_or("NVIDIA (Global)");

                                egui::ComboBox::from_id_salt("provider_select")
                                    .selected_text(selected_name)
                                    .width(combo_width)
                                    .show_ui(ui, |ui| {
                                        for (i, provider) in login_providers.iter().enumerate() {
                                            let is_selected = i == selected_provider_index;
                                            if ui.selectable_label(is_selected, &provider.login_provider_display_name).clicked() {
                                                actions.push(UiAction::SelectProvider(i));
                                            }
                                        }
                                    });
                            });

                            ui.add_space(25.0);

                            // Login button or loading state - centered
                            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                                if is_loading {
                                    ui.add_space(10.0);
                                    ui.spinner();
                                    ui.add_space(12.0);
                                    ui.label(
                                        egui::RichText::new("Opening browser...")
                                            .size(13.0)
                                            .color(egui::Color32::from_rgb(118, 185, 0))
                                    );
                                    ui.add_space(5.0);
                                    ui.label(
                                        egui::RichText::new("Complete login in your browser")
                                            .size(11.0)
                                            .color(egui::Color32::GRAY)
                                    );
                                } else {
                                    let login_btn = egui::Button::new(
                                        egui::RichText::new("Sign In")
                                            .size(15.0)
                                            .color(egui::Color32::WHITE)
                                            .strong()
                                    )
                                    .fill(egui::Color32::from_rgb(118, 185, 0))
                                    .corner_radius(6.0);

                                    if ui.add_sized([240.0, 42.0], login_btn).clicked() {
                                        actions.push(UiAction::StartLogin);
                                    }

                                    ui.add_space(15.0);

                                    ui.label(
                                        egui::RichText::new("Sign in with your NVIDIA account")
                                            .size(11.0)
                                            .color(egui::Color32::from_rgb(120, 120, 120))
                                    );
                                }
                            });
                        });
                    });

                ui.add_space(20.0);

                // Status message (if any)
                if !status_message.is_empty() && status_message != "Welcome to OpenNOW" {
                    ui.label(
                        egui::RichText::new(status_message)
                            .size(11.0)
                            .color(egui::Color32::from_rgb(150, 150, 150))
                    );
                }

                ui.add_space(40.0);

                // Footer info
                ui.label(
                    egui::RichText::new("Alliance Partners can select their region above")
                        .size(10.0)
                        .color(egui::Color32::from_rgb(80, 80, 80))
                );
            });
        });
    }

    fn render_games_screen(
        &self,
        ctx: &egui::Context,
        games: &[(usize, crate::app::GameInfo)],
        search_query: &mut String,
        _status_message: &str,
        _show_settings: bool,
        settings: &crate::app::Settings,
        _runtime: &tokio::runtime::Handle,
        game_textures: &HashMap<String, egui::TextureHandle>,
        new_textures: &mut Vec<(String, egui::TextureHandle)>,
        current_tab: GamesTab,
        subscription: Option<&crate::app::SubscriptionInfo>,
        selected_game_popup: Option<&crate::app::GameInfo>,
        servers: &[crate::app::ServerInfo],
        selected_server_index: usize,
        auto_server_selection: bool,
        ping_testing: bool,
        show_settings_modal: bool,
        actions: &mut Vec<UiAction>
    ) {
        // Top bar with tabs, search, and logout - subscription info moved to bottom
        egui::TopBottomPanel::top("top_bar")
            .frame(egui::Frame::new()
                .fill(egui::Color32::from_rgb(22, 22, 30))
                .inner_margin(egui::Margin { left: 0, right: 0, top: 10, bottom: 10 }))
            .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(15.0);

                // Logo
                ui.label(
                    egui::RichText::new("OpenNOW")
                        .size(24.0)
                        .color(egui::Color32::from_rgb(118, 185, 0))
                        .strong()
                );

                ui.add_space(20.0);

                // Tab buttons - solid style like login button
                let all_games_selected = current_tab == GamesTab::AllGames;
                let library_selected = current_tab == GamesTab::MyLibrary;

                let all_games_btn = egui::Button::new(
                    egui::RichText::new("All Games")
                        .size(13.0)
                        .color(egui::Color32::WHITE)
                        .strong()
                )
                .fill(if all_games_selected {
                    egui::Color32::from_rgb(118, 185, 0)
                } else {
                    egui::Color32::from_rgb(50, 50, 65)
                })
                .corner_radius(6.0);

                if ui.add_sized([90.0, 32.0], all_games_btn).clicked() && !all_games_selected {
                    actions.push(UiAction::SwitchTab(GamesTab::AllGames));
                }

                ui.add_space(8.0);

                let library_btn = egui::Button::new(
                    egui::RichText::new("My Library")
                        .size(13.0)
                        .color(egui::Color32::WHITE)
                        .strong()
                )
                .fill(if library_selected {
                    egui::Color32::from_rgb(118, 185, 0)
                } else {
                    egui::Color32::from_rgb(50, 50, 65)
                })
                .corner_radius(6.0);

                if ui.add_sized([90.0, 32.0], library_btn).clicked() && !library_selected {
                    actions.push(UiAction::SwitchTab(GamesTab::MyLibrary));
                }

                ui.add_space(20.0);

                // Search box in the middle
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(35, 35, 45))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin { left: 10, right: 10, top: 6, bottom: 6 })
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 75)))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("🔍")
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(120, 120, 140))
                            );
                            ui.add_space(6.0);
                            let search = egui::TextEdit::singleline(search_query)
                                .hint_text("Search games...")
                                .desired_width(200.0)
                                .frame(false)
                                .text_color(egui::Color32::WHITE);
                            ui.add(search);
                        });
                    });

                // Right side content
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(15.0);

                    // Logout button - solid style
                    let logout_btn = egui::Button::new(
                        egui::RichText::new("Logout")
                            .size(13.0)
                            .color(egui::Color32::WHITE)
                    )
                    .fill(egui::Color32::from_rgb(50, 50, 65))
                    .corner_radius(6.0);

                    if ui.add_sized([80.0, 32.0], logout_btn).clicked() {
                        actions.push(UiAction::Logout);
                    }

                    ui.add_space(10.0);

                    // Settings button - between hours and logout
                    let settings_btn = egui::Button::new(
                        egui::RichText::new("⚙")
                            .size(16.0)
                            .color(if show_settings_modal {
                                egui::Color32::from_rgb(118, 185, 0)
                            } else {
                                egui::Color32::WHITE
                            })
                    )
                    .fill(if show_settings_modal {
                        egui::Color32::from_rgb(50, 70, 50)
                    } else {
                        egui::Color32::from_rgb(50, 50, 65)
                    })
                    .corner_radius(6.0);

                    if ui.add_sized([36.0, 32.0], settings_btn).clicked() {
                        actions.push(UiAction::ToggleSettingsModal);
                    }
                });
            });
        });

        // Bottom bar with subscription stats
        egui::TopBottomPanel::bottom("bottom_bar")
            .frame(egui::Frame::new()
                .fill(egui::Color32::from_rgb(22, 22, 30))
                .inner_margin(egui::Margin { left: 15, right: 15, top: 8, bottom: 8 }))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(sub) = subscription {
                        // Membership tier badge
                        let (tier_bg, tier_fg) = match sub.membership_tier.as_str() {
                            "ULTIMATE" => (egui::Color32::from_rgb(80, 50, 120), egui::Color32::from_rgb(200, 150, 255)),
                            "PERFORMANCE" | "PRIORITY" => (egui::Color32::from_rgb(30, 70, 100), egui::Color32::from_rgb(100, 200, 255)),
                            _ => (egui::Color32::from_rgb(50, 50, 60), egui::Color32::GRAY),
                        };

                        egui::Frame::new()
                            .fill(tier_bg)
                            .corner_radius(4.0)
                            .inner_margin(egui::Margin { left: 8, right: 8, top: 4, bottom: 4 })
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(&sub.membership_tier)
                                        .size(11.0)
                                        .color(tier_fg)
                                        .strong()
                                );
                            });

                        ui.add_space(20.0);

                        // Hours icon and remaining
                        ui.label(
                            egui::RichText::new("⏱")
                                .size(14.0)
                                .color(egui::Color32::GRAY)
                        );
                        ui.add_space(5.0);

                        let hours_color = if sub.remaining_hours > 5.0 {
                            egui::Color32::from_rgb(118, 185, 0)
                        } else if sub.remaining_hours > 1.0 {
                            egui::Color32::from_rgb(255, 200, 50)
                        } else {
                            egui::Color32::from_rgb(255, 80, 80)
                        };

                        ui.label(
                            egui::RichText::new(format!("{:.1}h", sub.remaining_hours))
                                .size(13.0)
                                .color(hours_color)
                                .strong()
                        );
                        ui.label(
                            egui::RichText::new(format!(" / {:.0}h", sub.total_hours))
                                .size(12.0)
                                .color(egui::Color32::GRAY)
                        );

                        ui.add_space(20.0);

                        // Storage icon and space (if available)
                        if sub.has_persistent_storage {
                            if let Some(storage_gb) = sub.storage_size_gb {
                                ui.label(
                                    egui::RichText::new("💾")
                                        .size(14.0)
                                        .color(egui::Color32::GRAY)
                                );
                                ui.add_space(5.0);
                                ui.label(
                                    egui::RichText::new(format!("{} GB", storage_gb))
                                        .size(13.0)
                                        .color(egui::Color32::from_rgb(100, 180, 255))
                                );
                            }
                        }
                    } else {
                        ui.label(
                            egui::RichText::new("Loading subscription info...")
                                .size(12.0)
                                .color(egui::Color32::GRAY)
                        );
                    }

                    // Right side: server info
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Show selected server
                        if auto_server_selection {
                            let best_server = servers.iter()
                                .filter(|s| s.status == crate::app::ServerStatus::Online && s.ping_ms.is_some())
                                .min_by_key(|s| s.ping_ms.unwrap_or(9999));

                            if let Some(server) = best_server {
                                ui.label(
                                    egui::RichText::new(format!("🌐 Auto: {} ({}ms)", server.name, server.ping_ms.unwrap_or(0)))
                                        .size(12.0)
                                        .color(egui::Color32::from_rgb(118, 185, 0))
                                );
                            } else if ping_testing {
                                ui.label(
                                    egui::RichText::new("🌐 Testing servers...")
                                        .size(12.0)
                                        .color(egui::Color32::GRAY)
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new("🌐 Auto (waiting for ping)")
                                        .size(12.0)
                                        .color(egui::Color32::GRAY)
                                );
                            }
                        } else if let Some(server) = servers.get(selected_server_index) {
                            let ping_text = server.ping_ms.map(|p| format!(" ({}ms)", p)).unwrap_or_default();
                            ui.label(
                                egui::RichText::new(format!("🌐 {}{}", server.name, ping_text))
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(100, 180, 255))
                            );
                        }
                    });
                });
            });

        // Main content area
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(15.0);

            // Games content
            let header_text = match current_tab {
                GamesTab::AllGames => format!("All Games ({} available)", games.len()),
                GamesTab::MyLibrary => format!("My Library ({} games)", games.len()),
            };

            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(header_text)
                        .size(20.0)
                        .strong()
                        .color(egui::Color32::WHITE)
                );
            });

            ui.add_space(20.0);

            if games.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    let empty_text = match current_tab {
                        GamesTab::AllGames => "No games found",
                        GamesTab::MyLibrary => "Your library is empty.\nPurchase games from Steam, Epic, or other stores to see them here.",
                    };
                    ui.label(
                        egui::RichText::new(empty_text)
                            .size(14.0)
                            .color(egui::Color32::from_rgb(120, 120, 120))
                    );
                });
            } else {
                // Games grid - calculate columns based on available width
                let available_width = ui.available_width();
                let card_width = 220.0;
                let spacing = 16.0;
                let num_columns = ((available_width + spacing) / (card_width + spacing)).floor() as usize;
                let num_columns = num_columns.max(2).min(6); // Between 2 and 6 columns

                // Collect games to render (avoid borrow issues)
                let games_to_render: Vec<_> = games.iter().map(|(idx, game)| (*idx, game.clone())).collect();

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add_space(10.0);
                            ui.vertical(|ui| {
                                egui::Grid::new("games_grid")
                                    .num_columns(num_columns)
                                    .spacing([spacing, spacing])
                                    .show(ui, |ui| {
                                        for (col, (idx, game)) in games_to_render.iter().enumerate() {
                                            Self::render_game_card(ui, ctx, *idx, game, _runtime, game_textures, new_textures, actions);

                                            if (col + 1) % num_columns == 0 {
                                                ui.end_row();
                                            }
                                        }
                                    });
                            });
                        });
                    });
            }
        });

        // Game detail popup
        if let Some(game) = selected_game_popup {
            Self::render_game_popup(ctx, game, game_textures, actions);
        }

        // Settings modal
        if show_settings_modal {
            Self::render_settings_modal(ctx, settings, servers, selected_server_index, auto_server_selection, ping_testing, actions);
        }

        // Session conflict dialog
        if app.show_session_conflict {
            Self::render_session_conflict_dialog(ctx, &app.active_sessions, app.pending_game_launch.as_ref(), actions);
        }
    }

    /// Render the Settings modal with region selector and stream settings
    fn render_settings_modal(
        ctx: &egui::Context,
        settings: &crate::app::Settings,
        servers: &[crate::app::ServerInfo],
        selected_server_index: usize,
        auto_server_selection: bool,
        ping_testing: bool,
        actions: &mut Vec<UiAction>,
    ) {
        let modal_width = 500.0;
        let modal_height = 600.0;

        // Dark overlay
        egui::Area::new(egui::Id::new("settings_overlay"))
            .fixed_pos(egui::pos2(0.0, 0.0))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                let screen_rect = ctx.screen_rect();
                ui.allocate_response(screen_rect.size(), egui::Sense::click());
                ui.painter().rect_filled(
                    screen_rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180),
                );
            });

        // Modal window
        let screen_rect = ctx.screen_rect();
        let modal_pos = egui::pos2(
            (screen_rect.width() - modal_width) / 2.0,
            (screen_rect.height() - modal_height) / 2.0,
        );

        egui::Area::new(egui::Id::new("settings_modal"))
            .fixed_pos(modal_pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(28, 28, 35))
                    .corner_radius(12.0)
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 75)))
                    .inner_margin(egui::Margin::same(20))
                    .show(ui, |ui| {
                        ui.set_min_size(egui::vec2(modal_width, modal_height));

                        // Header with close button
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Settings")
                                    .size(20.0)
                                    .strong()
                                    .color(egui::Color32::WHITE)
                            );

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let close_btn = egui::Button::new(
                                    egui::RichText::new("✕")
                                        .size(16.0)
                                        .color(egui::Color32::WHITE)
                                )
                                .fill(egui::Color32::TRANSPARENT)
                                .corner_radius(4.0);

                                if ui.add(close_btn).clicked() {
                                    actions.push(UiAction::ToggleSettingsModal);
                                }
                            });
                        });

                        ui.add_space(15.0);
                        ui.separator();
                        ui.add_space(15.0);

                        egui::ScrollArea::vertical()
                            .max_height(modal_height - 100.0)
                            .show(ui, |ui| {
                                // === Stream Settings Section ===
                                ui.label(
                                    egui::RichText::new("Stream Settings")
                                        .size(16.0)
                                        .strong()
                                        .color(egui::Color32::WHITE)
                                );
                                ui.add_space(15.0);

                                egui::Grid::new("settings_grid")
                                    .num_columns(2)
                                    .spacing([20.0, 12.0])
                                    .min_col_width(100.0)
                                    .show(ui, |ui| {
                                        // Resolution dropdown
                                        ui.label(
                                            egui::RichText::new("Resolution")
                                                .size(13.0)
                                                .color(egui::Color32::GRAY)
                                        );
                                        egui::ComboBox::from_id_salt("resolution_combo")
                                            .selected_text(&settings.resolution)
                                            .width(180.0)
                                            .show_ui(ui, |ui| {
                                                for res in crate::app::config::RESOLUTIONS {
                                                    if ui.selectable_label(settings.resolution == res.0, format!("{} ({})", res.0, res.1)).clicked() {
                                                        actions.push(UiAction::UpdateSetting(SettingChange::Resolution(res.0.to_string())));
                                                    }
                                                }
                                            });
                                        ui.end_row();

                                        // FPS dropdown
                                        ui.label(
                                            egui::RichText::new("FPS")
                                                .size(13.0)
                                                .color(egui::Color32::GRAY)
                                        );
                                        egui::ComboBox::from_id_salt("fps_combo")
                                            .selected_text(format!("{} FPS", settings.fps))
                                            .width(180.0)
                                            .show_ui(ui, |ui| {
                                                for fps in crate::app::config::FPS_OPTIONS {
                                                    if ui.selectable_label(settings.fps == *fps, format!("{} FPS", fps)).clicked() {
                                                        actions.push(UiAction::UpdateSetting(SettingChange::Fps(*fps)));
                                                    }
                                                }
                                            });
                                        ui.end_row();

                                        // Codec dropdown
                                        ui.label(
                                            egui::RichText::new("Video Codec")
                                                .size(13.0)
                                                .color(egui::Color32::GRAY)
                                        );
                                        egui::ComboBox::from_id_salt("codec_combo")
                                            .selected_text(settings.codec.display_name())
                                            .width(180.0)
                                            .show_ui(ui, |ui| {
                                                for codec in crate::app::config::VideoCodec::all() {
                                                    if ui.selectable_label(settings.codec == *codec, codec.display_name()).clicked() {
                                                        actions.push(UiAction::UpdateSetting(SettingChange::Codec(*codec)));
                                                    }
                                                }
                                            });
                                        ui.end_row();

                                        // Max Bitrate slider
                                        ui.label(
                                            egui::RichText::new("Max Bitrate")
                                                .size(13.0)
                                                .color(egui::Color32::GRAY)
                                        );
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new(format!("{} Mbps", settings.max_bitrate_mbps))
                                                    .size(13.0)
                                                    .color(egui::Color32::WHITE)
                                            );
                                            ui.label(
                                                egui::RichText::new("(200 = unlimited)")
                                                    .size(10.0)
                                                    .color(egui::Color32::GRAY)
                                            );
                                        });
                                        ui.end_row();
                                    });

                                ui.add_space(25.0);
                                ui.separator();
                                ui.add_space(15.0);

                                // === Server Region Section ===
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("Server Region")
                                            .size(16.0)
                                            .strong()
                                            .color(egui::Color32::WHITE)
                                    );

                                    ui.add_space(20.0);

                                    // Ping test button
                                    let ping_btn_text = if ping_testing { "Testing..." } else { "Test Ping" };
                                    let ping_btn = egui::Button::new(
                                        egui::RichText::new(ping_btn_text)
                                            .size(11.0)
                                            .color(egui::Color32::WHITE)
                                    )
                                    .fill(if ping_testing {
                                        egui::Color32::from_rgb(80, 80, 100)
                                    } else {
                                        egui::Color32::from_rgb(60, 120, 60)
                                    })
                                    .corner_radius(4.0);

                                    if ui.add_sized([80.0, 24.0], ping_btn).clicked() && !ping_testing {
                                        actions.push(UiAction::StartPingTest);
                                    }

                                    if ping_testing {
                                        ui.spinner();
                                    }
                                });
                                ui.add_space(10.0);

                                // Server dropdown with Auto option and best server highlighted
                                let selected_text = if auto_server_selection {
                                    // Find best server for display
                                    let best = servers.iter()
                                        .filter(|s| s.status == crate::app::ServerStatus::Online && s.ping_ms.is_some())
                                        .min_by_key(|s| s.ping_ms.unwrap_or(9999));
                                    if let Some(best_server) = best {
                                        format!("Auto: {} ({}ms)", best_server.name, best_server.ping_ms.unwrap_or(0))
                                    } else {
                                        "Auto (Best Ping)".to_string()
                                    }
                                } else {
                                    servers.get(selected_server_index)
                                        .map(|s| {
                                            if let Some(ping) = s.ping_ms {
                                                format!("{} ({}ms)", s.name, ping)
                                            } else {
                                                s.name.clone()
                                            }
                                        })
                                        .unwrap_or_else(|| "Select a server...".to_string())
                                };

                                egui::ComboBox::from_id_salt("server_combo")
                                    .selected_text(selected_text)
                                    .width(300.0)
                                    .show_ui(ui, |ui| {
                                        // Auto option at the top
                                        let auto_label = {
                                            let best = servers.iter()
                                                .filter(|s| s.status == crate::app::ServerStatus::Online && s.ping_ms.is_some())
                                                .min_by_key(|s| s.ping_ms.unwrap_or(9999));
                                            if let Some(best_server) = best {
                                                format!("✨ Auto: {} ({}ms)", best_server.name, best_server.ping_ms.unwrap_or(0))
                                            } else {
                                                "✨ Auto (Best Ping)".to_string()
                                            }
                                        };

                                        if ui.selectable_label(auto_server_selection, auto_label).clicked() {
                                            actions.push(UiAction::SetAutoServerSelection(true));
                                        }

                                        ui.separator();
                                        ui.add_space(5.0);

                                        // Group by region
                                        let regions = ["Europe", "North America", "Canada", "Asia-Pacific", "Other"];
                                        for region in regions {
                                            let region_servers: Vec<_> = servers
                                                .iter()
                                                .enumerate()
                                                .filter(|(_, s)| s.region == region)
                                                .collect();

                                            if region_servers.is_empty() {
                                                continue;
                                            }

                                            ui.label(
                                                egui::RichText::new(region)
                                                    .size(11.0)
                                                    .strong()
                                                    .color(egui::Color32::from_rgb(118, 185, 0))
                                            );

                                            for (idx, server) in region_servers {
                                                let is_selected = !auto_server_selection && idx == selected_server_index;
                                                let ping_text = match server.status {
                                                    crate::app::ServerStatus::Online => {
                                                        server.ping_ms.map(|p| format!(" ({}ms)", p)).unwrap_or_default()
                                                    }
                                                    crate::app::ServerStatus::Testing => " (testing...)".to_string(),
                                                    crate::app::ServerStatus::Offline => " (offline)".to_string(),
                                                    crate::app::ServerStatus::Unknown => "".to_string(),
                                                };

                                                let label = format!("  {}{}", server.name, ping_text);
                                                if ui.selectable_label(is_selected, label).clicked() {
                                                    actions.push(UiAction::SelectServer(idx));
                                                }
                                            }

                                            ui.add_space(5.0);
                                        }
                                    });

                                ui.add_space(20.0);
                            });
                    });
            });
    }

    /// Render the session conflict dialog
    fn render_session_conflict_dialog(
        ctx: &egui::Context,
        active_sessions: &[ActiveSessionInfo],
        pending_game: Option<&GameInfo>,
        actions: &mut Vec<UiAction>,
    ) {
        use crate::app::session::ActiveSessionInfo;
        use crate::app::GameInfo;

        let modal_width = 500.0;
        let modal_height = 300.0;

        egui::Area::new(egui::Id::new("session_conflict_overlay"))
            .fixed_pos(egui::pos2(0.0, 0.0))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                let screen_rect = ctx.screen_rect();
                ui.allocate_response(screen_rect.size(), egui::Sense::click());
                ui.painter().rect_filled(
                    screen_rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200),
                );
            });

        let screen_rect = ctx.screen_rect();
        let modal_pos = egui::pos2(
            (screen_rect.width() - modal_width) / 2.0,
            (screen_rect.height() - modal_height) / 2.0,
        );

        egui::Area::new(egui::Id::new("session_conflict_modal"))
            .fixed_pos(modal_pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(28, 28, 35))
                    .corner_radius(12.0)
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 75)))
                    .inner_margin(egui::Margin::same(20))
                    .show(ui, |ui| {
                        ui.set_min_size(egui::vec2(modal_width, modal_height));

                        ui.label(
                            egui::RichText::new("⚠ Active Session Detected")
                                .size(20.0)
                                .strong()
                                .color(egui::Color32::from_rgb(255, 200, 80))
                        );

                        ui.add_space(15.0);

                        if let Some(session) = active_sessions.first() {
                            ui.label(
                                egui::RichText::new("You have an active GFN session running:")
                                    .size(14.0)
                                    .color(egui::Color32::LIGHT_GRAY)
                            );

                            ui.add_space(10.0);

                            egui::Frame::new()
                                .fill(egui::Color32::from_rgb(40, 40, 50))
                                .corner_radius(8.0)
                                .inner_margin(egui::Margin::same(12))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("App ID:")
                                                .size(13.0)
                                                .color(egui::Color32::GRAY)
                                        );
                                        ui.label(
                                            egui::RichText::new(format!("{}", session.app_id))
                                                .size(13.0)
                                                .color(egui::Color32::WHITE)
                                        );
                                    });

                                    if let Some(ref gpu) = session.gpu_type {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new("GPU:")
                                                    .size(13.0)
                                                    .color(egui::Color32::GRAY)
                                            );
                                            ui.label(
                                                egui::RichText::new(gpu)
                                                    .size(13.0)
                                                    .color(egui::Color32::WHITE)
                                            );
                                        });
                                    }

                                    if let Some(ref res) = session.resolution {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new("Resolution:")
                                                    .size(13.0)
                                                    .color(egui::Color32::GRAY)
                                            );
                                            ui.label(
                                                egui::RichText::new(format!("{} @ {}fps", res, session.fps.unwrap_or(60)))
                                                    .size(13.0)
                                                    .color(egui::Color32::WHITE)
                                            );
                                        });
                                    }

                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("Status:")
                                                .size(13.0)
                                                .color(egui::Color32::GRAY)
                                        );
                                        let status_text = match session.status {
                                            2 => "Ready",
                                            3 => "Running",
                                            _ => "Unknown",
                                        };
                                        ui.label(
                                            egui::RichText::new(status_text)
                                                .size(13.0)
                                                .color(egui::Color32::from_rgb(118, 185, 0))
                                        );
                                    });
                                });

                            ui.add_space(15.0);

                            if pending_game.is_some() {
                                ui.label(
                                    egui::RichText::new("GFN only allows one session at a time. You can either:")
                                        .size(13.0)
                                        .color(egui::Color32::LIGHT_GRAY)
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new("What would you like to do?")
                                        .size(13.0)
                                        .color(egui::Color32::LIGHT_GRAY)
                                );
                            }

                            ui.add_space(15.0);

                            ui.vertical_centered(|ui| {
                                let resume_btn = egui::Button::new(
                                    egui::RichText::new("Resume Existing Session")
                                        .size(14.0)
                                        .color(egui::Color32::WHITE)
                                )
                                .fill(egui::Color32::from_rgb(118, 185, 0))
                                .min_size(egui::vec2(200.0, 35.0));

                                if ui.add(resume_btn).clicked() {
                                    actions.push(UiAction::ResumeSession(session.clone()));
                                }

                                ui.add_space(8.0);

                                if let Some(game) = pending_game {
                                    let terminate_btn = egui::Button::new(
                                        egui::RichText::new(format!("End Session & Launch \"{}\"", game.title))
                                            .size(14.0)
                                            .color(egui::Color32::WHITE)
                                    )
                                    .fill(egui::Color32::from_rgb(220, 60, 60))
                                    .min_size(egui::vec2(200.0, 35.0));

                                    if ui.add(terminate_btn).clicked() {
                                        actions.push(UiAction::TerminateAndLaunch(session.session_id.clone(), game.clone()));
                                    }

                                    ui.add_space(8.0);
                                }

                                let cancel_btn = egui::Button::new(
                                    egui::RichText::new("Cancel")
                                        .size(14.0)
                                        .color(egui::Color32::LIGHT_GRAY)
                                )
                                .fill(egui::Color32::from_rgb(60, 60, 75))
                                .min_size(egui::vec2(200.0, 35.0));

                                if ui.add(cancel_btn).clicked() {
                                    actions.push(UiAction::CloseSessionConflict);
                                }
                            });
                        }
                    });
            });
    }

    /// Render the game detail popup
    fn render_game_popup(
        ctx: &egui::Context,
        game: &crate::app::GameInfo,
        game_textures: &HashMap<String, egui::TextureHandle>,
        actions: &mut Vec<UiAction>,
    ) {
        let popup_width = 400.0;
        let popup_height = 350.0;

        egui::Window::new("Game Details")
            .collapsible(false)
            .resizable(false)
            .fixed_size([popup_width, popup_height])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    // Game image
                    if let Some(ref image_url) = game.image_url {
                        if let Some(texture) = game_textures.get(image_url) {
                            let image_size = egui::vec2(popup_width - 40.0, 150.0);
                            ui.add(egui::Image::new(texture).fit_to_exact_size(image_size).corner_radius(8.0));
                        } else {
                            // Placeholder
                            let placeholder_size = egui::vec2(popup_width - 40.0, 150.0);
                            let (_, rect) = ui.allocate_space(placeholder_size);
                            ui.painter().rect_filled(rect, 8.0, egui::Color32::from_rgb(50, 50, 70));
                            let initial = game.title.chars().next().unwrap_or('?').to_uppercase().to_string();
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                initial,
                                egui::FontId::proportional(48.0),
                                egui::Color32::from_rgb(100, 100, 130),
                            );
                        }
                    }

                    ui.add_space(15.0);

                    // Game title
                    ui.label(
                        egui::RichText::new(&game.title)
                            .size(20.0)
                            .strong()
                            .color(egui::Color32::WHITE)
                    );

                    ui.add_space(8.0);

                    // Store badge
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Store:")
                                .size(12.0)
                                .color(egui::Color32::GRAY)
                        );
                        ui.label(
                            egui::RichText::new(&game.store.to_uppercase())
                                .size(12.0)
                                .color(egui::Color32::from_rgb(100, 180, 255))
                                .strong()
                        );
                    });

                    // Publisher if available
                    if let Some(ref publisher) = game.publisher {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Publisher:")
                                    .size(12.0)
                                    .color(egui::Color32::GRAY)
                            );
                            ui.label(
                                egui::RichText::new(publisher)
                                    .size(12.0)
                                    .color(egui::Color32::LIGHT_GRAY)
                            );
                        });
                    }

                    ui.add_space(20.0);

                    // Buttons
                    ui.horizontal(|ui| {
                        // Play button
                        let play_btn = egui::Button::new(
                            egui::RichText::new("  Play Now  ")
                                .size(16.0)
                                .strong()
                        )
                        .fill(egui::Color32::from_rgb(70, 180, 70))
                        .min_size(egui::vec2(120.0, 40.0));

                        if ui.add(play_btn).clicked() {
                            actions.push(UiAction::LaunchGameDirect(game.clone()));
                            actions.push(UiAction::CloseGamePopup);
                        }

                        ui.add_space(20.0);

                        // Close button
                        let close_btn = egui::Button::new(
                            egui::RichText::new("  Close  ")
                                .size(14.0)
                        )
                        .fill(egui::Color32::from_rgb(60, 60, 80))
                        .min_size(egui::vec2(80.0, 40.0));

                        if ui.add(close_btn).clicked() {
                            actions.push(UiAction::CloseGamePopup);
                        }
                    });
                });
            });
    }

    fn render_game_card(
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        _idx: usize,
        game: &crate::app::GameInfo,
        runtime: &tokio::runtime::Handle,
        game_textures: &HashMap<String, egui::TextureHandle>,
        new_textures: &mut Vec<(String, egui::TextureHandle)>,
        actions: &mut Vec<UiAction>,
    ) {
        // Card dimensions - larger for better visibility
        let card_width = 220.0;
        let image_height = 124.0; // 16:9 aspect ratio

        // Make the entire card clickable
        let game_for_click = game.clone();

        let response = egui::Frame::new()
            .fill(egui::Color32::from_rgb(28, 28, 36))
            .corner_radius(8.0)
            .inner_margin(0.0)
            .show(ui, |ui| {
                ui.set_min_width(card_width);

                ui.vertical(|ui| {
                    // Game box art image - full width, no padding
                    if let Some(ref image_url) = game.image_url {
                        // Check if texture is already loaded
                        if let Some(texture) = game_textures.get(image_url) {
                            // Display the image with rounded top corners
                            let size = egui::vec2(card_width, image_height);
                            ui.add(egui::Image::new(texture)
                                .fit_to_exact_size(size)
                                .corner_radius(egui::CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }));
                        } else {
                            // Check if image data is available in cache
                            if let Some((pixels, width, height)) = image_cache::get_image(image_url) {
                                // Create egui texture from pixels
                                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                                    [width as usize, height as usize],
                                    &pixels,
                                );
                                let texture = ctx.load_texture(
                                    image_url,
                                    color_image,
                                    egui::TextureOptions::LINEAR,
                                );
                                new_textures.push((image_url.clone(), texture.clone()));

                                // Display immediately
                                let size = egui::vec2(card_width, image_height);
                                ui.add(egui::Image::new(&texture)
                                    .fit_to_exact_size(size)
                                    .corner_radius(egui::CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }));
                            } else {
                                // Request loading
                                image_cache::request_image(image_url, runtime);

                                // Show placeholder
                                let placeholder_rect = ui.allocate_space(egui::vec2(card_width, image_height));
                                ui.painter().rect_filled(
                                    placeholder_rect.1,
                                    egui::CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 },
                                    egui::Color32::from_rgb(40, 40, 55),
                                );
                                // Loading spinner effect
                                ui.painter().text(
                                    placeholder_rect.1.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "...",
                                    egui::FontId::proportional(16.0),
                                    egui::Color32::from_rgb(80, 80, 100),
                                );
                            }
                        }
                    } else {
                        // No image URL - show placeholder with game initial
                        let placeholder_rect = ui.allocate_space(egui::vec2(card_width, image_height));
                        ui.painter().rect_filled(
                            placeholder_rect.1,
                            egui::CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 },
                            egui::Color32::from_rgb(45, 45, 65),
                        );
                        // Show first letter of game title
                        let initial = game.title.chars().next().unwrap_or('?').to_uppercase().to_string();
                        ui.painter().text(
                            placeholder_rect.1.center(),
                            egui::Align2::CENTER_CENTER,
                            initial,
                            egui::FontId::proportional(40.0),
                            egui::Color32::from_rgb(80, 80, 110),
                        );
                    }

                    // Text content area with padding
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.vertical(|ui| {
                            // Game title (truncated if too long)
                            let title = if game.title.chars().count() > 24 {
                                let truncated: String = game.title.chars().take(21).collect();
                                format!("{}...", truncated)
                            } else {
                                game.title.clone()
                            };
                            ui.label(
                                egui::RichText::new(title)
                                    .size(13.0)
                                    .strong()
                                    .color(egui::Color32::WHITE)
                            );

                            ui.add_space(2.0);

                            // Store badge with color coding
                            let store_color = match game.store.to_lowercase().as_str() {
                                "steam" => egui::Color32::from_rgb(102, 192, 244),
                                "epic" => egui::Color32::from_rgb(200, 200, 200),
                                "ubisoft" | "uplay" => egui::Color32::from_rgb(0, 150, 255),
                                "xbox" => egui::Color32::from_rgb(16, 124, 16),
                                "gog" => egui::Color32::from_rgb(190, 130, 255),
                                _ => egui::Color32::from_rgb(150, 150, 150),
                            };
                            ui.label(
                                egui::RichText::new(&game.store.to_uppercase())
                                    .size(10.0)
                                    .color(store_color)
                            );
                        });
                    });
                    ui.add_space(10.0);
                });
            });

        // Make the card clickable - check for click on the response rect
        let card_rect = response.response.rect;
        if ui.rect_contains_pointer(card_rect) {
            // Change cursor to pointer
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);

            // Highlight on hover - subtle glow effect
            ui.painter().rect_stroke(
                card_rect,
                8.0,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(118, 185, 0)),
                egui::StrokeKind::Outside
            );
        }

        if response.response.interact(egui::Sense::click()).clicked() {
            actions.push(UiAction::OpenGamePopup(game_for_click));
        }
    }

    fn render_session_screen(
        &self,
        ctx: &egui::Context,
        selected_game: &Option<crate::app::GameInfo>,
        status_message: &str,
        error_message: &Option<String>,
        actions: &mut Vec<UiAction>
    ) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(120.0);

                // Game title
                if let Some(ref game) = selected_game {
                    ui.label(
                        egui::RichText::new(&game.title)
                            .size(28.0)
                            .strong()
                            .color(egui::Color32::WHITE)
                    );
                }

                ui.add_space(40.0);

                // Spinner
                ui.spinner();

                ui.add_space(20.0);

                // Status
                ui.label(
                    egui::RichText::new(status_message)
                        .size(16.0)
                        .color(egui::Color32::LIGHT_GRAY)
                );

                // Error message
                if let Some(ref error) = error_message {
                    ui.add_space(20.0);
                    ui.label(
                        egui::RichText::new(error)
                            .size(14.0)
                            .color(egui::Color32::from_rgb(255, 100, 100))
                    );
                }

                ui.add_space(40.0);

                // Cancel button
                if ui.button("Cancel").clicked() {
                    actions.push(UiAction::StopStreaming);
                }
            });
        });
    }
}

/// Render stats panel (standalone function)
fn render_stats_panel(ctx: &egui::Context, stats: &crate::media::StreamStats, position: crate::app::StatsPosition) {
    use egui::{Align2, Color32, FontId, RichText};

    let (anchor, offset) = match position {
        crate::app::StatsPosition::BottomLeft => (Align2::LEFT_BOTTOM, [10.0, -10.0]),
        crate::app::StatsPosition::BottomRight => (Align2::RIGHT_BOTTOM, [-10.0, -10.0]),
        crate::app::StatsPosition::TopLeft => (Align2::LEFT_TOP, [10.0, 10.0]),
        crate::app::StatsPosition::TopRight => (Align2::RIGHT_TOP, [-10.0, 10.0]),
    };

    egui::Area::new(egui::Id::new("stats_panel"))
        .anchor(anchor, offset)
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(0, 0, 0, 200))
                .corner_radius(4.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.set_min_width(200.0);

                    // Resolution
                    let res_text = if stats.resolution.is_empty() {
                        "Connecting...".to_string()
                    } else {
                        stats.resolution.clone()
                    };

                    ui.label(
                        RichText::new(res_text)
                            .font(FontId::monospace(13.0))
                            .color(Color32::WHITE)
                    );

                    // Decoded FPS vs Render FPS (shows if renderer is bottlenecked)
                    let decode_fps = stats.fps;
                    let render_fps = stats.render_fps;
                    let target_fps = stats.target_fps as f32;

                    // Decode FPS color
                    let decode_color = if target_fps > 0.0 {
                        let ratio = decode_fps / target_fps;
                        if ratio >= 0.8 { Color32::GREEN }
                        else if ratio >= 0.5 { Color32::YELLOW }
                        else { Color32::from_rgb(255, 100, 100) }
                    } else { Color32::WHITE };

                    // Render FPS color (critical - this is what you actually see)
                    let render_color = if target_fps > 0.0 {
                        let ratio = render_fps / target_fps;
                        if ratio >= 0.8 { Color32::GREEN }
                        else if ratio >= 0.5 { Color32::YELLOW }
                        else { Color32::from_rgb(255, 100, 100) }
                    } else { Color32::WHITE };

                    // Show both FPS values
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("Decode: {:.0}", decode_fps))
                                .font(FontId::monospace(11.0))
                                .color(decode_color)
                        );
                        ui.label(
                            RichText::new(format!(" | Render: {:.0}", render_fps))
                                .font(FontId::monospace(11.0))
                                .color(render_color)
                        );
                        if stats.target_fps > 0 {
                            ui.label(
                                RichText::new(format!(" / {} fps", stats.target_fps))
                                    .font(FontId::monospace(11.0))
                                    .color(Color32::GRAY)
                            );
                        }
                    });

                    // Codec and bitrate
                    if !stats.codec.is_empty() {
                        ui.label(
                            RichText::new(format!(
                                "{} | {:.1} Mbps",
                                stats.codec,
                                stats.bitrate_mbps
                            ))
                            .font(FontId::monospace(11.0))
                            .color(Color32::LIGHT_GRAY)
                        );
                    }

                    // Latency (decode pipeline)
                    let latency_color = if stats.latency_ms < 30.0 {
                        Color32::GREEN
                    } else if stats.latency_ms < 60.0 {
                        Color32::YELLOW
                    } else {
                        Color32::RED
                    };

                    ui.label(
                        RichText::new(format!("Decode: {:.0} ms", stats.latency_ms))
                            .font(FontId::monospace(11.0))
                            .color(latency_color)
                    );

                    // Input latency (event creation to transmission)
                    if stats.input_latency_ms > 0.0 {
                        let input_color = if stats.input_latency_ms < 2.0 {
                            Color32::GREEN
                        } else if stats.input_latency_ms < 5.0 {
                            Color32::YELLOW
                        } else {
                            Color32::RED
                        };

                        ui.label(
                            RichText::new(format!("Input: {:.1} ms", stats.input_latency_ms))
                                .font(FontId::monospace(11.0))
                                .color(input_color)
                        );
                    }

                    if stats.packet_loss > 0.0 {
                        let loss_color = if stats.packet_loss < 1.0 {
                            Color32::YELLOW
                        } else {
                            Color32::RED
                        };

                        ui.label(
                            RichText::new(format!("Packet Loss: {:.1}%", stats.packet_loss))
                                .font(FontId::monospace(11.0))
                                .color(loss_color)
                        );
                    }

                    // Decode and render times
                    if stats.decode_time_ms > 0.0 || stats.render_time_ms > 0.0 {
                        ui.label(
                            RichText::new(format!(
                                "Decode: {:.1} ms | Render: {:.1} ms",
                                stats.decode_time_ms,
                                stats.render_time_ms
                            ))
                            .font(FontId::monospace(10.0))
                            .color(Color32::GRAY)
                        );
                    }

                    // Frame stats
                    if stats.frames_received > 0 {
                        ui.label(
                            RichText::new(format!(
                                "Frames: {} rx, {} dec, {} drop",
                                stats.frames_received,
                                stats.frames_decoded,
                                stats.frames_dropped
                            ))
                            .font(FontId::monospace(10.0))
                            .color(Color32::DARK_GRAY)
                        );
                    }

                    // GPU and server info
                    if !stats.gpu_type.is_empty() || !stats.server_region.is_empty() {
                        let info = format!(
                            "{}{}{}",
                            stats.gpu_type,
                            if !stats.gpu_type.is_empty() && !stats.server_region.is_empty() { " | " } else { "" },
                            stats.server_region
                        );

                        ui.label(
                            RichText::new(info)
                                .font(FontId::monospace(10.0))
                                .color(Color32::DARK_GRAY)
                        );
                    }
                });
        });
}

