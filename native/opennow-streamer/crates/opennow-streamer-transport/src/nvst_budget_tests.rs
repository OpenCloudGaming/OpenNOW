use super::*;

use std::net::UdpSocket;
use std::time::Duration;

use str0m::net::Receive;
use str0m::{Candidate, Input, Output};

use crate::nvst_budget::{
    NORMAL_BUDGET, TEARDOWN_RESERVE, TEXT_BATCH_GUARD, TRANSPORT_AGGREGATE_CAPACITY, WriteClass,
    admit_write,
};
use crate::nvst_input::{
    NvstEncodedInput, NvstInputRoute, SonyDeviceControl, sony_device_change_command,
    sony_report_command,
};

fn exchange(local: &mut Rtc, remote: &mut Rtc, now: Instant) {
    loop {
        match local.poll_output() {
            Ok(Output::Timeout(_)) => break,
            Ok(Output::Transmit(packet)) => {
                remote
                    .handle_input(Input::Receive(
                        now,
                        Receive {
                            proto: packet.proto,
                            source: packet.source,
                            destination: packet.destination,
                            contents: packet.contents.as_ref().try_into().unwrap(),
                        },
                    ))
                    .unwrap();
            }
            Ok(_) => {}
            Err(error) => panic!("bundle poll failed: {error}"),
        }
    }
}

pub(super) fn connect_bundle_pair() -> (Rtc, Rtc, NvstInputChannels, NvstInputChannels) {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut local = create_nvst_bundle_rtc(&socket).unwrap();
    let mut remote = create_nvst_bundle_rtc(&socket).unwrap();
    let local_candidate = Candidate::host("192.0.2.1:1000".parse().unwrap(), "udp").unwrap();
    let remote_candidate = Candidate::host("192.0.2.2:2000".parse().unwrap(), "udp").unwrap();
    local.add_local_candidate(local_candidate.clone());
    local.add_remote_candidate(remote_candidate.clone());
    remote.add_local_candidate(remote_candidate);
    remote.add_remote_candidate(local_candidate);
    let local_identity = local.direct_api().local_dtls_fingerprint().clone();
    let remote_identity = remote.direct_api().local_dtls_fingerprint().clone();
    let local_ice = local.direct_api().local_ice_credentials();
    let remote_ice = remote.direct_api().local_ice_credentials();
    local.direct_api().set_remote_fingerprint(remote_identity);
    remote.direct_api().set_remote_fingerprint(local_identity);
    local.direct_api().set_remote_ice_credentials(remote_ice);
    remote.direct_api().set_remote_ice_credentials(local_ice);
    local.direct_api().set_ice_controlling(true);
    remote.direct_api().set_ice_controlling(false);
    local.direct_api().start_dtls(true).unwrap();
    remote.direct_api().start_dtls(false).unwrap();
    let local_channels = NvstInputChannels::create(&mut local, true);
    let remote_channels = NvstInputChannels::create(&mut remote, true);
    local.direct_api().start_sctp(true);
    remote.direct_api().start_sctp(false);
    let origin = Instant::now();
    let mut established = false;
    for tick in 0..4000 {
        let now = origin + Duration::from_millis(tick);
        local.handle_input(Input::Timeout(now)).unwrap();
        remote.handle_input(Input::Timeout(now)).unwrap();
        exchange(&mut local, &mut remote, now);
        exchange(&mut remote, &mut local, now);
        if tick % 16 == 0
            && local.is_connected()
            && remote.is_connected()
            && local_channels.send_keepalive(&mut local, 1)
        {
            established = true;
            break;
        }
    }
    assert!(
        established,
        "the bundle pair never produced an established SCTP association"
    );
    (local, remote, local_channels, remote_channels)
}

fn sony_teardown_commands() -> Vec<Vec<u8>> {
    let report = [0_u8; 64];
    let mut commands = Vec::new();
    for low_id in 6..=9 {
        commands.push(sony_report_command(low_id, &report).unwrap());
        commands.push(sony_device_change_command(SonyDeviceControl::Removal, low_id).unwrap());
    }
    commands
}

#[test]
fn teardown_budget_covers_the_admitted_sony_bounds_with_full_framing() {
    let commands = sony_teardown_commands();
    assert_eq!(commands.len(), 8);
    let total: usize = commands.iter().map(Vec::len).sum();
    assert_eq!(total, 4 * (76 + 26));
    assert!(total <= TEARDOWN_RESERVE);
    assert!(total < NORMAL_BUDGET);
}

#[test]
fn actual_writers_reserve_teardown_capacity_under_a_normal_flood() {
    let (mut local, _remote, channels, _remote_channels) = connect_bundle_pair();

    let report_command = sony_report_command(6, &[0_u8; 64]).unwrap();
    let removal_command = sony_device_change_command(SonyDeviceControl::Removal, 6).unwrap();

    let mut normal_written = 0_usize;
    let mut accepted = 0_usize;
    while channels.send_control(&mut local, &report_command) {
        normal_written += report_command.len();
        accepted += 1;
        assert!(accepted <= TRANSPORT_AGGREGATE_CAPACITY / 8);
    }
    assert!(
        normal_written <= NORMAL_BUDGET,
        "normal writers spent {normal_written} bytes beyond the shared budget"
    );
    assert!(normal_written + report_command.len() > NORMAL_BUDGET);

    let buffered = channels.buffered_total(&mut local);
    assert!(buffered <= NORMAL_BUDGET);
    assert!(buffered >= NORMAL_BUDGET - report_command.len());
    assert!(
        TRANSPORT_AGGREGATE_CAPACITY - buffered >= TEARDOWN_RESERVE,
        "the normal flood consumed the teardown reservation"
    );

    assert!(
        !channels.send_control(&mut local, &report_command),
        "a normal write must not spend the teardown reservation"
    );
    assert!(
        !channels.send_encoded(
            &mut local,
            &NvstEncodedInput {
                route: NvstInputRoute::ControlReliable,
                bytes: vec![0_u8; report_command.len()],
            }
        ),
        "reliable encoded input must be normal traffic"
    );

    assert!(
        channels.send_control_teardown(&mut local, &removal_command),
        "teardown traffic must still be admitted at the normal budget edge"
    );

    let mut teardown_written = removal_command.len();
    let mut teardown_commands = 0_usize;
    loop {
        let command = sony_teardown_commands()
            .into_iter()
            .nth(teardown_commands % 8)
            .unwrap();
        if !channels.send_control_teardown(&mut local, &command) {
            break;
        }
        teardown_written += command.len();
        teardown_commands += 1;
        assert!(teardown_commands <= TRANSPORT_AGGREGATE_CAPACITY);
    }

    let worst_case_batch: usize = sony_teardown_commands().iter().map(Vec::len).sum();
    assert!(
        teardown_written >= worst_case_batch,
        "the reservation must cover the worst-case teardown batch"
    );
    assert!(
        teardown_written <= TEARDOWN_RESERVE,
        "teardown traffic spent {teardown_written} bytes beyond the reservation"
    );
    assert!(
        !channels.send_control(&mut local, &report_command),
        "normal traffic must not resume while the reservation is in use"
    );
    let buffered = channels.buffered_total(&mut local);
    assert!(buffered <= TRANSPORT_AGGREGATE_CAPACITY);
}

#[test]
fn rtcp_writer_is_normal_traffic() {
    let (mut local, _remote, channels, _remote_channels) = connect_bundle_pair();

    let rtcp = build_rtcp_receiver_report(
        1,
        RtcpReportBlock {
            media_ssrc: 2,
            fraction_lost: 0,
            cumulative_lost: 0,
            highest_sequence: 0,
            jitter: 0,
        },
    );

    let mut rtcp_written = 0_usize;
    let mut iterations = 0_usize;
    while channels.send_rtcp(&mut local, &rtcp) {
        rtcp_written += rtcp.len();
        iterations += 1;
        assert!(iterations <= TRANSPORT_AGGREGATE_CAPACITY);
    }
    assert!(
        rtcp_written <= NORMAL_BUDGET,
        "RTCP writes spent {rtcp_written} bytes beyond the shared budget"
    );
    let buffered = channels.buffered_total(&mut local);
    assert!(
        TRANSPORT_AGGREGATE_CAPACITY - buffered >= TEARDOWN_RESERVE,
        "RTCP writes consumed the teardown reservation"
    );
}

#[test]
fn the_text_guard_still_rejects_a_batch_that_the_shared_budget_cannot_hold() {
    const { assert!(TEXT_BATCH_GUARD > NORMAL_BUDGET) };
    assert_eq!(
        admit_write(0, TEXT_BATCH_GUARD, WriteClass::Normal),
        crate::nvst_budget::WriteAdmission::OverBudget
    );
}

fn sony_review_runtime(incarnation: u64, server_mask: u32) -> (HidRuntime, HidSession, NvstInputCodec) {
    let runtime = HidRuntime::new();
    let claim = opennow_streamer_hid::SdlDeviceClaim::new(0, incarnation, 0x054c, 0x05c4).unwrap();
    runtime.replace_inventory(&[Some(claim)]);
    runtime.open_endpoint();
    assert!(runtime.bind_session(1).is_some());
    (
        runtime,
        HidSession::new(SonyCapability {
            server_mask,
            ..SonyCapability::default()
        }),
        NvstInputCodec::default(),
    )
}

#[test]
fn parent_review_capture_pause_retains_sony_attachment() {
    let (mut rtc, _peer, channels, _peer_channels) = connect_bundle_pair();
    let (runtime, mut session, mut codec) = sony_review_runtime(11, 5);
    runtime.set_active(true);
    let mut mask = 0;
    assert!(pump_hid_pending(
        &runtime,
        &mut session,
        &mut codec,
        channels,
        &mut rtc,
        &mut mask,
        1
    ));
    assert_eq!(session.attached(0), Some(11));
    runtime.set_active(false);
    assert!(pump_hid_pending(
        &runtime,
        &mut session,
        &mut codec,
        channels,
        &mut rtc,
        &mut mask,
        2
    ));
    assert_eq!(
        session.attached(0),
        Some(11),
        "capture closure must retain the Sony attachment"
    );
    assert!(!session.is_tombstoned(0));
}

#[test]
fn parent_review_resume_preserves_undrained_release() {
    let (runtime, _session, _codec) = sony_review_runtime(11, 5);
    runtime.set_active(true);
    let mut snapshot = opennow_streamer_hid::SonySnapshot::neutral(0, 11, 1);
    snapshot.buttons = 1;
    runtime.submit_snapshot(snapshot);
    runtime.drain(32);
    runtime.set_active(false);
    runtime.set_active(true);
    assert!(
        runtime
            .drain(32)
            .iter()
            .any(|item| matches!(item, IngressItem::Release { .. })),
        "resume must not erase a release before the bundle writer consumes it"
    );
}

#[test]
fn unbound_capture_leaves_the_bundle_endpoint_inactive() {
    let (mut rtc, _peer, channels, _peer_channels) = connect_bundle_pair();
    let runtime = HidRuntime::new();
    let claim = opennow_streamer_hid::SdlDeviceClaim::new(0, 11, 0x054c, 0x05c4).unwrap();
    runtime.replace_inventory(&[Some(claim)]);
    runtime.set_active(true);
    let mut session = HidSession::new(SonyCapability {
        server_mask: 5,
        ..SonyCapability::default()
    });
    let mut codec = NvstInputCodec::default();
    let mut mask = 0;
    assert!(pump_hid_pending(
        &runtime,
        &mut session,
        &mut codec,
        channels,
        &mut rtc,
        &mut mask,
        1
    ));
    assert_eq!(session.attached(0), None);
    assert_eq!(mask, 0);
    let mut snapshot = opennow_streamer_hid::SonySnapshot::neutral(0, 11, 1);
    snapshot.buttons = 0x1000;
    assert_eq!(
        runtime.submit_snapshot(snapshot),
        opennow_streamer_hid::SnapshotAdmission::Unbound
    );
    assert!(runtime.drain(8).is_empty());
}

#[test]
fn repeated_capture_pause_cycles_send_one_neutral_report_per_source() {
    let (mut rtc, _peer, channels, _peer_channels) = connect_bundle_pair();
    let (runtime, mut session, mut codec) = sony_review_runtime(11, 5);
    runtime.set_active(true);
    let mut mask = 0;
    assert!(pump_hid_pending(
        &runtime,
        &mut session,
        &mut codec,
        channels,
        &mut rtc,
        &mut mask,
        1
    ));
    assert_eq!(mask, 0b0001);
    let neutral_report = sony_report_command(6, &[0_u8; 64]).unwrap();
    let neutral_len = neutral_report.len();
    assert_eq!(neutral_len, 76);
    for cycle in 0..8_u64 {
        let before = channels.buffered_total(&mut rtc);
        runtime.set_active(false);
        assert!(pump_hid_pending(
            &runtime,
            &mut session,
            &mut codec,
            channels,
            &mut rtc,
            &mut mask,
            100 + cycle
        ));
        let after = channels.buffered_total(&mut rtc);
        assert_eq!(
            after - before,
            neutral_len,
            "one capture pause must spend exactly one neutral report"
        );
        runtime.set_active(true);
        assert!(pump_hid_pending(
            &runtime,
            &mut session,
            &mut codec,
            channels,
            &mut rtc,
            &mut mask,
            200 + cycle
        ));
        assert_eq!(
            channels.buffered_total(&mut rtc),
            after,
            "resuming capture must not queue anything before new input arrives"
        );
    }
    assert_eq!(session.attached(0), Some(11));
    assert!(!session.is_tombstoned(0));
    assert_eq!(mask, 0b0001);
    let buffered = channels.buffered_total(&mut rtc);
    assert!(
        TRANSPORT_AGGREGATE_CAPACITY - buffered >= TEARDOWN_RESERVE,
        "repeated capture cycles drained the teardown reservation"
    );
    assert!(channels.send_control(&mut rtc, &neutral_report));
}

fn drain_peer_events(peer: &mut Rtc, now: Instant) -> Vec<str0m::Event> {
    let mut events = Vec::new();
    loop {
        match peer.poll_output() {
            Ok(Output::Timeout(_)) => break,
            Ok(Output::Event(event)) => events.push(event),
            Ok(Output::Transmit(_)) => {}
            Err(_) => break,
        }
    }
    let _ = now;
    events
}

#[test]
fn sony_shutdown_flush_reaches_the_receiving_peer() {
    let (mut local, mut peer, channels, _peer_channels) = connect_bundle_pair();
    let mut session = HidSession::new(SonyCapability {
        server_mask: 5,
        ..SonyCapability::default()
    });
    session
        .adopt(opennow_streamer_hid::SdlDeviceClaim::new(0, 11, 0x054c, 0x05c4).unwrap())
        .unwrap();
    let mut snapshot = opennow_streamer_hid::SonySnapshot::neutral(0, 11, 1_000_000);
    snapshot.buttons = 0x1000;
    let (_, prior) = session.build_report(&snapshot).unwrap();
    queue_hid_shutdown(&mut session, channels, &mut local);
    forward_rtc_outputs(
        &mut local,
        |output| {
            if let Output::Transmit(transmit) = output {
                peer.handle_input(Input::Receive(
                    Instant::now(),
                    Receive {
                        proto: transmit.proto,
                        source: transmit.source,
                        destination: transmit.destination,
                        contents: transmit.contents.as_ref().try_into().unwrap(),
                    },
                ))
                .unwrap();
            }
        },
        Duration::from_millis(200),
    );
    let mut payloads = Vec::new();
    for event in drain_peer_events(&mut peer, Instant::now()) {
        if let str0m::Event::ChannelData(data) = event {
            payloads.push(data.data);
        }
    }
    assert!(
        !payloads.is_empty(),
        "the shutdown release must reach the receiving peer"
    );
    let removal = sony_device_change_command(SonyDeviceControl::Removal, 6).unwrap();
    let release = sony_report_command(6, &[0_u8; 64]).unwrap();
    assert!(
        payloads.iter().any(|payload| payload == &removal),
        "the receiving peer must observe the Sony removal"
    );
    let observed_release = payloads
        .iter()
        .find(|payload| payload.len() == release.len())
        .expect("the receiving peer must observe the Sony release report");
    let report = &observed_release[observed_release.len() - 64..];
    assert_eq!(&report[10..12], &prior[10..12]);
    assert_eq!(report[0], 0x01);
}

#[test]
fn sony_rich_reports_reach_the_receiving_peer_after_admission() {
    let (mut local, mut peer, channels, _peer_channels) = connect_bundle_pair();
    let (runtime, mut session, mut codec) = sony_review_runtime(11, 5);
    runtime.set_active(true);
    let mut mask = 0;
    assert!(pump_hid_pending(
        &runtime,
        &mut session,
        &mut codec,
        channels,
        &mut local,
        &mut mask,
        1
    ));
    assert_eq!(mask, 0b0001);
    let mut snapshot = opennow_streamer_hid::SonySnapshot::neutral(0, 11, 2_000_000);
    snapshot.buttons = 0x1000;
    snapshot.left_stick_x = 12_000;
    assert_eq!(
        runtime.submit_snapshot(snapshot),
        opennow_streamer_hid::SnapshotAdmission::Admitted
    );
    assert!(pump_hid_pending(
        &runtime,
        &mut session,
        &mut codec,
        channels,
        &mut local,
        &mut mask,
        2
    ));
    forward_rtc_outputs(
        &mut local,
        |output| {
            if let Output::Transmit(transmit) = output {
                peer.handle_input(Input::Receive(
                    Instant::now(),
                    Receive {
                        proto: transmit.proto,
                        source: transmit.source,
                        destination: transmit.destination,
                        contents: transmit.contents.as_ref().try_into().unwrap(),
                    },
                ))
                .unwrap();
            }
        },
        Duration::from_millis(200),
    );
    let mut payloads = Vec::new();
    for event in drain_peer_events(&mut peer, Instant::now()) {
        if let str0m::Event::ChannelData(data) = event {
            payloads.push(data.data);
        }
    }
    let report = payloads
        .iter()
        .find(|payload| payload.len() == 76)
        .expect("the peer must receive the live Sony report");
    assert_eq!(report[report.len() - 64], 0x01);
    let buttons = u16::from_le_bytes([
        report[report.len() - 64 + 5],
        report[report.len() - 64 + 6],
    ]);
    assert_eq!(buttons & 0x20, 0x20);
}
