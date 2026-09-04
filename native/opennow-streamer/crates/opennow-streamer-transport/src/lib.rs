use std::sync::Arc;
use std::sync::Once;
use std::sync::mpsc::{SyncSender, TrySendError};

use str0m::crypto::from_feature_flags;
use thiserror::Error;

pub mod nvst;
mod nvst_control;
mod nvst_input;

pub use nvst::{
    BoundedFrameQueue, EncodedVideoAccessUnit, NvstBundleIdentity, NvstConfigError, NvstDropReason,
    NvstReceiveEvent, NvstReceiverState, NvstRecovery, NvstSrtpProfile, NvstUdpReceiverControl,
    NvstUdpReceiverError, NvstUdpReceiverSession, NvstUnsupportedFeature, NvstVideoCodec,
    NvstVideoConfig, NvstVideoReceiver, ReservedNvstBundle, SharedNvstFeedback,
    advertised_nvst_ipv4, parse_nvst_video_handoff, reserve_nvst_mjolnir_udp_socket,
    reserve_nvst_udp_socket, spawn_nvst_mjolnir_receiver, spawn_nvst_udp_receiver,
    spawn_nvst_udp_receiver_with_socket,
};

static INSTALL_CRYPTO: Once = Once::new();

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("input channel is not ready")]
    InputNotReady,
    #[error("encoded media consumer is no longer running")]
    MediaConsumerClosed,
    #[error("encoded media consumer is backpressured")]
    MediaConsumerBackpressured,
    #[error("transport worker is no longer running")]
    Closed,
}

impl TransportError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InputNotReady => "input-not-ready",
            Self::MediaConsumerClosed => "media-consumer-closed",
            Self::MediaConsumerBackpressured => "media-consumer-backpressured",
            Self::Closed => "transport-closed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct EncodedMediaFrame {
    pub mid: String,
    pub codec: String,
    pub payload: Arc<[u8]>,
    /// Sender-authored video frame index from the NVST GS packet header.
    /// Audio and transports without an equivalent identifier leave this unset.
    pub frame_index: Option<u32>,
    pub rtp_timestamp: u64,
    pub clock_rate_hz: u32,
    pub channels: Option<u8>,
    pub received_at_us: u64,
    pub keyframe: bool,
    pub contiguous: bool,
}

pub type MediaConsumer = SyncSender<EncodedMediaFrame>;

pub fn install_crypto() {
    INSTALL_CRYPTO.call_once(|| from_feature_flags().install_process_default());
}

fn deliver_media_frame(
    consumer: &MediaConsumer,
    frame: EncodedMediaFrame,
) -> Result<(), TransportError> {
    consumer.try_send(frame).map_err(|error| match error {
        TrySendError::Full(_) => TransportError::MediaConsumerBackpressured,
        TrySendError::Disconnected(_) => TransportError::MediaConsumerClosed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> EncodedMediaFrame {
        EncodedMediaFrame {
            mid: "video-0".to_owned(),
            codec: "H264".to_owned(),
            payload: Arc::from([1_u8, 2, 3, 4]),
            frame_index: Some(42),
            rtp_timestamp: 180_000,
            clock_rate_hz: 90_000,
            channels: None,
            received_at_us: 2_500,
            keyframe: true,
            contiguous: true,
        }
    }

    #[test]
    fn media_delivery_preserves_shared_payload() {
        let (consumer, receiver) = std::sync::mpsc::sync_channel(1);
        let expected = frame();
        let payload = expected.payload.clone();
        deliver_media_frame(&consumer, expected).expect("frame delivery");
        let delivered = receiver.recv().expect("delivered frame");
        assert!(Arc::ptr_eq(&delivered.payload, &payload));
    }

    #[test]
    fn media_delivery_reports_bounded_queue_backpressure() {
        let (consumer, _receiver) = std::sync::mpsc::sync_channel(1);
        deliver_media_frame(&consumer, frame()).expect("first frame delivery");
        assert!(matches!(
            deliver_media_frame(&consumer, frame()),
            Err(TransportError::MediaConsumerBackpressured)
        ));
    }
}
