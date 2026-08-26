use std::ffi::c_void;
use std::mem;
use std::ptr::{self, NonNull};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::{self, JoinHandle};

use objc2_audio_toolbox::{
    AURenderCallbackStruct, AudioComponentDescription, AudioComponentFindNext,
    AudioComponentInstanceDispose, AudioComponentInstanceNew, AudioOutputUnitStart,
    AudioOutputUnitStop, AudioUnit, AudioUnitInitialize, AudioUnitRenderActionFlags,
    AudioUnitSetProperty, AudioUnitUninitialize, kAudioUnitManufacturer_Apple,
    kAudioUnitProperty_SetRenderCallback, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Input,
    kAudioUnitSubType_DefaultOutput, kAudioUnitType_Output,
};
use objc2_core_audio_types::{
    AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp, kAudioFormatFlagIsFloat,
    kAudioFormatFlagIsPacked, kAudioFormatLinearPCM,
};
use opus::{Channels, Decoder};

use crate::failure::{BackendSubsystem, FailureReporter};
use crate::format::AudioFormat;
use crate::queue::{BoundedQueue, PushResult};
use crate::ring::PcmRing;

use super::{BackendError, Counters};

const MAX_OPUS_FRAME_SAMPLES_PER_CHANNEL: usize = 5_760;

pub(super) struct AudioPipeline {
    packets: Arc<BoundedQueue<Vec<u8>>>,
    ring: Arc<PcmRing>,
    output: Option<AudioOutput>,
    worker: Option<JoinHandle<()>>,
}

impl AudioPipeline {
    pub(super) fn start(
        format: AudioFormat,
        packet_capacity: usize,
        pcm_milliseconds: u32,
        counters: Arc<Counters>,
        failures: Arc<FailureReporter>,
    ) -> Result<Self, BackendError> {
        format.validate()?;
        let channels = usize::from(format.channels);
        let pcm_capacity = usize::try_from(format.sample_rate)
            .ok()
            .and_then(|rate| rate.checked_mul(channels))
            .and_then(|samples| samples.checked_mul(pcm_milliseconds as usize))
            .map(|samples| samples / 1_000)
            .filter(|samples| *samples > 0)
            .ok_or(crate::format::FormatError::QueueTooLarge)?;
        let ring = Arc::new(PcmRing::new(pcm_capacity));
        let packets: Arc<BoundedQueue<Vec<u8>>> = Arc::new(BoundedQueue::new(packet_capacity));
        let decoder_channels = if format.channels == 1 {
            Channels::Mono
        } else {
            Channels::Stereo
        };
        let mut decoder = Decoder::new(format.sample_rate, decoder_channels)
            .map_err(|error| BackendError::Opus(error.to_string()))?;
        let worker_packets = Arc::clone(&packets);
        let worker_ring = Arc::clone(&ring);
        let worker_counters = Arc::clone(&counters);
        let worker_failures = Arc::clone(&failures);
        let worker = thread::Builder::new()
            .name("opennow-opus-decode".into())
            .spawn(move || {
                let run_failures = Arc::clone(&worker_failures);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                    let mut pcm = vec![0.0; MAX_OPUS_FRAME_SAMPLES_PER_CHANNEL * channels];
                    while let Some(packet) = worker_packets.pop_wait() {
                        match decoder.decode_float(&packet, &mut pcm, false) {
                            Ok(samples_per_channel) => {
                                run_failures.audio_succeeded();
                                let sample_count = samples_per_channel * channels;
                                let written = worker_ring.push(&pcm[..sample_count]);
                                if written != sample_count {
                                    worker_counters.pcm_samples_dropped.fetch_add(
                                        (sample_count - written) as u64,
                                        Ordering::Relaxed,
                                    );
                                }
                            }
                            Err(error) => {
                                worker_counters
                                    .opus_decode_errors
                                    .fetch_add(1, Ordering::Relaxed);
                                if run_failures.audio_failed(error.to_string()) {
                                    break;
                                }
                            }
                        }
                    }
                }));
                if result.is_err() {
                    worker_failures.report_fatal(
                        BackendSubsystem::AudioWorker,
                        "the audio decode worker stopped unexpectedly".to_owned(),
                    );
                }
            })
            .map_err(|_| BackendError::Thread("Opus decoder"))?;

        let output = match AudioOutput::start(format, Arc::clone(&ring), counters) {
            Ok(output) => output,
            Err(error) => {
                packets.close();
                let _ = worker.join();
                return Err(error);
            }
        };
        Ok(Self {
            packets,
            ring,
            output: Some(output),
            worker: Some(worker),
        })
    }

    pub(super) fn submit(&self, packet: Vec<u8>) -> PushResult<Vec<u8>> {
        self.packets.push_drop_oldest(packet)
    }

    pub(super) fn set_paused(&mut self, paused: bool) -> Result<(), BackendError> {
        if paused && let Some(output) = self.output.as_mut() {
            output.set_paused(true)?;
        }
        self.packets.clear();
        self.ring.clear();
        if !paused && let Some(output) = self.output.as_mut() {
            output.set_paused(false)?;
        }
        Ok(())
    }

    pub(super) fn stop(mut self) {
        drop(self.output.take());
        self.packets.close_and_discard();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for AudioPipeline {
    fn drop(&mut self) {
        drop(self.output.take());
        self.packets.close_and_discard();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct AudioCallbackContext {
    ring: Arc<PcmRing>,
    counters: Arc<Counters>,
    channels: usize,
}

struct AudioOutput {
    unit: AudioUnit,
    initialized: bool,
    started: bool,
    callback_context: Box<AudioCallbackContext>,
}

// Audio Unit control calls have no thread affinity and AudioOutput is always owned behind Shared's
// audio Mutex. CoreAudio's concurrent render callback touches only the stable callback context.
unsafe impl Send for AudioOutput {}

impl AudioOutput {
    fn start(
        format: AudioFormat,
        ring: Arc<PcmRing>,
        counters: Arc<Counters>,
    ) -> Result<Self, BackendError> {
        let mut description = AudioComponentDescription {
            componentType: kAudioUnitType_Output,
            componentSubType: kAudioUnitSubType_DefaultOutput,
            componentManufacturer: kAudioUnitManufacturer_Apple,
            componentFlags: 0,
            componentFlagsMask: 0,
        };
        let component =
            unsafe { AudioComponentFindNext(ptr::null_mut(), NonNull::from(&mut description)) };
        if component.is_null() {
            return Err(BackendError::AppleApi {
                api: "AudioComponentFindNext",
                status: -1,
            });
        }
        let mut unit = ptr::null_mut();
        let status = unsafe { AudioComponentInstanceNew(component, NonNull::from(&mut unit)) };
        check_status("AudioComponentInstanceNew", status)?;
        if unit.is_null() {
            return Err(BackendError::AppleApi {
                api: "AudioComponentInstanceNew",
                status: -1,
            });
        }

        let mut output = Self {
            unit,
            initialized: false,
            started: false,
            callback_context: Box::new(AudioCallbackContext {
                ring,
                counters,
                channels: usize::from(format.channels),
            }),
        };
        let bytes_per_frame = u32::from(format.channels) * mem::size_of::<f32>() as u32;
        let stream_format = AudioStreamBasicDescription {
            mSampleRate: f64::from(format.sample_rate),
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
            mBytesPerPacket: bytes_per_frame,
            mFramesPerPacket: 1,
            mBytesPerFrame: bytes_per_frame,
            mChannelsPerFrame: u32::from(format.channels),
            mBitsPerChannel: 32,
            mReserved: 0,
        };
        let status = unsafe {
            AudioUnitSetProperty(
                output.unit,
                kAudioUnitProperty_StreamFormat,
                kAudioUnitScope_Input,
                0,
                (&stream_format as *const AudioStreamBasicDescription).cast(),
                mem::size_of_val(&stream_format) as u32,
            )
        };
        check_status("AudioUnitSetProperty(StreamFormat)", status)?;

        let callback = AURenderCallbackStruct {
            inputProc: Some(render_callback),
            inputProcRefCon: (&mut *output.callback_context as *mut AudioCallbackContext).cast(),
        };
        let status = unsafe {
            AudioUnitSetProperty(
                output.unit,
                kAudioUnitProperty_SetRenderCallback,
                kAudioUnitScope_Input,
                0,
                (&callback as *const AURenderCallbackStruct).cast(),
                mem::size_of_val(&callback) as u32,
            )
        };
        check_status("AudioUnitSetProperty(SetRenderCallback)", status)?;

        check_status("AudioUnitInitialize", unsafe {
            AudioUnitInitialize(output.unit)
        })?;
        output.initialized = true;
        check_status("AudioOutputUnitStart", unsafe {
            AudioOutputUnitStart(output.unit)
        })?;
        output.started = true;
        Ok(output)
    }

    fn set_paused(&mut self, paused: bool) -> Result<(), BackendError> {
        if paused && self.started {
            check_status("AudioOutputUnitStop", unsafe {
                AudioOutputUnitStop(self.unit)
            })?;
            self.started = false;
        } else if !paused && !self.started {
            check_status("AudioOutputUnitStart", unsafe {
                AudioOutputUnitStart(self.unit)
            })?;
            self.started = true;
        }
        Ok(())
    }
}

impl Drop for AudioOutput {
    fn drop(&mut self) {
        if self.started {
            let _ = unsafe { AudioOutputUnitStop(self.unit) };
            self.started = false;
        }
        if self.initialized {
            let _ = unsafe { AudioUnitUninitialize(self.unit) };
            self.initialized = false;
        }
        if !self.unit.is_null() {
            let _ = unsafe { AudioComponentInstanceDispose(self.unit) };
            self.unit = ptr::null_mut();
        }
    }
}

unsafe extern "C-unwind" fn render_callback(
    context: NonNull<c_void>,
    mut action_flags: NonNull<AudioUnitRenderActionFlags>,
    _timestamp: NonNull<AudioTimeStamp>,
    _bus_number: u32,
    frame_count: u32,
    buffers: *mut AudioBufferList,
) -> i32 {
    let context = unsafe { context.cast::<AudioCallbackContext>().as_ref() };
    let Some(buffers) = NonNull::new(buffers) else {
        return -50;
    };
    let buffers = unsafe { buffers.as_ref() };
    if buffers.mNumberBuffers == 0 {
        return -50;
    }
    let buffer = &buffers.mBuffers[0];
    let requested = frame_count as usize * context.channels;
    let available_samples = buffer.mDataByteSize as usize / mem::size_of::<f32>();
    let sample_count = requested.min(available_samples);
    let Some(data) = NonNull::new(buffer.mData.cast::<f32>()) else {
        return -50;
    };
    let output = unsafe { std::slice::from_raw_parts_mut(data.as_ptr(), sample_count) };
    let read = context.ring.pop_into(output);
    output[read..].fill(0.0);
    if read < requested {
        context.counters.pcm_underrun_frames.fetch_add(
            ((requested - read) / context.channels) as u64,
            Ordering::Relaxed,
        );
    }
    if read == 0 {
        unsafe {
            action_flags
                .as_mut()
                .insert(AudioUnitRenderActionFlags::UnitRenderAction_OutputIsSilence)
        };
    }
    0
}

fn check_status(api: &'static str, status: i32) -> Result<(), BackendError> {
    if status == 0 {
        Ok(())
    } else {
        Err(BackendError::AppleApi { api, status })
    }
}
