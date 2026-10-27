use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Default)]
pub struct EngineCancellation {
    epoch: Arc<AtomicU64>,
}

#[derive(Clone)]
pub struct StartupToken {
    owner: EngineCancellation,
    epoch: u64,
}

impl EngineCancellation {
    pub fn cancel(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
    }

    pub fn token(&self) -> StartupToken {
        StartupToken {
            owner: self.clone(),
            epoch: self.epoch.load(Ordering::Acquire),
        }
    }
}

impl StartupToken {
    pub(crate) fn is_cancelled(&self) -> bool {
        self.owner.epoch.load(Ordering::Acquire) != self.epoch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Engine, State, lock_lifecycle};
    use opennow_streamer_protocol::Command;
    use serde_json::json;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    fn start(id: &str, endpoints: serde_json::Value) -> Command {
        serde_json::from_value(json!({
            "id": id, "type": "start", "context": {
                "session": {"sessionId":"startup-test", "serverIp":"127.0.0.1", "rtspsEndpoints":endpoints},
                "settings":{"transportMode":"nvst"}, "shortcuts":{}
            }
        })).unwrap()
    }

    #[test]
    fn cancellation_rejects_queued_start_without_poisoning_the_next_epoch() {
        let (events, _receiver) = mpsc::channel();
        let mut engine = Engine::new(events);
        let cancellation = EngineCancellation::default();
        engine.set_cancellation(cancellation.clone());
        let queued = cancellation.token();
        cancellation.cancel();
        let (responses, running) =
            engine.handle_with_startup_token(start("old", json!([])), queued);
        assert!(running);
        assert_eq!(responses[0]["id"], "old");
        assert_eq!(responses[0]["code"], "nvst-start-aborted");
        assert_eq!(lock_lifecycle(&engine.lifecycle).state, State::Idle);
        let (responses, _) =
            engine.handle_with_startup_token(start("new", json!([])), cancellation.token());
        assert_eq!(responses[0]["code"], "missing-rtsps-endpoint");
    }

    #[test]
    fn cancellation_interrupts_engine_start_while_tls_peer_stays_open() {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (events, receiver) = mpsc::channel();
        let mut engine = Engine::new(events);
        let cancellation = EngineCancellation::default();
        engine.set_cancellation(cancellation.clone());
        let token = cancellation.token();
        let (completed, completion) = mpsc::channel();
        let worker = thread::spawn(move || {
            let (responses, running) = engine.handle_with_startup_token(
                start("in-flight", json!([format!("rtsps://{address}")])),
                token,
            );
            completed
                .send((responses, running, lock_lifecycle(&engine.lifecycle).state))
                .unwrap();
        });
        let (mut peer, _) = listener.accept().unwrap();
        drop(listener);
        peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut hello = [0; 4096];
        assert!(peer.read(&mut hello).unwrap() > 0);
        cancellation.cancel();
        let result = completion.recv_timeout(Duration::from_secs(1));
        drop(peer);
        worker.join().unwrap();
        let (responses, running, state) = result.unwrap();
        assert!(running);
        assert_eq!(state, State::Idle);
        assert_eq!(responses[0]["id"], "in-flight");
        assert_eq!(responses[0]["code"], "nvst-start-aborted");
        assert!(
            receiver
                .try_iter()
                .all(|event| event["state"] != "connected")
        );
    }
}
