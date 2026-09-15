//! Complete each ICE RTT probe once. str0m 0.23's ICE agent otherwise records a late duplicate
//! response again, replacing the real RTT with time since the original request (often seconds).

use super::*;

const MAX_PROBES: usize = 64;
const PROBE_LIFETIME: Duration = Duration::from_secs(60);

struct Probe {
    transaction: [u8; 12],
    sent_at: Instant,
    completed: bool,
}

#[derive(Default)]
pub(super) struct StunResponseGuard {
    probes: VecDeque<Probe>,
    pub(super) duplicates: u64,
}

impl StunResponseGuard {
    pub(super) fn sent(&mut self, packet: &[u8], now: Instant) {
        if !looks_like_stun(packet) || packet[..2] != STUN_BINDING_REQUEST.to_be_bytes() {
            return;
        }
        self.expire(now);
        let transaction = packet[8..20].try_into().expect("STUN header checked");
        if self
            .probes
            .iter()
            .any(|probe| probe.transaction == transaction)
        {
            return;
        }
        if self.probes.len() == MAX_PROBES {
            self.probes.pop_front();
        }
        self.probes.push_back(Probe {
            transaction,
            sent_at: now,
            completed: false,
        });
    }

    /// Only suppress authenticated successes for requests sent by this ICE agent. NATT replies,
    /// requests, errors and invalid packets retain their normal processing path.
    pub(super) fn duplicate(
        &mut self,
        packet: &[u8],
        credentials: &NvstStunCredentials,
        now: Instant,
    ) -> bool {
        if !looks_like_stun(packet)
            || packet[..2] != STUN_BINDING_SUCCESS_RESPONSE.to_be_bytes()
            || STUN_HEADER_LEN + usize::from(u16::from_be_bytes([packet[2], packet[3]]))
                != packet.len()
        {
            return false;
        }
        self.expire(now);
        let Some(probe) = self
            .probes
            .iter_mut()
            .find(|probe| probe.transaction == packet[8..20])
        else {
            return false;
        };
        if !valid_stun_fingerprint(packet)
            || !valid_stun_message_integrity(packet, credentials.remote_password.as_bytes())
            || str0m::ice::StunMessage::parse(packet).is_err()
        {
            return false;
        }
        if probe.completed {
            self.duplicates = self.duplicates.saturating_add(1);
            return true;
        }
        probe.completed = true;
        false
    }

    fn expire(&mut self, now: Instant) {
        while self
            .probes
            .front()
            .is_some_and(|probe| now.saturating_duration_since(probe.sent_at) >= PROBE_LIFETIME)
        {
            self.probes.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use str0m::ice::{IceAgent, StunMessage, StunPacket};

    fn credentials() -> NvstStunCredentials {
        NvstStunCredentials {
            local_username_fragment: "loc1".into(),
            local_password: "local-password-value-01".into(),
            remote_username_fragment: "remote01".into(),
            remote_password: "remote-password-value-01".into(),
        }
    }

    fn deliver(
        agent: &mut IceAgent,
        bytes: &[u8],
        now: Instant,
        local: SocketAddr,
        remote: SocketAddr,
    ) {
        assert!(agent.handle_packet(
            now,
            StunPacket {
                proto: RtcProtocol::Udp,
                source: remote,
                destination: local,
                message: StunMessage::parse(bytes).unwrap(),
            }
        ));
    }

    // Exercise the actual ICE library behind Event::PeerStats, including nomination. Replaying a
    // completed transaction used to turn a 202 ms probe into a 2502 ms probe with no new request.
    fn probe_rtt(guarded: bool, first_reply_ms: u64) -> Duration {
        let credentials = credentials();
        let local = "192.0.2.1:5000".parse().unwrap();
        let remote = "192.0.2.2:5001".parse().unwrap();
        let mut agent = IceAgent::with_hmac(
            IceCreds {
                ufrag: credentials.local_username_fragment.clone(),
                pass: credentials.local_password.clone(),
            },
            str0m::crypto::from_feature_flags().sha1_hmac_provider,
        );
        agent.set_remote_credentials(IceCreds {
            ufrag: credentials.remote_username_fragment.clone(),
            pass: credentials.remote_password.clone(),
        });
        agent.set_controlling(true);
        agent.add_local_candidate(Candidate::host(local, "udp").unwrap());
        agent.add_remote_candidate(Candidate::host(remote, "udp").unwrap());
        let mut now = Instant::now();
        let mut guard = StunResponseGuard::default();
        for _ in 0..20 {
            agent.handle_timeout(now);
            while let Some(sent) = agent.poll_transmit() {
                guard.sent(&sent.contents, now);
                let transaction = sent.contents[8..20].try_into().unwrap();
                let response = synthesize_ice_binding_success(
                    &transaction,
                    local,
                    &credentials.remote_password,
                );
                let received = now + Duration::from_millis(first_reply_ms);
                assert!(!guard.duplicate(&response, &credentials, received));
                deliver(&mut agent, &response, received, local, remote);
                if agent.nominated_pair_rtt().is_some() {
                    let duplicate_at = received + Duration::from_millis(2300);
                    if !guarded || !guard.duplicate(&response, &credentials, duplicate_at) {
                        deliver(&mut agent, &response, duplicate_at, local, remote);
                    }
                    return agent.nominated_pair_rtt().unwrap();
                }
            }
            now += Duration::from_secs(4);
        }
        panic!("ICE probe never nominated");
    }

    #[test]
    fn late_duplicate_cannot_turn_202_ms_into_2502_ms() {
        assert_eq!(probe_rtt(false, 202), Duration::from_millis(2502));
        assert_eq!(probe_rtt(true, 202), Duration::from_millis(202));
    }

    #[test]
    fn real_multi_second_rtt_is_preserved() {
        assert_eq!(probe_rtt(true, 3407), Duration::from_millis(3407));
    }

    #[test]
    fn invalid_unknown_and_non_response_packets_cannot_complete_a_probe() {
        let credentials = credentials();
        let now = Instant::now();
        let mut guard = StunResponseGuard::default();
        let request = build_stun_binding_request(&credentials, &[1; 12]);
        let response = synthesize_ice_binding_success(
            &[1; 12],
            "192.0.2.1:5000".parse().unwrap(),
            &credentials.remote_password,
        );
        assert!(!guard.duplicate(&response, &credentials, now));
        guard.sent(&request, now);
        assert!(!guard.duplicate(&request, &credentials, now));
        let mut invalid = response.clone();
        invalid[24] ^= 1;
        assert!(!guard.duplicate(&invalid, &credentials, now));
        assert!(!guard.duplicate(&response, &credentials, now));
        assert!(guard.duplicate(&response, &credentials, now));
        assert_eq!(guard.duplicates, 1);
    }

    #[test]
    fn tracking_is_bounded_and_expires() {
        let credentials = credentials();
        let now = Instant::now();
        let mut guard = StunResponseGuard::default();
        for id in 0..100u8 {
            guard.sent(&build_stun_binding_request(&credentials, &[id; 12]), now);
        }
        assert_eq!(guard.probes.len(), MAX_PROBES);
        guard.sent(
            &build_stun_binding_request(&credentials, &[100; 12]),
            now + PROBE_LIFETIME,
        );
        assert_eq!(guard.probes.len(), 1);
    }
}
