// Copyright 2023 The ChromiumOS Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

//! Stateless decoders.
//!
//! Stateless here refers to the backend API targeted by these decoders. The decoders themselves do
//! hold the decoding state so the backend doesn't need to.
//!
//! The [`StatelessDecoder`] struct is the basis of all stateless decoders. It is created by
//! combining a codec codec to a [backend](crate::backend), after which bitstream units can be
//! submitted through the [`StatelessDecoder::decode`] method.

#[cfg(feature = "av1")]
pub mod av1;
pub mod h264;
#[cfg(feature = "h265")]
pub mod h265;
pub mod vp8;
pub mod vp9;

use std::os::fd::AsFd;
use std::os::fd::AsRawFd;
use std::os::fd::BorrowedFd;
use std::time::Duration;

use nix::errno::Errno;
#[cfg(not(target_os = "freebsd"))]
use nix::sys::epoll::Epoll;
#[cfg(not(target_os = "freebsd"))]
use nix::sys::epoll::EpollCreateFlags;
#[cfg(not(target_os = "freebsd"))]
use nix::sys::epoll::EpollEvent;
#[cfg(not(target_os = "freebsd"))]
use nix::sys::epoll::EpollFlags;
use nix::sys::eventfd::EventFd;
use thiserror::Error;

use crate::decoder::BlockingMode;
use crate::decoder::DecodedHandle;
use crate::decoder::DecoderEvent;
use crate::decoder::DynDecodedHandle;
use crate::decoder::ReadyFramesQueue;
use crate::decoder::StreamInfo;
use crate::Resolution;

#[cfg(target_os = "freebsd")]
use kqueue_poll::Epoll;

/// FreeBSD has no epoll. A kqueue descriptor polls readable while any of its registered
/// descriptors is readable, which is all the decoder needs from its event union.
#[cfg(target_os = "freebsd")]
mod kqueue_poll {
    use std::os::fd::AsRawFd;
    use std::os::fd::BorrowedFd;
    use std::os::fd::FromRawFd;
    use std::os::fd::OwnedFd;

    use nix::errno::Errno;
    use nix::libc;

    pub struct Epoll(pub OwnedFd);

    impl Epoll {
        pub fn new() -> Result<Self, Errno> {
            let raw = unsafe { libc::kqueue() };
            if raw < 0 {
                return Err(Errno::last());
            }
            let fd = unsafe { OwnedFd::from_raw_fd(raw) };
            if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
                return Err(Errno::last());
            }
            Ok(Self(fd))
        }

        pub fn add(&self, fd: BorrowedFd<'_>) -> Result<(), Errno> {
            let mut change: libc::kevent = unsafe { std::mem::zeroed() };
            change.ident = fd.as_raw_fd() as libc::uintptr_t;
            change.filter = libc::EVFILT_READ;
            change.flags = libc::EV_ADD;
            let result = unsafe {
                libc::kevent(
                    self.0.as_raw_fd(),
                    &change,
                    1,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null(),
                )
            };
            if result < 0 {
                Err(Errno::last())
            } else {
                Ok(())
            }
        }
    }
}

/// Error returned by `new_picture` methods of the backend, usually to indicate which kind of
/// resource is needed before the picture can be successfully created.
#[derive(Error, Debug)]
pub enum NewPictureError {
    /// Indicates that the backend needs one output buffer to be returned to the pool before it can
    /// proceed.
    #[error("need one output buffer to be returned before operation can proceed")]
    OutOfOutputBuffers,
    /// An unrecoverable backend error has occured.
    #[error(transparent)]
    BackendError(#[from] anyhow::Error),
}

pub type NewPictureResult<T> = Result<T, NewPictureError>;

/// Error returned by stateless backend methods.
#[derive(Error, Debug)]
pub enum StatelessBackendError {
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Result type returned by stateless backend methods.
pub type StatelessBackendResult<T> = Result<T, StatelessBackendError>;

/// Decoder implementations can use this struct to represent their decoding state.
///
/// `F` is a type containing the parsed stream format, that the decoder will use for format
/// negotiation with the client.
#[derive(Clone, Default)]
enum DecodingState<F> {
    /// Decoder will ignore all input until format and resolution information passes by.
    #[default]
    AwaitingStreamInfo,
    /// Decoder is stopped until the client has confirmed the output format.
    AwaitingFormat(F),
    /// Decoder is currently decoding input.
    Decoding,
    /// Decoder has been reset after a flush, and can resume with the current parameters after
    /// seeing a key frame.
    Reset,
    // Decoder has recently seen a DRC frame and is flushing and waiting for all frames to be
    // output before performing the DRC operation.
    FlushingForDRC,
}

/// Error returned by the [`StatelessVideoDecoder::decode`] method.
#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("not enough output buffers available to continue, need {0} more")]
    NotEnoughOutputBuffers(usize),
    #[error("cannot accept more input until pending events are processed")]
    CheckEvents,
    #[error("error while parsing frame: {0}")]
    ParseFrameError(String),
    #[error("{0}")]
    ReferenceLost(String),
    #[error(transparent)]
    DecoderError(#[from] anyhow::Error),
    #[error(transparent)]
    BackendError(#[from] StatelessBackendError),
}

/// Convenience conversion for codecs that process a single frame per decode call.
impl From<NewPictureError> for DecodeError {
    fn from(err: NewPictureError) -> Self {
        match err {
            NewPictureError::OutOfOutputBuffers => DecodeError::NotEnoughOutputBuffers(1),
            NewPictureError::BackendError(e) => {
                DecodeError::BackendError(StatelessBackendError::Other(e))
            }
        }
    }
}

/// Error returned by the [`StatelessVideoDecoder::wait_for_next_event`] method.
#[derive(Debug, Error)]
pub enum WaitNextEventError {
    #[error("timed out while waiting for next decoder event")]
    TimedOut,
}

/// Specifies the type of picture that a backend will create for a given codec.
///
/// The picture type is state that is preserved from the start of a given frame to its submission
/// to the backend. Some codecs don't need it, in this case they can just set `Picture` to `()`.
pub trait StatelessDecoderBackendPicture<Codec: StatelessCodec> {
    /// Backend-specific type representing a frame being decoded. Useful for decoders that need
    /// to render a frame in several steps and to preserve its state in between.
    ///
    /// Backends that don't use this can simply set it to `()`.
    type Picture;
}

/// Common trait shared by all stateless video decoder backends, providing codec-independent
/// methods.
pub trait StatelessDecoderBackend {
    /// The type that the backend returns as a result of a decode operation.
    /// This will usually be some backend-specific type with a resource and a
    /// resource pool so that said buffer can be reused for another decode
    /// operation when it goes out of scope.
    type Handle: DecodedHandle;

    /// Returns the current decoding parameters, as parsed from the stream.
    fn stream_info(&self) -> Option<&StreamInfo>;

    // Resets the backend decoder to accommodate for a format change event.
    fn reset_backend(&mut self) -> anyhow::Result<()>;
}

/// Stateless video decoder interface.
///
/// A stateless decoder differs from a stateful one in that its input and output queues are not
/// operating independently: a new decode unit can only be processed if there is already an output
/// resource available to receive its decoded content.
///
/// Therefore [`decode`] can refuse work if there is no output resource
/// available at the time of calling, in which case the caller is responsible for calling
/// [`decode`] again with the same parameters after processing at least one
/// pending output frame and returning it to the decoder.
///
/// The `M` generic parameter is the type of the memory descriptor backing the output frames.
///
/// [`decode`]: StatelessVideoDecoder::decode
pub trait StatelessVideoDecoder {
    /// Type of the [`DecodedHandle`]s that decoded frames are returned into.
    type Handle: DecodedHandle;

    /// Attempts to decode `bitstream` if the current conditions allow it.
    ///
    /// This method will return [`DecodeError::CheckEvents`] if processing cannot take place until
    /// pending events are handled. This could either be because a change of output format has
    /// been detected that the client should acknowledge, or because there are no available output
    /// resources and dequeueing and returning pending frames will fix that. After the cause has
    /// been addressed, the client is responsible for calling this method again with the same data.
    ///
    /// The return value is the number of bytes in `bitstream` that have been processed. Usually
    /// this will be equal to the length of `bitstream`, but some codecs may only do partial
    /// processing if e.g. several units are sent at the same time. It is the responsibility of the
    /// caller to check that all submitted input has been processed, and to resubmit the
    /// unprocessed part if it hasn't. See the documentation of each codec for their expectations.
    fn decode(
        &mut self,
        timestamp: u64,
        bitstream: &[u8],
        alloc_cb: &mut dyn FnMut() -> Option<<Self::Handle as DecodedHandle>::Frame>,
    ) -> Result<usize, DecodeError>;

    /// Flush the decoder i.e. finish processing all pending decode requests and make sure the
    /// resulting frames are ready to be retrieved via [`next_event`].
    ///
    /// Note that after flushing, a key frame must be submitted before decoding can resume.
    ///
    /// [`next_event`]: StatelessVideoDecoder::next_event
    fn flush(&mut self) -> Result<(), DecodeError>;

    fn stream_info(&self) -> Option<&StreamInfo>;

    /// Returns the next event, if there is any pending.
    fn next_event(&mut self) -> Option<DecoderEvent<Self::Handle>>;

    /// Blocks until [`StatelessVideoDecoder::next_event`] is expected to return `Some` or
    /// `timeout` has elapsed.
    ///
    /// Wait for the next event and return it, or return `None` if `timeout` has been reached while
    /// waiting.
    fn wait_for_next_event(&mut self, timeout: Duration) -> Result<(), WaitNextEventError> {
        // Wait until the next event is available.
        let mut fd = nix::libc::pollfd {
            fd: self.poll_fd().as_raw_fd(),
            events: nix::libc::POLLIN,
            revents: 0,
        };
        // SAFETY: `fd` is a valid reference to a properly-filled `pollfd`.
        match unsafe { nix::libc::poll(&mut fd, 1, timeout.as_millis() as i32) } {
            0 => Err(WaitNextEventError::TimedOut),
            _ => Ok(()),
        }
    }

    /// Returns a file descriptor that signals `POLLIN` whenever an event is pending on this
    /// decoder.
    fn poll_fd(&self) -> BorrowedFd;

    /// Transforms the decoder into a [`StatelessVideoDecoder`] trait object.
    ///
    /// All decoders going through this method present the same virtual interface when they return.
    /// This is useful in order avoid monomorphization of application code that can control
    /// decoders using various codecs or backends.
    fn into_trait_object(self) -> DynStatelessVideoDecoder<<Self::Handle as DecodedHandle>::Frame>
    where
        Self: Sized + 'static,
        Self::Handle: 'static,
    {
        Box::new(DynStatelessVideoDecoderWrapper(self))
    }
}

/// Wrapper type for a `StatelessVideoDecoder` that can be turned into a trait object with a common
/// interface.
struct DynStatelessVideoDecoderWrapper<D: StatelessVideoDecoder>(D);

impl<D> StatelessVideoDecoder for DynStatelessVideoDecoderWrapper<D>
where
    D: StatelessVideoDecoder,
    <D as StatelessVideoDecoder>::Handle: 'static,
{
    type Handle = DynDecodedHandle<<D::Handle as DecodedHandle>::Frame>;

    fn decode(
        &mut self,
        timestamp: u64,
        bitstream: &[u8],
        alloc_cb: &mut dyn FnMut() -> Option<<Self::Handle as DecodedHandle>::Frame>,
    ) -> Result<usize, DecodeError> {
        self.0.decode(timestamp, bitstream, alloc_cb)
    }

    fn flush(&mut self) -> Result<(), DecodeError> {
        self.0.flush()
    }

    fn stream_info(&self) -> Option<&StreamInfo> {
        self.0.stream_info()
    }

    fn next_event(&mut self) -> Option<DecoderEvent<Self::Handle>> {
        self.0.next_event().map(|e| match e {
            DecoderEvent::FrameReady(h) => {
                DecoderEvent::FrameReady(Box::new(h) as DynDecodedHandle<_>)
            }
            DecoderEvent::FormatChanged => DecoderEvent::FormatChanged,
        })
    }

    fn poll_fd(&self) -> BorrowedFd {
        self.0.poll_fd()
    }
}

pub type DynStatelessVideoDecoder<D> = Box<dyn StatelessVideoDecoder<Handle = DynDecodedHandle<D>>>;

pub trait StatelessCodec {
    /// Type providing current format information for the codec: resolution, color format, etc.
    ///
    /// For H.264 this would be the Sps, for VP8 or VP9 the frame header.
    type FormatInfo;
    /// State that needs to be kept during a decoding operation, typed by backend.
    type DecoderState<H: DecodedHandle, P>;
}

/// A struct that serves as a basis to implement a stateless decoder.
///
/// A stateless decoder is defined by three generic parameters:
///
/// * A codec, represented by a type that implements [`StatelessCodec`]. This type defines the
/// codec-specific decoder state and other codec properties.
/// * A backend, i.e. an interface to talk to the hardware that accelerates decoding. An example is
/// the VAAPI backend that uses VAAPI for acceleration. The backend will typically itself be typed
/// against a memory decriptor, defining how memory is provided for decoded frames.
///
/// So for instance, a decoder for the H264 codec, using VAAPI for acceleration with self-managed
/// memory, will have the following type:
///
/// ```text
/// let decoder: StatelessDecoder<H264, VaapiBackend<()>>;
/// ```
///
/// This struct just manages the high-level decoder state as well as the queue of decoded frames.
/// All the rest is left to codec-specific code.
pub struct StatelessDecoder<C, B>
where
    C: StatelessCodec,
    B: StatelessDecoderBackend + StatelessDecoderBackendPicture<C>,
{
    /// The current coded resolution
    coded_resolution: Resolution,

    /// Whether the decoder should block on decode operations.
    blocking_mode: BlockingMode,

    ready_queue: ReadyFramesQueue<B::Handle>,

    decoding_state: DecodingState<C::FormatInfo>,

    /// The backend used for hardware acceleration.
    backend: B,

    /// Codec-specific state.
    codec: C::DecoderState<B::Handle, B::Picture>,

    /// Signaled whenever the decoder is in `AwaitingFormat` state.
    awaiting_format_event: EventFd,

    /// Union of `awaiting_format_event` and `ready_queue` to signal whenever there is an event
    /// (frame ready or format change) pending.
    epoll_fd: Epoll,
}

#[derive(Debug, Error)]
pub enum NewStatelessDecoderError {
    #[error("failed to create EventFd for ready frames queue: {0}")]
    ReadyFramesQueue(Errno),
    #[error("failed to create EventFd for awaiting format event: {0}")]
    AwaitingFormatEventFd(Errno),
    #[error("failed to create Epoll for decoder: {0}")]
    Epoll(Errno),
    #[error("failed to add poll FDs to decoder Epoll: {0}")]
    EpollAdd(Errno),
    #[error("failed to initialize the driver")]
    DriverInitialization,
}

impl<C, B> StatelessDecoder<C, B>
where
    C: StatelessCodec,
    B: StatelessDecoderBackend + StatelessDecoderBackendPicture<C>,
    C::DecoderState<B::Handle, B::Picture>: Default,
{
    pub fn new(backend: B, blocking_mode: BlockingMode) -> Result<Self, NewStatelessDecoderError> {
        let ready_queue =
            ReadyFramesQueue::new().map_err(NewStatelessDecoderError::ReadyFramesQueue)?;
        let awaiting_format_event =
            EventFd::new().map_err(NewStatelessDecoderError::AwaitingFormatEventFd)?;
        #[cfg(not(target_os = "freebsd"))]
        let epoll_fd = {
            let epoll_fd =
                Epoll::new(EpollCreateFlags::empty()).map_err(NewStatelessDecoderError::Epoll)?;
            epoll_fd
                .add(ready_queue.poll_fd(), EpollEvent::new(EpollFlags::EPOLLIN, 1))
                .map_err(NewStatelessDecoderError::EpollAdd)?;
            epoll_fd
                .add(awaiting_format_event.as_fd(), EpollEvent::new(EpollFlags::EPOLLIN, 2))
                .map_err(NewStatelessDecoderError::EpollAdd)?;
            epoll_fd
        };
        #[cfg(target_os = "freebsd")]
        let epoll_fd = {
            let epoll_fd = Epoll::new().map_err(NewStatelessDecoderError::Epoll)?;
            epoll_fd
                .add(ready_queue.poll_fd())
                .map_err(NewStatelessDecoderError::EpollAdd)?;
            epoll_fd
                .add(awaiting_format_event.as_fd())
                .map_err(NewStatelessDecoderError::EpollAdd)?;
            epoll_fd
        };

        Ok(Self {
            backend,
            blocking_mode,
            coded_resolution: Default::default(),
            decoding_state: Default::default(),
            ready_queue,
            codec: Default::default(),
            awaiting_format_event,
            epoll_fd,
        })
    }

    /// Switch the decoder into `AwaitingFormat` state, making it refuse any input until the
    /// `FormatChanged` event is processed.
    fn await_format_change(&mut self, format_info: C::FormatInfo) {
        self.decoding_state = DecodingState::AwaitingFormat(format_info);
        self.awaiting_format_event.write(1).unwrap();
    }

    // If the decoder is in a flushing state, the decoder will wait until the ready_queue is
    // empty. Once the ready_queue is empty, the decoder will reset the backend.
    fn wait_for_drc_flush(&mut self) -> Result<(), DecodeError> {
        if matches!(self.decoding_state, DecodingState::FlushingForDRC) {
            if self.ready_queue.queue.is_empty() {
                // We can start the DRC operation since the ready queue is empty.
                self.backend.reset_backend()?;
                self.decoding_state = DecodingState::Reset;
            } else {
                // Since the ready queue is not empty yet, we can't start the DRC.
                // Clear the awaiting format event as we cannot process this yet.
                self.awaiting_format_event.read().unwrap();
                return Err(DecodeError::CheckEvents);
            }
        }
        Ok(())
    }

    /// Returns the next pending event, if any, using `on_format_changed` as the format change
    /// callback of the [`StatelessDecoderFormatNegotiator`] if there is a resolution change event
    /// pending.
    fn query_next_event<F>(&mut self, on_format_changed: F) -> Option<DecoderEvent<B::Handle>>
    where
        Self: StatelessVideoDecoder<Handle = B::Handle>,
        C::FormatInfo: Clone,
        F: Fn(&mut Self, &C::FormatInfo) + 'static,
    {
        // The next event is either the next frame, or, if we are awaiting negotiation, the format
        // change event that will allow us to keep going.
        self.ready_queue.next().map(DecoderEvent::FrameReady).or_else(|| {
            if let DecodingState::AwaitingFormat(format_info) = self.decoding_state.clone() {
                on_format_changed(self, &format_info);
                self.decoding_state = DecodingState::Reset;
                self.awaiting_format_event.read().unwrap();
                Some(DecoderEvent::FormatChanged)
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::decoder::stateless::StatelessVideoDecoder;
    use crate::decoder::DecodedHandle;

    /// Stream that can be used in tests, along with the CRC32 of all of its frames.
    pub struct TestStream {
        /// Bytestream to decode.
        pub stream: &'static [u8],
        /// Expected CRC for each frame, one per line.
        pub crcs: &'static str,
    }

    /// Run the codec-specific `decoding_loop` on a `decoder` with a given `test`, linearly
    /// decoding the stream until its end.
    ///
    /// If `check_crcs` is `true`, then the expected CRCs of the decoded images are compared
    /// against the existing result. We may want to set this to false when using a decoder backend
    /// that does not produce actual frames.
    ///
    /// `dump_yuv` will dump all the decoded frames into `/tmp/framexxx.yuv`. Set this to true in
    /// order to debug the output of the test.
    pub fn test_decode_stream<D, H, FP, L>(
        decoding_loop: L,
        mut decoder: D,
        test: &TestStream,
        check_crcs: bool,
        dump_yuv: bool,
    ) where
        H: DecodedHandle,
        D: StatelessVideoDecoder<Handle = H>,
        L: Fn(&mut D, &[u8], &mut dyn FnMut(H)) -> anyhow::Result<()>,
    {
        let mut crcs = test.crcs.lines().enumerate();

        decoding_loop(&mut decoder, test.stream, &mut |handle| {
            let (frame_num, expected_crc) = crcs.next().expect("decoded more frames than expected");

            if check_crcs || dump_yuv {
                handle.sync().unwrap();
                let picture = handle.dyn_picture();
                let mut backend_handle = picture.dyn_mappable_handle().unwrap();

                let buffer_size = backend_handle.image_size();
                let mut nv12 = vec![0; buffer_size];

                backend_handle.read(&mut nv12).unwrap();

                if dump_yuv {
                    std::fs::write(format!("/tmp/frame{:03}.yuv", frame_num), &nv12).unwrap();
                }

                if check_crcs {
                    let frame_crc = format!("{:08x}", crc32fast::hash(&nv12));
                    assert_eq!(frame_crc, expected_crc, "at frame {}", frame_num);
                }
            }
        })
        .unwrap();

        assert_eq!(crcs.next(), None, "decoded less frames than expected");
    }
}
