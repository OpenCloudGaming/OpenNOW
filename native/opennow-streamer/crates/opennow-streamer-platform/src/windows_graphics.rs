use opennow_streamer_platform_windows::{AdoptedD3d11Context, D3d11Frame};

use crate::{
    GraphicsApi, GraphicsColorSpace, GraphicsContext, GraphicsFrame, GraphicsFrameInfo,
    GraphicsRecordCommand, GraphicsRecordedFrame, GraphicsTextureFormat,
};

impl From<opennow_streamer_platform_windows::D3d11ColorSpace> for GraphicsColorSpace {
    fn from(value: opennow_streamer_platform_windows::D3d11ColorSpace) -> Self {
        match value {
            opennow_streamer_platform_windows::D3d11ColorSpace::Sdr709 => Self::Sdr709,
            opennow_streamer_platform_windows::D3d11ColorSpace::Pq2020 => Self::Pq2020,
            opennow_streamer_platform_windows::D3d11ColorSpace::Hlg2020 => Self::Hlg2020,
        }
    }
}

impl GraphicsFrame for D3d11Frame {
    fn info(&self) -> GraphicsFrameInfo {
        GraphicsFrameInfo {
            width: self.width(),
            height: self.height(),
            sequence: self.sequence(),
            presentation_time_ns: self.presentation_time_ns(),
        }
    }

    fn record(
        &self,
        context: GraphicsContext,
        command: GraphicsRecordCommand,
    ) -> Result<GraphicsRecordedFrame, crate::GraphicsFrameError> {
        if context.api != GraphicsApi::D3d11 {
            return Err("a Windows decoded frame requires a D3D11 graphics context"
                .to_owned()
                .into());
        }
        if command.command_buffer != context.queue {
            return Err(
                "D3D11 record command does not use Qt's adopted immediate context"
                    .to_owned()
                    .into(),
            );
        }
        let frame = unsafe {
            self.record(
                AdoptedD3d11Context {
                    device: context.device as *mut std::ffi::c_void,
                    immediate_context: context.queue as *mut std::ffi::c_void,
                },
                command.frame_slot,
            )
        }
        .map_err(|error| error.to_string())?;
        Ok(GraphicsRecordedFrame {
            resource: frame.texture as usize as u64,
            resource_view: 0,
            color_space: frame.color_space.into(),
            texture_format: match frame.texture_format {
                opennow_streamer_platform_windows::D3d11TextureFormat::Rgba8 => {
                    GraphicsTextureFormat::Rgba8
                }
                opennow_streamer_platform_windows::D3d11TextureFormat::Rgb10A2 => {
                    GraphicsTextureFormat::Rgb10A2
                }
            },
            width: frame.width,
            height: frame.height,
            frame_slot: frame.frame_slot,
            generation: frame.generation,
            presentation_time_ns: frame.presentation_time_ns,
        })
    }
}
