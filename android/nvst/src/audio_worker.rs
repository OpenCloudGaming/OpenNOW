//! One bounded audio lane per attachment, joined before Java releases its codec/track.
use opennow_streamer_transport::{EncodedMediaFrame, MediaConsumer};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub struct AudioWorkLoop {
    frames: Receiver<EncodedMediaFrame>,
    stopped: Arc<AtomicBool>,
}

impl AudioWorkLoop {
    pub fn run(
        self,
        mut consume: impl FnMut(Option<EncodedMediaFrame>) -> Result<(), String>,
    ) -> Result<(), String> {
        while !self.stopped.load(Ordering::Acquire) {
            let frame = match self.frames.recv_timeout(Duration::from_millis(5)) {
                Ok(frame) => Some(frame),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            if self.stopped.load(Ordering::Acquire) {
                break;
            }
            // Idle ticks drain decoded PCM even if no further network packet arrives.
            consume(frame)?;
        }
        Ok(())
    }
}

pub struct AudioWorker {
    consumer: MediaConsumer,
    stopped: Arc<AtomicBool>,
    errors: Receiver<String>,
    join: Option<JoinHandle<()>>,
}

impl AudioWorker {
    pub fn start(
        run: impl FnOnce(AudioWorkLoop) -> Result<(), String> + Send + 'static,
    ) -> Result<Self, String> {
        let (consumer, frames) = mpsc::sync_channel(8);
        let (error_sender, errors) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let work = AudioWorkLoop {
            frames,
            stopped: stopped.clone(),
        };
        let join = thread::Builder::new()
            .name("OpenNOW-NVST-audio".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(work)))
                    .unwrap_or_else(|_| Err("NVST audio worker panicked".into()));
                if let Err(error) = result {
                    let _ = error_sender.send(error);
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            consumer,
            stopped,
            errors,
            join: Some(join),
        })
    }

    pub fn consumer(&self) -> MediaConsumer {
        self.consumer.clone()
    }

    pub fn check(&self) -> Result<(), String> {
        match self.errors.try_recv() {
            Ok(error) => Err(error),
            Err(mpsc::TryRecvError::Empty) => Ok(()),
            Err(mpsc::TryRecvError::Disconnected) => Err("NVST audio worker stopped".into()),
        }
    }
}

impl Drop for AudioWorker {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opennow_streamer_transport::NvstMediaConsumers;

    fn packet() -> EncodedMediaFrame {
        EncodedMediaFrame {
            mid: "0".into(),
            codec: "opus".into(),
            payload: Arc::from([1]),
            frame_index: None,
            rtp_timestamp: 960,
            clock_rate_hz: 48000,
            channels: Some(2),
            received_at_us: 0,
            keyframe: false,
            contiguous: true,
        }
    }

    #[test]
    fn audio_keeps_playing_while_video_queue_is_full() {
        let (played, playback) = mpsc::channel();
        let worker = AudioWorker::start(move |work| {
            work.run(|frame| {
                if let Some(frame) = frame {
                    played.send(frame.rtp_timestamp).unwrap();
                }
                Ok(())
            })
        })
        .unwrap();
        let (video, _stalled_decoder) = mpsc::sync_channel(1);
        let consumers = NvstMediaConsumers {
            video,
            audio: worker.consumer(),
        };
        consumers.video.try_send(packet()).unwrap();
        assert!(consumers.video.try_send(packet()).is_err());
        for timestamp in 0..16 {
            let mut frame = packet();
            frame.rtp_timestamp = timestamp;
            consumers.audio.try_send(frame).unwrap();
            assert_eq!(
                playback.recv_timeout(Duration::from_secs(1)).unwrap(),
                timestamp
            );
        }
        worker.check().unwrap();
    }

    #[test]
    fn idle_audio_still_drains_and_stop_joins_worker() {
        let (tick, ticks) = mpsc::channel();
        let worker = AudioWorker::start(move |work| {
            work.run(|frame| {
                tick.send(frame.is_none()).unwrap();
                Ok(())
            })
        })
        .unwrap();
        assert!(ticks.recv_timeout(Duration::from_secs(1)).unwrap());
        drop(worker);
        while ticks.try_recv().is_ok() {}
        assert_eq!(ticks.recv(), Err(mpsc::RecvError));
    }

    #[test]
    fn audio_backlog_is_bounded_and_errors_are_reported() {
        let (release, gate) = mpsc::channel();
        let worker = AudioWorker::start(move |_| {
            gate.recv().unwrap();
            Err("audio decoder failed".into())
        })
        .unwrap();
        for _ in 0..8 {
            worker.consumer.try_send(packet()).unwrap();
        }
        assert!(matches!(
            worker.consumer.try_send(packet()),
            Err(mpsc::TrySendError::Full(_))
        ));
        release.send(()).unwrap();
        assert_eq!(
            worker.errors.recv_timeout(Duration::from_secs(1)).unwrap(),
            "audio decoder failed"
        );
    }
}
