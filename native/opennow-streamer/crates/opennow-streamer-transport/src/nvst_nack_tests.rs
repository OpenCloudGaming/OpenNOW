use super::*;
use str0m::channel::Reliability;

use super::budget_tests::connect_bundle_pair;
use crate::nvst_budget::{NORMAL_BUDGET, TRANSPORT_AGGREGATE_CAPACITY};

fn receive_messages(local: &mut Rtc, remote: &mut Rtc, now: Instant) -> Vec<(String, Vec<u8>)> {
    let mut messages = Vec::new();
    for _ in 0..4 {
        for collect in [false, true] {
            let (source, destination) = if collect {
                (&mut *remote, &mut *local)
            } else {
                (&mut *local, &mut *remote)
            };
            loop {
                match source.poll_output().unwrap() {
                    Output::Timeout(_) => break,
                    Output::Transmit(packet) => destination
                        .handle_input(Input::Receive(
                            now,
                            Receive {
                                proto: packet.proto,
                                source: packet.source,
                                destination: packet.destination,
                                contents: packet.contents.as_ref().try_into().unwrap(),
                            },
                        ))
                        .unwrap(),
                    Output::Event(Event::ChannelData(data)) if collect => {
                        assert!(data.binary);
                        let channel = source.channel(data.id).unwrap();
                        messages.push((channel.config().unwrap().label.clone(), data.data));
                    }
                    _ => {}
                }
            }
        }
    }
    messages
}

#[test]
fn mjolnir_nack_uses_partial_control_and_preserves_wrapping_rtp_sequences() {
    let (mut local, mut remote, channels, _) = connect_bundle_pair();
    let now = Instant::now() + Duration::from_secs(1);
    receive_messages(&mut local, &mut remote, now);
    let partial = local.channel(channels.control_partial).unwrap();
    let config = partial.config().unwrap();
    assert!(!config.ordered);
    assert_eq!(
        config.reliability,
        Reliability::MaxPacketLifetime { lifetime: 300 }
    );
    let rtcp = channels.rtcp.expect("bundle RTCP channel");
    local.direct_api().close_data_channel(rtcp);
    receive_messages(&mut local, &mut remote, now);
    assert!(local.channel(rtcp).is_none());

    let feedback = NvstFeedbackState::default();
    feedback.request_nack(65534, 65537, now);
    send_pending_nack(&feedback, now, &mut local, channels, true, 1, 0x12345678);
    let messages = receive_messages(&mut local, &mut remote, now);
    assert_eq!(
        messages,
        vec![(
            "control_channel_partially_reliable".to_owned(),
            vec![0x17, 3, 13, 0, 2, 0, 1, 0xfe, 0xff, 7, 0, 0, 0, 0, 0, 0, 0],
        )]
    );
    assert_eq!(feedback.take_nack(now, Some(DEFAULT_NACK_RTT)), None);
    assert!(feedback.resolve_nack(65536));
    assert!(!feedback.keyframe_request_pending());
}

#[test]
fn non_mjolnir_nack_remains_rtcp_generic_on_the_rtcp_channel() {
    let (mut local, mut remote, channels, _) = connect_bundle_pair();
    let now = Instant::now() + Duration::from_secs(1);
    receive_messages(&mut local, &mut remote, now);
    local
        .direct_api()
        .close_data_channel(channels.control_partial);
    receive_messages(&mut local, &mut remote, now);
    assert!(local.channel(channels.control_partial).is_none());
    let feedback = NvstFeedbackState::default();
    feedback.request_nack(65534, 65537, now);
    send_pending_nack(&feedback, now, &mut local, channels, false, 1, 2);
    assert_eq!(
        receive_messages(&mut local, &mut remote, now),
        vec![(
            "rtcp_on_sctp_private".to_owned(),
            vec![0x81, 205, 0, 3, 0, 0, 0, 1, 0, 0, 0, 2, 0xff, 0xfe, 0, 7],
        )]
    );
    assert!(feedback.resolve_nack(65536));
}

#[test]
fn a_closed_nack_channel_does_not_spend_an_attempt_or_fall_back() {
    for mjolnir in [true, false] {
        let (mut local, mut remote, channels, _) = connect_bundle_pair();
        let now = Instant::now() + Duration::from_secs(1);
        receive_messages(&mut local, &mut remote, now);
        local.direct_api().close_data_channel(if mjolnir {
            channels.control_partial
        } else {
            channels.rtcp.expect("bundle RTCP channel")
        });
        receive_messages(&mut local, &mut remote, now);
        let feedback = NvstFeedbackState::default();
        feedback.request_nack(42, 42, now);
        for _ in 0..MAX_NACK_ATTEMPTS + 1 {
            send_pending_nack(&feedback, now, &mut local, channels, mjolnir, 1, 2);
        }
        assert!(receive_messages(&mut local, &mut remote, now).is_empty());
        assert!(!feedback.resolve_nack(42));
    }
}

#[test]
fn rejected_nacks_preserve_the_shared_budget_and_retry_attempts() {
    for mjolnir in [true, false] {
        let (mut local, _remote, channels, _) = connect_bundle_pair();
        let now = Instant::now();
        let buffered = channels.buffered_total(&mut local);
        assert!(channels.send_control(&mut local, &vec![0; NORMAL_BUDGET - buffered]));
        let feedback = NvstFeedbackState::default();
        feedback.request_nack(42, 42, now);
        for _ in 0..MAX_NACK_ATTEMPTS + 1 {
            send_pending_nack(&feedback, now, &mut local, channels, mjolnir, 1, 2);
        }
        let buffered = channels.buffered_total(&mut local);
        assert_eq!(buffered, NORMAL_BUDGET);
        assert!(buffered < TRANSPORT_AGGREGATE_CAPACITY);
        assert!(!feedback.resolve_nack(42));
        feedback.request_nack(43, 43, now);
        send_pending_nack(&feedback, now, &mut local, channels, mjolnir, 1, 2);
        assert_eq!(
            feedback.take_nack(now, Some(DEFAULT_NACK_RTT)),
            Some((43, 43))
        );
    }
}

#[test]
fn mjolnir_nacks_keep_the_existing_batch_retry_and_expiry_limits() {
    let (mut local, mut remote, channels, _) = connect_bundle_pair();
    let now = Instant::now() + Duration::from_secs(1);
    receive_messages(&mut local, &mut remote, now);
    let feedback = NvstFeedbackState::default();
    let rtt = Duration::from_millis(5);
    let retry_interval = rtt + NACK_RETRY_INTERVAL;
    feedback.publish_ping(true, now, rtt);
    feedback.request_nack(u64::MAX - 64, u64::MAX, now);
    for _ in 0..2 {
        send_pending_nack(&feedback, now, &mut local, channels, true, 1, 2);
    }
    let messages = receive_messages(&mut local, &mut remote, now);
    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[0].1,
        nack_v2(0, &(65471..65535).collect::<Vec<_>>())
            .unwrap()
            .encoded()
    );
    assert_eq!(messages[1].1, nack_v2(0, &[65535]).unwrap().encoded());
    assert_eq!(feedback.take_nack(now, Some(rtt)), None);
    for attempt in 1..MAX_NACK_ATTEMPTS {
        let retry_at = now + retry_interval * u32::from(attempt);
        send_pending_nack(
            &feedback,
            retry_at - Duration::from_nanos(1),
            &mut local,
            channels,
            true,
            1,
            2,
        );
        assert!(receive_messages(&mut local, &mut remote, retry_at).is_empty());
        for _ in 0..2 {
            send_pending_nack(&feedback, retry_at, &mut local, channels, true, 1, 2);
        }
        assert_eq!(receive_messages(&mut local, &mut remote, retry_at).len(), 2);
    }
    send_pending_nack(
        &feedback,
        now + retry_interval * 3,
        &mut local,
        channels,
        true,
        1,
        2,
    );
    assert!(receive_messages(&mut local, &mut remote, now + retry_interval * 3).is_empty());
    assert_eq!(
        feedback.take_nack(now + NACK_TRACKING_TIMEOUT, Some(rtt)),
        None
    );
    assert!(!feedback.resolve_nack(u64::MAX));
    assert!(!feedback.keyframe_request_pending());
}

#[test]
fn rtt_aware_nack_spacing_and_send_caps_match_the_wait_budget() {
    let cases: &[(Duration, &[u64])] = &[
        (Duration::ZERO, &[0, 4, 8]),
        (Duration::from_nanos(1), &[0, 5, 10]),
        (Duration::from_millis(5), &[0, 9, 18]),
        (Duration::from_millis(10), &[0, 14, 28]),
        (Duration::from_millis(17), &[0, 21, 42]),
        (Duration::from_micros(17_333), &[0, 22, 44]),
        (Duration::from_micros(17_334), &[0, 22]),
        (Duration::from_millis(20), &[0, 24]),
        (Duration::from_millis(26), &[0, 30]),
        (Duration::from_micros(26_001), &[0]),
        (Duration::from_millis(30), &[0]),
        (Duration::from_millis(52), &[0]),
        (Duration::from_millis(100), &[0]),
        (Duration::MAX, &[0]),
    ];
    for &(rtt, expected) in cases {
        let feedback = NvstFeedbackState::default();
        let now = Instant::now();
        feedback.request_nack(42, 42, now);
        let mut sent_at = Vec::new();
        for millis in 0..=52 {
            if let Some(range) = feedback.take_nack(now + Duration::from_millis(millis), Some(rtt))
            {
                assert_eq!(range, (42, 42));
                sent_at.push(millis);
            }
        }
        assert_eq!(sent_at, expected, "RTT {rtt:?}");
        assert!(!feedback.resolve_nack(42));
        assert!(!feedback.keyframe_request_pending());
    }
}

#[test]
fn rtt_aware_nack_cap_keeps_sent_packets_resolvable_until_the_deadline() {
    let feedback = NvstFeedbackState::default();
    let now = Instant::now();
    let rtt = Some(Duration::from_millis(40));
    feedback.request_nack(42, 43, now);
    assert_eq!(feedback.take_nack(now, rtt), Some((42, 43)));
    assert_eq!(
        feedback.take_nack(now + Duration::from_millis(44), rtt),
        None
    );
    assert!(feedback.resolve_nack(42));
    assert_eq!(feedback.take_nack(now + NACK_TRACKING_TIMEOUT, rtt), None);
    assert!(!feedback.resolve_nack(43));
}

#[test]
fn mjolnir_nack_sender_uses_fresh_rtt_with_bounded_unknown_and_stale_fallbacks() {
    for (video_rtt, stale, expected) in [
        (None, false, vec![0]),
        (Some(Duration::from_millis(5)), true, vec![0]),
        (Some(Duration::ZERO), false, vec![0, 4, 8]),
        (Some(Duration::from_millis(20)), false, vec![0, 24]),
    ] {
        let (mut local, mut remote, channels, _) = connect_bundle_pair();
        let now = Instant::now() + Duration::from_secs(1);
        receive_messages(&mut local, &mut remote, now);
        let feedback = NvstFeedbackState::default();
        if let Some(rtt) = video_rtt {
            feedback.publish_ping(
                true,
                if stale {
                    now - STREAM_PING_TIMEOUT
                } else {
                    now
                },
                rtt,
            );
        }
        feedback.request_nack(42, 42, now);
        let mut sent_at = Vec::new();
        for millis in 0..=52 {
            let at = now + Duration::from_millis(millis);
            send_pending_nack(&feedback, at, &mut local, channels, true, 1, 2);
            let messages = receive_messages(&mut local, &mut remote, at);
            assert!(messages.len() <= 1);
            if !messages.is_empty() {
                sent_at.push(millis);
            }
        }
        assert_eq!(sent_at, expected, "RTT {video_rtt:?}, stale={stale}");
    }
}

#[test]
fn mjolnir_nack_sender_adapts_to_new_rtt_without_resetting_attempts() {
    let (mut local, mut remote, channels, _) = connect_bundle_pair();
    let now = Instant::now() + Duration::from_secs(1);
    receive_messages(&mut local, &mut remote, now);
    let feedback = NvstFeedbackState::default();
    feedback.publish_ping(false, now, Duration::from_millis(5));
    feedback.request_nack(42, 42, now);
    for (millis, new_video_rtt, expected) in [
        (0, None, 1),
        (8, None, 0),
        (9, Some(20), 0),
        (23, None, 0),
        (24, None, 1),
        (28, Some(1), 0),
        (29, None, 1),
        (34, None, 0),
    ] {
        let at = now + Duration::from_millis(millis);
        if let Some(rtt) = new_video_rtt {
            feedback.publish_ping(true, at, Duration::from_millis(rtt));
        }
        send_pending_nack(&feedback, at, &mut local, channels, true, 1, 2);
        assert_eq!(
            receive_messages(&mut local, &mut remote, at).len(),
            expected,
            "at {millis} ms"
        );
    }
    assert!(feedback.resolve_nack(42));
}

#[test]
fn generic_nack_sender_keeps_fixed_retries_even_with_a_live_rtt() {
    let (mut local, mut remote, channels, _) = connect_bundle_pair();
    let now = Instant::now() + Duration::from_secs(1);
    receive_messages(&mut local, &mut remote, now);
    let feedback = NvstFeedbackState::default();
    feedback.publish_ping(true, now, Duration::from_millis(40));
    feedback.request_nack(42, 42, now);
    let mut sent_at = Vec::new();
    for millis in 0..=52 {
        let at = now + Duration::from_millis(millis);
        send_pending_nack(&feedback, at, &mut local, channels, false, 1, 2);
        for (channel, _) in receive_messages(&mut local, &mut remote, at) {
            assert_eq!(channel, "rtcp_on_sctp_private");
            sent_at.push(millis);
        }
    }
    assert_eq!(sent_at, [0, 4, 8]);
}
