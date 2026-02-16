//! D3D11 Zero-Copy Video Support for Windows
//!
//! This module provides zero-copy video rendering on Windows by keeping
//! decoded frames on GPU as D3D11 textures and sharing them with wgpu's DX12 backend.
//!
//! Flow:
//! 1. FFmpeg D3D11VA decodes to ID3D11Texture2D (GPU VRAM)
//! 2. We extract the texture from FFmpeg frame
//! 3. Create a DXGI shared handle (NT handle for cross-API sharing)
//! 4. Import into wgpu's DX12 backend via the hal layer
//!
//! This eliminates the expensive GPU->CPU->GPU round-trip that kills latency.

use anyhow::{anyhow, Result};
use log::{debug, info, warn};
use parking_lot::Mutex;

use windows::core::Interface;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Asynchronous, ID3D11Device, ID3D11Query, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_QUERY_DESC, D3D11_QUERY_EVENT,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_FORMAT_P010};
use windows::Win32::Graphics::Dxgi::{IDXGIResource1, DXGI_SHARED_RESOURCE_READ};

/// Wrapper for a D3D11 texture from FFmpeg hardware decoder
/// Holds the texture alive and provides access for wgpu import
pub struct D3D11TextureWrapper {
    /// The D3D11 texture (NV12 or P010 format)
    texture: ID3D11Texture2D,
    /// Texture array index (for texture arrays used by some decoders)
    array_index: u32,
    /// Shared NT handle for cross-API sharing (DX11 -> DX12)
    shared_handle: Mutex<Option<HANDLE>>,
    /// Texture dimensions
    pub width: u32,
    pub height: u32,
    /// Whether this is a 10-bit HDR texture (P010 format)
    pub is_10bit: bool,
}

// Safety: D3D11 COM objects are thread-safe (they use internal ref counting)
unsafe impl Send for D3D11TextureWrapper {}
unsafe impl Sync for D3D11TextureWrapper {}

impl std::fmt::Debug for D3D11TextureWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("D3D11TextureWrapper")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("array_index", &self.array_index)
            .field("is_10bit", &self.is_10bit)
            .field("has_shared_handle", &self.shared_handle.lock().is_some())
            .finish()
    }
}

impl D3D11TextureWrapper {
    /// Create a new wrapper from an existing ID3D11Texture2D
    ///
    /// This is used by the native DXVA decoder to wrap its output textures.
    /// The texture is cloned (AddRef'd) so the original can be safely dropped.
    pub fn from_texture(texture: ID3D11Texture2D, array_index: u32) -> Self {
        unsafe {
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut desc);

            let is_10bit = desc.Format == DXGI_FORMAT_P010;

            debug!(
                "D3D11TextureWrapper::from_texture: {}x{}, array_index={}, format={:?}, is_10bit={}",
                desc.Width, desc.Height, array_index, desc.Format, is_10bit
            );

            Self {
                texture,
                array_index,
                shared_handle: Mutex::new(None),
                width: desc.Width,
                height: desc.Height,
                is_10bit,
            }
        }
    }

    /// Create a new wrapper from FFmpeg's D3D11VA frame data
    ///
    /// # Safety
    /// The texture pointer must be valid and point to an ID3D11Texture2D
    pub unsafe fn from_ffmpeg_frame(
        texture_ptr: *mut std::ffi::c_void,
        array_index: i32,
    ) -> Option<Self> {
        if texture_ptr.is_null() {
            warn!("D3D11 texture pointer is null");
            return None;
        }

        // Cast to ID3D11Texture2D
        // FFmpeg stores the raw COM pointer in frame->data[0]
        let texture: ID3D11Texture2D = std::mem::transmute_copy(&texture_ptr);

        // Get texture description
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        texture.GetDesc(&mut desc);

        // Detect format - NV12 (8-bit SDR) or P010 (10-bit HDR)
        let is_10bit = desc.Format == DXGI_FORMAT_P010;
        let format_name = if is_10bit {
            "P010 (10-bit HDR)"
        } else if desc.Format == DXGI_FORMAT_NV12 {
            "NV12 (8-bit SDR)"
        } else {
            "Unknown"
        };

        debug!(
            "D3D11 texture: {}x{}, format={} ({:?}), array_size={}, bind_flags={:?}",
            desc.Width, desc.Height, format_name, desc.Format, desc.ArraySize, desc.BindFlags
        );

        // Verify it's NV12 or P010 format (expected from hardware decoders)
        if desc.Format != DXGI_FORMAT_NV12 && desc.Format != DXGI_FORMAT_P010 {
            warn!(
                "D3D11 texture format is {:?}, expected NV12 or P010",
                desc.Format
            );
            // Still proceed - might work with other formats
        }

        Some(Self {
            texture,
            array_index: array_index as u32,
            shared_handle: Mutex::new(None),
            width: desc.Width,
            height: desc.Height,
            is_10bit,
        })
    }

    /// Get or create a shared NT handle for this texture
    /// This handle can be used to import the texture into DX12
    pub fn get_shared_handle(&self) -> Result<HANDLE> {
        let mut guard = self.shared_handle.lock();
        if let Some(handle) = *guard {
            return Ok(handle);
        }

        unsafe {
            // Query IDXGIResource1 interface for shared handle creation
            let dxgi_resource: IDXGIResource1 = self
                .texture
                .cast()
                .map_err(|e| anyhow!("Failed to cast to IDXGIResource1: {:?}", e))?;

            // Create shared NT handle
            let handle = dxgi_resource
                .CreateSharedHandle(
                    None, // No security attributes
                    DXGI_SHARED_RESOURCE_READ.0,
                    None, // No name
                )
                .map_err(|e| anyhow!("Failed to create shared handle: {:?}", e))?;

            *guard = Some(handle);
            Ok(handle)
        }
    }

    /// Get the raw D3D11 texture
    pub fn texture(&self) -> &ID3D11Texture2D {
        &self.texture
    }

    /// Get the texture array index
    pub fn array_index(&self) -> u32 {
        self.array_index
    }

    /// Check if this texture is part of a texture array
    pub fn is_texture_array(&self) -> bool {
        unsafe {
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            self.texture.GetDesc(&mut desc);
            desc.ArraySize > 1
        }
    }

    /// Lock the texture and copy Y and UV planes to CPU memory
    /// This is the fallback path when zero-copy import fails
    pub fn lock_and_get_planes(&self) -> Result<LockedPlanes> {
        unsafe {
            // Get the device from the texture itself
            let device = self
                .texture
                .GetDevice()
                .map_err(|e| anyhow!("Failed to get D3D11 device from texture: {:?}", e))?;

            // Get the device context
            let context = device
                .GetImmediateContext()
                .map_err(|e| anyhow!("Failed to get device context: {:?}", e))?;

            // Create a staging texture for CPU access
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            self.texture.GetDesc(&mut desc);

            let staging_desc = D3D11_TEXTURE2D_DESC {
                Width: desc.Width,
                Height: desc.Height,
                MipLevels: 1,
                ArraySize: 1,
                Format: desc.Format,
                SampleDesc: desc.SampleDesc,
                Usage: D3D11_USAGE_STAGING,
                BindFlags: Default::default(),
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: Default::default(),
            };

            let mut staging_texture: Option<ID3D11Texture2D> = None;
            device
                .CreateTexture2D(&staging_desc, None, Some(&mut staging_texture))
                .map_err(|e| anyhow!("Failed to create staging texture: {:?}", e))?;
            let staging_texture = staging_texture.unwrap();

            // Copy from source texture (specific array slice) to staging
            context.CopySubresourceRegion(
                &staging_texture,
                0, // Destination subresource
                0,
                0,
                0, // Destination x, y, z
                &self.texture,
                self.array_index, // Source subresource (array index)
                None,             // Copy entire resource
            );

            // CRITICAL: Wait for GPU copy to complete before mapping
            // Flush() only submits commands but doesn't wait - we need a query to sync
            let query_desc = D3D11_QUERY_DESC {
                Query: D3D11_QUERY_EVENT,
                MiscFlags: 0,
            };
            let mut query: Option<ID3D11Query> = None;
            device
                .CreateQuery(&query_desc, Some(&mut query))
                .map_err(|e| anyhow!("Failed to create query: {:?}", e))?;
            let query = query.ok_or_else(|| anyhow!("Query creation returned None"))?;

            // Cast query to ID3D11Asynchronous for End/GetData calls
            let async_query: ID3D11Asynchronous = query
                .cast()
                .map_err(|e| anyhow!("Failed to cast query to ID3D11Asynchronous: {:?}", e))?;

            // Insert the query after the copy command
            context.End(&async_query);

            // Wait for GPU to complete (poll until done)
            loop {
                let result = context.GetData(&async_query, None, 0, 0);
                if result.is_ok() {
                    break; // Query signaled - GPU work is done
                }
                std::thread::yield_now();
            }

            // Map the staging texture
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context
                .Map(&staging_texture, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .map_err(|e| anyhow!("Failed to map staging texture: {:?}", e))?;

            // NV12 layout: Y plane (full height) followed by UV plane (half height)
            let y_height = desc.Height;
            let uv_height = desc.Height / 2;
            let row_pitch = mapped.RowPitch;

            // Copy Y plane
            let y_size = (row_pitch * y_height) as usize;
            let y_data = std::slice::from_raw_parts(mapped.pData as *const u8, y_size);
            let y_plane = y_data.to_vec();

            // Copy UV plane (starts after Y plane)
            let uv_offset = y_size;
            let uv_size = (row_pitch * uv_height) as usize;
            let uv_data =
                std::slice::from_raw_parts((mapped.pData as *const u8).add(uv_offset), uv_size);
            let uv_plane = uv_data.to_vec();

            // Unmap
            context.Unmap(&staging_texture, 0);

            Ok(LockedPlanes {
                y_plane,
                uv_plane,
                y_stride: row_pitch,
                uv_stride: row_pitch,
                width: desc.Width,
                height: desc.Height,
            })
        }
    }
}

impl Drop for D3D11TextureWrapper {
    fn drop(&mut self) {
        // Close the shared handle if we created one
        if let Some(handle) = self.shared_handle.lock().take() {
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(handle);
            }
        }
        // COM objects (texture) are automatically released when dropped
    }
}

/// Locked plane data from D3D11 texture
pub struct LockedPlanes {
    pub y_plane: Vec<u8>,
    pub uv_plane: Vec<u8>,
    pub y_stride: u32,
    pub uv_stride: u32,
    pub width: u32,
    pub height: u32,
}

/// Manager for D3D11 zero-copy textures
/// Handles device creation and texture import into wgpu
pub struct D3D11ZeroCopyManager {
    /// D3D11 device (shared with FFmpeg)
    device: ID3D11Device,
    /// Whether zero-copy is enabled
    enabled: bool,
}

impl D3D11ZeroCopyManager {
    /// Create a new manager with the given D3D11 device
    pub fn new(device: ID3D11Device) -> Self {
        info!("D3D11 zero-copy manager created");
        Self {
            device,
            enabled: true,
        }
    }

    /// Get the D3D11 device
    pub fn device(&self) -> &ID3D11Device {
        &self.device
    }

    /// Check if zero-copy is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Disable zero-copy (fallback to CPU path)
    pub fn disable(&mut self) {
        warn!("D3D11 zero-copy disabled, falling back to CPU path");
        self.enabled = false;
    }
}

/// Extract D3D11 texture from FFmpeg frame data pointers
///
/// FFmpeg D3D11VA frame layout:
/// - data[0] = ID3D11Texture2D*
/// - data[1] = texture array index (as intptr_t)
///
/// # Safety
/// The data pointers must be from a valid D3D11VA decoded frame
pub unsafe fn extract_d3d11_texture_from_frame(
    data0: *mut u8,
    data1: *mut u8,
) -> Option<D3D11TextureWrapper> {
    if data0.is_null() {
        return None;
    }

    let texture_ptr = data0 as *mut std::ffi::c_void;
    let array_index = data1 as isize as i32;

    D3D11TextureWrapper::from_ffmpeg_frame(texture_ptr, array_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locked_planes_layout() {
        // Test NV12 plane calculations
        let width = 1920u32;
        let height = 1080u32;

        // Y plane: full resolution
        let y_size = width * height;
        assert_eq!(y_size, 2073600);

        // UV plane: half height, same width (interleaved)
        let uv_size = width * (height / 2);
        assert_eq!(uv_size, 1036800);
    }
}
