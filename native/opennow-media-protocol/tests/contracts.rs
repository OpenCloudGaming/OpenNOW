use opennow_media_protocol::lease::{HostBoundPreparedLease, PreparedMedia};
use opennow_media_protocol::wire::{
    AUDIO_TRACK_ID, ControlMessage, MEDIA_HEADER_BYTES, MediaHeader, VIDEO_TRACK_ID,
    WorkerBootstrap, encode_control, read_control,
};
use opennow_media_protocol::{FrameProvenance, MAX_CONTROL_BYTES, SourceStamp};
use opennow_plugin_api::BUILTIN_GFN_ID;
use opennow_plugin_api::media::{AcceptedMedia, NativeOffer};
use opennow_plugin_api::provider::SecretBytes;
use opennow_plugin_package::{InstalledManifest, current_target};
use serde_json::{Value, json};

fn offer() -> NativeOffer {
    serde_json::from_value(json!({
        "version":1,"offerId":"native-offer","runtimeEpoch":71,"expiresAtMs":30000,
        "videoFormats":[{"encoding":"h264-annex-b","bitDepth":8,"chroma":"yuv420",
            "dynamicRange":"sdr","maxWidth":1920,"maxHeight":1080,"maxFps":60}],
        "audioFormats":[{"codec":"opus","sampleRate":48000,"channels":2}],
        "input":{"keyboard":true,"relativeMouse":true,"absoluteMouse":false,
            "text":true,"gamepadSlots":4,"rumble":true},
        "limits":{"maxVideoAccessUnitBytes":4096,"maxAudioPacketBytes":1275,
            "maxBufferedVideoBytes":8192,"maxBufferedVideoFrames":2,"maxBufferedAudioMs":100,
            "maxControlMessageBytes":65536,"maxPendingInputEvents":32}
    }))
    .unwrap()
}

fn accepted(offer: &NativeOffer) -> AcceptedMedia {
    serde_json::from_value(json!({
        "offerId":offer.offer_id,"runtimeEpoch":offer.runtime_epoch,
        "video":{"encoding":"h264-annex-b","width":1280,"height":720,"fps":60,
            "bitDepth":8,"chroma":"yuv420","color":{"range":"limited","primaries":"bt709",
                "transfer":"bt709","matrix":"bt709","chromaLocation":"left"}},
        "audio":{"codec":"opus","sampleRate":48000,"channels":2},"input":offer.input
    }))
    .unwrap()
}

fn manifest() -> InstalledManifest {
    let manifest: InstalledManifest = serde_json::from_value(json!({
        "schemaVersion":2,"protocolVersion":2,"id":"org.opennow.test.provider",
        "name":"Contract provider","version":"1.0.0","publisher":"OpenNOW tests",
        "description":"Private media contract fixture","authKinds":["anonymous"],
        "capabilities":["auth.anonymous.v2","catalog.public.v2","catalog.details.v2",
            "launch.v2","sessions.v2","media.worker.v1"],
        "entrypoints":{current_target():{"control":"control","media":"media"}},
        "files":[{"path":"control","sha256":"00".repeat(32)},
            {"path":"media","sha256":"11".repeat(32)}]
    }))
    .unwrap();
    manifest.validate().unwrap();
    manifest
}

fn lease(offer: &NativeOffer) -> HostBoundPreparedLease {
    serde_json::from_value(json!({
        "version":1,"leaseId":"host-lease","offerId":offer.offer_id,
        "runtimeEpoch":offer.runtime_epoch,"sourceId":"org.opennow.test.provider",
        "session":{"account":{"authority":"test-authority","account":"test-account"},
            "remoteId":"remote-session"},"attemptId":"host-attempt","expiresAtMs":20000,
        "media":{"kind":"worker","package":{
            "versionRoot":std::env::temp_dir().join("private-version-root"),
            "dataRoot":std::env::temp_dir().join("private-data-root"),"expectedManifest":manifest()},
            "prepared":{"accepted":accepted(offer),"bootstrap":SecretBytes::new(b"provider-private-token".to_vec()).unwrap()}}
    })).unwrap()
}

fn gfn_lease(offer: &NativeOffer) -> HostBoundPreparedLease {
    let mut value = serde_json::to_value(lease(offer)).unwrap();
    value["sourceId"] = json!(BUILTIN_GFN_ID);
    value["media"] = json!({"kind":"gfn","context":{"session":{"sessionId":"remote-session"},
        "credential":"gfn-private-token"}});
    serde_json::from_value(value).unwrap()
}

#[test]
fn prepared_lease_requires_current_version_offer_runtime_and_admission_deadline() {
    let offer = offer();
    let valid = lease(&offer);
    valid.validate_against(&offer, 19999).unwrap();
    assert!(valid.validate_against(&offer, 20000).is_err());
    for (field, value) in [
        ("version", json!(2)),
        ("offerId", json!("another-offer")),
        ("runtimeEpoch", json!(72)),
        ("expiresAtMs", json!(0)),
        ("expiresAtMs", json!(30001)),
    ] {
        let mut changed = serde_json::to_value(&valid).unwrap();
        changed[field] = value;
        let changed: HostBoundPreparedLease = serde_json::from_value(changed).unwrap();
        assert!(changed.validate_against(&offer, 1000).is_err(), "{field}");
    }
}

#[test]
fn worker_lease_requires_matching_source_and_absolute_host_package_roots() {
    let offer = offer();
    let original = serde_json::to_value(lease(&offer)).unwrap();
    for pointer in [
        "/sourceId",
        "/media/package/versionRoot",
        "/media/package/dataRoot",
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(pointer).unwrap() = json!(if pointer == "/sourceId" {
            "org.opennow.other.provider"
        } else {
            "relative/provider-root"
        });
        let changed: HostBoundPreparedLease = serde_json::from_value(changed).unwrap();
        assert!(changed.validate_against(&offer, 1000).is_err(), "{pointer}");
    }
    let mut builtin = original;
    builtin["sourceId"] = json!(BUILTIN_GFN_ID);
    let builtin: HostBoundPreparedLease = serde_json::from_value(builtin).unwrap();
    assert!(builtin.validate_against(&offer, 1000).is_err());
}

#[test]
fn accepted_worker_media_cannot_switch_offer_runtime_or_expand_capabilities() {
    let offer = offer();
    let original = serde_json::to_value(lease(&offer)).unwrap();
    for (field, replacement) in [
        ("/offerId", json!("other-offer")),
        ("/runtimeEpoch", json!(72)),
        ("/video/width", json!(1921)),
        ("/audio/channels", json!(1)),
        ("/input/absoluteMouse", json!(true)),
    ] {
        let mut changed = original.clone();
        *changed["media"]["prepared"]["accepted"]
            .pointer_mut(field)
            .unwrap() = replacement;
        let changed: HostBoundPreparedLease = serde_json::from_value(changed).unwrap();
        assert!(changed.validate_against(&offer, 1000).is_err(), "{field}");
    }
}

#[test]
fn gfn_private_context_must_match_the_builtin_source_and_remote_session() {
    let offer = offer();
    let valid = gfn_lease(&offer);
    valid.validate_against(&offer, 1000).unwrap();
    for (field, replacement) in [
        ("/sourceId", json!("org.opennow.test.provider")),
        (
            "/media/context/session/sessionId",
            json!("different-session"),
        ),
        ("/media/context/session/sessionId", Value::Null),
        ("/media/context", json!("not-an-owned-context")),
    ] {
        let mut changed = serde_json::to_value(&valid).unwrap();
        *changed.pointer_mut(field).unwrap() = replacement;
        let changed: HostBoundPreparedLease = serde_json::from_value(changed).unwrap();
        assert!(changed.validate_against(&offer, 1000).is_err(), "{field}");
    }
}

#[test]
fn host_binding_preserves_source_account_authority_session_and_attempt_exactly() {
    let offer = offer();
    let original = lease(&offer);
    for account in [
        json!({"authority":"test-authority","account":"test-account"}),
        Value::Null,
    ] {
        let mut value = serde_json::to_value(&original).unwrap();
        value["session"]["account"] = account;
        let lease: HostBoundPreparedLease = serde_json::from_value(value).unwrap();
        lease.validate_against(&offer, 1000).unwrap();
        let binding = serde_json::to_value(lease.worker_binding()).unwrap();
        assert_eq!(
            binding,
            json!({"leaseId":lease.lease_id,"sourceId":lease.source_id,
            "session":lease.session,"attemptId":lease.attempt_id})
        );
        assert_eq!(
            lease.public_binding(),
            json!({"leaseId":lease.lease_id,"sourceId":lease.source_id,
            "session":lease.session,"attemptId":lease.attempt_id,"runtimeEpoch":lease.runtime_epoch})
        );
    }
    for field in [
        "/leaseId",
        "/attemptId",
        "/session/remoteId",
        "/session/account/authority",
        "/session/account/account",
    ] {
        let mut value = serde_json::to_value(&original).unwrap();
        *value.pointer_mut(field).unwrap() = json!("");
        assert!(
            serde_json::from_value::<HostBoundPreparedLease>(value).is_err(),
            "{field}"
        );
    }
}

#[test]
fn no_audio_is_valid_without_advertising_or_inventing_an_opus_format() {
    let mut offer = offer();
    offer.audio_formats = Default::default();
    offer.validate().unwrap();
    let mut accepted = accepted(&offer);
    assert!(accepted.validate_against(&offer, 1000).is_err());
    accepted.audio = None;
    accepted.validate_against(&offer, 1000).unwrap();
    assert_eq!(
        serde_json::to_value(&accepted).unwrap()["audio"],
        Value::Null
    );
}

fn header() -> MediaHeader {
    MediaHeader {
        attempt_generation: 17,
        track_id: VIDEO_TRACK_ID,
        payload_bytes: 4096,
        source: SourceStamp {
            sender_frame_id: Some((1 << 53) + 113),
            timestamp: u64::MAX - 1,
            clock_rate_hz: 90_000,
            ssrc: Some(u32::MAX),
        },
        keyframe: true,
        contiguous: false,
    }
}

#[test]
fn fixed_56_byte_header_preserves_optional_zero_and_large_sender_identifiers() {
    assert_eq!(MEDIA_HEADER_BYTES, 56);
    for sender_frame_id in [Some((1 << 53) + 113), Some(u64::MAX), Some(0), None] {
        for ssrc in [Some(u32::MAX), Some(0), None] {
            let mut expected = header();
            expected.source.sender_frame_id = sender_frame_id;
            expected.source.ssrc = ssrc;
            let encoded = expected.encode();
            assert_eq!(encoded.len(), 56);
            assert_eq!(&encoded[..4], b"ONW1");
            assert_eq!(encoded[4] & 4 != 0, sender_frame_id.is_some());
            assert_eq!(encoded[4] & 8 != 0, ssrc.is_some());
            let decoded = MediaHeader::decode(&encoded, &offer().limits).unwrap();
            assert_eq!(decoded, expected);
            let provenance = decoded.provenance();
            assert_eq!(provenance.source, Some(expected.source));
            provenance.validate().unwrap();
            let roundtrip: FrameProvenance =
                serde_json::from_slice(&serde_json::to_vec(&provenance).unwrap()).unwrap();
            assert_eq!(roundtrip, provenance);
        }
    }
}

#[test]
fn media_header_rejects_unknown_flags_reserved_bytes_and_invalid_lengths() {
    let valid = header().encode();
    let limits = offer().limits;
    for length in 0..MEDIA_HEADER_BYTES {
        assert!(MediaHeader::decode(&valid[..length], &limits).is_err());
    }
    let mut oversized = valid.to_vec();
    oversized.push(0);
    assert!(MediaHeader::decode(&oversized, &limits).is_err());
    for index in [0, 5, 6, 7, 48, 49, 50, 51, 52, 53, 54, 55] {
        let mut invalid = valid;
        invalid[index] ^= 1;
        assert!(
            MediaHeader::decode(&invalid, &limits).is_err(),
            "byte {index}"
        );
    }
    for bit in [16, 32, 64, 128] {
        let mut invalid = valid;
        invalid[4] |= bit;
        assert!(
            MediaHeader::decode(&invalid, &limits).is_err(),
            "flag {bit}"
        );
    }
}

#[test]
fn media_header_requires_attempt_clock_known_track_and_per_track_payload_bounds() {
    let limits = offer().limits;
    for (track_id, maximum, clock_rate_hz) in [
        (VIDEO_TRACK_ID, limits.max_video_access_unit_bytes, 90_000),
        (AUDIO_TRACK_ID, limits.max_audio_packet_bytes, 48_000),
    ] {
        for payload_bytes in [1, maximum] {
            let mut valid = header();
            valid.track_id = track_id;
            valid.payload_bytes = payload_bytes;
            valid.source.clock_rate_hz = clock_rate_hz;
            assert_eq!(
                MediaHeader::decode(&valid.encode(), &limits).unwrap(),
                valid
            );
        }
        for payload_bytes in [0, maximum + 1, u32::MAX] {
            let invalid = MediaHeader {
                track_id,
                payload_bytes,
                ..header()
            };
            assert!(MediaHeader::decode(&invalid.encode(), &limits).is_err());
        }
    }
    for track_id in [0, 3, u32::MAX] {
        assert!(
            MediaHeader::decode(
                &MediaHeader {
                    track_id,
                    ..header()
                }
                .encode(),
                &limits
            )
            .is_err()
        );
    }
    assert!(
        MediaHeader::decode(
            &MediaHeader {
                attempt_generation: 0,
                ..header()
            }
            .encode(),
            &limits
        )
        .is_err()
    );
    let mut no_clock = header();
    no_clock.source.clock_rate_hz = 0;
    assert!(MediaHeader::decode(&no_clock.encode(), &limits).is_err());
}

#[test]
fn unknown_provenance_stays_absent_and_known_provenance_requires_its_clock_and_identity() {
    let unknown = FrameProvenance::default();
    unknown.validate().unwrap();
    assert_eq!(
        serde_json::to_value(unknown).unwrap()["source"],
        Value::Null
    );
    let known = header().provenance();
    assert!(
        FrameProvenance {
            attempt_generation: 0,
            ..known
        }
        .validate()
        .is_err()
    );
    assert!(
        FrameProvenance {
            track_id: 0,
            ..known
        }
        .validate()
        .is_err()
    );
    let mut invalid = known;
    invalid.source.as_mut().unwrap().clock_rate_hz = 0;
    assert!(invalid.validate().is_err());
}

fn bootstrap() -> WorkerBootstrap {
    let offer = offer();
    WorkerBootstrap {
        version: 1,
        binding: lease(&offer).worker_binding(),
        attempt_generation: 17,
        control_port: 43210,
        authentication: SecretBytes::new(b"0123456789abcdef0123456789abcdef".to_vec()).unwrap(),
        accepted: accepted(&offer),
        limits: offer.limits,
        provider_bootstrap: SecretBytes::new(b"provider-private-token".to_vec()).unwrap(),
    }
}

#[test]
fn bootstrap_validation_requires_version_attempt_port_and_exact_authentication_length() {
    let valid = bootstrap();
    valid.validate().unwrap();
    for field in ["version", "attemptGeneration", "controlPort"] {
        let mut changed = serde_json::to_value(&valid).unwrap();
        changed[field] = json!(0);
        let changed: WorkerBootstrap = serde_json::from_value(changed).unwrap();
        assert!(changed.validate().is_err(), "{field}");
    }
    for length in [0, 31, 33] {
        let mut changed = valid.clone();
        changed.authentication = SecretBytes::new(vec![1; length]).unwrap();
        assert!(changed.validate().is_err());
    }
    let mut silent = valid;
    silent.accepted.audio = None;
    silent.validate().unwrap();
}

#[test]
fn private_lease_bootstrap_and_control_debug_never_expose_secrets() {
    let offer = offer();
    let worker = lease(&offer);
    let gfn = gfn_lease(&offer);
    let bootstrap = bootstrap();
    let hello = ControlMessage::Hello {
        version: 1,
        authentication: bootstrap.authentication.clone(),
        attempt_generation: 17,
    };
    let debug = format!("{worker:?} {gfn:?} {bootstrap:?} {hello:?}");
    for secret in [
        "provider-private-token",
        "gfn-private-token",
        "0123456789abcdef",
        "private-version-root",
        "private-data-root",
    ] {
        assert!(!debug.contains(secret), "debug exposed {secret}");
    }
    for secret in [&bootstrap.authentication, &bootstrap.provider_bootstrap] {
        let encoded = serde_json::to_value(secret).unwrap();
        assert!(!debug.contains(encoded.as_str().unwrap()));
        assert_eq!(format!("{secret:?}"), "SecretBytes([redacted])");
    }
    assert!(matches!(worker.media, PreparedMedia::Worker { .. }));
    let public = worker.public_binding().to_string();
    for private_field in [
        "bootstrap",
        "prepared",
        "package",
        "versionRoot",
        "dataRoot",
    ] {
        assert!(!public.contains(private_field));
    }
}

#[test]
fn control_frames_obey_exact_length_bounds_and_reject_truncation_or_unknown_fields() {
    let message = ControlMessage::Attached {
        attempt_generation: u64::MAX,
    };
    let encoded = encode_control(&message, MAX_CONTROL_BYTES).unwrap();
    let size = encoded.len() - 4;
    assert_eq!(
        u32::from_le_bytes(encoded[..4].try_into().unwrap()) as usize,
        size
    );
    assert!(encode_control(&message, size - 1).is_err());
    assert!(encode_control(&message, size).is_ok());
    assert!(matches!(
        read_control(&mut encoded.as_slice(), size).unwrap(),
        ControlMessage::Attached {
            attempt_generation: u64::MAX
        }
    ));
    assert!(read_control(&mut encoded.as_slice(), size - 1).is_err());
    for length in [0, 3, encoded.len() - 1] {
        assert!(read_control(&mut &encoded[..length], MAX_CONTROL_BYTES).is_err());
    }
    for length in [0, MAX_CONTROL_BYTES as u32 + 1, u32::MAX] {
        assert!(read_control(&mut length.to_le_bytes().as_slice(), usize::MAX).is_err());
    }
    let payload = br#"{"type":"attached","attemptGeneration":17,"providerOverride":true}"#;
    let mut invalid = (payload.len() as u32).to_le_bytes().to_vec();
    invalid.extend_from_slice(payload);
    assert!(read_control(&mut invalid.as_slice(), MAX_CONTROL_BYTES).is_err());
}
