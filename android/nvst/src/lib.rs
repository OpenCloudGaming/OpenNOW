//! Android owns presentation and the allocated CloudMatch session. This adapter owns only an attachment.
mod microphone;
mod nvst_rtsp;

use jni::{
    JNIEnv,
    objects::{JByteArray, JObject, JShortArray, JString, JValue},
    sys::{jboolean, jlong},
};
use opennow_streamer_protocol::SessionContext;
use opennow_streamer_transport::*;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicI64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Default)]
struct Attachment {
    stopped: AtomicBool,
    microphone: Mutex<Option<microphone::MicrophoneEncoder>>,
    microphone_permit: Mutex<Arc<AtomicBool>>,
    control: Mutex<Option<NvstUdpReceiverControl>>,
    feedback: Mutex<Option<SharedNvstFeedback>>,
}
static NEXT: AtomicI64 = AtomicI64::new(1);
static ATTACHMENTS: OnceLock<Mutex<HashMap<i64, Arc<Attachment>>>> = OnceLock::new();
fn attachments() -> &'static Mutex<HashMap<i64, Arc<Attachment>>> {
    ATTACHMENTS.get_or_init(Mutex::default)
}
fn attachment(id: i64) -> Option<Arc<Attachment>> {
    attachments().lock().ok()?.get(&id).cloned()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_opencloudgaming_opennow_NvstBridge_create(
    _: JNIEnv,
    _: JObject,
) -> jlong {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    attachments()
        .lock()
        .unwrap()
        .insert(id, Arc::new(Attachment::default()));
    id
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_opencloudgaming_opennow_NvstBridge_stop(
    _: JNIEnv,
    _: JObject,
    id: jlong,
) {
    if let Some(state) = attachment(id) {
        state.stopped.store(true, Ordering::Release);
        state
            .microphone_permit
            .lock()
            .unwrap()
            .store(false, Ordering::Release);
        if let Some(control) = state.control.lock().unwrap().as_ref() {
            let _ = control.stop();
        }
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_opencloudgaming_opennow_NvstBridge_input(
    env: JNIEnv,
    _: JObject,
    id: jlong,
    bytes: JByteArray,
    partial: jboolean,
) -> jboolean {
    let Some(state) = attachment(id) else {
        return 0;
    };
    if state.stopped.load(Ordering::Acquire) {
        return 0;
    }
    let Ok(bytes) = env.convert_byte_array(bytes) else {
        return 0;
    };
    let control = state.control.lock().unwrap().clone();
    control.is_some_and(|c| c.queue_input(bytes, partial != 0).is_ok()) as jboolean
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_opencloudgaming_opennow_NvstBridge_microphoneEnabled(
    _: JNIEnv,
    _: JObject,
    id: jlong,
    enabled: jboolean,
) {
    if let Some(state) = attachment(id) {
        let mut permit = state.microphone_permit.lock().unwrap();
        // Revoke queued frames permanently, even if unmute follows before dequeue.
        permit.store(false, Ordering::Release);
        *permit = Arc::new(AtomicBool::new(
            enabled != 0 && !state.stopped.load(Ordering::Acquire),
        ));
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_opencloudgaming_opennow_NvstBridge_microphone(
    env: JNIEnv,
    _: JObject,
    id: jlong,
    pcm: JShortArray,
    timestamp: jlong,
    restart: jboolean,
) -> jboolean {
    let Some(state) = attachment(id) else {
        return 0;
    };
    if state.stopped.load(Ordering::Acquire)
        || env.get_array_length(&pcm).ok() != Some(microphone::FRAME_SAMPLES as i32)
    {
        return 0;
    }
    let permit = state.microphone_permit.lock().unwrap().clone();
    if !permit.load(Ordering::Acquire) {
        return 0;
    }
    let mut samples = [0_i16; microphone::FRAME_SAMPLES];
    if env.get_short_array_region(&pcm, 0, &mut samples).is_err() {
        return 0;
    }
    let payload = {
        let Ok(mut encoder) = state.microphone.lock() else {
            return 0;
        };
        if encoder.is_none() {
            let Ok(created) = microphone::MicrophoneEncoder::new() else {
                return 0;
            };
            *encoder = Some(created);
        }
        let Ok(payload) =
            encoder
                .as_mut()
                .unwrap()
                .encode(&samples, timestamp as u32, restart != 0)
        else {
            return 0;
        };
        payload
    };
    let control = state.control.lock().unwrap().clone();
    control.is_some_and(|c| {
        !state.stopped.load(Ordering::Acquire)
            && c.send_microphone(payload, timestamp as u32, restart != 0, permit)
    }) as jboolean
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_opencloudgaming_opennow_NvstBridge_keyframe(
    _: JNIEnv,
    _: JObject,
    id: jlong,
) {
    if let Some(state) = attachment(id) {
        if let Some(feedback) = state.feedback.lock().unwrap().as_ref() {
            feedback.request_keyframe();
        }
    }
}
fn event(
    env: &mut JNIEnv,
    callback: &JObject,
    name: &str,
    detail: &str,
) -> jni::errors::Result<()> {
    env.with_local_frame(8, |env| {
        let name = env.new_string(name)?;
        let detail = env.new_string(detail)?;
        env.call_method(
            callback,
            "onNativeEvent",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            &[JValue::Object(&name), JValue::Object(&detail)],
        )?;
        Ok(())
    })
}
struct Receivers {
    bundle: Option<NvstUdpReceiverSession>,
    video: Option<NvstUdpReceiverSession>,
}
impl Drop for Receivers {
    fn drop(&mut self) {
        if let Some(video) = self.video.take() {
            video.stop();
        }
        if let Some(bundle) = self.bundle.take() {
            bundle.stop();
        }
    }
}
fn run(
    env: &mut JNIEnv,
    callback: &JObject,
    context: &str,
    state: &Attachment,
) -> Result<(), String> {
    let mut context: SessionContext = serde_json::from_str(context).map_err(|e| e.to_string())?;
    if state.stopped.load(Ordering::Acquire) {
        return Ok(());
    }
    let mut bundle = ReservedNvstBundle::reserve().map_err(|e| e.to_string())?;
    let mut prepared = nvst_rtsp::prepare_owned_nvst(&context, &mut bundle)
        .map_err(|e| format!("{}: {}", e.code, e.message))?;
    if state.stopped.load(Ordering::Acquire) {
        return Ok(());
    }
    context.nvst_video = Some(prepared.handoff.clone());
    let config =
        parse_nvst_video_handoff(&serde_json::to_value(context).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
            .ok_or("Missing NVST handoff")?;
    let feedback = config.feedback();
    *state.feedback.lock().unwrap() = Some(feedback.clone());
    let (socket, rtc, video_socket) = bundle.into_parts();
    let (sender, media) = mpsc::sync_channel(8);
    let (events_sender, events) = mpsc::channel();
    let transport = spawn_nvst_udp_receiver_with_socket(
        config.clone(),
        sender.clone(),
        events_sender.clone(),
        Some(socket),
        Some(rtc),
    )
    .map_err(|e| e.to_string())?;
    *state.control.lock().unwrap() = Some(transport.control());
    let mut receivers = Receivers {
        bundle: Some(transport),
        video: None,
    };
    receivers.video = Some(
        spawn_nvst_mjolnir_receiver(video_socket, config, sender, events_sender)
            .map_err(|e| e.to_string())?,
    );
    if state.stopped.load(Ordering::Acquire) {
        return Ok(());
    }
    prepared.announce().map_err(|e| e.message)?;
    let _rtsp = prepared.finish().map_err(|e| e.message)?;
    event(env, callback, "connected", "NVST encrypted UDP").map_err(|e| e.to_string())?;
    let mut last_stats = Instant::now();
    let mut round_trip_ms = None;
    while !state.stopped.load(Ordering::Acquire) {
        for notification in events.try_iter().take(128) {
            match notification {
                NvstReceiveEvent::RoundTripTime(rtt) => {
                    round_trip_ms = rtt.map(|value| value.as_secs_f64() * 1000.0);
                }
                NvstReceiveEvent::ServerInput(bytes) => {
                    env.with_local_frame(4, |env| -> jni::errors::Result<()> {
                        let data = env.byte_array_from_slice(&bytes)?;
                        env.call_method(
                            callback,
                            "onNativeInput",
                            "([B)V",
                            &[JValue::Object(&data)],
                        )?;
                        Ok(())
                    })
                    .map_err(|e| e.to_string())?;
                }
                NvstReceiveEvent::InputReady(version) => {
                    event(env, callback, "input-ready", &version.to_string())
                        .map_err(|e| e.to_string())?;
                }
                NvstReceiveEvent::InputUnavailable(_) => {
                    event(env, callback, "input-unavailable", "").map_err(|e| e.to_string())?;
                }
                NvstReceiveEvent::RecoveryNeeded(nvst::NvstRecovery::Timeout { .. }) => {
                    return Err("NVST media timed out; cloud session retained".into());
                }
                NvstReceiveEvent::RecoveryNeeded(_) => feedback.request_keyframe(),
                _ => {}
            }
        }
        match media.recv_timeout(Duration::from_millis(10)) {
            Ok(frame) => {
                let accepted = env
                    .with_local_frame(8, |env| -> jni::errors::Result<bool> {
                        let codec = env.new_string(&frame.codec)?;
                        // The Arc stays alive through this synchronous call. Android copies into a
                        // retained decoder buffer before returning; it never stores this borrowed view.
                        let bytes = unsafe {
                            env.new_direct_byte_buffer(
                                frame.payload.as_ptr() as *mut u8,
                                frame.payload.len(),
                            )?
                        };
                        env.call_method(
                            callback,
                            "onNativeMedia",
                            "(Ljava/lang/String;Ljava/nio/ByteBuffer;JZZ)Z",
                            &[
                                JValue::Object(&codec),
                                JValue::Object(&bytes),
                                JValue::Long(
                                    (frame.rtp_timestamp.saturating_mul(1_000_000_000)
                                        / u64::from(frame.clock_rate_hz.max(1)))
                                        as i64,
                                ),
                                JValue::Bool(frame.keyframe as u8),
                                JValue::Bool(frame.contiguous as u8),
                            ],
                        )?
                        .z()
                    })
                    .map_err(|e| e.to_string())?;
                if let Some(index) = frame.frame_index {
                    if accepted {
                        feedback.publish_accepted_frame(
                            index,
                            frame.payload.len() as u32,
                            Instant::now(),
                        );
                    } else {
                        feedback.request_keyframe();
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("NVST media receiver closed".into());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if last_stats.elapsed() >= Duration::from_secs(1) {
            let metrics = feedback
                .network_metrics()
                .map(|(jitter, loss)| format!("{jitter},{loss}"))
                .unwrap_or_else(|| ",".into());
            let detail = format!(
                "{metrics},{}",
                round_trip_ms.map(|rtt| rtt.to_string()).unwrap_or_default()
            );
            event(env, callback, "network", &detail).map_err(|e| e.to_string())?;
            last_stats = Instant::now();
        }
    }
    Ok(())
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_opencloudgaming_opennow_NvstBridge_run(
    mut env: JNIEnv,
    _: JObject,
    id: jlong,
    context: JString,
    callback: JObject,
) {
    let Some(state) = attachment(id) else {
        return;
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let context: String = env.get_string(&context).map_err(|e| e.to_string())?.into();
        run(&mut env, &callback, &context, &state)
    }))
    .unwrap_or_else(|_| Err("NVST transport panic; cloud session retained".into()));
    // Do not replace a Java exception with another JNI call.
    if !state.stopped.load(Ordering::Acquire) && !env.exception_check().unwrap_or(true) {
        if let Err(message) = result {
            let _ = event(&mut env, &callback, "error", &message);
        }
    }
    attachments().lock().unwrap().remove(&id);
}
