// Copyright 2023 The ChromiumOS Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::anyhow;
use anyhow::Context as AnyhowContext;
use libva::{
    Buffer, Context, Display, Picture, PictureEnd, PictureNew, PictureSync, Surface,
    SurfaceMemoryDescriptor, VaError,
};

use crate::decoder::stateless::StatelessBackendResult;
use crate::decoder::stateless::StatelessCodec;
use crate::decoder::stateless::StatelessDecoderBackend;
use crate::decoder::stateless::StatelessDecoderBackendPicture;
use crate::decoder::DecodedHandle as DecodedHandleTrait;
use crate::decoder::StreamInfo;
use crate::video_frame::VideoFrame;
use crate::DecodedFormat;
use crate::Rect;
use crate::Resolution;

/// A decoded frame handle.
pub(crate) type DecodedHandle<V> = Rc<RefCell<VaapiDecodedHandle<V>>>;

/// Gets the VASurfaceID for the given `picture`.
pub(crate) fn va_surface_id<V: VideoFrame>(
    handle: &Option<DecodedHandle<V>>,
) -> libva::VASurfaceID {
    match handle {
        None => libva::VA_INVALID_SURFACE,
        Some(handle) => handle.borrow().surface().id(),
    }
}

impl<V: VideoFrame> DecodedHandleTrait for DecodedHandle<V> {
    type Frame = V;

    fn video_frame(&self) -> Arc<Self::Frame> {
        self.borrow().backing_frame.clone()
    }

    fn coded_resolution(&self) -> Resolution {
        self.borrow().surface().size().into()
    }

    fn display_resolution(&self) -> Resolution {
        self.borrow().display_resolution
    }

    fn timestamp(&self) -> u64 {
        self.borrow().timestamp()
    }

    fn is_ready(&self) -> bool {
        self.borrow().state.is_ready().unwrap_or(true)
    }

    fn sync(&self) -> anyhow::Result<()> {
        self.borrow_mut().sync().context("while syncing picture")?;

        Ok(())
    }
}

/// A trait for providing the basic information needed to setup libva for decoding.
pub(crate) trait VaStreamInfo {
    /// Returns the VA profile of the stream.
    fn va_profile(&self) -> anyhow::Result<i32>;
    /// Returns the RT format of the stream.
    #[allow(dead_code)]
    fn rt_format(&self) -> anyhow::Result<u32>;
    /// Returns the minimum number of surfaces required to decode the stream.
    fn min_num_surfaces(&self) -> usize;
    /// Returns the coded size of the surfaces required to decode the stream.
    fn coded_size(&self) -> Resolution;
    /// Returns the visible rectangle within the coded size for the stream.
    fn visible_rect(&self) -> Rect;
}

/// Rendering state of a VA picture.
enum PictureState<M: SurfaceMemoryDescriptor> {
    Ready(Picture<PictureSync, Surface<M>>),
    Pending(Picture<PictureEnd, Surface<M>>),
    // Only set in sync when we take ownership of the VA picture.
    Invalid,
}

impl<M: SurfaceMemoryDescriptor> PictureState<M> {
    /// Make sure that all pending operations on the picture have completed.
    fn sync(&mut self) -> Result<(), VaError> {
        let res;

        (*self, res) = match std::mem::replace(self, PictureState::Invalid) {
            state @ PictureState::Ready(_) => (state, Ok(())),
            PictureState::Pending(picture) => match picture.sync() {
                Ok(picture) => (PictureState::Ready(picture), Ok(())),
                Err((e, picture)) => (PictureState::Pending(picture), Err(e)),
            },
            PictureState::Invalid => unreachable!(),
        };

        res
    }

    fn surface(&self) -> &Surface<M> {
        match self {
            PictureState::Ready(picture) => picture.surface(),
            PictureState::Pending(picture) => picture.surface(),
            PictureState::Invalid => unreachable!(),
        }
    }

    fn timestamp(&self) -> u64 {
        match self {
            PictureState::Ready(picture) => picture.timestamp(),
            PictureState::Pending(picture) => picture.timestamp(),
            PictureState::Invalid => unreachable!(),
        }
    }

    fn is_ready(&self) -> Result<bool, VaError> {
        match self {
            PictureState::Ready(_) => Ok(true),
            PictureState::Pending(picture) => picture
                .surface()
                .query_status()
                .map(|s| s == libva::VASurfaceStatus::VASurfaceReady),
            PictureState::Invalid => unreachable!(),
        }
    }

    fn new_from_same_surface(&self, timestamp: u64) -> Picture<PictureNew, Surface<M>> {
        match &self {
            PictureState::Ready(picture) => Picture::new_from_same_surface(timestamp, picture),
            PictureState::Pending(picture) => Picture::new_from_same_surface(timestamp, picture),
            PictureState::Invalid => unreachable!(),
        }
    }
}

/// VA-API backend handle.
///
/// This includes the VA picture which can be pending rendering or complete, as well as useful
/// meta-information.
pub struct VaapiDecodedHandle<V: VideoFrame> {
    backing_frame: Arc<V>,
    state: PictureState<<V as VideoFrame>::MemDescriptor>,
    /// Actual resolution of the visible rectangle in the decoded buffer.
    display_resolution: Resolution,
}

impl<V: VideoFrame> VaapiDecodedHandle<V> {
    /// Creates a new pending handle on `surface_id`.
    fn new(picture: VaapiPicture<V>, display_resolution: Resolution) -> anyhow::Result<Self> {
        let backing_frame = picture.backing_frame;
        let picture = picture.picture.begin()?.render()?.end()?;
        Ok(Self {
            backing_frame: backing_frame,
            state: PictureState::Pending(picture),
            display_resolution: display_resolution,
        })
    }

    fn sync(&mut self) -> Result<(), VaError> {
        self.state.sync()
    }

    /// Creates a new picture from the surface backing the current one. Useful for interlaced
    /// decoding. TODO: Do we need this for other purposes? We don't intend to support interlaced.
    pub(crate) fn new_picture_from_same_surface(&self, timestamp: u64) -> VaapiPicture<V> {
        VaapiPicture {
            picture: self.state.new_from_same_surface(timestamp),
            backing_frame: self.backing_frame.clone(),
        }
    }

    pub(crate) fn surface(&self) -> &Surface<<V as VideoFrame>::MemDescriptor> {
        self.state.surface()
    }

    /// Returns the timestamp of this handle.
    fn timestamp(&self) -> u64 {
        self.state.timestamp()
    }
}

pub struct VaapiBackend<V: VideoFrame> {
    pub display: Arc<Display>,
    pub context: Rc<Context>,
    stream_info: StreamInfo,
    // TODO: We should try to support context reuse
    _supports_context_reuse: bool,
    _phantom_data: PhantomData<V>,
}

/// Edge length of the placeholder VAContext created before the first SPS.
pub const INITIAL_CONTEXT_SIZE: u32 = 64;

impl<V: VideoFrame> VaapiBackend<V> {
    pub(crate) fn new(display: Arc<libva::Display>, supports_context_reuse: bool) -> Self {
        // Placeholder until the first SPS arrives (`new_sequence` replaces the
        // context). Mesa radeonsi (AMD VCN) rejects H.264 decode contexts
        // smaller than 64x64 with VA_STATUS_ERROR_RESOLUTION_NOT_SUPPORTED.
        let init_stream_info = StreamInfo {
            format: DecodedFormat::NV12,
            coded_resolution: Resolution::from((INITIAL_CONTEXT_SIZE, INITIAL_CONTEXT_SIZE)),
            display_resolution: Resolution::from((INITIAL_CONTEXT_SIZE, INITIAL_CONTEXT_SIZE)),
            min_num_frames: 1,
        };
        let config = display
            .create_config(
                vec![libva::VAConfigAttrib {
                    type_: libva::VAConfigAttribType::VAConfigAttribRTFormat,
                    value: libva::VA_RT_FORMAT_YUV420,
                }],
                libva::VAProfile::VAProfileH264Main,
                libva::VAEntrypoint::VAEntrypointVLD,
            )
            .expect("Could not create initial VAConfig!");
        let context = display
            .create_context::<<V as VideoFrame>::MemDescriptor>(
                &config,
                init_stream_info.coded_resolution.width,
                init_stream_info.coded_resolution.height,
                None,
                true,
            )
            .expect("Could not create initial VAContext!");
        Self {
            display: display,
            context: context,
            _supports_context_reuse: supports_context_reuse,
            stream_info: init_stream_info,
            _phantom_data: Default::default(),
        }
    }

    pub(crate) fn new_sequence<StreamData>(
        &mut self,
        stream_params: &StreamData,
    ) -> StatelessBackendResult<()>
    where
        for<'a> &'a StreamData: VaStreamInfo,
    {
        self.stream_info.display_resolution = Resolution::from(stream_params.visible_rect());
        self.stream_info.coded_resolution = stream_params.coded_size().clone();
        self.stream_info.min_num_frames = stream_params.min_num_surfaces();

        // TODO: Handle context re-use
        // TODO: We should obtain RT_FORMAT from stream_info
        let config = self
            .display
            .create_config(
                vec![libva::VAConfigAttrib {
                    type_: libva::VAConfigAttribType::VAConfigAttribRTFormat,
                    value: libva::VA_RT_FORMAT_YUV420,
                }],
                stream_params.va_profile().map_err(|_| anyhow!("Could not get VAProfile!"))?,
                libva::VAEntrypoint::VAEntrypointVLD,
            )
            .map_err(|_| anyhow!("Could not create VAConfig!"))?;
        let context = self
            .display
            .create_context::<<V as VideoFrame>::MemDescriptor>(
                &config,
                self.stream_info.coded_resolution.width,
                self.stream_info.coded_resolution.height,
                None,
                true,
            )
            .map_err(|_| anyhow!("Could not create VAContext!"))?;
        self.context = context;

        Ok(())
    }

    pub(crate) fn process_picture<Codec: StatelessCodec>(
        &mut self,
        picture: VaapiPicture<V>,
    ) -> StatelessBackendResult<<Self as StatelessDecoderBackend>::Handle>
    where
        Self: StatelessDecoderBackendPicture<Codec>,
        for<'a> &'a Codec::FormatInfo: VaStreamInfo,
    {
        Ok(Rc::new(RefCell::new(VaapiDecodedHandle::new(
            picture,
            self.stream_info.display_resolution.clone(),
        )?)))
    }
}

/// Shortcut for pictures used for the VAAPI backend.
pub struct VaapiPicture<V: VideoFrame> {
    picture: Picture<PictureNew, Surface<V::MemDescriptor>>,
    backing_frame: Arc<V>,
}

impl<V: VideoFrame> VaapiPicture<V> {
    pub fn new(timestamp: u64, context: Rc<Context>, backing_frame: V) -> Self {
        let display = context.display();
        let surface = backing_frame
            .to_native_handle(display)
            .expect("Failed to export video frame to vaapi picture!")
            .into();
        Self {
            backing_frame: Arc::new(backing_frame),
            picture: Picture::new(timestamp, context, surface),
        }
    }

    pub fn surface(&self) -> &Surface<V::MemDescriptor> {
        self.picture.surface()
    }

    pub fn add_buffer(&mut self, buffer: Buffer) {
        self.picture.add_buffer(buffer)
    }
}

impl<V: VideoFrame> StatelessDecoderBackend for VaapiBackend<V> {
    type Handle = DecodedHandle<V>;

    fn stream_info(&self) -> Option<&StreamInfo> {
        Some(&self.stream_info)
    }

    fn reset_backend(&mut self) -> anyhow::Result<()> {
        //TODO(bchoobineh): Implement VAAPI DRC
        Ok(())
    }
}
