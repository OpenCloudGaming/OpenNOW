use super::*;
use opennow_media_protocol::lease::HostBoundPreparedLease;
use opennow_plugin_api::BUILTIN_GFN_ID;

fn command(kind: &str, context: Value) -> Command {
    serde_json::from_value(json!({
        "id":"contract-command","type":kind,"protocolVersion":PROTOCOL_VERSION,"context":context
    }))
    .unwrap()
}

fn engine() -> Engine {
    let (events, _) = std::sync::mpsc::channel();
    Engine::new(events)
}

fn capabilities() -> Value {
    json!({
        "supportsVideoDecode":true,"supportsAudioDecode":true,
        "videoBackends":[
            {"backend":"ffmpeg","available":true,"codecs":[
                {"codec":"h264","available":true,"colorQualities":["8bit_420","8bit_444","10bit_420"]},
                {"codec":"hevc","available":true,"colorQualities":["8bit_420","10bit_420","10bit_444"]},
                {"codec":"av1","available":false,"colorQualities":["8bit_420"]}]},
            {"backend":"cuda","available":true,"codecs":[
                {"codec":"av1","available":true,"colorQualities":["8bit_420","8bit_444","10bit_420"]},
                {"codec":"hevc","available":true,"colorQualities":["10bit_420"]},
                {"codec":"unknown-codec","available":true,"colorQualities":["8bit_420"]}]},
            {"backend":"vulkan","available":true,"codecs":[
                {"codec":"h264","available":true,"colorQualities":["8bit_420","8bit_444","10bit_420"]},
                {"codec":"hevc","available":true,"colorQualities":["8bit_420","10bit_420","10bit_444"]}]},
            {"backend":"unavailable","available":false,"codecs":[
                {"codec":"hevc","available":true,"colorQualities":["8bit_444"]}]}
        ]
    })
}

fn offer(epoch: u64) -> NativeOffer {
    create_offer(
        &capabilities(),
        &LocalMediaPolicy::default(),
        epoch,
        now_ms(),
    )
    .unwrap()
}

fn retain(engine: &mut Engine, offer: NativeOffer) {
    engine.native_offers.insert(
        offer.offer_id.as_str().into(),
        RetainedOffer {
            offer,
            policy: LocalMediaPolicy::default(),
        },
    );
}

fn gfn_context() -> Value {
    json!({"session":{"sessionId":"owned-gfn-session","serverIp":"127.0.0.1","iceServers":[]},
        "settings":{"codec":"H264","fps":60},"shortcuts":{}})
}

fn lease(offer: &NativeOffer) -> HostBoundPreparedLease {
    serde_json::from_value(json!({
        "version":1,"leaseId":"host-lease","offerId":offer.offer_id,"runtimeEpoch":offer.runtime_epoch,
        "sourceId":BUILTIN_GFN_ID,"session":{"account":{"authority":"gfn-authority","account":"gfn-account"},
            "remoteId":"owned-gfn-session"},"attemptId":"host-attempt","expiresAtMs":offer.expires_at_ms,
        "media":{"kind":"gfn","context":gfn_context()}
    })).unwrap()
}

fn status(engine: &Engine) -> Value {
    engine
        .media_status(command("media-status", Value::Null))
        .unwrap()
        .remove(0)
}

fn occupy_with_retained_worker_lease(engine: &mut Engine) -> Value {
    let mut binding = lease(&offer(engine.native_epoch)).public_binding();
    binding["sourceId"] = json!("org.opennow.test.provider");
    engine.active_lease = Some(ActiveLease {
        binding: binding.clone(),
        start_id: "original-start".into(),
        published_baseline: None,
    });
    binding
}

#[test]
fn worker_start_response_reports_the_offer_bound_replay_policy() {
    for enabled in [false, true] {
        let retained = RetainedOffer {
            offer: offer(71),
            policy: LocalMediaPolicy {
                replay_buffer_enabled: enabled,
                ..Default::default()
            },
        };
        let response = worker_start_response("worker-start", &retained.policy);
        assert_eq!(response["id"], "worker-start");
        assert_eq!(response["type"], "ok");
        assert_eq!(response["transport"], "provider-worker");
        assert_eq!(response["inputReady"], false);
        assert_eq!(response["replayEnabled"], enabled);
    }
}

#[test]
fn native_offer_filters_unavailable_backends_codecs_and_unimplemented_profiles() {
    let offer = offer(71);
    offer.validate().unwrap();
    assert_eq!(offer.video_formats.len(), 6);
    assert!(
        offer
            .video_formats
            .iter()
            .all(|format| format.dynamic_range == DynamicRange::Sdr)
    );
    assert!(
        !offer
            .video_formats
            .iter()
            .any(|format| format.encoding == VideoEncoding::H264AnnexB
                && (format.bit_depth != 8 || format.chroma != Chroma::Yuv420))
    );
    assert!(
        !offer
            .video_formats
            .iter()
            .any(|format| format.encoding == VideoEncoding::Av1Obu
                && format.chroma == Chroma::Yuv444)
    );
    assert!(
        !offer
            .video_formats
            .iter()
            .any(|format| format.encoding == VideoEncoding::HevcAnnexB
                && format.bit_depth == 8
                && format.chroma == Chroma::Yuv444)
    );
    assert_eq!(
        offer.audio_formats.as_ref(),
        &[AudioFormat {
            codec: AudioCodec::Opus,
            sample_rate: 48000,
            channels: 2
        }]
    );
}

#[test]
#[cfg(target_os = "linux")]
fn auto_cannot_offer_software_only_formats_but_explicit_software_can() {
    let mut caps = capabilities();
    caps["videoBackends"] = json!([caps["videoBackends"][0].clone()]);
    assert!(create_offer(&caps, &LocalMediaPolicy::default(), 71, 1000).is_err());
    for backend in ["software", "ffmpeg"] {
        let policy = LocalMediaPolicy {
            video_backend: backend.into(),
            ..Default::default()
        };
        let offer = create_offer(&caps, &policy, 71, 1000).unwrap();
        assert!(offer.video_formats.iter().any(|format| {
            format.encoding == VideoEncoding::H264AnnexB
                && format.bit_depth == 8
                && format.chroma == Chroma::Yuv420
        }));
    }
}

#[test]
fn native_offer_honors_explicit_backend_and_supported_aliases() {
    for (backend, encodings) in [
        (
            "ffmpeg",
            vec![VideoEncoding::H264AnnexB, VideoEncoding::HevcAnnexB],
        ),
        (
            "software",
            vec![VideoEncoding::H264AnnexB, VideoEncoding::HevcAnnexB],
        ),
        (
            "cuda",
            vec![VideoEncoding::Av1Obu, VideoEncoding::HevcAnnexB],
        ),
        (
            "nvdec",
            vec![VideoEncoding::Av1Obu, VideoEncoding::HevcAnnexB],
        ),
    ] {
        let policy = LocalMediaPolicy {
            video_backend: backend.into(),
            ..Default::default()
        };
        let offer = create_offer(&capabilities(), &policy, 71, 1000).unwrap();
        assert!(
            offer
                .video_formats
                .iter()
                .all(|format| encodings.contains(&format.encoding)),
            "{backend}"
        );
        for encoding in encodings {
            assert!(
                offer
                    .video_formats
                    .iter()
                    .any(|format| format.encoding == encoding),
                "{backend}"
            );
        }
        assert_eq!(offer.runtime_epoch, 71);
        assert_eq!(offer.expires_at_ms, 31000);
    }
    for backend in ["missing", "unavailable"] {
        let policy = LocalMediaPolicy {
            video_backend: backend.into(),
            ..Default::default()
        };
        assert!(create_offer(&capabilities(), &policy, 71, 1000).is_err());
    }
}

#[test]
fn absent_decoder_or_runtime_never_produces_an_offer_and_absent_audio_stays_absent() {
    let mut caps = capabilities();
    caps["supportsAudioDecode"] = json!(false);
    let offer = create_offer(&caps, &LocalMediaPolicy::default(), 71, 1000).unwrap();
    assert!(offer.audio_formats.is_empty());
    caps["supportsVideoDecode"] = json!(false);
    assert!(create_offer(&caps, &LocalMediaPolicy::default(), 71, 1000).is_err());
    assert!(create_offer(&capabilities(), &LocalMediaPolicy::default(), 0, 1000).is_err());
    let mut engine = engine();
    let failure = engine
        .media_offer(command(
            "media-offer",
            json!({"localPolicy":LocalMediaPolicy::default()}),
        ))
        .unwrap_err();
    assert_eq!(failure["code"], "media-offer-unavailable");
    assert!(engine.native_offers.is_empty());
    assert_eq!(status(&engine)["nativeIdle"], true);
}

#[test]
fn offer_requires_current_protocol_and_strict_host_policy() {
    let mut engine = engine();
    let mut old = command(
        "media-offer",
        json!({"localPolicy":LocalMediaPolicy::default()}),
    );
    old.protocol_version = Some(PROTOCOL_VERSION - 1);
    assert_eq!(
        engine.media_offer(old).unwrap_err()["code"],
        "media-offer-unavailable"
    );
    for policy in [
        json!({"maxBitrateMbps":0}),
        json!({"videoBackend":"../provider-backend"}),
        json!({"shortcuts":{"providerAction":"Ctrl+P"}}),
        json!({"providerPath":"/not-host-owned"}),
    ] {
        assert_eq!(
            engine
                .media_offer(command("media-offer", json!({"localPolicy":policy})))
                .unwrap_err()["code"],
            "invalid-media-policy"
        );
    }
    assert!(engine.native_offers.is_empty());
}

#[test]
fn capability_only_offer_cache_is_bounded_prunes_expiry_and_cancel_never_changes_occupancy() {
    let mut engine = engine();
    let active = occupy_with_retained_worker_lease(&mut engine);
    {
        let mut lifecycle = lock_lifecycle(&engine.lifecycle);
        lifecycle.state = State::Connected;
        lifecycle.generation = 29;
    }
    let before = status(&engine);
    let mut ids = Vec::new();
    for now in 1000..1004 {
        let policy = LocalMediaPolicy {
            max_bitrate_mbps: 42.5,
            ..Default::default()
        };
        let offer = create_offer(&capabilities(), &policy, engine.native_epoch, now).unwrap();
        let id = offer.offer_id.as_str().to_owned();
        engine.cache_native_offer(offer, policy, now).unwrap();
        assert!(!ids.contains(&id));
        assert_eq!(engine.native_offers[&id].policy.max_bitrate_mbps, 42.5);
        ids.push(id);
        assert_eq!(status(&engine), before);
        assert_eq!(lock_lifecycle(&engine.lifecycle).generation, 29);
    }
    let policy = LocalMediaPolicy::default();
    let overflow = create_offer(&capabilities(), &policy, engine.native_epoch, 1004).unwrap();
    assert!(
        engine
            .cache_native_offer(overflow, policy.clone(), 1004)
            .is_err()
    );
    assert_eq!(engine.native_offers.len(), 4);
    let replacement = create_offer(&capabilities(), &policy, engine.native_epoch, 31000).unwrap();
    engine
        .cache_native_offer(replacement, policy, 31000)
        .unwrap();
    assert_eq!(engine.native_offers.len(), 4);
    assert!(!engine.native_offers.contains_key(&ids[0]));
    for _ in 0..2 {
        let mut cancel = command("media-cancel-offer", Value::Null);
        cancel.offer_id = Some(ids[1].clone());
        engine.cancel_media_offer(cancel).unwrap();
        assert_eq!(engine.native_offers.len(), 3);
        assert_eq!(status(&engine), before);
        assert_eq!(engine.active_lease.as_ref().unwrap().binding, active);
    }
}

#[test]
fn cache_rejects_duplicate_expired_foreign_runtime_and_invalid_policy_offers() {
    let mut engine = engine();
    let policy = LocalMediaPolicy::default();
    let original = create_offer(&capabilities(), &policy, engine.native_epoch, 1000).unwrap();
    let id = original.offer_id.as_str().to_owned();
    engine
        .cache_native_offer(original.clone(), policy.clone(), 1000)
        .unwrap();
    assert!(
        engine
            .cache_native_offer(original.clone(), policy.clone(), 1001)
            .is_err()
    );
    for invalid in ["expired", "runtime", "policy"] {
        let mut offer = create_offer(&capabilities(), &policy, engine.native_epoch, 1000).unwrap();
        let mut changed_policy = policy.clone();
        match invalid {
            "expired" => offer.expires_at_ms = 1000,
            "runtime" => offer.runtime_epoch += 1,
            "policy" => changed_policy.max_bitrate_mbps = 0.0,
            _ => unreachable!(),
        }
        assert!(
            engine
                .cache_native_offer(offer, changed_policy, 1000)
                .is_err(),
            "{invalid}"
        );
        assert_eq!(engine.native_offers.len(), 1);
        assert_eq!(engine.native_offers[&id].offer, original);
        assert_eq!(engine.native_offers[&id].policy, policy);
    }
}

#[test]
fn bare_external_spike_and_provider_worker_contexts_never_enter_owned_gfn_lane() {
    for context in [
        json!({"externalSpike":{"packageRoot":"/unapproved"}}),
        json!({"worker":{}}),
        json!({"packageRoot":"/unapproved"}),
        json!({"preparedWorker":{}}),
        json!({"settings":{"transportMode":"provider-worker"}}),
    ] {
        let mut engine = engine();
        let failure = engine.start(command("start", context)).unwrap_err();
        assert_eq!(failure["code"], "prepared-lease-required");
        assert_eq!(status(&engine)["nativeIdle"], true);
    }
}

#[test]
fn existing_owned_bare_gfn_lane_reaches_nvst_validation_without_becoming_a_worker_start() {
    let mut engine = engine();
    let failure = engine.start(command("start", gfn_context())).unwrap_err();
    assert_eq!(failure["code"], "nvst-handoff-required");
    assert_eq!(status(&engine)["nativeIdle"], true);
}

#[test]
fn prepared_start_rejects_expired_wrong_runtime_and_replayed_offers_before_transport() {
    for invalid in ["expired", "runtime", "offer"] {
        let mut engine = engine();
        let mut offer = offer(engine.native_epoch);
        if invalid == "expired" {
            offer.expires_at_ms = 1;
        }
        let mut lease = lease(&offer);
        if invalid == "runtime" {
            lease.runtime_epoch += 1;
        }
        if invalid == "offer" {
            lease.offer_id = OfferId::new("not-retained").unwrap();
        }
        retain(&mut engine, offer);
        let failure = engine
            .start(command("start", json!({"lease":lease})))
            .unwrap_err();
        assert_eq!(
            failure["code"],
            if invalid == "offer" {
                "native-offer-stale"
            } else {
                "invalid-prepared-lease"
            }
        );
        assert_eq!(status(&engine)["nativeIdle"], true);
        let replay = engine
            .start(command("start", json!({"lease":lease})))
            .unwrap_err();
        assert_eq!(replay["code"], "native-offer-stale");
    }
}

#[test]
fn prepared_admission_failure_consumes_offer_without_claiming_playback_or_occupancy() {
    let mut engine = engine();
    let offer = offer(engine.native_epoch);
    let lease = lease(&offer);
    retain(&mut engine, offer);
    let failure = engine
        .start(command("start", json!({"lease":lease})))
        .unwrap_err();
    assert_eq!(failure["code"], "nvst-handoff-required");
    assert!(engine.native_offers.is_empty());
    assert_eq!(status(&engine)["nativeIdle"], true);
    assert!(status(&engine)["active"].is_null());
}

#[test]
fn connected_legacy_gfn_cannot_be_overwritten_or_have_its_offer_consumed_by_a_new_lease() {
    let mut engine = engine();
    let offer = offer(engine.native_epoch);
    let lease = lease(&offer);
    retain(&mut engine, offer);
    {
        let mut lifecycle = lock_lifecycle(&engine.lifecycle);
        lifecycle.state = State::Connected;
        lifecycle.generation = 31;
        lifecycle.context = Some(serde_json::from_value(gfn_context()).unwrap());
    }
    let before = status(&engine);
    let failure = engine
        .start(command("start", json!({"lease":lease})))
        .unwrap_err();
    assert_eq!(failure["code"], "native-session-busy");
    assert_eq!(status(&engine), before);
    assert_eq!(engine.native_offers.len(), 1);
    assert_eq!(lock_lifecycle(&engine.lifecycle).generation, 31);
    assert_eq!(
        lock_lifecycle(&engine.lifecycle)
            .context
            .as_ref()
            .unwrap()
            .session
            .session_id,
        "owned-gfn-session"
    );
}

#[test]
fn idle_lifecycle_with_legacy_reserved_transport_is_not_idle_or_safe_to_overwrite() {
    let mut engine = engine();
    engine.nvst_bind(command("nvst-bind", Value::Null)).unwrap();
    assert_eq!(lock_lifecycle(&engine.lifecycle).state, State::Idle);
    let before = status(&engine);
    assert_eq!(before["nativeIdle"], false);
    assert_eq!(before["legacyActive"], true);
    assert!(before["active"].is_null());
    let offer = offer(engine.native_epoch);
    let lease = lease(&offer);
    retain(&mut engine, offer);
    let failure = engine
        .start(command("start", json!({"lease":lease})))
        .unwrap_err();
    assert_eq!(failure["code"], "native-session-busy");
    assert_eq!(status(&engine), before);
    assert_eq!(engine.native_offers.len(), 1);
    assert!(engine.reserved_nvst_bundle.is_some());
}

#[test]
fn retained_worker_lease_fences_bare_gfn_even_after_lifecycle_becomes_idle() {
    let mut engine = engine();
    let original = occupy_with_retained_worker_lease(&mut engine);
    assert_eq!(lock_lifecycle(&engine.lifecycle).state, State::Idle);
    let failure = engine.start(command("start", gfn_context())).unwrap_err();
    assert_eq!(failure["code"], "native-session-busy");
    assert_eq!(engine.active_lease.as_ref().unwrap().binding, original);
}

#[test]
fn retained_worker_lease_fences_new_prepared_lease_until_explicit_stop() {
    let mut engine = engine();
    let original = occupy_with_retained_worker_lease(&mut engine);
    let offer = offer(engine.native_epoch);
    let lease = lease(&offer);
    retain(&mut engine, offer);
    let failure = engine
        .start(command("start", json!({"lease":lease})))
        .unwrap_err();
    assert_eq!(failure["code"], "native-session-busy");
    assert_eq!(engine.active_lease.as_ref().unwrap().binding, original);
    assert_eq!(engine.native_offers.len(), 1);
    engine.stop("contract cleanup");
    assert_eq!(status(&engine)["nativeIdle"], true);
}

#[test]
fn status_distinguishes_empty_native_connected_legacy_and_retained_bound_lease() {
    let mut engine = engine();
    let empty = status(&engine);
    assert_eq!(empty["nativeIdle"], true);
    assert_eq!(empty["legacyActive"], false);
    assert!(empty["active"].is_null());
    assert_eq!(empty["runtimeEpoch"], engine.native_epoch);
    lock_lifecycle(&engine.lifecycle).state = State::Connected;
    let legacy = status(&engine);
    assert_eq!(legacy["nativeIdle"], false);
    assert_eq!(legacy["legacyActive"], true);
    assert!(legacy["active"].is_null());
    let binding = occupy_with_retained_worker_lease(&mut engine);
    assert_eq!(status(&engine)["active"]["state"], "negotiating");
    lock_lifecycle(&engine.lifecycle).state = State::Idle;
    let retained = status(&engine);
    assert_eq!(retained["nativeIdle"], false);
    assert_eq!(retained["legacyActive"], false);
    assert_eq!(retained["active"]["state"], "recovering");
    assert_eq!(retained["active"]["startId"], "original-start");
    for field in [
        "leaseId",
        "sourceId",
        "session",
        "attemptId",
        "runtimeEpoch",
    ] {
        assert_eq!(retained["active"][field], binding[field]);
    }
    engine.stop("contract cleanup");
    assert_eq!(status(&engine), empty);
}

#[test]
fn absent_audio_disables_native_audio_without_fabricating_mono_opus() {
    let offer = offer(71);
    let mut accepted: AcceptedMedia = serde_json::from_value(json!({
        "offerId":offer.offer_id,"runtimeEpoch":offer.runtime_epoch,"audio":null,"input":offer.input,
        "video":{"encoding":"h264-annex-b","bitDepth":8,"chroma":"yuv420","width":1280,"height":720,"fps":60,
            "color":{"range":"limited","primaries":"bt709","transfer":"bt709","matrix":"bt709","chromaLocation":"left"}}
    })).unwrap();
    accepted.validate_against(&offer, now_ms()).unwrap();
    assert!(!stream_config(&accepted, &LocalMediaPolicy::default()).audio_enabled);
    accepted.audio = Some(AudioFormat {
        codec: AudioCodec::Opus,
        sample_rate: 48000,
        channels: 1,
    });
    assert!(accepted.validate_against(&offer, now_ms()).is_err());
    accepted.audio.as_mut().unwrap().channels = 2;
    accepted.validate_against(&offer, now_ms()).unwrap();
    assert!(stream_config(&accepted, &LocalMediaPolicy::default()).audio_enabled);
}
