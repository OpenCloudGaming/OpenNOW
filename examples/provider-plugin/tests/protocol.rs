use opennow_plugin_api::PluginId;
use opennow_plugin_api::provider::*;
use opennow_sdk_demo::{CAPABILITIES, PLUGIN_ID, VERSION};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::Duration;

struct Process {
    child: Child,
    input: ChildStdin,
    replies: Receiver<Vec<u8>>,
    reader: Option<JoinHandle<()>>,
    epoch: u64,
    next: u64,
}

impl Process {
    fn start(directory: &Path, epoch: u64) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_opennow-sdk-demo"))
            .env("OPENNOW_PLUGIN_DATA_DIR", directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (sender, replies) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let mut frame = Vec::new();
                if reader.read_until(b'\n', &mut frame).unwrap_or(0) == 0 {
                    break;
                }
                if sender.send(frame).is_err() {
                    break;
                }
            }
        });
        let mut process = Self {
            child,
            input,
            replies,
            reader: Some(reader),
            epoch,
            next: 0,
        };
        let response = process.call(ProviderRequest::Hello(ProviderHello {
            plugin_id: PluginId::new(PLUGIN_ID).unwrap(),
            version: Text::new(VERSION).unwrap(),
            capabilities: List::new(CAPABILITIES.to_vec()).unwrap(),
        }));
        assert!(matches!(
            response.outcome.reply(),
            Some(ProviderReply::Hello(_))
        ));
        process
    }

    fn call(&mut self, request: ProviderRequest) -> ProviderResponseV2 {
        self.next += 1;
        let request = HostRequestV2 {
            v: Version2,
            epoch: self.epoch.try_into().unwrap(),
            id: Text::new(self.next.to_string()).unwrap(),
            timeout_ms: 10000,
            request,
        };
        let bytes = serde_json::to_vec(&HostMessageV2::Request(Box::new(request.clone()))).unwrap();
        self.input.write_all(&bytes).unwrap();
        self.input.write_all(b"\n").unwrap();
        self.input.flush().unwrap();
        let bytes = self
            .replies
            .recv_timeout(Duration::from_secs(5))
            .expect("control response deadline");
        let PluginMessageV2::Response(response) = PluginMessageV2::decode(&bytes).unwrap();
        response.validate_for(&request).unwrap();
        response
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[test]
fn real_control_process_preserves_authorized_attempt_across_a_new_epoch() {
    let directory = tempfile::tempdir().unwrap();
    let mut process = Process::start(directory.path(), 1);
    let begin = process.call(ProviderRequest::AuthBegin(BeginAuth {
        authority: None,
        kind: AuthKind::Pairing,
        remember: true,
    }));
    let Some(ProviderReply::AuthBegin(AuthState::Pending {
        challenge: AuthChallenge::Pairing { attempt, .. },
    })) = begin.outcome.reply()
    else {
        panic!("missing pairing")
    };
    let attempt = attempt.clone();
    assert!(matches!(
        process
            .call(ProviderRequest::AuthPoll(AuthAttempt {
                attempt: attempt.clone()
            }))
            .outcome
            .reply(),
        Some(ProviderReply::AuthPoll(AuthState::Authorized { .. }))
    ));
    drop(process);
    let mut process = Process::start(directory.path(), 2);
    assert!(matches!(
        process
            .call(ProviderRequest::AuthStatus(Empty {}))
            .outcome
            .reply(),
        Some(ProviderReply::AuthStatus(AuthState::SignedOut))
    ));
    let complete = process.call(ProviderRequest::AuthComplete(CompleteAuth {
        attempt: attempt.clone(),
        proof: None,
    }));
    assert!(matches!(
        complete.outcome.reply(),
        Some(ProviderReply::AuthComplete(AuthState::SignedIn { .. }))
    ));
    let again = process.call(ProviderRequest::AuthComplete(CompleteAuth {
        attempt,
        proof: None,
    }));
    assert_eq!(complete.outcome, again.outcome);
    assert!(matches!(
        process
            .call(ProviderRequest::Shutdown(Empty {}))
            .outcome
            .reply(),
        Some(ProviderReply::Shutdown(_))
    ));
    assert!(process.child.wait().unwrap().success());
}

#[test]
fn real_control_process_rejects_unknown_operations_without_protocol_output() {
    let directory = tempfile::tempdir().unwrap();
    let mut process = Process::start(directory.path(), 1);
    process.input.write_all(b"{\"type\":\"request\",\"v\":2,\"epoch\":1,\"id\":\"bad\",\"timeoutMs\":1000,\"request\":{\"method\":\"execute\",\"params\":{}}}\n").unwrap();
    process.input.flush().unwrap();
    assert!(
        process
            .replies
            .recv_timeout(Duration::from_secs(5))
            .is_err()
    );
    assert!(!process.child.wait().unwrap().success());
}
