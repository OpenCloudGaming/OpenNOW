use super::decoder::DecodedVideoFrame;
use super::embedded::MAX_FRAME_SLOTS;
use crate::VideoFormat;
use crate::y410_color::Y410Constants;
use ::windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use ::windows::Win32::Graphics::Direct3D::{
    D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, D3D_SRV_DIMENSION_TEXTURE2D, ID3DBlob,
};
use ::windows::Win32::Graphics::Direct3D11::*;
use ::windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_R10G10B10A2_UINT, DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_Y410, DXGI_SAMPLE_DESC,
};
use ::windows::core::PCSTR;
struct OutputSlot {
    texture: ID3D11Texture2D,
    view: ID3D11RenderTargetView,
}
pub(super) struct Y410Converter {
    device: ID3D11Device,
    immediate: ID3D11DeviceContext,
    deferred: ID3D11DeviceContext,
    input: ID3D11Texture2D,
    input_view: ID3D11ShaderResourceView,
    vertex: ID3D11VertexShader,
    pixel: ID3D11PixelShader,
    quantization: ID3D11Buffer,
    rasterizer: ID3D11RasterizerState,
    slots: [Option<OutputSlot>; MAX_FRAME_SLOTS],
    format: VideoFormat,
}
impl Y410Converter {
    pub(super) fn new(
        device: &ID3D11Device,
        immediate: &ID3D11DeviceContext,
        format: VideoFormat,
    ) -> Result<Self, String> {
        let constants = Y410Constants::new(format)?;
        let mut input = None;
        let mut input_view = None;
        let mut deferred = None;
        unsafe {
            device
                .CreateTexture2D(
                    &D3D11_TEXTURE2D_DESC {
                        Width: format.width,
                        Height: format.height,
                        MipLevels: 1,
                        ArraySize: 1,
                        Format: DXGI_FORMAT_Y410,
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1,
                            Quality: 0,
                        },
                        Usage: D3D11_USAGE_DEFAULT,
                        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                        ..Default::default()
                    },
                    None,
                    Some(&mut input),
                )
                .map_err(|error| format!("create Y410 shader input: {error}"))?;
        }
        let input = input.ok_or("no Y410 shader input")?;
        unsafe {
            device
                .CreateShaderResourceView(
                    &input,
                    Some(&D3D11_SHADER_RESOURCE_VIEW_DESC {
                        Format: DXGI_FORMAT_R10G10B10A2_UINT,
                        ViewDimension: D3D_SRV_DIMENSION_TEXTURE2D,
                        Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                            Texture2D: D3D11_TEX2D_SRV {
                                MostDetailedMip: 0,
                                MipLevels: 1,
                            },
                        },
                    }),
                    Some(&mut input_view),
                )
                .map_err(|error| format!("create exact Y410 UINT view: {error}"))?;
            device
                .CreateDeferredContext(0, Some(&mut deferred))
                .map_err(|error| format!("create Y410 deferred context: {error}"))?;
        }
        let vertex_code = compile(c"vertex_main", c"vs_5_0")?;
        let pixel_code = compile(c"pixel_main", c"ps_5_0")?;
        let mut vertex = None;
        let mut pixel = None;
        unsafe {
            device
                .CreateVertexShader(
                    std::slice::from_raw_parts(
                        vertex_code.GetBufferPointer().cast(),
                        vertex_code.GetBufferSize(),
                    ),
                    None,
                    Some(&mut vertex),
                )
                .map_err(|error| format!("create Y410 vertex shader: {error}"))?;
            device
                .CreatePixelShader(
                    std::slice::from_raw_parts(
                        pixel_code.GetBufferPointer().cast(),
                        pixel_code.GetBufferSize(),
                    ),
                    None,
                    Some(&mut pixel),
                )
                .map_err(|error| format!("create Y410 pixel shader: {error}"))?;
        }
        let mut quantization = None;
        let mut rasterizer = None;
        unsafe {
            device
                .CreateBuffer(
                    &D3D11_BUFFER_DESC {
                        ByteWidth: std::mem::size_of::<Y410Constants>() as u32,
                        Usage: D3D11_USAGE_IMMUTABLE,
                        BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                        ..Default::default()
                    },
                    Some(&D3D11_SUBRESOURCE_DATA {
                        pSysMem: std::ptr::from_ref(&constants).cast(),
                        ..Default::default()
                    }),
                    Some(&mut quantization),
                )
                .map_err(|error| format!("create Y410 quantization constants: {error}"))?;
            device
                .CreateRasterizerState(
                    &D3D11_RASTERIZER_DESC {
                        FillMode: D3D11_FILL_SOLID,
                        CullMode: D3D11_CULL_NONE,
                        DepthClipEnable: true.into(),
                        ..Default::default()
                    },
                    Some(&mut rasterizer),
                )
                .map_err(|error| format!("create Y410 rasterizer: {error}"))?;
        }
        Ok(Self {
            device: device.clone(),
            immediate: immediate.clone(),
            deferred: deferred.ok_or("no Y410 deferred context")?,
            input,
            input_view: input_view.ok_or("no Y410 shader view")?,
            vertex: vertex.ok_or("no Y410 vertex shader")?,
            pixel: pixel.ok_or("no Y410 pixel shader")?,
            quantization: quantization.ok_or("no Y410 quantization buffer")?,
            rasterizer: rasterizer.ok_or("no Y410 rasterizer")?,
            slots: std::array::from_fn(|_| None),
            format,
        })
    }
    pub(super) fn record(
        &mut self,
        slot: usize,
        frame: &DecodedVideoFrame,
    ) -> Result<&ID3D11Texture2D, String> {
        if slot >= MAX_FRAME_SLOTS || frame.format != self.format {
            return Err("Y410 converter frame or slot mismatch".to_owned());
        }
        if self.slots[slot].is_none() {
            let mut texture = None;
            let mut view = None;
            unsafe {
                self.device
                    .CreateTexture2D(
                        &D3D11_TEXTURE2D_DESC {
                            Width: self.format.width,
                            Height: self.format.height,
                            MipLevels: 1,
                            ArraySize: 1,
                            Format: DXGI_FORMAT_R10G10B10A2_UNORM,
                            SampleDesc: DXGI_SAMPLE_DESC {
                                Count: 1,
                                Quality: 0,
                            },
                            Usage: D3D11_USAGE_DEFAULT,
                            BindFlags: (D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET).0
                                as u32,
                            ..Default::default()
                        },
                        None,
                        Some(&mut texture),
                    )
                    .map_err(|error| format!("create Y410 RGB10A2 slot: {error}"))?;
            }
            let texture = texture.ok_or("no Y410 RGB10A2 slot")?;
            unsafe {
                self.device
                    .CreateRenderTargetView(&texture, None, Some(&mut view))
            }
            .map_err(|error| format!("create Y410 RGB10A2 view: {error}"))?;
            self.slots[slot] = Some(OutputSlot {
                texture,
                view: view.ok_or("no Y410 RGB10A2 view")?,
            });
        }
        let output = self.slots[slot].as_ref().ok_or("no Y410 output slot")?;
        let region = D3D11_BOX {
            left: frame.aperture.x,
            top: frame.aperture.y,
            front: 0,
            right: frame.aperture.x + frame.aperture.width,
            bottom: frame.aperture.y + frame.aperture.height,
            back: 1,
        };
        unsafe {
            self.deferred.CopySubresourceRegion(
                &self.input,
                0,
                0,
                0,
                0,
                &frame.texture,
                frame.subresource,
                Some(&region),
            );
            self.deferred
                .IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            self.deferred.VSSetShader(&self.vertex, None);
            self.deferred.PSSetShader(&self.pixel, None);
            self.deferred
                .PSSetShaderResources(0, Some(&[Some(self.input_view.clone())]));
            self.deferred
                .PSSetConstantBuffers(0, Some(&[Some(self.quantization.clone())]));
            self.deferred.RSSetState(&self.rasterizer);
            self.deferred.RSSetViewports(Some(&[D3D11_VIEWPORT {
                Width: self.format.width as f32,
                Height: self.format.height as f32,
                MaxDepth: 1.0,
                ..Default::default()
            }]));
            self.deferred
                .OMSetRenderTargets(Some(&[Some(output.view.clone())]), None);
            self.deferred.Draw(3, 0);
            let mut commands = None;
            self.deferred
                .FinishCommandList(false, Some(&mut commands))
                .map_err(|error| format!("finish Y410 conversion commands: {error}"))?;
            self.immediate
                .ExecuteCommandList(&commands.ok_or("no Y410 conversion commands")?, true);
            self.device
                .GetDeviceRemovedReason()
                .map_err(|error| format!("Y410 conversion device lost: {error}"))?;
        }
        Ok(&output.texture)
    }
}
fn compile(entry: &std::ffi::CStr, target: &std::ffi::CStr) -> Result<ID3DBlob, String> {
    let source = include_bytes!("y410.hlsl");
    let mut code = None;
    let mut errors = None;
    let result = unsafe {
        D3DCompile(
            source.as_ptr().cast(),
            source.len(),
            PCSTR::null(),
            None,
            None,
            PCSTR(entry.as_ptr().cast()),
            PCSTR(target.as_ptr().cast()),
            0,
            0,
            &mut code,
            Some(&mut errors),
        )
    };
    if let Err(error) = result {
        let detail = errors
            .map(|errors| unsafe {
                String::from_utf8_lossy(std::slice::from_raw_parts(
                    errors.GetBufferPointer().cast(),
                    errors.GetBufferSize(),
                ))
                .into_owned()
            })
            .unwrap_or_default();
        return Err(format!("compile Y410 shader: {error}: {detail}"));
    }
    code.ok_or_else(|| "no Y410 shader bytecode".to_owned())
}
