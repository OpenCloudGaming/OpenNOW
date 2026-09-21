use std::collections::HashSet;

use ::windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_10_1,
    D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use ::windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
    D3D11_DECODER_PROFILE_AV1_VLD_PROFILE0, D3D11_DECODER_PROFILE_H264_VLD_FGT,
    D3D11_DECODER_PROFILE_H264_VLD_NOFGT, D3D11_DECODER_PROFILE_HEVC_VLD_MAIN,
    D3D11_DECODER_PROFILE_HEVC_VLD_MAIN10, D3D11_SDK_VERSION, D3D11_VIDEO_DECODER_DESC,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11VideoDevice,
};
use ::windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_NV12, DXGI_FORMAT_P010};
use ::windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_FLAG_REMOTE, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_ERROR_NOT_FOUND,
    DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE, IDXGIAdapter, IDXGIAdapter1, IDXGIFactory1,
    IDXGIFactory6,
};
use ::windows::core::{BOOL, GUID, Interface};

use crate::adapter_decode::{AdapterDecodeIndex, DecodeProfile};

const PROFILE_WIDTHS: [(u32, u32); 2] = [(1920, 1080), (1280, 720)];

pub(crate) fn probe_adapter_decode() -> Vec<AdapterDecodeIndex> {
    let adapters = match enumerate_adapters() {
        Ok(adapters) => adapters,
        Err(error) => {
            video_log!("Windows graphics adapter decode index failed: {error}");
            return Vec::new();
        }
    };
    let mut indexed = Vec::with_capacity(adapters.len());
    for (adapter, name, luid) in adapters {
        let mut index = AdapterDecodeIndex::named(luid, name);
        match query_decode_profiles(&adapter) {
            Ok(profiles) => {
                for (profile, supported) in profiles {
                    index.apply_profile(profile, supported);
                }
                index.finish();
            }
            Err(error) => {
                index.reason = Some(bounded_reason(error));
            }
        }
        video_log!(
            "Windows adapter decode index name={} h264={} h265={} h265_main10={} av1={} reason={}",
            index.name,
            index.h264,
            index.h265,
            index.h265_main10,
            index.av1,
            index.reason.as_deref().unwrap_or("")
        );
        indexed.push(index);
    }
    indexed
}

fn enumerate_adapters() -> Result<Vec<(IDXGIAdapter1, String, u64)>, String> {
    unsafe {
        let factory: IDXGIFactory1 =
            CreateDXGIFactory1().map_err(|error| format!("CreateDXGIFactory1: {error}"))?;
        let preferred: Option<IDXGIFactory6> = factory.cast().ok();
        let mut adapters = Vec::new();
        let mut seen = HashSet::new();
        for index in 0..64u32 {
            let adapter = if let Some(preferred) = &preferred {
                match preferred.EnumAdapterByGpuPreference::<IDXGIAdapter1>(
                    index,
                    DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
                ) {
                    Ok(adapter) => adapter,
                    Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                    Err(error) => {
                        return Err(format!("EnumAdapterByGpuPreference: {error}"));
                    }
                }
            } else {
                match factory.EnumAdapters1(index) {
                    Ok(adapter) => adapter,
                    Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                    Err(error) => return Err(format!("EnumAdapters1: {error}")),
                }
            };
            let description = match adapter.GetDesc1() {
                Ok(description) => description,
                Err(error) => {
                    video_log!("Windows adapter {index} description failed: {error}");
                    continue;
                }
            };
            let excluded = (DXGI_ADAPTER_FLAG_SOFTWARE.0 | DXGI_ADAPTER_FLAG_REMOTE.0) as u32;
            if description.Flags & excluded != 0 {
                continue;
            }
            let luid = ((description.AdapterLuid.HighPart as u32 as u64) << 32)
                | u64::from(description.AdapterLuid.LowPart);
            if luid == 0 || !seen.insert(luid) {
                continue;
            }
            let name = adapter_name(&description.Description);
            adapters.push((adapter, name, luid));
        }
        Ok(adapters)
    }
}

fn query_decode_profiles(adapter: &IDXGIAdapter1) -> Result<Vec<(DecodeProfile, bool)>, String> {
    let dxgi_adapter: IDXGIAdapter = adapter
        .cast()
        .map_err(|error| format!("DXGI adapter interface: {error}"))?;
    let (device, _context) = create_video_device(&dxgi_adapter)?;
    let video: ID3D11VideoDevice = device
        .cast()
        .map_err(|error| format!("D3D11 video device: {error}"))?;
    let count = unsafe { video.GetVideoDecoderProfileCount() }.min(64);
    let mut profiles = Vec::new();
    for index in 0..count {
        let profile = match unsafe { video.GetVideoDecoderProfile(index) } {
            Ok(profile) => profile,
            Err(error) => {
                video_log!("Windows decoder profile {index} failed: {error}");
                continue;
            }
        };
        let kind = classify_profile(profile);
        if kind == DecodeProfile::Other {
            continue;
        }
        let format = if kind == DecodeProfile::H265Main10 {
            DXGI_FORMAT_P010
        } else {
            DXGI_FORMAT_NV12
        };
        let supported = profile_supported(&video, profile, format)
            || (kind == DecodeProfile::Av1 && profile_supported(&video, profile, DXGI_FORMAT_P010));
        profiles.push((kind, supported));
    }
    Ok(profiles)
}

fn create_video_device(
    adapter: &IDXGIAdapter,
) -> Result<(ID3D11Device, ID3D11DeviceContext), String> {
    let levels = [
        D3D_FEATURE_LEVEL_11_1,
        D3D_FEATURE_LEVEL_11_0,
        D3D_FEATURE_LEVEL_10_1,
        D3D_FEATURE_LEVEL_10_0,
    ];
    let mut device = None;
    let mut context = None;
    unsafe {
        D3D11CreateDevice(
            Some(adapter),
            D3D_DRIVER_TYPE_UNKNOWN,
            ::windows::Win32::Foundation::HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
            Some(&levels),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
        .map_err(|error| format!("D3D11CreateDevice: {error}"))?;
    }
    Ok((
        device.ok_or("D3D11CreateDevice returned no device")?,
        context.ok_or("D3D11CreateDevice returned no immediate context")?,
    ))
}

fn classify_profile(profile: GUID) -> DecodeProfile {
    if profile == D3D11_DECODER_PROFILE_H264_VLD_NOFGT
        || profile == D3D11_DECODER_PROFILE_H264_VLD_FGT
    {
        DecodeProfile::H264
    } else if profile == D3D11_DECODER_PROFILE_HEVC_VLD_MAIN {
        DecodeProfile::H265Main
    } else if profile == D3D11_DECODER_PROFILE_HEVC_VLD_MAIN10 {
        DecodeProfile::H265Main10
    } else if profile == D3D11_DECODER_PROFILE_AV1_VLD_PROFILE0 {
        DecodeProfile::Av1
    } else {
        DecodeProfile::Other
    }
}

fn profile_supported(video: &ID3D11VideoDevice, profile: GUID, format: DXGI_FORMAT) -> bool {
    unsafe {
        let supported = video
            .CheckVideoDecoderFormat(&profile, format)
            .unwrap_or(BOOL(0));
        if !supported.as_bool() {
            return false;
        }
        PROFILE_WIDTHS.iter().any(|(width, height)| {
            let description = D3D11_VIDEO_DECODER_DESC {
                Guid: profile,
                SampleWidth: *width,
                SampleHeight: *height,
                OutputFormat: format,
            };
            video.GetVideoDecoderConfigCount(&description).unwrap_or(0) > 0
        })
    }
}

fn adapter_name(description: &[u16]) -> String {
    let end = description
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(description.len());
    crate::adapter_decode::sanitize_adapter_name(&String::from_utf16_lossy(&description[..end]))
}

fn bounded_reason(value: impl AsRef<str>) -> String {
    value
        .as_ref()
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(180)
        .collect()
}
